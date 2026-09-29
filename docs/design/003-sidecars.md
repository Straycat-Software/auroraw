# Design note 003: the photo sidecar and the version sidecar

> **Status: adopted (D-087).** Third design note of work package WP1
> ([M1 plan](../m1-plan.md) §5 and §6, items 4 and 8). It answers questions 4 and 26 of the
> specification (§10): what is written in a **photo sidecar** and a **version sidecar**, in which
> XMP properties, how keywords are referred to, how the version's copy of the photo's metadata
> stays consistent (D-027), and which metadata fields M1 supports. It builds on
> [note 001](001-workspace-layout.md) (where the files are and how they are written) and
> [note 002](002-state-files.md) (the state files). The content fingerprint and the relinking
> of files are note 004; the development chain and history are M2's. Items are **[decided]**
> since D-087.

## 1. The question

Two kinds of file carry the truth about a photo: the **photo sidecar** (the original's metadata,
D-023, and a cache of what was read from the original, D-074) and the **version sidecars** (a copy of
the photo's metadata, the version's own metadata, and the development, D-023 and D-027). The
specification fixes the principle: photo sidecars are standard XMP, readable by other software;
version sidecars are XMP if practical (§5.8). What exactly goes in them, and how, so that the
catalogue can be rebuilt without the originals, other software can read what it should, and nothing
we do not understand is lost?

## 2. Requirements

| # | Requirement | From |
| --- | --- | --- |
| 1 | The photo sidecar is **standard XMP**, and a foreign tool reading it finds rating, label, title, caption, keywords (flat and hierarchical), creator, rights and location in the places it expects. | Spec §5.7, §5.8 |
| 2 | It **caches the original's capture data** (time, camera, lens, exposure, size, GPS) so a rebuild never reopens the originals. | D-074 |
| 3 | **EXIF corrections are overlays**: the original value stays visible and can be restored. | Spec §5.7 |
| 4 | **Keywords have a stable identity** in the vocabulary; renaming or moving one updates every photo that uses it, **without rewriting their sidecars**. | D-045 |
| 5 | A version sidecar is **self-contained**: it copies the photo's metadata and adds its own, and records **which fields it overrides**; when the photo changes, the copies **follow**, except for the overridden fields. | D-023, D-027, spec §5.5 |
| 6 | **Effective value** = the version's if it defines one, otherwise the photo's; keywords are the **union**; a version can only **add** keywords. | Spec §5.5, D-014 |
| 7 | **Existing XMP** files from other software are read (never modified) and their ratings and keywords are copied in; hierarchies written by other software are understood. | Spec §5.2, §5.7 |
| 8 | A file made by a **newer** Auroraw, or edited by another tool, keeps everything that is not understood. | Architecture §5.5 |
| 9 | A **rebuild** of 100,000 photos and their versions reads them all: parsing must be fast, in the order of tens of thousands of files a second on all cores. | D-026, spike 3 |
| 10 | The **write** of a single change stays one small file, and the version copies must not make an action in Cull mode slow. | Spec §9 |
| 11 | Privacy: the file paths of the original and private data stay in the workspace and are **not** part of what other software or an export receives. | D-046, D-047 |

## 3. The options for the file format

**A. XMP written and read by Auroraw's own code** on a general-purpose XML parser (`quick-xml`),
with a small model of the properties we know and an opaque copy of the rest. Full control
over canonical writing and preservation, no C++ build. The work is ours.

**B. The Adobe XMP Toolkit** (through the `xmp-toolkit` crate). The reference implementation, but
a large C++ library to build on three platforms and to package, for a use that is small and
well-defined. Rejected for the build and packaging cost, which is also why spike 4 used a Rust
decoder rather than LibRaw.

**C. Exiv2 or ExifTool** as a library or a subprocess. Exiv2 is GPL C++; ExifTool is Perl and a
subprocess per file. Rejected for speed and packaging (requirement 9); **ExifTool is used in the
tests** to check that what we write is read correctly.

**D. A JSON sidecar with an XMP export.** Simpler for us, and contrary to requirement 1 and to the
specification's promise of a standard XMP.

**Proposal: A.** Auroraw writes a **strict, canonical subset** of XMP and reads the **general
form**, since other software writes it in several equivalent ways (properties as attributes,
several `rdf:Description` blocks, `rdf:parseType="Resource"`).

## 4. The photo sidecar

### 4.1 Form [proposed]

- One `x:xmpmeta` with one `rdf:RDF` and **one `rdf:Description`**, inside the usual `<?xpacket?>`
  wrapper, UTF-8, LF, two-space indentation.
- **Every property as an element** (never as an attribute), in a **documented fixed order**, arrays
  as `rdf:Seq`, `rdf:Bag` or `rdf:Alt` as the XMP specification says. The same content gives the
  same bytes (as in note 002), so an unchanged sidecar is not rewritten.
- **Unknown properties** (other namespaces, or Auroraw properties from a newer version) are kept as
  opaque, namespace-correct subtrees and written after the known ones, in their original order.
  A second `rdf:Description` from another tool is merged into the model on reading and remembered as
  such, never lost.
- Times are ISO 8601, **with the UTC offset when it is known**; timestamps written by Auroraw are UTC.

### 4.2 The namespace [proposed]

Auroraw's own properties live in the namespace `https://auroraw.org/ns/1.0/`, prefix `aur`. The
address does not have to answer, but the project owns the domain (D-083) and a page describing the
schema can be published there. The version of this part of the file is `aur:Schema`, an integer
(note 002 rules apply: a newer schema is not modified).

### 4.3 What the photo sidecar contains [proposed]

| What | XMP property | Notes |
| --- | --- | --- |
| **Identity** | `aur:PhotoId` | 32 hexadecimal characters (note 001). |
| **Schema** | `aur:Schema` | Integer. |
| **Rating** | `xmp:Rating` | Stars 0 to 5, always the stars. A rejected photo keeps its stars (see the flag). |
| **Flag** | `aur:Flag` | `picked`, `rejected`, or absent. |
| **Colour label** | `xmp:Label` | The label's name, as other software writes it. |
| **Title** | `dc:title` | Language alternative, `x-default`. |
| **Caption** | `dc:description` | Language alternative. |
| **Keywords, flat** | `dc:subject` | The **leaf** keywords assigned, by name: **a snapshot** (§6). |
| **Keywords, hierarchy** | `lr:hierarchicalSubject` | Full paths with `\|` (`Place\|Canada\|Quebec`), the form Lightroom, digiKam-compatible tools and others read: **a snapshot** (§6). |
| **Keywords, identity** | `aur:KeywordIds` | A bag of the assigned keywords' identifiers (§6). |
| **Other metadata** | The list of §7 | Creator, rights, credit, location, and so on. |
| **The original's data**, as read | `exif:DateTimeOriginal` (an ISO 8601 text that carries the sub-seconds and the UTC offset when they are known), `tiff:Make`, `tiff:Model`, `aux:SerialNumber`, `aux:Lens`, `exif:ExposureTime`, `exif:FNumber`, `exif:ISOSpeedRatings`, `exif:FocalLength`, `exif:FocalLengthIn35mmFilm`, `exif:PixelXDimension` and `exif:PixelYDimension`, `tiff:Orientation`, `exif:GPSLatitude`, `exif:GPSLongitude`, `exif:GPSAltitude` | The standard properties, **holding the original's values**, as XMP sidecars conventionally do (D-074). |
| **EXIF overlays** | `aur:Overlay` | A structure with the fields corrected: `CaptureTime`, `Gps` (latitude, longitude, altitude), `Camera`, `Lens`, `Orientation`. The effective value is the overlay's if present, otherwise the original's. |
| **The files of the photo** | `aur:Files` | A sequence, one entry per file: `Role` (`original` or `companion`, the JPEG of a RAW+JPEG pair, D-032), `Name`, `Format`, `Size`, `Fingerprint`, `Hash` (when known), and `Locations` (source identifier, path relative to the source, last seen). The definition of `Fingerprint` and `Hash`, and the relinking, are note 004. |
| **The main version** | `aur:MainVersion` | The identifier of the version the grid shows (D-063). |
| **Imported** | `aur:Imported` | UTC time of the first add. |

Roughly 3 KB for a photo with a dozen keywords. **The versions of a photo are not listed in its
sidecar**: they are the files `versions/<xx>/<photo id>.*.xmp` (adding a version does not rewrite the
photo sidecar, and a version created on another machine is found by the scan).

### 4.4 A sketch [proposed]

```xml
<?xpacket begin="&#xFEFF;" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description
        xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/"
        xmlns:lr="http://ns.adobe.com/lightroom/1.0/" xmlns:exif="http://ns.adobe.com/exif/1.0/"
        xmlns:aur="https://auroraw.org/ns/1.0/">
      <aur:Schema>1</aur:Schema>
      <aur:PhotoId>3f2a91c0d77e4b5a8c1e0f9d2b6a4c31</aur:PhotoId>
      <xmp:Rating>4</xmp:Rating>
      <aur:Flag>picked</aur:Flag>
      <dc:subject><rdf:Bag><rdf:li>Heron</rdf:li></rdf:Bag></dc:subject>
      <lr:hierarchicalSubject><rdf:Bag><rdf:li>Fauna|Birds|Heron</rdf:li></rdf:Bag></lr:hierarchicalSubject>
      <aur:KeywordIds><rdf:Bag><rdf:li>91c4e07a2b5d6f18</rdf:li></rdf:Bag></aur:KeywordIds>
      <exif:DateTimeOriginal>2026-05-14T06:41:09.250</exif:DateTimeOriginal>
      <aur:Files><rdf:Seq><rdf:li rdf:parseType="Resource">
        <aur:Role>original</aur:Role><aur:Name>DSC00396.ARW</aur:Name>
        <aur:Size>61865984</aur:Size><aur:Fingerprint>sampled-v1:…</aur:Fingerprint>
        <aur:Hash>blake3:…</aur:Hash>
        <aur:Locations><rdf:Seq><rdf:li rdf:parseType="Resource">
          <aur:Source>b83e1f52a09c47d6</aur:Source><aur:Path>2026/2026-05-14/DSC00396.ARW</aur:Path>
        </rdf:li></rdf:Seq></aur:Locations>
      </rdf:li></rdf:Seq></aur:Files>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
```

(The sketch is abridged; the full property order is in the schema page written with WP1.)

### 4.5 Rating, flags and the other software's convention [proposed]

Other software often encodes "rejected" as `xmp:Rating = -1`. Auroraw keeps **stars and flag apart**
(a rejected three-star photo keeps its three stars, spec §5.3), so the workspace sidecar carries the
stars in `xmp:Rating` and the flag in `aur:Flag`. The mapping is made at the **boundary**:

- **Reading XMP that has Auroraw's own properties** (`aur:Flag`, `aur:Stars`): they win. The flag
  and the stars are taken from them, whatever `xmp:Rating` says.
- **Reading foreign XMP** (no `aur:Flag`): `xmp:Rating = -1` becomes flag *rejected* with no stars;
  other values are stars.
- **The XMP export to the source folders** (D-024), where other software is the reader, offers to
  write `-1` for rejected photos, since that is what they understand. **The stars are not lost in
  that file**: an exported rejected photo carries `xmp:Rating = -1` for the other software **and**
  `aur:Flag = rejected` and `aur:Stars = <the stars>` for Auroraw. Re-reading its own export
  (which it recognises, D-047) gives back exactly the same flag and stars, and another tool
  ignores the two `aur:` properties. Without the `-1` option, `xmp:Rating` holds the stars and
  `aur:Flag` the flag, as in the workspace, and the round trip is exact too.

## 5. The version sidecar

### 5.1 What it contains [proposed]

| What | XMP property | Notes |
| --- | --- | --- |
| Identity | `aur:VersionId`, `aur:PhotoId`, `aur:Schema` | |
| Name, creation time | `aur:Name`, `aur:Created` | |
| **The overridden fields** | `aur:Overrides` | A bag of the field names the version defines itself: `rating`, `flag`, `label`, `title`, `caption` (spec §5.5). Keywords are additive and are not in this list. |
| **The keywords it adds** | `aur:AddedKeywordIds` | Identifiers, as in the photo (a version can only add, D-014). |
| **The effective metadata** | The same properties as the photo sidecar (§4.3): rating, flag, label, title, caption, keywords (**the union**), the other fields of §7, the original's data and overlays | A **copy**, so the file is self-contained (D-023) and other software reading it sees the version's own metadata. For an overridden field it holds the version's value; for the others, the photo's. |
| **The provenance of the copy** | `aur:CopiedFrom` | A digest of the photo's metadata at the time of the copy (§5.3). |
| **The development** | `aur:Pipeline…` | Empty in M1: the chain, the history and the snapshots are written in M2 (§5.4). |

### 5.2 Which is the truth [proposed]

There are two copies of the same information, so the rule is written down: **the photo sidecar plus
the version's own overrides and additions are the truth; the copy in a version sidecar is derived.**
The effective value of a version is computed from them, and the copy is refreshed to match. If they
disagree (a crash, an edit by another tool), the truth wins and the copy is rewritten, never the other
way round. This is what lets D-027 ("the copies follow") be true without making the copies a second
source of truth.

### 5.3 How the copy follows, and what an action costs [proposed]

The digest in `aur:CopiedFrom` is a **BLAKE3 hash of the canonical form of the fields that are copied**
(the properties of §4.3 other than the identity, the files and the locations, with the keywords
counted **by identifier**, not by name: §6). The write path of
architecture §5.3 becomes:

1. Write the **photo sidecar** (atomic), then the database transaction. **This is all an action waits for.**
2. The version copies are refreshed **afterwards, in the background**, in one batch, for each version
   whose copy would change: the fields changed and not overridden by that version.
3. At start-up, and after a crash, the **reconcile** compares each version's `aur:CopiedFrom` with the
   photo's current digest and refreshes the ones that differ.

Rating a photo in Cull mode therefore writes one small file, whatever the number of its versions. A
batch edit of 10,000 photos with about 1.4 versions each rewrites 10,000 photo sidecars first
and about 14,000 version sidecars in the background: on NTFS 1 to 2 ms per file as measured on the GitHub runner (note 001 §4.2), so
20 to 45 seconds there, a job with progress that does not block the interface, run on several threads.

### 5.4 The development, reserved for M2 [proposed]

M1 writes no development. M2's note decides how the chain, the history and the snapshots are stored
(spec §10, questions 17 and 20): inside the XMP as a JSON value of `aur:Pipeline`, or in a companion
file, or both. **The room is reserved now**: `aur:PipelineSchema` is the version of that part, and a
companion file is named `<photo id>.<version id>.<suffix>` (for example `.history.json`), which is a
name an M1 reader lists as unknown and never touches (note 001 §5.6). A version with a development a
reader does not understand is opened **read-only for the development** and its metadata still works.

## 6. Keywords: identity, and names as a snapshot [proposed]

- **A keyword is identified by its identifier** in the vocabulary (note 002); `aur:KeywordIds` in a
  sidecar says which keywords a photo has. **Auroraw displays, searches and filters by identifier.**
- **The names in `dc:subject` and `lr:hierarchicalSubject` are a snapshot**: what the names were
  when the sidecar was last written. They are there for three reasons, none of which is the
  everyday running of Auroraw:
  1. **Recovery.** `vocabulary.json` is a single file. If it is lost or damaged, the vocabulary is
     rebuilt from the sidecars: each identifier with its path gives back the entry, **with the same
     identifier**, so nothing else has to change.
  2. **Self-contained sidecars** (D-023). Photos copied to another catalogue carry the paths that
     the destination's vocabulary merges by (D-067).
  3. **Other software** that reads a workspace sidecar directly, which is rare, since the
     workspace is not next to the originals; the XMP export is the normal channel (§8).
- **Renaming, moving or merging a keyword changes the vocabulary file and nothing else.** No
  sidecar is rewritten, no background job runs, no stamp is kept. A rename of a keyword used by
  22,000 photos is one small write, not fifty thousand.
- **The snapshot is refreshed only when it costs nothing**: whenever a sidecar is **written anyway**
  (an edit of that photo, a version copy refresh, an import step), its names are rewritten from the
  current vocabulary; and **always in an export** (§8), which builds names from the vocabulary and
  not from the snapshot. There is also a maintenance command, **"Refresh keyword names in the
  sidecars"**, run on request (and by the backup helper of M4), for someone who wants other software
  to see current names.
- **Consequence to accept**: a tool that reads the workspace sidecars directly can see an old
  keyword name until the sidecar is next written or the command is run. Auroraw itself never does.
- **The digest of the version copies** (§5.3) is computed over the keyword **identifiers**, not
  the names, so renaming a keyword does not make every version copy look out of date.
- **On rebuild**, for each identifier in a sidecar:
  - found in the vocabulary: **the vocabulary's name and path win**, the snapshot is ignored;
  - **not found** (the vocabulary was lost, or the photo comes from another workspace): the entry is
    **created with that identifier and the snapshot's path**, which is the recovery above, and
    reported so that the photographer knows the vocabulary was rebuilt from sidecars;
  - a sidecar **without identifiers** (foreign XMP, or written before the vocabulary existed) is
    resolved **by path**, creating the missing entries with new identifiers.
- **Vocabulary names cannot contain `|`** (the separator of the hierarchical form); the interface
  refuses it.
- **A keyword removed from the vocabulary** (deleted, or merged into another) is removed from the
  photos, or replaced by its merge target, **in one batch job** (this one does need to touch the
  sidecars, since the photos' keyword sets change), and the identifier is never reused. In M1 the
  deletion is one undoable action on the coordinator (D-099); past `BACKGROUND_THRESHOLD` photos it
  runs as a cancellable background job instead (D-127), the vocabulary keeping the branch until every
  carrying photo has actually lost it.
- **Do-not-export keywords** (D-045) are written in the workspace, which is private, and **dropped
  from what leaves it**: exports and the XMP export to the source folders (§8).

## 7. The metadata fields of M1 [proposed]

Closes item 8 of the M1 plan (spec §10, question 26). The IPTC Core fields, a useful subset of the
IPTC Extension, and the XMP basics. **Custom fields are not in M1.** Overridable per version means
the version can hold its own value (spec §5.5); everything else is the photo's.

| Field | XMP property | Overridable |
| --- | --- | --- |
| Title | `dc:title` | Yes |
| Caption | `dc:description` | Yes |
| Rating, colour label | `xmp:Rating`, `xmp:Label` | Yes |
| Flag | `aur:Flag` | Yes |
| Keywords | `dc:subject`, `lr:hierarchicalSubject`, `aur:KeywordIds` | Add only |
| Creator | `dc:creator` | No |
| Copyright | `dc:rights` | No |
| Usage terms, web statement | `xmpRights:UsageTerms`, `xmpRights:WebStatement` | No |
| Credit, source | `photoshop:Credit`, `photoshop:Source` | No |
| Headline, instructions | `photoshop:Headline`, `photoshop:Instructions` | No |
| Sublocation, city, region, country, country code | `Iptc4xmpCore:Location`, `photoshop:City`, `photoshop:State`, `photoshop:Country`, `Iptc4xmpCore:CountryCode` | No |
| Persons shown, event | `Iptc4xmpExt:PersonInImage`, `Iptc4xmpExt:Event` | No |
| Original's capture data, overlays | §4.3 | No |

Location fields hold the text; the coordinates are the GPS of the original and its overlay. Reading
these from a **foreign XMP** (§8) uses the same properties, and also the older forms other software still writes.

## 8. What crosses the boundary [proposed]

The workspace sidecars are **private working files**, readable by other software, not what is handed
over. Three boundaries have their own mapping, made by the code that crosses them (WP7, WP10, M2):

| Boundary | What is done |
| --- | --- |
| **Reading foreign XMP** at import (Lightroom, darktable, digiKam, ExifTool; never modified) | Read: `xmp:Rating` (-1 is rejected), `xmp:Label`, `dc:subject`, `lr:hierarchicalSubject`, `digiKam:TagsList`, the title, caption, creator, rights and IPTC properties above. Ignored (and not copied): develop settings of other software, which are not portable (spec §5.8). Ratings and keywords are copied into the photo sidecar, and keywords are matched to the vocabulary **by path** (case aside, as the vocabulary itself compares; a path it lacks is created). In practice (D-133): the rating is one axis, `-1` for rejected (`xmp:Rating` `-1`, or `aur:Flag` rejected) to `5`, a new photo arriving rejected with no stars; the keywords are the union of `lr:hierarchicalSubject`, `digiKam:TagsList` (whose `/` is the hierarchy) and the `dc:subject` names that are not a level of any path; the file tied to a photo is `photo.ARW.xmp`, else `photo.xmp` when only one photo of the folder has that stem. |
| **XMP export to the source folders** (D-024, D-028) | A **derived** file built from the photo sidecar: the effective EXIF values in the standard properties (overlays applied), `-1` for rejected if chosen, the **do-not-export keywords removed**, and **no `aur:Files`, locations or identifiers of Auroraw** other than the marker that lets Auroraw recognise its own export (D-047) and the few properties that make the round trip exact: `aur:Flag` and, when `-1` is written, `aur:Stars`. |
| **Metadata in exported images** (D-046) | The effective metadata of the exported version according to the recipe: by default everything except the location and the camera's serial number, and never the do-not-export keywords, paths or workspace identifiers. |

### 8.1 When an XMP file already exists at the destination [proposed]

The XMP export writes next to the originals (D-024), where a file of the same name may already
exist: written by Lightroom or darktable, or by an earlier Auroraw export. **The export merges by
default; it never overwrites blindly.**

**Why not simply overwrite.** The existing file often holds the other application's develop settings
(`crs:` and other namespaces), which Auroraw cannot reproduce; replacing the file would destroy that
work, against the spirit of D-018. And the other tool may have changed the rating or the keywords
since Auroraw last looked.

**The behaviour**

1. **Merge.** The existing file is read into the same XMP model as a sidecar (§4.1), which keeps every
   property it does not understand. The export rewrites **only the properties Auroraw owns**: rating,
   label, title, caption, keywords and the IPTC fields of §7 (and `aur:Flag` and `aur:Stars`, §4.5).
   Everything else, including develop settings and unknown namespaces, is written back as it was.
2. **A three-way check first**, as D-047 describes. The **base** is what Auroraw last read from or
   wrote to that file. For each field:
   - unchanged in the file since the base: replaced by Auroraw's value, no question asked;
   - changed in the file **and** unchanged in Auroraw since the base: **the file's value is taken
     into the photo sidecar** (an external change, D-047), and the file already has it;
   - changed on both sides to different values: a **conflict**, listed ("12 files were changed by
     another application") for the photographer to settle: Auroraw's value, the file's value, or
     review one by one.
3. **Keywords** are merged as sets against the base: a keyword the other tool **added** is offered
   (kept and added to the photo, or dropped), one it **removed** is offered the same way; the
   result written is the photo's keyword set after the photographer's choice, so the two sides
   end up equal. Do-not-export keywords never reach the file (§6).
4. **The base is stored locally**, in the catalogue, not in the workspace: the values Auroraw last read
   from or wrote to each such file, with the file's size and modification time. It describes this
   machine's view of files outside the workspace. If it is lost (a rebuild, another machine), the
   export has no base and **falls back to two-way**: every field that differs is treated as a
   conflict and asked about, once.
5. **The options in the export dialog:**
   - **Merge** (default), as above;
   - **Replace**, after a confirmation: the existing file is **moved to the workspace's `removed/`
     folder**, never deleted (architecture §5.3), and a new derived file is written;
   - **Skip files that already exist**, for someone who wants only to add the missing ones.
6. **No feedback loop.** After writing, Auroraw records the new size and modification time as the base,
   so its own write is not reported as an external change (D-047). A file that carries Auroraw's
   marker and whose content matches the base is recognised as its own export.
7. **Which file is meant** follows D-028: `photo.xmp` by default, or `photo.ARW.xmp`, as the photographer
   chose. Two photos that would write the same name (a RAW and a JPEG sharing a stem, both with
   `photo.xmp`) are reported before anything is written, and the pair's RAW takes the name (D-032).
8. **A file that cannot be parsed** is **never overwritten or merged**: it is reported and skipped,
   unless the photographer chooses Replace, which keeps the old one in `removed/`.
9. **Writes are atomic.** The temporary file is created **beside the destination** and renamed, since the
   workspace's `.auroraw/tmp/` (note 001) may be on another volume than the source. A read-only or
   unreachable source is reported per file, without stopping the batch.

## 9. Consequences for WP1

- The `format` crate holds the XMP model, the general **reader**, the strict canonical **writer**, and the
  mapping of §4.3 and §7, with **fixtures** for both sidecars (schema 1), including real XMP files from
  other software.
- **Tests**: round trip; canonical bytes; unknown properties, extra namespaces and second
  `rdf:Description` blocks kept; the attribute form and the element form read the same; a truncated or
  invalid file is an error and is never overwritten; a newer `aur:Schema` is not modified; **ExifTool reads the
  fixtures back correctly** (in CI); the digest of the copied fields is stable across platforms and **does not change when a keyword is
  renamed**; **renaming a keyword writes no sidecar**; a sidecar written for another reason gets current
  names; a **lost vocabulary is rebuilt from the sidecars with the same identifiers**; the maintenance
  command refreshes the names; **an export read back gives the same
  rating, flag, label, title, caption and keywords** (rejected photos with stars included, with and
  without the `-1` option).
- A **parse benchmark** on 100,000 photo and 143,000 version sidecars, to hold requirement 9
  (tens of thousands of files a second on all cores), run on the three platforms.
- **Merge fixtures** for §8.1 (WP10 builds the feature; the model must already support it): real XMP files
  from Lightroom and darktable with develop settings, and a test that **a merge keeps every `crs:`
  property byte for byte** while updating rating, label, title, caption and keywords.
- The schema is written as a short **public page** (in `docs/`, later on auroraw.org) so that other tools
  can read what is in a sidecar.

## 10. Not settled here

| Item | Where |
| --- | --- |
| The hash and the fingerprint in `aur:Files`, relinking, "original changed" | Note 004 |
| The development chain, the history, the snapshots | M2 |
| The interface of the three-way merge and of conflicts (§8.1), and the same merge for external changes noticed outside an export (D-047) | WP10, done for external changes noticed at a scan (D-134); the export's own use of the same merge comes with the export |
| Whether a foreign tool that rewrites a workspace sidecar (it should not) is detected | WP10 |
| The exact property order and the full schema page | WP1 |

## 11. What is asked

Approval of: the choice of writing and reading XMP with our own code (§3); the form of §4.1 and the
namespace (§4.2); the content of the photo sidecar (§4.3) and the treatment of rating and flag (§4.5);
the content of the version sidecar and the rule that **the photo plus the version's overrides is the
truth and the copy is derived** (§5.1 to §5.3); the room reserved for the development (§5.4); the
treatment of keywords (§6), where **names in sidecars are a snapshot** and a rename writes no sidecar; the field list of M1 (§7); and the boundaries (§8), including **the merge into an existing XMP file (§8.1)**, as **D-087**.
The point most worth a second look is §5.3: **version copies are refreshed in the background**, which
is faster than writing them with every action, and slightly relaxes the letter of D-027 ("the copies
follow") into "the copies follow, shortly, and are repaired if a crash interrupts them".
