// SPDX-License-Identifier: GPL-3.0-or-later
//! The image view's service (D-100): the photo asked for is delivered upright at its size, a picture that is
//! kept is answered at once (even if the original went away), a missing original is reported as failed, the
//! photos said to be next are made ahead, and no more than the cache's capacity is kept.

use std::path::Path;
use std::time::{Duration, Instant};

use auroraw_engine::{
    AddSourceRequest, Engine, Event, EventReceiver, PREVIEW_CACHE, PreviewService,
};
use auroraw_testkit::{TempDir, temp_dir};
use auroraw_types::PhotoId;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

struct Setup {
    dir: TempDir,
    engine: Engine,
    photos: Vec<PhotoId>,
    folder: std::path::PathBuf,
}

fn jpeg(path: &Path, seed: u8, width: u32, height: u32) {
    let img = ImageBuffer::from_fn(width, height, |x, y| {
        Rgb([(x % 251) as u8 ^ seed, (y % 241) as u8, seed])
    });
    DynamicImage::ImageRgb8(img)
        .save_with_format(path, ImageFormat::Jpeg)
        .unwrap();
}

fn wait_for(events: &EventReceiver, wanted: impl Fn(&Event) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match events.recv_timeout(Duration::from_millis(200)) {
            Some(event) if wanted(&event) => return,
            Some(_) => {}
            None => assert!(Instant::now() < deadline, "the event never came"),
        }
    }
}

/// A catalogue of `count` 300 x 200 photos in one folder.
fn setup(count: usize) -> Setup {
    let dir = temp_dir();
    let folder = dir.path().join("Trip");
    std::fs::create_dir_all(&folder).unwrap();
    for i in 0..count {
        jpeg(&folder.join(format!("p{i:04}.jpg")), i as u8, 300, 200);
    }
    let (engine, events) = Engine::create(
        &dir.path().join("Main"),
        &dir.path().join("main.sqlite"),
        "Main",
    )
    .unwrap();
    let added = engine
        .add_source(AddSourceRequest {
            root: folder.clone(),
            name: None,
            merge: false,
        })
        .unwrap();
    wait_for(
        &events,
        |e| matches!(e, Event::IndexFinished { job, .. } if *job == added.job),
    );
    let photos = engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 10_000)
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    Setup {
        dir,
        engine,
        photos,
        folder,
    }
}

/// Asks for `id` and waits for the answer: the JPEG's size, or `None` when it failed.
fn ask(service: &PreviewService, id: PhotoId) -> Option<(u32, u32)> {
    service.request(id);
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Some((_, image)) = service.poll().into_iter().find(|(got, _)| *got == id) {
            return Some((image.image.width, image.image.height));
        }
        if service.poll_failed().contains(&id) {
            return None;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("no answer for {id}");
}

fn wait_until(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !condition() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_photo_is_delivered_at_its_size_and_upright_from_its_sidecar() {
    let s = setup(2);
    let service = s.engine.start_previews(2).unwrap();
    assert_eq!(ask(&service, s.photos[0]), Some((300, 200)));

    // The sidecar's orientation turns it: a portrait shot, stored sideways in the file.
    let mut sidecar = s
        .engine
        .workspace()
        .read_photo(&s.photos[1])
        .unwrap()
        .unwrap()
        .current()
        .unwrap();
    sidecar.meta.original.orientation = Some(6);
    s.engine.workspace().write_photo(&sidecar).unwrap();
    assert_eq!(ask(&service, s.photos[1]), Some((200, 300)));
}

#[test]
fn a_picture_that_is_kept_is_answered_at_once_and_a_missing_original_is_reported() {
    let s = setup(3);
    let service = s.engine.start_previews(2).unwrap();
    assert!(ask(&service, s.photos[0]).is_some());
    assert!(service.is_ready(&s.photos[0]));

    // Every original goes away: what was made is still shown, what was not is reported.
    std::fs::remove_dir_all(&s.folder).unwrap();
    service.request(s.photos[0]);
    let answered = service.poll();
    assert_eq!(
        answered.len(),
        1,
        "answered without a worker: already in the inbox"
    );
    assert_eq!(ask(&service, s.photos[1]), None);
    assert!(!service.is_ready(&s.photos[1]));
    drop(s.dir);
}

#[test]
fn the_photos_said_to_be_next_are_made_ahead_and_the_previous_list_is_forgotten() {
    let s = setup(6);
    let service = s.engine.start_previews(2).unwrap();
    service.prefetch(&s.photos[1..4]);
    wait_until("the next photos were made", || {
        s.photos[1..4].iter().all(|p| service.is_ready(p))
    });
    assert!(!service.is_ready(&s.photos[5]), "not asked for, not made");
    // Nothing was asked for, so nothing is delivered.
    assert!(service.poll().is_empty());
    // Asked for now, they arrive without waiting for a worker.
    service.request(s.photos[2]);
    assert_eq!(service.poll().len(), 1);
}

#[test]
fn no_more_than_the_capacity_is_kept_and_the_latest_stay() {
    let count = PREVIEW_CACHE + 4;
    let s = setup(count);
    let service = s.engine.start_previews(2).unwrap();
    for id in &s.photos {
        assert!(ask(&service, *id).is_some());
    }
    assert_eq!(service.kept(), PREVIEW_CACHE);
    assert!(
        service.is_ready(s.photos.last().unwrap()),
        "the newest stays"
    );
    assert!(!service.is_ready(&s.photos[0]), "the oldest went");
}

#[test]
fn walking_through_two_hundred_photos_with_the_next_ones_made_ahead_is_fast() {
    let s = setup(200);
    let service = s.engine.start_previews(2).unwrap();
    let mut times = Vec::new();
    for (i, id) in s.photos.iter().enumerate() {
        service.prefetch(&s.photos[(i + 1).min(s.photos.len())..(i + 3).min(s.photos.len())]);
        let start = Instant::now();
        assert!(ask(&service, *id).is_some());
        times.push(start.elapsed());
        // The person looks at the picture for a moment before the next key.
        std::thread::sleep(Duration::from_millis(15));
    }
    times.sort();
    let median = times[times.len() / 2];
    let worst = *times.last().unwrap();
    eprintln!("200 previews walked: median {median:?}, worst {worst:?}");
    assert!(
        median < Duration::from_millis(250),
        "a key press waited {median:?} for its picture at the median"
    );
}

/// A folder with a sharp picture (stripes) and the same picture blurred, as `a.jpg` and `b.jpg`.
fn sharp_and_soft() -> Setup {
    let dir = temp_dir();
    let folder = dir.path().join("Trip");
    std::fs::create_dir_all(&folder).unwrap();
    let sharp = ImageBuffer::from_fn(240, 160, |x, _| {
        let v = if (x / 6) % 2 == 0 { 25 } else { 230 };
        Rgb([v, v, v])
    });
    let soft = image::imageops::blur(&sharp, 3.0);
    for (name, img) in [
        ("a.jpg", sharp),
        ("b.jpg", image::imageops::blur(&soft, 0.1)),
    ] {
        DynamicImage::ImageRgb8(img)
            .save_with_format(folder.join(name), ImageFormat::Jpeg)
            .unwrap();
    }
    let (engine, events) = Engine::create(
        &dir.path().join("Main"),
        &dir.path().join("main.sqlite"),
        "Main",
    )
    .unwrap();
    let added = engine
        .add_source(AddSourceRequest {
            root: folder.clone(),
            name: None,
            merge: false,
        })
        .unwrap();
    wait_for(
        &events,
        |e| matches!(e, Event::IndexFinished { job, .. } if *job == added.job),
    );
    let mut rows = engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 10)
        .unwrap();
    rows.sort_by(|a, b| a.filename.cmp(&b.filename));
    Setup {
        dir,
        engine,
        photos: rows.into_iter().map(|r| r.id).collect(),
        folder,
    }
}

#[test]
fn frames_are_ranked_by_sharpness_without_keeping_their_pictures() {
    let s = sharp_and_soft();
    let service = s.engine.start_previews(2).unwrap();
    assert_eq!(
        service.sharpness(&s.photos[0]),
        None,
        "nothing measured yet"
    );
    service.analyse(&s.photos);
    wait_until("both were measured", || {
        s.photos.iter().all(|p| service.sharpness(p).is_some())
    });
    let (sharp, soft) = (
        service.sharpness(&s.photos[0]).unwrap(),
        service.sharpness(&s.photos[1]).unwrap(),
    );
    assert!(
        sharp > 2.0 * soft,
        "the sharp frame scores higher: {sharp} against {soft}"
    );
    assert_eq!(service.kept(), 0, "measuring keeps no picture");
    assert!(
        service.aids(&s.photos[0]).is_none(),
        "and so has no histogram to give"
    );

    // Asking for the picture gives its aids and masks, and its score is the same one.
    assert!(ask(&service, s.photos[0]).is_some());
    let aids = service.aids(&s.photos[0]).unwrap();
    assert_eq!(aids.histogram[0].iter().sum::<u32>(), 240 * 160);
    assert!(
        (service.sharpness(&s.photos[0]).unwrap() - aids.sharpness).abs()
            < 1e-3 * aids.sharpness.max(1.0)
    );
    let picture = service.poll();
    assert!(picture.is_empty(), "already polled by ask");
}

#[test]
fn an_overlay_is_made_when_asked_from_the_picture_and_lies_over_it() {
    use auroraw_engine::MaskKind;
    let s = sharp_and_soft();
    let service = s.engine.start_previews(2).unwrap();
    let lit = |png: &[u8]| {
        let mask = image::load_from_memory(png).unwrap().to_rgba8();
        assert_eq!(mask.dimensions(), (240, 160), "the picture's own size");
        mask.pixels().filter(|p| p.0[3] > 0).count()
    };
    let wait_mask = |id: PhotoId, kind: MaskKind| {
        service.request_mask(id, kind);
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if let Some((_, _, png)) = service
                .poll_masks()
                .into_iter()
                .find(|(got, k, _)| *got == id && *k == kind)
            {
                return png;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no overlay");
    };
    let sharp = wait_mask(s.photos[0], MaskKind::Peaking);
    let soft = wait_mask(s.photos[1], MaskKind::Peaking);
    let (a, b) = (lit(&sharp), lit(&soft));
    assert!(a > b, "a sharp frame lights up more: {a} against {b}");
    assert!(
        service.is_ready(&s.photos[0]),
        "the picture was made for it and is kept"
    );
    // Asked again: from what was made, at once.
    service.request_mask(s.photos[0], MaskKind::Peaking);
    assert_eq!(service.poll_masks().len(), 1);
    let clipping = wait_mask(s.photos[0], MaskKind::Clipping);
    let _ = lit(&clipping);
    // No original, no overlay.
    std::fs::remove_dir_all(&s.folder).unwrap();
    let mut asked = None;
    service.request_mask(PhotoId::random(), MaskKind::Peaking);
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && asked.is_none() {
        asked = service.poll_masks_failed().into_iter().next();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        asked.is_some(),
        "an overlay that cannot be made is reported"
    );
}
