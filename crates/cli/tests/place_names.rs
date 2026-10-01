// SPDX-License-Identifier: GPL-3.0-or-later
//! Finding the place names through the real `auroraw-cli` binary (WP10, design note 008): a photo with a
//! position gets its city, region, country and code in the asked language; a photo without one is counted
//! and left alone; and a file that is not a places file is refused with an error, not a panic.

use std::process::{Command, Output};

use auroraw_format::sidecar::PhotoSidecar;
use auroraw_places::{Level, NewArea, NewPlace, PackBuilder};
use auroraw_testkit::temp_dir;
use auroraw_types::PhotoId;
use auroraw_workspace::Workspace;

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_auroraw-cli"))
        .args(args)
        .output()
        .unwrap()
}

fn ok(out: &Output) -> String {
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn meta_of(workspace: &std::path::Path, photo: PhotoId) -> auroraw_format::sidecar::Metadata {
    Workspace::open(workspace)
        .unwrap()
        .read_photo(&photo)
        .unwrap()
        .unwrap()
        .current()
        .unwrap()
        .meta
}

#[test]
fn the_place_names_of_a_photo_are_written_in_the_asked_language() {
    let dir = temp_dir();
    let workspace = dir.path().join("Main");
    let catalogue = dir.path().join("main.sqlite");
    let (ws, cat) = (workspace.to_str().unwrap(), catalogue.to_str().unwrap());
    ok(&cli(&["create", ws, cat, "Main"]));

    let (located, unlocated) = (PhotoId::random(), PhotoId::random());
    let mut photo = PhotoSidecar::new(located);
    photo.meta.original.gps_latitude = Some("5,0.0000N".into());
    photo.meta.original.gps_longitude = Some("2,0.0000E".into());
    {
        // (Closed before the binary runs: it writes the sidecars.)
        let store = Workspace::open(&workspace).unwrap();
        store.write_photo(&photo).unwrap();
        store.write_photo(&PhotoSidecar::new(unlocated)).unwrap();
    }
    ok(&cli(&["rebuild", ws, cat]));

    let pack = dir.path().join("places.sqlite");
    let mut builder = PackBuilder::create(&pack).unwrap();
    let square = vec![
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 10.0),
        (0.0, 10.0),
        (0.0, 0.0),
    ];
    builder
        .add_area(&NewArea {
            level: Level::Country,
            code: "AA".into(),
            country_code: "AA".into(),
            parent: None,
            name: "Aland".into(),
            names: vec![("fr".into(), "Alandie".into())],
            geonames_id: None,
            parts: vec![vec![square]],
        })
        .unwrap();
    builder
        .add_place(&NewPlace {
            id: 1,
            name: "Westville".into(),
            lat: 5.0,
            lon: 2.0,
            country_code: "AA".into(),
            population: 5_000,
            section: false,
        })
        .unwrap();
    builder.finish().unwrap();
    let pack = pack.to_str().unwrap();

    let (a, b) = (located.to_string(), unlocated.to_string());

    // The workspace is open elsewhere, so the sidecars cannot be written: the report says so, it does not
    // claim the names were filled.
    let held = Workspace::open(&workspace).unwrap();
    let out = ok(&cli(&["place-names", ws, cat, pack, &a]));
    assert!(out.contains("1 photo(s): 0 filled"), "{out}");
    assert!(
        out.contains("1 failed (could not be read or written)"),
        "{out}"
    );
    drop(held);

    // A preview says what a run would change, and writes nothing.
    let out = ok(&cli(&[
        "place-names",
        ws,
        cat,
        pack,
        "--language",
        "fr",
        "--preview",
        &a,
        &b,
    ]));
    assert!(out.contains("nothing is written"), "{out}");
    assert!(
        out.contains("city: (empty) -> Westville  (1 photo(s))"),
        "{out}"
    );
    assert!(out.contains("country: (empty) -> Alandie"), "{out}");
    assert!(out.contains("2 photo(s): 1 filled"), "{out}");
    assert_eq!(
        meta_of(&workspace, located).country,
        None,
        "still nothing written"
    );

    let out = ok(&cli(&[
        "place-names",
        ws,
        cat,
        pack,
        "--language",
        "fr",
        &a,
        &b,
    ]));
    assert!(out.contains("2 photo(s): 1 filled"), "{out}");
    assert!(out.contains("1 without a position"), "{out}");
    let meta = meta_of(&workspace, located);
    assert_eq!(
        (
            meta.city.as_deref(),
            meta.country.as_deref(),
            meta.country_code.as_deref()
        ),
        (Some("Westville"), Some("Alandie"), Some("AA"))
    );
    let meta = meta_of(&workspace, unlocated);
    assert_eq!(meta.country, None);

    // A second run has nothing to write.
    let out = ok(&cli(&["place-names", ws, cat, pack, &a]));
    assert!(out.contains("1 already had their places"), "{out}");

    // A file that is not a places file is an error, and nothing was started.
    let bad = dir.path().join("bad.sqlite");
    std::fs::write(&bad, b"not a database").unwrap();
    let out = cli(&["place-names", ws, cat, bad.to_str().unwrap(), &a]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("error:"));
}
