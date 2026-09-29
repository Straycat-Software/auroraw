// SPDX-License-Identifier: GPL-3.0-or-later
//! A `Command::Batch` past `BACKGROUND_THRESHOLD` items runs as a background job instead of in one
//! synchronous, all-or-nothing loop (D-126 volet B): still one history entry, but a cancelled one keeps
//! whatever prefix already landed rather than rolling it all back, and a concurrent `CancelJob` reaches
//! it promptly instead of waiting behind the whole batch (the ack-paced sends in `batch_job::spawn`
//! exist specifically so this holds even for a very large batch).

use std::time::{Duration, Instant};

use auroraw_catalogue::Catalogue;
use auroraw_engine::{Command, Engine, Event, EventReceiver, LabelKind, Outcome};
use auroraw_format::sidecar::PhotoSidecar;
use auroraw_testkit::{TempDir, temp_dir};
use auroraw_types::PhotoId;
use auroraw_workspace::Workspace;

struct Fixture {
    _dir: TempDir,
    engine: Engine,
    events: EventReceiver,
    photos: Vec<PhotoId>,
}

/// A workspace with `photos` photos, unrated, and an empty vocabulary.
fn fixture(photos: usize) -> Fixture {
    let dir = temp_dir();
    let root = dir.path().join("W");
    let catalogue = dir.path().join("catalogue.sqlite");
    let photo_ids: Vec<PhotoId> = (0..photos).map(|_| PhotoId::random()).collect();
    let ws = Workspace::create(&root, "Main").unwrap();
    for id in &photo_ids {
        ws.write_photo(&PhotoSidecar::new(*id)).unwrap();
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
        photos: photo_ids,
    }
}

impl Fixture {
    fn rating(&self, photo: PhotoId) -> Option<u8> {
        self.engine
            .workspace()
            .read_photo(&photo)
            .unwrap()
            .unwrap()
            .current()
            .unwrap()
            .meta
            .rating
    }

    /// Waits for the job's own end (`Event::JobFinished`/`JobCancelled`, sent through `Inbound::Report`
    /// strictly before `finish_batch_job` records anything, `batch_job::spawn`'s own send order), then
    /// for the `HistoryChanged` that follows when there was something to record; `None` when a short
    /// quiet spell follows instead (nothing landed: an empty batch, or one cancelled before its first
    /// item did).
    fn wait_for_batch(&self, job: auroraw_engine::JobId) -> Option<auroraw_engine::Label> {
        let deadline = Instant::now() + Duration::from_secs(30);
        let job_str = job.to_string();
        loop {
            match self.events.recv_timeout(Duration::from_millis(100)) {
                Some(Event::JobFinished(j) | Event::JobCancelled(j))
                    if j.to_string() == job_str =>
                {
                    break;
                }
                Some(_) => {}
                None => assert!(Instant::now() < deadline, "the batch job never ended"),
            }
        }
        match self.events.recv_timeout(Duration::from_millis(500)) {
            Some(Event::HistoryChanged(state)) => state.undo,
            _ => None,
        }
    }
}

/// A batch past the threshold applies fully, one history entry, undoable like a small one.
#[test]
fn a_large_batch_runs_as_a_background_job_and_undo_brings_it_all_back() {
    let n = auroraw_engine::BACKGROUND_THRESHOLD + 1;
    let f = fixture(n);
    let commands = f
        .photos
        .iter()
        .map(|photo_id| Command::SetRating {
            photo_id: *photo_id,
            rating: 4,
        })
        .collect();

    let Outcome::BatchStarted { job } = f
        .engine
        .submit_and_wait(Command::Batch { commands })
        .unwrap()
    else {
        panic!("expected BatchStarted");
    };
    let label = f.wait_for_batch(job).unwrap();
    assert_eq!((label.kind, label.count), (LabelKind::Rating, n));
    for photo in &f.photos {
        assert_eq!(f.rating(*photo), Some(4));
    }

    let Outcome::History(_) = f.engine.undo().unwrap() else {
        panic!("expected History");
    };
    for photo in &f.photos {
        assert_eq!(f.rating(*photo), None, "back to unrated, not 0");
    }
    f.engine.redo().unwrap();
    for photo in &f.photos {
        assert_eq!(f.rating(*photo), Some(4));
    }
}

/// Cancelling a large batch keeps whatever prefix already landed (no rollback, unlike a small batch's
/// own all-or-nothing failure path) as its own, real, undoable step; a concurrent cancel reaches it
/// well short of the end (the ack-paced sends this specifically tests: without them, the cancel would
/// queue behind every already-sent item and this assertion would fail).
#[test]
fn cancelling_a_large_batch_keeps_whatever_already_landed() {
    let n = auroraw_engine::BACKGROUND_THRESHOLD + 1;
    let f = fixture(n);
    let commands = f
        .photos
        .iter()
        .map(|photo_id| Command::SetRating {
            photo_id: *photo_id,
            rating: 3,
        })
        .collect();

    let Outcome::BatchStarted { job } = f
        .engine
        .submit_and_wait(Command::Batch { commands })
        .unwrap()
    else {
        panic!("expected BatchStarted");
    };
    f.engine.submit(Command::CancelJob { job_id: job }).unwrap();
    let label = f.wait_for_batch(job);

    let rated = f.photos.iter().filter(|p| f.rating(**p) == Some(3)).count();
    assert!(rated < n, "the cancel actually caught it short of the end");
    match label {
        Some(l) if rated > 0 => assert_eq!((l.kind, l.count), (LabelKind::Rating, rated)),
        _ => assert_eq!(rated, 0, "nothing landed, nothing to record"),
    }
}
