// SPDX-License-Identifier: GPL-3.0-or-later
//! The three-way comparison of design note 003 §8.1: for each field, what Auroraw last read from the
//! file (the **base**), what the file says now, and what the photo says now (**mine**). Pure: it
//! decides nothing about files, keywords' identifiers or history, only which side changed.

use std::collections::BTreeMap;

use super::fields::{Fields, keyword_key, normalise_path};
use crate::sidecar::Metadata;

/// One field of [`Fields`] that can change on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    /// The rating axis (stars and the rejected mark).
    Rating,
    /// The colour label.
    Label,
    /// The title.
    Title,
    /// The caption.
    Caption,
    /// The creators.
    Creator,
    /// The copyright notice.
    Rights,
    /// The usage terms.
    UsageTerms,
    /// The web statement of rights.
    WebStatement,
    /// The credit line.
    Credit,
    /// The source.
    Source,
    /// The headline.
    Headline,
    /// The instructions.
    Instructions,
    /// The sublocation.
    Sublocation,
    /// The city.
    City,
    /// The region or state.
    Region,
    /// The country.
    Country,
    /// The ISO country code.
    CountryCode,
    /// The persons shown.
    Persons,
    /// The event.
    Event,
    /// The keywords (merged as sets, never in conflict).
    Keywords,
}

impl Field {
    /// Every field, in the order they are listed to a person.
    pub const ALL: [Field; 20] = [
        Field::Rating,
        Field::Label,
        Field::Title,
        Field::Caption,
        Field::Keywords,
        Field::Creator,
        Field::Rights,
        Field::UsageTerms,
        Field::WebStatement,
        Field::Credit,
        Field::Source,
        Field::Headline,
        Field::Instructions,
        Field::Sublocation,
        Field::City,
        Field::Region,
        Field::Country,
        Field::CountryCode,
        Field::Persons,
        Field::Event,
    ];

    /// A stable ASCII key (the metadata panel's own keys for the text fields), for an interface.
    pub fn key(self) -> &'static str {
        match self {
            Field::Rating => "rating",
            Field::Label => "label",
            Field::Title => "title",
            Field::Caption => "caption",
            Field::Creator => "creator",
            Field::Rights => "rights",
            Field::UsageTerms => "usage-terms",
            Field::WebStatement => "web-statement",
            Field::Credit => "credit",
            Field::Source => "source",
            Field::Headline => "headline",
            Field::Instructions => "instructions",
            Field::Sublocation => "sublocation",
            Field::City => "city",
            Field::Region => "region",
            Field::Country => "country",
            Field::CountryCode => "country-code",
            Field::Persons => "persons",
            Field::Event => "event",
            Field::Keywords => "keywords",
        }
    }

    /// The field a key names, the reverse of [`Field::key`].
    pub fn parse(key: &str) -> Option<Field> {
        Field::ALL.into_iter().find(|f| f.key() == key)
    }
}

impl Fields {
    /// A field's value as text, the form two sides are compared in: `""` when there is none, lists
    /// with a line each, the rating as `-1` to `5`.
    pub fn get(&self, field: Field) -> String {
        let one = |v: &Option<String>| v.clone().unwrap_or_default();
        match field {
            Field::Rating => self.rating.map(|r| r.to_string()).unwrap_or_default(),
            Field::Label => one(&self.label),
            Field::Title => one(&self.title),
            Field::Caption => one(&self.caption),
            Field::Creator => self.creator.join("\n"),
            Field::Rights => one(&self.rights),
            Field::UsageTerms => one(&self.usage_terms),
            Field::WebStatement => one(&self.web_statement),
            Field::Credit => one(&self.credit),
            Field::Source => one(&self.source),
            Field::Headline => one(&self.headline),
            Field::Instructions => one(&self.instructions),
            Field::Sublocation => one(&self.sublocation),
            Field::City => one(&self.city),
            Field::Region => one(&self.region),
            Field::Country => one(&self.country),
            Field::CountryCode => one(&self.country_code),
            Field::Persons => self.persons.join("\n"),
            Field::Event => one(&self.event),
            Field::Keywords => self.keywords.join("\n"),
        }
    }

    /// Puts this field of `self` (the file's fields, say) into `meta`, the photo's metadata. **Not the
    /// keywords** (they need identifiers). The rating axis is applied the way the photo keeps it: `-1`
    /// sets the Rejected flag and keeps the stars (spec §5.3), `1` to `5` sets the stars and lifts a
    /// Rejected flag, unset clears the stars and lifts it; a Picked flag is never touched, except by a
    /// rejection.
    pub fn apply_field(&self, field: Field, meta: &mut Metadata) {
        match field {
            Field::Rating => match self.rating {
                Some(-1) => meta.flag = Some(crate::sidecar::Flag::Rejected),
                other => {
                    meta.rating = other.map(|n| n as u8);
                    if meta.flag == Some(crate::sidecar::Flag::Rejected) {
                        meta.flag = None;
                    }
                }
            },
            Field::Label => meta.label = self.label.clone(),
            Field::Title => meta.title = self.title.clone(),
            Field::Caption => meta.caption = self.caption.clone(),
            Field::Creator => meta.creator = self.creator.clone(),
            Field::Rights => meta.rights = self.rights.clone(),
            Field::UsageTerms => meta.usage_terms = self.usage_terms.clone(),
            Field::WebStatement => meta.web_statement = self.web_statement.clone(),
            Field::Credit => meta.credit = self.credit.clone(),
            Field::Source => meta.source = self.source.clone(),
            Field::Headline => meta.headline = self.headline.clone(),
            Field::Instructions => meta.instructions = self.instructions.clone(),
            Field::Sublocation => meta.sublocation = self.sublocation.clone(),
            Field::City => meta.city = self.city.clone(),
            Field::Region => meta.region = self.region.clone(),
            Field::Country => meta.country = self.country.clone(),
            Field::CountryCode => meta.country_code = self.country_code.clone(),
            Field::Persons => meta.persons = self.persons.clone(),
            Field::Event => meta.event = self.event.clone(),
            Field::Keywords => {}
        }
    }
}

/// A field the file changed and the photo did not: the file's value is taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange {
    /// Which field.
    pub field: Field,
    /// The photo's value (unchanged since the base).
    pub mine: String,
    /// The file's new value.
    pub file: String,
}

/// A field changed on both sides to different values: the photographer settles it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// Which field.
    pub field: Field,
    /// What was last read from the file; `None` when there is no base (a two-way comparison).
    pub base: Option<String>,
    /// The photo's value.
    pub mine: String,
    /// The file's value.
    pub file: String,
}

/// The keywords the file added or removed since the base, that the photo does not already agree with,
/// as paths.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeywordMerge {
    /// Keywords the file gained, in the file's spelling.
    pub add: Vec<String>,
    /// Keywords the file lost that the photo still has, in the photo's spelling.
    pub remove: Vec<String>,
}

/// What a comparison found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Merge {
    /// Fields changed only in the file: taken.
    pub taken: Vec<FieldChange>,
    /// Fields changed on both sides to different values.
    pub conflicts: Vec<Conflict>,
    /// The keyword sets, merged against the base.
    pub keywords: KeywordMerge,
    /// Fields the file changed to exactly what the photo already says: nothing to do.
    pub converged: Vec<Field>,
}

impl Merge {
    /// Whether there is nothing to take and nothing to ask.
    pub fn is_empty(&self) -> bool {
        self.taken.is_empty()
            && self.conflicts.is_empty()
            && self.keywords.add.is_empty()
            && self.keywords.remove.is_empty()
    }
}

fn key_map(paths: &[String]) -> BTreeMap<String, String> {
    paths
        .iter()
        .filter_map(|p| normalise_path(p))
        .map(|p| (keyword_key(&p), p))
        .collect()
}

/// Compares the file's `fields` with the `base` (what was last read from it) and with `mine` (the
/// photo's own, through [`Fields::from_metadata`]).
///
/// Per field, comparing the values as text: unchanged in the file since the base → nothing (the photo's
/// value stands); changed in the file and unchanged in the photo → **taken**; the file changed to what the
/// photo already says → **converged**; otherwise a **conflict**. With no base (`None`) every field that
/// differs is a conflict. Keywords are sets against the base: those the file added that the photo lacks
/// are offered, those it removed that the photo still has are offered, and they never conflict.
pub fn merge(base: Option<&Fields>, file: &Fields, mine: &Fields) -> Merge {
    let mut out = Merge::default();
    for field in Field::ALL {
        if field == Field::Keywords {
            continue;
        }
        let (file_v, mine_v) = (file.get(field), mine.get(field));
        match base.map(|b| b.get(field)) {
            Some(base_v) if file_v == base_v => {}
            Some(base_v) if mine_v == base_v => out.taken.push(FieldChange {
                field,
                mine: mine_v,
                file: file_v,
            }),
            Some(_) if mine_v == file_v => out.converged.push(field),
            Some(base_v) => out.conflicts.push(Conflict {
                field,
                base: Some(base_v),
                mine: mine_v,
                file: file_v,
            }),
            None if file_v == mine_v => {}
            None => out.conflicts.push(Conflict {
                field,
                base: None,
                mine: mine_v,
                file: file_v,
            }),
        }
    }
    let (f, m) = (key_map(&file.keywords), key_map(&mine.keywords));
    match base {
        Some(base) => {
            let b = key_map(&base.keywords);
            out.keywords.add = f
                .iter()
                .filter(|(k, _)| !b.contains_key(*k) && !m.contains_key(*k))
                .map(|(_, path)| path.clone())
                .collect();
            out.keywords.remove = b
                .keys()
                .filter(|k| !f.contains_key(*k))
                .filter_map(|k| m.get(k).cloned())
                .collect();
        }
        None => {
            out.keywords.add = f
                .iter()
                .filter(|(k, _)| !m.contains_key(*k))
                .map(|(_, path)| path.clone())
                .collect();
        }
    }
    out
}
