# Auroraw: testing strategy

> **Status: adopted (D-081).** How Auroraw is tested: what is checked at which level, with
> what data, on which machines, and what stops a change from being merged. It builds on the
> [architecture](architecture.md) and on what the spikes already proved works. Items are tagged
> **[decided]** (a decision exists), **[proposed]** (to be validated) or **[open]**. Continuous
> integration itself (workflows, releases, packaging) is the next document; this one says *what*
> is run, that one says *where and when*.

## 1. Principles [proposed]

1. **Test where the risk is.** The risks that the spikes exposed are the ones tested hardest:
   shaders that behave differently on another graphics API, a catalogue that disagrees with its
   workspace, a sidecar that loses what it does not understand, a plugin that escapes its limits,
   a change that makes a drag slower than 50 ms.
2. **A photographer's data is never a test fixture.** Tests use generated data and public
   samples, in temporary folders, and never read the user's configuration, catalogues or photos.
3. **Every test is hermetic and deterministic**: no network, no clock dependence, fixed random
   seeds, its own temporary directory. A test that fails sometimes is a defect, fixed or removed
   within a week (§10).
4. **Most of the code is testable without a window or a GPU**, because the architecture makes it
   so (rules 3 and 6 of architecture §3.2). The interface is the thin layer that needs the
   least automated checking and the most human checking.
5. **A bug gets a test first.** A fix without a test that failed before it is incomplete.
6. **Speed budgets are tests**, run on known hardware (§7), not aspirations.

## 2. The levels [proposed]

| Level | What it checks | Where | Runs |
| --- | --- | --- | --- |
| **Unit** | A function or type in one module | `cargo test`, next to the code | Every change, all platforms |
| **Property** | Invariants over generated inputs: round trips, orderings, "never panics" | `proptest` | Every change (few cases), nightly (many) |
| **Reference** | A GPU stage against its CPU reference; the same pixels on every graphics API | `pipeline` tests, smoke test | Every change (software adapters); real GPUs on Patrick's machines |
| **Format** | Sidecars and state files: round trip, unknown content kept, every past version still readable | `format` tests with a fixture per schema version | Every change |
| **Catalogue** | Queries, migrations, and that a **rebuild equals the original** | `catalogue`, `workspace` tests on generated workspaces | Every change; the 100,000-photo case nightly |
| **Engine** | Whole scenarios through the `cli` and the engine API, no window: import, cull, keyword, develop, export, rebuild | `engine` integration tests | Every change |
| **Plugin** | The host's limits and permissions, and each plugin family's interface | `plugin-host` tests with hostile plugins; a conformance kit for plugin authors | Every change |
| **Fuzz** | Parsers and decoders given hostile input: no crash, no hang, no memory growth | `cargo-fuzz` targets | Nightly, a few minutes each |
| **Crash consistency** | A process killed at any point of a write leaves a workspace and catalogue that reconcile | fault-injection tests | Every change (short), nightly (long) |
| **Interface** | The accessibility tree, the translations, the models; screenshots of key views | Qt Quick Test, the AT-SPI script | Every change (model level), release (screenshots) |
| **Performance** | Budgets of the specification | The benchmark harness of the spikes | Nightly on Patrick's machines; trend on CI |
| **Manual** | Colour on a real display, screen readers, feel of the drag, packaging | A checklist | Each release, and when a risky area changes |

The proportions follow the architecture: many unit, property and format tests; a solid layer of
engine scenarios; few interface tests.

## 3. What is tested, module by module [proposed]

**`format`.** For every file type: `write(read(x)) == x` on real files, `read(write(m)) == m` on
generated models (property), **unknown elements and attributes survive an edit** (an
Auroraw-newer file edited by an older reader), and a **fixture for every schema version ever
released**, kept forever, so no upgrade breaks an old workspace. A malformed file returns an
error naming the file and the position, never a panic.

**`catalogue` and `workspace`.** Migrations from every schema version, on a catalogue with data.
The **rebuild test of spike 3 becomes permanent**: build a workspace, build the catalogue from
it, rebuild from scratch, compare row by row and by aggregates (this test found no differences
in the spike and would catch any field that is written to the database but not to the sidecar).
Queries are tested against a **slow, obviously correct reference** (a filter written as a plain
loop over the model) on generated data, to catch the wrong result rather than the wrong speed.
The effective rating is checked after every kind of change (photo, version, main version
changed, version deleted).

**Crash consistency.** The write path (architecture §5.3) is the core promise. A test harness
makes the sidecar write and the database transaction fail, or stops the process, at each step
(before the temporary file, after it, after the rename, mid-transaction), then reopens and runs
the reconciliation, and asserts: no edit lost, no half-written file, database never *ahead* of the
workspace, and the state after reconciliation equals the state of a rebuild.

**`sources` and `import`.** A fake source (in memory) that can go offline, return errors, be slow
or lie about sizes; a temporary folder tree with cards' folder layouts; RAW+JPEG pairs; a copy
interrupted at every stage and resumed; a corrupted copy detected by checksum; a card that is
**never modified** (the test compares a checksum of the whole tree before and after, D-031).

**`imaging` and colour.** Decoders are tested on the public sample files for every maker
(§5), against stored **checksums of the decoded mosaic** (the spike found native and WebAssembly
identical), and on truncated and corrupt versions of them (they must fail, not hang). Colour:
a synthetic chart of known colours through the pipeline against **reference values computed
independently** (a small Rust routine, not the shaders), with the tolerance of §4.

**`pipeline`.** Section 4.

**`develop`.** History as a property: any sequence of edits, undos and redos ends in the state
that replaying the surviving edits would give. Styles: applying a style to a version changes
exactly its operations. A version sidecar written by the current release **renders the same
pixels** in every later release (a golden render per operation and per released operation
version, §4), and one with a missing plugin opens with the operation disabled and its settings
kept.

**`plugin-host`.** The hostile cases of spike 4 become permanent tests, on all three platforms:
panic, endless loop, memory bomb, deep recursion, denied file, path traversal (`../`, absolute
paths, Windows drive and UNC forms, symbolic links), network, process, thread, environment.
After each case, a normal plugin runs to prove the host is unharmed. A declaration that cannot be
satisfied is refused with its reason. The **GPU shader check** (D-077) has a corpus of shaders
that must be accepted and shaders that must be refused, each with the expected reason, and runs
the acceptance on all three graphics APIs.

**`export`.** A rendered export is compared with the golden image; the written metadata is read
back and compared with the effective values of the version (D-046), including the fields left out
by the recipe (location, serial number).

**`engine`.** Scenarios through the public commands: import 50 photos with two cameras and RAW+JPEG
pairs, cull, rate, keyword, make two versions, export, edit a sidecar from outside and confirm the
change is signalled and applied on confirmation, rebuild, and compare. A scenario is a small
script over the `cli`, so it doubles as an example for users.
**The command history** (D-096) is checked as a property: any sequence of edits, batches, undos and
redos ends in the state a model gives that keeps the whole world before every action; a batch is one
step and all or nothing; a never-rated photo goes back to unset; a step about a photo that has gone is
dropped without harming the rest (`crates/engine/tests/history.rs`). The interface tests check the
Edit menu's names, Ctrl+Z and Ctrl+Y on the grid, and that the photo undone is shown.

## 4. The image engine [decided by the spikes; detail proposed]

This is the highest-risk module, and the one where the spikes already fixed the method.

1. **A CPU reference for every stage**, written for clarity, not speed, in the test tree. It is
   what the shader is checked against, not a fallback.
2. **Tolerance.** GPU against reference, and any two graphics APIs against each other: at most
   **one level in 8 bits on at least 99.9 % of pixels**, measured on the final output, and a
   tighter bound on the linear f32 intermediates of each stage (spike 4 reached 1.5e-8 for a
   simple stage; the bound is set per stage from measurement, not from hope). Denoising and other
   neighbourhood operations are compared with a looser, stated tolerance.
3. **Inputs.** Synthetic images that expose specific faults (a ramp for banding, a zone plate for
   the demosaic, hard edges for haloes, clipped highlights, a black frame, a white frame, extreme
   parameter values, tiny and huge sizes, sizes that are not a multiple of the tile), plus the
   real sample RAW files for every input path (Bayer, X-Trans, linear).
4. **The smoke test** of spike 1 is kept and grows: it compiles every shader and runs each on a
   known input on every platform, as a **blocking** step. It caught the DirectX-only compile
   failure that the other two APIs accepted, and it stays blocking.
5. **Tiling and banding equal the whole.** A property test: the render of an image in bands,
   with the smallest allowed band, equals the render in one piece to the same tolerance.
   This is the test that finds halo errors.
6. **Stage cache correctness.** After any sequence of parameter changes, the cached result equals
   a render from scratch (property test), and a change to a late stage reruns nothing earlier
   (counted, not timed).
7. **Memory pressure.** A test adapter or wrapper that refuses allocations above a limit: the
   engine must shrink, retry or fail cleanly, never panic or hang (the out-of-memory failures of
   spike 1). Device loss is injected on the software adapter.
8. **Determinism.** The same input on the same adapter twice gives byte-identical output.
9. **Golden renders** at 8 bits, stored as small PNG files with the parameters that produced them,
   for every operation, at its released versions. Golden images are updated only by an explicit,
   reviewed command that shows the difference; an update is a decision, not a side effect.
   **From the release of a pipeline definition version, the golden renders of its stages are part of
   its contract and are not regenerated**: the definition's fingerprint sees the names of its steps, not
   their maths, so the goldens are what sees the maths; a change that alters one is a new operation
   version or a new definition version (design note 006 §3.6), never an update.

Real GPUs are not available to continuous integration. CI therefore runs the reference and
smoke tests on **software adapters** (lavapipe on Linux, WARP on Windows, the Metal adapter of the
macOS runner) and Patrick's machines run the same tests on the real Vulkan, DirectX 12 and Metal
adapters, on demand and before each release [open: how to make that routine, see §7].

## 5. Test data [proposed]

| Data | Source | In the repository? |
| --- | --- | --- |
| **Sample RAW files** (about 225 MB for the seven of the spikes) | The CC0 files of raw.pixls.us, fetched by `fetch-samples.sh` with recorded **checksums** | **No**: downloaded and cached by CI |
| **Synthetic images** | Generated by code (ramps, zone plate, charts, edge cases) | Generated, never stored |
| **Golden renders** | Produced by the reference and checked by eye once | Yes, small PNGs, with their parameters |
| **A generated workspace and catalogue** | The dataset generator of spike 3, made deterministic from a seed and sizes (1,000 for every change, 100,000 nightly) | Generated |
| **Fixtures per format version** | Real sidecars and state files written by each release | Yes, tiny, kept forever |
| **Fuzz corpora** | Seeds from the fixtures and the samples' metadata; crashes found are added | Yes, small |
| **Hostile plugins** | The `hostile` plugin of spike 4 and new ones | Source in the repository, built by CI |

The samples cover a maker each, and the list is a place to add a file for **every camera bug
that is reported**. A sample must be CC0 or explicitly redistributable; nothing else enters.

## 6. The interface [proposed]

The toolkit is the layer least worth testing automatically, and the one where accessibility and
translation can fail unnoticed. What is checked:

- **Models** (the grid's virtualised rows, the tree, the filters): unit tests without a window.
  The property that the spike found valuable: **the model never returns an empty cell for a
  row that is on screen**, given a fake thumbnail source with delays.
- **Accessibility tree**: the AT-SPI script of spike 2 becomes a check that every interactive
  control on every main screen has a role and a name, and can be reached by keyboard. On Windows
  and macOS the equivalent is a manual check with NVDA and VoiceOver at each release [open:
  automation on these platforms].
- **Translations**: a **pseudo-locale** (accented and 40 % longer text, with brackets) run
  through every screen to find clipped or hard-coded strings; a check that no message id is
  missing or unused; plural forms tested for the languages that have them. A new language is
  accepted when the check passes.
- **Screenshots** of the main views on the software renderer, compared with a tolerance, on
  release candidates, to catch a layout that broke. They are not a gate on every change:
  screenshots differ across platforms and fonts, and a flaky visual test is worse than none.
- **Keyboard**: every command is reachable by the keyboard, checked by listing the command set
  against the shortcut table.
- **Responsiveness**: an instrumented build reports the longest interface-thread stall; the
  interface test scenarios fail if it exceeds a frame budget with the engine under load
  (spike 2: nothing heavy on the interface thread).
- **The interface is tested without a display first, on a real machine second.** Qt Quick Test
  (D-094) runs the whole interface offscreen with the software renderer, in a process of its own per
  suite (`crates/ui/tests/qml/tst_*.qml`, started by `tests/qml.rs`, so `cargo nextest run` runs
  them): each test makes the real window on a machine of its own (a folder standing for a person's
  computer, made by the harness), clicks and keys are real events sent to the window, and the
  engine's events arrive through the real event bus. The suites cover the welcome list and workspace
  lifecycle, every menu command and shortcut (and their writing), the Edit items on a focused field,
  the modal behaviour of dialogs, the catalogue task (add, scan, refuse, merge, remove, restore), the
  Import dialog (destination kinds, plain copy, camera folders, the form remembered), the card banner
  and the folder pickers (the mounted cards are injected; no native dialog opens offscreen), rating and
  paging from the keyboard, a resized window's columns, thumbnails made while a source is scanned,
  the language applied live with its plural forms, and the application's own start-up, which must say
  nothing about its QML. `docs/ui-parity-checklist.md` maps the 54 scenarios of the Slint shell to them.
  With `AUR_SNAPSHOT_DIR=<folder>` every view is also drawn to a PNG, in English and in French
  (CI keeps them as artifacts), which is how layout, clipping and French text are looked at without
  touching anyone's desktop.
- **A test waits on what it asserts.** `wait(300)` followed by `compare(grid.columns, 8)` says that the grid has laid
  out after 300 ms, and nothing waits for it to have: on a slow runner it fails (issues #27, #63, #72). The line that
  reads is a `tryCompare`, or a `tryVerify` with a message made when the wait ends; a delegate is read again at each
  poll (`itemAtIndex()` is `null` until the next layout); a click waits for a drawn frame (`drawn()`). `cargo xtask check`
  refuses a new `wait(N)` followed by a bare `compare` or `verify`; the sites that exist are listed in
  `crates/ui/tests/qml/waits-to-remove.txt`, and the list only shrinks (a site that is fixed must leave it:
  `cargo xtask waits --shrink`).
- **What only a real machine shows**, checked by a person before a release: GPU rendering, the
  platform's input methods (ibus, IME), real fonts and scaling, frame rate, AT-SPI/NVDA/VoiceOver.
  **An automated session never drives a person's own desktop with synthetic input**: on
  2026-09-23 `xdotool type` into the import view froze that display twice and forced a restart
  each time (`xdotool type` hanging GNOME/X11 desktops on long strings is a known upstream
  problem, and a text field with ibus was never ruled out). The person runs the application
  and types in its fields by hand, and a window is captured alone, never the whole screen.

## 7. Performance [proposed]

| Budget (specification §9) | Test |
| --- | --- |
| Slider to pixel under 50 ms | Stage timings from the harness, at draft and final quality, on a reference machine |
| Export 24 MP under 2 s | The export scenario on the sample files |
| Photo from the cache under 100 ms | Open a version from a warm and a cold thumbnail database |
| Open 100,000 photos, a few seconds | The generated large catalogue, cold cache where the platform allows |
| Search under 200 ms | The query set of spike 3 |
| 60 frames per second | The scripted scroll and zoom of spikes 2 and 3, on the real display |
| Memory | Peak use through a scripted session, with and without a large catalogue |

- **Where.** Timings from a shared CI runner are noise (spike 1 showed a virtual GPU 10 times
  slower than the real one), so **CI does not gate on time**. It runs the harness, stores the
  numbers, and draws a trend. The gate is Patrick's machines: the Linux desktop, the Windows
  dual-boot, the Mac when available, each with its GPU recorded in the result.
- **How.** The harness writes the JSON files that the spikes already produce, with the machine,
  the adapter and the commit. A script compares a run with a **stored baseline for that machine**
  and reports any budget exceeded or any stage more than 20 % slower.
- **When.** Before each release, and on demand for a change that touches the pipeline or the
  catalogue. A run takes minutes, not hours (the spikes' full run did).
- **Counts before times.** Where a performance property can be stated as a count (the number of
  stages rerun, the number of thumbnail requests, the number of copies of an image), it is tested
  as a count, on every change, because a count is exact on any machine.
- **Frame time is measured on a real session**, never on a remote desktop (software-rendered).

## 8. Robustness [proposed]

- **Fuzzing** targets: the XMP and JSON readers, the plugin declaration parser, the import of a
  card's folder names and file names, the metadata reader, and the decoders (through the sandbox,
  where a hang or an out-of-memory becomes a plugin failure, itself tested). Fuzzing runs
  nightly for a fixed time; a crash is a bug with a saved input that joins the corpus.
- **Hostile data in the workspace**: a sidecar that is truncated, empty, huge, with the wrong
  encoding, with a future schema version, or with a path that points outside the workspace. The
  application refuses it with a message, keeps the file, and never overwrites what it could not
  read.
- **File system behaviour**: a read-only source, a full disk (a write that fails halfway, tested
  with a size-limited file system), a disappearing network share, file names with non-ASCII
  characters, very long paths, case-insensitive and case-preserving names, Windows reserved names
  (`CON`, `NUL`), trailing dots and spaces. These run on every platform because they are exactly
  where the platforms differ (NTFS and antivirus, spike 3).
- **Concurrency**: the single-writer rule is tested by hammering the engine with commands from
  several threads while a rebuild and an import run, then comparing the final state with a
  sequential replay. Thread and race checkers (`loom` for the small synchronisation pieces,
  the sanitizers on nightly Linux) cover the rest.

## 9. Standards of the change [proposed]

A change is ready to merge when:

1. It builds, with **no warnings** (`clippy` and `rustfmt` enforced), on Linux, Windows and macOS.
2. Its tests pass on all three, including the blocking smoke test.
3. It carries **its tests**: a new behaviour has a test, a bug fix has a regression test.
4. If it changes a **format**, it adds a fixture and a migration, and the compatibility tests pass.
5. If it adds a **user-visible string**, the string is translatable and the pseudo-locale check
   passes; if it adds a **control**, the control has an accessible name and a keyboard path.
6. If it adds a **dependency**, the licence is compatible with GPL-3.0 (`cargo-deny` checks the
   whole tree on every change), the dependency is justified in the change, and it has no known
   vulnerability (`cargo-deny` reads the RustSec advisories as well).
7. If it touches the pipeline or the catalogue, the **performance run** has been done on at least
   one reference machine, and the result is in the change.
8. It is reviewed.

**Coverage** is measured (`cargo-llvm-cov`) and shown, but it is **not a gate**: a percentage
invites tests that run code without checking it. It is used to find untested branches in the write
path, the format code and the plugin host, where a gap is expensive.

## 10. The suite itself [proposed]

- **Speed.** The per-change suite finishes in **under ten minutes** on CI (parallel jobs per platform).
  Anything longer goes to the nightly run. A test that is slow enough to be skipped by
  developers is moved or made faster.
- **Flaky tests.** A test that fails without a change is quarantined the same day (marked, kept
  running but not blocking), an issue is opened, and it is fixed or deleted within a week. A
  quarantine list that grows is a process failure, reported at each release.
  The quarantine list (a test ignored on one platform, or for good, until it is fixed) is empty. A quarantined
  test gets a row here: the test, since when, where, the issue, why, and the date it is due.
- **No test depends on another's result or order.**
- **Tooling.** `cargo nextest` (faster, isolates each test), `proptest`, `cargo-fuzz`, `insta` or a
  small in-house comparison for golden files, `cargo-llvm-cov`, `cargo-deny` (which also covers the advisory database).
  The choices are ordinary and replaceable. [open: `insta` against a hand-made golden helper; the
  image comparison needs a tolerance that `insta` does not offer.]
- **Documentation of tests.** Each module's `tests/` starts with a short note on what it covers
  and what it deliberately does not. Golden files record how they were made.

## 11. Before a release [proposed]

A checklist run by Patrick and me, in addition to the automatic suite:

1. All platform jobs green on the release commit, the nightly fuzz and large-catalogue runs green
   for the week before.
2. The performance run on each reference machine, compared with the previous release.
3. A **real display**, on Linux and Windows: the colour chart with the display's own profile, a
   wide gamut screen if one is available; the image at 100 %, drag latency, no tearing.
4. **Screen readers**: Orca, NVDA, VoiceOver: open the application, reach the main views, import,
   rate, export.
5. **A clean install** and an **upgrade** from the previous release with a real workspace of a few
   thousand photos, including the schema migration; an old workspace opens without loss.
6. **Interruptions**: pull the power cord equivalent (kill the process) during an import, an
   export and a batch edit; reopen; nothing lost.
7. **An offline source** and a returning one; an external edit of a sidecar; an unplugged card.
8. **Packaging**: each installer installs, launches, updates and uninstalls, and the signed
   binaries verify on a machine that never saw the build.
9. **Translations** compile and load in every shipped language.
10. The changelog and the migration notes.

## 12. What is not tested [proposed]

Stated so that nobody assumes otherwise:

- Colour on every display and every graphics driver: the reference and cross-API tests cover the
  arithmetic, not the monitors.
- Every camera's RAW format: the sample set covers the makers, and the rest depends on reports.
  A photo that fails to decode is a bug report with a sample.
- Third-party plugins: the sandbox and the checks limit what they can do; their own correctness is
  their authors'. A **conformance kit** (a test program a plugin author runs against their own
  plugin: declaration valid, limits respected, results stable) is part of the plugin SDK in M5.
- Network services (Prooftide and others): the publication code is tested against a **recorded and
  a simulated server**, including errors and slow responses. Tests against the live service belong
  to the Prooftide plugin's own repository, run there, never by default.
- AI models: a model's output is not bit-stable across hardware. AI features are tested by
  properties (the mask covers the subject in a set of known images above a stated overlap, the
  denoise result is closer to a clean reference than the input) rather than by golden pixels [open:
  detail in M5].

## 13. Open points

| # | Item | To settle |
| --- | --- | --- |
| 1 | How to make real-GPU runs routine when only Patrick's machines have them (a script he runs? a self-hosted runner?) | With the CI document |
| 2 | Golden image comparison: `insta` or a hand-made helper with a tolerance | Start of M2 |
| 3 | Accessibility automation on Windows and macOS | M1, once the interface runs there |
| 4 | Tolerances per stage for the linear intermediates | Set in M2 from measurement |
| 5 | Hosting of the sample RAW files if raw.pixls.us is unavailable | With the CI document |
| 6 | The AI test method | M5 planning |
| 7 | How much of the nightly fuzz is affordable on CI minutes | With the CI document |
