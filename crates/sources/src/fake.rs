// SPDX-License-Identifier: GPL-3.0-or-later
//! An in-memory [`Source`], for tests that need one without touching a real disk (testing
//! strategy §3: "a fake source that can go offline... or lie about sizes"). Not test-gated: other
//! crates use it as a dev-dependency the way `auroraw_catalogue::dataset` is used.

use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use auroraw_plugin_api::source::{Entry, Source, SourceError, SourceState, Watch};

use crate::relink::FoundFile;

struct Inner {
    state: SourceState,
    writable: bool,
    files: HashMap<String, (Vec<u8>, Option<SystemTime>)>,
}

/// A source backed by an in-memory map of path to bytes, shared (`Arc`) so a test can mutate it
/// (add a file, go offline) through one handle while the `Source` itself is held by whatever it
/// is testing.
#[derive(Clone)]
pub struct FakeSource(Arc<Mutex<Inner>>);

impl Default for FakeSource {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeSource {
    /// An empty, online, writable fake source.
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Inner {
            state: SourceState::Online,
            writable: true,
            files: HashMap::new(),
        })))
    }

    /// Adds or replaces a file.
    pub fn put(&self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        self.0
            .lock()
            .unwrap()
            .files
            .insert(path.into(), (bytes.into(), Some(SystemTime::now())));
    }

    /// Removes a file, as if it had been deleted.
    pub fn remove(&self, path: &str) {
        self.0.lock().unwrap().files.remove(path);
    }

    /// Renames a file, as if it had been moved on disk.
    pub fn rename(&self, from: &str, to: &str) {
        let mut inner = self.0.lock().unwrap();
        if let Some(entry) = inner.files.remove(from) {
            inner.files.insert(to.to_string(), entry);
        }
    }

    /// Sets the state this source reports.
    pub fn set_state(&self, state: SourceState) {
        self.0.lock().unwrap().state = state;
    }

    /// Sets whether this source reports itself as writable.
    pub fn set_writable(&self, writable: bool) {
        self.0.lock().unwrap().writable = writable;
    }

    /// Every file, fingerprinted, for [`crate::relink::reconcile`]: the same shape
    /// [`crate::filesystem::FilesystemSource::scan`] produces, so a test can exercise the
    /// reconcile logic against a fake source without touching a disk.
    pub fn scan(&self) -> Vec<FoundFile> {
        let inner = self.0.lock().unwrap();
        inner
            .files
            .iter()
            .map(|(path, (bytes, _))| {
                let (_size, fingerprint) =
                    auroraw_format::fingerprint::fingerprint(&mut std::io::Cursor::new(bytes))
                        .expect("hashing memory cannot fail");
                FoundFile {
                    path: path.clone(),
                    fingerprint,
                }
            })
            .collect()
    }
}

impl Source for FakeSource {
    fn state(&self) -> SourceState {
        self.0.lock().unwrap().state
    }

    fn writable(&self) -> bool {
        self.0.lock().unwrap().writable
    }

    fn list(&self) -> Result<Vec<Entry>, SourceError> {
        let inner = self.0.lock().unwrap();
        if inner.state != SourceState::Online {
            return Err(SourceError::Unreachable);
        }
        Ok(inner
            .files
            .iter()
            .map(|(path, (bytes, modified))| Entry {
                path: path.clone(),
                size: bytes.len() as u64,
                modified: *modified,
            })
            .collect())
    }

    fn stat(&self, path: &str) -> Result<Entry, SourceError> {
        let inner = self.0.lock().unwrap();
        let (bytes, modified) = inner.files.get(path).ok_or_else(|| SourceError::NotFound {
            path: path.to_string(),
        })?;
        Ok(Entry {
            path: path.to_string(),
            size: bytes.len() as u64,
            modified: *modified,
        })
    }

    fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, SourceError> {
        let inner = self.0.lock().unwrap();
        let (bytes, _) = inner.files.get(path).ok_or_else(|| SourceError::NotFound {
            path: path.to_string(),
        })?;
        let start = (offset as usize).min(bytes.len());
        let end = start.saturating_add(len as usize).min(bytes.len());
        Ok(bytes[start..end].to_vec())
    }

    fn watch(&self, _on_change: Sender<()>) -> Result<Option<Watch>, SourceError> {
        // A fake source is polled by tests, not watched: matching a network share's "rescan on
        // demand and at intervals" (architecture §9.1), the simplest of the two mechanisms.
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_list_and_read_agree() {
        let src = FakeSource::new();
        src.put("a.jpg", b"hello".to_vec());
        let listed = src.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].size, 5);
        assert_eq!(src.read_range("a.jpg", 1, 3).unwrap(), b"ell");
    }

    #[test]
    fn going_offline_fails_list_but_not_state() {
        let src = FakeSource::new();
        src.put("a.jpg", b"x".to_vec());
        src.set_state(SourceState::Offline);
        assert_eq!(src.state(), SourceState::Offline);
        assert!(matches!(src.list(), Err(SourceError::Unreachable)));
    }

    #[test]
    fn rename_moves_a_file_without_losing_it() {
        let src = FakeSource::new();
        src.put("old.jpg", b"x".to_vec());
        src.rename("old.jpg", "new.jpg");
        assert!(src.stat("old.jpg").is_err());
        assert!(src.stat("new.jpg").is_ok());
    }
}
