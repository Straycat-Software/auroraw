// SPDX-License-Identifier: GPL-3.0-or-later
//! A [`Decoder`] backed by a sandboxed WebAssembly plugin: `rawler-decoder`, the productized
//! `rawler` decoder (architecture §8.1, "the first real WebAssembly plugin"), and the wire
//! protocol `plugins/rawler-decoder` implements on the guest side of the same C interface
//! [`crate::plugin`] speaks on the host side.
//!
//! `import` fills a 16-byte buffer with four little-endian `u32` words: the address and the length of
//! the **block** (the image's metadata, `auroraw_plugin_api::block`), and the address and the length
//! in bytes of the **samples**, which the block describes (type and count). The host reads the block,
//! asks it what the samples are, reads them in that type, and builds the image from both.

use crate::grants::Grants;
use crate::plugin::{CompiledPlugin, PluginHost};
use auroraw_plugin_api::block::MAX_BLOCK_LEN;
use auroraw_plugin_api::{Decoder, DecoderError, RawImage, Samples};
use std::sync::Arc;
use std::time::Duration;

/// The `out` buffer's size: four little-endian `u32` values (the block's address and length, the
/// samples' address and length in bytes), matching `plugins/rawler-decoder`'s own doc comment.
const OUT_SIZE: usize = 16;

/// A memory ceiling generous enough for the largest embedded mosaic seen so far (spike 4: up to
/// 347 MB of guest memory for a 60 MP file) with real headroom, since a decode that hits the
/// ceiling fails that file rather than the host.
const DEFAULT_MEMORY_LIMIT: usize = 512 << 20;

/// How long a single decode may run before the host interrupts it. Generous: a slow decode is
/// still a real one (spike 4's own slowest sample took over a second natively), and this guards
/// against a hang, not against a merely slow file.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// [`Decoder`] implemented by `plugins/rawler-decoder` running under [`PluginHost`]. A fresh
/// instance is started for every call (spike 4's own measurement shape: instantiating from an
/// already-compiled module costs 0.07 ms), so calling [`Decoder::decode`] from several threads at
/// once decodes several files in parallel, each in its own sandbox.
pub struct WasmDecoder {
    host: Arc<PluginHost>,
    plugin: CompiledPlugin,
}

impl WasmDecoder {
    /// Compiles `wasm` (the bytes of `rawler-decoder`'s `.wasm` file) against `host`.
    pub fn new(host: Arc<PluginHost>, wasm: &[u8]) -> crate::error::Result<Self> {
        let plugin = host.load(wasm)?;
        Ok(Self { host, plugin })
    }
}

impl Decoder for WasmDecoder {
    fn decode(&self, bytes: &[u8]) -> Result<RawImage, DecoderError> {
        let grants = Grants {
            memory_limit: Some(DEFAULT_MEMORY_LIMIT),
            ..Grants::default()
        };
        let mut instance = self
            .host
            .instantiate(&self.plugin, &grants)
            .map_err(|e| DecoderError::Failed(e.to_string()))?;

        let at = instance
            .alloc(bytes.len())
            .map_err(|e| DecoderError::Failed(e.to_string()))?;
        instance
            .write(at, bytes)
            .map_err(|e| DecoderError::Failed(e.to_string()))?;
        let out_at = instance
            .alloc(OUT_SIZE)
            .map_err(|e| DecoderError::Failed(e.to_string()))?;

        let rc: i32 = instance
            .call("import", (at, bytes.len() as u32, out_at), DEFAULT_TIMEOUT)
            .map_err(|e| DecoderError::Failed(e.to_string()))?;
        if rc != 0 {
            return Err(DecoderError::Invalid(format!(
                "the decoder plugin refused this file (code {rc})"
            )));
        }

        let mut out = [0u8; OUT_SIZE];
        instance
            .read(out_at, &mut out)
            .map_err(|e| DecoderError::Failed(e.to_string()))?;
        let word =
            |i: usize| u32::from_le_bytes(out[i * 4..i * 4 + 4].try_into().expect("4 bytes"));
        let (block_ptr, block_len, samples_ptr, samples_len) =
            (word(0), word(1) as usize, word(2), word(3) as usize);

        // The block is the plugin's word about the image, and the plugin is not trusted: a length past the cap is
        // refused before anything is allocated for it.
        if block_len > MAX_BLOCK_LEN {
            return Err(DecoderError::Failed(format!(
                "the decoder plugin's block is {block_len} bytes, more than {MAX_BLOCK_LEN}"
            )));
        }
        let mut block = vec![0u8; block_len];
        instance
            .read(block_ptr, &mut block)
            .map_err(|e| DecoderError::Failed(e.to_string()))?;
        let malformed = |e: auroraw_plugin_api::BlockError| {
            DecoderError::Failed(format!("the decoder plugin's block is malformed: {e}"))
        };
        let (kind, count) = RawImage::block_samples(&block).map_err(malformed)?;
        // The samples the block describes, and no more: a plugin that says one thing and holds another is refused.
        if count.checked_mul(kind.size() as u64) != Some(samples_len as u64) {
            return Err(DecoderError::Failed(format!(
                "the decoder plugin's block describes {count} samples of {} bytes and it holds {samples_len} bytes",
                kind.size()
            )));
        }
        // (The plugin's memory is capped, and a length beyond the cap cannot be real: nothing is allocated for it.)
        if samples_len > DEFAULT_MEMORY_LIMIT {
            return Err(DecoderError::Failed(format!(
                "the decoder plugin says it holds {samples_len} bytes of samples, more than its memory"
            )));
        }
        let mut raw = vec![0u8; samples_len];
        instance
            .read(samples_ptr, &mut raw)
            .map_err(|e| DecoderError::Failed(e.to_string()))?;
        let samples = Samples::from_le_bytes(kind, &raw).map_err(malformed)?;
        drop(raw);

        // The instance and its memory are dropped here: no explicit `release` call needed, since
        // nothing outlives this function that still points into the plugin's memory (unlike the
        // spike's own long-lived benchmark instances, which called it to reuse one instance for
        // many files).
        RawImage::from_block(&block, samples).map_err(malformed)
    }
}
