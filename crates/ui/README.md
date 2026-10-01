# auroraw-ui

The Qt Quick user interface (decision D-094; it replaced a first one in Slint, D-072), which
`cargo run -p auroraw-app` runs.

- `src/`: thin Rust objects over the engine (`launcher`, the list models in `models`, the event `bus`),
  the C++ glue (`glue.cpp`: the asynchronous thumbnail image provider, the translation loader and the
  shortcut writer; `quicktest.cpp`: the test runner's entry), and the pure logic (`app_settings`,
  `gridmath`, `commands`).
- `qml/`: the screens. `Theme.qml` holds the neutral grey and the accent colours; the style is Fusion
  everywhere.
- `i18n/`: Qt Linguist `.ts` files, compiled by `lrelease` in `build.rs`. Update them after changing a
  `qsTr` string with `cargo xtask translations`, then translate what is new. It runs Qt's `lupdate` on both
  files and **puts back the locations the committed messages already had** (`lupdate` rewrites every
  location, which makes one new string a diff of a thousand lines): the diff is the new messages and
  nothing else, and it says which messages are new and which are gone (a reworded one loses its translation: its old text is the place to start). On a branch stacked on another, `cargo xtask translations origin/<base>` keeps the base's
  locations instead of `HEAD`'s. English is the source text but has a file too (`auroraw_en.ts`), because
  only a translation can hold plural forms (`qsTr("%n photo(s)", "", count)`): give the new plural messages
  their forms there and leave the rest as it is. `tests/translations.rs` checks them.
- `tests/qml/`: QtQuickTest suites, run offscreen with real input events by `tests/qml.rs`, one process
  per suite, each on a throwaway machine (`AURORAW_TEST_HOME`). To look at what a run draws, add a
  `grabImage(...).save(...)` to a test.

Trying a typeface without a rebuild: `AURORAW_FONT="Inter" AURORAW_FONT_SIZE=13 cargo run -p auroraw-app
`.
