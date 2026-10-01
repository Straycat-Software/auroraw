#!/usr/bin/env bash
# Downloads the public data the file of places is built from (design note 008 §2) into testdata/places,
# verifying each file against its recorded SHA-256, then builds testdata/places/places.sqlite:
#
#   cities1000.zip   GeoNames, CC BY 4.0 (https://www.geonames.org/), the towns above 1,000 inhabitants
#   ne_10m_admin_0_countries.geojson, ne_10m_admin_1_states_provinces.geojson
#                    Natural Earth 5.1.2, public domain (https://www.naturalearthdata.com/), the boundaries
#
# Natural Earth is pinned by its tag. GeoNames publishes one dump that it replaces every day and keeps no
# old ones: the checksum below is of the snapshot of 2026-09-30, so a later download will not match. To
# take a newer snapshot, run `UPDATE_PLACES_CHECKSUM=1 tools/fetch-places.sh`, which prints the new line to
# put here; the change is a reviewed act (the release notes say which snapshot a release carries).
#
# Works on Linux, macOS and on Windows through Git Bash. About 65 MB of downloads, a 24 MB file out.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p testdata/places

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

fetch() { # <url> <local name> <sha256>
  local local="testdata/places/$2"
  if [ -s "$local" ]; then
    [ "$(sha256 "$local")" = "$3" ] && { echo "have $2"; return; }
    echo "$2: checksum mismatch, refetching" >&2
    rm -f "$local"
  fi
  echo "fetching $2"
  curl -fsSL --retry 3 -o "$local" "$1"
  local got
  got="$(sha256 "$local")"
  if [ "$got" != "$3" ]; then
    if [ -n "${UPDATE_PLACES_CHECKSUM:-}" ]; then
      echo "fetch  $1  $2  $got"
      return
    fi
    echo "$2: checksum mismatch after download (got $got, expected $3)" >&2
    rm -f "$local"
    exit 1
  fi
}

NE=https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/geojson
fetch "https://download.geonames.org/export/dump/cities1000.zip" cities1000.zip 8cdadfcde41c82b9cc302bbaf8ec30132ef4fe2bb27ab0d6de5e5a4b701a15b5
fetch "$NE/ne_10m_admin_0_countries.geojson"         ne_10m_admin_0_countries.geojson         239eec57ac17f100a11e2536cffc56752c318b50ae765b0918ff7aab4ce8f255
fetch "$NE/ne_10m_admin_1_states_provinces.geojson"  ne_10m_admin_1_states_provinces.geojson  22d0e3ad85eb3e27f17cabf8ba2d50e554fbc27a87796ff891d958185da62fb5

if [ ! -s testdata/places/cities1000.txt ] || [ testdata/places/cities1000.zip -nt testdata/places/cities1000.txt ]; then
  if command -v unzip >/dev/null 2>&1; then
    unzip -o -q testdata/places/cities1000.zip -d testdata/places
  else
    python3 -m zipfile -e testdata/places/cities1000.zip testdata/places
  fi
fi

rm -f testdata/places/places.sqlite
cargo run --release --locked -p auroraw-places --example build-places -- testdata/places testdata/places/places.sqlite
echo "built testdata/places/places.sqlite"
