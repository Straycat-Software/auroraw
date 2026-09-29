// SPDX-License-Identifier: GPL-3.0-or-later
//! Foreign XMP files next to originals (spec §5.7, D-047; design note 003 §8): what another
//! application (Lightroom, darktable, digiKam, ExifTool) wrote beside a photo, read into plain
//! comparable values. **Nothing here writes a foreign file** (D-018): Auroraw reads it, remembers
//! what it read (the *base*, kept by the catalogue) and, later, compares.
//!
//! [`Fields`] is the shared shape of both sides of a comparison: the same normalisation is applied to
//! what a file holds and to what a photo sidecar holds, so an equality between them means what it says.

mod fields;
mod reader;

pub use fields::{Fields, keyword_key, normalise_path};
pub use reader::{ExternalError, MAX_BYTES, read};
