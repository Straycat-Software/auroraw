// SPDX-License-Identifier: GPL-3.0-or-later
use auroraw_import::ItemOutcome;
use auroraw_types::{KeywordId, PhotoId, SourceId};

use crate::command::Command;
use crate::history::HistoryState;
use crate::job::JobId;

/// Something the engine reports, in the order it happened (architecture §4.3). Delivered on
/// [`crate::EventReceiver`]; batch-drain it rather than reacting to one at a time (§4.3: events
/// are batched, not one wake-up each).
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A command was applied. Carries the command itself, so a caller that submitted several
    /// commands from different threads can reconstruct the exact order the single writer applied
    /// them in (testing strategy §8, the concurrency check).
    Applied(Command),
    /// A command failed; the engine is otherwise unaffected; the next command still runs
    /// (matching the plugin host's "never take the process down", extended here to commands).
    Failed {
        /// The command that failed.
        command: Command,
        /// Why, as text (the concrete error type does not need to cross the event channel).
        error: String,
    },
    /// A rebuild finished.
    RebuildFinished {
        /// How many photos the new catalogue has.
        photos: usize,
        /// How many versions.
        versions: usize,
    },
    /// A reconcile finished.
    ReconcileFinished {
        /// How many photo sidecars were re-read and reapplied.
        changed_photos: usize,
        /// How many photos the workspace no longer has.
        removed_photos: usize,
    },
    /// A photo's catalogue row (and, for a rating or flag change, its sidecar) changed.
    PhotoChanged(PhotoId),
    /// What Undo and Redo would do changed (an action was done, undone or redone, or steps were
    /// forgotten).
    HistoryChanged(HistoryState),
    /// An action was undone or redone: the photos it touched (`PhotoChanged` follows for each), so
    /// that the interface can show them.
    HistoryApplied {
        /// Redone rather than undone.
        redo: bool,
        /// The photos, in the order the action changed them.
        photos: Vec<PhotoId>,
    },
    /// A keyword was added to the vocabulary.
    KeywordCreated(KeywordId),
    /// A keyword was renamed; the catalogue already reflects it. `affected` sidecars still carry
    /// the old name as a snapshot (note 003 §6) and are refreshed by the background job `job`.
    KeywordRenamed {
        /// The keyword.
        keyword_id: KeywordId,
        /// The background refresh job doing the rest of the work.
        job: JobId,
        /// How many sidecars it will touch.
        affected: usize,
    },
    /// A series was made, changed or dissolved (by a step, an undo or a redo, or by detection): the grid lists
    /// photos in series differently now.
    SeriesChanged,
    /// A manual collection was made, changed or deleted, or photos went in or out of one (by a step, an undo, a
    /// redo, or a photo leaving with its source): the collections panel lists them differently now.
    CollectionsChanged,
    /// A keyword and its branch were moved (the catalogue already reflects it; the sidecars' path
    /// snapshots follow in a background job).
    KeywordMoved(KeywordId),
    /// A keyword and its branch were deleted.
    KeywordDeleted(KeywordId),
    /// A background job made progress.
    JobProgress {
        /// The job.
        job: JobId,
        /// How many items it has finished.
        done: usize,
        /// How many it has in total.
        total: usize,
    },
    /// A background job finished (ran to completion).
    JobFinished(JobId),
    /// A background job stopped early, cancelled by `Command::CancelJob`.
    JobCancelled(JobId),
    /// The engine has stopped: no more events follow.
    Stopped,
    /// A source was registered.
    SourceAdded(SourceId),
    /// A source's scan finished (or the source was not reachable, in which case every count is
    /// zero and `reachable` is `false`).
    SourceScanned {
        /// The source scanned.
        source_id: SourceId,
        /// Whether the source could be reached at all.
        reachable: bool,
        /// How many files matched exactly what the catalogue expected.
        confirmed: usize,
        /// How many files changed at their known path (design note 004 §6.5).
        changed: usize,
        /// How many files were relinked silently (D-019).
        relinked: usize,
        /// How many known files were found nowhere.
        missing: usize,
        /// Paths that matched nothing known: offered for [`crate::Command::AddNewPhotos`].
        new: Vec<String>,
        /// How many found files were ambiguous (ambiguity is never resolved automatically).
        ambiguous: usize,
        /// How many found files were confirmed (whole-file hash) second locations of an existing photo (D-036,
        /// D-108): the exact duplicates report.
        second_locations: usize,
    },
    /// Confirmed new files became photos.
    PhotosAdded {
        /// The source they were found in.
        source_id: SourceId,
        /// How many.
        count: usize,
    },
    /// An import started, on the background job that runs it.
    ImportStarted {
        /// The job doing the work.
        job: JobId,
    },
    /// One file the import job looked at was resolved: copied, skipped (design note 004 §6.3,
    /// item 4), or failed (retried on the next resume).
    ImportItem {
        /// The job.
        job: JobId,
        /// Its path inside the source.
        source_path: String,
        /// The new photo, when one was registered (absent for a skip or a failure).
        photo_id: Option<PhotoId>,
        /// What happened.
        outcome: ItemOutcome,
    },
    /// An index job listed its source and knows what it will do: `new_files` files become new
    /// photos, of which `restorable` match photos removed earlier. When `restorable` is not zero
    /// the job waits for [`crate::Command::ContinueIndex`].
    IndexPlanned {
        /// The job.
        job: JobId,
        /// The source.
        source_id: SourceId,
        /// How many photos the source's unknown files make.
        new_files: usize,
        /// How many of them are photos that were removed earlier.
        restorable: usize,
    },
    /// An index job finished.
    IndexFinished {
        /// The job.
        job: JobId,
        /// The source.
        source_id: SourceId,
        /// New photos added.
        added: usize,
        /// Removed photos restored.
        restored: usize,
        /// Files the catalogue already knew.
        known: usize,
        /// Files that could not be read.
        failed: usize,
        /// Files that turned out to be a confirmed second location of an existing photo, from the same source or
        /// a *different* one (D-036, D-108): joined to it instead of becoming a new photo.
        second_locations: usize,
        /// Known files found nowhere this scan (D-109): kept, never removed automatically (D-019, D-031).
        missing: usize,
    },
    /// An index job could not run at all (the source is not reachable, or cannot be listed).
    IndexAborted {
        /// The job.
        job: JobId,
        /// Why, for a person.
        reason: String,
    },
    /// A source was removed from the catalogue.
    SourceRemoved {
        /// The job.
        job: JobId,
        /// The source.
        source_id: SourceId,
        /// Photos taken out of the catalogue (their sidecars are in `removed/`).
        removed: usize,
        /// Photos kept because they have another location.
        kept: usize,
    },
    /// An import job could not run at all (the source vanished or cannot be listed): no
    /// `ImportFinished` follows, only `JobFinished`.
    ImportAborted {
        /// The job.
        job: JobId,
        /// Why, for a person.
        reason: String,
    },
    /// An import job finished (every file it found has a settled outcome).
    ImportFinished {
        /// The job.
        job: JobId,
        /// How many files were copied and verified.
        copied: usize,
        /// How many were already in the catalogue (design note 004 §6.3, item 4).
        skipped: usize,
        /// How many failed and would be retried on the next resume.
        failed: usize,
    },
}
