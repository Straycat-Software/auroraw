// SPDX-License-Identifier: GPL-3.0-or-later
//! A perceptual hash for visual similarity (spec §5.3, WP9's similar-photo suggestions, D-105): it
//! falls out of a thumbnail almost for free. This crate only makes it; the engine decides what
//! "close" means and what a person is offered because of it.

use image::{DynamicImage, ImageFormat};

use crate::Thumbnail;
use image::imageops::FilterType;

/// A 64-bit difference hash: shrink to 9x8 grayscale and record, for each of the 8x8 pixels,
/// whether it is brighter than its right neighbour. Two photos of the same scene hash a short
/// Hamming distance apart even after a recompress or a small crop; two different scenes usually
/// do not. A simple, well-known, and cheap algorithm — accuracy tuning is WP9's job once it has
/// a real photo library to tune against.
pub fn perceptual_hash(image: &DynamicImage) -> u64 {
    let small = image.resize_exact(9, 8, FilterType::Triangle).to_luma8();
    let mut hash = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            let left = small.get_pixel(x, y).0[0];
            let right = small.get_pixel(x + 1, y).0[0];
            hash = (hash << 1) | u64::from(left > right);
        }
    }
    hash
}

/// The hash of a photo as its thumbnail shows it (D-105): the stored 256 px JPEG, already upright, decoded (about a
/// millisecond) and hashed. The same hash whether the thumbnail was just made or comes from the cache, and one that
/// does not depend on the file's orientation flag. `None` when the JPEG cannot be decoded.
pub fn thumbnail_hash(thumbnail: &Thumbnail) -> Option<u64> {
    let image = image::load_from_memory_with_format(&thumbnail.jpeg, ImageFormat::Jpeg).ok()?;
    Some(perceptual_hash(&image))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    fn gradient(w: u32, h: u32, reverse: bool) -> DynamicImage {
        let buf = ImageBuffer::from_fn(w, h, |x, _| {
            let v = (x * 255 / w.max(1)) as u8;
            Rgb([if reverse { 255 - v } else { v }; 3])
        });
        DynamicImage::ImageRgb8(buf)
    }

    fn flat(w: u32, h: u32, value: u8) -> DynamicImage {
        DynamicImage::ImageRgb8(ImageBuffer::from_pixel(w, h, Rgb([value; 3])))
    }

    #[test]
    fn the_same_image_always_hashes_the_same() {
        let image = gradient(64, 48, false);
        assert_eq!(perceptual_hash(&image), perceptual_hash(&image));
    }

    #[test]
    fn a_flat_image_has_no_bit_set() {
        // Every neighbour comparison is equal, never "brighter than", so every bit is 0.
        assert_eq!(perceptual_hash(&flat(64, 48, 128)), 0);
    }

    #[test]
    fn very_different_images_hash_far_apart() {
        let a = gradient(64, 48, false);
        let b = gradient(64, 48, true);
        let distance = (perceptual_hash(&a) ^ perceptual_hash(&b)).count_ones();
        assert!(
            distance > 32,
            "expected a large Hamming distance, got {distance}"
        );
    }

    fn scene(seed: u32, w: u32, h: u32) -> DynamicImage {
        // A blocky "scene": each 8x8 block a pseudo-random grey, so that two seeds are two unrelated pictures.
        let mut state = seed.wrapping_mul(2654435761).wrapping_add(12345);
        let mut blocks = Vec::new();
        for _ in 0..64 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            blocks.push((state >> 24) as u8);
        }
        DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |x, y| {
            let v = blocks[((y * 8 / h) * 8 + (x * 8 / w)) as usize];
            Rgb([v; 3])
        }))
    }

    fn thumbnail_of(image: &DynamicImage) -> Thumbnail {
        let mut jpeg = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut jpeg), ImageFormat::Jpeg)
            .unwrap();
        Thumbnail {
            jpeg,
            width: image.width(),
            height: image.height(),
        }
    }

    #[test]
    fn a_thumbnails_hash_is_close_to_the_same_scene_brightened_and_far_from_another() {
        let a = scene(1, 256, 192);
        let brighter = DynamicImage::ImageRgb8(ImageBuffer::from_fn(256, 192, |x, y| {
            let p = a.to_rgb8().get_pixel(x, y).0[0];
            Rgb([p.saturating_add(12); 3])
        }));
        let other = scene(2, 256, 192);
        let (ha, hb, ho) = (
            thumbnail_hash(&thumbnail_of(&a)).unwrap(),
            thumbnail_hash(&thumbnail_of(&brighter)).unwrap(),
            thumbnail_hash(&thumbnail_of(&other)).unwrap(),
        );
        assert!(
            (ha ^ hb).count_ones() <= 6,
            "the same scene: {}",
            (ha ^ hb).count_ones()
        );
        assert!(
            (ha ^ ho).count_ones() >= 16,
            "another scene: {}",
            (ha ^ ho).count_ones()
        );
    }

    #[test]
    fn a_broken_jpeg_has_no_hash() {
        let broken = Thumbnail {
            jpeg: vec![1, 2, 3],
            width: 1,
            height: 1,
        };
        assert_eq!(thumbnail_hash(&broken), None);
    }

    #[test]
    #[ignore = "a measurement: run on demand with --release --nocapture"]
    fn what_hashing_a_256_pixel_thumbnail_costs() {
        let thumbnail = thumbnail_of(&scene(3, 256, 192));
        let started = std::time::Instant::now();
        let runs = 2_000;
        for _ in 0..runs {
            std::hint::black_box(thumbnail_hash(&thumbnail));
        }
        println!(
            "thumbnail_hash: {:.1} us each (JPEG decode and dHash of a {}x{} thumbnail)",
            started.elapsed().as_secs_f64() * 1e6 / f64::from(runs),
            thumbnail.width,
            thumbnail.height
        );
    }
}
