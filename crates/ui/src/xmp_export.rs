// SPDX-License-Identifier: GPL-3.0-or-later
//! The XMP export to the source folders, as the export dialog calls it (WP10, D-024, D-138): start one
//! for the selected photos or for a whole source, cancel it, and remember the choices last made. The
//! work is the engine's (`Command::ExportXmp`); its progress and its end arrive on the bus like any
//! other job's, and the sentences are QML's, so that they are translated.

use std::str::FromStr;
use std::sync::Mutex;

use auroraw_engine::{Command, JobId, Outcome, XmpExisting, XmpExportOptions, XmpNaming, XmpScope};
use auroraw_types::{PhotoId, SourceId};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

use crate::app_settings::{self, AppSettings};
use crate::session;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        type XmpExport = super::XmpExportRust;

        /// The choices last made, as JSON: `{"naming": "stem" | "full", "existing": "merge" | "skip",
        /// "minusOne": bool}`. Replace is never one of them: it is asked for each time.
        #[qinvokable]
        fn choices(self: &XmpExport) -> QString;

        /// Starts an export and remembers the choices. `scope` is `"selection"` with `target` the photos'
        /// identifiers joined by commas, or `"source"` with `target` the source's identifier; `naming` is
        /// `"stem"` or `"full"`, `existing` is `"merge"`, `"replace"` or `"skip"`. Returns the job's
        /// identifier, or `error:` and why not.
        #[qinvokable]
        fn start(
            self: &XmpExport,
            scope: &QString,
            target: &QString,
            naming: &QString,
            existing: &QString,
            minus_one: bool,
        ) -> QString;

        /// Cancels the export `start` began, if it is still running (what it already wrote stays written).
        #[qinvokable]
        fn cancel(self: &XmpExport);
    }
}

/// The Rust side: the job the last `start` began, to cancel it.
#[derive(Default)]
pub struct XmpExportRust {
    job: Mutex<Option<JobId>>,
}

fn text(value: &str) -> QString {
    QString::from(value)
}

impl qobject::XmpExport {
    pub fn choices(&self) -> QString {
        let settings = AppSettings::load(&app_settings::settings_path());
        text(
            &serde_json::json!({
                "naming": settings.xmp_naming,
                "existing": settings.xmp_existing,
                "minusOne": settings.xmp_minus_one,
            })
            .to_string(),
        )
    }

    pub fn start(
        &self,
        scope: &QString,
        target: &QString,
        naming: &QString,
        existing: &QString,
        minus_one: bool,
    ) -> QString {
        let Some(session) = session::current() else {
            return text("error:no workspace is open");
        };
        let scope = match scope.to_string().as_str() {
            "selection" => XmpScope::Photos(
                target
                    .to_string()
                    .split(',')
                    .filter(|id| !id.is_empty())
                    .filter_map(|id| PhotoId::from_str(id).ok())
                    .collect(),
            ),
            "source" => match SourceId::from_str(&target.to_string()) {
                Ok(id) => XmpScope::Source(id),
                Err(_) => return text("error:that is not a source"),
            },
            other => return text(&format!("error:unknown scope {other}")),
        };
        let naming_text = naming.to_string();
        let existing_text = existing.to_string();
        let options = XmpExportOptions {
            naming: if naming_text == "full" {
                XmpNaming::FullName
            } else {
                XmpNaming::Stem
            },
            existing: match existing_text.as_str() {
                "replace" => XmpExisting::Replace,
                "skip" => XmpExisting::SkipExisting,
                _ => XmpExisting::Merge,
            },
            rejected_as_minus_one: minus_one,
        };
        // Remembered for next time, best effort; Replace is never remembered.
        let path = app_settings::settings_path();
        let mut settings = AppSettings::load(&path);
        settings.xmp_naming = if naming_text == "full" {
            "full"
        } else {
            "stem"
        }
        .into();
        if existing_text != "replace" {
            settings.xmp_existing = if existing_text == "skip" {
                "skip"
            } else {
                "merge"
            }
            .into();
        }
        settings.xmp_minus_one = minus_one;
        settings.save(&path);

        match session
            .engine
            .submit_and_wait(Command::ExportXmp { scope, options })
        {
            Ok(Outcome::XmpExportStarted { job, .. }) => {
                *self.rust().job.lock().unwrap() = Some(job);
                text(&job.to_string())
            }
            Ok(_) => text("error:the export did not start"),
            Err(e) => text(&format!("error:{e}")),
        }
    }

    pub fn cancel(&self) {
        let job = self.rust().job.lock().unwrap().take();
        if let (Some(session), Some(job_id)) = (session::current(), job) {
            let _ = session.engine.submit(Command::CancelJob { job_id });
        }
    }
}
