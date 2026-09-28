// SPDX-License-Identifier: GPL-3.0-or-later
//! The background worker behind `Command::IndexSource` (M1 plan, workflow revision D-090): adding a
//! folder to the catalogue "in place" copies nothing (spec §5.1); it lists the folder, reads what
//! each photo's file says about itself into its sidecar (D-074), and registers the photo. None of
//! that runs on the coordinator thread; only the catalogue write of a finished photo goes back to
//! it ([`Inbound::Imported`]), exactly as an import does.
//!
//! Files that match a photo removed earlier (its sidecar waits in the workspace's `removed/`) are
//! reported first, and the job pauses for a person's answer: restore the photo, with its ratings,
//! keywords and versions, or add the file as a new one.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use auroraw_catalogue::Catalogue;
use auroraw_format::sidecar::{FileEntry, FileRole, Location, PhotoSidecar};
use auroraw_import::{DiscoveredFile, PairRule, PhotoGroup, pair_files};
use auroraw_plugin_api::source::{Source, SourceState};
use auroraw_sources::filesystem::FilesystemSource;
use auroraw_sources::relink::{self, FoundFile, KnownFile, ScanOutcome};
use auroraw_types::{ContentHash, Fingerprint, PhotoId, SourceId, Timestamp};
use auroraw_workspace::{RemovedPhoto, Workspace};

use crate::command::Command;
use crate::coordinator::Inbound;
use crate::event::Event;
use crate::import_job::{discover_one, stat_of};
use crate::job::{CancelToken, JobId};
use crate::reconcile_apply;
use crate::thumbnails::source_root;

pub(crate) struct IndexJob {
    pub job: JobId,
    pub workspace: Arc<Workspace>,
    pub source: FilesystemSource,
    pub source_id: SourceId,
    /// Folders inside this source that are the roots of other sources, relative to it and with
    /// `/` separators: never descended into, so a photo is never registered twice.
    pub skip: Vec<String>,
    /// Sources inside this one to merge into it first, with their folder relative to it.
    pub merge: Vec<(SourceId, String)>,
    pub catalogue_path: PathBuf,
    pub events: mpsc::Sender<Event>,
    pub inbound: mpsc::Sender<Inbound>,
    pub cancel: CancelToken,
    /// The person's answer to a pause (restore or not).
    pub decision: mpsc::Receiver<bool>,
}

pub(crate) fn spawn(job: IndexJob) {
    std::thread::spawn(move || run(job));
}

fn inside_skipped(path: &str, skip: &[String]) -> bool {
    skip.iter()
        .any(|folder| path == folder || path.starts_with(&format!("{folder}/")))
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn file_entry(
    role: FileRole,
    file: &DiscoveredFile,
    fingerprint: Fingerprint,
    source_id: SourceId,
) -> FileEntry {
    let name = file_name(&file.path);
    let format = Path::new(&name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_uppercase());
    FileEntry {
        role,
        name,
        format,
        size: file.size,
        fingerprint,
        hash: None,
        locations: vec![Location {
            source: source_id,
            path: file.path.clone(),
            seen: Some(Timestamp::now()),
            extra: Vec::new(),
        }],
        extra: Vec::new(),
    }
}

/// A photo the job will add: one or two files of the source, their fingerprints, and the
/// metadata read from the original.
struct Candidate {
    group: PhotoGroup,
    original_fingerprint: Fingerprint,
    companion_fingerprint: Option<Fingerprint>,
    metadata: Option<auroraw_format::sidecar::Metadata>,
    removed: Option<usize>,
}

/// Waits for the person's answer, giving up if the job is cancelled or the engine is gone.
fn wait_for_answer(job: &IndexJob) -> Option<bool> {
    loop {
        if job.cancel.is_cancelled() {
            return None;
        }
        match job.decision.recv_timeout(Duration::from_millis(200)) {
            Ok(answer) => return Some(answer),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
}

fn run(job: IndexJob) {
    let abort = |reason: String| {
        let _ = job.events.send(Event::IndexAborted {
            job: job.job,
            reason,
        });
        let _ = job.events.send(Event::JobFinished(job.job));
    };
    if job.source.state() != SourceState::Online {
        return abort("the source is not reachable".to_string());
    }
    let entries = match job.source.list() {
        Ok(entries) => entries,
        Err(e) => return abort(format!("the source cannot be listed: {e}")),
    };
    let Ok(catalogue) = Catalogue::open(&job.catalogue_path) else {
        return abort("the catalogue cannot be read".to_string());
    };

    let known_rows = catalogue
        .known_files_in_source(&job.source_id)
        .unwrap_or_default();
    // Which of these known paths is a photo's *primary* location (D-108): the others (secondary locations,
    // D-036) are the `location` table's, never the `photo` row's, and are handled differently below.
    let mut is_primary: HashMap<String, bool> = known_rows
        .iter()
        .map(|(_, path, _, primary)| (path.clone(), *primary))
        .collect();
    let mut known: Vec<KnownFile> = known_rows
        .into_iter()
        .map(|(photo_id, path, fingerprint, _)| KnownFile {
            photo_id,
            path,
            fingerprint,
        })
        .collect();
    // Photos of sources being merged in become this source's own before anything is listed; their new paths
    // are known here (and always primary), whether or not the catalogue has caught up with the merge yet.
    for merged in merge_sources(&job, &catalogue) {
        is_primary.insert(merged.path.clone(), true);
        known.push(merged);
    }

    // Paired (D-032) before reconciling, not after (D-109): a companion's own fingerprint is never part of
    // what the catalogue tracks for a photo (only its original's is, one row per photo), so only a group's
    // original is fed to `reconcile` below, the same pure function `coordinator::scan_source` uses — never a
    // companion on its own, which would otherwise risk becoming a phantom "new" file every time this source is
    // rescanned.
    let mut groups_by_path: HashMap<String, PhotoGroup> = pair_files(
        entries
            .iter()
            .filter(|entry| {
                auroraw_imaging::is_photo_file(Path::new(&entry.path))
                    && !inside_skipped(&entry.path, &job.skip)
            })
            .map(|entry| DiscoveredFile {
                path: entry.path.clone(),
                size: entry.size,
                capture_time: None,
                camera: None,
            })
            .collect(),
        PairRule::Both,
    )
    .into_iter()
    .map(|group| (group.original.path.clone(), group))
    .collect();

    let mut failed = 0;
    let mut found = Vec::with_capacity(groups_by_path.len());
    for group in groups_by_path.values() {
        match job.source.fingerprint_of(&group.original.path) {
            Ok(fingerprint) => found.push(FoundFile {
                path: group.original.path.clone(),
                fingerprint,
            }),
            Err(_) => failed += 1,
        }
    }
    let found_fingerprint: HashMap<&str, Fingerprint> = found
        .iter()
        .map(|f| (f.path.as_str(), f.fingerprint))
        .collect();

    let mut confirmed = 0;
    let mut missing = 0;
    let mut known_second_locations = 0;
    let mut new_groups = Vec::new();
    for outcome in relink::reconcile(&found, &known) {
        if job.cancel.is_cancelled() {
            // Through `inbound`, not `events`, directly: `merge_sources`, just above, may have messages
            // still queued (`Inbound::Report`'s own doc comment).
            let _ = job
                .inbound
                .send(Inbound::Report(Event::JobCancelled(job.job)));
            return;
        }
        match outcome {
            // Exactly as before, on this exact path: nothing to do (unlike `scan_source`, index_job carries no
            // earlier "original changed"/"missing" mark to clear, since a photo can only ever be given one by
            // this same reconcile, on this same pass).
            ScanOutcome::Confirmed { .. } => confirmed += 1,
            ScanOutcome::OriginalChanged { photo_id, path } => {
                let _ = job.inbound.send(Inbound::Apply(reconcile_apply::changed(
                    photo_id,
                    job.source_id,
                    path,
                    &is_primary,
                )));
            }
            ScanOutcome::Relinked { photo_id, from, to } => {
                let filename = file_name(&to);
                let fingerprint = found
                    .iter()
                    .find(|f| f.path == to)
                    .map(|f| f.fingerprint)
                    .expect("the relink target was just found");
                let _ = job.inbound.send(Inbound::Apply(reconcile_apply::relinked(
                    photo_id,
                    job.source_id,
                    from,
                    to,
                    filename,
                    fingerprint,
                    &is_primary,
                )));
            }
            ScanOutcome::Missing { photo_id, path } => {
                let _ = job.inbound.send(Inbound::Apply(reconcile_apply::missing(
                    photo_id,
                    job.source_id,
                    path,
                    &is_primary,
                )));
                missing += 1;
            }
            // Undecidable here, exactly as it is for `scan_source`: left alone, the way an ambiguous new path
            // always has been (D-019, D-031) — nothing resolves it automatically either way.
            ScanOutcome::Ambiguous { .. } => {}
            ScanOutcome::New { path } => {
                if let Some(group) = groups_by_path.remove(&path) {
                    new_groups.push(group);
                }
            }
            ScanOutcome::SecondLocation { photo_id, path } => {
                let fingerprint = found
                    .iter()
                    .find(|f| f.path == path)
                    .map(|f| f.fingerprint)
                    .expect("the second-location path was just found");
                if confirm_known_second_location(&job, &catalogue, photo_id, &path, fingerprint) {
                    known_second_locations += 1;
                } else if let Some(group) = groups_by_path.remove(&path) {
                    // The sampled fingerprint collided (D-108's own residual risk, confirmed away): not
                    // actually the same file after all, so it is simply a new, unrelated photo.
                    new_groups.push(group);
                }
            }
        }
    }
    // By path, case aside: photos enter the catalogue (and get their thumbnails) in the order a person reads
    // a folder, whatever order the source listed them in.
    new_groups.sort_by_cached_key(|group| {
        (
            group.original.path.to_lowercase(),
            group.original.path.clone(),
        )
    });

    // Which new photos are ones that were removed earlier.
    let removed = job.workspace.removed_photos();
    let mut by_fingerprint: HashMap<String, usize> = HashMap::new();
    for (index, removed_photo) in removed.iter().enumerate() {
        if let Some(original) = removed_photo
            .photo
            .files
            .iter()
            .find(|file| file.role == FileRole::Original)
        {
            by_fingerprint.insert(original.fingerprint.to_string(), index);
        }
    }
    let mut candidates = Vec::with_capacity(new_groups.len());
    let mut claimed: HashSet<usize> = HashSet::new();
    for group in new_groups {
        if job.cancel.is_cancelled() {
            let _ = job
                .inbound
                .send(Inbound::Report(Event::JobCancelled(job.job)));
            return;
        }
        // Fingerprinted once already, above, to feed `reconcile`.
        let Some(original_fingerprint) =
            found_fingerprint.get(group.original.path.as_str()).copied()
        else {
            failed += 1;
            continue;
        };
        let companion_fingerprint = group
            .companion
            .as_ref()
            .and_then(|c| job.source.fingerprint_of(&c.path).ok());
        let removed_match = by_fingerprint
            .get(&original_fingerprint.to_string())
            .copied()
            .filter(|index| claimed.insert(*index));
        candidates.push(Candidate {
            group,
            original_fingerprint,
            companion_fingerprint,
            metadata: None,
            removed: removed_match,
        });
    }
    let restorable = candidates.iter().filter(|c| c.removed.is_some()).count();
    let _ = job.events.send(Event::IndexPlanned {
        job: job.job,
        source_id: job.source_id,
        new_files: candidates.len(),
        restorable,
    });
    let restore = if restorable > 0 {
        match wait_for_answer(&job) {
            Some(answer) => answer,
            None => {
                let _ = job
                    .inbound
                    .send(Inbound::Report(Event::JobCancelled(job.job)));
                return;
            }
        }
    } else {
        false
    };

    let root = job.source.root().to_path_buf();
    let total = candidates.len();
    let (mut added, mut restored, mut second_locations) = (0, 0, known_second_locations);
    let mut versions_came_back = false;
    for (done, mut candidate) in candidates.into_iter().enumerate() {
        if job.cancel.is_cancelled() {
            // Through `inbound`: an earlier candidate in this same loop may have a `LocationAdded` still
            // queued ahead of it.
            let _ = job
                .inbound
                .send(Inbound::Report(Event::JobCancelled(job.job)));
            return;
        }
        let outcome = match candidate.removed.filter(|_| restore) {
            Some(index) => restore_one(&job, &removed[index], &candidate),
            None if try_join_existing_photo(
                &job,
                &catalogue,
                &candidate.group.original.path,
                candidate.original_fingerprint,
            ) =>
            {
                Ok(Landed::Joined)
            }
            None => {
                let (_, metadata) = discover_one(&root, &candidate.group.original.path);
                candidate.metadata = metadata;
                add_one(&job, candidate)
            }
        };
        match outcome {
            Ok(Landed::Added) => added += 1,
            Ok(Landed::Restored { versions }) => {
                restored += 1;
                versions_came_back |= versions > 0;
            }
            Ok(Landed::Joined) => second_locations += 1,
            Err(()) => failed += 1,
        }
        let _ = job.events.send(Event::JobProgress {
            job: job.job,
            done: done + 1,
            total,
        });
    }
    if versions_came_back {
        // Restored version sidecars are new to the catalogue: a reconcile applies them.
        let _ = job.inbound.send(Inbound::Command {
            command: Command::Reconcile,
            reply: None,
        });
    }
    let _ = job.inbound.send(Inbound::Report(Event::IndexFinished {
        job: job.job,
        source_id: job.source_id,
        added,
        restored,
        known: confirmed,
        failed,
        second_locations,
        missing,
    }));
    let _ = job
        .inbound
        .send(Inbound::Report(Event::JobFinished(job.job)));
}

/// Moves every photo of the merged sources into this one: each location's source becomes this
/// source and its path gets the merged folder's place in it. Returns the photos' primary files as
/// `KnownFile`s (always primary, D-109: a merge is exactly what promotes them to being this
/// source's own primary location), fed to `reconcile` below alongside what the catalogue already
/// knows for this source, whether or not it has caught up with the merge yet. The merged sources'
/// entries are then dropped.
fn merge_sources(job: &IndexJob, catalogue: &Catalogue) -> Vec<KnownFile> {
    let mut moved_files = Vec::new();
    for (inner, prefix) in &job.merge {
        let mut moved = 0;
        for photo_id in catalogue.photos_in_source(inner).unwrap_or_default() {
            let Some(mut photo) = job
                .workspace
                .read_photo(&photo_id)
                .ok()
                .flatten()
                .and_then(|loaded| loaded.current())
            else {
                continue;
            };
            // This in-memory copy is only to work out the photo's new primary location, needed synchronously
            // below (`known`/`is_primary`, fed to this job's own reconciliation right after); the sidecar
            // itself is rewritten by the coordinator, from a fresh read of its own (`Inbound::MergeLocations`,
            // the coordinator's the only writer, note 001 §5.4/D-099).
            let mut primary = None;
            for file in &mut photo.files {
                for location in &mut file.locations {
                    if location.source == *inner {
                        location.source = job.source_id;
                        location.path = format!("{prefix}/{}", location.path);
                    }
                }
                if primary.is_none()
                    && file.role == FileRole::Original
                    && let Some(location) = file.locations.first()
                {
                    primary = Some((file.name.clone(), file.fingerprint, location.path.clone()));
                }
            }
            if let Some((_, fingerprint, path)) = primary {
                let _ = job.inbound.send(Inbound::MergeLocations {
                    photo_id,
                    from_source: *inner,
                    into_source: job.source_id,
                    prefix: prefix.clone(),
                });
                moved_files.push(KnownFile {
                    photo_id,
                    path,
                    fingerprint,
                });
                moved += 1;
            }
        }
        let _ = job.inbound.send(Inbound::SourceGone {
            job: job.job,
            source_id: *inner,
            removed: 0,
            kept: moved,
            announce: false,
        });
    }
    moved_files
}

enum Landed {
    Added,
    Restored {
        versions: usize,
    },
    /// The file joined an existing photo from a different source as a second location, instead of becoming a new
    /// photo (D-036, D-108).
    Joined,
}

/// Reads and hashes a candidate file of this source's own listing (the one about to be joined or checked),
/// while it is still on disk right now.
fn hash_of_found(job: &IndexJob, path: &str) -> Option<ContentHash> {
    let full = job
        .source
        .root()
        .join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
    let bytes = std::fs::read(&full).ok()?;
    let (_, hash) =
        auroraw_format::fingerprint::content_hash(&mut std::io::Cursor::new(&bytes)).ok()?;
    Some(hash)
}

/// A single-file candidate's fingerprint matches a photo the catalogue already has, at a *different* source
/// (`known_files_in_source`, scoped to this source alone, cannot see it — that is `index_job`'s own reason this
/// check exists, on top of `coordinator::scan_source`'s: a fresh source's first index never revisits a file once it
/// has become its own photo). Confirms with the whole-file hash of both copies before joining anything (D-108: a
/// sampled fingerprint alone is not proof enough to invite someone to go delete a file over it). `true` when a
/// location was recorded (the sidecar and the catalogue both, through `Inbound::LocationAdded`) and the file is
/// therefore not going to become a new photo. A companion (RAW+JPEG, D-032) is decided by its original alone,
/// exactly like `import_job::find_duplicate` already does elsewhere: the companion is left unindexed on disk
/// either way (D-109 — it used to also gate this whole check off entirely, which is issues #6 and #8).
fn try_join_existing_photo(
    job: &IndexJob,
    catalogue: &Catalogue,
    path: &str,
    fingerprint: Fingerprint,
) -> bool {
    let Ok(candidates) = catalogue.find_by_fingerprint(&fingerprint) else {
        return false;
    };
    if candidates.is_empty() {
        return false;
    }
    let Some(found_hash) = hash_of_found(job, path) else {
        return false;
    };
    candidates
        .into_iter()
        .any(|candidate| confirm_candidate(job, candidate, path, fingerprint, found_hash))
}

/// A found file's fingerprint matches exactly one known file *within this source itself*, whose old location is
/// also still present (`ScanOutcome::SecondLocation`, from the same `sources::relink::reconcile`
/// `coordinator::scan_source` uses, D-109): confirmed the same way as a cross-source join above, just for the
/// one candidate `reconcile` already identified instead of searching every fingerprint match.
fn confirm_known_second_location(
    job: &IndexJob,
    catalogue: &Catalogue,
    photo_id: PhotoId,
    path: &str,
    fingerprint: Fingerprint,
) -> bool {
    let Ok(candidates) = catalogue.find_by_fingerprint(&fingerprint) else {
        return false;
    };
    let Some(candidate) = candidates.into_iter().find(|c| c.photo_id == photo_id) else {
        return false;
    };
    let Some(found_hash) = hash_of_found(job, path) else {
        return false;
    };
    confirm_candidate(job, candidate, path, fingerprint, found_hash)
}

/// Confirms one fingerprint-matching candidate by whole-file hash and, on a match, records the join.
fn confirm_candidate(
    job: &IndexJob,
    candidate: auroraw_catalogue::FingerprintCandidate,
    path: &str,
    fingerprint: Fingerprint,
    found_hash: ContentHash,
) -> bool {
    let primary_hash = match candidate.hash {
        Some(hash) => hash,
        None => match hash_of_existing(job, &candidate) {
            Some(hash) => {
                // Backfilled for later: the same reasoning `import_job::find_duplicate` already relies on.
                let _ = job.inbound.send(Inbound::Imported {
                    photo: None,
                    stat: None,
                    backfill: Some((candidate.photo_id, hash)),
                });
                hash
            }
            None => return false,
        },
    };
    if primary_hash != found_hash {
        return false;
    }
    record_second_location(job, candidate.photo_id, path, fingerprint, found_hash)
}

/// Reads and hashes an existing candidate's own file, when its hash is not already known.
fn hash_of_existing(
    job: &IndexJob,
    candidate: &auroraw_catalogue::FingerprintCandidate,
) -> Option<ContentHash> {
    let root = source_root(&job.workspace, candidate.source_id?)?;
    let bytes = std::fs::read(
        root.join(
            candidate
                .path
                .as_ref()?
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        ),
    )
    .ok()?;
    let (_, hash) =
        auroraw_format::fingerprint::content_hash(&mut std::io::Cursor::new(&bytes)).ok()?;
    Some(hash)
}

/// Tells the coordinator to push a new `Location` onto the photo's sidecar (a fresh read of its own) and
/// record it in the catalogue: nothing here needs a read first, every field is already decided by the
/// caller (D-036, D-108).
fn record_second_location(
    job: &IndexJob,
    photo_id: PhotoId,
    path: &str,
    fingerprint: Fingerprint,
    hash: ContentHash,
) -> bool {
    let filename = file_name(path);
    let _ = job.inbound.send(Inbound::LocationAdded {
        photo_id,
        source_id: job.source_id,
        path: path.to_string(),
        filename,
        fingerprint,
        hash,
    });
    true
}

fn register(job: &IndexJob, photo: PhotoSidecar) -> Result<(), ()> {
    let stat_path = job.workspace.photo_path(&photo.photo_id);
    let stat = match std::fs::metadata(&stat_path) {
        Ok(m) => stat_of(m.len(), m.modified().ok()),
        Err(_) => return Err(()),
    };
    let _ = job.inbound.send(Inbound::Imported {
        photo: Some(Box::new(photo)),
        stat: Some(stat),
        backfill: None,
    });
    Ok(())
}

fn add_one(job: &IndexJob, candidate: Candidate) -> Result<Landed, ()> {
    let mut photo = PhotoSidecar::new(PhotoId::random());
    photo.meta = candidate.metadata.unwrap_or_default();
    photo.imported = Some(Timestamp::now());
    photo.files.push(file_entry(
        FileRole::Original,
        &candidate.group.original,
        candidate.original_fingerprint,
        job.source_id,
    ));
    if let (Some(companion), Some(fingerprint)) =
        (&candidate.group.companion, candidate.companion_fingerprint)
    {
        photo.files.push(file_entry(
            FileRole::Companion,
            companion,
            fingerprint,
            job.source_id,
        ));
    }
    job.workspace.write_photo(&photo).map_err(|_| ())?;
    register(job, photo)?;
    Ok(Landed::Added)
}

/// Puts a removed photo back: its sidecar (ratings, keywords) with its files' locations pointing at
/// this source, its versions from `removed/` with it.
fn restore_one(
    job: &IndexJob,
    removed: &RemovedPhoto,
    candidate: &Candidate,
) -> Result<Landed, ()> {
    let mut photo = removed.photo.clone();
    let found = [
        Some((&candidate.group.original, candidate.original_fingerprint)),
        candidate
            .group
            .companion
            .as_ref()
            .zip(candidate.companion_fingerprint),
    ];
    for file in &mut photo.files {
        // The source it was in is gone: its locations are stale. Where this source has the same
        // file (by fingerprint), that is the new location; where it does not, the file is missing.
        file.locations.clear();
        if let Some((discovered, _)) = found
            .iter()
            .flatten()
            .find(|(_, fingerprint)| *fingerprint == file.fingerprint)
        {
            file.locations.push(Location {
                source: job.source_id,
                path: discovered.path.clone(),
                seen: Some(Timestamp::now()),
                extra: Vec::new(),
            });
        }
    }
    let versions = job
        .workspace
        .restore_photo(removed, &photo)
        .map_err(|_| ())?;
    register(job, photo)?;
    Ok(Landed::Restored { versions })
}
