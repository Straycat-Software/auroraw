// SPDX-License-Identifier: GPL-3.0-or-later
//! The place filter through the engine (WP10, design note 008 §5): the place columns of the catalogue follow what
//! the engine writes to a photo's sidecar, an Undo included, and a catalogue made before they existed is filled
//! when the engine opens it.

use std::time::{Duration, Instant};

use auroraw_catalogue::{Catalogue, Filter, FlagFilter, PlaceColumns, PlaceFilter};
use auroraw_engine::{Command, Engine, Event, EventReceiver, MetadataField};
use auroraw_format::sidecar::PhotoSidecar;
use auroraw_testkit::{TempDir, temp_dir};
use auroraw_types::PhotoId;
use auroraw_workspace::Workspace;

struct Fixture {
    dir: TempDir,
    engine: Engine,
    events: EventReceiver,
    photos: Vec<PhotoId>,
}

/// A workspace with these photos, `(country, region, city)` each, and an engine on it, rebuilt.
fn fixture(places: &[(&str, &str, &str)]) -> Fixture {
    let dir = temp_dir();
    let root = dir.path().join("W");
    let catalogue = dir.path().join("catalogue.sqlite");
    let ws = Workspace::create(&root, "Main").unwrap();
    let mut photos = Vec::new();
    for (country, region, city) in places {
        let mut photo = PhotoSidecar::new(PhotoId::random());
        let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
        photo.meta.country = some(country);
        photo.meta.region = some(region);
        photo.meta.city = some(city);
        ws.write_photo(&photo).unwrap();
        photos.push(photo.photo_id);
    }
    let workspace_id = ws.workspace_id();
    drop(ws);
    drop(Catalogue::create(&catalogue, workspace_id).unwrap());
    let (engine, events) = Engine::open(&root, &catalogue).unwrap();
    engine.submit_and_wait(Command::Rebuild).unwrap();
    events.drain();
    Fixture {
        dir,
        engine,
        events,
        photos,
    }
}

fn outline(f: &Fixture) -> Vec<String> {
    fn walk(nodes: &[auroraw_catalogue::PlaceNode], depth: usize, out: &mut Vec<String>) {
        for node in nodes {
            out.push(format!(
                "{}{} {}",
                " ".repeat(depth),
                node.label,
                node.count
            ));
            walk(&node.children, depth + 1, out);
        }
    }
    let facets = f
        .engine
        .read_catalogue()
        .unwrap()
        .place_facets(&Filter::default())
        .unwrap();
    let mut out = Vec::new();
    walk(&facets.countries, 0, &mut out);
    out
}

fn wait_for(events: &EventReceiver, what: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match events.recv_timeout(Duration::from_millis(100)) {
            Some(event) if what(&event) => return event,
            Some(_) => {}
            None => assert!(Instant::now() < deadline, "the event never came"),
        }
    }
}

#[test]
fn the_places_of_the_photos_are_listed_and_follow_what_the_engine_writes() {
    let f = fixture(&[
        ("Canada", "Québec", "Montréal"),
        ("Canada", "Québec", "Montréal"),
        ("Canada", "Ontario", "Ottawa"),
        ("France", "", ""),
        ("", "", ""),
    ]);
    assert_eq!(
        outline(&f),
        [
            "Canada 3",
            " Ontario 1",
            "  Ottawa 1",
            " Québec 2",
            "  Montréal 2",
            "France 1"
        ]
    );

    // A person writes a city: the tree follows, at once, from the sidecar the engine wrote.
    f.engine
        .submit_and_wait(Command::SetMetadataField {
            photo_id: f.photos[0],
            field: MetadataField::City,
            value: "Laval".into(),
        })
        .unwrap();
    assert_eq!(
        outline(&f),
        [
            "Canada 3",
            " Ontario 1",
            "  Ottawa 1",
            " Québec 2",
            "  Laval 1",
            "  Montréal 1",
            "France 1"
        ]
    );
    // ... and Undo takes it back.
    f.engine.undo().unwrap();
    assert_eq!(outline(&f)[3..5], [" Québec 2", "  Montréal 2"]);

    // Giving a photo a country puts it in the tree; the filter selects it.
    f.engine
        .submit_and_wait(Command::SetMetadataField {
            photo_id: f.photos[4],
            field: MetadataField::Country,
            value: "france".into(),
        })
        .unwrap();
    let catalogue = f.engine.read_catalogue().unwrap();
    let france = catalogue
        .place_facets(&Filter::default())
        .unwrap()
        .countries[1]
        .filter
        .clone();
    assert_eq!(
        france.country.as_deref(),
        Some("FR"),
        "the name `france` has its country's code for a key"
    );
    let selected = catalogue
        .list_filtered(
            &Filter {
                place: Some(france),
                flags: FlagFilter::All,
                ..Filter::default()
            },
            None,
            100,
        )
        .unwrap();
    assert_eq!(
        selected.len(),
        2,
        "the French one, and the one that just got 'france'"
    );
}

#[test]
fn a_catalogue_made_before_the_place_columns_is_filled_when_the_engine_opens_it() {
    let f = fixture(&[
        ("Canada", "Québec", "Montréal"),
        ("Canada", "Ontario", "Ottawa"),
        ("France", "Bretagne", "Rennes"),
    ]);
    let full = outline(&f);
    assert_eq!(full.len(), 8);

    // Close the engine, and make the catalogue as schema 5 left it: the columns exist and are empty, and it asks for
    // the pass.
    let Fixture {
        dir,
        engine,
        events,
        photos,
    } = f;
    drop(engine);
    wait_for(&events, |e| matches!(e, Event::Stopped));
    let path = dir.path().join("catalogue.sqlite");
    {
        let mut old = Catalogue::open(&path).unwrap();
        for id in &photos {
            old.apply_place_columns(id, &PlaceColumns::default())
                .unwrap();
        }
        old.mark_place_columns_stale().unwrap();
        assert_eq!(old.place_facets(&Filter::default()).unwrap().placed, 0);
    }

    let (engine, events) = Engine::open(&dir.path().join("W"), &path).unwrap();
    let Event::PlaceColumnsFilled {
        photos: covered,
        failed,
    } = wait_for(&events, |e| matches!(e, Event::PlaceColumnsFilled { .. }))
    else {
        unreachable!()
    };
    assert_eq!(covered, 3);
    assert_eq!(failed, 0);
    let catalogue = engine.read_catalogue().unwrap();
    assert!(!catalogue.place_columns_stale().unwrap(), "asked once");
    let facets = catalogue.place_facets(&Filter::default()).unwrap();
    assert_eq!(facets.placed, 3);
    let canada = PlaceFilter {
        country: facets.countries[0].filter.country.clone(),
        ..PlaceFilter::default()
    };
    assert_eq!(
        catalogue
            .list_filtered(
                &Filter {
                    place: Some(canada),
                    ..Filter::default()
                },
                None,
                10
            )
            .unwrap()
            .len(),
        2
    );

    // Opened again, it does not do it again.
    drop(catalogue);
    drop(engine);
    wait_for(&events, |e| matches!(e, Event::Stopped));
    let (engine, events) = Engine::open(&dir.path().join("W"), &path).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !events
            .drain()
            .iter()
            .any(|e| matches!(e, Event::PlaceColumnsFilled { .. })),
        "the pass is done"
    );
    drop(engine);
}

#[test]
fn the_pass_counts_the_photos_it_cannot_read_and_does_not_start_again_for_them() {
    let f = fixture(&[
        ("Canada", "Québec", "Montréal"),
        ("Canada", "Ontario", "Ottawa"),
        ("France", "", ""),
    ]);
    let Fixture {
        dir,
        engine,
        events,
        photos,
    } = f;
    drop(engine);
    wait_for(&events, |e| matches!(e, Event::Stopped));
    let path = dir.path().join("catalogue.sqlite");
    {
        let mut old = Catalogue::open(&path).unwrap();
        for id in &photos {
            old.apply_place_columns(id, &PlaceColumns::default())
                .unwrap();
        }
        old.mark_place_columns_stale().unwrap();
    }
    // One photo's sidecar is gone (a card taken out, a file deleted by hand): the pass cannot read it.
    let root = dir.path().join("W");
    let ws = Workspace::open(&root).unwrap();
    std::fs::remove_file(ws.photo_path(&photos[1])).unwrap();
    drop(ws);

    let (engine, events) = Engine::open(&root, &path).unwrap();
    let Event::PlaceColumnsFilled {
        photos: covered,
        failed,
    } = wait_for(&events, |e| matches!(e, Event::PlaceColumnsFilled { .. }))
    else {
        unreachable!()
    };
    assert_eq!(
        (covered, failed),
        (3, 1),
        "it says how many it could not read"
    );
    let catalogue = engine.read_catalogue().unwrap();
    assert_eq!(
        catalogue.place_facets(&Filter::default()).unwrap().placed,
        2
    );
    assert!(
        !catalogue.place_columns_stale().unwrap(),
        "the marker is not left for one photo: that would read every sidecar again at each open"
    );
    drop(catalogue);
    drop(engine);
    wait_for(&events, |e| matches!(e, Event::Stopped));
    let (engine, events) = Engine::open(&root, &path).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !events
            .drain()
            .iter()
            .any(|e| matches!(e, Event::PlaceColumnsFilled { .. })),
        "and the next open does not start the pass again"
    );
    drop(engine);
}
