# Design note 008: place names, offline, and the place filter

> **Status: approved by Patrick on 2026-09-30 (D-147)**, after his two comments (polygons from the start,
> and the record of what Auroraw wrote follows the person's edits) and his answers to §7. WP10's last two items ([M1
> plan](../m1-plan.md) §5, §6 item 12; specification §5.7 "Location", open question 29; D-048). It
> answers question 29 of the specification (§10): the size of the database, the attribution its licences
> require, how it is updated, and the levels of detail. Items are tagged
> **[decided]**, **[proposed]** or **[open]**; what was marked **[to measure]** or **[to verify]** was what I
> remembered of the data sets, not a measurement, and the first slice settled it. **Slices 1 and 2 are
> built (#49, #52): §8 says what was measured and where the build departs from §2 to §4** (the city rule,
> where a person's write is caught, what the record holds, the order country then region); a paragraph of
> those sections that is no longer true carries a pointer to it. **The engine's side of slice 3 (the filter and
> the tree) is built (#62): §5.1.**

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

### 5.1 As built: slice 3, the engine's side

The catalogue is at **schema 6**, and the tree and the filter are in the `catalogue` crate (`place.rs`); the engine
only has to write the sidecar, as it does, and the columns follow. What was decided building it:

- **The photo row keeps the four fields** as the sidecar says them (`country`, `region`, `city`, `country_code`) **and
  three keys** (`place_country`, `place_region`, `place_city`) that the filter selects and groups by. A key is the text
  *folded* (`fold_place`): case and diacritics dropped through Unicode decomposition **with the compatibility forms**
  (`icu_normalizer`, already in the build through `idna`, so no new package: a full-width `Ａ` is `A`, a ligature `ﬁ` is
  `fi`, `Ĳ` is `IJ`), `ß` as `ss`, `æ`, `œ`, `ø`, `đ`, `ł`, `ħ` as their plain letters, the Greek final `ς` as `σ` (a capital
  `Σ` lower-cases to `σ` in any place of the word), the invisible characters dropped (a soft hyphen, a zero-width space, a
  direction mark or isolate, the Arabic letter mark, a variation selector, a tag character, a byte-order mark: a copy and
  paste of a web page carries them; they are the "default ignorable" format and selector characters of Unicode, listed
  as ranges in `place.rs` with a test for every character of every range and its neighbours), the spacing accents
  dropped like the marks they stand for (a PDF's text writes `ü` as `u` and `¨`, and the compatibility decomposition
  would make that a space: `Mu¨nchen` is `munchen`; a test finds every such character in Unicode), hyphens, dashes, apostrophes and white space
  made one, so that `Trois-Rivières` and `Trois Rivieres` are one place. (Issue #84, Django's review of #62: these five
  ways of writing a place gave two nodes; `Muenchen`/`München` and `St-Jean`/`Saint-Jean` still do, since they are
  transliteration and abbreviation and not folding. It raised the version of the keys to 2.) **A country's key is
  its ISO 3166-1 alpha-2 code, found from the photo's own fields and from nothing else**: its code field if that says a
  country (`CA`, or `CAN`, the alpha-3 code the IPTC standard also allows), else its name looked up in a **fixed table of
  the names countries go by**, else the code field as written, else the folded name. `Canada` next to `CA` (Auroraw's own
  fill), `Canada` alone (an application that gives no code), `CAN` and `DE` in the country field are therefore one country
  each, and `Germany`, `Allemagne` and `Deutschland` next to `DE` are one. (The first version of this note let a photo
  without a code be a node of its own, to keep a key from depending on the other photos; the review of #62 showed it is
  the commonest case, a library that mixes Auroraw's fill with another application's meeting it at once, in the place a
  photographer clicks first. A table is a function of the photo's own fields, so a rebuild gives the same keys in any
  order.) The table, `crates/catalogue/src/country_names.tsv`, is made by `cargo run -p auroraw-places --example
  country-names` from Natural Earth (public domain; so nothing is added to the credits) in English and French, which
  Auroraw ships, and in German, Spanish, Italian, Portuguese and Dutch, because the people whose tools write the country
  in the language of their system (`Deutschland`, `España`) are exactly those whose libraries mix with Auroraw's fill
  (the second review of #62), with the names people still write (`UK`, `Ivory Coast`, `Holland`); it is 22 kB, committed
  and regenerated by hand. A name it does not have (`Tyskland`: a language it does not carry) stays a node of its own, as
  before. **The table can join nodes that are one country and must not join two**: a name two countries answer to is not
  in it (`Saint-Martin`, which the French and the Dutch half of the island share in every language), and neither is a
  name that Natural Earth gives to one country and people use for two. That is `Congo`, which is Brazzaville to the data
  and very often Kinshasa in a photographer's metadata: the example keeps a short list of such names (`SHARED_IN_USE`),
  so a bare `Congo` is a node of its own, while `Republic of the Congo`, `Congo-Brazzaville`, `DR Congo`,
  `Congo-Kinshasa` and the codes are joined correctly. The likeliest others (`Guinea`, `Samoa`, `Georgia`, `Ireland`, the
  Virgin Islands, the two Koreas, `Macedonia`) were looked at and are unambiguous. Another language is one more entry in
  the example and a rerun (and a new `PLACE_KEYS_VERSION` once catalogues exist with the old one).
- **The keys are stored, so they have a version.** `PLACE_KEYS_VERSION` covers the folding and the table; the catalogue
  records the version its columns were made with (`meta.place_keys`), and when it is opened by a program with another it
  asks for the columns to be filled again, as for a file made before schema 6. A test that holds a fixed list of keys
  fails when a correction changes one without the version being raised.
- **Regions and cities are keyed by their folded name only**: the sidecar has no region code, so `Colombie-Britannique`
  (a fill in French) and `British Columbia` (one in English) are two regions of Canada. Finding the place names again
  in another language (the refresh of §4) brings them together; nothing else does, and a better key is not in sight.
- **A key is opaque.** Whoever shows a place gives the key back and never makes one. `Filter.place` is a
  `PlaceFilter { country, region, city }` of keys, each optional, from the most general down; the empty key of a region
  or a city means "none".
- **The tree** is `Catalogue::place_facets(&Filter)`: country, region, city, with the number of photos at each node, of
  **the photos that pass every filter but the place** (so that choosing Québec still shows Ontario). A node carries its
  label, its count, the filter that selects exactly its photos, and its children. A photo with a country and a city but
  **no region** is a city directly under its country, whose filter has the empty region key (so that it is told apart
  from the same city in a region). A photo with a **region or a city and no country** (a person emptied the country, which
  stays empty, §4: a settled answer, no longer a gap the next run fills; or another application never wrote one) is under
  **one more node, after the countries**: Patrick, on issue #60, "Let's go with a no country node". It is
  `PlaceFacets::no_country` and `"noCountry"` in the JSON, `null` when no photo in view needs it, a Node as the others
  with regions and cities under it, and an **empty label** (the menu says "(no country)" in the interface's language, and
  puts it after the countries). Its filter is `{ "country": "" }`: the empty country key, like the empty key of a region
  or a city, means "none", and here "none, with a region or a city" (the photos with nothing at all are in no node and
  not in `placed`). `placed` counts the photos of the node, so that the button is enabled by them alone.
- **The label of a node** is the spelling most photos have; on a tie, the best written: mixed case before capitals or lower
  case, then the most accents kept, then the smallest text. It is computed from the photos in view, so a node whose
  only photos in view are written in capitals shows capitals. A country's **code** labels it only when no photo in view
  names it (three photos with the code alone do not label the node `CA` over two that say `Canada`). The nodes are sorted
  by their folded label, made once for each node.
- **The contract of the library's menu is the catalogue's**: `PlaceFacets::to_json()` is the text the menu reads
  (`{ "placed": N, "pending": bool, "countries": [ { "label", "count", "filter", "children": [..] } ], "noCountry": Node | null }`),
  `PlaceFilter::from_json` and `to_json` are the text it gives back, so the interface wires three calls and writes no JSON
  of its own. **`pending`** is true while the place columns are being filled (the first open after the upgrade, or after
  a change of the keys): the tree is then a part of the places, or none, `placed: 0` does not mean that no photo has a
  place, and the menu says it is reading them instead of saying there is nothing (the review of #62).
- **An index that holds the tree**: the tree is read from a partial covering index (`photo_place`, only the photos that
  have a country: the keys, the four texts, the flag and the rating), so that the common case does not touch the table.
  The node of the photos with no country has a second, smaller one (`photo_place_nocountry`, `WHERE place_country IS NULL
  AND (place_region IS NOT NULL OR place_city IS NOT NULL)`), which the tree's query and the filter's read because they
  state its condition. **It holds `place_country` as its last column**, always NULL there, because the query reads the
  column and an index without it is not covering (SQLite then reads the table for each photo; Django's plan test of #84
  found the first form did). A test holds both indexes as **covering** for the usual filters with `EXPLAIN QUERY PLAN`,
  on the very queries the tree runs (`countries_sql`, `no_country_sql`), so that a column added to them that the index
  lacks fails on any machine. Schema 6 carries both, the migration makes both, and a catalogue made with the first form
  has the second one made again when its keys are refilled (version 2).
  Measured on 100,000 photos, 46,000 of them placed (2,300 of them under "(no country)"), 3,655 nodes in the tree
  (the node of the photos with no country holds the places of every country), in release: **20 ms** for the tree
  without a filter and 14 ms with a rating filter (it was 17 and 12 ms before the node, and 120 and 41 ms with an index
  on the keys alone, a table row looked up for each photo), 37 ms with the JSON (28 before: more nodes to write); the
  first page of a region's photos 0.5 ms and of the biggest country's 6 ms. A
  filter on keywords, labels, series or a collection is not in the index and costs a lookup a photo. It runs on the
  caller's thread: a frame and a bit at 100,000 photos, nothing at 20,000.
- **A catalogue made before schema 6** has the columns empty. A rebuild fills them; and so does one pass over the
  sidecars that the engine does when it opens the catalogue (`meta.place_columns = 'stale'` asks for it, until it is
  done): the coordinator walks the photos 200 at a time between the commands of the person, reading each sidecar when it
  writes the columns so that a photo edited meanwhile is never overwritten with what it said before, and sends
  `Event::PlaceColumnsFilled { photos, failed }` once, for the interface to read the tree again. A photo whose sidecar
  cannot be read is **counted in `failed` and not retried**: leaving the marker for it would read every sidecar of the
  library again at each open, and its columns are made when its sidecar is next read or written.

What is left of slice 3 is the interface's: wiring the three calls into the grid, and telling the menu when the places of
the photos in view change without the photos changing (D-151, the review of #58).

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
| A lookup | **115 µs** on average over 10,000 random positions in release (89 µs before the review below, which added a country scan for the points in the sea); a cache keeps the large polygons decoded. |
| Names | **No region and no country lacks a French name** (4,589 of 4,589; 258 of 258). 13 countries have no ISO code (disputed areas, dependencies): they get a name and no code. |
| What no polygon holds | 2,955 towns of 171,091 (1.7 %) are in no region and 42 in no country; they are found by their country code where they can be. Seven features of the regions layer have no name (marine pieces) and are left out. |
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

**The country is decided first, and the towns no region contains are found** (review of the crate, by Bob, who tried it
on a synthetic file; both are now tests, and the second a test on the real file). The first version looked for the
region first, with the 5 km coastal tolerance, and took the country from it; a point inside a country with no regions
of its own, within 5 km of a neighbour's region, was given the *neighbour's* country and region. Now the country that
contains the point is found first, the region is looked for in that country only (containment, then the tolerance), and
the tolerance finds a country only for a point in none (a coast, a ferry, an island the polygons do not show). The
builder asks the same question for a town, so a town and a point at the same spot get the same country and region. And
the towns that **no region contains** (the 1.7 %, mostly harbours, beaches and islands the 1:10 million polygons leave
out) were invisible to a photo that has a region, because the town had to be of the same *region*: the candidates are now
the towns of the region **and the towns of the same country that have none**, never a town of another region. On the real
file Tsim Sha Tsui used to read Mong Kok, a district, and reads Victoria, which GeoNames makes Hong Kong's town; nine more
towns have no region than before (the ones a neighbour's polygon had claimed).

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
  the new place has none; a field that is a person's is never touched and is not in the record; **a field a
  person emptied is an answer, not a gap** (§4): the record keeps it with an empty text, and the run leaves it
  empty, a refresh included, until a person writes it. (The first version of slice 2 filled an emptied field
  again, which contradicted §4's own text; Bob's review found it.) Emptying a field that was empty says
  nothing and leaves an answer already given; undoing the emptying gives the name back to Auroraw; another
  application emptying it, in an accepted external change, is an answer too.
- **`aur:PlaceFilled`** holds the position and the four values (no identifiers), and, for a field a person
  emptied, that field with an empty text: four states in all (absent, written, emptied, and not listed
  because it is theirs). A record that holds only answers has no position. Additive: no schema bump, two
  fixtures, round trips, and a test that what a newer version adds inside it is kept.
- **A person's write takes the field out of the record**, the same words included (§4). It is the engine's
  one function `place_names::released`, called by the two paths that write a place field: `SetMetadataField`
  (the panel, a paste and a batch all go through it) and the accepted external change. It is not in `format`
  as §4 first said: `Metadata`'s fields are public and have no setters, and undo and redo set the same
  fields and must *not* release (the change carries the record's before and after, and they restore it). A
  version cannot override a place field (`OverrideField` has no such variant), so that path does not exist
  yet. The belt of §4 is built: a field in the record that no longer says what was recorded is treated as
  the person's.
- **The report** counts what *landed*: photos, filled, already had their places (including a photo that
  something changed between the lookup and the write, so that the coordinator's own pass found nothing to
  write), no position, in no country, failed (could not be read, or written: a photo that left, a workspace
  that cannot be written).
- **The preview** (`Command::PreviewPlaceNames`, Patrick's wish for §4's "with the before and after shown"):
  the same worker, the same lookups and the same rule on the same reads, and nothing sent to be written, so no
  step. It reports the counts a run would report and the changes it would make **grouped** by field, old text
  and new text (`City: Westville → Eastburg, 400 photos`, the biggest first, at most 200 groups and three
  example photos each, with the number of groups there are in all), because a refresh replaces hundreds of
  names with a few. A run that follows recomputes, so it can differ from the preview by the photos that
  changed in between.
- **The CLI**: `auroraw-cli place-names <workspace> <catalogue> <places-file> (--source <id> | <photo>…)
  [--language <code>] [--refresh] [--preview]`.
- **Measured**: 10,000 photos in **4.95 s** in release (about 0.5 ms a photo, with the sidecar read and
  rewritten and the catalogue row), one step, undone as one (`cargo test -p auroraw-engine --release --test
  place_names -- --ignored`).

What slices 3 to 5 still have to do is as §6 says.
