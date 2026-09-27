// SPDX-License-Identifier: GPL-3.0-or-later
//! Making a thumbnail from a decoded preview (architecture §5.7, D-075): 256 px on the long
//! edge, encoded as a small JPEG. Resizing avoids copies where `fast_image_resize` lets it
//! (spike 3: cropping inside the resizer, not around it, was the fix that raised generation from
//! 350 to 830 thumbnails per second).

use std::path::Path;

use fast_image_resize as fir;
use image::DynamicImage;

use crate::error::{ImagingError, Result};

/// A generated thumbnail: JPEG bytes and the size actually produced (never upscaled past the
/// source, so a tiny embedded preview stays tiny).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnail {
    /// The encoded JPEG.
    pub jpeg: Vec<u8>,
    /// Its width.
    pub width: u32,
    /// Its height.
    pub height: u32,
}

/// `tiff:Orientation`'s eight values (1 to 8), applied before resizing so the stored thumbnail is
/// already upright: the sidecar's own [`crate::Metadata::orientation`], not re-read from the
/// file (architecture: "applies the sidecar's stored preview dimensions and orientation").
pub(crate) fn apply_orientation(image: DynamicImage, orientation: Option<u32>) -> DynamicImage {
    match orientation {
        Some(2) => image.fliph(),
        Some(3) => image.rotate180(),
        Some(4) => image.flipv(),
        Some(5) => image.rotate90().fliph(),
        Some(6) => image.rotate90(),
        Some(7) => image.rotate270().fliph(),
        Some(8) => image.rotate270(),
        _ => image,
    }
}

/// Resizes `image` to `long_edge` pixels on its longer side (never upscaling) and encodes it as
/// a JPEG at quality 85 (spike 3's own thumbnails, 8.1 KB each at 256 px, used the same order of
/// quality). `path` is only for the error message: the image is already decoded.
pub fn make_thumbnail(
    image: DynamicImage,
    orientation: Option<u32>,
    long_edge: u32,
    path: &Path,
) -> Result<Thumbnail> {
    make_scaled(image, orientation, long_edge, 85, path)
}

/// [`make_thumbnail`] with the JPEG quality (1 to 100) chosen by the caller: what the image view is served
/// at is not what a 256 px thumbnail is.
pub(crate) fn make_scaled(
    image: DynamicImage,
    orientation: Option<u32>,
    long_edge: u32,
    quality: u8,
    path: &Path,
) -> Result<Thumbnail> {
    let resized = scaled_rgb(image, orientation, long_edge, path)?;
    encode_jpeg(&resized, quality, path)
}

/// The image upright and at most `long_edge` pixels on its long side (never enlarged).
pub(crate) fn scaled_rgb(
    image: DynamicImage,
    orientation: Option<u32>,
    long_edge: u32,
    path: &Path,
) -> Result<image::RgbImage> {
    let image = apply_orientation(image, orientation);
    let rgb = image.to_rgb8();
    let (src_w, src_h) = rgb.dimensions();
    let scale = (long_edge as f32 / src_w.max(src_h).max(1) as f32).min(1.0);
    let dst_w = ((src_w as f32 * scale).round() as u32).max(1);
    let dst_h = ((src_h as f32 * scale).round() as u32).max(1);
    if (dst_w, dst_h) == (src_w, src_h) {
        return Ok(rgb);
    }
    let src_image =
        fir::images::Image::from_vec_u8(src_w, src_h, rgb.into_raw(), fir::PixelType::U8x3)
            .map_err(|e| decode_err(path, e))?;
    let mut dst_image = fir::images::Image::new(dst_w, dst_h, fir::PixelType::U8x3);
    fir::Resizer::new()
        .resize(&src_image, &mut dst_image, None)
        .map_err(|e| decode_err(path, e))?;
    image::RgbImage::from_raw(dst_w, dst_h, dst_image.into_vec())
        .ok_or_else(|| decode_err(path, "the resized image has the wrong size"))
}

/// An RGB image as a JPEG at `quality`.
pub(crate) fn encode_jpeg(rgb: &image::RgbImage, quality: u8, path: &Path) -> Result<Thumbnail> {
    let (width, height) = rgb.dimensions();
    let mut jpeg = Vec::new();
    jpeg_encoder::Encoder::new(&mut jpeg, quality)
        .encode(
            rgb.as_raw(),
            width as u16,
            height as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .map_err(|e| decode_err(path, e))?;
    Ok(Thumbnail {
        jpeg,
        width,
        height,
    })
}

fn decode_err(path: &Path, message: impl std::fmt::Display) -> ImagingError {
    ImagingError::Decode {
        path: path.to_path_buf(),
        message: message.to_string(),
    }
}
