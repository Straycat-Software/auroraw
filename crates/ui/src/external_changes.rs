// SPDX-License-Identifier: GPL-3.0-or-later
//! The changes other applications made to XMP files next to originals, waiting for an answer (WP10, D-047):
//! what the review window lists and what its buttons answer. Read from what a scan stored (nothing here
//! touches a source), answered by commands whose results arrive on the bus like any other edit's.

use std::str::FromStr;

use auroraw_engine::{Command, ExternalField};
use auroraw_types::PhotoId;
use cxx_qt_lib::QString;

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
        type ExternalChanges = super::ExternalChangesRust;

        /// The photos with a change waiting, as JSON: an array of `{id, filename, xmpPath, changes: [{field,
        /// mine, file, conflict}], keywordsAdded: [], keywordsRemoved: []}`, by file path. `field` is the
        /// metadata panel's own key (`title`, ...) or `rating`, `label`; values are text (the rating as
        /// `-1` to `5`, empty for none).
        #[qinvokable]
        fn list(self: &ExternalChanges) -> QString;

        /// How many photos have a change waiting.
        #[qinvokable]
        fn pending(self: &ExternalChanges) -> i32;

        /// Accepts the changes of the photos in `ids` (a JSON array of identifiers): one step of the history.
        /// `useFile` (a JSON array of `{id, field}`) names the conflicts to settle in the file's favour; the
        /// photo's own value stays for the others.
        #[qinvokable]
        fn accept(self: &ExternalChanges, ids: &QString, use_file: &QString);

        /// Declines the changes of the photos in `ids`: the file becomes the base as it stands, nothing is
        /// applied.
        #[qinvokable]
        fn ignore(self: &ExternalChanges, ids: &QString);

        /// Accepts every change waiting, the photo's own value kept wherever both sides changed.
        #[qinvokable]
        #[cxx_name = "acceptAll"]
        fn accept_all(self: &ExternalChanges);

        /// Declines every change waiting.
        #[qinvokable]
        #[cxx_name = "ignoreAll"]
        fn ignore_all(self: &ExternalChanges);
    }
}

#[derive(Default)]
pub struct ExternalChangesRust {}

fn photos_of(json: &QString) -> Vec<PhotoId> {
    serde_json::from_str::<Vec<String>>(&json.to_string())
        .unwrap_or_default()
        .iter()
        .filter_map(|id| PhotoId::from_str(id).ok())
        .collect()
}

fn all_pending() -> Vec<PhotoId> {
    session::current()
        .and_then(|s| s.engine.external_changes().ok())
        .map(|changes| changes.into_iter().map(|c| c.photo_id).collect())
        .unwrap_or_default()
}

impl qobject::ExternalChanges {
    pub fn list(&self) -> QString {
        let Some(session) = session::current() else {
            return QString::from("[]");
        };
        let photos: Vec<serde_json::Value> = session
            .engine
            .external_changes()
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.photo_id.to_string(),
                    "filename": p.filename,
                    "xmpPath": p.xmp_path,
                    "changes": p.changes.iter().map(|c| serde_json::json!({
                        "field": c.field.key(),
                        "mine": c.mine,
                        "file": c.file,
                        "conflict": c.conflict,
                    })).collect::<Vec<_>>(),
                    "keywordsAdded": p.keywords_added,
                    "keywordsRemoved": p.keywords_removed,
                })
            })
            .collect();
        QString::from(serde_json::Value::Array(photos).to_string().as_str())
    }

    pub fn pending(&self) -> i32 {
        session::current().map_or(0, |s| s.engine.external_pending() as i32)
    }

    pub fn accept(&self, ids: &QString, use_file: &QString) {
        let Some(session) = session::current() else {
            return;
        };
        #[derive(serde::Deserialize)]
        struct Choice {
            id: String,
            field: String,
        }
        let use_file: Vec<(PhotoId, ExternalField)> =
            serde_json::from_str::<Vec<Choice>>(&use_file.to_string())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|c| {
                    Some((
                        PhotoId::from_str(&c.id).ok()?,
                        ExternalField::parse(&c.field)?,
                    ))
                })
                .collect();
        let _ = session.engine.submit(Command::AcceptExternalChanges {
            photos: photos_of(ids),
            use_file,
        });
    }

    pub fn ignore(&self, ids: &QString) {
        if let Some(session) = session::current() {
            let _ = session.engine.submit(Command::IgnoreExternalChanges {
                photos: photos_of(ids),
            });
        }
    }

    pub fn accept_all(&self) {
        if let Some(session) = session::current() {
            let _ = session.engine.submit(Command::AcceptExternalChanges {
                photos: all_pending(),
                use_file: Vec::new(),
            });
        }
    }

    pub fn ignore_all(&self) {
        if let Some(session) = session::current() {
            let _ = session.engine.submit(Command::IgnoreExternalChanges {
                photos: all_pending(),
            });
        }
    }
}
