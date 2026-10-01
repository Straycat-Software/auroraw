// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::{Path, PathBuf};

use auroraw_types::WorkspaceId;
use rusqlite::{Connection, OptionalExtension};

use crate::error::{CatalogueError, Result};
use crate::place::PLACE_KEYS_VERSION;

const SCHEMA: &str = include_str!("schema.sql");
const INDEXES: &str = include_str!("indexes.sql");

/// The schema version this crate reads and writes, written to `PRAGMA user_version`.
pub const CURRENT_SCHEMA: u32 = 6;

/// An open catalogue.
pub struct Catalogue {
    pub(crate) conn: Connection,
    path: Option<PathBuf>,
}

fn set_pragmas(conn: &Connection) -> rusqlite::Result<()> {
    // WAL and a busy timeout (D-073): many readers, one writer (the engine, architecture §4).
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

impl Catalogue {
    /// Opens an in-memory catalogue (schema applied fresh): for tests and for building a
    /// catalogue that will be dumped to a file (see `rebuild`).
    pub fn open_in_memory(workspace_id: WorkspaceId) -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let mut cat = Self { conn, path: None };
        cat.create_schema(workspace_id)?;
        Ok(cat)
    }

    /// Creates a new catalogue file. Fails if one already exists at `path`.
    pub fn create(path: &Path, workspace_id: WorkspaceId) -> Result<Self> {
        if path.exists() {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT),
                Some(format!("{} already exists", path.display())),
            )
            .into());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CatalogueError::io(parent, e))?;
        }
        let conn = Connection::open(path)?;
        set_pragmas(&conn)?;
        let mut cat = Self {
            conn,
            path: Some(path.to_path_buf()),
        };
        cat.create_schema(workspace_id)?;
        Ok(cat)
    }

    /// Opens an existing catalogue file.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        set_pragmas(&conn)?;
        let cat = Self {
            conn,
            path: Some(path.to_path_buf()),
        };
        let found: u32 = cat
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if found > CURRENT_SCHEMA {
            return Err(CatalogueError::NewerSchema {
                found,
                supported: CURRENT_SCHEMA,
            });
        }
        // Each later version adds an `if found < N { migrate... }` step here, in its own transaction,
        // raising `user_version` by one.
        if found < 2 {
            cat.migrate_to_2()?;
        }
        if found < 3 {
            cat.migrate_to_3()?;
        }
        if found < 4 {
            cat.migrate_to_4()?;
        }
        if found < 5 {
            cat.migrate_to_5()?;
        }
        if found < 6 {
            cat.migrate_to_6()?;
        }
        cat.check_place_keys()?;
        Ok(cat)
    }

    /// Asks for the place columns to be filled again when the keys in them were made by another version of the keys
    /// (design note 008 §5.1, [`PLACE_KEYS_VERSION`]): a file made before the version was recorded has none, and
    /// is taken to have another.
    fn check_place_keys(&self) -> Result<()> {
        if self.meta("place_keys")?.as_deref() != Some(PLACE_KEYS_VERSION) {
            self.conn.execute(
                "INSERT OR REPLACE INTO meta(key, value) VALUES ('place_columns', 'stale')",
                [],
            )?;
            self.conn.execute(
                "INSERT OR REPLACE INTO meta(key, value) VALUES ('place_keys', ?1)",
                [PLACE_KEYS_VERSION],
            )?;
        }
        Ok(())
    }

    fn create_schema(&mut self, workspace_id: WorkspaceId) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute_batch(SCHEMA)?;
        tx.execute_batch(INDEXES)?;
        tx.execute(
            "INSERT INTO meta(key, value) VALUES ('workspace_id', ?1)",
            [workspace_id.to_string()],
        )?;
        tx.execute(
            "INSERT INTO meta(key, value) VALUES ('place_keys', ?1)",
            [PLACE_KEYS_VERSION],
        )?;
        tx.pragma_update(None, "user_version", CURRENT_SCHEMA)?;
        tx.commit()?;
        Ok(())
    }

    /// Schema 2 (D-105): the photo's perceptual hash. Made under an immediate transaction that looks again at the version,
    /// so that two connections opening a version 1 file at once do not both add the column.
    fn migrate_to_2(&self) -> Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            let found: u32 = self
                .conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if found < 2 {
                self.conn
                    .execute_batch("ALTER TABLE photo ADD COLUMN phash INTEGER")?;
                self.conn.pragma_update(None, "user_version", 2)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => Ok(self.conn.execute_batch("COMMIT")?),
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Schema 3 (D-108): a photo's secondary locations (D-036, exact duplicates), one row a location. Made under an
    /// immediate transaction that looks again at the version, so that two connections opening an older file at once
    /// do not both create the table.
    fn migrate_to_3(&self) -> Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            let found: u32 = self
                .conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if found < 3 {
                self.conn.execute_batch(
                    "CREATE TABLE location(
                       photo_id TEXT NOT NULL REFERENCES photo(id),
                       source_id TEXT NOT NULL,
                       path TEXT NOT NULL,
                       filename TEXT NOT NULL,
                       fingerprint TEXT NOT NULL,
                       hash TEXT NOT NULL,
                       seen INTEGER,
                       PRIMARY KEY(photo_id, source_id, path)
                     ) WITHOUT ROWID;
                     CREATE INDEX location_source ON location(source_id);",
                )?;
                self.conn.pragma_update(None, "user_version", 3)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => Ok(self.conn.execute_batch("COMMIT")?),
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Schema 4 (D-045, WP10 slice 2): a keyword's synonyms, `|`-joined. Made under an immediate
    /// transaction that looks again at the version, so that two connections opening an older file
    /// at once do not both add the column.
    fn migrate_to_4(&self) -> Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            let found: u32 = self
                .conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if found < 4 {
                self.conn.execute_batch(
                    "ALTER TABLE keyword ADD COLUMN synonyms TEXT NOT NULL DEFAULT ''",
                )?;
                self.conn.pragma_update(None, "user_version", 4)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => Ok(self.conn.execute_batch("COMMIT")?),
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Schema 5 (D-047, WP10): the base of an external XMP file, one row a photo. Made under an immediate
    /// transaction that looks again at the version, so that two connections opening an older file at once do not
    /// both create the table (`IF NOT EXISTS` also keeps a file that was made older by hand, in a test, from
    /// colliding with a table it already has).
    fn migrate_to_5(&self) -> Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            let found: u32 = self
                .conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if found < 5 {
                self.conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS external_xmp(
                       photo_id TEXT PRIMARY KEY REFERENCES photo(id),
                       path TEXT NOT NULL,
                       size INTEGER NOT NULL,
                       modified_ns INTEGER,
                       base TEXT NOT NULL,
                       pending TEXT,
                       pending_size INTEGER,
                       pending_modified_ns INTEGER
                     );
                     CREATE INDEX IF NOT EXISTS external_pending
                       ON external_xmp(photo_id) WHERE pending IS NOT NULL;",
                )?;
                self.conn.pragma_update(None, "user_version", 5)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => Ok(self.conn.execute_batch("COMMIT")?),
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Schema 6 (D-147, WP10, design note 008 §5): the place of a photo, as the four fields and the three keys the place
    /// filter uses, with an index. The columns of the photos already there are empty: a rebuild fills them, and so does
    /// the engine's one pass over the sidecars, which `meta.place_columns = 'stale'` asks for until it has run. Made under
    /// an immediate transaction that looks again at the version, so that two connections opening an older file at once
    /// do not both add the columns.
    fn migrate_to_6(&self) -> Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            let found: u32 = self
                .conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if found < 6 {
                // (A column is added only if the file lacks it: a file made older by hand, in a test, already has
                // them, and `ALTER TABLE` has no `IF NOT EXISTS`.)
                for column in [
                    "country",
                    "region",
                    "city",
                    "country_code",
                    "place_country",
                    "place_region",
                    "place_city",
                ] {
                    let present: i64 = self.conn.query_row(
                        "SELECT COUNT(*) FROM pragma_table_info('photo') WHERE name = ?1",
                        [column],
                        |r| r.get(0),
                    )?;
                    if present == 0 {
                        self.conn.execute_batch(&format!(
                            "ALTER TABLE photo ADD COLUMN {column} TEXT"
                        ))?;
                    }
                }
                self.conn.execute_batch(
                    "CREATE INDEX IF NOT EXISTS photo_place
                       ON photo(place_country, place_region, place_city, country, region, city, country_code,
                                effective_flag, effective_rating)
                       WHERE place_country IS NOT NULL;
                     CREATE INDEX IF NOT EXISTS photo_place_nocountry
                       ON photo(place_region, place_city, region, city, effective_flag, effective_rating)
                       WHERE place_country IS NULL AND (place_region IS NOT NULL OR place_city IS NOT NULL);
                     INSERT OR REPLACE INTO meta(key, value) VALUES ('place_columns', 'stale');",
                )?;
                self.conn.pragma_update(None, "user_version", 6)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => Ok(self.conn.execute_batch("COMMIT")?),
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// The path of the catalogue file, or `None` for an in-memory catalogue.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The workspace this catalogue indexes.
    pub fn workspace_id(&self) -> Result<WorkspaceId> {
        let text: String = self.conn.query_row(
            "SELECT value FROM meta WHERE key = 'workspace_id'",
            [],
            |r| r.get(0),
        )?;
        text.parse().map_err(|_| {
            rusqlite::Error::InvalidColumnType(
                0,
                "workspace_id".into(),
                rusqlite::types::Type::Text,
            )
            .into()
        })
    }

    /// A value from the `meta` table, such as `rebuilt_at`.
    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_catalogue_has_the_current_schema_and_its_workspace_id() {
        let id = WorkspaceId::random();
        let cat = Catalogue::open_in_memory(id).unwrap();
        assert_eq!(cat.workspace_id().unwrap(), id);
        let version: u32 = cat
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA);
    }

    #[test]
    fn a_newer_schema_is_refused() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("c.db");
        {
            let cat = Catalogue::create(&path, WorkspaceId::random()).unwrap();
            cat.conn
                .pragma_update(None, "user_version", CURRENT_SCHEMA + 1)
                .unwrap();
        }
        match Catalogue::open(&path) {
            Err(CatalogueError::NewerSchema { found, supported }) => {
                assert_eq!(found, CURRENT_SCHEMA + 1);
                assert_eq!(supported, CURRENT_SCHEMA);
            }
            other => panic!("{other:?}", other = other.map(|_| ())),
        }
    }

    #[test]
    fn creating_over_an_existing_file_is_refused() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("c.db");
        Catalogue::create(&path, WorkspaceId::random()).unwrap();
        assert!(Catalogue::create(&path, WorkspaceId::random()).is_err());
    }

    #[test]
    fn a_schema_3_file_gains_the_synonyms_column_on_open() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("c.db");
        {
            let cat = Catalogue::create(&path, WorkspaceId::random()).unwrap();
            // Made as schema 3 would have it: no synonyms column, an older version stamped.
            cat.conn
                .execute_batch("ALTER TABLE keyword DROP COLUMN synonyms")
                .unwrap();
            cat.conn.pragma_update(None, "user_version", 3).unwrap();
        }
        let cat = Catalogue::open(&path).unwrap();
        let version: u32 = cat
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA);
        // The column exists and behaves: usable in a statement, defaults to empty.
        cat.conn
            .execute(
                "INSERT INTO keyword(id, name, path) VALUES ('a', 'A', 'A')",
                [],
            )
            .unwrap();
        let synonyms: String = cat
            .conn
            .query_row("SELECT synonyms FROM keyword WHERE id = 'a'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(synonyms, "");
    }

    #[test]
    fn keys_made_by_another_version_of_the_keys_are_made_again() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("c.db");
        {
            let cat = Catalogue::create(&path, WorkspaceId::random()).unwrap();
            assert_eq!(
                cat.meta("place_keys").unwrap().as_deref(),
                Some(PLACE_KEYS_VERSION),
                "a new catalogue records the version its keys are made with"
            );
            assert!(!cat.place_columns_stale().unwrap());
            // Made by a program with other keys (or before the version was recorded: no value at all).
            cat.conn
                .execute(
                    "UPDATE meta SET value = 'older' WHERE key = 'place_keys'",
                    [],
                )
                .unwrap();
        }
        let cat = Catalogue::open(&path).unwrap();
        assert!(
            cat.place_columns_stale().unwrap(),
            "the columns are to be filled again"
        );
        assert_eq!(
            cat.meta("place_keys").unwrap().as_deref(),
            Some(PLACE_KEYS_VERSION)
        );
        cat.mark_place_columns_fresh().unwrap();
        drop(cat);
        // Opened again by the same version: nothing to ask.
        let cat = Catalogue::open(&path).unwrap();
        assert!(!cat.place_columns_stale().unwrap());
        // No version recorded at all is another version.
        cat.conn
            .execute("DELETE FROM meta WHERE key = 'place_keys'", [])
            .unwrap();
        drop(cat);
        let cat = Catalogue::open(&path).unwrap();
        assert!(cat.place_columns_stale().unwrap());
    }

    #[test]
    fn a_schema_5_file_gains_the_place_columns_on_open_and_asks_for_them_to_be_filled() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("c.db");
        {
            let cat = Catalogue::create(&path, WorkspaceId::random()).unwrap();
            // Made as schema 5 would have it: no place columns, no index, an older version stamped.
            cat.conn
                .execute_batch("DROP INDEX photo_place; DROP INDEX photo_place_nocountry")
                .unwrap();
            for column in [
                "country",
                "region",
                "city",
                "country_code",
                "place_country",
                "place_region",
                "place_city",
            ] {
                cat.conn
                    .execute_batch(&format!("ALTER TABLE photo DROP COLUMN {column}"))
                    .unwrap();
            }
            cat.conn.pragma_update(None, "user_version", 5).unwrap();
        }
        let cat = Catalogue::open(&path).unwrap();
        let version: u32 = cat
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA);
        assert!(
            cat.place_columns_stale().unwrap(),
            "the columns exist and are empty: they are to be filled"
        );
        for index in ["photo_place", "photo_place_nocountry"] {
            let present: i64 = cat
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                    [index],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(present, 1, "the migration makes the index {index}");
        }
        // The columns are usable, and the tree is empty until they are filled.
        assert_eq!(cat.place_facets(&Default::default()).unwrap().placed, 0);
        cat.mark_place_columns_fresh().unwrap();
        assert!(!cat.place_columns_stale().unwrap());
        // Opening it again does not migrate twice, nor ask again.
        drop(cat);
        let cat = Catalogue::open(&path).unwrap();
        assert!(!cat.place_columns_stale().unwrap());
        // A catalogue made at schema 6 never asks.
        let fresh = Catalogue::open_in_memory(WorkspaceId::random()).unwrap();
        assert!(!fresh.place_columns_stale().unwrap());
    }

    #[test]
    fn a_schema_4_file_gains_the_external_xmp_table_on_open() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("c.db");
        {
            let cat = Catalogue::create(&path, WorkspaceId::random()).unwrap();
            // Made as schema 4 would have it: no such table, an older version stamped.
            cat.conn.execute_batch("DROP TABLE external_xmp").unwrap();
            cat.conn.pragma_update(None, "user_version", 4).unwrap();
        }
        let cat = Catalogue::open(&path).unwrap();
        let version: u32 = cat
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA);
        let rows: i64 = cat
            .conn
            .query_row("SELECT COUNT(*) FROM external_xmp", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0, "the table exists, empty");
        // Opening it again does not try to make it twice.
        drop(cat);
        Catalogue::open(&path).unwrap();
    }
}
