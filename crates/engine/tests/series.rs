// SPDX-License-Identifier: GPL-3.0-or-later
//! Series (WP9, D-101): photos of one camera taken close together form series by themselves when photos arrive
//! (and on demand), a series can be made, taken apart and resolved by hand, each of those a step of the history,
//! resolving keeps the picked and rejects the rest and undoing it gives every flag and rating back, and the
//! series (and their resolution) survive a rebuild.

use std::path::Path;
use std::time::{Duration, Instant};

use auroraw_catalogue::{Catalogue, SeriesFilter};
use auroraw_engine::{
    AddSourceRequest, Command, Engine, EngineError, Event, EventReceiver, LabelKind, Outcome,
};
use auroraw_format::sidecar::{Flag, PhotoSidecar};
use auroraw_testkit::{TempDir, temp_dir};
use auroraw_types::{PhotoId, SeriesId};
use auroraw_workspace::Workspace;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

struct Fixture {
    _dir: TempDir,
    engine: Engine,
    events: EventReceiver,
    photos: Vec<PhotoId>,
}

/// A photo taken `seconds` after 10:00:00 on one day by `camera`, with this exposure time (`None`: the exposure is
/// not written).
struct Shot {
    seconds: u32,
    camera: &'static str,
    exposure: Option<&'static str>,
}

const fn shot(seconds: u32) -> Shot {
    Shot {
        seconds,
        camera: "Cam A",
        exposure: Some("1/250"),
    }
}

fn fixture(shots: &[Shot]) -> Fixture {
    let dir = temp_dir();
    let root = dir.path().join("W");
    let catalogue = dir.path().join("catalogue.sqlite");
    let ws = Workspace::create(&root, "Main").unwrap();
    let mut photos = Vec::new();
    for (i, s) in shots.iter().enumerate() {
        let mut photo = PhotoSidecar::new(PhotoId::random());
        let o = &mut photo.meta.original;
        o.capture_time = Some(format!(
            "2026-03-01T10:{:02}:{:02}Z",
            s.seconds / 60,
            s.seconds % 60
        ));
        o.make = Some("Test".into());
        o.model = Some(s.camera.into());
        o.exposure_time = s.exposure.map(str::to_string);
        o.f_number = Some("56/10".into());
        o.iso = vec!["100".into()];
        o.focal_length = Some("50/1".into());
        photo.meta.rating = Some((i % 5) as u8 + 1);
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
        _dir: dir,
        engine,
        events,
        photos,
    }
}

impl Fixture {
    fn detect(&self, regroup: bool) -> (usize, usize) {
        let Outcome::SeriesDetected { series, photos } = self
            .engine
            .submit_and_wait(Command::DetectSeries { regroup })
            .unwrap()
        else {
            panic!("expected SeriesDetected");
        };
        (series, photos)
    }

    /// Each series as (kind, resolved, the indices of its photos in the fixture), sorted.
    fn series(&self) -> Vec<(String, bool, Vec<usize>)> {
        let cat = self.engine.read_catalogue().unwrap();
        let mut out: Vec<_> = cat
            .series_infos()
            .unwrap()
            .into_iter()
            .map(|info| {
                let mut members: Vec<usize> = cat
                    .series_members(&info.id)
                    .unwrap()
                    .iter()
                    .map(|m| self.photos.iter().position(|p| p == m).unwrap())
                    .collect();
                members.sort_unstable();
                (info.kind, info.resolved, members)
            })
            .collect();
        out.sort_by_key(|s| s.2.clone());
        out
    }

    fn series_id_of(&self, photo: usize) -> Option<SeriesId> {
        self.engine
            .read_catalogue()
            .unwrap()
            .photo(&self.photos[photo])
            .unwrap()
            .unwrap()
            .series_id
    }

    fn flag(&self, photo: usize) -> u8 {
        self.engine
            .read_catalogue()
            .unwrap()
            .photo(&self.photos[photo])
            .unwrap()
            .unwrap()
            .flag
    }

    fn rating(&self, photo: usize) -> u8 {
        self.engine
            .read_catalogue()
            .unwrap()
            .photo(&self.photos[photo])
            .unwrap()
            .unwrap()
            .rating
    }

    fn undo_label(&self) -> Option<auroraw_engine::Label> {
        self.events
            .drain()
            .into_iter()
            .filter_map(|e| match e {
                Event::HistoryChanged(state) => Some(state.undo),
                _ => None,
            })
            .next_back()
            .flatten()
    }

    fn set_flag(&self, photo: usize, flag: Option<Flag>) {
        self.engine
            .submit_and_wait(Command::SetFlag {
                photo_id: self.photos[photo],
                flag,
            })
            .unwrap();
    }
}

/// Two bursts (five and two frames), a bracket of three, and loners: at 0 to 4 s, 60 and 61, 120 to 122 (three
/// exposure times), and photos at 300, 500 and (another camera) 0.
fn shots() -> Vec<Shot> {
    let mut v: Vec<Shot> = (0..5).map(shot).collect(); // 0..=4: a burst
    v.push(shot(60));
    v.push(shot(61)); // 5, 6: a burst of two
    for (i, exposure) in ["1/500", "1/125", "1/30"].into_iter().enumerate() {
        v.push(Shot {
            seconds: 120 + i as u32,
            camera: "Cam A",
            exposure: Some(exposure),
        }); // 7, 8, 9: a bracket
    }
    v.push(shot(300)); // 10
    v.push(shot(500)); // 11
    v.push(Shot {
        seconds: 1,
        camera: "Cam B",
        exposure: Some("1/250"),
    }); // 12: same second as the burst, another camera
    v
}

#[test]
fn photos_of_one_camera_close_together_form_series_and_the_gap_can_change() {
    let f = fixture(&shots());
    assert_eq!(f.detect(false), (3, 10));
    assert_eq!(
        f.series(),
        vec![
            ("burst".to_string(), false, vec![0, 1, 2, 3, 4]),
            ("burst".to_string(), false, vec![5, 6]),
            ("bracket".to_string(), false, vec![7, 8, 9]),
        ]
    );
    assert_eq!(f.series_id_of(10), None, "a loner is in none");
    assert_eq!(
        f.series_id_of(12),
        None,
        "another camera's photo is not the burst's"
    );
    // Detecting again forms nothing new.
    assert_eq!(f.detect(false), (0, 0));
    // A wider gap, applied by regrouping: with 60 s, hops of 56, 59 and 59 s join everything of camera A up to 122 s.
    f.engine
        .submit_and_wait(Command::SetSeriesGap { seconds: 60 })
        .unwrap();
    f.detect(true);
    let joined = f.series();
    assert_eq!(
        joined.len(),
        1,
        "everything of camera A up to 500 s is within 60 s hops? {joined:?}"
    );
}

#[test]
fn a_series_made_by_hand_is_a_step_and_undo_puts_the_photos_back_where_they_were() {
    let f = fixture(&shots());
    f.detect(false);
    // Group photos of two series and a loner: they leave their series, one series is left with two.
    let Outcome::SeriesGrouped(new) = f
        .engine
        .submit_and_wait(Command::GroupPhotos {
            photos: vec![f.photos[3], f.photos[4], f.photos[5], f.photos[10]],
        })
        .unwrap()
    else {
        panic!("expected SeriesGrouped");
    };
    let label = f.undo_label().unwrap();
    assert_eq!((label.kind, label.count), (LabelKind::SeriesGroup, 4));
    assert_eq!(
        f.series(),
        vec![
            ("burst".to_string(), false, vec![0, 1, 2]),
            ("manual".to_string(), false, vec![3, 4, 5, 10]),
            ("bracket".to_string(), false, vec![7, 8, 9]),
        ],
        "the burst was left with three, the pair was dissolved (photo 6 is in none)"
    );
    assert_eq!(f.series_id_of(3), Some(new));
    assert_eq!(f.series_id_of(6), None);

    f.engine.undo().unwrap();
    assert_eq!(
        f.series(),
        vec![
            ("burst".to_string(), false, vec![0, 1, 2, 3, 4]),
            ("burst".to_string(), false, vec![5, 6]),
            ("bracket".to_string(), false, vec![7, 8, 9]),
        ]
    );
    assert_eq!(f.series_id_of(10), None);
    f.engine.redo().unwrap();
    assert_eq!(f.series_id_of(10), Some(new));

    // Taking a photo out, dissolving, and one photo is not a series.
    f.engine
        .submit_and_wait(Command::RemoveFromSeries {
            photos: vec![f.photos[10]],
        })
        .unwrap();
    assert_eq!(f.series_id_of(10), None);
    f.engine
        .submit_and_wait(Command::DissolveSeries { series: new })
        .unwrap();
    assert_eq!(f.series_id_of(3), None);
    f.engine.undo().unwrap();
    assert_eq!(f.series_id_of(3), Some(new));
    let refused = f.engine.submit_and_wait(Command::GroupPhotos {
        photos: vec![f.photos[0]],
    });
    assert!(matches!(refused, Err(EngineError::InvalidCommand(_))));
}

#[test]
fn resolving_picks_the_kept_and_rejects_the_rest_and_undo_gives_every_flag_and_rating_back() {
    let f = fixture(&shots());
    f.detect(false);
    let series = f.series_id_of(0).unwrap();
    // Some flags are there already: a picked frame that will be rejected, a rejected one that will be kept.
    f.set_flag(1, Some(Flag::Picked));
    f.set_flag(2, Some(Flag::Rejected));
    let flags_before: Vec<u8> = (0..5).map(|i| f.flag(i)).collect();
    let ratings_before: Vec<u8> = (0..5).map(|i| f.rating(i)).collect();
    f.events.drain();

    f.engine
        .submit_and_wait(Command::ResolveSeries {
            series,
            keep: vec![f.photos[2], f.photos[4]],
        })
        .unwrap();
    let label = f.undo_label().unwrap();
    assert_eq!((label.kind, label.count), (LabelKind::SeriesResolve, 5));
    assert_eq!(
        (0..5).map(|i| f.flag(i)).collect::<Vec<_>>(),
        [2, 2, 1, 2, 1],
        "kept: picked, the others: rejected"
    );
    assert_eq!(
        f.series()[0],
        ("burst".to_string(), true, vec![0, 1, 2, 3, 4])
    );
    // (A connection that is still open would keep Windows from replacing the catalogue in the rebuild below.)
    let resolved: Vec<_> = f
        .engine
        .read_catalogue()
        .unwrap()
        .list_filtered(
            &auroraw_catalogue::Filter {
                flags: auroraw_catalogue::FlagFilter::All,
                series: SeriesFilter::Resolved,
                ..Default::default()
            },
            None,
            100,
        )
        .unwrap();
    assert_eq!(
        resolved.len(),
        5,
        "the filter on resolved series lists its photos"
    );

    // One step undoes it all: flags, ratings, the series' state.
    f.engine.undo().unwrap();
    assert_eq!((0..5).map(|i| f.flag(i)).collect::<Vec<_>>(), flags_before);
    assert_eq!(
        (0..5).map(|i| f.rating(i)).collect::<Vec<_>>(),
        ratings_before
    );
    assert!(!f.series()[0].1);
    f.engine.redo().unwrap();
    assert_eq!(
        (0..5).map(|i| f.flag(i)).collect::<Vec<_>>(),
        [2, 2, 1, 2, 1]
    );

    // A series survives a rebuild from the workspace, resolved and all.
    let before = f.series();
    f.engine.submit_and_wait(Command::Rebuild).unwrap();
    assert_eq!(
        f.series(),
        before,
        "the series and their resolution survive a rebuild"
    );

    // Reopening leaves the flags; nothing to keep is refused.
    f.engine
        .submit_and_wait(Command::ReopenSeries { series })
        .unwrap();
    assert!(!f.series()[0].1);
    assert_eq!(f.flag(0), 2);
    let none_kept = f.engine.submit_and_wait(Command::ResolveSeries {
        series,
        keep: vec![f.photos[9]],
    });
    assert!(matches!(none_kept, Err(EngineError::InvalidCommand(_))));
}

#[test]
fn regrouping_keeps_what_was_made_by_hand_or_resolved() {
    let f = fixture(&shots());
    f.detect(false);
    let burst = f.series_id_of(0).unwrap();
    f.engine
        .submit_and_wait(Command::ResolveSeries {
            series: burst,
            keep: vec![f.photos[0]],
        })
        .unwrap();
    f.engine
        .submit_and_wait(Command::GroupPhotos {
            photos: vec![f.photos[10], f.photos[11]],
        })
        .unwrap();
    f.engine
        .submit_and_wait(Command::SetSeriesGap { seconds: 0 })
        .unwrap();
    f.detect(true);
    let kinds: Vec<_> = f.series().into_iter().map(|s| (s.0, s.1, s.2)).collect();
    assert!(
        kinds.contains(&("burst".to_string(), true, vec![0, 1, 2, 3, 4])),
        "resolved stays: {kinds:?}"
    );
    assert!(
        kinds.contains(&("manual".to_string(), false, vec![10, 11])),
        "manual stays: {kinds:?}"
    );
    assert!(
        !kinds.iter().any(|s| s.2 == vec![5, 6]),
        "the pair 60 and 61 s apart is not one at a gap of 0: {kinds:?}"
    );
}

fn jpeg(path: &Path, seed: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let img = ImageBuffer::from_fn(32, 24, |x, y| {
        Rgb([(x * 8) as u8 ^ seed, (y * 9) as u8, seed])
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

#[test]
fn a_series_shrinks_when_photos_leave_with_their_source_and_goes_with_the_last() {
    let dir = temp_dir();
    let (engine, events) = Engine::create(
        &dir.path().join("Main"),
        &dir.path().join("main.sqlite"),
        "Main",
    )
    .unwrap();
    let mut sources = Vec::new();
    for (name, count) in [("A", 3u8), ("B", 2u8)] {
        let folder = dir.path().join(name);
        for i in 0..count {
            jpeg(
                &folder.join(format!("{name}{i}.jpg")),
                i + name.as_bytes()[0],
            );
        }
        let added = engine
            .add_source(AddSourceRequest {
                root: folder,
                name: None,
                merge: false,
            })
            .unwrap();
        wait_for(
            &events,
            |e| matches!(e, Event::IndexFinished { job, .. } if *job == added.job),
        );
        sources.push(added.source_id);
    }
    let all: Vec<PhotoId> = engine
        .read_catalogue()
        .unwrap()
        .list_recent(None, 100)
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(all.len(), 5);
    let Outcome::SeriesGrouped(series) = engine
        .submit_and_wait(Command::GroupPhotos {
            photos: all.clone(),
        })
        .unwrap()
    else {
        panic!()
    };
    let members = |engine: &Engine| {
        engine
            .read_catalogue()
            .unwrap()
            .series_members(&series)
            .unwrap()
            .len()
    };
    assert_eq!(members(&engine), 5);
    for (n, source) in sources.iter().enumerate() {
        let Outcome::RemoveStarted { job } = engine
            .submit_and_wait(Command::RemoveSource { source_id: *source })
            .unwrap()
        else {
            panic!()
        };
        wait_for(
            &events,
            |e| matches!(e, Event::SourceRemoved { job: j, .. } if *j == job),
        );
        if n == 0 {
            assert_eq!(members(&engine), 2, "the photos of A left the series");
        }
    }
    assert!(
        engine
            .read_catalogue()
            .unwrap()
            .series_infos()
            .unwrap()
            .is_empty(),
        "with fewer than two photos left the series is dissolved"
    );
    assert!(
        engine.workspace().read_series(&series).unwrap().is_none(),
        "and its file is gone"
    );
}
