// SPDX-License-Identifier: GPL-3.0-or-later
//! The place of a photo in the catalogue, and the place filter (WP10, design note 008 §5, D-147): the photo row keeps
//! the four place fields as the sidecar says them and the **keys** the filter selects and groups by, the filter takes
//! a place down to a country, a region or a city, and [`Catalogue::place_facets`] answers the question the library's
//! place menu asks: which countries, regions and cities are the photos in view in, and how many in each.
//!
//! **The keys.** The text of a place field comes from several hands (Auroraw, another application, a person), so
//! `Montreal`, `Montréal` and `MONTRÉAL` must be one place: a key is the text [folded](fold_place) for case and
//! diacritics. A country's key is its ISO 3166-1 alpha-2 code, found from the photo's own fields: its code field if that
//! says a country, else its name looked up in a fixed table of the names countries go by (`country.rs`),
//! else the folded text (so that `Canada` with a code and `canada` without one are one country, and so are `Germany`,
//! `Allemagne` and `Deutschland` written next to the same `DE`). A key is **opaque**: whoever shows a place gives the
//! key back and never makes one.
//!
//! **The tree** is country, then region, then city. A photo with a country and a city but no region is under its
//! country directly, as a city whose region is the empty key. A photo with a region or a city and **no country** is
//! under one more node, after the countries, whose filter has the empty country key (issue #60; the menu says it as
//! "(no country)" in the interface's language, so its label is empty); it has regions and cities as a country has. A
//! photo with none of the three is in no node: the filter cannot select it, and it is not counted in `placed`.

use std::collections::{BTreeMap, HashMap};

use auroraw_format::sidecar::Metadata;
use rusqlite::{Transaction, params};
use serde_json::{Value, json};

use crate::error::Result;
use crate::open::Catalogue;
use crate::query::Filter;

/// Whether `c` is a combining mark that a folded place name drops (the accents of `é`, `ñ`, `ệ`, once decomposed).
fn is_combining_mark(c: char) -> bool {
    matches!(c,
        '\u{0300}'..='\u{036F}'   // combining diacritical marks
        | '\u{1AB0}'..='\u{1AFF}' // extended
        | '\u{1DC0}'..='\u{1DFF}' // supplement
        | '\u{20D0}'..='\u{20FF}' // for symbols
        | '\u{FE20}'..='\u{FE2F}' // half marks
    )
}

/// Whether `c` is a character that shows nothing and is not white space: a soft hyphen, a zero-width space or joiner,
/// a direction mark, a word joiner, a byte-order mark. A text copied from a web page or a document carries them, and
/// `Paris` followed by one is not another place (issue #84).
fn is_invisible(c: char) -> bool {
    matches!(c,
        '\u{00AD}'                // soft hyphen
        | '\u{200B}'..='\u{200F}' // zero-width space and joiners, direction marks
        | '\u{202A}'..='\u{202E}' // direction embeddings and overrides
        | '\u{2060}'..='\u{2064}' // word joiner and invisible operators
        | '\u{2066}'..='\u{2069}' // direction isolates
        | '\u{FEFF}'              // byte-order mark
    )
}

/// A place name folded for case and diacritics, the form the place filter compares and groups by: `Montréal`,
/// `Montreal` and `MONTRÉAL` are `montreal`. Decomposed **with the compatibility forms** (so that `é` is `e` and a mark,
/// a full-width `Ａ` is `A`, a ligature `ﬁ` is `fi`, `Ĳ` is `IJ`), the marks dropped, lower-cased, the letters that do not
/// decompose given their plain form (`ß` as `ss`, `æ` as `ae`, `œ` as `oe`, `ø` as `o`, `đ` and `ð` as `d`, `ł` as `l`,
/// `ħ` as `h`, dotless `ı` as `i`, the Greek final `ς` as `σ`, since a capital `Σ` lower-cases to `σ` whatever its place in
/// the word), the invisible characters dropped, the typographic apostrophes made one, and hyphens, dashes, underscores and
/// runs of white space made a single space, so that `Trois-Rivières` and `Trois Rivieres` are one place. Empty for a text
/// with nothing in it.
pub fn fold_place(text: &str) -> String {
    // (The acute accent U+00B4 has a compatibility decomposition, a space and a combining mark, which would turn
    // `L´Assomption` into two words: it is the apostrophe it was taken for before the normalisation.)
    let text = text.replace('\u{00B4}', "\u{2019}");
    let decomposed = icu_normalizer::DecomposingNormalizer::new_nfkd().normalize(&text);
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in decomposed.chars() {
        if is_combining_mark(c) || is_invisible(c) {
            continue;
        }
        let c = match c {
            '\u{2018}' | '\u{2019}' | '\u{02BC}' | '\u{0060}' | '\u{00B4}' => '\'',
            '-' | '_' | '\u{2010}'..='\u{2015}' => ' ',
            c => c,
        };
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        for lower in c.to_lowercase() {
            match lower {
                'ß' => out.push_str("ss"),
                'æ' => out.push_str("ae"),
                'œ' => out.push_str("oe"),
                'ø' => out.push('o'),
                'đ' | 'ð' => out.push('d'),
                'ł' => out.push('l'),
                'ħ' => out.push('h'),
                'ı' => out.push('i'),
                'ς' => out.push('σ'),
                other => out.push(other),
            }
        }
    }
    out
}

/// The version of the keys: of [`fold_place`] and of the table of countries (`country_names.tsv`). The keys are stored in
/// the photo rows, so a correction of either (a letter that does not decompose, a country named another way) leaves the
/// stored keys as they were until the columns are made again: the catalogue records the version it made them with
/// (`meta.place_keys`) and, when it is opened by a program with another, asks for them to be filled again from the
/// sidecars. **Raise it with any change that gives some text another key**; a test that holds a fixed list of keys
/// fails until it is.
pub const PLACE_KEYS_VERSION: &str = "2";

/// What the photo row keeps of a photo's place: the four fields as the sidecar says them, and the keys the filter
/// uses (see the module documentation). `None` for a field that is empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaceColumns {
    /// `photoshop:City`.
    pub city: Option<String>,
    /// `photoshop:State`.
    pub region: Option<String>,
    /// `photoshop:Country`.
    pub country: Option<String>,
    /// `Iptc4xmpCore:CountryCode`.
    pub country_code: Option<String>,
    /// The country's key: its ISO alpha-2 code, from its code field or its name (see `country.rs`), else the
    /// folded text.
    pub country_key: Option<String>,
    /// The region's key: its folded name.
    pub region_key: Option<String>,
    /// The city's key: its folded name.
    pub city_key: Option<String>,
}

fn non_empty(text: &Option<String>) -> Option<String> {
    text.as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

fn key_of(text: &Option<String>) -> Option<String> {
    text.as_deref()
        .map(fold_place)
        .filter(|key| !key.is_empty())
}

impl PlaceColumns {
    /// The columns of a photo's metadata.
    pub fn of(meta: &Metadata) -> PlaceColumns {
        let (city, region, country, country_code) = (
            non_empty(&meta.city),
            non_empty(&meta.region),
            non_empty(&meta.country),
            non_empty(&meta.country_code),
        );
        let country_key = crate::country::country_key(country_code.as_deref(), country.as_deref());
        PlaceColumns {
            region_key: key_of(&region),
            city_key: key_of(&city),
            city,
            region,
            country,
            country_code,
            country_key,
        }
    }
}

/// The photos with a region or a city and **no country**, which the tree gathers under its last node, "(no country)"
/// (issue #60). The same condition is the `WHERE` of the index `photo_place_nocountry`, so that the tree and the
/// filter read that index alone (SQLite uses a partial index when the query states the index's condition).
pub(crate) const NO_COUNTRY: &str =
    "p.place_country IS NULL AND (p.place_region IS NOT NULL OR p.place_city IS NOT NULL)";

/// A place to filter by, from the most general down: a country, a region of it, a city of it. Each is a **key** as the
/// catalogue made it ([`PlaceFacets`] gives them). A level left out is not constrained; the empty key of a region
/// or a city means "none": the photos of the country that have no region. The empty key of the **country** is the
/// "(no country)" node: the photos that have a region or a city and no country (not the photos with no place at all,
/// which no node holds).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaceFilter {
    /// The country's key.
    pub country: Option<String>,
    /// The region's key (`Some("")`: photos with no region).
    pub region: Option<String>,
    /// The city's key (`Some("")`: photos with no city).
    pub city: Option<String>,
}

impl PlaceFilter {
    /// Whether it constrains nothing.
    pub fn is_empty(&self) -> bool {
        self.country.is_none() && self.region.is_none() && self.city.is_none()
    }

    fn to_value(&self) -> Value {
        let mut object = serde_json::Map::new();
        for (name, key) in [
            ("country", &self.country),
            ("region", &self.region),
            ("city", &self.city),
        ] {
            if let Some(key) = key {
                object.insert(name.into(), Value::String(key.clone()));
            }
        }
        Value::Object(object)
    }

    /// The filter as the JSON text the library's place menu gives back and takes: `{ "country": key, "region": key,
    /// "city": key }`, a level left out when it is not constrained.
    pub fn to_json(&self) -> String {
        self.to_value().to_string()
    }

    /// The filter a JSON text says (see [`PlaceFilter::to_json`]); `None` for text that is not one, and for an
    /// empty text or object, which constrain nothing.
    pub fn from_json(text: &str) -> Option<PlaceFilter> {
        let Value::Object(object) = serde_json::from_str::<Value>(text).ok()? else {
            return None;
        };
        let key = |name: &str| object.get(name).and_then(Value::as_str).map(str::to_string);
        let filter = PlaceFilter {
            country: key("country"),
            region: key("region"),
            city: key("city"),
        };
        (!filter.is_empty()).then_some(filter)
    }

    /// The conditions of the filter on the photo row `p`, and their values.
    pub(crate) fn conditions(
        &self,
        conditions: &mut Vec<String>,
        values: &mut Vec<rusqlite::types::Value>,
    ) {
        for (column, key) in [
            ("p.place_country", &self.country),
            ("p.place_region", &self.region),
            ("p.place_city", &self.city),
        ] {
            match key {
                None => {}
                // The empty country key is the "(no country)" node, which has a place; the empty key of a region or a
                // city is "none" and nothing more.
                Some(key) if key.is_empty() && column == "p.place_country" => {
                    conditions.push(NO_COUNTRY.to_string())
                }
                Some(key) if key.is_empty() => conditions.push(format!("{column} IS NULL")),
                Some(key) => {
                    conditions.push(format!("{column} = ?"));
                    values.push(rusqlite::types::Value::Text(key.clone()));
                }
            }
        }
    }
}

/// A node of the tree of places the photos in view are in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceNode {
    /// The spelling to show: the most common one among the photos of the node (the best written on a tie, so that
    /// it does not flicker).
    pub label: String,
    /// How many photos are under the node.
    pub count: u64,
    /// What selects them.
    pub filter: PlaceFilter,
    /// The regions of a country, the cities of a region (and of a country, for those with no region).
    pub children: Vec<PlaceNode>,
}

impl PlaceNode {
    fn to_json(&self) -> Value {
        json!({
            "label": self.label,
            "count": self.count,
            "filter": self.filter.to_value(),
            "children": self.children.iter().map(PlaceNode::to_json).collect::<Vec<_>>(),
        })
    }
}

/// The places the photos in view are in: the answer of [`Catalogue::place_facets`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaceFacets {
    /// How many photos in view have a place: a country, or a region or a city without one.
    pub placed: u64,
    /// The countries, by label.
    pub countries: Vec<PlaceNode>,
    /// The photos that have a region or a city and no country, as a node of their own (its filter's country is the
    /// empty key; its label is empty, for the menu to say in the interface's language); `None` when no photo in view
    /// needs it. It is not among the countries, since the menu puts it after them.
    pub no_country: Option<PlaceNode>,
    /// Whether the place columns are still being filled from the sidecars (the first open after the upgrade to
    /// schema 6, or after a change of the folding): the tree is then a part of the places, or none, and `placed: 0`
    /// does not mean that no photo has a place.
    pub pending: bool,
}

impl PlaceFacets {
    /// The tree as the JSON text the library's place menu reads: `{ "placed": N, "pending": bool, "countries": [Node],
    /// "noCountry": Node | null }`, a Node being `{ "label", "count", "filter", "children": [Node] }`.
    pub fn to_json(&self) -> String {
        json!({
            "placed": self.placed,
            "pending": self.pending,
            "countries": self.countries.iter().map(PlaceNode::to_json).collect::<Vec<_>>(),
            "noCountry": self.no_country.as_ref().map(PlaceNode::to_json),
        })
        .to_string()
    }
}

/// The spellings seen at a node and how often.
#[derive(Default)]
struct Spellings(HashMap<String, u64>);

impl Spellings {
    fn add(&mut self, text: Option<&str>, photos: u64) {
        if let Some(text) = text.filter(|t| !t.is_empty()) {
            *self.0.entry(text.to_string()).or_default() += photos;
        }
    }

    /// The most common spelling, or `fallback` when no photo spells it. Spellings that tie are told apart by how
    /// well they are written, so that the label is the one a person would have chosen and does not flicker with the
    /// order of the photos: first the one in mixed case (`Montréal` over `MONTRÉAL` and `montréal`), then the one that
    /// keeps the most accents (`Montréal` over `Montreal`), then the smallest text.
    fn label(&self, fallback: &str) -> String {
        fn quality(text: &str) -> (bool, usize) {
            let mixed =
                text.chars().any(char::is_uppercase) && text.chars().any(char::is_lowercase);
            (mixed, text.chars().filter(|c| !c.is_ascii()).count())
        }
        self.0
            .iter()
            .max_by(|a, b| {
                a.1.cmp(b.1)
                    .then_with(|| quality(a.0).cmp(&quality(b.0)))
                    .then_with(|| b.0.cmp(a.0))
            })
            .map_or_else(|| fallback.to_string(), |(text, _)| text.clone())
    }
}

#[derive(Default)]
struct Branch {
    count: u64,
    spellings: Spellings,
    /// The codes photos of the node give where they name no country: shown only when no photo names it, so that
    /// three photos with the code alone do not label the node `CA` over two that say `Canada`.
    codes: Spellings,
    children: BTreeMap<String, Branch>,
}

impl Branch {
    /// The label of the node: its most common spelling, else its most common code, else `key`.
    fn label(&self, key: &str) -> String {
        if self.spellings.0.is_empty() {
            self.codes.label(key)
        } else {
            self.spellings.label(key)
        }
    }
}

fn nodes(
    branches: BTreeMap<String, Branch>,
    filter_of: &dyn Fn(&str) -> PlaceFilter,
) -> Vec<PlaceNode> {
    let mut out: Vec<PlaceNode> = branches
        .into_iter()
        .map(|(key, branch)| PlaceNode {
            label: branch.label(&key),
            count: branch.count,
            filter: filter_of(&key),
            children: Vec::new(),
        })
        .collect();
    sort_nodes(&mut out);
    out
}

/// Puts nodes in the order of their labels as a person reads them (folded for case and accents, the text itself on a
/// tie). The folded label is made once for each node, not at every comparison.
fn sort_nodes(nodes: &mut [PlaceNode]) {
    nodes.sort_by_cached_key(|node| (fold_place(&node.label), node.label.clone()));
}

/// The query that reads the countries of the tree, for the `conditions` of a filter (those of every filter but the
/// place). The one text the tree runs and the tests read the plan of: a column added to it is a column the covering index
/// must have, and a test says so.
pub(crate) fn countries_sql(conditions: &[String]) -> String {
    let mut conditions = conditions.to_vec();
    conditions.push("p.place_country IS NOT NULL".into());
    format!(
        "SELECT p.place_country, p.place_region, p.place_city, p.country, p.region, p.city, p.country_code, COUNT(*)
         FROM photo p WHERE {} GROUP BY 1, 2, 3, 4, 5, 6, 7",
        conditions.join(" AND ")
    )
}

/// The query that reads the node of the photos with a region or a city and no country (see [`countries_sql`]).
pub(crate) fn no_country_sql(conditions: &[String]) -> String {
    let mut conditions = conditions.to_vec();
    conditions.push(NO_COUNTRY.into());
    format!(
        "SELECT p.place_region, p.place_city, p.region, p.city, COUNT(*)
         FROM photo p WHERE {} GROUP BY 1, 2, 3, 4",
        conditions.join(" AND ")
    )
}

impl Branch {
    /// Counts `photos` photos at a place under this branch: a region (and the city under it), or a city directly under
    /// it; the empty key is "no region".
    fn add_place(
        &mut self,
        region_key: Option<String>,
        city_key: Option<String>,
        region: Option<String>,
        city: Option<String>,
        photos: u64,
    ) {
        let region_branch = self
            .children
            .entry(region_key.clone().unwrap_or_default())
            .or_default();
        if region_key.is_some() {
            region_branch.count += photos;
            region_branch.spellings.add(region.as_deref(), photos);
        }
        if let Some(city_key) = city_key {
            let city_branch = region_branch.children.entry(city_key).or_default();
            city_branch.count += photos;
            city_branch.spellings.add(city.as_deref(), photos);
        }
    }
}

/// The node of a country (or, for the empty key, of the photos that have no country): its regions, then the cities of
/// a region, and the cities of a region-less group directly under it.
fn country_node(country_key: &str, branch: Branch) -> PlaceNode {
    let label = branch.label(country_key);
    let count = branch.count;
    let mut children: Vec<PlaceNode> = Vec::new();
    for (region_key, region_branch) in branch.children {
        if region_key.is_empty() {
            // Cities with no region: under the country, selected by "no region" and the city.
            children.extend(nodes(region_branch.children, &|city| PlaceFilter {
                country: Some(country_key.to_string()),
                region: Some(String::new()),
                city: Some(city.to_string()),
            }));
            continue;
        }
        let region_node = PlaceNode {
            label: region_branch.label(&region_key),
            count: region_branch.count,
            filter: PlaceFilter {
                country: Some(country_key.to_string()),
                region: Some(region_key.clone()),
                city: None,
            },
            children: nodes(region_branch.children, &|city| PlaceFilter {
                country: Some(country_key.to_string()),
                region: Some(region_key.clone()),
                city: Some(city.to_string()),
            }),
        };
        children.push(region_node);
    }
    sort_nodes(&mut children);
    PlaceNode {
        label,
        count,
        filter: PlaceFilter {
            country: Some(country_key.to_string()),
            ..PlaceFilter::default()
        },
        children,
    }
}

impl Catalogue {
    /// The tree of places the photos a [`Filter`] lets through are in, **leaving out the filter's own place**: so that
    /// the tree stays one to move around in while a place is chosen (choosing Quebec still shows Ontario, with its
    /// count). Every other filter composes as for [`Catalogue::list_filtered`].
    pub fn place_facets(&self, filter: &Filter) -> Result<PlaceFacets> {
        let mut conditions: Vec<String> = Vec::new();
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        filter.conditions(false, &mut conditions, &mut values);
        let mut placed = 0;

        // The countries.
        let sql = countries_sql(&conditions);
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(values.clone()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)? as u64,
            ))
        })?;
        let mut countries: BTreeMap<String, Branch> = BTreeMap::new();
        for row in rows {
            let (country_key, region_key, city_key, country, region, city, code, photos) = row?;
            placed += photos;
            let country_branch = countries.entry(country_key).or_default();
            country_branch.count += photos;
            // The name if the photo has one, else the code (a label of last resort); the node's key is the fallback.
            match country.as_deref().filter(|name| !name.is_empty()) {
                Some(name) => country_branch.spellings.add(Some(name), photos),
                None => country_branch.codes.add(code.as_deref(), photos),
            }
            country_branch.add_place(region_key, city_key, region, city, photos);
        }
        let mut tree: Vec<PlaceNode> = countries
            .into_iter()
            .map(|(key, branch)| country_node(&key, branch))
            .collect();
        sort_nodes(&mut tree);

        // The photos with a region or a city and no country: one more node, after the countries (issue #60).
        let sql = no_country_sql(&conditions);
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(values), |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)? as u64,
            ))
        })?;
        let mut none = Branch::default();
        for row in rows {
            let (region_key, city_key, region, city, photos) = row?;
            placed += photos;
            none.count += photos;
            none.add_place(region_key, city_key, region, city, photos);
        }
        let no_country = (none.count > 0).then(|| country_node("", none));
        Ok(PlaceFacets {
            placed,
            countries: tree,
            no_country,
            pending: self.place_columns_stale()?,
        })
    }

    /// Whether the place columns of the photo rows may be out of date: a catalogue made before schema 6 had none, and
    /// they are filled by reading each sidecar once ([`Catalogue::apply_place_columns`] then
    /// [`Catalogue::mark_place_columns_fresh`]), which a rebuild does by itself.
    pub fn place_columns_stale(&self) -> Result<bool> {
        let stale: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'place_columns'",
                [],
                |r| r.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(stale.as_deref() == Some("stale"))
    }

    /// Asks for the place columns to be filled again from the sidecars (the engine does it when it opens the
    /// catalogue): after a repair, or in a test of that pass.
    pub fn mark_place_columns_stale(&self) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES ('place_columns', 'stale')",
            [],
        )?;
        Ok(())
    }

    /// Every photo of the catalogue, in no order: what a pass over the sidecars walks.
    pub fn photo_ids(&self) -> Result<Vec<auroraw_types::PhotoId>> {
        let mut statement = self.conn.prepare("SELECT id FROM photo")?;
        let ids = statement
            .query_map([], |r| r.get::<_, String>(0))?
            .filter_map(|id| id.ok()?.parse().ok())
            .collect();
        Ok(ids)
    }

    /// Says that the place columns are up to date.
    pub fn mark_place_columns_fresh(&self) -> Result<()> {
        self.conn
            .execute("DELETE FROM meta WHERE key = 'place_columns'", [])?;
        Ok(())
    }

    /// Sets the place columns of one photo to `columns`, and nothing else of the row. Not an error if the photo is
    /// not there (it left meanwhile).
    pub fn apply_place_columns(
        &mut self,
        photo: &auroraw_types::PhotoId,
        columns: &PlaceColumns,
    ) -> Result<()> {
        let tx: Transaction = self.conn.transaction()?;
        set_columns(&tx, &photo.to_string(), columns)?;
        tx.commit()?;
        Ok(())
    }
}

/// `UPDATE`s the place columns of the photo `id`.
pub(crate) fn set_columns(tx: &Transaction, id: &str, c: &PlaceColumns) -> rusqlite::Result<usize> {
    tx.execute(
        "UPDATE photo SET country = ?1, region = ?2, city = ?3, country_code = ?4,
            place_country = ?5, place_region = ?6, place_city = ?7 WHERE id = ?8",
        params![
            c.country,
            c.region,
            c.city,
            c.country_code,
            c.country_key,
            c.region_key,
            c.city_key,
            id
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_folded_for_case_and_diacritics() {
        for same in [
            "Montréal",
            "Montreal",
            "MONTRÉAL",
            "  montréal ",
            "Montre\u{301}al",
        ] {
            assert_eq!(fold_place(same), "montreal", "{same:?}");
        }
        assert_eq!(fold_place("São Paulo"), "sao paulo");
        assert_eq!(
            fold_place("Hà Nội"),
            "ha noi",
            "stacked marks of Vietnamese"
        );
        assert_eq!(fold_place("Đà Nẵng"), "da nang");
        assert_eq!(
            fold_place("Αθήνα"),
            "αθηνα",
            "Greek loses its accent and keeps its letters"
        );
        assert_eq!(
            fold_place("Москва"),
            "москва",
            "Cyrillic is only lower-cased"
        );
        assert_eq!(fold_place("İstanbul"), "istanbul");
    }

    #[test]
    fn one_place_written_with_other_forms_of_the_same_letters_is_one_key() {
        // Issue #84: five ways of writing one place that gave two nodes.
        for (a, b) in [
            // A capital sigma lower-cases to `σ` and a typed final `ς` stays `ς`: the same word.
            ("ΚΟΡΙΝΘΟΣ", "Κόρινθος"),
            // Full-width letters, as a Japanese input method writes them.
            ("Ａｒｉｓ", "Aris"),
            ("Ｔｏｋｙｏ", "tokyo"),
            // Ligatures.
            ("ﬁord", "fiord"),
            ("Ĳmuiden", "IJmuiden"),
            // Characters that show nothing, from a copy and paste of a web page or a document.
            ("Paris\u{200B}", "Paris"),
            ("Pa\u{00AD}ris", "Paris"),
            ("\u{FEFF}Paris", "Paris"),
            ("Pa\u{2060}ris\u{200E}", "Paris"),
            ("Par\u{202B}is\u{202C}", "Paris"),
        ] {
            assert_eq!(fold_place(a), fold_place(b), "{a:?} and {b:?}");
        }
        assert_eq!(fold_place("Κόρινθος"), "κορινθοσ");
        // What was already one key still is, and what is two places stays two: transliteration and abbreviation are
        // not folding.
        assert_eq!(fold_place("L´Assomption"), fold_place("L'Assomption"));
        assert_eq!(fold_place("Montréal"), "montreal");
        assert_ne!(fold_place("Muenchen"), fold_place("München"));
        assert_ne!(fold_place("St-Jean"), fold_place("Saint-Jean"));
        // A text of invisible characters alone has nothing in it.
        assert_eq!(fold_place("\u{200B}\u{FEFF}"), "");
    }

    #[test]
    fn the_letters_that_do_not_decompose_get_their_plain_form() {
        assert_eq!(fold_place("Straße"), "strasse");
        assert_eq!(fold_place("STRASSE"), "strasse");
        assert_eq!(fold_place("Œuvre"), "oeuvre");
        assert_eq!(fold_place("Ærø"), "aero");
        assert_eq!(fold_place("København"), "kobenhavn");
        assert_eq!(fold_place("Łódź"), "lodz");
        assert_eq!(fold_place("Ħamrun"), "hamrun");
    }

    #[test]
    fn the_keys_are_the_ones_the_version_says() {
        // The keys are stored: if one of these changes on purpose, raise PLACE_KEYS_VERSION, so that the catalogues
        // made with the old keys are filled again; then change the list.
        assert_eq!(PLACE_KEYS_VERSION, "2");
        for (text, key) in [
            ("Montréal", "montreal"),
            ("Trois-Rivières", "trois rivieres"),
            ("L’Assomption", "l'assomption"),
            ("Straße", "strasse"),
            ("Łódź", "lodz"),
            ("Hà Nội", "ha noi"),
            ("İstanbul", "istanbul"),
            ("Αθήνα", "αθηνα"),
            ("ΚΟΡΙΝΘΟΣ", "κορινθοσ"),
            ("Ｔｏｋｙｏ", "tokyo"),
            ("ﬁord", "fiord"),
            ("Paris\u{200B}", "paris"),
        ] {
            assert_eq!(fold_place(text), key, "{text:?}");
        }
        for (code, name, key) in [
            (Some("CA"), Some("Canada"), "CA"),
            (None, Some("canada"), "CA"),
            (Some("can"), None, "CA"),
            (None, Some("États-Unis"), "US"),
            (None, Some("Allemagne"), "DE"),
            (None, Some("Deutschland"), "DE"),
            (None, Some("Congo"), "congo"),
            (None, Some("Tyskland"), "tyskland"),
            (Some("zz"), None, "ZZ"),
            (None, Some("Atlantis"), "atlantis"),
        ] {
            assert_eq!(
                crate::country::country_key(code, name).as_deref(),
                Some(key),
                "{code:?} {name:?}"
            );
        }
    }

    #[test]
    fn hyphens_apostrophes_and_white_space_are_one_thing() {
        assert_eq!(fold_place("Trois-Rivières"), fold_place("Trois Rivieres"));
        assert_eq!(fold_place("Trois-Rivières"), "trois rivieres");
        assert_eq!(fold_place("L’Assomption"), fold_place("L'Assomption"));
        assert_eq!(fold_place("Saint  \t Jean"), "saint jean");
        assert_eq!(fold_place("Stratford–upon–Avon"), "stratford upon avon");
        assert_eq!(fold_place(""), "");
        assert_eq!(
            fold_place("  - \t"),
            "",
            "nothing left of a text with nothing in it"
        );
    }

    #[test]
    fn a_photos_columns_hold_its_fields_and_the_keys_the_filter_uses() {
        let mut meta = Metadata {
            city: Some(" Montréal ".into()),
            region: Some("Québec".into()),
            country: Some("Canada".into()),
            country_code: Some("ca".into()),
            ..Metadata::default()
        };
        let c = PlaceColumns::of(&meta);
        assert_eq!(c.city.as_deref(), Some("Montréal"), "trimmed");
        assert_eq!(
            c.country_key.as_deref(),
            Some("CA"),
            "the code, upper-case, over the name"
        );
        assert_eq!(c.region_key.as_deref(), Some("quebec"));
        assert_eq!(c.city_key.as_deref(), Some("montreal"));
        // Without a code, the code the name has in the table of countries: the same country.
        meta.country_code = None;
        assert_eq!(PlaceColumns::of(&meta).country_key.as_deref(), Some("CA"));
        // A name the table does not have: the folded name.
        meta.country = Some("Atlantis".into());
        assert_eq!(
            PlaceColumns::of(&meta).country_key.as_deref(),
            Some("atlantis")
        );
        meta.country = Some("Canada".into());
        // Only a code is a country; only a city is none.
        meta.country = None;
        meta.country_code = Some("CA".into());
        assert_eq!(PlaceColumns::of(&meta).country_key.as_deref(), Some("CA"));
        meta.country_code = None;
        let city_only = PlaceColumns::of(&meta);
        assert_eq!(city_only.country_key, None);
        assert_eq!(city_only.city_key.as_deref(), Some("montreal"));
        // Empty texts are no texts.
        let empty = Metadata {
            city: Some(String::new()),
            region: Some("  ".into()),
            ..Metadata::default()
        };
        assert_eq!(PlaceColumns::of(&empty), PlaceColumns::default());
    }

    /// The lines of SQLite's plan for `sql` run with `values`.
    fn plan(cat: &Catalogue, sql: &str, values: Vec<rusqlite::types::Value>) -> Vec<String> {
        let mut statement = cat
            .conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap();
        statement
            .query_map(rusqlite::params_from_iter(values), |row| {
                row.get::<_, String>(3)
            })
            .unwrap()
            .map(|line| line.unwrap())
            .collect()
    }

    #[test]
    fn the_tree_reads_its_covering_indexes_for_the_usual_filters() {
        // The tree is 20 ms at 100,000 photos because it never touches the table: the countries' query reads
        // `photo_place` alone, and the node of the photos with no country reads `photo_place_nocountry` alone (SQLite
        // uses a partial index when the query states the index's condition). The queries are the ones `place_facets`
        // runs (`countries_sql`, `no_country_sql`), so that a column added to them that the index does not hold fails
        // here, on any machine, and not in a measurement nobody reads (issue #84).
        use crate::query::FlagFilter;
        let cat = Catalogue::open_in_memory(auroraw_types::WorkspaceId::random()).unwrap();
        let filters = [
            ("no filter", Filter::default()),
            (
                "a minimum rating",
                Filter {
                    min_rating: 4,
                    ..Filter::default()
                },
            ),
            (
                "the picked photos",
                Filter {
                    flags: FlagFilter::Picked,
                    ..Filter::default()
                },
            ),
        ];
        for (what, filter) in filters {
            let mut conditions = Vec::new();
            let mut values = Vec::new();
            filter.conditions(false, &mut conditions, &mut values);
            let countries = plan(&cat, &countries_sql(&conditions), values.clone());
            assert!(
                countries
                    .iter()
                    .any(|line| line.contains("COVERING INDEX photo_place ")
                        || line.ends_with("COVERING INDEX photo_place")),
                "{what}, the countries: {countries:?}"
            );
            let none = plan(&cat, &no_country_sql(&conditions), values);
            assert!(
                none.iter()
                    .any(|line| line.contains("COVERING INDEX photo_place_nocountry")),
                "{what}, the node of the photos with no country: {none:?}"
            );
        }
        // And the filter's own query for that node, which the first photos of a node come from.
        let selected = plan(
            &cat,
            &format!("SELECT p.id FROM photo p WHERE {NO_COUNTRY} AND p.place_region = 'x'"),
            Vec::new(),
        );
        assert!(
            selected
                .iter()
                .any(|line| line.contains("photo_place_nocountry")),
            "{selected:?}"
        );
    }

    #[test]
    fn a_place_filter_is_the_json_the_menu_gives_back() {
        let filter = PlaceFilter {
            country: Some("CA".into()),
            region: Some("quebec".into()),
            city: None,
        };
        assert_eq!(filter.to_json(), r#"{"country":"CA","region":"quebec"}"#);
        assert_eq!(PlaceFilter::from_json(&filter.to_json()), Some(filter));
        // "No region" is the empty key, and survives.
        let none = PlaceFilter {
            country: Some("SG".into()),
            region: Some(String::new()),
            city: Some("singapore".into()),
        };
        assert_eq!(PlaceFilter::from_json(&none.to_json()), Some(none));
        // Nothing, or not a filter, is no filter.
        for text in [
            "",
            "{}",
            "[]",
            "null",
            "not json",
            r#"{"town":"x"}"#,
            r#"{"country":1}"#,
        ] {
            assert_eq!(PlaceFilter::from_json(text), None, "{text:?}");
        }
        // A level of another type is ignored, the others kept.
        assert_eq!(
            PlaceFilter::from_json(r#"{"country":"CA","region":3,"extra":true}"#),
            Some(PlaceFilter {
                country: Some("CA".into()),
                ..PlaceFilter::default()
            })
        );
    }
}
