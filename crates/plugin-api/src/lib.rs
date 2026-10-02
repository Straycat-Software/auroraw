// SPDX-License-Identifier: MIT OR Apache-2.0
//! The plugin API: the declaration schema and the interfaces between Auroraw and its plugins.
//!
//! This crate is licensed MIT OR Apache-2.0 so that a plugin, free or not, can include it
//! (decision D-080). It therefore depends on no crate of the application. The [`source`]
//! interface arrived first (WP4), since the core's own local-folder and removable-volume sources
//! need it before anything else here exists, and still runs as native code (architecture §8.6);
//! WP6 adds the declaration schema, permissions and [`decoder`], and productizes the wasmtime
//! host (`plugin-host`) that loads the first real sandboxed plugin. Experimental until milestone
//! M5 (architecture §8.2b): everything in this crate can change without a deprecation period.

pub mod declaration;
pub mod decoder;
pub mod operation;
pub mod permissions;
pub mod source;
pub mod spaces;
pub mod stages;

pub use declaration::{Declaration, DeclarationError, Family, HOST_API_VERSION, Placement};
pub use decoder::{Decoder, DecoderError, RawImage};
pub use operation::{
    CostClass, MAX_KEY_LEN, OperationId, ParamError, ParamKind, ParamSpec, ParamValue,
};
pub use permissions::Permissions;
