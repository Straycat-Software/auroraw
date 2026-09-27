// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::{Path, PathBuf};

use auroraw_types::WorkspaceId;
use rusqlite::{Connection, OptionalExtension};

use crate::error::{CatalogueError, Result};

const SCHEMA: &str = include_str!("schema.sql");
const INDEXES: &str = include_str!("indexes.sql");

/// The schema version this crate reads and writes, written to `PRAGMA user_version`.
pub const CURRENT_SCHEMA: u32 = 2;

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
}
