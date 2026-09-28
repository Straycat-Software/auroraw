// SPDX-License-Identifier: GPL-3.0-or-later
//! Where a folder dialog should open.

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
        type Folders = super::FoldersRust;

        /// The folder a dialog should open at for what is typed in a field: the folder itself when it
        /// exists, else the closest folder above it that does, else an empty text (the system's own
        /// default place).
        #[qinvokable]
        fn closest(self: &Folders, typed: &QString) -> QString;

        /// Where the picker named `key` last went, empty for none yet (issue #18): for a picker with
        /// no field of its own to remember its choice through (`FolderPicker`'s and
        /// `FileSaveDialog`'s own `rememberAs`).
        #[qinvokable]
        fn last(self: &Folders, key: &QString) -> QString;

        /// Remembers `folder` as where the picker named `key` last went (issue #18).
        #[qinvokable]
        fn remember(self: &Folders, key: &QString, folder: &QString);
    }
}

use cxx_qt_lib::QString;

use crate::app_settings::{self, AppSettings};

/// Holds nothing.
#[derive(Default)]
pub struct FoldersRust {}

/// See [`qobject::Folders::closest`].
pub fn closest_folder(typed: &str) -> Option<std::path::PathBuf> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    std::path::Path::new(typed)
        .ancestors()
        .find(|candidate| !candidate.as_os_str().is_empty() && candidate.is_dir())
        .map(std::path::Path::to_path_buf)
}

impl qobject::Folders {
    pub fn closest(&self, typed: &QString) -> QString {
        closest_folder(&typed.to_string())
            .map(|folder| QString::from(folder.to_string_lossy().as_ref()))
            .unwrap_or_default()
    }

    pub fn last(&self, key: &QString) -> QString {
        QString::from(
            AppSettings::last_folder(&app_settings::settings_path(), &key.to_string()).as_str(),
        )
    }

    pub fn remember(&self, key: &QString, folder: &QString) {
        AppSettings::remember_folder(
            &app_settings::settings_path(),
            &key.to_string(),
            &folder.to_string(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialog_opens_at_the_closest_folder_that_exists() {
        let dir = auroraw_testkit::temp_dir();
        let existing = dir.path().join("Photos");
        std::fs::create_dir_all(&existing).unwrap();

        assert_eq!(closest_folder(""), None);
        assert_eq!(closest_folder("   "), None);
        assert_eq!(
            closest_folder(&existing.to_string_lossy()),
            Some(existing.clone())
        );
        assert_eq!(
            closest_folder(&existing.join("2026/September/half-typed").to_string_lossy()),
            Some(existing),
            "the folders that do not exist yet are skipped"
        );
    }
}
