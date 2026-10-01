# Auroraw: governance and contributions

> **Status: adopted (D-082).** How the project is run in the open: who decides, under
> what licence code is accepted, how someone contributes, how conduct, translations, plugins and
> security reports are handled. Items are tagged **[decided]**, **[proposed]** or **[open]**.
> The repository is public and outside contributions are possible. The licence questions were
> settled first, because relicensing later needs the consent of every contributor (D-082).
>
> This is a practical policy, not legal advice. The wording of the licence exception (§2.3)
> should be read by someone qualified before the first release.

## 1. Roles and decisions [proposed]

| Role | Who | What they do |
| --- | --- | --- |
| **Owner** | Patrick Fournier | Holds the repository, the signing accounts and the name. Takes the final decision. |
| **Collaborator** | Claude Code | Works on the project at the owner's request, in the owner's repository, on `dev`, under the owner's review. Commits are authored as "Claude Code". |
| **QA reviewer** | Django (Claude Code), from 2026-10-01 (D-152) | Answers for everything that touches quality: the tests, the CI, the unreliable tests and the quarantine list. Reviews **every pull request** for how it is tested and whether it will keep working; **his approval is required before a pull request is merged**. |
| **Maintainers** | Added by the owner when there are contributors who are reliable and active | Review and merge changes, triage issues. |
| **Contributors** | Anyone | Propose changes, report problems, translate, write plugins, provide samples. |

How decisions are made:

- **The decision log** ([decisions.md](decisions.md)) is the record. A decision that changes what
  the application does, a file format, the plugin API, the licence or the governance is numbered
  (D-xxx) **after the owner approves it explicitly**, in the issue or pull request where it was
  discussed. Nothing else counts as a decision.
- **Small changes** (a fix, a refactor inside a module, a translation) need a review, not a decision.
- **Substantial changes** start as an **issue with a short proposal** (what, why, alternatives,
  effect on formats and plugins) before the code is written. A change that touches a **file
  format, the plugin API, the pipeline definition or the licence** always needs the owner's
  approval and a decision entry; a pull request that changes one of these without one is not merged.
- **Disagreement** is settled by the owner, with the reasons written down in the issue.
- The **specification and the architecture** are living documents: a change to them is a pull
  request like any other, and the code follows them, not the reverse.

## 2. Licences [decided, D-013, D-080, D-082]

### 2.1 What is under which licence

| Part | Licence | Notes |
| --- | --- | --- |
| The application: every crate except those below | **GPL-3.0-or-later** (D-013, D-080, D-082) | LGPL was considered and dropped (D-080). |
| The plugin API, the plugin SDK, the declaration schema and the example plugins | **MIT OR Apache-2.0** (D-080) | So that a plugin, free or not, can include them without any obligation. |
| Documentation (`docs/`) | **CC BY-SA 4.0** (D-083) | Text in `docs/LICENSE`. Code samples inside the documents are GPL-3.0-or-later. |
| Translations | Same as the application (GPL-3.0) | Translators agree to this when they contribute. |
| Sample photos and golden images in the repository | CC0 or a licence that allows redistribution, recorded next to the file | Nothing without a recorded licence enters. |
| The name and logo | Not licensed under the GPL | See §8. |

**Tie-break (D-080): if any licensing question turns out to contradict the GPL-3.0, everything is
GPL-3.0.** A permissive licence is used only where it costs the GPL nothing, that is, for the
small interface files that plugin authors must be free to include.

### 2.2 Files and dependencies

- Every source file starts with an **SPDX identifier** (`SPDX-License-Identifier: GPL-3.0-or-later`, or
  `MIT OR Apache-2.0` for the SDK), checked in CI.
- Each crate declares its `license` in `Cargo.toml`; `cargo deny` fails a build whose dependency
  licences are incompatible with the GPL-3.0 (testing strategy §9, CI §5). MIT, Apache-2.0, BSD,
  ISC, Zlib, MPL-2.0 and LGPL dependencies pass; **AGPL, proprietary, "non-commercial" and
  unlicensed ones do not**.
- Models and databases distributed with the application or its official index (AI models,
  lens and place-name databases) need a redistributable licence, shown to the user, as already
  required for models in the specification (§5.11).

### 2.3 Plugins [decided, D-082; finishes D-057]

A plugin is loaded into a sandbox and talks to Auroraw only through the documented plugin API,
never by linking with the application's code. The policy:

1. **A plugin may be under any licence**, free or proprietary, provided it uses only the plugin API.
   The SDK being MIT OR Apache-2.0 is what makes this workable for a WebAssembly module, which
   includes the SDK code statically.
2. The project states this in an **additional permission under section 7 of the GPL-3.0**, in a
   file named `PLUGIN-EXCEPTION`, that says a work that communicates with Auroraw solely through
   the plugin API, as a separate program or WebAssembly module, is not a work based on Auroraw
   for the purposes of the licence. The text is in the repository, marked as a draft; it is to be
   reviewed by a qualified person before the first release (see the note above).
3. The **official index** (§6) shows each plugin's licence and accepts free and proprietary
   plugins alike, as long as they meet its criteria; the user sees the licence and the
   permissions before installing (D-055).
4. Plugins that go beyond the plugin API (patching the application, reaching into its memory)
   are not covered and, given the sandbox, are not possible anyway.

If this exception turns out to conflict with the GPL, the tie-break of §2.1 applies and the
plugins' licences are reconsidered, not the application's.

### 2.4 The contributor agreement [decided, D-082]

Two usual ways to state that a contributor may submit their work:

| | **DCO** (Developer Certificate of Origin) | **CLA** (Contributor License Agreement) |
| --- | --- | --- |
| How | A line `Signed-off-by: Name <email>` on each commit (`git commit -s`), checked by CI. Says: I wrote this or may submit it, under the project's licence. | A document signed once, assigning rights or granting a broad licence. |
| Friction | Almost none. | A signature step that discourages small contributions. |
| **Relicensing later** | **Needs every contributor's consent.** | **Possible by the project alone.** |
| Used by | The Linux kernel, many GPL projects | Projects that dual-license or may change licence |

**Decision: the DCO.** It is the light way, and D-080 has just confirmed that the licence is not
going to change. The price is that a change of licence, in either direction, needs the consent
of everyone who contributed. Every commit carries a `Signed-off-by` line, checked on each pull
request by `.github/workflows/dco.yml`.

### 2.5 Contributions written with AI help [proposed]

- The person who submits a change is responsible for it, whatever tool wrote it: they must
  understand it, be able to defend it in review, and certify the DCO for it.
- Say so in the pull request if a substantial part was generated. It is information for the
  reviewer, not a bar.
- Do not submit code whose licence you do not know (including generated code that reproduces
  someone else's project). When in doubt, do not.
- The owner works with Claude Code as a collaborator on the repository (§1); those commits are the
  owner's responsibility, and **they carry the owner's `Signed-off-by` line** (D-082).

## 3. Contributing [proposed]

Files to add at the root of the repository, all short:

| File | Content |
| --- | --- |
| `CONTRIBUTING.md` | How to build and run, the checklist of the testing strategy §9, how to propose a change, the DCO, the style |
| `CODE_OF_CONDUCT.md` | §4 |
| `SECURITY.md` | §7 |
| `.github/ISSUE_TEMPLATE/` | Bug report, feature proposal, camera or file that does not decode (with a sample), translation |
| `.github/PULL_REQUEST_TEMPLATE.md` | The checklist of the testing strategy §9 |
| `.github/CODEOWNERS` | The QA reviewer, asked to review every pull request (D-152) |
| `PLUGIN-EXCEPTION` | §2.3, once approved |

**Working rules:**

- **Issues first for anything large.** A pull request that arrives with a new feature and no prior
  issue may be declined without a review of the code, kindly.
- **Small, focused changes.** One concern per pull request. A review of a thousand lines changes
  its own quality.
- **The checklist decides "ready".** The change builds without warnings on three platforms, has
  tests, carries fixtures for a format change, has translatable strings and accessible controls,
  **describes in the user manual (`docs/manual/`) any change a photographer can see** (D-102), and
  justifies a new dependency (testing strategy §9).
- **Reviews** are by a maintainer other than the author; the owner reviews changes to formats,
  the plugin API, the pipeline definition, the write path and the sandbox. **The QA reviewer's
  approval is needed on every pull request before it is merged** (D-152), in addition to these:
  the other reviews look at the design, the QA review at how the change is shown to work. The
  requirement is the repository's branch rule on `dev` and `main` (a required approval from the
  code owners, `.github/CODEOWNERS`), which the owner sets.
- **Language.** Code, comments, commit messages, documents, issues, pull requests and
  discussions are **all in English**, with no exception (D-082).
- **Commit messages**: a short imperative title, then a body that says why. History is
  kept clean on `dev`.
- **Style.** `rustfmt` and `clippy` decide. Names, comments and structure follow the
  surrounding code.
- **Good first issues** are labelled and kept stocked, and each has a pointer to where to start.
- **Response times** are a goal, not a promise: a first answer to an issue or pull request within a week.

## 4. Conduct [proposed]

The **Contributor Covenant 2.1**, unchanged, with a contact for reports. It applies in every
project space (issues, pull requests, discussions) and when someone represents the project.
The contact is a dedicated address so that reports do not depend on one person's inbox
[open: the address, to be created by Patrick]. Reports are handled by the owner and, when there
are maintainers, by two people, and are confidential.

## 5. Translations [proposed]

- **Source strings are English**, marked in the interface's code for Qt Linguist (architecture §10,
  D-094). The French translation is maintained by the owner, and is the first checked in review.
- **How people translate**: **Qt Linguist `.ts` files sent as pull requests** (D-082, D-094; a
  `.po` file converts with Qt's `lconvert`). No translation platform for now; one can be added
  later if translators ask for it (Weblate reads `.ts`).
- **A language ships** when it is at least **90 %** translated for the strings of the main
  views, passes the pseudo-locale and message-id checks (testing strategy §6), and has a named
  person who agrees to review its changes.
- **String freeze** two weeks before a release; new strings after that wait for the next one.
- **Every string has context**: a comment for translators on where it appears and what its
  placeholders mean. A string without one is a review comment.
- A language without a maintainer for a year is marked "unmaintained" and shipped only if it still
  passes the checks.

## 6. Plugins and the index [proposed, following D-054 to D-056]

The rules for the official **index** (built-in catalogue, D-054); manual installation stays
possible and is marked as not vetted.

| Level | Meaning |
| --- | --- |
| **Official** | Built and released by the project (Prooftide export, official decoders, the core's own operations). |
| **Reviewed** | Submitted by its author, source or binary reviewed, checked by the safety and portability checks (D-077), passes the conformance kit. Listed with the reviewer and the date. |
| **Community** | Listed without a review of the plugin's own code, but **sandboxed** and checked automatically. Marked as such. |
| **Not in the index** | Installed by hand. The application says so and shows the permissions again. |

Common criteria for the index (all levels):

- A **declaration** (D-078) that the host accepts, with its **permissions** justified in the plugin's
  description. Excess permissions (a network permission for a plugin that edits pixels) are refused.
- A visible **licence**, and a place to report problems.
- **No telemetry, no hidden network access.** Network permissions name their destinations.
- For **models** (AI): a redistributable licence, a stated origin and the model's size (spec §5.11).
- A **native-level** plugin (D-055) is listed only at the Reviewed level, and is marked.
- Someone who **maintains** it: a plugin whose author has vanished and that breaks after an API
  change is delisted after notice.

The index's **removal policy**: a plugin found to be malicious, or to break the rules above,
is removed and its installations are warned on next start. The owner decides; the reason is
published.

**Stability promises for authors.** From M5 the plugin API follows SemVer: no breaking change
inside a major version; a deprecated feature stays for at least two minor releases and a year;
each release notes API changes. Until M5 the API is experimental and says so (architecture §8.6).

## 7. Security [proposed]

- **`SECURITY.md`** asks reporters to use GitHub's **private vulnerability reporting**, which is
  **switched on** for the repository.
- **In scope**: an escape from the plugin sandbox, a plugin reading what it was not granted, a
  crash or code execution from a crafted sidecar, RAW file, plugin package or index entry,
  a way to make the application send data without consent, a flaw in the update check or the
  signing.
- **Targets**: an acknowledgement within 7 days, a fix or a mitigation plan within 90, and a
  coordinated disclosure after a fix ships. Credits to reporters who want them.
- **Supported versions**: the latest release, and the previous minor while the project is young.
- The fuzzing, the hostile-plugin tests and the `cargo deny` advisory check (testing strategy §8, CI §5) are the
  routine defence; a security fix always gets a regression test.

## 8. The name and the project's assets [decided in part, D-083]

- **The name.** "Auroraw" was checked briefly by the owner, who also **registered the domain
  auroraw.org**. A thorough trademark search and a check of the package names on Flathub and
  other stores are still to do before the first public release.
- **The logo** and other artwork: licence recorded; a permissive licence for the artwork used in
  the application, with the **name and logo reserved** so that a fork can be honest about being one.
- **Domains, accounts and signing keys** belong to the owner (CI §6.3), or to a legal entity if one
  is ever created.

## 9. Community channels [proposed]

**GitHub Discussions** for questions and ideas, **issues** for defects and proposals. No chat
server at first: chat needs moderation and fragments what should be findable. A channel is
added when there are enough people to justify it. Announcements go in the release notes.

## 10. Funding [decided, D-082]

`.github/FUNDING.yml` shows the owner's GitHub Sponsors button. The project does not depend
on it, and no feature is withheld for money.

## 11. What is next

The repository files of §3 are in place (`CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`,
`PLUGIN-EXCEPTION`, the issue and pull request templates, `FUNDING.yml`, the DCO check). The SPDX
header check comes with the first product code. Next: **the plan of milestone M1.**

## 12. Open points

| # | Item | Needed before |
| --- | --- | --- |
| 1 | A qualified review of the `PLUGIN-EXCEPTION` wording | The first release |
| 2 | Trademark search and store package names for "Auroraw" | The first public release |
| 3 | What auroraw.org hosts (the project page, the documentation, the plugin index) | The first public release |
