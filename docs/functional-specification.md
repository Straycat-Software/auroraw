# Auroraw: functional specification

> **Status: living draft.** This document describes *what* the application does, not *how* it is
> built. Every item carries a status:
>
> - **[decided]**: confirmed by Patrick, recorded in [decisions.md](decisions.md).
> - **[proposed]**: working proposal, to be validated.
> - **[open]**: unresolved question.

## 1. Vision

Auroraw is a free (GPL-3.0-or-later) application for photographers: organise photos, develop RAW files and
film negative scans non-destructively, convert between formats, edit metadata and deliver
galleries, all in a simple, fast and professional workflow.

**What should set it apart** [decided]:

- **Against darktable**: simplicity and an intuitive workflow.
- **Against Lightroom** [D-089]: as simple to use for everyday work and at least on par in speed
  and capabilities, while allowing **advanced development pipelines** and **never needing a round
  trip between the cloud and the local machine** to carry out an operation. (An ambitious goal,
  knowingly. The comparison will be refined once Patrick has tried Lightroom.)
- **By design**: openness (documented data formats, plugins, no lock-in), development versions
  as a first-class concept, and delivery to clients built into the workflow.

**Simple by default, deep on demand** [decided, D-089]. Everyday work needs no configuration:
guided panels by task and a default pipeline that works out of the box (§5.4, §5.6). Depth is
the same model, not a second application. The advanced pipeline is the **configurable pipeline
definition** of §5.6 (D-037): a documented, shareable file that advanced users and plugin authors
edit, outside the everyday interface, on top of what every photographer already has (versions and
snapshots, styles, local adjustments, plugin operations). There is no expert mode that changes
how the everyday interface behaves, and what needs explaining appears only once it is used.

**One place, no round trip** [decided, D-089]. Everything Auroraw does to a photo (import,
culling, development including its AI features, export) runs on the photographer's machine, on
files it can read. No operation requires sending an original to a service to be processed or
bringing a result back, and no photo has a "cloud" state and a "local" state that the
photographer must reconcile by hand. Online storage can still be a source (§5.1, §5.10) and
delivery to a gallery service is still an explicit action (principle 5), but neither is a step
that development depends on. How originals on online storage reach the machine is still open
(§10, question 41).

### Audience and scope [decided]

- Priority 1: digital photography. Priority 2: film workflows (negative scans).
- Single-user. No shared catalogue or team collaboration for now.
- Still images only. Video is out of scope.
- Platforms: Linux, Windows and macOS from the start.
- Multilingual (the interface language is selectable in the application).

## 2. Concepts and vocabulary

| Term | Definition |
| --- | --- |
| **Source** | A place where files live: local folder, network share, removable or offline drive, memory card, or a source provided by a plugin. |
| **Original** | An image file as it exists on a source. Never modified by Auroraw. |
| **Photo** | What the photographer thinks of as "one shot": one original, or a RAW+JPEG group treated as a unit. It has a stable identity in the catalogue, independent of file name and location. |
| **Series** | Several related photos (burst, bracketing, near-duplicates) grouped for culling. Distinct from versions. |
| **Version** | One development of a photo: its chain of operations, its history, its name. Creating a version copies no pixels. A photo always has a default version, which exists only in the catalogue until the first edit and is displayed with the base look (§5.6) [proposed]. |
| **Workspace** | A folder, owned by exactly one catalogue (one workspace per catalogue), that receives everything Auroraw writes about photos: sidecars and, optionally, exports. Originals' folders are written to only in the cases of §5.1. |
| **Photo sidecar** | An XMP file in the workspace holding the metadata of an original (rating, keywords, captions, IPTC...). |
| **Version sidecar** | A file in the workspace holding one version: a copy of its photo's metadata, the version's own metadata, and its development chain, history and snapshots. |
| **Snapshot** | A named state of a version's history, which can be returned to or turned into a new version. |
| **Operation** | A non-destructive development step (exposure, curve, mask, etc.), parameterised and ordered in the chain. |
| **Style** | A named set of settings on one or several tools, reusable and applicable to one or more versions. A tool preset is a style with a single tool (D-040). |
| **Collection** | A grouping chosen by the photographer (manual) or defined by a query (smart). |
| **Catalogue** | What the photographer opens and works in: a local database that indexes photos, versions, metadata and previews, paired with its workspace. The database can be rebuilt; the workspace holds what must not be lost (D-025, D-026). |
| **Export recipe** | A named, reusable setting for producing files (format, size, profile, metadata, naming, watermark, etc.). |
| **Main version** | The version of a photo that supplies its grid thumbnail and is the default for export and publication (D-038). |
| **Base look** | The style applied to a RAW before any edit: neutral by default (D-042). |
| **Local adjustment** | A mask together with the settings it carries, a first-class object of a version (D-043). |
| **Publication** | The record of what was sent to a destination (a gallery, a folder, a site): photos, versions, published names, dates (D-049). |
| **Revision** | A later publication of the same gallery, published as a new gallery (v2, v3...) (D-053). |

## 3. Guiding principles

1. **Originals are never modified** [decided]. After import Auroraw does not move, rename or
   delete them either (D-018).
2. **Nothing is trapped in the catalogue** [proposed]. The catalogue is an index and a working
   area that can be rebuilt. Metadata and versions are also written to open files in the
   workspace (see §5.1 and §5.4).
3. **Workflows, not panels** [proposed]. The interface is organised around tasks (Catalogue,
   Cull, Develop, Publish), not a stack of tools. Import is a function, not a task: it opens as a
   dialog from the File menu or from a card's notification (D-090).
4. **Keyboard first** [proposed]. All routine culling and navigation works without a mouse.
5. **Local AI by default** [decided, D-061]. The core calls no external AI service. A plugin that
   does is off by default, needs consent per plugin and shows what leaves the machine.
   Publishing to a gallery service is a different matter: an explicit action of the photographer.
6. **Openness and interoperability** [decided for Prooftide, proposed elsewhere]. No feature may
   require a particular service or piece of software; integrations go through open formats and
   plugins.
7. **Measured performance budgets** [proposed]. See §9.
8. **Simple by default, deep on demand** [decided, D-089]. The everyday path never depends on
   the advanced one, and the advanced one is the same model, reached on demand (Vision).
9. **Local, no round trip** [decided, D-089]. No operation of the core needs a cloud service or
   a manual move of files between a cloud and the local machine (Vision; D-061 for AI).

## 4. Reference workflow [proposed]

0. **Catalogue** (D-090): the sources whose photos are in the catalogue are added and removed
   here. Adding a folder copies nothing; it scans the folder and builds the sidecars.
1. **Import** (a function of the File menu, not a task): copies the photos of a card or a folder
   to another folder, verified, with an optional second destination, renaming and metadata
   applied when the destination is in the catalogue.
2. **Cull**: ratings, flags, labels, comparison, resolving series and duplicates.
3. **Develop**: one or more versions of the kept photos; styles and settings sync.
4. **Publish**: a client gallery (Prooftide or another), with selections returned to the
   catalogue.
5. **Deliver**: batch export of the chosen versions through recipes.

Moving from one step to the next should be a single gesture (key or button), with no needless
dialog in between.

## 5. Features by domain

The milestone of each feature (M1 to M5) is described in §8.

### 5.1 Sources, catalogues and workspaces

**Catalogues** [decided, D-017]

- A photographer can have **several catalogues, all equal**, and picks one when opening the
  application. The intended use is separate areas of activity (family versus professional, for
  example).
- The application must be **fully functional with a single catalogue**. The first launch offers
  to create a workspace or to pick a known one, JetBrains-style (D-090, amending D-017's "creates
  one silently"); later launches reopen the last one, and offer the same choice when it cannot be
  found. Switching and creating catalogues is available from the File menu.
- Catalogues are isolated: search, collections, keyword lists and duplicate detection work within
  the open catalogue [proposed].
- Auroraw writes nothing into sources (see below), so catalogues do not need to claim source
  folders exclusively: two catalogues may reference the same folder, each with its own workspace
  and its own metadata. A notice tells the photographer when a folder is already in another
  catalogue [proposed].
- Each catalogue has **exactly one workspace** [decided, D-025]. To the photographer the two can
  present as a single thing: "my catalogue and its folder" [proposed].
- The catalogue database stays on the **local disk**, in the user's data folder, and is
  rebuildable from the workspace and the sources [decided, D-026]. The database is **SQLite**
  [decided, D-073]. Thumbnails and previews live in a separate cache, a SQLite database with
  32 KB pages that can be deleted without losing anything [decided, D-075, provisional pending a
  Windows measurement].

**Writing into sources** [decided, D-018, refined by D-022 and D-024]

- Auroraw writes into a source in only three cases: copying files at import; an explicit request
  from the photographer to export XMP files into source folders (see below); and an export that
  the photographer chooses to place in a source folder, which triggers a warning. It never
  modifies, moves, renames or deletes an original.
- Consequences [proposed]: the Rejected flag deletes nothing. A "Rejected" view offers *Show in
  file manager* and an exportable list of rejected files, so tidying happens outside Auroraw.
  Adding a folder "in place" copies and touches nothing. Read-only media and archives need no
  special handling.

**Workspaces** [decided, D-022 and D-023]

- A workspace is a **folder** that receives the sidecars of the originals and of their
  versions, and optionally exports. Each catalogue has exactly one, and a workspace is never
  shared between catalogues.
- The **photo sidecar** contains only the metadata of the original. It is standard XMP. It also
  caches what was read from the original file (capture time, camera, lens, exposure, size, GPS),
  so the catalogue can be rebuilt without reopening the originals, which may be offline
  [decided, D-074].
- The **version sidecar** copies the photo's metadata and adds the version's own: overrides,
  development chain, history and snapshots. A version sidecar is therefore self-contained.
- Sidecars are not next to the originals, so a sidecar is tied to its original by **content
  fingerprint**, with the last known path and file name as hints [proposed].
- **Exports** (JPEG and others) may go into the workspace or anywhere else. Exporting into a
  source folder is discouraged and triggers a warning, not a ban [decided, D-024].
- **XMP export to the source folders** [decided, D-024]: an option for compatibility with other
  software. It writes the photo sidecars of the RAW files next to the originals, on explicit
  request only (for a selection, a folder or a whole source). It is one-way and covers
  photo-level metadata. The naming convention (`photo.xmp` or `photo.ARW.xmp`) is the
  photographer's choice, `photo.xmp` by default [decided, D-028]. It is on request only:
  automatic mirroring is not planned for v1 [decided, D-066]. An existing file is merged, never overwritten blindly: only what Auroraw owns is rewritten, the
  other application's develop settings stay, and a file that application changed since Auroraw last saw
  it is held back for the review of external changes [decided, D-138].
- **Existing XMP** files found next to originals at import (from Lightroom or others) are read,
  never modified, and their ratings, labels, keywords, title, caption and IPTC fields are copied into
  the photo sidecar when the photo is first added; a photo the catalogue already knew is only
  remembered as it stands, so that later changes can be told from it (§5.7) [decided, D-133].
- Because sidecars live in the workspace, metadata edits are written immediately, whatever the
  state of the source: read-only card, unplugged drive, network share down [proposed].
- The catalogue can be rebuilt from the workspace and the sources [decided, D-026]. For that to
  be complete, everything that would otherwise exist only in the database (collections, smart
  collection definitions, the list of sources, gallery publications) is also written into the
  workspace; the format is open. Backing up the work is copying a folder.
- When a photo's metadata changes, the copies in its version sidecars **follow**, except for the
  fields a version overrides [decided, D-027]. A version sidecar is a denormalised copy of the
  effective values.

The exact layout of a workspace and its default location are open (§10).

**Source states** [proposed]

| State | Meaning | What works |
| --- | --- | --- |
| Online | Reachable | Everything |
| Offline | Not reachable (unplugged drive, network down) | Browse, search, sort, rate, tag, view previews. Metadata edits are written to the workspace as usual. Anything that needs the original (development, export, publication) is shown as unavailable, not hidden. |
| Missing | The expected location no longer exists | Same as offline, plus a prompt to relocate the source |

Each source shows its state as a badge. **Developing on an offline source is not supported in
v1** [decided, D-021]: a reduced-resolution proxy would not give results identical to the
original for detail-dependent operations (see §10).

Viewing an offline photo relies on the preview cache: if the cache is deleted while a source is
offline, those photos cannot be displayed until the source returns [proposed].

**Detecting new, changed and moved files** [decided, D-019]

- Online sources are **monitored**. New files are reported ("42 new photos in this folder") and
  added with one click; nothing enters the catalogue without confirmation. Folders or patterns
  can be ignored [proposed].
- **Moved or renamed** files are found again through a content fingerprint. An unambiguous match
  is relinked automatically and silently; ambiguous cases are put to the photographer.
- **Changed** files (edited in another application) keep their versions; the photo is marked
  "original changed" and previews are refreshed on request [proposed].
- **Deleted** files show as missing and are never removed from the catalogue automatically; a
  "Remove missing photos" action exists [proposed].
- Network shares do not deliver reliable change notifications, so those sources are rescanned on
  demand and at intervals [proposed].

**Search, filters and sorting**

- Search, filter and sort on all metadata (EXIF, IPTC, tags, ratings, dates, lens, version,
  publication state, etc.) [decided].
- Manual and smart collections, hierarchical tags, ratings [decided].

**Backup, rebuild and moving photos** [decided, D-064 and D-067]

- **Backup** is copying the workspace, which holds everything that must not be lost. v1 offers a
  **backup helper** and **rebuilding the catalogue from the workspace**, plus documented
  compatibility with file-sync tools to carry a workspace between machines. **Built-in sync is
  not in v1** [decided, D-064].
- **Moving photos between catalogues** is in v1 [decided, D-067]; the milestone is M4
  [decided], since it needs the version sidecars (M2) and the publication records (M4) and
  shares its machinery with rebuilding from a workspace. Proposed behaviour:
  - "Copy to catalogue" and "Move to catalogue" on a selection.
  - Both copy the photo and version sidecars into the destination workspace and add the sources
    to the destination catalogue if needed. The destination's keyword vocabulary merges by path,
    and conflicts are put to the photographer.
  - A move then removes the photos from the origin catalogue's index. Their sidecars go to a
    recoverable folder in the origin workspace rather than being deleted. Originals are never
    touched.
  - Publications and client-selection collections stay with the origin catalogue, since the
    galleries were published from it.
  - Sidecars are self-contained (D-023), so no special format is needed.

### 5.2 Import

**Ways to bring photos in** [proposed]

| Way | What happens |
| --- | --- |
| Copy from a card or a folder | Files are copied, verified, to a destination folder of the photographer's choosing (with an optional second destination). The destination need not be a source of the catalogue: when it is, or is added as one (offered, D-090), the copied photos also enter the catalogue; otherwise it is only a copy. A card is never registered. The card's own camera folders can be kept and merged, or the template used (D-093). Besides this, Auroraw writes into a source only for the explicit XMP export and for exports the photographer places there (§5.1). |
| Add in place | An existing folder becomes a source, from the Catalogue panel. Nothing is copied or written: the folder is scanned in the background and each photo's sidecar built from what its file says. A folder inside a source is already covered; one containing sources can merge them (D-092). |
| Other | Cameras and phones connected directly, cloud services and so on, through source plugins (§5.10). |

**Import profiles** [decided, D-029]

- A profile stores everything an import needs: destination folder template, renaming template,
  backup copy destinations, metadata template (creator, copyright, keywords), a style to apply,
  and the RAW+JPEG rule below. Applying a style at import creates the photo's first version, on
  top of the base look [proposed].
- When a card is inserted, Auroraw offers "Import with profile X": **one click**. Nothing
  starts on its own; the click is always the photographer's [proposed].
- The full dialog is the profile editor (and can run a one-off import with unsaved settings).
  Everyday imports do not go through it.

**Everything is imported, culling comes after** [decided, D-030]

- No pre-selection: every file on the card is copied.
- Culling can begin as soon as the first photos are copied, while the copy continues; thumbnails
  come from the previews embedded in the files [proposed].

**Copy safety** [proposed]

- Each copy is verified by checksum against the source.
- An optional second copy (backup) goes to another location and is verified the same way.
- An interrupted import resumes where it stopped.
- Files already imported are recognised by content fingerprint and skipped, with a report.
- A file is never overwritten; a name collision gets a unique suffix.

**After the import** [decided, D-031]

- Auroraw **deletes nothing on the card**. When every copy and backup is verified it shows "All
  N files copied and verified to X and Y" and offers to eject the card. The photographer formats
  the card in the camera.

**RAW+JPEG pairs** [decided, D-032]

- A pair is **one photo with two files**. The RAW is the file that is developed; the JPEG is a
  companion (the camera's rendering, quick delivery). Both are copied.
- A profile setting can ignore the JPEGs or the RAW files. A JPEG without a RAW is a photo like
  any other.

**Metadata and geolocation at import** [proposed]

- The profile's metadata template is written to the photo sidecars in the workspace, never into
  the copied files.
- Geolocation from GPX tracks [decided, v1] applies at import or afterwards, with a correction
  for the camera clock offset and time zone. Photos can be shown on a map.

**Formats** [decided]

- Import of all RAW formats (through import plugins; the core bundles the most common ones) and
  of all image formats used in the photo and film industries. Video is excluded.

### 5.3 Culling, series and duplicates

**Vocabulary** [decided in principle, details proposed]

- Rating from 0 to 5 stars, flags (Picked, Rejected) and colour labels. While culling they apply
  to the photo; a version may override them later (§5.5).
- Every action is one key and can be undone, several steps back [proposed].
- Rejected photos are hidden by default, with a toggle to show them. **Nothing is ever deleted**
  (D-018): the Rejected view offers *Show in file manager* and an exportable list (§5.1).

**Cull mode** [decided, D-033]

A dedicated, full-screen mode for culling. The grid stays the navigation view.

- The image fills the screen, with a discreet filmstrip [proposed].
- One key per rating, flag and label; optional auto-advance after each action [proposed].
- Side-by-side comparison of 2 to 4 images, with zoom and pan synchronised [proposed].
- A series expands in place [proposed].
- Culling can start on the last import, even while the copy is still running (§5.2) [proposed].
- Quality aids that need no AI, in M1 [proposed]: focus peaking, clipping warnings, blur score,
  optional histogram.
- AI aids (closed eyes, best frame of a series) are not among the first AI features (§5.11).
  When they come, they only **suggest** and never rate or reject on their own [proposed].

**Series** [decided, D-034 and D-035]

- **Objective series form automatically**, at import and whenever photos are added: bursts and
  bracketing recognised from metadata (sequence, gaps between capture times, exposure
  parameters). The time gap is adjustable. RAW+JPEG pairs are already one photo (§5.2).
- **Visually similar photos are suggested**, not grouped: "5 similar photos", and the
  photographer confirms. Similarity is computed locally from the previews [proposed].
- A series collapses into one thumbnail with a count and expands in place. Members can be added,
  removed, merged or split by hand [proposed].
- **Resolving a series**: the photographer designates the photos to keep, and **the others get
  the Rejected flag**. The series stays expandable. It is one gesture and can be undone.
- A series shows whether it is resolved, and can be filtered on that [proposed].
- Series membership and resolution are catalogue state, so they are also written into the
  workspace (D-026) [proposed].
- A series is not a version: a series groups several photos, a version is one photo developed
  several ways.

**Duplicates** [decided, D-036]

- **Exact duplicates** (the same content fingerprint at several locations, for example the
  working disk and its backup) are **one photo with several locations**. The catalogue uses
  whichever location is available, and sidecars and versions are not duplicated. The photo's
  information shows all its locations.
- An optional **duplicates report** lists the photos that exist at several locations (which
  sources, which sizes) so the photographer can tidy up in the file manager. Auroraw never
  deletes files. The list can be exported. Milestone: M1 or shortly after [proposed].
- **Near duplicates** are handled as series suggestions, not as duplicates.
- Import already skips files it has imported before (§5.2).

### 5.4 Development experience and versions

**Interface** [decided, D-037]

- The Develop view is organised in **guided panels by task** (Light, Colour, Detail, Geometry
  and so on) showing the common controls. Advanced tools are added on demand and appear only
  once used [proposed].
- The internal order of operations is **not something the photographer rearranges** in everyday
  use: a default pipeline that works out of the box is provided (§5.6). Configuring the pipeline
  is possible for advanced users and plugin authors, outside the everyday interface.
- Tools shared by every view [proposed]: before/after (split or side by side), histogram,
  clipping warnings, 100% zoom, and moving to the previous or next photo from the keyboard
  without leaving Develop.

**History and snapshots** [proposed, principle decided in D-006]

- Each version has a **linear history**, kept persistently in its sidecar; a button compacts it.
  Editing after an undo discards the undone steps: exploring variants is what versions and
  snapshots are for.
- A **snapshot** is a named bookmark in a version's history. It can be compared with the current
  state, restored, or turned into a new version.

**Versions** [decided: versions, virtual copies and snapshots in v1]

- A photo can have as many versions as wanted ("high-contrast black and white", "client
  version", "cinema experiment"). A version is created from scratch, from another version, from
  a snapshot or from a style. A one-key "duplicate and try" creates a version from the current
  one [proposed].
- Versions **share the original** and copy no pixels.
- **Main version** [decided, D-038]: each photo has one main version, chosen by the photographer
  (by default the last one edited). It supplies the grid thumbnail and is the default for export
  and publication. Other versions are **collapsed** under the photo, with a badge showing how
  many there are, and unfold on demand.
- In Develop, a **version strip** switches between versions with one key [proposed].
- The grid shows the main version's effective metadata (§5.5) and marks a photo whose value is
  overridden. Cull mode works on the photo's values (the composition) and Develop on the
  version's values (the rendering) [decided, D-063].
- Two versions can be compared side by side or with a wipe [proposed].
- Every output operation (export, publication) applies to a chosen version, to the main version
  or to a set of versions [proposed].

**Reusing settings between photos** [decided, D-039 and D-040]

- **Selective copy and paste**: copy chosen settings, paste them onto one or several photos.
- **Styles**: a style is a named set of settings covering one or several tools. There is **one
  concept only**: what other software calls a tool preset is simply a style that holds a single
  tool, and appears in that tool's menu as well as in the global style list.
- Applying a style or a paste is a one-off action; it leaves **no lasting link** between photos.
  Changing a style later does not alter photos that already used it.
- **Auto-sync** (optional switch): each adjustment made on the active photo is applied live to
  the selection, for a series or a whole shoot.
- A style can be applied at import (import profile, §5.2) and can be applied selectively
  (which tools) [proposed].
- Styles live at user level, shared by all catalogues (D-052, §5.8).

**Persistence of versions** [proposed]:

| Layer | Contains | Format |
| --- | --- | --- |
| Catalogue | Index, previews, queries, anything that speeds things up | Local database, rebuildable |
| Photo sidecar (workspace) | The original's metadata: keywords, ratings, captions, IPTC, rights, and a cache of its capture data (D-074) | Standard XMP, readable by other software |
| Version sidecar (workspace) | A copy of the photo's metadata, the version's own metadata, its chain, history and snapshots | Open, documented format built on XMP if practical, with a schema version in each file |
| State files (workspace) | What would otherwise live only in the database: collections, series, keyword vocabulary, sources, publications | Open, documented format (§10, question 3) |

The workspace lets the catalogue be rebuilt from the sidecars and the sources. Development
settings from other software are not portable as-is; a partial import of common settings
(Lightroom, darktable) is conceivable later [open].

### 5.5 Metadata: photo and version [decided for ratings and keywords; other fields proposed]

Some metadata can be **overridden at the version level**. Example given: rate the photo for its
composition, then rate each version for its rendering.

**Rule** [decided in principle, details proposed]: the effective value of an overridable field is
the version's if it defines one, otherwise the photo's. The interface shows both and makes clear
where the displayed value comes from.

| Field | Scope | Proposed behaviour |
| --- | --- | --- |
| Rating (stars) | Photo, overridable per version | Replacement. |
| Flag / colour label | Photo, overridable per version | Replacement. |
| Keywords | Photo, plus additions on the version | Union of the photo's and the version's keywords. In v1 a version can only add, not remove [decided, D-014]. |
| Title, caption | Photo, overridable per version | Replacement. |
| Capture EXIF, GPS | Photo only | Not overridable. |
| Copyright, creator | Photo, applied at export | Not overridable (except by export recipe). |

**Consequences**:

- Filters and sorts ("4 stars or more") apply to the effective value; one can choose to filter on
  the photo alone.
- Export and publication write the **effective** metadata of the exported version into the
  produced file.
- The photo sidecar carries the photo's values as-is. A version sidecar carries the effective
  values and records which fields it overrides, so inheritance survives a rebuild [proposed].

### 5.6 Development (RAW, scans, images)

**Rendering model** [decided, D-041]

- The pipeline is **scene-referred**: calculation is linear, in floating point, with no
  brightness ceiling, and a single display transform produces the visible image.
- The interface exposes **familiar controls** (exposure, contrast, highlights, shadows, whites,
  blacks) that drive suitable operations internally. The scene-referred model's own tools are not
  shown as such (consistent with D-037).
- What this brings: highlight recovery, the film-industry formats (OpenEXR, ACES) and more
  accurate rendering, without darktable's learning curve.

**Colour management** [decided]

- Input profiles, a wide-gamut linear working space, output profiles. Soft proofing is planned
  for later and the architecture allows it.
- The working space is fixed by default, in the Rec.2020 class [proposed]. The display profile
  is detected on each operating system [proposed].

**GPU and CPU** [decided: GPU acceleration; proposed: CPU fallback]

- Development runs on the GPU. A **CPU fallback** is required for machines without a capable
  GPU: the same result within a stated tolerance, only slower.

**Base look** [decided, D-042]

- When a version is created, Auroraw applies a **base look**, which is a style (D-040): a
  faithful, pleasant, tone-mapped image with no artistic treatment. An unedited photo is shown
  with the base look, and no version is written until its first edit.
- The photographer can choose another base look by default, in the preferences or in an import
  profile. The base looks shipped include the neutral one and a **flat linear** one.
- A base look that imitates the camera's embedded JPEG is a **planned evolution** [decided]. The
  manufacturers' renderings are proprietary and differ for every camera, so it is more likely a
  plugin or a community profile.

**Operations** [proposed]

| Milestone | Operations |
| --- | --- |
| M2 | RAW input (demosaicing, white balance, exposure and black point, highlight reconstruction, hot pixels). Tone (contrast, highlights, shadows, whites, blacks, curve). Colour (saturation, vibrance, hue-saturation-luminance, colour grading). Detail (sharpening, noise reduction). Geometry (crop, straighten). |
| M3 | Local adjustments, lens corrections, negative conversion, and advanced operations (dehaze, clarity, perspective, vignetting, grain). |

**Local adjustments** [decided, D-043; milestone M3]

- A local adjustment is a **first-class object**: a mask together with the settings it carries.
  They form a list that can be stacked, reordered and hidden.
- Mask types [decided, v1]: gradient, radial, brush, luminosity range and colour range.
  Combining masks (add, subtract, intersect) is proposed.
- Internally a local adjustment becomes an operation with a blend mask; the photographer never
  has to know that.
- AI-assisted selections (subject, sky, people) come in M5 (§5.11) [decided, D-058].

**Lens corrections** [decided: v1, milestone M3]: through Lensfun.

**Styles and settings reuse** [decided, v1]: see §5.4.

**Negative scans** [decided, priority 2; D-044]

- The inputs supported first are:
  - negatives photographed with a **camera on a light table** (RAW);
  - files from a **dedicated film scanner** (16-bit TIFF or DNG, with or without an infrared
    channel);
  - **third-party or lab scans**, including positives and partly processed files.
- Flatbed scanner files open as ordinary images. They get no specific handling (scanner
  profile) in v1.
- v1 tool: inversion, film-base (orange mask) sampling, per-channel curves, frame cropping.
  Later: emulsion profiles, infrared dust detection, multi-exposure scans.
- The inversion is an operation in the input stage of the pipeline, working on linear data
  [proposed].
- Driving a scanner (acquisition) would be the job of a source plugin, outside v1 [proposed].

**Later** [decided]: merges (HDR, panorama, focus stacking).

**Pipeline composition and operation plugins** [proposed, following D-037]

The photographer does not manage the order of operations, but plugins must fit into it. The
model:

- The pipeline is described by a **pipeline definition**: a documented, versioned list of
  **stages**, each working in a defined data space (for example: raw, scene-linear working
  space, display-referred, output). Auroraw ships a default definition that works.
- Each operation, built-in or plugin, **declares** its stage, optional ordering constraints
  within the stage ("after exposure", "before sharpening"), and its capabilities (GPU shader,
  needs neighbouring pixels and how many, depends on pixel scale, supports masks) and the panel
  it belongs in (Light, Colour, Detail...).
- Auroraw **places** an operation from its declaration; a plugin with no constraint goes at the
  end of its stage. Conflicts are reported at load time, not silently resolved.
- The order used is **recorded in each version's sidecar**, with the version of each operation
  and its plugin. An old edit therefore renders the same way after a plugin is installed or
  updated.
- A version that uses a **missing plugin** opens with that operation disabled and clearly marked;
  its settings are kept in the sidecar and are never dropped.
- Advanced users and plugin authors can **configure the pipeline definition** (a documented,
  shareable file). Configuration is outside the everyday interface, changes only apply to new
  versions, and is validated (an operation cannot leave its stage).

### 5.7 Metadata

**Fields** [proposed]

- EXIF: all fields are read; the common ones can be edited.
- IPTC Core and a subset of IPTC Extension: title, caption, creator, copyright, usage terms,
  credit, source, location (sublocation, city, region, country), date created, keywords.
- XMP basics: title, description, creator, rights, keywords, rating, label.
- Custom fields are conceivable later, through plugins [open].

**EXIF edits are overlays** [proposed, following D-018]

- Originals are never modified, so a correction (time and time zone, GPS, camera and lens names
  for old lenses, orientation) is stored in the photo sidecar as an **overlay**. The original
  value stays visible and can be restored.
- The overlay applies in the catalogue, in exports and in the XMP export to source folders.

**Batch editing** [proposed]

- With several photos selected, the metadata panel shows "multiple values" wherever they differ,
  and an edit applies to the whole selection.
- Metadata templates (creator, copyright, keywords) apply at import (D-029) and at export.
- Find and replace on a field; time shift on the whole selection (for a camera clock that was
  off), which also feeds the GPX matching (§5.2).

**Keywords** [decided, D-045]

- A **hierarchical vocabulary** (Place > Canada > Quebec) that the photographer builds and
  reorganises, with optional **synonyms** and a **do not export** flag per keyword, for private
  words (client names, internal notes). At export the recipe chooses whether parents are
  included.
- Renaming, merging or moving a keyword updates every photo that uses it. The vocabulary is
  written into the workspace (D-026), so it follows the catalogue.
- Assigning is fast: type-ahead from the vocabulary, or drag and drop [proposed].
- A version can add keywords to those of its photo (D-014).
- For compatibility, sidecars carry both the flat keyword list and the hierarchy in the forms
  other software reads, and the hierarchies written by other software are understood on import
  [proposed].

**Location** [decided, D-048]

- Photos are shown on a map. A location is set by GPX matching (§5.2), by dragging photos onto
  the map, or by hand [decided, v1].
- Place names (city, region, country) come from a **bundled offline database** (GeoNames, under
  a CC BY licence), so nothing leaves the machine. Finer place names (district, street) come
  from an optional plugin that queries an online service, only with the photographer's consent.

**Metadata in exported files** [decided, D-046]

- It is **configurable per export recipe**, with named settings offered: *Everything*, *No
  location*, *Rights only*, *None*.
- The **default** is everything except the location and the camera's serial number, the two
  classic leaks.
- The metadata written is the **effective** metadata of the exported version (§5.5).

**External changes to XMP files next to originals** [decided, D-047]

- Monitored sources (§5.1) also report XMP files changed by another application after the
  import: "12 photos have metadata changed by another application". The photographer accepts
  with one click, or ignores.
- The merge compares against the last state Auroraw read, so only fields changed outside are
  applied, and a field changed on both sides is put to the photographer [decided, D-134].
- Auroraw recognises the XMP files it exported itself (D-024) and never reports them as external
  changes [proposed].

### 5.8 Export

**Recipes** [decided, v1]

- Export to all image formats used in the photo and film industries [decided], including
  converting a RAW to DNG.
- A recipe holds [proposed]: format and its options, size (long edge, resolution), output
  sharpening, colour profile and bit depth, the metadata setting (§5.7, D-046), the naming
  template (with a `{version}` token), the watermark, the destination and its sub-folders.
- There are three kinds of recipe [proposed]: **image export**, **copy of the original files**
  (to hand over RAW or JPEG files unchanged) and **gallery publication** (through a plugin,
  §5.9).
- The **watermark** is a general export operation, available to every recipe, not reserved to a
  gallery plugin. It offers text or a logo, corner or centre placement, and a placement that
  avoids faces. The logic comes from Prooftide, ideally as a shared module [proposed].
- Recipes, styles and import profiles live at **user level**, shared by all the catalogues, in
  the user's configuration folder, and can be imported and exported as files [decided, D-052].
  A publication keeps a **copy of the recipe it used**, so it can be republished identically.

**Running an export** [proposed]

- Exports go through a **background queue**: not blocking, cancellable, with a report for each
  file.
- Each image is rendered from its original through the full pipeline, at full quality.
- Export of one or several versions per photo, the main version by default (§5.4).
- **Milestones** [decided, D-062]: a minimal export (JPEG, TIFF, PNG; size, profile, metadata
  setting) ships in M2 so that development is usable on its own. Recipes, the queue, the
  watermark and publication come in M4.
- Destination: the workspace or any folder. Exporting into a source folder shows a warning
  [decided, D-024].

**Tracking publications** [decided, D-049]

- Whatever the destination (a gallery, a delivery folder, a site), Auroraw records what was
  published: photo, version, published name, date and a fingerprint of the settings used
  [proposed]. This record is catalogue state and is also written into the workspace (D-026).
- A photo published and then edited shows a **"modified since publication"** badge.
- **Republishing is manual**: a "republish changes" action sends the modified items. Nothing is
  ever sent on its own, so an unfinished edit never reaches a client.
- What "republish" does depends on the destination [proposed]: one that can **update items in
  place** (a delivery folder) receives only the modified items; one that cannot (a service that
  only creates galleries, Prooftide included) receives a **new revision** (§5.9).

### 5.9 Galleries and Prooftide

**Principle** [decided]: a polished integration without interdependence. Auroraw must work with
gallery services other than Prooftide, and Prooftide must stay usable with Lightroom and other
workflows. Prooftide may be extended to integrate better with Auroraw, but such extensions are
optional and backward compatible.

**Model** [proposed]:

1. **Gallery export plugin**: Prooftide is the first implementation, separate from the core.
   Another service plugs in through the same mechanism.
2. **Publishing a version**: rendering (colour, resizing, watermark) happens in Auroraw's
   pipeline, within the service's constraints. For Prooftide: JPEG, 1600 px maximum, 15 MB per
   file, unique file names in a gallery, and the limits of the photographer's plan. The plugin
   warns before a publication would exceed the plan's limits.
3. **Published name** [decided, D-009]: Auroraw generates a unique file name for each published
   version and keeps the mapping *gallery → photo, version, published name* in the publication
   record (§5.8). **No server change is required.** Proposed rule: the name stays that of the
   original when a single version of the photo is published, and gets a suffix
   (`marie_black-and-white.jpg`) when several are published or on collision.
4. **Delivery**: from the selection, one action exports the chosen versions through a recipe, or
   copies the original files. Delivered versions are rendered from the catalogue.
5. **Licence key**: it is kept in the operating system's keychain, not in clear text
   [proposed].

**Revisions of a gallery** [decided, D-053]

- "Republish changes" on a gallery publishes the **whole collection again as a new gallery**, a
  revision (v2, v3...), named after the first one ("Marie wedding (v2)") and with a new link for
  the client.
- Nothing changes on the Prooftide side: no need to replace or remove single photos. It works
  for any service that can only create galleries; a plugin declares whether it can update in
  place.
- A publication is the **series of its revisions**. Each revision has its own record (§5.8) and
  its own client-selection collection; a cumulative view of the selections across revisions is
  offered.
- At republish time the photographer chooses the scope: the **whole collection** (default) or
  only the modified and new photos.
- Each revision uses a gallery slot and its photos count towards the plan's limits. The plugin
  says so before publishing and offers to delete earlier revisions once they are no longer
  needed. On a plan with a single gallery, that means deleting before publishing.
- The client's earlier selection does not carry over by itself. Pre-selecting it in the new
  revision would need an optional API extension [open].

**Client feedback** [decided, D-050]

- Each published gallery gets a **"client selection" collection**, filled automatically, with a
  badge on the photos. Selections are resolved through the published-name mapping above.
- The photographer may also have Auroraw apply a flag, a colour label or a keyword of their
  choice to the selected photos, per gallery. A photo chosen in two galleries never conflicts,
  since the collection is per gallery.
- **Annotations** (text, audio, drawing for Prooftide) are stored in the workspace with the
  photo and shown on it, in Cull mode and in Develop: text in a panel, the drawing as an overlay,
  the audio playable [proposed].
- Feedback is fetched on request, and optionally checked periodically with a notification
  [proposed].
- A gallery published from Prooftide's own application can be **linked** to the catalogue: its
  selections are matched to photos by file name, and ambiguous matches are put to the
  photographer [proposed].

**Prooftide API** [decided, D-051]

- The routes the plugin uses become a **documented, versioned public API**, with a compatibility
  policy, that Auroraw and any other client can use. The present application-version mechanism
  (the 426 response) is the starting point. The plugin knows no more than any other client.
- This is work on the Prooftide side, backward compatible: creating a gallery, uploading photos,
  listing and deleting galleries, selections, annotations, and the account's plan limits and use,
  which the plugin needs to warn before exceeding them [proposed].

**Interoperability safeguards** [proposed]:

- **Fallback without a plugin**: Auroraw can export a folder of ready-to-publish JPEGs, which can
  be handed to the current Prooftide application or to any other service.
- **Generic selection import**: from a list of file names or a selection file, for any gallery
  service or any client.

### 5.10 Plugins

Four families [decided]: **import** (including RAW reading), **export** (images and galleries),
**image-processing operations**, **image sources**.

**What each family provides** [proposed]

| Family | What it provides |
| --- | --- |
| Import | Reads a file format: pixels (linear), metadata, embedded preview and thumbnail. RAW decoders belong here. |
| Export | Writes a file format, with its options declared. Also gallery services: creating, updating and deleting items, the service's limits, whether it can update in place, and fetching client feedback (§5.9). |
| Operation | Parameters, a shader or kernel, its stage and ordering constraints in the pipeline (§5.6), mask support, the panel it belongs in. |
| Source | Lists, describes, reads and watches files; reports online or offline; read-only or writable; handles its own sign-in. |

**Plugin manager and behaviour** [proposed]

- A plugin manager built into the application lists installed plugins and lets the user enable,
  disable and update them, see their permissions, licence and errors.
- A failing plugin never crashes the application and never corrupts a catalogue or a workspace.
- Each plugin declares an identifier, its version, the API version it targets, its family, its
  permissions and the languages it provides. It ships its own translations.
- A plugin's interface is **declarative** (forms, sliders, curves, pickers) and displayed by
  Auroraw, so it stays consistent, translatable and accessible. Fully custom interfaces are not
  planned for v1.
- Versions record which plugins they need (§5.6), and a missing plugin never loses settings.

**Declaration** [decided, D-078]

- Every plugin carries a declaration: identifier, version, API version, family, panel, the
  pipeline stage and ordering constraints (§5.6), parameters with limits and defaults, and
  permissions. The host refuses a declaration it cannot satisfy, with the reason.

**GPU operations** [decided, D-077]

- An operation plugin may ship its shader as **data**, with a **CPU twin** in WebAssembly that
  must give the same result. The host validates the shader, places the operation from its
  declaration, and refuses it if it is invalid.
- A GPU shader cannot be interrupted the way a WebAssembly loop can, so a **safety and portability
  check** is mandatory before a plugin is admitted to the index: every loop proven bounded, and
  the shader compiled on every graphics API (Vulkan, Metal, DirectX 12).

**Distribution** [decided, D-054]

- A **catalogue of plugins** built into the application, fed by a **public, open index** (a
  repository describing plugins, versions and fingerprints), with **trust levels**: official,
  verified, community.
- **Manual installation** from a file or an address stays possible, for private plugins and for
  development.
- No hosted store: no accounts, moderation or payments to run.

**Trust and permissions** [decided, D-055]

- A plugin runs in a **sandbox** by default. It declares what it needs (network, files,
  keychain) and the user sees those permissions **before installing**.
- The sandbox is **WebAssembly, run by wasmtime** [decided, D-076]. A plugin has no access to
  files, the network, the environment or other processes unless the host grants it; each has
  limits on memory and running time and is stopped if it exceeds them, without harm to the host.
  Plugins are compiled once at installation and the compiled copy is kept.
- Plugins that need native code (fast RAW decoders) may request a **native** level. It is
  clearly flagged and needs explicit confirmation.

**Source plugins: first scenarios** [decided, D-056]

- The interface must first allow: a **folder on a local disk** and a **card mounted in the file
  system** (the core's own sources use the same interface, so it is exercised from the start
  [proposed]); a **camera or phone connected** by USB (PTP or MTP); and **online storage** (S3,
  WebDAV, Google Drive, Dropbox), with the online and offline states of §5.1.
- **Scanner acquisition** is not among the first scenarios.
- **Importing from other software** (Lightroom, digiKam, darktable catalogues) is kept for
  later, together with the question of importing their settings (§10).

**Timeline** [decided]

- Internal modules use the same interface as external plugins: the core's own sources from
  milestone M1, operations and the other families from M2, so the interface is validated early. The public API is frozen in milestone M5, not before; until
  then it is marked experimental [proposed].
- Operation plugins must be able to run on the GPU and declare where they sit in the pipeline
  (§5.6).

**Licensing** [D-080, D-082]: the application is GPL-3.0-or-later; the plugin API, SDK and
declaration schema are MIT OR Apache-2.0; a plugin that uses only the plugin API may have any
licence (docs/governance.md §2.3).

Language, ABI, isolation technology and hosting of the index are handled in the "development
process" phase (§10).

### 5.11 AI

Integrated where relevant [decided]; principles in §3 (local by default).

**First priorities, milestone M5** [decided, D-058]

- **Selections for masks**: subject, sky and people, to create the mask of a local adjustment in
  one click (§5.6). "People" means segmenting a person in the image, not identifying anyone.
- **Noise reduction**, with **upscaling** to follow. It is a pixel operation, so the model used
  is recorded (see below).
- Not prioritised for now, and considered later: assisted culling (closed eyes, sharpness, best
  frame of a series), semantic search and keyword suggestion, captions and translation of
  metadata.

**Principles** [proposed]

- **AI suggests, the photographer decides.** It never rates, rejects, tags or edits an image on
  its own. Suggested keywords would go to a queue, and only the accepted ones become metadata in
  the sidecar.
- **Reproducibility**: an AI operation that touches pixels records the model's identifier and
  fingerprint in the version sidecar, as plugins do (§5.6). Without that model, the version
  opens with the operation disabled and marked, and no setting is lost.
- **Derived data** (scores, visual fingerprints, suggestions) can be recomputed, so it stays in
  the catalogue's cache and not in the workspace, unlike the catalogue state of D-026.
- **Not planned**: face recognition (biometric and legally sensitive, for instance under the GDPR
  and Quebec's Law 25) and generative retouching (filling in or removing objects: authenticity,
  model provenance and licences). They would be added only at Patrick's explicit request.
- The official index only accepts **redistributable models**, and shows the licence before
  download.

**Models** [decided, D-059]

- The core ships **no model**. Each feature offers to download its model pack on demand, showing
  its size and licence; it is cached locally and can be deleted.
- A model pack is a plugin in the sense of §5.10: same index, same trust levels.

**Hardware** [decided, D-060]

- Every AI feature is available on the **CPU**, slowly: it runs as a **background task** with an
  estimated duration and never blocks the interface. With a capable GPU it is faster.

**Online AI** [decided, D-061]

- The core calls **no external AI service**. A plugin may, for example for captions or
  translation. It is **off by default**, needs consent per plugin, and shows what leaves the
  machine (a reduced image, a crop, metadata). No telemetry.

### 5.12 Interface and languages

- Multilingual interface [decided]; translations are plain language files open to contributions.
- Professional and friendly [decided]: see the principles in §3.
- Light and dark themes, with a neutral-background viewing mode for judging colour [proposed].
- Accessibility (keyboard, contrast, screen readers) [proposed].

## 6. Formats [decided in principle]

| Category | Scope |
| --- | --- |
| RAW | All manufacturer RAW formats, DNG included. The core relies on an existing decoding library; exotic cases go through import plugins. |
| Images | JPEG, TIFF, PNG, WebP, HEIF/HEIC, AVIF, JPEG XL, OpenEXR, DPX, PSD (read), etc. |
| Film industry (stills) | DPX, OpenEXR, 16 and 32-bit TIFF, ACES/OCIO management considered. |
| Video | Out of scope. |

The detailed list (read-only or read and write, per format) will be drawn up in milestone M1
[open].

## 7. Explicitly not in v1

Merges (HDR, panorama, stacking), soft proofing and printing, tethering, multi-user
collaboration, shared catalogues, video. Architecture decisions taken before those milestones
must not rule them out (notably soft proofing, which depends on colour management).

## 8. Milestones [decided]

| Milestone | Content | What it gives the user |
| --- | --- | --- |
| **M1** | Catalogues and workspaces (photo sidecars, keyword vocabulary, state files), sources, import (card and folders, import profiles), previews, culling, series and duplicates, keywords, ratings, IPTC/XMP, XMP export to source folders, search and filters, GPS | A complete culling and organising tool |
| **M2** | Non-destructive pipeline (basic operations), versions and snapshots, presets and styles, colour management, GPU acceleration, internal modules written as plugins, minimal export (JPEG, TIFF, PNG; size, profile, metadata setting) | Basic RAW development, from the card to a file |
| **M3** | Masks and local retouching, lens corrections, negative scan module, advanced operations | Professional-level development |
| **M4** | Export recipes, queue and watermark, Prooftide plugin, selection feedback, backup helper, rebuild from the workspace, moving photos between catalogues | The full loop: from the card to the client |
| **M5** | Public, documented plugin API, AI features (subject, sky and people masks; noise reduction), polish, documentation | Openness and ecosystem |

Each milestone must be usable on its own. The exact content of each will be refined when we get
to it.

## 9. Performance budgets [proposed, to be validated by measurement]

| Situation | Target |
| --- | --- |
| Scrolling a grid of 10,000 thumbnails | No perceptible stutter |
| Showing a photo from the preview cache | Under 100 ms |
| Visual feedback for a development adjustment | Under 50 ms on a 24 MP image, standard GPU |
| Opening the catalogue cold with 100,000 photos | A few seconds |
| Search across 100,000 photos | Under 200 ms |

## 10. Open questions

Sorted by when they need an answer. No question is left to decide before the technical phase.

| When | Questions |
| --- | --- |
| **Resolved** | 5, 19, 39, 40 |
| **Technical phase** | 1, 2, 3, 4, 6, 10, 11, 13, 17, 20, 21, 22, 25, 28, 29, 31, 34, 35, 36, 38 |
| **Milestone planning**, when the milestone is reached | 7, 8, 9, 12, 14, 15, 16, 18, 23, 24, 26, 27, 30, 32, 33, 37 |

1. **Second machine and file sync**: backup and sync are settled for v1 by D-064. What remains:
   how the second machine's local database notices and absorbs changes in a synced workspace
   (rescan by modification time and fingerprint).
2. **Workspace layout and default location**: mirror of the source folder tree, or one folder per
   photo? Human-readable names plus a short identifier, or identifiers only? Where does a new
   workspace go by default?
3. **Format of the catalogue-only state** written into the workspace (collections, smart
   collections, sources, gallery publications).
4. **Format of the version sidecar**: pure XMP with an Auroraw namespace, or XMP plus a companion
   file for the history.
5. ~~Automatic XMP mirroring~~: resolved, not in v1 (D-066).
6. **Content fingerprint**: what is hashed (whole file or head, tail and size) so relinking and
   sidecar association stay fast on network shares without false matches. A fingerprint alone
   cannot follow a file edited elsewhere, since its content changes: a photo needs a stable
   identifier stored in its sidecar, with the fingerprint, the last known path and capture data
   as hints for relinking.
7. **Workspace unavailable** (for example on a network share that is down): read-only mode, or
   queue the edits?
8. **Moving photos between catalogues**: wanted, in M4 (D-067). To settle when
   planning M4: how keyword vocabularies merge, and the details of the recoverable folder.
9. **Offline development**: reconsider after the pipeline exists (M2 or later). A proxy would be
   reliable for colour, tone and geometry, but not for operations that depend on pixel scale:
   sharpening, noise reduction, local contrast, retouching, lens corrections at the edges. It
   could be offered for the reliable subset, with a warning.
10. **Prooftide API v1**: the exact scope of the documented API, its compatibility policy, and how
    a plugin identifies itself and is authorised (work in the Prooftide repository).
11. **Plugin technology**: the sandbox is settled (D-076) and so are the declaration and GPU
    shaders as data (D-077, D-078). Left open: the interface between host and plugin (a hand-made C
    interface was used; the WebAssembly component model would give typed interfaces), what
    "native" means for decoders, WebAssembly threads, and how the index is hosted and its
    entries signed.
12. **Importing settings** from other software (Lightroom, darktable): useful, and how far?
13. **Dependency licenses**: compatibility with GPL-3.0 (RAW libraries, Lensfun and its database,
    AI models).
14. **Import details**: default destination and renaming templates and their tokens, what happens
    with cards from several cameras or with clashing file numbers, time zone handling for GPX,
    and how card insertion is detected on each operating system.
15. **Culling details**: how an objective series is detected (metadata rules, default time gap),
    how visual similarity is computed (perceptual hashes or learned embeddings, and the cost on
    100,000 photos), which key bindings, how a series' cover is chosen, and which location a
    multi-location photo prefers (online first, then fastest).
16. **Duplicates with different metadata**: when two copies of the same file carry different XMP
    (ratings, keywords) found at import, how are they merged?
17. **Pipeline definition**: exact stages and data spaces, the ordering-constraint language, how a
    plugin's shader is packaged, and how conflicts between plugins are shown to the user. Spike 1
    adds three inputs: the order of operations decides how interactive the application feels
    (white balance after the denoiser is 200 times cheaper to change), so a definition may need to
    express which operations are heavy and rarely changed; an operation may need a cheaper draft
    quality for dragging; and a plugin's shader must stay within a subset of WGSL that every
    graphics API accepts, since DirectX rejected a construct Vulkan and Metal allowed.
18. **Auto-sync**: how it behaves with photos that already have different settings for the same
    tool (overwrite, or only relative changes such as an exposure delta)?
19. ~~Main version and metadata~~: resolved by D-063.
20. **History size**: brush strokes and masks can make histories large; what does "compact"
    keep, and at what point is it proposed automatically?
21. **Working space**: fixed in the Rec.2020 class, or selectable? How are OCIO and ACES handled
    for the cinema formats?
22. **GPU programming interface** on the three platforms, and the tolerance that defines "the same
    result" between GPU and CPU (a technology decision for the next phase).
23. **Base look**: the tone-mapping method, and how the flat linear look is presented to the
    photographer.
24. **Negative conversion**: how the film base and the emulsion are modelled, how infrared dust
    detection works, and what a camera-scanned negative needs (light source, exposure) to convert
    well.
25. **Masks**: how brush strokes are stored so they stay independent of the image resolution.
26. **Field list**: the exact IPTC Extension fields, and whether custom fields are wanted.
27. **Keyword vocabulary**: importing an existing vocabulary (a Lightroom keyword list, digiKam),
    and which hierarchical XMP forms are written and read.
28. **XMP monitoring**: how to avoid feedback loops between the XMP export (D-024) and the
    detection of external changes, and how quickly a change is reported.
29. **Geocoding database**: size, attribution required by its licence, update mechanism, and the
    levels of detail offered.
30. **Gallery revisions**: carrying the client's previous selection over to a new revision (an
    optional Prooftide API extension), and how revisions are presented to the client.
31. **Modified since publication**: what enters the settings fingerprint (pipeline settings,
    recipe, original fingerprint, metadata) and what it costs to compute.
32. **Client feedback**: refresh rhythm, notifications, and what happens to a selection when a
    published photo is no longer in the catalogue.
33. **Linking an existing gallery**: how ambiguities are resolved when several photos share a
    file name.
34. **Plugin licensing (D-057)**: settled by D-080 and D-082 (core GPL-3.0-or-later, API and SDK
    MIT OR Apache-2.0, plugins under any licence through the plugin exception).
35. **Source plugin details**: how a source reports changes for monitoring (§5.1), how sign-in
    and refreshed credentials are stored, and how a camera source presents its files to the
    import (§5.2).
36. **AI runtime**: the inference technology and model format, and how CPU, GPU and the operating
    systems' own accelerators are used on each platform (a technology decision).
37. **Model acceptance in the official index**: the criteria on licence and provenance, notably
    for weights released under non-commercial or research-only terms.
38. **AI noise reduction in the pipeline**: where it sits (before or after demosaicing), and how
    to keep results stable across CPU and GPU (see 22).
39. ~~Export before M4~~: resolved by D-062.
40. ~~Rating level while culling~~: resolved by D-063.
41. **Originals on online storage without a round trip** (follows D-089; partly answered by
    D-090: an original on a network or cloud source is copied to the machine **on demand, when it
    is developed**, and the copy is the working file): for an original on an
    online source (S3, WebDAV, Google Drive, Dropbox; §5.1, §5.10), how development and export
    obtain its bytes with no manual download step: fetched once on demand into a cache with a
    rule the photographer sets, what is shown while it is fetched, and how this sits with D-021
    (developing while offline stays unsupported, so an unreachable original is "unavailable").
42. ~~Advanced pipelines beyond the pipeline definition~~: resolved by D-089. An advanced
    pipeline is the configurable pipeline definition of §5.6 (D-037) and nothing more is planned
    for it; its details stay in question 17.

## 11. Next steps

1. ~~Detail each domain of §5 with usage scenarios.~~ Done on 2026-09-19 for sources and
   catalogues, culling, development and versions, metadata, export and Prooftide, plugins and AI.
2. ~~Review the whole specification for consistency, and sort the open questions of §10.~~ Done
   on 2026-09-19. The six questions it raised were settled the same day (D-062 to D-067).
3. ~~Define the development process~~ (architecture, technology stack, testing, continuous
   integration, open source governance, plugin licensing). Done on 2026-09-20: see the
   documents listed in the README (D-068 to D-083).
4. ~~Plan milestone M1~~: drafted in [m1-plan.md](m1-plan.md).
