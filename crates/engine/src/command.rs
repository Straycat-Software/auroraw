// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;

use auroraw_format::sidecar::{ColourLabel, Flag};
use auroraw_import::Profile;
use auroraw_types::{KeywordId, PhotoId, SeriesId, SourceId};

/// A change the engine's single writer applies, in the order it receives them (architecture
/// §4.3). Sent with [`crate::Engine::submit`] (fire and forget) or
/// [`crate::Engine::submit_and_wait`] (blocks for the outcome).
// `Import` carries a whole profile and several paths; commands are built one at a time by a person's
// gesture, so the size of the largest variant costs nothing worth a `Box` in every match.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Rebuilds the catalogue from the workspace (architecture §5.4).
    Rebuild,
    /// Compares the workspace against the catalogue's stored stats and refreshes every sidecar
    /// that changed (architecture §5.3): this is how an edit made outside Auroraw, or by another
    /// machine, reaches the catalogue.
    Reconcile,
    /// Sets a photo's own rating (0 to 5). The effective rating (D-063) follows unless the main
    /// version overrides it.
    SetRating {
        /// The photo.
        photo_id: PhotoId,
        /// The new rating.
        rating: u8,
    },
    /// Sets a photo's own flag.
    SetFlag {
        /// The photo.
        photo_id: PhotoId,
        /// The new flag, or `None` to clear it.
        flag: Option<Flag>,
    },
    /// Sets a photo's own colour label (`None` takes it off). A step of the history, and usable in a batch.
    SetLabel {
        /// The photo.
        photo_id: PhotoId,
        /// The colour, or `None` for no label.
        label: Option<ColourLabel>,
    },
    /// Applies several edits (`SetRating`, `SetFlag`, `SetLabel`, `AddKeyword`, `RemoveKeyword`, `CreateKeyword`) as **one action**:
    /// one step of the history, and all or nothing (a failing edit takes back the ones before it). How a
    /// batch of ratings, a series resolved, or a paste of metadata onto many photos is undone in one go.
    Batch {
        /// The edits, in order.
        commands: Vec<Command>,
    },
    /// Undoes the last action of the person's (D-096). Reports [`crate::Outcome::History`], or
    /// [`crate::Outcome::Nothing`] when there is nothing to undo.
    Undo,
    /// Redoes the action that was undone last; a new action discards what was undone.
    Redo,
    /// Adds a keyword to a photo.
    AddKeyword {
        /// The photo.
        photo_id: PhotoId,
        /// The keyword.
        keyword_id: KeywordId,
    },
    /// Removes a keyword from a photo.
    RemoveKeyword {
        /// The photo.
        photo_id: PhotoId,
        /// The keyword.
        keyword_id: KeywordId,
    },
    /// Adds a keyword to the vocabulary. An action of the person's (D-099): it is a step of the history,
    /// and it can be part of a [`Command::Batch`] with the keyword's first assignments (which name it by
    /// `id`, chosen by the caller for that reason). Refused when a sibling already has the name.
    CreateKeyword {
        /// Its name.
        name: String,
        /// Its parent, or `None` for a top-level keyword.
        parent: Option<KeywordId>,
        /// Its identifier, or `None` to draw one (reported by [`crate::Outcome::KeywordCreated`]).
        id: Option<KeywordId>,
    },
    /// Renames a keyword (a step of the history). Changes the vocabulary and every catalogue row it (or a
    /// descendant of it) affects immediately; the sidecars that carry a now-stale name snapshot are
    /// refreshed afterwards, in the background (note 003 §6), reported through
    /// [`crate::Event::KeywordRenamed`] and the `Job*` events that follow it.
    RenameKeyword {
        /// The keyword.
        keyword_id: KeywordId,
        /// Its new name.
        new_name: String,
    },
    /// Moves a keyword, with its branch, under another keyword or to the top level (a step of the
    /// history). Refused under itself or a descendant, and when a sibling has its name there. The paths
    /// the sidecars carry are refreshed in the background, as for a rename.
    MoveKeyword {
        /// The keyword.
        keyword_id: KeywordId,
        /// Its new parent, or `None` for the top level.
        new_parent: Option<KeywordId>,
    },
    /// Deletes a keyword and its whole branch (a step of the history, and an undo brings all of it back):
    /// the keywords leave the vocabulary, the catalogue and every photo that carried one. Done on the
    /// coordinator, like a batch of ratings; [`crate::Outcome::KeywordsChanged`] says how many keywords
    /// and photos it touched.
    DeleteKeyword {
        /// The keyword at the top of the branch.
        keyword_id: KeywordId,
    },
    /// Groups these photos into a new **manual** series (a step of the history, D-101): they leave the series they
    /// were in (one left with fewer than two photos is dissolved). Merging two series is grouping all their photos;
    /// splitting one is grouping some of its members. Reports [`crate::Outcome::SeriesGrouped`].
    GroupPhotos {
        /// The photos, at least two.
        photos: Vec<PhotoId>,
    },
    /// Takes these photos out of their series (a step of the history); a series left with fewer than two photos is
    /// dissolved.
    RemoveFromSeries {
        /// The photos.
        photos: Vec<PhotoId>,
    },
    /// Dissolves a series (a step of the history): its photos belong to none, and keep their flags.
    DissolveSeries {
        /// The series.
        series: SeriesId,
    },
    /// Resolves a series (spec §5.3, D-035; one step of the history): the photos in `keep` are Picked, the others
    /// of the series Rejected, and the series remembers that it is resolved and what was kept. Undoing it gives
    /// every flag back.
    ResolveSeries {
        /// The series.
        series: SeriesId,
        /// The photos to keep, members of the series (at least one).
        keep: Vec<PhotoId>,
    },
    /// Reopens a resolved series (a step of the history): it is no longer resolved; the flags are left as they are.
    ReopenSeries {
        /// The series.
        series: SeriesId,
    },
    /// Sets the largest gap, in seconds, between two photos of one series when series are formed by themselves
    /// (D-101). Not a step of the history.
    SetSeriesGap {
        /// The gap, in seconds.
        seconds: u32,
    },
    /// Forms series among the photos that are in none (what the coordinator also does when a scan or an import
    /// finishes). With `regroup`, the series that were formed by themselves and are not resolved are dissolved
    /// first, so that a new gap applies to them; series made by hand or resolved are never touched. Not a step of
    /// the history.
    DetectSeries {
        /// Whether to dissolve the automatic, unresolved series first.
        regroup: bool,
    },
    /// Cancels a background job (a keyword rename's sidecar refresh) started earlier.
    CancelJob {
        /// The job to cancel.
        job_id: crate::job::JobId,
    },
    /// Registers a folder or a removable volume's mount point as a source, "in place": nothing
    /// is copied (spec §5.1). `root` is this machine's real path to it, kept only in the
    /// workspace's `hint` and the catalogue (design note 002 §6.6), never synced.
    AddSource {
        /// Its display name.
        name: String,
        /// Where it is on this machine.
        root: PathBuf,
        /// `sources::filesystem::LOCAL_FOLDER` or `sources::filesystem::REMOVABLE_VOLUME`.
        kind: String,
    },
    /// Scans a source and reconciles what it finds against the catalogue (design note 004
    /// §6.3-§6.4): confirms unchanged files, marks a changed or missing one, relinks a moved or
    /// renamed one silently, and reports new and ambiguous files for
    /// [`Command::AddNewPhotos`] or a person to resolve. Does nothing if the source is not
    /// reachable right now.
    ScanSource {
        /// The source to scan.
        source_id: SourceId,
    },
    /// Adds photos for files a [`Command::ScanSource`] reported as new, once confirmed (D-019):
    /// never on its own. Each photo's metadata starts empty; reading it is WP5's job.
    AddNewPhotos {
        /// The source the files were found in.
        source_id: SourceId,
        /// Their paths inside the source, as `Event::SourceScanned` (or a fresh scan) reported.
        paths: Vec<String>,
    },
    /// Copies every photo file `source_root` holds (a card or a folder; it is not a source of the
    /// catalogue and nothing is written to it, D-031) into `destination_root`, laid out by the
    /// profile's template (RAW and JPEG of one shot travel together, D-032), verified by whole-file
    /// hash (design note 004 §6.3, item 5), and to each of `backup_roots` too. A file that is already
    /// at its planned place with the same content is skipped, and one whose name is taken by
    /// something else is numbered: nothing is ever overwritten.
    ///
    /// With a `registration` (the destination is inside one of the catalogue's sources) each photo
    /// also becomes a photo of the catalogue, with the profile's metadata template written to its
    /// sidecar, and a photo the catalogue already has (by whole-file hash) is skipped. Without one it
    /// is a plain copy.
    ///
    /// Everything runs on a background worker (`Event::JobProgress`, one `Event::PhotoChanged` per
    /// registered photo): the coordinator never blocks on a large card (spec §5.2, "does not stall
    /// the interface thread"). `state_path` is where this job's resumable progress is kept: an
    /// interrupted import, resubmitted with the same path, picks up where it stopped.
    Import {
        /// The card or folder to copy from.
        source_root: PathBuf,
        /// The folder to copy into.
        destination_root: PathBuf,
        /// Where the photos are registered, or `None` for a plain copy.
        registration: Option<crate::import_job::Registration>,
        /// Destination templates, pairing, and the metadata template.
        profile: Profile,
        /// A session name for the `{shoot}` template token.
        shoot: Option<String>,
        /// Extra verified-copy destinations, resolved to real paths.
        backup_roots: Vec<PathBuf>,
        /// Where this job's resumable state is kept.
        state_path: PathBuf,
    },
    /// Scans a registered source in the background and adds every file it holds that the
    /// catalogue does not know as a photo (RAW and JPEG of one shot as one photo, D-032), reading
    /// each file's metadata into its sidecar (D-074): nothing is copied (spec §5.1, "adding a
    /// folder in place"). Reports `Event::IndexPlanned` first; if some of the files match photos
    /// that were removed earlier (their sidecars wait in the workspace's `removed/`), the job
    /// pauses there until [`Command::ContinueIndex`] says whether to restore them.
    IndexSource {
        /// The source to scan.
        source_id: SourceId,
        /// Sources whose folders are inside this one, to be merged into it first: their photos are
        /// kept, with their ratings and versions, and become this source's (each location gets this
        /// source's identifier and the folder's place in it), then their entries go.
        merge: Vec<SourceId>,
    },
    /// Answers the pause of an index job (`Event::IndexPlanned` with something to restore).
    ContinueIndex {
        /// The index job.
        job_id: crate::job::JobId,
        /// Whether to restore the photos that were removed earlier (with their ratings, keywords
        /// and versions), rather than add their files as new photos.
        restore: bool,
    },
    /// Takes a source out of the catalogue, in the background. Every photo whose original is only
    /// in this source is removed from the catalogue and its sidecars are moved, recoverably, to the
    /// workspace's `removed/`; a photo that has another location keeps it. Originals are never
    /// touched.
    RemoveSource {
        /// The source to remove.
        source_id: SourceId,
    },
}
