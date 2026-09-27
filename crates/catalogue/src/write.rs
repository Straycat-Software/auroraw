// SPDX-License-Identifier: GPL-3.0-or-later
//! Incremental writes: updating one photo's or one keyword's row without a full rebuild
//! (M1 plan WP2's "batch changes in one transaction", completed here as WP3's engine needs it).
//!
//! Each function here touches only the columns its caller's kind of change can affect: a rating
//! or keyword edit never rewrites `capture_time`, `series_id` or `version_count`, for instance.
//! That is what lets these run as a small, fast update instead of the delete-and-reinsert
//! [`crate::rebuild_to_file`] does for a whole catalogue.

use auroraw_format::sidecar::{PhotoSidecar, VersionSidecar};
use auroraw_format::state::{KeywordEntry, SourceEntry};
use auroraw_types::{ContentHash, Fingerprint, KeywordId, PhotoId, SourceId};
use rusqlite::{OptionalExtension, params};

use crate::SidecarStat;
use crate::effective::{effective_flag, effective_rating, flag_code};
use crate::error::{CatalogueError, Result};
use crate::open::Catalogue;

impl Catalogue {
    /// Applies a photo's own metadata (rating, flag, label, title, caption, keywords) to its
    /// existing row. `main_version`, if the photo has one, is used to compute the effective
    /// rating and flag (D-063); everything else about the photo (its files, camera, capture time,
    /// series membership, version count) is left exactly as it was. Fails if the photo is not
    /// already in the catalogue: a fresh photo enters through a rebuild, WP4's import, or (later)
    /// an explicit `add_photo`, which this crate does not need yet.
    pub fn apply_photo_metadata(
        &mut self,
        photo: &PhotoSidecar,
        stat: SidecarStat,
        main_version: Option<&VersionSidecar>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        let id = photo.photo_id.to_string();
        let own_rating = photo.meta.rating.unwrap_or(0) as i64;
        let own_flag = flag_code(photo.meta.flag);
        let (eff_rating, overridden) = effective_rating(photo, main_version);
        let eff_flag = effective_flag(photo, main_version);

        let changed = tx.execute(
            "UPDATE photo SET rating = ?1, flag = ?2, label = ?3, title = ?4, caption = ?5,
                effective_rating = ?6, effective_flag = ?7, rating_overridden = ?8,
                sidecar_size = ?9, sidecar_modified = ?10
             WHERE id = ?11",
            params![
                own_rating,
                own_flag,
                photo.meta.label,
                photo.meta.title,
                photo.meta.caption,
                eff_rating,
                eff_flag,
                overridden as i64,
                stat.size as i64,
                stat.modified,
                id
            ],
        )?;
        if changed == 0 {
            return Err(CatalogueError::NotFound { kind: "photo", id });
        }

        tx.execute("DELETE FROM photo_keyword WHERE photo_id = ?1", [&id])?;
        for keyword_id in &photo.meta.keyword_ids {
            tx.execute(
                "INSERT OR IGNORE INTO photo_keyword(photo_id, keyword_id) VALUES (?1, ?2)",
                params![id, keyword_id.to_string()],
            )?;
        }
        let keyword_names = photo
            .meta
            .keyword_paths
            .iter()
            .map(|p| p.rsplit('|').next().unwrap_or(p))
            .collect::<Vec<_>>()
            .join(" ");
        tx.execute(
            "UPDATE photo_fts SET caption = ?1, title = ?2, keywords = ?3
             WHERE rowid IN (SELECT rowid FROM photo WHERE id = ?4)",
            params![photo.meta.caption, photo.meta.title, keyword_names, id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The row an existing photo has, `None` if it is not in the catalogue: lets a caller decide
    /// whether it needs the main version's sidecar before calling [`Self::apply_photo_metadata`]
    /// (only when the photo actually has one).
    pub fn main_version_of(&self, id: &PhotoId) -> Result<Option<auroraw_types::VersionId>> {
        let text: Option<String> = self
            .conn
            .query_row(
                "SELECT main_version_id FROM photo WHERE id = ?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(text.and_then(|t| t.parse().ok()))
    }

    /// Inserts or updates one keyword's `name` and `path`. The path is the caller's
    /// responsibility (`crate::keyword_paths`): a rename can change more than this one row, if
    /// the keyword has descendants, and the caller updates each affected row the same way.
    pub fn apply_keyword(&mut self, entry: &KeywordEntry, path: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO keyword(id, parent_id, name, path, export) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                parent_id = excluded.parent_id, name = excluded.name, path = excluded.path, export = excluded.export",
            params![entry.id.to_string(), entry.parent.map(|p| p.to_string()), entry.name, path, entry.export as i64],
        )?;
        Ok(())
    }

    /// Removes keywords from the catalogue: the photos and versions that carry them stop carrying
    /// them, then the rows go (parents and children in one transaction, so the order among them does
    /// not matter). The sidecars are the caller's: it has already taken these keywords off the photos
    /// (or, undoing the creation of a keyword, nobody has them).
    pub fn remove_keywords(&mut self, ids: &[KeywordId]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;
        for id in ids {
            let id = id.to_string();
            tx.execute("DELETE FROM photo_keyword WHERE keyword_id = ?1", [&id])?;
            tx.execute("DELETE FROM version_keyword WHERE keyword_id = ?1", [&id])?;
            tx.execute("DELETE FROM keyword WHERE id = ?1", [&id])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Inserts or updates a registered source's `kind` and `name` (WP4). The real path on this
    /// machine is never stored here (design note 002 §6.6): it lives in the workspace's
    /// `SourceEntry.hint`, read fresh by whoever needs to open the source.
    pub fn apply_source(&mut self, entry: &SourceEntry) -> Result<()> {
        self.conn.execute(
            "INSERT INTO source(id, kind, name) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET kind = excluded.kind, name = excluded.name",
            params![entry.id.to_string(), entry.kind, entry.name],
        )?;
        Ok(())
    }

    /// Adds a photo discovered by a source scan (design note 004 §6.3, item 4's "in place"
    /// case: no import, so no whole-file hash yet, only the sampled fingerprint) that a person
    /// confirmed. Its metadata is empty: nothing here has read the file's EXIF yet (WP5's job).
    /// Fails if a photo with this identifier already exists.
    pub fn apply_new_photo(&mut self, photo: &PhotoSidecar, stat: SidecarStat) -> Result<()> {
        let tx = self.conn.transaction()?;
        crate::populate::insert_photo(&tx, photo, stat, &[])?;
        tx.commit()?;
        Ok(())
    }

    /// Takes a photo out of the catalogue (its sidecar is dealt with by the caller, recoverably):
    /// the row, its keywords and versions, its place in collections and its search entry.
    pub fn remove_photo(&mut self, photo_id: &PhotoId) -> Result<()> {
        let id = photo_id.to_string();
        let tx = self.conn.transaction()?;
        let rowid: Option<i64> = tx
            .query_row("SELECT rowid FROM photo WHERE id = ?1", [&id], |r| r.get(0))
            .optional()?;
        let Some(rowid) = rowid else {
            return Ok(());
        };
        tx.execute("DELETE FROM photo_fts WHERE rowid = ?1", [rowid])?;
        tx.execute(
            "DELETE FROM version_keyword WHERE version_id IN
                 (SELECT id FROM version WHERE photo_id = ?1)",
            [&id],
        )?;
        tx.execute("DELETE FROM version WHERE photo_id = ?1", [&id])?;
        tx.execute("DELETE FROM photo_keyword WHERE photo_id = ?1", [&id])?;
        tx.execute("DELETE FROM collection_member WHERE photo_id = ?1", [&id])?;
        tx.execute("DELETE FROM photo WHERE id = ?1", [&id])?;
        tx.commit()?;
        Ok(())
    }

    /// Takes a source out of the catalogue's list. Its photos are the caller's business (remove
    /// them first, or move them, as the sources' owner decides). Also drops any secondary
    /// location this source held (D-108): nothing should outlive the source it names.
    pub fn remove_source(&mut self, source_id: &SourceId) -> Result<()> {
        self.conn.execute(
            "DELETE FROM location WHERE source_id = ?1",
            [source_id.to_string()],
        )?;
        self.conn
            .execute("DELETE FROM source WHERE id = ?1", [source_id.to_string()])?;
        Ok(())
    }

    /// Updates an existing photo's file location after a reconcile relinks it (design note 004
    /// §6.4): the file moved or was renamed within its source, silently, since the match was
    /// unique. Nothing about the photo's metadata changes. Also drops any secondary location
    /// (D-108) that now matches these same values: a location cannot be primary and also listed
    /// as a secondary of the same photo (this covers a source removal promoting the photo's
    /// remaining secondary location to primary, `remove_source`'s own caller in `engine`).
    pub fn apply_relink(
        &mut self,
        photo_id: &PhotoId,
        source_id: &SourceId,
        path: &str,
        filename: &str,
        fingerprint: &Fingerprint,
    ) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE photo SET source_id = ?1, path = ?2, filename = ?3, fingerprint = ?4,
                original_changed = 0, original_missing = 0
             WHERE id = ?5",
            params![
                source_id.to_string(),
                path,
                filename,
                fingerprint.to_string(),
                photo_id.to_string()
            ],
        )?;
        if changed == 0 {
            return Err(CatalogueError::NotFound {
                kind: "photo",
                id: photo_id.to_string(),
            });
        }
        self.conn.execute(
            "DELETE FROM location WHERE photo_id = ?1 AND source_id = ?2 AND path = ?3",
            params![photo_id.to_string(), source_id.to_string(), path],
        )?;
        Ok(())
    }

    /// Marks or clears a photo's "original changed" state (design note 004 §6.5): the file at
    /// its known path no longer matches the recorded fingerprint.
    pub fn mark_original_changed(&mut self, photo_id: &PhotoId, changed: bool) -> Result<()> {
        self.set_reconcile_flag("original_changed", photo_id, changed)
    }

    /// Marks or clears a photo's "missing" state: the last reconcile of its source found no file
    /// matching it at all. Never removes the photo (D-019, D-031).
    pub fn mark_missing(&mut self, photo_id: &PhotoId, missing: bool) -> Result<()> {
        self.set_reconcile_flag("original_missing", photo_id, missing)
    }

    /// Records a photo's whole-file hash, once known (design note 004 §6.1): at import, or when
    /// import's skip decision reads an existing candidate's file to settle a fingerprint match
    /// that had none recorded yet (WP7).
    pub fn apply_hash(&mut self, photo_id: &PhotoId, hash: &ContentHash) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE photo SET hash = ?1 WHERE id = ?2",
            params![hash.to_string(), photo_id.to_string()],
        )?;
        if changed == 0 {
            return Err(CatalogueError::NotFound {
                kind: "photo",
                id: photo_id.to_string(),
            });
        }
        Ok(())
    }

    fn set_reconcile_flag(
        &mut self,
        column: &'static str,
        photo_id: &PhotoId,
        value: bool,
    ) -> Result<()> {
        let sql = format!("UPDATE photo SET {column} = ?1 WHERE id = ?2");
        let changed = self
            .conn
            .execute(&sql, params![value as i64, photo_id.to_string()])?;
        if changed == 0 {
            return Err(CatalogueError::NotFound {
                kind: "photo",
                id: photo_id.to_string(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rebuild::{RebuildInput, build};
    use auroraw_format::sidecar::Flag;
    use auroraw_types::{KeywordId, WorkspaceId};

    fn one_photo() -> (PhotoSidecar, SidecarStat) {
        let mut p = PhotoSidecar::new(PhotoId::random());
        p.meta.rating = Some(2);
        p.meta.keyword_ids = vec![KeywordId::from_bytes([1; 8])];
        p.meta.keyword_paths = vec!["Fauna|Heron".into()];
        (
            p,
            SidecarStat {
                size: 100,
                modified: Some(1),
            },
        )
    }

    /// Both keywords `one_photo` and the tests' edits can reference, so `photo_keyword`'s foreign
    /// key is satisfied.
    fn vocabulary() -> Vec<KeywordEntry> {
        [([1u8; 8], "Heron"), ([2; 8], "Lake")]
            .into_iter()
            .map(|(id, name)| KeywordEntry {
                id: KeywordId::from_bytes(id),
                name: name.into(),
                parent: None,
                synonyms: vec![],
                export: true,
                extra: Default::default(),
            })
            .collect()
    }

    #[test]
    fn apply_photo_metadata_changes_only_what_it_should() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (photo, stat) = one_photo();
        let vocab = vocabulary();
        build(
            &mut cat,
            &RebuildInput {
                photos: &[(photo.clone(), stat)],
                vocabulary: &vocab,
                ..Default::default()
            },
        )
        .unwrap();
        let before = cat.photo(&photo.photo_id).unwrap().unwrap();

        let mut edited = photo.clone();
        edited.meta.rating = Some(5);
        edited.meta.flag = Some(Flag::Picked);
        let new_stat = SidecarStat {
            size: 150,
            modified: Some(2),
        };
        cat.apply_photo_metadata(&edited, new_stat, None).unwrap();

        let after = cat.photo(&photo.photo_id).unwrap().unwrap();
        assert_eq!(after.rating, 5);
        assert_eq!(after.effective_rating, 5);
        assert_eq!(after.flag, 1);
        assert_eq!(
            after.filename, before.filename,
            "untouched by a rating change"
        );
        assert_eq!(after.camera, before.camera, "untouched by a rating change");
    }

    #[test]
    fn remove_keywords_drops_the_rows_and_what_carried_them() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (photo, stat) = one_photo();
        let mut vocab = vocabulary();
        // "Lake" is a child of "Heron": both go in one call, in any order.
        vocab[1].parent = Some(vocab[0].id);
        build(
            &mut cat,
            &RebuildInput {
                photos: &[(photo.clone(), stat)],
                vocabulary: &vocab,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(cat.keywords_with_counts().unwrap().len(), 2);
        let ids: Vec<KeywordId> = vocab.iter().map(|k| k.id).collect();
        cat.remove_keywords(&ids).unwrap();
        assert!(cat.keywords_with_counts().unwrap().is_empty());
        assert_eq!(
            cat.list_by_keyword(&ids[0], false, None, 10).unwrap().len(),
            0
        );
        assert!(
            cat.photo(&photo.photo_id).unwrap().is_some(),
            "the photo stays"
        );
    }

    #[test]
    fn apply_photo_metadata_updates_keywords_and_the_search_index() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (photo, stat) = one_photo();
        let vocab = vocabulary();
        build(
            &mut cat,
            &RebuildInput {
                photos: &[(photo.clone(), stat)],
                vocabulary: &vocab,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            cat.list_by_keyword(&KeywordId::from_bytes([1; 8]), false, None, 10)
                .unwrap()
                .len(),
            1
        );

        let mut edited = photo.clone();
        edited.meta.keyword_ids = vec![KeywordId::from_bytes([2; 8])];
        edited.meta.keyword_paths = vec!["Places|Lake".into()];
        edited.meta.title = Some("A heron at the lake".into());
        cat.apply_photo_metadata(&edited, SidecarStat::default(), None)
            .unwrap();

        assert!(
            cat.list_by_keyword(&KeywordId::from_bytes([1; 8]), false, None, 10)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            cat.list_by_keyword(&KeywordId::from_bytes([2; 8]), false, None, 10)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(cat.search("heron", 10).unwrap().len(), 1);
    }

    #[test]
    fn apply_photo_metadata_on_an_unknown_photo_is_an_error() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (photo, stat) = one_photo();
        assert!(matches!(
            cat.apply_photo_metadata(&photo, stat, None),
            Err(CatalogueError::NotFound { kind: "photo", .. })
        ));
    }

    #[test]
    fn apply_keyword_inserts_then_updates() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let entry = KeywordEntry {
            id: KeywordId::from_bytes([9; 8]),
            name: "Fauna".into(),
            parent: None,
            synonyms: vec![],
            export: true,
            extra: Default::default(),
        };
        cat.apply_keyword(&entry, "Fauna").unwrap();
        cat.apply_keyword(
            &KeywordEntry {
                name: "Wildlife".into(),
                ..entry.clone()
            },
            "Wildlife",
        )
        .unwrap();
        let name: String = cat
            .conn
            .query_row(
                "SELECT name FROM keyword WHERE id = ?1",
                [entry.id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "Wildlife");
    }

    fn photo_with_file(source_id: SourceId, path: &str, fingerprint: Fingerprint) -> PhotoSidecar {
        use auroraw_format::sidecar::{FileEntry, FileRole, Location};
        let mut p = PhotoSidecar::new(PhotoId::random());
        p.files.push(FileEntry {
            role: FileRole::Original,
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            format: None,
            size: 10,
            fingerprint,
            hash: None,
            locations: vec![Location {
                source: source_id,
                path: path.to_string(),
                seen: None,
                extra: vec![],
            }],
            extra: vec![],
        });
        p
    }

    #[test]
    fn apply_source_inserts_then_updates() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let id = SourceId::random();
        cat.apply_source(&SourceEntry {
            id,
            kind: "local-folder".into(),
            name: "Working disk".into(),
            hint: Default::default(),
            ignore: vec![],
            config: Default::default(),
            extra: Default::default(),
        })
        .unwrap();
        cat.apply_source(&SourceEntry {
            id,
            kind: "local-folder".into(),
            name: "Renamed disk".into(),
            hint: Default::default(),
            ignore: vec![],
            config: Default::default(),
            extra: Default::default(),
        })
        .unwrap();
        let row = cat.source(&id).unwrap().unwrap();
        assert_eq!(row.name, "Renamed disk");
        assert_eq!(cat.list_sources().unwrap().len(), 1);
    }

    #[test]
    fn apply_new_photo_and_known_files_in_source_agree() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let source_id = SourceId::random();
        let fp = Fingerprint::from_bytes([7; 32]);
        let photo = photo_with_file(source_id, "a.jpg", fp);
        cat.apply_new_photo(
            &photo,
            SidecarStat {
                size: 1,
                modified: Some(1),
            },
        )
        .unwrap();

        let row = cat.photo(&photo.photo_id).unwrap().unwrap();
        assert_eq!(row.source_id, Some(source_id));
        assert_eq!(row.path.as_deref(), Some("a.jpg"));
        assert!(!row.original_changed);
        assert!(!row.original_missing);

        let known = cat.known_files_in_source(&source_id).unwrap();
        assert_eq!(known, vec![(photo.photo_id, "a.jpg".to_string(), fp, true)]);
    }

    #[test]
    fn apply_relink_moves_the_location_and_clears_reconcile_flags() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let source_id = SourceId::random();
        let fp = Fingerprint::from_bytes([1; 32]);
        let photo = photo_with_file(source_id, "old.jpg", fp);
        cat.apply_new_photo(
            &photo,
            SidecarStat {
                size: 1,
                modified: Some(1),
            },
        )
        .unwrap();
        cat.mark_missing(&photo.photo_id, true).unwrap();

        cat.apply_relink(&photo.photo_id, &source_id, "new.jpg", "new.jpg", &fp)
            .unwrap();

        let row = cat.photo(&photo.photo_id).unwrap().unwrap();
        assert_eq!(row.path.as_deref(), Some("new.jpg"));
        assert!(
            !row.original_missing,
            "relinking clears a stale missing mark"
        );
    }

    #[test]
    fn mark_original_changed_and_mark_missing_round_trip() {
        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let (photo, stat) = one_photo();
        let vocab = vocabulary();
        build(
            &mut cat,
            &RebuildInput {
                photos: &[(photo.clone(), stat)],
                vocabulary: &vocab,
                ..Default::default()
            },
        )
        .unwrap();

        cat.mark_original_changed(&photo.photo_id, true).unwrap();
        assert!(
            cat.photo(&photo.photo_id)
                .unwrap()
                .unwrap()
                .original_changed
        );
        cat.mark_original_changed(&photo.photo_id, false).unwrap();
        assert!(
            !cat.photo(&photo.photo_id)
                .unwrap()
                .unwrap()
                .original_changed
        );

        assert!(matches!(
            cat.mark_missing(&PhotoId::random(), true),
            Err(CatalogueError::NotFound { kind: "photo", .. })
        ));
    }

    #[test]
    fn apply_hash_is_found_by_a_later_fingerprint_lookup() {
        use auroraw_types::ContentHash;

        let mut cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let source_id = SourceId::random();
        let fp = Fingerprint::from_bytes([3; 32]);
        let photo = photo_with_file(source_id, "a.raw", fp);
        cat.apply_new_photo(
            &photo,
            SidecarStat {
                size: 1,
                modified: Some(1),
            },
        )
        .unwrap();

        let candidates = cat.find_by_fingerprint(&fp).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].photo_id, photo.photo_id);
        assert_eq!(
            candidates[0].hash, None,
            "not imported yet: no hash recorded"
        );
        assert_eq!(candidates[0].source_id, Some(source_id));
        assert_eq!(candidates[0].path.as_deref(), Some("a.raw"));

        let hash = ContentHash::from_bytes([9; 32]);
        cat.apply_hash(&photo.photo_id, &hash).unwrap();
        let candidates = cat.find_by_fingerprint(&fp).unwrap();
        assert_eq!(candidates[0].hash, Some(hash));

        assert!(matches!(
            cat.apply_hash(&PhotoId::random(), &hash),
            Err(CatalogueError::NotFound { kind: "photo", .. })
        ));
    }

    #[test]
    fn find_by_fingerprint_is_empty_for_an_unknown_fingerprint() {
        let cat = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        let fp = Fingerprint::from_bytes([1; 32]);
        assert!(cat.find_by_fingerprint(&fp).unwrap().is_empty());
    }
}
