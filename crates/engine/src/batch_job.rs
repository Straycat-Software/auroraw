// SPDX-License-Identifier: GPL-3.0-or-later
//! The background worker behind a `Command::Batch` or `Command::DeleteKeyword` large enough that
//! doing it synchronously on the coordinator's own thread would be felt (D-126 volet B): above
//! [`BACKGROUND_THRESHOLD`] items, the coordinator spawns this instead of looping itself, one item
//! at a time, exactly the way `apply_edit` already applies one on the coordinator's own thread.
//!
//! Unlike `refresh.rs`/`remove_job.rs`/`index_job.rs`, an item here needs no read of its own to
//! decide anything (the coordinator's `apply_edit` already does its own fresh read right before
//! writing) — so nothing would naturally pace the job's sends the way a job with its own disk read
//! is paced. Without pacing, a `Command::CancelJob` submitted concurrently could land far behind
//! thousands of already-queued item messages on the shared, unbounded `Inbound` channel, arriving
//! too late to matter. This job paces itself instead: it sends one item, waits for the
//! coordinator's acknowledgment that it was applied, checks for cancellation, then sends the next
//! — so a cancel interleaves within roughly one item's processing time, however large the job.

use std::sync::mpsc;

use crate::command::Command;
use crate::coordinator::Inbound;
use crate::event::Event;
use crate::job::{CancelToken, JobId};

/// Above this many items, `Command::Batch` and `Command::DeleteKeyword` run as a background job
/// instead of looping on the coordinator's own thread. Provisional, not profiled: every item costs
/// one sidecar read, one atomic rewrite, one re-stat and one SQL transaction regardless of what it
/// changes (`Coordinator::edit_photo`/`persist_photo`), and this keeps the synchronous path under
/// roughly a couple of seconds even at a pessimistic per-item cost, comfortably above every batch
/// any existing test or everyday selection reaches. One `const`, easy to retune.
pub const BACKGROUND_THRESHOLD: usize = 200;

/// Applies `items` one at a time, through the coordinator (`Inbound::BatchItem`), reporting
/// progress and honouring `cancel` between items. `Inbound::BatchDone` is what actually finishes
/// the job on the coordinator's side (records the history entry, and, for a `DeleteKeyword` sweep,
/// removes the vocabulary branch once every photo has lost it) — this thread only decides when to
/// stop sending and reports progress as it goes.
pub(crate) fn spawn(
    job: JobId,
    items: Vec<Command>,
    events: mpsc::Sender<Event>,
    inbound: mpsc::Sender<Inbound>,
    cancel: CancelToken,
) {
    std::thread::spawn(move || {
        let total = items.len();
        for (done, edit) in items.into_iter().enumerate() {
            if cancel.is_cancelled() {
                // Through `inbound`, not `events`, directly: it must not be observed as done before
                // the coordinator has actually drained every `BatchItem` already sent for this job
                // (`Inbound::Report`'s own doc comment).
                let _ = inbound.send(Inbound::Report(Event::JobCancelled(job)));
                let _ = inbound.send(Inbound::BatchDone { job });
                return;
            }
            let (ack_tx, ack_rx) = mpsc::channel();
            if inbound
                .send(Inbound::BatchItem {
                    job,
                    edit,
                    ack: ack_tx,
                })
                .is_err()
            {
                return;
            }
            // Waits for the coordinator to have actually applied this one item: what paces this job to
            // the coordinator's own speed, so a concurrent cancel is never stuck behind a whole batch
            // that outran it.
            let _ = ack_rx.recv();
            let _ = events.send(Event::JobProgress {
                job,
                done: done + 1,
                total,
            });
        }
        let _ = inbound.send(Inbound::Report(Event::JobFinished(job)));
        let _ = inbound.send(Inbound::BatchDone { job });
    });
}
