// SPDX-License-Identifier: GPL-3.0-or-later
//! WP4's "done when": a folder is added, edited outside the application, unplugged and
//! replugged, and the catalogue follows without loss - scripted through the real `auroraw-cli`
//! binary. "A card is recognised as a volume on all three platforms" is `source add --removable`
//! plus `auroraw_sources::volumes`'s own tests; a real card is a pre-release checklist item
//! (testing strategy §11, item 7), not something CI has one of.

use std::fs;
use std::process::{Command, Output};

use auroraw_testkit::temp_dir;

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_auroraw-cli"))
        .args(args)
        .output()
        .unwrap()
}

fn ok(out: &Output) -> String {
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).unwrap()
}

#[test]
fn a_folder_is_added_edited_outside_and_survives_unplug_and_replug() {
    let dir = temp_dir();
    let workspace = dir.path().join("Main");
    let catalogue = dir.path().join("main.sqlite");
    let ws = workspace.to_str().unwrap();
    let cat = catalogue.to_str().unwrap();
    ok(&cli(&["create", ws, cat, "Main"]));

    let card = dir.path().join("Card");
    fs::create_dir_all(&card).unwrap();
    fs::write(card.join("IMG_0001.ARW"), b"the first photo's bytes").unwrap();
    let card_str = card.to_str().unwrap();

    let out = ok(&cli(&["source", "add", ws, cat, card_str, "My card"]));
    let source_id = out
        .split_whitespace()
        .nth(2)
        .expect("added source <id> (...) at ...")
        .to_string();

    let out = ok(&cli(&["source", "list", ws, cat]));
    assert!(out.contains(&source_id), "{out}");
    assert!(out.contains("My card"), "{out}");

    let out = ok(&cli(&["source", "scan", ws, cat, &source_id]));
    assert!(out.contains("new: IMG_0001.ARW"), "{out}");

    let out = ok(&cli(&["source", "add-new", ws, cat, &source_id, "--all"]));
    assert!(out.contains("1/1 added"), "{out}");
    let photo_id = out
        .lines()
        .next()
        .unwrap()
        .strip_prefix("added photo ")
        .unwrap()
        .to_string();

    let out = ok(&cli(&["list", ws, cat]));
    assert!(out.contains(&photo_id), "{out}");

    // Rescanning finds nothing new: the file is now confirmed.
    let out = ok(&cli(&["source", "scan", ws, cat, &source_id]));
    assert!(out.contains("confirmed=1"), "{out}");
    assert!(!out.contains("new:"), "{out}");

    // Renamed outside the application (content untouched): relinked silently, not offered as a
    // new file.
    fs::rename(card.join("IMG_0001.ARW"), card.join("IMG_0001_renamed.ARW")).unwrap();
    let out = ok(&cli(&["source", "scan", ws, cat, &source_id]));
    assert!(out.contains("relinked=1"), "{out}");
    assert!(!out.contains("new:"), "{out}");

    // Edited outside the application, at its (now relinked) path: the fingerprint no longer
    // matches what the catalogue recorded when it was added. "Accept"ing the change and
    // recomputing the fingerprint is a WP8 interface feature (design note 004 §6.5) this work
    // package does not build; reconcile only needs to notice and mark it, which it does.
    fs::write(
        card.join("IMG_0001_renamed.ARW"),
        b"a completely different set of bytes",
    )
    .unwrap();
    let out = ok(&cli(&["source", "scan", ws, cat, &source_id]));
    assert!(out.contains("changed=1"), "{out}");

    // Unplugged: the card's mount point disappears.
    let elsewhere = dir.path().join("Card-unplugged");
    fs::rename(&card, &elsewhere).unwrap();
    let out = ok(&cli(&["source", "scan", ws, cat, &source_id]));
    assert!(out.contains("not reachable"), "{out}");

    // Replugged, at the same path: nothing lost. The file still carries the earlier, unaccepted
    // edit, so the same "original changed" mark reappears rather than a stray "missing" (an
    // unreachable source never makes a file "gone", only offline or missing outright does).
    fs::rename(&elsewhere, &card).unwrap();
    let out = ok(&cli(&["source", "scan", ws, cat, &source_id]));
    assert!(out.contains("changed=1"), "{out}");
    assert!(out.contains("missing=0"), "{out}");

    let out = ok(&cli(&["list", ws, cat]));
    assert!(
        out.contains(&photo_id),
        "the photo survived the whole scenario: {out}"
    );
}

/// The m1-plan's own "two locations" scenario (D-036, D-108): the same working folder added a second time (as its
/// backup), through the CLI's manual `source add`/`scan`/`add-new` flow — a different path than `Engine::add_source`'s
/// convenience (the one `crates/engine/tests/duplicates.rs` exercises), so both are covered.
#[test]
fn adding_the_same_folder_a_second_time_as_a_backup_makes_one_photo_with_two_locations() {
    let dir = temp_dir();
    let workspace = dir.path().join("Main");
    let catalogue = dir.path().join("main.sqlite");
    let ws = workspace.to_str().unwrap();
    let cat = catalogue.to_str().unwrap();
    ok(&cli(&["create", ws, cat, "Main"]));

    let working = dir.path().join("Working");
    fs::create_dir_all(&working).unwrap();
    fs::write(
        working.join("a.jpg"),
        b"the same photo's bytes, byte for byte",
    )
    .unwrap();

    let out = ok(&cli(&[
        "source",
        "add",
        ws,
        cat,
        working.to_str().unwrap(),
        "Working",
    ]));
    let source_id = out.split_whitespace().nth(2).unwrap().to_string();
    ok(&cli(&["source", "scan", ws, cat, &source_id]));
    let out = ok(&cli(&["source", "add-new", ws, cat, &source_id, "--all"]));
    assert!(out.contains("1/1 added"), "{out}");

    let backup = dir.path().join("Backup");
    fs::create_dir_all(&backup).unwrap();
    fs::write(
        backup.join("a_copy.jpg"),
        b"the same photo's bytes, byte for byte",
    )
    .unwrap();

    let out = ok(&cli(&[
        "source",
        "add",
        ws,
        cat,
        backup.to_str().unwrap(),
        "Backup disk",
    ]));
    let backup_id = out.split_whitespace().nth(2).unwrap().to_string();
    let out = ok(&cli(&["source", "scan", ws, cat, &backup_id]));
    assert!(out.contains("new: a_copy.jpg"), "{out}");
    let out = ok(&cli(&["source", "add-new", ws, cat, &backup_id, "--all"]));
    assert!(
        out.contains("0/1 added"),
        "joined to the existing photo, not added as a new one: {out}"
    );

    let out = ok(&cli(&["list", ws, cat]));
    assert_eq!(out.lines().last().unwrap(), "1 photo(s)", "{out}");

    let out = ok(&cli(&["duplicates", ws, cat]));
    assert!(out.contains("a.jpg"), "{out}");
    assert!(out.contains("Working — a.jpg"), "{out}");
    assert!(out.contains("Backup disk — a_copy.jpg"), "{out}");
    assert_eq!(out.lines().last().unwrap(), "1 duplicate photo(s)", "{out}");
}
