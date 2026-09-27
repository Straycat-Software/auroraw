// SPDX-License-Identifier: GPL-3.0-or-later
//! The background worker behind `Command::Import` (spec §5.2, design note 004 §6.3 items 4-5):
//! lists the source, reads each file's metadata, pairs and plans them, then reads, verifies and
//! registers each photo, none of it on the coordinator thread (architecture §4.2's "does not
//! stall the interface thread"). Only the catalogue write for a landed photo goes back to the
//! coordinator ([`Inbound::Imported`]), the same split [`crate::refresh`] already uses for a
//! keyword rename's sidecar refresh: the sidecar write is self-contained and safe from any
//! thread, the database connection is not.
//!
//! Resumable state (spec §5.2: "an interrupted import resumes") is kept per **photo**, not per
//! file: a RAW+JPEG pair is one photo with two files (D-032), so a run interrupted between the
//! two writes retries the whole pair rather than leaving a photo with only one of its files.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;

use auroraw_catalogue::{Catalogue, SidecarStat};
use auroraw_format::sidecar::{FileEntry, FileRole, Location, Metadata, PhotoSidecar};
use auroraw_import::{
    DiscoveredFile, ImportState, ItemOutcome, PlannedFile, PlannedPhoto, Profile, Root, UsedPaths,
    pair_files, plan, read_source, write_verified,
};
use auroraw_plugin_api::source::{Source, SourceState};
use auroraw_sources::filesystem::FilesystemSource;
use auroraw_types::{ContentHash, Fingerprint, KeywordId, PhotoId, SourceId, Timestamp};
use auroraw_workspace::Workspace;

use crate::coordinator::Inbound;
use crate::event::Event;
use crate::job::{CancelToken, JobId};

/// Everything the job needs, gathered on the coordinator thread before the background thread
/// starts (source lookup, keyword resolution): kept in one struct so `spawn`'s signature stays
/// readable.
pub(crate) struct ImportJob {
    pub job: JobId,
    pub workspace: Arc<Workspace>,
    pub source: FilesystemSource,
    pub dest_root: PathBuf,
    /// Where the imported photos are registered in the catalogue, or `None` for a plain copy: the
    /// destination is not one of the catalogue's sources, so no sidecar is written and nothing is
    /// registered. Photos already in the catalogue are then not skipped either: copying a card to
    /// a second place is what was asked.
    pub registration: Option<Registration>,
    pub profile: Profile,
    pub backup_roots: Vec<PathBuf>,
    pub state_path: PathBuf,
    pub catalogue_path: PathBuf,
    pub template_keywords: Vec<(KeywordId, String)>,
    pub events: mpsc::Sender<Event>,
    pub inbound: mpsc::Sender<Inbound>,
    pub cancel: CancelToken,
}

/// Where imported photos live in the catalogue: the source that covers the destination folder, and
/// the destination's path inside it (empty when the destination is the source's own folder), with
/// `/` separators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    /// The catalogue source that covers the destination folder.
    pub source_id: SourceId,
    /// The destination folder relative to that source's folder.
    pub prefix: String,
}

pub(crate) fn spawn(job: ImportJob) {
    std::thread::spawn(move || run(job));
}

pub(crate) fn stat_of(size: u64, modified: Option<std::time::SystemTime>) -> SidecarStat {
    let modified = modified
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    SidecarStat { size, modified }
}

fn to_original(m: auroraw_imaging::Metadata) -> auroraw_format::sidecar::Original {
    auroraw_format::sidecar::Original {
        capture_time: m.capture_time,
        make: m.make,
        model: m.model,
        serial: m.serial,
        lens: m.lens,
        exposure_time: m.exposure_time,
        f_number: m.f_number,
        iso: m.iso,
        focal_length: m.focal_length,
        focal_length_35mm: m.focal_length_35mm,
        pixel_width: m.pixel_width,
        pixel_height: m.pixel_height,
        orientation: m.orientation,
        gps_latitude: m.gps_latitude,
        gps_longitude: m.gps_longitude,
        gps_altitude: m.gps_altitude,
    }
}

/// This file's capture time, parsed from its metadata's ISO 8601 text, and a camera name for the
/// template and for sorting several cameras together (M1 plan §6, item 6). A file whose metadata
/// cannot be read at all (corrupt, or a format this build's decoder does not know) still gets a
/// [`DiscoveredFile`], with nothing but its path and size: planned and copied like any other,
/// just placed by whatever the template renders with empty tokens rather than dropped from the
/// import entirely (spec §5.2: everything is imported).
pub(crate) fn discover_one(root: &Path, path: &str) -> (DiscoveredFile, Option<Metadata>) {
    let full = root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
    let size = std::fs::metadata(&full).map(|m| m.len()).unwrap_or(0);
    let Ok(read) = auroraw_imaging::read_metadata(&full) else {
        return (
            DiscoveredFile {
                path: path.to_string(),
                size,
                capture_time: None,
                camera: None,
            },
            None,
        );
    };
    let capture_time = read.capture_time.as_deref().and_then(|t| {
        time::OffsetDateTime::parse(t, &time::format_description::well_known::Rfc3339).ok()
    });
    let camera = read.model.clone().or_else(|| read.make.clone());
    let meta = Metadata {
        original: to_original(read),
        ..Metadata::default()
    };
    (
        DiscoveredFile {
            path: path.to_string(),
            size,
            capture_time,
            camera,
        },
        Some(meta),
    )
}

/// A source's root path on this machine, from the workspace's `sources.json` hint (design note
/// 002 §6.6): read directly, since this runs off the coordinator thread and `Workspace`'s reads
/// need no coordination.
fn source_root(workspace: &Workspace, source_id: SourceId) -> Option<PathBuf> {
    let sources = workspace.read_sources().ok()??.current()?;
    let entry = sources.sources.into_iter().find(|s| s.id == source_id)?;
    entry
        .hint
        .get("path")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
}

/// Whether `hash` matches an existing photo already in the catalogue (design note 004 §6.3, item
/// 4): every candidate whose fingerprint matched is checked, reading its own file to compute its
/// hash on demand when the catalogue does not have one recorded yet (cheap: an already-imported
/// file is local, note 004's own reasoning). Returns the matching photo, and, when a candidate's
/// hash had to be computed here, what to backfill onto its catalogue row so the next check is
/// free. **A fingerprint match alone never returns a duplicate**: only a hash match does.
fn find_duplicate(
    catalogue: &Catalogue,
    workspace: &Workspace,
    fingerprint: &Fingerprint,
    hash: &ContentHash,
) -> (Option<PhotoId>, Option<(PhotoId, ContentHash)>) {
    let Ok(candidates) = catalogue.find_by_fingerprint(fingerprint) else {
        return (None, None);
    };
    for candidate in candidates {
        if let Some(existing_hash) = candidate.hash {
            if existing_hash == *hash {
                return (Some(candidate.photo_id), None);
            }
            continue;
        }
        let (Some(source_id), Some(path)) = (candidate.source_id, &candidate.path) else {
            continue;
        };
        let Some(root) = source_root(workspace, source_id) else {
            continue;
        };
        let full = root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let Ok(bytes) = std::fs::read(&full) else {
            continue;
        };
        let Ok((_, computed)) =
            auroraw_format::fingerprint::content_hash(&mut std::io::Cursor::new(&bytes))
        else {
            continue;
        };
        if computed == *hash {
            return (
                Some(candidate.photo_id),
                Some((candidate.photo_id, computed)),
            );
        }
    }
    (None, None)
}

fn build_metadata(job: &ImportJob, meta: Option<Metadata>) -> Metadata {
    let mut meta = meta.unwrap_or_default();
    for (id, path) in &job.template_keywords {
        meta.push_keyword(*id, path.clone());
    }
    meta
}

fn file_entry(
    registration: &Registration,
    role: FileRole,
    planned: &PlannedFile,
    hash: ContentHash,
    fingerprint: Fingerprint,
) -> FileEntry {
    let name = planned
        .destination
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let format = planned
        .destination
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_uppercase());
    FileEntry {
        role,
        name,
        format,
        size: planned.size,
        fingerprint,
        hash: Some(hash),
        locations: vec![Location {
            source: registration.source_id,
            path: {
                let inside = planned.destination.to_string_lossy().replace('\\', "/");
                if registration.prefix.is_empty() {
                    inside
                } else {
                    format!("{}/{inside}", registration.prefix)
                }
            },
            seen: Some(Timestamp::now()),
            extra: Vec::new(),
        }],
        extra: Vec::new(),
    }
}

/// Whether two files hold the same bytes, by whole-file hash.
fn same_content(a: &Path, b: &Path) -> bool {
    let hash = |path: &Path| {
        let mut file = std::fs::File::open(path).ok()?;
        auroraw_format::fingerprint::content_hash(&mut file)
            .ok()
            .map(|(_, hash)| hash)
    };
    matches!((hash(a), hash(b)), (Some(x), Some(y)) if x == y)
}

fn full_backups(job: &ImportJob, backups: &[PathBuf]) -> Vec<PathBuf> {
    backups
        .iter()
        .zip(&job.backup_roots)
        .map(|(rel, root)| root.join(rel))
        .collect()
}

/// What happened to one planned photo.
enum GroupOutcome {
    /// Copied and verified; the photo it was registered as, when it was registered.
    Copied(Option<PhotoId>),
    Skipped,
    Failed(String),
}

/// Reads, verifies and registers one planned photo (its original, and its companion if it has
/// one) as a single `PhotoSidecar` with one or two files (D-032). Nothing is written at all if
/// the original turns out to already be in the catalogue (design note 004 §6.3, item 4): the
/// whole photo is a duplicate, and its companion is not copied either.
fn import_group(
    job: &ImportJob,
    catalogue: Option<&Catalogue>,
    planned: &PlannedPhoto,
    meta_by_path: &HashMap<String, Metadata>,
) -> GroupOutcome {
    let original_read = match read_source(&job.source, &planned.original.source_path) {
        Ok(r) => r,
        Err(e) => return GroupOutcome::Failed(e.to_string()),
    };

    let mut backfill = None;
    // Only a registering import consults the catalogue: a plain copy is what was asked for, even of
    // photos the catalogue already knows.
    if let (Some(catalogue), Some(_)) = (catalogue, &job.registration) {
        let (duplicate, found_backfill) = find_duplicate(
            catalogue,
            &job.workspace,
            &original_read.fingerprint,
            &original_read.hash,
        );
        if duplicate.is_some() {
            if let Some((id, hash)) = found_backfill {
                let _ = job.inbound.send(Inbound::Imported {
                    photo: None,
                    stat: None,
                    backfill: Some((id, hash)),
                });
            }
            return GroupOutcome::Skipped;
        }
        backfill = found_backfill;
    }

    let full_destination = job.dest_root.join(&planned.original.destination);
    let original_backups = full_backups(job, &planned.original.backup_destinations);
    if let Err(e) = write_verified(
        &original_read.bytes,
        &original_read.hash,
        &full_destination,
        &original_backups,
    ) {
        return GroupOutcome::Failed(e.to_string());
    }

    let Some(registration) = &job.registration else {
        // A plain copy: the companion is copied too, and that is all.
        if let Some(companion) = &planned.companion
            && let Ok(companion_read) = read_source(&job.source, &companion.source_path)
        {
            let _ = write_verified(
                &companion_read.bytes,
                &companion_read.hash,
                &job.dest_root.join(&companion.destination),
                &full_backups(job, &companion.backup_destinations),
            );
        }
        return GroupOutcome::Copied(None);
    };

    let photo_id = PhotoId::random();
    let mut photo = PhotoSidecar::new(photo_id);
    photo.meta = build_metadata(
        job,
        meta_by_path.get(&planned.original.source_path).cloned(),
    );
    photo.imported = Some(Timestamp::now());
    photo.files.push(file_entry(
        registration,
        FileRole::Original,
        &planned.original,
        original_read.hash,
        original_read.fingerprint,
    ));

    // A companion that fails to read or copy is not fatal to the photo: the original (the file
    // that is developed) is what matters most, and the failure is visible in the photo having
    // only one file where the plan expected two.
    if let Some(companion) = &planned.companion
        && let Ok(companion_read) = read_source(&job.source, &companion.source_path)
    {
        let full_destination = job.dest_root.join(&companion.destination);
        let companion_backups = full_backups(job, &companion.backup_destinations);
        if write_verified(
            &companion_read.bytes,
            &companion_read.hash,
            &full_destination,
            &companion_backups,
        )
        .is_ok()
        {
            photo.files.push(file_entry(
                registration,
                FileRole::Companion,
                companion,
                companion_read.hash,
                companion_read.fingerprint,
            ));
        }
    }

    if let Err(e) = job.workspace.write_photo(&photo) {
        return GroupOutcome::Failed(e.to_string());
    }
    let stat_path = job.workspace.photo_path(&photo_id);
    let stat = match std::fs::metadata(&stat_path) {
        Ok(m) => stat_of(m.len(), m.modified().ok()),
        Err(e) => return GroupOutcome::Failed(e.to_string()),
    };
    let _ = job.inbound.send(Inbound::Imported {
        photo: Some(Box::new(photo)),
        stat: Some(stat),
        backfill,
    });
    GroupOutcome::Copied(Some(photo_id))
}

fn run(job: ImportJob) {
    let abort = |reason: &str| {
        let _ = job.events.send(Event::ImportAborted {
            job: job.job,
            reason: reason.to_string(),
        });
        let _ = job.events.send(Event::JobFinished(job.job));
    };
    if job.source.state() != SourceState::Online {
        abort("the source is not reachable");
        return;
    }
    let entries = match job.source.list() {
        Ok(entries) => entries,
        Err(e) => {
            abort(&format!("the source cannot be listed: {e}"));
            return;
        }
    };
    let source_root_path = job.source.root().to_path_buf();

    let mut discovered = Vec::with_capacity(entries.len());
    let mut meta_by_path = HashMap::new();
    // Only still images are imported: a card also holds sidecar and system files, and videos are
    // out of scope (spec §1).
    for entry in entries
        .iter()
        .filter(|e| auroraw_imaging::is_photo_file(Path::new(&e.path)))
    {
        let (file, meta) = discover_one(&source_root_path, &entry.path);
        if let Some(meta) = meta {
            meta_by_path.insert(file.path.clone(), meta);
        }
        discovered.push(file);
    }

    let groups = pair_files(discovered, job.profile.pair_rule);

    // A file already at its exact planned place with the same content is not copied again and is
    // not given a numbered twin: a second import of the same card into the same folders adds
    // nothing. (Found by asking the plan where every file would go if nothing were there.)
    let identical: HashSet<String> = plan(
        groups.clone(),
        &job.profile.destination_template,
        &job.profile.backup_templates,
        &mut UsedPaths::new(),
        &|_, _| false,
    )
    .iter()
    .filter(|photo| {
        let destination = job.dest_root.join(&photo.original.destination);
        std::fs::metadata(&destination).is_ok_and(|m| m.is_file() && m.len() == photo.original.size)
            && same_content(
                &source_root_path.join(
                    photo
                        .original
                        .source_path
                        .replace('/', std::path::MAIN_SEPARATOR_STR),
                ),
                &destination,
            )
    })
    .map(|photo| photo.original.source_path.clone())
    .collect();
    let (identical_groups, groups): (Vec<_>, Vec<_>) = groups
        .into_iter()
        .partition(|group| identical.contains(&group.original.path));

    let mut used = UsedPaths::new();
    let on_disk = |root: Root, relative: &Path| match root {
        Root::Destination => job.dest_root.join(relative).exists(),
        Root::Backup(i) => job
            .backup_roots
            .get(i)
            .is_some_and(|backup| backup.join(relative).exists()),
    };
    let planned: Vec<PlannedPhoto> = plan(
        groups,
        &job.profile.destination_template,
        &job.profile.backup_templates,
        &mut used,
        &on_disk,
    );

    let mut state = ImportState::load(&job.state_path).unwrap_or_default();
    let catalogue = Catalogue::open(&job.catalogue_path).ok();

    let total = planned.len() + identical_groups.len();
    let (mut copied, mut skipped, mut failed) = (0, 0, 0);
    for (done, group) in identical_groups.iter().enumerate() {
        skipped += 1;
        let key = group.original.path.clone();
        let outcome = ItemOutcome::Skipped {
            reason: "already at the destination (identical)".into(),
        };
        state.record(key.clone(), outcome.clone());
        let _ = state.save(&job.state_path);
        let _ = job.events.send(Event::ImportItem {
            job: job.job,
            source_path: key,
            photo_id: None,
            outcome,
        });
        let _ = job.events.send(Event::JobProgress {
            job: job.job,
            done: done + 1,
            total,
        });
    }
    let offset = identical_groups.len();
    for (done, planned_photo) in planned.iter().enumerate() {
        let done = done + offset;
        if job.cancel.is_cancelled() {
            let _ = job.events.send(Event::JobCancelled(job.job));
            return;
        }
        let key = planned_photo.original.source_path.clone();
        if state.is_settled(&key) {
            // Resumed: an earlier run already settled this one; count it so the final report
            // covers the whole import, not only this run's share.
            match state.outcome(&key) {
                Some(ItemOutcome::Copied) => copied += 1,
                _ => skipped += 1,
            }
        } else {
            let outcome = import_group(&job, catalogue.as_ref(), planned_photo, &meta_by_path);
            let (item_outcome, photo_id) = match outcome {
                GroupOutcome::Copied(id) => {
                    copied += 1;
                    (ItemOutcome::Copied, id)
                }
                GroupOutcome::Skipped => {
                    skipped += 1;
                    (
                        ItemOutcome::Skipped {
                            reason: "already imported (verified identical)".into(),
                        },
                        None,
                    )
                }
                GroupOutcome::Failed(error) => {
                    failed += 1;
                    (ItemOutcome::Failed { error }, None)
                }
            };
            state.record(key.clone(), item_outcome.clone());
            let _ = state.save(&job.state_path);
            let _ = job.events.send(Event::ImportItem {
                job: job.job,
                source_path: key,
                photo_id,
                outcome: item_outcome,
            });
        }
        let _ = job.events.send(Event::JobProgress {
            job: job.job,
            done: done + 1,
            total,
        });
    }

    if failed == 0 {
        // Nothing left to resume. Leaving the state behind would make a later import from a
        // reformatted card that reuses file names skip files it has never seen.
        let _ = std::fs::remove_file(&job.state_path);
    }
    let _ = job.inbound.send(Inbound::Report(Event::ImportFinished {
        job: job.job,
        copied,
        skipped,
        failed,
    }));
    let _ = job
        .inbound
        .send(Inbound::Report(Event::JobFinished(job.job)));
}
