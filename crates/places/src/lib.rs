// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline place names: the country, the region and the town of a position (design note 008, D-147).
//!
//! A read-only SQLite file holds the polygons of the countries and the regions (Natural Earth) and the
//! towns (GeoNames). [`Places::locate`] answers *which country and region contain this point* from the
//! polygons, with a coastal tolerance for a beach or a ferry, and *which town is nearest* among the towns
//! of the same region, so that a point just inside a region is not given the town across the border. The
//! file is written by [`PackBuilder`], which `examples/build-places.rs` drives from the published data
//! ([`sources`]); the application never downloads anything.
//!
//! The crate depends on nothing of the application: it is tested alone, on a small file the tests write.

mod error;
mod geometry;
mod locate;
mod pack;
pub mod sources;

pub use error::{PlacesError, Result};
pub use locate::{
    COASTAL_TOLERANCE_M, Info, Located, Named, Options, Places, TOWN_RADIUS_M, TOWN_REACH_M, Town,
};
pub use pack::{FORMAT, Level, NewArea, NewPlace, PackBuilder, Polygon, Summary};
