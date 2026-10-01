// SPDX-License-Identifier: GPL-3.0-or-later
//! `cargo xtask translations`: updates the interface's Linguist files (`crates/ui/i18n/*.ts`) from the QML
//! with Qt's `lupdate`, and **puts back the locations of the messages that were already there**.
//!
//! `lupdate` rewrites every `<location>` of a file, so one new string shows up as a diff of a thousand lines
//! (the form depends on the Qt version too), which buries the change and conflicts with every other branch that
//! touches the files. Here the messages the committed file already had keep the locations it gave them, and
//! only the new ones carry the ones `lupdate` wrote: the diff is the new messages and nothing else. (A location
//! is only a hint for a translator's tool; nothing reads it, and a line number that has drifted is harmless.)
//!
//! A message is the same message when its context (the component), its source text and its comment are the
//! same. What is new in the files is left for a translator: French is translated by hand, and English holds
//! only the plural forms (`crates/ui/README.md`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The languages the interface has a file for (`auroraw_<code>.ts`); `en` holds the plural forms of the source.
const LANGUAGES: &[&str] = &["fr", "en"];

/// A message's identity: its context, its source text and its comment, as the file spells them.
type Key = (String, String, Option<String>);

/// A `.ts` file as a sequence of lines, with each `<message>` gathered.
enum Piece {
    Line(String),
    Message {
        context: String,
        /// From the `<message …>` line to the `</message>` line.
        lines: Vec<String>,
    },
}

fn pieces(text: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut context = String::new();
    let mut open: Option<Vec<String>> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(lines) = &mut open {
            lines.push(line.to_string());
            if trimmed.starts_with("</message>") {
                let lines = open.take().unwrap_or_default();
                out.push(Piece::Message {
                    context: context.clone(),
                    lines,
                });
            }
        } else if trimmed.starts_with("<message") {
            open = Some(vec![line.to_string()]);
        } else {
            if let Some(name) = trimmed
                .strip_prefix("<name>")
                .and_then(|rest| rest.strip_suffix("</name>"))
            {
                context = name.to_string();
            }
            out.push(Piece::Line(line.to_string()));
        }
    }
    // (A message that never ends is not a file Linguist wrote: keep what there is, untouched.)
    if let Some(lines) = open {
        out.extend(lines.into_iter().map(Piece::Line));
    }
    out
}

/// What is between `<tag>` and `</tag>` in `lines`, as written (escapes included), which may span lines.
fn tag_text(lines: &[String], tag: &str) -> Option<String> {
    let text = lines.join("\n");
    let start = text.find(&format!("<{tag}>"))? + tag.len() + 2;
    let end = start + text[start..].find(&format!("</{tag}>"))?;
    Some(text[start..end].to_string())
}

/// The message's key, and its `<location>` lines apart from the others (the opening line is not in either).
fn split(context: &str, lines: &[String]) -> (Key, Vec<String>, Vec<String>) {
    let (locations, rest): (Vec<String>, Vec<String>) = lines[1..]
        .iter()
        .cloned()
        .partition(|line| line.trim_start().starts_with("<location"));
    let key = (
        context.to_string(),
        tag_text(&rest, "source").unwrap_or_default(),
        tag_text(&rest, "comment"),
    );
    (key, locations, rest)
}

/// What [`restore`] did.
pub struct Restored {
    /// The new file, its messages that `old` had carrying `old`'s locations.
    pub text: String,
    /// How many messages kept their locations.
    pub kept: usize,
    /// How many messages are new (`old` did not have them).
    pub added: usize,
    /// The messages `old` had and `new` no longer has (context and source text): reworded, or removed. A reworded one
    /// loses its translation (that is `lupdate`'s doing, with `-no-obsolete`), and its old text is the translator's
    /// starting point for the new one.
    pub gone: Vec<(String, String)>,
}

/// `new` (what `lupdate` wrote) with the locations of the messages `old` (the committed file) already had.
pub fn restore(old: &str, new: &str) -> Restored {
    let mut known: HashMap<Key, Vec<String>> = HashMap::new();
    let mut seen: std::collections::HashSet<Key> = std::collections::HashSet::new();
    for piece in pieces(old) {
        if let Piece::Message { context, lines } = piece {
            let (key, locations, _) = split(&context, &lines);
            known.insert(key, locations);
        }
    }
    let newline = if new.contains("\r\n") { "\r\n" } else { "\n" };
    let (mut kept, mut added) = (0, 0);
    let mut out: Vec<String> = Vec::new();
    for piece in pieces(new) {
        match piece {
            Piece::Line(line) => out.push(line),
            Piece::Message { context, lines } => {
                let (key, locations, rest) = split(&context, &lines);
                seen.insert(key.clone());
                let locations = match known.get(&key) {
                    Some(old_locations) => {
                        kept += 1;
                        old_locations.clone()
                    }
                    None => {
                        added += 1;
                        locations
                    }
                };
                out.push(lines[0].clone());
                out.extend(locations);
                out.extend(rest);
            }
        }
    }
    let mut text = out.join(newline);
    if new.ends_with('\n') {
        text.push_str(newline);
    }
    let mut gone: Vec<(String, String)> = known
        .into_keys()
        .filter(|key| !seen.contains(key))
        .map(|(context, source, _)| (context, source))
        .collect();
    gone.sort();
    gone.dedup();
    Restored {
        text,
        kept,
        added,
        gone,
    }
}

/// The directory of Qt's command line tools, from the `qmake` the interface's build uses (`QMAKE`, or `qmake6`, or
/// `qmake`): the same way `crates/ui/build.rs` finds `lrelease`.
fn qt_bins() -> Result<PathBuf, String> {
    let qmake = std::env::var("QMAKE").unwrap_or_else(|_| {
        if Command::new("qmake6").arg("-v").output().is_ok() {
            "qmake6".into()
        } else {
            "qmake".into()
        }
    });
    let out = Command::new(&qmake)
        .args(["-query", "QT_INSTALL_BINS"])
        .output()
        .map_err(|e| format!("cannot run `{qmake} -query QT_INSTALL_BINS`: {e}"))?;
    Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

fn lupdate() -> Result<PathBuf, String> {
    let bins = qt_bins()?;
    ["lupdate", "lupdate.exe"]
        .iter()
        .map(|name| bins.join(name))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            format!(
                "Qt's lupdate is not in {} (install Qt's tools: qt6-l10n-tools on Debian and Ubuntu, \
                 the qttools module with aqtinstall)",
                bins.display()
            )
        })
}

/// The file as `reference` (a commit, `HEAD` by default) has it, or `None` when it has none.
fn committed(root: &Path, reference: &str, file: &str) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["show", &format!("{reference}:{file}")])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `cargo xtask translations [<ref>]`: the messages that `<ref>` (default `HEAD`; the branch's base in a stack) has
/// keep their locations.
pub fn run(reference: Option<String>) -> bool {
    let reference = reference.unwrap_or_else(|| "HEAD".into());
    let root = crate::root();
    let ui = root.join("crates").join("ui");
    let lupdate = match lupdate() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("{e}");
            return false;
        }
    };
    let mut screens: Vec<String> = std::fs::read_dir(ui.join("qml"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".qml"))
        .map(|name| format!("qml/{name}"))
        .collect();
    screens.sort();
    for language in LANGUAGES {
        let file = format!("crates/ui/i18n/auroraw_{language}.ts");
        let status = Command::new(&lupdate)
            .args(&screens)
            // (Absolute locations: a new message's own location says the file and the line whole. A relative one is
            // "so many lines after the previous message's", and the previous message has just got its old location
            // back, so it would point at a place that has nothing to do with the new string.)
            .args([
                "-no-obsolete",
                "-locations",
                "absolute",
                "-ts",
                &format!("i18n/auroraw_{language}.ts"),
            ])
            .args(["-source-language", "en", "-target-language", language])
            .current_dir(&ui)
            .output();
        match status {
            Ok(out) if out.status.success() => {}
            Ok(out) => {
                eprintln!(
                    "lupdate failed on {file}:\n{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                return false;
            }
            Err(e) => {
                eprintln!("cannot run lupdate: {e}");
                return false;
            }
        }
        let path = root.join(&file);
        let Some(old) = committed(&root, &reference, &file) else {
            println!("{file}: not in {reference}, left as lupdate wrote it");
            continue;
        };
        let new = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("{file}: {e}");
                return false;
            }
        };
        let restored = restore(&old, &new);
        if let Err(e) = std::fs::write(&path, &restored.text) {
            eprintln!("{file}: {e}");
            return false;
        }
        println!(
            "{file}: {} messages keep the locations of {reference}, {} are new (translate them: French by hand, \
             English only the plural forms)",
            restored.kept, restored.added
        );
        if !restored.gone.is_empty() {
            println!(
                "{file}: {} of {reference}'s messages are gone (reworded, or removed); if one was reworded, its \
                 old translation is the place to start:",
                restored.gone.len()
            );
            for (context, source) in &restored.gone {
                let shown: String = source.chars().take(80).collect();
                println!("    {context}: {shown}");
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<TS version=\"2.1\" language=\"fr\">\n<context>\n    <name>Menu</name>\n    <message>\n        <location filename=\"../qml/Menu.qml\" line=\"12\"/>\n        <source>Open</source>\n        <translation>Ouvrir</translation>\n    </message>\n    <message>\n        <location line=\"+5\"/>\n        <source>Close</source>\n        <comment>a menu</comment>\n        <translation>Fermer</translation>\n    </message>\n</context>\n<context>\n    <name>Dialog</name>\n    <message>\n        <location filename=\"../qml/Dialog.qml\" line=\"3\"/>\n        <source>Open</source>\n        <translation>Ouvert</translation>\n    </message>\n</context>\n</TS>\n";

    /// What `lupdate` would write after a line was added at the top of `Menu.qml` and a message to `Dialog.qml`:
    /// every location moved, and the existing messages' translations kept.
    const NEW: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<TS version=\"2.1\" language=\"fr\">\n<context>\n    <name>Menu</name>\n    <message>\n        <location filename=\"../qml/Menu.qml\" line=\"13\"/>\n        <source>Open</source>\n        <translation>Ouvrir</translation>\n    </message>\n    <message>\n        <location line=\"+5\"/>\n        <source>Close</source>\n        <comment>a menu</comment>\n        <translation>Fermer</translation>\n    </message>\n</context>\n<context>\n    <name>Dialog</name>\n    <message>\n        <location filename=\"../qml/Dialog.qml\" line=\"4\"/>\n        <source>Open</source>\n        <translation>Ouvert</translation>\n    </message>\n    <message>\n        <location line=\"+7\"/>\n        <source>Save</source>\n        <translation type=\"unfinished\"></translation>\n    </message>\n</context>\n</TS>\n";

    #[test]
    fn the_messages_the_file_had_keep_their_locations_and_the_new_one_has_its_own() {
        let restored = restore(OLD, NEW);
        assert_eq!((restored.kept, restored.added), (3, 1));
        // Same as the new file, but for the three old locations being the old ones (12 and 3, not 13 and 4).
        let expected = NEW
            .replace("line=\"13\"", "line=\"12\"")
            .replace("line=\"4\"", "line=\"3\"");
        assert_eq!(restored.text, expected);
        // (The new message's own location is what lupdate wrote.)
        assert!(restored.text.contains("<location line=\"+7\"/>"));
    }

    #[test]
    fn a_message_is_the_same_by_its_context_its_source_and_its_comment() {
        // "Open" in `Dialog` is not "Open" in `Menu`: each keeps its own location.
        let restored = restore(OLD, NEW);
        let menu = restored.text.find("<name>Menu</name>").unwrap();
        let dialog = restored.text.find("<name>Dialog</name>").unwrap();
        assert!(restored.text[menu..dialog].contains("Menu.qml\" line=\"12\""));
        assert!(restored.text[dialog..].contains("Dialog.qml\" line=\"3\""));
        // The same text with another comment is another message: it is new.
        let other = NEW.replace("<comment>a menu</comment>", "<comment>another</comment>");
        assert_eq!(restore(OLD, &other).added, 2);
    }

    #[test]
    fn a_message_the_new_file_no_longer_has_is_reported_as_gone() {
        // `Dialog`'s "Open" was reworded: the old one is gone, and the new one is new.
        let reworded = NEW.replace(
            "<source>Open</source>\n        <translation>Ouvert",
            "<source>Opened</source>\n        <translation>Ouvert",
        );
        let restored = restore(OLD, &reworded);
        assert_eq!(
            restored.gone,
            vec![("Dialog".to_string(), "Open".to_string())]
        );
        assert_eq!(restored.added, 2, "the reworded one and the new one");
        assert!(restore(OLD, NEW).gone.is_empty());
    }

    #[test]
    fn nothing_changes_when_nothing_was_new() {
        let restored = restore(OLD, OLD);
        assert_eq!(restored.text, OLD);
        assert_eq!((restored.kept, restored.added), (3, 0));
    }

    #[test]
    fn line_endings_and_the_last_newline_are_kept() {
        let crlf = NEW.replace('\n', "\r\n");
        let restored = restore(OLD, &crlf);
        assert!(restored.text.contains("\r\n") && restored.text.ends_with("\r\n"));
        assert!(!restored.text.replace("\r\n", "").contains('\n'));
        let bare = restore(OLD, NEW.trim_end());
        assert!(!bare.text.ends_with('\n'));
    }

    #[test]
    fn a_source_on_several_lines_is_one_source() {
        let old = "<context>\n    <name>A</name>\n    <message>\n        <location line=\"1\"/>\n        <source>one\ntwo</source>\n        <translation>un\ndeux</translation>\n    </message>\n</context>\n";
        let new = old.replace("line=\"1\"", "line=\"9\"");
        let restored = restore(old, &new);
        assert_eq!((restored.kept, restored.added), (1, 0));
        assert_eq!(restored.text, old);
    }
}
