# Design note 008: place names, offline, and the place filter

> **Status: approved by Patrick on 2026-09-30 (D-147)**, after his two comments (polygons from the start,
> and the record of what Auroraw wrote follows the person's edits) and his answers to §7. WP10's last two items ([M1
> plan](../m1-plan.md) §5, §6 item 12; specification §5.7 "Location", open question 29; D-048). It
> answers question 29 of the specification (§10): the size of the database, the attribution its licences
> require, how it is updated, and the levels of detail. Nothing here is built. Items are tagged
> **[decided]**, **[proposed]** or **[open]**; what is marked **[to measure]** or **[to verify]** is what I
> remember of the data sets, not a measurement, and the first slice settles it before anything depends on
> it.

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

**Two public data sets, both bundled**, because they answer two different questions:

- **Which country and which region is this point in?** An *area* question, answered by **polygons**:
  **Natural Earth** (public domain, no attribution required, credited anyway), the country layer
  (`admin-0`) and the first-level subdivisions layer (`admin-1`: states, provinces, regions), at the
  1:10 million scale **[to verify: the coverage of admin-1, and whether a finer scale is worth its size]**.
  Each polygon carries its names in several languages (`NAME_EN`, `NAME_FR`… **[to verify how complete the
  French names of the regions are]**), its ISO codes and the GeoNames identifier of the area.
- **Which town is nearest?** A *point* question, answered by **GeoNames** (CC BY 4.0): `cities1000`, every
  populated place above 1,000 inhabitants, about 150,000 **[to measure]**, with its name in the place's
  own spelling (`Montréal`), its position, its country and its first-level code.

**An earlier version of this note proposed the nearest town alone for all three.** The reason was to keep
the first slice small, and it did not weigh what a polygon gives: the *right* country and region at a
border, on a coast and far from any town (a hike 40 km from the nearest village is still in Quebec), names
of the region and the country **in the interface's language** (Natural Earth carries them; GeoNames' region
names are ASCII English), and a town that is constrained to the region the point is in, which removes the
worst of the border errors. What it costs is the second data set (a few megabytes **[to measure]**), a
point-in-polygon routine (holes, multipolygons, the antimeridian), and a choice of boundaries (below). Patrick
asked for it from the start, and I agree.

**How it reaches the application.** One read-only SQLite file, `places.sqlite`, **built by a script from
pinned snapshots of both sets** (`tools/build-places.py`, which checks each download against a recorded
SHA-256 like `tools/fetch-samples.sh`) and **shipped beside the binary** in the packages. It is not in git
(data is not source). It holds:

```text
areas(id, level, code, parent, name_en, name_fr, ..., geonames_id)   -- level: country, region
area_boxes   -- an R*Tree of each area's bounding boxes (one row per polygon part)
area_shapes(area_id, part, rings)   -- the rings as integer micro-degrees, delta-coded: a compact blob
places(id, name, lat, lon, country_code, admin1_code, population)   -- plus an R*Tree of (lat, lon)
meta(key, value)   -- each source's date, name, licence text and worldview, a format version
```

A build without the file still works: **the feature reports "place names are not installed"** and the rest
of the application is unaffected. Tests use a small hand-made fixture (a few squares, holes and a
meridian-crossing shape, a dozen towns), so no download is involved (testing strategy §1).

**Which boundaries.** Natural Earth draws the borders as they stand on the ground, and names disputed
areas as it has decided to; other "worldviews" exist. Auroraw takes **no political position**: it ships one
data set as it is, says which in the About window and in the file's `meta`, and never overwrites a value a
person wrote, so a photographer who disagrees corrects the field. **[open: confirm the default
worldview]**.

**Updates.** The snapshots are refreshed by the maintainers at each release, by re-running the script;
their dates are shown. The application never fetches them.

**Attribution**: GeoNames asks for credit and a link to CC BY 4.0 (the About window, the manual's page on
metadata, the file's `meta`); Natural Earth asks for none and is named there too. The shipped file is data,
not code: the GPL-3.0-or-later is not its licence, and the packages' notice says so.

## 3. The lookup [proposed]

1. **Country and region: the polygon that contains the point.** The R*Tree gives the parts whose box holds
   the point; a crossing-number test (holes included, a part at a time) decides. The region's polygon
   knows its country; a point in a country without a region layer (or in an enclave's hole) gets the country
   alone. Microseconds per photo.
2. **A point that no polygon contains** is on a coast, a beach, a ferry or the open sea, and 1:10m
   shorelines are generalised, so the point may be a few hundred metres "into the water" of a place it is
   plainly at. A **coastal tolerance**: the nearest polygon within **5 km** counts **[open]**; beyond it,
   there is no country (the open sea has none, and saying so is the honest answer).
3. **The city: the nearest populated place in the same region**, within **25 km** **[open]**. "In the same
   region" is what removes the border error: a point just inside Quebec does not get a town of Ontario. A
   point in no region (or with no mapped region) falls back to the same country. Beyond the radius the city
   stays empty and the country and region are still filled: a photo 40 km from a village has a region
   and no city, which is true.
4. **The names.** The country and the region **in the interface's language**, from Natural Earth's name for
   it (English when the language is missing there); the city **in the place's own spelling**, as GeoNames
   has it. Naming *cities* in the interface's language needs GeoNames' alternate names for the languages
   Auroraw ships (hundreds of thousands of rows) and stays **[open]**, made only if Patrick wants it.
   Country code: the ISO 3166-1 alpha-2 code of the country.
5. **Sublocation is never touched**: it is where a person says "the market", "Chez Marie".

**The limits, said plainly.** The city is *the nearest town*, not the municipality that contains the point
(no global offline set of municipal boundaries exists, and the ones that do are gigabytes); the manual says
so. The polygons are generalised: near a border the answer can be wrong by a kilometre or two **[to
verify]**, and in a country that has disputed areas it is the data set's answer. A photo inside a hole or
an enclave (Lesotho, the Vatican) is tested. Nothing is ever written over what a person wrote.

## 4. What gets written, and who owns it [proposed]

**Only what is empty.** A field a person typed, or another application wrote, is **never overwritten**.
The job fills `city`, `region`, `country` and `country_code` where they are empty, and leaves `sublocation`
alone.

**A record of what Auroraw wrote**, in the sidecar, as one additive, optional property, `aur:PlaceFilled`:
the **position** used (rounded to the metre), the **area and town identifiers**, and **each of the four
values**, field by field. It exists so that a corrected position can refresh the names without trampling
anyone (no schema bump, a fixture, unknown content preserved, as D-145 does for the altitude reference).

**The record follows the person's edits** (Patrick's second comment). A field listed in `aur:PlaceFilled`
means "Auroraw wrote this and nobody has touched it since". So:

- **Any write to one of the four fields that is not the place job's takes that field out of the record
  at once**, *whatever it writes*: a new value, **the same value** (typing "Montréal" over "Montréal" is
  a person confirming it, and it is theirs), an **empty** one (clearing a city is an answer, not a gap to
  refill). It covers every path that writes the field: the metadata panel, a batch edit, a paste of
  metadata (D-128), the acceptance of an external change (D-134), a version's override. It is done where
  the field is set, in `format` (`Metadata`'s setter for those fields), not in each caller, so that a new
  path cannot forget it, and a test goes through each path.
- The fill is **one undoable step that holds the record with the values**: undoing it restores the fields
  and the record as they were, and so does undoing a person's edit (the change carries both). Redo
  likewise.
- **The refresh** (the position moved: a GPX match, an overlay, a hand correction) looks at the photo's
  record: for each field **still in it**, the value is replaced by the new place's; a field that is not in
  it is left alone. As a belt to these braces, a field still in the record but **no longer equal to what
  was recorded** (an edit from outside this application that no path here saw, such as a sidecar changed by
  hand) is treated as the person's and dropped from the record too. The refresh is **offered, never
  silent**, with the before and after shown, as one undoable step.
- `country` and `country_code` are two fields and two entries, so a person can change the country's
  name and leave the code; consistency between them is theirs.

**When.**

1. **On request**: `Tools ▸ Find place names…` on the selected photos or a whole source, as a cancellable
   job with progress and one undo step (the machinery of D-127, the threshold of 200 included), and a
   report: *filled, already had a place, no position, in open water*.
2. **At import**, as an import-profile option, **off by default** **[open]**.
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

### 5.1 As built: slice 3, the engine's side

The catalogue is at **schema 6**, and the tree and the filter are in the `catalogue` crate (`place.rs`); the engine
only has to write the sidecar, as it does, and the columns follow. What was decided building it:

- **The photo row keeps the four fields** as the sidecar says them (`country`, `region`, `city`, `country_code`) **and
  three keys** (`place_country`, `place_region`, `place_city`) that the filter selects and groups by. A key is the text
  *folded* (`fold_place`): case and diacritics dropped through Unicode decomposition (`icu_normalizer`, already in the
  build through `idna`, so no new package), `ß` as `ss`, `æ`, `œ`, `ø`, `đ`, `ł`, `ħ` as their plain letters, hyphens, dashes,
  apostrophes and white space made one, so that `Trois-Rivières` and `Trois Rivieres` are one place. **A country's key is
  its ISO code when the photo has one** (upper-case), else its folded name: `Germany`, `Allemagne` and `Deutschland` next
  to `DE`, written by three tools, are one country. A photo of the same country written by a tool that gave no code is a
  node of its own, by name, and the tree then shows two of them; the alternative (matching the names of photos that
  have a code) would make a key depend on the order the photos are read, which a rebuild must not.
- **A key is opaque.** Whoever shows a place gives the key back and never makes one. `Filter.place` is a
  `PlaceFilter { country, region, city }` of keys, each optional, from the most general down; the empty key of a region
  or a city means "none".
- **The tree** is `Catalogue::place_facets(&Filter)`: country, region, city, with the number of photos at each node, of
  **the photos that pass every filter but the place** (so that choosing Québec still shows Ontario). A node carries its
  label, its count, the filter that selects exactly its photos, and its children. A photo with a country and a city but
  **no region** is a city directly under its country, whose filter has the empty region key (so that it is told apart
  from the same city in a region). A photo with **no country is in no node**, and the filter cannot select it; so it is not
  counted in `placed`, the number of photos in view that have a country. (With a field a person emptied staying empty,
  §4, such photos are a settled state and no longer a gap the next run fills; a "(no country)" node would reach them,
  and is not built.)
- **The label of a node** is the spelling most photos have; on a tie, the best written: mixed case before capitals or lower
  case, then the most accents kept, then the smallest text. It is computed from the photos in view, so a node whose
  only photos in view are written in capitals shows capitals.
- **The contract of the library's menu is the catalogue's**: `PlaceFacets::to_json()` is the text the menu reads
  (`{ "placed": N, "countries": [ { "label", "count", "filter", "children": [..] } ] }`), `PlaceFilter::from_json` and
  `to_json` are the text it gives back, so the interface wires three calls and writes no JSON of its own.
- **An index that holds the tree**: the tree is read from a partial covering index (`photo_place`, only the photos that
  have a country: the keys, the four texts, the flag and the rating), so that the common case does not touch the table.
  Measured on 100,000 photos, 46,000 of them placed, 2,400 nodes in the tree, in release: **19 ms** for the tree without
  a filter and 12 ms with a rating filter (120 ms and 41 ms with an index on the keys alone, a table row looked up for
  each photo), 28 ms with the JSON; the first page of a region's photos 0.5 ms and of the biggest country's 6 ms. A
  filter on keywords, labels, series or a collection is not in the index and costs a lookup a photo. It runs on the
  caller's thread: a frame and a bit at 100,000 photos, nothing at 20,000.
- **A catalogue made before schema 6** has the columns empty. A rebuild fills them; and so does one pass over the
  sidecars that the engine does when it opens the catalogue (`meta.place_columns = 'stale'` asks for it, until it is
  done): the coordinator walks the photos 200 at a time between the commands of the person, reading each sidecar when it
  writes the columns so that a photo edited meanwhile is never overwritten with what it said before, and sends
  `Event::PlaceColumnsFilled` once, for the interface to read the tree again.

What is left of slice 3 is the interface's: wiring the three calls into the grid, and telling the menu when the places of
the photos in view change without the photos changing (D-151, the review of #58).

## 6. Slices [proposed]

Each a pull request, with its tests:

1. **Build and measure the pack**: `tools/build-places.py` for both sets, the file, its size and the lookup
   time on the three platforms, the fixture; a `places` crate (`auroraw-places`: `open`, `locate`, the
   point-in-polygon arithmetic and the nearest-town rule), with its row in the `ALLOWED` table. This
   settles every **[to measure]** and **[to verify]** of §2 and the radii of §3.
2. **The job and the record**: `Command::FindPlaceNames`, the report, `aur:PlaceFilled` and the setter that
   releases a field when a person writes it, undo, the CLI, the refresh.
3. **The filter**: schema 6, `Filter.place`, the facet query, the engine's commands.
4. **The interface** (Bob): the Tools command and its dialog, the import-profile option, the place menu, the
   attribution in About, the manual.
5. **Packaging**: the file in the packages and the release checklist's line for it (WP12).

Tests to write with them: the lookup against known points (Montréal, Sydney, Reykjavík, a beach, a ferry
in open water, a pole, both sides of the antimeridian), **both sides of a real border and a point in an
enclave**, a point 40 km from any town, a field a person typed that survives, **a field a person rewrote
(to a new value, to the same value, to nothing) that leaves the record through each writing path and is
never refreshed**, undo and redo of a fill and of such an edit, a refresh that replaces only what Auroraw
wrote, a 10,000-photo run as one undoable step, and the filter and the facet on a generated catalogue.

## 7. What Patrick decided (2026-09-30)

1. **Bundle both sets** in every package, at the size slice 1 measures; if the total surprises (more than a
   few tens of megabytes), a smaller scale of polygons or a base with a download for the rest: **as proposed**.
2. **Names in which language?** Country and region in the interface's language from Natural Earth, the city
   in its own spelling; cities in the interface's language later, only if wanted: **as proposed**.
3. **At import**: an import-profile option, **off by default**.
4. **The coastal tolerance (5 km) and the town radius (25 km)** as starting values: **as proposed**.
5. **The boundaries**: Natural Earth's default worldview, with the choice said in About: **as proposed**.
6. **The feature is worth its place in M1**, against the release checklist's remaining items (WP11, WP12):
   **yes**.
