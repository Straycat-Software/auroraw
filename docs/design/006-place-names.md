# Design note 006: place names, offline, and the place filter

> **Status: proposal, for Patrick's approval.** WP10's last two items ([M1 plan](../m1-plan.md) §5, §6
> item 12; specification §5.7 "Location", open question 29; D-048). It answers question 29 of the
> specification (§10): the size of the database, the attribution its licence requires, how it is
> updated, and the levels of detail. Nothing here is built. Items are tagged **[decided]**,
> **[proposed]** or **[open]**; the sizes marked **[to measure]** are what I remember of GeoNames' files,
> not measurements, and the first slice measures them before anything depends on them.

## 1. The question, and what exists

A photo that has a position (read from the file, matched from a GPX track, or corrected by hand) should be
able to say *where*: a city, a region, a country. Two uses: the IPTC fields other software shows and
searches (and the XMP export writes), and **a place filter** in the library ("everything from Quebec").
D-048 decided the source: a **bundled offline database** (GeoNames), with finer names only through an
optional online plugin and with consent. Nothing has been built.

What is already there:

- **The fields.** The photo sidecar carries `photoshop:City`, `photoshop:State` (the region),
  `photoshop:Country`, `Iptc4xmpCore:CountryCode` and `Iptc4xmpCore:Location` (the sublocation)
  (`Metadata.city`, `region`, `country`, `country_code`, `sublocation`); the engine edits them
  (`MetadataField::City`…) in batches, undoably; the XMP export writes and reads them.
- **The position.** The catalogue has `gps_lat` and `gps_lon` for each photo (the effective position: the
  photographer's correction wins over the file's); the sidecar holds the original's and the overlay's.
- **The means.** The bundled SQLite has the R*Tree module (`SQLITE_ENABLE_RTREE`), so a spatial index
  costs no dependency. Background jobs with progress, cancellation and one undo step exist
  (`batch_job`, D-126, D-127).

What is not: any place database, any lookup, any code that fills the fields, any place filter. The filter
of the library has rating, flags, label, series, keyword and collection (D-098, D-125); a place is the
next fact.

## 2. The data [proposed]

**GeoNames** (CC BY 4.0) publishes, among others, `citiesNNN.zip` (every populated place above NNN
inhabitants, with its position, country code and first-level administrative code), `admin1CodesASCII.txt`
(the names of the regions), and `countryInfo.txt` (the countries). The sizes, from memory **[to
measure]**: `cities15000` about 25,000 places, `cities5000` about 50,000, `cities1000` about 150,000,
`cities500` about 200,000; the largest is a few tens of megabytes as text and a few megabytes compressed.

**Proposal: bundle `cities1000`**, not a downloadable pack. The plan of M1 (§6, item 12) said "a
downloadable pack"; D-048 said "bundled". Bundled wins on the measurements to come, if they show what I
expect: 150,000 places is about the size of one RAW file of a modern camera, in an application that already
ships Qt. Bundling means **no network at all, no consent
prompt (D-061), no pack manager, no update mechanism to build and secure**. A bigger or finer database is
the optional online plugin of D-048, unchanged.

**How it reaches the application.** A read-only SQLite file, `places.sqlite`, **built by a script from a
pinned GeoNames snapshot** (`tools/build-places.py`, which checks the download against a recorded SHA-256
like `tools/fetch-samples.sh`) and **shipped beside the binary** in the packages (`share/auroraw/` on
Linux, next to the executable elsewhere). It is not in git (tens of megabytes of data are not source). It
holds:

```text
places(id, name, ascii_name, lat, lon, country_code, admin1_code, population)   -- plus an R*Tree of (lat, lon)
regions(country_code, admin1_code, name)
countries(code, name)
meta(key, value)        -- the snapshot's date, its source, its licence text, a format version
```

A build without the file still works: **the feature reports "place names are not installed"** and the
rest of the application is unaffected. Tests use a small fixture database written in the test, so no
download is involved (testing strategy §1).

**Updates.** The snapshot is refreshed by the maintainers at each release, by re-running the script; its
date is shown. The application never fetches it. A place that changed name since is the photographer's to
edit.

**Attribution** (CC BY 4.0 asks for credit, a link to the licence and a note of changes): the About
window, the manual's page on metadata, and the `meta` table of the file itself, which travels with it. The
shipped file is data, not code: the GPL-3.0-or-later of the application is not its licence, and the notice
of the packages says so.

## 3. The lookup [proposed]

**The nearest populated place** to the position, within a radius, by the R*Tree (a bounding box, then the
great-circle distance of the few candidates). A lookup is microseconds; 10,000 photos take well under a
second, and the positions come from the catalogue, so no file is read.

- **The radius.** 25 km in this first version **[open]**: beyond it the photo gets **no place**, and is
  counted as such. The photographer hiking between two villages has a position that belongs to no town.
- **What is written.** The nearest place's name as the **city**; the **region** from `regions` through the
  place's country and first-level code; the **country** name and its **ISO code** from `countries`.
  Sublocation is never touched (it is where a person says "the market", "Chez Marie").
- **The limit, said plainly.** This is *nearest place*, not *containing polygon*. Near a border, or on a
  coast, the nearest town can be in the next region or country. A polygon lookup (the administrative
  boundaries of Natural Earth, public domain, a few megabytes simplified) fixes it for the country and the
  region, and is a later refinement of the same function; the radius and the nearest-place rule stay the
  fallback. Saying "near" in the interface is not an option (the IPTC fields say "city"), so the manual
  says it instead, and nothing is ever overwritten that a person wrote.

**Language.** GeoNames' `name` is the place's own name in UTF-8 (`Montréal`); its region names are ASCII
(`Quebec`); its country names are English. A French-speaking photographer may want `Québec`. The first
version writes **GeoNames' names as they are**; naming places in the interface's language needs GeoNames'
alternate names for the languages Auroraw ships (English and French at first, hundreds of thousands of rows
otherwise) and is a **[open]** refinement with its own size to measure, made only if Patrick wants it.

## 4. What gets written, and when [proposed]

**Only what is empty.** A field a person typed, or another application wrote, is **never overwritten**.
The job fills `city`, `region`, `country` and `country_code` where they are empty, and leaves `sublocation`
alone.

**A record of what was written**, so that a correction of the position can refresh the names without
trampling anyone. The sidecar gets one additive property, `aur:PlaceFilled`: the position used (rounded to
the metre), the place's identifier, and the four values written. The rule on a refresh is the three-way
rule of D-134, on a smaller scale: a field is replaced only if it **still equals what Auroraw wrote**;
a field the photographer has edited since is theirs and stays. A position that moved (a GPX match, an
overlay) makes the photo a candidate for a refresh; the refresh is offered, never silent. The property is
additive and optional (no schema bump, a fixture, unknown content preserved), as D-145 does for the altitude
reference.

**When.**

1. **On request**: `Tools ▸ Find place names…` on the selected photos or a whole source, as a cancellable
   job with progress and one undo step (the machinery of D-127, the threshold of 200 included), and a
   report: *filled, already had a place, no position, too far from any place*.
2. **At import**, as an import-profile option, **off by default** **[open]**: the profile already does the
   GPX matching and the metadata template; this is one more optional step that fills the fields after
   the position is known.
3. **Never on its own** for existing photos, and never on a scan: the fields are the photographer's.

**Privacy.** The names are location metadata. D-046 sends everything except the location and the camera
serial by default, and the XMP export of D-024 is an explicit act; the recipes of M4 will treat the city,
region and country as *location* with the GPS (the setting "No location" drops them), as D-046 means. The
note records that so that M4 does not have to rediscover it.

## 5. The place filter [proposed]

The catalogue is derived from the sidecars and can be rebuilt, so it may keep what speeds a query:

- **Three columns on the photo row**, `country`, `region`, `city` (the effective metadata's own values,
  typed, filled or written by another application, all the same), and an index. The schema goes from 5 to
  6 by the usual migration; a rebuild fills them. `country_code` is kept too, for a stable grouping.
- **`Filter.place`** (`PlaceFilter { country, region, city }`, each optional, from the most general down):
  it joins the conditions of D-098 in the one composed query. A node of the tree selects everything under
  it.
- **A facet query** for the interface: the tree *Country ▸ Region ▸ City* with the number of photos at each
  node, for the photos in view, by one `GROUP BY`. Grouping is on the text **folded for case and
  diacritics** (`Montreal` and `Montréal` are one node, shown in the most common spelling), because the
  fields come from several hands.
- **The interface** is a place menu beside the flag and series filters of the library's toolbar, or a
  section of the filter panel when WP11 builds it; the choice is Bob's. It is the same facet that WP11's
  smart collections will save.

## 6. Slices [proposed]

Each a pull request, with its tests:

1. **Measure and build the database**: `tools/build-places.py`, the file, its size and lookup time on the
   three platforms, the fixture; a `places` crate (`auroraw-places`: `open`, `nearest`, the pure
   arithmetic), with its row in the `ALLOWED` table. This settles the **[to measure]** of §2 and the
   radius of §3.
2. **The job**: `Command::FindPlaceNames`, the report, the sidecar's `aur:PlaceFilled`, undo, the CLI, the
   refresh rule.
3. **The filter**: schema 6, `Filter.place`, the facet query, the engine's commands.
4. **The interface** (Bob): the Tools command and its dialog, the import-profile option, the place menu,
   the attribution in About, the manual.
5. **Packaging**: the file in the packages and the release checklist's line for it (WP12).

Tests to write with them: the lookup against known cities (Montréal, Sydney, Reykjavík, a point in the
ocean, a pole, the antimeridian), a border photo that shows the nearest-place limit, a field a person typed
that survives, a refresh that replaces only what Auroraw wrote, a 10,000-photo run as one undoable step,
and the filter and the facet on a generated catalogue.

## 7. What I need decided

1. **Bundle `cities1000`** in every package (proposed), or a smaller base with a download for the rest?
   Decided by slice 1's measurement; the answer only matters to you if the size surprises.
2. **Names in which language?** GeoNames' own (proposed for the first version), or the interface's
   language (more data, later)?
3. **At import**: an option off by default (proposed), or on when the profile has a GPX track?
4. **Nearest place for now, polygons later** (proposed), knowing the border limit of §3.
5. **The feature is worth its place in M1** (it is on WP10's list), against the release checklist's
   remaining items (WP11, WP12).
