// SPDX-License-Identifier: GPL-3.0-or-later
//! The quality aids of culling (spec §5.3, D-103): how sharp a picture is, where it is sharp (focus peaking), where
//! it is clipped (highlights and shadows) and its histogram. They read the picture as the image view shows it, the
//! camera's embedded preview or the JPEG itself, **not the RAW data**: what a camera's own processing did to the
//! tones and the sharpness shows in them, and a real clipping of the sensor's data is not the same as a clipped
//! preview. Redoing them on the RAW data needs the image engine (M2).
//!
//! The numbers (`Aids`) are measured on an analysis copy at most [`ANALYSIS_EDGE`] pixels on its long side, so
//! that the sharpness of two frames of one series, whatever their size, can be compared. The masks (peaking,
//! clipping) are made at the picture's own size, to lie exactly over it.

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, GrayImage, Rgba, RgbaImage};

use crate::error::{ImagingError, Result};

/// The long edge, in pixels, of the copy the numbers are measured on.
pub const ANALYSIS_EDGE: u32 = 1024;

/// A channel at or above this is clipped in the highlights.
const HIGH: u8 = 250;
/// All channels at or below this: clipped in the shadows.
const LOW: u8 = 5;
/// A luma Laplacian (on the scale of 0 to 1020) above this is an edge in focus.
const EDGE: i32 = 64;

/// What is measured on a picture.
#[derive(Debug, Clone, PartialEq)]
pub struct Aids {
    /// How sharp: the variance of the Laplacian of the luma. Only meaningful compared with another picture's
    /// (frames of a series), never as a number of its own.
    pub sharpness: f32,
    /// Luma, red, green and blue, 256 bins each.
    pub histogram: [[u32; 256]; 4],
    /// The share of pixels (0 to 1) with a channel in the highlights' clip.
    pub clipped_high: f32,
    /// The share of pixels (0 to 1) whose channels are all in the shadows' clip.
    pub clipped_low: f32,
}

fn analysis_copy(image: &DynamicImage) -> DynamicImage {
    let (w, h) = image.dimensions();
    if w.max(h) <= ANALYSIS_EDGE {
        return image.clone();
    }
    let scale = ANALYSIS_EDGE as f32 / w.max(h) as f32;
    image.resize(
        ((w as f32 * scale).round() as u32).max(1),
        ((h as f32 * scale).round() as u32).max(1),
        FilterType::Triangle,
    )
}

/// The variance of the Laplacian of a grayscale image (its interior).
fn laplacian_variance(luma: &GrayImage) -> f32 {
    let (w, h) = luma.dimensions();
    if w < 3 || h < 3 {
        return 0.0;
    }
    let get = |x: u32, y: u32| i32::from(luma.get_pixel(x, y).0[0]);
    let (mut sum, mut sum_sq, mut n) = (0f64, 0f64, 0f64);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let l = 4 * get(x, y) - get(x - 1, y) - get(x + 1, y) - get(x, y - 1) - get(x, y + 1);
            let l = f64::from(l);
            sum += l;
            sum_sq += l * l;
            n += 1.0;
        }
    }
    let mean = sum / n;
    ((sum_sq / n) - mean * mean).max(0.0) as f32
}

/// Measures a picture.
pub fn analyse(image: &DynamicImage) -> Aids {
    let copy = analysis_copy(image);
    let rgb = copy.to_rgb8();
    let luma = copy.to_luma8();
    let mut histogram = [[0u32; 256]; 4];
    let (mut high, mut low) = (0u64, 0u64);
    for (pixel, l) in rgb.pixels().zip(luma.pixels()) {
        let [r, g, b] = pixel.0;
        histogram[0][usize::from(l.0[0])] += 1;
        histogram[1][usize::from(r)] += 1;
        histogram[2][usize::from(g)] += 1;
        histogram[3][usize::from(b)] += 1;
        if r.max(g).max(b) >= HIGH {
            high += 1;
        }
        if r.max(g).max(b) <= LOW {
            low += 1;
        }
    }
    let pixels = (rgb.width() as u64 * rgb.height() as u64).max(1) as f32;
    Aids {
        sharpness: laplacian_variance(&luma),
        histogram,
        clipped_high: high as f32 / pixels,
        clipped_low: low as f32 / pixels,
    }
}

/// Focus peaking: red where the picture has an edge in focus (a strong luma gradient), transparent elsewhere, at
/// the picture's own size. A soft frame lights up little, a sharp one a lot.
pub fn peaking_mask(image: &DynamicImage) -> RgbaImage {
    let luma = image.to_luma8();
    let (w, h) = luma.dimensions();
    let mut mask = RgbaImage::new(w, h);
    if w < 3 || h < 3 {
        return mask;
    }
    let get = |x: u32, y: u32| i32::from(luma.get_pixel(x, y).0[0]);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            // The second derivative: strong on a crisp edge, weak on a soft ramp (unlike the gradient, which a
            // blurred edge keeps over a wide band), so that only what is in focus lights up.
            let l = 4 * get(x, y) - get(x - 1, y) - get(x + 1, y) - get(x, y - 1) - get(x, y + 1);
            if l.abs() > EDGE {
                mask.put_pixel(x, y, Rgba([255, 40, 40, 220]));
            }
        }
    }
    mask
}

/// Clipping warnings: red where a channel is in the highlights' clip, blue where every channel is in the shadows',
/// transparent elsewhere, at the picture's own size.
pub fn clipping_mask(image: &DynamicImage) -> RgbaImage {
    let rgb = image.to_rgb8();
    let mut mask = RgbaImage::new(rgb.width(), rgb.height());
    for (x, y, pixel) in rgb.enumerate_pixels() {
        let [r, g, b] = pixel.0;
        let top = r.max(g).max(b);
        if top >= HIGH {
            mask.put_pixel(x, y, Rgba([255, 0, 0, 200]));
        } else if top <= LOW {
            mask.put_pixel(x, y, Rgba([0, 90, 255, 200]));
        }
    }
    mask
}

/// A mask as PNG bytes (transparency kept). Compressed for speed: a mask is made when someone waits for it.
pub fn encode_mask(mask: &RgbaImage) -> Result<Vec<u8>> {
    use image::ImageEncoder;
    use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
    let mut bytes = Vec::new();
    PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, PngFilter::Sub)
        .write_image(
            mask.as_raw(),
            mask.width(),
            mask.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| ImagingError::Decode {
            path: std::path::PathBuf::new(),
            message: e.to_string(),
        })?;
    Ok(bytes)
}

/// Which overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaskKind {
    /// Where the picture is in focus.
    Peaking,
    /// Where the picture is clipped.
    Clipping,
}

/// The mask of `kind` for a picture given as JPEG bytes (what the image view shows), as a PNG of the same size.
pub fn mask_png(jpeg: &[u8], kind: MaskKind) -> Result<Vec<u8>> {
    let picture =
        image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg).map_err(|e| {
            ImagingError::Decode {
                path: std::path::PathBuf::new(),
                message: e.to_string(),
            }
        })?;
    encode_mask(&match kind {
        MaskKind::Peaking => peaking_mask(&picture),
        MaskKind::Clipping => clipping_mask(&picture),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma, Rgb};

    /// Vertical black and white stripes, `period` pixels wide.
    fn stripes(w: u32, h: u32, period: u32) -> DynamicImage {
        DynamicImage::ImageLuma8(ImageBuffer::from_fn(w, h, |x, _| {
            Luma([if (x / period).is_multiple_of(2) {
                20
            } else {
                235
            }])
        }))
    }

    fn flat(w: u32, h: u32, value: u8) -> DynamicImage {
        DynamicImage::ImageRgb8(ImageBuffer::from_pixel(w, h, Rgb([value; 3])))
    }

    #[test]
    fn a_sharp_picture_scores_higher_than_the_same_picture_blurred() {
        let sharp = stripes(300, 200, 6);
        let soft = DynamicImage::ImageRgba8(image::imageops::blur(&sharp, 3.0));
        assert!(analyse(&sharp).sharpness > 4.0 * analyse(&soft).sharpness);
        assert_eq!(
            analyse(&flat(64, 48, 128)).sharpness,
            0.0,
            "nothing to see is not sharp"
        );
    }

    #[test]
    fn the_score_does_not_depend_on_the_size_of_the_picture() {
        // The same scene at two sizes above the analysis size gives the same score (both are measured at 1024 px).
        let big = stripes(2048, 1024, 24);
        let bigger = stripes(3072, 1536, 36);
        let (a, b) = (analyse(&big).sharpness, analyse(&bigger).sharpness);
        assert!((a - b).abs() / a.max(b) < 0.15, "{a} and {b}");
    }

    #[test]
    fn the_histogram_counts_every_pixel_once_in_each_channel() {
        let image = DynamicImage::ImageRgb8(ImageBuffer::from_fn(40, 30, |x, _| {
            Rgb([(x * 6) as u8, 100, 255 - (x * 6) as u8])
        }));
        let aids = analyse(&image);
        for channel in &aids.histogram {
            assert_eq!(channel.iter().sum::<u32>(), 40 * 30);
        }
        assert_eq!(aids.histogram[2][100], 40 * 30, "green is one value");
    }

    #[test]
    fn clipping_is_measured_and_shown_where_it_is() {
        let image = DynamicImage::ImageRgb8(ImageBuffer::from_fn(10, 10, |x, _| match x {
            0..=1 => Rgb([255, 255, 255]),
            2..=4 => Rgb([0, 0, 0]),
            _ => Rgb([120, 120, 120]),
        }));
        let aids = analyse(&image);
        assert!((aids.clipped_high - 0.2).abs() < 1e-6);
        assert!((aids.clipped_low - 0.3).abs() < 1e-6);
        let mask = clipping_mask(&image);
        assert_eq!(
            mask.get_pixel(0, 5).0,
            [255, 0, 0, 200],
            "highlights are red"
        );
        assert_eq!(
            mask.get_pixel(3, 5).0,
            [0, 90, 255, 200],
            "shadows are blue"
        );
        assert_eq!(mask.get_pixel(8, 5).0[3], 0, "the rest is clear");
    }

    #[test]
    fn peaking_lights_the_edges_and_nothing_else_and_a_soft_picture_less() {
        let sharp = stripes(120, 60, 20);
        let mask = peaking_mask(&sharp);
        assert_eq!(mask.dimensions(), (120, 60), "at the picture's own size");
        assert!(
            mask.get_pixel(20, 30).0[3] > 0,
            "on the edge between two stripes"
        );
        assert_eq!(mask.get_pixel(10, 30).0[3], 0, "inside a stripe");
        let lit = |m: &RgbaImage| m.pixels().filter(|p| p.0[3] > 0).count();
        let soft = DynamicImage::ImageRgba8(image::imageops::blur(&sharp, 6.0));
        assert!(lit(&peaking_mask(&soft)) < lit(&mask));
        assert_eq!(lit(&peaking_mask(&flat(50, 50, 90))), 0);
    }

    #[test]
    fn a_mask_is_a_png_that_keeps_its_transparency() {
        let mask = clipping_mask(&flat(8, 8, 255));
        let bytes = encode_mask(&mask).unwrap();
        let back = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(back.get_pixel(0, 0).0, [255, 0, 0, 200]);
    }
}
