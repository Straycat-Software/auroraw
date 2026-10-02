// SPDX-License-Identifier: GPL-3.0-or-later
//! Import: profiles, verified copy, RAW+JPEG pairing, GPX matching (architecture §9.2, spec
//! §5.2). Work package WP7.
//!
//! A pure, catalogue-agnostic crate, like `sources` and `imaging` beside it (architecture §3.2):
//! [`pair_files`] and [`plan`](fn@plan) take a plain `Vec` of already-discovered files and return where
//! things should go, with no I/O of their own; [`copy_verified`] is the one place that touches
//! disk, reading a source and writing a destination. Assembling a `PhotoSidecar`, registering the
//! result in the catalogue, resolving keyword paths against the vocabulary, and reading each
//! file's metadata in the first place are `engine`'s job (this crate does not depend on
//! `format`'s sidecar types, `catalogue`, or `imaging`, mirroring how
//! `engine::Coordinator::add_new_photos` already assembles a `PhotoSidecar` itself rather than
//! `sources` doing it).
//!
//! Series detection is explicitly **not** here despite this crate's very first (WP0) doc comment
//! once saying so: the M1 plan's own WP7 paragraph never lists it, and WP9 ("Culling") does
//! (architecture §5.3). What is here: import profiles ([`Profile`], D-029), pairing
//! ([`pair_files`], D-032), planning a destination and a unique name for every file ([`plan`](fn@plan), M1
//! plan §6 item 6), verified copy ([`copy_verified`], design note 004 §6.3 items 4-5), resumable
//! per-job progress ([`ImportState`]), and GPX matching ([`parse_gpx`], [`position_at`]).

mod atomic;
mod copy;
mod discover;
mod error;
mod gpx;
mod pair;
mod plan;
mod profile;
mod state;
mod template;

pub use copy::{CopyOutcome, SourceRead, copy_verified, read_source, write_verified};
pub use discover::DiscoveredFile;
pub use error::{ImportError, Result};
pub use gpx::{GpsFix, Trackpoint, corrected_time, parse as parse_gpx, position_at};
pub use pair::{PhotoGroup, pair_files};
pub use plan::{Exists, PlannedFile, PlannedPhoto, Root, UsedPaths, plan};
pub use profile::{MetadataTemplate, PairRule, Profile};
pub use state::{ImportState, ItemOutcome};
pub use template::{TemplateContext, render as render_template};
