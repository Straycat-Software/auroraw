// SPDX-License-Identifier: GPL-3.0-or-later
//! What one file on a card or in a folder looks like before it is copied: a plain, already
//! collected fact, not something [`crate::pair`] or [`plan`](fn@crate::plan) read from disk themselves
//! (both are pure functions over a `Vec` of these, easy to test without a real source). The
//! caller (`engine`, which already reads a file's metadata for other reasons, `imaging::process`)
//! fills `capture_time` and `camera` in; this crate has no decoder of its own to read them with.

use time::OffsetDateTime;

/// One file a source listed, with what its metadata says about it.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredFile {
    /// Its path inside the source.
    pub path: String,
    /// Its size in bytes.
    pub size: u64,
    /// Its capture time, if the file has metadata and it could be parsed.
    pub capture_time: Option<OffsetDateTime>,
    /// The camera that made it, if known.
    pub camera: Option<String>,
}

/// The file name's stem (without its extension) and its extension (without the dot, as found,
/// case preserved), splitting on the last `/` and the last `.`.
pub(crate) fn stem_and_extension(path: &str) -> (&str, &str) {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, ext),
        _ => (name, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_plain_name() {
        assert_eq!(stem_and_extension("IMG_0042.CR3"), ("IMG_0042", "CR3"));
    }

    #[test]
    fn splits_a_name_inside_a_folder() {
        assert_eq!(
            stem_and_extension("DCIM/100CANON/IMG_0042.CR3"),
            ("IMG_0042", "CR3")
        );
    }

    #[test]
    fn a_name_with_no_extension_is_its_own_stem() {
        assert_eq!(stem_and_extension("README"), ("README", ""));
    }

    #[test]
    fn a_dotfile_with_no_further_dot_is_its_own_stem() {
        assert_eq!(stem_and_extension(".hidden"), (".hidden", ""));
    }
}
