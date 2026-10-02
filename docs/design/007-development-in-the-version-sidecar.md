# Design note 007: the development in the version sidecar, the history and the undo

> **Status: proposed.** Note of work package WP17 ([M2 plan](../m2-plan.md) §5, §6 items 4 and 5), the
> last of the notes that WP17 waits for. It answers questions 4 and 20 of the specification (§10):
> what a version sidecar holds of the **development** (the operations, the history, the snapshots),
> in which file, how it survives a crash and a newer program, what "compact" keeps; and it settles
> how **undo** works in Develop. It builds on [note 003](003-sidecars.md) §5.4 (which reserved the
> room), [note 005](005-image-engine-interfaces.md) §2.2 (the `Recipe` the pipeline receives),
> [note 006](006-pipeline-definition.md) §3.6 and §6 (the definition version and the base look) and
> D-006, D-023, D-038, D-042, D-096, D-142. Items are tagged **[proposed]**; **[estimated]** marks a
> number from a synthetic file, to be replaced by a measurement with the real writer in WP17's first
> pull request (§11). Nothing here is decided until Patrick approves it (§12 says what he is asked).

## 1. The question

A version is "one development of a photo: its chain of operations, its history, its name"
(specification §3). M1 wrote versions' **metadata** and reserved the rest. M2 has to keep, for each
version, three things that behave differently:

- the **state**: the operations in the order they run, each with its parameters. It is what the
  pipeline renders and what the catalogue needs to know about; it is small and changes at every edit;
- the **log**: the steps that led to the state, which Ctrl+Z walks. It grows with every gesture;
- the **snapshots**: named states that can be compared, restored, or turned into a new version.

The questions: where each one lives, what makes the whole survive a process killed in the middle of
an edit, what an old Auroraw does with a file a newer one wrote, what a plugin that is gone or has
changed does to a saved edit, and what "undo" means when there are two mechanisms (the in-memory
journal of D-096 and the version's own history).

## 2. What we start from [read]

- **The reservation** (note 003 §5.4): `aur:Pipeline…` properties, `aur:PipelineSchema`, and a
  companion file named `<photo id>.<version id>.<suffix>`, which an M1 reader lists as unknown and
  never touches (note 001 §5.6). A development a reader does not understand opens **read-only for
  the development**, and the version's metadata still works.
- **The code today.** `VersionSidecar` has a field `extra` for the properties it does not know,
  "kept and written back" (`crates/format/src/sidecar/version.rs`), a `VERSION_SCHEMA` of 1, and a
  reader that reports a newer schema (`Loaded::Newer`) and does not interpret it. The catalogue has
  a `version` table (name, rating, flag, label, created, sidecar stat) and `photo.main_version_id`.
  **No engine command makes a version**, so the only version sidecars that exist are those of tests
  and datasets: the migration of M1 files costs nothing, and a fixture holds it (§9).
- **What the pipeline receives** (note 005 §2.2): a `Recipe`, a definition version and a list of
  `OperationInstance { operation, op_version, enabled, params: Vec<ParamValue> }`, **already placed**
  by `develop`. The pipeline knows nothing of versions, files or history.
- **What the definition promises** (note 006 §3.6): a released definition is never edited, an edit
  renders with the definition it names, and the operation's `op_version` and the definition version
  are both recorded in the sidecar. Architecture §7.2 adds: "the pipeline definition version **and
  order used**".
- **The journal** (D-096): in memory, per workspace, 500 steps, before-and-after pairs, for ratings,
  flags, keywords, collections and series. "The persistent history of a *version* (M2, in the
  sidecar) is a different mechanism."
- **The declaration** (D-142): an operation declares its parameters by **key**. The review of #107
  added, in #109 (not merged when this is written), its `operation_version`, the older versions it
  `also_reads`, and whether a recipe `allows_several` of it; this note relies on them.

## 3. What a version holds [proposed]

A version is: its identity and metadata (note 003, unchanged), and a **development**:

| Part | What | Why it is there |
| --- | --- | --- |
| **Definition version** | The number of the pipeline definition the version renders with (note 006 §3.6). Fixed when the version is made; an existing version keeps it until the person migrates it (a later, explicit action; there is only a v1 in M2). | An old edit renders as before. |
| **Base look** | The name of the look the version started from (`neutral`, `flat-linear`, a style's name). Informational: the look is already **in the operations**. | The interface can say where a version came from. |
| **Operations** | The ordered list of instances: operation identifier, `op_version`, the release of the plugin that provided it, `enabled`, and the parameters. | The state. |
| **History** | The steps and the position in them (§5). | Undo. |
| **Snapshots** | Named states (§6). | Compare, restore, branch. |

Five rules, each with its reason:

1. **The state is the truth; the history is a log that leads to it.** If they disagree, the state
   wins and the history is the one that is repaired (§4.3). This is note 003 §5.2 again: one thing
   that is true, the rest derived.
2. **Parameters are stored by key, with their type**, not by position. The recipe's `params` is a
   positional list "typed by the declaration"; a sidecar that stored it as such would break the day a
   later `op_version` adds a parameter before another. `develop` builds the positional list from the
   declaration (a missing key takes its default; a key the declaration does not know is kept in the
   file and not rendered). The value is `plugin-api`'s `ParamValue`, which already serialises with its
   kind (`{"float":0.7}`, `{"colour":[1,0.5,0]}`), so a parameter of a plugin that is gone is still
   exactly what it was: an integer stays an integer, an enum an enum.
3. **The order used is stored, and an operation is placed once.** The order comes from the
   declarations (note 006 §3.4), and a plugin's update may change its constraints. If the order were
   computed at every open, an old edit could render in another order after an update. So the sidecar
   stores the **order in which the operations run**; adding an operation places it by its declaration
   among the stored ones, and the others do not move. A person never reorders (specification §5.4).
4. **The default recipe is written out in full at the first edit.** D-042: "no version is written
   until the first edit", and an unedited photo renders from the default recipe, which holds the
   neutral `tone map` (note 006 §6). At the first edit the version is made with that recipe **spelt
   out**, not "the default" by reference, since the default of a later Auroraw may differ. The history's
   first state is that recipe.
5. **A missing or newer operation is kept, not dropped** (§8). The sidecar stores what the person set,
   never what the machine could run: `enabled` is the person's, and "disabled because the plugin is
   absent" is decided when the recipe is built, and written nowhere.

### 3.1 The default version and the main version

A photo "always has a default version, which exists only in the catalogue until the first edit"
(specification §3). **The default version has no row and no file.** Its identity is the photo's; the
grid renders it from the default recipe. The first edit **makes** a version (a random identifier,
the name empty), writes its sidecar, and sets it as the photo's **main version** (D-038: by default
the last edited, `aur:MainVersion` in the photo sidecar, which exists). From then on the photo has
that version, and "create another" adds one. Deleting the last version returns the photo to the
default version. Deleting the main one makes the most recently edited the main one.

Creating a version has five origins (specification §5.4): from scratch (the default recipe), from
another version (its state, an empty history), from a snapshot, from a style (WP20), and the
one-key **duplicate and try**, which is "from the current version" and puts the person in it.
**Creating a version never copies pixels, and never copies the history**: the new version starts
its own, with the state it was made from as its first state.

## 4. Where it is written [proposed]

### 4.1 Two files, and why

The plan proposed the history as JSON inside one XMP property. The numbers say that it works for
the state and not for the log:

| What | Size [estimated] | Note |
| --- | --- | --- |
| A version sidecar today (a copy of the photo's metadata, and its own fields) | of the order of the photo sidecar's 1.6 KB (spike 3) to 3 KB with a dozen keywords (note 003 §4.3) | |
| The state, 12 operations, one 5-point curve, as **one JSON text** | 1.5 KB | |
| The state as **typed XMP structures** (one struct per parameter, nested sequences for lists and curves) | 6.2 KB | readable by anyone, 4 times the bytes |
| The state as **typed envelope, parameters as JSON text** | 3.3 KB | |
| A history of 200 steps (a long editing session) | 35 KB | 179 bytes a step |
| A history of 1,000 steps | 175 KB | |
| A history of 10,000 steps (brush strokes, M3) | 1.7 MB | |

Two things follow. First, the **metadata copy follows the photo in the background** (note 003 §5.3):
every change of a rating rewrites the version sidecars of the photo, 14,000 files for a batch of 10,000
photos. If the log were inside that file, a rating would rewrite megabytes of history it did not
touch. Second, the **rebuild parses every version sidecar** (3.2 s warm for 143,106 versions, spike 3):
the log is not needed to rebuild, and should not be read.

So: **two files**, split by what each is for.

- **`versions/<xx>/<photo id>.<version id>.xmp`** holds the metadata (unchanged) and the **state**:
  everything that rendering, the rebuild and the catalogue need. About 3 to 5 KB more for an edited
  version.
- **`versions/<xx>/<photo id>.<version id>.history.jsonl`** holds the **log and the snapshots**,
  read only when the version is opened in Develop. It exists from the first step. A version with no
  step has none.

The state in the XMP is **typed in its structure and compact in its values** (the third row):

```xml
<aur:PipelineSchema>1</aur:PipelineSchema>
<aur:DefinitionVersion>1</aur:DefinitionVersion>
<aur:BaseLook>neutral</aur:BaseLook>
<aur:RecipeDigest>9f2c…(64 hex)</aur:RecipeDigest>   <!-- the pipeline's hash of the recipe, note 005 §2.2 -->
<aur:HistoryCount>14</aur:HistoryCount>              <!-- lines of the history file that are real -->
<aur:HistoryCursor>12</aur:HistoryCursor>            <!-- steps applied; the others can be redone -->
<aur:Operations><rdf:Seq>
  <rdf:li rdf:parseType="Resource">
    <aur:Id>auroraw.exposure</aur:Id>
    <aur:OpVersion>1</aur:OpVersion>
    <aur:PluginVersion>0.2.0</aur:PluginVersion>       <!-- the release that wrote it: provenance only -->
    <aur:Enabled>True</aur:Enabled>
    <aur:Params>{"ev":{"float":0.7}}</aur:Params>      <!-- by key, sorted: the same state, the same bytes -->
  </rdf:li>
  …
</rdf:Seq></aur:Operations>
```

A reader other than ours sees the list of operations, their versions and whether they are on, and
the values as JSON text: enough to understand a file, which is all "open and documented" asks. The
alternative, one XMP structure per parameter, is 4 times the bytes for nothing a third party can
use (no other program knows what `auroraw.exposure` means), and it makes the state 4 times as large
for the rebuild to parse. The XMP model already represents structures and nested sequences (`Value::Struct`, `Value::Array`),
so this is a choice and not a limit; if the measurement of §11 says otherwise, the alternative costs
one function.

**Floats are exact.** The pipeline hashes the bit pattern of a value (note 005 §2.2); a parameter that
came back from the file one bit away would be another cache key and another image. The workspace
enables `serde_json`'s `float_roundtrip`; a test writes and reads a few thousand random `f64` bit
patterns (except NaN, which no declaration admits) and compares bits. `-0.0` is written as it is; the
pipeline's encoding normalises it.

### 4.2 The history file

JSON Lines: one object per line, so that a step is **appended** and the file is never rewritten for
an ordinary edit.

```json
{"schema":1,"photo":"…","version":"…","snapshots":[{"name":"Before the crop","at":"2026-10-02T14:03:11Z","definition":1,"operations":[{"id":"auroraw.exposure","v":1,"on":true,"params":{"ev":{"float":0.0}}}]}]}
{"n":1,"t":"2026-10-02T14:03:40Z","name":"Exposure","changes":[{"op":"auroraw.exposure","before":{"params":{"ev":{"float":0.0}}},"after":{"params":{"ev":{"float":0.7}}}}],"digest":"3f9a…"}
```

- **Line 1 is the header**: identity and the **snapshots** (§6). It is rewritten, with the rest of
  the file, by the rare operations that change it (a snapshot made or deleted, a compaction, a new
  edit after an undo): temporary file, then rename, as every sidecar.
- **Every other line is a step.** A step is a list of **changes**, each on one operation instance:
  the keys it changed with their value before and after; `before` absent means the instance was added
  (`after` is then the whole instance), `after` absent means it was removed. `enabled` is a field
  like the others. A step is reversible by reading it the other way, as in D-096, and **no step
  stores a whole recipe**. `digest` is the pipeline's hash of the recipe **after** the step.
- A line, a header or a change may carry **fields this version does not know**; they are kept when
  the file is rewritten (`serde` `flatten` into a map). An unknown *kind* of change makes the step
  unreadable and the history **read-only from there** (§8).

### 4.3 The commit, and the crashes [proposed]

Two files cannot be written together, so one of them says what is real. **The XMP is the commit
point**: `HistoryCount` says how many lines of the history are real, and the XMP's state is the truth.
An edit does, on the coordinator (the single writer, D-126):

1. append the step to the history file (not yet real);
2. write the XMP: the new state, its digest, `HistoryCount` and `HistoryCursor` (atomic: temporary
   file, rename);
3. update the catalogue (as architecture §5.3: the workspace first, then the database).

A crash can only leave the history **ahead of the state, by at most one line**, never behind it,
the same promise the database has. On opening a version in Develop:

| What is found | What it means | What is done |
| --- | --- | --- |
| The file has more lines than `HistoryCount` | A crash between 1 and 2 | The extra lines are ignored, and removed at the next write. Nothing was lost that the person had been shown. |
| `HistoryCount` is larger than the file | The history file was damaged or lost | The state is kept; the history **restarts** from it, empty, and the person is told. The XMP is not touched. |
| The step at the cursor has a `digest` that is not `RecipeDigest` | One of the two files was edited by something else | The state is kept; the history is **set aside** (renamed `….history.jsonl.set-aside`, never deleted) and a new one starts from the state. |
| The history file is absent and `HistoryCount` is 0 | A version with no step | Normal. |

The cost of an edit is an append and a small atomic write: well under a frame, and the render does not
wait for it (a render writes nothing: note 005 §4). A process killed in the middle of a drag loses
the gesture in progress and nothing before it.

**The background refresh of the metadata copy** (note 003 §5.3) rewrites the XMP and must not touch
the development. It does not parse it: it sends the coordinator its intent (D-126) and the
coordinator rewrites the file **with the development carried over as it is** (`extra`). All writers
of a version sidecar are the coordinator.

## 5. The history and the undo [proposed]

### 5.1 A step is a gesture

The interface commits at the end of a gesture: the release of a slider, the end of a curve drag,
a click on a switch, a typed value. The engine gets **one command per commit**, with the operation,
the keys and their new values; frames while dragging are renders, not edits, and write nothing.

**Coalescing, in the engine.** A new step is merged into the previous one when it is on the same
version, touches the same keys of the same single operation, comes within 2 seconds, and nothing
(a snapshot, an undo, another operation) came between. The merged step keeps the first `before` and
the last `after`. This is what makes ten presses of an arrow key on a slider one step. A step whose
`after` equals its `before` is not a step (D-096).

### 5.2 Linear, with a cursor

The history is **linear** (specification §5.4): a list of steps and a **cursor**, the number of steps
applied. Undo moves the cursor back and writes the state at the new cursor; redo moves it forward.
**A new edit after an undo discards the undone steps**, which is the specification's rule: exploring
variants is what versions and snapshots are for. The cursor is **persisted** (`HistoryCursor`): a
version reopened after an undo is where the person left it, and the steps ahead can still be redone
until the next edit. The state in the XMP is always the state **at the cursor**, so an export, a
thumbnail and the rebuild never see a step that was undone.

### 5.3 Two histories, and which Ctrl+Z means what

Plan §6 item 5 proposes that the version's history **is** Develop's undo, that D-096's journal keeps
serving Cull and the organising views, and that the two are not merged. I confirm it, and name what
it costs:

- **The command names stay clear.** `Command::Undo` and `Redo` remain the journal's. The version's are
  `Command::UndoDevelopment { version }` and `RedoDevelopment { version }`, and the interface sends the
  one of the active view. The Edit menu names the step it would undo ("Undo Exposure", "Undo Rating").
- **What is journaled stays journaled.** The rating, flag and keywords of a photo are metadata, in the
  photo's sidecar (and in the version's when the version overrides them, D-063), and stay in the
  journal in every view. **A rating given in Develop is undone by the journal, not by Develop's
  Ctrl+Z.** That is the sharp edge of not merging. My proposal for the interface (Bob's to accept): in
  Develop the Edit menu offers both, "Undo Exposure" on Ctrl+Z and "Undo Rating" on another shortcut,
  and the one that applies is always named. Merging the two by time would need one history that holds
  both kinds of step, which D-096 and the sidecars do not give.
- **Switching version or photo does not lose a history**: each version's is in its file. The journal
  is not affected.

### 5.4 Compaction (question 20) [proposed numbers]

"A button compacts it" (specification §5.4). **What it keeps**: the steps up to the cursor are replaced
by **one step per operation that differs from where the history began** (its base: the state of the
first step's `before`, or of the last compaction), `before` the base instance, `after` the current
one. The undone steps ahead of the cursor are discarded. Snapshots are untouched, since they hold
whole states (§6). After compaction the person can still undo to the base and no further inside it.
It is **not undoable** and the button says so before it acts.

**When it is proposed.** At **1,000 steps** (175 KB [estimated]) the interface offers it, once per
session and per version, and the person can decline. At **10,000 steps** the engine folds the oldest
half into one base step without asking, so that the file cannot grow without bound; this is the only
case where Auroraw forgets something the person did, it happens after a first offer, and it is
the same rule as D-096's bound of 500. These numbers are proposals for Patrick (§12).

## 6. Snapshots [proposed]

A snapshot is a named, **immutable full state**: its name, the time, the definition version and the
operations. It is not a position in the log, because the log is compacted and truncated and a snapshot
must outlive both. They live in the **header of the history file**.

- **Make**: from the state at the cursor. Name optional (a default from the time). Rewrites the header.
- **Compare** with the current state: the interface asks the pipeline for two renders (note 005 §2.1);
  the snapshot is a recipe like any other.
- **Restore**: an ordinary step, "Restore *name*", whose changes are the difference between the current
  state and the snapshot's. It is **undoable**. A snapshot taken under another definition version than
  the version's is refused for restoring, with the reason (a version has one definition), and can still
  be turned into a version.
- **Turn into a version**: a new version whose first state is the snapshot's, with the snapshot's
  definition version and an empty history.
- **Delete**: rewrites the header. Not undoable (it asks).

## 7. In the engine [proposed]

`develop` is the pure model: the state, the steps and their application in both directions, the
compaction, the building of a `Recipe` from a state and the registry of declarations. It does no I/O.
`format` reads and writes the two files (`sidecar::version` for the XMP part, a new `history` module
for the JSON Lines); the coordinator is the only writer.

| Command | What it does | Undoable by |
| --- | --- | --- |
| `CreateVersion { photo, from, name }` | `from` is scratch, a version, a snapshot or (WP20) a style. Makes the version, writes both files if it has steps, sets it main if it is the photo's first. | The journal (an entry "Create version") |
| `DuplicateAndTry { version }` | `CreateVersion` from the current one, and tells the interface to switch. | The journal |
| `RenameVersion`, `SetMainVersion`, `DeleteVersion` | Metadata of the version. A deletion moves both files to `removed/` (D-091) and is recoverable. | The journal |
| `SetOperation { version, changes }` | A commit of §5.1: parameters, `enabled`, or an operation added or removed. | `UndoDevelopment` |
| `Snapshot`, `RestoreSnapshot`, `DeleteSnapshot` | §6 | `RestoreSnapshot` by `UndoDevelopment`; the others by nothing |
| `UndoDevelopment`, `RedoDevelopment`, `CompactHistory` | §5 | `CompactHistory` by nothing |

Events: `VersionsChanged { photo }`, `DevelopmentChanged { version, step, recipe_digest }` (the render
service of WP18 listens to the second, and renders from `Develop::recipe(version)`), and
`HistoryChanged { version }` for the interface's Edit menu.

**Making or deleting a version is a journal entry; editing it is a history step.** The first edit of
an unedited photo is both, and undoing it in Develop returns the version to its first state, not to
"no version": the version stays (the person can delete it).

`Develop::recipe(version)` returns the `Recipe` and the list of **inert** operations (§8) for the
interface to mark; the pipeline never sees what is not renderable.

## 8. When the file is not what the code expects [proposed]

The rule is the architecture's (§5.5): a reader **preserves what it does not understand** and
never silently migrates a workspace file to an older format. For the development:

| Case | What happens | Kept |
| --- | --- | --- |
| **A plugin is absent** | Its instances are in the recipe as `enabled: false` and marked in the interface ("the plugin X is missing"). Other operations render and can be edited. | Its parameters, byte for byte, and its place. When the plugin returns, the person's own `enabled` applies again. |
| **A stored `op_version` is higher than the plugin knows** (a file from a newer Auroraw) | The same as an absent plugin, marked "needs a newer X". | The same. |
| **A stored `op_version` is one the plugin `also_reads`** | Rendered by the plugin, which interprets the old values. The instance is **not rewritten** until the person edits it. | The old version, until an edit. |
| **A parameter key the declaration does not know** | Not rendered, not shown. | In the file. |
| **A definition version this build does not have** (a newer file) | The version opens **read-only for the development**: it is shown with its metadata and its last thumbnail, not rendered, not edited. | Everything. |
| **A newer `PipelineSchema`** | The same: read-only for the development, the metadata still works (that is what a schema of its own is for, §9). | Everything. |
| **A step of the history the code cannot read** | The history is usable up to the step before it; from there it is read-only and a new edit starts a new history after setting this one aside (§4.3). | The file. |
| **An invalid XMP or history file** | Reported once in the workspace notes (note 001 §5.6) and **not overwritten** until the person decides. | The file. |

What happens to an instance written at an older `op_version` **when it is edited** (does the plugin
migrate its values, and through what call) is a plugin-API question that does not arise in M2: every
built-in operation is at version 1. It is left open (§12) with this note's constraint: the file must
record `op_version` per instance, which it does.

## 9. Schema, fixture, migration, catalogue [proposed]

- **`aur:Schema` stays at 1; the development has its own, `aur:PipelineSchema`**, reserved in note 003
  §5.4 and starting at 1. This is deliberate. A bump of `aur:Schema` would make an M1 build report
  every developed version as "newer" and not interpret it (`Loaded::Newer`), so that its **metadata**
  would stop working, against note 003 §5.4's promise. With a schema of its own, an M1 build reads the
  metadata of an M2 file, keeps the development as unknown properties (`extra`) when it rewrites the
  file for a rating, and leaves the history file alone. The "schema bump with a fixture and a
  migration" of the plan is therefore the appearance of `PipelineSchema`, with the fixtures below.
- **A version sidecar of M1** (metadata only, no `PipelineSchema`) is read as a version **with no
  development**: its state is the default recipe at the current definition, written out at the first
  edit (§3, rule 4). Reading it needs no migration, and it is **not rewritten** until something
  changes in it, since "workspace files are never migrated silently". The fixtures are the M1 file
  (kept), a file with a development and a history, one with a missing plugin, and one from "the
  future" (an unknown field, an unknown kind of change, a higher definition version, a higher
  `PipelineSchema`), which an M1-style edit of a rating must leave intact.
- **The catalogue** gains, on `version`, the definition version and the **recipe digest**. The
  digest is what the main version's thumbnail is keyed by (a thumbnail is stale when the digest
  changes) and what makes a rebuild comparable. No history, no snapshot and no operation is indexed:
  a rebuild reads the XMP and not the history file. The migration of the catalogue is a rebuild, as
  always (architecture §5.5).
- **The grid** shows the main version's thumbnail: a developed photo's is a render, so it waits on
  WP18's service; until then an edited photo shows its embedded preview, and a version created in
  test shows the same. This is a dependency, not a problem for this note.

## 10. Styles, paste and auto-sync: what they store [proposed, WP20 decides]

Note 006 §8 asked how a definition version meets a style. The proposal, short because WP20 is its own
work package:

- A **style** is a set of operation instances in the same form as a state (by key, with `op_version`),
  with the **definition version it was made under**. Nothing else: no history, no order (the order is
  placed when it is applied).
- **Applying** a style or a paste is one step (`changes` on several instances), placed by the target's
  definition with the registry's own checks (the stage exists, the constraint names an operation of
  the same stage). A style whose operations the target cannot place is refused with the operation named.
  There is no lasting link (D-039): changing a style later changes nothing.
- **A step across several versions** (paste to a selection, auto-sync, a style at import) is one step
  *in each version's history*, all carrying the same `batch` field. Undoing one in Develop undoes it in
  that version only; the interface may offer "undo on all" which undoes the steps with that `batch`
  whose version cursor is still on them. Question 6 of the plan ("one undo step") is read as this.

## 11. How it is checked [proposed]

| Property | By |
| --- | --- |
| A state written and read back is the same state and the same digest, including **every bit of every float** | Round trip, and a property test on random `f64` bit patterns |
| The bytes are canonical: the same state, the same file; an unchanged file is not rewritten | A test, as for the other sidecars |
| A process killed at each point of §4.3 reopens to the state before or after the edit, never in between, with a consistent history | A test that stops the coordinator after step 1, and after step 2; and WP17's scripted CLI session that kills the process |
| What a newer file contains is kept when an older build edits something else in it | The "future" fixture of §9, edited and compared |
| A missing plugin opens disabled, marked, with its values intact; the plugin's return restores it | WP17's "done when" |
| Undo and redo through a long session end where they began; a new edit after an undo discards the undone steps | A property test on random sequences, checked against a plain model |
| The history is bounded | The test of §5.4 at 10,000 steps |
| The history and XMP parsers survive hostile input | A fuzz target `version_history`, next to `raw_block` |
| ExifTool reads the XMP and shows the operations | An ExifTool test, like the others (testing strategy) |
| **The sizes of §4.1, with the real writer, and the rebuild time with developed versions** (143,106 versions, each with 12 operations), against the rebuild of the same workspace with metadata-only versions | **Measured first, in WP17's first pull request**, before the format is frozen. If the typed envelope costs the rebuild more than the plan can afford (the threshold is set with the measurement, not before), the state moves to the one-JSON-text form (§4.1), half the size |

## 12. What this changes elsewhere, and what is asked

| Where | What | Who |
| --- | --- | --- |
| `crates/format`, `sidecar::version` and a new `history` module | The development part, the JSON Lines, the future fixtures, the fuzz target | Alice (WP17) |
| New crate `develop` | The pure model of §7; depends on `plugin-api`, `pipeline` (the `Recipe`, the definition, the registry) and `types`; `xtask`'s table says so | Alice (WP17) |
| `crates/pipeline` | Exposes the recipe digest as a function `develop` and the catalogue can call (it is the canonical encoding of note 005 §2.2, already there) | Charlie |
| `crates/engine` | The commands and events of §7 | Alice |
| `crates/catalogue` | Two columns on `version`, rebuilt | Alice |
| The remove job and `DeleteVersion` | Move **both** files of a version to `removed/` (D-091) | Alice |
| The Edit menu in Develop | Two undos, each named (§5.3) | Bob |
| D-142 | Parameters are stored by key, with their kind | this note's decision |

**Asked of Patrick**, each a yes or a change:

1. **Two files**: the XMP holds the state and the metadata, a JSON Lines file holds the log and the
   snapshots (§4.1). The plan's proposal was one XMP; I recommend the split for the reasons of §4.1.
2. **The compaction numbers** (§5.4): offered at 1,000 steps, forced fold of the oldest half at 10,000.
3. **The cursor is persisted** (§5.2): a version reopened after an undo can still redo.
4. **The first edit spells out the default recipe** (§3, rule 4), so that a later change of the default
   look never moves an edit that was saved.
5. **Two undos in Develop** (§5.3): the development's on Ctrl+Z, the journal's on another shortcut,
   each named in the Edit menu. If you would rather have one, it is a larger change (one history that
   holds both kinds of step) that I would put in a note of its own.

**For Charlie**: the digest function, and the mapping of §3 rule 2 (by key to the recipe's position).
**For Bob**: §5.3 and the marking of an inert operation (§8). **For Django**: the table of §11.

**Not settled here**: what happens to an instance at an older `op_version` when it is edited (§8; a
`migrate` in the plugin API, when the first operation reaches version 2); migrating a version to a
newer definition (an explicit action, when there is a v2); where **masks and local adjustments**
(M3, D-043) go: a `mask` field is added with a definition version (note 005 §2.2), and an old reader
keeps it as an unknown field; and the file syntax of a configurable pipeline definition (note 006 §3.7).
