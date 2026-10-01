// SPDX-License-Identifier: GPL-3.0-or-later
//! The lookup (design note 008 §3): the polygon that contains the point gives the region and the
//! country, a coastal tolerance covers a beach or a generalised shore, and the nearest town of the same
//! region gives the city.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::error::{PlacesError, Result};
use crate::geometry::{self, Ring};
use crate::pack::{FORMAT, Level};

/// How far from a polygon a point still counts as in it: a beach, a ferry in a harbour, a shoreline that
/// 1:10 million generalisation has moved. Beyond it the point is in open water and has no country.
pub const COASTAL_TOLERANCE_M: f64 = 5_000.0;

/// How far from the point the nearest town may be to be its city.
pub const TOWN_RADIUS_M: f64 = 25_000.0;

/// The nearest town is not always the town a person means. Downtown Montréal is 400 m from the borough of
/// Ville-Marie and 1.7 km from Montréal's own point; Copacabana is a neighbourhood of Rio de Janeiro, whose
/// point is 6.5 km away. A town **claims** the points within this many metres for each square root of its
/// inhabitants (10 m x sqrt(1.7 million) is 13 km for Montréal, 26 km for Rio, 1.2 km for a town of 15,000,
/// 350 m for a village of 1,200: the radius of a city of that size, roughly), and a point claimed by several
/// goes to the biggest; a point nobody claims goes to the nearest.
pub const TOWN_REACH_M: f64 = 10.0;

/// How many points of decoded polygons a lookup keeps, to spare decoding Canada for every photo of a
/// day in Quebec. About 16 MB of rings.
const CACHE_POINTS: usize = 2_000_000;

/// The distances a lookup uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// See [`COASTAL_TOLERANCE_M`].
    pub coastal_tolerance_m: f64,
    /// See [`TOWN_RADIUS_M`].
    pub town_radius_m: f64,
    /// See [`TOWN_REACH_M`].
    pub town_reach_m: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            coastal_tolerance_m: COASTAL_TOLERANCE_M,
            town_radius_m: TOWN_RADIUS_M,
            town_reach_m: TOWN_REACH_M,
        }
    }
}

/// A country or a region, named for a language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    /// Its identifier in the file.
    pub id: i64,
    /// Its code: the ISO 3166-1 alpha-2 of a country, the ISO 3166-2 of a region; empty if it has none.
    pub code: String,
    /// Its name in the language asked for (English when the file does not have it, its own otherwise).
    pub name: String,
}

/// The town nearest to a position, in its own spelling.
#[derive(Debug, Clone, PartialEq)]
pub struct Town {
    /// Its identifier (GeoNames').
    pub id: i64,
    /// Its name.
    pub name: String,
    /// How far it is from the position, in metres.
    pub distance_m: f64,
}

/// Where a position is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Located {
    /// The country, if the position is in one (or close enough to its shore).
    pub country: Option<Named>,
    /// The region, if the position is in one of a country that has regions.
    pub region: Option<Named>,
    /// The nearest town of the same region within the radius.
    pub city: Option<Town>,
}

impl Located {
    /// Whether nothing was found: the open sea, or a pole the polygons do not reach.
    pub fn is_empty(&self) -> bool {
        self.country.is_none() && self.region.is_none() && self.city.is_none()
    }
}

/// What a file of places says of itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    /// The layout of the file.
    pub format: u32,
    /// Countries and regions.
    pub areas: usize,
    /// Towns.
    pub places: usize,
    /// What the builder recorded: the sources, their dates and licences.
    pub meta: Vec<(String, String)>,
}

/// Decoded polygons, kept while they are being asked for.
struct Cache {
    rings: HashMap<i64, Arc<Vec<Ring>>>,
    points: usize,
    limit: usize,
}

/// The state of a lookup: the cache of polygons.
pub(crate) struct Lookup {
    cache: Mutex<Cache>,
}

impl Lookup {
    fn new(limit: usize) -> Lookup {
        Lookup {
            cache: Mutex::new(Cache {
                rings: HashMap::new(),
                points: 0,
                limit,
            }),
        }
    }

    /// A lookup that keeps every polygon it decodes (the builder's, which asks for each town once).
    pub(crate) fn unbounded() -> Lookup {
        Lookup::new(usize::MAX)
    }

    fn rings(&self, conn: &Connection, part: i64) -> Result<Arc<Vec<Ring>>> {
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = cache.rings.get(&part) {
            return Ok(Arc::clone(found));
        }
        let bytes: Vec<u8> =
            conn.query_row("SELECT rings FROM parts WHERE id = ?1", [part], |row| {
                row.get(0)
            })?;
        let rings = Arc::new(geometry::decode(&bytes).ok_or(PlacesError::Damaged(part))?);
        let points = rings.iter().map(Vec::len).sum::<usize>();
        if cache.points + points > cache.limit {
            cache.rings.clear();
            cache.points = 0;
        }
        cache.points += points;
        cache.rings.insert(part, Arc::clone(&rings));
        Ok(rings)
    }

    /// The parts of this level whose box meets `[lon_low, lon_high] x [lat_low, lat_high]`: part,
    /// area, and the area of the box (to prefer the smallest of several). With `parent`, only the areas
    /// that are in that country (the regions of one country).
    fn candidates(
        conn: &Connection,
        level: Level,
        lon: (f64, f64),
        lat: (f64, f64),
        parent: Option<i64>,
    ) -> Result<Vec<(i64, i64, f64)>> {
        let mut statement = conn.prepare_cached(
            "SELECT b.id, p.area, (b.max_lon - b.min_lon) * (b.max_lat - b.min_lat) \
             FROM part_boxes b JOIN parts p ON p.id = b.id JOIN areas a ON a.id = p.area \
             WHERE p.level = ?1 AND b.max_lon >= ?2 AND b.min_lon <= ?3 AND b.max_lat >= ?4 AND b.min_lat <= ?5 \
             AND (?6 IS NULL OR a.parent = ?6)",
        )?;
        let rows = statement.query_map(
            params![level as i64, lon.0, lon.1, lat.0, lat.1, parent],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The area of this level that contains the point, or, with a tolerance, the nearest one within it. With
    /// `parent`, among the areas of that country only.
    pub(crate) fn area_at(
        &self,
        conn: &Connection,
        level: Level,
        lon: f64,
        lat: f64,
        tolerance_m: f64,
        parent: Option<i64>,
    ) -> Result<Option<i64>> {
        let mut inside: Option<(f64, i64)> = None;
        for (part, area, size) in Self::candidates(conn, level, (lon, lon), (lat, lat), parent)? {
            if inside.is_some_and(|(best, _)| best <= size) {
                continue;
            }
            if geometry::contains(&self.rings(conn, part)?, lon, lat) {
                inside = Some((size, area));
            }
        }
        if let Some((_, area)) = inside {
            return Ok(Some(area));
        }
        if tolerance_m <= 0.0 {
            return Ok(None);
        }
        let dlat = geometry::degrees_of_latitude(tolerance_m);
        let dlon = geometry::degrees_of_longitude(tolerance_m, lat);
        let mut nearest: Option<(f64, i64)> = None;
        for range in geometry::longitude_ranges(lon, dlon) {
            for (part, area, _) in
                Self::candidates(conn, level, range, (lat - dlat, lat + dlat), parent)?
            {
                let distance = geometry::distance_m(&self.rings(conn, part)?, lon, lat);
                if distance <= tolerance_m && nearest.is_none_or(|(best, _)| distance < best) {
                    nearest = Some((distance, area));
                }
            }
        }
        Ok(nearest.map(|(_, area)| area))
    }

    /// The country and the region a point is in, **the country decided first**: the country that contains
    /// the point, then the region of that country that contains it or, failing that, is nearest within the
    /// tolerance. Deciding the region first would give a point in a country with no regions of its own the
    /// neighbour's country, whenever a region of the neighbour is within the tolerance (review of the
    /// places crate). Only a point in **no country** (a coast, a ferry, an island the polygons do not show)
    /// uses the tolerance to find one: through the nearest region, else the nearest country.
    pub(crate) fn country_and_region(
        &self,
        conn: &Connection,
        lon: f64,
        lat: f64,
        tolerance_m: f64,
    ) -> Result<(Option<i64>, Option<i64>)> {
        if let Some(country) = self.area_at(conn, Level::Country, lon, lat, 0.0, None)? {
            let region = self.area_at(conn, Level::Region, lon, lat, tolerance_m, Some(country))?;
            return Ok((Some(country), region));
        }
        let region = self.area_at(conn, Level::Region, lon, lat, tolerance_m, None)?;
        if let Some(country) = match region {
            Some(region) => self.parent_of(conn, region)?,
            None => None,
        } {
            return Ok((Some(country), region));
        }
        let country = self.area_at(conn, Level::Country, lon, lat, tolerance_m, None)?;
        Ok((country, None))
    }

    /// The country a region is in.
    pub(crate) fn parent_of(&self, conn: &Connection, area: i64) -> Result<Option<i64>> {
        Ok(conn
            .query_row("SELECT parent FROM areas WHERE id = ?1", [area], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .optional()?
            .flatten())
    }

    /// The town of the country `country` that the point is "in": the biggest of those that claim it
    /// ([`TOWN_REACH_M`]), else the nearest within the radius. With a `region`, the towns of that region **and
    /// the towns of the country that no region contains** (the 1:10 million polygons leave out harbours,
    /// beaches and islands, and a photo that has a region must still see them); never a town of another
    /// region, so that a point just inside one is not given the town across its border.
    fn town_of(
        conn: &Connection,
        country: i64,
        region: Option<i64>,
        lon: f64,
        lat: f64,
        options: &Options,
    ) -> Result<Option<Town>> {
        let radius_m = options.town_radius_m;
        let dlat = geometry::degrees_of_latitude(radius_m);
        let dlon = geometry::degrees_of_longitude(radius_m, lat);
        let mut statement = conn.prepare_cached(
            "SELECT id, name, lat, lon, population, section FROM places \
             WHERE country_area = ?5 AND (?6 IS NULL OR region_area IS NULL OR region_area = ?6) \
             AND lat BETWEEN ?3 AND ?4 AND lon BETWEEN ?1 AND ?2",
        )?;
        let mut near: Vec<(Town, i64, bool)> = Vec::new();
        for range in geometry::longitude_ranges(lon, dlon) {
            let rows = statement.query_map(
                params![range.0, range.1, lat - dlat, lat + dlat, country, region],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, bool>(5)?,
                    ))
                },
            )?;
            for row in rows {
                let (id, name, town_lat, town_lon, population, section) = row?;
                let distance_m = geometry::haversine_m(lat, lon, town_lat, town_lon);
                if distance_m <= radius_m {
                    near.push((
                        Town {
                            id,
                            name,
                            distance_m,
                        },
                        population,
                        section,
                    ));
                }
            }
        }
        // A neighbourhood is not the city: when there is a town that is not one, within reach, the
        // neighbourhoods are left out (Copacabana is a place, Rio de Janeiro is the city).
        if near.iter().any(|(_, _, section)| !section) {
            near.retain(|(_, _, section)| !section);
        }
        let reach = |population: i64| options.town_reach_m * (population.max(0) as f64).sqrt();
        let claimed = near
            .iter()
            .filter(|(town, population, _)| town.distance_m <= reach(*population))
            .max_by(|a, b| {
                a.1.cmp(&b.1)
                    .then_with(|| b.0.distance_m.total_cmp(&a.0.distance_m))
            });
        let chosen = claimed.or_else(|| {
            near.iter()
                .min_by(|a, b| a.0.distance_m.total_cmp(&b.0.distance_m))
        });
        Ok(chosen.map(|(town, _, _)| town.clone()))
    }

    /// An area's code and its name in `lang` (English, then its own, when that one is missing).
    fn named(conn: &Connection, area: i64, lang: &str) -> Result<Named> {
        let (code, own): (String, String) = conn.query_row(
            "SELECT code, name FROM areas WHERE id = ?1",
            [area],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let name: Option<String> = conn
            .query_row(
                "SELECT name FROM area_names WHERE area = ?1 AND lang IN (?2, 'en') \
                 ORDER BY lang = ?2 DESC LIMIT 1",
                params![area, lang],
                |row| row.get(0),
            )
            .optional()?;
        Ok(Named {
            id: area,
            code,
            name: name.unwrap_or(own),
        })
    }
}

/// A file of places, open for lookups.
pub struct Places {
    conn: Connection,
    lookup: Lookup,
    options: Options,
}

impl Places {
    /// Opens the file at `path`, read-only.
    pub fn open(path: &Path) -> Result<Places> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let format: u32 = conn
            .query_row("SELECT value FROM meta WHERE key = 'format'", [], |row| {
                row.get::<_, String>(0)
            })
            .ok()
            .and_then(|text| text.parse().ok())
            .unwrap_or(0);
        if format != FORMAT {
            return Err(PlacesError::Format {
                found: format,
                supported: FORMAT,
            });
        }
        Ok(Places {
            conn,
            lookup: Lookup::new(CACHE_POINTS),
            options: Options::default(),
        })
    }

    /// The same file with other distances.
    #[must_use]
    pub fn with_options(mut self, options: Options) -> Places {
        self.options = options;
        self
    }

    /// What the file says of itself.
    pub fn info(&self) -> Result<Info> {
        let count = |table: &str| -> Result<usize> {
            Ok(self
                .conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                })? as usize)
        };
        let mut statement = self
            .conn
            .prepare("SELECT key, value FROM meta ORDER BY key")?;
        let meta = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        Ok(Info {
            format: FORMAT,
            areas: count("areas")?,
            places: count("places")?,
            meta,
        })
    }

    /// Where a position is, with the country and the region named in `lang` (`fr`, `en`, `fr-CA`: only
    /// the language part counts).
    pub fn locate(&self, lat: f64, lon: f64, lang: &str) -> Result<Located> {
        if !(lat.is_finite() && lon.is_finite() && (-90.0..=90.0).contains(&lat)) {
            return Err(PlacesError::Position { lat, lon });
        }
        let lon = if (-180.0..=180.0).contains(&lon) {
            lon
        } else {
            (lon + 180.0).rem_euclid(360.0) - 180.0
        };
        let lang = lang.split(['-', '_']).next().unwrap_or("en").to_lowercase();
        let (country, region) = self.lookup.country_and_region(
            &self.conn,
            lon,
            lat,
            self.options.coastal_tolerance_m,
        )?;
        // The town must be in the region the point is in (or in none, in the same country), so that a point
        // just inside one is not given the town across the border; with no region, in the country.
        let town = match country {
            Some(country) => Lookup::town_of(&self.conn, country, region, lon, lat, &self.options)?,
            None => None,
        };
        Ok(Located {
            country: country
                .map(|id| Lookup::named(&self.conn, id, &lang))
                .transpose()?,
            region: region
                .map(|id| Lookup::named(&self.conn, id, &lang))
                .transpose()?,
            city: town,
        })
    }
}
