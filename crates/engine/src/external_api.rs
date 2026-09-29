// SPDX-License-Identifier: GPL-3.0-or-later
//! The changes other applications made to XMP files next to originals, waiting for an answer (D-047,
//! WP10): read-only, like `duplicates_api`. A scan notices them and stores the file's newer fields
//! beside the base (`Catalogue::set_external_pending`); this recomputes, from those stored fields and the
//! photos as they are now, what accepting would do. **No source is touched**, so the review works with
//! the source offline; accepting applies from the stored fields too (`Command::AcceptExternalChanges`).

pub use auroraw_format::sidecar::external::Field as ExternalField;
use auroraw_types::PhotoId;

use crate::Engine;
use crate::error::Result;
use crate::external_merge::pending_diff;

/// One field of a photo that the file's newer state changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalChange {
    /// Which field.
    pub field: ExternalField,
    /// What the photo says now, as text (`""` for none; the rating as `-1` to `5`).
    pub mine: String,
    /// What the file says now.
    pub file: String,
    /// Changed on both sides to different values: accepting keeps the photo's value unless told to
    /// take the file's.
    pub conflict: bool,
}

/// A photo whose external XMP file changed, with what would change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalPhoto {
    /// The photo.
    pub photo_id: PhotoId,
    /// Its original file's name.
    pub filename: String,
    /// The XMP file, inside the photo's source.
    pub xmp_path: String,
    /// The fields the file changed (not the keywords), in the order they are listed to a person.
    pub changes: Vec<ExternalChange>,
    /// Keywords the file gained that the photo lacks.
    pub keywords_added: Vec<String>,
    /// Keywords the file lost that the photo still has.
    pub keywords_removed: Vec<String>,
}

impl Engine {
    /// Every photo with a change waiting for an answer, by XMP file path. A change whose fields the photo
    /// already agrees with (edited by hand to the same values since) is not one, and is not listed.
    pub fn external_changes(&self) -> Result<Vec<ExternalPhoto>> {
        let catalogue = self.read_catalogue()?;
        let rows = catalogue.pending_externals()?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let key_paths = self
            .workspace()
            .read_vocabulary()
            .ok()
            .flatten()
            .and_then(|loaded| loaded.current())
            .map(|v| auroraw_catalogue::keyword_paths(&v.keywords))
            .unwrap_or_default();
        let mut out = Vec::new();
        for row in rows {
            let Some(photo) = self
                .workspace()
                .read_photo(&row.photo_id)
                .ok()
                .flatten()
                .and_then(|loaded| loaded.current())
            else {
                continue;
            };
            let Some(diff) = pending_diff(&row, &photo.meta, &key_paths) else {
                continue;
            };
            if diff.is_empty() {
                continue;
            }
            let mut changes: Vec<ExternalChange> = diff
                .taken
                .into_iter()
                .map(|c| ExternalChange {
                    field: c.field,
                    mine: c.mine,
                    file: c.file,
                    conflict: false,
                })
                .chain(diff.conflicts.into_iter().map(|c| ExternalChange {
                    field: c.field,
                    mine: c.mine,
                    file: c.file,
                    conflict: true,
                }))
                .collect();
            changes.sort_by_key(|c| {
                ExternalField::ALL
                    .iter()
                    .position(|f| *f == c.field)
                    .unwrap_or(usize::MAX)
            });
            let filename = catalogue
                .photo(&row.photo_id)
                .ok()
                .flatten()
                .map(|p| p.filename)
                .unwrap_or_default();
            out.push(ExternalPhoto {
                photo_id: row.photo_id,
                filename,
                xmp_path: row.path,
                changes,
                keywords_added: diff.keywords.add,
                keywords_removed: diff.keywords.remove,
            });
        }
        Ok(out)
    }

    /// How many photos have a change waiting for an answer (the banner's number).
    pub fn external_pending(&self) -> usize {
        self.external_changes().map_or(0, |changes| changes.len())
    }
}
