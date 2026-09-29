// SPDX-License-Identifier: GPL-3.0-or-later
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use auroraw_format::sidecar::{PhotoSidecar, VersionSidecar};
use auroraw_format::state::{
    Collection, Loaded, Marker, Series, Sources, StateBody, Vocabulary, read_state, write_state,
};
use auroraw_types::{CollectionId, PhotoId, SeriesId, Timestamp, VersionId, WorkspaceId};

use crate::error::WorkspaceError;
use crate::layout::{self, LAYOUT_VERSION};
use crate::scan::{self, Scan};
use crate::write::{self, Interrupt};

/// Sidecars and state files are small; anything larger than this is refused, not read.
const MAX_FILE: u64 = 64 * 1024 * 1024;

/// Whether this handle may write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// This handle holds the writer's lock (or the file system offers none).
    ReadWrite,
    /// Another program holds the lock, or the folder cannot be written.
    ReadOnly,
}

/// What a write did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    /// The file was written.
    Written,
    /// The file already had exactly these bytes and was left untouched.
    Unchanged,
}

/// An open workspace.
#[derive(Debug)]
pub struct Workspace {
    root: PathBuf,
    marker: Marker,
    access: Access,
    sync: bool,
    /// Held for as long as the workspace is open for writing; released on drop.
    _lock: Option<File>,
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> WorkspaceError + '_ {
    move |e| WorkspaceError::io(path, e)
}

fn read_capped(path: &Path) -> Result<Option<Vec<u8>>, WorkspaceError> {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(WorkspaceError::io(path, e)),
    };
    let size = file.metadata().map_err(io_err(path))?.len();
    if size > MAX_FILE {
        return Err(WorkspaceError::TooLarge {
            path: path.to_path_buf(),
            size,
        });
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.read_to_end(&mut bytes).map_err(io_err(path))?;
    Ok(Some(bytes))
}

impl Workspace {
    /// Creates a workspace in `root` (which may exist and be empty) and opens it for writing.
    pub fn create(root: &Path, catalogue_name: &str) -> Result<Self, WorkspaceError> {
        if layout::marker(root).exists() {
            return Err(WorkspaceError::AlreadyExists(root.to_path_buf()));
        }
        for dir in [
            root.to_path_buf(),
            root.join(layout::PHOTOS),
            root.join(layout::VERSIONS),
            root.join(layout::STATE),
            root.join(layout::INTERNAL).join(layout::TMP),
        ] {
            fs::create_dir_all(&dir).map_err(io_err(&dir))?;
        }
        let lock = Self::take_lock(root)?.ok_or(WorkspaceError::ReadOnly)?;
        let marker = Marker {
            layout: LAYOUT_VERSION,
            workspace_id: WorkspaceId::random(),
            catalogue_name: catalogue_name.to_string(),
            created: Timestamp::now(),
            created_by: auroraw_types::app_version().to_string(),
            extra: Default::default(),
        };
        let ws = Self {
            root: root.to_path_buf(),
            marker,
            access: Access::ReadWrite,
            sync: false,
            _lock: lock,
        };
        ws.write_bytes(&layout::marker(root), &write_state(&ws.marker))?;
        ws.write_bytes(&root.join(layout::README), layout::README_TEXT.as_bytes())?;
        Ok(ws)
    }

    /// Opens an existing workspace. The result is read-write if this handle got the writer's lock,
    /// read-only if another program holds it.
    pub fn open(root: &Path) -> Result<Self, WorkspaceError> {
        let marker_path = layout::marker(root);
        let bytes = read_capped(&marker_path)?
            .ok_or_else(|| WorkspaceError::NotAWorkspace(root.to_path_buf()))?;
        let marker = match read_state::<Marker>(&bytes) {
            Ok(Loaded::Current(m)) => m,
            Ok(Loaded::Newer(_)) => return Err(WorkspaceError::NewerMarker),
            Err(source) => {
                return Err(WorkspaceError::State {
                    path: marker_path,
                    source,
                });
            }
        };
        if marker.layout > LAYOUT_VERSION {
            return Err(WorkspaceError::NewerLayout {
                found: marker.layout,
                supported: LAYOUT_VERSION,
            });
        }
        let (access, lock) = match Self::take_lock(root) {
            Ok(Some(lock)) => (Access::ReadWrite, lock),
            Ok(None) | Err(_) => (Access::ReadOnly, None),
        };
        let ws = Self {
            root: root.to_path_buf(),
            marker,
            access,
            sync: false,
            _lock: lock,
        };
        if ws.access == Access::ReadWrite {
            ws.remove_leftovers();
        }
        Ok(ws)
    }

    /// The advisory lock. `Ok(None)` means another program holds it; `Ok(Some(None))` means the
    /// file system offers no locks, and writing goes ahead without one (design note 001 §5.5).
    fn take_lock(root: &Path) -> Result<Option<Option<File>>, WorkspaceError> {
        let dir = root.join(layout::INTERNAL);
        let path = dir.join(layout::LOCK);
        fs::create_dir_all(dir.join(layout::TMP)).map_err(io_err(&dir))?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(io_err(&path))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Some(file))),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(e)) if e.kind() == io::ErrorKind::Unsupported => Ok(Some(None)),
            Err(TryLockError::Error(e)) => Err(WorkspaceError::io(path, e)),
        }
    }

    /// What is left in `.auroraw/tmp/` is the leftover of a crash: only the lock holder removes it.
    fn remove_leftovers(&self) {
        let tmp = self.root.join(layout::INTERNAL).join(layout::TMP);
        if let Ok(entries) = fs::read_dir(tmp) {
            for entry in entries.flatten() {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    /// The folder of the workspace.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The marker.
    pub fn marker(&self) -> &Marker {
        &self.marker
    }

    /// The identifier of the workspace.
    pub fn workspace_id(&self) -> WorkspaceId {
        self.marker.workspace_id
    }

    /// Whether this handle may write.
    pub fn access(&self) -> Access {
        self.access
    }

    /// Whether every write is flushed to disk before the rename. Off by default: an interactive
    /// edit does not wait for the disk (note 001 §5.4, decided per batch by the engine).
    pub fn set_sync(&mut self, sync: bool) {
        self.sync = sync;
    }

    // ---- paths ----

    /// The path of a photo sidecar.
    pub fn photo_path(&self, id: &PhotoId) -> PathBuf {
        layout::photo(&self.root, id)
    }

    /// The path of a version sidecar.
    pub fn version_path(&self, photo: &PhotoId, version: &VersionId) -> PathBuf {
        layout::version(&self.root, photo, version)
    }

    /// The path of the vocabulary.
    pub fn vocabulary_path(&self) -> PathBuf {
        layout::vocabulary(&self.root)
    }

    /// The path of the sources file.
    pub fn sources_path(&self) -> PathBuf {
        layout::sources(&self.root)
    }

    /// The path of a collection.
    pub fn collection_path(&self, id: &CollectionId) -> PathBuf {
        layout::collection(&self.root, id)
    }

    /// The path of a series.
    pub fn series_path(&self, id: &SeriesId) -> PathBuf {
        layout::series(&self.root, id)
    }

    /// The default folder for exports; created by the exporter, never scanned.
    pub fn exports_dir(&self) -> PathBuf {
        self.root.join(layout::EXPORTS)
    }

    // ---- writing ----

    fn write_bytes(&self, path: &Path, bytes: &[u8]) -> Result<WriteOutcome, WorkspaceError> {
        if self.access == Access::ReadOnly {
            return Err(WorkspaceError::ReadOnly);
        }
        self.write_bytes_with(path, bytes, Interrupt::Never)
    }

    pub(crate) fn write_bytes_with(
        &self,
        path: &Path,
        bytes: &[u8],
        interrupt: Interrupt,
    ) -> Result<WriteOutcome, WorkspaceError> {
        if let Ok(existing) = fs::read(path)
            && existing == bytes
        {
            return Ok(WriteOutcome::Unchanged);
        }
        let tmp = self
            .root
            .join(layout::INTERNAL)
            .join(layout::TMP)
            .join(format!("{}.tmp", VersionId::random()));
        write::write_atomic(path, &tmp, bytes, self.sync, interrupt).map_err(io_err(path))?;
        Ok(WriteOutcome::Written)
    }

    /// Writes a photo sidecar (canonical bytes; untouched if nothing changed).
    pub fn write_photo(&self, photo: &PhotoSidecar) -> Result<WriteOutcome, WorkspaceError> {
        self.write_bytes(&self.photo_path(&photo.photo_id), &photo.to_bytes())
    }

    /// Writes a version sidecar.
    pub fn write_version(&self, version: &VersionSidecar) -> Result<WriteOutcome, WorkspaceError> {
        self.write_bytes(
            &self.version_path(&version.photo_id, &version.version_id),
            &version.to_bytes(),
        )
    }

    /// Writes the vocabulary.
    pub fn write_vocabulary(
        &self,
        vocabulary: &Vocabulary,
    ) -> Result<WriteOutcome, WorkspaceError> {
        self.write_bytes(&self.vocabulary_path(), &vocabulary.to_bytes())
    }

    /// Writes the sources.
    pub fn write_sources(&self, sources: &Sources) -> Result<WriteOutcome, WorkspaceError> {
        self.write_bytes(&self.sources_path(), &write_state(sources))
    }

    /// Writes a collection.
    pub fn write_collection(
        &self,
        collection: &Collection,
    ) -> Result<WriteOutcome, WorkspaceError> {
        self.write_bytes(
            &self.collection_path(&collection.id),
            &write_state(collection),
        )
    }

    /// Writes a series.
    pub fn write_series(&self, series: &Series) -> Result<WriteOutcome, WorkspaceError> {
        self.write_bytes(&self.series_path(&series.id), &write_state(series))
    }

    // ---- reading ----

    /// Reads a photo sidecar; `None` if there is none.
    pub fn read_photo(&self, id: &PhotoId) -> Result<Option<Loaded<PhotoSidecar>>, WorkspaceError> {
        let path = self.photo_path(id);
        match read_capped(&path)? {
            None => Ok(None),
            Some(bytes) => PhotoSidecar::from_bytes(&bytes)
                .map(Some)
                .map_err(|source| WorkspaceError::Sidecar { path, source }),
        }
    }

    /// Reads a version sidecar; `None` if there is none.
    pub fn read_version(
        &self,
        photo: &PhotoId,
        version: &VersionId,
    ) -> Result<Option<Loaded<VersionSidecar>>, WorkspaceError> {
        let path = self.version_path(photo, version);
        match read_capped(&path)? {
            None => Ok(None),
            Some(bytes) => VersionSidecar::from_bytes(&bytes)
                .map(Some)
                .map_err(|source| WorkspaceError::Sidecar { path, source }),
        }
    }

    fn read_state_at<T: StateBody>(
        &self,
        path: PathBuf,
    ) -> Result<Option<Loaded<T>>, WorkspaceError> {
        match read_capped(&path)? {
            None => Ok(None),
            Some(bytes) => read_state::<T>(&bytes)
                .map(Some)
                .map_err(|source| WorkspaceError::State { path, source }),
        }
    }

    /// Reads the vocabulary; `None` if there is none yet.
    pub fn read_vocabulary(&self) -> Result<Option<Loaded<Vocabulary>>, WorkspaceError> {
        self.read_state_at(self.vocabulary_path())
    }

    /// Reads the sources; `None` if there are none yet.
    pub fn read_sources(&self) -> Result<Option<Loaded<Sources>>, WorkspaceError> {
        self.read_state_at(self.sources_path())
    }

    /// Reads a collection.
    pub fn read_collection(
        &self,
        id: &CollectionId,
    ) -> Result<Option<Loaded<Collection>>, WorkspaceError> {
        self.read_state_at(self.collection_path(id))
    }

    /// Takes a collection's file out of the workspace, recoverably (it goes to `removed/`, as a series' does).
    /// Nothing to do when there is no such file.
    pub fn remove_collection(&self, id: &CollectionId) -> Result<(), WorkspaceError> {
        let path = self.collection_path(id);
        if path.exists() {
            self.remove_recoverably(&path)?;
        }
        Ok(())
    }

    /// Takes a series' file out of the workspace, recoverably (it goes to `removed/`, like a photo's sidecar does
    /// when its source is removed). Nothing to do when there is no such file.
    pub fn remove_series(&self, id: &SeriesId) -> Result<(), WorkspaceError> {
        let path = self.series_path(id);
        if path.exists() {
            self.remove_recoverably(&path)?;
        }
        Ok(())
    }

    /// Reads a series.
    pub fn read_series(&self, id: &SeriesId) -> Result<Option<Loaded<Series>>, WorkspaceError> {
        self.read_state_at(self.series_path(id))
    }

    // ---- scanning and removing ----

    /// Walks the workspace once: every recognised file with its size and modification time, and
    /// everything else, listed and left alone (design note 001 §5.6).
    pub fn scan(&self) -> Result<Scan, WorkspaceError> {
        scan::scan(&self.root)
    }

    /// Moves a file of the workspace to `removed/`, keeping its place in the tree and adding the
    /// time to its name. Nothing is ever deleted (architecture §5.3). Returns the new path.
    pub fn remove_recoverably(&self, path: &Path) -> Result<PathBuf, WorkspaceError> {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        let destination = self.recoverable_path(path, relative)?;
        let source = self.root.join(relative);
        write::rename_with_retry(&source, &destination).map_err(io_err(&source))?;
        Ok(destination)
    }

    /// Keeps a copy of a file that is not in the workspace and is about to be replaced (the XMP export's
    /// Replace option, design note 003 §8.1 item 5) under `removed/`, at `relative` with the time added
    /// to its name. Nothing is ever deleted (architecture §5.3). Returns the path it was kept at.
    pub fn keep_recoverably(
        &self,
        relative: &Path,
        bytes: &[u8],
    ) -> Result<PathBuf, WorkspaceError> {
        let destination = self.recoverable_path(relative, relative)?;
        fs::write(&destination, bytes).map_err(io_err(&destination))?;
        Ok(destination)
    }

    /// A place under `removed/` for `relative`, free, in a folder that exists; `shown` is the path to
    /// name in an error.
    fn recoverable_path(&self, shown: &Path, relative: &Path) -> Result<PathBuf, WorkspaceError> {
        if self.access == Access::ReadOnly {
            return Err(WorkspaceError::ReadOnly);
        }
        let escapes = relative.is_absolute()
            || relative
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)));
        if escapes || relative.as_os_str().is_empty() {
            return Err(WorkspaceError::Outside(shown.to_path_buf()));
        }
        let stamp = Timestamp::now().to_string().replace(['-', ':'], "");
        let stem = relative
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let extension = relative.extension().and_then(|s| s.to_str());
        let folder = self
            .root
            .join(layout::REMOVED)
            .join(relative.parent().unwrap_or(Path::new("")));
        fs::create_dir_all(&folder).map_err(io_err(&folder))?;
        for attempt in 0.. {
            let suffix = if attempt == 0 {
                String::new()
            } else {
                format!("-{attempt}")
            };
            let name = match extension {
                Some(e) => format!("{stem}.{stamp}{suffix}.{e}"),
                None => format!("{stem}.{stamp}{suffix}"),
            };
            let destination = folder.join(name);
            if !destination.exists() {
                return Ok(destination);
            }
        }
        unreachable!("the loop only ends by returning")
    }
}

/// A photo sidecar found under `removed/` (moved there by [`Workspace::remove_recoverably`]).
#[derive(Debug, Clone)]
pub struct RemovedPhoto {
    /// Where the sidecar is now.
    pub path: PathBuf,
    /// What it holds.
    pub photo: PhotoSidecar,
}

fn files_under(folder: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "xmp") {
            out.push(path);
        }
    }
}

impl Workspace {
    /// The live version sidecars of a photo (the files that move with it when it is removed).
    pub fn version_files_of(&self, photo: &PhotoId) -> Vec<PathBuf> {
        let Some(shard) = self
            .version_path(photo, &VersionId::random())
            .parent()
            .map(Path::to_path_buf)
        else {
            return Vec::new();
        };
        let prefix = format!("{photo}.");
        let mut files = Vec::new();
        files_under(&shard, &mut files);
        files.retain(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix))
        });
        files
    }

    /// Every photo sidecar under `removed/` that can be read: what a person removed earlier,
    /// with its ratings and keywords, waiting to be restored (a file that cannot be read, or that
    /// is from a newer schema, is left alone and left out).
    pub fn removed_photos(&self) -> Vec<RemovedPhoto> {
        let mut files = Vec::new();
        files_under(
            &self.root.join(layout::REMOVED).join(layout::PHOTOS),
            &mut files,
        );
        files.sort();
        files
            .into_iter()
            .filter_map(|path| {
                let bytes = read_capped(&path).ok()??;
                let photo = PhotoSidecar::from_bytes(&bytes).ok()?.current()?;
                Some(RemovedPhoto { path, photo })
            })
            .collect()
    }

    /// Puts a removed photo back: writes `photo` (the removed sidecar, usually with its location
    /// updated) at its live place, moves its version sidecars back from `removed/`, and then
    /// takes the removed copy away, so that it is not offered again. Returns how many version
    /// sidecars came back.
    pub fn restore_photo(
        &self,
        removed: &RemovedPhoto,
        photo: &PhotoSidecar,
    ) -> Result<usize, WorkspaceError> {
        if self.access == Access::ReadOnly {
            return Err(WorkspaceError::ReadOnly);
        }
        self.write_photo(photo)?;
        let mut versions = 0;
        let mut files = Vec::new();
        files_under(
            &self.root.join(layout::REMOVED).join(layout::VERSIONS),
            &mut files,
        );
        let prefix = format!("{}.", photo.photo_id);
        for path in files {
            let belongs = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix));
            if !belongs {
                continue;
            }
            let Some(bytes) = read_capped(&path)? else {
                continue;
            };
            let Ok(loaded) = VersionSidecar::from_bytes(&bytes) else {
                continue;
            };
            if let Some(version) = loaded.current() {
                self.write_version(&version)?;
                fs::remove_file(&path).map_err(io_err(&path))?;
                versions += 1;
            }
        }
        fs::remove_file(&removed.path).map_err(io_err(&removed.path))?;
        Ok(versions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auroraw_testkit::temp_dir;

    #[test]
    fn a_crash_between_the_temporary_file_and_the_rename_loses_nothing() {
        let dir = temp_dir();
        let ws = Workspace::create(&dir.path().join("w"), "Main").unwrap();
        let id = PhotoId::random();
        let mut photo = PhotoSidecar::new(id);
        photo.meta.rating = Some(2);
        ws.write_photo(&photo).unwrap();
        let before = fs::read(ws.photo_path(&id)).unwrap();

        // the process dies after writing the new content to the temporary file
        photo.meta.rating = Some(5);
        let err = ws.write_bytes_with(
            &ws.photo_path(&id),
            &photo.to_bytes(),
            Interrupt::BeforeRename,
        );
        assert!(err.is_err());
        assert_eq!(
            fs::read(ws.photo_path(&id)).unwrap(),
            before,
            "the old file is intact"
        );
        let tmp = ws.root().join(layout::INTERNAL).join(layout::TMP);
        assert_eq!(
            fs::read_dir(&tmp).unwrap().count(),
            1,
            "and a leftover is waiting"
        );

        // the next opening (after the lock is released) removes the leftover and changes nothing else
        let root = ws.root().to_path_buf();
        drop(ws);
        let ws = Workspace::open(&root).unwrap();
        assert_eq!(ws.access(), Access::ReadWrite);
        assert_eq!(fs::read_dir(&tmp).unwrap().count(), 0);
        assert_eq!(fs::read(ws.photo_path(&id)).unwrap(), before);
    }

    #[test]
    fn a_removed_photo_can_be_listed_and_put_back_with_its_versions() {
        let dir = temp_dir();
        let ws = Workspace::create(&dir.path().join("w"), "Main").unwrap();
        let id = PhotoId::random();
        let mut photo = PhotoSidecar::new(id);
        photo.meta.rating = Some(4);
        ws.write_photo(&photo).unwrap();
        let version = VersionSidecar::new(&photo, VersionId::random());
        ws.write_version(&version).unwrap();
        assert!(ws.removed_photos().is_empty());

        ws.remove_recoverably(&ws.photo_path(&id)).unwrap();
        ws.remove_recoverably(&ws.version_path(&id, &version.version_id))
            .unwrap();
        assert!(ws.read_photo(&id).unwrap().is_none());

        let removed = ws.removed_photos();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].photo.meta.rating, Some(4));

        let versions = ws.restore_photo(&removed[0], &removed[0].photo).unwrap();
        assert_eq!(versions, 1);
        assert_eq!(
            ws.read_photo(&id)
                .unwrap()
                .unwrap()
                .current()
                .unwrap()
                .meta
                .rating,
            Some(4)
        );
        assert!(ws.read_version(&id, &version.version_id).unwrap().is_some());
        assert!(ws.removed_photos().is_empty(), "it is not offered again");
    }
}
