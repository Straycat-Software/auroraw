// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `Decoder` family's interface (architecture §8.1, row "Import": "pixels (linear), metadata,
//! embedded preview; RAW decoders"). WP6 gives this the same treatment WP4 gave [`crate::Source`]:
//! a plain trait a native implementation and a sandboxed WebAssembly one both satisfy, so the
//! host and its callers do not know which they are holding.
//!
//! This is narrower than `imaging`'s own decoding (WP5): `imaging::embedded_preview` reads the
//! small JPEG preview a camera already wrote, cheaply, for thumbnails. [`Decoder::decode`] is the
//! full decode to the sensor's own mosaic that spike 4 measured (`rawler`, native and as a
//! WebAssembly plugin, bit-identical): the source data a future develop pipeline demosaics and
//! renders (M2); what it returns is [`RawImage`], and how its metadata crosses the sandbox is the block of [`crate::block`].
//! The two live in different crates on purpose: `imaging` never needs a full decode
//! for a thumbnail, and this trait never needs `imaging`'s preview or colour handling.

use crate::raw::RawImage;
use thiserror::Error;

/// What went wrong decoding a file.
#[derive(Debug, Error)]
pub enum DecoderError {
    /// The bytes are not a file this decoder understands, or are damaged past recovery.
    #[error("cannot decode this file: {0}")]
    Invalid(String),
    /// The decoder (a plugin call, most often) failed for a reason that is not about the file
    /// itself: a trap, a denied grant, a limit reached.
    #[error("the decoder failed: {0}")]
    Failed(String),
}

/// Decodes a RAW file's own bytes to its sensor mosaic. Implemented natively for comparison and
/// as the sandboxed WebAssembly plugin `plugin-host` loads (WP6); both take the same bytes and
/// must return the same [`RawImage`] (spike 4's identical-pixels check).
///
/// A decoder never touches the file system itself: the caller reads the file and hands over its
/// bytes, matching spike 4's `rawimport` plugin ("the plugin never touches the file system") and
/// needing none of [`crate::Permissions`]'s folder grants.
pub trait Decoder: Send + Sync {
    /// Decodes `bytes` (a whole RAW file, read by the caller) to its sensor mosaic.
    fn decode(&self, bytes: &[u8]) -> Result<RawImage, DecoderError>;
}
