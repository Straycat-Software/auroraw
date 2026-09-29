// SPDX-License-Identifier: GPL-3.0-or-later
//! Atomic writes: a temporary file, then a rename (design note 001 §5.4).

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

/// How long a rename is retried when another program holds the target open (Windows: antivirus
/// scanners and search indexers do).
const RETRY_FOR: &[u64] = &[5, 10, 20, 40, 80, 160];

/// Where a write may be interrupted, for tests of crash consistency.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Interrupt {
    /// Do not interrupt.
    Never,
    /// Stop after the temporary file is complete, before the rename: a crash at this moment.
    BeforeRename,
}

#[cfg(windows)]
fn retryable(e: &io::Error) -> bool {
    // ERROR_ACCESS_DENIED (5), ERROR_SHARING_VIOLATION (32), ERROR_LOCK_VIOLATION (33)
    matches!(e.kind(), io::ErrorKind::PermissionDenied)
        || matches!(e.raw_os_error(), Some(5 | 32 | 33))
}

#[cfg(not(windows))]
fn retryable(_: &io::Error) -> bool {
    false
}

pub(crate) fn rename_with_retry(from: &Path, to: &Path) -> io::Result<()> {
    let mut delays = RETRY_FOR.iter();
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if retryable(&e) => match delays.next() {
                Some(ms) => std::thread::sleep(Duration::from_millis(*ms)),
                None => return Err(e),
            },
            Err(e) => return Err(e),
        }
    }
}

/// Writes `bytes` to `target` through `tmp`. The parent folder of `target` is created if it does
/// not exist (the common case costs no extra call: on Windows a folder check is 0.15 ms, a sixth
/// of the whole write). A crash leaves the old file or the new one, never a mixture; a leftover
/// temporary file is removed at the next opening.
pub(crate) fn write_atomic(
    target: &Path,
    tmp: &Path,
    bytes: &[u8],
    sync: bool,
    interrupt: Interrupt,
) -> io::Result<()> {
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(tmp)?;
        file.write_all(bytes)?;
        if sync {
            file.sync_all()?;
        }
        drop(file);
        if interrupt == Interrupt::BeforeRename {
            return Err(io::Error::other("interrupted before the rename"));
        }
        match rename_with_retry(tmp, target) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // The shard folder does not exist yet: create it, and rename again.
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                rename_with_retry(tmp, target)
            }
            other => other,
        }
    })();
    if result.is_err() && interrupt == Interrupt::Never {
        let _ = fs::remove_file(tmp);
    }
    result
}

/// Writes `bytes` to `target` atomically with the temporary file **beside** it (`<name>.part`), for a
/// file that is not in the workspace: the XMP export writes into a source folder, which may be on
/// another volume than the workspace's `.auroraw/tmp/`, where a rename would not be atomic (design
/// note 003 §8.1 item 9). A crash leaves the old file or the new one. The folder must exist: an
/// export never creates folders in someone's source.
pub fn write_beside(target: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = target.parent()
        && !parent.as_os_str().is_empty()
        && !parent.is_dir()
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the folder does not exist",
        ));
    }
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let mut tmp_name = name.to_os_string();
    tmp_name.push(".part");
    let tmp = target.with_file_name(tmp_name);
    write_atomic(target, &tmp, bytes, true, Interrupt::Never)
}
