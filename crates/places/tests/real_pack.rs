// SPDX-License-Identifier: GPL-3.0-or-later
//! The lookup on the real file, built from Natural Earth and GeoNames by `tools/fetch-places.sh` and
//! `examples/build-places.rs`. The file is not in git: the tests are skipped without it (set
//! `AUR_PLACES_PACK` to its path, or leave it in `testdata/places/places.sqlite`), and they are the
//! measurement of design note 008 §2 and §3 on real boundaries.

use std::path::PathBuf;
use std::time::Instant;

use auroraw_places::Places;

fn pack() -> Option<Places> {
    let path = std::env::var_os("AUR_PLACES_PACK")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/places/places.sqlite")
        });
    if !path.exists() {
        eprintln!("skipped: no file of places at {}", path.display());
        return None;
    }
    Some(Places::open(&path).expect("the file opens"))
}

struct Case {
    what: &'static str,
    lat: f64,
    lon: f64,
    country: &'static str,
    region: &'static str,
    city: Option<&'static str>,
}

#[test]
fn known_places_get_their_country_region_and_town() {
    let Some(places) = pack() else { return };
    let cases = [
        Case {
            what: "Montréal",
            lat: 45.5019,
            lon: -73.5674,
            country: "CA",
            region: "CA-QC",
            city: Some("Montréal"),
        },
        Case {
            what: "Sydney",
            lat: -33.8688,
            lon: 151.2093,
            country: "AU",
            region: "AU-NSW",
            city: Some("Sydney"),
        },
        Case {
            what: "Paris",
            lat: 48.8566,
            lon: 2.3522,
            country: "FR",
            region: "",
            city: Some("Paris"),
        },
        Case {
            what: "Reykjavík",
            lat: 64.1466,
            lon: -21.9426,
            country: "IS",
            region: "",
            city: Some("Reykjavík"),
        },
        Case {
            what: "Copacabana beach",
            lat: -22.9711,
            lon: -43.1822,
            country: "BR",
            region: "BR-RJ",
            city: Some("Rio de Janeiro"),
        },
        Case {
            what: "Windsor, Ontario, across the river from Detroit",
            lat: 42.3149,
            lon: -83.0364,
            country: "CA",
            region: "CA-ON",
            city: Some("Windsor"),
        },
        Case {
            what: "Detroit, Michigan, across the river from Windsor",
            lat: 42.3314,
            lon: -83.0458,
            country: "US",
            region: "US-MI",
            city: Some("Detroit"),
        },
        Case {
            what: "Taveuni, Fiji, east of the antimeridian",
            lat: -16.85,
            lon: -179.97,
            country: "FJ",
            region: "",
            city: None,
        },
        Case {
            what: "Chukotka, Russia, west of the antimeridian",
            lat: 66.0,
            lon: 179.5,
            country: "RU",
            region: "RU-CHU",
            city: None,
        },
        Case {
            what: "Chukotka, Russia, east of the antimeridian",
            lat: 66.0,
            lon: -175.0,
            country: "RU",
            region: "RU-CHU",
            city: None,
        },
    ];
    for case in cases {
        let found = places.locate(case.lat, case.lon, "en").unwrap();
        let country = found.country.as_ref().map(|c| c.code.as_str());
        assert_eq!(country, Some(case.country), "{}: {found:?}", case.what);
        if !case.region.is_empty() {
            let region = found.region.as_ref().map(|r| r.code.as_str());
            assert_eq!(region, Some(case.region), "{}: {found:?}", case.what);
        }
        if let Some(city) = case.city {
            assert_eq!(
                found.city.as_ref().map(|t| t.name.as_str()),
                Some(city),
                "{}: {found:?}",
                case.what
            );
        }
    }
}

#[test]
fn names_come_in_the_language_asked_for() {
    let Some(places) = pack() else { return };
    let quebec = places.locate(46.8139, -71.2080, "fr").unwrap();
    assert_eq!(quebec.country.unwrap().name, "Canada");
    assert_eq!(quebec.region.as_ref().unwrap().name, "Québec");
    let english = places.locate(46.8139, -71.2080, "en").unwrap();
    assert_eq!(english.region.unwrap().name, "Quebec");
    let germany = places.locate(52.52, 13.405, "fr").unwrap();
    assert_eq!(germany.country.unwrap().name, "Allemagne");
}

#[test]
fn the_open_sea_and_the_far_north_have_no_country() {
    let Some(places) = pack() else { return };
    assert!(
        places.locate(0.0, -30.0, "en").unwrap().is_empty(),
        "mid-Atlantic"
    );
    assert!(
        places.locate(90.0, 0.0, "en").unwrap().is_empty(),
        "the North Pole"
    );
    let antarctica = places.locate(-80.0, 0.0, "en").unwrap();
    assert!(
        antarctica.country.is_some(),
        "Antarctica is on the map: {antarctica:?}"
    );
}

#[test]
fn ten_thousand_lookups_take_seconds_not_minutes() {
    let Some(places) = pack() else { return };
    let mut random = fastrand::Rng::with_seed(1);
    // Positions spread over the inhabited latitudes: land and sea alike.
    let started = Instant::now();
    let mut found = 0;
    for _ in 0..10_000 {
        let lat = random.f64() * 120.0 - 50.0;
        let lon = random.f64() * 360.0 - 180.0;
        found += usize::from(!places.locate(lat, lon, "en").unwrap().is_empty());
    }
    let elapsed = started.elapsed().as_secs_f64();
    eprintln!("10,000 random positions: {found} on land or near a shore, {elapsed:.2} s");
    assert!(elapsed < 120.0, "{elapsed} s");
}
