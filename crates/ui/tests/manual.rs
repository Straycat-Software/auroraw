// SPDX-License-Identifier: GPL-3.0-or-later
//! The user manual (`docs/manual/`, D-102) holds together: the index links every page, every relative link
//! and every picture a page names exists, every link to a heading finds it, and no picture is left unused.
//! (That the keyboard page lists every shortcut is a test of the command table, in `src/commands.rs`.)

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn manual() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/manual")
}

fn pages() -> Vec<PathBuf> {
    let mut pages: Vec<PathBuf> = std::fs::read_dir(manual())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    pages.sort();
    pages
}

/// Every `](target)` of a Markdown text.
fn links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("](") {
        rest = &rest[at + 2..];
        if let Some(end) = rest.find(')') {
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    out
}

/// The anchor GitHub gives a heading: lower case, spaces to hyphens, punctuation dropped.
fn slug(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

fn anchors(text: &str) -> BTreeSet<String> {
    text.lines()
        .filter(|l| l.starts_with('#'))
        .map(|l| slug(l.trim_start_matches('#')))
        .collect()
}

#[test]
fn the_index_links_every_page() {
    let index = std::fs::read_to_string(manual().join("README.md")).unwrap();
    for page in pages() {
        let name = page.file_name().unwrap().to_string_lossy().to_string();
        if name == "README.md" {
            continue;
        }
        assert!(
            index.contains(&format!("]({name})")),
            "README.md does not link {name}"
        );
    }
}

#[test]
fn every_link_and_picture_of_a_page_exists() {
    let mut used_images: BTreeSet<String> = BTreeSet::new();
    for page in pages() {
        let text = std::fs::read_to_string(&page).unwrap();
        let name = page.file_name().unwrap().to_string_lossy().to_string();
        for target in links(&text) {
            if target.starts_with("http") || target.is_empty() {
                continue;
            }
            let (file, fragment) = match target.split_once('#') {
                Some((f, a)) => (f, Some(a)),
                None => (target.as_str(), None),
            };
            let (path, text_of_target) = if file.is_empty() {
                (page.clone(), text.clone())
            } else {
                let path = manual().join(file);
                assert!(
                    path.exists(),
                    "{name}: the link ({target}) points at nothing"
                );
                let target_text = if file.ends_with(".md") {
                    std::fs::read_to_string(&path).unwrap()
                } else {
                    String::new()
                };
                if file.starts_with("images/") {
                    used_images.insert(file.trim_start_matches("images/").to_string());
                }
                (path, target_text)
            };
            if let Some(fragment) = fragment {
                assert!(
                    anchors(&text_of_target).contains(fragment),
                    "{name}: ({target}) names no heading of {}",
                    path.display()
                );
            }
        }
    }
    // No picture that no page shows.
    let on_disk: BTreeSet<String> = std::fs::read_dir(manual().join("images"))
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    let unused: Vec<_> = on_disk.difference(&used_images).collect();
    assert!(unused.is_empty(), "pictures no page shows: {unused:?}");
}
