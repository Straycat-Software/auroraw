// SPDX-License-Identifier: GPL-3.0-or-later
//! The image engine: develops RAW images on the GPU with wgpu (architecture §6; design note 005).
//!
//! It is **headless**: it depends on neither the catalogue nor the interface, so it is tested
//! alone and `cargo test -p auroraw-pipeline` needs no Qt (architecture §3.2, rule 3). One thread
//! owns the graphics device, the queue and, later, the stage caches and the memory budget; the rest
//! of the application talks to it by messages.
//!
//! What exists so far is the foundation the stages stand on, and the first stages:
//!
//! - choosing an adapter, with a typed error when there is none ([`adapter`], [`AdapterChoice`]);
//! - the GPU thread, which re-creates a lost device before the next job ([`Pipeline`]);
//! - allocation and read-back that report running out of memory as an error, not a panic;
//! - the smoke test that compiles every shader and checks it against a CPU reference
//!   ([`Pipeline::smoke_test`]).
//!
//! - the first stages of the chain (design note 006 §3.3): the levels, the Bayer demosaic, the
//!   camera-to-working step with the white balance folded in, the exposure and the output transform
//!   (the flat linear look), each with a CPU reference it is checked against.
//!
//! The render API (recipes, views, bands, caches, reports) comes next, following design note 005.

pub mod adapter;
mod colour;
mod gpu;
mod reference;
mod scenes;
mod smoke;
mod stages;
mod thread;

pub use adapter::{
    AdapterChoice, AdapterInfo, AdapterKind, Backend, ChooseError, choose, choose_for,
    list_adapters,
};
pub use gpu::EngineLimits;
pub use smoke::{Measured, ShaderReport, SmokeError, SmokeReport, Tolerance};
pub use thread::{Config, OpenError, Pipeline};

#[cfg(test)]
mod stage_tests;
#[cfg(test)]
mod tests;
