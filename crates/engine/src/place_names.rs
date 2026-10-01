// SPDX-License-Identifier: GPL-3.0-or-later
//! Finding the place names of photos (WP10, design note 008, D-147): what is written, and the background
//! worker that looks the positions up.
//!
//! The rule of what is written is [`fill_place`], a pure function of a photo's metadata and what was found
//! for its position: **only a field that is empty is filled**, and a field Auroraw filled earlier (the
//! record `aur:PlaceFilled` lists it, and it still says what was written) follows the position when a
//! refresh is asked for; a field a person wrote has left the record, and is never touched. The worker looks
//! each photo up on its own thread, and sends what it found to the coordinator as an edit
//! ([`crate::Command::FillPlace`]) through the same paced channel as a large batch, so that the whole run
//! is one undoable step and a cancellation lands within about one photo.

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
    /// Photos that already had every field, or whose fields are a person's: nothing to write.
    pub had_place: usize,
    /// Photos with no position (or one that cannot be read).
    pub no_position: usize,
    /// Photos whose position is in no country: open water, a pole.
    pub open_water: usize,
    /// Photos that could not be read, or whose names could not be written (they left the workspace
    /// meanwhile, or the folder cannot be written).
    pub failed: usize,
}

const FIELDS: [PlaceField; 4] = [
    PlaceField::City,
    PlaceField::Region,
    PlaceField::Country,
    PlaceField::CountryCode,
];

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
    for field in FIELDS {
        let current = meta.place_field(field).map(str::to_string);
        let ours = current.is_some() && record.get(field) == current.as_deref();
        match current {
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
            // A person's: not in the record, whatever it said before.
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
/// `was` is the record before the write; returns how the record changed, for the history to undo.
pub(crate) fn released(
    meta: &mut Metadata,
    field: &MetadataField,
    was: Option<PlaceFilled>,
) -> Option<(Option<PlaceFilled>, Option<PlaceFilled>)> {
    if let Some(place) = field.place_field() {
        meta.release_place_field(place);
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
    pub events: mpsc::Sender<Event>,
    pub inbound: mpsc::Sender<Inbound>,
    pub cancel: CancelToken,
}

pub(crate) fn spawn(job: PlaceJob) {
    std::thread::spawn(move || run(job));
}

/// Looks one photo up. `Ok(Some(edit))` is something to write; `Ok(None)` is counted in `report`.
fn examine(
    places: &Option<Places>,
    workspace: &Workspace,
    photo_id: PhotoId,
    language: &str,
    refresh: bool,
    report: &mut PlaceNamesReport,
) -> Option<Command> {
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
            if fill_place(&mut meta, photo_id, &fill).is_some() {
                report.filled += 1;
                Some(Command::FillPlace { photo_id, fill })
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
        events,
        inbound,
        cancel,
    } = job;
    let total = photos.len();
    let mut report = PlaceNamesReport::default();
    // (The coordinator opened the file before it started this, so a failure here is a race with the file
    // being replaced: every photo is then counted as failed.)
    let places = Places::open(&pack).ok();
    for (done, photo_id) in photos.into_iter().enumerate() {
        if cancel.is_cancelled() {
            // Through `inbound`, so that it is not seen before the coordinator applied every item; the
            // coordinator records the step before it reports.
            let _ = inbound.send(Inbound::PlaceNamesDone {
                job: id,
                report,
                cancelled: true,
            });
            return;
        }
        report.photos += 1;
        if let Some(edit) = examine(
            &places,
            &workspace,
            photo_id,
            &language,
            refresh,
            &mut report,
        ) {
            let (ack, acked) = mpsc::channel();
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
        let _ = events.send(Event::JobProgress {
            job: id,
            done: done + 1,
            total,
        });
    }
    let _ = inbound.send(Inbound::PlaceNamesDone {
        job: id,
        report,
        cancelled: false,
    });
}
