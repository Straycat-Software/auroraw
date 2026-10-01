// SPDX-License-Identifier: MIT OR Apache-2.0
//! The block: how a decoder's metadata crosses the sandbox (decision D-141, design note 005 §3.2 item 5).
//!
//! Plugins are WebAssembly in any language (D-076): a LibRaw plugin is C++ and cannot emit a Rust `serde` format, and a
//! NaN, which JSON cannot carry, belongs in no language-neutral block. Everything about a decoded image but its samples
//! therefore travels as a **versioned, little-endian block of tagged sections**; the samples keep their own path
//! (written into the plugin's memory by the plugin, read back by the host). This module is the specification of the
//! block and the one implementation of it in Rust ([`RawImage::to_block`], [`RawImage::from_block`]); a plugin written in
//! another language implements it by hand from the table below, and the fuzz target `raw_block` and the fixtures of
//! `plugin-host` are its conformance tests.
//!
//! ## Layout
//!
//! ```text
//! block   = "ARIB" version:u16 flags:u16 section*
//! section = tag:u16 length:u32 payload[length]
//! ```
//!
//! All integers are little-endian, all floats IEEE 754 (`f32` unless said), and **no float may be NaN or infinite**.
//! `version` is [`VERSION`]; a reader refuses a higher one. `flags` is `0` and ignored. A section runs to the end of its
//! `length` and a known section must use exactly its payload. **A reader skips a tag it does not know**, so that DNG
//! forward matrices, baseline exposure, linearisation tables or opcode lists can be added as tags without breaking
//! the ABI; tags from [`tag::PRIVATE`] up are a decoder's own. Sections come in any order; the ones marked repeatable
//! may occur several times, the others once.
//!
//! | Tag | Section | Payload |
//! | --- | --- | --- |
//! | 1 | geometry (required) | `width:u32 height:u32`, the full readout |
//! | 2 | layout (required) | `kind:u8` (0 Cfa, 1 LinearRgb, 2 Mono); Cfa: `width:u8 height:u8 colours:u8[width*height]`; LinearRgb: `components:u8` |
//! | 3 | samples (required) | `kind:u8` (0 u16, 1 f32) `count:u64`: what the sample buffer holds |
//! | 4 | levels (required) | black: `rows:u16 columns:u16 components:u8 values:f32[rows*columns*components]`; white: `count:u8 values:f32[count]` |
//! | 5 | white balance | `f32 x 3`: R G B |
//! | 6 | colour matrix (repeatable) | `illuminant:u8 rows:u8 (3 or 4) values:f32[rows*3]` |
//! | 7 | crop | `x:u32 y:u32 width:u32 height:u32` |
//! | 8 | active area | the same |
//! | 9 | orientation | `u8`, the EXIF value (0 unknown) |
//! | 10 | camera | `make` then `model`, each `length:u16` and UTF-8 |
//! | 11 | input profile | `kind:u8` (0 sRGB, 1 Adobe RGB, 2 Rec. 2020, 3 ProPhoto, 4 ICC); ICC: the profile's bytes to the end |
//! | 12 | ISO | `u32` |
//! | 13 | noise profile | `count:u8 (scale:f64 offset:f64)[count]` |
//!
//! ## The samples
//!
//! The samples are `width * height * components` values of the type section 3 says, little-endian, in the plugin's
//! memory. In this repository's plugin the host hands `import` a 16-byte buffer and the plugin fills four
//! little-endian `u32` words: the address and the length of the block, the address and the length in bytes of the samples.

use crate::raw::{
    BlackLevel, CameraId, ColourMatrix, Illuminant, InputProfile, Levels, NamedProfile,
    NoiseProfile, Orientation, RawImage, Rect, SampleKind, Samples, SensorLayout,
};
use thiserror::Error;

/// The four bytes a block starts with.
pub const MAGIC: [u8; 4] = *b"ARIB";

/// The version of the block this crate writes and reads.
pub const VERSION: u16 = 1;

/// The largest block a reader accepts: an ICC profile can be a few megabytes, nothing else is large.
pub const MAX_BLOCK_LEN: usize = 16 << 20;

/// The tags of the sections.
pub mod tag {
    /// Width and height of the readout.
    pub const GEOMETRY: u16 = 1;
    /// The layout of the samples.
    pub const LAYOUT: u16 = 2;
    /// The type and the count of the samples.
    pub const SAMPLES: u16 = 3;
    /// The black and white levels.
    pub const LEVELS: u16 = 4;
    /// The as-shot white balance.
    pub const WHITE_BALANCE: u16 = 5;
    /// A colour matrix (repeatable).
    pub const COLOUR_MATRIX: u16 = 6;
    /// The recommended crop.
    pub const CROP: u16 = 7;
    /// The active area.
    pub const ACTIVE_AREA: u16 = 8;
    /// The orientation.
    pub const ORIENTATION: u16 = 9;
    /// The camera's make and model.
    pub const CAMERA: u16 = 10;
    /// The input profile.
    pub const INPUT_PROFILE: u16 = 11;
    /// The ISO speed.
    pub const ISO: u16 = 12;
    /// The noise profile.
    pub const NOISE_PROFILE: u16 = 13;
    /// The first tag a decoder may use for itself; readers skip them.
    pub const PRIVATE: u16 = 0x8000;
}

/// What is wrong with a block, or with an image that cannot be written as one.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BlockError {
    /// The block does not start with [`MAGIC`].
    #[error("not a block: the first four bytes are not \"ARIB\"")]
    BadMagic,
    /// The block is of a version this reader does not know.
    #[error("block version {0} is newer than the {VERSION} this reader knows")]
    UnsupportedVersion(u16),
    /// The block is longer than [`MAX_BLOCK_LEN`].
    #[error("the block is {0} bytes, more than {MAX_BLOCK_LEN}")]
    TooLong(usize),
    /// The block ends inside a header or a section.
    #[error("the block ends too soon")]
    Truncated,
    /// A section says it is longer than the rest of the block.
    #[error("section {0} runs past the end of the block")]
    SectionPastEnd(u16),
    /// A section that may occur once occurs twice.
    #[error("section {0} occurs twice")]
    Duplicate(u16),
    /// A required section is not there.
    #[error("the required section {0} is missing")]
    Missing(u16),
    /// A section is not what its tag says.
    #[error("section {tag}: {what}")]
    BadSection {
        /// The section.
        tag: u16,
        /// What is wrong.
        what: &'static str,
    },
    /// A float is NaN or infinite.
    #[error("section {0} holds a NaN or an infinite number")]
    NotFinite(u16),
    /// The samples are not a whole number of values.
    #[error("{bytes} bytes of samples are not a whole number of {size}-byte values")]
    SamplesNotWhole {
        /// How many bytes.
        bytes: usize,
        /// How many bytes a value takes.
        size: usize,
    },
    /// The samples given are not the ones the block describes.
    #[error(
        "the block describes {expected_count} {expected:?} samples, there are {given_count} {given:?}"
    )]
    SamplesMismatch {
        /// What the block says.
        expected: SampleKind,
        /// What the block says.
        expected_count: u64,
        /// What was given.
        given: SampleKind,
        /// What was given.
        given_count: u64,
    },
}

fn bad(tag: u16, what: &'static str) -> BlockError {
    BlockError::BadSection { tag, what }
}

// ---------------------------------------------------------------------------------------------------------------------
// What an image must be to be written or read: one check for both directions.
// ---------------------------------------------------------------------------------------------------------------------

fn finite(tag: u16, values: &[f32]) -> Result<(), BlockError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(BlockError::NotFinite(tag))
    }
}

fn check(image: &RawImage) -> Result<(), BlockError> {
    if image.width == 0 || image.height == 0 {
        return Err(bad(tag::GEOMETRY, "the readout has no pixels"));
    }
    match &image.layout {
        SensorLayout::Cfa {
            width,
            height,
            colours,
        } => {
            if *width == 0 || *height == 0 {
                return Err(bad(tag::LAYOUT, "an empty pattern"));
            }
            if colours.len() != usize::from(*width) * usize::from(*height) {
                return Err(bad(
                    tag::LAYOUT,
                    "the pattern's colours are not width * height",
                ));
            }
            if colours.iter().any(|c| *c > 3) {
                return Err(bad(tag::LAYOUT, "a pattern colour above 3"));
            }
        }
        SensorLayout::LinearRgb {
            components,
            profile,
        } => {
            if *components == 0 {
                return Err(bad(tag::LAYOUT, "no components"));
            }
            if let InputProfile::Icc(bytes) = profile
                && bytes.len() > MAX_BLOCK_LEN
            {
                return Err(BlockError::TooLong(bytes.len()));
            }
        }
        SensorLayout::Mono => {}
    }
    let pixels = u64::from(image.width) * u64::from(image.height);
    let expected = pixels * image.components() as u64;
    if image.samples.len() as u64 != expected {
        return Err(bad(
            tag::SAMPLES,
            "the samples are not width * height * components",
        ));
    }
    let black = &image.levels.black;
    if black.rows == 0 || black.columns == 0 || black.components == 0 {
        return Err(bad(tag::LEVELS, "an empty black level pattern"));
    }
    if black.values.len()
        != usize::from(black.rows) * usize::from(black.columns) * usize::from(black.components)
    {
        return Err(bad(
            tag::LEVELS,
            "black values are not rows * columns * components",
        ));
    }
    finite(tag::LEVELS, &black.values)?;
    if image.levels.white.is_empty() || image.levels.white.len() > 255 {
        return Err(bad(tag::LEVELS, "white needs 1 to 255 values"));
    }
    finite(tag::LEVELS, &image.levels.white)?;
    if let Some(wb) = &image.white_balance {
        finite(tag::WHITE_BALANCE, wb)?;
    }
    for m in &image.colour {
        if !(3..=4).contains(&m.rows) || m.xyz_to_camera.len() != usize::from(m.rows) * 3 {
            return Err(bad(tag::COLOUR_MATRIX, "a matrix of 3 or 4 rows of 3"));
        }
        finite(tag::COLOUR_MATRIX, &m.xyz_to_camera)?;
    }
    for text in [&image.camera.make, &image.camera.model] {
        if text.len() > usize::from(u16::MAX) {
            return Err(bad(tag::CAMERA, "a name longer than 65535 bytes"));
        }
    }
    if let Some(noise) = &image.noise_profile {
        if noise.pairs.is_empty() || noise.pairs.len() > 255 {
            return Err(bad(tag::NOISE_PROFILE, "1 to 255 pairs"));
        }
        if !noise.pairs.iter().flatten().all(|v| v.is_finite()) {
            return Err(BlockError::NotFinite(tag::NOISE_PROFILE));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// Writing.
// ---------------------------------------------------------------------------------------------------------------------

struct Writer(Vec<u8>);

impl Writer {
    fn section(&mut self, tag: u16, body: impl FnOnce(&mut Vec<u8>)) {
        self.0.extend_from_slice(&tag.to_le_bytes());
        let length_at = self.0.len();
        self.0.extend_from_slice(&[0; 4]);
        body(&mut self.0);
        let length = (self.0.len() - length_at - 4) as u32;
        self.0[length_at..length_at + 4].copy_from_slice(&length.to_le_bytes());
    }
}

fn put_f32s(out: &mut Vec<u8>, values: &[f32]) {
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn put_rect(out: &mut Vec<u8>, r: &Rect) {
    for v in [r.x, r.y, r.width, r.height] {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

impl RawImage {
    /// The block that describes this image, everything but the samples (which travel by their own path). Refuses an
    /// image that is not valid (a NaN, a pattern of the wrong size, samples that are not `width * height * components`).
    pub fn to_block(&self) -> Result<Vec<u8>, BlockError> {
        check(self)?;
        let mut w = Writer(Vec::with_capacity(256));
        w.0.extend_from_slice(&MAGIC);
        w.0.extend_from_slice(&VERSION.to_le_bytes());
        w.0.extend_from_slice(&0u16.to_le_bytes());
        w.section(tag::GEOMETRY, |o| {
            o.extend_from_slice(&self.width.to_le_bytes());
            o.extend_from_slice(&self.height.to_le_bytes());
        });
        w.section(tag::LAYOUT, |o| match &self.layout {
            SensorLayout::Cfa {
                width,
                height,
                colours,
            } => {
                o.extend_from_slice(&[0, *width, *height]);
                o.extend_from_slice(colours);
            }
            SensorLayout::LinearRgb { components, .. } => o.extend_from_slice(&[1, *components]),
            SensorLayout::Mono => o.push(2),
        });
        w.section(tag::SAMPLES, |o| {
            o.push(match self.samples.kind() {
                SampleKind::U16 => 0,
                SampleKind::F32 => 1,
            });
            o.extend_from_slice(&(self.samples.len() as u64).to_le_bytes());
        });
        w.section(tag::LEVELS, |o| {
            let b = &self.levels.black;
            o.extend_from_slice(&b.rows.to_le_bytes());
            o.extend_from_slice(&b.columns.to_le_bytes());
            o.push(b.components);
            put_f32s(o, &b.values);
            o.push(self.levels.white.len() as u8);
            put_f32s(o, &self.levels.white);
        });
        if let Some(wb) = &self.white_balance {
            w.section(tag::WHITE_BALANCE, |o| put_f32s(o, wb));
        }
        for m in &self.colour {
            w.section(tag::COLOUR_MATRIX, |o| {
                o.extend_from_slice(&[m.illuminant.0, m.rows]);
                put_f32s(o, &m.xyz_to_camera);
            });
        }
        if let Some(r) = &self.crop {
            w.section(tag::CROP, |o| put_rect(o, r));
        }
        if let Some(r) = &self.active_area {
            w.section(tag::ACTIVE_AREA, |o| put_rect(o, r));
        }
        if self.orientation != Orientation::Unknown {
            w.section(tag::ORIENTATION, |o| o.push(self.orientation.to_exif()));
        }
        if !self.camera.make.is_empty() || !self.camera.model.is_empty() {
            w.section(tag::CAMERA, |o| {
                for text in [&self.camera.make, &self.camera.model] {
                    o.extend_from_slice(&(text.len() as u16).to_le_bytes());
                    o.extend_from_slice(text.as_bytes());
                }
            });
        }
        if let SensorLayout::LinearRgb { profile, .. } = &self.layout {
            match profile {
                InputProfile::Unspecified => {}
                InputProfile::Named(n) => w.section(tag::INPUT_PROFILE, |o| {
                    o.push(match n {
                        NamedProfile::Srgb => 0,
                        NamedProfile::AdobeRgb => 1,
                        NamedProfile::Rec2020 => 2,
                        NamedProfile::ProPhoto => 3,
                    })
                }),
                InputProfile::Icc(bytes) => w.section(tag::INPUT_PROFILE, |o| {
                    o.push(4);
                    o.extend_from_slice(bytes);
                }),
            }
        }
        if let Some(iso) = self.iso {
            w.section(tag::ISO, |o| o.extend_from_slice(&iso.to_le_bytes()));
        }
        if let Some(noise) = &self.noise_profile {
            w.section(tag::NOISE_PROFILE, |o| {
                o.push(noise.pairs.len() as u8);
                for [scale, offset] in &noise.pairs {
                    o.extend_from_slice(&scale.to_le_bytes());
                    o.extend_from_slice(&offset.to_le_bytes());
                }
            });
        }
        if w.0.len() > MAX_BLOCK_LEN {
            return Err(BlockError::TooLong(w.0.len()));
        }
        Ok(w.0)
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Reading.
// ---------------------------------------------------------------------------------------------------------------------

/// A bounds-checked cursor over one section's payload.
struct Cursor<'a> {
    tag: u16,
    data: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], BlockError> {
        if self.data.len() < n {
            return Err(bad(self.tag, "the section is shorter than its content"));
        }
        let (head, rest) = self.data.split_at(n);
        self.data = rest;
        Ok(head)
    }
    fn u8(&mut self) -> Result<u8, BlockError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, BlockError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32, BlockError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&mut self) -> Result<u64, BlockError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes(b.try_into().expect("8 bytes")))
    }
    fn f32s(&mut self, n: usize) -> Result<Vec<f32>, BlockError> {
        let bytes = self.take(n.checked_mul(4).ok_or(bad(self.tag, "too many values"))?)?;
        let values: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        finite(self.tag, &values)?;
        Ok(values)
    }
    fn f64(&mut self) -> Result<f64, BlockError> {
        let b = self.take(8)?;
        let v = f64::from_le_bytes(b.try_into().expect("8 bytes"));
        if v.is_finite() {
            Ok(v)
        } else {
            Err(BlockError::NotFinite(self.tag))
        }
    }
    fn rect(&mut self) -> Result<Rect, BlockError> {
        Ok(Rect {
            x: self.u32()?,
            y: self.u32()?,
            width: self.u32()?,
            height: self.u32()?,
        })
    }
    fn text(&mut self) -> Result<String, BlockError> {
        let n = usize::from(self.u16()?);
        String::from_utf8(self.take(n)?.to_vec())
            .map_err(|_| bad(self.tag, "a name that is not UTF-8"))
    }
    /// A known section uses exactly its payload.
    fn end(&self) -> Result<(), BlockError> {
        if self.data.is_empty() {
            Ok(())
        } else {
            Err(bad(self.tag, "bytes left after the content"))
        }
    }
}

/// The sections of a block, in the order they came, with the header checked. Unknown tags are not here.
fn sections(block: &[u8]) -> Result<Vec<(u16, &[u8])>, BlockError> {
    if block.len() > MAX_BLOCK_LEN {
        return Err(BlockError::TooLong(block.len()));
    }
    if block.len() < 8 {
        return Err(BlockError::Truncated);
    }
    if block[..4] != MAGIC {
        return Err(BlockError::BadMagic);
    }
    let version = u16::from_le_bytes([block[4], block[5]]);
    if version == 0 || version > VERSION {
        return Err(BlockError::UnsupportedVersion(version));
    }
    let mut rest = &block[8..];
    let mut out = Vec::new();
    while !rest.is_empty() {
        if rest.len() < 6 {
            return Err(BlockError::Truncated);
        }
        let tag = u16::from_le_bytes([rest[0], rest[1]]);
        let length = u32::from_le_bytes([rest[2], rest[3], rest[4], rest[5]]) as usize;
        rest = &rest[6..];
        if length > rest.len() {
            return Err(BlockError::SectionPastEnd(tag));
        }
        let (payload, after) = rest.split_at(length);
        rest = after;
        out.push((tag, payload));
    }
    Ok(out)
}

/// What the block says about the samples: their type and their count.
fn samples_section(sections: &[(u16, &[u8])]) -> Result<(SampleKind, u64), BlockError> {
    let mut found = None;
    for (t, payload) in sections {
        if *t == tag::SAMPLES {
            if found.is_some() {
                return Err(BlockError::Duplicate(tag::SAMPLES));
            }
            let mut c = Cursor {
                tag: *t,
                data: payload,
            };
            let kind = match c.u8()? {
                0 => SampleKind::U16,
                1 => SampleKind::F32,
                _ => return Err(bad(*t, "an unknown sample type")),
            };
            let count = c.u64()?;
            c.end()?;
            found = Some((kind, count));
        }
    }
    found.ok_or(BlockError::Missing(tag::SAMPLES))
}

impl RawImage {
    /// The type and the count of the samples a block describes: what a reader needs to know to read them from the
    /// plugin's memory before it can build the image with [`RawImage::from_block`].
    pub fn block_samples(block: &[u8]) -> Result<(SampleKind, u64), BlockError> {
        samples_section(&sections(block)?)
    }

    /// The image a block describes, with the `samples` read from the plugin's memory. The samples must be the ones the
    /// block describes (the same type and count); a block that is not well formed, or that describes an impossible
    /// image, is an error and never a panic.
    pub fn from_block(block: &[u8], samples: Samples) -> Result<RawImage, BlockError> {
        let all = sections(block)?;
        let (kind, count) = samples_section(&all)?;
        if kind != samples.kind() || count != samples.len() as u64 {
            return Err(BlockError::SamplesMismatch {
                expected: kind,
                expected_count: count,
                given: samples.kind(),
                given_count: samples.len() as u64,
            });
        }
        let mut geometry = None;
        let mut layout: Option<SensorLayout> = None;
        let mut levels = None;
        let mut white_balance = None;
        let mut colour = Vec::new();
        let mut crop = None;
        let mut active_area = None;
        let mut orientation = None;
        let mut camera = None;
        let mut profile = None;
        let mut iso = None;
        let mut noise_profile = None;
        let once = |slot: bool, t: u16| {
            if slot {
                Err(BlockError::Duplicate(t))
            } else {
                Ok(())
            }
        };
        for (t, payload) in all {
            let mut c = Cursor {
                tag: t,
                data: payload,
            };
            match t {
                tag::GEOMETRY => {
                    once(geometry.is_some(), t)?;
                    geometry = Some((c.u32()?, c.u32()?));
                    c.end()?;
                }
                tag::LAYOUT => {
                    once(layout.is_some(), t)?;
                    layout = Some(match c.u8()? {
                        0 => {
                            let (width, height) = (c.u8()?, c.u8()?);
                            let colours =
                                c.take(usize::from(width) * usize::from(height))?.to_vec();
                            c.end()?;
                            SensorLayout::Cfa {
                                width,
                                height,
                                colours,
                            }
                        }
                        1 => {
                            let components = c.u8()?;
                            c.end()?;
                            SensorLayout::LinearRgb {
                                components,
                                profile: InputProfile::Unspecified,
                            }
                        }
                        2 => {
                            c.end()?;
                            SensorLayout::Mono
                        }
                        _ => return Err(bad(t, "an unknown layout")),
                    });
                }
                tag::SAMPLES => {}
                tag::LEVELS => {
                    once(levels.is_some(), t)?;
                    let rows = c.u16()?;
                    let columns = c.u16()?;
                    let components = c.u8()?;
                    let n = usize::from(rows) * usize::from(columns) * usize::from(components);
                    let values = c.f32s(n)?;
                    let white_n = usize::from(c.u8()?);
                    let white = c.f32s(white_n)?;
                    c.end()?;
                    levels = Some(Levels {
                        black: BlackLevel {
                            rows,
                            columns,
                            components,
                            values,
                        },
                        white,
                    });
                }
                tag::WHITE_BALANCE => {
                    once(white_balance.is_some(), t)?;
                    let v = c.f32s(3)?;
                    c.end()?;
                    white_balance = Some([v[0], v[1], v[2]]);
                }
                tag::COLOUR_MATRIX => {
                    let illuminant = Illuminant(c.u8()?);
                    let rows = c.u8()?;
                    if !(3..=4).contains(&rows) {
                        return Err(bad(t, "a matrix of 3 or 4 rows"));
                    }
                    let xyz_to_camera = c.f32s(usize::from(rows) * 3)?;
                    c.end()?;
                    colour.push(ColourMatrix {
                        illuminant,
                        rows,
                        xyz_to_camera,
                    });
                }
                tag::CROP => {
                    once(crop.is_some(), t)?;
                    crop = Some(c.rect()?);
                    c.end()?;
                }
                tag::ACTIVE_AREA => {
                    once(active_area.is_some(), t)?;
                    active_area = Some(c.rect()?);
                    c.end()?;
                }
                tag::ORIENTATION => {
                    once(orientation.is_some(), t)?;
                    let value = c.u8()?;
                    if value > 8 {
                        return Err(bad(t, "an orientation above 8"));
                    }
                    c.end()?;
                    orientation = Some(Orientation::from_exif(value));
                }
                tag::CAMERA => {
                    once(camera.is_some(), t)?;
                    let make = c.text()?;
                    let model = c.text()?;
                    c.end()?;
                    camera = Some(CameraId { make, model });
                }
                tag::INPUT_PROFILE => {
                    once(profile.is_some(), t)?;
                    profile = Some(match c.u8()? {
                        0 => InputProfile::Named(NamedProfile::Srgb),
                        1 => InputProfile::Named(NamedProfile::AdobeRgb),
                        2 => InputProfile::Named(NamedProfile::Rec2020),
                        3 => InputProfile::Named(NamedProfile::ProPhoto),
                        4 => InputProfile::Icc(c.data.to_vec()),
                        _ => return Err(bad(t, "an unknown input profile")),
                    });
                }
                tag::ISO => {
                    once(iso.is_some(), t)?;
                    iso = Some(c.u32()?);
                    c.end()?;
                }
                tag::NOISE_PROFILE => {
                    once(noise_profile.is_some(), t)?;
                    let n = usize::from(c.u8()?);
                    let mut pairs = Vec::with_capacity(n);
                    for _ in 0..n {
                        pairs.push([c.f64()?, c.f64()?]);
                    }
                    c.end()?;
                    noise_profile = Some(NoiseProfile { pairs });
                }
                // A tag this reader does not know: skipped, whatever its number.
                _ => {}
            }
        }
        let (width, height) = geometry.ok_or(BlockError::Missing(tag::GEOMETRY))?;
        let mut layout = layout.ok_or(BlockError::Missing(tag::LAYOUT))?;
        // The input profile belongs to the pixels that already have their channels; on a mosaic it means nothing.
        if let (SensorLayout::LinearRgb { profile: slot, .. }, Some(found)) = (&mut layout, profile)
        {
            *slot = found;
        }
        let image = RawImage {
            width,
            height,
            layout,
            samples,
            levels: levels.ok_or(BlockError::Missing(tag::LEVELS))?,
            white_balance,
            colour,
            crop,
            active_area,
            orientation: orientation.unwrap_or_default(),
            camera: camera.unwrap_or_default(),
            iso,
            noise_profile,
        };
        check(&image)?;
        Ok(image)
    }
}
