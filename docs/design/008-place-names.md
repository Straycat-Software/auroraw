# Design note 008: place names, offline, and the place filter

> **Status: approved by Patrick on 2026-09-30 (D-147)**, after his two comments (polygons from the start,
> and the record of what Auroraw wrote follows the person's edits) and his answers to §7. WP10's last two items ([M1
> plan](../m1-plan.md) §5, §6 item 12; specification §5.7 "Location", open question 29; D-048). It
> answers question 29 of the specification (§10): the size of the database, the attribution its licences
> require, how it is updated, and the levels of detail. Items are tagged
> **[decided]**, **[proposed]** or **[open]**; what was marked **[to measure]** or **[to verify]** was what I
> remembered of the data sets, not a measurement, and the first slice settled it. **Slices 1 and 2 are
> built (#49, #52): §8 says what was measured and where the build departs from §2 to §4** (the city rule,
> where a person's write is caught, what the record holds); a paragraph of those sections that is no longer
> true carries a pointer to it.

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
  1:10 million scale (settled in §8.1: 4,589 regions, and no finer scale is needed for the size it would cost).
  Each polygon carries its names in several languages (`NAME_EN`, `NAME_FR`… (complete: §8.1)), its ISO codes and the GeoNames identifier of the area.
- **Which town is nearest?** A *point* question, answered by **GeoNames** (CC BY 4.0): `cities1000`, every
  populated place above 1,000 inhabitants, 171,091 of them in the snapshot of 2026-09-30, with its name in the place's
  own spelling (`Montréal`), its position, its country and its first-level code.

**An earlier version of this note proposed the nearest town alone for all three.** The reason was to keep
the first slice small, and it did not weigh what a polygon gives: the *right* country and region at a
border, on a coast and far from any town (a hike 40 km from the nearest village is still in Quebec), names
of the region and the country **in the interface's language** (Natural Earth carries them; GeoNames' region
names are ASCII English), and a town that is constrained to the region the point is in, which removes the
worst of the border errors. What it costs is the second data set (measured: §8.1), a
point-in-polygon routine (holes, multipolygons, the antimeridian), and a choice of boundaries (below). Patrick
asked for it from the start, and I agree.

**How it reaches the application.** One read-only SQLite file, `places.sqlite`, **built from
pinned snapshots of both sets** (`tools/fetch-places.sh` downloads them, checking each against a recorded
SHA-256 like `tools/fetch-samples.sh`, and runs the builder, which is Rust: §8.1) and **shipped beside the
binary** in the packages. It is not in git
(data is not source). It holds:

```text
areas(id, level, code, country_code, parent, name, geonames_id)   -- level: country, region
area_names(area, lang, name)         -- one row per language an area has a name in
parts(id, area, level, rings)        -- one polygon part: its rings as integer micro-degrees, delta-coded varints
part_boxes   -- an R*Tree (id, min/max lon, min/max lat) of each part's bounding box
places(id, name, lat, lon, country_code, country_area, region_area, population, section)
                                     -- indexed by (region_area, lat) and (country_area, lat)
meta(key, value)   -- each source's date, name, licence text and worldview, the layout version (FORMAT)
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
   plainly at. A **coastal tolerance**: the nearest polygon within **5 km** counts (a starting value, approved); beyond it,
   there is no country (the open sea has none, and saying so is the honest answer).
3. **The city: a populated place of the same region**, within **25 km** (a starting value, approved). "In
   the same region" is what removes the border error: a point just inside Quebec does not get a town of
   Ontario. A point in no region (or with no mapped region) falls back to the same country. Beyond the radius
   the city stays empty and the country and region are still filled: a photo 40 km from a village has a
   region and no city, which is true. **Which place of the region is not simply the nearest** (the real data
   showed it: downtown Montréal is nearer to the borough of Ville-Marie than to "Montréal"): see the reach
   rule in §8.1.
4. **The names.** The country and the region **in the interface's language**, from Natural Earth's name for
   it (English when the language is missing there); the city **in the place's own spelling**, as GeoNames
   has it. Naming *cities* in the interface's language needs GeoNames' alternate names for the languages
   Auroraw ships (hundreds of thousands of rows) and stays **[open]**, made only if Patrick wants it.
   Country code: the ISO 3166-1 alpha-2 code of the country.
5. **Sublocation is never touched**: it is where a person says "the market", "Chez Marie".

**The limits, said plainly.** The city is *a town point*, not the municipality that contains the point
(no global offline set of municipal boundaries exists, and the ones that do are gigabytes); the manual says
so. The polygons are generalised: near a border the answer can be wrong by a kilometre or two (§8.1 measured the
borders it could), and in a country that has disputed areas it is the data set's answer. A photo inside a hole or
an enclave (Lesotho, the Vatican) is tested. Nothing is ever written over what a person wrote.

## 4. What gets written, and who owns it [proposed]

**Only what is empty.** A field a person typed, or another application wrote, is **never overwritten**.
The job fills `city`, `region`, `country` and `country_code` where they are empty, and leaves `sublocation`
alone.

**A record of what Auroraw wrote**, in the sidecar, as one additive, optional property, `aur:PlaceFilled`:
the **position** used (rounded to the metre) and **each of the four values**, field by field. (An earlier
text also listed the area and town identifiers; nothing reads them and a new file may renumber them, so
they are not kept: §8.2.) It exists so that a corrected position can refresh the names without trampling
anyone (no schema bump, a fixture, unknown content preserved, as D-145 does for the altitude reference).

**The record follows the person's edits** (Patrick's second comment). A field listed in `aur:PlaceFilled`
means "Auroraw wrote this and nobody has touched it since". So:

- **Any write to one of the four fields that is not the place job's takes that field out of the record
  at once**, *whatever it writes*: a new value, **the same value** (typing "Montréal" over "Montréal" is
  a person confirming it, and it is theirs), an **empty** one (clearing a city is an answer, not a gap to
  refill). It covers every path that writes the field: the metadata panel, a batch edit, a paste of
  metadata (D-128), the acceptance of an external change (D-134), a version's override. It is meant to be done
  in one place, so that a new path cannot forget it, and a test goes through each path. (As built it is in
  the engine, not in `format`, because `Metadata`'s fields are public and undo must not release: §8.2.)
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

## 6. Slices [proposed]

Each a pull request, with its tests:

1. **Build and measure the pack** (done, #49): the builder and the script that fetches both sets, the file,
   its size and the lookup time, the fixture; a `places` crate (`auroraw-places`: `open`, `locate`, the
   point-in-polygon arithmetic and the town rule), with its row in the `ALLOWED` table. This settled every
   **[to measure]** and **[to verify]** of §2 and the radii of §3 (§8.1); the lookup time is measured on Linux only so far.
2. **The job and the record** (done, #52): `Command::FindPlaceNames`, the report, `aur:PlaceFilled` and the
   release of a field when a person writes it, undo, the CLI, the refresh (§8.2).
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

## 8. As built (slices 1 and 2)

### 8.1 Slice 1: the pack and the lookup (#49)

**The builder is Rust, not a Python script**, so the encoding of the polygons is written once, in the crate
that reads it, and tested there. `PackBuilder` writes the file; `sources::{countries, regions, places}` parse
Natural Earth 5.1.2 (GeoJSON, `admin_0_countries` and `admin_1_states_provinces`) and GeoNames' `cities1000`;
`examples/build-places.rs` drives them; `tools/fetch-places.sh` downloads the three files with recorded
SHA-256 and calls it, into `testdata/places/` (git-ignored). GeoNames replaces its dump daily and keeps none,
so the checksum of that file is of the day it was recorded and the script says how to re-record it.

**What the real data measured** (the snapshot of 2026-09-30):

| | |
| --- | --- |
| The file | **24.1 MiB**, built in **3.6 s** in release (about 16 MiB deflated). 4,847 areas (258 countries, 4,589 regions), 12,660 polygon parts, 1.84 million points, 171,091 towns. The polygons are 8 MiB and the towns 8 MiB. |
| A lookup | **87 µs** on average over 10,000 random positions in release; a cache keeps the large polygons decoded. |
| Names | **No region and no country lacks a French name** (4,589 of 4,589; 258 of 258). 13 countries have no ISO code (disputed areas, dependencies): they get a name and no code. |
| What no polygon holds | 2,946 towns of 171,091 (1.7 %) are in no region and 42 in no country; they are found by their country code where they can be. Seven features of the regions layer have no name (marine pieces) and are left out. |
| Real borders | Windsor (Ontario) and Detroit (Michigan), 2 km apart across a river, give Canada / Ontario / Windsor and the United States / Michigan / Detroit: the same-region rule works on a real border. Both sides of the antimeridian (Chukotka, Taveuni) work; mid-Atlantic and the North Pole are empty; the South Pole is Antarctica. |

The size is a few tens of megabytes at most, so decision 1 of §7 ("bundle both sets") holds without the
smaller scale. `countryInfo.txt` and `admin1CodesASCII.txt`, once listed, are not needed: Natural Earth carries
the names and the polygons decide the region.

**The city is chosen by reach, not by distance alone.** The nearest town is often a borough or a
neighbourhood: downtown Montréal is 400 m from the borough of Ville-Marie and 1.7 km from "Montréal";
Copacabana is a neighbourhood of Rio de Janeiro, and GeoNames has a record for each. The rule as built, among
the towns of the point's region within 25 km: a town **claims** the points within **10 m for each square root
of its inhabitants** (13 km for Montréal, 26 km for Rio, 1.2 km for a town of 15,000, 350 m for a village of
1,200: roughly the radius of a city of that size); a point claimed by several goes to the **biggest** of them;
a point nobody claims goes to the **nearest**. GeoNames' own "section of a populated place" records (`PPLX`,
9,484 of them) are left out whenever a real town is within reach. The three numbers (5 km, 25 km, 10 m) are
options of `Places` with these defaults. It is a refinement of the "nearest populated place" of §3, not a
change of the radii Patrick approved.

**Limits that remain.** The city is a *town point*, not the municipality that contains the position
(Westmount, an independent city inside Montréal, reads as Montréal); the polygons are the 1:10 million
generalisation.

### 8.2 Slice 2: the job and the record (#52)

- **`Command::FindPlaceNames { scope, pack, language, refresh }`**, scope the selection or one source. The
  position is the photographer's correction when there is one, else the original's, read from the sidecars
  (the catalogue's `gps_lat` and `gps_lon` are not filled). A background job paced by the coordinator's
  acknowledgement like a large batch, so a cancel lands within about one photo; **always a job**, even for one
  photo, rather than synchronous under 200 (one path to test; the interface may show no progress for a tiny
  run). The run is **one undoable step** ("place names"); a cancelled run keeps what it found as its own
  step. A pack that cannot be opened is refused at once and nothing starts.
- **The rule** is one pure function (`place_names::fill_place`): an empty field is filled; a field that is
  Auroraw's (in the record and still saying what it wrote) follows the position only on `refresh`, and goes if
  the new place has none; a field that is a person's is never touched and is not in the record. A cleared
  field is empty, so the next run fills it again.
- **`aur:PlaceFilled`** holds the position and the four values (no identifiers). Additive: no schema bump, a
  fixture, a round trip, and a test that what a newer version adds inside it is kept.
- **A person's write takes the field out of the record**, the same words included (§4). It is the engine's
  one function `place_names::released`, called by the two paths that write a place field: `SetMetadataField`
  (the panel, a paste and a batch all go through it) and the accepted external change. It is not in `format`
  as §4 first said: `Metadata`'s fields are public and have no setters, and undo and redo set the same
  fields and must *not* release (the change carries the record's before and after, and they restore it). A
  version cannot override a place field (`OverrideField` has no such variant), so that path does not exist
  yet. The belt of §4 is built: a field in the record that no longer says what was recorded is treated as
  the person's.
- **The report** counts what *landed*: photos, filled, already had their places, no position, in no country,
  failed (a photo that left, or a workspace that cannot be written).
- **The CLI**: `auroraw-cli place-names <workspace> <catalogue> <places-file> (--source <id> | <photo>…)
  [--language <code>] [--refresh]`.
- **Measured**: 10,000 photos in **4.95 s** in release (about 0.5 ms a photo, with the sidecar read and
  rewritten and the catalogue row), one step, undone as one (`cargo test -p auroraw-engine --release --test
  place_names -- --ignored`).

What slices 3 to 5 still have to do is as §6 says.
