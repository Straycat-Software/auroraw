// SPDX-License-Identifier: GPL-3.0-or-later
//! The background worker behind `Command::RemoveSource` (M1 plan, workflow revision D-091): taking a
//! source out of the catalogue takes its photos out with it, but never destroys anything: a photo's
//! sidecar and its versions are moved to the workspace's `removed/`, from where adding the source
//! again offers to restore them, and the originals are not touched. A photo that also has a
//! location in another source stays, with that location alone.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;

use auroraw_catalogue::Catalogue;
use auroraw_types::SourceId;
use auroraw_workspace::Workspace;

use crate::coordinator::Inbound;
use crate::event::Event;
use crate::job::{CancelToken, JobId};

pub(crate) struct RemoveJob {
    pub job: JobId,
    pub workspace: Arc<Workspace>,
    pub source_id: SourceId,
    pub catalogue_path: PathBuf,
    pub events: mpsc::Sender<Event>,
    pub inbound: mpsc::Sender<Inbound>,
    pub cancel: CancelToken,
}

pub(crate) fn spawn(job: RemoveJob) {
    std::thread::spawn(move || run(job));
}

fn run(job: RemoveJob) {
    let Ok(catalogue) = Catalogue::open(&job.catalogue_path) else {
        let _ = job.events.send(Event::JobFinished(job.job));
        return;
    };
    let photos = catalogue
        .photos_in_source(&job.source_id)
        .unwrap_or_default();
    let total = photos.len();
    let (mut removed, mut kept) = (0, 0);
    for (done, photo_id) in photos.into_iter().enumerate() {
        if job.cancel.is_cancelled() {
            // What was removed stays removed and the source stays: running it again finishes. Through
            // `inbound`, not `events`, directly: it must not be observed as done before the coordinator has
            // drained every `Relocated`/`Removed` message already sent for photos before this one
            // (`Inbound::Report`'s own doc comment).
            let _ = job
                .inbound
                .send(Inbound::Report(Event::JobCancelled(job.job)));
            return;
        }
        // Only a read, to decide which of the two the coordinator should do (it is the one that actually
        // drops the locations or moves the files away, on its own thread, fresh, its own single writer).
        let sidecar = job
            .workspace
            .read_photo(&photo_id)
            .ok()
            .flatten()
            .and_then(|l| l.current());
        let other_location = sidecar.as_ref().and_then(|photo| {
            photo.files.iter().find_map(|file| {
                file.locations
                    .iter()
                    .find(|l| l.source != job.source_id)
                    .map(|l| (file.clone(), l.clone()))
            })
        });
        match other_location {
            // Also somewhere else: keep the photo, drop this source's locations, and move its
            // catalogue row to the location that remains.
            Some((file, location)) => {
                let _ = job.inbound.send(Inbound::Relocated {
                    photo_id,
                    dropped_source: job.source_id,
                    source_id: location.source,
                    path: location.path.clone(),
                    filename: file.name.clone(),
                    fingerprint: file.fingerprint,
                });
                kept += 1;
            }
            // Only here (including a sidecar that could not be read at all): the sidecar and its versions
            // go to `removed/`, the row leaves the catalogue, exactly as when it is a plain read failure
            // today (the coordinator's own handler no-ops on whatever it does not find).
            None => {
                let _ = job.inbound.send(Inbound::Removed { photo_id });
                removed += 1;
            }
        }
        let _ = job.events.send(Event::JobProgress {
            job: job.job,
            done: done + 1,
            total,
        });
    }
    let _ = job.inbound.send(Inbound::SourceGone {
        job: job.job,
        source_id: job.source_id,
        removed,
        kept,
        announce: true,
    });
    let _ = job
        .inbound
        .send(Inbound::Report(Event::JobFinished(job.job)));
}
