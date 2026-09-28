// SPDX-License-Identifier: GPL-3.0-or-later
//! The duplicates report (WP9, D-036, D-108): every photo with more than one confirmed location, read-only, and
//! "show in file manager" for one specific location (which is not necessarily a photo's current primary one, so
//! `PhotoGrid::showInFileManager` — a photo id, its primary — does not fit here).

use std::str::FromStr;

use auroraw_types::SourceId;
use cxx_qt_lib::QString;

use crate::reveal;
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
        type Duplicates = super::DuplicatesRust;

        /// Every photo with more than one confirmed location, as JSON: an array of `{filename, primary: {source,
        /// path}, extra: [{source, path}]}`, one entry a photo, its primary first.
        #[qinvokable]
        fn list(self: &Duplicates) -> QString;

        /// Opens one location's file in the platform's file manager (D-106's `reveal`, same test-mode stub).
        /// `sourceId` is the location's source (not necessarily the photo's own primary source). `false` when the
        /// id does not parse or the source has no root here (offline, or not registered on this machine).
        #[qinvokable]
        #[cxx_name = "revealLocation"]
        fn reveal_location(self: &Duplicates, source_id: &QString, path: &QString) -> bool;

        /// Writes the report (the same text `duplicates` prints on the CLI, issue #13) to `path`; `false` on a
        /// write error.
        #[qinvokable]
        #[cxx_name = "exportTo"]
        fn export_to(self: &Duplicates, path: &QString) -> bool;
    }
}

#[derive(Default)]
pub struct DuplicatesRust {}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

impl qobject::Duplicates {
    pub fn list(&self) -> QString {
        let Some(session) = session::current() else {
            return QString::from("[]");
        };
        let duplicates = session.engine.duplicate_photos().unwrap_or_default();
        let location = |l: &auroraw_engine::LocationRef| {
            format!(
                "{{\"sourceId\":\"{}\",\"sourceName\":\"{}\",\"path\":\"{}\"}}",
                l.source_id,
                escape(&l.source_name),
                escape(&l.path)
            )
        };
        let entries: Vec<String> = duplicates
            .iter()
            .map(|d| {
                let extra: Vec<String> = d.extra.iter().map(location).collect();
                format!(
                    "{{\"id\":\"{}\",\"filename\":\"{}\",\"primary\":{},\"extra\":[{}]}}",
                    d.photo_id,
                    escape(&d.filename),
                    location(&d.primary),
                    extra.join(",")
                )
            })
            .collect();
        QString::from(format!("[{}]", entries.join(",")).as_str())
    }

    pub fn reveal_location(&self, source_id: &QString, path: &QString) -> bool {
        let (Some(session), Ok(source_id)) = (
            session::current(),
            SourceId::from_str(&source_id.to_string()),
        ) else {
            return false;
        };
        let Some(full) = session.engine.location_path(source_id, &path.to_string()) else {
            return false;
        };
        reveal::reveal(&full);
        true
    }

    pub fn export_to(&self, path: &QString) -> bool {
        let Some(session) = session::current() else {
            eprintln!("Duplicates::exportTo: no session");
            return false;
        };
        let path = path.to_string();
        let duplicates = match session.engine.duplicate_photos() {
            Ok(duplicates) => duplicates,
            Err(e) => {
                eprintln!("Duplicates::exportTo: duplicate_photos failed: {e}");
                return false;
            }
        };
        match std::fs::write(&path, auroraw_engine::duplicates_report(&duplicates)) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("Duplicates::exportTo: write to {path:?} failed: {e}");
                false
            }
        }
    }
}
