// SPDX-License-Identifier: GPL-3.0-or-later
//! Machines for the QML suites: a folder standing for a person's computer (data, cache and Pictures
//! folders), empty or with a workspace of generated photos and a folder of more to add.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use auroraw_engine::{AddSourceRequest, Command, Engine, Event, LocalDirs};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

/// Writes `count` small distinct JPEGs named `<prefix>_NNNN.jpg` in `folder`.
pub fn write_photos(folder: &Path, prefix: &str, count: u32) {
    std::fs::create_dir_all(folder).unwrap();
    for n in 0..count {
        let img = ImageBuffer::from_fn(160, 120, |x, y| {
            Rgb([
                (x as u8).wrapping_mul(2) ^ (n as u8).wrapping_mul(37),
                (y as u8).wrapping_mul(2),
                (n as u8).wrapping_mul(11),
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
