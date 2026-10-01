// SPDX-License-Identifier: GPL-3.0-or-later
//! Finding the place names (WP10, design note 008, D-147): what is written and what is left alone, the
//! record of what Auroraw wrote and how a person's write takes a field out of it, the refresh, and the
//! run as one undoable, cancellable step.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use auroraw_catalogue::Catalogue;
use auroraw_engine::{
    Command, Engine, EngineError, Event, EventReceiver, JobId, LabelKind, MetadataField, Outcome,
    PlaceNamesReport, PlaceScope,
};
use auroraw_format::sidecar::{Metadata, Overlay, OverlayGps, PhotoSidecar};
use auroraw_places::{Level, NewArea, NewPlace, PackBuilder};
use auroraw_testkit::{TempDir, temp_dir};
use auroraw_types::PhotoId;
use auroraw_workspace::Workspace;

struct Fixture {
    dir: TempDir,
    engine: Engine,
    events: EventReceiver,
    pack: PathBuf,
    /// What Undo would undo, as the last `HistoryChanged` said.
    undo: std::cell::RefCell<Option<(LabelKind, usize)>>,
}

type Ring = Vec<(f64, f64)>;

fn square(x0: f64, x1: f64, y0: f64, y1: f64) -> Ring {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)]
}

fn area(level: Level, code: &str, en: &str, fr: &str, parts: Vec<Ring>) -> NewArea {
    NewArea {
        level,
        code: code.into(),
        country_code: code.split('-').next().unwrap().into(),
        parent: None,
        name: en.into(),
        names: vec![
            ("en".to_string(), en.to_string()),
            ("fr".to_string(), fr.to_string()),
        ],
        geonames_id: None,
        parts: vec![parts],
    }
}

fn town(id: i64, name: &str, lat: f64, lon: f64) -> NewPlace {
    NewPlace {
        id,
        name: name.into(),
        lat,
        lon,
        country_code: "AA".into(),
        population: 5_000,
        section: false,
    }
}

/// Aland (0..10 both ways) in two regions cut along longitude 5, a town in each.
fn places_file(path: &Path) {
    let mut pack = PackBuilder::create(path).unwrap();
    let aland = pack
        .add_area(&area(
            Level::Country,
            "AA",
            "Aland",
            "Alandie",
            vec![square(0.0, 10.0, 0.0, 10.0)],
        ))
        .unwrap();
    let mut west = area(
        Level::Region,
        "AA-W",
        "West",
        "Ouest",
        vec![square(0.0, 5.0, 0.0, 10.0)],
    );
    west.parent = Some(aland);
    pack.add_area(&west).unwrap();
    let mut east = area(
        Level::Region,
        "AA-E",
        "East",
        "Est",
        vec![square(5.0, 10.0, 0.0, 10.0)],
    );
    east.parent = Some(aland);
    pack.add_area(&east).unwrap();
    pack.add_place(&town(1, "Westville", 5.0, 2.0)).unwrap();
    pack.add_place(&town(2, "Eastburg", 5.0, 8.0)).unwrap();
    pack.finish().unwrap();
}

/// Decimal degrees in XMP's text form.
fn xmp(value: f64, positive: char, negative: char) -> String {
    let magnitude = value.abs();
    let hemisphere = if value < 0.0 { negative } else { positive };
    format!(
        "{},{:.4}{hemisphere}",
        magnitude.trunc(),
        magnitude.fract() * 60.0
    )
}

fn photo_at(lat: f64, lon: f64) -> PhotoSidecar {
    let mut photo = PhotoSidecar::new(PhotoId::random());
    photo.meta.original.gps_latitude = Some(xmp(lat, 'N', 'S'));
    photo.meta.original.gps_longitude = Some(xmp(lon, 'E', 'W'));
    photo
}

/// A workspace holding these photos, and an engine on it.
fn fixture(photos: &[PhotoSidecar]) -> Fixture {
    let dir = temp_dir();
    let root = dir.path().join("W");
    let catalogue = dir.path().join("catalogue.sqlite");
    let ws = Workspace::create(&root, "Main").unwrap();
    for photo in photos {
        ws.write_photo(photo).unwrap();
    }
    let workspace_id = ws.workspace_id();
    drop(ws);
    drop(Catalogue::create(&catalogue, workspace_id).unwrap());
    let (engine, events) = Engine::open(&root, &catalogue).unwrap();
    engine.submit_and_wait(Command::Rebuild).unwrap();
    events.drain();
    let pack = dir.path().join("places.sqlite");
    places_file(&pack);
    Fixture {
        dir,
        engine,
        events,
        pack,
        undo: std::cell::RefCell::new(None),
    }
}

struct Found {
    report: PlaceNamesReport,
    cancelled: bool,
}

impl Fixture {
    fn meta(&self, photo: PhotoId) -> Metadata {
        self.engine
            .workspace()
            .read_photo(&photo)
            .unwrap()
            .unwrap()
            .current()
            .unwrap()
            .meta
    }

    fn start(&self, scope: PlaceScope, language: &str, refresh: bool) -> JobId {
        let Outcome::PlaceNamesStarted { job, .. } = self
            .engine
            .submit_and_wait(Command::FindPlaceNames {
                scope,
                pack: self.pack.clone(),
                language: language.into(),
                refresh,
            })
            .unwrap()
        else {
            panic!("expected PlaceNamesStarted");
        };
        job
    }

    /// Waits for the run's report and for its end (the step is in the history by then).
    fn wait(&self, job: JobId) -> Found {
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut found = None;
        loop {
            match self.events.recv_timeout(Duration::from_millis(100)) {
                Some(Event::PlaceNamesFound {
                    job: j,
                    report,
                    cancelled,
                }) if j == job => found = Some(Found { report, cancelled }),
                Some(Event::JobFinished(j) | Event::JobCancelled(j)) if j == job => {
                    return found.expect("the report comes before the end");
                }
                Some(event) => self.note(&event),
                None => assert!(Instant::now() < deadline, "the run never ended"),
            }
        }
    }

    fn find(&self, photos: &[PhotoId], language: &str, refresh: bool) -> Found {
        let job = self.start(PlaceScope::Photos(photos.to_vec()), language, refresh);
        self.wait(job)
    }

    fn note(&self, event: &Event) {
        if let Event::HistoryChanged(state) = event {
            *self.undo.borrow_mut() = state.undo.as_ref().map(|l| (l.kind, l.count));
        }
    }

    fn undo_label(&self) -> Option<(LabelKind, usize)> {
        for event in self.events.drain() {
            self.note(&event);
        }
        *self.undo.borrow()
    }

    fn set(&self, photo: PhotoId, field: MetadataField, value: &str) {
        self.engine
            .submit_and_wait(Command::SetMetadataField {
                photo_id: photo,
                field,
                value: value.into(),
            })
            .unwrap();
    }
}

fn places(meta: &Metadata) -> [Option<&str>; 4] {
    [
        meta.city.as_deref(),
        meta.region.as_deref(),
        meta.country.as_deref(),
        meta.country_code.as_deref(),
    ]
}

/// The positions of the photos; the sidecar of a position moved by the photographer, in `Overlay`.
fn moved(f: &Fixture, photo: PhotoId, lat: f64, lon: f64) {
    let mut sidecar = f
        .engine
        .workspace()
        .read_photo(&photo)
        .unwrap()
        .unwrap()
        .current()
        .unwrap();
    sidecar.meta.overlay = Some(Overlay {
        gps: Some(OverlayGps {
            latitude: xmp(lat, 'N', 'S'),
            longitude: xmp(lon, 'E', 'W'),
            altitude: None,
            extra: Vec::new(),
        }),
        ..Overlay::default()
    });
    f.engine.workspace().write_photo(&sidecar).unwrap();
}

#[test]
fn the_names_of_where_photos_were_taken_are_written_in_the_asked_language_as_one_step() {
    let (west, east) = (photo_at(5.0, 2.0), photo_at(5.0, 8.0));
    let (no_gps, sea) = (PhotoSidecar::new(PhotoId::random()), photo_at(-40.0, 0.0));
    let ids = [west.photo_id, east.photo_id, no_gps.photo_id, sea.photo_id];
    let f = fixture(&[west, east, no_gps, sea]);

    let found = f.find(&ids, "fr", false);
    assert!(!found.cancelled);
    assert_eq!(
        found.report,
        PlaceNamesReport {
            photos: 4,
            filled: 2,
            had_place: 0,
            no_position: 1,
            open_water: 1,
            failed: 0,
        }
    );
    let w = f.meta(ids[0]);
    assert_eq!(
        places(&w),
        [
            Some("Westville"),
            Some("Ouest"),
            Some("Alandie"),
            Some("AA")
        ]
    );
    let record = w
        .place_filled
        .clone()
        .expect("what was written is recorded");
    assert_eq!(record.city.as_deref(), Some("Westville"));
    assert_eq!(record.country_code.as_deref(), Some("AA"));
    assert!(record.latitude.starts_with("5.0"), "{}", record.latitude);
    let e = f.meta(ids[1]);
    assert_eq!(places(&e)[..2], [Some("Eastburg"), Some("Est")]);
    assert_eq!(places(&f.meta(ids[2])), [None; 4], "no position: nothing");
    assert_eq!(places(&f.meta(ids[3])), [None; 4], "open water: nothing");
    assert!(f.meta(ids[3]).place_filled.is_none());

    assert_eq!(f.undo_label(), Some((LabelKind::PlaceNames, 2)));
    f.engine.undo().unwrap();
    for id in ids {
        let meta = f.meta(id);
        assert_eq!(places(&meta), [None; 4]);
        assert!(
            meta.place_filled.is_none(),
            "the record goes with the names"
        );
    }
    f.engine.redo().unwrap();
    assert_eq!(f.meta(ids[0]), w);
    assert_eq!(f.meta(ids[1]), e);
}

#[test]
fn the_names_come_in_english_when_asked_for_it() {
    let photo = photo_at(5.0, 2.0);
    let id = photo.photo_id;
    let f = fixture(&[photo]);
    f.find(&[id], "en", false);
    assert_eq!(
        places(&f.meta(id)),
        [Some("Westville"), Some("West"), Some("Aland"), Some("AA")]
    );
}

#[test]
fn only_what_is_empty_is_filled_and_what_a_person_wrote_is_never_touched() {
    let mut partly = photo_at(5.0, 2.0);
    partly.meta.city = Some("Home".into());
    let mut fully = photo_at(5.0, 8.0);
    fully.meta.city = Some("A".into());
    fully.meta.region = Some("B".into());
    fully.meta.country = Some("C".into());
    fully.meta.country_code = Some("D".into());
    let ids = [partly.photo_id, fully.photo_id];
    let f = fixture(&[partly, fully]);

    let found = f.find(&ids, "fr", false);
    assert_eq!((found.report.filled, found.report.had_place), (1, 1));
    let partly = f.meta(ids[0]);
    assert_eq!(
        places(&partly),
        [Some("Home"), Some("Ouest"), Some("Alandie"), Some("AA")]
    );
    let record = partly.place_filled.unwrap();
    assert_eq!(
        record.city, None,
        "the city is the person's, not in the record"
    );
    assert_eq!(record.region.as_deref(), Some("Ouest"));
    let fully = f.meta(ids[1]);
    assert_eq!(places(&fully), [Some("A"), Some("B"), Some("C"), Some("D")]);
    assert!(fully.place_filled.is_none());
    assert_eq!(f.undo_label(), Some((LabelKind::PlaceNames, 1)));
}

#[test]
fn a_second_run_finds_nothing_to_do_and_adds_no_step() {
    let photo = photo_at(5.0, 2.0);
    let id = photo.photo_id;
    let f = fixture(&[photo]);
    f.find(&[id], "fr", false);
    let before = f.meta(id);
    let again = f.find(&[id], "fr", false);
    assert_eq!((again.report.filled, again.report.had_place), (0, 1));
    assert_eq!(f.meta(id), before);
    f.engine.undo().unwrap();
    assert_eq!(places(&f.meta(id)), [None; 4], "one step to undo, not two");
}

#[test]
fn a_field_a_person_writes_leaves_the_record_and_undoing_the_write_brings_it_back() {
    let photo = photo_at(5.0, 2.0);
    let id = photo.photo_id;
    let f = fixture(&[photo]);
    f.find(&[id], "fr", false);

    f.set(id, MetadataField::City, "Chez moi");
    let meta = f.meta(id);
    assert_eq!(meta.city.as_deref(), Some("Chez moi"));
    let record = meta.place_filled.unwrap();
    assert_eq!(record.city, None);
    assert_eq!(record.region.as_deref(), Some("Ouest"));

    f.engine.undo().unwrap();
    let meta = f.meta(id);
    assert_eq!(meta.city.as_deref(), Some("Westville"));
    assert_eq!(
        meta.place_filled.unwrap().city.as_deref(),
        Some("Westville")
    );
    f.engine.redo().unwrap();
    assert_eq!(f.meta(id).place_filled.unwrap().city, None);
}

#[test]
fn clearing_a_field_and_writing_it_in_a_batch_release_it_the_same_way() {
    let photo = photo_at(5.0, 2.0);
    let id = photo.photo_id;
    let f = fixture(&[photo]);
    f.find(&[id], "fr", false);

    f.engine
        .submit_and_wait(Command::Batch {
            commands: vec![
                Command::SetMetadataField {
                    photo_id: id,
                    field: MetadataField::Region,
                    value: "Mon coin".into(),
                },
                Command::SetMetadataField {
                    photo_id: id,
                    field: MetadataField::Country,
                    value: String::new(),
                },
            ],
        })
        .unwrap();
    let meta = f.meta(id);
    assert_eq!(meta.region.as_deref(), Some("Mon coin"));
    assert_eq!(meta.country, None);
    let record = meta.place_filled.unwrap();
    assert_eq!((record.region, record.country), (None, None));
    assert_eq!(record.city.as_deref(), Some("Westville"));
    assert_eq!(record.country_code.as_deref(), Some("AA"));

    // Cleared, a field is empty: the next run fills it again, and only it.
    f.find(&[id], "fr", false);
    let meta = f.meta(id);
    assert_eq!(meta.country.as_deref(), Some("Alandie"));
    assert_eq!(meta.region.as_deref(), Some("Mon coin"));

    // Every field written by a person: the record is gone.
    for (field, value) in [
        (MetadataField::City, "x"),
        (MetadataField::Country, "y"),
        (MetadataField::CountryCode, "z"),
    ] {
        f.set(id, field, value);
    }
    assert!(f.meta(id).place_filled.is_none());
}

#[test]
fn a_refresh_moves_what_auroraw_wrote_with_the_position_and_leaves_what_a_person_wrote() {
    let photo = photo_at(5.0, 2.0);
    let id = photo.photo_id;
    let f = fixture(&[photo]);
    f.find(&[id], "fr", false);
    f.set(id, MetadataField::Country, "Mon pays");
    // The photographer corrects the position, to the east.
    moved(&f, id, 5.0, 8.0);

    // Without a refresh, nothing follows.
    let plain = f.find(&[id], "fr", false);
    assert_eq!((plain.report.filled, plain.report.had_place), (0, 1));
    assert_eq!(f.meta(id).city.as_deref(), Some("Westville"));

    let refreshed = f.find(&[id], "fr", true);
    assert_eq!(refreshed.report.filled, 1);
    let meta = f.meta(id);
    assert_eq!(
        places(&meta),
        [Some("Eastburg"), Some("Est"), Some("Mon pays"), Some("AA")],
        "city and region follow; the country is the person's"
    );
    let record = meta.place_filled.unwrap();
    assert_eq!(record.city.as_deref(), Some("Eastburg"));
    assert_eq!(record.country, None);
    assert!(record.longitude.starts_with("8.0"), "{}", record.longitude);

    f.engine.undo().unwrap();
    assert_eq!(f.meta(id).city.as_deref(), Some("Westville"));
}

#[test]
fn typing_what_auroraw_wrote_changes_nothing_and_a_refresh_keeps_other_words() {
    // A person typing what Auroraw wrote changes nothing: it stays Auroraw's. A person's own different
    // words are never overwritten, even by a refresh.
    let photo = photo_at(5.0, 2.0);
    let id = photo.photo_id;
    let f = fixture(&[photo]);
    f.find(&[id], "fr", false);
    f.set(id, MetadataField::City, "Westville");
    assert_eq!(
        f.meta(id).place_filled.unwrap().city.as_deref(),
        Some("Westville")
    );
    f.set(id, MetadataField::Region, "Mine");
    moved(&f, id, 5.0, 8.0);
    f.find(&[id], "fr", true);
    let meta = f.meta(id);
    assert_eq!(meta.region.as_deref(), Some("Mine"));
    assert_eq!(meta.city.as_deref(), Some("Eastburg"));
}

#[test]
fn a_places_file_that_cannot_be_used_is_refused_at_once_and_nothing_starts() {
    let photo = photo_at(5.0, 2.0);
    let id = photo.photo_id;
    let f = fixture(&[photo]);
    for pack in [
        f.dir.path().join("missing.sqlite"),
        // Not a places file at all.
        {
            let path = f.dir.path().join("other.sqlite");
            std::fs::write(&path, b"not a database").unwrap();
            path
        },
    ] {
        let err = f
            .engine
            .submit_and_wait(Command::FindPlaceNames {
                scope: PlaceScope::Photos(vec![id]),
                pack,
                language: "fr".into(),
                refresh: false,
            })
            .unwrap_err();
        assert!(matches!(err, EngineError::Places(_)), "{err:?}");
    }
    assert_eq!(f.undo_label(), None);
    assert_eq!(places(&f.meta(id)), [None; 4]);
}

#[test]
fn a_photo_that_left_the_workspace_is_counted_as_failed_and_the_run_goes_on() {
    let photo = photo_at(5.0, 2.0);
    let id = photo.photo_id;
    let f = fixture(&[photo]);
    let found = f.find(&[PhotoId::random(), id], "fr", false);
    assert_eq!(
        (
            found.report.photos,
            found.report.failed,
            found.report.filled
        ),
        (2, 1, 1)
    );
}

#[test]
fn a_large_run_is_one_step_and_cancelling_it_keeps_what_was_found() {
    let n = 400;
    let photos: Vec<PhotoSidecar> = (0..n)
        .map(|i| photo_at(1.0 + (i % 8) as f64, 2.0))
        .collect();
    let ids: Vec<PhotoId> = photos.iter().map(|p| p.photo_id).collect();
    let f = fixture(&photos);

    let job = f.start(PlaceScope::Photos(ids.clone()), "fr", false);
    f.engine.submit(Command::CancelJob { job_id: job }).unwrap();
    let found = f.wait(job);
    let filled = ids
        .iter()
        .filter(|id| f.meta(**id).country.is_some())
        .count();
    assert!(
        found.cancelled && filled < n,
        "the cancel caught the run short: {filled}"
    );
    assert_eq!(found.report.filled, filled, "the report counts what landed");
    match f.undo_label() {
        Some((kind, count)) => assert_eq!((kind, count), (LabelKind::PlaceNames, filled)),
        None => assert_eq!(filled, 0),
    }

    // Running it again finishes the rest, as one more step.
    let rest = f.find(&ids, "fr", false);
    assert_eq!(rest.report.filled, n - filled);
    assert!(ids.iter().all(|id| f.meta(*id).country.is_some()));
}

/// WP10's "done when": ten thousand photos, one step. Slow in a debug build, so run on purpose:
/// `cargo test -p auroraw-engine --release --test place_names -- --ignored`.
#[test]
#[ignore = "ten thousand sidecars: run on purpose"]
fn ten_thousand_photos_are_filled_as_one_step_and_undone_as_one() {
    let n = 10_000;
    let photos: Vec<PhotoSidecar> = (0..n)
        .map(|i| {
            photo_at(
                0.5 + (i % 90) as f64 / 10.0,
                0.5 + (i / 90 % 90) as f64 / 10.0,
            )
        })
        .collect();
    let ids: Vec<PhotoId> = photos.iter().map(|p| p.photo_id).collect();
    let f = fixture(&photos);

    let started = Instant::now();
    let found = f.find(&ids, "fr", false);
    let took = started.elapsed();
    eprintln!("{n} photos in {took:?}");
    assert!(!found.cancelled);
    assert_eq!((found.report.photos, found.report.filled), (n, n));
    assert_eq!(f.undo_label(), Some((LabelKind::PlaceNames, n)));
    assert!(
        ids.iter()
            .all(|id| f.meta(*id).country.as_deref() == Some("Alandie"))
    );

    f.engine.undo().unwrap();
    assert!(ids.iter().all(|id| f.meta(*id).place_filled.is_none()));
}
