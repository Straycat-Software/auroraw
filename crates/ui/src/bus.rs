// SPDX-License-Identifier: GPL-3.0-or-later
//! The event bus: what the engine reports, as QML signals on one singleton. A thread blocks on the
//! engine's event receiver and queues each event onto the GUI thread, where the `Bus` emits it; QML
//! and the other objects connect to what they care about. Each milestone adds the events its screens need.

// `index_finished`'s own arguments land one over clippy's default limit (cxx_qt's macro-generated
// code counts them as one more than the plain Rust signature does): `Bus` is a plain event relay,
// not a place to invent a struct just to carry six plain numbers.
#![allow(clippy::too_many_arguments)]

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type Bus = super::BusRust;

        /// A photo's catalogue row changed (a rating, a new photo...).
        #[qsignal]
        #[cxx_name = "photoChanged"]
        fn photo_changed(self: Pin<&mut Bus>, photo_id: &QString);

        /// What Undo and Redo would do changed: the kind of the step (`rating`, `flag`, `keywords`,
        /// `batch`; empty when there is none) and how many photos it touched, for each.
        #[qsignal]
        #[cxx_name = "historyChanged"]
        fn history_changed(
            self: Pin<&mut Bus>,
            undo_kind: &QString,
            undo_count: i32,
            redo_kind: &QString,
            redo_count: i32,
        );

        /// An action was undone or redone: the photos it touched, their identifiers joined by commas.
        #[qsignal]
        #[cxx_name = "historyApplied"]
        fn history_applied(self: Pin<&mut Bus>, photo_ids: &QString);

        /// A series was made, changed or dissolved (a step, an undo, or the detection that follows photos arriving):
        /// the grid lists photos differently now.
        #[qsignal]
        #[cxx_name = "seriesChanged"]
        fn series_changed(self: Pin<&mut Bus>);

        /// A collection was made, changed or deleted, or a photo left its collections with its source: the
        /// collections panel lists them differently now.
        #[qsignal]
        #[cxx_name = "collectionsChanged"]
        fn collections_changed(self: Pin<&mut Bus>);

        /// A background job made progress.
        #[qsignal]
        #[cxx_name = "jobProgress"]
        fn job_progress(self: Pin<&mut Bus>, job: &QString, done: i32, total: i32);

        /// A scan found what a source holds; when some of it was removed earlier, it waits for an
        /// answer.
        #[qsignal]
        #[cxx_name = "indexPlanned"]
        fn index_planned(self: Pin<&mut Bus>, job: &QString, new_files: i32, restorable: i32);

        /// A scan could not run at all.
        #[qsignal]
        #[cxx_name = "indexAborted"]
        fn index_aborted(self: Pin<&mut Bus>, job: &QString, reason: &QString);

        /// A source was taken out of the catalogue.
        #[qsignal]
        #[cxx_name = "sourceRemoved"]
        fn source_removed(self: Pin<&mut Bus>, job: &QString, removed: i32, kept: i32);

        /// An import finished.
        #[qsignal]
        #[cxx_name = "importFinished"]
        fn import_finished(
            self: Pin<&mut Bus>,
            job: &QString,
            copied: i32,
            skipped: i32,
            failed: i32,
        );

        /// An import could not run at all.
        #[qsignal]
        #[cxx_name = "importAborted"]
        fn import_aborted(self: Pin<&mut Bus>, job: &QString, reason: &QString);

        /// A job was cancelled before its end.
        #[qsignal]
        #[cxx_name = "jobCancelled"]
        fn job_cancelled(self: Pin<&mut Bus>, job: &QString);

        /// A background job ran to completion (D-126 volet B: a large `Batch`'s own completion, and,
        /// incidentally, any other job kind's — nothing else emits this yet).
        #[qsignal]
        #[cxx_name = "jobFinished"]
        fn job_finished(self: Pin<&mut Bus>, job: &QString);

        /// A keyword branch was deleted (D-126 volet B): a small one right away, `job` empty; one past
        /// `BACKGROUND_THRESHOLD` photos once its background sweep actually ends, `job` its id.
        /// `finished` is `false` only for a sweep that was cancelled before every carrying photo was
        /// reached (the keyword stayed in the vocabulary; deleting it again resumes the sweep).
        #[qsignal]
        #[cxx_name = "keywordDeleted"]
        fn keyword_deleted(
            self: Pin<&mut Bus>,
            job: &QString,
            keyword_id: &QString,
            keywords: i32,
            photos: i32,
            finished: bool,
        );

        /// A scan of a source finished. `duplicates` (issue #16) is how many files turned out to
        /// be a confirmed second location of an existing photo: joined to it, not added as their
        /// own.
        #[qsignal]
        #[cxx_name = "indexFinished"]
        fn index_finished(
            self: Pin<&mut Bus>,
            job: &QString,
            added: i32,
            restored: i32,
            known: i32,
            failed: i32,
            duplicates: i32,
        );
    }

    // Lets the event thread queue work onto the GUI thread.
    impl cxx_qt::Threading for Bus {}
    impl cxx_qt::Initialize for Bus {}
}

use core::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use auroraw_engine::{Event, EventReceiver};
use cxx_qt::{CxxQtThread, Threading};
use cxx_qt_lib::QString;

use crate::session::Session;

/// The Rust side of the bus: it holds nothing.
#[derive(Default)]
pub struct BusRust {}

/// How to reach the bus from the event thread, set when the singleton is created.
static GUI: Mutex<Option<CxxQtThread<qobject::Bus>>> = Mutex::new(None);

impl cxx_qt::Initialize for qobject::Bus {
    fn initialize(self: Pin<&mut Self>) {
        *GUI.lock().unwrap() = Some(self.qt_thread());
    }
}

/// Queues `emit` onto the GUI thread, if the bus exists yet.
fn on_gui(emit: impl FnOnce(Pin<&mut qobject::Bus>) + Send + 'static) {
    if let Some(gui) = GUI.lock().unwrap().as_ref() {
        let _ = gui.queue(emit);
    }
}

/// Starts the thread that carries `events` (of the workspace `session` belongs to) to the bus. It
/// ends when the session is dropped (another workspace opened, or the application quit) or the
/// engine says it has stopped. It holds the session weakly, so it never keeps a workspace's folder
/// locked.
pub fn start_pump(events: EventReceiver, session: &Arc<Session>) {
    let session: Weak<Session> = Arc::downgrade(session);
    std::thread::spawn(move || {
        loop {
            match events.recv_timeout(Duration::from_millis(200)) {
                Some(Event::Stopped) => break,
                Some(event) => {
                    let Some(alive) = session.upgrade() else {
                        break;
                    };
                    dispatch(event, &alive);
                }
                None => {
                    if session.strong_count() == 0 {
                        break;
                    }
                }
            }
        }
    });
}

fn dispatch(event: Event, session: &Session) {
    match event {
        Event::PhotoChanged(id) => {
            // A photo that just entered the catalogue gets its thumbnail made at once, in the order
            // photos arrive, without waiting for a grid to show it.
            session.thumbs.warm(id);
            let id = id.to_string();
            on_gui(move |bus| bus.photo_changed(&QString::from(id.as_str())));
        }
        Event::HistoryChanged(state) => {
            let (undo, redo) = (state.undo, state.redo);
            let kind = |label: Option<auroraw_engine::Label>| {
                label.map_or(String::new(), |l| l.kind.name().to_string())
            };
            let count = |label: Option<auroraw_engine::Label>| label.map_or(0, |l| l.count as i32);
            let (undo_kind, redo_kind) = (kind(undo), kind(redo));
            let (undo_count, redo_count) = (count(undo), count(redo));
            on_gui(move |bus| {
                bus.history_changed(
                    &QString::from(undo_kind.as_str()),
                    undo_count,
                    &QString::from(redo_kind.as_str()),
                    redo_count,
                )
            });
        }
        Event::SeriesChanged => on_gui(|bus| bus.series_changed()),
        Event::CollectionsChanged => on_gui(|bus| bus.collections_changed()),
        // A rebuilt catalogue has no hashes: they are made again from the thumbnails (D-105).
        Event::RebuildFinished { .. } => {
            session.thumbs.service().warm_unhashed();
        }
        Event::HistoryApplied { photos, .. } => {
            let ids: Vec<String> = photos.iter().map(ToString::to_string).collect();
            let ids = ids.join(",");
            on_gui(move |bus| bus.history_applied(&QString::from(ids.as_str())));
        }
        Event::JobProgress { job, done, total } => {
            let job = job.to_string();
            on_gui(move |bus| {
                bus.job_progress(&QString::from(job.as_str()), done as i32, total as i32)
            });
        }
        Event::IndexFinished {
            job,
            added,
            restored,
            known,
            failed,
            second_locations,
            ..
        } => {
            let job = job.to_string();
            on_gui(move |bus| {
                bus.index_finished(
                    &QString::from(job.as_str()),
                    added as i32,
                    restored as i32,
                    known as i32,
                    failed as i32,
                    second_locations as i32,
                )
            });
        }
        Event::IndexPlanned {
            job,
            new_files,
            restorable,
            ..
        } => {
            let job = job.to_string();
            on_gui(move |bus| {
                bus.index_planned(
                    &QString::from(job.as_str()),
                    new_files as i32,
                    restorable as i32,
                )
            });
        }
        Event::IndexAborted { job, reason } => {
            let job = job.to_string();
            on_gui(move |bus| {
                bus.index_aborted(
                    &QString::from(job.as_str()),
                    &QString::from(reason.as_str()),
                )
            });
        }
        Event::SourceRemoved {
            job, removed, kept, ..
        } => {
            let job = job.to_string();
            on_gui(move |bus| {
                bus.source_removed(&QString::from(job.as_str()), removed as i32, kept as i32)
            });
        }
        Event::ImportFinished {
            job,
            copied,
            skipped,
            failed,
        } => {
            let job = job.to_string();
            on_gui(move |bus| {
                bus.import_finished(
                    &QString::from(job.as_str()),
                    copied as i32,
                    skipped as i32,
                    failed as i32,
                )
            });
        }
        Event::ImportAborted { job, reason } => {
            let job = job.to_string();
            on_gui(move |bus| {
                bus.import_aborted(
                    &QString::from(job.as_str()),
                    &QString::from(reason.as_str()),
                )
            });
        }
        Event::JobCancelled(job) => {
            let job = job.to_string();
            on_gui(move |bus| bus.job_cancelled(&QString::from(job.as_str())));
        }
        Event::JobFinished(job) => {
            let job = job.to_string();
            on_gui(move |bus| bus.job_finished(&QString::from(job.as_str())));
        }
        Event::KeywordDeleted {
            keyword_id,
            job,
            keywords,
            photos,
            finished,
        } => {
            let job = job.map_or_else(String::new, |j| j.to_string());
            let keyword_id = keyword_id.to_string();
            on_gui(move |bus| {
                bus.keyword_deleted(
                    &QString::from(job.as_str()),
                    &QString::from(keyword_id.as_str()),
                    keywords as i32,
                    photos as i32,
                    finished,
                )
            });
        }
        _ => {}
    }
}
