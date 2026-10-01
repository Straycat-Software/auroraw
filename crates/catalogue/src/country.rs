// SPDX-License-Identifier: GPL-3.0-or-later
//! The key of a photo's country (design note 008 §5.1).
//!
//! A country is written by several hands: Auroraw writes the name and the ISO code (`Canada`, `CA`), another application
//! often writes the name alone, a person writes what they like (`canada`, `CAN`, `DE`). The place filter must see one
//! country in them. The key is therefore **the photo's ISO 3166-1 alpha-2 code, found from the photo's own fields and
//! nothing else**: its code field if that says a country (`CA`, or `CAN`, the alpha-3 code the IPTC standard also
//! allows), else its name field looked up in a fixed table of the names countries go by (`Canada`, `Allemagne`,
//! `United States`, `USA`), else the code field as written, else the name folded. Nothing depends on the other photos,
//! so a rebuild of the catalogue gives the same keys in any order.
//!
//! The table is `country_names.tsv`, made by the `country-names` example of the `auroraw-places` crate from Natural
//! Earth (public domain), in English and French and in the languages whose users' tools write the country in the
//! language of the system (German, Spanish, Italian, Portuguese, Dutch), with the names people still write. A name
//! the table does not have (`Tyskland`, a country in a language it does not carry) stays a node of its own, under its
//! folded name, as it was before the table. The table can only join nodes that are one country: a name two countries
//! answer to is not in it (`Saint-Martin`), nor a name that Natural Earth gives to one and people use for two (`Congo`).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::place::fold_place;

/// The table: one country a line, `alpha-2 TAB alpha-3 TAB name TAB name…`, `#` lines being comments.
const TABLE: &str = include_str!("country_names.tsv");

struct Countries {
    /// The alpha-2 codes.
    codes: HashSet<&'static str>,
    /// Alpha-3 code to alpha-2.
    by_alpha3: HashMap<&'static str, &'static str>,
    /// Folded name to alpha-2. A name two countries share is not in it.
    by_name: HashMap<String, &'static str>,
}

fn countries() -> &'static Countries {
    static TABLE_OF_COUNTRIES: OnceLock<Countries> = OnceLock::new();
    TABLE_OF_COUNTRIES.get_or_init(|| {
        let mut codes = HashSet::new();
        let mut by_alpha3 = HashMap::new();
        // `None`: two countries claim the name.
        let mut names: HashMap<String, Option<&'static str>> = HashMap::new();
        for line in TABLE
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let mut fields = line.split('\t');
            let (Some(code), Some(alpha3)) = (fields.next(), fields.next()) else {
                continue;
            };
            codes.insert(code);
            if !alpha3.is_empty() {
                by_alpha3.insert(alpha3, code);
            }
            for name in fields {
                let folded = fold_place(name);
                if folded.is_empty() {
                    continue;
                }
                match names.get(&folded) {
                    Some(Some(owner)) if *owner != code => {
                        names.insert(folded, None);
                    }
                    Some(_) => {}
                    None => {
                        names.insert(folded, Some(code));
                    }
                }
            }
        }
        Countries {
            codes,
            by_alpha3,
            by_name: names
                .into_iter()
                .filter_map(|(name, owner)| Some((name, owner?)))
                .collect(),
        }
    })
}

/// The alpha-2 code of the country that `text` says: a code (two letters, or three for the alpha-3 code, in any
/// case) or a name of a country, folded as the place filter folds. `None` for text that is neither.
pub(crate) fn code_of(text: &str) -> Option<&'static str> {
    let text = text.trim();
    let table = countries();
    let letters = text.chars().all(|c| c.is_ascii_alphabetic());
    let upper = text.to_ascii_uppercase();
    if letters
        && text.len() == 2
        && let Some(code) = table.codes.get(upper.as_str())
    {
        return Some(code);
    }
    if letters
        && text.len() == 3
        && let Some(code) = table.by_alpha3.get(upper.as_str())
    {
        return Some(code);
    }
    table.by_name.get(&fold_place(text)).copied()
}

/// The key of a country from the photo's code field and name field (both already trimmed, `None` when empty): see the
/// module documentation. `None` when the photo says no country.
pub(crate) fn country_key(code: Option<&str>, name: Option<&str>) -> Option<String> {
    if let Some(found) = code.and_then(code_of).or_else(|| name.and_then(code_of)) {
        return Some(found.to_string());
    }
    code.map(str::to_uppercase)
        .or_else(|| name.map(fold_place).filter(|key| !key.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_well_formed() {
        let mut seen = HashSet::new();
        for line in TABLE
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let fields: Vec<&str> = line.split('\t').collect();
            assert!(fields.len() >= 3, "a code, an alpha-3 and a name: {line:?}");
            assert!(
                fields[0].len() == 2 && fields[0].chars().all(|c| c.is_ascii_uppercase()),
                "{line:?}"
            );
            assert!(
                fields[1].is_empty()
                    || (fields[1].len() == 3 && fields[1].chars().all(|c| c.is_ascii_uppercase())),
                "{line:?}"
            );
            assert!(seen.insert(fields[0]), "{} twice", fields[0]);
        }
        assert!(seen.len() > 230, "{} countries", seen.len());
    }

    #[test]
    fn every_name_of_the_table_finds_its_country_unless_two_countries_share_it() {
        let mut found_none = Vec::new();
        for line in TABLE
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let mut fields = line.split('\t');
            let code = fields.next().unwrap();
            fields.next();
            for name in fields {
                match code_of(name) {
                    Some(found) => assert_eq!(found, code, "{name:?}"),
                    None => found_none.push((code, name)),
                }
            }
        }
        // The two halves of Saint Martin go by the same names, in every language: the island is the one case.
        assert!(!found_none.is_empty());
        for (code, name) in &found_none {
            assert!(
                ["MF", "SX"].contains(code),
                "{name:?} of {code} finds no country, and only the two halves of Saint Martin share their names"
            );
        }
        assert_eq!(code_of("Saint-Martin"), None);
    }

    #[test]
    fn a_country_is_found_by_its_code_its_alpha_3_code_or_a_name_in_any_case_and_spelling() {
        for (text, code) in [
            ("CA", "CA"),
            ("ca", "CA"),
            (" Ca ", "CA"),
            ("CAN", "CA"),
            ("can", "CA"),
            ("Canada", "CA"),
            ("CANADA", "CA"),
            ("DE", "DE"),
            ("DEU", "DE"),
            ("Germany", "DE"),
            ("Allemagne", "DE"),
            ("United States", "US"),
            ("United States of America", "US"),
            ("USA", "US"),
            ("U.S.A.", "US"),
            ("États-Unis", "US"),
            ("Etats Unis", "US"),
            ("UK", "GB"),
            ("Royaume-Uni", "GB"),
            ("Côte d’Ivoire", "CI"),
            ("Cote d'Ivoire", "CI"),
            ("Ivory Coast", "CI"),
            ("Republic of the Congo", "CG"),
            ("Congo-Brazzaville", "CG"),
            ("Dem. Rep. Congo", "CD"),
            ("Democratic Republic of the Congo", "CD"),
            ("Congo-Kinshasa", "CD"),
            ("DR Congo", "CD"),
            // The languages whose users' tools write the country in the language of the system.
            ("Deutschland", "DE"),
            ("Alemanha", "DE"),
            ("Duitsland", "DE"),
            ("España", "ES"),
            ("Spanien", "ES"),
            ("Spagna", "ES"),
            ("République française", "FR"),
            ("Frankreich", "FR"),
            ("Vereinigte Staaten", "US"),
            ("Norway", "NO"),
            ("France", "FR"),
            ("Kosovo", "XK"),
        ] {
            assert_eq!(code_of(text), Some(code), "{text:?}");
        }
        // Nothing, a name that is not one, a name two countries answer to (Saint-Martin), a name Natural Earth gives
        // to one country and people use for two (`Congo`: Brazzaville to the data, Kinshasa to many photographers),
        // and a language the table does not have (Danish).
        for text in [
            "",
            "  ",
            "Atlantis",
            "ZZ",
            "XYZ",
            "Saint-Martin",
            "Congo",
            "Tyskland",
        ] {
            assert_eq!(code_of(text), None, "{text:?}");
        }
    }

    #[test]
    fn the_key_is_the_code_when_the_photo_says_or_implies_one() {
        // Auroraw's own fill, another application's, a hand-typed code, a name in the code field.
        for (code, name) in [
            (Some("CA"), Some("Canada")),
            (None, Some("Canada")),
            (None, Some("canada")),
            (Some("ca"), None),
            (Some("CAN"), None),
            (None, Some("CAN")),
            (Some("Canada"), None),
            (Some("ZZ"), Some("Canada")),
        ] {
            assert_eq!(
                country_key(code, name).as_deref(),
                Some("CA"),
                "{code:?} {name:?}"
            );
        }
        // The code wins over a name that says another country, as it always did.
        assert_eq!(
            country_key(Some("CA"), Some("Mexico")).as_deref(),
            Some("CA")
        );
    }

    #[test]
    fn what_the_table_does_not_know_stays_a_node_of_its_own() {
        assert_eq!(
            country_key(None, Some("Tyskland")).as_deref(),
            Some("tyskland")
        );
        assert_eq!(
            country_key(None, Some("Congo")).as_deref(),
            Some("congo"),
            "a name shared in use is a node of its own, not Brazzaville's"
        );
        assert_eq!(
            country_key(None, Some("Atlantis")).as_deref(),
            Some("atlantis")
        );
        assert_eq!(country_key(Some("zz"), None).as_deref(), Some("ZZ"));
        assert_eq!(
            country_key(Some("zz"), Some("Atlantis")).as_deref(),
            Some("ZZ")
        );
        assert_eq!(country_key(None, None), None);
        assert_eq!(country_key(None, Some(" - ")), None);
    }
}
