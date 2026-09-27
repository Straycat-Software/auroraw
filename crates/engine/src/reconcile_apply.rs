// SPDX-License-Identifier: GPL-3.0-or-later
//! What one `sources::relink::ScanOutcome` means for the catalogue, once we know whether its path is a photo's
//! *primary* location (`photo.path`) or one of its *secondary* ones (the `location` table, D-036, D-108).
//!
//! Shared between `coordinator::scan_source` (applies a [`CatalogueAction`] directly, since it already owns the
//! catalogue, on the coordinator thread) and `index_job` (sends one over `Inbound::Apply`, since a background
//! job never writes the catalogue itself): before this, the two had grown their own, independent copies of this
//! same primary/secondary branching, and `index_job`'s copy had simply never been written for the
//! `OriginalChanged`/`Relinked`/`Missing` outcomes at all (issues #6, #7, #8). One decision function, two
//! appliers, so the two paths cannot drift apart on what an outcome means again.
//!
//! `Confirmed` (nothing changed) and `New`/`Ambiguous`/`SecondLocation` (not yet a known photo, or only a
//! candidate) are not part of this: the first needs no per-path decision (both callers just clear flags
//! unconditionally, which is safe — see `coordinator::scan_source`'s own comment on why), and the rest are
//! about *becoming* a photo or a confirmed duplicate, a bigger job each caller already does its own way (RAW+JPEG
//! pairing and restoring for `index_job`, nothing but a report for `scan_source`).

use auroraw_types::{Fingerprint, PhotoId, SourceId};

/// What to do to the catalogue for one `OriginalChanged`, `Relinked` or `Missing` outcome.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CatalogueAction {
    /// A primary file's content changed at its known path.
    MarkChanged { photo_id: PhotoId },
    /// A secondary (duplicate) location's content changed: not the same file any more, so it is no longer a
    /// verified duplicate. Nothing on disk is touched; only Auroraw's own record of it is dropped.
    DropLocation {
        photo_id: PhotoId,
        source_id: SourceId,
        path: String,
    },
    /// A primary file moved or was renamed within its source: applied silently (D-019), as it always has been.
    Relink {
        photo_id: PhotoId,
        source_id: SourceId,
        to: String,
        filename: String,
        fingerprint: Fingerprint,
    },
    /// A secondary location moved or was renamed within its source: follows it in the `location` table: the
    /// photo's own row (its primary) is never touched by this.
    RelinkLocation {
        photo_id: PhotoId,
        source_id: SourceId,
        from: String,
        to: String,
        filename: String,
        fingerprint: Fingerprint,
    },
    /// A primary file is nowhere to be found this scan. Kept, never removed automatically (D-019, D-031).
    MarkMissing { photo_id: PhotoId },
}

/// Whether `path` is the photo's primary location or a secondary one: looked up in the map
/// `known_files_in_source`'s own flag builds (`true` when the path is missing from the map, matching every
/// caller's existing convention — a path reconcile already classified as known for this source but that somehow
/// is not in the map is treated as primary, the safer default).
fn is_primary(path: &str, primary_paths: &std::collections::HashMap<String, bool>) -> bool {
    primary_paths.get(path).copied().unwrap_or(true)
}

pub(crate) fn changed(
    photo_id: PhotoId,
    source_id: SourceId,
    path: String,
    primary_paths: &std::collections::HashMap<String, bool>,
) -> CatalogueAction {
    if is_primary(&path, primary_paths) {
        CatalogueAction::MarkChanged { photo_id }
    } else {
        CatalogueAction::DropLocation {
            photo_id,
            source_id,
            path,
        }
    }
}

pub(crate) fn missing(
    photo_id: PhotoId,
    source_id: SourceId,
    path: String,
    primary_paths: &std::collections::HashMap<String, bool>,
) -> CatalogueAction {
    if is_primary(&path, primary_paths) {
        CatalogueAction::MarkMissing { photo_id }
    } else {
        CatalogueAction::DropLocation {
            photo_id,
            source_id,
            path,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn relinked(
    photo_id: PhotoId,
    source_id: SourceId,
    from: String,
    to: String,
    filename: String,
    fingerprint: Fingerprint,
    primary_paths: &std::collections::HashMap<String, bool>,
) -> CatalogueAction {
    if is_primary(&from, primary_paths) {
        CatalogueAction::Relink {
            photo_id,
            source_id,
            to,
            filename,
            fingerprint,
        }
    } else {
        CatalogueAction::RelinkLocation {
            photo_id,
            source_id,
            from,
            to,
            filename,
            fingerprint,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo() -> PhotoId {
        PhotoId::random()
    }

    fn source() -> SourceId {
        SourceId::random()
    }

    fn fp() -> Fingerprint {
        Fingerprint::from_bytes([7; 32])
    }

    fn primary_map(secondary_path: &str) -> std::collections::HashMap<String, bool> {
        std::collections::HashMap::from([(secondary_path.to_string(), false)])
    }

    #[test]
    fn a_primary_path_changed_marks_the_photo_changed() {
        let id = photo();
        let action = changed(
            id,
            source(),
            "a.jpg".into(),
            &std::collections::HashMap::new(),
        );
        assert_eq!(action, CatalogueAction::MarkChanged { photo_id: id });
    }

    #[test]
    fn a_secondary_path_changed_drops_the_location_instead() {
        let id = photo();
        let src = source();
        let action = changed(id, src, "b.jpg".into(), &primary_map("b.jpg"));
        assert_eq!(
            action,
            CatalogueAction::DropLocation {
                photo_id: id,
                source_id: src,
                path: "b.jpg".into()
            }
        );
    }

    #[test]
    fn a_primary_path_missing_marks_the_photo_missing() {
        let id = photo();
        let action = missing(
            id,
            source(),
            "a.jpg".into(),
            &std::collections::HashMap::new(),
        );
        assert_eq!(action, CatalogueAction::MarkMissing { photo_id: id });
    }

    #[test]
    fn a_secondary_path_missing_drops_the_location_instead() {
        let id = photo();
        let src = source();
        let action = missing(id, src, "b.jpg".into(), &primary_map("b.jpg"));
        assert_eq!(
            action,
            CatalogueAction::DropLocation {
                photo_id: id,
                source_id: src,
                path: "b.jpg".into()
            }
        );
    }

    #[test]
    fn a_primary_relink_applies_to_the_photos_own_row() {
        let id = photo();
        let src = source();
        let action = relinked(
            id,
            src,
            "old.jpg".into(),
            "new.jpg".into(),
            "new.jpg".into(),
            fp(),
            &std::collections::HashMap::new(),
        );
        assert_eq!(
            action,
            CatalogueAction::Relink {
                photo_id: id,
                source_id: src,
                to: "new.jpg".into(),
                filename: "new.jpg".into(),
                fingerprint: fp(),
            }
        );
    }

    #[test]
    fn a_secondary_relink_follows_the_location_only() {
        let id = photo();
        let src = source();
        let action = relinked(
            id,
            src,
            "old.jpg".into(),
            "new.jpg".into(),
            "new.jpg".into(),
            fp(),
            &primary_map("old.jpg"),
        );
        assert_eq!(
            action,
            CatalogueAction::RelinkLocation {
                photo_id: id,
                source_id: src,
                from: "old.jpg".into(),
                to: "new.jpg".into(),
                filename: "new.jpg".into(),
                fingerprint: fp(),
            }
        );
    }
}
