// SPDX-License-Identifier: GPL-3.0-or-later
//! Import as a person uses it (spec §5.2, M1 plan WP7 and D-090, D-093): copying the photos of a card or
//! folder to another folder, verified, with an optional second destination, on the engine's own calls.
//! The sentences it leads to (progress, results, what the destination is to the catalogue) are QML's, so that they are
//! translated; this object answers with codes and JSON.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, job)]
        #[qproperty(QString, volumes_override, cxx_name = "volumesOverride")]
        type ImportForm = super::ImportFormRust;

        /// What was typed in the form the last time, as JSON (`Settings`), or the defaults.
        #[qinvokable]
        #[cxx_name = "loadRemembered"]
        fn load_remembered(self: &ImportForm) -> QString;

        /// What the fields mean for the catalogue, as JSON: whether the card keeps camera folders
        /// (`hasFolders`, `folders`), and what the destination is to the catalogue (`kind`: empty,
        /// `covered`, `not-covered` or `contains`; `names`; `registering`: whether the photos will
        /// enter the catalogue).
        #[qinvokable]
        fn inspect(
            self: &ImportForm,
            source: &QString,
            destination: &QString,
            add_destination: bool,
        ) -> QString;

        /// The removable volumes mounted now, as JSON (`name`, `path`, `hasDcim`), camera cards first.
        /// Tests set `volumesOverride` (JSON) to stand for what a person plugs in.
        #[qinvokable]
        fn volumes(self: &ImportForm) -> QString;

        /// Remembers the fields (JSON: `Settings`) and starts the import as a background job
        /// (`job` is its job); empty, or why not.
        #[qinvokable]
        fn start(self: Pin<&mut ImportForm>, fields: &QString) -> QString;

        /// Stops the import that runs.
        #[qinvokable]
        fn cancel(self: &ImportForm);

        /// The folder the last import made a source (its photos are scanned once the copy is done);
        /// empty when there is none. Asking clears it.
        #[qinvokable]
        #[cxx_name = "takeAddedSource"]
        fn take_added_source(self: Pin<&mut ImportForm>) -> QString;
    }
}

use core::pin::Pin;
use std::path::PathBuf;

use auroraw_engine::{Command, DestinationKind, Engine, ImportRequest, JobId, paths};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use serde::Deserialize;
use serde_json::json;

use crate::import_settings::Settings;
use crate::session;

/// The Rust side of the form.
#[derive(Default)]
pub struct ImportFormRust {
    pub(crate) job: QString,
    pub(crate) volumes_override: QString,
    running: Option<JobId>,
    added_source: Option<PathBuf>,
}

/// The fields as QML sends them.
#[derive(Deserialize)]
struct Form {
    #[serde(flatten)]
    settings: Settings,
}

fn text(value: &str) -> QString {
    QString::from(value)
}

impl qobject::ImportForm {
    pub fn load_remembered(&self) -> QString {
        let settings = session::current()
            .map(|session| Settings::load(&session.data_dir.join("import-settings.json")))
            .unwrap_or_default();
        text(&serde_json::to_string(&settings).expect("settings serialise"))
    }

    pub fn inspect(
        &self,
        source: &QString,
        destination: &QString,
        add_destination: bool,
    ) -> QString {
        let info = paths::resolve(&source.to_string())
            .map(|source| Engine::inspect_import_source(&source))
            .unwrap_or_default();
        let kind = session::current().and_then(|session| {
            let destination = paths::resolve(&destination.to_string()).ok()?;
            session.engine.import_destination(&destination).ok()
        });
        let (kind, names, registering) = match kind {
            None => ("", String::new(), false),
            Some(DestinationKind::Covered(source)) => ("covered", source.name, true),
            Some(DestinationKind::NotCovered) => ("not-covered", String::new(), add_destination),
            Some(DestinationKind::ContainsSources(sources)) => {
                let names: Vec<String> =
                    sources.iter().map(|s| format!("\"{}\"", s.name)).collect();
                ("contains", names.join(", "), false)
            }
        };
        text(
            &json!({
                "hasFolders": !info.camera_folders.is_empty(),
                "folders": info.camera_folders.join(", "),
                "kind": kind,
                "names": names,
                "registering": registering,
            })
            .to_string(),
        )
    }

    pub fn volumes(&self) -> QString {
        if !self.volumes_override.is_empty() {
            return self.volumes_override.clone();
        }
        let volumes: Vec<_> = Engine::removable_volumes()
            .into_iter()
            .map(|v| {
                json!({
                    "name": v.name,
                    "path": v.mount_point.to_string_lossy(),
                    "hasDcim": v.has_dcim,
                })
            })
            .collect();
        text(&serde_json::Value::Array(volumes).to_string())
    }

    pub fn start(mut self: Pin<&mut Self>, fields: &QString) -> QString {
        let Some(session) = session::current() else {
            return text("no workspace is open");
        };
        let Ok(form) = serde_json::from_str::<Form>(&fields.to_string()) else {
            return text("the form could not be read");
        };
        // Remembered whatever comes of it: what a person typed is worth keeping.
        form.settings
            .save(&session.data_dir.join("import-settings.json"));
        let resolve = |typed: &str| paths::resolve(typed).map_err(|e| e.to_string());
        let request = (|| -> Result<ImportRequest, String> {
            Ok(ImportRequest {
                source_root: resolve(&form.settings.source)?,
                destination_root: resolve(&form.settings.destination)?,
                profile: form.settings.profile(),
                backup_root: match form.settings.backup.trim() {
                    "" => None,
                    backup => Some(resolve(backup)?),
                },
                state_dir: session.data_dir.join("import"),
                add_destination_as_source: form.settings.add_destination,
            })
        })();
        let request = match request {
            Ok(request) => request,
            Err(reason) => return text(&reason),
        };
        let destination = request.destination_root.clone();
        match session.engine.import(request) {
            Ok(started) => {
                let mut this = self.as_mut().rust_mut();
                this.running = Some(started.job);
                this.added_source = started.added_source.map(|_| destination);
                self.set_job(text(&started.job.to_string()));
                QString::default()
            }
            Err(e) => text(&e.to_string()),
        }
    }

    pub fn cancel(&self) {
        if let (Some(session), Some(job_id)) = (session::current(), self.running) {
            let _ = session.engine.submit(Command::CancelJob { job_id });
        }
    }

    pub fn take_added_source(mut self: Pin<&mut Self>) -> QString {
        self.as_mut()
            .rust_mut()
            .added_source
            .take()
            .map(|folder| text(folder.to_string_lossy().as_ref()))
            .unwrap_or_default()
    }
}
