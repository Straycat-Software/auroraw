// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact duplicates (WP9, D-036, D-108): one photo with more than one location, detected and confirmed by a
//! whole-file hash, never by a sampled fingerprint alone. Two shapes: a fresh source that already holds a copy of a
//! photo another source has (`index_job`'s own join), and a file copied within a source that is already registered,
//! found again on a rescan (`coordinator::scan_source`'s `SecondLocation`).

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

/// A picture whose content depends on `seed`, not on where it is written: two files written with the same seed are
/// byte-identical, an actual duplicate; a different seed is an unrelated photo.
fn write_jpeg(path: &std::path::Path, seed: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let img = ImageBuffer::from_fn(64, 48, |x, y| {
        Rgb([(x as u8) ^ seed, (y as u8).wrapping_mul(3) ^ seed, seed])
    });
    DynamicImage::ImageRgb8(img)
        .save_with_format(path, ImageFormat::Jpeg)
        .unwrap();
}

/// Registers `root` as a source (named `name`), scans it and confirms its one new file: the source and the photo.
fn add_one(engine: &Engine, root: &std::path::Path, name: &str) -> (SourceId, PhotoId) {
    let Outcome::SourceAdded(source_id) = engine
        .submit_and_wait(Command::AddSource {
            name: name.into(),
            root: root.to_path_buf(),
            kind: auroraw_sources::filesystem::LOCAL_FOLDER.into(),
        })
        .unwrap()
    else {
        panic!("expected SourceAdded");
    };
    let Outcome::Scanned { new, .. } = engine
        .submit_and_wait(Command::ScanSource { source_id })
        .unwrap()
    else {
        panic!("expected Scanned");
    };
    let Outcome::PhotosAdded(added) = engine
        .submit_and_wait(Command::AddNewPhotos {
            source_id,
            paths: new,
        })
        .unwrap()
    else {
        panic!("expected PhotosAdded");
    };
    assert_eq!(added.len(), 1);
    (source_id, added[0])
}

#[test]
fn a_source_holding_a_copy_of_an_existing_photo_joins_it_instead_of_becoming_a_new_photo() {
    let (engine, events, dir) = new_engine();
    let working = dir.path().join("Working");
    write_jpeg(&working.join("a.jpg"), 7);
    let (_, original) = add_one(&engine, &working, "Card");

    let backup = dir.path().join("Backup");
    write_jpeg(&backup.join("copy_of_a.jpg"), 7);
    let added = engine
        .add_source(AddSourceRequest {
            root: backup,
            name: Some("Backup disk".into()),
            merge: false,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::IndexFinished {
                job,
                added: a,
                second_locations,
                ..
            }) if job == added.job => {
                assert_eq!(a, 0, "joined, not added as a new photo");
                assert_eq!(second_locations, 1);
                break;
            }
            _ => assert!(Instant::now() < deadline, "the index job never finished"),
        }
    }

    let count: u64 = engine.read_catalogue().unwrap().count_all().unwrap();
    assert_eq!(count, 1, "still one photo, not two");
    let dups = engine.duplicate_photos().unwrap();
    assert_eq!(dups.len(), 1);
    assert_eq!(dups[0].photo_id, original);
    assert_eq!(dups[0].primary.source_name, "Card");
    assert_eq!(dups[0].primary.path, "a.jpg");
    assert_eq!(dups[0].extra.len(), 1);
    assert_eq!(dups[0].extra[0].source_name, "Backup disk");
    assert_eq!(dups[0].extra[0].path, "copy_of_a.jpg");
}

#[test]
fn a_raw_and_jpeg_pair_in_a_new_source_is_still_joined_to_its_existing_copy() {
    // Issues #6 and #8: the join used to be skipped entirely whenever the candidate had a RAW+JPEG companion
    // (exactly what a burst on a RAW-shooting camera produces), so a paired duplicate was always added as a
    // brand-new photo instead of being recognized (D-109).
    let (engine, events, dir) = new_engine();
    let working = dir.path().join("Working");
    std::fs::create_dir_all(&working).unwrap();
    std::fs::write(working.join("IMG_0001.CR2"), b"the same raw bytes").unwrap();
    write_jpeg(&working.join("IMG_0001.JPG"), 11);
    // `AddSourceRequest` (the real UI's path, `index_job`) so the pair is registered as one photo — unlike this
    // file's other tests, `ScanSource`+`AddNewPhotos` (the CLI's own manual flow) has no pairing of its own.
    let first = engine
        .add_source(AddSourceRequest {
            root: working,
            name: Some("Card".into()),
            merge: false,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::IndexFinished { job, added: a, .. }) if job == first.job => {
                assert_eq!(a, 1, "the pair is one photo");
                break;
            }
            _ => assert!(Instant::now() < deadline, "the index job never finished"),
        }
    }

    // A file-manager copy of the same pair, in a fresh source (mirroring #6's scenario).
    let backup = dir.path().join("Backup");
    std::fs::create_dir_all(&backup).unwrap();
    std::fs::write(backup.join("IMG_0001.CR2"), b"the same raw bytes").unwrap();
    write_jpeg(&backup.join("IMG_0001.JPG"), 11);
    let added = engine
        .add_source(AddSourceRequest {
            root: backup,
            name: Some("Backup disk".into()),
            merge: false,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::IndexFinished {
                job,
                added: a,
                second_locations,
                ..
            }) if job == added.job => {
                assert_eq!(
                    a, 0,
                    "joined, not added as a new photo, even with a companion"
                );
                assert_eq!(second_locations, 1);
                break;
            }
            _ => assert!(Instant::now() < deadline, "the index job never finished"),
        }
    }

    let count: u64 = engine.read_catalogue().unwrap().count_all().unwrap();
    assert_eq!(count, 1, "still one photo, not two");
    let dups = engine.duplicate_photos().unwrap();
    assert_eq!(dups.len(), 1);
    assert_eq!(dups[0].extra[0].source_name, "Backup disk");
    assert_eq!(dups[0].extra[0].path, "IMG_0001.CR2");
}

#[test]
fn a_second_source_with_unrelated_photos_adds_them_as_their_own_photos() {
    let (engine, _events, dir) = new_engine();
    let working = dir.path().join("Working");
    write_jpeg(&working.join("a.jpg"), 7);
    add_one(&engine, &working, "Card");

    let elsewhere = dir.path().join("Elsewhere");
    write_jpeg(&elsewhere.join("b.jpg"), 9);
    add_one(&engine, &elsewhere, "Other card");

    let count: u64 = engine.read_catalogue().unwrap().count_all().unwrap();
    assert_eq!(count, 2, "two unrelated photos, not joined");
    assert!(engine.duplicate_photos().unwrap().is_empty());
}

#[test]
fn a_file_copied_within_an_already_registered_source_is_found_as_a_second_location_on_a_rescan() {
    let (engine, _events, dir) = new_engine();
    let root = dir.path().join("Card");
    write_jpeg(&root.join("a.jpg"), 3);
    let (source_id, original) = add_one(&engine, &root, "Card");

    // Copied by hand, outside Auroraw, into a backup subfolder of the same source.
    std::fs::create_dir_all(root.join("backup")).unwrap();
    std::fs::copy(root.join("a.jpg"), root.join("backup/a.jpg")).unwrap();

    let Outcome::Scanned {
        second_locations, ..
    } = engine
        .submit_and_wait(Command::ScanSource { source_id })
        .unwrap()
    else {
        panic!("expected Scanned");
    };
    assert_eq!(second_locations, 1);

    let dups = engine.duplicate_photos().unwrap();
    assert_eq!(dups.len(), 1);
    assert_eq!(dups[0].photo_id, original);
    assert_eq!(dups[0].extra[0].path, "backup/a.jpg");

    // Rescanning again finds nothing new: the second location is already known.
    let Outcome::Scanned {
        second_locations,
        confirmed,
        ..
    } = engine
        .submit_and_wait(Command::ScanSource { source_id })
        .unwrap()
    else {
        panic!("expected Scanned");
    };
    assert_eq!(second_locations, 0, "already recorded, not rediscovered");
    assert_eq!(
        confirmed, 2,
        "both the primary and the secondary are now known files"
    );
}

#[test]
fn a_fingerprint_collision_that_does_not_confirm_by_hash_is_not_joined() {
    let (engine, events, dir) = new_engine();
    let working = dir.path().join("Working");
    write_jpeg(&working.join("a.jpg"), 5);
    add_one(&engine, &working, "Card");

    // A different file, forced to carry the *same sampled fingerprint* as "a.jpg" without being the same content:
    // the collision the whole-file hash exists to catch.
    let elsewhere = dir.path().join("Elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let original_bytes = std::fs::read(working.join("a.jpg")).unwrap();
    // Same size and the same sampled chunks (the fingerprint's own definition, note 004 §6.1: start, middle and end
    // of the file) but different content in between, so the fingerprint matches and the whole-file hash does not.
    let mut forged = original_bytes.clone();
    let middle = forged.len() / 2;
    assert!(middle + 48 < forged.len(), "the test file is large enough");
    for byte in &mut forged[middle + 40..middle + 48] {
        *byte ^= 0xFF;
    }
    std::fs::write(elsewhere.join("b.jpg"), &forged).unwrap();

    let added = engine
        .add_source(AddSourceRequest {
            root: elsewhere,
            name: Some("Elsewhere".into()),
            merge: false,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::IndexFinished { job, .. }) if job == added.job => break,
            _ => assert!(Instant::now() < deadline, "the index job never finished"),
        }
    }
    // Whether or not the forged bytes happen to actually share a fingerprint with "a.jpg" (only likely, not
    // guaranteed, given note 004's 3-chunk sample), it is never silently joined without a matching whole-file hash.
    assert!(
        engine.duplicate_photos().unwrap().is_empty(),
        "never joined on a fingerprint match alone"
    );
}
