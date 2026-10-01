// SPDX-License-Identifier: GPL-3.0-or-later
//! What the interface remembers between launches, per workspace: the fields of the import dialog. A plain JSON file at a path the caller gives (this crate resolves no directory
//! itself, like every crate under `engine`); a missing or unreadable file is a first run, never an
//! error, since nothing here is worth refusing to start over.

use std::path::Path;

use auroraw_engine::{MetadataTemplate, PairRule, Profile};
use serde::{Deserialize, Serialize};

/// The folders and names an import needs, as a person typed them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The folder photos are copied into.
    #[serde(alias = "archive")]
    pub destination: String,
    /// A second folder every photo is also copied into, or empty for none.
    pub backup: String,
    /// The folders-and-names template; empty means [`DEFAULT_TEMPLATE`].
    pub template: String,
    /// The last card or folder imported from.
    pub source: String,
    /// `template`, or `folders` to keep a card's own camera folders.
    pub layout: String,
    /// Whether to add a destination that is not in the catalogue as a source (the checkbox's
    /// last state).
    pub add_destination: bool,
    /// Whether to find the place names of the photos the import registers, once it has finished (design note 008
    /// §4: an option of the import, off by default).
    pub find_places: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            destination: String::new(),
            backup: String::new(),
            template: String::new(),
            source: String::new(),
            layout: "template".into(),
            add_destination: true,
            find_places: false,
        }
    }
}

/// Year folder, then a day folder, then the camera's own file name (M1 plan §6 item 6).
pub const DEFAULT_TEMPLATE: &str = "{year}/{date}/{original}.{ext}";

/// Keeps the card's folders below `DCIM` and every name as it was.
pub const KEEP_FOLDERS_TEMPLATE: &str = "{path}";

impl Settings {
    /// Reads `path`, or the defaults if it is missing or not readable as settings.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Writes `path`, best effort: failing to remember a field is not worth interrupting an
    /// import for.
    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let bytes = serde_json::to_vec_pretty(self).expect("settings always serialise");
        let _ = std::fs::write(path, bytes);
    }

    /// The template to import with: `{path}` (the card's own folders, names as found) when that
    /// layout is chosen, else the person's own, else the default.
    pub fn template_or_default(&self) -> &str {
        let template = self.template.trim();
        if self.layout == "folders" {
            KEEP_FOLDERS_TEMPLATE
        } else if template.is_empty() {
            DEFAULT_TEMPLATE
        } else {
            template
        }
    }

    /// The import profile these fields describe.
    pub fn profile(&self) -> Profile {
        Profile {
            name: "Default".into(),
            destination_template: self.template_or_default().to_string(),
            backup_templates: Vec::new(),
            pair_rule: PairRule::Both,
            metadata_template: MetadataTemplate {
                keyword_paths: Vec::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_corrupt_file_is_a_first_run() {
        let dir = auroraw_testkit::temp_dir();
        assert_eq!(
            Settings::load(&dir.path().join("none.json")),
            Settings::default()
        );
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, b"{ not json").unwrap();
        assert_eq!(Settings::load(&bad), Settings::default());
    }

    #[test]
    fn settings_round_trip_and_tolerate_fields_added_later() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("nested/settings.json");
        let settings = Settings {
            destination: "/photos".into(),
            ..Settings::default()
        };
        settings.save(&path);
        assert_eq!(Settings::load(&path), settings);
        std::fs::write(&path, br#"{"archive": "/a", "somethingNew": 1}"#).unwrap();
        // A file written when the folder was still called the archive keeps working.
        assert_eq!(Settings::load(&path).destination, "/a");
    }

    #[test]
    fn finding_place_names_is_off_until_it_is_asked_for_and_is_remembered() {
        // (Design note 008 §4: the option of the import is off by default; a file from before it has no such field.)
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"destination": "/photos"}"#).unwrap();
        assert!(!Settings::load(&path).find_places);
        let on = Settings {
            find_places: true,
            ..Settings::default()
        };
        on.save(&path);
        assert!(Settings::load(&path).find_places);
    }

    #[test]
    fn an_old_file_with_the_removed_creator_and_rights_fields_still_loads() {
        // Issue #4: the fields are gone, but a settings.json from before that keeps working (serde
        // ignores keys the struct no longer has).
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            br#"{"destination": "/photos", "creator": "Patrick", "rights": "(c) Patrick"}"#,
        )
        .unwrap();
        assert_eq!(Settings::load(&path).destination, "/photos");
    }

    #[test]
    fn the_profile_falls_back_to_the_default_template() {
        let mut settings = Settings::default();
        assert_eq!(settings.profile().destination_template, DEFAULT_TEMPLATE);
        settings.template = "{year}/{original}.{ext}".into();
        let profile = settings.profile();
        settings.layout = "folders".into();
        assert_eq!(settings.profile().destination_template, "{path}");
        settings.layout = "template".into();
        assert_eq!(profile.destination_template, "{year}/{original}.{ext}");
    }
}
