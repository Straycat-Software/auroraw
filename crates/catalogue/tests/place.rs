// SPDX-License-Identifier: GPL-3.0-or-later
//! The place filter and the tree of places (WP10, design note 008 §5): a catalogue whose photos are in known places,
//! written the way several hands write them (case, accents, a code or none), asked for the tree and for what is under
//! each node. The expected answers come from a table of **logical** places the test keeps (a photo is in Montréal
//! whatever way its city is spelt), not from the catalogue's own folding.

use std::collections::BTreeSet;

use auroraw_catalogue::{
    Catalogue, Cursor, Filter, FlagFilter, PlaceFacets, PlaceFilter, PlaceNode, SidecarStat,
    dataset, rebuild_to_file,
};
use auroraw_testkit::{TempDir, temp_dir};
use auroraw_types::{PhotoId, WorkspaceId};
use serde_json::{Value, json};

/// Where a photo is, as the test knows it, and how its sidecar writes that.
#[derive(Clone, Default)]
struct Spot {
    /// The logical place: the names the tree should show for the photo.
    country: String,
    region: String,
    city: String,
    /// What the sidecar says: city, region, country, country code.
    written: [Option<String>; 4],
    rating: u8,
}

fn spot(country: &str, region: &str, city: &str) -> Spot {
    let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
    Spot {
        country: country.into(),
        region: region.into(),
        city: city.into(),
        written: [some(city), some(region), some(country), None],
        rating: 3,
    }
}

impl Spot {
    fn with_code(mut self, code: &str) -> Spot {
        self.written[3] = Some(code.into());
        self
    }
    fn written(mut self, city: Option<&str>, region: Option<&str>, country: Option<&str>) -> Spot {
        self.written[0] = city.map(str::to_string);
        self.written[1] = region.map(str::to_string);
        self.written[2] = country.map(str::to_string);
        self
    }
    fn rated(mut self, rating: u8) -> Spot {
        self.rating = rating;
        self
    }
}

fn built(spots: &[Spot]) -> (Catalogue, TempDir, Vec<PhotoId>) {
    let mut data = dataset::generate(spots.len().max(1), 7);
    data.versions.clear();
    data.series.clear();
    data.collections.clear();
    data.photos.truncate(spots.len());
    let mut ids = Vec::new();
    for ((photo, stat), spot) in data.photos.iter_mut().zip(spots) {
        photo.main_version = None;
        photo.meta.rating = Some(spot.rating);
        photo.meta.flag = None;
        photo.meta.city = spot.written[0].clone();
        photo.meta.region = spot.written[1].clone();
        photo.meta.country = spot.written[2].clone();
        photo.meta.country_code = spot.written[3].clone();
        *stat = SidecarStat::of_bytes(&photo.to_bytes());
        ids.push(photo.photo_id);
    }
    let dir = temp_dir();
    let path = dir.path().join("catalogue.db");
    let cat = rebuild_to_file(&path, WorkspaceId::random(), &data.as_rebuild_input()).unwrap();
    (cat, dir, ids)
}

fn every_row(cat: &Catalogue, filter: &Filter) -> Vec<PhotoId> {
    let mut out = Vec::new();
    let mut after: Option<Cursor> = None;
    loop {
        let page = cat.list_filtered(filter, after, 7).unwrap();
        let Some(last) = page.last() else { break };
        after = Some(last.cursor());
        out.extend(page.iter().map(|r| r.id));
    }
    out
}

fn all() -> Filter {
    Filter {
        flags: FlagFilter::All,
        ..Filter::default()
    }
}

/// `label count` for the whole tree, indented, in the order it comes.
fn outline(facets: &PlaceFacets) -> Vec<String> {
    fn walk(nodes: &[PlaceNode], depth: usize, out: &mut Vec<String>) {
        for node in nodes {
            out.push(format!(
                "{}{} {}",
                " ".repeat(depth),
                node.label,
                node.count
            ));
            walk(&node.children, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    walk(&facets.countries, 0, &mut out);
    out
}

fn world() -> Vec<Spot> {
    vec![
        spot("Canada", "Québec", "Montréal").with_code("CA"),
        spot("Canada", "Québec", "Montréal").with_code("CA"),
        spot("Canada", "Québec", "Montréal")
            .with_code("CA")
            .written(Some("Montreal"), Some("Quebec"), Some("Canada")),
        spot("Canada", "Québec", "Québec").with_code("CA"),
        spot("Canada", "Ontario", "Ottawa").with_code("CA"),
        spot("France", "Île-de-France", "Paris").with_code("FR"),
        spot("France", "Île-de-France", "Paris")
            .with_code("FR")
            .written(Some("PARIS"), Some("ile-de-france"), Some("France")),
        spot("Singapore", "", "Singapore").with_code("SG"),
        // A country and a region, no city.
        spot("Canada", "Ontario", "").with_code("CA"),
        // A city and nothing else: in no node.
        spot("", "", "Reykjavík").written(Some("Reykjavík"), None, None),
        // Nothing at all.
        spot("", "", ""),
    ]
}

#[test]
fn the_tree_has_the_countries_the_regions_and_the_cities_with_their_counts() {
    let (cat, _dir, _) = built(&world());
    let facets = cat.place_facets(&all()).unwrap();
    assert_eq!(
        facets.placed, 9,
        "the photos that have a country; a city alone is not one"
    );
    assert_eq!(
        outline(&facets),
        [
            "Canada 6",
            " Ontario 2",
            "  Ottawa 1",
            " Québec 4",
            "  Montréal 3",
            "  Québec 1",
            "France 2",
            " Île-de-France 2",
            "  Paris 2",
            "Singapore 1",
            " Singapore 1",
        ]
    );
}

#[test]
fn what_a_node_says_selects_exactly_the_photos_under_it() {
    let spots = world();
    let (cat, _dir, ids) = built(&spots);
    let facets = cat.place_facets(&all()).unwrap();
    check(&cat, &spots, &ids, &facets.countries, &mut Vec::new());
}

/// The logical places, by the labels the tree shows: a node's photos are those whose logical path starts with its own
/// path, and the filter the node carries selects exactly those (and as many as the node counts).
fn check(
    cat: &Catalogue,
    spots: &[Spot],
    ids: &[PhotoId],
    nodes: &[PlaceNode],
    path: &mut Vec<String>,
) {
    for node in nodes {
        path.push(node.label.clone());
        let expected: BTreeSet<PhotoId> = spots
            .iter()
            .zip(ids)
            .filter(|(s, _)| {
                let wanted: Vec<&str> = path.iter().map(String::as_str).collect();
                // A region-less photo's city is at depth 1, under its country.
                let deep: Vec<&str> = if s.region.is_empty() && wanted.len() > 1 {
                    vec![&s.country, &s.city]
                } else {
                    vec![&s.country, &s.region, &s.city]
                };
                !s.country.is_empty()
                    && deep.len() >= wanted.len()
                    && deep[..wanted.len()] == wanted[..]
            })
            .map(|(_, id)| *id)
            .collect();
        let got: Vec<PhotoId> = every_row(
            cat,
            &Filter {
                place: Some(node.filter.clone()),
                flags: FlagFilter::All,
                ..Filter::default()
            },
        );
        assert_eq!(
            got.iter().copied().collect::<BTreeSet<_>>(),
            expected,
            "{path:?} says {:?}",
            node.filter
        );
        assert_eq!(got.len() as u64, node.count, "{path:?}");
        check(cat, spots, ids, &node.children, path);
        path.pop();
    }
}

#[test]
fn one_place_written_several_ways_is_one_node_labelled_by_the_most_common() {
    let (cat, _dir, _) = built(&[
        spot("Canada", "Québec", "Montréal"),
        spot("Canada", "Québec", "Montréal"),
        spot("Canada", "Québec", "Montréal").written(
            Some("Montreal"),
            Some("Quebec"),
            Some("Canada"),
        ),
        spot("Canada", "Québec", "Montréal").written(
            Some("MONTRÉAL"),
            Some("QUÉBEC"),
            Some("CANADA"),
        ),
        spot("Canada", "Québec", "Montréal").written(Some("Montreal "), None, Some("canada")),
    ]);
    let facets = cat.place_facets(&all()).unwrap();
    assert_eq!(
        outline(&facets),
        ["Canada 5", " Montreal 1", " Québec 4", "  Montréal 4"],
        "the fifth has no region: its city is under the country, apart"
    );
    // The region-less photo's city is a node of its own, selected by "no region", not folded into Québec's.
    let country = &facets.countries[0];
    assert_eq!(country.children.len(), 2);
    let regionless = country
        .children
        .iter()
        .find(|n| n.label == "Montreal")
        .expect("a city under the country");
    assert_eq!(regionless.count, 1);
    assert_eq!(
        regionless.filter.region.as_deref(),
        Some(""),
        "'no region' is the empty key"
    );
}

#[test]
fn a_tie_between_spellings_goes_to_the_best_written_so_that_the_label_does_not_flicker() {
    let (cat, _dir, _) = built(&[
        spot("Canada", "Québec", "Montréal").written(
            Some("Montreal"),
            Some("Québec"),
            Some("Canada"),
        ),
        spot("Canada", "Québec", "Montréal").written(
            Some("Montréal"),
            Some("Québec"),
            Some("Canada"),
        ),
    ]);
    let labels = outline(&cat.place_facets(&all()).unwrap());
    assert_eq!(
        labels,
        ["Canada 2", " Québec 2", "  Montréal 2"],
        "the one with its accent, not the one that sorts first"
    );
    // Capitals and lower case lose to a name written the way names are.
    let (cat, _dir, _) = built(&[
        spot("France", "Île-de-France", "Paris").written(
            Some("PARIS"),
            Some("ile-de-france"),
            Some("france"),
        ),
        spot("France", "Île-de-France", "Paris").written(
            Some("Paris"),
            Some("Île-de-France"),
            Some("France"),
        ),
    ]);
    assert_eq!(
        outline(&cat.place_facets(&all()).unwrap()),
        ["France 2", " Île-de-France 2", "  Paris 2"]
    );
}

#[test]
fn a_country_is_its_code_when_the_photo_has_one_so_that_three_names_are_one_country() {
    let (cat, _dir, _) = built(&[
        spot("Germany", "Bayern", "München").with_code("DE"),
        spot("Germany", "Bayern", "München").with_code("DE"),
        spot("Germany", "Berlin", "Berlin").with_code("DE").written(
            Some("Berlin"),
            Some("Berlin"),
            Some("Allemagne"),
        ),
        spot("Germany", "Berlin", "Berlin").with_code("de").written(
            Some("Berlin"),
            Some("Berlin"),
            Some("Deutschland"),
        ),
        // The same country, written by a tool that gave no code: its own node, by name.
        spot("Germany", "Bayern", "München").written(
            Some("München"),
            Some("Bayern"),
            Some("Deutschland"),
        ),
    ]);
    let facets = cat.place_facets(&all()).unwrap();
    assert_eq!(
        outline(&facets),
        [
            "Deutschland 1",
            " Bayern 1",
            "  München 1",
            "Germany 4",
            " Bayern 2",
            "  München 2",
            " Berlin 2",
            "  Berlin 2",
        ]
    );
    assert_eq!(facets.countries[1].filter.country.as_deref(), Some("DE"));
    assert_eq!(
        facets.countries[0].filter.country.as_deref(),
        Some("deutschland")
    );
}

#[test]
fn a_photo_with_only_a_code_is_in_its_country_and_a_node_is_never_nameless() {
    let (cat, _dir, _) = built(&[spot("", "", "").with_code("IS").written(None, None, None)]);
    assert_eq!(
        outline(&cat.place_facets(&all()).unwrap()),
        ["IS 1"],
        "the code is the name, failing one"
    );
}

#[test]
fn the_other_filters_apply_and_the_places_own_does_not() {
    let spots = vec![
        spot("Canada", "Québec", "Montréal").rated(5),
        spot("Canada", "Québec", "Montréal").rated(2),
        spot("Canada", "Ontario", "Ottawa").rated(4),
        spot("France", "Île-de-France", "Paris").rated(1),
    ];
    let (cat, _dir, _) = built(&spots);
    let rated = Filter {
        min_rating: 4,
        flags: FlagFilter::All,
        ..Filter::default()
    };
    assert_eq!(
        outline(&cat.place_facets(&rated).unwrap()),
        [
            "Canada 2",
            " Ontario 1",
            "  Ottawa 1",
            " Québec 1",
            "  Montréal 1"
        ],
        "the rating counts"
    );
    // Choosing Québec still shows Ontario and France: the tree leaves out the place it is asked for.
    let chosen = Filter {
        place: Some(PlaceFilter {
            country: Some("canada".into()),
            region: Some("quebec".into()),
            city: None,
        }),
        ..all()
    };
    assert_eq!(
        cat.place_facets(&chosen).unwrap(),
        cat.place_facets(&all()).unwrap()
    );
    // And what the grid lists is the intersection.
    let both = Filter {
        min_rating: 4,
        place: chosen.place.clone(),
        flags: FlagFilter::All,
        ..Filter::default()
    };
    assert_eq!(every_row(&cat, &both).len(), 1);
    assert_eq!(every_row(&cat, &chosen).len(), 2);
}

#[test]
fn writing_a_photos_metadata_moves_it_in_the_tree() {
    let (mut cat, _dir, ids) = built(&[
        spot("Canada", "Québec", "Montréal"),
        spot("Canada", "Québec", "Montréal"),
    ]);
    assert_eq!(
        outline(&cat.place_facets(&all()).unwrap()),
        ["Canada 2", " Québec 2", "  Montréal 2"]
    );

    // The first photo's city is rewritten another way: still one node.
    let mut data = dataset::generate(2, 7);
    data.versions.clear();
    let mut photo = data.photos[0].0.clone();
    photo.photo_id = ids[0];
    photo.main_version = None;
    photo.meta.city = Some("MONTREAL".into());
    photo.meta.region = Some("Québec".into());
    photo.meta.country = Some("Canada".into());
    photo.meta.country_code = None;
    cat.apply_photo_metadata(&photo, SidecarStat::of_bytes(&photo.to_bytes()), None)
        .unwrap();
    assert_eq!(
        outline(&cat.place_facets(&all()).unwrap()),
        ["Canada 2", " Québec 2", "  Montréal 2"]
    );

    // Its country emptied: it leaves the tree, and the filter no longer selects it.
    photo.meta.country = None;
    cat.apply_photo_metadata(&photo, SidecarStat::of_bytes(&photo.to_bytes()), None)
        .unwrap();
    let facets = cat.place_facets(&all()).unwrap();
    assert_eq!(facets.placed, 1);
    assert_eq!(outline(&facets), ["Canada 1", " Québec 1", "  Montréal 1"]);
    let canada = Filter {
        place: Some(PlaceFilter {
            country: Some("canada".into()),
            ..PlaceFilter::default()
        }),
        ..all()
    };
    assert_eq!(every_row(&cat, &canada), vec![ids[1]]);
}

#[test]
fn a_region_less_city_is_selected_apart_from_the_same_city_in_a_region() {
    let (cat, _dir, ids) = built(&[
        spot("Singapore", "", "Singapore").with_code("SG"),
        spot("Singapore", "Central", "Singapore").with_code("SG"),
        spot("Singapore", "", "").with_code("SG"),
    ]);
    let facets = cat.place_facets(&all()).unwrap();
    assert_eq!(
        outline(&facets),
        ["Singapore 3", " Central 1", "  Singapore 1", " Singapore 1"]
    );
    let regionless_city = facets.countries[0]
        .children
        .iter()
        .find(|n| n.filter.region.as_deref() == Some(""))
        .expect("the city with no region");
    let got = every_row(
        &cat,
        &Filter {
            place: Some(regionless_city.filter.clone()),
            ..all()
        },
    );
    assert_eq!(
        got,
        vec![ids[0]],
        "not the Singapore of the region, not the photo with no city"
    );
}

#[test]
fn the_tree_is_json_in_the_shape_the_menu_reads() {
    let (cat, _dir, _) = built(&[
        spot("Canada", "Québec", "Montréal").with_code("CA"),
        spot("Singapore", "", "Singapore").with_code("SG"),
    ]);
    let text = cat.place_facets(&all()).unwrap().to_json();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        value,
        json!({
            "placed": 2,
            "countries": [
                { "label": "Canada", "count": 1, "filter": { "country": "CA" }, "children": [
                    { "label": "Québec", "count": 1, "filter": { "country": "CA", "region": "quebec" }, "children": [
                        { "label": "Montréal", "count": 1,
                          "filter": { "country": "CA", "region": "quebec", "city": "montreal" }, "children": [] }
                    ] }
                ] },
                { "label": "Singapore", "count": 1, "filter": { "country": "SG" }, "children": [
                    { "label": "Singapore", "count": 1,
                      "filter": { "country": "SG", "region": "", "city": "singapore" }, "children": [] }
                ] },
            ]
        })
    );
    // And a node's filter is what comes back: the same JSON, read as a filter, selects the node.
    let canada = &value["countries"][0]["children"][0]["children"][0]["filter"];
    let filter = PlaceFilter::from_json(&canada.to_string()).unwrap();
    assert_eq!(filter.city.as_deref(), Some("montreal"));
}

#[test]
fn an_empty_catalogue_has_an_empty_tree() {
    let (cat, _dir, _) = built(&[]);
    let facets = cat.place_facets(&Filter::default()).unwrap();
    assert_eq!(facets, PlaceFacets::default());
    assert_eq!(facets.to_json(), r#"{"countries":[],"placed":0}"#);
}

/// The tree the logical places say, as the outline lines the catalogue's tree is read as.
fn expected_outline(spots: &[Spot], min_rating: u8) -> (u64, Vec<String>) {
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Node {
        count: u64,
        children: BTreeMap<String, Node>,
    }
    let mut countries: BTreeMap<String, Node> = BTreeMap::new();
    let mut placed = 0;
    for s in spots
        .iter()
        .filter(|s| s.rating >= min_rating && !s.country.is_empty())
    {
        placed += 1;
        let country = countries.entry(s.country.clone()).or_default();
        country.count += 1;
        if s.region.is_empty() {
            // A city with no region: under the country, as itself.
            if !s.city.is_empty() {
                country.children.entry(s.city.clone()).or_default().count += 1;
            }
        } else {
            let region = country.children.entry(s.region.clone()).or_default();
            region.count += 1;
            if !s.city.is_empty() {
                region.children.entry(s.city.clone()).or_default().count += 1;
            }
        }
    }
    fn lines(nodes: &BTreeMap<String, Node>, depth: usize, out: &mut Vec<String>) {
        let mut sorted: Vec<(&String, &Node)> = nodes.iter().collect();
        sorted.sort_by(|a, b| {
            auroraw_catalogue::fold_place(a.0)
                .cmp(&auroraw_catalogue::fold_place(b.0))
                .then_with(|| a.0.cmp(b.0))
        });
        for (label, node) in sorted {
            out.push(format!("{}{} {}", " ".repeat(depth), label, node.count));
            lines(&node.children, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    lines(&countries, 0, &mut out);
    (placed, out)
}

type Region = (&'static str, &'static [&'static str]);
type Country = (&'static str, &'static str, &'static [Region]);

#[test]
fn a_larger_catalogue_gives_the_tree_its_logical_places_say_and_every_node_selects_its_photos() {
    // 1,500 photos in a small world, written the way several hands write them: mostly as the place is spelt, a tenth in
    // capitals or lower case, a country always with its code.
    let world: [Country; 6] = [
        (
            "Canada",
            "CA",
            &[
                ("Québec", &["Montréal", "Québec", "Laval"]),
                ("Ontario", &["Ottawa", "Toronto"]),
            ],
        ),
        (
            "France",
            "FR",
            &[
                ("Bretagne", &["Rennes", "Brest"]),
                ("Île-de-France", &["Paris"]),
                ("Occitanie", &["Toulouse", "Nîmes", "Albi", "Cahors"]),
            ],
        ),
        (
            "Germany",
            "DE",
            &[
                ("Bayern", &["München", "Nürnberg"]),
                ("Berlin", &["Berlin"]),
            ],
        ),
        ("Singapore", "SG", &[("", &["Singapore"])]),
        (
            "Brasil",
            "BR",
            &[
                ("São Paulo", &["São Paulo", "Santos"]),
                ("Rio de Janeiro", &["Rio de Janeiro", "Niterói"]),
            ],
        ),
        (
            "Japan",
            "JP",
            &[
                ("Hokkaidō", &["Sapporo"]),
                ("Kantō", &["Tōkyō", "Yokohama"]),
            ],
        ),
    ];
    let mut r = fastrand::Rng::with_seed(5);
    let variant = |text: &str, r: &mut fastrand::Rng| match r.u8(0..20) {
        0 | 1 => text.to_uppercase(),
        2 => text.to_lowercase(),
        _ => text.to_string(),
    };
    let mut spots = Vec::new();
    for _ in 0..1500 {
        let (country, code, regions) = world[r.usize(0..world.len()).min(r.usize(0..world.len()))];
        let (region, cities) = regions[r.usize(0..regions.len())];
        let city = if r.u8(0..6) == 0 {
            ""
        } else {
            cities[r.usize(0..cities.len())]
        };
        let rating = r.u8(0..=5);
        spots.push(match r.u8(0..10) {
            // No place at all.
            0 => Spot {
                rating,
                ..Spot::default()
            },
            // A city and nothing else.
            1 => spot("", "", city)
                .written((!city.is_empty()).then_some(city), None, None)
                .rated(rating),
            _ => {
                let city_written = (!city.is_empty()).then(|| variant(city, &mut r));
                let region_written = (!region.is_empty()).then(|| variant(region, &mut r));
                let country_written = variant(country, &mut r);
                spot(country, region, city)
                    .with_code(code)
                    .written(
                        city_written.as_deref(),
                        region_written.as_deref(),
                        Some(&country_written),
                    )
                    .rated(rating)
            }
        });
    }
    let (cat, _dir, ids) = built(&spots);
    for min_rating in [0, 3, 5] {
        let filter = Filter {
            min_rating,
            flags: FlagFilter::All,
            ..Filter::default()
        };
        let facets = cat.place_facets(&filter).unwrap();
        let (placed, expected) = expected_outline(&spots, min_rating);
        assert_eq!(facets.placed, placed, "rating {min_rating}");
        // (To the case: a node whose photos are all written in capitals shows them in capitals, and the labels
        // have tests of their own.)
        let lower = |lines: Vec<String>| -> Vec<String> {
            lines.into_iter().map(|l| l.to_lowercase()).collect()
        };
        assert_eq!(
            lower(outline(&facets)),
            lower(expected),
            "rating {min_rating}"
        );
    }
    let facets = cat.place_facets(&all()).unwrap();
    check(&cat, &spots, &ids, &facets.countries, &mut Vec::new());
}
