// SPDX-License-Identifier: GPL-3.0-or-later
//! `cargo xtask waits`: no new fixed pause followed by one read in the interface's QML tests (issue #73).
//!
//! A test that says `wait(300)` and then `compare(grid.columns, 8)` claims that the grid has laid out after 300 ms and
//! nothing waits for it to have: on a slow runner it fails (issues #27, #63, #72). The line that reads should be a
//! wait on what it asserts (`tryCompare`, `tryVerify`). The suites had 85 such sites when this check was written; they
//! are listed in [`LIST`], and the check is a **ratchet**:
//!
//! - a site that is **not** in the list fails the check (a new pause followed by a bare read);
//! - an entry of the list that matches **no** site fails the check too (the site was fixed: remove its entry, or let
//!   `cargo xtask waits --shrink` do it), so that the list only shrinks.
//!
//! A site is a line with a `wait(<number>)` call (not in a comment) whose next line of code, within three lines, starts
//! with `compare(` or `verify(`. It is identified by the file and the text of the two lines, not by a line number, so
//! that editing the file above it does not move it. `cargo xtask waits --list` prints the sites in the list's format.

use std::collections::BTreeMap;
use std::path::Path;

/// The folder of the suites.
const SUITES: &str = "crates/ui/tests/qml";
/// The list of the sites that exist and are to go: one per line, the file, the pause and the line that reads, separated
/// by tabs; `#` starts a comment. A site that occurs twice is listed twice.
const LIST: &str = "crates/ui/tests/qml/waits-to-remove.txt";

/// What a site is, without its place: the file and the text of its two lines.
type Key = (String, String, String);

/// Where a site is.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Site {
    key: Key,
    /// The line of the pause, counting from 1.
    line: usize,
}

/// The code of a line: what is before a `//` comment.
fn code(line: &str) -> &str {
    line.split("//").next().unwrap_or("")
}

/// Whether the code has a call `wait(<digits>)` of its own (not `tc.wait(…)`, `waitForRendering(…)`, `tryWait(…)`).
fn has_pause(code: &str) -> bool {
    code.match_indices("wait(").any(|(at, _)| {
        let before = code[..at].chars().next_back();
        let after = &code[at + "wait(".len()..];
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
            && digits > 0
            && after[digits..].starts_with(')')
    })
}

/// Whether a line opens a `while` or a `for`.
fn starts_a_loop(line: &str) -> bool {
    ["while ", "while(", "for ", "for("]
        .iter()
        .any(|keyword| line.starts_with(keyword))
}

/// A line as the list writes it: trimmed, with tabs turned to spaces.
fn normal(line: &str) -> String {
    line.trim().replace('\t', " ")
}

/// The sites of one suite.
fn sites(file: &str, text: &str) -> Vec<Site> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !has_pause(code(line)) {
            continue;
        }
        // A pause that is the body of a loop is a poll, and a read after the loop is the right shape (`reach()`,
        // `tryEntries()`): the loop waits for what the read asserts and the read says what it found.
        let previous = lines[..index]
            .iter()
            .rev()
            .take(3)
            .map(|l| l.trim())
            .find(|l| !l.is_empty() && !l.starts_with("//"));
        if previous.is_some_and(starts_a_loop) || starts_a_loop(line.trim()) {
            continue;
        }
        let next = lines[index + 1..]
            .iter()
            .take(3)
            .map(|l| l.trim())
            .find(|l| !l.is_empty() && !l.starts_with("//"));
        if let Some(read) = next
            && (read.starts_with("compare(") || read.starts_with("verify("))
        {
            found.push(Site {
                key: (file.to_string(), normal(line), normal(read)),
                line: index + 1,
            });
        }
    }
    found
}

/// The sites of every suite, in the order of the files and of the lines.
fn all_sites(root: &Path) -> Vec<Site> {
    let mut files: Vec<_> = std::fs::read_dir(root.join(SUITES))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "qml"))
        .collect();
    files.sort();
    files
        .iter()
        .flat_map(|path| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            sites(&name, &std::fs::read_to_string(path).unwrap_or_default())
        })
        .collect()
}

/// The list, as how many times each site is listed.
fn listed(text: &str) -> BTreeMap<Key, usize> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        if let (Some(file), Some(pause), Some(read)) = (parts.next(), parts.next(), parts.next()) {
            *map.entry((file.to_string(), pause.to_string(), read.to_string()))
                .or_insert(0) += 1;
        }
    }
    map
}

/// What the check finds: the sites that are not in the list, and the listed ones that no longer exist (with how many
/// times each).
struct Verdict {
    new: Vec<Site>,
    stale: Vec<(Key, usize)>,
}

fn judge(found: &[Site], list: &BTreeMap<Key, usize>) -> Verdict {
    let mut by_key: BTreeMap<&Key, Vec<&Site>> = BTreeMap::new();
    for site in found {
        by_key.entry(&site.key).or_default().push(site);
    }
    let mut new = Vec::new();
    for (key, sites) in &by_key {
        let allowed = list.get(*key).copied().unwrap_or(0);
        // (The later ones of a site that occurs several times are the new ones.)
        new.extend(sites.iter().skip(allowed).map(|s| (*s).clone()));
    }
    let stale = list
        .iter()
        .filter_map(|(key, &count)| {
            let now = by_key.get(key).map_or(0, Vec::len);
            (count > now).then(|| (key.clone(), count - now))
        })
        .collect();
    Verdict { new, stale }
}

/// The list's text for these sites, with its header.
fn list_text(found: &[Site]) -> String {
    let mut out = String::from(
        "# The pauses of the QML suites followed by a bare compare/verify that are still to be fixed (issue #73).\n\
         # A new one is refused by `cargo xtask check`: wait on what the next line asserts (tryCompare, tryVerify).\n\
         # A site that no longer exists must leave this list: `cargo xtask waits --shrink` removes it. The list only shrinks.\n\
         # The file, the pause and the line that reads, separated by tabs.\n",
    );
    for site in found {
        out.push_str(&format!("{}\t{}\t{}\n", site.key.0, site.key.1, site.key.2));
    }
    out
}

/// `cargo xtask waits [--list | --shrink]`.
pub fn run(option: Option<String>) -> bool {
    let root = crate::root();
    let found = all_sites(&root);
    let list_path = root.join(LIST);
    let list_file = std::fs::read_to_string(&list_path).unwrap_or_default();
    match option.as_deref() {
        Some("--list") => {
            print!("{}", list_text(&found));
            return true;
        }
        Some("--shrink") => {
            let verdict = judge(&found, &listed(&list_file));
            let keep: Vec<Site> = found
                .iter()
                .filter(|s| !verdict.new.contains(s))
                .cloned()
                .collect();
            if let Err(e) = std::fs::write(&list_path, list_text(&keep)) {
                eprintln!("{LIST}: {e}");
                return false;
            }
            println!(
                "waits: {} entries removed from {LIST}, {} left",
                verdict.stale.iter().map(|(_, n)| n).sum::<usize>(),
                keep.len()
            );
            return true;
        }
        Some(other) => {
            eprintln!("usage: cargo xtask waits [--list | --shrink] (not `{other}`)");
            return false;
        }
        None => {}
    }
    let verdict = judge(&found, &listed(&list_file));
    for site in &verdict.new {
        eprintln!(
            "{SUITES}/{}:{}: a fixed pause followed by one read:\n    {}\n    {}\n  Wait for what the second line asserts \
             (tryCompare, or tryVerify with a message made when the wait ends), not for a duration that guesses how long it \
             takes (docs/testing-strategy.md §6, issue #73).",
            site.key.0, site.line, site.key.1, site.key.2
        );
    }
    for ((file, pause, read), count) in &verdict.stale {
        eprintln!(
            "{LIST}: {count} entr{} for {file} no longer match{} a site (fixed, or edited):\n    {pause}\n    {read}\n  \
             Remove {} (`cargo xtask waits --shrink`): the list only shrinks.",
            if *count == 1 { "y" } else { "ies" },
            if *count == 1 { "es" } else { "" },
            if *count == 1 { "it" } else { "them" },
        );
    }
    let ok = verdict.new.is_empty() && verdict.stale.is_empty();
    println!(
        "waits: {} sites in the suites, {} listed to go{}",
        found.len(),
        listed(&list_file).values().sum::<usize>(),
        if ok { "" } else { ", with errors" }
    );
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> Vec<(String, String)> {
        sites("tst_x.qml", text)
            .into_iter()
            .map(|s| (s.key.1, s.key.2))
            .collect()
    }

    #[test]
    fn a_pause_followed_by_a_bare_read_is_a_site() {
        let found = one(
            "    app.width = 1100\n    wait(300)\n    compare(grid.columns, 6)\n    verify(true)\n",
        );
        assert_eq!(
            found,
            [(
                "wait(300)".to_string(),
                "compare(grid.columns, 6)".to_string()
            )]
        );
        assert_eq!(one("wait(60)\nverify(x)\n").len(), 1);
    }

    #[test]
    fn a_pause_followed_by_a_wait_or_by_something_else_is_not() {
        assert!(one("wait(300)\ntryCompare(grid, \"columns\", 6)\n").is_empty());
        assert!(one("wait(300)\ntryVerify(() => ok)\n").is_empty());
        assert!(one("wait(300)\nmouseClick(button)\ncompare(a, b)\n").is_empty());
        assert!(one("wait(300)\n").is_empty(), "nothing after it");
    }

    #[test]
    fn blank_lines_and_comments_between_the_two_are_skipped_but_only_three_lines_are_looked_at() {
        assert_eq!(
            one("wait(100)\n\n    // (the grid has settled)\n    compare(a, 1)\n").len(),
            1
        );
        assert!(one("wait(100)\nx = 1\ny = 2\nz = 3\ncompare(a, 1)\n").is_empty());
    }

    #[test]
    fn a_pause_that_is_the_body_of_a_loop_is_a_poll_and_not_a_site() {
        let poll = "while (dialog.phase !== phase && Date.now() < deadline)\n    wait(20)\ncompare(dialog.phase, phase)\n";
        assert!(one(poll).is_empty());
        let for_loop =
            "for (let waited = 0; waited < 30000; waited += 100)\n    wait(100)\ncompare(a, 1)\n";
        assert!(one(for_loop).is_empty());
        assert!(one("while (x) wait(20)\ncompare(a, 1)\n").is_empty());
        // A pause after the loop is not its body.
        assert_eq!(
            one("while (x)\n    y()\nwait(20)\ncompare(a, 1)\n").len(),
            1
        );
    }

    #[test]
    fn only_a_call_of_wait_with_a_number_counts() {
        assert!(
            one("// wait(300)\ncompare(a, 1)\n").is_empty(),
            "in a comment"
        );
        assert!(
            one("tc.wait(300)\ncompare(a, 1)\n").is_empty(),
            "a method of another object"
        );
        assert!(one("waitForRendering(item)\ncompare(a, 1)\n").is_empty());
        assert!(one("tryWait(300)\ncompare(a, 1)\n").is_empty());
        assert!(one("wait(delay)\ncompare(a, 1)\n").is_empty(), "no number");
        assert_eq!(one("click(b); wait(30) // then\nverify(x)\n").len(), 1);
    }

    #[test]
    fn a_site_is_known_by_its_lines_and_not_by_where_it_is() {
        let before = sites("tst_x.qml", "wait(300)\ncompare(a, 1)\n");
        let after = sites(
            "tst_x.qml",
            "// a line added above\nwait(300)\ncompare(a, 1)\n",
        );
        assert_eq!(before[0].key, after[0].key);
        assert_ne!((before[0].line), after[0].line);
    }

    #[test]
    fn a_new_site_is_refused_and_a_site_that_is_gone_is_refused_too() {
        let found = sites("tst_x.qml", "wait(300)\ncompare(a, 1)\n");
        // Listed: fine.
        let list = listed(&list_text(&found));
        let verdict = judge(&found, &list);
        assert!(verdict.new.is_empty() && verdict.stale.is_empty());
        // Not listed: a new one.
        let verdict = judge(&found, &BTreeMap::new());
        assert_eq!(verdict.new.len(), 1);
        // Listed but fixed (the suite has no such site any more): stale.
        let fixed = sites("tst_x.qml", "tryCompare(a, \"b\", 1)\n");
        let verdict = judge(&fixed, &list);
        assert!(verdict.new.is_empty());
        assert_eq!(verdict.stale.len(), 1);
    }

    #[test]
    fn a_site_that_occurs_twice_is_listed_twice_and_a_third_is_new() {
        let two = "wait(200)\nverify(x)\nwait(200)\nverify(x)\n";
        let found = sites("tst_x.qml", two);
        assert_eq!(found.len(), 2);
        let list = listed(&list_text(&found));
        assert_eq!(list.values().sum::<usize>(), 2);
        let three = sites("tst_x.qml", &format!("{two}wait(200)\nverify(x)\n"));
        let verdict = judge(&three, &list);
        assert_eq!(verdict.new.len(), 1);
        assert_eq!(verdict.new[0].line, 5, "the later one is the new one");
        // One of the two fixed: one entry stale.
        let one_left = sites("tst_x.qml", "wait(200)\nverify(x)\n");
        assert_eq!(judge(&one_left, &list).stale[0].1, 1);
    }

    #[test]
    fn the_list_ignores_comments_and_blank_lines() {
        let text = "# a comment\n\ntst_x.qml\twait(1)\tcompare(a, 1)\n";
        assert_eq!(listed(text).len(), 1);
    }
}
