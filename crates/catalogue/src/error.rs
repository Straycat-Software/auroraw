// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;

/// What can go wrong with a catalogue.
#[derive(Debug, thiserror::Error)]
pub enum CatalogueError {
    /// A SQLite operation failed.
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    /// A file system operation on the database file failed.
    #[error("{path}: {source}")]
    Io {
        /// The file concerned.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
    /// The catalogue file was written by a newer version of Auroraw.
    #[error(
        "this catalogue is schema {found}, newer than the schema {supported} this version reads"
    )]
    NewerSchema {
        /// The schema found.
        found: u32,
        /// The schema this version supports.
        supported: u32,
    },
    /// The registry file could not be read as JSON.
    #[error("{path}: {source}")]
    Registry {
        /// The file concerned.
        path: PathBuf,
        /// The error.
        source: serde_json::Error,
    },
    /// A value stored as JSON (an external file's base, D-047) could not be written.
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// An incremental update named an entity that is not in the catalogue (a rebuild is needed
    /// first, or the caller has the wrong identifier).
    #[error("{kind} {id} is not in the catalogue")]
    NotFound {
        /// What kind of entity (`"photo"`, `"keyword"`).
        kind: &'static str,
        /// Its identifier, as text.
        id: String,
    },
}

impl CatalogueError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

/// The result type this crate uses throughout.
pub type Result<T> = std::result::Result<T, CatalogueError>;
