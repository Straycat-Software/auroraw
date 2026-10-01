// SPDX-License-Identifier: GPL-3.0-or-later
//! The SQLite catalogue: an index over a workspace, entirely rebuildable from it (architecture
//! §5, D-026, D-073). This crate depends only on `format` and `types` (architecture §3.2): it
//! knows nothing about the workspace on disk. Its caller (the `workspace` crate's scan, later
//! the `engine`) reads sidecars and state files and hands them to [`rebuild_to_file`], or applies
//! incremental changes through [`Catalogue::apply_photo_metadata`] and
//! [`Catalogue::apply_keyword`].
//!
//! - [`open`] creates and opens a catalogue, with its migrations (`PRAGMA user_version`).
//! - [`rebuild`] builds a fresh catalogue file from a workspace's content, atomically.
//! - [`write`] applies a change to one photo's or one keyword's existing row (WP3's engine).
//! - [`query`] answers the questions the grid and the search panel ask, with keyset paging.
//! - [`place`] keeps the place of a photo as columns and keys, and answers the place filter and the tree of places
//!   the library's menu shows.
//! - [`reconcile`] says which sidecars the catalogue's copy no longer matches.
//! - [`effective`] computes the effective rating and flag a version's overrides give a photo
//!   (D-063).
//! - [`registry`] is the small, local, per-machine list of a person's catalogues (design note
//!   001 §5.7).
//! - [`dataset`] generates a synthetic but realistic catalogue for tests and benchmarks (moved
//!   here from spike 3, M1 plan WP2).

mod collections;
mod effective;
mod error;
mod external;
mod location;
mod open;
mod place;
mod populate;
mod query;
mod rebuild;
mod reconcile;
mod registry;
mod series;
mod similar;
mod write;

pub mod dataset;

pub use collections::CollectionRow;
pub use effective::{effective_flag, effective_rating};
pub use error::CatalogueError;
pub use external::{ExternalKnown, ExternalPending, ExternalRow, ExternalStat};
pub use location::{DuplicatePhoto, LocationRef};
pub use open::{CURRENT_SCHEMA, Catalogue};
pub use place::{PlaceColumns, PlaceFacets, PlaceFilter, PlaceNode, fold_place};
pub use query::{
    Cursor, Filter, FingerprintCandidate, FlagFilter, KeywordRow, PhotoRow, SeriesFilter,
    SourceCounts, SourceRow,
};
pub use rebuild::{RebuildInput, keyword_paths, rebuild_to_file};
pub use reconcile::ReconcileReport;
pub use registry::{Registry, RegistryEntry};
pub use series::{SeriesCandidate, SeriesInfo};
pub use similar::{SimilarCandidate, SimilarSet};

/// A file's size and modification time, as the catalogue last saw them (design note 004 §6.2).
/// Local to this machine: never written into the workspace. The caller (the workspace scan)
/// supplies these; this crate never touches the file system beyond the catalogue database file
/// itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SidecarStat {
    /// The file's size in bytes.
    pub size: u64,
    /// Seconds since the Unix epoch, when the file system reports one.
    pub modified: Option<i64>,
}

impl SidecarStat {
    /// A stat for bytes not yet written anywhere (their would-be size, no modification time):
    /// useful for tests that build sidecars in memory.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self {
            size: bytes.len() as u64,
            modified: None,
        }
    }
}
