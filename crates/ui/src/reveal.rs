// SPDX-License-Identifier: GPL-3.0-or-later
//! "Show in file manager" (WP9, D-106): opens the operating system's file manager on a photo's original file. On
//! macOS and Windows it selects the file; on Linux there is no portable way to do that (D-106), so it opens the
//! containing folder instead. Under the QML suites (`AURORAW_TEST_HOME` set, `crates/ui/src/lib.rs`) no process is
//! ever started: the path is only recorded, the same way a native dialog is stood in for under test rather than
//! actually opened.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

static LAST_REVEALED: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Reveals `path` in the platform's file manager, or, under the QML suites, only records it.
pub(crate) fn reveal(path: &Path) {
    reveal_or_record(path, std::env::var_os("AURORAW_TEST_HOME").is_some());
}

fn reveal_or_record(path: &Path, testing: bool) {
    if testing {
        *LAST_REVEALED.lock().expect("not poisoned") = Some(path.to_path_buf());
        return;
    }
    open_file_manager(path);
}

/// What the last call to [`reveal`] was asked to show, while under test.
pub(crate) fn last_revealed() -> Option<PathBuf> {
    LAST_REVEALED.lock().expect("not poisoned").clone()
}

#[cfg(target_os = "macos")]
fn open_file_manager(path: &Path) {
    let _ = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
}

#[cfg(target_os = "windows")]
fn open_file_manager(path: &Path) {
    let mut arg = std::ffi::OsString::from("/select,");
    arg.push(path);
    let _ = std::process::Command::new("explorer").arg(arg).spawn();
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn open_file_manager(path: &Path) {
    // No portable way to select a file in a Linux file manager (D-106): the containing folder opens instead.
    let folder = path.parent().unwrap_or(path);
    let _ = std::process::Command::new("xdg-open").arg(folder).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_test_nothing_is_spawned_and_the_path_is_only_remembered() {
        let path = Path::new("/tmp/whatever-auroraw-test/a.jpg");
        reveal_or_record(path, true);
        assert_eq!(last_revealed().as_deref(), Some(path));
    }
}
