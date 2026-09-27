// SPDX-License-Identifier: GPL-3.0-or-later
//! The image view's service (WP9, D-100): the photo on screen at a size worth looking at, and the
//! photos around it made ahead of time, so that a key press finds its image ready. Shaped like
//! [`crate::ThumbnailService`] (worker threads with their own connection to the catalogue, nothing
//! that touches the coordinator's), but the pictures are big and few: they are kept in a small
//! in-memory LRU of encoded JPEGs (8 by default) instead of a database.
//!
//! Each picture comes with what was measured on it and the masks that lie over it (D-103: sharpness, histogram,
//! clipping; peaking and clipping masks). The sharpness of a photo is also kept in a small table that outlives the
//! picture, so that the frames of a series can be ranked ([`PreviewService::analyse`]) without keeping them.
//!
//! A person walking through a folder asks for the photo on screen ([`PreviewService::request`],
//! answered by [`PreviewService::poll`]) and says which photos are next ([`PreviewService::prefetch`],
//! which replaces the previous list: photos the view has left behind are no longer worth making).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use auroraw_catalogue::Catalogue;
use auroraw_imaging::{
    Aids, MaskKind, VIEW_MAX_EDGE, ViewPicture, mask_png, measure_file, view_picture,
};
use auroraw_types::PhotoId;
use auroraw_workspace::Workspace;

use crate::error::Result;
use crate::thumbnails::source_root;

/// How many pictures are kept.
pub const DEFAULT_CAPACITY: usize = 8;

/// The pictures made, the most recently used last.
struct Cache {
    capacity: usize,
    order: VecDeque<PhotoId>,
    images: HashMap<PhotoId, Arc<ViewPicture>>,
}

impl Cache {
    fn get(&mut self, id: &PhotoId) -> Option<Arc<ViewPicture>> {
        let image = self.images.get(id)?.clone();
        self.order.retain(|other| other != id);
        self.order.push_back(*id);
        Some(image)
    }

    fn put(&mut self, id: PhotoId, image: Arc<ViewPicture>) {
        self.order.retain(|other| *other != id);
        self.order.push_back(id);
        self.images.insert(id, image);
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.images.remove(&oldest);
            }
        }
    }
}

/// What waits for a worker, and what somebody is waiting for.
#[derive(Default)]
struct Queues {
    /// Asked for by [`PreviewService::request`], the newest first.
    urgent: Vec<PhotoId>,
    /// Given by [`PreviewService::prefetch`], in the order they will be needed.
    warm: VecDeque<PhotoId>,
    /// Given by [`PreviewService::request_mask`]: overlays somebody is waiting for.
    masks: VecDeque<(PhotoId, MaskKind)>,
    /// Given by [`PreviewService::analyse`]: photos whose sharpness is wanted, made when nothing else waits.
    measure: VecDeque<PhotoId>,
    /// A worker is making these.
    active: HashSet<PhotoId>,
    /// Asked for and not yet delivered (or reported failed).
    wanted: HashSet<PhotoId>,
}

#[derive(Default)]
struct Inbox {
    ready: Vec<(PhotoId, Arc<ViewPicture>)>,
    failed: Vec<PhotoId>,
}

struct Shared {
    queue: Mutex<Queues>,
    cv: Condvar,
    stop: AtomicBool,
    cache: Mutex<Cache>,
    inbox: Mutex<Inbox>,
    /// The sharpness of every photo measured so far (4 bytes a photo).
    scores: Mutex<HashMap<PhotoId, f32>>,
    /// The overlays made (a few, most recent last), and what is ready to be collected.
    mask_cache: Mutex<MaskCache>,
    mask_inbox: Mutex<MaskInbox>,
}

/// How many overlays are kept (they are small: mostly transparent).
const MASK_CAPACITY: usize = 24;

#[derive(Default)]
struct MaskCache {
    order: VecDeque<(PhotoId, MaskKind)>,
    masks: HashMap<(PhotoId, MaskKind), Arc<Vec<u8>>>,
}

#[derive(Default)]
struct MaskInbox {
    ready: Vec<(PhotoId, MaskKind, Arc<Vec<u8>>)>,
    failed: Vec<(PhotoId, MaskKind)>,
}

/// A background service that turns a photo id into the picture the image view shows.
pub struct PreviewService {
    shared: Arc<Shared>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl PreviewService {
    /// Starts `workers` worker threads (two are enough: the photo on screen and the next one) keeping
    /// `capacity` pictures.
    pub fn start(
        workspace: Arc<Workspace>,
        catalogue_path: PathBuf,
        workers: usize,
        capacity: usize,
    ) -> Result<Self> {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queues::default()),
            cv: Condvar::new(),
            stop: AtomicBool::new(false),
            cache: Mutex::new(Cache {
                capacity: capacity.max(1),
                order: VecDeque::new(),
                images: HashMap::new(),
            }),
            inbox: Mutex::new(Inbox::default()),
            scores: Mutex::new(HashMap::new()),
            mask_cache: Mutex::new(MaskCache::default()),
            mask_inbox: Mutex::new(MaskInbox::default()),
        });
        let handles = (0..workers.max(1))
            .map(|_| {
                let shared = shared.clone();
                let workspace = workspace.clone();
                let catalogue_path = catalogue_path.clone();
                std::thread::spawn(move || worker(shared, workspace, catalogue_path))
            })
            .collect();
        Ok(Self {
            shared,
            workers: handles,
        })
    }

    /// Asks for `id`'s picture: it is delivered by [`Self::poll`] at once when it is already made, else when a
    /// worker has made it (before any photo that was only asked for ahead of time), and reported by
    /// [`Self::poll_failed`] when it cannot be made. Asking again before it arrived asks nothing more.
    pub fn request(&self, id: PhotoId) {
        let cached = self.shared.cache.lock().expect("not poisoned").get(&id);
        if let Some(image) = cached {
            self.shared
                .inbox
                .lock()
                .expect("not poisoned")
                .ready
                .push((id, image));
            return;
        }
        let mut queue = self.shared.queue.lock().expect("not poisoned");
        if !queue.wanted.insert(id) {
            return;
        }
        // A worker already making it (it was asked for ahead of time) delivers it when done.
        if queue.active.contains(&id) {
            return;
        }
        // One that finished between the look above and the lock is there now.
        let cached = self.shared.cache.lock().expect("not poisoned").get(&id);
        if let Some(image) = cached {
            queue.wanted.remove(&id);
            drop(queue);
            self.shared
                .inbox
                .lock()
                .expect("not poisoned")
                .ready
                .push((id, image));
            return;
        }
        queue.warm.retain(|other| *other != id);
        queue.urgent.push(id);
        self.shared.cv.notify_one();
    }

    /// Says which photos the view will want next, the most likely first; they are made when no worker has a
    /// photo asked for. The previous list is forgotten.
    pub fn prefetch(&self, ids: &[PhotoId]) {
        let made: HashSet<PhotoId> = {
            let cache = self.shared.cache.lock().expect("not poisoned");
            ids.iter()
                .filter(|id| cache.images.contains_key(id))
                .copied()
                .collect()
        };
        let mut queue = self.shared.queue.lock().expect("not poisoned");
        queue.warm = ids
            .iter()
            .filter(|id| {
                !made.contains(id) && !queue.active.contains(id) && !queue.urgent.contains(id)
            })
            .copied()
            .collect();
        drop(queue);
        self.shared.cv.notify_all();
    }

    /// Asks for the sharpness of these photos to be measured (the frames of a series about to be compared), after
    /// everything that is more urgent, without keeping their pictures.
    pub fn analyse(&self, ids: &[PhotoId]) {
        let known = self.shared.scores.lock().expect("not poisoned");
        let mut queue = self.shared.queue.lock().expect("not poisoned");
        for id in ids {
            if !known.contains_key(id) && !queue.measure.contains(id) {
                queue.measure.push_back(*id);
            }
        }
        drop(queue);
        drop(known);
        self.shared.cv.notify_all();
    }

    /// Asks for an overlay of a photo's picture (the focus peaking or the clipping): made from the picture (which is
    /// made first when it is not kept) and delivered by [`Self::poll_masks`], at once when it was made before.
    pub fn request_mask(&self, id: PhotoId, kind: MaskKind) {
        let cached = self
            .shared
            .mask_cache
            .lock()
            .expect("not poisoned")
            .masks
            .get(&(id, kind))
            .cloned();
        if let Some(png) = cached {
            self.shared
                .mask_inbox
                .lock()
                .expect("not poisoned")
                .ready
                .push((id, kind, png));
            return;
        }
        let mut queue = self.shared.queue.lock().expect("not poisoned");
        if !queue.masks.contains(&(id, kind)) {
            queue.masks.push_back((id, kind));
        }
        drop(queue);
        self.shared.cv.notify_one();
    }

    /// Every overlay made since the last call, without blocking.
    pub fn poll_masks(&self) -> Vec<(PhotoId, MaskKind, Arc<Vec<u8>>)> {
        std::mem::take(&mut self.shared.mask_inbox.lock().expect("not poisoned").ready)
    }

    /// Every overlay that could not be made since the last call (the picture could not).
    pub fn poll_masks_failed(&self) -> Vec<(PhotoId, MaskKind)> {
        std::mem::take(&mut self.shared.mask_inbox.lock().expect("not poisoned").failed)
    }

    /// What was measured on a photo's picture, when the picture is kept.
    pub fn aids(&self, id: &PhotoId) -> Option<Aids> {
        self.shared
            .cache
            .lock()
            .expect("not poisoned")
            .images
            .get(id)
            .map(|picture| picture.aids.clone())
    }

    /// A photo's sharpness, when it has been measured (with its picture or by [`Self::analyse`]).
    pub fn sharpness(&self, id: &PhotoId) -> Option<f32> {
        self.shared
            .scores
            .lock()
            .expect("not poisoned")
            .get(id)
            .copied()
    }

    /// Every picture a worker (or the cache) has given since the last call, without blocking.
    pub fn poll(&self) -> Vec<(PhotoId, Arc<ViewPicture>)> {
        std::mem::take(&mut self.shared.inbox.lock().expect("not poisoned").ready)
    }

    /// Every photo a worker gave up on since the last call (the original is not there, or unreadable).
    pub fn poll_failed(&self) -> Vec<PhotoId> {
        std::mem::take(&mut self.shared.inbox.lock().expect("not poisoned").failed)
    }

    /// Whether `id`'s picture is made and kept (a request would be answered at once).
    pub fn is_ready(&self, id: &PhotoId) -> bool {
        self.shared
            .cache
            .lock()
            .expect("not poisoned")
            .images
            .contains_key(id)
    }

    /// How many pictures are kept.
    pub fn kept(&self) -> usize {
        self.shared.cache.lock().expect("not poisoned").images.len()
    }
}

impl Drop for PreviewService {
    fn drop(&mut self) {
        // Under the queue's lock, as `ThumbnailService` does: a worker about to wait must not miss it.
        {
            let _queue = self.shared.queue.lock().expect("not poisoned");
            self.shared.stop.store(true, Ordering::Relaxed);
        }
        self.shared.cv.notify_all();
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}

fn worker(shared: Arc<Shared>, workspace: Arc<Workspace>, catalogue_path: PathBuf) {
    let Ok(catalogue) = Catalogue::open(&catalogue_path) else {
        return;
    };
    loop {
        let id = {
            let mut queue = shared.queue.lock().expect("not poisoned");
            loop {
                if shared.stop.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(id) = queue.urgent.pop() {
                    queue.active.insert(id);
                    break id;
                }
                // An overlay somebody waits for comes before the pictures made ahead.
                if let Some((id, kind)) = queue.masks.pop_front() {
                    drop(queue);
                    mask_job(&shared, &catalogue, &workspace, id, kind);
                    queue = shared.queue.lock().expect("not poisoned");
                    continue;
                }
                if let Some(id) = queue.warm.pop_front() {
                    queue.active.insert(id);
                    break id;
                }
                if let Some(id) = queue.measure.pop_front() {
                    // Only a measure: no picture is kept, and nobody waits for it.
                    drop(queue);
                    measure(&shared, &catalogue, &workspace, id);
                    queue = shared.queue.lock().expect("not poisoned");
                    continue;
                }
                queue = shared.cv.wait(queue).expect("not poisoned");
            }
        };
        let made = generate(&catalogue, &workspace, id);
        if let Some(image) = &made {
            shared
                .scores
                .lock()
                .expect("not poisoned")
                .insert(id, image.aids.sharpness);
            shared
                .cache
                .lock()
                .expect("not poisoned")
                .put(id, image.clone());
        }
        let wanted = {
            let mut queue = shared.queue.lock().expect("not poisoned");
            queue.active.remove(&id);
            queue.wanted.remove(&id)
        };
        if wanted {
            let mut inbox = shared.inbox.lock().expect("not poisoned");
            match made {
                Some(image) => inbox.ready.push((id, image)),
                None => inbox.failed.push(id),
            }
        }
    }
}

/// The original's path and its orientation, from the catalogue and the photo's sidecar.
fn original_of(
    catalogue: &Catalogue,
    workspace: &Workspace,
    id: PhotoId,
) -> Option<(PathBuf, Option<u32>)> {
    let row = catalogue.photo(&id).ok().flatten()?;
    let root = source_root(workspace, row.source_id?)?;
    let full = root.join(row.path?.replace('/', std::path::MAIN_SEPARATOR_STR));
    let orientation = workspace
        .read_photo(&id)
        .ok()
        .flatten()
        .and_then(|loaded| loaded.current())
        .and_then(|photo| photo.meta.original.orientation);
    Some((full, orientation))
}

fn generate(catalogue: &Catalogue, workspace: &Workspace, id: PhotoId) -> Option<Arc<ViewPicture>> {
    let (full, orientation) = original_of(catalogue, workspace, id)?;
    view_picture(&full, orientation, VIEW_MAX_EDGE)
        .ok()
        .map(Arc::new)
}

/// Makes one overlay: from the kept picture, or from one made for the purpose.
fn mask_job(
    shared: &Shared,
    catalogue: &Catalogue,
    workspace: &Workspace,
    id: PhotoId,
    kind: MaskKind,
) {
    let kept = shared.cache.lock().expect("not poisoned").get(&id);
    let picture = kept.or_else(|| {
        let made = generate(catalogue, workspace, id)?;
        shared
            .scores
            .lock()
            .expect("not poisoned")
            .insert(id, made.aids.sharpness);
        shared
            .cache
            .lock()
            .expect("not poisoned")
            .put(id, made.clone());
        Some(made)
    });
    let png = picture.and_then(|p| mask_png(&p.image.jpeg, kind).ok());
    match png {
        Some(png) => {
            let png = Arc::new(png);
            {
                let mut cache = shared.mask_cache.lock().expect("not poisoned");
                cache.order.retain(|k| *k != (id, kind));
                cache.order.push_back((id, kind));
                cache.masks.insert((id, kind), png.clone());
                while cache.order.len() > MASK_CAPACITY {
                    if let Some(oldest) = cache.order.pop_front() {
                        cache.masks.remove(&oldest);
                    }
                }
            }
            shared
                .mask_inbox
                .lock()
                .expect("not poisoned")
                .ready
                .push((id, kind, png));
        }
        None => shared
            .mask_inbox
            .lock()
            .expect("not poisoned")
            .failed
            .push((id, kind)),
    }
}

fn measure(shared: &Shared, catalogue: &Catalogue, workspace: &Workspace, id: PhotoId) {
    if shared
        .scores
        .lock()
        .expect("not poisoned")
        .contains_key(&id)
    {
        return;
    }
    if let Some((full, _)) = original_of(catalogue, workspace, id)
        && let Ok(aids) = measure_file(&full)
    {
        shared
            .scores
            .lock()
            .expect("not poisoned")
            .insert(id, aids.sharpness);
    }
}
