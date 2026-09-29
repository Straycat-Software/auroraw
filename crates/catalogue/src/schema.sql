-- Schema version 1 (architecture §5.4, design notes 001-004, M1 plan WP2).
--
-- The catalogue is an index, rebuildable from the workspace (D-026): every row here can be
-- recomputed from a photo sidecar, a version sidecar or a state file. The primary keys of
-- entities that have an identity of their own in the workspace (photo, version, keyword,
-- series, collection, source) are their hexadecimal identifiers as text, so a row can be found
-- or replaced without a lookup table. Camera and lens have no identity outside the catalogue:
-- they are strings deduplicated for filtering, and their integer ids are internal only.
--
-- `sidecar_size` and `sidecar_modified` are the file's size and modification time **as last
-- read**, local to this machine (design note 004 §6.2): the reconcile scan (architecture §5.3)
-- compares them with what it finds on disk to decide whether a sidecar needs to be re-read.

-- Catalogue-level facts: `workspace_id` (the workspace this catalogue indexes, so a mismatch
-- offers a rebuild instead of trusting stale data, note 001 §5.3) and `rebuilt_at`.
CREATE TABLE meta(
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE camera(
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE lens(
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE source(
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  name TEXT NOT NULL
);

CREATE TABLE keyword(
  id TEXT PRIMARY KEY,
  parent_id TEXT REFERENCES keyword(id),
  name TEXT NOT NULL,
  path TEXT NOT NULL,
  export INTEGER NOT NULL DEFAULT 1,
  -- Alternative names, `|`-joined (never available inside one: `checked_name` already refuses it),
  -- empty for none. Added by schema 4 (D-045's own synonyms, WP10 slice 2).
  synonyms TEXT NOT NULL DEFAULT ''
);

CREATE TABLE series(
  id TEXT PRIMARY KEY,
  cover_photo_id TEXT,
  kind TEXT NOT NULL,
  resolved INTEGER NOT NULL DEFAULT 0,
  sidecar_size INTEGER NOT NULL,
  sidecar_modified INTEGER
);

CREATE TABLE collection(
  id TEXT PRIMARY KEY,
  parent_id TEXT,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  sidecar_size INTEGER NOT NULL,
  sidecar_modified INTEGER
);

CREATE TABLE collection_member(
  collection_id TEXT NOT NULL REFERENCES collection(id),
  position INTEGER NOT NULL,
  photo_id TEXT NOT NULL,
  version_id TEXT,
  PRIMARY KEY(collection_id, position)
) WITHOUT ROWID;

CREATE TABLE photo(
  id TEXT PRIMARY KEY,
  source_id TEXT,
  path TEXT,
  filename TEXT NOT NULL,
  fingerprint TEXT,
  -- The whole-file hash, known once the file was imported or a background job computes it later
  -- (design note 004 §6.1, §6.2). Absent otherwise: never a substitute for `fingerprint` in a
  -- decision that could lose data (§6.3, items 4 and 5 -- import's job, WP7).
  hash TEXT,
  capture_time INTEGER NOT NULL,
  camera_id INTEGER REFERENCES camera(id),
  lens_id INTEGER REFERENCES lens(id),
  iso INTEGER,
  aperture REAL,
  shutter REAL,
  focal_length REAL,
  width INTEGER,
  height INTEGER,
  rating INTEGER NOT NULL DEFAULT 0,
  flag INTEGER NOT NULL DEFAULT 0,
  label TEXT,
  title TEXT,
  caption TEXT,
  gps_lat REAL,
  gps_lon REAL,
  series_id TEXT REFERENCES series(id),
  main_version_id TEXT,
  version_count INTEGER NOT NULL DEFAULT 0,
  effective_rating INTEGER NOT NULL DEFAULT 0,
  effective_flag INTEGER NOT NULL DEFAULT 0,
  rating_overridden INTEGER NOT NULL DEFAULT 0,
  imported INTEGER,
  sidecar_size INTEGER NOT NULL,
  sidecar_modified INTEGER,
  -- Set by a source reconcile (design note 004 §6.3, §6.5), never by a rebuild (which only has
  -- the workspace, not a fresh scan): "original changed" means the file at `path` no longer
  -- matches `fingerprint`; "original missing" means no file matching this photo was found in its
  -- source at all. Cleared by the next reconcile that confirms or relinks the file.
  original_changed INTEGER NOT NULL DEFAULT 0,
  original_missing INTEGER NOT NULL DEFAULT 0,
  -- The 64-bit perceptual hash of the photo's thumbnail (the bits of a u64), for the similar-photo suggestions
  -- (WP9, D-105). Added by schema 2. A cache of what the thumbnail shows: a rebuild leaves it unset and the
  -- thumbnail workers make it again.
  phash INTEGER
);

-- A reconcile looks a found file up by fingerprint (design note 004 §6.4) and a scan pages
-- through one source's known files; the plain b-tree index built on `photo(id)`'s own primary
-- key does not help either.
CREATE INDEX idx_photo_fingerprint ON photo(fingerprint);
CREATE INDEX idx_photo_source ON photo(source_id);
-- Import's skip decision looks up candidates by fingerprint (above) and, once found, compares
-- hashes; this index is for the rarer reverse lookup (has this exact whole file been seen before,
-- regardless of its fingerprint bucket -- not used by WP7 but cheap to keep in step with the
-- fingerprint index rather than added later against a populated column).
CREATE INDEX idx_photo_hash ON photo(hash);

-- A photo's secondary locations (D-108, WP9, D-036's "one photo, several locations"): `photo`'s own
-- source_id/path stay the primary location, unchanged everywhere else; a location here is confirmed by a
-- whole-file hash match before it is added (a sampled fingerprint alone is not proof enough to invite
-- someone to go delete a file over it).
CREATE TABLE location(
  photo_id TEXT NOT NULL REFERENCES photo(id),
  source_id TEXT NOT NULL,
  path TEXT NOT NULL,
  filename TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  hash TEXT NOT NULL,
  seen INTEGER,
  PRIMARY KEY(photo_id, source_id, path)
) WITHOUT ROWID;
CREATE INDEX location_source ON location(source_id);

-- What Auroraw last read from the XMP file another application keeps next to a photo's original
-- (D-047, WP10): the *base* of the three-way comparison of design note 003 §8.1. One row a photo, keyed by
-- the photo alone (one tracked file each; a different path re-baselines it silently). `path` is inside the
-- photo's primary source, '/' separators. `size` and `modified_ns` (nanoseconds since the Unix epoch, so a
-- sub-second file system does not hide a rewrite) are the file's as listed when it was read; `base` is the
-- JSON of `format::sidecar::external::Fields` read from it. A change noticed later and not yet answered is
-- `pending` (the file's newer Fields) with its own stat, so that the same unanswered file is not read again
-- at every scan. Local to this machine and lost by a rebuild: a photo without a row is baselined silently
-- at the next scan, never reported.
CREATE TABLE external_xmp(
  photo_id TEXT PRIMARY KEY REFERENCES photo(id),
  path TEXT NOT NULL,
  size INTEGER NOT NULL,
  modified_ns INTEGER,
  base TEXT NOT NULL,
  pending TEXT,
  pending_size INTEGER,
  pending_modified_ns INTEGER
);
CREATE INDEX external_pending ON external_xmp(photo_id) WHERE pending IS NOT NULL;

CREATE TABLE version(
  id TEXT PRIMARY KEY,
  photo_id TEXT NOT NULL REFERENCES photo(id),
  name TEXT,
  rating INTEGER,
  flag INTEGER,
  label TEXT,
  created INTEGER,
  sidecar_size INTEGER NOT NULL,
  sidecar_modified INTEGER
);

CREATE TABLE photo_keyword(
  photo_id TEXT NOT NULL REFERENCES photo(id),
  keyword_id TEXT NOT NULL REFERENCES keyword(id),
  PRIMARY KEY(photo_id, keyword_id)
) WITHOUT ROWID;

CREATE TABLE version_keyword(
  version_id TEXT NOT NULL REFERENCES version(id),
  keyword_id TEXT NOT NULL REFERENCES keyword(id),
  PRIMARY KEY(version_id, keyword_id)
) WITHOUT ROWID;

-- Self-contained, not `content=''`: a contentless FTS5 index supports INSERT but not the plain
-- UPDATE/DELETE that keeping it in step with an edited photo needs (its own 'delete' command
-- requires the old column values, which a contentless table cannot give back). The duplicated
-- text costs little next to a photo's row and buys a normal SQL update path (crate::write).
CREATE VIRTUAL TABLE photo_fts USING fts5(
  filename, caption, title, keywords,
  tokenize='unicode61 remove_diacritics 2'
);
