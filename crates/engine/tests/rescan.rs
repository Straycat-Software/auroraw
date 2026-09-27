// SPDX-License-Identifier: GPL-3.0-or-later
//! Rescanning a source through the real production path (`Command::IndexSource`, `index_job`, D-109): before
//! the unification with `coordinator::scan_source`'s reconcile, `index_job` never noticed a file had vanished,
//! changed, or moved within its own source (issue #7) — it only ever partitioned discovered files into known-by-
//! exact-path and new. These tests exercise the real "rescan" path (the same one the UI's own rescan button
//! calls), not `Command::ScanSource`, which `tests/duplicates.rs`'s own within-source case already covers.

use std::time::{Duration, Instant};

use auroraw_engine::{AddSourceRequest, Command, Engine, Event, EventReceiver, Outcome};
use auroraw_testkit::TempDir;
use auroraw_types::{PhotoId, SourceId};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

fn new_engine() -> (Engine, EventReceiver, TempDir) {
    let dir = auroraw_testkit::temp_dir();
    let (engine, events) = Engine::create(
        &dir.path().join("Main"),
        &dir.path().join("main.sqlite"),
        "Main",
    )
    .unwrap();
    (engine, events, dir)
}

fn write_jpeg(path: &std::path::Path, seed: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let img = ImageBuffer::from_fn(64, 48, |x, y| {
        Rgb([(x as u8) ^ seed, (y as u8).wrapping_mul(3) ^ seed, seed])
    });
    DynamicImage::ImageRgb8(img)
        .save_with_format(path, ImageFormat::Jpeg)
        .unwrap();
}

/// Waits for `source_id`'s `IndexFinished` and returns `(added, missing)`.
fn index(engine: &Engine, events: &EventReceiver, source_id: SourceId) -> (usize, usize) {
    let Outcome::IndexStarted { job } = engine
        .submit_and_wait(Command::IndexSource {
            source_id,
            merge: Vec::new(),
        })
        .unwrap()
    else {
        panic!("expected IndexStarted");
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::IndexFinished {
                job: j,
                added,
                missing,
                ..
            }) if j == job => return (added, missing),
            _ => assert!(Instant::now() < deadline, "the index job never finished"),
        }
    }
}

/// Adds `root` as a source (named `name`) through the real path (`index_job`) and waits for its one photo.
fn add_one(
    engine: &Engine,
    events: &EventReceiver,
    root: &std::path::Path,
    name: &str,
) -> (SourceId, PhotoId) {
    let added = engine
        .add_source(AddSourceRequest {
            root: root.to_path_buf(),
            name: Some(name.into()),
            merge: false,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let source_id = loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::IndexFinished { job, .. }) if job == added.job => break added.source_id,
            _ => assert!(Instant::now() < deadline, "the index job never finished"),
        }
    };
    let rows = engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 10)
        .unwrap();
    let row = rows
        .into_iter()
        .find(|r| r.source_id == Some(source_id))
        .expect("the one photo just indexed");
    (source_id, row.id)
}

#[test]
fn a_file_deleted_from_its_source_is_flagged_missing_on_a_rescan() {
    let (engine, events, dir) = new_engine();
    let root = dir.path().join("Card");
    write_jpeg(&root.join("a.jpg"), 1);
    let (source_id, photo_id) = add_one(&engine, &events, &root, "Card");

    std::fs::remove_file(root.join("a.jpg")).unwrap();
    let (added, missing) = index(&engine, &events, source_id);
    assert_eq!(added, 0);
    assert_eq!(missing, 1);

    let row = engine
        .read_catalogue()
        .unwrap()
        .photo(&photo_id)
        .unwrap()
        .expect("the photo is kept");
    assert!(row.original_missing, "flagged missing, not removed (D-019)");
    assert!(!row.original_changed);
}

#[test]
fn a_secondary_locations_file_deleted_is_pruned_without_touching_the_primarys_flags() {
    let (engine, events, dir) = new_engine();
    let root = dir.path().join("Card");
    write_jpeg(&root.join("a.jpg"), 2);
    let (source_id, photo_id) = add_one(&engine, &events, &root, "Card");

    // A second location within the same source (copied by hand), confirmed on the first rescan.
    std::fs::create_dir_all(root.join("backup")).unwrap();
    std::fs::copy(root.join("a.jpg"), root.join("backup/a.jpg")).unwrap();
    let (_, missing) = index(&engine, &events, source_id);
    assert_eq!(missing, 0);
    assert_eq!(engine.duplicate_photos().unwrap().len(), 1);

    // The copy is deleted: the report drops it, and the primary is untouched.
    std::fs::remove_file(root.join("backup/a.jpg")).unwrap();
    let (added, missing) = index(&engine, &events, source_id);
    assert_eq!(added, 0);
    // `missing` counts every `Missing` outcome (primary or secondary), matching `scan_source`'s own
    // `Outcome::Scanned::missing`: what it *means* for the catalogue is what differs (below).
    assert_eq!(missing, 1);

    assert!(
        engine.duplicate_photos().unwrap().is_empty(),
        "the pruned location is gone from the report"
    );
    let row = engine
        .read_catalogue()
        .unwrap()
        .photo(&photo_id)
        .unwrap()
        .unwrap();
    assert!(!row.original_missing, "the primary was never touched");
    assert!(!row.original_changed);
}

#[test]
fn a_file_renamed_within_its_source_relinks_instead_of_being_flagged_missing_and_readded() {
    let (engine, events, dir) = new_engine();
    let root = dir.path().join("Card");
    write_jpeg(&root.join("a.jpg"), 3);
    let (source_id, photo_id) = add_one(&engine, &events, &root, "Card");

    std::fs::create_dir_all(root.join("2024")).unwrap();
    std::fs::rename(root.join("a.jpg"), root.join("2024/a.jpg")).unwrap();
    let (added, missing) = index(&engine, &events, source_id);
    assert_eq!(added, 0, "not a new photo");
    assert_eq!(
        missing, 0,
        "not flagged missing either: it moved, silently (D-019)"
    );

    let count: u64 = engine.read_catalogue().unwrap().count_all().unwrap();
    assert_eq!(count, 1);
    let row = engine
        .read_catalogue()
        .unwrap()
        .photo(&photo_id)
        .unwrap()
        .unwrap();
    assert_eq!(row.path.as_deref(), Some("2024/a.jpg"));
    assert!(!row.original_missing);
}

#[test]
fn a_changed_file_is_reported_and_not_added_again() {
    let (engine, events, dir) = new_engine();
    let root = dir.path().join("Card");
    write_jpeg(&root.join("a.jpg"), 4);
    let (source_id, photo_id) = add_one(&engine, &events, &root, "Card");

    // The same path, different content (a card reused, or the file edited outside Auroraw).
    write_jpeg(&root.join("a.jpg"), 5);
    let (added, missing) = index(&engine, &events, source_id);
    assert_eq!(added, 0, "still the same photo, not a new one");
    assert_eq!(missing, 0);

    let count: u64 = engine.read_catalogue().unwrap().count_all().unwrap();
    assert_eq!(count, 1);
    let row = engine
        .read_catalogue()
        .unwrap()
        .photo(&photo_id)
        .unwrap()
        .unwrap();
    assert!(
        row.original_changed,
        "D-109: index_job now re-fingerprints known files too"
    );
}
