// SPDX-License-Identifier: GPL-3.0-or-later
//! The catalogue panel's model (spec §5.1, M1 plan WP8): the sources of the open workspace, and the
//! calls that add, rescan and remove one. A scan or a removal is a background job; this object holds
//! the one that is running (`job`), and QML tells its events from any other job's by it. The
//! sentences (progress, results) are QML's, so that they are translated.

use core::pin::Pin;

use auroraw_engine::{AddPlan, AddSourceRequest, Command, Engine, Outcome, SourceInfo, paths};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};

use crate::app_settings::{self, AppSettings};
use crate::models::qobject::SourceList;
use crate::session;

/// Qt::UserRole and the next ones.
const ROLE_NAME: i32 = 0x0100;
const ROLE_PATH: i32 = 0x0101;
const ROLE_ONLINE: i32 = 0x0102;
const ROLE_PHOTOS: i32 = 0x0103;
const ROLE_WORKED_ON: i32 = 0x0104;

/// The Rust side of the model.
#[derive(Default)]
pub struct SourceListRust {
    pub(crate) count: i32,
    pub(crate) job: QString,
    sources: Vec<SourceInfo>,
    /// The job in `job`, to answer its question or cancel it.
    running: Option<auroraw_engine::JobId>,
}

fn text(value: &str) -> QString {
    QString::from(value)
}

fn engine() -> Option<Engine> {
    session::current().map(|session| session.engine.clone())
}

impl SourceList {
    fn start(mut self: Pin<&mut Self>, job: auroraw_engine::JobId) {
        self.as_mut().rust_mut().running = Some(job);
        self.set_job(text(&job.to_string()));
    }

    pub fn refresh(mut self: Pin<&mut Self>) {
        let sources = engine()
            .and_then(|engine| engine.sources().ok())
            .unwrap_or_default();
        let count = sources.len() as i32;
        // SAFETY: every begin is followed by its end, with nothing in between that can fail.
        unsafe {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().sources = sources;
            self.as_mut().end_reset_model();
        }
        self.set_count(count);
    }

    pub fn resolve(&self, input: &QString) -> QString {
        match paths::resolve(&input.to_string()) {
            Ok(root) => text(root.to_string_lossy().as_ref()),
            Err(e) => text(&format!("error:{e}")),
        }
    }

    pub fn plan(&self, folder: &QString) -> QString {
        let Some(engine) = engine() else {
            return text("error:no workspace is open");
        };
        let root = match paths::resolve(&folder.to_string()) {
            Ok(root) => root,
            Err(e) => return text(&format!("error:{e}")),
        };
        match engine.plan_add_source(&root) {
            Ok(AddPlan::Free) => text("free"),
            Ok(AddPlan::InsideExisting(source)) => text(&format!("inside:{}", source.name)),
            Ok(AddPlan::ContainsExisting(inner)) => {
                let names: Vec<String> = inner.iter().map(|s| format!("\"{}\"", s.name)).collect();
                text(&format!("contains:{}", names.join(", ")))
            }
            Err(e) => text(&format!("error:{e}")),
        }
    }

    pub fn add(mut self: Pin<&mut Self>, folder: &QString, name: &QString, merge: bool) -> QString {
        let Some(engine) = engine() else {
            return text("no workspace is open");
        };
        let root = match paths::resolve(&folder.to_string()) {
            Ok(root) => root,
            Err(e) => return text(&e.to_string()),
        };
        let name = name.to_string().trim().to_string();
        match engine.add_source(AddSourceRequest {
            root: root.clone(),
            name: Some(name).filter(|n| !n.is_empty()),
            merge,
        }) {
            Ok(added) => {
                // Remembered for next time (issue #10), best effort.
                let path = app_settings::settings_path();
                let settings = AppSettings {
                    last_source_folder: root.to_string_lossy().into_owned(),
                    ..AppSettings::load(&path)
                };
                settings.save(&path);
                self.as_mut().start(added.job);
                self.refresh();
                QString::default()
            }
            Err(e) => text(&e.to_string()),
        }
    }

    /// The folder last added (issue #10), for the Add Source dialog to start from.
    pub fn last_folder(&self) -> QString {
        text(&AppSettings::load(&app_settings::settings_path()).last_source_folder)
    }

    fn counts(&self, row: i32) -> Option<auroraw_engine::SourceCounts> {
        let source = self.sources.get(usize::try_from(row).ok()?)?;
        engine()?.source_counts(source.id).ok()
    }

    pub fn photos_at(&self, row: i32) -> i32 {
        self.counts(row).map_or(0, |c| c.photos as i32)
    }

    pub fn worked_on_at(&self, row: i32) -> i32 {
        self.counts(row).map_or(0, |c| c.worked_on as i32)
    }

    pub fn remove(self: Pin<&mut Self>, row: i32) -> QString {
        let (Some(engine), Some(source)) = (
            engine(),
            usize::try_from(row)
                .ok()
                .and_then(|row| self.sources.get(row).map(|s| s.id)),
        ) else {
            return text("no such source");
        };
        match engine.submit_and_wait(Command::RemoveSource { source_id: source }) {
            Ok(Outcome::RemoveStarted { job }) => {
                self.start(job);
                QString::default()
            }
            Ok(other) => text(&format!("unexpected answer: {other:?}")),
            Err(e) => text(&e.to_string()),
        }
    }

    pub fn rescan(self: Pin<&mut Self>, row: i32) -> QString {
        let (Some(engine), Some(source)) = (
            engine(),
            usize::try_from(row)
                .ok()
                .and_then(|row| self.sources.get(row).map(|s| s.id)),
        ) else {
            return text("no such source");
        };
        match engine.submit_and_wait(Command::IndexSource {
            source_id: source,
            merge: Vec::new(),
        }) {
            Ok(Outcome::IndexStarted { job }) => {
                self.start(job);
                QString::default()
            }
            Ok(other) => text(&format!("unexpected answer: {other:?}")),
            Err(e) => text(&e.to_string()),
        }
    }

    pub fn rescan_folder(mut self: Pin<&mut Self>, folder: &QString) -> QString {
        self.as_mut().refresh();
        let Ok(wanted) = paths::resolve(&folder.to_string()) else {
            return text("no such folder");
        };
        let row = self.sources.iter().position(|source| {
            paths::resolve(&source.path.to_string_lossy()).is_ok_and(|path| path == wanted)
        });
        match row {
            Some(row) => self.rescan(row as i32),
            None => text("no such source"),
        }
    }

    pub fn continue_scan(&self, restore: bool) {
        if let (Some(engine), Some(job_id)) = (engine(), self.running) {
            let _ = engine.submit(Command::ContinueIndex { job_id, restore });
        }
    }

    pub fn cancel_scan(&self) {
        if let (Some(engine), Some(job_id)) = (engine(), self.running) {
            let _ = engine.submit(Command::CancelJob { job_id });
        }
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(source) = usize::try_from(index.row())
            .ok()
            .and_then(|row| self.sources.get(row))
        else {
            return QVariant::default();
        };
        match role {
            ROLE_NAME => QVariant::from(&text(&source.name)),
            ROLE_PATH => QVariant::from(&text(source.path.to_string_lossy().as_ref())),
            ROLE_ONLINE => QVariant::from(&source.online),
            ROLE_PHOTOS => QVariant::from(&(source.photos as i32)),
            ROLE_WORKED_ON => QVariant::from(&(source.worked_on as i32)),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_NAME, QByteArray::from("name"));
        roles.insert(ROLE_PATH, QByteArray::from("path"));
        roles.insert(ROLE_ONLINE, QByteArray::from("online"));
        roles.insert(ROLE_PHOTOS, QByteArray::from("photos"));
        roles.insert(ROLE_WORKED_ON, QByteArray::from("workedOn"));
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.sources.len() as i32
    }
}
