// SPDX-License-Identifier: GPL-3.0-or-later
//! The photo sidecar and the version sidecar (design note 003), on top of the XMP model.
//!
//! Every property this version understands becomes a typed field; every other property is kept in
//! `extra` and written back after the known ones, so nothing is lost. A sidecar whose Auroraw
//! schema is newer than this version knows is reported as such and never interpreted.

pub mod external;
mod extract;
mod metadata;
mod photo;
mod version;

pub use metadata::{
    ColourLabel, CustomField, Flag, Keyword, Metadata, Original, Overlay, OverlayGps,
};
pub use photo::{FileEntry, FileRole, Location, PHOTO_SCHEMA, PhotoSidecar};
pub use version::{OverrideField, VERSION_SCHEMA, VersionSidecar};

use crate::xmp::XmpError;

/// Why a sidecar could not be read.
#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    /// The XMP itself is invalid.
    #[error(transparent)]
    Xmp(#[from] XmpError),
    /// The document is XMP but not one of Auroraw's sidecars.
    #[error("not an Auroraw sidecar: missing {0}")]
    Missing(&'static str),
    /// A required property has a value that cannot be understood.
    #[error("invalid {field}: {value:?}")]
    Invalid {
        /// The property.
        field: &'static str,
        /// What was found.
        value: String,
    },
}
