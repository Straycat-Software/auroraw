// SPDX-License-Identifier: GPL-3.0-or-later
//! The command set (spec §3: "every action is a command reachable by keyboard, menu and later a
//! palette"): a small, explicit table so "every command has a keyboard shortcut, or a place in the
//! menu" is something a test can check by listing the table, rather than something only a person
//! clicking through the interface could notice was missing (testing strategy §6, D-153: a menu
//! command may have no key of its own, the menu is reached by `Alt` and a letter). The QML side is `qml/AppActions.qml`
//! (one `Action` per menu command, with its `commandId`) and `qml/AppMenu.qml`; the tests below hold
//! the two together. The grid's keys (rating, moving the selection) are the library view's own and
//! join the cross-check with it (milestone Q3).
//!
//! Grows with later work packages: WP9's cull mode adds flags, labels and series commands to this
//! same table; a menu and a command palette read it too, once built.

/// One command the interface can carry out and how a person reaches it from the keyboard.
pub struct CommandSpec {
    /// A stable identifier: the `commandId` of its `Action` in QML.
    pub id: &'static str,
    /// A name, for a future palette.
    pub name: &'static str,
    /// The keyboard shortcut, as shown to a person (not parsed). Empty only for a command of the menu (D-153).
    pub shortcut: &'static str,
    /// Whether the hamburger menu lists it (`qml/AppMenu.qml`).
    pub menu: bool,
}

const fn command(
    id: &'static str,
    name: &'static str,
    shortcut: &'static str,
    menu: bool,
) -> CommandSpec {
    CommandSpec {
        id,
        name,
        shortcut,
        menu,
    }
}

/// Every command the shell currently exposes.
pub const COMMANDS: &[CommandSpec] = &[
    command("grid.rate-0", "Rate 0 (clear)", "0", false),
    command("grid.rate-1", "Rate 1", "1", false),
    command("grid.rate-2", "Rate 2", "2", false),
    command("grid.rate-3", "Rate 3", "3", false),
    command("grid.rate-4", "Rate 4", "4", false),
    command("grid.rate-5", "Rate 5", "5", false),
    command("grid.flag-pick", "Pick", "P", false),
    command("grid.flag-reject", "Reject", "X", false),
    command("grid.flag-clear", "Clear the flag", "U", false),
    command("grid.label-red", "Red label", "6", false),
    command("grid.label-yellow", "Yellow label", "7", false),
    command("grid.label-green", "Green label", "8", false),
    command("grid.label-blue", "Blue label", "9", false),
    command("grid.open", "Open in the image view", "Return", false),
    command("grid.series-toggle", "Open or close the series", "E", false),
    command("grid.resolve", "Resolve the series", "R", false),
    command(
        "grid.group",
        "Group the selection as a series",
        "Ctrl+G",
        false,
    ),
    command(
        "grid.ungroup",
        "Take out of the series",
        "Ctrl+Shift+G",
        false,
    ),
    command(
        "grid.compare",
        "Compare the selection or the series",
        "C",
        false,
    ),
    command("grid.mark", "Mark or unmark to keep", "K", false),
    command(
        "grid.similar",
        "Show or hide the similar photos",
        "M",
        false,
    ),
    command("view.zoom", "Fit, or 100 %", "Z", false),
    command("view.peaking", "Show or hide focus peaking", "S", false),
    command(
        "view.clipping",
        "Show or hide the clipping warning",
        "O",
        false,
    ),
    command("view.histogram", "Show or hide the histogram", "H", false),
    command("view.full-screen", "Full screen", "F", false),
    command("view.info", "Show or hide the information", "I", false),
    command("view.filmstrip", "Show or hide the filmstrip", "T", false),
    command("view.auto-advance", "Auto-advance", "A", false),
    command("view.close", "Back to the grid", "Escape", false),
    command("grid.up", "Move selection up", "Up", false),
    command("grid.down", "Move selection down", "Down", false),
    command(
        "grid.left",
        "Move selection left (in the image view: the previous photo)",
        "Left",
        false,
    ),
    command(
        "grid.right",
        "Move selection right (in the image view: the next photo)",
        "Right",
        false,
    ),
    command(
        "grid.page-up",
        "Move selection one page up",
        "PageUp",
        false,
    ),
    command(
        "grid.page-down",
        "Move selection one page down",
        "PageDown",
        false,
    ),
    command("grid.home", "Select the first photo", "Home", false),
    command("grid.end", "Select the last photo", "End", false),
    command("file.new-workspace", "New workspace", "Ctrl+N", true),
    command("file.open-workspace", "Open workspace", "Ctrl+O", true),
    command("file.settings", "Settings", "Ctrl+,", true),
    command("file.import", "Import", "Ctrl+I", true),
    command("file.export-xmp", "Export XMP files", "Ctrl+Shift+E", true),
    command(
        "file.find-place-names",
        "Find place names",
        "Ctrl+Shift+L",
        true,
    ),
    command("file.quit", "Quit", "Ctrl+Q", true),
    command("edit.undo", "Undo", "Ctrl+Z", true),
    command("edit.redo", "Redo", "Ctrl+Y", true),
    command("edit.cut", "Cut", "Ctrl+X", true),
    command("edit.copy", "Copy", "Ctrl+C", true),
    command("edit.paste", "Paste", "Ctrl+V", true),
    command("edit.select-all", "Select all", "Ctrl+A", true),
    command("edit.keywords", "Keywords", "Ctrl+K", true),
    command("edit.duplicates", "Duplicate photos", "Ctrl+D", true),
    command("edit.select-none", "Select none", "Ctrl+Shift+A", true),
    command(
        "edit.invert-selection",
        "Invert selection",
        "Ctrl+Shift+I",
        true,
    ),
    // The View section (D-153): each filter of the library's bar, and what its other buttons do, is a command, so that a
    // person without a mouse reaches them. They have their place in the menu and no key of their own, but Clear all filters.
    command("filter.rating-0", "Show photos with any rating", "", true),
    command(
        "filter.rating-1",
        "Show photos rated 1 star or more",
        "",
        true,
    ),
    command(
        "filter.rating-2",
        "Show photos rated 2 stars or more",
        "",
        true,
    ),
    command(
        "filter.rating-3",
        "Show photos rated 3 stars or more",
        "",
        true,
    ),
    command(
        "filter.rating-4",
        "Show photos rated 4 stars or more",
        "",
        true,
    ),
    command("filter.rating-5", "Show photos rated 5 stars", "", true),
    command(
        "filter.flags-0",
        "Show the photos that are not rejected",
        "",
        true,
    ),
    command(
        "filter.flags-1",
        "Show every photo, rejected ones included",
        "",
        true,
    ),
    command("filter.flags-2", "Show the picked photos", "", true),
    command("filter.flags-3", "Show the rejected photos", "", true),
    command(
        "filter.colour-red",
        "Show the photos with the red label",
        "",
        true,
    ),
    command(
        "filter.colour-yellow",
        "Show the photos with the yellow label",
        "",
        true,
    ),
    command(
        "filter.colour-green",
        "Show the photos with the green label",
        "",
        true,
    ),
    command(
        "filter.colour-blue",
        "Show the photos with the blue label",
        "",
        true,
    ),
    command(
        "filter.colour-purple",
        "Show the photos with the purple label",
        "",
        true,
    ),
    command(
        "filter.colour-any",
        "Show the photos whatever their colour label",
        "",
        true,
    ),
    command(
        "filter.series-0",
        "Show the photos whether or not they are in a series",
        "",
        true,
    ),
    command(
        "filter.series-1",
        "Show the photos that are in a series",
        "",
        true,
    ),
    command(
        "filter.series-2",
        "Show the photos of series that are not resolved",
        "",
        true,
    ),
    command(
        "filter.series-3",
        "Show the photos of resolved series",
        "",
        true,
    ),
    command("filter.series-open-all", "Open every series", "", true),
    command("filter.series-close-all", "Close every series", "", true),
    command("filter.clear", "Clear all filters", "Ctrl+Shift+X", true),
    command("view.refresh", "Refresh the list", "", true),
    command("view.export-list", "Export the list", "", true),
    command("edit.delete", "Delete", "Del", true),
    command("help.about", "About", "F1", true),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn qml_file(name: &str) -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("qml")
                .join(name),
        )
        .unwrap()
    }

    /// What the `shortcut:` of a menu command's `Action` says: a Qt standard key where the platform
    /// has one (so the Mac gets its own), else the sequence itself.
    fn qml_shortcut(id: &str) -> &'static str {
        match id {
            "file.new-workspace" => "StandardKey.New",
            "file.open-workspace" => "StandardKey.Open",
            "file.quit" => "StandardKey.Quit",
            "edit.undo" => "StandardKey.Undo",
            "edit.redo" => "StandardKey.Redo",
            "edit.cut" => "StandardKey.Cut",
            "edit.copy" => "StandardKey.Copy",
            "edit.paste" => "StandardKey.Paste",
            "edit.select-all" => "StandardKey.SelectAll",
            "edit.delete" => "StandardKey.Delete",
            "help.about" => "StandardKey.HelpContents",
            "edit.select-none" => "\"Ctrl+Shift+A\"",
            "edit.invert-selection" => "\"Ctrl+Shift+I\"",
            "edit.keywords" => "\"Ctrl+K\"",
            "edit.duplicates" => "\"Ctrl+D\"",
            "file.settings" => "\"Ctrl+,\"",
            "file.import" => "\"Ctrl+I\"",
            "file.export-xmp" => "\"Ctrl+Shift+E\"",
            "file.find-place-names" => "\"Ctrl+Shift+L\"",
            "filter.clear" => "\"Ctrl+Shift+X\"",
            other => panic!("{other} has no shortcut spelled in the test yet"),
        }
    }

    /// The `Action`s of `AppActions.qml`, as (command id, text of its block).
    fn actions() -> Vec<(String, String)> {
        let text = qml_file("AppActions.qml");
        let mut found = Vec::new();
        let mut rest = text.as_str();
        while let Some(at) = rest.find("commandId: \"") {
            rest = &rest[at + "commandId: \"".len()..];
            let id = &rest[..rest.find('"').unwrap()];
            let block_end = rest.find("\n    }\n").unwrap_or(rest.len());
            found.push((id.to_string(), rest[..block_end].to_string()));
        }
        found
    }

    /// Every menu command has an `Action` with the shortcut the table says, and every `Action` is a
    /// command of the table: the two cannot drift apart. A textual check, not a semantic one: it
    /// would not catch an action wired to the wrong handler.
    #[test]
    fn every_menu_command_is_an_action_with_its_shortcut() {
        let actions = actions();
        for command in COMMANDS.iter().filter(|c| c.menu) {
            let (_, block) = actions
                .iter()
                .find(|(id, _)| id == command.id)
                .unwrap_or_else(|| panic!("{}: no Action in AppActions.qml", command.id));
            if command.shortcut.is_empty() {
                // A command of the menu with no key of its own (D-153): its Action says none either.
                assert!(
                    !block.contains("shortcut:"),
                    "{}: the table gives no shortcut and AppActions.qml gives one",
                    command.id
                );
                continue;
            }
            let expected = format!("shortcut: {}", qml_shortcut(command.id));
            assert!(
                block.contains(&expected),
                "{}: AppActions.qml does not say {expected:?}",
                command.id
            );
        }
        for (id, _) in &actions {
            assert!(
                COMMANDS.iter().any(|c| c.id == id && c.menu),
                "AppActions.qml has {id:?}, which is not a menu command of the table"
            );
        }
    }

    /// The menu lists each action of the table's menu commands once, in `AppMenu.qml`.
    #[test]
    fn the_menu_lists_exactly_the_menu_commands() {
        let menu = qml_file("AppMenu.qml");
        let actions = qml_file("AppActions.qml");
        let mut listed = Vec::new();
        for line in menu.lines() {
            let Some(at) = line.find("root.actions.") else {
                continue;
            };
            let name = line[at + "root.actions.".len()..]
                .split(|c: char| !c.is_alphanumeric())
                .next()
                .unwrap();
            // The Action property named `name`, and its command id.
            let start = actions
                .find(&format!("readonly property Action {name}:"))
                .unwrap_or_else(|| panic!("AppMenu.qml lists {name}, which AppActions.qml lacks"));
            let block = &actions[start..];
            let at = block.find("commandId: \"").unwrap() + "commandId: \"".len();
            let id = &block[at..at + block[at..].find('"').unwrap()];
            listed.push(id.to_string());
        }
        for command in COMMANDS {
            let count = listed.iter().filter(|id| *id == command.id).count();
            assert_eq!(
                count,
                usize::from(command.menu),
                "{}: menu = {} in the table, listed {count} time(s) in AppMenu.qml",
                command.id,
                command.menu
            );
        }
        for id in &listed {
            assert!(
                COMMANDS.iter().any(|c| c.id == id),
                "{id:?} is not a command"
            );
        }
    }

    /// A command is reached by a key or from the menu (spec §3: keyboard first; D-153): the menu opens with `Alt` and a
    /// letter, so a command that is in it needs no key of its own, and one that is not in a menu needs one.
    #[test]
    fn every_command_has_a_shortcut_or_a_place_in_the_menu() {
        for command in COMMANDS {
            assert!(
                !command.shortcut.is_empty() || command.menu,
                "{} has no keyboard shortcut and is not in the menu (spec §3: keyboard first)",
                command.name
            );
        }
    }

    #[test]
    fn no_two_commands_share_a_shortcut_or_an_id() {
        for (i, a) in COMMANDS.iter().enumerate() {
            for b in &COMMANDS[i + 1..] {
                // (Commands of the menu with no key of their own all have the empty shortcut.)
                if !a.shortcut.is_empty() {
                    assert_ne!(
                        a.shortcut, b.shortcut,
                        "{} and {} both claim {:?}",
                        a.name, b.name, a.shortcut
                    );
                }
                assert_ne!(a.id, b.id);
            }
        }
    }

    /// The keyboard page of the user manual (`docs/manual/keyboard-shortcuts.md`) mentions every shortcut of the
    /// table, so that a key cannot be added in silence: whoever adds a command adds its line to the manual (D-102).
    #[test]
    fn the_manual_lists_every_shortcut_of_the_table() {
        let page = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/manual/keyboard-shortcuts.md"),
        )
        .expect("the manual has a keyboard page");
        for command in COMMANDS.iter().filter(|c| !c.shortcut.is_empty()) {
            assert!(
                page.contains(&format!("`{}`", command.shortcut)),
                "{} ({}): docs/manual/keyboard-shortcuts.md does not mention `{}`",
                command.id,
                command.name,
                command.shortcut
            );
        }
    }
}
