// SPDX-License-Identifier: MIT OR Apache-2.0
//! `RawImage`: what a decoder returns and what the image pipeline starts from (decision D-141, design note 005 §3).
//!
//! A RAW file decoded to its sensor's own data: not yet demosaiced, not yet colour-managed. It carries what a pipeline
//! needs to start (the layout of the colour filter, the black and white levels, the as-shot white balance, the camera's
//! colour matrices, the recommended crop, the orientation, the camera), and **every piece of data may be absent**: a
//! monochrome file has no matrix and no white balance, and a decoder that cannot read a file says so with
//! [`DecoderError::Invalid`](crate::DecoderError::Invalid), a routine outcome and not a failure.
//!
//! Inputs that are not sensor data (a TIFF scan, a PNG, a JPEG, an sRAW) are [`SensorLayout::LinearRgb`] with an
//! [`InputProfile`], since the pipeline must know what colours it is in.
//!
//! Across the sandbox everything but the samples travels as a language-neutral block of tagged sections, documented in
//! [`crate::block`]; the types here are the Rust side of it.

use crate::block::BlockError;

/// A rectangle in sensor pixels, from the sensor's origin (the top-left of the full readout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    /// The left edge.
    pub x: u32,
    /// The top edge.
    pub y: u32,
    /// The width.
    pub width: u32,
    /// The height.
    pub height: u32,
}

/// How the picture is to be turned to be upright: the eight values of the EXIF `Orientation` tag, and `Unknown` for a
/// decoder that does not know (it is not `Normal`: some decoders report `Normal` for a file whose EXIF says otherwise,
/// and a caller that has the EXIF prefers it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Orientation {
    /// The decoder did not say.
    #[default]
    Unknown,
    /// EXIF 1: as it is.
    Normal,
    /// EXIF 2: mirrored left to right.
    MirrorHorizontal,
    /// EXIF 3: turned by 180 degrees.
    Rotate180,
    /// EXIF 4: mirrored top to bottom.
    MirrorVertical,
    /// EXIF 5: mirrored along the main diagonal.
    Transpose,
    /// EXIF 6: to be turned 90 degrees clockwise.
    Rotate90,
    /// EXIF 7: mirrored along the other diagonal.
    Transverse,
    /// EXIF 8: to be turned 270 degrees clockwise.
    Rotate270,
}

impl Orientation {
    /// The orientation of an EXIF value (`0`, and anything above 8, is `Unknown`).
    pub fn from_exif(value: u8) -> Orientation {
        match value {
            1 => Orientation::Normal,
            2 => Orientation::MirrorHorizontal,
            3 => Orientation::Rotate180,
            4 => Orientation::MirrorVertical,
            5 => Orientation::Transpose,
            6 => Orientation::Rotate90,
            7 => Orientation::Transverse,
            8 => Orientation::Rotate270,
            _ => Orientation::Unknown,
        }
    }

    /// The EXIF value (`0` for `Unknown`).
    pub fn to_exif(self) -> u8 {
        match self {
            Orientation::Unknown => 0,
            Orientation::Normal => 1,
            Orientation::MirrorHorizontal => 2,
            Orientation::Rotate180 => 3,
            Orientation::MirrorVertical => 4,
            Orientation::Transpose => 5,
            Orientation::Rotate90 => 6,
            Orientation::Transverse => 7,
            Orientation::Rotate270 => 8,
        }
    }
}

/// The light a colour matrix was made for, as an EXIF `LightSource` code (so that any decoder, in any language, can
/// write it without a table of ours).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Illuminant(pub u8);

impl Illuminant {
    /// EXIF `Unknown`.
    pub const UNKNOWN: Illuminant = Illuminant(0);
    /// Standard illuminant A (tungsten, 2856 K).
    pub const A: Illuminant = Illuminant(17);
    /// Daylight D55.
    pub const D55: Illuminant = Illuminant(20);
    /// Daylight D65.
    pub const D65: Illuminant = Illuminant(21);
    /// Daylight D75.
    pub const D75: Illuminant = Illuminant(22);
    /// Daylight D50.
    pub const D50: Illuminant = Illuminant(23);
}

/// A camera's colour matrix for one illuminant: CIE XYZ to the camera's own channels.
///
/// `rows` camera channels (3, or 4 for a four-colour sensor) by 3 columns, row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct ColourMatrix {
    /// What it is for.
    pub illuminant: Illuminant,
    /// The channels of the camera: 3 or 4.
    pub rows: u8,
    /// `rows * 3` values, row-major, all finite.
    pub xyz_to_camera: Vec<f32>,
}

/// The make and the model of the camera, as the decoder cleaned them (possibly empty).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CameraId {
    /// The make.
    pub make: String,
    /// The model.
    pub model: String,
}

/// A colour space named by the input, for data that is not sensor data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedProfile {
    /// sRGB (IEC 61966-2-1).
    Srgb,
    /// Adobe RGB (1998).
    AdobeRgb,
    /// Rec. 2020.
    Rec2020,
    /// ProPhoto RGB.
    ProPhoto,
}

/// The colours of an input that is not sensor data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum InputProfile {
    /// The file does not say.
    #[default]
    Unspecified,
    /// A space the file names.
    Named(NamedProfile),
    /// An ICC profile, as its bytes.
    Icc(Vec<u8>),
}

/// What the samples are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SensorLayout {
    /// A colour filter array over a single-channel sensor: Bayer (2x2), X-Trans (6x6) or another pattern. The pattern is
    /// anchored at the sensor's origin, not at the crop (a Fujifilm crop can start on an odd row of a 6x6 pattern).
    Cfa {
        /// The pattern's width.
        width: u8,
        /// The pattern's height.
        height: u8,
        /// The colour at each place of the pattern, row-major, `width * height` of them: `0` red, `1` green, `2` blue,
        /// `3` a fourth colour (emerald, or a second green).
        colours: Vec<u8>,
    },
    /// Pixels that already have their channels (an sRAW, a scan, a PNG or a JPEG): the first stage of the pipeline is
    /// skipped.
    LinearRgb {
        /// How many channels a pixel has.
        components: u8,
        /// The colours they are in.
        profile: InputProfile,
    },
    /// A monochrome sensor: one channel, no filter.
    Mono,
}

/// The type of the samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleKind {
    /// Unsigned 16 bits.
    U16,
    /// 32-bit floating point (a float DNG).
    F32,
}

impl SampleKind {
    /// How many bytes a sample takes.
    pub fn size(self) -> usize {
        match self {
            SampleKind::U16 => 2,
            SampleKind::F32 => 4,
        }
    }
}

/// The samples, `width * height * components` of them, row-major. Non-exhaustive: other types can come.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Samples {
    /// Unsigned 16 bits.
    U16(Vec<u16>),
    /// 32-bit floating point.
    F32(Vec<f32>),
}

impl Samples {
    /// The type of the samples.
    pub fn kind(&self) -> SampleKind {
        match self {
            Samples::U16(_) => SampleKind::U16,
            Samples::F32(_) => SampleKind::F32,
        }
    }

    /// How many samples there are.
    pub fn len(&self) -> usize {
        match self {
            Samples::U16(v) => v.len(),
            Samples::F32(v) => v.len(),
        }
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Reads samples of `kind` from little-endian bytes (what a plugin wrote into its memory). The length of `bytes`
    /// must be a whole number of samples.
    pub fn from_le_bytes(kind: SampleKind, bytes: &[u8]) -> Result<Samples, BlockError> {
        if !bytes.len().is_multiple_of(kind.size()) {
            return Err(BlockError::SamplesNotWhole {
                bytes: bytes.len(),
                size: kind.size(),
            });
        }
        Ok(match kind {
            SampleKind::U16 => Samples::U16(
                bytes
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect(),
            ),
            SampleKind::F32 => Samples::F32(
                bytes
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect(),
            ),
        })
    }
}

/// The black level: a repeating pattern of `rows x columns x components` values, like `rawler`'s (the values of a
/// float DNG differ within the pattern: 2047, 2047, 2048, 2047).
#[derive(Debug, Clone, PartialEq)]
pub struct BlackLevel {
    /// The pattern's height (1 when one value serves the whole sensor).
    pub rows: u16,
    /// The pattern's width.
    pub columns: u16,
    /// The channels each place has a value for (1 for a mosaic, 3 for an sRAW).
    pub components: u8,
    /// `rows * columns * components` values, row-major, the channels of one place together; all finite.
    pub values: Vec<f32>,
}

/// The levels the samples are measured against.
#[derive(Debug, Clone, PartialEq)]
pub struct Levels {
    /// Where black is.
    pub black: BlackLevel,
    /// Where white is, one value for each channel (or one for all): at least one, all finite.
    pub white: Vec<f32>,
}

/// The noise model a DNG carries (the `NoiseProfile` tag): for each channel, a scale and an offset, so that the
/// variance of a sample of value `x` is `scale * x + offset`.
#[derive(Debug, Clone, PartialEq)]
pub struct NoiseProfile {
    /// `(scale, offset)` for each channel, at least one, all finite.
    pub pairs: Vec<[f64; 2]>,
}

/// A RAW file decoded to its sensor's own data (see the module documentation).
#[derive(Debug, Clone, PartialEq)]
pub struct RawImage {
    /// The width of the full readout, in pixels.
    pub width: u32,
    /// The height of the full readout, in pixels.
    pub height: u32,
    /// What the samples are.
    pub layout: SensorLayout,
    /// The samples.
    pub samples: Samples,
    /// The black and white levels.
    pub levels: Levels,
    /// The as-shot white balance, R G B (a decoder's fourth value, a second green, is not carried), when the file has one.
    pub white_balance: Option<[f32; 3]>,
    /// The camera's colour matrices, one for each illuminant the file has (possibly none).
    pub colour: Vec<ColourMatrix>,
    /// The crop the camera recommends, in sensor pixels.
    pub crop: Option<Rect>,
    /// The part of the sensor that sees light, in sensor pixels.
    pub active_area: Option<Rect>,
    /// How the picture is to be turned.
    pub orientation: Orientation,
    /// The camera.
    pub camera: CameraId,
    /// The ISO speed the file says, when it does.
    pub iso: Option<u32>,
    /// The noise model, when the file carries one (a DNG).
    pub noise_profile: Option<NoiseProfile>,
}

impl RawImage {
    /// How many channels the samples have at each pixel.
    pub fn components(&self) -> usize {
        match &self.layout {
            SensorLayout::Cfa { .. } | SensorLayout::Mono => 1,
            SensorLayout::LinearRgb { components, .. } => usize::from(*components),
        }
    }
}
