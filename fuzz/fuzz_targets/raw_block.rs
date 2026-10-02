// SPDX-License-Identifier: GPL-3.0-or-later
#![no_main]
//! The block that carries a decoder's metadata across the sandbox (plugin-api, D-141) on arbitrary bytes: the reader
//! must never panic, hang or grow without bound, and an image it accepts must be written and read back the same.
//! The corpus is seeded with the block of each sample file (written by the decoder tests of `plugin-host` under
//! `AUR_UPDATE_GOLDEN=1`), so that the fuzzer starts from the shapes that exist.

use auroraw_plugin_api::{RawImage, SampleKind, Samples};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok((kind, count)) = RawImage::block_samples(data) else {
        return;
    };
    // The samples travel by their own path; give the block the ones it describes, unless it describes more than a
    // test should allocate: the sections are parsed and checked first either way (a block from a real file describes
    // millions of samples, and its metadata is what the seeds are for), and only the match with the samples comes
    // after.
    let samples = if count <= 1 << 16 {
        match kind {
            SampleKind::U16 => Samples::U16(vec![0; count as usize]),
            SampleKind::F32 => Samples::F32(vec![0.0; count as usize]),
        }
    } else {
        Samples::U16(Vec::new())
    };
    if let Ok(image) = RawImage::from_block(data, samples) {
        let block = image.to_block().expect("what was read can be written");
        let back = RawImage::from_block(&block, image.samples.clone())
            .expect("what is written can be read");
        assert_eq!(back, image, "the image survives its block");
    }
});
