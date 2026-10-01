// SPDX-License-Identifier: GPL-3.0-or-later
//! The lookup, on a small file the test writes: two regions of one country with an enclave in the middle
//! (a country of its own), a country across the antimeridian, towns on both sides of a border (design note
//! 008 §3, §6).

use auroraw_places::{
    FORMAT, Level, NewArea, NewPlace, Options, PackBuilder, Places, PlacesError, Summary,
};
use auroraw_testkit::{TempDir, temp_dir};

type Ring = Vec<(f64, f64)>;

fn square(x0: f64, x1: f64, y0: f64, y1: f64) -> Ring {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)]
}

fn area(level: Level, code: &str, en: &str, fr: Option<&str>, parts: Vec<Vec<Ring>>) -> NewArea {
    let mut names = vec![("en".to_string(), en.to_string())];
    names.extend(fr.map(|fr| ("fr".to_string(), fr.to_string())));
    NewArea {
        level,
        code: code.into(),
        country_code: code.split('-').next().unwrap().into(),
        parent: None,
        name: en.into(),
        names,
        geonames_id: None,
        parts,
    }
}

fn town(id: i64, name: &str, lat: f64, lon: f64, country: &str) -> NewPlace {
    sized(id, name, lat, lon, country, 5_000)
}

fn sized(id: i64, name: &str, lat: f64, lon: f64, country: &str, population: u64) -> NewPlace {
    NewPlace {
        id,
        name: name.into(),
        lat,
        lon,
        country_code: country.into(),
        population,
        section: false,
    }
}

/// Aland (0..10 both ways, with a hole where the enclave Benclave is), cut in two regions along lon 5;
/// Cland across the antimeridian (170..180 and -180..-170), one region.
fn fixture() -> (TempDir, Summary, Places) {
    let dir = temp_dir();
    let path = dir.path().join("places.sqlite");
    let mut pack = PackBuilder::create(&path).unwrap();
    let hole = square(4.0, 6.0, 4.0, 6.0);

    let aland = pack
        .add_area(&area(
            Level::Country,
            "AA",
            "Aland",
            Some("Alandie"),
            vec![vec![square(0.0, 10.0, 0.0, 10.0), hole.clone()]],
        ))
        .unwrap();
    let mut west = area(
        Level::Region,
        "AA-W",
        "West",
        Some("Ouest"),
        vec![vec![square(0.0, 5.0, 0.0, 10.0), hole.clone()]],
    );
    west.parent = Some(aland);
    pack.add_area(&west).unwrap();
    let mut east = area(
        Level::Region,
        "AA-E",
        "East",
        Some("Est"),
        vec![vec![square(5.0, 10.0, 0.0, 10.0), hole.clone()]],
    );
    east.parent = Some(aland);
    pack.add_area(&east).unwrap();

    let benclave = pack
        .add_area(&area(
            Level::Country,
            "BB",
            "Benclave",
            None,
            vec![vec![hole.clone()]],
        ))
        .unwrap();
    let mut only = area(Level::Region, "BB-1", "Only", None, vec![vec![hole]]);
    only.parent = Some(benclave);
    pack.add_area(&only).unwrap();

    let both = vec![
        vec![square(170.0, 180.0, 0.0, 10.0)],
        vec![square(-180.0, -170.0, 0.0, 10.0)],
    ];
    let cland = pack
        .add_area(&area(Level::Country, "CC", "Cland", None, both.clone()))
        .unwrap();
    let mut coast = area(Level::Region, "CC-1", "Coast", None, both);
    coast.parent = Some(cland);
    pack.add_area(&coast).unwrap();

    for place in [
        town(1, "Westville", 5.0, 2.0, "AA"),
        town(2, "Eastburg", 5.0, 8.0, "AA"),
        town(3, "Borderton", 2.0, 5.1, "AA"),
        town(4, "Enclavia", 5.0, 5.0, "BB"),
        town(5, "Antipodes", 5.0, 179.9, "CC"),
        town(6, "Wrapville", 3.0, -179.9, "CC"),
        // On an island the polygons do not show: found by its country code.
        town(7, "Skerry", 20.0, 20.0, "AA"),
        // Nowhere at all.
        town(8, "Nowhere", -40.0, 0.0, "ZZ"),
        // A neighbourhood and the city 6 km from it, in the far north-west.
        NewPlace {
            section: true,
            ..sized(12, "Quarter", 9.5, 0.5, "AA", 60_000)
        },
        sized(13, "Cityville", 9.5, 0.5 + 6.0 / 109.6, "AA", 300_000),
        // A borough, the city it is part of 2.8 km east, and a metropolis 16.6 km east.
        sized(9, "Borough", 7.0, 1.0, "AA", 5_000),
        sized(10, "Bigtown", 7.0, 1.025, "AA", 900_000),
        sized(11, "Metropolis", 7.0, 1.15, "AA", 2_000_000),
    ] {
        pack.add_place(&place).unwrap();
    }
    pack.set_meta("source", "a fixture").unwrap();
    let summary = pack.finish().unwrap();
    let places = Places::open(&path).unwrap();
    (dir, summary, places)
}

#[test]
fn the_summary_counts_what_was_written_and_what_no_polygon_holds() {
    let (_dir, summary, places) = fixture();
    assert_eq!(summary.areas, 7);
    assert_eq!(summary.parts, 1 + 1 + 1 + 1 + 1 + 2 + 2);
    assert_eq!(summary.places, 13);
    assert_eq!(summary.places_without_region, 2, "Skerry and Nowhere");
    assert_eq!(summary.places_without_country, 1, "Nowhere");
    let info = places.info().unwrap();
    assert_eq!((info.format, info.areas, info.places), (FORMAT, 7, 13));
    assert!(info.meta.contains(&("source".into(), "a fixture".into())));
}

#[test]
fn a_point_is_given_its_country_its_region_and_the_nearest_town_of_that_region() {
    let (_dir, _, places) = fixture();
    let here = places.locate(5.05, 2.05, "en").unwrap();
    let country = here.country.expect("a country");
    let region = here.region.expect("a region");
    assert_eq!(
        (country.code.as_str(), country.name.as_str()),
        ("AA", "Aland")
    );
    assert_eq!(
        (region.code.as_str(), region.name.as_str()),
        ("AA-W", "West")
    );
    let city = here.city.expect("a town");
    assert_eq!(city.name, "Westville");
    assert!(
        (city.distance_m - 7_800.0).abs() < 500.0,
        "{}",
        city.distance_m
    );
}

#[test]
fn the_town_across_a_border_is_not_the_points_town() {
    let (_dir, _, places) = fixture();
    // Borderton is 16 km away, but in the other region: the point, in the West, has no town of its own
    // within reach, and says so.
    let near_the_border = places.locate(2.0, 4.95, "en").unwrap();
    assert_eq!(near_the_border.region.unwrap().name, "West");
    assert_eq!(near_the_border.country.unwrap().name, "Aland");
    assert_eq!(near_the_border.city, None);
    // The same point, looking at the East from the other side of the line, finds it.
    let across = places.locate(2.0, 5.05, "en").unwrap();
    assert_eq!(across.region.unwrap().name, "East");
    assert_eq!(across.city.unwrap().name, "Borderton");
}

#[test]
fn an_enclave_is_its_own_country_and_the_hole_is_not_the_surrounding_one() {
    let (_dir, _, places) = fixture();
    let inside = places.locate(5.0, 5.0, "en").unwrap();
    assert_eq!(inside.country.unwrap().name, "Benclave");
    assert_eq!(inside.region.unwrap().name, "Only");
    assert_eq!(inside.city.unwrap().name, "Enclavia");
}

#[test]
fn a_shore_is_forgiven_a_few_kilometres_and_the_open_sea_has_no_country() {
    let (_dir, _, places) = fixture();
    // 2.2 km west of Aland's western edge: a beach, a harbour.
    let beach = places.locate(5.0, -0.02, "en").unwrap();
    assert_eq!(beach.region.unwrap().name, "West");
    assert_eq!(beach.country.unwrap().name, "Aland");
    // 22 km out: the sea.
    let sea = places.locate(5.0, -0.2, "en").unwrap();
    assert!(sea.is_empty(), "{sea:?}");
    assert!(places.locate(5.0, 15.0, "en").unwrap().is_empty());
    // With no tolerance the beach is not land either.
    let (dir, _, _) = fixture();
    let strict = Places::open(&dir.path().join("places.sqlite"))
        .unwrap()
        .with_options(Options {
            coastal_tolerance_m: 0.0,
            ..Options::default()
        });
    assert!(strict.locate(5.0, -0.02, "en").unwrap().is_empty());
}

#[test]
fn a_country_across_the_antimeridian_is_one_country_and_its_towns_are_neighbours() {
    let (_dir, _, places) = fixture();
    let east_of_the_line = places.locate(5.0, 179.95, "en").unwrap();
    assert_eq!(east_of_the_line.country.unwrap().name, "Cland");
    assert_eq!(east_of_the_line.city.unwrap().name, "Antipodes");
    // 12 km from Wrapville across the line, 222 km from Antipodes.
    let across = places.locate(3.0, 179.99, "en").unwrap();
    let city = across.city.expect("a town across the antimeridian");
    assert_eq!(city.name, "Wrapville");
    assert!(
        (city.distance_m - 12_300.0).abs() < 600.0,
        "{}",
        city.distance_m
    );
    // 185 is -175.
    assert_eq!(
        places
            .locate(5.0, 185.0, "en")
            .unwrap()
            .country
            .unwrap()
            .name,
        "Cland"
    );
}

#[test]
fn a_point_far_from_any_town_still_has_its_country_and_its_region() {
    let (_dir, _, places) = fixture();
    let far = places.locate(9.0, 9.0, "en").unwrap();
    assert_eq!(far.country.unwrap().name, "Aland");
    assert_eq!(far.region.unwrap().name, "East");
    assert_eq!(far.city, None);
}

#[test]
fn names_come_in_the_asked_language_then_english_then_their_own() {
    let (_dir, _, places) = fixture();
    let french = places.locate(5.05, 2.05, "fr").unwrap();
    assert_eq!(french.country.as_ref().unwrap().name, "Alandie");
    assert_eq!(french.region.as_ref().unwrap().name, "Ouest");
    assert_eq!(
        places
            .locate(5.05, 2.05, "fr-CA")
            .unwrap()
            .country
            .unwrap()
            .name,
        "Alandie",
        "only the language counts"
    );
    // Cland has no French: English.
    assert_eq!(
        places
            .locate(5.0, 179.95, "fr")
            .unwrap()
            .country
            .unwrap()
            .name,
        "Cland"
    );
    // A language the file never had: English.
    assert_eq!(
        places
            .locate(5.05, 2.05, "xx")
            .unwrap()
            .country
            .unwrap()
            .name,
        "Aland"
    );
}

#[test]
fn a_position_that_is_not_one_is_an_error_not_an_empty_answer() {
    let (_dir, _, places) = fixture();
    for (lat, lon) in [
        (91.0, 0.0),
        (-90.5, 0.0),
        (f64::NAN, 0.0),
        (0.0, f64::INFINITY),
    ] {
        assert!(
            matches!(
                places.locate(lat, lon, "en"),
                Err(PlacesError::Position { .. })
            ),
            "{lat} {lon}"
        );
    }
}

#[test]
fn a_file_that_is_not_places_or_is_of_a_newer_layout_is_refused() {
    let dir = temp_dir();
    let other = dir.path().join("other.sqlite");
    rusqlite::Connection::open(&other)
        .unwrap()
        .execute_batch("CREATE TABLE something(x)")
        .unwrap();
    assert!(matches!(
        Places::open(&other),
        Err(PlacesError::Format { found: 0, .. })
    ));

    let newer = dir.path().join("newer.sqlite");
    let mut pack = PackBuilder::create(&newer).unwrap();
    pack.set_meta("format", "99").unwrap();
    pack.finish().unwrap();
    assert!(matches!(
        Places::open(&newer),
        Err(PlacesError::Format {
            found: 99,
            supported: FORMAT
        })
    ));
    assert!(matches!(
        PackBuilder::create(&newer),
        Err(PlacesError::Exists(_))
    ));
    assert!(Places::open(&dir.path().join("missing.sqlite")).is_err());
}

#[test]
fn a_city_claims_the_points_around_it_and_a_borough_does_not_keep_them() {
    let (dir, _, places) = fixture();
    // On the borough (5,000 inhabitants, 700 m of reach): the city (900,000, 9.5 km of reach) is 2.8 km
    // away and claims the point; the metropolis, 16.6 km away, reaches 14 km and does not.
    let downtown = places.locate(7.0, 1.0, "en").unwrap();
    assert_eq!(downtown.city.unwrap().name, "Bigtown");
    // Beside the metropolis, it is the one that claims the point (the city, 14 km off, reaches 9.5 km).
    let beside = places.locate(7.0, 1.14, "en").unwrap();
    assert_eq!(beside.city.unwrap().name, "Metropolis");
    // With no reach the nearest is the nearest, borough or not.
    let strict = Places::open(&dir.path().join("places.sqlite"))
        .unwrap()
        .with_options(Options {
            town_reach_m: 0.0,
            ..Options::default()
        });
    assert_eq!(
        strict.locate(7.0, 1.0, "en").unwrap().city.unwrap().name,
        "Borough"
    );
}

#[test]
fn a_neighbourhood_gives_way_to_the_city_near_it_but_stands_alone_when_there_is_none() {
    let (dir, _, places) = fixture();
    // On the neighbourhood (a GeoNames "section of a populated place"), the city is 6 km away: the city.
    assert_eq!(
        places.locate(9.5, 0.5, "en").unwrap().city.unwrap().name,
        "Cityville"
    );
    // Where no city is within reach, the neighbourhood is the answer: better than none.
    let narrow = Places::open(&dir.path().join("places.sqlite"))
        .unwrap()
        .with_options(Options {
            town_radius_m: 3_000.0,
            ..Options::default()
        });
    assert_eq!(
        narrow.locate(9.5, 0.5, "en").unwrap().city.unwrap().name,
        "Quarter"
    );
}

/// Two countries side by side, for the borders the generalised polygons blur: `Xland`, with a region of its
/// own, and `Yland` beside it, two kilometres across and with no region at all, with a town in it.
fn neighbours() -> (TempDir, Places) {
    let dir = temp_dir();
    let path = dir.path().join("places.sqlite");
    let mut pack = PackBuilder::create(&path).unwrap();
    let x = pack
        .add_area(&area(
            Level::Country,
            "XL",
            "Xland",
            None,
            vec![vec![square(0.0, 1.0, 0.0, 1.0)]],
        ))
        .unwrap();
    let mut region = area(
        Level::Region,
        "XL-1",
        "Xregion",
        None,
        vec![vec![square(0.0, 1.0, 0.0, 1.0)]],
    );
    region.parent = Some(x);
    pack.add_area(&region).unwrap();
    // 1.00..1.02 by 0.40..0.42: touching Xland's region, and with no region layer of its own.
    pack.add_area(&area(
        Level::Country,
        "YL",
        "Yland",
        None,
        vec![vec![square(1.0, 1.02, 0.40, 0.42)]],
    ))
    .unwrap();
    pack.add_place(&town(1, "Yburg", 0.41, 1.01, "YL")).unwrap();
    pack.finish().unwrap();
    (dir, Places::open(&path).unwrap())
}

#[test]
fn a_country_without_regions_beside_a_region_is_not_taken_for_the_neighbour() {
    // Review of the places crate (Bob), point 1. The middle of Yland is 1.1 km from Xland's region, well
    // within the coastal tolerance; the country is decided by containment first, and the tolerance only
    // serves a point that is in no country.
    let (_dir, places) = neighbours();
    let yland = places.locate(0.41, 1.01, "en").unwrap();
    assert_eq!(yland.country.unwrap().name, "Yland");
    assert!(yland.region.is_none(), "Yland has no region");
    assert_eq!(yland.city.unwrap().name, "Yburg");
    // The other side of the same border is still Xland, and still in its region.
    let xland = places.locate(0.41, 0.995, "en").unwrap();
    assert_eq!(xland.country.unwrap().name, "Xland");
    assert_eq!(xland.region.unwrap().name, "Xregion");
}

/// A country whose region stops short of its coast, with a harbour in the strip the region does not cover and a
/// town of the next country that has no region either.
fn harbour() -> (TempDir, Places) {
    let dir = temp_dir();
    let path = dir.path().join("places.sqlite");
    let mut pack = PackBuilder::create(&path).unwrap();
    let x = pack
        .add_area(&area(
            Level::Country,
            "XL",
            "Xland",
            None,
            vec![vec![square(0.0, 1.2, 0.0, 1.0)]],
        ))
        .unwrap();
    let mut region = area(
        Level::Region,
        "XL-1",
        "Xregion",
        None,
        vec![vec![square(0.0, 1.0, 0.0, 1.0)]],
    );
    region.parent = Some(x);
    pack.add_area(&region).unwrap();
    pack.add_area(&area(
        Level::Country,
        "ZL",
        "Zland",
        None,
        vec![vec![square(1.2, 2.2, 0.0, 1.0)]],
    ))
    .unwrap();
    // In Xland but outside every region of it: 300 m past the region's edge.
    pack.add_place(&town(1, "Harbour", 0.70, 1.003, "XL"))
        .unwrap();
    // In Zland, which has no region either.
    pack.add_place(&town(2, "Zport", 0.20, 1.21, "ZL")).unwrap();
    let summary = pack.finish().unwrap();
    assert_eq!(summary.places_without_region, 2);
    (dir, Places::open(&path).unwrap())
}

#[test]
fn a_town_that_no_region_contains_is_found_from_a_point_in_its_countrys_region() {
    // Review of the places crate (Bob), point 2: the towns outside the generalised polygons are mostly the
    // coastal ones (harbours, beaches, islands), and a photo that has a region could not see them.
    let (_dir, places) = harbour();
    let found = places.locate(0.70, 0.995, "en").unwrap();
    assert_eq!(found.region.unwrap().name, "Xregion");
    assert_eq!(found.city.unwrap().name, "Harbour");
}

#[test]
fn a_town_with_no_region_is_still_a_town_of_its_own_country_only() {
    // The border guard stays: Zport is 23.9 km from this point and has no region, but it is Zland's.
    let (_dir, places) = harbour();
    let found = places.locate(0.20, 0.995, "en").unwrap();
    assert_eq!(found.country.unwrap().name, "Xland");
    assert!(found.city.is_none(), "no town of Xland within 25 km");
    // And from its own side it is found.
    let zland = places.locate(0.20, 1.25, "en").unwrap();
    assert_eq!(zland.city.unwrap().name, "Zport");
}
