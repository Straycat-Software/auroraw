// SPDX-License-Identifier: GPL-3.0-or-later
//! The workspace folder on disk (design notes 001 and 002): where the sidecars and state files
//! live, how they are named, and how they are written safely.
//!
//! - `photos/<xx>/<photo id>.xmp` and `versions/<xx>/<photo id>.<version id>.xmp`, the shard `<xx>`
//!   being the first two characters of the photo identifier, created when first needed;
//! - `state/` for the vocabulary, the sources, the collections and the series;
//! - `workspace.json`, the marker; `.auroraw/` for temporary files and the writer's lock;
//! - `exports/` and `removed/` (recoverable files, never deleted).
//!
//! Every write goes through a temporary file and a rename. One writer at a time holds an advisory
//! lock; a second opener gets a read-only workspace. Files this version does not know are left
//! alone and reported.

mod error;
mod layout;
mod scan;
mod workspace;
mod write;

pub use error::WorkspaceError;
pub use layout::LAYOUT_VERSION;
pub use scan::{Entry, FileStat, Foreign, ForeignReason, Scan};
pub use workspace::{Access, RemovedPhoto, Workspace, WriteOutcome};
pub use write::{write_beside, write_new_beside};
