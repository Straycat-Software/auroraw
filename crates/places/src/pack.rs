// SPDX-License-Identifier: GPL-3.0-or-later
//! The file the lookup reads, and the builder that writes it (design note 008 §2).
//!
//! ```text
//! areas(id, level, code, country_code, parent, name, geonames_id)   -- level 0: a country, 1: a region
//! area_names(area, lang, name)
//! parts(id, area, level, rings)    -- one polygon each, its rings encoded (`geometry::encode`)
//! part_boxes                       -- an R*Tree of the boxes of the parts
//! places(id, name, lat, lon, country_code, country_area, region_area, population, section)
//!                                  -- indexed by (region, latitude) and (country, latitude)
//! meta(key, value)                 -- the format, where the data came from, under which licence
//! ```

use std::path::Path;

use rusqlite::{Connection, params};

use crate::error::{PlacesError, Result};
use crate::geometry;
use crate::locate::Lookup;

/// The version of the file's layout. A newer file is refused.
pub const FORMAT: u32 = 1;

pub(crate) const SCHEMA: &str = "
CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
CREATE TABLE areas(
  id INTEGER PRIMARY KEY, level INTEGER NOT NULL, code TEXT NOT NULL, country_code TEXT NOT NULL,
  parent INTEGER, name TEXT NOT NULL, geonames_id INTEGER);
CREATE TABLE area_names(
  area INTEGER NOT NULL, lang TEXT NOT NULL, name TEXT NOT NULL, PRIMARY KEY(area, lang)) WITHOUT ROWID;
CREATE TABLE parts(id INTEGER PRIMARY KEY, area INTEGER NOT NULL, level INTEGER NOT NULL, rings BLOB NOT NULL);
CREATE VIRTUAL TABLE part_boxes USING rtree(id, min_lon, max_lon, min_lat, max_lat);
CREATE TABLE places(
  id INTEGER PRIMARY KEY, name TEXT NOT NULL, lat REAL NOT NULL, lon REAL NOT NULL,
  country_code TEXT NOT NULL, country_area INTEGER, region_area INTEGER, population INTEGER NOT NULL,
  section INTEGER NOT NULL DEFAULT 0);
";

/// What an area is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// A country.
    Country = 0,
    /// A region: a state, a province, a first-level subdivision.
    Region = 1,
}

/// One polygon: a list of rings (the first the outer edge, the others holes), each a list of
/// `(longitude, latitude)` points in degrees.
pub type Polygon = Vec<Vec<(f64, f64)>>;

/// An area to put in the file: its names and its polygons.
#[derive(Debug, Clone)]
pub struct NewArea {
    /// Country or region.
    pub level: Level,
    /// Its own code: the ISO 3166-1 alpha-2 of a country, the ISO 3166-2 of a region; empty if it has none.
    pub code: String,
    /// The ISO 3166-1 alpha-2 code of the country it is in (its own, for a country); empty if none.
    pub country_code: String,
    /// The country a region is in, as the identifier `add_area` returned for it.
    pub parent: Option<i64>,
    /// Its name in the place's own language.
    pub name: String,
    /// Its names by language code (`en`, `fr`...).
    pub names: Vec<(String, String)>,
    /// GeoNames' identifier for it, if known.
    pub geonames_id: Option<u64>,
    /// Its polygons.
    pub parts: Vec<Polygon>,
}

/// A town to put in the file.
#[derive(Debug, Clone)]
pub struct NewPlace {
    /// Its identifier (GeoNames').
    pub id: i64,
    /// Its name in its own spelling.
    pub name: String,
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lon: f64,
    /// The ISO 3166-1 alpha-2 code of its country.
    pub country_code: String,
    /// Its population.
    pub population: u64,
    /// A section of a populated place (a neighbourhood, GeoNames' `PPLX`), which is not what a person
    /// calls the city when there is a city near.
    pub section: bool,
}

/// What a finished file holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// Countries and regions.
    pub areas: usize,
    /// Polygons.
    pub parts: usize,
    /// Points of all the polygons.
    pub points: usize,
    /// Towns.
    pub places: usize,
    /// Towns that no region's polygon contains (they are found by their country).
    pub places_without_region: usize,
    /// Towns that belong to no country at all.
    pub places_without_country: usize,
}

/// Writes a file of places. The areas are added first (a region after its country), then the towns;
/// `finish` decides which region and country each town is in, and compacts the file.
pub struct PackBuilder {
    conn: Connection,
    parts: usize,
    points: usize,
    areas: usize,
    places: usize,
}

impl PackBuilder {
    /// Starts a file at `path`, which must not exist.
    pub fn create(path: &Path) -> Result<PackBuilder> {
        if path.exists() {
            return Err(PlacesError::Exists(path.to_path_buf()));
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF;")?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch("BEGIN")?;
        conn.execute(
            "INSERT INTO meta(key, value) VALUES ('format', ?1)",
            [FORMAT.to_string()],
        )?;
        Ok(PackBuilder {
            conn,
            parts: 0,
            points: 0,
            areas: 0,
            places: 0,
        })
    }

    /// Records where the data came from and under which licence, or anything else worth keeping.
    pub fn set_meta(&mut self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Adds an area and its polygons; returns its identifier.
    pub fn add_area(&mut self, area: &NewArea) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO areas(level, code, country_code, parent, name, geonames_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                area.level as i64,
                area.code,
                area.country_code,
                area.parent,
                area.name,
                area.geonames_id.map(|id| id as i64)
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        for (lang, name) in &area.names {
            if !name.is_empty() {
                self.conn.execute(
                    "INSERT OR REPLACE INTO area_names(area, lang, name) VALUES (?1, ?2, ?3)",
                    params![id, lang, name],
                )?;
            }
        }
        for rings in &area.parts {
            let (min_lon, max_lon, min_lat, max_lat) = geometry::bounding_box(rings);
            if min_lon > max_lon {
                continue; // a part with no points
            }
            self.conn.execute(
                "INSERT INTO parts(area, level, rings) VALUES (?1, ?2, ?3)",
                params![id, area.level as i64, geometry::encode(rings)],
            )?;
            let part = self.conn.last_insert_rowid();
            self.conn.execute(
                "INSERT INTO part_boxes(id, min_lon, max_lon, min_lat, max_lat) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![part, min_lon, max_lon, min_lat, max_lat],
            )?;
            self.parts += 1;
            self.points += rings.iter().map(Vec::len).sum::<usize>();
        }
        self.areas += 1;
        Ok(id)
    }

    /// Adds a town.
    pub fn add_place(&mut self, place: &NewPlace) -> Result<()> {
        self.conn
            .prepare_cached(
                "INSERT INTO places(id, name, lat, lon, country_code, population, section) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?
            .execute(params![
                place.id,
                place.name,
                place.lat,
                place.lon,
                place.country_code,
                place.population as i64,
                place.section
            ])?;
        self.places += 1;
        Ok(())
    }

    /// Decides the region and the country of every town, writes the counts, and compacts the file.
    pub fn finish(self) -> Result<Summary> {
        let lookup = Lookup::unbounded();
        let towns: Vec<(i64, f64, f64, String)> = {
            let mut statement = self
                .conn
                .prepare("SELECT id, lat, lon, country_code FROM places")?;
            statement
                .query_map([], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })?
                .collect::<std::result::Result<_, _>>()?
        };
        let (mut without_region, mut without_country) = (0, 0);
        for (id, lat, lon, country_code) in towns {
            let region = lookup.area_at(&self.conn, Level::Region, lon, lat, 0.0)?;
            let mut country = match region {
                Some(region) => lookup.parent_of(&self.conn, region)?,
                None => None,
            };
            if country.is_none() {
                country = lookup.area_at(&self.conn, Level::Country, lon, lat, 0.0)?;
            }
            if country.is_none() && !country_code.is_empty() {
                // On an island the polygons do not show, or a shore they cut: the code GeoNames gave.
                country = self
                    .conn
                    .query_row(
                        "SELECT id FROM areas WHERE level = 0 AND code = ?1",
                        [&country_code],
                        |row| row.get(0),
                    )
                    .ok();
            }
            without_region += usize::from(region.is_none());
            without_country += usize::from(country.is_none());
            self.conn.execute(
                "UPDATE places SET country_area = ?2, region_area = ?3 WHERE id = ?1",
                params![id, country, region],
            )?;
        }
        self.conn.execute_batch(
            "CREATE INDEX places_by_region ON places(region_area, lat); \
             CREATE INDEX places_by_country ON places(country_area, lat); \
             CREATE INDEX parts_by_area ON parts(area);",
        )?;
        self.conn.execute_batch("COMMIT")?;
        self.conn.execute_batch("VACUUM")?;
        Ok(Summary {
            areas: self.areas,
            parts: self.parts,
            points: self.points,
            places: self.places,
            places_without_region: without_region,
            places_without_country: without_country,
        })
    }
}
