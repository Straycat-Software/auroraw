// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading the published data: Natural Earth's GeoJSON and GeoNames' text, on small samples written as
//! those files write them.

use auroraw_places::{Level, sources};

const COUNTRIES: &str = r#"{"type":"FeatureCollection","features":[
 {"type":"Feature","properties":{"NAME":"France","ADMIN":"France","ADM0_A3":"FRA","ISO_A2":"-99","ISO_A2_EH":"FR",
  "NAME_EN":"France","NAME_FR":"France","NAME_DE":"Frankreich"},
  "geometry":{"type":"MultiPolygon","coordinates":[
    [[[0,0],[10,0],[10,10],[0,10],[0,0]],[[4,4],[6,4],[6,6],[4,6],[4,4]]],
    [[[20,20],[21,20],[21,21],[20,21],[20,20]]]]}},
 {"type":"Feature","properties":{"NAME":"Somaliland","ADM0_A3":"SOL","ISO_A2":"-99","ISO_A2_EH":"-99","WB_A2":"-99",
  "NAME_EN":"Somaliland","NAME_FR":""},
  "geometry":{"type":"Polygon","coordinates":[[[40,0],[41,0],[41,1],[40,0]]]}}
]}"#;

const REGIONS: &str = r#"{"type":"FeatureCollection","features":[
 {"type":"Feature","properties":{"name":"Québec","name_en":"Quebec","name_fr":"Québec","iso_a2":"CA",
  "iso_3166_2":"CA-QC","adm0_a3":"CAN","gn_id":6115047},
  "geometry":{"type":"Polygon","coordinates":[[[-80,45],[-60,45],[-60,60],[-80,60],[-80,45]]]}},
 {"type":"Feature","properties":{"name":null,"name_en":null,"iso_a2":"AQ","iso_3166_2":"AQ-X02~","adm0_a3":"ATA"},
  "geometry":{"type":"Polygon","coordinates":[[[0,-80],[1,-80],[1,-79],[0,-80]]]}}
]}"#;

#[test]
fn countries_come_with_their_codes_names_and_every_part() {
    let countries = sources::countries(COUNTRIES, sources::LANGUAGES).unwrap();
    assert_eq!(countries.len(), 2);
    let france = &countries[0];
    assert_eq!(france.key, "FRA");
    assert_eq!(france.area.level, Level::Country);
    // `ISO_A2` is "-99" for France: the `_EH` code is the one.
    assert_eq!(france.area.code, "FR");
    assert_eq!(france.area.country_code, "FR");
    assert_eq!(france.area.name, "France");
    assert!(france.area.names.contains(&("fr".into(), "France".into())));
    assert!(
        !france.area.names.iter().any(|(lang, _)| lang == "de"),
        "only the languages asked for"
    );
    assert_eq!(france.area.parts.len(), 2, "a multipolygon is its polygons");
    assert_eq!(france.area.parts[0].len(), 2, "an outer ring and a hole");
    assert_eq!(france.area.parts[0][1][0], (4.0, 4.0));
    // A country with no code at all, and an empty French name: kept, without them.
    let somaliland = &countries[1];
    assert_eq!(somaliland.area.code, "");
    assert!(!somaliland.area.names.iter().any(|(lang, _)| lang == "fr"));
}

#[test]
fn regions_name_their_country_and_the_nameless_ones_are_left_out() {
    let regions = sources::regions(REGIONS, sources::LANGUAGES).unwrap();
    assert_eq!(
        regions.len(),
        1,
        "a marine area with no name is not a region"
    );
    let quebec = &regions[0];
    assert_eq!(quebec.country_key, "CAN");
    assert_eq!(quebec.area.level, Level::Region);
    assert_eq!(quebec.area.code, "CA-QC");
    assert_eq!(quebec.area.country_code, "CA");
    assert_eq!(quebec.area.geonames_id, Some(6_115_047));
    assert!(quebec.area.names.contains(&("fr".into(), "Québec".into())));
    assert!(quebec.area.names.contains(&("en".into(), "Quebec".into())));
}

#[test]
fn data_that_is_not_what_it_should_be_is_an_error_that_says_so() {
    for bad in [
        "not json",
        "{}",
        r#"{"features":[{"properties":{"NAME":"X"}}]}"#,
    ] {
        assert!(sources::countries(bad, &["en"]).is_err(), "{bad}");
    }
    let bad_geometry = r#"{"features":[{"properties":{"NAME":"X"},"geometry":{"type":"Point","coordinates":[0,0]}}]}"#;
    assert!(sources::countries(bad_geometry, &["en"]).is_err());
}

#[test]
fn geonames_lines_become_towns() {
    let line = "6077243\tMontréal\tMontreal\tMontreal,Montréal\t45.50884\t-73.58781\tP\tPPLA2\tCA\t\t10\t06\t\t\t1600000\t\t216\tAmerica/Toronto\t2024-01-01";
    let towns = sources::places(&format!("{line}\n\n")).unwrap();
    assert_eq!(towns.len(), 1);
    let town = &towns[0];
    assert_eq!((town.id, town.name.as_str()), (6_077_243, "Montréal"));
    assert_eq!((town.lat, town.lon), (45.50884, -73.58781));
    assert_eq!(town.country_code, "CA");
    assert_eq!(town.population, 1_600_000);
    assert!(!town.section);
    assert!(
        sources::places(&line.replace("PPLA2", "PPLX")).unwrap()[0].section,
        "a section of a populated place"
    );
    assert!(sources::places("1\tonly\tthree\n").is_err());
    assert!(sources::places(&line.replace("45.50884", "north")).is_err());
}
