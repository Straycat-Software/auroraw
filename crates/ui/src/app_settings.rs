// SPDX-License-Identifier: GPL-3.0-or-later
//! What the application remembers about a person, not about a workspace: for now, the language of
//! the interface and the width of the keyword panel. A plain JSON file in the machine's data folder; a missing or unreadable file is a
//! first run, never an error.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The languages the interface is translated into: English (the source text of every string) and the
/// `.ts` files of `i18n/`.
pub const LANGUAGES: &[&str] = &["en", "fr"];

/// The application's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// `system` (follow the machine) or one of [`LANGUAGES`].
    pub language: String,
    /// The width the keyword panel was dragged to, in pixels.
    pub keyword_panel_width: i32,
    /// The image view moves on to the next photo after a rating, flag or label key.
    pub auto_advance: bool,
    /// The image view shows its filmstrip.
    pub show_filmstrip: bool,
    /// The image view shows the line about the photo.
    pub show_info: bool,
    /// The largest gap, in seconds, between two photos of one series (D-101).
    pub series_gap: u32,
    /// The image view and the comparison show the focus peaking overlay (D-103).
    pub show_peaking: bool,
    /// ...the clipping overlay.
    pub show_clipping: bool,
    /// ...the histogram.
    pub show_histogram: bool,
    /// The size of the grid's thumbnails, in pixels on the long side (96 to 256).
    pub thumb_size: u32,
    /// How many photos the comparison shows side by side (2 to 4).
    pub compare_panes: u32,
    /// Photos are similar when at most this many of their hashes' 64 bits differ (D-105).
    pub similar_distance: u32,
    /// ...and were taken at most this many minutes apart.
    pub similar_minutes: u32,
    /// Where a folder or file picker last went, keyed by a short name of its own (issues #9, #10,
    /// #18): `"workspace"` (the New workspace dialog's parent folder), `"source"` (the Add Source
    /// dialog), and one per picker with no field of its own to remember its choice through instead
    /// (`"open-workspace"`, `"export-photos"`, `"export-duplicates"`). One map rather than a field
    /// per picker, so a new one needs only a new key, not a new field here and at every call site.
    pub last_folders: HashMap<String, String>,
    /// What to do at launch, when none was named on the command line (issue #11): `"reopen"` (the
    /// last workspace) or `"list"` (the Welcome screen's known workspaces).
    pub startup_behavior: String,
}

/// The keyword panel's width when nothing was chosen, and the limits of what can be.
pub const KEYWORD_PANEL_WIDTH: i32 = 280;
pub const KEYWORD_PANEL_MIN: i32 = 200;
pub const KEYWORD_PANEL_MAX: i32 = 640;

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "system".into(),
            keyword_panel_width: KEYWORD_PANEL_WIDTH,
            auto_advance: false,
            show_filmstrip: true,
            show_info: true,
            series_gap: 2,
            show_peaking: false,
            show_clipping: false,
            show_histogram: false,
            thumb_size: 160,
            compare_panes: 2,
            similar_distance: 10,
            similar_minutes: 30,
            last_folders: HashMap::new(),
            startup_behavior: "reopen".into(),
        }
    }
}

/// Where the machine keeps `AppSettings`, for anything that needs to read or change one field of it
/// without going through `Launcher` (`SourceList::add`, for instance).
pub(crate) fn settings_path() -> std::path::PathBuf {
    crate::launch().dirs.data.join("app-settings.json")
}

impl AppSettings {
    /// Reads `path`, or the defaults if it is missing or not readable as settings.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .map(|mut settings| {
                // A hand-edited file cannot make the panel unusable.
                settings.keyword_panel_width = settings
                    .keyword_panel_width
                    .clamp(KEYWORD_PANEL_MIN, KEYWORD_PANEL_MAX);
                settings.thumb_size = settings.thumb_size.clamp(96, 256);
                settings.compare_panes = settings.compare_panes.clamp(2, 4);
                settings.similar_distance = settings.similar_distance.clamp(1, 24);
                settings.similar_minutes = settings.similar_minutes.clamp(1, 10_080);
                if settings.startup_behavior != "list" {
                    settings.startup_behavior = "reopen".into();
                }
                settings
            })
            .unwrap_or_default()
    }

    /// Writes `path`, best effort: failing to remember a language is not worth an error dialog.
    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let bytes = serde_json::to_vec_pretty(self).expect("settings always serialise");
        let _ = std::fs::write(path, bytes);
    }

    /// Where the picker named `key` last went, empty for none yet (issue #18).
    pub fn last_folder(path: &Path, key: &str) -> String {
        Self::load(path)
            .last_folders
            .get(key)
            .cloned()
            .unwrap_or_default()
    }

    /// Remembers `folder` as where the picker named `key` last went, best effort (issue #18).
    pub fn remember_folder(path: &Path, key: &str, folder: &str) {
        let mut settings = Self::load(path);
        settings
            .last_folders
            .insert(key.to_string(), folder.to_string());
        settings.save(path);
    }
}

/// The language to use for a setting: itself when it is one of ours, else the machine's own when
/// that is one of ours, else English.
pub fn resolve_language(setting: &str) -> &'static str {
    let wanted = if LANGUAGES.contains(&setting) {
        setting.to_string()
    } else {
        sys_locale::get_locale().unwrap_or_default()
    };
    let primary = wanted
        .split(['-', '_', '.', '@'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    LANGUAGES
        .iter()
        .copied()
        .find(|language| *language == primary)
        .unwrap_or("en")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_corrupt_file_is_the_defaults_and_a_chosen_language_round_trips() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("nested/app-settings.json");
        assert_eq!(AppSettings::load(&path).language, "system");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(AppSettings::load(&path), AppSettings::default());

        let chosen = AppSettings {
            language: "fr".into(),
            keyword_panel_width: 350,
            auto_advance: true,
            show_filmstrip: false,
            show_info: false,
            series_gap: 5,
            show_peaking: true,
            show_clipping: true,
            show_histogram: true,
            thumb_size: 200,
            compare_panes: 3,
            similar_distance: 12,
            similar_minutes: 45,
            last_folders: HashMap::from([
                (
                    "workspace".to_string(),
                    "/home/patrick/Pictures".to_string(),
                ),
                ("source".to_string(), "/mnt/backup".to_string()),
            ]),
            startup_behavior: "list".into(),
        };
        chosen.save(&path);
        assert_eq!(AppSettings::load(&path), chosen);
    }

    #[test]
    fn a_pickers_last_folder_is_remembered_by_its_own_key_and_does_not_touch_another() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("app-settings.json");
        assert_eq!(AppSettings::last_folder(&path, "export-photos"), "");

        AppSettings::remember_folder(&path, "export-photos", "/home/patrick/Pictures");
        assert_eq!(
            AppSettings::last_folder(&path, "export-photos"),
            "/home/patrick/Pictures"
        );
        assert_eq!(AppSettings::last_folder(&path, "export-duplicates"), "");

        AppSettings::remember_folder(&path, "export-duplicates", "/home/patrick/Reports");
        assert_eq!(
            AppSettings::last_folder(&path, "export-photos"),
            "/home/patrick/Pictures",
            "remembering a second key does not disturb the first"
        );
    }

    #[test]
    fn the_panel_width_has_a_default_and_stays_within_its_limits() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("app-settings.json");
        // A file from before the width existed.
        std::fs::write(&path, br#"{ "language": "fr" }"#).unwrap();
        let old = AppSettings::load(&path);
        assert_eq!(old.language, "fr");
        assert_eq!(old.keyword_panel_width, KEYWORD_PANEL_WIDTH);
        for (written, read) in [
            (5, KEYWORD_PANEL_MIN),
            (5000, KEYWORD_PANEL_MAX),
            (400, 400),
        ] {
            std::fs::write(&path, format!(r#"{{ "keyword_panel_width": {written} }}"#)).unwrap();
            assert_eq!(AppSettings::load(&path).keyword_panel_width, read);
        }
    }

    #[test]
    fn a_chosen_language_is_used_and_anything_else_falls_back_to_a_bundled_one() {
        assert_eq!(resolve_language("fr"), "fr");
        assert_eq!(resolve_language("en"), "en");
        // "system" and unknown codes resolve to something we translate, whatever the machine says.
        assert!(LANGUAGES.contains(&resolve_language("system")));
        assert!(LANGUAGES.contains(&resolve_language("klingon")));
    }
}
