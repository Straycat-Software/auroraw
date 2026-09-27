// SPDX-License-Identifier: GPL-3.0-or-later
//! A photo's secondary locations (WP9, D-036, D-108): the catalogue's `photo` row keeps meaning "the
//! primary location" everywhere else; this is only the *extra* places a photo's file was also found,
//! each confirmed by a whole-file hash before it is recorded (`crate::open`'s schema 3). The duplicates
//! report (D-036: "an optional duplicates report helps the photographer tidy up; Auroraw itself deletes
//! nothing") is `duplicate_photos`: read-only, nothing here removes a photo's *primary* location or a
//! file from disk.

use auroraw_types::{ContentHash, Fingerprint, PhotoId, SourceId};
use rusqlite::{OptionalExtension, params};

use crate::error::Result;
use crate::open::Catalogue;

/// One place a photo's file is: a source, by name, and a path inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationRef {
    /// The source.
    pub source_id: SourceId,
    /// The source's name, as the person gave it.
    pub source_name: String,
    /// The path inside that source.
    pub path: String,
}

/// A photo that exists at more than one location: its primary (the catalogue's own row) and every
/// confirmed secondary one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicatePhoto {
    /// The photo.
    pub photo_id: PhotoId,
    /// Its original file's name.
    pub filename: String,
    /// Its primary location.
    pub primary: LocationRef,
    /// Its other, confirmed locations.
    pub extra: Vec<LocationRef>,
}

impl Catalogue {
    /// A photo's own whole-file hash, if already known (WP7's import, or an earlier confirmation): read before
    /// confirming a candidate second location (D-108), so the primary's file is not reread and rehashed when it does
    /// not have to be.
    pub fn photo_hash(&self, photo_id: &PhotoId) -> Result<Option<ContentHash>> {
        let text: Option<String> = self
            .conn
            .query_row(
                "SELECT hash FROM photo WHERE id = ?1",
                [photo_id.to_string()],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(text.and_then(|h| h.parse().ok()))
    }

    /// Records a photo's confirmed secondary location (D-108): `fingerprint` and `hash` are the whole file's, both
    /// already checked equal to the primary's before this is called. Idempotent: scanning the same location again
    /// changes nothing (the primary key is `(photo_id, source_id, path)`).
    pub fn insert_location(
        &mut self,
        photo_id: &PhotoId,
        source_id: &SourceId,
        path: &str,
        filename: &str,
        fingerprint: &Fingerprint,
        hash: &ContentHash,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO location(photo_id, source_id, path, filename, fingerprint, hash, seen)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%s', 'now'))",
            params![
                photo_id.to_string(),
                source_id.to_string(),
                path,
                filename,
                fingerprint.to_string(),
                hash.to_string(),
            ],
        )?;
        Ok(())
    }

    /// Drops one secondary location: the copy it named is no longer there, no longer verified, or has become the
    /// primary location itself (`apply_relink`). Never an error when there was nothing to drop (pruning a stale
    /// record is not a failure): nothing on disk is touched either way (D-018, D-036).
    pub fn remove_location(
        &mut self,
        photo_id: &PhotoId,
        source_id: &SourceId,
        path: &str,
    ) -> Result<()> {
        self.conn.execute(
            "DELETE FROM location WHERE photo_id = ?1 AND source_id = ?2 AND path = ?3",
            params![photo_id.to_string(), source_id.to_string(), path],
        )?;
        Ok(())
    }

    /// Every photo that has at least one confirmed secondary location: its primary location and every other one,
    /// each with the name of the source that holds it. Read-only; nothing here decides which copy to keep (D-036).
    pub fn duplicate_photos(&self) -> Result<Vec<DuplicatePhoto>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.filename, p.source_id, COALESCE(ps.name, '?'), p.path,
                    l.source_id, COALESCE(ls.name, '?'), l.path
             FROM photo p
             JOIN location l ON l.photo_id = p.id
             LEFT JOIN source ps ON ps.id = p.source_id
             LEFT JOIN source ls ON ls.id = l.source_id
             ORDER BY p.id, l.path",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
            ))
        })?;
        let mut out: Vec<DuplicatePhoto> = Vec::new();
        for row in rows {
            let (
                id,
                filename,
                primary_source_id,
                primary_source,
                primary_path,
                extra_source_id,
                extra_source,
                extra_path,
            ) = row?;
            let Ok(photo_id) = id.parse::<PhotoId>() else {
                continue;
            };
            let Ok(extra_source_id) = extra_source_id.parse::<SourceId>() else {
                continue;
            };
            let extra = LocationRef {
                source_id: extra_source_id,
                source_name: extra_source,
                path: extra_path,
            };
            if let Some(last) = out.last_mut()
                && last.photo_id == photo_id
            {
                last.extra.push(extra);
                continue;
            }
            let Some(primary_source_id) =
                primary_source_id.and_then(|s| s.parse::<SourceId>().ok())
            else {
                continue;
            };
            out.push(DuplicatePhoto {
                photo_id,
                filename,
                primary: LocationRef {
                    source_id: primary_source_id,
                    source_name: primary_source,
                    path: primary_path.unwrap_or_default(),
                },
                extra: vec![extra],
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use auroraw_types::WorkspaceId;

    use super::*;

    fn photo_row(cat: &Catalogue, id: PhotoId, source: &str, path: &str) {
        cat.conn
            .execute(
                "INSERT INTO photo(id, source_id, path, filename, fingerprint, hash, capture_time, sidecar_size)
                 VALUES (?1, ?2, ?3, 'a.jpg', 'fp', 'h', 0, 1)",
                params![id.to_string(), source, path],
            )
            .unwrap();
    }

    fn source_row(cat: &Catalogue, id: &str, name: &str) {
        cat.conn
            .execute(
                "INSERT INTO source(id, kind, name) VALUES (?1, 'local-folder', ?2)",
                params![id, name],
            )
            .unwrap();
    }

    #[test]
    fn a_location_round_trips_and_is_idempotent() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let id = PhotoId::random();
        let source_id = SourceId::random();
        photo_row(&cat, id, &SourceId::random().to_string(), "a.jpg");
        assert!(cat.duplicate_photos().unwrap().is_empty());

        let fp = Fingerprint::from_bytes([1; 32]);
        let hash = ContentHash::from_bytes([2; 32]);
        cat.insert_location(&id, &source_id, "backup/a.jpg", "a.jpg", &fp, &hash)
            .unwrap();
        // Scanning the same location again changes nothing.
        cat.insert_location(&id, &source_id, "backup/a.jpg", "a.jpg", &fp, &hash)
            .unwrap();

        let dups = cat.duplicate_photos().unwrap();
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].photo_id, id);
        assert_eq!(dups[0].extra.len(), 1);
        assert_eq!(dups[0].extra[0].path, "backup/a.jpg");

        cat.remove_location(&id, &source_id, "backup/a.jpg")
            .unwrap();
        assert!(cat.duplicate_photos().unwrap().is_empty());
        // Removing an already-gone location is not an error.
        cat.remove_location(&id, &source_id, "backup/a.jpg")
            .unwrap();
    }

    #[test]
    fn the_report_names_the_sources_and_lists_every_extra_location() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let id = PhotoId::random();
        let (card, backup_disk) = (SourceId::random(), SourceId::random());
        source_row(&cat, &card.to_string(), "Card");
        source_row(&cat, &backup_disk.to_string(), "Backup disk");
        photo_row(&cat, id, &card.to_string(), "a.jpg");
        let fp = Fingerprint::from_bytes([1; 32]);
        let hash = ContentHash::from_bytes([2; 32]);
        cat.insert_location(&id, &backup_disk, "photos/a.jpg", "a.jpg", &fp, &hash)
            .unwrap();

        let dups = cat.duplicate_photos().unwrap();
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].filename, "a.jpg");
        assert_eq!(dups[0].primary.source_name, "Card");
        assert_eq!(dups[0].primary.path, "a.jpg");
        assert_eq!(dups[0].extra[0].source_name, "Backup disk");
        assert_eq!(dups[0].extra[0].path, "photos/a.jpg");
    }

    #[test]
    fn a_photo_with_two_extra_locations_lists_both() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let id = PhotoId::random();
        photo_row(&cat, id, &SourceId::random().to_string(), "a.jpg");
        let fp = Fingerprint::from_bytes([1; 32]);
        let hash = ContentHash::from_bytes([2; 32]);
        cat.insert_location(&id, &SourceId::random(), "b1/a.jpg", "a.jpg", &fp, &hash)
            .unwrap();
        cat.insert_location(&id, &SourceId::random(), "b2/a.jpg", "a.jpg", &fp, &hash)
            .unwrap();
        let dups = cat.duplicate_photos().unwrap();
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].extra.len(), 2);
    }

    /// A sidecar that already carries a second `Location` on its original file (as the coordinator writes one once
    /// a duplicate is confirmed, D-108) keeps it through `insert_photo` (`apply_new_photo` here, and `rebuild` calls
    /// the very same function): the hash makes it a confirmed secondary location, not merely present in the sidecar.
    #[test]
    fn a_sidecar_with_a_second_location_and_a_known_hash_gets_it_in_the_catalogue() {
        use auroraw_format::sidecar::{FileEntry, FileRole, Location};

        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (primary_source, backup_source) = (SourceId::random(), SourceId::random());
        let fp = Fingerprint::from_bytes([3; 32]);
        let hash = ContentHash::from_bytes([4; 32]);
        let mut photo = auroraw_format::sidecar::PhotoSidecar::new(PhotoId::random());
        photo.files.push(FileEntry {
            role: FileRole::Original,
            name: "a.jpg".into(),
            format: None,
            size: 10,
            fingerprint: fp,
            hash: Some(hash),
            locations: vec![
                Location {
                    source: primary_source,
                    path: "a.jpg".into(),
                    seen: None,
                    extra: Vec::new(),
                },
                Location {
                    source: backup_source,
                    path: "backup/a.jpg".into(),
                    seen: None,
                    extra: Vec::new(),
                },
            ],
            extra: Vec::new(),
        });
        cat.apply_new_photo(
            &photo,
            crate::SidecarStat {
                size: 1,
                modified: Some(1),
            },
        )
        .unwrap();

        let dups = cat.duplicate_photos().unwrap();
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].photo_id, photo.photo_id);
        assert_eq!(dups[0].primary.path, "a.jpg");
        assert_eq!(dups[0].extra[0].path, "backup/a.jpg");

        // A rebuild (the same `insert_photo`, through a different entry point) preserves it too.
        let rebuilt = crate::rebuild_to_file(
            &auroraw_testkit::temp_dir().path().join("c.db"),
            cat.workspace_id().unwrap(),
            &crate::RebuildInput {
                photos: &[(
                    photo,
                    crate::SidecarStat {
                        size: 1,
                        modified: Some(1),
                    },
                )],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(rebuilt.duplicate_photos().unwrap().len(), 1);
    }

    /// The reverse: no hash yet on the file means the second location is not confirmed, so a rebuild leaves it out
    /// (nothing was lost from the sidecar; it just is not indexed as a duplicate until confirmed).
    #[test]
    fn a_second_location_with_no_known_hash_yet_is_not_indexed_as_a_duplicate() {
        use auroraw_format::sidecar::{FileEntry, FileRole, Location};

        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let mut photo = auroraw_format::sidecar::PhotoSidecar::new(PhotoId::random());
        photo.files.push(FileEntry {
            role: FileRole::Original,
            name: "a.jpg".into(),
            format: None,
            size: 10,
            fingerprint: Fingerprint::from_bytes([5; 32]),
            hash: None,
            locations: vec![
                Location {
                    source: SourceId::random(),
                    path: "a.jpg".into(),
                    seen: None,
                    extra: Vec::new(),
                },
                Location {
                    source: SourceId::random(),
                    path: "backup/a.jpg".into(),
                    seen: None,
                    extra: Vec::new(),
                },
            ],
            extra: Vec::new(),
        });
        cat.apply_new_photo(
            &photo,
            crate::SidecarStat {
                size: 1,
                modified: Some(1),
            },
        )
        .unwrap();
        assert!(cat.duplicate_photos().unwrap().is_empty());
    }
}

#[cfg(test)]
mod migration_tests {
    use auroraw_types::WorkspaceId;

    use super::*;

    #[test]
    fn a_version_2_catalogue_gets_the_location_table_when_it_is_opened() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("old.db");
        {
            let cat = Catalogue::create(&path, WorkspaceId::random()).unwrap();
            cat.conn.execute_batch("DROP TABLE location").unwrap();
            cat.conn.pragma_update(None, "user_version", 2).unwrap();
        }
        let mut cat = Catalogue::open(&path).unwrap();
        let version: u32 = cat
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);
        let id = PhotoId::random();
        cat.conn
            .execute(
                "INSERT INTO photo(id, source_id, path, filename, fingerprint, hash, capture_time, sidecar_size)
                 VALUES (?1, ?2, 'a.jpg', 'a.jpg', 'fp', 'h', 0, 1)",
                params![id.to_string(), SourceId::random().to_string()],
            )
            .unwrap();
        cat.insert_location(
            &id,
            &SourceId::random(),
            "b.jpg",
            "a.jpg",
            &Fingerprint::from_bytes([1; 32]),
            &ContentHash::from_bytes([2; 32]),
        )
        .unwrap();
        assert_eq!(cat.duplicate_photos().unwrap().len(), 1);
        // Opened again, nothing more happens.
        drop(cat);
        let cat = Catalogue::open(&path).unwrap();
        assert_eq!(cat.duplicate_photos().unwrap().len(), 1);
    }
}
