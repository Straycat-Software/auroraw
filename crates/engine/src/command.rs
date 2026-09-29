// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;

use auroraw_format::sidecar::{ColourLabel, CustomField, Flag, Metadata};
use auroraw_import::Profile;
use auroraw_types::{CollectionId, KeywordId, PhotoId, SeriesId, SourceId};

/// One of the plain-text metadata fields the metadata panel edits (spec §5.7; WP10, slice 1): not
/// rating, flag, label or keywords, which have their own commands, and not GPS, which is its own
/// overlay mechanism (not this slice's). `Creator` and `Persons` are the two list fields; every
/// other one is a single line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataField {
    /// The title.
    Title,
    /// The caption.
    Caption,
    /// The creators, one a line.
    Creator,
    /// The copyright notice.
    Rights,
    /// The usage terms.
    UsageTerms,
    /// The web statement of rights.
    WebStatement,
    /// The credit line.
    Credit,
    /// The source.
    Source,
    /// The headline.
    Headline,
    /// The instructions.
    Instructions,
    /// The sublocation.
    Sublocation,
    /// The city.
    City,
    /// The region or state.
    Region,
    /// The country.
    Country,
    /// The ISO country code.
    CountryCode,
    /// The persons shown, one a line.
    Persons,
    /// The event.
    Event,
    /// A field outside the fixed set above, by name (WP10's own custom-field support): no UI
    /// creates one yet, but the engine and the format already carry it end to end.
    Custom(String),
}

impl MetadataField {
    /// The field's current text, as the panel shows it: a single line for most fields, one name a
    /// line for `Creator` and `Persons`. Empty when the field is not set (or, for `Custom`, not
    /// present).
    pub fn get(&self, meta: &Metadata) -> String {
        match self {
            Self::Title => meta.title.clone().unwrap_or_default(),
            Self::Caption => meta.caption.clone().unwrap_or_default(),
            Self::Creator => meta.creator.join("\n"),
            Self::Rights => meta.rights.clone().unwrap_or_default(),
            Self::UsageTerms => meta.usage_terms.clone().unwrap_or_default(),
            Self::WebStatement => meta.web_statement.clone().unwrap_or_default(),
            Self::Credit => meta.credit.clone().unwrap_or_default(),
            Self::Source => meta.source.clone().unwrap_or_default(),
            Self::Headline => meta.headline.clone().unwrap_or_default(),
            Self::Instructions => meta.instructions.clone().unwrap_or_default(),
            Self::Sublocation => meta.sublocation.clone().unwrap_or_default(),
            Self::City => meta.city.clone().unwrap_or_default(),
            Self::Region => meta.region.clone().unwrap_or_default(),
            Self::Country => meta.country.clone().unwrap_or_default(),
            Self::CountryCode => meta.country_code.clone().unwrap_or_default(),
            Self::Persons => meta.persons.join("\n"),
            Self::Event => meta.event.clone().unwrap_or_default(),
            Self::Custom(name) => meta
                .custom
                .iter()
                .find(|c| &c.name == name)
                .map(|c| c.value.clone())
                .unwrap_or_default(),
        }
    }

    /// Sets the field from `value`: empty clears it (for `Custom`, removes it rather than leaving
    /// an empty one behind).
    pub fn set(&self, meta: &mut Metadata, value: String) {
        fn one(value: String) -> Option<String> {
            (!value.is_empty()).then_some(value)
        }
        fn many(value: &str) -> Vec<String> {
            value
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        }
        match self {
            Self::Title => meta.title = one(value),
            Self::Caption => meta.caption = one(value),
            Self::Creator => meta.creator = many(&value),
            Self::Rights => meta.rights = one(value),
            Self::UsageTerms => meta.usage_terms = one(value),
            Self::WebStatement => meta.web_statement = one(value),
            Self::Credit => meta.credit = one(value),
            Self::Source => meta.source = one(value),
            Self::Headline => meta.headline = one(value),
            Self::Instructions => meta.instructions = one(value),
            Self::Sublocation => meta.sublocation = one(value),
            Self::City => meta.city = one(value),
            Self::Region => meta.region = one(value),
            Self::Country => meta.country = one(value),
            Self::CountryCode => meta.country_code = one(value),
            Self::Persons => meta.persons = many(&value),
            Self::Event => meta.event = one(value),
            Self::Custom(name) => {
                meta.custom.retain(|c| &c.name != name);
                if !value.is_empty() {
                    meta.custom.push(CustomField {
                        name: name.clone(),
                        value,
                    });
                }
            }
        }
    }

    /// A stable, ASCII key for the field, the way the interface names it (a QML string, JSON):
    /// kebab-case for the 17 known fields, `custom:<name>` for a custom one (no UI sends this yet,
    /// but the key round-trips it all the same).
    pub fn key(&self) -> String {
        match self {
            Self::Title => "title".into(),
            Self::Caption => "caption".into(),
            Self::Creator => "creator".into(),
            Self::Rights => "rights".into(),
            Self::UsageTerms => "usage-terms".into(),
            Self::WebStatement => "web-statement".into(),
            Self::Credit => "credit".into(),
            Self::Source => "source".into(),
            Self::Headline => "headline".into(),
            Self::Instructions => "instructions".into(),
            Self::Sublocation => "sublocation".into(),
            Self::City => "city".into(),
            Self::Region => "region".into(),
            Self::Country => "country".into(),
            Self::CountryCode => "country-code".into(),
            Self::Persons => "persons".into(),
            Self::Event => "event".into(),
            Self::Custom(name) => format!("custom:{name}"),
        }
    }

    /// The field a key names, the reverse of [`Self::key`]; `None` for anything else (an unknown
    /// key is refused, not guessed at).
    pub fn parse(key: &str) -> Option<Self> {
        Some(match key {
            "title" => Self::Title,
            "caption" => Self::Caption,
            "creator" => Self::Creator,
            "rights" => Self::Rights,
            "usage-terms" => Self::UsageTerms,
            "web-statement" => Self::WebStatement,
            "credit" => Self::Credit,
            "source" => Self::Source,
            "headline" => Self::Headline,
            "instructions" => Self::Instructions,
            "sublocation" => Self::Sublocation,
            "city" => Self::City,
            "region" => Self::Region,
            "country" => Self::Country,
            "country-code" => Self::CountryCode,
            "persons" => Self::Persons,
            "event" => Self::Event,
            _ => {
                let name = key.strip_prefix("custom:")?;
                Self::Custom(name.to_string())
            }
        })
    }
}

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
    /// Sets one metadata field of a photo (spec §5.7; WP10, slice 1). A step of the history, and
    /// usable in a batch, the same way `SetRating` is: "multiple values" over a selection, applied
    /// to all of it as one undoable action.
    SetMetadataField {
        /// The photo.
        photo_id: PhotoId,
        /// Which field.
        field: MetadataField,
        /// The field's new text (empty clears it).
        value: String,
    },
    /// Applies several edits (`SetRating`, `SetFlag`, `SetLabel`, `AddKeyword`, `RemoveKeyword`, `CreateKeyword`) as **one action**:
    /// one step of the history. How a batch of ratings, a series resolved, or a paste of metadata onto many
    /// photos is undone in one go. Up to [`crate::batch_job::BACKGROUND_THRESHOLD`] items, this is all or
    /// nothing (a failing edit takes back the ones before it) and blocks the coordinator until it is done;
    /// past it, it runs as a cancellable background job instead (D-126 volet B, `Outcome::BatchStarted`):
    /// no rollback (a cancelled batch keeps whatever prefix it already applied, as its own, possibly
    /// partial, undoable step), since holding thousands of edits back for an all-or-nothing guarantee
    /// would defeat the point of not blocking on them.
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
    /// Removes several keywords from one photo in a single change (D-126 volet B): what a large
    /// `DeleteKeyword` sweep sends per photo instead of one `RemoveKeyword` per keyword of the
    /// branch, so the whole branch's departure from one photo is one `Change::Keywords`, not
    /// several. Not built by a person's own gesture; the coordinator's own background job uses it.
    RemoveKeywords {
        /// The photo.
        photo_id: PhotoId,
        /// The keywords.
        keyword_ids: Vec<KeywordId>,
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
    /// Deletes a keyword and its whole branch: the keywords leave the vocabulary, the catalogue and every
    /// photo that carried one, the photos first (so that a rebuild never finds a sidecar naming a keyword
    /// that is gone). Up to [`crate::batch_job::BACKGROUND_THRESHOLD`] photos, this is one step of the
    /// history, done on the coordinator like a batch of ratings, all or nothing; an undo brings all of it
    /// back; [`crate::Outcome::KeywordsChanged`] says how many keywords and photos it touched. Past that
    /// many, it runs as a cancellable background job instead (D-126 volet B, `Outcome::DeleteKeywordStarted`):
    /// the keyword stays in the vocabulary until every carrying photo has swept it, so a cancelled sweep
    /// never leaves a dangling reference; the history entry (a possibly partial one, if cancelled) is only
    /// recorded once the sweep stops, and resubmitting the same keyword resumes it, exactly like
    /// `Command::RemoveSource` finishing what an earlier run left.
    DeleteKeyword {
        /// The keyword at the top of the branch.
        keyword_id: KeywordId,
    },
    /// Sets a keyword's synonyms and export flag (a step of the history, spec §5.7, D-045, WP10 slice 2):
    /// both fields together, as one edit. No uniqueness check (a synonym is not a name anything else keys
    /// off of, and two keywords sharing one is not an error).
    SetKeywordProperties {
        /// The keyword.
        keyword_id: KeywordId,
        /// Its alternative names, replacing whatever it had.
        synonyms: Vec<String>,
        /// Whether it is included when photos are exported. Nothing reads this yet (WP5.8).
        export: bool,
    },
    /// Makes a **manual** collection (a step of the history, WP10 slice 3), with these photos as its first
    /// members: making it and filling it is one step. Refused when a sibling already has the name (whatever the
    /// case). Reports [`crate::Outcome::CollectionCreated`].
    CreateCollection {
        /// Its name.
        name: String,
        /// The collection it goes inside, or `None` for the top level.
        parent: Option<CollectionId>,
        /// Its identifier, or `None` to draw one.
        id: Option<CollectionId>,
        /// Its first members, in this order.
        photos: Vec<PhotoId>,
    },
    /// Renames a collection (a step of the history).
    RenameCollection {
        /// The collection.
        collection_id: CollectionId,
        /// Its new name.
        new_name: String,
    },
    /// Moves a collection, with what is inside it, under another collection or to the top level (a step of the
    /// history). Refused under itself or one of its own, and when a sibling has its name there.
    MoveCollection {
        /// The collection.
        collection_id: CollectionId,
        /// Its new parent, or `None` for the top level.
        new_parent: Option<CollectionId>,
    },
    /// Deletes a collection and the collections inside it (a step of the history; an undo brings all of them
    /// back). The photos are not touched: only their membership goes.
    DeleteCollection {
        /// The collection at the top of the branch.
        collection_id: CollectionId,
    },
    /// Adds photos to a collection (a step of the history), after the members it has; the ones it holds already
    /// are left where they are.
    AddToCollection {
        /// The collection.
        collection_id: CollectionId,
        /// The photos.
        photos: Vec<PhotoId>,
    },
    /// Takes photos out of a collection (a step of the history); the ones it does not hold are ignored.
    RemoveFromCollection {
        /// The collection.
        collection_id: CollectionId,
        /// The photos.
        photos: Vec<PhotoId>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn every_known_field() -> Vec<MetadataField> {
        vec![
            MetadataField::Title,
            MetadataField::Caption,
            MetadataField::Creator,
            MetadataField::Rights,
            MetadataField::UsageTerms,
            MetadataField::WebStatement,
            MetadataField::Credit,
            MetadataField::Source,
            MetadataField::Headline,
            MetadataField::Instructions,
            MetadataField::Sublocation,
            MetadataField::City,
            MetadataField::Region,
            MetadataField::Country,
            MetadataField::CountryCode,
            MetadataField::Persons,
            MetadataField::Event,
        ]
    }

    #[test]
    fn every_known_fields_key_parses_back_to_itself_and_a_custom_ones_name_round_trips() {
        for field in every_known_field() {
            assert_eq!(MetadataField::parse(&field.key()), Some(field));
        }
        let custom = MetadataField::Custom("Model release".into());
        assert_eq!(custom.key(), "custom:Model release");
        assert_eq!(MetadataField::parse(&custom.key()), Some(custom));
        assert_eq!(MetadataField::parse("not-a-field"), None);
    }

    #[test]
    fn getting_and_setting_a_field_agree_with_each_other_for_every_known_field() {
        for field in every_known_field() {
            let mut meta = Metadata::default();
            assert_eq!(field.get(&meta), "", "{field:?} starts empty");
            field.set(&mut meta, "a value".into());
            assert_eq!(field.get(&meta), "a value");
            field.set(&mut meta, "".into());
            assert_eq!(field.get(&meta), "", "{field:?} clears back to empty");
        }
    }

    #[test]
    fn a_custom_field_is_found_by_name_among_several() {
        let mut meta = Metadata::default();
        MetadataField::Custom("A".into()).set(&mut meta, "1".into());
        MetadataField::Custom("B".into()).set(&mut meta, "2".into());
        assert_eq!(MetadataField::Custom("A".into()).get(&meta), "1");
        assert_eq!(MetadataField::Custom("B".into()).get(&meta), "2");
        assert_eq!(MetadataField::Custom("C".into()).get(&meta), "");
        MetadataField::Custom("A".into()).set(&mut meta, "".into());
        assert_eq!(meta.custom.len(), 1, "cleared, not left empty");
        assert_eq!(meta.custom[0].name, "B");
    }
}
