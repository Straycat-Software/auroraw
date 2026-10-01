// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline place names, as the dialog (Tools ▸ Find place names…) and the Import dialog call them (WP10, design
//! note 008, D-147): ask for a run on the selection or on a source, cancel it, and, after an import that asked
//! for it, run it on the photos the import registered. The work is the engine's (`Command::FindPlaceNames`);
//! its progress and its end arrive on the bus like any other job's, and the sentences are QML's, so that they
//! are translated.
//!
//! The places file, `places.sqlite`, is data and not code: it is shipped beside the program (slice 5 of the note
//! packages it), or named by `AURORAW_PLACES`, or, in a checkout, the one `tools/fetch-places.sh` builds. Without
//! it the feature says so and nothing else is affected.

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;

use auroraw_engine::{Command, JobId, Outcome, PlaceScope};
use auroraw_types::{PhotoId, SourceId};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

use crate::session;

/// The name of the places file.
const FILE: &str = "places.sqlite";

/// The places file this run of the application can use, or `None`. `AURORAW_PLACES` names it and is the only
/// place looked at when it is set (a test says where its fixture is, and a missing one is "not installed", not
/// "use another"); else the file beside the program; else, **in a debug build only**, the checkout's
/// `testdata/places/places.sqlite`, found from where this crate's source is and not from the working directory (a
/// shipped Auroraw run from a folder that happens to hold such a file must not use it).
pub fn pack_path() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("AURORAW_PLACES") {
        let named = PathBuf::from(named);
        return named.is_file().then_some(named);
    }
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(FILE)));
    let checkout = cfg!(debug_assertions).then(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/places")
            .join(FILE)
    });
    [beside, checkout]
        .into_iter()
        .flatten()
        .find(|path| path.is_file())
}

/// The photos each running import has registered so far, by job: what the import's events say, kept for the
/// run of place names that may follow it. Taken when the import has finished and the person asked for the run, and
/// forgotten (`forgetImport`) when the import ended and nobody did: cancelled, aborted, or without the option.
static IMPORTED: Mutex<Option<HashMap<String, Vec<PhotoId>>>> = Mutex::new(None);

/// Notes that the import `job` (its identifier as the bus gives it) registered `photo`: called from the bus, on the
/// thread of the engine's events.
pub fn note_imported(job: &str, photo: PhotoId) {
    IMPORTED
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .entry(job.to_string())
        .or_default()
        .push(photo);
}

fn take_imported(job: &str) -> Vec<PhotoId> {
    IMPORTED
        .lock()
        .unwrap()
        .as_mut()
        .and_then(|map| map.remove(job))
        .unwrap_or_default()
}

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        type PlaceNames = super::PlaceNamesRust;

        /// Whether the places file is there to be used.
        #[qinvokable]
        fn installed(self: &PlaceNames) -> bool;

        /// Starts a run and returns the job's identifier, or `error:` and why not. `scope` is `"selection"` with
        /// `target` the photos' identifiers joined by commas, or `"source"` with `target` the source's identifier;
        /// `language` is the interface's (`en`, `fr`), the names being given in it; `refresh` lets the names Auroraw
        /// found earlier follow the photo's position.
        #[qinvokable]
        fn start(
            self: &PlaceNames,
            scope: &QString,
            target: &QString,
            language: &QString,
            refresh: bool,
        ) -> QString;

        /// Shows what the same run would do, and writes nothing (`Command::PreviewPlaceNames`, note 008 §4): the
        /// job's identifier, or `error:` and why not. Its end arrives as `placeNamesPreview`; `cancel` stops it.
        #[qinvokable]
        #[cxx_name = "startPreview"]
        fn start_preview(
            self: &PlaceNames,
            scope: &QString,
            target: &QString,
            language: &QString,
            refresh: bool,
        ) -> QString;

        /// Starts a run on the photos that the import `import_job` registered, if there were any: its job's
        /// identifier, or an empty text when there is nothing to do, or `error:` and why not.
        #[qinvokable]
        #[cxx_name = "startAfterImport"]
        fn start_after_import(
            self: &PlaceNames,
            import_job: &QString,
            language: &QString,
        ) -> QString;

        /// Forgets the photos that the import `import_job` registered: it ended and no run was asked for.
        #[qinvokable]
        #[cxx_name = "forgetImport"]
        fn forget_import(self: &PlaceNames, import_job: &QString);

        /// Cancels the run (or the preview) `start` began, if it is still running (what a run found stays, as one
        /// step).
        #[qinvokable]
        fn cancel(self: &PlaceNames);
    }
}

/// The Rust side: the job the last `start` began, to cancel it.
#[derive(Default)]
pub struct PlaceNamesRust {
    job: Mutex<Option<JobId>>,
}

fn text(value: &str) -> QString {
    QString::from(value)
}

impl PlaceNamesRust {
    /// Starts the run for `scope` (or, with `preview`, what it would do), and remembers its job.
    fn run(&self, scope: PlaceScope, language: &str, refresh: bool, preview: bool) -> QString {
        let Some(session) = session::current() else {
            return text("error:no workspace is open");
        };
        let Some(pack) = pack_path() else {
            return text("error:place names are not installed");
        };
        let language = language.to_string();
        let command = if preview {
            Command::PreviewPlaceNames {
                scope,
                pack,
                language,
                refresh,
            }
        } else {
            Command::FindPlaceNames {
                scope,
                pack,
                language,
                refresh,
            }
        };
        match session.engine.submit_and_wait(command) {
            Ok(Outcome::PlaceNamesStarted { job, .. }) => {
                *self.job.lock().unwrap() = Some(job);
                text(&job.to_string())
            }
            Ok(_) => text("error:the run did not start"),
            Err(e) => text(&format!("error:{e}")),
        }
    }
}

impl qobject::PlaceNames {
    pub fn installed(&self) -> bool {
        pack_path().is_some()
    }

    pub fn start(
        &self,
        scope: &QString,
        target: &QString,
        language: &QString,
        refresh: bool,
    ) -> QString {
        self.begin(scope, target, language, refresh, false)
    }

    pub fn start_preview(
        &self,
        scope: &QString,
        target: &QString,
        language: &QString,
        refresh: bool,
    ) -> QString {
        self.begin(scope, target, language, refresh, true)
    }

    fn begin(
        &self,
        scope: &QString,
        target: &QString,
        language: &QString,
        refresh: bool,
        preview: bool,
    ) -> QString {
        let scope = match scope.to_string().as_str() {
            "selection" => PlaceScope::Photos(
                target
                    .to_string()
                    .split(',')
                    .filter(|id| !id.is_empty())
                    .filter_map(|id| PhotoId::from_str(id).ok())
                    .collect(),
            ),
            "source" => match SourceId::from_str(&target.to_string()) {
                Ok(id) => PlaceScope::Source(id),
                Err(_) => return text("error:that is not a source"),
            },
            other => return text(&format!("error:unknown scope {other}")),
        };
        self.rust()
            .run(scope, &language.to_string(), refresh, preview)
    }

    pub fn start_after_import(&self, import_job: &QString, language: &QString) -> QString {
        let photos = take_imported(&import_job.to_string());
        if photos.is_empty() {
            return QString::default();
        }
        self.rust().run(
            PlaceScope::Photos(photos),
            &language.to_string(),
            false,
            false,
        )
    }

    pub fn forget_import(&self, import_job: &QString) {
        take_imported(&import_job.to_string());
    }

    pub fn cancel(&self) {
        let job = self.rust().job.lock().unwrap().take();
        if let (Some(session), Some(job_id)) = (session::current(), job) {
            let _ = session.engine.submit(Command::CancelJob { job_id });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_an_import_registered_is_handed_over_once() {
        let a = PhotoId::random();
        let b = PhotoId::random();
        note_imported("job-test-1", a);
        note_imported("job-test-1", b);
        note_imported("job-test-2", PhotoId::random());
        assert_eq!(take_imported("job-test-1"), vec![a, b]);
        assert!(take_imported("job-test-1").is_empty(), "taken once");
        assert_eq!(
            take_imported("job-test-2").len(),
            1,
            "each import has its own"
        );
    }
}
