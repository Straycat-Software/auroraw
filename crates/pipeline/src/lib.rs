// SPDX-License-Identifier: GPL-3.0-or-later
//! The image engine: develops RAW images on the GPU with wgpu (architecture §6; design note 005).
//!
//! It is **headless**: it depends on neither the catalogue nor the interface, so it is tested
//! alone and `cargo test -p auroraw-pipeline` needs no Qt (architecture §3.2, rule 3). One thread
//! owns the graphics device, the queue and, later, the stage caches and the memory budget; the rest
//! of the application talks to it by messages.
//!
//! What exists so far is the foundation the stages will stand on:
//!
//! - choosing an adapter, with a typed error when there is none ([`adapter`], [`AdapterChoice`]);
//! - the GPU thread, which re-creates a lost device before the next job ([`Pipeline`]);
//! - allocation and read-back that report running out of memory as an error, not a panic;
//! - the smoke test that compiles every shader and checks it against a CPU reference
//!   ([`Pipeline::smoke_test`]).
//!
//! The render API (recipes, views, reports) comes next, following design note 005.

pub mod adapter;
mod gpu;
mod smoke;
mod thread;

pub use adapter::{
    AdapterChoice, AdapterInfo, AdapterKind, Backend, ChooseError, choose, list_adapters,
};
pub use smoke::{
    MAX_FRACTION_OVER_ONE_LEVEL, MAX_LEVEL_DIFFERENCE, ShaderReport, SmokeError, SmokeReport,
};
pub use thread::{Config, OpenError, Pipeline};

#[cfg(test)]
mod tests;
