// SPDX-License-Identifier: GPL-3.0-or-later
//! The exact-duplicates report (WP9, D-036, D-108): read-only, like `sources_api`/`similar_api`. A duplicate is
//! detected and confirmed during a scan (`coordinator::scan_source`'s `SecondLocation` handling); this only reads
//! what was found, and resolves one specific location's file for "show in file manager" (not necessarily a photo's
//! current primary location).

use std::path::PathBuf;

pub use auroraw_catalogue::{DuplicatePhoto, LocationRef};
use auroraw_types::SourceId;

use crate::Engine;
use crate::error::Result;
use crate::thumbnails::source_root;

impl Engine {
    /// Every photo that exists at more than one location, its primary and every confirmed secondary one. Nothing
    /// here decides which copy to keep, or deletes anything (D-036, D-018).
    pub fn duplicate_photos(&self) -> Result<Vec<DuplicatePhoto>> {
        Ok(self.read_catalogue()?.duplicate_photos()?)
    }

    /// One specific location's file on this machine, `None` when its source has no root here (offline, or not
    /// registered on this machine). Used to reveal one of a duplicate's locations, which is not necessarily the
    /// photo's current primary one.
    pub fn location_path(&self, source_id: SourceId, path: &str) -> Option<PathBuf> {
        let root = source_root(self.workspace(), source_id)?;
        Some(root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR)))
    }
}

/// The duplicates report as plain text, one photo per block, its primary and every extra location named by
/// source and path (issue #13): shared by the CLI's own `duplicates` command and the UI's "Export the list…", so
/// the two never drift apart on what the report says.
pub fn format_report(duplicates: &[DuplicatePhoto]) -> String {
    let mut out = String::new();
    for photo in duplicates {
        out.push_str(&photo.filename);
        out.push('\n');
        out.push_str(&format!(
            "  {} — {}\n",
            photo.primary.source_name, photo.primary.path
        ));
        for extra in &photo.extra {
            out.push_str(&format!("  {} — {}\n", extra.source_name, extra.path));
        }
    }
    out.push_str(&format!("{} duplicate photo(s)\n", duplicates.len()));
    out
}
