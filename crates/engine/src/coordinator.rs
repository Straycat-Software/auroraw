// SPDX-License-Identifier: GPL-3.0-or-later
//! The single writer (architecture §4.2): one thread, owning the only mutable `Catalogue`
//! connection, applying commands strictly one at a time. Everything that changes the workspace or
//! the catalogue goes through here, whichever thread asked for it.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::SystemTime;

use auroraw_catalogue::{Catalogue, ExternalPending, ExternalStat, SidecarStat};
use auroraw_format::sidecar::external::{
    Field as ExternalField, Fields, Merge, keyword_key, merge,
};
use auroraw_format::sidecar::{FileEntry, FileRole, Location, PhotoSidecar, VersionSidecar};
use auroraw_format::state::{KeywordEntry, SourceEntry, Vocabulary};
use auroraw_import::{DiscoveredFile, PairRule, Profile, pair_files};
use auroraw_plugin_api::source::{Source, SourceState};
use auroraw_sources::filesystem::FilesystemSource;
use auroraw_sources::relink::{self, FoundFile, KnownFile, ScanOutcome};
use auroraw_types::{CollectionId, ContentHash, KeywordId, PhotoId, SeriesId, SourceId, Timestamp};
use auroraw_workspace::{FileStat, Workspace};

use crate::batch_job;
use crate::command::Command;
use crate::error::{EngineError, Result};
use crate::event::Event;
use crate::external_merge::{metadata_field, mine_of, pending_diff};
use crate::external_xmp::ExternalSeen;
use crate::history::{
    Change, Direction, Entry, History, HistoryState, KeywordDelta, KeywordSet, Label, LabelKind,
    VocabularyAction,
};
use crate::import_job::{self, ImportJob};
use crate::index_job::{self, IndexJob};
use crate::job::{CancelToken, JobId};
use crate::reconcile_apply::{self, CatalogueAction};
use crate::remove_job::{self, RemoveJob};

/// What a command that waits for its result (`Engine::submit_and_wait`) gets back.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The command applied with nothing more specific to report.
    Applied,
    /// `CreateKeyword`'s new identifier.
    KeywordCreated(KeywordId),
    /// `CreateCollection`'s new identifier.
    CollectionCreated(CollectionId),
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
    /// `MoveKeyword`'s and a small `DeleteKeyword`'s report: how many keywords the action touched and how
    /// many photos it changed (a move: how many sidecars will have their paths refreshed).
    KeywordsChanged {
        /// Keywords moved or deleted (with their branch).
        keywords: usize,
        /// Photos changed.
        photos: usize,
    },
    /// A `DeleteKeyword` past [`crate::batch_job::BACKGROUND_THRESHOLD`] photos started as a background
    /// sweep instead (D-126 volet B): its job, and how many keywords and photos it will touch, if it runs
    /// to completion. [`Event::KeywordDeleted`] reports how it actually ended.
    DeleteKeywordStarted {
        /// The job doing the sweep.
        job: JobId,
        /// How many keywords the branch has.
        keywords: usize,
        /// How many photos carry the branch right now.
        photos: usize,
    },
    /// A `Batch` past [`crate::batch_job::BACKGROUND_THRESHOLD`] items started as a background job
    /// instead (D-126 volet B): its id, for progress and cancellation. Unlike a small batch, this is not
    /// yet applied when it is returned.
    BatchStarted {
        /// The job applying the batch.
        job: JobId,
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
    /// `AcceptExternalChanges` or `IgnoreExternalChanges`: how many photos had a change waiting and were
    /// answered.
    ExternalResolved {
        /// How many photos.
        photos: usize,
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
    /// A remove job decided a photo has no other location: move its version files and its own sidecar to
    /// `removed/` (recoverably), then take its row out of the catalogue. The coordinator's own writer, not
    /// the job's: the decision and this message are sent adjacent to each other, so nothing else gets a
    /// chance to write this exact photo in between.
    Removed { photo_id: PhotoId },
    /// A remove job decided a photo also has a location outside the source being removed (`dropped_source`):
    /// drop that source's `Location`s from a fresh read of the sidecar and point the catalogue at what
    /// remains. An index job merging sources sends [`Inbound::MergeLocations`] instead (it rewrites rather
    /// than drops).
    Relocated {
        photo_id: PhotoId,
        dropped_source: SourceId,
        source_id: SourceId,
        path: String,
        filename: String,
        fingerprint: auroraw_types::Fingerprint,
    },
    /// An index job merging sources decided a photo's `Location`s under `from_source` now belong to
    /// `into_source`, with `prefix` ahead of their path (the merged folder's place in the surviving
    /// source): rewrite them in a fresh read of the sidecar and point the catalogue at the new primary one.
    MergeLocations {
        photo_id: PhotoId,
        from_source: SourceId,
        into_source: SourceId,
        prefix: String,
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
    /// an existing photo from a *different* source (D-036, D-108): push or update that `Location` in a fresh
    /// read of the sidecar (setting the file's own hash too, now that it is known), write it, and record it in
    /// the catalogue.
    LocationAdded {
        photo_id: PhotoId,
        source_id: SourceId,
        path: String,
        filename: String,
        fingerprint: auroraw_types::Fingerprint,
        hash: ContentHash,
    },
    /// An index job reconciled a known file against a fresh scan (`sources::relink::reconcile`, the same
    /// function `scan_source` uses) and worked out what it means for the catalogue
    /// (`reconcile_apply::CatalogueAction`, D-109): applied here, the coordinator's only writer, exactly the
    /// way `scan_source` applies its own directly.
    Apply(CatalogueAction),
    /// The last [`crate::Engine`] handle was dropped: cancel what runs in the background and stop.
    /// (The coordinator holds a sender to its own queue for the jobs it starts, so a closed queue
    /// never signals the end by itself.)
    Stop,
    Command {
        command: Command,
        reply: Option<Reply>,
    },
    /// A keyword path-refresh job asks that one photo's `keyword_paths` be brought in line with `paths`
    /// (identifier to fresh path): a fresh read, the substitution, the write, all on the coordinator thread,
    /// like every other edit (the job itself no longer touches the sidecar).
    Refreshed {
        photo_id: PhotoId,
        paths: Arc<HashMap<KeywordId, String>>,
    },
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
    /// An index job needs these keyword paths (`"Fauna|Birds"`) resolved against the vocabulary, creating
    /// what is missing (as the import template does: no history entry), to give a new photo the keywords
    /// of the XMP file beside it (D-047, WP10). Answers, in order, each path's identifier and its
    /// canonical spelling in the vocabulary (`None` for a path with nothing in it, or if the vocabulary
    /// could not be written). The job waits for the answer: it needs the identifiers before it writes the
    /// photo's sidecar.
    ResolveKeywords {
        paths: Vec<String>,
        reply: mpsc::Sender<Vec<Option<(KeywordId, String)>>>,
    },
    /// An index job read the external XMP files (D-047) of these photos: record what was read as their
    /// base (slice 1: silently; a photo's own sidecar was already given the file's fields by the job when
    /// it is new).
    ExternalXmp { seen: Vec<ExternalSeen> },
    /// An index job is done with the external XMP files of `source_id`: `unreadable` of them could not be
    /// read at all.
    ExternalDone {
        source_id: SourceId,
        unreadable: usize,
    },
    /// A `batch_job` (a large `Batch` or `DeleteKeyword` sweep, D-126 volet B) applied one item: `edit`
    /// is run through [`Coordinator::apply_edit`], exactly as a small batch's own loop would run it,
    /// and `ack` is signalled once it lands, so the job thread paces its next send to the coordinator's
    /// own speed (`crate::batch_job`'s own doc comment says why that matters for cancellation).
    BatchItem {
        job: JobId,
        edit: Command,
        ack: mpsc::Sender<()>,
    },
    /// A `batch_job` is over, finished or cancelled: apply what its completion means (one history
    /// entry either way, and, for a `DeleteKeyword` sweep, the vocabulary branch too, once every photo
    /// has lost it).
    BatchDone { job: JobId },
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

#[path = "collection_ops.rs"]
mod collection_ops;
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
    /// Background batch jobs in flight (D-126 volet B), by the job that is applying them.
    batch_jobs: HashMap<JobId, BatchJob>,
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

/// A background batch job in flight (D-126 volet B): what it has applied so far, and what its
/// completion means.
struct BatchJob {
    /// The changes its items have made so far, in the order they landed.
    changes: Vec<Change>,
    kind: BatchJobKind,
}

enum BatchJobKind {
    /// A `Command::Batch` past the threshold: finishing it just records whatever landed, cancelled
    /// or not.
    Batch,
    /// A `Command::DeleteKeyword` past the threshold: finishing it re-checks the catalogue fresh and
    /// removes the vocabulary branch too, but only once no photo carries any of it any more (whether
    /// because the sweep ran to completion, or happened to be cancelled at exactly that point makes
    /// no difference) — never while a photo might still name a keyword the vocabulary no longer has.
    DeleteKeyword {
        keyword_id: KeywordId,
        branch: Vec<KeywordId>,
    },
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
            batch_jobs: HashMap::new(),
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
                    if let Ok((mut photo, _)) = self.read_photo(&photo_id) {
                        for file in &mut photo.files {
                            if file.fingerprint == fingerprint {
                                file.hash.get_or_insert(hash);
                                if !file
                                    .locations
                                    .iter()
                                    .any(|l| l.source == source_id && l.path == path)
                                {
                                    file.locations.push(Location {
                                        source: source_id,
                                        path: path.clone(),
                                        seen: Some(Timestamp::now()),
                                        extra: Vec::new(),
                                    });
                                }
                            }
                        }
                        let _ = self.persist_photo(photo);
                    }
                    let _ = self.catalogue.insert_location(
                        &photo_id,
                        &source_id,
                        &path,
                        &filename,
                        &fingerprint,
                        &hash,
                    );
                }
                Inbound::Apply(action) => {
                    let _ = self.apply_catalogue_action(action);
                }
                Inbound::Removed { photo_id } => {
                    for version in self.workspace.version_files_of(&photo_id) {
                        let _ = self.workspace.remove_recoverably(&version);
                    }
                    let sidecar_path = self.workspace.photo_path(&photo_id);
                    if sidecar_path.exists() {
                        let _ = self.workspace.remove_recoverably(&sidecar_path);
                    }
                    // A photo that leaves a series shrinks it, and it leaves its collections (both read the
                    // catalogue's rows about the photo, so before they go).
                    self.leave_series_on_removal(photo_id);
                    self.leave_collections_on_removal(photo_id);
                    let _ = self.catalogue.remove_photo(&photo_id);
                    // What was done to a photo that has left cannot be undone.
                    if self.history.forget_photo(photo_id) {
                        self.report_history();
                    }
                }
                Inbound::Relocated {
                    photo_id,
                    dropped_source,
                    source_id,
                    path,
                    filename,
                    fingerprint,
                } => {
                    if let Ok((mut photo, _)) = self.read_photo(&photo_id) {
                        for file in &mut photo.files {
                            file.locations.retain(|l| l.source != dropped_source);
                        }
                        let _ = self.persist_photo(photo);
                    }
                    let _ = self.catalogue.apply_relink(
                        &photo_id,
                        &source_id,
                        &path,
                        &filename,
                        &fingerprint,
                    );
                }
                Inbound::MergeLocations {
                    photo_id,
                    from_source,
                    into_source,
                    prefix,
                } => {
                    let mut primary = None;
                    if let Ok((mut photo, _)) = self.read_photo(&photo_id) {
                        for file in &mut photo.files {
                            for location in &mut file.locations {
                                if location.source == from_source {
                                    location.source = into_source;
                                    location.path = format!("{prefix}/{}", location.path);
                                }
                            }
                            if primary.is_none()
                                && file.role == FileRole::Original
                                && let Some(location) = file.locations.first()
                            {
                                primary = Some((
                                    file.name.clone(),
                                    file.fingerprint,
                                    location.path.clone(),
                                ));
                            }
                        }
                        let _ = self.persist_photo(photo);
                    }
                    if let Some((filename, fingerprint, path)) = primary {
                        let _ = self.catalogue.apply_relink(
                            &photo_id,
                            &into_source,
                            &path,
                            &filename,
                            &fingerprint,
                        );
                    }
                }
                Inbound::SourceGone {
                    job,
                    source_id,
                    removed,
                    kept,
                    announce,
                } => self.finish_remove_source(job, source_id, removed, kept, announce),
                Inbound::Command { command, reply } => self.handle_command(command, reply),
                Inbound::Refreshed { photo_id, paths } => self.handle_refreshed(photo_id, &paths),
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
                Inbound::BatchItem { job, edit, ack } => {
                    if let Ok(Some(change)) = self.apply_edit(&edit)
                        && let Some(state) = self.batch_jobs.get_mut(&job)
                    {
                        state.changes.push(change);
                    } // an Err (the photo left meanwhile) or a no-op edit is simply skipped: no rollback,
                    // matching remove_job/index_job's own "what was done stays done" precedent.
                    let _ = ack.send(());
                }
                Inbound::BatchDone { job } => self.finish_batch_job(job),
                Inbound::ResolveKeywords { paths, reply } => {
                    let _ = reply.send(self.resolve_keywords(&paths));
                }
                Inbound::ExternalXmp { seen } => self.handle_external_seen(seen),
                Inbound::ExternalDone {
                    source_id,
                    unreadable,
                } => {
                    let photos = self.settle_external();
                    let _ = self.events.send(Event::ExternalChanges {
                        source_id: Some(source_id),
                        photos,
                        unreadable,
                    });
                }
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
            | Command::SetMetadataField { .. }
            | Command::AddKeyword { .. }
            | Command::RemoveKeyword { .. }
            | Command::RemoveKeywords { .. } => {
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
            Command::SetKeywordProperties {
                keyword_id,
                synonyms,
                export,
            } => self.set_keyword_properties(keyword_id, synonyms, export),
            Command::CreateCollection {
                name,
                parent,
                id,
                photos,
            } => self.create_collection(name, parent, id, photos),
            Command::RenameCollection {
                collection_id,
                new_name,
            } => self.rename_collection(collection_id, new_name),
            Command::MoveCollection {
                collection_id,
                new_parent,
            } => self.move_collection(collection_id, new_parent),
            Command::DeleteCollection { collection_id } => self.delete_collection(collection_id),
            Command::AddToCollection {
                collection_id,
                photos,
            } => self.add_to_collection(collection_id, photos),
            Command::RemoveFromCollection {
                collection_id,
                photos,
            } => self.remove_from_collection(collection_id, photos),
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
            Command::AcceptExternalChanges { photos, use_file } => {
                self.accept_external(photos, use_file)
            }
            Command::IgnoreExternalChanges { photos } => self.ignore_external(photos),
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
                backup_roots,
                state_path,
            } => self.start_import(
                source_root,
                destination_root,
                registration,
                profile,
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
            Command::SetMetadataField {
                photo_id,
                field,
                value,
            } => self.edit_photo(*photo_id, |m| {
                let before = field.get(m);
                (before != *value).then(|| {
                    field.set(m, value.clone());
                    Change::Metadata {
                        photo: *photo_id,
                        field: field.clone(),
                        before,
                        after: value.clone(),
                    }
                })
            }),
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
            Command::RemoveKeywords {
                photo_id,
                keyword_ids,
            } => self.edit_photo(*photo_id, |m| {
                if !m.keyword_ids.iter().any(|k| keyword_ids.contains(k)) {
                    return None;
                }
                let before = keyword_set(m);
                let mut i = 0;
                while i < m.keyword_ids.len() {
                    if keyword_ids.contains(&m.keyword_ids[i]) {
                        m.keyword_ids.remove(i);
                        if i < m.keyword_paths.len() {
                            m.keyword_paths.remove(i);
                        }
                    } else {
                        i += 1;
                    }
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

    /// Applies several edits as one action: one step in the history, all or nothing. Past
    /// `batch_job::BACKGROUND_THRESHOLD` items, runs as a background job instead (D-126 volet B):
    /// see `Command::Batch`'s own doc comment for what changes.
    fn batch(&mut self, commands: Vec<Command>) -> Result<Outcome> {
        if commands.len() > batch_job::BACKGROUND_THRESHOLD {
            let job = self.spawn_job();
            self.batch_jobs.insert(
                job,
                BatchJob {
                    changes: Vec::new(),
                    kind: BatchJobKind::Batch,
                },
            );
            batch_job::spawn(
                job,
                commands,
                self.events.clone(),
                self.inbound.clone(),
                self.jobs[&job].clone(),
            );
            return Ok(Outcome::BatchStarted { job });
        }
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
        if let Change::Collections { deltas, .. } = change {
            return self.write_collections(deltas, direction);
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

    /// Sets a keyword's synonyms and export flag together, as one step (spec §5.7, D-045, WP10
    /// slice 2). No sidecar snapshot depends on either field, so unlike a rename this does not
    /// force a background job when the keyword carries no photos.
    fn set_keyword_properties(
        &mut self,
        keyword_id: KeywordId,
        synonyms: Vec<String>,
        export: bool,
    ) -> Result<Outcome> {
        let vocabulary = self.read_vocabulary()?;
        let entry = find_keyword(&vocabulary.keywords, keyword_id)?.clone();
        if entry.synonyms == synonyms && entry.export == export {
            return Ok(Outcome::Applied);
        }
        let delta = KeywordDelta {
            id: keyword_id,
            after: Some(KeywordEntry {
                synonyms,
                export,
                ..entry.clone()
            }),
            before: Some(entry),
        };
        let refresh =
            self.apply_vocabulary(std::slice::from_ref(&delta), Direction::Redo, false)?;
        self.record(vec![Change::Vocabulary {
            action: VocabularyAction::SetProperties,
            keywords: vec![delta],
        }]);
        Ok(Outcome::KeywordsChanged {
            keywords: 1,
            photos: refresh.affected,
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
    /// a sidecar naming a keyword that is gone. Up to `batch_job::BACKGROUND_THRESHOLD` photos, all or
    /// nothing, synchronously; past it, a background sweep instead (D-126 volet B), `DeleteKeyword`'s
    /// own doc comment says what changes.
    fn delete_keyword(&mut self, keyword_id: KeywordId) -> Result<Outcome> {
        let vocabulary = self.read_vocabulary()?;
        find_keyword(&vocabulary.keywords, keyword_id)?;
        let branch = descendants_of(&vocabulary.keywords, keyword_id);
        let photos = self.catalogue.photos_with_keywords(&branch)?;

        if photos.len() > batch_job::BACKGROUND_THRESHOLD {
            let job = self.spawn_job();
            self.batch_jobs.insert(
                job,
                BatchJob {
                    changes: Vec::new(),
                    kind: BatchJobKind::DeleteKeyword {
                        keyword_id,
                        branch: branch.clone(),
                    },
                },
            );
            let items = photos
                .iter()
                .map(|p| Command::RemoveKeywords {
                    photo_id: *p,
                    keyword_ids: branch.clone(),
                })
                .collect();
            batch_job::spawn(
                job,
                items,
                self.events.clone(),
                self.inbound.clone(),
                self.jobs[&job].clone(),
            );
            return Ok(Outcome::DeleteKeywordStarted {
                job,
                keywords: branch.len(),
                photos: photos.len(),
            });
        }

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
        let _ = self.events.send(Event::KeywordDeleted {
            keyword_id,
            job: None,
            keywords: branch.len(),
            photos: touched,
            finished: true,
        });
        Ok(Outcome::KeywordsChanged {
            keywords: branch.len(),
            photos: touched,
        })
    }

    /// A background batch job (`Command::Batch` or a `DeleteKeyword` sweep, D-126 volet B) is over,
    /// finished or cancelled: records the one history entry it makes either way, and, for a
    /// `DeleteKeyword` sweep that left no photo still carrying the branch, removes it from the
    /// vocabulary too (re-checked fresh here, not assumed from whether it was cancelled: a photo that
    /// picked the keyword back up from an unrelated command while the sweep ran is exactly as real a
    /// reason to keep the branch as one the sweep had not reached yet).
    fn finish_batch_job(&mut self, job: JobId) {
        self.jobs.remove(&job);
        let Some(state) = self.batch_jobs.remove(&job) else {
            return;
        };
        match state.kind {
            BatchJobKind::Batch => self.record(state.changes),
            BatchJobKind::DeleteKeyword { keyword_id, branch } => {
                let mut changes = state.changes;
                let touched = changes.len();
                let finished = matches!(
                    self.catalogue.photos_with_keywords(&branch),
                    Ok(remaining) if remaining.is_empty()
                );
                if finished && let Ok(vocabulary) = self.read_vocabulary() {
                    let deltas: Vec<KeywordDelta> = branch
                        .iter()
                        .filter_map(|id| vocabulary.keywords.iter().find(|k| k.id == *id))
                        .map(|entry| KeywordDelta {
                            id: entry.id,
                            before: Some(entry.clone()),
                            after: None,
                        })
                        .collect();
                    if !deltas.is_empty()
                        && self
                            .apply_vocabulary(&deltas, Direction::Redo, false)
                            .is_ok()
                    {
                        changes.push(Change::Vocabulary {
                            action: VocabularyAction::Delete,
                            keywords: deltas,
                        });
                    }
                }
                self.record(changes);
                let _ = self.events.send(Event::KeywordDeleted {
                    keyword_id,
                    job: Some(job),
                    keywords: branch.len(),
                    photos: touched,
                    finished,
                });
            }
        }
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

    fn handle_refreshed(&mut self, id: PhotoId, paths: &HashMap<KeywordId, String>) {
        // A photo that has gone since (a removed source) is not an error: reconcile would catch it.
        let Ok((mut photo, _)) = self.read_photo(&id) else {
            return;
        };
        for (path, keyword_id) in photo
            .meta
            .keyword_paths
            .iter_mut()
            .zip(photo.meta.keyword_ids.iter())
        {
            if let Some(fresh) = paths.get(keyword_id) {
                *path = fresh.clone();
            }
        }
        let _ = self.persist_photo(photo);
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
        // Paired (D-032) before reconciling, not after: a companion's own fingerprint is never part of what the
        // catalogue tracks for a photo (only its original's is, one row per photo, D-109), so only a group's
        // original is fed to `reconcile` — a companion is never independently tested against `known` and can
        // never become a phantom "new" file of its own on a later rescan.
        let listed: Vec<DiscoveredFile> = source
            .list()?
            .into_iter()
            .filter(|entry| auroraw_imaging::is_photo_file(Path::new(&entry.path)))
            .map(|entry| DiscoveredFile {
                path: entry.path,
                size: entry.size,
                capture_time: None,
                camera: None,
            })
            .collect();
        let groups = pair_files(listed, PairRule::Both);
        let mut found = Vec::with_capacity(groups.len());
        // A group's companion is never fed to `reconcile` (above); kept here only so that a group whose
        // original turns out to be genuinely new (or an unconfirmed fingerprint collision, below) still offers
        // its companion too, exactly as a scan always has (`AddNewPhotos` has no pairing of its own either way,
        // D-109's own named limit).
        let mut companion_of: HashMap<String, String> = HashMap::new();
        for group in &groups {
            found.push(FoundFile {
                path: group.original.path.clone(),
                fingerprint: source.fingerprint_of(&group.original.path)?,
            });
            if let Some(companion) = &group.companion {
                companion_of.insert(group.original.path.clone(), companion.path.clone());
            }
        }
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
                    self.apply_catalogue_action(reconcile_apply::changed(
                        photo_id,
                        source_id,
                        path,
                        &is_primary,
                    ))?;
                    changed += 1;
                }
                ScanOutcome::Relinked { photo_id, from, to } => {
                    let filename = to.rsplit('/').next().unwrap_or(&to).to_string();
                    let fingerprint = found
                        .iter()
                        .find(|f| f.path == to)
                        .map(|f| f.fingerprint)
                        .expect("the relink target was just found");
                    self.apply_catalogue_action(reconcile_apply::relinked(
                        photo_id,
                        source_id,
                        from,
                        to,
                        filename,
                        fingerprint,
                        &is_primary,
                    ))?;
                    relinked += 1;
                }
                ScanOutcome::Missing { photo_id, path } => {
                    self.apply_catalogue_action(reconcile_apply::missing(
                        photo_id,
                        source_id,
                        path,
                        &is_primary,
                    ))?;
                    missing += 1;
                }
                ScanOutcome::New { path } => {
                    if let Some(companion) = companion_of.get(&path) {
                        new.push(companion.clone());
                    }
                    new.push(path);
                }
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
                        if let Some(companion) = companion_of.get(&path) {
                            new.push(companion.clone());
                        }
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

    /// Applies one `reconcile_apply::CatalogueAction` (D-109): the decision (primary vs secondary) was already
    /// made, by `scan_source` itself or by an index job that sent it here as `Inbound::Apply`; this is the one
    /// place that turns it into the matching `Catalogue` calls, so the two callers cannot drift on what an
    /// outcome means again.
    fn apply_catalogue_action(&mut self, action: CatalogueAction) -> Result<()> {
        match action {
            CatalogueAction::MarkChanged { photo_id } => {
                self.catalogue.mark_original_changed(&photo_id, true)?;
                Ok(())
            }
            CatalogueAction::DropLocation {
                photo_id,
                source_id,
                path,
            } => {
                self.catalogue
                    .remove_location(&photo_id, &source_id, &path)?;
                Ok(())
            }
            CatalogueAction::Relink {
                photo_id,
                source_id,
                to,
                filename,
                fingerprint,
            } => {
                self.catalogue
                    .apply_relink(&photo_id, &source_id, &to, &filename, &fingerprint)?;
                Ok(())
            }
            CatalogueAction::RelinkLocation {
                photo_id,
                source_id,
                from,
                to,
                filename,
                fingerprint,
            } => {
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
                Ok(())
            }
            CatalogueAction::MarkMissing { photo_id } => {
                self.catalogue.mark_missing(&photo_id, true)?;
                Ok(())
            }
        }
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
    pub(crate) fn resolve_keyword_path(
        vocabulary: &mut Vocabulary,
        path: &str,
    ) -> Option<KeywordId> {
        let mut parent: Option<KeywordId> = None;
        for segment in path.split('|').map(str::trim).filter(|s| !s.is_empty()) {
            // Whatever the case, as the vocabulary itself does (`ensure_name_is_free`): "heron" under
            // "Birds" is the "Heron" already there, not a second one.
            let wanted = segment.to_lowercase();
            let existing = vocabulary
                .keywords
                .iter()
                .find(|k| k.parent == parent && k.name.to_lowercase() == wanted)
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

    /// Resolves keyword paths for an index job (`Inbound::ResolveKeywords`): the vocabulary is read once,
    /// written once if anything was created, and the new keywords go into the catalogue, parents first.
    fn resolve_keywords(&mut self, paths: &[String]) -> Vec<Option<(KeywordId, String)>> {
        let none = || vec![None; paths.len()];
        let Ok(mut vocabulary) = self.read_vocabulary() else {
            return none();
        };
        let before: std::collections::HashSet<KeywordId> =
            vocabulary.keywords.iter().map(|k| k.id).collect();
        let ids: Vec<Option<KeywordId>> = paths
            .iter()
            .map(|path| Self::resolve_keyword_path(&mut vocabulary, path))
            .collect();
        let key_paths = auroraw_catalogue::keyword_paths(&vocabulary.keywords);
        let mut created: Vec<&KeywordEntry> = vocabulary
            .keywords
            .iter()
            .filter(|k| !before.contains(&k.id))
            .collect();
        if !created.is_empty() {
            let mut written = vocabulary.clone();
            written.updated = Timestamp::now();
            if self.workspace.write_vocabulary(&written).is_err() {
                return none();
            }
            created.sort_by_key(|k| key_paths.get(&k.id).map_or(0, |p| p.matches('|').count()));
            for entry in created {
                if let Some(path) = key_paths.get(&entry.id) {
                    let _ = self.catalogue.apply_keyword(entry, path);
                }
                let _ = self.events.send(Event::KeywordCreated(entry.id));
            }
        }
        ids.into_iter()
            .map(|id| id.and_then(|id| key_paths.get(&id).map(|path| (id, path.clone()))))
            .collect()
    }

    /// The external XMP files an index job read (`Inbound::ExternalXmp`, D-047): a photo whose tracked file
    /// is this one is compared with what the file says now, against its base and against the photo as it
    /// is (slice 2's three-way check); anything else (no base yet, another file tracked before) is
    /// baselined silently. A comparison with nothing to take or ask (the file only changed in ways that
    /// are not fields, or to what the photo already says) simply moves the base along; otherwise the
    /// file's newer fields wait as a pending change for an answer. A photo that has left since is not
    /// there, and its row is refused by the foreign key: nothing to record.
    fn handle_external_seen(&mut self, seen: Vec<ExternalSeen>) {
        let key_paths = self
            .read_vocabulary()
            .map(|v| auroraw_catalogue::keyword_paths(&v.keywords))
            .unwrap_or_default();
        for one in seen {
            let row = self.catalogue.external_of(&one.photo_id).ok().flatten();
            let tracked = row.filter(|r| r.path == one.path);
            let compared = tracked.and_then(|row| {
                let (photo, _) = self.read_photo(&one.photo_id).ok()?;
                Some(merge(
                    Some(&row.base),
                    &one.fields,
                    &mine_of(&photo.meta, &key_paths),
                ))
            });
            match compared {
                Some(changes) if !changes.is_empty() => {
                    let _ = self.catalogue.set_external_pending(
                        &one.photo_id,
                        &ExternalPending {
                            stat: one.stat,
                            file: one.fields,
                        },
                    );
                }
                _ => {
                    let _ = self.catalogue.set_external_base(
                        &one.photo_id,
                        &one.path,
                        one.stat,
                        &one.fields,
                    );
                }
            }
        }
    }

    /// Looks at every change waiting for an answer with fresh eyes: one the photo has come to agree with
    /// (edited by hand to the same values since) is settled on the spot; the rest are counted. What the
    /// banner and `Event::ExternalChanges` report.
    fn settle_external(&mut self) -> usize {
        let Ok(rows) = self.catalogue.pending_externals() else {
            return 0;
        };
        if rows.is_empty() {
            return 0;
        }
        let key_paths = self
            .read_vocabulary()
            .map(|v| auroraw_catalogue::keyword_paths(&v.keywords))
            .unwrap_or_default();
        let (mut waiting, mut agreed) = (0, Vec::new());
        for row in rows {
            let Ok((photo, _)) = self.read_photo(&row.photo_id) else {
                continue;
            };
            let Some(diff) = pending_diff(&row, &photo.meta, &key_paths) else {
                continue;
            };
            match row.pending {
                Some(pending) if diff.is_empty() => {
                    agreed.push((row.photo_id, pending.stat, pending.file));
                }
                _ => waiting += 1,
            }
        }
        let _ = self.catalogue.settle_externals(&agreed);
        waiting
    }

    /// Tells the interface how many changes wait for an answer now.
    fn report_external(&mut self) {
        let photos = self.settle_external();
        let _ = self.events.send(Event::ExternalChanges {
            source_id: None,
            photos,
            unreadable: 0,
        });
    }

    /// Declines the changes waiting for these photos (`Command::IgnoreExternalChanges`): the file
    /// becomes the base as it stands, nothing is applied and nothing is recorded in the history.
    fn ignore_external(&mut self, photos: Vec<PhotoId>) -> Result<Outcome> {
        let mut answered = Vec::new();
        for photo in photos {
            if answered.iter().any(|(id, _, _)| *id == photo) {
                continue;
            }
            if let Some(pending) = self
                .catalogue
                .external_of(&photo)?
                .and_then(|row| row.pending)
            {
                answered.push((photo, pending.stat, pending.file));
            }
        }
        self.catalogue.settle_externals(&answered)?;
        self.report_external();
        Ok(Outcome::ExternalResolved {
            photos: answered.len(),
        })
    }

    /// Accepts the changes waiting for these photos (`Command::AcceptExternalChanges`), all as one step
    /// of the history: keywords the vocabulary lacks are created first (one `Change::Vocabulary` at the
    /// head, so that an undo takes them off the photos before it takes them out), then each photo takes
    /// its fields and keyword changes in one write. Nothing is applied if anything fails.
    fn accept_external(
        &mut self,
        photos: Vec<PhotoId>,
        use_file: Vec<(PhotoId, ExternalField)>,
    ) -> Result<Outcome> {
        struct Plan {
            photo: PhotoId,
            stat: ExternalStat,
            file: Fields,
            merge: Merge,
        }
        let vocabulary = self.read_vocabulary()?;
        let key_paths = auroraw_catalogue::keyword_paths(&vocabulary.keywords);
        let mut plans: Vec<Plan> = Vec::new();
        for photo in photos {
            if plans.iter().any(|p| p.photo == photo) {
                continue;
            }
            let Some(row) = self.catalogue.external_of(&photo)? else {
                continue;
            };
            let Some(pending) = row.pending.clone() else {
                continue;
            };
            let Ok((sidecar, _)) = self.read_photo(&photo) else {
                continue;
            };
            let Some(diff) = pending_diff(&row, &sidecar.meta, &key_paths) else {
                continue;
            };
            plans.push(Plan {
                photo,
                stat: pending.stat,
                file: pending.file,
                merge: diff,
            });
        }

        // The keywords the files gained that the vocabulary lacks, made on a copy first so that the
        // change to record is exactly what was made.
        let mut working = vocabulary.clone();
        let known: std::collections::HashSet<KeywordId> =
            vocabulary.keywords.iter().map(|k| k.id).collect();
        let mut resolved: HashMap<String, KeywordId> = HashMap::new();
        for path in plans.iter().flat_map(|p| p.merge.keywords.add.iter()) {
            if let Some(id) = Self::resolve_keyword_path(&mut working, path) {
                resolved.insert(keyword_key(path), id);
            }
        }
        let created: Vec<KeywordEntry> = working
            .keywords
            .iter()
            .filter(|k| !known.contains(&k.id))
            .cloned()
            .collect();
        let mut changes: Vec<Change> = Vec::new();
        if !created.is_empty() {
            let deltas: Vec<KeywordDelta> = created
                .iter()
                .map(|entry| KeywordDelta {
                    id: entry.id,
                    before: None,
                    after: Some(entry.clone()),
                })
                .collect();
            self.apply_vocabulary(&deltas, Direction::Redo, false)?;
            for entry in &created {
                let _ = self.events.send(Event::KeywordCreated(entry.id));
            }
            changes.push(Change::Vocabulary {
                action: VocabularyAction::Create,
                keywords: deltas,
            });
        }
        let paths_after = auroraw_catalogue::keyword_paths(&working.keywords);

        let mut touched = 0;
        for plan in &plans {
            match self.apply_external_plan(
                plan.photo,
                &plan.file,
                &plan.merge,
                &use_file,
                &resolved,
                &key_paths,
                &paths_after,
            ) {
                Ok(made) => {
                    touched += usize::from(!made.is_empty());
                    changes.extend(made);
                }
                Err(e) => {
                    self.take_back(&changes);
                    return Err(e);
                }
            }
        }
        let answered: Vec<_> = plans
            .iter()
            .map(|p| (p.photo, p.stat, p.file.clone()))
            .collect();
        if let Err(e) = self.catalogue.settle_externals(&answered) {
            self.take_back(&changes);
            return Err(e.into());
        }
        if !changes.is_empty() {
            self.history.record(Entry::labelled(
                Label {
                    kind: LabelKind::ExternalChanges,
                    count: touched,
                },
                changes,
            ));
            self.report_history();
        }
        self.report_external();
        Ok(Outcome::ExternalResolved {
            photos: answered.len(),
        })
    }

    /// One photo's share of an accept: the fields only the file changed, the conflicts settled in the
    /// file's favour, the keywords it gained and lost, written at once. What changed, as history changes.
    #[allow(clippy::too_many_arguments)]
    fn apply_external_plan(
        &mut self,
        photo_id: PhotoId,
        file: &Fields,
        diff: &Merge,
        use_file: &[(PhotoId, ExternalField)],
        resolved: &HashMap<String, KeywordId>,
        key_paths: &HashMap<KeywordId, String>,
        paths_after: &HashMap<KeywordId, String>,
    ) -> Result<Vec<Change>> {
        let (mut photo, _) = self.read_photo(&photo_id)?;
        let mut changes = Vec::new();
        let mut fields: Vec<ExternalField> = diff.taken.iter().map(|c| c.field).collect();
        fields.extend(
            diff.conflicts
                .iter()
                .map(|c| c.field)
                .filter(|f| use_file.contains(&(photo_id, *f))),
        );
        for field in fields {
            let meta = &mut photo.meta;
            match field {
                ExternalField::Rating => {
                    let (stars, flag) = (meta.rating, meta.flag);
                    file.apply_field(field, meta);
                    if stars != meta.rating {
                        changes.push(Change::Rating {
                            photo: photo_id,
                            before: stars,
                            after: meta.rating,
                        });
                    }
                    if flag != meta.flag {
                        changes.push(Change::Flag {
                            photo: photo_id,
                            before: flag,
                            after: meta.flag,
                        });
                    }
                }
                ExternalField::Label => {
                    let before = meta.label.clone();
                    file.apply_field(field, meta);
                    if before != meta.label {
                        changes.push(Change::Label {
                            photo: photo_id,
                            before,
                            after: meta.label.clone(),
                        });
                    }
                }
                other => {
                    if let Some(text_field) = metadata_field(other) {
                        let before = text_field.get(meta);
                        file.apply_field(other, meta);
                        let after = text_field.get(meta);
                        if before != after {
                            changes.push(Change::Metadata {
                                photo: photo_id,
                                field: text_field,
                                before,
                                after,
                            });
                        }
                    }
                }
            }
        }
        if !diff.keywords.add.is_empty() || !diff.keywords.remove.is_empty() {
            let before = keyword_set(&photo.meta);
            let gone: std::collections::HashSet<String> = diff
                .keywords
                .remove
                .iter()
                .map(|p| keyword_key(p))
                .collect();
            let mut i = 0;
            while i < photo.meta.keyword_ids.len() {
                let id = photo.meta.keyword_ids[i];
                let path = key_paths
                    .get(&id)
                    .cloned()
                    .or_else(|| photo.meta.keyword_paths.get(i).cloned());
                if path.is_some_and(|p| gone.contains(&keyword_key(&p))) {
                    photo.meta.keyword_ids.remove(i);
                    if i < photo.meta.keyword_paths.len() {
                        photo.meta.keyword_paths.remove(i);
                    }
                } else {
                    i += 1;
                }
            }
            for path in &diff.keywords.add {
                if let Some(id) = resolved.get(&keyword_key(path))
                    && !photo.meta.keyword_ids.contains(id)
                {
                    let canonical = paths_after.get(id).cloned().unwrap_or_else(|| path.clone());
                    photo.meta.push_keyword(*id, canonical);
                }
            }
            let after = keyword_set(&photo.meta);
            if before != after {
                changes.push(Change::Keywords {
                    photo: photo_id,
                    before,
                    after,
                });
            }
        }
        if !changes.is_empty() {
            self.persist_photo(photo)?;
        }
        Ok(changes)
    }

    #[allow(clippy::too_many_arguments)]
    fn start_import(
        &mut self,
        source_root: PathBuf,
        destination_root: PathBuf,
        registration: Option<crate::import_job::Registration>,
        mut profile: Profile,
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
