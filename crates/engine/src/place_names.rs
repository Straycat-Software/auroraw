// SPDX-License-Identifier: GPL-3.0-or-later
//! Finding the place names of photos (WP10, design note 008, D-147): what is written, and the background
//! worker that looks the positions up.
//!
//! The rule of what is written is [`fill_place`], a pure function of a photo's metadata and what was found
//! for its position: **only a field that is empty is filled**; a field Auroraw filled earlier (the record
//! `aur:PlaceFilled` lists it, and it still says what was written) follows the position when a refresh is
//! asked for; a field a person wrote has left the record and is never touched; and **a field a person
//! emptied is an answer, not a gap**: the record keeps it empty and the run leaves it so. The worker looks
//! each photo up on its own thread, and sends what it found to the coordinator as an edit
//! ([`crate::Command::FillPlace`]) through the same paced channel as a large batch, so that the whole run
//! is one undoable step and a cancellation lands within about one photo.
//!
//! The same worker also **previews** a run ([`crate::Command::PreviewPlaceNames`]): the same lookups and
//! the same rule, on the same reads, but nothing is sent to be written. What it would change comes back
//! grouped (`City: Westville → Eastburg, 400 photos`), which is what a person can read when a refresh
//! would replace hundreds of names.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use auroraw_format::sidecar::{Metadata, PlaceField, PlaceFilled};
use auroraw_places::Places;
use auroraw_types::{PhotoId, SourceId};
use auroraw_workspace::Workspace;

use crate::command::{Command, MetadataField};
use crate::coordinator::Inbound;
use crate::event::Event;
use crate::history::{Change, PlaceState};
use crate::job::{CancelToken, JobId};

/// Which photos the place names are found for.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaceScope {
    /// These photos (the selection).
    Photos(Vec<PhotoId>),
    /// Every photo whose original is in this source.
    Source(SourceId),
}

/// What was found for a position, ready to be written (the names are in the language asked for).
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceFill {
    /// The latitude the names were found for, in decimal degrees as text.
    pub latitude: String,
    /// The longitude, likewise.
    pub longitude: String,
    /// The city, if a town was near.
    pub city: Option<String>,
    /// The region, if the position is in one.
    pub region: Option<String>,
    /// The country, if the position is in one.
    pub country: Option<String>,
    /// The ISO country code.
    pub country_code: Option<String>,
    /// Whether the fields Auroraw filled earlier follow this position (a refresh), or are left as they
    /// are (a first fill, which only fills what is empty).
    pub refresh: bool,
}

/// What a run of finding place names did, for the report at its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlaceNamesReport {
    /// The photos looked at.
    pub photos: usize,
    /// Photos that got at least one field.
    pub filled: usize,
    /// Photos with nothing to write: every field is there already, or is a person's, or a person emptied it
    /// (including a photo another edit filled between the lookup and the write).
    pub had_place: usize,
    /// Photos with no position (or one that cannot be read).
    pub no_position: usize,
    /// Photos whose position is in no country: open water, a pole.
    pub open_water: usize,
    /// Photos that could not be read, or whose names could not be written (they left the workspace
    /// meanwhile, or the folder cannot be written).
    pub failed: usize,
}

/// How many groups a preview carries at most (the biggest first); the rest are counted in
/// [`PlacePreview::groups_total`].
pub const PREVIEW_GROUPS: usize = 200;

/// How many photos of a group a preview names, to show a few of what it is about.
pub const PREVIEW_EXAMPLES: usize = 3;

/// One change a run would make, and how many photos it is made on: a field, what it says, what it would say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceChangeGroup {
    /// The field.
    pub field: PlaceField,
    /// What it says now (`None`: empty).
    pub before: Option<String>,
    /// What it would say (`None`: empty).
    pub after: Option<String>,
    /// How many photos.
    pub photos: usize,
    /// The first of them, in the order the photos were looked at (at most [`PREVIEW_EXAMPLES`]).
    pub examples: Vec<PhotoId>,
}

/// What a run **would** do, from [`crate::Command::PreviewPlaceNames`]: the counts a real run would report, and
/// the changes it would make, grouped.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacePreview {
    /// The report of the run it previews (`filled` is the photos that would get names).
    pub report: PlaceNamesReport,
    /// The changes, the biggest first (by photos, then by field and text), at most [`PREVIEW_GROUPS`].
    pub groups: Vec<PlaceChangeGroup>,
    /// How many different changes there are in all, which can be more than `groups` holds.
    pub groups_total: usize,
}

/// What a group is keyed by: the field's position in [`PlaceField::ALL`], the old text and the new one.
type GroupKey = (usize, Option<String>, Option<String>);

/// The changes of a run so far, grouped by field, old text and new text: for each, how many photos and the
/// first few.
#[derive(Default)]
struct Groups(BTreeMap<GroupKey, (usize, Vec<PhotoId>)>);

impl Groups {
    fn add(&mut self, photo: PhotoId, before: &PlaceState, after: &PlaceState) {
        for (index, field) in PlaceField::ALL.into_iter().enumerate() {
            let (was, becomes) = (before.get(field), after.get(field));
            if was != becomes {
                let entry = self
                    .0
                    .entry((index, was.map(str::to_string), becomes.map(str::to_string)))
                    .or_default();
                entry.0 += 1;
                if entry.1.len() < PREVIEW_EXAMPLES {
                    entry.1.push(photo);
                }
            }
        }
    }

    fn finish(self, report: PlaceNamesReport) -> PlacePreview {
        let mut groups: Vec<PlaceChangeGroup> = self
            .0
            .into_iter()
            .map(
                |((index, before, after), (photos, examples))| PlaceChangeGroup {
                    field: PlaceField::ALL[index],
                    before,
                    after,
                    photos,
                    examples,
                },
            )
            .collect();
        // (A stable sort over a map in key order: ties come by field, then by text.)
        groups.sort_by_key(|group| std::cmp::Reverse(group.photos));
        let groups_total = groups.len();
        groups.truncate(PREVIEW_GROUPS);
        PlacePreview {
            report,
            groups,
            groups_total,
        }
    }
}

fn found(fill: &PlaceFill, field: PlaceField) -> Option<&String> {
    match field {
        PlaceField::City => fill.city.as_ref(),
        PlaceField::Region => fill.region.as_ref(),
        PlaceField::Country => fill.country.as_ref(),
        PlaceField::CountryCode => fill.country_code.as_ref(),
    }
}

/// Writes what was found into `meta` by the rule above, and says what changed (`None` when nothing did).
pub(crate) fn fill_place(meta: &mut Metadata, photo: PhotoId, fill: &PlaceFill) -> Option<Change> {
    let before = PlaceState::of(meta);
    let mut record = meta.place_filled.clone().unwrap_or_default();
    let mut written = false;
    for field in PlaceField::ALL {
        let current = meta.place_field(field).map(str::to_string);
        let ours = current.is_some() && record.get(field) == current.as_deref();
        match current {
            // Emptied by a person: their answer, kept empty (a refresh included) until they write it.
            None if record.is_cleared(field) => {}
            // Empty: the position tells.
            None => {
                if let Some(value) = found(fill, field) {
                    meta.set_place_field(field, Some(value.clone()));
                    record.set(field, Some(value.clone()));
                    written = true;
                } else {
                    record.set(field, None);
                }
            }
            // Auroraw's, and the position moved: follows it (and goes when the new place has none).
            Some(_) if ours && fill.refresh => {
                let new = found(fill, field).cloned();
                if new != current {
                    meta.set_place_field(field, new.clone());
                    written = true;
                }
                record.set(field, new);
            }
            // Auroraw's, no refresh asked: left as it is.
            Some(_) if ours => {}
            // A person's: not in the record, whatever it said before (and no answer of emptiness stands
            // over a text that is there).
            Some(_) => record.set(field, None),
        }
    }
    if record.is_empty() && record.extra.is_empty() {
        meta.place_filled = None;
    } else {
        if written || meta.place_filled.is_none() {
            record.latitude = fill.latitude.clone();
            record.longitude = fill.longitude.clone();
        }
        meta.place_filled = Some(record);
    }
    let after = PlaceState::of(meta);
    (before != after).then_some(Change::Place {
        photo,
        before,
        after,
    })
}

/// What looking a position up came to.
enum Lookup {
    /// Names to write.
    Found(PlaceFill),
    /// The position is in no country: open water, a pole.
    Nowhere,
    /// The file could not be read.
    Failed,
}

fn look_up(places: &Places, lat: f64, lon: f64, language: &str, refresh: bool) -> Lookup {
    let Ok(located) = places.locate(lat, lon, language) else {
        return Lookup::Failed;
    };
    if located.is_empty() {
        return Lookup::Nowhere;
    }
    Lookup::Found(PlaceFill {
        latitude: format!("{lat:.5}"),
        longitude: format!("{lon:.5}"),
        city: located.city.map(|town| town.name),
        region: located.region.map(|region| region.name),
        country_code: located
            .country
            .as_ref()
            .map(|country| country.code.clone())
            .filter(|code| !code.is_empty()),
        country: located.country.map(|country| country.name),
        refresh,
    })
}

/// A person wrote `field` of `meta` (or another application did, and it was accepted): a place field
/// leaves the record of what Auroraw filled (design note 008 §4), because it is not Auroraw's any more.
/// `was` is the record before the write; `before` and `after` are the field's text before and after it.
///
/// **A write that empties a field that had a text is an answer**, not a gap to refill: the record keeps it
/// as an emptied field. Writing nothing over a field that was empty already says nothing, and leaves
/// the record as it is (an answer already given stays). Returns how the record changed, for the history
/// to undo.
pub(crate) fn released(
    meta: &mut Metadata,
    field: &MetadataField,
    was: Option<PlaceFilled>,
    before: &str,
    after: &str,
) -> Option<(Option<PlaceFilled>, Option<PlaceFilled>)> {
    if let Some(place) = field.place_field() {
        if !after.is_empty() {
            meta.release_place_field(place);
        } else if !before.is_empty() {
            meta.release_place_field(place);
            meta.decline_place_field(place);
        }
    }
    (meta.place_filled != was).then(|| (was, meta.place_filled.clone()))
}

/// What the worker needs.
pub(crate) struct PlaceJob {
    pub job: JobId,
    pub workspace: Arc<Workspace>,
    pub photos: Vec<PhotoId>,
    pub pack: PathBuf,
    pub language: String,
    pub refresh: bool,
    /// Preview what the run would do instead of doing it.
    pub preview: bool,
    pub events: mpsc::Sender<Event>,
    pub inbound: mpsc::Sender<Inbound>,
    pub cancel: CancelToken,
}

pub(crate) fn spawn(job: PlaceJob) {
    std::thread::spawn(move || run(job));
}

/// Looks one photo up. `Some` is something to write (what was found, and the change it makes); `None` is
/// counted in `report`.
fn examine(
    places: &Option<Places>,
    workspace: &Workspace,
    photo_id: PhotoId,
    language: &str,
    refresh: bool,
    report: &mut PlaceNamesReport,
) -> Option<(PlaceFill, Change)> {
    let Some(places) = places else {
        report.failed += 1;
        return None;
    };
    let Some(photo) = workspace
        .read_photo(&photo_id)
        .ok()
        .flatten()
        .and_then(|loaded| loaded.current())
    else {
        report.failed += 1;
        return None;
    };
    let Some((lat, lon)) = photo.meta.position() else {
        report.no_position += 1;
        return None;
    };
    match look_up(places, lat, lon, language, refresh) {
        Lookup::Failed => {
            report.failed += 1;
            None
        }
        Lookup::Nowhere => {
            report.open_water += 1;
            None
        }
        Lookup::Found(fill) => {
            // The rule the coordinator will apply, run on this read to know whether there is anything to write.
            let mut meta = photo.meta;
            if let Some(change) = fill_place(&mut meta, photo_id, &fill) {
                report.filled += 1;
                Some((fill, change))
            } else {
                report.had_place += 1;
                None
            }
        }
    }
}

fn run(job: PlaceJob) {
    let PlaceJob {
        job: id,
        workspace,
        photos,
        pack,
        language,
        refresh,
        preview,
        events,
        inbound,
        cancel,
    } = job;
    let total = photos.len();
    let mut report = PlaceNamesReport::default();
    let mut groups = Groups::default();
    // (The coordinator opened the file before it started this, so a failure here is a race with the file
    // being replaced: every photo is then counted as failed.)
    let places = Places::open(&pack).ok();
    let end = |report, groups: Groups, cancelled| {
        // Through `inbound`, so that it is not seen before the coordinator applied every item; a real run
        // has the coordinator record its step before it reports, and a preview has no step.
        let _ = inbound.send(if preview {
            Inbound::PlaceNamesPreviewDone {
                job: id,
                preview: groups.finish(report),
                cancelled,
            }
        } else {
            Inbound::PlaceNamesDone {
                job: id,
                report,
                cancelled,
            }
        });
    };
    for (done, photo_id) in photos.into_iter().enumerate() {
        if cancel.is_cancelled() {
            end(report, groups, true);
            return;
        }
        report.photos += 1;
        if let Some((fill, change)) = examine(
            &places,
            &workspace,
            photo_id,
            &language,
            refresh,
            &mut report,
        ) {
            if preview {
                if let Change::Place { before, after, .. } = &change {
                    groups.add(photo_id, before, after);
                }
            } else {
                let (ack, acked) = mpsc::channel();
                let edit = Command::FillPlace { photo_id, fill };
                if inbound
                    .send(Inbound::BatchItem { job: id, edit, ack })
                    .is_err()
                {
                    return;
                }
                // Waits for the coordinator to have applied it: what paces this worker, so that a cancellation
                // is never stuck behind a whole run (`batch_job`'s own doc comment).
                let _ = acked.recv();
            }
        }
        let _ = events.send(Event::JobProgress {
            job: id,
            done: done + 1,
            total,
        });
    }
    end(report, groups, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(city: Option<&str>) -> PlaceState {
        PlaceState {
            city: city.map(str::to_string),
            ..PlaceState::default()
        }
    }

    #[test]
    fn a_preview_keeps_the_biggest_groups_and_says_how_many_there_are() {
        let mut groups = Groups::default();
        // 250 different new names, one photo each, and one name given to twenty photos.
        for i in 0..250 {
            groups.add(
                PhotoId::random(),
                &state(None),
                &state(Some(&format!("Town {i:03}"))),
            );
        }
        for _ in 0..20 {
            groups.add(PhotoId::random(), &state(None), &state(Some("Big Town")));
        }
        let preview = groups.finish(PlaceNamesReport::default());
        assert_eq!(preview.groups_total, 251);
        assert_eq!(preview.groups.len(), PREVIEW_GROUPS);
        assert_eq!(preview.groups[0].after.as_deref(), Some("Big Town"));
        assert_eq!(preview.groups[0].photos, 20);
        assert_eq!(
            preview.groups[0].examples.len(),
            PREVIEW_EXAMPLES,
            "a few of the twenty, not all"
        );
        // Ties come in the order of the text, so the same run shows the same groups.
        assert_eq!(preview.groups[1].after.as_deref(), Some("Town 000"));
    }

    #[test]
    fn only_the_fields_that_change_make_a_group() {
        let mut groups = Groups::default();
        let before = PlaceState {
            city: Some("A".into()),
            country: Some("X".into()),
            ..PlaceState::default()
        };
        let after = PlaceState {
            city: Some("B".into()),
            country: Some("X".into()),
            ..PlaceState::default()
        };
        groups.add(PhotoId::random(), &before, &after);
        let preview = groups.finish(PlaceNamesReport::default());
        assert_eq!(preview.groups.len(), 1);
        assert_eq!(preview.groups[0].field, PlaceField::City);
        assert_eq!(preview.groups[0].before.as_deref(), Some("A"));
    }
}
