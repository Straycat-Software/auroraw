// SPDX-License-Identifier: GPL-3.0-or-later
//! Machines for the QML suites: a folder standing for a person's computer (data, cache and Pictures
//! folders), empty or with a workspace of generated photos and a folder of more to add.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use auroraw_engine::{AddSourceRequest, Command, Engine, Event, LocalDirs};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

/// Writes `count` small distinct JPEGs named `<prefix>_NNNN.jpg` in `folder`. Several suites call this more than
/// once, on different folders, for one workspace (a source, then another to add): the image's content is seeded
/// from the folder's own path as well as the index, so two calls never produce byte-identical files even when their
/// indices overlap (a single scalar byte driving the whole picture, tried here once, is not enough — JPEG's lossy
/// quantization collapsed a few of them to identical files in practice, which D-108's duplicate detection then
/// correctly, but confusingly for a test expecting distinct photos, joined into one).
pub fn write_photos(folder: &Path, prefix: &str, count: u32) {
    std::fs::create_dir_all(folder).unwrap();
    let folder_seed = folder.to_string_lossy().bytes().fold(0u64, |acc, b| {
        acc.wrapping_mul(131).wrapping_add(u64::from(b))
    });
    for n in 0..count {
        // 64 pseudo-random bytes (an 8x8 grid of blocks), not one: the picture's content varies enough that two
        // different (folder, index) pairs are not going to collide after JPEG's lossy encoding.
        let mut state = folder_seed
            .wrapping_add(u64::from(n))
            .wrapping_mul(2_654_435_761)
            .wrapping_add(12_345);
        let mut blocks = [0u8; 64];
        for b in &mut blocks {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            *b = (state >> 56) as u8;
        }
        let img = ImageBuffer::from_fn(160, 120, |x, y| {
            let block = blocks[((y * 8 / 120) * 8 + (x * 8 / 160)) as usize];
            Rgb([
                (x as u8).wrapping_mul(2) ^ block,
                (y as u8).wrapping_mul(2),
                block,
            ])
        });
        DynamicImage::ImageRgb8(img)
            .save_with_format(
                folder.join(format!("{prefix}_{n:04}.jpg")),
                ImageFormat::Jpeg,
            )
            .unwrap();
    }
}

/// A machine that has one workspace, "Main", with a source of `photos` photos scanned into it.
pub fn machine_with_photos(home: &Path, photos: u32) {
    let dirs = LocalDirs {
        data: home.join("data"),
        cache: home.join("cache"),
    };
    let card = home.join("Card");
    write_photos(&card, "IMG", photos);
    let root: PathBuf = home.join("Pictures").join("Auroraw").join("Main");
    let opened = Engine::create_workspace(&root, "Main", &dirs).unwrap();
    let added = opened
        .engine
        .add_source(AddSourceRequest {
            root: card,
            name: Some("Card".into()),
            merge: false,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        match opened.events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::IndexFinished { job, .. }) if job == added.job => break,
            _ => assert!(Instant::now() < deadline, "the scan never finished"),
        }
    }
}

/// [`machine_with_photos`] whose photos were taken at known times by one camera: the first `burst` of them (in the
/// grid's order, oldest first) one second apart, the next three a bracket (three exposure times), the rest a
/// minute apart. The capture times are written into the sidecars and the catalogue rebuilt, then series are
/// detected, so the library has one burst, one bracket and loners. (The generated JPEGs carry no EXIF of their own.)
pub fn machine_with_series(home: &Path, photos: u32, burst: u32) {
    let dirs = LocalDirs {
        data: home.join("data"),
        cache: home.join("cache"),
    };
    let card = home.join("Card");
    write_photos(&card, "IMG", photos);
    let root: PathBuf = home.join("Pictures").join("Auroraw").join("Main");
    let opened = Engine::create_workspace(&root, "Main", &dirs).unwrap();
    let added = opened
        .engine
        .add_source(AddSourceRequest {
            root: card,
            name: Some("Card".into()),
            merge: false,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        match opened.events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::IndexFinished { job, .. }) if job == added.job => break,
            _ => assert!(Instant::now() < deadline, "the scan never finished"),
        }
    }
    // The photos by file name (IMG_0000, IMG_0001...), and when each was taken.
    let mut rows = opened
        .engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 10_000)
        .unwrap();
    rows.sort_by(|a, b| a.filename.cmp(&b.filename));
    for (i, row) in rows.iter().enumerate() {
        let i = i as u32;
        let seconds = if i < burst {
            i
        } else if i < burst + 3 {
            600 + (i - burst)
        } else {
            1200 + (i - burst - 3) * 60
        };
        let mut sidecar = opened
            .engine
            .workspace()
            .read_photo(&row.id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        let o = &mut sidecar.meta.original;
        o.capture_time = Some(format!(
            "2026-03-01T{:02}:{:02}:{:02}Z",
            10 + seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        ));
        o.make = Some("Test".into());
        o.model = Some("Cam".into());
        o.f_number = Some("56/10".into());
        o.iso = vec!["100".into()];
        o.focal_length = Some("50/1".into());
        o.exposure_time = Some(match i.checked_sub(burst) {
            Some(k) if k < 3 => ["1/500", "1/125", "1/30"][k as usize].to_string(),
            _ => "1/250".to_string(),
        });
        opened.engine.workspace().write_photo(&sidecar).unwrap();
    }
    opened.engine.submit_and_wait(Command::Rebuild).unwrap();
    opened
        .engine
        .submit_and_wait(Command::DetectSeries { regroup: false })
        .unwrap();
}

/// A picture of 8 by 8 blocks of pseudo-random greys: two seeds are two unrelated scenes, and `lift` brightens one
/// (a photo of the same scene a little later, its hash a few bits away).
fn write_scene(path: &Path, seed: u32, lift: u8) {
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(12_345);
    let mut blocks = Vec::new();
    for _ in 0..64 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        blocks.push(((state >> 24) as u8) / 2 + 40);
    }
    let img = ImageBuffer::from_fn(320, 240, |x, y| {
        let v = blocks[((y * 8 / 240) * 8 + (x * 8 / 320)) as usize];
        Rgb([v.saturating_add(lift); 3])
    });
    DynamicImage::ImageRgb8(img)
        .save_with_format(path, ImageFormat::Jpeg)
        .unwrap();
}

/// A machine whose workspace holds `photos` photos (IMG_0000...): the first four show one scene, brightened a little
/// more each time, taken at 10:00, 10:10, 10:20 and 10:55 (so with the default half hour the first three look alike and
/// the fourth does not yet); the others are unrelated scenes, one a minute apart from 11:00, all one camera at a slow
/// pace (no series forms). The times are written into the sidecars and the catalogue rebuilt, so no photo has a hash
/// until the application's thumbnail workers make it.
pub fn machine_with_similar(home: &Path, photos: u32) {
    let dirs = LocalDirs {
        data: home.join("data"),
        cache: home.join("cache"),
    };
    let card = home.join("Card");
    std::fs::create_dir_all(&card).unwrap();
    for n in 0..photos {
        let (seed, lift) = if n < 4 {
            (1, (n * 6) as u8)
        } else {
            (10 + n, 0)
        };
        write_scene(&card.join(format!("IMG_{n:04}.jpg")), seed, lift);
    }
    let root: PathBuf = home.join("Pictures").join("Auroraw").join("Main");
    let opened = Engine::create_workspace(&root, "Main", &dirs).unwrap();
    let added = opened
        .engine
        .add_source(AddSourceRequest {
            root: card,
            name: Some("Card".into()),
            merge: false,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        match opened.events.recv_timeout(Duration::from_millis(200)) {
            Some(Event::IndexFinished { job, .. }) if job == added.job => break,
            _ => assert!(Instant::now() < deadline, "the scan never finished"),
        }
    }
    let mut rows = opened
        .engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 10_000)
        .unwrap();
    rows.sort_by(|a, b| a.filename.cmp(&b.filename));
    for (i, row) in rows.iter().enumerate() {
        let i = i as u32;
        let minutes = match i {
            0 => 0,
            1 => 10,
            2 => 20,
            3 => 55,
            _ => 60 + (i - 4),
        };
        let mut sidecar = opened
            .engine
            .workspace()
            .read_photo(&row.id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        let o = &mut sidecar.meta.original;
        o.capture_time = Some(format!(
            "2026-03-01T{:02}:{:02}:00Z",
            10 + minutes / 60,
            minutes % 60
        ));
        o.make = Some("Test".into());
        o.model = Some("Cam".into());
        opened.engine.workspace().write_photo(&sidecar).unwrap();
    }
    opened.engine.submit_and_wait(Command::Rebuild).unwrap();
    opened
        .engine
        .submit_and_wait(Command::DetectSeries { regroup: false })
        .unwrap();
}

/// [`machine_with_photos`] with an extra "Backup" source holding an exact copy of `IMG_0000.jpg` (WP9, D-036,
/// D-108): one photo ends up with two confirmed locations, "Card" (its primary) and "Backup".
pub fn machine_with_duplicate(home: &Path, photos: u32) {
    let dirs = LocalDirs {
        data: home.join("data"),
        cache: home.join("cache"),
    };
    let card = home.join("Card");
    write_photos(&card, "IMG", photos);
    let backup = home.join("Backup");
    std::fs::create_dir_all(&backup).unwrap();
    std::fs::copy(card.join("IMG_0000.jpg"), backup.join("IMG_0000_copy.jpg")).unwrap();

    let root: PathBuf = home.join("Pictures").join("Auroraw").join("Main");
    let opened = Engine::create_workspace(&root, "Main", &dirs).unwrap();
    for (name, folder) in [("Card", card), ("Backup", backup)] {
        let added = opened
            .engine
            .add_source(AddSourceRequest {
                root: folder,
                name: Some(name.into()),
                merge: false,
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            match opened.events.recv_timeout(Duration::from_millis(200)) {
                Some(Event::IndexFinished { job, .. }) if job == added.job => break,
                _ => assert!(Instant::now() < deadline, "the scan never finished"),
            }
        }
    }
}

/// Decimal degrees in XMP's text form (`"5,30.0000N"`), as a sidecar holds a position.
fn xmp_degrees(value: f64, positive: char, negative: char) -> String {
    let magnitude = value.abs();
    let hemisphere = if value < 0.0 { negative } else { positive };
    format!(
        "{},{:.4}{hemisphere}",
        magnitude.trunc(),
        magnitude.fract() * 60.0
    )
}

/// The places file of the suites about place names: "Aland", a country of 10 by 10 degrees at the origin cut into
/// two regions along the longitude 5 (West, Ouest in French; East, Est), a town in each: Westville (5, 2) and
/// Eastburg (5, 8). Anything outside is open water. No download is involved (design note 008 §2, testing strategy).
pub fn write_places_pack(path: &Path) {
    use auroraw_places::{Level, NewArea, NewPlace, PackBuilder};
    type Ring = Vec<(f64, f64)>;
    let square = |x0: f64, x1: f64, y0: f64, y1: f64| -> Ring {
        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)]
    };
    let area = |level: Level, code: &str, en: &str, fr: &str, ring: Ring| NewArea {
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
        parts: vec![vec![ring]],
    };
    let town = |id: i64, name: &str, lat: f64, lon: f64| NewPlace {
        id,
        name: name.into(),
        lat,
        lon,
        country_code: "AA".into(),
        population: 5_000,
        section: false,
    };
    let mut pack = PackBuilder::create(path).unwrap();
    let aland = pack
        .add_area(&area(
            Level::Country,
            "AA",
            "Aland",
            "Alandie",
            square(0.0, 10.0, 0.0, 10.0),
        ))
        .unwrap();
    let mut west = area(
        Level::Region,
        "AA-W",
        "West",
        "Ouest",
        square(0.0, 5.0, 0.0, 10.0),
    );
    west.parent = Some(aland);
    pack.add_area(&west).unwrap();
    let mut east = area(
        Level::Region,
        "AA-E",
        "East",
        "Est",
        square(5.0, 10.0, 0.0, 10.0),
    );
    east.parent = Some(aland);
    pack.add_area(&east).unwrap();
    pack.add_place(&town(1, "Westville", 5.0, 2.0)).unwrap();
    pack.add_place(&town(2, "Eastburg", 5.0, 8.0)).unwrap();
    pack.finish().unwrap();
}

/// [`machine_with_photos`] with a places file at `<home>/places.sqlite` (the suites point `AURORAW_PLACES` at it) and
/// positions in the sidecars, by file name: `IMG_0000`, `IMG_0001`, `IMG_0006` and `IMG_0007` in the West of Aland,
/// `IMG_0002` and `IMG_0008` in the East, `IMG_0003` in open water, `IMG_0004` in the West with a city a person typed
/// ("Mine"); the others have no position.
pub fn machine_with_places(home: &Path, photos: u32) {
    machine_with_photos(home, photos);
    write_places_pack(&home.join("places.sqlite"));
    let dirs = LocalDirs {
        data: home.join("data"),
        cache: home.join("cache"),
    };
    let root: PathBuf = home.join("Pictures").join("Auroraw").join("Main");
    let opened = Engine::open_workspace(&root, &dirs).unwrap();
    let mut rows = opened
        .engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 10_000)
        .unwrap();
    rows.sort_by(|a, b| a.filename.cmp(&b.filename));
    let places: [(usize, f64, f64); 8] = [
        (0, 5.0, 2.1),
        (1, 5.0, 2.1),
        (2, 5.0, 8.1),
        (3, -40.0, 100.0),
        (4, 5.0, 2.1),
        (6, 5.0, 2.1),
        (7, 5.0, 2.1),
        (8, 5.0, 8.1),
    ];
    for (index, lat, lon) in places {
        let Some(row) = rows.get(index) else { continue };
        let mut sidecar = opened
            .engine
            .workspace()
            .read_photo(&row.id)
            .unwrap()
            .unwrap()
            .current()
            .unwrap();
        sidecar.meta.original.gps_latitude = Some(xmp_degrees(lat, 'N', 'S'));
        sidecar.meta.original.gps_longitude = Some(xmp_degrees(lon, 'E', 'W'));
        if index == 4 {
            sidecar.meta.city = Some("Mine".into());
        }
        opened.engine.workspace().write_photo(&sidecar).unwrap();
    }
    opened.engine.submit_and_wait(Command::Rebuild).unwrap();
}

/// Puts a GPS position into the EXIF of the JPEG at `path`, as a camera with a receiver would have, so that an import
/// reads it from the file.
pub fn put_gps_in_jpeg(path: &Path, lat: f64, lon: f64) {
    let degrees = |value: f64| {
        let magnitude = value.abs();
        let d = magnitude.trunc();
        let minutes = (magnitude - d) * 60.0;
        exif::Value::Rational(vec![
            exif::Rational {
                num: d as u32,
                denom: 1,
            },
            exif::Rational {
                num: (minutes * 10_000.0).round() as u32,
                denom: 10_000,
            },
            exif::Rational { num: 0, denom: 1 },
        ])
    };
    let ascii = |text: &str| exif::Value::Ascii(vec![text.as_bytes().to_vec()]);
    let fields = [
        (
            exif::Tag::GPSLatitudeRef,
            ascii(if lat < 0.0 { "S" } else { "N" }),
        ),
        (exif::Tag::GPSLatitude, degrees(lat)),
        (
            exif::Tag::GPSLongitudeRef,
            ascii(if lon < 0.0 { "W" } else { "E" }),
        ),
        (exif::Tag::GPSLongitude, degrees(lon)),
    ]
    .map(|(tag, value)| exif::Field {
        tag,
        ifd_num: exif::In::PRIMARY,
        value,
    });
    let mut writer = exif::experimental::Writer::new();
    for field in &fields {
        writer.push_field(field);
    }
    let mut tiff = std::io::Cursor::new(Vec::new());
    writer.write(&mut tiff, false).unwrap();
    let tiff = tiff.into_inner();
    let jpeg = std::fs::read(path).unwrap();
    assert_eq!(&jpeg[..2], &[0xFF, 0xD8], "a JPEG");
    // APP1: the marker, a length that counts itself, "Exif" and two zeros, the TIFF block.
    let mut segment = vec![0xFF, 0xE1];
    segment.extend(((tiff.len() + 8) as u16).to_be_bytes());
    segment.extend(b"Exif\0\0");
    segment.extend(&tiff);
    let mut out = jpeg[..2].to_vec();
    out.extend(segment);
    out.extend(&jpeg[2..]);
    std::fs::write(path, out).unwrap();
}
