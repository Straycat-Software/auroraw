// SPDX-License-Identifier: GPL-3.0-or-later
use auroraw_types::KeywordId;
use serde::{Deserialize, Serialize};

use crate::sidecar::{ColourLabel, Flag, Metadata};

/// The fields of a photo that Auroraw compares with a foreign XMP file, as plain normalised values.
///
/// One value per field, the same shape whether it came from a file ([`super::read`]) or from a photo
/// sidecar ([`Fields::from_metadata`]), so that two `Fields` are equal exactly when the photographer
/// would say the two sides agree. Deliberately absent: the original's capture data, the overlay, the
/// custom fields, keyword identifiers and everything `aur:`-namespaced — nothing another tool edits.
///
/// **The rating is one axis** (`-1` rejected, `0` unset, `1` to `5` stars), the way other software
/// keeps it: Auroraw's stars and Rejected flag are projected onto it, so that a single external
/// "reject" is one field changing, not two.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fields {
    /// `-1` rejected, `1` to `5` stars; `None` for unset (a `0` is the same thing).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<i8>,
    /// The colour label; one of the five names in their canonical spelling, else the file's own text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The caption.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// The creators, in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub creator: Vec<String>,
    /// The copyright notice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rights: Option<String>,
    /// The usage terms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_terms: Option<String>,
    /// The web statement of rights.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_statement: Option<String>,
    /// The credit line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit: Option<String>,
    /// The source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The headline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headline: Option<String>,
    /// The instructions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// The sublocation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sublocation: Option<String>,
    /// The city.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    /// The region or state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// The country.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// The ISO country code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country_code: Option<String>,
    /// The persons shown, sorted and without duplicates (a set).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub persons: Vec<String>,
    /// The event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// The keywords as paths with `|` between levels, without duplicates (compared in lower case) and
    /// sorted by that: a set.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

/// Text as the comparison sees it: line ends as LF, trimmed, and nothing when nothing is left.
fn text(value: &str) -> Option<String> {
    let value = value.replace("\r\n", "\n").replace('\r', "\n");
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn opt(value: &Option<String>) -> Option<String> {
    value.as_deref().and_then(text)
}

fn ordered(values: &[String]) -> Vec<String> {
    values.iter().filter_map(|v| text(v)).collect()
}

fn set(values: &[String]) -> Vec<String> {
    let mut values = ordered(values);
    values.sort();
    values.dedup();
    values
}

/// A keyword path as the comparison sees it: levels trimmed, empty levels dropped.
pub fn normalise_path(path: &str) -> Option<String> {
    let levels: Vec<&str> = path
        .split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    (!levels.is_empty()).then(|| levels.join("|"))
}

/// What identifies a keyword path when two sides are compared: the normalised path in lower case
/// (the vocabulary itself refuses two siblings that differ only by case).
pub fn keyword_key(path: &str) -> String {
    normalise_path(path).unwrap_or_default().to_lowercase()
}

fn keyword_set(paths: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = std::collections::BTreeMap::new();
    for path in paths {
        if let Some(path) = normalise_path(&path) {
            seen.entry(path.to_lowercase()).or_insert(path);
        }
    }
    seen.into_values().collect()
}

fn label(value: &Option<String>) -> Option<String> {
    let t = opt(value)?;
    Some(
        t.parse::<ColourLabel>()
            .map(|c| c.name().to_string())
            .unwrap_or(t),
    )
}

/// Stars and Rejected flag projected onto the single foreign rating axis.
fn axis(meta: &Metadata) -> Option<i8> {
    if meta.flag == Some(Flag::Rejected) {
        return Some(-1);
    }
    match meta.rating {
        Some(n @ 1..=5) => Some(n as i8),
        _ => None,
    }
}

impl Fields {
    /// The fields of a photo's metadata, or of a foreign file already read into a [`Metadata`], with the
    /// normalisation both sides share. `keyword_path` gives a keyword's current path from its
    /// identifier (the vocabulary's word, not the sidecar's possibly stale snapshot); for an identifier
    /// it does not know, the snapshot at the same position is used when the two lists line up.
    pub fn from_metadata(
        meta: &Metadata,
        keyword_path: impl Fn(&KeywordId) -> Option<String>,
    ) -> Self {
        let paths: Vec<String> = if meta.keyword_ids.is_empty() {
            meta.keyword_paths.clone()
        } else {
            let aligned = meta.keyword_ids.len() == meta.keyword_paths.len();
            meta.keyword_ids
                .iter()
                .enumerate()
                .filter_map(|(i, id)| {
                    keyword_path(id).or_else(|| aligned.then(|| meta.keyword_paths[i].clone()))
                })
                .collect()
        };
        Self {
            rating: axis(meta),
            label: label(&meta.label),
            title: opt(&meta.title),
            caption: opt(&meta.caption),
            creator: ordered(&meta.creator),
            rights: opt(&meta.rights),
            usage_terms: opt(&meta.usage_terms),
            web_statement: opt(&meta.web_statement),
            credit: opt(&meta.credit),
            source: opt(&meta.source),
            headline: opt(&meta.headline),
            instructions: opt(&meta.instructions),
            sublocation: opt(&meta.sublocation),
            city: opt(&meta.city),
            region: opt(&meta.region),
            country: opt(&meta.country),
            country_code: opt(&meta.country_code),
            persons: set(&meta.persons),
            event: opt(&meta.event),
            keywords: keyword_set(paths),
        }
    }

    /// Copies these fields into `meta`, the metadata of a photo being added, over whatever it holds for
    /// the fields the file has a value for. **Not the keywords** (they need identifiers, which only the
    /// vocabulary can give). A rejected rating sets the Rejected flag and no stars (a photo that
    /// arrives has none to keep).
    pub fn fill(&self, meta: &mut Metadata) {
        match self.rating {
            Some(-1) => {
                meta.flag = Some(Flag::Rejected);
                meta.rating = None;
            }
            Some(n @ 1..=5) => meta.rating = Some(n as u8),
            _ => {}
        }
        macro_rules! copy {
            ($($field:ident),*) => {$(
                if let Some(v) = &self.$field {
                    meta.$field = Some(v.clone());
                }
            )*};
        }
        copy!(
            label,
            title,
            caption,
            rights,
            usage_terms,
            web_statement,
            credit,
            source,
            headline,
            instructions,
            sublocation,
            city,
            region,
            country,
            country_code,
            event
        );
        if !self.creator.is_empty() {
            meta.creator = self.creator.clone();
        }
        if !self.persons.is_empty() {
            meta.persons = self.persons.clone();
        }
    }
}
