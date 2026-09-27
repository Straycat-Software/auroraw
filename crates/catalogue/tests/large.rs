// SPDX-License-Identifier: GPL-3.0-or-later
//! A measurement, not a check: builds a catalogue of 100,000 photos and times the queries of
//! spike 3 through the product API (M1 plan WP2 "done when",
//! docs/spikes/03-catalogue-and-grid.md). Run on demand, nightly on the three platforms:
//! `cargo test -p auroraw-catalogue --release --test large -- --ignored --nocapture`

use std::time::Instant;

use auroraw_catalogue::{dataset, rebuild_to_file};
use auroraw_testkit::temp_dir;
use auroraw_types::WorkspaceId;

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

#[test]
#[ignore = "a measurement: run on demand with AUR_LARGE=<photos>"]
fn queries_at_scale() {
    let n: usize = std::env::var("AUR_LARGE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000);
    let started = Instant::now();
    let data = dataset::generate(n, 100);
    let generated = ms(started.elapsed());

    let dir = temp_dir();
    let started = Instant::now();
    let cat = rebuild_to_file(
        &dir.path().join("catalogue.db"),
        WorkspaceId::random(),
        &data.as_rebuild_input(),
    )
    .unwrap();
    let rebuilt = ms(started.elapsed());

    let median = |mut v: Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    let time = |f: &dyn Fn()| {
        let mut v = Vec::with_capacity(30);
        for _ in 0..30 {
            let t = Instant::now();
            f();
            v.push(ms(t.elapsed()));
        }
        median(v)
    };

    let path = cat.path().unwrap().to_path_buf();
    drop(cat);
    let started = Instant::now();
    let mut cat = auroraw_catalogue::Catalogue::open(&path).unwrap();
    let open_ms = ms(started.elapsed());

    let count_all = time(&|| {
        cat.count_all().unwrap();
    });
    let page_first = time(&|| {
        cat.list_recent(None, 200).unwrap();
    });
    let some_keyword = data.vocabulary[data.vocabulary.len() / 2].id;
    let by_keyword = time(&|| {
        cat.list_by_keyword(&some_keyword, false, None, 200)
            .unwrap();
    });
    let by_rating = time(&|| {
        cat.list_by_min_rating(4, None, 200).unwrap();
    });
    let count_by_rating = time(&|| {
        cat.count_by_min_rating(4).unwrap();
    });
    let search = time(&|| {
        cat.search("photograph", 200).unwrap();
    });
    let one_photo = data.photos[data.photos.len() / 2].0.photo_id;
    let by_id = time(&|| {
        cat.photo(&one_photo).unwrap();
    });

    // The similar-photo suggestions (D-105): every photo gets a hash, then the set around one photo is read (the photos
    // within 30 minutes, their hashes compared in memory with the reference's).
    let mut state = 88_172_645_463_325_252u64;
    let started = Instant::now();
    for (photo, _) in &data.photos {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        cat.apply_phash(&photo.photo_id, state).unwrap();
    }
    let hashed_ms = ms(started.elapsed());
    let unhashed = time(&|| {
        cat.unhashed_count().unwrap();
    });
    let in_window = std::cell::Cell::new(0usize);
    let similar = time(&|| {
        let set = cat.similar_set(&one_photo, 30 * 60).unwrap().unwrap();
        in_window.set(set.candidates.len());
        let _near = set
            .candidates
            .iter()
            .filter(|c| (c.phash ^ set.reference).count_ones() <= 10)
            .count();
    });

    println!(
        "{{\"photos\": {n}, \"generate_ms\": {generated:.0}, \"rebuild_ms\": {rebuilt:.0}, \"open_cold_ms\": {open_ms:.2}, \
         \"count_all_ms\": {count_all:.3}, \"page_first_ms\": {page_first:.3}, \"by_keyword_ms\": {by_keyword:.3}, \
         \"by_rating_ms\": {by_rating:.3}, \"count_by_rating_ms\": {count_by_rating:.3}, \"search_ms\": {search:.3}, \"by_id_ms\": {by_id:.3}, \
         \"hash_all_writes_ms\": {hashed_ms:.0}, \"unhashed_count_ms\": {unhashed:.3}, \"similar_set_ms\": {similar:.3}, \"similar_window_photos\": {}}}",
        in_window.get()
    );
}
