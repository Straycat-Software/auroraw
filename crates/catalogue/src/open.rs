// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::{Path, PathBuf};

use auroraw_types::WorkspaceId;
use rusqlite::{Connection, OptionalExtension};

use crate::error::{CatalogueError, Result};

const SCHEMA: &str = include_str!("schema.sql");
const INDEXES: &str = include_str!("indexes.sql");

/// The schema version this crate reads and writes, written to `PRAGMA user_version`.
pub const CURRENT_SCHEMA: u32 = 5;

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
        Ok(cat)
    }

    fn create_schema(&mut self, workspace_id: WorkspaceId) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute_batch(SCHEMA)?;
        tx.execute_batch(INDEXES)?;
        tx.execute(
            "INSERT INTO meta(key, value) VALUES ('workspace_id', ?1)",
            [workspace_id.to_string()],
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
