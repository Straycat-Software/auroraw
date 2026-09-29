// SPDX-License-Identifier: GPL-3.0-or-later
//! Auroraw's own side of the three-way comparison of an external XMP file (D-047; design note 003
//! §8.1), shared by the coordinator (which decides and applies) and the read API (which only shows).

use std::collections::HashMap;

use auroraw_catalogue::ExternalRow;
use auroraw_format::sidecar::Metadata;
use auroraw_format::sidecar::external::{Field, Fields, Merge, merge};
use auroraw_types::KeywordId;

use crate::command::MetadataField;

/// What a photo says, in the form a file is compared in: keywords by their current path in the
/// vocabulary, not the possibly stale snapshot in the sidecar.
pub(crate) fn mine_of(meta: &Metadata, key_paths: &HashMap<KeywordId, String>) -> Fields {
    Fields::from_metadata(meta, |id| key_paths.get(id).cloned())
}

/// What a photo's noticed change amounts to now, against what the photo says now: `None` when nothing
/// waits for it.
pub(crate) fn pending_diff(
    row: &ExternalRow,
    meta: &Metadata,
    key_paths: &HashMap<KeywordId, String>,
) -> Option<Merge> {
    let pending = row.pending.as_ref()?;
    Some(merge(
        Some(&row.base),
        &pending.file,
        &mine_of(meta, key_paths),
    ))
}

/// The text field the metadata panel and the history know a [`Field`] by, for the ones that are one of
/// the seventeen (a change to it is a `Change::Metadata`, undone like typing it).
pub(crate) fn metadata_field(field: Field) -> Option<MetadataField> {
    Some(match field {
        Field::Title => MetadataField::Title,
        Field::Caption => MetadataField::Caption,
        Field::Creator => MetadataField::Creator,
        Field::Rights => MetadataField::Rights,
        Field::UsageTerms => MetadataField::UsageTerms,
        Field::WebStatement => MetadataField::WebStatement,
        Field::Credit => MetadataField::Credit,
        Field::Source => MetadataField::Source,
        Field::Headline => MetadataField::Headline,
        Field::Instructions => MetadataField::Instructions,
        Field::Sublocation => MetadataField::Sublocation,
        Field::City => MetadataField::City,
        Field::Region => MetadataField::Region,
        Field::Country => MetadataField::Country,
        Field::CountryCode => MetadataField::CountryCode,
        Field::Persons => MetadataField::Persons,
        Field::Event => MetadataField::Event,
        Field::Rating | Field::Label | Field::Keywords => return None,
    })
}
