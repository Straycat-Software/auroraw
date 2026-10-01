// SPDX-License-Identifier: GPL-3.0-or-later
//! What can go wrong.

use std::path::PathBuf;

/// What can go wrong reading or writing a file of places.
#[derive(Debug, thiserror::Error)]
pub enum PlacesError {
    /// The file could not be opened or read as a database.
    #[error("the file of places could not be read: {0}")]
    Database(#[from] rusqlite::Error),
    /// The file is not a file of places, or is one of a newer layout than this version reads.
    #[error(
        "not a file of places this version reads (layout {found}, this version reads {supported})"
    )]
    Format {
        /// The layout the file says it has (0 when it says none).
        found: u32,
        /// The layout this version reads.
        supported: u32,
    },
    /// The position is not a latitude between -90 and 90 and a longitude.
    #[error("not a position: latitude {lat}, longitude {lon}")]
    Position {
        /// The latitude given.
        lat: f64,
        /// The longitude given.
        lon: f64,
    },
    /// A polygon stored in the file does not decode: the file is damaged.
    #[error("the polygon {0} in the file of places is damaged")]
    Damaged(i64),
    /// The data to build a file from is not what was expected.
    #[error("the source data is not what was expected: {0}")]
    Source(String),
    /// The file to write exists already.
    #[error("{0} exists already")]
    Exists(PathBuf),
}

/// A result with [`PlacesError`].
pub type Result<T> = std::result::Result<T, PlacesError>;
