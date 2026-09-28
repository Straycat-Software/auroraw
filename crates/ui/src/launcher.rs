// SPDX-License-Identifier: GPL-3.0-or-later
//! The launcher: which workspaces this machine knows, creating and opening one, and the interface's
//! own settings (the language), on the engine's own calls.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, screen)]
        #[qproperty(QString, workspace_name, cxx_name = "workspaceName")]
        /// Counts the workspaces opened: it changes when another workspace replaces the open one.
        #[qproperty(i32, workspace_serial, cxx_name = "workspaceSerial")]
        /// Why the welcome list is shown instead of a workspace: `lost:<folder>` or
        /// `open:<folder><tab><reason>`.
        #[qproperty(QString, note)]
        #[qproperty(QString, language)]
        #[qproperty(QString, effective_language, cxx_name = "effectiveLanguage")]
        type Launcher = super::LauncherRust;

        /// Opens the workspace named at launch, else the last one, or leaves the welcome screen.
        #[qinvokable]
        fn start(self: Pin<&mut Launcher>);

        /// The application's version.
        #[qinvokable]
        fn version(self: &Launcher) -> QString;

        /// Tests: the machine to use, a folder under `AURORAW_TEST_HOME` (call before `start`).
        #[qinvokable]
        #[cxx_name = "useMachine"]
        fn use_machine(self: &Launcher, name: &QString);

        /// An environment variable (the tests are given their folders that way).
        #[qinvokable]
        fn env(self: &Launcher, name: &QString) -> QString;

        /// The folder proposed to hold new workspaces.
        #[qinvokable]
        #[cxx_name = "defaultParent"]
        fn default_parent(self: &Launcher) -> QString;

        /// `base`, or `base 2`... when a folder of that name is already in `parent`.
        #[qinvokable]
        #[cxx_name = "freeName"]
        fn free_name(self: &Launcher, parent: &QString, base: &QString) -> QString;

        /// Where the workspace would be created, or an empty text.
        #[qinvokable]
        fn preview(self: &Launcher, name: &QString, parent: &QString) -> QString;

        /// Creates and opens a workspace; an empty text, or why not.
        #[qinvokable]
        fn create(self: Pin<&mut Launcher>, name: &QString, parent: &QString) -> QString;

        /// Opens the workspace in the folder `path`; an empty text, or why not.
        #[qinvokable]
        #[cxx_name = "openPath"]
        fn open_path(self: Pin<&mut Launcher>, path: &QString) -> QString;

        /// The languages the interface can be shown in (`en`, `fr`...).
        #[qinvokable]
        fn languages(self: &Launcher) -> QStringList;

        /// Chooses the interface's language (`system`, or one of `languages()`), remembers it and
        /// retranslates what is on screen.
        #[qinvokable]
        #[cxx_name = "chooseLanguage"]
        fn choose_language(self: Pin<&mut Launcher>, code: &QString);

        /// The width the keyword panel was last dragged to.
        #[qinvokable]
        #[cxx_name = "keywordPanelWidth"]
        fn keyword_panel_width(self: &Launcher) -> i32;

        /// An option of the image view (`autoAdvance`, `filmstrip`, `info`).
        #[qinvokable]
        #[cxx_name = "viewOption"]
        fn view_option(self: &Launcher, name: &QString) -> bool;

        /// Remembers an option of the image view.
        #[qinvokable]
        #[cxx_name = "setViewOption"]
        fn set_view_option(self: &Launcher, name: &QString, on: bool);

        /// A whole-number option: `thumbSize` (the grid's thumbnails, 96 to 256), `comparePanes` (2 to 4),
        /// `similarDistance` (1 to 24 bits) or `similarMinutes` (1 to 10080).
        #[qinvokable]
        #[cxx_name = "intOption"]
        fn int_option(self: &Launcher, name: &QString) -> i32;

        /// Remembers a whole-number option.
        #[qinvokable]
        #[cxx_name = "setIntOption"]
        fn set_int_option(self: &Launcher, name: &QString, value: i32);

        /// The largest gap, in seconds, between two photos of one series.
        #[qinvokable]
        #[cxx_name = "seriesGap"]
        fn series_gap(self: &Launcher) -> i32;

        /// What to do at launch (issue #11): `"reopen"` (the default, the last workspace) or `"list"` (the
        /// Welcome screen's known workspaces).
        #[qinvokable]
        #[cxx_name = "startupBehavior"]
        fn startup_behavior(self: &Launcher) -> QString;

        /// Remembers the startup behavior.
        #[qinvokable]
        #[cxx_name = "setStartupBehavior"]
        fn set_startup_behavior(self: &Launcher, value: &QString);

        /// Remembers the series gap and tells the engine (series formed later use it).
        #[qinvokable]
        #[cxx_name = "setSeriesGap"]
        fn set_series_gap(self: &Launcher, seconds: i32);

        /// Dissolves the series that were formed by themselves and are not resolved, and forms them again with
        /// the gap (series made by hand or resolved stay).
        #[qinvokable]
        #[cxx_name = "regroupSeries"]
        fn regroup_series(self: &Launcher);

        /// Remembers the width of the keyword panel.
        #[qinvokable]
        #[cxx_name = "setKeywordPanelWidth"]
        fn set_keyword_panel_width(self: &Launcher, width: i32);
    }
}

use core::pin::Pin;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

use auroraw_engine::{Engine, LocalDirs, OpenedWorkspace, paths};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};

use crate::app_settings::{AppSettings, LANGUAGES, resolve_language};
use crate::glue;
use crate::session::{self, Collector, Session};
use crate::{bus, translation_for};

/// The Rust side of the launcher.
#[derive(Default)]
pub struct LauncherRust {
    screen: QString,
    workspace_name: QString,
    workspace_serial: i32,
    note: QString,
    language: QString,
    effective_language: QString,
    dirs: Option<LocalDirs>,
    pictures: PathBuf,
    /// The session this launcher opened, so that destroying it releases the workspace's folder.
    session: Option<Weak<Session>>,
}

impl Drop for LauncherRust {
    fn drop(&mut self) {
        if let Some(session) = &self.session {
            session::clear_if_current(session);
        }
    }
}

/// `base`, or `base 2`, `base 3`... when a folder of that name is already in `parent`: the name to
/// offer for a new workspace.
fn free_workspace_name(parent: &Path, base: &str) -> String {
    let taken = |name: &str| parent.join(paths::folder_name(name, "Workspace")).exists();
    if !taken(base) {
        return base.to_string();
    }
    (2u32..)
        .map(|n| format!("{base} {n}"))
        .find(|name| !taken(name))
        .expect("some number is free")
}

/// Whether a workspace can be made in `folder`: it does not exist, or is an empty folder.
fn is_free(folder: &Path) -> bool {
    match std::fs::read_dir(folder) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => !folder.exists(),
    }
}

fn text(value: &str) -> QString {
    QString::from(value)
}

impl qobject::Launcher {
    fn dirs(&self) -> LocalDirs {
        self.dirs.clone().expect("start() was called")
    }

    fn settings_path(&self) -> PathBuf {
        self.dirs().data.join("app-settings.json")
    }

    fn show(mut self: Pin<&mut Self>, opened: OpenedWorkspace) {
        let OpenedWorkspace {
            engine,
            events,
            name,
            previews_path,
            workspace_id,
            ..
        } = opened;
        let service = engine
            .start_thumbnails(&previews_path, 4)
            .expect("the previews database opens");
        // The photos that have no perceptual hash yet (D-105) are hashed with their thumbnails, in the background.
        service.warm_unhashed();
        // The gap the person chose applies to the series formed from now on.
        let _ = engine.submit(auroraw_engine::Command::SetSeriesGap {
            seconds: AppSettings::load(&self.settings_path()).series_gap,
        });
        let previews = engine
            .start_previews(2)
            .expect("the preview service starts");
        let session = Arc::new(Session {
            engine,
            data_dir: self.dirs().workspace_data(workspace_id),
            thumbs: Collector::new(service, glue::thumbnail_deliverer()),
            previews: Collector::new(previews, glue::thumbnail_deliverer()),
        });
        bus::start_pump(events, &session);
        self.as_mut().rust_mut().session = Some(Arc::downgrade(&session));
        session::set_current(Some(session));
        self.as_mut().set_workspace_name(text(&name));
        let serial = *self.workspace_serial() + 1;
        self.as_mut().set_workspace_serial(serial);
        self.as_mut().set_screen(text("workspace"));
    }

    /// Opens the workspace in `root`; the reason when it cannot be.
    fn open_root(mut self: Pin<&mut Self>, root: &Path) -> Option<String> {
        // One workspace at a time: the open one lets go of its folder first.
        session::set_current(None);
        match Engine::open_workspace(root, &self.dirs()) {
            Ok(opened) => {
                self.as_mut().show(opened);
                None
            }
            Err(e) => Some(e.to_string()),
        }
    }

    pub fn start(mut self: Pin<&mut Self>) {
        crate::glue::load_fonts();
        // SAFETY: `self` is a live QObject made by QML, whose engine gets the thumbnail provider.
        unsafe {
            let object = self.as_mut().get_unchecked_mut() as *mut Self as *mut std::ffi::c_void;
            crate::glue::install_thumbnails(object);
        }
        let launch = crate::launch();
        {
            let mut rust = self.as_mut().rust_mut();
            rust.dirs = Some(launch.dirs.clone());
            rust.pictures = launch.pictures.clone();
        }
        let settings = AppSettings::load(&self.settings_path());
        let effective = resolve_language(&settings.language);
        // The language is installed before the first screen exists, so that nothing is drawn in
        // another one (and so that every window of a test process starts from its own machine's).
        // SAFETY: `self` is a live QObject made by QML; its engine retranslates.
        unsafe {
            let object = self.as_mut().get_unchecked_mut() as *mut Self as *mut std::ffi::c_void;
            crate::glue::set_translation(translation_for(effective), object);
        }
        self.as_mut().set_language(text(&settings.language));
        self.as_mut().set_effective_language(text(effective));
        self.as_mut().set_screen(text("welcome"));

        // A workspace named at launch first, else the last one opened.
        if let Some(path) = &launch.open {
            if let Some(reason) = self.as_mut().open_root(path) {
                // (A code and its details: the sentence is QML's, so that it is translated.)
                let note = format!("open:{}\t{reason}", path.display());
                self.as_mut().set_note(text(&note));
            }
            return;
        }
        // Issue #11: skip straight to the Welcome screen's known-workspaces list when that is
        // what was asked for, instead of reopening the last workspace.
        if settings.startup_behavior != "list"
            && let Some(last) = Engine::last_opened_workspace(&launch.dirs)
        {
            let shown = last.found && self.as_mut().open_root(&last.path).is_none();
            if !shown {
                let note = format!("lost:{}", last.path.display());
                self.as_mut().set_note(text(&note));
            }
        }
    }

    pub fn version(&self) -> QString {
        text(Engine::version())
    }

    pub fn use_machine(&self, name: &QString) {
        crate::use_machine(&name.to_string());
    }

    pub fn env(&self, name: &QString) -> QString {
        let value = std::env::var(name.to_string()).unwrap_or_default();
        text(&value)
    }

    pub fn default_parent(&self) -> QString {
        // The parent folder chosen last time (issue #9), else `~/Pictures/Auroraw`.
        let remembered = match &self.dirs {
            Some(_) => AppSettings::load(&self.settings_path()).last_workspace_folder,
            None => String::new(),
        };
        if remembered.is_empty() {
            text(&self.pictures.join("Auroraw").to_string_lossy())
        } else {
            text(&remembered)
        }
    }

    pub fn free_name(&self, parent: &QString, base: &QString) -> QString {
        let parent = paths::resolve(&parent.to_string()).unwrap_or_default();
        text(&free_workspace_name(&parent, &base.to_string()))
    }

    pub fn preview(&self, name: &QString, parent: &QString) -> QString {
        let name = name.to_string();
        match (name.trim(), paths::resolve(&parent.to_string())) {
            ("", _) | (_, Err(_)) => QString::default(),
            (name, Ok(parent)) => text(
                &parent
                    .join(paths::folder_name(name, "Workspace"))
                    .to_string_lossy(),
            ),
        }
    }

    pub fn create(mut self: Pin<&mut Self>, name: &QString, parent: &QString) -> QString {
        let name = name.to_string();
        let name = name.trim();
        let parent = match paths::resolve(&parent.to_string()) {
            Ok(parent) => parent,
            Err(e) => return text(&e.to_string()),
        };
        let root = parent.join(paths::folder_name(name, "Workspace"));
        if !is_free(&root) {
            return text(&format!("taken:{}", root.display()));
        }
        session::set_current(None);
        match Engine::create_workspace(&root, name, &self.dirs()) {
            Ok(opened) => {
                // Remembered for next time (issue #9), best effort.
                let path = self.settings_path();
                let settings = AppSettings {
                    last_workspace_folder: parent.to_string_lossy().into_owned(),
                    ..AppSettings::load(&path)
                };
                settings.save(&path);
                self.as_mut().show(opened);
                QString::default()
            }
            Err(e) => text(&e.to_string()),
        }
    }

    pub fn open_path(mut self: Pin<&mut Self>, path: &QString) -> QString {
        let root = match paths::resolve(&path.to_string()) {
            Ok(root) => root,
            Err(e) => return text(&e.to_string()),
        };
        match self.as_mut().open_root(&root) {
            Some(reason) => text(&reason),
            None => QString::default(),
        }
    }

    pub fn languages(&self) -> QStringList {
        LANGUAGES.iter().map(|code| text(code)).collect()
    }

    pub fn choose_language(mut self: Pin<&mut Self>, code: &QString) {
        let code = code.to_string();
        let settings = AppSettings {
            language: code.clone(),
            ..AppSettings::load(&self.settings_path())
        };
        settings.save(&self.settings_path());
        let effective = resolve_language(&code);
        // SAFETY: `self` is a live QObject made by QML; its engine retranslates.
        unsafe {
            let object = self.as_mut().get_unchecked_mut() as *mut Self as *mut std::ffi::c_void;
            crate::glue::set_translation(translation_for(effective), object);
        }
        self.as_mut().set_language(text(&code));
        self.as_mut().set_effective_language(text(effective));
    }

    pub fn keyword_panel_width(&self) -> i32 {
        match &self.dirs {
            Some(_) => AppSettings::load(&self.settings_path()).keyword_panel_width,
            None => crate::app_settings::KEYWORD_PANEL_WIDTH,
        }
    }

    pub fn view_option(&self, name: &QString) -> bool {
        let settings = match &self.dirs {
            Some(_) => AppSettings::load(&self.settings_path()),
            None => AppSettings::default(),
        };
        match name.to_string().as_str() {
            "autoAdvance" => settings.auto_advance,
            "filmstrip" => settings.show_filmstrip,
            "info" => settings.show_info,
            "peaking" => settings.show_peaking,
            "clipping" => settings.show_clipping,
            "histogram" => settings.show_histogram,
            _ => false,
        }
    }

    pub fn set_view_option(&self, name: &QString, on: bool) {
        if self.dirs.is_none() {
            return;
        }
        let path = self.settings_path();
        let mut settings = AppSettings::load(&path);
        match name.to_string().as_str() {
            "autoAdvance" => settings.auto_advance = on,
            "filmstrip" => settings.show_filmstrip = on,
            "info" => settings.show_info = on,
            "peaking" => settings.show_peaking = on,
            "clipping" => settings.show_clipping = on,
            "histogram" => settings.show_histogram = on,
            _ => return,
        }
        settings.save(&path);
    }

    fn settings_now(&self) -> AppSettings {
        match &self.dirs {
            Some(_) => AppSettings::load(&self.settings_path()),
            None => AppSettings::default(),
        }
    }

    pub fn int_option(&self, name: &QString) -> i32 {
        let settings = self.settings_now();
        match name.to_string().as_str() {
            "thumbSize" => settings.thumb_size as i32,
            "comparePanes" => settings.compare_panes as i32,
            "similarDistance" => settings.similar_distance as i32,
            "similarMinutes" => settings.similar_minutes as i32,
            _ => 0,
        }
    }

    pub fn set_int_option(&self, name: &QString, value: i32) {
        if self.dirs.is_none() {
            return;
        }
        let path = self.settings_path();
        let mut settings = AppSettings::load(&path);
        match name.to_string().as_str() {
            "thumbSize" => settings.thumb_size = value.clamp(96, 256) as u32,
            "comparePanes" => settings.compare_panes = value.clamp(2, 4) as u32,
            "similarDistance" => settings.similar_distance = value.clamp(1, 24) as u32,
            "similarMinutes" => settings.similar_minutes = value.clamp(1, 10_080) as u32,
            _ => return,
        }
        settings.save(&path);
    }

    pub fn series_gap(&self) -> i32 {
        match &self.dirs {
            Some(_) => AppSettings::load(&self.settings_path()).series_gap as i32,
            None => AppSettings::default().series_gap as i32,
        }
    }

    pub fn set_series_gap(&self, seconds: i32) {
        let seconds = seconds.clamp(0, 600) as u32;
        if self.dirs.is_some() {
            let path = self.settings_path();
            let mut settings = AppSettings::load(&path);
            settings.series_gap = seconds;
            settings.save(&path);
        }
        if let Some(session) = crate::session::current() {
            let _ = session
                .engine
                .submit(auroraw_engine::Command::SetSeriesGap { seconds });
        }
    }

    pub fn startup_behavior(&self) -> QString {
        text(&self.settings_now().startup_behavior)
    }

    pub fn set_startup_behavior(&self, value: &QString) {
        if self.dirs.is_none() {
            return;
        }
        let path = self.settings_path();
        let mut settings = AppSettings::load(&path);
        settings.startup_behavior = match value.to_string().as_str() {
            "list" => "list".into(),
            _ => "reopen".into(),
        };
        settings.save(&path);
    }

    pub fn regroup_series(&self) {
        if let Some(session) = crate::session::current() {
            let _ = session
                .engine
                .submit(auroraw_engine::Command::DetectSeries { regroup: true });
        }
    }

    pub fn set_keyword_panel_width(&self, width: i32) {
        if self.dirs.is_some() {
            let path = self.settings_path();
            AppSettings {
                keyword_panel_width: width,
                ..AppSettings::load(&path)
            }
            .save(&path);
        }
    }
}
