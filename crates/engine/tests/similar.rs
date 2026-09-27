// SPDX-License-Identifier: GPL-3.0-or-later
//! The similar-photo suggestions (WP9, D-105): the thumbnail workers hash each photo's thumbnail, the engine records
//! it, and `similar_photos` offers the photos that look alike and were taken near each other, nearest first.

use std::time::{Duration, Instant};

use auroraw_engine::{Command, Engine, Outcome, SimilarQuery};
use auroraw_types::PhotoId;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

/// A picture of 8 by 8 blocks of pseudo-random greys: two seeds are two unrelated scenes; `lift` brightens one.
fn scene(seed: u32, lift: u8) -> DynamicImage {
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(12_345);
    let mut blocks = Vec::new();
    for _ in 0..64 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        blocks.push(((state >> 24) as u8) / 2 + 40);
    }
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(320, 240, |x, y| {
        let v = blocks[((y * 8 / 240) * 8 + (x * 8 / 320)) as usize];
        Rgb([v.saturating_add(lift); 3])
    }))
}

/// (file name, the scene it shows, minutes after 10:00 it was taken).
const PHOTOS: [(&str, u32, u8, u32); 7] = [
    ("p0.jpg", 1, 0, 0),   // the reference
    ("p1.jpg", 1, 10, 10), // the same scene, brighter, ten minutes later
    ("p2.jpg", 1, 20, 20), // and again
    ("p3.jpg", 2, 0, 5),   // another scene, in between
    ("p4.jpg", 3, 0, 15),  // another scene
    // The same scene, hours later: a lift of 1, not 0, so its file is not byte-identical to p0's (D-108 would
    // otherwise join it to p0 as a confirmed duplicate instead of adding it as its own photo) — a shift this small
    // moves no pixel far enough to change any of the dHash's neighbour comparisons, so its hash still matches p0's
    // exactly, which is what this fixture needs.
    ("p5.jpg", 1, 1, 200),
    ("p6.jpg", 1, 5, 12), // the same scene, in a resolved series
];

struct Library {
    engine: Engine,
    ids: Vec<PhotoId>,
    _dir: auroraw_testkit::TempDir,
}

fn library() -> Library {
    let dir = auroraw_testkit::temp_dir();
    let (engine, _events) = Engine::create(
        &dir.path().join("Main"),
        &dir.path().join("main.sqlite"),
        "Main",
    )
    .unwrap();
    let card = dir.path().join("Card");
    std::fs::create_dir_all(&card).unwrap();
    for (name, seed, lift, _) in PHOTOS {
        scene(seed, lift)
            .save_with_format(card.join(name), ImageFormat::Jpeg)
            .unwrap();
    }
    let Outcome::SourceAdded(source_id) = engine
        .submit_and_wait(Command::AddSource {
            name: "Card".into(),
            root: card,
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
    engine
        .submit_and_wait(Command::AddNewPhotos {
            source_id,
            paths: new,
        })
        .unwrap();
    // The photos by file name, and when each was taken (the generated JPEGs carry no EXIF of their own).
    let mut rows = engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 100)
        .unwrap();
    rows.sort_by(|a, b| a.filename.cmp(&b.filename));
    assert_eq!(rows.len(), PHOTOS.len());
    for (row, (_, _, _, minutes)) in rows.iter().zip(PHOTOS) {
        let mut sidecar = engine
            .workspace()
            .read_photo(&row.id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        sidecar.meta.original.capture_time = Some(format!(
            "2026-03-01T{:02}:{:02}:00Z",
            10 + minutes / 60,
            minutes % 60
        ));
        engine.workspace().write_photo(&sidecar).unwrap();
    }
    engine.submit_and_wait(Command::Rebuild).unwrap();
    let ids = rows.iter().map(|r| r.id).collect();
    Library {
        engine,
        ids,
        _dir: dir,
    }
}

/// Starts the thumbnail service on the library and waits until every photo has its hash.
fn hashed(library: &Library) -> auroraw_engine::ThumbnailService {
    let service = library
        .engine
        .start_thumbnails(&library._dir.path().join("previews.db"), 2)
        .unwrap();
    assert_eq!(
        service.warm_unhashed(),
        PHOTOS.len(),
        "a rebuilt catalogue has no hash yet"
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while library.engine.unhashed_count().unwrap() > 0 {
        assert!(Instant::now() < deadline, "the hashes never arrived");
        std::thread::sleep(Duration::from_millis(20));
    }
    service
}

#[test]
fn the_workers_hash_every_thumbnail_and_the_engine_offers_the_photos_that_look_alike_nearest_first()
{
    let library = library();
    let _service = hashed(&library);
    let ids = &library.ids;

    let found = library
        .engine
        .similar_photos(ids[0], SimilarQuery::default())
        .unwrap();
    let listed: Vec<PhotoId> = found.iter().map(|p| p.id).collect();
    // The same scene a few minutes apart, brightened: p1, p2 and p6 (p6 is not settled yet). Not the other scenes,
    // and not p5, hours away.
    assert!(
        listed.contains(&ids[1]) && listed.contains(&ids[2]) && listed.contains(&ids[6]),
        "{found:?}"
    );
    assert!(
        !listed.contains(&ids[3]) && !listed.contains(&ids[4]),
        "unrelated scenes: {found:?}"
    );
    assert!(!listed.contains(&ids[5]), "hours apart: {found:?}");
    assert!(!listed.contains(&ids[0]), "not itself");
    assert!(
        found.windows(2).all(|w| w[0].distance <= w[1].distance),
        "nearest first"
    );

    // A wider window brings in the photo from hours later; a distance of 0 keeps only identical hashes.
    let wide = SimilarQuery {
        window_secs: 6 * 3600,
        ..SimilarQuery::default()
    };
    let with_late = library.engine.similar_photos(ids[0], wide).unwrap();
    assert!(with_late.iter().any(|p| p.id == ids[5]));
    let exact = SimilarQuery {
        max_distance: 0,
        ..wide
    };
    let identical = library.engine.similar_photos(ids[0], exact).unwrap();
    assert!(identical.iter().all(|p| p.distance == 0));
    assert!(
        identical.iter().any(|p| p.id == ids[5]),
        "the unbrightened one, hours later, hashes the same"
    );
}

#[test]
fn a_photo_of_a_resolved_series_is_not_offered_and_one_without_a_hash_offers_nothing() {
    let library = library();
    let ids = library.ids.clone();
    // Nothing is hashed before the service has run: no suggestion, and every photo is waiting.
    assert_eq!(library.engine.unhashed_count().unwrap(), PHOTOS.len());
    assert!(
        library
            .engine
            .similar_photos(ids[0], SimilarQuery::default())
            .unwrap()
            .is_empty()
    );
    let _service = hashed(&library);
    // p6 and p4 form a series and it is resolved: p6 is settled, so it is no longer offered for p0.
    let Outcome::SeriesGrouped(series) = library
        .engine
        .submit_and_wait(Command::GroupPhotos {
            photos: vec![ids[6], ids[4]],
        })
        .unwrap()
    else {
        panic!("expected SeriesGrouped");
    };
    library
        .engine
        .submit_and_wait(Command::ResolveSeries {
            series,
            keep: vec![ids[6]],
        })
        .unwrap();
    let found = library
        .engine
        .similar_photos(ids[0], SimilarQuery::default())
        .unwrap();
    assert!(!found.iter().any(|p| p.id == ids[6]), "{found:?}");
    assert!(found.iter().any(|p| p.id == ids[1]));
}
