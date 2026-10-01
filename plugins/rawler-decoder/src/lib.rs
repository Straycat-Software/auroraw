// SPDX-License-Identifier: GPL-3.0-or-later
//! The `rawler` RAW decoder, compiled to `wasm32-wasip1` and run through `auroraw-plugin-host`'s
//! sandbox (architecture §8.1, "Import"; spike 4). This plugin never touches the file system,
//! the network, or the clock: the host hands it a file's bytes and reads a decoded image back
//! through the hand-made C interface `auroraw-plugin-api::Decoder` and `auroraw-plugin-host`
//! agree on (architecture §8.2b): `alloc` a buffer, write into it, call, read the result.
//!
//! `import`'s `out` parameter is a 16-byte buffer the host wrote (`alloc(16)`), filled with four
//! little-endian `u32` words: the address and the length of the **block** (the metadata, a
//! versioned block of tagged sections, `auroraw_plugin_api::block`), then the address and the
//! length in bytes of the **samples**, which the block describes (type and count). The block is
//! made by the same crate the host reads it with, so the plugin and the host cannot disagree on it.

use auroraw_plugin_api as api;
use rawler::RawImageData;
use rawler::decoders::RawDecodeParams;
use rawler::rawimage::{RawImage as Decoded, RawPhotometricInterpretation};
use rawler::rawsource::RawSource;

/// Reserves `n` bytes in this instance's own memory and returns their address, so the host never
/// has to guess at an address inside this sandbox.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(n: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(n);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

/// What the last successful [`import`] kept, so its block and samples stay valid for the host to
/// read after the call returns; freed by [`release`].
struct Held {
    block: Vec<u8>,
    samples: Samples,
}

enum Samples {
    None,
    U16(Vec<u16>),
    F32(Vec<f32>),
}

static mut HELD: Held = Held {
    block: Vec::new(),
    samples: Samples::None,
};

/// A black level of `rawler`, as the pattern the block carries. A list whose length is not
/// `width * height * cpp` is not a pattern: it is averaged, so that a decoder's odd file gives a
/// level and not a refusal.
fn black_level(image: &Decoded) -> api::BlackLevel {
    let value = |r: &rawler::formats::tiff::Rational| {
        if r.d == 0 {
            0.0
        } else {
            r.n as f32 / r.d as f32
        }
    };
    let b = &image.blacklevel;
    let (rows, columns, components) = (b.height.max(1), b.width.max(1), b.cpp.max(1));
    if !b.levels.is_empty() && b.levels.len() == rows * columns * components {
        return api::BlackLevel {
            rows: rows as u16,
            columns: columns as u16,
            components: components as u8,
            values: b.levels.iter().map(value).collect(),
        };
    }
    let mean = if b.levels.is_empty() {
        0.0
    } else {
        b.levels.iter().map(value).sum::<f32>() / b.levels.len() as f32
    };
    api::BlackLevel {
        rows: 1,
        columns: 1,
        components: 1,
        values: vec![mean],
    }
}

fn rect(r: &rawler::imgop::Rect) -> api::Rect {
    api::Rect {
        x: r.p.x as u32,
        y: r.p.y as u32,
        width: r.d.w as u32,
        height: r.d.h as u32,
    }
}

/// What `rawler` decoded, as the image the block describes. `None` when it cannot be one (a
/// colour filter of a kind the block has no number for, a value that is not finite): the file is
/// refused, as any file this plugin cannot give a faithful image of.
fn map(image: Decoded, iso: Option<u32>) -> Option<api::RawImage> {
    let layout = match &image.photometric {
        RawPhotometricInterpretation::Cfa(config) => api::SensorLayout::Cfa {
            width: u8::try_from(config.cfa.width).ok()?,
            height: u8::try_from(config.cfa.height).ok()?,
            colours: config.cfa.flat_pattern(),
        },
        RawPhotometricInterpretation::LinearRaw | RawPhotometricInterpretation::BlackIsZero => {
            if image.cpp == 1 {
                api::SensorLayout::Mono
            } else {
                api::SensorLayout::LinearRgb {
                    components: u8::try_from(image.cpp).ok()?,
                    profile: api::InputProfile::Unspecified,
                }
            }
        }
    };
    let levels = api::Levels {
        black: black_level(&image),
        white: if image.whitelevel.0.is_empty() {
            vec![u16::MAX as f32]
        } else {
            image.whitelevel.0.iter().map(|w| *w as f32).collect()
        },
    };
    // `rawler`'s fourth value is NaN on every sample file, and a file without a balance has all NaN or all zero:
    // three finite, positive values or nothing.
    let [r, g, b, _] = image.wb_coeffs;
    let white_balance = (r.is_finite() && g.is_finite() && b.is_finite() && r > 0.0 && g > 0.0 && b > 0.0)
        .then_some([r, g, b]);
    // In the order of the EXIF light source code, so that the block is the same on every run (a HashMap is not).
    let mut colour: Vec<api::ColourMatrix> = image
        .color_matrix
        .iter()
        .filter_map(|(illuminant, values)| {
            let rows = match values.len() {
                9 => 3u8,
                12 => 4u8,
                _ => return None,
            };
            values.iter().all(|v| v.is_finite()).then(|| api::ColourMatrix {
                illuminant: api::Illuminant(*illuminant as u8),
                rows,
                xyz_to_camera: values.clone(),
            })
        })
        .collect();
    colour.sort_by_key(|m| m.illuminant.0);
    let (width, height) = (image.width as u32, image.height as u32);
    let crop = image.crop_area.as_ref().map(rect);
    let active_area = image.active_area.as_ref().map(rect);
    let orientation = api::Orientation::from_exif(image.orientation.to_u16() as u8);
    let camera = api::CameraId {
        make: image.clean_make.clone(),
        model: image.clean_model.clone(),
    };
    let samples = match image.data {
        RawImageData::Integer(samples) => api::Samples::U16(samples),
        RawImageData::Float(samples) => api::Samples::F32(samples),
    };
    Some(api::RawImage {
        width,
        height,
        layout,
        samples,
        levels,
        white_balance,
        colour,
        crop,
        active_area,
        orientation,
        camera,
        iso,
        // `rawler` 0.8.0 does not surface a DNG's NoiseProfile on a decoded image (its `dng_tags` are for its own
        // writer): the tag is defined and tested in the block, and nothing is invented here.
        noise_profile: None,
    })
}

/// The ISO speed the file says, from the EXIF `rawler` reads in a pass of its own over the metadata.
fn iso_of(source: &RawSource, params: &RawDecodeParams) -> Option<u32> {
    let decoder = rawler::get_decoder(source).ok()?;
    let metadata = decoder.raw_metadata(source, params).ok()?;
    metadata
        .exif
        .iso_speed
        .or_else(|| metadata.exif.iso_speed_ratings.map(u32::from))
        .filter(|iso| *iso > 0)
}

/// Decodes the `len` bytes at `ptr` (a whole RAW file, copied in by the host), and writes to the
/// four `u32` slots at `out` the address and the length of the block, then the address and the
/// length in bytes of the samples. Returns `0` on success, `-1` when `rawler` cannot decode the
/// file, `-3` when what it decoded cannot be given as an image of the block (it is refused, never
/// guessed at); no other code is part of this plugin's contract beyond "failure".
///
/// # Safety
///
/// `ptr` must be valid for `len` reads and `out` for 4 writes of `u32`: true of any address this
/// same instance's own [`alloc`] returned, which is the only address the host (`auroraw-plugin-host`)
/// ever passes here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn import(ptr: *const u8, len: u32, out: *mut u32) -> i32 {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let source = RawSource::new_from_slice(bytes);
    let params = RawDecodeParams::default();
    let image = match rawler::decode(&source, &params) {
        Ok(image) => image,
        Err(_) => return -1,
    };
    let iso = iso_of(&source, &params);
    let Some(mut image) = map(image, iso) else {
        return -3;
    };
    let Ok(block) = image.to_block() else {
        return -3;
    };
    // The samples move out of the image without a copy (a 100 MP mosaic is 200 MB).
    let samples = std::mem::replace(&mut image.samples, api::Samples::U16(Vec::new()));
    #[allow(static_mut_refs)]
    unsafe {
        HELD.block = block;
        let (samples_ptr, samples_bytes) = match samples {
            api::Samples::U16(v) => {
                HELD.samples = Samples::U16(v);
                match &HELD.samples {
                    Samples::U16(v) => (v.as_ptr() as u32, v.len() as u32 * 2),
                    _ => unreachable!(),
                }
            }
            api::Samples::F32(v) => {
                HELD.samples = Samples::F32(v);
                match &HELD.samples {
                    Samples::F32(v) => (v.as_ptr() as u32, v.len() as u32 * 4),
                    _ => unreachable!(),
                }
            }
            _ => return -3,
        };
        let words = std::slice::from_raw_parts_mut(out, 4);
        words[0] = HELD.block.as_ptr() as u32;
        words[1] = HELD.block.len() as u32;
        words[2] = samples_ptr;
        words[3] = samples_bytes;
    }
    0
}

/// Frees what [`import`] kept. The host calls this once it has read the block and the samples
/// back; the host must not read them afterwards.
#[unsafe(no_mangle)]
pub extern "C" fn release() {
    #[allow(static_mut_refs)]
    unsafe {
        HELD = Held {
            block: Vec::new(),
            samples: Samples::None,
        };
    }
}
