// SPDX-License-Identifier: GPL-3.0-or-later
//! Rendering a destination path template (M1 plan §6, item 6): `{year}`, `{month}`, `{day}`,
//! `{date}` (`{year}-{month}-{day}`), `{hour}`, `{minute}`, `{second}`, `{time}` (`{hour}{minute}{second}`),
//! `{seq}` (a sequence number, `{seq:04}` for zero-padded width 4), `{camera}`, `{original}` (the
//! source file's name without its extension), `{ext}` (its extension, **as found**: an imported file
//! never changes case), `{name}` (the whole file name, as found), `{folder}` (the name of the folder it
//! is in on the card, `100CANON` for `DCIM/100CANON/IMG_0001.CR2`) and `{path}` (its path on the card below
//! `DCIM` when there is one, folders and name as found: `{path}` alone keeps the card's own layout).
//!
//! A value that is not known for a file (`{camera}` with no make or model) renders as an empty piece
//! rather than failing the whole import: a slightly odd path is recoverable, a photo the profile
//! refuses to place is not.

use time::OffsetDateTime;

/// What a template is rendered against, for one planned file.
pub struct TemplateContext<'a> {
    /// The capture time, if known; every date and time token is empty without it.
    pub capture_time: Option<OffsetDateTime>,
    /// This file's position in the import, sorted by capture time across every camera on the
    /// card (M1 plan §6, item 6: "several cameras sort by capture time").
    pub sequence: u32,
    /// The camera model, if known.
    pub camera: Option<&'a str>,
    /// The source file's name, without its extension.
    pub original_stem: &'a str,
    /// The source file's extension, as found (rendered as found).
    pub extension: &'a str,
    /// The file's path in the source, with `/` separators.
    pub source_path: &'a str,
}

/// Characters not safe in a path component on at least one of the three platforms (Windows is the
/// strictest: `< > : " | ? *` and control characters, plus the separators themselves so a
/// rendered value never accidentally introduces an extra path segment).
fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"|?*/\\".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// The name of the folder a file is in (`100CANON` for `DCIM/100CANON/IMG_0001.CR2`), empty at the
/// root of the source.
fn folder_of(path: &str) -> &str {
    let mut parts = path.rsplit('/');
    parts.next();
    parts.next().unwrap_or("")
}

/// The path below the camera's `DCIM` folder when it has one (`100CANON/IMG_0001.CR2`), else the
/// whole path: the layout a card keeps, whatever else is above it. Components are kept as found.
fn path_below_dcim(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let below = parts
        .iter()
        .position(|part| part.eq_ignore_ascii_case("DCIM"))
        .map(|at| &parts[at + 1..])
        .filter(|rest| !rest.is_empty())
        .unwrap_or(&parts[..]);
    below.join("/")
}

fn field(name: &str, spec: Option<&str>, ctx: &TemplateContext) -> String {
    let date = ctx.capture_time;
    match name {
        "year" => date.map(|d| format!("{:04}", d.year())).unwrap_or_default(),
        "month" => date
            .map(|d| format!("{:02}", u8::from(d.month())))
            .unwrap_or_default(),
        "day" => date.map(|d| format!("{:02}", d.day())).unwrap_or_default(),
        "date" => date
            .map(|d| format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day()))
            .unwrap_or_default(),
        "hour" => date.map(|d| format!("{:02}", d.hour())).unwrap_or_default(),
        "minute" => date
            .map(|d| format!("{:02}", d.minute()))
            .unwrap_or_default(),
        "second" => date
            .map(|d| format!("{:02}", d.second()))
            .unwrap_or_default(),
        "time" => date
            .map(|d| format!("{:02}{:02}{:02}", d.hour(), d.minute(), d.second()))
            .unwrap_or_default(),
        "seq" => {
            let width: usize = spec.and_then(|s| s.parse().ok()).unwrap_or(1);
            format!("{:0width$}", ctx.sequence, width = width)
        }
        "camera" => sanitize(ctx.camera.unwrap_or_default()),
        "original" => sanitize(ctx.original_stem),
        "ext" => sanitize(ctx.extension),
        "name" => sanitize(
            ctx.source_path
                .rsplit('/')
                .next()
                .unwrap_or(ctx.source_path),
        ),
        "folder" => sanitize(folder_of(ctx.source_path)),
        "path" => path_below_dcim(ctx.source_path),
        // An unknown token renders as itself, wrapped, rather than silently vanishing: a typo in
        // a template should be visible in the resulting path, not swallowed.
        other => spec.map_or_else(|| format!("{{{other}}}"), |s| format!("{{{other}:{s}}}")),
    }
}

/// Turns rendered template text into a relative path that can only ever stay inside the folder it
/// is joined to: a template is text a person types, and a value it renders can be empty (a photo
/// with no capture time renders `{year}` as nothing, so `{year}/{original}` would become
/// `/IMG.cr3`, an absolute path that `join` would let replace the whole destination). Empty, `.`
/// and `..` components are dropped, every remaining component is sanitised (which also removes a
/// Windows drive prefix's `:`), and if nothing is left `fallback` is used.
pub fn safe_relative(rendered: &str, fallback: &str) -> std::path::PathBuf {
    let path: std::path::PathBuf = rendered
        .split(['/', '\\'])
        .map(|component| sanitize(component).trim_end_matches('.').trim().to_string())
        .filter(|component| !component.is_empty() && component != "." && component != "..")
        .collect();
    if path.as_os_str().is_empty() {
        std::path::PathBuf::from(fallback)
    } else {
        path
    }
}

/// Renders `template` against `ctx`. `{name}` and `{name:spec}` tokens are replaced; text outside
/// braces, including `/` path separators, passes through unchanged.
pub fn render(template: &str, ctx: &TemplateContext) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '{' {
            out.push(c);
            continue;
        }
        let mut token = String::new();
        let mut closed = false;
        for c in chars.by_ref() {
            if c == '}' {
                closed = true;
                break;
            }
            token.push(c);
        }
        if !closed {
            out.push('{');
            out.push_str(&token);
            continue;
        }
        let (name, spec) = match token.split_once(':') {
            Some((n, s)) => (n, Some(s)),
            None => (token.as_str(), None),
        };
        out.push_str(&field(name, spec, ctx));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn ctx(capture_time: Option<OffsetDateTime>) -> TemplateContext<'static> {
        TemplateContext {
            capture_time,
            sequence: 7,
            camera: Some("EOS R5 Mark II"),
            original_stem: "IMG_0042",
            extension: "CR3",
            source_path: "DCIM/100CANON/IMG_0042.CR3",
        }
    }

    #[test]
    fn renders_the_default_destination_shape() {
        let c = ctx(Some(datetime!(2026-09-23 14:05:09 UTC)));
        assert_eq!(
            render("{year}/{date}/{camera}_{seq:04}_{original}.{ext}", &c),
            "2026/2026-09-23/EOS R5 Mark II_0007_IMG_0042.CR3"
        );
    }

    #[test]
    fn a_missing_capture_time_renders_date_tokens_empty_not_a_failure() {
        let c = ctx(None);
        assert_eq!(render("{year}-{month}-{day}", &c), "--");
    }

    #[test]
    fn sequence_without_a_width_pads_to_one_digit() {
        let c = ctx(Some(datetime!(2026-01-01 0:00 UTC)));
        assert_eq!(render("{seq}", &c), "7");
    }

    #[test]
    fn an_unknown_token_is_left_visible_rather_than_silently_dropped() {
        let c = ctx(None);
        assert_eq!(render("{nope}", &c), "{nope}");
    }

    #[test]
    fn a_camera_name_with_unsafe_characters_is_sanitised() {
        let mut c = ctx(None);
        c.camera = Some("A:B/C");
        assert_eq!(render("{camera}", &c), "A_B_C");
    }

    #[test]
    fn an_unclosed_brace_passes_through() {
        let c = ctx(None);
        assert_eq!(render("weird{oops", &c), "weird{oops");
    }

    #[test]
    fn a_rendered_path_can_never_leave_the_folder_it_is_joined_to() {
        let root = std::path::Path::new("/archive");
        for hostile in [
            "/etc/passwd",
            "//IMG.CR3",
            "../../outside/IMG.CR3",
            "a/../../IMG.CR3",
            "C:\\Windows\\IMG.CR3",
            "C:/IMG.CR3",
            "\\\\server\\share\\IMG.CR3",
            "./IMG.CR3",
        ] {
            let relative = safe_relative(hostile, "fallback.CR3");
            assert!(relative.is_relative(), "{hostile}: {relative:?}");
            assert!(
                relative
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_))),
                "{hostile}: {relative:?}"
            );
            assert!(root.join(&relative).starts_with(root), "{hostile}");
        }
    }

    #[test]
    fn a_photo_with_no_capture_time_lands_in_the_folder_above_instead_of_the_filesystem_root() {
        let c = ctx(None);
        assert_eq!(
            safe_relative(&render("{year}/{date}/{original}.{ext}", &c), "x"),
            std::path::PathBuf::from("IMG_0042.CR3")
        );
    }

    #[test]
    fn nothing_left_after_cleaning_falls_back() {
        assert_eq!(
            safe_relative("../..//", "IMG.CR3"),
            std::path::PathBuf::from("IMG.CR3")
        );
    }

    #[test]
    fn a_file_keeps_its_case_and_the_card_layout_tokens_read_the_source_path() {
        let c = ctx(None);
        assert_eq!(render("{original}.{ext}", &c), "IMG_0042.CR3");
        assert_eq!(render("{name}", &c), "IMG_0042.CR3");
        assert_eq!(render("{folder}/{name}", &c), "100CANON/IMG_0042.CR3");
        assert_eq!(render("{path}", &c), "100CANON/IMG_0042.CR3");
    }

    #[test]
    fn the_path_token_starts_below_dcim_and_otherwise_keeps_everything() {
        assert_eq!(
            path_below_dcim("DCIM/100CANON/IMG_1.CR2"),
            "100CANON/IMG_1.CR2"
        );
        assert_eq!(
            path_below_dcim("card/dcim/101NIKON/D1.NEF"),
            "101NIKON/D1.NEF"
        );
        assert_eq!(path_below_dcim("Trip/Day 1/a.ARW"), "Trip/Day 1/a.ARW");
        assert_eq!(path_below_dcim("a.ARW"), "a.ARW");
        assert_eq!(
            path_below_dcim("DCIM"),
            "DCIM",
            "nothing below it: the path stays"
        );
        assert_eq!(folder_of("a.ARW"), "");
    }
}
