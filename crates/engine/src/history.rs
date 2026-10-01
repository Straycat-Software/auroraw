// SPDX-License-Identifier: GPL-3.0-or-later
//! The command journal (decision D-096): what a person did to their photos, so that it can be undone
//! and redone. The engine owns it because the coordinator is the single writer (architecture §4.2): it
//! sees the state before it changes it, so it records what changed, and nothing above it can miss an
//! action or record one wrongly.
//!
//! A [`Change`] is a state change as a **before and after pair**, not a command to replay: undo and redo
//! are the same operation in two directions, a never-rated photo goes back to *unset* rather than to 0,
//! and a new kind of edit adds a variant and its two directions and nothing else. An [`Entry`] is one
//! action of the person's, possibly many changes (a batch, resolving a series). The history lives in
//! memory, for the open workspace, and is bounded; the persistent history of a *version* (M2) is another
//! mechanism, in the sidecars.

use std::collections::VecDeque;

use auroraw_format::sidecar::{Flag, Metadata, PlaceFilled};
use auroraw_format::state::{Collection, KeywordEntry, Series};
use auroraw_types::{CollectionId, KeywordId, PhotoId, SeriesId};

use crate::command::MetadataField;

/// How many steps the history keeps before it forgets the oldest.
pub const DEFAULT_LIMIT: usize = 500;

/// A photo's keywords, as the sidecar holds them: identifiers and, aligned with them, their paths as of
/// the last write.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeywordSet {
    /// The identifiers.
    pub ids: Vec<KeywordId>,
    /// Their paths.
    pub paths: Vec<String>,
}

/// Which way a change is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Back to the state before.
    Undo,
    /// Forward to the state after.
    Redo,
}

/// What was done to the vocabulary (it names the step, and tells the engine what to check).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VocabularyAction {
    /// A keyword was made.
    Create,
    /// A keyword was renamed.
    Rename,
    /// A keyword was moved under another (or to the top level).
    Move,
    /// A keyword and its branch were deleted.
    Delete,
    /// A keyword's synonyms and export flag were set (WP10 slice 2).
    SetProperties,
}

/// What was done to a series (it names the step).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesAction {
    /// Photos were grouped into a new series.
    Group,
    /// Photos left their series, or a series was dissolved.
    Ungroup,
    /// A series was resolved (the others rejected, the kept ones picked).
    Resolve,
    /// A resolved series was reopened.
    Reopen,
}

/// What was done to collections (it names the step).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionAction {
    /// A collection was made (with its first photos).
    Create,
    /// A collection was renamed.
    Rename,
    /// A collection was moved under another (or to the top level).
    Move,
    /// A collection and the ones inside it were deleted.
    Delete,
    /// Photos were added to a collection.
    Add,
    /// Photos were taken out of a collection.
    Remove,
}

/// One collection, as its state file was and as it became (`None`: there was none).
#[derive(Debug, Clone, PartialEq)]
pub struct CollectionDelta {
    /// Which collection.
    pub id: CollectionId,
    /// Its state before.
    pub before: Option<Collection>,
    /// Its state after.
    pub after: Option<Collection>,
}

/// One keyword of the vocabulary, as it was and as it became (`None`: it did not exist).
#[derive(Debug, Clone, PartialEq)]
pub struct KeywordDelta {
    /// Which keyword.
    pub id: KeywordId,
    /// The entry before.
    pub before: Option<KeywordEntry>,
    /// The entry after.
    pub after: Option<KeywordEntry>,
}

/// A photo's place fields and the record of what Auroraw filled into them, together: the unit the place
/// names change and undo as.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaceState {
    /// `photoshop:City`.
    pub city: Option<String>,
    /// `photoshop:State`.
    pub region: Option<String>,
    /// `photoshop:Country`.
    pub country: Option<String>,
    /// `Iptc4xmpCore:CountryCode`.
    pub country_code: Option<String>,
    /// `aur:PlaceFilled`.
    pub filled: Option<PlaceFilled>,
}

impl PlaceState {
    /// The text of a field in this state (`None`: empty).
    pub fn get(&self, field: auroraw_format::sidecar::PlaceField) -> Option<&str> {
        use auroraw_format::sidecar::PlaceField;
        match field {
            PlaceField::City => self.city.as_deref(),
            PlaceField::Region => self.region.as_deref(),
            PlaceField::Country => self.country.as_deref(),
            PlaceField::CountryCode => self.country_code.as_deref(),
        }
    }

    /// The state of `meta`.
    pub fn of(meta: &Metadata) -> Self {
        Self {
            city: meta.city.clone(),
            region: meta.region.clone(),
            country: meta.country.clone(),
            country_code: meta.country_code.clone(),
            filled: meta.place_filled.clone(),
        }
    }

    /// Puts `meta` in this state.
    pub fn put(&self, meta: &mut Metadata) {
        meta.city = self.city.clone();
        meta.region = self.region.clone();
        meta.country = self.country.clone();
        meta.country_code = self.country_code.clone();
        meta.place_filled = self.filled.clone();
    }
}

/// One state change of one photo, or of the vocabulary, as it was and as it became.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// The photo's own rating.
    Rating {
        /// The photo.
        photo: PhotoId,
        /// The rating before (`None`: never rated).
        before: Option<u8>,
        /// The rating after.
        after: Option<u8>,
    },
    /// The photo's own flag.
    Flag {
        /// The photo.
        photo: PhotoId,
        /// The flag before.
        before: Option<Flag>,
        /// The flag after.
        after: Option<Flag>,
    },
    /// The photo's own colour label (the sidecar's text, so that a label another program wrote and that is
    /// not one of ours goes back exactly).
    Label {
        /// The photo.
        photo: PhotoId,
        /// The label before.
        before: Option<String>,
        /// The label after.
        after: Option<String>,
    },
    /// The photo's keywords.
    Keywords {
        /// The photo.
        photo: PhotoId,
        /// The keywords before.
        before: KeywordSet,
        /// The keywords after.
        after: KeywordSet,
    },
    /// One of the photo's metadata fields (spec §5.7; WP10, slice 1).
    Metadata {
        /// The photo.
        photo: PhotoId,
        /// Which field.
        field: MetadataField,
        /// Its text before.
        before: String,
        /// Its text after.
        after: String,
        /// For a place field (city, region, country, country code): the record of what Auroraw filled as it
        /// was and as it became, because a person's write takes the field out of it (design note 008 §4) and
        /// undoing the write brings it back. `None` for every other field.
        place: Option<(Option<PlaceFilled>, Option<PlaceFilled>)>,
    },
    /// The place names of a photo (the four place fields and the record of what Auroraw filled), as they
    /// were and as they became: what finding the place names does, undone as one (WP10, design note 008).
    Place {
        /// The photo.
        photo: PhotoId,
        /// Before.
        before: PlaceState,
        /// After.
        after: PlaceState,
    },
    /// A series, as its state file was and as it became (`None`: there was none). Applied by the coordinator
    /// (the file and the catalogue's rows), not by [`Change::apply`].
    Series {
        /// What was done.
        action: SeriesAction,
        /// The series.
        id: SeriesId,
        /// Its state before.
        before: Option<Series>,
        /// Its state after.
        after: Option<Series>,
    },
    /// The vocabulary: only the entries that changed. Applied by the coordinator (it also brings the
    /// catalogue and the sidecars' path snapshots in line), not by [`Change::apply`].
    Vocabulary {
        /// What was done.
        action: VocabularyAction,
        /// The keywords it touched.
        keywords: Vec<KeywordDelta>,
    },
    /// Manual collections: the state files that changed, as they were and as they became. Applied by the
    /// coordinator (the files and the catalogue's rows), not by [`Change::apply`] (WP10, slice 3).
    Collections {
        /// What was done.
        action: CollectionAction,
        /// The collections it touched.
        deltas: Vec<CollectionDelta>,
    },
}

impl Change {
    /// The photo the change is about (none for a change of the vocabulary, a series or a collection).
    pub fn photo(&self) -> Option<PhotoId> {
        match self {
            Change::Rating { photo, .. }
            | Change::Flag { photo, .. }
            | Change::Label { photo, .. }
            | Change::Keywords { photo, .. }
            | Change::Metadata { photo, .. }
            | Change::Place { photo, .. } => Some(*photo),
            Change::Vocabulary { .. } | Change::Series { .. } | Change::Collections { .. } => None,
        }
    }

    fn kind(&self) -> LabelKind {
        match self {
            Change::Rating { .. } => LabelKind::Rating,
            Change::Flag { .. } => LabelKind::Flag,
            Change::Label { .. } => LabelKind::ColourLabel,
            Change::Keywords { .. } => LabelKind::Keywords,
            Change::Metadata { field, .. } => LabelKind::of_metadata_field(field),
            Change::Place { .. } => LabelKind::PlaceNames,
            Change::Series { action, .. } => match action {
                SeriesAction::Group => LabelKind::SeriesGroup,
                SeriesAction::Ungroup => LabelKind::SeriesUngroup,
                SeriesAction::Resolve => LabelKind::SeriesResolve,
                SeriesAction::Reopen => LabelKind::SeriesReopen,
            },
            Change::Vocabulary { action, .. } => match action {
                VocabularyAction::Create => LabelKind::KeywordCreate,
                VocabularyAction::Rename => LabelKind::KeywordRename,
                VocabularyAction::Move => LabelKind::KeywordMove,
                VocabularyAction::Delete => LabelKind::KeywordDelete,
                VocabularyAction::SetProperties => LabelKind::KeywordProperties,
            },
            Change::Collections { action, .. } => match action {
                CollectionAction::Create => LabelKind::CollectionCreate,
                CollectionAction::Rename => LabelKind::CollectionRename,
                CollectionAction::Move => LabelKind::CollectionMove,
                CollectionAction::Delete => LabelKind::CollectionDelete,
                CollectionAction::Add => LabelKind::CollectionAdd,
                CollectionAction::Remove => LabelKind::CollectionRemove,
            },
        }
    }

    /// Puts `meta` in the state `direction` leads to (nothing for a change of the vocabulary).
    pub fn apply(&self, meta: &mut Metadata, direction: Direction) {
        let undo = direction == Direction::Undo;
        match self {
            Change::Rating { before, after, .. } => {
                meta.rating = if undo { *before } else { *after }
            }
            Change::Flag { before, after, .. } => meta.flag = if undo { *before } else { *after },
            Change::Label { before, after, .. } => {
                meta.label = if undo { before.clone() } else { after.clone() }
            }
            Change::Keywords { before, after, .. } => {
                let set = if undo { before } else { after };
                meta.keyword_ids = set.ids.clone();
                meta.keyword_paths = set.paths.clone();
            }
            Change::Metadata {
                field,
                before,
                after,
                place,
                ..
            } => {
                field.set(meta, if undo { before.clone() } else { after.clone() });
                if let Some((was, became)) = place {
                    meta.place_filled = if undo { was.clone() } else { became.clone() };
                }
            }
            Change::Place { before, after, .. } => {
                if undo { before } else { after }.put(meta);
            }
            Change::Vocabulary { .. } | Change::Series { .. } | Change::Collections { .. } => {}
        }
    }
}

/// What a step is about, for a menu to name it (the sentence is the interface's, so that it is translated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelKind {
    /// Ratings.
    Rating,
    /// Flags.
    Flag,
    /// Colour labels.
    ColourLabel,
    /// Keywords.
    Keywords,
    /// The title (spec §5.7; WP10, slice 1).
    MetaTitle,
    /// The caption.
    MetaCaption,
    /// The creators.
    MetaCreator,
    /// The copyright notice.
    MetaRights,
    /// The usage terms.
    MetaUsageTerms,
    /// The web statement of rights.
    MetaWebStatement,
    /// The credit line.
    MetaCredit,
    /// The source.
    MetaSource,
    /// The headline.
    MetaHeadline,
    /// The instructions.
    MetaInstructions,
    /// The sublocation.
    MetaSublocation,
    /// The city.
    MetaCity,
    /// The region or state.
    MetaRegion,
    /// The country.
    MetaCountry,
    /// The ISO country code.
    MetaCountryCode,
    /// The persons shown.
    MetaPersons,
    /// The event.
    MetaEvent,
    /// A custom metadata field (no UI makes one yet, but the engine already names this step
    /// distinctly from the seventeen known fields above, by a single shared label: a bounded set
    /// of menu strings cannot name an unbounded set of field names).
    MetaCustom,
    /// Several kinds of change at once.
    Batch,
    /// A keyword was made (with or without photos given it).
    KeywordCreate,
    /// A keyword was renamed.
    KeywordRename,
    /// A keyword was moved.
    KeywordMove,
    /// A keyword and its branch were deleted.
    KeywordDelete,
    /// A keyword's synonyms and export flag were set (WP10 slice 2).
    KeywordProperties,
    /// A collection was made (with or without photos put in it).
    CollectionCreate,
    /// A collection was renamed.
    CollectionRename,
    /// A collection was moved.
    CollectionMove,
    /// A collection and the ones inside it were deleted.
    CollectionDelete,
    /// Photos were added to a collection.
    CollectionAdd,
    /// Photos were taken out of a collection.
    CollectionRemove,
    /// Photos were grouped into a series.
    SeriesGroup,
    /// Photos left their series, or a series was dissolved.
    SeriesUngroup,
    /// A series was resolved.
    SeriesResolve,
    /// A series was reopened.
    SeriesReopen,
    /// Changes another application made to XMP files next to originals were accepted (D-047, WP10).
    ExternalChanges,
    /// Place names were found for photos (design note 008).
    PlaceNames,
}

impl LabelKind {
    /// A stable name, for the interface.
    pub fn name(self) -> &'static str {
        match self {
            LabelKind::Rating => "rating",
            LabelKind::Flag => "flag",
            LabelKind::ColourLabel => "label",
            LabelKind::Keywords => "keywords",
            LabelKind::MetaTitle => "meta-title",
            LabelKind::MetaCaption => "meta-caption",
            LabelKind::MetaCreator => "meta-creator",
            LabelKind::MetaRights => "meta-rights",
            LabelKind::MetaUsageTerms => "meta-usage-terms",
            LabelKind::MetaWebStatement => "meta-web-statement",
            LabelKind::MetaCredit => "meta-credit",
            LabelKind::MetaSource => "meta-source",
            LabelKind::MetaHeadline => "meta-headline",
            LabelKind::MetaInstructions => "meta-instructions",
            LabelKind::MetaSublocation => "meta-sublocation",
            LabelKind::MetaCity => "meta-city",
            LabelKind::MetaRegion => "meta-region",
            LabelKind::MetaCountry => "meta-country",
            LabelKind::MetaCountryCode => "meta-country-code",
            LabelKind::MetaPersons => "meta-persons",
            LabelKind::MetaEvent => "meta-event",
            LabelKind::MetaCustom => "meta-custom",
            LabelKind::Batch => "batch",
            LabelKind::KeywordCreate => "keyword-create",
            LabelKind::KeywordRename => "keyword-rename",
            LabelKind::KeywordMove => "keyword-move",
            LabelKind::KeywordDelete => "keyword-delete",
            LabelKind::KeywordProperties => "keyword-properties",
            LabelKind::CollectionCreate => "collection-create",
            LabelKind::CollectionRename => "collection-rename",
            LabelKind::CollectionMove => "collection-move",
            LabelKind::CollectionDelete => "collection-delete",
            LabelKind::CollectionAdd => "collection-add",
            LabelKind::CollectionRemove => "collection-remove",
            LabelKind::SeriesGroup => "series-group",
            LabelKind::SeriesUngroup => "series-ungroup",
            LabelKind::SeriesResolve => "series-resolve",
            LabelKind::SeriesReopen => "series-reopen",
            LabelKind::ExternalChanges => "external-changes",
            LabelKind::PlaceNames => "place-names",
        }
    }

    /// Which label a metadata field's own change is named by.
    fn of_metadata_field(field: &MetadataField) -> Self {
        match field {
            MetadataField::Title => LabelKind::MetaTitle,
            MetadataField::Caption => LabelKind::MetaCaption,
            MetadataField::Creator => LabelKind::MetaCreator,
            MetadataField::Rights => LabelKind::MetaRights,
            MetadataField::UsageTerms => LabelKind::MetaUsageTerms,
            MetadataField::WebStatement => LabelKind::MetaWebStatement,
            MetadataField::Credit => LabelKind::MetaCredit,
            MetadataField::Source => LabelKind::MetaSource,
            MetadataField::Headline => LabelKind::MetaHeadline,
            MetadataField::Instructions => LabelKind::MetaInstructions,
            MetadataField::Sublocation => LabelKind::MetaSublocation,
            MetadataField::City => LabelKind::MetaCity,
            MetadataField::Region => LabelKind::MetaRegion,
            MetadataField::Country => LabelKind::MetaCountry,
            MetadataField::CountryCode => LabelKind::MetaCountryCode,
            MetadataField::Persons => LabelKind::MetaPersons,
            MetadataField::Event => LabelKind::MetaEvent,
            MetadataField::Custom(_) => LabelKind::MetaCustom,
        }
    }
}

/// What a step is: a kind and how many photos (or, for a change of the vocabulary, keywords) it touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Label {
    /// What kind of change.
    pub kind: LabelKind,
    /// How many photos, or keywords for a change of the vocabulary.
    pub count: usize,
}

/// One action of the person's: the changes it made, in the order it made them.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// What it was.
    pub label: Label,
    /// The changes.
    pub changes: Vec<Change>,
}

/// What a collection step says it counted: the photos it put in or took out (a creation: the ones it was made
/// with), the collections a deletion took, else 1.
fn collection_count(action: CollectionAction, deltas: &[CollectionDelta]) -> usize {
    let members = |state: &Option<Collection>| state.as_ref().map_or(0, |c| c.members.len());
    let first = deltas.first();
    match action {
        CollectionAction::Create => first.map_or(0, |d| members(&d.after)),
        CollectionAction::Add => {
            first.map_or(0, |d| members(&d.after).saturating_sub(members(&d.before)))
        }
        CollectionAction::Remove => {
            first.map_or(0, |d| members(&d.before).saturating_sub(members(&d.after)))
        }
        CollectionAction::Delete => deltas.len(),
        CollectionAction::Rename | CollectionAction::Move => 1,
    }
}

impl Entry {
    /// An entry for `changes` (not empty), labelled by what they have in common. An action that changed the
    /// vocabulary is named by that change, whatever photos it also touched (making a keyword and giving it to
    /// three photos is "keyword created"), and counts keywords.
    pub fn new(changes: Vec<Change>) -> Self {
        debug_assert!(!changes.is_empty());
        let vocabulary = changes.iter().find_map(|c| match c {
            Change::Vocabulary { keywords, .. } => Some((c.kind(), keywords.len())),
            _ => None,
        });
        // A series step is named by its series change and counts the photos the series involves.
        let series = changes.iter().find_map(|c| match c {
            Change::Series { .. } => Some(c.kind()),
            _ => None,
        });
        // A collection step is named by its collection change and counts what it did.
        let collections = changes.iter().find_map(|c| match c {
            Change::Collections { action, deltas } => {
                Some((c.kind(), collection_count(*action, deltas)))
            }
            _ => None,
        });
        let label = match vocabulary {
            Some((kind, count)) => Label { kind, count },
            None if series.is_some() => {
                // The photos of the series the step is about: the new one for a grouping, else the first one.
                let primary = if series == Some(LabelKind::SeriesGroup) {
                    changes
                        .iter()
                        .rev()
                        .find(|c| matches!(c, Change::Series { .. }))
                } else {
                    changes.iter().find(|c| matches!(c, Change::Series { .. }))
                };
                let count = match primary {
                    Some(Change::Series { before, after, .. }) => after
                        .as_ref()
                        .or(before.as_ref())
                        .map_or(0, |state| state.members.len()),
                    _ => 0,
                };
                Label {
                    kind: series.expect("checked"),
                    count,
                }
            }
            None if collections.is_some() => {
                let (kind, count) = collections.expect("checked");
                Label { kind, count }
            }
            None => {
                let kind = match changes.first().map(Change::kind) {
                    Some(first) if changes.iter().all(|c| c.kind() == first) => first,
                    _ => LabelKind::Batch,
                };
                let mut photos: Vec<PhotoId> = changes.iter().filter_map(Change::photo).collect();
                photos.sort_by_key(|p| p.to_string());
                photos.dedup();
                Label {
                    kind,
                    count: photos.len(),
                }
            }
        };
        Self { label, changes }
    }

    /// An entry named `label` whatever its changes are: for an action whose kind is not what `new` would
    /// guess from them (accepting external changes may create keywords, which `new` would call "keyword
    /// created").
    pub fn labelled(label: Label, changes: Vec<Change>) -> Self {
        debug_assert!(!changes.is_empty());
        Self { label, changes }
    }

    /// Whether the entry changed the vocabulary.
    pub fn has_vocabulary(&self) -> bool {
        self.changes
            .iter()
            .any(|c| matches!(c, Change::Vocabulary { .. }))
    }

    /// Whether the entry changed a series.
    pub fn has_series(&self) -> bool {
        self.changes
            .iter()
            .any(|c| matches!(c, Change::Series { .. }))
    }

    /// Whether the entry changed a collection.
    pub fn has_collections(&self) -> bool {
        self.changes
            .iter()
            .any(|c| matches!(c, Change::Collections { .. }))
    }

    /// Whether the entry changed a state that is not a photo's (the vocabulary, a series, a collection): undoing
    /// it does what it can for the photos that are still there rather than failing when one has gone.
    pub fn has_state_change(&self) -> bool {
        self.has_vocabulary() || self.has_series() || self.has_collections()
    }

    /// The photos the entry touches, each once, in the order of the changes.
    pub fn photos(&self) -> Vec<PhotoId> {
        let mut seen = Vec::new();
        for photo in self.changes.iter().filter_map(Change::photo) {
            if !seen.contains(&photo) {
                seen.push(photo);
            }
        }
        seen
    }
}

/// What a menu needs to know: what Undo and Redo would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HistoryState {
    /// What Undo would undo, if anything.
    pub undo: Option<Label>,
    /// What Redo would redo, if anything.
    pub redo: Option<Label>,
}

/// The two stacks.
#[derive(Debug)]
pub struct History {
    undo: VecDeque<Entry>,
    redo: Vec<Entry>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::with_limit(DEFAULT_LIMIT)
    }
}

impl History {
    /// A history that keeps at most `limit` steps.
    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// Records a new action: what was undone can no longer be redone (editing after an undo discards the
    /// undone steps, spec §5.4), and the oldest step goes when there are too many.
    pub fn record(&mut self, entry: Entry) {
        self.redo.clear();
        self.undo.push_back(entry);
        while self.undo.len() > self.limit {
            self.undo.pop_front();
        }
    }

    /// Takes the step Undo would undo.
    pub fn take_undo(&mut self) -> Option<Entry> {
        self.undo.pop_back()
    }

    /// Takes the step Redo would redo.
    pub fn take_redo(&mut self) -> Option<Entry> {
        self.redo.pop()
    }

    /// A step that was undone can be redone.
    pub fn put_redo(&mut self, entry: Entry) {
        self.redo.push(entry);
    }

    /// A step that was redone can be undone again (the redo stack is not cleared: it is not a new action).
    pub fn put_undo(&mut self, entry: Entry) {
        self.undo.push_back(entry);
        while self.undo.len() > self.limit {
            self.undo.pop_front();
        }
    }

    /// Forgets what the history holds about `photo` (it left the workspace): whether anything went. A step
    /// that is only about it goes; a step that also changed the vocabulary stays without that photo's
    /// changes, so that a deleted keyword can still be brought back for the photos that remain.
    pub fn forget_photo(&mut self, photo: PhotoId) -> bool {
        let mut forgot = false;
        for entry in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            if entry.has_state_change() {
                let before = entry.changes.len();
                entry.changes.retain(|c| c.photo() != Some(photo));
                forgot |= entry.changes.len() != before;
                // A series' snapshots forget the photo too, so that an undo does not put it back.
                for change in &mut entry.changes {
                    if let Change::Series { before, after, .. } = change {
                        for state in [before, after].into_iter().flatten() {
                            let members = state.members.len() + state.kept.len();
                            state.members.retain(|m| *m != photo);
                            state.kept.retain(|m| *m != photo);
                            forgot |= state.members.len() + state.kept.len() != members;
                        }
                    }
                    // So do a collection's, for the same reason.
                    if let Change::Collections { deltas, .. } = change {
                        for delta in deltas {
                            for state in [&mut delta.before, &mut delta.after].into_iter().flatten()
                            {
                                let members = state.members.len();
                                state.members.retain(|m| m.photo() != photo);
                                forgot |= state.members.len() != members;
                            }
                        }
                    }
                }
            }
        }
        let before = self.undo.len() + self.redo.len();
        let keep = |entry: &Entry| {
            entry.has_state_change() || entry.changes.iter().all(|c| c.photo() != Some(photo))
        };
        self.undo.retain(keep);
        self.redo.retain(keep);
        forgot || self.undo.len() + self.redo.len() != before
    }

    /// Forgets every step that changed one of these series (they were dissolved by something that is not a step:
    /// the automatic regrouping): whether anything went.
    pub fn forget_series(&mut self, series: &std::collections::HashSet<SeriesId>) -> bool {
        let before = self.undo.len() + self.redo.len();
        let touches = |entry: &Entry| {
            entry
                .changes
                .iter()
                .any(|c| matches!(c, Change::Series { id, .. } if series.contains(id)))
        };
        self.undo.retain(|e| !touches(e));
        self.redo.retain(|e| !touches(e));
        self.undo.len() + self.redo.len() != before
    }

    /// What Undo and Redo would do.
    pub fn state(&self) -> HistoryState {
        HistoryState {
            undo: self.undo.back().map(|e| e.label),
            redo: self.redo.last().map(|e| e.label),
        }
    }

    /// How many steps can be undone.
    #[cfg(test)]
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rating(photo: PhotoId, before: Option<u8>, after: Option<u8>) -> Change {
        Change::Rating {
            photo,
            before,
            after,
        }
    }

    fn one(photo: PhotoId, n: u8) -> Entry {
        Entry::new(vec![rating(photo, None, Some(n))])
    }

    #[test]
    fn a_change_goes_back_and_forward_and_an_unrated_photo_goes_back_to_unset() {
        let change = rating(PhotoId::random(), None, Some(4));
        let mut meta = Metadata::default();
        change.apply(&mut meta, Direction::Redo);
        assert_eq!(meta.rating, Some(4));
        change.apply(&mut meta, Direction::Undo);
        assert_eq!(meta.rating, None, "unset, not 0");
    }

    #[test]
    fn keywords_and_their_paths_travel_together() {
        let id = KeywordId::random();
        let change = Change::Keywords {
            photo: PhotoId::random(),
            before: KeywordSet::default(),
            after: KeywordSet {
                ids: vec![id],
                paths: vec!["Travel/Peru".into()],
            },
        };
        let mut meta = Metadata::default();
        change.apply(&mut meta, Direction::Redo);
        assert_eq!((meta.keyword_ids.len(), meta.keyword_paths.len()), (1, 1));
        change.apply(&mut meta, Direction::Undo);
        assert!(meta.keyword_ids.is_empty() && meta.keyword_paths.is_empty());
    }

    #[test]
    fn a_new_action_discards_what_was_undone() {
        let (a, b) = (PhotoId::random(), PhotoId::random());
        let mut history = History::default();
        history.record(one(a, 1));
        let undone = history.take_undo().unwrap();
        history.put_redo(undone);
        assert!(history.state().redo.is_some());
        history.record(one(b, 2));
        assert_eq!(history.state().redo, None, "editing after an undo");
    }

    #[test]
    fn undo_and_redo_walk_the_stacks_in_order_without_clearing_redo() {
        let photo = PhotoId::random();
        let mut history = History::default();
        history.record(one(photo, 1));
        history.record(one(photo, 2));
        let second = history.take_undo().unwrap();
        history.put_redo(second.clone());
        let first = history.take_undo().unwrap();
        history.put_redo(first);
        assert_eq!(history.state().undo, None);
        let again = history.take_redo().unwrap();
        history.put_undo(again);
        assert!(
            history.state().redo.is_some(),
            "the second step can still be redone"
        );
        assert!(history.state().undo.is_some());
    }

    #[test]
    fn the_oldest_step_goes_when_there_are_too_many() {
        let photo = PhotoId::random();
        let mut history = History::with_limit(3);
        for n in 0..5 {
            history.record(one(photo, n));
        }
        assert_eq!(history.undo_depth(), 3);
        let newest = history.take_undo().unwrap();
        assert_eq!(newest.changes, vec![rating(photo, None, Some(4))]);
    }

    #[test]
    fn an_entry_is_labelled_by_what_its_changes_have_in_common() {
        let (a, b) = (PhotoId::random(), PhotoId::random());
        let ratings = Entry::new(vec![rating(a, None, Some(1)), rating(b, None, Some(1))]);
        assert_eq!(
            ratings.label,
            Label {
                kind: LabelKind::Rating,
                count: 2
            }
        );
        let same_photo_twice =
            Entry::new(vec![rating(a, None, Some(1)), rating(a, Some(1), Some(2))]);
        assert_eq!(same_photo_twice.label.count, 1, "photos, not changes");
        let mixed = Entry::new(vec![
            rating(a, None, Some(1)),
            Change::Flag {
                photo: b,
                before: None,
                after: Some(Flag::Picked),
            },
        ]);
        assert_eq!(mixed.label.kind, LabelKind::Batch);
        assert_eq!(mixed.photos(), vec![a, b]);
    }

    #[test]
    fn steps_about_a_photo_that_left_are_forgotten() {
        let (a, b) = (PhotoId::random(), PhotoId::random());
        let mut history = History::default();
        history.record(one(a, 1));
        history.record(one(b, 2));
        assert!(history.forget_photo(a));
        assert!(!history.forget_photo(a), "nothing left to forget");
        assert_eq!(history.undo_depth(), 1);
    }

    fn vocabulary_change(action: VocabularyAction, n: usize) -> Change {
        let keywords = (0..n)
            .map(|_| KeywordDelta {
                id: KeywordId::random(),
                before: None,
                after: None,
            })
            .collect();
        Change::Vocabulary { action, keywords }
    }

    #[test]
    fn a_step_that_changed_the_vocabulary_is_named_by_it_and_counts_keywords() {
        let photo = PhotoId::random();
        let deleted = Entry::new(vec![
            Change::Keywords {
                photo,
                before: KeywordSet::default(),
                after: KeywordSet::default(),
            },
            vocabulary_change(VocabularyAction::Delete, 3),
        ]);
        assert_eq!(
            deleted.label,
            Label {
                kind: LabelKind::KeywordDelete,
                count: 3
            }
        );
        assert!(deleted.has_vocabulary());
        assert_eq!(deleted.photos(), vec![photo]);
        let created = Entry::new(vec![
            vocabulary_change(VocabularyAction::Create, 1),
            rating(photo, None, Some(1)),
        ]);
        assert_eq!(created.label.kind, LabelKind::KeywordCreate);
        let mut meta = Metadata::default();
        vocabulary_change(VocabularyAction::Move, 1).apply(&mut meta, Direction::Redo);
        assert_eq!(meta, Metadata::default(), "a photo is not what it changes");
    }

    #[test]
    fn a_photo_that_left_takes_its_own_steps_but_only_its_share_of_a_vocabulary_step() {
        let (a, b) = (PhotoId::random(), PhotoId::random());
        let mut history = History::default();
        history.record(one(a, 1));
        history.record(Entry::new(vec![
            Change::Keywords {
                photo: a,
                before: KeywordSet::default(),
                after: KeywordSet::default(),
            },
            Change::Keywords {
                photo: b,
                before: KeywordSet::default(),
                after: KeywordSet::default(),
            },
            vocabulary_change(VocabularyAction::Delete, 1),
        ]));
        assert!(history.forget_photo(a));
        assert_eq!(
            history.undo_depth(),
            1,
            "the rating of `a` went, the delete stayed"
        );
        let delete = history.take_undo().unwrap();
        assert_eq!(delete.photos(), vec![b], "without the photo that left");
        assert!(delete.has_vocabulary());
    }
}
