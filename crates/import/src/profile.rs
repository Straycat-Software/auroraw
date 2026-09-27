// SPDX-License-Identifier: GPL-3.0-or-later
//! An import profile: everything one import needs (D-029), stored at user level, shared by every
//! catalogue (D-052, design note 002 §6.6) -- this crate resolves no platform directory itself
//! (`catalogue::Registry`'s own precedent), it only reads and writes whatever path its caller
//! gives it, importable and exportable as a plain file because it already is one.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{ImportError, Result};

/// Whether a profile copies both files of a RAW+JPEG pair, or only one (D-032).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PairRule {
    /// Copies both the RAW and its companion JPEG.
    Both,
    /// Copies only the RAW; its companion JPEG is left on the card.
    RawOnly,
    /// Copies only the JPEG; the RAW is left on the card.
    JpegOnly,
}

/// The metadata template (D-029): written to every imported photo's sidecar, never into the
/// copied file itself (spec §5.2). Keywords are paths (`"Family|Wedding"`), not identifiers: this
/// crate has no vocabulary to resolve them against (that needs the catalogue), so the caller
/// resolves each path to a `KeywordId`, creating one if it does not exist yet, the way
/// `engine::Coordinator::edit_keywords` already resolves a path for a single keyword.
///
/// A creator and a copyright notice used to be part of this too, asked once per import; removed
/// (issue #4) since the Import dialog is the only thing that ever fed them, and a photo's own
/// discovered metadata (its embedded EXIF/XMP creator and rights, if any) is what `Metadata`
/// itself keeps instead of being overwritten by a template's guess.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataTemplate {
    /// Keyword paths to add to every imported photo.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyword_paths: Vec<String>,
}

/// Everything one import profile stores (D-029). A style is part of the specification's own
/// schema for this ("a style to apply, creating the photo's first version") but has nowhere to
/// apply to before the develop pipeline exists (M2): not carried here yet, added when a style is
/// something this crate can actually apply, rather than as an inert field with no consumer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// This profile's display name.
    pub name: String,
    /// Where files are copied, as a path template rendered relative to the destination source's
    /// root ([`crate::template`]).
    pub destination_template: String,
    /// Extra verified copies, as path templates rendered relative to their own roots (a caller
    /// supplies the roots at import time: a backup destination is not necessarily a registered
    /// source).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backup_templates: Vec<String>,
    /// Whether a RAW+JPEG pair copies both files.
    pub pair_rule: PairRule,
    /// The metadata written to every imported photo's sidecar.
    #[serde(default)]
    pub metadata_template: MetadataTemplate,
}

impl Profile {
    /// Reads a profile from a plain JSON file (D-052: "importable and exportable as files").
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).map_err(|e| ImportError::io(path, e))?;
        serde_json::from_slice(&bytes).map_err(|e| ImportError::Profile {
            path: path.to_path_buf(),
            source: e,
        })
    }

    /// Writes a profile to a plain JSON file, through a temporary file and a rename (design note
    /// 001 §5.4's pattern; this crate does not depend on `workspace`, so it keeps its own small
    /// copy rather than reaching for one written for JSON state files with a different envelope).
    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).expect("a profile always serialises");
        crate::atomic::write(path, &bytes).map_err(|e| ImportError::io(path, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Profile {
        Profile {
            name: "Default".into(),
            destination_template: "{year}/{year}-{month}-{day}/{camera}_{seq:04}_{original}".into(),
            backup_templates: vec!["backup/{year}/{original}".into()],
            pair_rule: PairRule::Both,
            metadata_template: MetadataTemplate {
                keyword_paths: vec!["Family".into()],
            },
        }
    }

    #[test]
    fn a_profile_round_trips_through_a_file() {
        let dir = auroraw_testkit::temp_dir();
        let path = dir.path().join("Default.json");
        let profile = sample();
        profile.save(&path).unwrap();
        assert_eq!(Profile::load(&path).unwrap(), profile);
    }

    #[test]
    fn loading_a_missing_profile_is_an_error_not_a_default() {
        let dir = auroraw_testkit::temp_dir();
        assert!(Profile::load(&dir.path().join("missing.json")).is_err());
    }
}
