// SPDX-License-Identifier: GPL-3.0-or-later
//! Deciding which decoder handles a file: `image` for the standard formats it already
//! understands well (JPEG, PNG, TIFF), `rawler` for everything else (the camera RAW formats and
//! DNG, which is TIFF-based but never has this literal extension).

use std::path::Path;

/// Whether `path`'s extension names a format `image` decodes directly, rather than a camera RAW
/// format `rawler` decodes.
pub(crate) fn is_standard(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    matches!(
        ext.as_deref(),
        Some("jpg" | "jpeg" | "png" | "tif" | "tiff")
    )
}

/// The camera RAW formats `rawler` reads, by extension (lowercase).
const RAW_EXTENSIONS: &[&str] = &[
    "3fr", "ari", "arw", "bay", "cr2", "cr3", "crw", "dcr", "dcs", "dng", "erf", "fff", "iiq",
    "k25", "kdc", "mef", "mos", "mrw", "nef", "nrw", "orf", "pef", "raf", "raw", "rw2", "rwl",
    "sr2", "srf", "srw", "x3f",
];

/// Whether `path` names a still image Auroraw handles: a camera RAW file, DNG, JPEG, PNG or TIFF,
/// judged by its extension (case does not matter). What a source lists is every file in it; this
/// says which of them are photos (sidecars, videos, thumbnails and system files are not).
pub fn is_photo_file(path: &Path) -> bool {
    if is_standard(path) {
        return true;
    }
    is_raw(path)
}

/// Whether `path`'s extension names a camera RAW format (case does not matter), issue #5: a
/// photo's own original file is a RAW file when this is true of it (D-032's own original/companion
/// pairing already prefers the RAW as `original` whenever a pair exists, so this alone tells a
/// photo apart from one that was only ever a JPEG, ignoring what the rule dropped).
pub fn is_raw(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|ext| RAW_EXTENSIONS.contains(&ext.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photos_are_recognised_by_extension_whatever_its_case() {
        for name in [
            "IMG_1.CR2",
            "a.cr3",
            "b.NEF",
            "c.Jpg",
            "d.jpeg",
            "e.TIFF",
            "f.dng",
            "g.RW2",
        ] {
            assert!(is_photo_file(Path::new(name)), "{name}");
        }
        for name in [
            "README.txt",
            "clip.MOV",
            "IMG_1.xmp",
            "IMG_1.THM",
            ".DS_Store",
            "noext",
            "a.cr2.bak",
        ] {
            assert!(!is_photo_file(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn only_camera_raw_extensions_are_raw() {
        for name in ["IMG_1.CR2", "a.cr3", "b.NEF", "c.RW2", "d.dng"] {
            assert!(is_raw(Path::new(name)), "{name}");
        }
        for name in ["e.jpg", "f.JPEG", "g.png", "h.tiff", "noext"] {
            assert!(!is_raw(Path::new(name)), "{name}");
        }
    }
}
