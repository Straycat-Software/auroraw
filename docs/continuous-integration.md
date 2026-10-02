# Auroraw: continuous integration and releases

> **Status: adopted (D-081).** Where and when the tests of the
> [testing strategy](testing-strategy.md) run, how a change reaches `dev` and `main`, how a
> release is built, signed and published on three platforms, and how the supply chain is kept
> safe. Items are tagged **[decided]**, **[proposed]** (adopted as the working plan) or
> **[open]**. The signing and update policies were approved with D-081.

## 1. Aims [proposed]

1. **A change is known to work on Linux, Windows and macOS before it is merged**, not after a
   user reports it. The spikes already found three platform-only failures (a DirectX compiler
   rejection, a Linux-only system call, a Windows-only limit) that only a matrix would catch.
2. **Fast feedback**: under ten minutes for the per-change run (testing strategy §10).
3. **Releases are boring**: one tag produces every installer, signed, with checksums and notes, by
   a script that has run many times before.
4. **The pipeline is safe against its own inputs**: a contribution, a dependency or a plugin cannot
   reach the signing keys.
5. **Cheap to run**: hosted runners on a public repository, no servers to maintain.

## 2. Branches and how a change flows [proposed, following D-006 and the standing git rules]

| Branch | Role | Rules |
| --- | --- | --- |
| `main` | The last release. | Updated only by a merge at release time; every commit on it is a released state; tags `vX.Y.Z` point at it. Protected: no direct push, the release checks must pass. |
| `dev` | Integration. | The default branch for work. Protected by the repository's ruleset: a pull request, the per-change checks green, a review, the QA group's approval (D-152), and, once the owner switches the rule on, the merge goes through the **merge queue** (below). |
| `feature/...`, `fix/...` | One change each, from `dev`. | Short-lived; deleted after the merge. |

A change: branch from `dev`, push, the **per-change workflow** runs, review, and the pull request is put in the
**merge queue**, once the owner has switched on the ruleset's rule "Require merge queue" (until then the owner merges
by hand, and the rule "the branch is up to date with `dev`" sends each pull request back through a run after every
merge, 15 to 45 minutes each, issue #77). The queue merges the pull request with the ones ahead of it, runs the
per-change workflow again on that result (the `merge_group` event) and keeps the merge only if it is green. A merge then
costs one run of the workflow for everyone in the queue. The queue merges by **merge commit**: history stays readable,
and the sign-off check (`dco.yml`) skips merge commits, which a squash would not (the commit GitHub makes carries no
`Signed-off-by` of its own). A queue entry reads the Rust caches of `dev`, the default branch, and writes none
(see "The caches" in §3.1): its ref is new each time and nothing would read it again.
A release: `dev` is merged into `main`, tagged, and the **release workflow** builds from the tag.
A fix for a released version: a branch from the tag, released as a patch, merged back into `dev`
[open: only needed once there are users to support].

## 3. The workflows [proposed]

### 3.1 Per change: `ci.yml`

Runs on every push to `dev` and `main`, on every pull request and in the merge queue (`merge_group`). Jobs run in
parallel; the slowest sets the time.

| Job | What | Platforms |
| --- | --- | --- |
| **Lint** | `rustfmt --check`, `clippy` with warnings as errors, `cargo xtask check` (SPDX headers, the allowed dependencies between crates), and later the repository's own checks (links between documents resolve) | Linux |
| **Build and test** | `cargo build --locked`, `cargo nextest run` (unit, property with few cases, format, catalogue, engine scenarios, plugin host and hostile plugins, crash consistency short) | Linux x64, Windows x64, macOS arm64 |
| **Plugins** | Build the plugins for `wasm32-wasip1` (the spike's step) and run the conformance tests against them | Linux, Windows, macOS |
| **GPU reference** | The smoke test (`auroraw-pipeline`: every shader compiled and run against its CPU reference) on the adapter each runner offers: lavapipe, WARP, the runner's Metal adapter (reported as integrated). **Blocking.** The stage-against-reference tests join it with the stages. Part of the `test` jobs: Linux installs lavapipe, `AUR_REQUIRE_GPU=1` makes a missing adapter a failure instead of a skip, and `cargo run -p auroraw-pipeline --example adapters` prints which adapter the runner offered and runs the smoke test on it, since the output of passing tests is hidden | All three |
| **Dependencies** | `cargo deny check` (licences compatible with GPL-3.0, bans, sources, and the RustSec advisories, which is what `cargo audit` reads) | Linux |
| **Translations** | Every `qsTr` string of the QML is in each `.ts` file and nothing else is, every translation is finished and keeps its placeholders (a Rust test); the files compile with `lrelease`; pseudo-locale run of every screen [open] | All three |
| **Accessibility** | The AT-SPI check that every control has a role and a name | Linux, under a virtual display |
| **Docs** | The documents build; the spike and decision indexes are consistent | Linux |

**Time control.** Cargo's registry and build cache are kept (`Swatinem/rust-cache` keyed on the lock
file and the toolchain), tests run in parallel, and the smoke test uses a tiny image. The budget
is ten minutes at the median; a job that grows past it is split or moved to the nightly run.

**The caches.** The repository's Actions caches hold 10 GB, and GitHub evicts the least recently used ones when they
pass it. A job's Rust cache is 0.6 to 1.8 GB. A cache saved on the default branch (`dev`) is read by every run; one saved on
another ref is read only by the runs of that ref. So **only runs on `dev` write the Rust caches** (`save-if` in
`ci.yml`): a pull request or a queue entry reads `dev`'s and builds what its change adds on top. Saved from a pull
request, a cache is read by nobody else, and those copies (7.5 GB of the 10) evicted the ones of `dev` that every run
reads: the Rust cache was not found on 2, 4 and 13 jobs in 100 (Linux, macOS, Windows), and a cold job takes 6 to 19
minutes longer than a warm one (issue #77). A run on `dev` is not cancelled by the next push (`concurrency` in
`ci.yml`): it is the run that writes the cache, and a cancelled run writes nothing (14 of the last 60 runs on `dev`
were cancelled). A cache is rewritten only when its key changes, that is when a manifest, the lock file or the
toolchain does.

**The runners.** GitHub-hosted `ubuntu-24.04`, `windows-2025` (or the current `windows-latest`),
and `macos-15` on Apple silicon, pinned by name rather than `latest` so an image update does not
change the result unannounced, and moved forward deliberately. Linux needs the packages the spikes
found (`mesa-vulkan-drivers`, `libvulkan1`, fontconfig, xkbcommon, wayland and X libraries).
Standard runners for a **public** repository are free; the repository's visibility is therefore
part of this plan (the repository is public).

### 3.2 Nightly: `nightly.yml`

Runs once a night on `dev`, and on demand. Failures open an issue automatically.

| Job | What |
| --- | --- |
| **Property, long** | The property tests with thousands of cases and a random seed that is printed, so a failure can be replayed |
| **Fuzz** | Each fuzz target for a few minutes; new crashes are uploaded with their input |
| **Large catalogue** | Generate the 100,000-photo dataset (about 3.5 GB, well within a runner's disk) and run the query, rebuild, and crash-consistency tests on it, on all three platforms (this is where the NTFS behaviour of spike 3 gets watched) |
| **Sanitizers and races** | Address and thread sanitizers on the engine's concurrency tests, Linux only; `loom` on the small synchronisation pieces |
| **Performance trend** | The harness on the runner, stored with the commit and drawn as a trend. **Never a gate** (testing strategy §7), because a shared runner's timings are noise |
| **Latest toolchain** | Build with the newest stable and the beta compiler, to see breakage early |
| **Dependency drift** | The freshest allowed versions of the dependencies build and pass |

### 3.3 Release: `release.yml`

Runs on a tag `vX.Y.Z` pushed to `main`. See §6.

### 3.4 The spikes' workflow

`spikes.yml` was deleted with the spike code (WP0, tag `spikes-final`). It is the ancestor of
`ci.yml`; its GPU smoke test is back with the `auroraw-pipeline` crate (§3.1, "GPU reference"), and
the sandbox tests came with the plugin host in WP6.

## 4. Running on real machines [proposed; the open item of the testing strategy]

Hosted runners have no real GPU. Real Vulkan, DirectX 12 and Metal results, and honest timings,
come from Patrick's machines. Three ways, in the order proposed:

1. **A command anyone with the hardware runs**: `cargo xtask gpu-check` builds, lists the adapters,
   runs the reference and smoke tests on each real adapter, and the performance harness, then
   writes one JSON file with the machine, adapters, commit and results. A second command,
   `cargo xtask compare <file>`, compares it with the stored baseline for that machine and prints
   what changed. This replaces the ad hoc scripts of the spikes (`run-all.sh`) and works the same
   on the three systems.
2. **The result is posted, not run remotely.** The JSON goes as a comment on the pull request or
   the release issue. Nothing on Patrick's machines is reachable from GitHub.
3. **No self-hosted runner** on a personal machine for a public repository: a workflow triggered
   by a pull request would run a stranger's code on it. If dedicated hardware appears (a small
   test machine that is not used for anything else, run with ephemeral jobs and no access to
   secrets), this can be revisited [open].

The `xtask` crate is also the home for the other developer commands (`fetch-samples`, generate the
test catalogue, update golden images, package), so that the same command works locally and in
CI, and the workflow files stay short.

## 5. Supply chain and secrets [proposed]

- **Pin.** Third-party actions are pinned to a full commit hash, with the version in a comment;
  a bot (Dependabot) proposes updates as pull requests. The Rust toolchain is pinned in
  `rust-toolchain.toml`; `Cargo.lock` is committed and builds use `--locked`.
- **Dependencies.** `cargo deny` blocks a dependency with an incompatible or unknown licence,
  a known vulnerability, or a source other than crates.io without a reviewed exception.
  New dependencies are justified in the change (testing strategy §9). `cargo deny` also reads the
  RustSec advisory database, so a separate `cargo audit` is not run. Updates arrive weekly as
  grouped pull requests and pass the same checks.
- **Permissions.** Each workflow declares the least it needs (`contents: read` by default).
  Pull requests from forks run **without secrets** and cannot publish anything.
- **Secrets.** The signing keys and tokens live only in a protected **release environment** that
  requires Patrick's approval for each release run and is available only to tags on `main`. They
  are never available to `ci.yml` or to a pull request. No secret is ever printed, and logs are
  reviewed once for leaks.
- **Provenance.** Release artifacts get GitHub's build **attestations**, so a downloaded
  installer can be verified to come from this repository's workflow, and a **software bill of
  materials** (CycloneDX, from `cargo cyclonedx`) is published with each release.
- **Reproducibility** is a goal, not a promise: the same commit built twice on the same runner
  image should give the same binary, and the release notes say which toolchain was used.
- **Plugins in the index** (D-054) get their own pipeline, described with the plugin index in M5.
  This document covers the application only.

## 6. Releases [proposed]

### 6.1 Version numbers [proposed]

`MAJOR.MINOR.PATCH`, with three separate version spaces that must not be confused:

| What | Versioned how |
| --- | --- |
| The **application** | SemVer, `0.x` until the first stable release (after M4). A `0.x` minor may change anything but the promise below. |
| **File formats** (sidecars, state files, catalogue schema) | Their own integers, written inside the files (architecture §5.5). A release can change none, one or several. **A new release always opens the workspaces of the older ones.** |
| The **plugin API** | Its own number in every declaration (D-078), experimental until M5. |

Each release notes which format versions and which API version it carries, and whether a
migration runs on first start (with a backup made first).

### 6.2 What a release contains

| Platform | Deliverables |
| --- | --- |
| **Linux x64** | A **Flatpak** (the sandboxed, distribution-neutral route, published to Flathub), an **AppImage**, and a plain tarball. The Flatpak grants what the application needs (the photo folders the user picks through the portal, the GPU, the display) and no more, which suits the read-only-originals promise. |
| **Windows x64** | A signed **installer** (per-user, no administrator rights needed) and a **portable** zip. |
| **macOS arm64** | A signed and **notarised** `.dmg` with the application bundle. |
| **All** | A checksum file, the SBOM, the release notes taken from the changelog, the attestations. |

The plugins of the core (Prooftide export, official decoders) are **bundled as WebAssembly files**
with the compiled-plugin cache made at first start, per machine, not shipped (a compiled plugin
depends on the CPU).

### 6.3 Signing [decided, D-081]

Unsigned software triggers Windows SmartScreen and macOS Gatekeeper warnings that most
photographers will not click past. What each platform needs:

| | What is needed | Cost |
| --- | --- | --- |
| **macOS** | An Apple Developer Program membership, for the signing certificate and notarisation. **No way around it** for an application meant to be double-clicked. | About 99 USD a year |
| **Windows** | A code-signing certificate. Options: **SignPath Foundation** signs open source projects for free after a review of the project; **Azure Trusted Signing** is a low monthly fee; a traditional certificate from an authority costs more and, since 2023, needs a hardware token. Unsigned builds work but show a warning. | Free to about 10 USD a month |
| **Linux** | Flatpak and AppImage do not need a certificate; Flathub signs its repository. Optionally sign the tarball and checksums with a project key (a detached signature). | Free |

Decision: apply to SignPath for Windows, use the Apple membership Patrick already holds for
Prooftide, and **ship the first 0.x pre-releases unsigned** with a clear note, so that
signing is never on the critical path of the milestones. The keys and the account belong to
the project's owner, not to me: I cannot and should not hold them.

### 6.4 Updates [decided, D-081]

The application makes **no network request the user did not consent to** (D-061), so it cannot
silently check for a new version. Decision: a **"Check for updates"** command and an opt-in
setting that fetches one small file listing the latest version, nothing else sent; Flatpak and
package managers update themselves without any of this; no automatic download or install in
v1. Windows and macOS users install the new release over the old one. Whether a proper updater
(Sparkle on macOS, an equivalent on Windows) is worth its complexity is decided after the first
release, from real use.

### 6.5 Crash reports [open]

A crash writes a **local** report (the application's log tail, the versions, the adapter), and a
dialog offers to open the folder or copy the text into an issue. Nothing is sent automatically.
An opt-in uploader is not planned.

### 6.6 The release procedure

1. On `dev`: the checks are green for the last week, the nightly runs are green, the performance
   and manual checks of the testing strategy §11 are done and recorded.
2. Update the changelog and the version; merge `dev` into `main`; tag `vX.Y.Z`.
3. `release.yml` builds each platform, runs the test suite once more against the built
   artifacts (a launch test: the binary starts, opens a sample workspace, renders a sample
   image, exits), signs (after Patrick's approval), attaches checksums, SBOM and attestations,
   and creates a **draft** release.
4. Patrick installs each artifact on a clean machine or VM and runs the packaging checks.
5. Patrick publishes the draft. Flathub and other channels follow.
6. `main` is merged back into `dev` if anything changed on it.

A **pre-release** channel is the same procedure with a tag such as `v0.3.0-rc.1` and the release
marked as a pre-release. **Nightly builds** from `dev` are kept as CI artifacts for 14 days, unsigned,
for testers; they are not linked from the front page.

## 7. What it costs [proposed]

For a public repository, hosted runners on all three platforms are free. If the repository were
private the macOS and Windows minutes would multiply the bill: another reason to confirm it is
public. Expected use: a per-change run of about 10 minutes on three platforms (roughly 30
runner-minutes), a nightly run of an hour or so, a release of a few tens of minutes. Storage
of artifacts is capped by retention (14 days for nightly builds, a few days for pull requests,
releases permanent). The recurring costs that money buys are only the signing accounts of §6.3.

## 8. From the spikes to the product [decided, D-081; done in WP0]

- The spike code has answered its questions. It is **archived**: the tag `spikes-final` marks the
  last commit that has it, and the `spikes/` directory was removed from `dev`. The reports in
  `docs/spikes/` and the raw results stay, as they are the record.
- The repository looks like this:

```
Cargo.toml            workspace (resolver 3, edition 2024, shared lints and licence)
rust-toolchain.toml   the pinned toolchain
deny.toml             dependency policy
crates/               types, format, catalogue, workspace, sources, imaging, import,
                      plugin-api (MIT OR Apache-2.0), plugin-host, engine, ui, cli, app,
                      testkit (tests only)     (architecture §3.1; pipeline, develop,
                      export and publish are added with M2 and M4)
xtask/                developer commands: cargo xtask check (SPDX headers, crate layers)
tools/                scripts: fetch-samples.sh, a11y-check.py
testdata/             fixtures per format version, golden images (small); samples/ is fetched
                      and ignored by git
plugins/              the core's own plugins (built for wasm32-wasip1), from WP6
packaging/            Flatpak manifest, installer scripts, bundle metadata, from WP12
fuzz/                 fuzz targets, from WP1
docs/                 as before
```

- The harnesses of the spikes that the strategy keeps are **moved** as their work packages come
  up (see [spikes/README.md](spikes/README.md)), not rewritten.

## 9. Open points

| # | Item | To settle |
| --- | --- | --- |
| 1 | ~~The repository is public~~ | Done: public, free runners |
| 2 | ~~Signing~~ | Decided, D-081; the SignPath application is still to be made |
| 3 | Flatpak details: permissions, the runtime, Vulkan and GPU access on the sandbox | M1 packaging spike, small |
| 4 | Intel Macs (x86_64): supported, or Apple silicon only | Before the first macOS release |
| 5 | Linux on arm64, Windows on arm64 | After the first release, on demand |
| 6 | Whether a real updater is worth it, beyond the opt-in check (D-081) | After the first release |
| 7 | Real-GPU runs: the `xtask` command is enough, or dedicated hardware later | After M2 |
| 8 | Branch protection rules on `dev` | When the first outside contributor arrives |
| 9 | Hosting for large test data (the sample RAW files) if the source goes away | With the first golden tests |
