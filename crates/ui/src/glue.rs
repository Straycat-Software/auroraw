// SPDX-License-Identifier: GPL-3.0-or-later
//! The Rust half of the C++ glue (`glue.cpp`), everything the interface needs that cxx-qt-lib does not
//! wrap: the `image://thumbs/<photo id>` and `image://preview/<photo id>` providers, loading a translation, and the QtQuickTest runner.
//! The only module with `unsafe` besides the cxx-qt bridges (D-094).

use std::ffi::c_void;
use std::pin::Pin;
use std::str::FromStr;

use auroraw_types::PhotoId;
use cxx_qt_lib::QQmlApplicationEngine;

use crate::session;

unsafe extern "C" {
    fn auroraw_add_import_path(engine: *mut c_void, path: *const u8, len: usize);
    fn auroraw_install_thumbnails(object: *mut c_void);
    fn auroraw_quit_after(ms: i32);
    fn auroraw_add_font(data: *const u8, len: usize);
    fn auroraw_set_translation(data: *const u8, len: usize, object: *mut c_void);
    fn auroraw_thumbnail_ready(token: u64, bytes: *const u8, len: usize);
    fn auroraw_shortcut_text(
        standard: i32,
        text: *const u8,
        len: usize,
        out: *mut u8,
        cap: usize,
    ) -> usize;
}

/// A shortcut as this platform writes it (`Ctrl+N`, `⌘N`): `standard` is a `QKeySequence::StandardKey`,
/// or negative to read `text` as a key sequence.
pub fn shortcut_text(standard: i32, text: &str) -> String {
    let mut out = [0u8; 64];
    // SAFETY: `text` and `out` are valid for the lengths given; the glue writes at most `out.len()`.
    let written = unsafe {
        auroraw_shortcut_text(
            standard,
            text.as_ptr(),
            text.len(),
            out.as_mut_ptr(),
            out.len(),
        )
    };
    String::from_utf8_lossy(&out[..written.min(out.len())]).into_owned()
}

/// The typeface the interface is set in (IBM Plex Sans, SIL Open Font License 1.1: `assets/fonts/OFL.txt`),
/// carried in the executable so that it looks the same everywhere. Made available once, by its family
/// name (`FONT_FAMILY`).
pub fn load_fonts() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        for font in [
            &include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf")[..],
            &include_bytes!("../assets/fonts/IBMPlexSans-Italic.ttf")[..],
            &include_bytes!("../assets/fonts/IBMPlexSans-Bold.ttf")[..],
            &include_bytes!("../assets/fonts/IBMPlexSans-BoldItalic.ttf")[..],
        ] {
            // SAFETY: the bytes are valid for the call (the glue copies them).
            unsafe { auroraw_add_font(font.as_ptr(), font.len()) }
        }
    });
}

/// Tests: has the application quit by itself after `ms` milliseconds.
pub fn quit_after(ms: i32) {
    // SAFETY: only called once the application object exists.
    unsafe { auroraw_quit_after(ms) }
}

/// Adds `path` (such as `qrc:/qt/qml`) where `engine` looks for QML modules.
pub fn add_import_path(mut engine: Pin<&mut QQmlApplicationEngine>, path: &str) {
    // SAFETY: the pointer is only used for this call (the engine is a C++ object behind a pin), and
    // `path` is valid for its length.
    unsafe {
        let raw = engine.as_mut().get_unchecked_mut() as *mut QQmlApplicationEngine;
        auroraw_add_import_path(raw as *mut c_void, path.as_ptr(), path.len())
    }
}

/// Registers the thumbnail provider (`image://thumbs/<photo id>`) on the engine of `object`.
///
/// # Safety
/// `object` points at a live QObject made by QML.
pub unsafe fn install_thumbnails(object: *mut c_void) {
    // SAFETY: the caller guarantees `object`.
    unsafe { auroraw_install_thumbnails(object) }
}

/// Installs the translation in `qm` (compiled `.qm` bytes, `'static` because Qt reads them in place)
/// in place of the previous one; `None` goes back to the source texts. When `object` is given (a
/// QObject made by QML, such as the launcher) its engine retranslates what is on screen.
///
/// # Safety
/// `object` is null or points at a live QObject.
pub unsafe fn set_translation(qm: Option<&'static [u8]>, object: *mut c_void) {
    let (data, len) = qm.map_or((std::ptr::null(), 0), |bytes| (bytes.as_ptr(), bytes.len()));
    // SAFETY: the bytes are `'static`, a null pointer with a length of 0 means "none", and the
    // caller guarantees `object`.
    unsafe { auroraw_set_translation(data, len, object) }
}

/// How the collector answers the image provider's requests: the C++ side decodes the JPEG (on the
/// collector's thread, off the interface's) and hands the image to the request that waits for it.
pub fn thumbnail_deliverer() -> session::Deliver {
    std::sync::Arc::new(|token, jpeg| {
        let (bytes, len) = jpeg.map_or((std::ptr::null(), 0), |b| (b.as_ptr(), b.len()));
        // SAFETY: `bytes` and `len` describe `jpeg` (or nothing, null with 0), which outlives the call.
        unsafe { auroraw_thumbnail_ready(token, bytes, len) }
    })
}

/// A request of an image provider (`image://thumbs/<photo id>` for `kind` 0, `image://preview/<photo id>`
/// for 1, the id being `id`, UTF-8, `len` bytes); the answer goes to `auroraw_thumbnail_ready` under `token`.
///
/// # Safety
/// `id` points at `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn auroraw_thumbnail_request(
    kind: i32,
    id: *const u8,
    len: usize,
    token: u64,
) {
    // SAFETY: the caller guarantees `id` and `len`.
    let name = unsafe { std::slice::from_raw_parts(id, len) };
    let asked = std::str::from_utf8(name)
        .ok()
        .and_then(|text| PhotoId::from_str(text).ok())
        .zip(session::current());
    match asked {
        // 1 the picture of the image view, 2 its focus peaking overlay, 3 its clipping overlay.
        Some((id, session)) if kind >= 1 => session.previews.request(id, (kind - 1) as u8, token),
        Some((id, session)) => session.thumbs.request(id, 0, token),
        // SAFETY: no bytes (null, 0) is allowed.
        None => unsafe { auroraw_thumbnail_ready(token, std::ptr::null(), 0) },
    }
}

#[cfg(feature = "quicktest")]
mod quicktest {
    use std::ffi::{CString, c_char, c_int};

    unsafe extern "C" {
        fn auroraw_quick_test(argc: c_int, argv: *mut *mut c_char, dir: *const c_char) -> c_int;
    }

    /// Runs the QtQuickTest suites named on the command line (`-input <file>`), the folder of the
    /// tests being `dir`. Returns the number of failures.
    pub fn run(dir: &str) -> i32 {
        let args: Vec<CString> = std::env::args()
            .map(|a| CString::new(a).expect("no NUL in an argument"))
            .collect();
        let dir = CString::new(dir).expect("no NUL in a path");
        let mut argv: Vec<*mut c_char> = args.iter().map(|a| a.as_ptr() as *mut _).collect();
        argv.push(std::ptr::null_mut());
        // SAFETY: argv is a null-terminated array of live C strings; dir is a live C string.
        unsafe { auroraw_quick_test(args.len() as c_int, argv.as_mut_ptr(), dir.as_ptr()) }
    }
}

#[cfg(feature = "quicktest")]
pub use quicktest::run as quick_test;
