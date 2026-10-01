// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading the published data sets into what [`crate::PackBuilder`] takes (design note 008 §2): Natural
//! Earth's GeoJSON for the countries and the regions, GeoNames' tab-separated file for the towns.

use serde_json::Value;

use crate::error::{PlacesError, Result};
use crate::pack::{Level, NewArea, NewPlace, Polygon};

/// The languages the names of the countries and the regions are kept in: those Auroraw ships.
pub const LANGUAGES: &[&str] = &["en", "fr"];

fn source_error(message: impl Into<String>) -> PlacesError {
    PlacesError::Source(message.into())
}

/// A text property, or `None` when it is missing, empty, or Natural Earth's "-99" for "none".
fn text(properties: &Value, key: &str) -> Option<String> {
    let value = properties.get(key)?.as_str()?.trim();
    (!value.is_empty() && value != "-99").then(|| value.to_string())
}

/// The polygons of a GeoJSON geometry: each a list of rings of `(longitude, latitude)`.
fn polygons(geometry: &Value) -> Result<Vec<Polygon>> {
    let ring = |value: &Value| -> Result<Vec<(f64, f64)>> {
        value
            .as_array()
            .ok_or_else(|| source_error("a ring is not a list"))?
            .iter()
            .map(|point| match (point.get(0), point.get(1)) {
                (Some(lon), Some(lat)) => lon
                    .as_f64()
                    .zip(lat.as_f64())
                    .ok_or_else(|| source_error("a coordinate is not a number")),
                _ => Err(source_error("a point has no coordinates")),
            })
            .collect()
    };
    let polygon = |value: &Value| -> Result<Polygon> {
        value
            .as_array()
            .ok_or_else(|| source_error("a polygon is not a list"))?
            .iter()
            .map(ring)
            .collect()
    };
    let coordinates = geometry
        .get("coordinates")
        .ok_or_else(|| source_error("a geometry has no coordinates"))?;
    match geometry.get("type").and_then(Value::as_str) {
        Some("Polygon") => Ok(vec![polygon(coordinates)?]),
        Some("MultiPolygon") => coordinates
            .as_array()
            .ok_or_else(|| source_error("a multipolygon is not a list"))?
            .iter()
            .map(polygon)
            .collect(),
        other => Err(source_error(format!("a geometry of type {other:?}"))),
    }
}

fn names(properties: &Value, prefix: &str, languages: &[&str]) -> Vec<(String, String)> {
    languages
        .iter()
        .filter_map(|lang| {
            let key = format!(
                "{prefix}{}",
                if prefix == "NAME_" {
                    lang.to_uppercase()
                } else {
                    lang.to_string()
                }
            );
            text(properties, &key).map(|name| ((*lang).to_string(), name))
        })
        .collect()
}

/// A country of Natural Earth's `admin_0_countries`, with the three-letter code regions point to it by.
pub struct Country {
    /// Natural Earth's `ADM0_A3`.
    pub key: String,
    /// The area to add.
    pub area: NewArea,
}

/// A region of Natural Earth's `admin_1_states_provinces`, with the country it names.
pub struct Region {
    /// The `adm0_a3` of its country.
    pub country_key: String,
    /// The area to add (its `parent` still to be set).
    pub area: NewArea,
}

fn features(geojson: &str) -> Result<Vec<Value>> {
    let document: Value =
        serde_json::from_str(geojson).map_err(|e| source_error(format!("not GeoJSON: {e}")))?;
    match document.get("features") {
        Some(Value::Array(features)) => Ok(features.clone()),
        _ => Err(source_error("no list of features")),
    }
}

/// The countries of Natural Earth's `ne_10m_admin_0_countries.geojson`.
pub fn countries(geojson: &str, languages: &[&str]) -> Result<Vec<Country>> {
    let mut out = Vec::new();
    for feature in features(geojson)? {
        let properties = feature
            .get("properties")
            .ok_or_else(|| source_error("a feature has no properties"))?;
        let name = text(properties, "NAME")
            .or_else(|| text(properties, "ADMIN"))
            .ok_or_else(|| source_error("a country has no name"))?;
        // `ISO_A2` is "-99" for France and Norway (overseas territories): the `_EH` code fixes them.
        let code = text(properties, "ISO_A2_EH")
            .or_else(|| text(properties, "ISO_A2"))
            .or_else(|| text(properties, "WB_A2"))
            .unwrap_or_default();
        let key = text(properties, "ADM0_A3").unwrap_or_else(|| name.clone());
        let geometry = feature
            .get("geometry")
            .ok_or_else(|| source_error("a feature has no geometry"))?;
        out.push(Country {
            key,
            area: NewArea {
                level: Level::Country,
                code: code.clone(),
                country_code: code,
                parent: None,
                names: names(properties, "NAME_", languages),
                name,
                geonames_id: None,
                parts: polygons(geometry)?,
            },
        });
    }
    Ok(out)
}

/// The regions of Natural Earth's `ne_10m_admin_1_states_provinces.geojson`.
pub fn regions(geojson: &str, languages: &[&str]) -> Result<Vec<Region>> {
    let mut out = Vec::new();
    for feature in features(geojson)? {
        let properties = feature
            .get("properties")
            .ok_or_else(|| source_error("a feature has no properties"))?;
        // A few features of the set have no name and no code of a real region (marine areas, unassigned
        // pieces of an island group): they are not regions a person would call their own place.
        let Some(name) = text(properties, "name").or_else(|| text(properties, "name_en")) else {
            continue;
        };
        let geometry = feature
            .get("geometry")
            .ok_or_else(|| source_error("a feature has no geometry"))?;
        let geonames_id = properties
            .get("gn_id")
            .and_then(Value::as_u64)
            .filter(|id| *id > 0);
        out.push(Region {
            country_key: text(properties, "adm0_a3").unwrap_or_default(),
            area: NewArea {
                level: Level::Region,
                code: text(properties, "iso_3166_2").unwrap_or_default(),
                country_code: text(properties, "iso_a2").unwrap_or_default(),
                parent: None,
                names: names(properties, "name_", languages),
                name,
                geonames_id,
                parts: polygons(geometry)?,
            },
        });
    }
    Ok(out)
}

/// The towns of GeoNames' `citiesNNN.txt`: one per line, tab-separated (identifier, name, ASCII name,
/// alternate names, latitude, longitude, feature class, feature code, country code, ..., population).
pub fn places(text: &str) -> Result<Vec<NewPlace>> {
    let mut out = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() < 15 {
            return Err(source_error(format!(
                "line {} has {} columns, not the 19 of GeoNames",
                number + 1,
                columns.len()
            )));
        }
        let bad = |what: &str| source_error(format!("line {}: {what}", number + 1));
        out.push(NewPlace {
            id: columns[0].parse().map_err(|_| bad("no identifier"))?,
            name: columns[1].to_string(),
            lat: columns[4].parse().map_err(|_| bad("no latitude"))?,
            lon: columns[5].parse().map_err(|_| bad("no longitude"))?,
            country_code: columns[8].to_string(),
            population: columns[14].parse().unwrap_or(0),
            // PPLX: "section of populated place", a neighbourhood or a borough's district.
            section: columns[7] == "PPLX",
        });
    }
    Ok(out)
}
