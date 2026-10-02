# Contributing to Auroraw

Thank you for your interest. Auroraw is a free application for photographers (organise, develop
RAW files and negative scans non-destructively, convert, edit metadata, deliver galleries).

**Where the project is.** The design is complete (specification, architecture, testing strategy, release plan,
governance, the plan of milestone M1), four technical spikes have measured the technology, and
the product code has begun with milestone M1 (its first work package lays out the repository). For now the
most useful contributions are **reading the documents in [docs/](docs/) and telling us what is
wrong or missing**, and **sample files** (see below).

## Building and testing

The interface is Qt Quick (D-094), so building the workspace needs **Qt 6.4 or later** (6.8 is the one
tested in CI), **cmake 3.24 or later**, a C++ compiler, and `qmake` findable (on the path, or in the
`QMAKE` variable). Qt's tools `lrelease` and `lupdate` must be in the same install (the translations
are compiled at build time).

- **Ubuntu 24.04**: `sudo apt install qt6-base-dev qt6-declarative-dev qt6-l10n-tools qt6-tools-dev-tools
  qml6-module-qtquick qml6-module-qtquick-controls qml6-module-qtquick-dialogs
  qml6-module-qtquick-layouts qml6-module-qtquick-templates qml6-module-qtquick-window
  qml6-module-qtqml qml6-module-qtqml-models qml6-module-qttest cmake lld`, then
  `export QMAKE=/usr/bin/qmake6`. (`lld` is optional: without it cxx-qt links with `gold` and cargo
  prints a warning.)
- **Windows and macOS**: Qt from the online installer, or `pip install aqtinstall` then
  `aqt install-qt <host> desktop 6.8.3 <arch>` (the CI uses `jurplel/install-qt-action`), with its
  `bin` folder on the path.

```bash
cargo build --workspace
cargo test --workspace        # or: cargo nextest run --workspace
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps   # a link in a doc comment to a private or missing item fails
cargo xtask check             # SPDX headers, the allowed dependencies between crates, the QML waits (see below)
```

Sample RAW files for the tests come from `tools/fetch-samples.sh` (seventeen CC0 files, about 490 MB, kept
out of git in `testdata/samples/`).
The file of place names (design note 008) is built from public data by `tools/fetch-places.sh` (about 65 MB
of downloads, a 24 MB file, kept out of git in `testdata/places/`); the tests of `auroraw-places` on the real
boundaries are skipped without it, and everything else needs nothing of it.

## Ground rules

- **Everything is in English**: code, comments, commit messages, documents, issues and
  discussions. This keeps the project readable by everyone who may contribute.
- **Be kind.** The [code of conduct](CODE_OF_CONDUCT.md) applies everywhere. Reports go to
  auroraw-conduct@straycat.ca.
- **Decisions are recorded.** What Auroraw does, its file formats, the plugin API, the licence
  and the governance change only through an approved decision in
  [docs/decisions.md](docs/decisions.md). See [docs/governance.md](docs/governance.md).

## Proposing a change

1. **Open an issue first** for anything larger than a small fix: what problem, why, what
   alternatives. A pull request with a new feature and no prior issue may be declined without a
   review of the code.
2. Keep pull requests **small and focused**: one concern each.
3. Branch from `dev` and open the pull request against `dev`. `main` holds releases only.

## Sign your commits (the DCO)

Every commit must carry a `Signed-off-by` line, which certifies that you wrote the change or
have the right to submit it under the project's licence
([Developer Certificate of Origin](https://developercertificate.org/)):

```bash
git commit -s
```

A check on each pull request refuses commits without it. If you used AI tools to write a
substantial part of a change, say so in the pull request; you remain responsible for it.

## Licences

- The application is **GPL-3.0-or-later**. Every source file starts with
  `SPDX-License-Identifier: GPL-3.0-or-later`.
- The plugin API, the plugin SDK, the declaration schema and the example plugins are
  **MIT OR Apache-2.0**, and say so in their headers.
- The documentation in `docs/` is **CC BY-SA 4.0**.
- Plugins that use only the plugin API may have any licence ([PLUGIN-EXCEPTION](PLUGIN-EXCEPTION)).
- A new dependency must have a licence compatible with the GPL-3.0 (no AGPL, proprietary or
  non-commercial licences) and must be justified in the pull request.

## Before you ask for a review

The pull request template lists what "ready" means. In short: it builds without warnings on
Linux, Windows and macOS; it has tests (a bug fix has a regression test); a format change carries
a fixture and a migration; new strings are translatable and new controls are keyboard-reachable
and named for screen readers; a change a photographer can see is described in the
[user manual](docs/manual/README.md) (a page, a line in the keyboard shortcuts, a picture if a view changed:
`cargo xtask manual-images`). The reasons are in [docs/testing-strategy.md](docs/testing-strategy.md) §9.

## Translations

Translations are Qt Linguist `.ts` files (`crates/ui/i18n/auroraw_<code>.ts`, editable with Qt
Linguist or any text editor), **sent as pull requests**. Say in an issue which language
you take, so that nobody works on the same one twice. A language ships when it is at least 90 %
translated for the main views, passes the automatic checks, and has a named person who reviews
its changes. Source strings are in English. See [docs/governance.md](docs/governance.md) §5.

## Sample files

A camera whose files do not open needs a sample. Only files that are **CC0 or otherwise
redistributable** can be used (raw.pixls.us accepts such files). Do not send photos of people
without their permission.

## Security

Report vulnerabilities privately (see [SECURITY.md](SECURITY.md)), not as public issues.
