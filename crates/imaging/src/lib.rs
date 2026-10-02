// SPDX-License-Identifier: GPL-3.0-or-later
//! Decoders, embedded previews, thumbnails, the previews database and the preview colour
//! (architecture §3.1, §5.7, D-075). Work package WP5.
//!
//! Native code, dispatching by extension itself (`format::is_standard`), and stays that way:
//! WP6 added `plugin_api::Decoder`, but for the sensor's own mosaic (architecture §8.1, "Import:
//! pixels (linear)"), a full RAW decode this crate never needs for a cheap embedded-preview
//! thumbnail. The two are separate on purpose, not a promotion of this crate's own code.
//!
//! - [`read_metadata`] reads what a photo sidecar caches (design note 003 §4.3): camera, lens,
//!   exposure, capture time, GPS, as text in the shapes `auroraw_format::sidecar::Original`
//!   expects (this crate does not depend on `format`; the two are siblings under `engine`).
//! - [`embedded_preview`] decodes the image a thumbnail is made from: the file itself for a
//!   standard format, the largest embedded preview `rawler` can find for a RAW one.
//! - [`make_thumbnail`] resizes and encodes it, applying orientation first.
//! - [`perceptual_hash`] is a similarity fingerprint, and [`thumbnail_hash`] makes it from a stored thumbnail (D-105); WP9 decides
//!   what to do with it.
//! - [`PreviewsDb`] is the cache database thumbnails live in (D-075).
//!
//! Colour: an embedded preview is, in practice, always an sRGB JPEG already (what a camera's own
//! processor writes for its own LCD and for other software to show unmodified) — this crate
//! trusts that rather than building colour management for it. A wide-gamut embedded preview,
//! and a RAW file's own sensor data converted through a working space, both need the image
//! engine (M2); WP5 does not have one to convert through.

mod aids;
mod error;
mod format;
mod metadata;
mod phash;
mod preview;
mod previews;
mod thumbnail;

pub use aids::{
    ANALYSIS_EDGE, Aids, MaskKind, analyse, clipping_mask, encode_mask, mask_png, peaking_mask,
};
pub use error::{ImagingError, Result};
pub use format::{is_photo_file, is_raw};
pub use metadata::{Metadata, read_metadata};
pub use phash::{perceptual_hash, thumbnail_hash};
pub use preview::{
    VIEW_MAX_EDGE, ViewPicture, embedded_preview, measure_file, view_image, view_picture,
};
pub use previews::PreviewsDb;
pub use thumbnail::{Thumbnail, make_thumbnail};

use std::path::Path;

/// Everything a future import (WP7) wants from one file, in one call: metadata, a thumbnail
/// (256 px on the long edge, D-075) and its perceptual hash. The returned [`Metadata`]'s pixel
/// dimensions are the preview's (this crate makes no full RAW decode), filled in here rather
/// than requiring a second read of the file.
pub fn process(path: &Path) -> Result<(Metadata, Thumbnail, u64)> {
    let mut metadata = read_metadata(path)?;
    let preview = embedded_preview(path)?;
    metadata.pixel_width.get_or_insert(preview.width());
    metadata.pixel_height.get_or_insert(preview.height());
    let hash = perceptual_hash(&preview);
    let thumbnail = make_thumbnail(preview, metadata.orientation, 256, path)?;
    Ok((metadata, thumbnail, hash))
}
