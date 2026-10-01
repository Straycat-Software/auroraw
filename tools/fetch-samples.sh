#!/usr/bin/env bash
# Downloads the CC0 RAW samples (raw.pixls.us) used by the tests and benchmarks into
# testdata/samples, verifying each against its recorded checksum (testing strategy §5).
# Works on Linux, macOS and on Windows through Git Bash. About 490 MB in total (the first eight files about 225 MB; the
# nine of the second round, which fill the gaps of the first, about 266 MB: a float DNG, a 103 MP file, a monochrome
# sensor, a rotated GRBG file, two Sony files whose EXIF rotation `rawler` does not report, and three files `rawler`
# refuses; design note 005 §3.3b). The first eight are fetched by their place in the site's tree (`/data/`); the nine by
# the site's own address for the file (`/getfile.php/<id>/nice/<name>`), which is how they were chosen and checked.
# A local name never holds the site's "(4:3)": a colon is not part of a file name on Windows (it starts an alternate data
# stream), so the local name says "(4x3)".
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p testdata/samples
base=https://raw.pixls.us/data

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

fetch() { # <path on the server, or a whole URL> <local name> <sha256>
  case "$2" in *:*) echo "$2: a local name cannot hold a colon (on Windows it starts an alternate data stream)" >&2; exit 1 ;; esac
  local local="testdata/samples/$2"
  local url="$base/$1"
  case "$1" in http://*|https://*) url="$1" ;; esac
  if [ -s "$local" ]; then
    [ "$(sha256 "$local")" = "$3" ] && { echo "have $2"; return; }
    echo "$2: checksum mismatch, refetching" >&2
    rm -f "$local"
  fi
  echo "fetching $2"
  curl -fsSL --retry 3 -o "$local" "$url"   # -L: the server redirects to /download/
  local got
  got="$(sha256 "$local")"
  if [ "$got" != "$3" ]; then
    echo "$2: checksum mismatch after download (got $got, expected $3)" >&2
    rm -f "$local"
    exit 1
  fi
}

fetch "Canon/EOS%205D%20Mark%20IV/B13A0732.CR2"           B13A0732.CR2                        25d928f9e9a65525ac796bdff13e93b7b36b627568d7a17338e12dc44424078a
fetch "Canon/Canon%20EOS%20R5m2/CRAW.CR3"                 canon-r5m2-CRAW.CR3                 b3c08a5c7a97299dfd58896a12b939e7b43ec78682761812524825e6e5d087be
fetch "Nikon/D850/Nikon-D850-14bit-compressed.NEF"        Nikon-D850-14bit-compressed.NEF     e54b5d4f0d5c721a90309741c60754e687750f1b2d366ac2587d870eef121126
fetch "Sony/ILCE-7RM4/DSC00396.ARW"                       DSC00396.ARW                        3421b919ee81224d59f78872ee099a021bff0e2a47ae5fbbac385b182ac7241e
fetch "FUJIFILM/X-T50/DSCF0120.RAF"                       DSCF0120.RAF                        3a8a9ac6d92274969aaa1f55ea669191b98bb8f588d00ec8ec17afd8b3963dea
fetch "Panasonic/DC-S5/dc-s5_6k4k.RW2"                    dc-s5_6k4k.RW2                      350f4ad0839383b3e7bcb0feb2dda827fc81e97ab59917df2a1d824bb979234d
fetch "OLYMPUS/E-M5%20Mark%20III/PB290154.ORF"            PB290154.ORF                        24de6ac3bd5e668ee22fd0dbf2e344608949eb85f7342f28470ce709c9d51226
fetch "Leica/M9/L1049390.DNG"                             L1049390.DNG                        d1d162fce62210b8951c862189a6098329b5163b7f7492dc397c6e53bba04239

# The second round (design note 005 §3.3b), fetched by the site's own address for each file. Each checksum is the one the
# site lists and that of the file as it was downloaded and read with `rawler` 0.8.0.
site=https://raw.pixls.us/getfile.php
fetch "$site/883/nice/Canon%20-%20EOS%205D%20Mark%20III%20-%2032bit%2032bit%20RAW.dng"                          Canon_-_EOS_5D_Mark_III_-_32bit_32bit_RAW.dng                         94993fefdac13a68a40f0c4f47479256ff53790d8af8ad01ecf6b78d7d2b553f
fetch "$site/2824/nice/Eyedeas%20-%20E1%20-%2016bit%20%284%3A3%29.DNG"                                           "Eyedeas_-_E1_-_16bit_(4x3).DNG"                                      203b51bcdeb2fd9258699f6de438817e8a52c3fdf3db5a6a2435c86769fd444e
fetch "$site/7773/nice/Fujifilm%20-%20GFX100S%20II%20-%2016bit%20compressed%20%284%3A3%29.RAF"                  "Fujifilm_-_GFX100S_II_-_16bit_compressed_(4x3).RAF"                  63527912c729242e7fd7b325c915de5d80ca19d1d33a72271a397a6cb1d2c400
fetch "$site/2558/nice/GoPro%20-%20HERO6%20Black%20-%2016bit%20%284%3A3%29.GPR"                                  "GoPro_-_HERO6_Black_-_16bit_(4x3).GPR"                               17a24d42735464525773048172c888ea24d13ba4b907e64580b63c3bab987ae5
fetch "$site/974/nice/Leica%20-%20M%20Monochrom%20-%2016bit%20%283%3A2%29.DNG"                                   "Leica_-_M_Monochrom_-_16bit_(3x2).DNG"                               149d5b3fbd5cd40d51c1f56d5e392a81870d129e047a6c8a596447590e7a0e4f
fetch "$site/1027/nice/PARROT%20-%20Bebop%20Drone%20-%2016bit%20%284%3A3%29.dng"                                 "PARROT_-_Bebop_Drone_-_16bit_(4x3).dng"                              aeee3ad6fb8ba3219258af980c5ba508848c323e88c043aebe9486c1e1c603b4
fetch "$site/3839/nice/Samsung%20-%20SM-G973U%20-%2016bit%2016bit%20%282.1132075471698%29.dng"                  "Samsung_-_SM-G973U_-_16bit_16bit_(2.1132075471698).dng"              6492b7e0ac8e012524c0c83606a104cc9bf3b68f30fa60c7b73ffbe4dc584ab1
fetch "$site/2349/nice/Sony%20-%20DSLR-A450%20-%2012bit%2012bit%20compressed%20%283%3A2%29.ARW"                 "Sony_-_DSLR-A450_-_12bit_12bit_compressed_(3x2).ARW"                 7c2205a14fbfb05aa22e2e290750ea8894229a52a106a67516b4e797759e37ad
fetch "$site/4418/nice/Sony%20-%20SLT-A58%20-%2012bit%2012bit%20compressed%20%283%3A2%29.ARW"                   "Sony_-_SLT-A58_-_12bit_12bit_compressed_(3x2).ARW"                   debc7085446d1710eef0abf372b9e651c00249f856d2863f9e08c10b3d4ac426
du -sh testdata/samples
