// SPDX-License-Identifier: GPL-3.0-or-later
//! Builds the file of places from the published data (design note 008 §2).
//!
//! `cargo run --release -p auroraw-places --example build-places -- <source folder> <output file>`
//!
//! The source folder holds `ne_10m_admin_0_countries.geojson`, `ne_10m_admin_1_states_provinces.geojson`
//! and `cities1000.txt`, as `tools/fetch-places.sh` leaves them. The output file must not exist.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use auroraw_places::{PackBuilder, sources};

fn main() {
    let mut arguments = std::env::args().skip(1);
    let (Some(folder), Some(output)) = (arguments.next(), arguments.next()) else {
        eprintln!("usage: build-places <source folder> <output file>");
        std::process::exit(2);
    };
    let (folder, output) = (PathBuf::from(folder), PathBuf::from(output));
    let started = Instant::now();
    let read = |name: &str| {
        std::fs::read_to_string(folder.join(name))
            .unwrap_or_else(|e| panic!("cannot read {name} in {}: {e}", folder.display()))
    };
    let countries = sources::countries(
        &read("ne_10m_admin_0_countries.geojson"),
        sources::LANGUAGES,
    )
    .expect("the countries");
    let regions = sources::regions(
        &read("ne_10m_admin_1_states_provinces.geojson"),
        sources::LANGUAGES,
    )
    .expect("the regions");
    let towns = sources::places(&read("cities1000.txt")).expect("the towns");

    let mut pack = PackBuilder::create(&output).expect("the output file");
    let mut by_key: HashMap<String, i64> = HashMap::new();
    let mut by_code: HashMap<String, i64> = HashMap::new();
    for country in &countries {
        let id = pack.add_area(&country.area).expect("a country");
        by_key.insert(country.key.clone(), id);
        if !country.area.code.is_empty() {
            by_code.entry(country.area.code.clone()).or_insert(id);
        }
    }
    let mut orphans = 0;
    for mut region in regions {
        region.area.parent = by_key
            .get(&region.country_key)
            .or_else(|| by_code.get(&region.area.country_code))
            .copied();
        orphans += usize::from(region.area.parent.is_none());
        pack.add_area(&region.area).expect("a region");
    }
    for town in &towns {
        pack.add_place(town).expect("a town");
    }
    pack.set_meta(
        "countries",
        "Natural Earth 5.1.2, ne_10m_admin_0_countries (public domain)",
    )
    .unwrap();
    pack.set_meta(
        "regions",
        "Natural Earth 5.1.2, ne_10m_admin_1_states_provinces (public domain)",
    )
    .unwrap();
    pack.set_meta(
        "towns",
        "GeoNames cities1000 (CC BY 4.0, https://www.geonames.org/)",
    )
    .unwrap();
    pack.set_meta(
        "worldview",
        "Natural Earth's default (boundaries as they are on the ground)",
    )
    .unwrap();
    pack.set_meta("languages", &sources::LANGUAGES.join(","))
        .unwrap();
    let summary = pack.finish().expect("the file");
    let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    println!("{summary:#?}");
    println!("regions without a country: {orphans}");
    println!(
        "{} written: {:.1} MiB, in {:.1} s",
        output.display(),
        size as f64 / (1024.0 * 1024.0),
        started.elapsed().as_secs_f64()
    );
}
