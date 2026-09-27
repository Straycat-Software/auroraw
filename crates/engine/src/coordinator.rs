// SPDX-License-Identifier: GPL-3.0-or-later
//! The single writer (architecture §4.2): one thread, owning the only mutable `Catalogue`
//! connection, applying commands strictly one at a time. Everything that changes the workspace or
//! the catalogue goes through here, whichever thread asked for it.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::SystemTime;

use auroraw_catalogue::{Catalogue, SidecarStat};
use auroraw_format::sidecar::{FileEntry, FileRole, Location, PhotoSidecar, VersionSidecar};
use auroraw_format::state::{KeywordEntry, SourceEntry, Vocabulary};
use auroraw_import::Profile;
use auroraw_plugin_api::source::{Source, SourceState};
use auroraw_sources::filesystem::FilesystemSource;
use auroraw_sources::relink::{self, FoundFile, KnownFile, ScanOutcome};
use auroraw_types::{ContentHash, KeywordId, PhotoId, SeriesId, SourceId, Timestamp};
use auroraw_workspace::{FileStat, Workspace};

use crate::command::Command;
use crate::error::{EngineError, Result};
use crate::event::Event;
use crate::history::{
    Change, Direction, Entry, History, HistoryState, KeywordDelta, KeywordSet, VocabularyAction,
};
use crate::import_job::{self, ImportJob};
use crate::index_job::{self, IndexJob};
use crate::job::{CancelToken, JobId};
use crate::remove_job::{self, RemoveJob};

/// What a command that waits for its result (`Engine::submit_and_wait`) gets back.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The command applied with nothing more specific to report.
    Applied,
    /// `CreateKeyword`'s new identifier.
    KeywordCreated(KeywordId),
    /// `RenameKeyword`'s background refresh job and how many sidecars it will touch.
    RenameStarted {
        /// The job doing the refresh.
        job: JobId,
        /// How many sidecars it will touch.
        affected: usize,
    },
    /// `AddSource`'s new identifier.
    SourceAdded(SourceId),
    /// `ScanSource`'s report: what the reconcile applied, and what it left for a person or
    /// `AddNewPhotos`.
    Scanned {
        /// Whether the source could be reached at all.
        reachable: bool,
        /// How many files matched exactly what the catalogue expected.
        confirmed: usize,
        /// How many files changed at their known path.
        changed: usize,
        /// How many files were relinked silently.
        relinked: usize,
        /// How many known files were found nowhere.
        missing: usize,
        /// Paths that matched nothing known: offer these to `AddNewPhotos`.
        new: Vec<String>,
        /// How many found files were ambiguous.
        ambiguous: usize,
        /// How many found files were confirmed second locations of an existing photo (D-036, D-108).
        second_locations: usize,
    },
    /// `AddNewPhotos`'s new identifiers, in the order their paths were given.
    PhotosAdded(Vec<PhotoId>),
    /// `Import`'s background job.
    ImportStarted {
        /// The job doing the import.
        job: JobId,
    },
    /// `IndexSource`'s background job.
    IndexStarted {
        /// The job scanning the source.
        job: JobId,
    },
    /// `MoveKeyword`'s and `DeleteKeyword`'s report: how many keywords the action touched and how many
    /// photos it changed (a move: how many sidecars will have their paths refreshed).
    KeywordsChanged {
        /// Keywords moved or deleted (with their branch).
        keywords: usize,
        /// Photos changed.
        photos: usize,
    },
    /// `GroupPhotos`'s new series.
    SeriesGrouped(SeriesId),
    /// `DetectSeries`'s report: how many series were formed and how many photos they hold.
    SeriesDetected {
        /// Series formed.
        series: usize,
        /// Photos in them.
        photos: usize,
    },
    /// `RemoveSource`'s background job.
    RemoveStarted {
        /// The job removing the source.
        job: JobId,
    },
    /// `Undo` or `Redo` did it, and this is what the next Undo and Redo would do.
    History(HistoryState),
    /// `Undo` or `Redo` had nothing to undo or redo.
    Nothing,
}

pub(crate) type Reply = mpsc::Sender<Result<Outcome>>;

pub(crate) enum Inbound {
    /// An event a job wants reported after everything it sent before it has been applied: the
    /// catalogue writes it queued are ahead of it in this queue, so whoever reacts to the event by
    /// reading the catalogue sees them all.
    Report(Event),
    /// A remove job took a photo's sidecar out of the workspace (recoverably): take its row out of
    /// the catalogue.
    Removed { photo_id: PhotoId },
    /// A remove job dropped a source from a photo that has another location: point its row at it.
    Relocated {
        photo_id: PhotoId,
        source_id: SourceId,
        path: String,
        filename: String,
        fingerprint: auroraw_types::Fingerprint,
    },
    /// A remove job is done with a source's photos (or an index job merged them into another
    /// source): take the source itself out. `announce` is whether `Event::SourceRemoved` is
    /// reported: a removal is one, a merge is part of an index job that reports its own end.
    SourceGone {
        job: JobId,
        source_id: SourceId,
        removed: usize,
        kept: usize,
        announce: bool,
    },
    /// A thumbnail worker made the perceptual hash of a photo's thumbnail (D-105): record it.
    Hashed { photo_id: PhotoId, phash: u64 },
    /// An index job confirmed (by whole-file hash) that a file it was about to add is really a second location of
    /// an existing photo from a *different* source (D-036, D-108): its sidecar already carries the new `Location`
    /// (written by the job, under the guard); record it in the catalogue too.
    LocationAdded {
        photo_id: PhotoId,
        source_id: SourceId,
        path: String,
        filename: String,
        fingerprint: auroraw_types::Fingerprint,
        hash: ContentHash,
    },
    /// The last [`crate::Engine`] handle was dropped: cancel what runs in the background and stop.
    /// (The coordinator holds a sender to its own queue for the jobs it starts, so a closed queue
    /// never signals the end by itself.)
    Stop,
    Command {
        command: Command,
        reply: Option<Reply>,
    },
    /// A background job refreshed one sidecar: read it again and bring the catalogue's row in line, on the
    /// coordinator thread, like everything else (what the job read may be older than what a person did
    /// to the photo since, so the message carries no metadata).
    Refreshed { photo_id: PhotoId },
    /// The path-refresh job that was running is over (finished or cancelled): the next one may start.
    RefreshDone,
    /// An import job (`crate::import_job`) landed one photo, or discovered (while checking a
    /// fingerprint match) the whole-file hash of a photo that only had a fingerprint recorded:
    /// `photo` and `stat` are `None` for a check that found nothing to register, `backfill` is
    /// `Some` whenever a hash was discovered either way.
    Imported {
        photo: Option<Box<PhotoSidecar>>,
        stat: Option<SidecarStat>,
        backfill: Option<(PhotoId, ContentHash)>,
    },
}

fn keyword_set(meta: &auroraw_format::sidecar::Metadata) -> KeywordSet {
    KeywordSet {
        ids: meta.keyword_ids.clone(),
        paths: meta.keyword_paths.clone(),
    }
}

fn sidecar_stat(stat: &FileStat) -> SidecarStat {
    stat_from(stat.size, stat.modified)
}

fn stat_from(size: u64, modified: Option<SystemTime>) -> SidecarStat {
    let modified = modified
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    SidecarStat { size, modified }
}

#[path = "series_ops.rs"]
mod series_ops;

pub(crate) struct Coordinator {
    workspace: Arc<Workspace>,
    catalogue: Catalogue,
    events: mpsc::Sender<Event>,
    inbound: mpsc::Sender<Inbound>,
    jobs: HashMap<JobId, CancelToken>,
    /// Where the answer to an index job's pause goes.
    index_decisions: HashMap<JobId, mpsc::Sender<bool>>,
    next_job: u64,
    /// What the person did to their photos, for Undo and Redo (D-096).
    history: History,
    /// The path refreshes of the sidecars (after a keyword is renamed, moved, or one of those is undone)
    /// run one at a time, in the order asked, so that the last vocabulary wins.
    refresh_queue: VecDeque<RefreshRequest>,
    refresh_running: bool,
    /// The largest gap between two photos of one series, in seconds (D-101).
    series_gap: u32,
}

/// A path refresh waiting for its turn.
struct RefreshRequest {
    job: JobId,
    photo_ids: Vec<PhotoId>,
    paths: HashMap<KeywordId, String>,
}

/// What a change of the vocabulary set going: the refresh job (when there is one) and how many photos it
/// will touch.
struct Refresh {
    job: Option<JobId>,
    affected: usize,
}

impl Coordinator {
    pub(crate) fn new(
        workspace: Arc<Workspace>,
        catalogue: Catalogue,
        events: mpsc::Sender<Event>,
        inbound: mpsc::Sender<Inbound>,
    ) -> Self {
        Self {
            workspace,
            catalogue,
            events,
            inbound,
            jobs: HashMap::new(),
            index_decisions: HashMap::new(),
            next_job: 0,
            history: History::default(),
            refresh_queue: VecDeque::new(),
            refresh_running: false,
            series_gap: crate::series_detect::DEFAULT_GAP,
        }
    }

    /// Runs until the channel closes (every `Engine` handle and every job's sender dropped).
    pub(crate) fn run(mut self, rx: mpsc::Receiver<Inbound>) {
        for message in rx {
            match message {
                Inbound::Stop => {
                    for token in self.jobs.values() {
                        token.cancel();
                    }
                    break;
                }
                Inbound::Report(event) => {
                    // Photos arrived: the ones that belong together form series (D-101).
                    let arrived = matches!(
                        event,
                        Event::IndexFinished { .. } | Event::ImportFinished { .. }
                    );
                    let _ = self.events.send(event);
                    if arrived {
                        let _ = self.detect_series(false);
                    }
                }
                Inbound::Hashed { photo_id, phash } => {
                    // (A photo that has left since is not there: nothing to record.)
                    let _ = self.catalogue.apply_phash(&photo_id, phash);
                }
                Inbound::LocationAdded {
                    photo_id,
                    source_id,
                    path,
                    filename,
                    fingerprint,
                    hash,
                } => {
                    let _ = self.catalogue.insert_location(
                        &photo_id,
                        &source_id,
                        &path,
                        &filename,
                        &fingerprint,
                        &hash,
                    );
                }
                Inbound::Removed { photo_id } => {
                    // A photo that leaves a series shrinks it.
                    self.leave_series_on_removal(photo_id);
                    let _ = self.catalogue.remove_photo(&photo_id);
                    // What was done to a photo that has left cannot be undone.
                    if self.history.forget_photo(photo_id) {
                        self.report_history();
                    }
                }
                Inbound::Relocated {
                    photo_id,
                    source_id,
                    path,
                    filename,
                    fingerprint,
                } => {
                    let _ = self.catalogue.apply_relink(
                        &photo_id,
                        &source_id,
                        &path,
                        &filename,
                        &fingerprint,
                    );
                }
                Inbound::SourceGone {
                    job,
                    source_id,
                    removed,
                    kept,
                    announce,
                } => self.finish_remove_source(job, source_id, removed, kept, announce),
                Inbound::Command { command, reply } => self.handle_command(command, reply),
                Inbound::Refreshed { photo_id } => self.handle_refreshed(photo_id),
                Inbound::RefreshDone => {
                    self.refresh_running = false;
                    if let Some(next) = self.refresh_queue.pop_front() {
                        self.start_refresh(next);
                    }
                }
                Inbound::Imported {
                    photo,
                    stat,
                    backfill,
                } => self.handle_imported(photo.map(|p| *p), stat, backfill),
            }
        }
        let _ = self.events.send(Event::Stopped);
    }

    fn handle_command(&mut self, command: Command, reply: Option<Reply>) {
        let result = self.apply(command.clone());
        match result {
            Ok(outcome) => {
                let _ = self.events.send(Event::Applied(command));
                if let Some(reply) = reply {
                    let _ = reply.send(Ok(outcome));
                }
            }
            Err(e) => {
                let _ = self.events.send(Event::Failed {
                    command,
                    error: e.to_string(),
                });
                if let Some(reply) = reply {
                    let _ = reply.send(Err(e));
                }
            }
        }
    }

    fn apply(&mut self, command: Command) -> Result<Outcome> {
        match command {
            Command::Rebuild => self.rebuild(),
            Command::Reconcile => self.reconcile(),
            Command::SetRating { .. }
            | Command::SetFlag { .. }
            | Command::SetLabel { .. }
            | Command::AddKeyword { .. }
            | Command::RemoveKeyword { .. } => {
                let change = self.apply_edit(&command)?;
                self.record(change.into_iter().collect());
                Ok(Outcome::Applied)
            }
            Command::Batch { commands } => self.batch(commands),
            Command::Undo => self.travel(Direction::Undo),
            Command::Redo => self.travel(Direction::Redo),
            Command::CreateKeyword { name, parent, id } => {
                let (id, change) = self.make_keyword(&name, parent, id)?;
                self.record(vec![change]);
                Ok(Outcome::KeywordCreated(id))
            }
            Command::RenameKeyword {
                keyword_id,
                new_name,
            } => self.rename_keyword(keyword_id, new_name),
            Command::MoveKeyword {
                keyword_id,
                new_parent,
            } => self.move_keyword(keyword_id, new_parent),
            Command::DeleteKeyword { keyword_id } => self.delete_keyword(keyword_id),
            Command::GroupPhotos { photos } => self.group_photos(photos),
            Command::RemoveFromSeries { photos } => self.remove_from_series(photos),
            Command::DissolveSeries { series } => self.dissolve_series(series),
            Command::ResolveSeries { series, keep } => self.resolve_series(series, keep),
            Command::ReopenSeries { series } => self.reopen_series(series),
            Command::SetSeriesGap { seconds } => {
                self.series_gap = seconds;
                Ok(Outcome::Applied)
            }
            Command::DetectSeries { regroup } => self.detect_series(regroup),
            Command::CancelJob { job_id } => self.cancel_job(job_id),
            Command::AddSource { name, root, kind } => self.add_source(name, root, kind),
            Command::ScanSource { source_id } => self.scan_source(source_id),
            Command::AddNewPhotos { source_id, paths } => self.add_new_photos(source_id, paths),
            Command::IndexSource { source_id, merge } => self.start_index(source_id, merge),
            Command::ContinueIndex { job_id, restore } => {
                if let Some(answer) = self.index_decisions.get(&job_id) {
                    let _ = answer.send(restore);
                }
                Ok(Outcome::Applied)
            }
            Command::RemoveSource { source_id } => self.start_remove(source_id),
            Command::Import {
                source_root,
                destination_root,
                registration,
                profile,
                shoot,
                backup_roots,
                state_path,
            } => self.start_import(
                source_root,
                destination_root,
                registration,
                profile,
                shoot,
                backup_roots,
                state_path,
            ),
        }
    }

    fn read_photo(&self, id: &PhotoId) -> Result<(PhotoSidecar, SidecarStat)> {
        let loaded = self
            .workspace
            .read_photo(id)?
            .ok_or_else(|| EngineError::NotFound {
                kind: "photo",
                id: id.to_string(),
            })?;
        let photo = loaded.current().ok_or_else(|| EngineError::NotFound {
            kind: "photo (newer schema)",
            id: id.to_string(),
        })?;
        let meta = std::fs::metadata(self.workspace.photo_path(id)).map_err(EngineError::Io)?;
        Ok((photo, stat_from(meta.len(), meta.modified().ok())))
    }

    fn main_version_of(&self, photo: &PhotoSidecar) -> Result<Option<VersionSidecar>> {
        let Some(version_id) = photo.main_version else {
            return Ok(None);
        };
        let loaded = self.workspace.read_version(&photo.photo_id, &version_id)?;
        Ok(loaded.and_then(|l| l.current()))
    }

    /// Applies one of the edit commands (the ones that go in the history) and says what changed: `None`
    /// when the photo already was as asked, which is not worth a step.
    fn apply_edit(&mut self, command: &Command) -> Result<Option<Change>> {
        match command {
            Command::SetRating { photo_id, rating } => {
                let rating = Some(*rating);
                self.edit_photo(*photo_id, |m| {
                    let before = std::mem::replace(&mut m.rating, rating);
                    (before != rating).then_some(Change::Rating {
                        photo: *photo_id,
                        before,
                        after: rating,
                    })
                })
            }
            Command::SetFlag { photo_id, flag } => self.edit_photo(*photo_id, |m| {
                let before = std::mem::replace(&mut m.flag, *flag);
                (before != *flag).then_some(Change::Flag {
                    photo: *photo_id,
                    before,
                    after: *flag,
                })
            }),
            Command::SetLabel { photo_id, label } => {
                let after = label.map(|l| l.name().to_string());
                self.edit_photo(*photo_id, |m| {
                    let before = std::mem::replace(&mut m.label, after.clone());
                    (before != after).then_some(Change::Label {
                        photo: *photo_id,
                        before,
                        after: after.clone(),
                    })
                })
            }
            Command::AddKeyword {
                photo_id,
                keyword_id,
            } => {
                let path = if self
                    .read_photo(photo_id)?
                    .0
                    .meta
                    .keyword_ids
                    .contains(keyword_id)
                {
                    String::new()
                } else {
                    self.keyword_path(keyword_id)?
                };
                self.edit_photo(*photo_id, |m| {
                    if m.keyword_ids.contains(keyword_id) {
                        return None;
                    }
                    let before = keyword_set(m);
                    m.push_keyword(*keyword_id, path);
                    Some(Change::Keywords {
                        photo: *photo_id,
                        before,
                        after: keyword_set(m),
                    })
                })
            }
            Command::RemoveKeyword {
                photo_id,
                keyword_id,
            } => self.edit_photo(*photo_id, |m| {
                let i = m.keyword_ids.iter().position(|k| k == keyword_id)?;
                let before = keyword_set(m);
                m.keyword_ids.remove(i);
                if i < m.keyword_paths.len() {
                    m.keyword_paths.remove(i);
                }
                Some(Change::Keywords {
                    photo: *photo_id,
                    before,
                    after: keyword_set(m),
                })
            }),
            Command::CreateKeyword { name, parent, id } => {
                Ok(Some(self.make_keyword(name, *parent, *id)?.1))
            }
            other => Err(EngineError::InvalidCommand(format!(
                "{other:?} is not an edit of a photo: it cannot be part of a batch"
            ))),
        }
    }

    /// Reads a photo's sidecar, lets `edit` change its metadata (it says what changed), and writes it
    /// back and into the catalogue: only when something changed.
    fn edit_photo(
        &mut self,
        photo_id: PhotoId,
        edit: impl FnOnce(&mut auroraw_format::sidecar::Metadata) -> Option<Change>,
    ) -> Result<Option<Change>> {
        let workspace = self.workspace.clone();
        let _guard = workspace.sidecar_guard();
        let (mut photo, _) = self.read_photo(&photo_id)?;
        let change = edit(&mut photo.meta);
        // (A same-value edit still writes: it is how a sidecar that was changed outside is written back.)
        self.persist_photo(photo)?;
        Ok(change)
    }

    /// Writes a photo's sidecar and brings the catalogue's row in line with what was written.
    fn persist_photo(&mut self, photo: PhotoSidecar) -> Result<()> {
        let photo_id = photo.photo_id;
        self.workspace.write_photo(&photo)?;
        let (photo, stat) = self.read_photo(&photo_id)?; // the just-written size and time
        let main_version = self.main_version_of(&photo)?;
        self.catalogue
            .apply_photo_metadata(&photo, stat, main_version.as_ref())?;
        let _ = self.events.send(Event::PhotoChanged(photo_id));
        Ok(())
    }

    /// Puts one more action in the history.
    fn record(&mut self, changes: Vec<Change>) {
        if changes.is_empty() {
            return;
        }
        self.history.record(Entry::new(changes));
        self.report_history();
    }

    fn report_history(&self) {
        let _ = self
            .events
            .send(Event::HistoryChanged(self.history.state()));
    }

    /// Applies several edits as one action: one step in the history, all or nothing.
    fn batch(&mut self, commands: Vec<Command>) -> Result<Outcome> {
        let mut changes: Vec<Change> = Vec::new();
        for command in &commands {
            match self.apply_edit(command) {
                Ok(change) => changes.extend(change),
                Err(e) => {
                    // The edits that were made are taken back, so that a batch is not half done.
                    for change in changes.iter().rev() {
                        let _ = self.write_change(change, Direction::Undo);
                    }
                    return Err(e);
                }
            }
        }
        self.record(changes);
        Ok(Outcome::Applied)
    }

    /// Undoes the last action, or redoes the last one undone.
    fn travel(&mut self, direction: Direction) -> Result<Outcome> {
        let taken = match direction {
            Direction::Undo => self.history.take_undo(),
            Direction::Redo => self.history.take_redo(),
        };
        let Some(entry) = taken else {
            return Ok(Outcome::Nothing);
        };
        // Every photo has to be there, or the step is dropped (and the rest of the history stays). A step
        // that changed the vocabulary is done for the photos that remain instead: a keyword that was
        // deleted must be able to come back even if some of its photos left with a source meanwhile.
        let vocabulary = entry.has_vocabulary();
        let stateful = entry.has_state_change();
        if let Some(missing) = entry
            .changes
            .iter()
            .filter_map(Change::photo)
            .find(|photo| !stateful && self.read_photo(photo).is_err())
        {
            self.report_history();
            return Err(EngineError::NotFound {
                kind: "photo",
                id: missing.to_string(),
            });
        }
        let ordered: Vec<&Change> = match direction {
            Direction::Undo => entry.changes.iter().rev().collect(),
            Direction::Redo => entry.changes.iter().collect(),
        };
        for change in ordered {
            if stateful
                && let Some(photo) = change.photo()
                && self.read_photo(&photo).is_err()
            {
                continue;
            }
            if let Err(e) = self.write_change(change, direction) {
                self.report_history();
                return Err(e);
            }
        }
        // (The photos of a step about the vocabulary are not shown: a keyword coming back is not about them.)
        let photos = if vocabulary {
            Vec::new()
        } else {
            entry.photos()
        };
        match direction {
            Direction::Undo => self.history.put_redo(entry),
            Direction::Redo => self.history.put_undo(entry),
        }
        let _ = self.events.send(Event::HistoryApplied {
            redo: direction == Direction::Redo,
            photos,
        });
        self.report_history();
        Ok(Outcome::History(self.history.state()))
    }

    /// Puts a photo in the state one change leads to.
    fn write_change(&mut self, change: &Change, direction: Direction) -> Result<()> {
        if let Change::Vocabulary { keywords, .. } = change {
            self.apply_vocabulary(keywords, direction, false)?;
            return Ok(());
        }
        if let Change::Series {
            id, before, after, ..
        } = change
        {
            let state = match direction {
                Direction::Undo => before,
                Direction::Redo => after,
            };
            return self.write_series_state(*id, state);
        }
        let photo_id = change
            .photo()
            .expect("a change that is not about the vocabulary has a photo");
        let workspace = self.workspace.clone();
        let _guard = workspace.sidecar_guard();
        let (mut photo, _) = self.read_photo(&photo_id)?;
        change.apply(&mut photo.meta, direction);
        self.persist_photo(photo)
    }

    fn keyword_path(&self, id: &KeywordId) -> Result<String> {
        let vocabulary = self.read_vocabulary()?;
        let paths = auroraw_catalogue::keyword_paths(&vocabulary.keywords);
        paths.get(id).cloned().ok_or_else(|| EngineError::NotFound {
            kind: "keyword",
            id: id.to_string(),
        })
    }

    fn read_vocabulary(&self) -> Result<auroraw_format::state::Vocabulary> {
        match self.workspace.read_vocabulary()? {
            Some(loaded) => loaded.current().ok_or_else(|| EngineError::NotFound {
                kind: "vocabulary (newer schema)",
                id: String::new(),
            }),
            None => Ok(auroraw_format::state::Vocabulary {
                updated: Timestamp::now(),
                keywords: Vec::new(),
                extra: Default::default(),
            }),
        }
    }

    /// Makes a keyword: the change to record, and the keyword's identifier.
    fn make_keyword(
        &mut self,
        name: &str,
        parent: Option<KeywordId>,
        id: Option<KeywordId>,
    ) -> Result<(KeywordId, Change)> {
        let name = checked_name(name)?;
        let vocabulary = self.read_vocabulary()?;
        if let Some(parent) = parent {
            find_keyword(&vocabulary.keywords, parent)?;
        }
        ensure_name_is_free(&vocabulary.keywords, &name, parent, None)?;
        let id = id.unwrap_or_else(KeywordId::random);
        if vocabulary.keywords.iter().any(|k| k.id == id) {
            return Err(EngineError::InvalidCommand(format!(
                "the keyword identifier {id} is already used"
            )));
        }
        let delta = KeywordDelta {
            id,
            before: None,
            after: Some(KeywordEntry {
                id,
                name,
                parent,
                synonyms: Vec::new(),
                export: true,
                extra: Default::default(),
            }),
        };
        self.apply_vocabulary(std::slice::from_ref(&delta), Direction::Redo, false)?;
        let _ = self.events.send(Event::KeywordCreated(id));
        Ok((
            id,
            Change::Vocabulary {
                action: VocabularyAction::Create,
                keywords: vec![delta],
            },
        ))
    }

    /// Puts the vocabulary in the state the deltas lead to (`Redo`: after, `Undo`: before): the file, the
    /// catalogue's keyword rows (the changed keywords and every keyword under them, whose paths moved),
    /// and, for the photos that carry those, the sidecars' path snapshots in a background job. The one
    /// place a change of the vocabulary is made, whether it is done, undone or redone.
    fn apply_vocabulary(
        &mut self,
        keywords: &[KeywordDelta],
        direction: Direction,
        always_job: bool,
    ) -> Result<Refresh> {
        let mut vocabulary = self.read_vocabulary()?;
        let mut removed = Vec::new();
        for delta in keywords {
            let target = match direction {
                Direction::Undo => &delta.before,
                Direction::Redo => &delta.after,
            };
            vocabulary.keywords.retain(|k| k.id != delta.id);
            match target {
                Some(entry) => vocabulary.keywords.push(entry.clone()),
                None => removed.push(delta.id),
            }
        }
        vocabulary.updated = Timestamp::now();
        self.workspace.write_vocabulary(&vocabulary)?;

        let paths = auroraw_catalogue::keyword_paths(&vocabulary.keywords);
        let mut changed: Vec<KeywordId> = Vec::new();
        for delta in keywords.iter().filter(|d| !removed.contains(&d.id)) {
            for id in descendants_of(&vocabulary.keywords, delta.id) {
                if !changed.contains(&id) {
                    changed.push(id);
                }
            }
        }
        // Parents before children, for the catalogue's foreign key.
        changed.sort_by_key(|id| paths.get(id).map_or(0, |p| p.matches('|').count()));
        for id in &changed {
            if let (Some(entry), Some(path)) = (
                vocabulary.keywords.iter().find(|k| k.id == *id),
                paths.get(id),
            ) {
                self.catalogue.apply_keyword(entry, path)?;
            }
        }
        self.catalogue.remove_keywords(&removed)?;

        let photo_ids = self.catalogue.photos_with_keywords(&changed)?;
        let affected = photo_ids.len();
        if photo_ids.is_empty() && !always_job {
            return Ok(Refresh {
                job: None,
                affected,
            });
        }
        let job = self.spawn_job();
        self.queue_refresh(RefreshRequest {
            job,
            photo_ids,
            paths,
        });
        Ok(Refresh {
            job: Some(job),
            affected,
        })
    }

    fn queue_refresh(&mut self, request: RefreshRequest) {
        if self.refresh_running {
            self.refresh_queue.push_back(request);
        } else {
            self.start_refresh(request);
        }
    }

    fn start_refresh(&mut self, request: RefreshRequest) {
        self.refresh_running = true;
        crate::refresh::spawn(
            request.job,
            self.workspace.clone(),
            request.photo_ids,
            request.paths,
            self.events.clone(),
            self.inbound.clone(),
            self.jobs[&request.job].clone(),
        );
    }

    fn rename_keyword(&mut self, keyword_id: KeywordId, new_name: String) -> Result<Outcome> {
        let name = checked_name(&new_name)?;
        let vocabulary = self.read_vocabulary()?;
        let entry = find_keyword(&vocabulary.keywords, keyword_id)?.clone();
        ensure_name_is_free(&vocabulary.keywords, &name, entry.parent, Some(keyword_id))?;
        let delta = KeywordDelta {
            id: keyword_id,
            after: Some(KeywordEntry {
                name,
                ..entry.clone()
            }),
            before: Some(entry),
        };
        let refresh = self.apply_vocabulary(std::slice::from_ref(&delta), Direction::Redo, true)?;
        self.record(vec![Change::Vocabulary {
            action: VocabularyAction::Rename,
            keywords: vec![delta],
        }]);
        let job = refresh.job.expect("a rename always has its job");
        let _ = self.events.send(Event::KeywordRenamed {
            keyword_id,
            job,
            affected: refresh.affected,
        });
        Ok(Outcome::RenameStarted {
            job,
            affected: refresh.affected,
        })
    }

    fn move_keyword(
        &mut self,
        keyword_id: KeywordId,
        new_parent: Option<KeywordId>,
    ) -> Result<Outcome> {
        let vocabulary = self.read_vocabulary()?;
        let entry = find_keyword(&vocabulary.keywords, keyword_id)?.clone();
        if entry.parent == new_parent {
            return Ok(Outcome::Applied);
        }
        if let Some(parent) = new_parent {
            find_keyword(&vocabulary.keywords, parent)?;
            if descendants_of(&vocabulary.keywords, keyword_id).contains(&parent) {
                return Err(EngineError::KeywordCycle);
            }
        }
        ensure_name_is_free(
            &vocabulary.keywords,
            &entry.name,
            new_parent,
            Some(keyword_id),
        )?;
        let branch = descendants_of(&vocabulary.keywords, keyword_id).len();
        let delta = KeywordDelta {
            id: keyword_id,
            after: Some(KeywordEntry {
                parent: new_parent,
                ..entry.clone()
            }),
            before: Some(entry),
        };
        let refresh =
            self.apply_vocabulary(std::slice::from_ref(&delta), Direction::Redo, false)?;
        self.record(vec![Change::Vocabulary {
            action: VocabularyAction::Move,
            keywords: vec![delta],
        }]);
        let _ = self.events.send(Event::KeywordMoved(keyword_id));
        Ok(Outcome::KeywordsChanged {
            keywords: branch,
            photos: refresh.affected,
        })
    }

    /// Deletes a keyword and its branch: the photos that carry any of it lose it first (one change per
    /// photo, as `RemoveKeyword`), then the vocabulary loses the entries, so that a rebuild never finds
    /// a sidecar naming a keyword that is gone. All or nothing.
    fn delete_keyword(&mut self, keyword_id: KeywordId) -> Result<Outcome> {
        let vocabulary = self.read_vocabulary()?;
        find_keyword(&vocabulary.keywords, keyword_id)?;
        let branch = descendants_of(&vocabulary.keywords, keyword_id);
        let photos = self.catalogue.photos_with_keywords(&branch)?;

        let mut changes: Vec<Change> = Vec::new();
        for photo in &photos {
            let edited = self.edit_photo(*photo, |m| {
                if !m.keyword_ids.iter().any(|k| branch.contains(k)) {
                    return None;
                }
                let before = keyword_set(m);
                let mut i = 0;
                while i < m.keyword_ids.len() {
                    if branch.contains(&m.keyword_ids[i]) {
                        m.keyword_ids.remove(i);
                        if i < m.keyword_paths.len() {
                            m.keyword_paths.remove(i);
                        }
                    } else {
                        i += 1;
                    }
                }
                Some(Change::Keywords {
                    photo: *photo,
                    before,
                    after: keyword_set(m),
                })
            });
            match edited {
                Ok(change) => changes.extend(change),
                Err(e) => {
                    self.take_back(&changes);
                    return Err(e);
                }
            }
        }
        let deltas: Vec<KeywordDelta> = branch
            .iter()
            .filter_map(|id| vocabulary.keywords.iter().find(|k| k.id == *id))
            .map(|entry| KeywordDelta {
                id: entry.id,
                before: Some(entry.clone()),
                after: None,
            })
            .collect();
        if let Err(e) = self.apply_vocabulary(&deltas, Direction::Redo, false) {
            self.take_back(&changes);
            return Err(e);
        }
        let touched = changes.len();
        changes.push(Change::Vocabulary {
            action: VocabularyAction::Delete,
            keywords: deltas,
        });
        self.record(changes);
        let _ = self.events.send(Event::KeywordDeleted(keyword_id));
        Ok(Outcome::KeywordsChanged {
            keywords: branch.len(),
            photos: touched,
        })
    }

    /// Takes back changes that were made, newest first (an action that failed half way).
    fn take_back(&mut self, changes: &[Change]) {
        for change in changes.iter().rev() {
            let _ = self.write_change(change, Direction::Undo);
        }
    }

    fn spawn_job(&mut self) -> JobId {
        let id = JobId(self.next_job);
        self.next_job += 1;
        self.jobs.insert(id, CancelToken::new());
        id
    }

    fn cancel_job(&mut self, job_id: JobId) -> Result<Outcome> {
        if let Some(token) = self.jobs.get(&job_id) {
            token.cancel();
        }
        // A path refresh that has not started is simply dropped.
        if let Some(position) = self.refresh_queue.iter().position(|r| r.job == job_id) {
            self.refresh_queue.remove(position);
            let _ = self.events.send(Event::JobCancelled(job_id));
        }
        Ok(Outcome::Applied)
    }

    fn handle_refreshed(&mut self, id: PhotoId) {
        let workspace = self.workspace.clone();
        let _guard = workspace.sidecar_guard();
        // A photo that has gone since (a removed source) is not an error: reconcile would catch it.
        let Ok((photo, stat)) = self.read_photo(&id) else {
            return;
        };
        let Ok(main_version) = self.main_version_of(&photo) else {
            return;
        };
        if self
            .catalogue
            .apply_photo_metadata(&photo, stat, main_version.as_ref())
            .is_ok()
        {
            let _ = self.events.send(Event::PhotoChanged(id));
        }
    }

    fn rebuild(&mut self) -> Result<Outcome> {
        let scan = self.workspace.scan()?;
        let photos: Vec<(PhotoSidecar, SidecarStat)> = scan
            .photos
            .iter()
            .filter_map(|e| {
                self.workspace
                    .read_photo(&e.key)
                    .ok()
                    .flatten()
                    .and_then(|l| l.current())
                    .map(|p| (p, sidecar_stat(&e.stat)))
            })
            .collect();
        let versions: Vec<(VersionSidecar, SidecarStat)> = scan
            .versions
            .iter()
            .filter_map(|e| {
                let (photo, version) = e.key;
                self.workspace
                    .read_version(&photo, &version)
                    .ok()
                    .flatten()
                    .and_then(|l| l.current())
                    .map(|v| (v, sidecar_stat(&e.stat)))
            })
            .collect();
        let vocabulary = self
            .workspace
            .read_vocabulary()?
            .and_then(|l| l.current())
            .map(|v| v.keywords)
            .unwrap_or_default();
        let sources = self
            .workspace
            .read_sources()?
            .and_then(|l| l.current())
            .map(|s| s.sources)
            .unwrap_or_default();
        let collections: Vec<_> = scan
            .collections
            .iter()
            .filter_map(|e| {
                self.workspace
                    .read_collection(&e.key)
                    .ok()
                    .flatten()
                    .and_then(|l| l.current())
                    .map(|c| (c, sidecar_stat(&e.stat)))
            })
            .collect();
        let series: Vec<_> = scan
            .series
            .iter()
            .filter_map(|e| {
                self.workspace
                    .read_series(&e.key)
                    .ok()
                    .flatten()
                    .and_then(|l| l.current())
                    .map(|s| (s, sidecar_stat(&e.stat)))
            })
            .collect();

        let input = auroraw_catalogue::RebuildInput {
            photos: &photos,
            versions: &versions,
            vocabulary: &vocabulary,
            sources: &sources,
            collections: &collections,
            series: &series,
        };
        // The engine always opens a catalogue file (Engine::create/open never uses an in-memory
        // one: rebuild_to_file needs somewhere to atomically replace).
        let path = self
            .catalogue
            .path()
            .expect("the engine's catalogue is always a file")
            .to_path_buf();
        let workspace_id = self.workspace.workspace_id();
        let photo_count = photos.len();
        let version_count = versions.len();
        // Windows refuses to replace a file that is still open (unlike POSIX, where renaming
        // over an open file just works): close our own connection to `path` before
        // `rebuild_to_file` renames the freshly built catalogue over it.
        self.catalogue = Catalogue::open_in_memory(workspace_id)?;
        self.catalogue = auroraw_catalogue::rebuild_to_file(&path, workspace_id, &input)?;
        let _ = self.events.send(Event::RebuildFinished {
            photos: photo_count,
            versions: version_count,
        });
        Ok(Outcome::Applied)
    }

    fn reconcile(&mut self) -> Result<Outcome> {
        let scan = self.workspace.scan()?;
        let current: Vec<(PhotoId, SidecarStat)> = scan
            .photos
            .iter()
            .map(|e| (e.key, sidecar_stat(&e.stat)))
            .collect();
        let report = self.catalogue.reconcile_photos(&current)?;
        let mut changed = 0;
        for id in &report.changed {
            if let Ok((photo, stat)) = self.read_photo(id) {
                let main_version = self.main_version_of(&photo).ok().flatten();
                if self
                    .catalogue
                    .apply_photo_metadata(&photo, stat, main_version.as_ref())
                    .is_ok()
                {
                    changed += 1;
                    let _ = self.events.send(Event::PhotoChanged(*id));
                }
            }
        }
        let _ = self.events.send(Event::ReconcileFinished {
            changed_photos: changed,
            removed_photos: report.removed.len(),
        });
        Ok(Outcome::Applied)
    }

    fn read_sources(&self) -> Result<auroraw_format::state::Sources> {
        match self.workspace.read_sources()? {
            Some(loaded) => loaded.current().ok_or_else(|| EngineError::NotFound {
                kind: "sources (newer schema)",
                id: String::new(),
            }),
            None => Ok(auroraw_format::state::Sources {
                updated: Timestamp::now(),
                sources: Vec::new(),
                extra: Default::default(),
            }),
        }
    }

    fn source_entry(&self, source_id: &SourceId) -> Result<SourceEntry> {
        self.read_sources()?
            .sources
            .into_iter()
            .find(|s| s.id == *source_id)
            .ok_or_else(|| EngineError::NotFound {
                kind: "source",
                id: source_id.to_string(),
            })
    }

    /// The real path on this machine (design note 002 §6.6: never stored anywhere but the
    /// workspace's `hint` and, once registered, the catalogue).
    fn source_root(entry: &SourceEntry) -> Result<PathBuf> {
        entry
            .hint
            .get("path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .ok_or_else(|| EngineError::NotFound {
                kind: "source path (hint)",
                id: entry.id.to_string(),
            })
    }

    /// Every M1 source is a folder here or a removable volume's mount point, read the same way
    /// (`sources::filesystem`).
    fn open_source(entry: &SourceEntry) -> Result<FilesystemSource> {
        Ok(FilesystemSource::new(Self::source_root(entry)?))
    }

    fn add_source(&mut self, name: String, root: PathBuf, kind: String) -> Result<Outcome> {
        let mut sources = self.read_sources()?;
        let id = SourceId::random();
        let mut hint = serde_json::Map::new();
        hint.insert(
            "path".to_string(),
            serde_json::Value::String(root.to_string_lossy().into_owned()),
        );
        let entry = SourceEntry {
            id,
            kind,
            name,
            hint,
            ignore: Vec::new(),
            config: Default::default(),
            extra: Default::default(),
        };
        sources.sources.push(entry.clone());
        sources.updated = Timestamp::now();
        self.workspace.write_sources(&sources)?;
        self.catalogue.apply_source(&entry)?;
        let _ = self.events.send(Event::SourceAdded(id));
        Ok(Outcome::SourceAdded(id))
    }

    fn scan_source(&mut self, source_id: SourceId) -> Result<Outcome> {
        let entry = self.source_entry(&source_id)?;
        let source = Self::open_source(&entry)?;
        if source.state() != SourceState::Online {
            let _ = self.events.send(Event::SourceScanned {
                source_id,
                reachable: false,
                confirmed: 0,
                changed: 0,
                relinked: 0,
                missing: 0,
                new: Vec::new(),
                ambiguous: 0,
                second_locations: 0,
            });
            return Ok(Outcome::Scanned {
                reachable: false,
                confirmed: 0,
                changed: 0,
                relinked: 0,
                missing: 0,
                new: Vec::new(),
                ambiguous: 0,
                second_locations: 0,
            });
        }
        let found: Vec<FoundFile> = source.scan()?;
        let known_rows = self.catalogue.known_files_in_source(&source_id)?;
        // Which of these known paths is the photo's *primary* location (D-108): the others (secondary locations,
        // D-036) are the `location` table's, never the `photo` row's, and are handled differently below.
        let is_primary: HashMap<String, bool> = known_rows
            .iter()
            .map(|(_, path, _, primary)| (path.clone(), *primary))
            .collect();
        let known: Vec<KnownFile> = known_rows
            .into_iter()
            .map(|(photo_id, path, fingerprint, _)| KnownFile {
                photo_id,
                path,
                fingerprint,
            })
            .collect();
        let outcomes = relink::reconcile(&found, &known);

        let (
            mut confirmed,
            mut changed,
            mut relinked,
            mut missing,
            mut ambiguous,
            mut second_locations,
        ) = (0, 0, 0, 0, 0, 0);
        let mut new = Vec::new();
        for outcome in outcomes {
            match outcome {
                // Always safe, never only for the primary path: `reconcile` reports a genuinely missing known
                // path (if any) as a trailing `Missing` *after* every found-derived outcome of this same pass, so
                // if the primary is truly unreachable this scan, its flags are correctly reasserted later below,
                // regardless of what an unrelated secondary's `Confirmed` cleared here first.
                ScanOutcome::Confirmed { photo_id } => {
                    self.catalogue.mark_original_changed(&photo_id, false)?;
                    self.catalogue.mark_missing(&photo_id, false)?;
                    confirmed += 1;
                }
                ScanOutcome::OriginalChanged { photo_id, path } => {
                    if is_primary.get(&path).copied().unwrap_or(true) {
                        self.catalogue.mark_original_changed(&photo_id, true)?;
                    } else {
                        // Not the same content any more: not a verified duplicate. Nothing on disk is touched.
                        self.catalogue
                            .remove_location(&photo_id, &source_id, &path)?;
                    }
                    changed += 1;
                }
                ScanOutcome::Relinked { photo_id, from, to } => {
                    let filename = to.rsplit('/').next().unwrap_or(&to).to_string();
                    let fingerprint = found
                        .iter()
                        .find(|f| f.path == to)
                        .map(|f| f.fingerprint)
                        .expect("the relink target was just found");
                    if is_primary.get(&from).copied().unwrap_or(true) {
                        self.catalogue.apply_relink(
                            &photo_id,
                            &source_id,
                            &to,
                            &filename,
                            &fingerprint,
                        )?;
                    } else {
                        // A secondary location moved or was renamed within its source: follow it in `location`,
                        // never touch the photo's own row.
                        self.catalogue
                            .remove_location(&photo_id, &source_id, &from)?;
                        if let Some(hash) = self.catalogue.photo_hash(&photo_id)? {
                            self.catalogue.insert_location(
                                &photo_id,
                                &source_id,
                                &to,
                                &filename,
                                &fingerprint,
                                &hash,
                            )?;
                        }
                    }
                    relinked += 1;
                }
                ScanOutcome::Missing { photo_id, path } => {
                    if is_primary.get(&path).copied().unwrap_or(true) {
                        self.catalogue.mark_missing(&photo_id, true)?;
                    } else {
                        // The copy is simply gone: nothing to report about it any more (nothing was deleted by
                        // Auroraw; this only prunes Auroraw's own record of where a copy had been seen).
                        self.catalogue
                            .remove_location(&photo_id, &source_id, &path)?;
                    }
                    missing += 1;
                }
                ScanOutcome::New { path } => new.push(path),
                ScanOutcome::Ambiguous { .. } => ambiguous += 1,
                ScanOutcome::SecondLocation { photo_id, path } => {
                    let fingerprint = found
                        .iter()
                        .find(|f| f.path == path)
                        .map(|f| f.fingerprint)
                        .expect("the second-location path was just found");
                    if self.confirm_second_location(photo_id, source_id, &path, fingerprint)? {
                        second_locations += 1;
                    } else {
                        // The sampled fingerprint collided: not actually the same file (D-108's own residual
                        // risk, confirmed away). It is simply a new, unrelated photo.
                        new.push(path);
                    }
                }
            }
        }
        let _ = self.events.send(Event::SourceScanned {
            source_id,
            reachable: true,
            confirmed,
            changed,
            relinked,
            missing,
            new: new.clone(),
            ambiguous,
            second_locations,
        });
        Ok(Outcome::Scanned {
            reachable: true,
            confirmed,
            changed,
            relinked,
            missing,
            new,
            ambiguous,
            second_locations,
        })
    }

    /// A found file's fingerprint matched exactly one existing photo, whose old location is also still present
    /// (D-036, D-108): before recording anything, confirm with the **whole-file hash** of both copies (a sampled
    /// fingerprint alone is not proof enough to invite someone to go delete a file over it, design note 004's own
    /// residual-risk table). `true` when confirmed and recorded (the sidecar and the catalogue both); `false` on a
    /// hash mismatch (an astronomically unlikely collision: not the same file after all) or if either file could
    /// not be read (nothing recorded either way, silently — the next scan tries again).
    fn confirm_second_location(
        &mut self,
        photo_id: PhotoId,
        source_id: SourceId,
        path: &str,
        fingerprint: auroraw_types::Fingerprint,
    ) -> Result<bool> {
        let Some(root) = crate::thumbnails::source_root(&self.workspace, source_id) else {
            return Ok(false);
        };
        let Ok(bytes) = std::fs::read(root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR)))
        else {
            return Ok(false);
        };
        let Ok((_, found_hash)) =
            auroraw_format::fingerprint::content_hash(&mut std::io::Cursor::new(&bytes))
        else {
            return Ok(false);
        };
        let primary_hash = match self.catalogue.photo_hash(&photo_id)? {
            Some(hash) => hash,
            None => {
                let Some(row) = self.catalogue.photo(&photo_id)? else {
                    return Ok(false);
                };
                let (Some(primary_source), Some(primary_path)) = (row.source_id, row.path) else {
                    return Ok(false);
                };
                let Some(primary_root) =
                    crate::thumbnails::source_root(&self.workspace, primary_source)
                else {
                    return Ok(false);
                };
                let Ok(primary_bytes) = std::fs::read(
                    primary_root.join(primary_path.replace('/', std::path::MAIN_SEPARATOR_STR)),
                ) else {
                    return Ok(false);
                };
                let Ok((_, hash)) = auroraw_format::fingerprint::content_hash(
                    &mut std::io::Cursor::new(&primary_bytes),
                ) else {
                    return Ok(false);
                };
                self.catalogue.apply_hash(&photo_id, &hash)?;
                hash
            }
        };
        if primary_hash != found_hash {
            return Ok(false);
        }
        let filename = path.rsplit('/').next().unwrap_or(path).to_string();
        let _guard = self.workspace.sidecar_guard();
        let Some(mut photo) = self
            .workspace
            .read_photo(&photo_id)
            .ok()
            .flatten()
            .and_then(|loaded| loaded.current())
        else {
            return Ok(false);
        };
        for file in &mut photo.files {
            if file.fingerprint == fingerprint {
                file.hash.get_or_insert(found_hash);
                if !file
                    .locations
                    .iter()
                    .any(|l| l.source == source_id && l.path == path)
                {
                    file.locations.push(Location {
                        source: source_id,
                        path: path.to_string(),
                        seen: Some(Timestamp::now()),
                        extra: Vec::new(),
                    });
                }
            }
        }
        if self.workspace.write_photo(&photo).is_err() {
            return Ok(false);
        }
        self.catalogue.insert_location(
            &photo_id,
            &source_id,
            path,
            &filename,
            &fingerprint,
            &found_hash,
        )?;
        Ok(true)
    }

    /// A file about to be confirmed as a new photo (`AddNewPhotos`) whose fingerprint already matches an existing
    /// one, from any source (D-036, D-108): `find_by_fingerprint` is not source-scoped, unlike `scan_source`'s own
    /// check. Confirmed the same way, by a whole-file hash. `Some(photo_id)` when joined (nothing new is added for
    /// this path); `None` on no candidate or no confirmed match, in which case it becomes its own new photo as
    /// usual.
    fn try_join_new_file(
        &mut self,
        source_id: SourceId,
        path: &str,
        fingerprint: auroraw_types::Fingerprint,
    ) -> Result<Option<PhotoId>> {
        for candidate in self.catalogue.find_by_fingerprint(&fingerprint)? {
            if self.confirm_second_location(candidate.photo_id, source_id, path, fingerprint)? {
                return Ok(Some(candidate.photo_id));
            }
        }
        Ok(None)
    }

    fn add_new_photos(&mut self, source_id: SourceId, paths: Vec<String>) -> Result<Outcome> {
        let entry = self.source_entry(&source_id)?;
        let source = Self::open_source(&entry)?;
        let mut added = Vec::new();
        for path in paths {
            let fingerprint = source.fingerprint_of(&path)?;
            if self
                .try_join_new_file(source_id, &path, fingerprint)?
                .is_some()
            {
                continue;
            }
            let stat = source.stat(&path)?;
            let filename = path.rsplit('/').next().unwrap_or(&path).to_string();
            let photo_id = PhotoId::random();
            let mut photo = PhotoSidecar::new(photo_id);
            photo.imported = Some(Timestamp::now());
            photo.files.push(FileEntry {
                role: FileRole::Original,
                name: filename,
                format: None,
                size: stat.size,
                fingerprint,
                hash: None,
                locations: vec![Location {
                    source: source_id,
                    path: path.clone(),
                    seen: Some(Timestamp::now()),
                    extra: Vec::new(),
                }],
                extra: Vec::new(),
            });
            self.workspace.write_photo(&photo)?;
            let (photo, stat) = self.read_photo(&photo_id)?;
            self.catalogue.apply_new_photo(&photo, stat)?;
            added.push(photo_id);
        }
        let _ = self.events.send(Event::PhotosAdded {
            source_id,
            count: added.len(),
        });
        Ok(Outcome::PhotosAdded(added))
    }

    /// Resolves `path` (`"Family|Wedding"`) against `vocabulary`, creating whatever segment does
    /// not exist yet, and returns the leaf's identifier. Mutates `vocabulary` in place; the
    /// caller writes it and applies every touched keyword to the catalogue once, after resolving
    /// every path a profile's metadata template names, not once per segment.
    fn resolve_keyword_path(vocabulary: &mut Vocabulary, path: &str) -> Option<KeywordId> {
        let mut parent: Option<KeywordId> = None;
        for segment in path.split('|').filter(|s| !s.is_empty()) {
            let existing = vocabulary
                .keywords
                .iter()
                .find(|k| k.name == segment && k.parent == parent)
                .map(|k| k.id);
            parent = Some(existing.unwrap_or_else(|| {
                let id = KeywordId::random();
                vocabulary.keywords.push(KeywordEntry {
                    id,
                    name: segment.to_string(),
                    parent,
                    synonyms: Vec::new(),
                    export: true,
                    extra: Default::default(),
                });
                id
            }));
        }
        parent
    }

    #[allow(clippy::too_many_arguments)]
    fn start_import(
        &mut self,
        source_root: PathBuf,
        destination_root: PathBuf,
        registration: Option<crate::import_job::Registration>,
        mut profile: Profile,
        shoot: Option<String>,
        backup_roots: Vec<PathBuf>,
        state_path: PathBuf,
    ) -> Result<Outcome> {
        let source = FilesystemSource::new(source_root);
        let dest_root = destination_root;
        // A plain copy writes no sidecar, so there is nowhere for the metadata template to go.
        if registration.is_none() {
            profile.metadata_template = Default::default();
        }

        // Every keyword the profile's template names is resolved (and created if needed) once,
        // here, before the background job starts: per-photo would mean racing to create "the
        // same" new keyword from several photos at once, and the vocabulary is small enough that
        // there is no cost to doing it all up front.
        let mut vocabulary = self.read_vocabulary()?;
        let mut template_keywords = Vec::new();
        for path in &profile.metadata_template.keyword_paths {
            if let Some(id) = Self::resolve_keyword_path(&mut vocabulary, path) {
                template_keywords.push((id, path.clone()));
            }
        }
        vocabulary.updated = Timestamp::now();
        self.workspace.write_vocabulary(&vocabulary)?;
        let paths = auroraw_catalogue::keyword_paths(&vocabulary.keywords);
        for (id, _) in &template_keywords {
            if let (Some(entry), Some(path)) = (
                vocabulary.keywords.iter().find(|k| k.id == *id),
                paths.get(id),
            ) {
                self.catalogue.apply_keyword(entry, path)?;
            }
        }

        let job = self.spawn_job();
        let catalogue_path = self
            .catalogue
            .path()
            .expect("the engine's catalogue is always a file")
            .to_path_buf();
        import_job::spawn(ImportJob {
            job,
            workspace: self.workspace.clone(),
            source,
            dest_root,
            registration,
            profile,
            shoot,
            backup_roots,
            state_path,
            catalogue_path,
            template_keywords,
            events: self.events.clone(),
            inbound: self.inbound.clone(),
            cancel: self.jobs[&job].clone(),
        });
        let _ = self.events.send(Event::ImportStarted { job });
        Ok(Outcome::ImportStarted { job })
    }

    /// Starts the background scan of a source (`Command::IndexSource`).
    fn start_index(&mut self, source_id: SourceId, merge: Vec<SourceId>) -> Result<Outcome> {
        let entry = self.source_entry(&source_id)?;
        let root = Self::source_root(&entry)?;
        let source = Self::open_source(&entry)?;
        let slash = |relative: PathBuf| {
            relative
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/")
        };
        // Other sources whose folder is inside this one: their files belong to them, unless they
        // are being merged into this one, in which case they are this source's own.
        let mut skip = Vec::new();
        let mut merged = Vec::new();
        for other in self.read_sources()?.sources {
            if other.id == source_id {
                continue;
            }
            let Ok(other_root) = Self::source_root(&other) else {
                continue;
            };
            let Some(relative) = crate::paths::relative_to(&other_root, &root)
                .filter(|relative| !relative.as_os_str().is_empty())
            else {
                continue;
            };
            if merge.contains(&other.id) {
                merged.push((other.id, slash(relative)));
            } else {
                skip.push(slash(relative));
            }
        }
        let job = self.spawn_job();
        let (answer_tx, answer_rx) = mpsc::channel();
        self.index_decisions.insert(job, answer_tx);
        let catalogue_path = self
            .catalogue
            .path()
            .expect("the engine's catalogue is always a file")
            .to_path_buf();
        index_job::spawn(IndexJob {
            job,
            workspace: self.workspace.clone(),
            source,
            source_id,
            skip,
            merge: merged,
            catalogue_path,
            events: self.events.clone(),
            inbound: self.inbound.clone(),
            cancel: self.jobs[&job].clone(),
            decision: answer_rx,
        });
        Ok(Outcome::IndexStarted { job })
    }

    /// Starts taking a source out (`Command::RemoveSource`).
    fn start_remove(&mut self, source_id: SourceId) -> Result<Outcome> {
        self.source_entry(&source_id)?;
        let job = self.spawn_job();
        let catalogue_path = self
            .catalogue
            .path()
            .expect("the engine's catalogue is always a file")
            .to_path_buf();
        remove_job::spawn(RemoveJob {
            job,
            workspace: self.workspace.clone(),
            source_id,
            catalogue_path,
            events: self.events.clone(),
            inbound: self.inbound.clone(),
            cancel: self.jobs[&job].clone(),
        });
        Ok(Outcome::RemoveStarted { job })
    }

    /// The photos of a removed source are gone: take the source out of the workspace's list and
    /// the catalogue's.
    fn finish_remove_source(
        &mut self,
        job: JobId,
        source_id: SourceId,
        removed: usize,
        kept: usize,
        announce: bool,
    ) {
        if let Ok(mut sources) = self.read_sources() {
            sources.sources.retain(|entry| entry.id != source_id);
            sources.updated = Timestamp::now();
            if self.workspace.write_sources(&sources).is_ok() {
                let _ = self.catalogue.remove_source(&source_id);
                if announce {
                    let _ = self.events.send(Event::SourceRemoved {
                        job,
                        source_id,
                        removed,
                        kept,
                    });
                }
            }
        }
    }

    fn handle_imported(
        &mut self,
        photo: Option<PhotoSidecar>,
        stat: Option<SidecarStat>,
        backfill: Option<(PhotoId, ContentHash)>,
    ) {
        if let Some((id, hash)) = backfill {
            let _ = self.catalogue.apply_hash(&id, &hash);
        }
        if let (Some(photo), Some(stat)) = (photo, stat) {
            let id = photo.photo_id;
            if self.catalogue.apply_new_photo(&photo, stat).is_ok() {
                let _ = self.events.send(Event::PhotoChanged(id));
            }
        }
    }
}

/// `id` and every keyword whose parent chain includes it.
fn descendants_of(vocabulary: &[KeywordEntry], id: KeywordId) -> Vec<KeywordId> {
    let by_id: HashMap<KeywordId, &KeywordEntry> = vocabulary.iter().map(|k| (k.id, k)).collect();
    let mut out = Vec::new();
    for entry in vocabulary {
        let mut current = Some(entry.id);
        let mut seen = std::collections::HashSet::new();
        while let Some(cur) = current {
            if cur == id {
                out.push(entry.id);
                break;
            }
            if !seen.insert(cur) {
                break;
            }
            current = by_id.get(&cur).and_then(|k| k.parent);
        }
    }
    out
}

/// A keyword's name as it will be kept: trimmed, not empty, without the `|` that separates a path.
fn checked_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.contains('|') {
        return Err(EngineError::KeywordName);
    }
    Ok(name.to_string())
}

fn find_keyword(vocabulary: &[KeywordEntry], id: KeywordId) -> Result<&KeywordEntry> {
    vocabulary
        .iter()
        .find(|k| k.id == id)
        .ok_or_else(|| EngineError::NotFound {
            kind: "keyword",
            id: id.to_string(),
        })
}

/// Two siblings do not share a name (whatever the case); `except` is the keyword being renamed or moved.
fn ensure_name_is_free(
    vocabulary: &[KeywordEntry],
    name: &str,
    parent: Option<KeywordId>,
    except: Option<KeywordId>,
) -> Result<()> {
    let wanted = name.to_lowercase();
    if vocabulary
        .iter()
        .any(|k| k.parent == parent && Some(k.id) != except && k.name.to_lowercase() == wanted)
    {
        return Err(EngineError::KeywordNameTaken(name.to_string()));
    }
    Ok(())
}
