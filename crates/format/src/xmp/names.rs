// SPDX-License-Identifier: GPL-3.0-or-later
/// The namespaces this crate knows, with the prefix each is written with.
pub mod ns {
    /// `rdf`.
    pub const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    /// `x`, the wrapper element.
    pub const META: &str = "adobe:ns:meta/";
    /// `xml`, for `xml:lang`.
    pub const XML: &str = "http://www.w3.org/XML/1998/namespace";
    /// `xmp`.
    pub const XMP: &str = "http://ns.adobe.com/xap/1.0/";
    /// `dc`.
    pub const DC: &str = "http://purl.org/dc/elements/1.1/";
    /// `lr`.
    pub const LR: &str = "http://ns.adobe.com/lightroom/1.0/";
    /// `exif`.
    pub const EXIF: &str = "http://ns.adobe.com/exif/1.0/";
    /// `tiff`.
    pub const TIFF: &str = "http://ns.adobe.com/tiff/1.0/";
    /// `aux`.
    pub const AUX: &str = "http://ns.adobe.com/exif/1.0/aux/";
    /// `photoshop`.
    pub const PHOTOSHOP: &str = "http://ns.adobe.com/photoshop/1.0/";
    /// `xmpRights`.
    pub const XMP_RIGHTS: &str = "http://ns.adobe.com/xap/1.0/rights/";
    /// `Iptc4xmpCore`.
    pub const IPTC_CORE: &str = "http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/";
    /// `Iptc4xmpExt`.
    pub const IPTC_EXT: &str = "http://iptc.org/std/Iptc4xmpExt/2008-02-29/";
    /// `aur`, Auroraw's own properties (note 003 §4.2).
    pub const AUR: &str = "https://auroraw.org/ns/1.0/";
    /// `digiKam`, read from files digiKam wrote (never written: Auroraw has no property of its own to
    /// put there).
    pub const DIGIKAM: &str = "http://www.digikam.org/ns/1.0/";
}

/// The namespaces written with a fixed prefix, in the order they are declared.
pub(crate) const KNOWN: &[(&str, &str)] = &[
    ("aur", ns::AUR),
    ("aux", ns::AUX),
    ("dc", ns::DC),
    ("exif", ns::EXIF),
    ("Iptc4xmpCore", ns::IPTC_CORE),
    ("Iptc4xmpExt", ns::IPTC_EXT),
    ("lr", ns::LR),
    ("photoshop", ns::PHOTOSHOP),
    ("tiff", ns::TIFF),
    ("xmp", ns::XMP),
    ("xmpRights", ns::XMP_RIGHTS),
];

/// Whether `s` can be written as an XML name without a colon (an NCName). The reader refuses
/// names that are not, so that everything it accepts can be written and read again.
pub(crate) fn is_ncname(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// A character XML 1.0 allows in text and attribute values.
pub(crate) fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

/// The text with every character XML does not allow replaced by U+FFFD, which is what the writer
/// would do: a file with stray control characters (some tools write them) stays readable, and what
/// is read is what is written.
pub(crate) fn clean(s: String) -> String {
    if s.chars().all(is_xml_char) {
        s
    } else {
        s.chars()
            .map(|c| if is_xml_char(c) { c } else { '\u{FFFD}' })
            .collect()
    }
}
