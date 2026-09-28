// SPDX-License-Identifier: GPL-3.0-or-later
//! The background job that follows a keyword rename (note 003 §6): every sidecar that carries
//! the renamed keyword (or one of its descendants) gets its name snapshot refreshed. Runs on its
//! own thread, but only to decide and to pace itself (cancellation, progress): the actual read,
//! patch and write happen on the coordinator thread, its own single writer for every sidecar
//! (D-099) — this job sends one small message a photo (`Inbound::Refreshed`) rather than touching
//! the workspace at all.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc;

use auroraw_types::{KeywordId, PhotoId};

use crate::coordinator::Inbound;
use crate::event::Event;
use crate::job::{CancelToken, JobId};

/// Asks the coordinator to refresh the path snapshot of `photo_ids`' sidecars from `paths`, one small
/// message a photo, reporting progress itself as it goes.
pub(crate) fn spawn(
    job: JobId,
    photo_ids: Vec<PhotoId>,
    paths: HashMap<KeywordId, String>,
    events: mpsc::Sender<Event>,
    inbound: mpsc::Sender<Inbound>,
    cancel: CancelToken,
) {
    std::thread::spawn(move || {
        let paths = Arc::new(paths);
        let total = photo_ids.len();
        for (done, id) in photo_ids.into_iter().enumerate() {
            let done = done + 1;
            if cancel.is_cancelled() {
                // Through `inbound`, not `events`, directly: it must not be observed as done before the
                // coordinator has actually drained every `Refreshed` message already sent for this job
                // (`Inbound::Report`'s own doc comment).
                let _ = inbound.send(Inbound::Report(Event::JobCancelled(job)));
                let _ = inbound.send(Inbound::RefreshDone);
                return;
            }
            let _ = inbound.send(Inbound::Refreshed {
                photo_id: id,
                paths: paths.clone(),
            });
            let _ = events.send(Event::JobProgress { job, done, total });
        }
        let _ = inbound.send(Inbound::Report(Event::JobFinished(job)));
        let _ = inbound.send(Inbound::RefreshDone);
    });
}
