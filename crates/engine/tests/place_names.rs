// SPDX-License-Identifier: GPL-3.0-or-later
//! Finding the place names (WP10, design note 008, D-147): what is written and what is left alone, the
//! record of what Auroraw wrote and how a person's write takes a field out of it, the refresh, and the
//! run as one undoable, cancellable step.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use auroraw_catalogue::Catalogue;
use auroraw_engine::{
    Command, Engine, EngineError, Event, EventReceiver, JobId, LabelKind, MetadataField, Outcome,
    PlaceField, PlaceNamesReport, PlacePreview, PlaceScope,
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

    /// Previews a run and waits for what it would do.
    fn preview_of(&self, scope: PlaceScope, language: &str, refresh: bool) -> (PlacePreview, bool) {
        let Outcome::PlaceNamesStarted { job, .. } = self
            .engine
            .submit_and_wait(Command::PreviewPlaceNames {
                scope,
                pack: self.pack.clone(),
                language: language.into(),
                refresh,
            })
            .unwrap()
        else {
            panic!("expected PlaceNamesStarted");
        };
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut found = None;
        loop {
            match self.events.recv_timeout(Duration::from_millis(100)) {
                Some(Event::PlaceNamesPreview {
                    job: j,
                    preview,
                    cancelled,
                }) if j == job => found = Some((preview, cancelled)),
                Some(Event::JobFinished(j) | Event::JobCancelled(j)) if j == job => {
                    return found.expect("the preview comes before the end");
                }
                Some(event) => self.note(&event),
                None => assert!(Instant::now() < deadline, "the preview never ended"),
            }
        }
    }

    fn preview(&self, photos: &[PhotoId], language: &str, refresh: bool) -> PlacePreview {
        self.preview_of(PlaceScope::Photos(photos.to_vec()), language, refresh)
            .0
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
fn writing_a_field_in_a_batch_releases_it_and_emptying_one_is_an_answer_not_a_gap() {
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
    assert_eq!(record.region, None, "written: theirs, out of the record");
    assert!(record.is_cleared(PlaceField::Country), "emptied: an answer");
    assert_eq!(record.city.as_deref(), Some("Westville"));
    assert_eq!(record.country_code.as_deref(), Some("AA"));

    // The answer stands: the next run, with or without a refresh, leaves the country empty (design note 008
    // §4: "clearing a city is an answer, not a gap to refill"), and finds nothing to do.
    for refresh in [false, true] {
        let again = f.find(&[id], "fr", refresh);
        assert_eq!(
            (again.report.filled, again.report.had_place),
            (0, 1),
            "refresh {refresh}"
        );
        let meta = f.meta(id);
        assert_eq!(meta.country, None, "refresh {refresh}");
        assert_eq!(meta.region.as_deref(), Some("Mon coin"));
    }

    // Every field written by a person: the record is gone, the answer with the rest.
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
fn emptying_a_field_can_be_undone_is_not_repeated_and_ends_when_a_person_writes_it() {
    let photo = photo_at(5.0, 2.0);
    let id = photo.photo_id;
    let f = fixture(&[photo]);
    f.find(&[id], "fr", false);

    f.set(id, MetadataField::City, "");
    assert_eq!(f.meta(id).city, None);
    assert!(
        f.meta(id)
            .place_filled
            .unwrap()
            .is_cleared(PlaceField::City)
    );
    assert_eq!(f.undo_label(), Some((LabelKind::MetaCity, 1)));

    // Emptying what is empty already says nothing: no second step, and the answer stands.
    f.set(id, MetadataField::City, "");
    f.engine.undo().unwrap();
    let back = f.meta(id);
    assert_eq!(back.city.as_deref(), Some("Westville"));
    assert_eq!(
        back.place_filled.as_ref().unwrap().city.as_deref(),
        Some("Westville"),
        "one undo takes the answer back: the name is Auroraw's again"
    );
    f.engine.redo().unwrap();
    assert!(
        f.meta(id)
            .place_filled
            .unwrap()
            .is_cleared(PlaceField::City)
    );

    // A person writing the field ends the answer: it is theirs, with a text.
    f.set(id, MetadataField::City, "Chez moi");
    let meta = f.meta(id);
    assert_eq!(meta.city.as_deref(), Some("Chez moi"));
    assert!(!meta.place_filled.unwrap().is_cleared(PlaceField::City));
    // And emptying it again is a new answer.
    f.set(id, MetadataField::City, "");
    assert!(
        f.meta(id)
            .place_filled
            .unwrap()
            .is_cleared(PlaceField::City)
    );
    assert_eq!(f.find(&[id], "fr", true).report.filled, 0);
    assert_eq!(f.meta(id).city, None);
}

#[test]
fn a_field_a_person_typed_and_then_emptied_is_an_answer_too_even_without_a_record() {
    // A photo Auroraw never filled: a person types a city, clears it. It stays empty when the source is run.
    let mut photo = photo_at(5.0, 2.0);
    photo.meta.city = Some("Chez moi".into());
    let id = photo.photo_id;
    let f = fixture(&[photo]);
    f.set(id, MetadataField::City, "");
    let meta = f.meta(id);
    assert!(
        meta.place_filled
            .as_ref()
            .unwrap()
            .is_cleared(PlaceField::City)
    );
    assert_eq!(
        (
            meta.place_filled.as_ref().unwrap().latitude.as_str(),
            meta.place_filled.as_ref().unwrap().longitude.as_str()
        ),
        ("", ""),
        "a record of an answer has no position"
    );
    let found = f.find(&[id], "fr", false);
    assert_eq!(
        found.report.filled, 1,
        "the region, country and code are filled"
    );
    let meta = f.meta(id);
    assert_eq!(meta.city, None, "the city is not");
    assert_eq!(meta.region.as_deref(), Some("Ouest"));
    let record = meta.place_filled.unwrap();
    assert!(record.is_cleared(PlaceField::City));
    assert!(
        record.latitude.starts_with("5.0"),
        "the position comes with the first fill"
    );
}

#[test]
fn a_preview_shows_what_a_run_would_do_grouped_and_writes_nothing() {
    let (west, west2, east) = (photo_at(5.0, 2.0), photo_at(5.0, 2.1), photo_at(5.0, 8.0));
    let (none, sea) = (PhotoSidecar::new(PhotoId::random()), photo_at(-40.0, 0.0));
    let ids = [
        west.photo_id,
        west2.photo_id,
        east.photo_id,
        none.photo_id,
        sea.photo_id,
    ];
    let f = fixture(&[west, west2, east, none, sea]);

    let preview = f.preview(&ids, "fr", false);
    assert_eq!(
        preview.report,
        PlaceNamesReport {
            photos: 5,
            filled: 3,
            had_place: 0,
            no_position: 1,
            open_water: 1,
            failed: 0,
        },
        "the counts a run would report"
    );
    let shown: Vec<(PlaceField, Option<&str>, Option<&str>, usize)> = preview
        .groups
        .iter()
        .map(|g| (g.field, g.before.as_deref(), g.after.as_deref(), g.photos))
        .collect();
    assert_eq!(
        shown,
        [
            (PlaceField::Country, None, Some("Alandie"), 3),
            (PlaceField::CountryCode, None, Some("AA"), 3),
            (PlaceField::City, None, Some("Westville"), 2),
            (PlaceField::Region, None, Some("Ouest"), 2),
            (PlaceField::City, None, Some("Eastburg"), 1),
            (PlaceField::Region, None, Some("Est"), 1),
        ],
        "the biggest first, ties by field and then by text"
    );
    assert_eq!(preview.groups_total, 6);
    // A few of the photos of a group, in the order they were looked at.
    assert_eq!(preview.groups[0].examples, ids[..3], "all three, in order");
    assert_eq!(preview.groups[2].examples, ids[..2]);

    // Nothing was written, and there is nothing to undo.
    for id in ids {
        assert_eq!(places(&f.meta(id)), [None; 4]);
        assert!(f.meta(id).place_filled.is_none());
    }
    assert_eq!(f.undo_label(), None);
    // The run that follows does what the preview said.
    let found = f.find(&ids, "fr", false);
    assert_eq!(found.report, preview.report);
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
fn typing_the_same_words_confirms_them_and_a_refresh_then_keeps_them() {
    // A person typing what Auroraw wrote is confirming it, and it is theirs (design note 008 §4): the
    // field leaves the record, as a step that can be undone, and a refresh never moves it.
    let photo = photo_at(5.0, 2.0);
    let id = photo.photo_id;
    let f = fixture(&[photo]);
    f.find(&[id], "fr", false);

    f.set(id, MetadataField::City, "Westville");
    let meta = f.meta(id);
    assert_eq!(
        meta.city.as_deref(),
        Some("Westville"),
        "the words did not change"
    );
    assert_eq!(
        meta.place_filled.unwrap().city,
        None,
        "but they are the person's now"
    );
    assert_eq!(f.undo_label(), Some((LabelKind::MetaCity, 1)));

    // Writing it again, now that it is theirs, is the no-op it always was: no second step.
    f.set(id, MetadataField::City, "Westville");
    f.engine.undo().unwrap();
    assert_eq!(
        f.meta(id).place_filled.unwrap().city.as_deref(),
        Some("Westville"),
        "one undo gives it back to Auroraw"
    );
    f.engine.redo().unwrap();

    f.set(id, MetadataField::Region, "Mine");
    moved(&f, id, 5.0, 8.0);
    f.find(&[id], "fr", true);
    let meta = f.meta(id);
    assert_eq!(
        (meta.city.as_deref(), meta.region.as_deref()),
        (Some("Westville"), Some("Mine")),
        "neither moves with the position: both are the person's"
    );
    assert_eq!(meta.country.as_deref(), Some("Alandie"));
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

    // The preview first: the same lookups, nothing written.
    let started = Instant::now();
    let preview = f.preview(&ids, "fr", false);
    eprintln!("{n} photos previewed in {:?}", started.elapsed());
    assert_eq!((preview.report.photos, preview.report.filled), (n, n));
    assert!(preview.groups_total <= 8, "{}", preview.groups_total);
    assert_eq!(f.undo_label(), None);

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

#[test]
fn a_preview_of_a_refresh_names_the_names_it_would_replace_and_the_run_does_that() {
    let photo = photo_at(5.0, 2.0);
    let id = photo.photo_id;
    let f = fixture(&[photo]);
    f.find(&[id], "fr", false);
    // The photographer corrects the position, to the east.
    moved(&f, id, 5.0, 8.0);

    // Without a refresh, there is nothing to do, and the preview says so.
    let plain = f.preview(&[id], "fr", false);
    assert!(plain.groups.is_empty());
    assert_eq!((plain.report.filled, plain.report.had_place), (0, 1));

    // With one: the city and the region follow, the country and the code are the same words.
    let refresh = f.preview(&[id], "fr", true);
    let shown: Vec<(PlaceField, Option<&str>, Option<&str>)> = refresh
        .groups
        .iter()
        .map(|g| (g.field, g.before.as_deref(), g.after.as_deref()))
        .collect();
    assert_eq!(
        shown,
        [
            (PlaceField::City, Some("Westville"), Some("Eastburg")),
            (PlaceField::Region, Some("Ouest"), Some("Est")),
        ]
    );
    assert_eq!(
        f.meta(id).city.as_deref(),
        Some("Westville"),
        "still nothing written"
    );

    let found = f.find(&[id], "fr", true);
    assert_eq!(found.report, refresh.report, "the run does what was shown");
    assert_eq!(f.meta(id).city.as_deref(), Some("Eastburg"));
    // The run is one step, the first fill is another, and the previews were none: two undos reach the start.
    f.engine.undo().unwrap();
    assert_eq!(f.meta(id).city.as_deref(), Some("Westville"));
    f.engine.undo().unwrap();
    assert_eq!(f.meta(id).city, None);
}

#[test]
fn a_preview_leaves_out_what_a_person_wrote_and_what_a_person_emptied() {
    let photo = photo_at(5.0, 2.0);
    let id = photo.photo_id;
    let f = fixture(&[photo]);
    f.set(id, MetadataField::City, "Chez moi");
    f.set(id, MetadataField::Region, "Mon coin");
    f.set(id, MetadataField::Region, "");
    let preview = f.preview(&[id], "fr", true);
    let fields: Vec<PlaceField> = preview.groups.iter().map(|g| g.field).collect();
    assert_eq!(fields, [PlaceField::Country, PlaceField::CountryCode]);
}

#[test]
fn a_preview_can_be_cancelled_and_covers_the_photos_it_got_to() {
    let n = 2_000;
    let photos: Vec<PhotoSidecar> = (0..n)
        .map(|i| photo_at(1.0 + (i % 8) as f64, 2.0))
        .collect();
    let ids: Vec<PhotoId> = photos.iter().map(|p| p.photo_id).collect();
    let f = fixture(&photos);
    let Outcome::PlaceNamesStarted { job, .. } = f
        .engine
        .submit_and_wait(Command::PreviewPlaceNames {
            scope: PlaceScope::Photos(ids.clone()),
            pack: f.pack.clone(),
            language: "fr".into(),
            refresh: false,
        })
        .unwrap()
    else {
        panic!("expected PlaceNamesStarted");
    };
    f.engine.submit(Command::CancelJob { job_id: job }).unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let (preview, cancelled) = loop {
        match f.events.recv_timeout(Duration::from_millis(100)) {
            Some(Event::PlaceNamesPreview {
                preview, cancelled, ..
            }) => break (preview, cancelled),
            Some(_) => {}
            None => assert!(Instant::now() < deadline, "the preview never ended"),
        }
    };
    assert!(
        cancelled && preview.report.photos < n,
        "the cancel caught it short: {}",
        preview.report.photos
    );
    assert_eq!(f.undo_label(), None);
    assert!(
        ids.iter().all(|id| f.meta(*id).country.is_none()),
        "nothing was written"
    );
}

#[test]
fn a_preview_with_a_places_file_that_cannot_be_used_is_refused_at_once() {
    let photo = photo_at(5.0, 2.0);
    let id = photo.photo_id;
    let f = fixture(&[photo]);
    let err = f
        .engine
        .submit_and_wait(Command::PreviewPlaceNames {
            scope: PlaceScope::Photos(vec![id]),
            pack: f.dir.path().join("missing.sqlite"),
            language: "fr".into(),
            refresh: false,
        })
        .unwrap_err();
    assert!(matches!(err, EngineError::Places(_)), "{err:?}");
}

#[test]
fn two_runs_on_the_same_photos_count_each_photo_once_and_a_lost_race_is_not_a_failure() {
    // Both workers look every photo up before either has written it; the coordinator applies the first and finds
    // nothing left to write for the second. That is "already had their place", not "could not be updated".
    let n = 600;
    let photos: Vec<PhotoSidecar> = (0..n)
        .map(|i| photo_at(1.0 + (i % 8) as f64, 2.0))
        .collect();
    let ids: Vec<PhotoId> = photos.iter().map(|p| p.photo_id).collect();
    let f = fixture(&photos);
    let a = f.start(PlaceScope::Photos(ids.clone()), "fr", false);
    let b = f.start(PlaceScope::Photos(ids.clone()), "fr", false);
    let (mut first, mut second) = (None, None);
    let deadline = Instant::now() + Duration::from_secs(60);
    while first.is_none() || second.is_none() {
        match f.events.recv_timeout(Duration::from_millis(100)) {
            Some(Event::PlaceNamesFound { job, report, .. }) if job == a => first = Some(report),
            Some(Event::PlaceNamesFound { job, report, .. }) if job == b => second = Some(report),
            Some(event) => f.note(&event),
            None => assert!(Instant::now() < deadline, "the runs never ended"),
        }
    }
    let (first, second) = (first.unwrap(), second.unwrap());
    assert_eq!(
        first.filled + second.filled,
        n,
        "each photo was filled by one of them"
    );
    assert_eq!(first.failed + second.failed, 0, "{first:?} {second:?}");
    assert_eq!(first.photos, n);
    assert_eq!(second.photos, n);
    assert_eq!(first.filled + first.had_place, n);
    assert_eq!(second.filled + second.had_place, n);
}
