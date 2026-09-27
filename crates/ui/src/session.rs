// SPDX-License-Identifier: GPL-3.0-or-later
//! What the Qt objects share: the open workspace's engine, and the collector the image provider waits
//! on. (The provider runs on Qt's image loading threads, outside every QObject.)

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use auroraw_engine::{Engine, MaskKind, PreviewService, ThumbnailService};
use auroraw_types::PhotoId;

/// An open workspace. It is dropped as soon as another opens, which releases the workspace's folder.
pub struct Session {
    pub engine: Engine,
    /// This machine's folder for what the workspace remembers (the import form, resumable imports).
    pub data_dir: std::path::PathBuf,
    pub thumbs: Collector<ThumbnailService>,
    /// The pictures of the image view (WP9): big, few, kept by the service itself.
    pub previews: Collector<PreviewService>,
}

static CURRENT: Mutex<Option<Arc<Session>>> = Mutex::new(None);

pub fn set_current(session: Option<Arc<Session>>) {
    *CURRENT.lock().unwrap() = session;
}

/// Lets go of the current session if it is `session` (the one a launcher that is being destroyed
/// opened; a newer launcher's own session stays).
pub fn clear_if_current(session: &Weak<Session>) {
    let mut current = CURRENT.lock().unwrap();
    if current
        .as_ref()
        .is_some_and(|c| std::ptr::eq(Arc::as_ptr(c), session.as_ptr()))
    {
        *current = None;
    }
}

pub fn current() -> Option<Arc<Session>> {
    CURRENT.lock().unwrap().clone()
}

#[derive(Default)]
struct Done {
    ready: HashMap<PhotoId, Vec<u8>>,
    /// Photos no thumbnail can be made for: not asked for again until the photo changes.
    failed: HashSet<PhotoId>,
    /// The requests (tokens of the image provider's responses) waiting for a photo's image (aspect 0) or one of
    /// its overlays (1 peaking, 2 clipping).
    waiting: HashMap<(PhotoId, u8), Vec<u64>>,
}

/// What a [`Collector`] needs of the service it polls: an image service that answers asynchronously. A photo has
/// an image (aspect 0) and, for the image view's service, two overlays (aspect 1 the focus peaking, 2 the clipping).
pub trait ImageSource: Send + Sync + 'static {
    /// Asks for one aspect of the photo's image.
    fn ask(&self, id: PhotoId, aspect: u8);
    /// The images that arrived since the last call, as bytes (JPEG, or PNG for an overlay).
    fn arrived(&self) -> Vec<(PhotoId, u8, Vec<u8>)>;
    /// The photos (and aspects) that cannot be made since the last call.
    fn given_up(&self) -> Vec<(PhotoId, u8)>;
    /// Whether the collector keeps what arrived (and what failed) to answer a later request itself: thumbnails
    /// are small and asked for again and again as a grid scrolls, the pictures of the view are neither, and
    /// the service keeps its own few.
    const KEEPS: bool;
}

impl ImageSource for ThumbnailService {
    fn ask(&self, id: PhotoId, _aspect: u8) {
        self.request(id)
    }
    fn arrived(&self) -> Vec<(PhotoId, u8, Vec<u8>)> {
        self.poll()
            .into_iter()
            .map(|(id, t)| (id, 0, t.jpeg))
            .collect()
    }
    fn given_up(&self) -> Vec<(PhotoId, u8)> {
        self.poll_failed().into_iter().map(|id| (id, 0)).collect()
    }
    const KEEPS: bool = true;
}

impl ImageSource for PreviewService {
    fn ask(&self, id: PhotoId, aspect: u8) {
        match aspect {
            1 => self.request_mask(id, MaskKind::Peaking),
            2 => self.request_mask(id, MaskKind::Clipping),
            _ => self.request(id),
        }
    }
    fn arrived(&self) -> Vec<(PhotoId, u8, Vec<u8>)> {
        let mut out: Vec<(PhotoId, u8, Vec<u8>)> = self
            .poll()
            .into_iter()
            .map(|(id, picture)| (id, 0, picture.image.jpeg.clone()))
            .collect();
        out.extend(self.poll_masks().into_iter().map(|(id, kind, png)| {
            let aspect = if kind == MaskKind::Peaking { 1 } else { 2 };
            (id, aspect, png.as_ref().clone())
        }));
        out
    }
    fn given_up(&self) -> Vec<(PhotoId, u8)> {
        let mut out: Vec<(PhotoId, u8)> =
            self.poll_failed().into_iter().map(|id| (id, 0)).collect();
        out.extend(
            self.poll_masks_failed()
                .into_iter()
                .map(|(id, kind)| (id, if kind == MaskKind::Peaking { 1 } else { 2 })),
        );
        out
    }
    const KEEPS: bool = false;
}

/// Turns a service's poll-based delivery into an answer for each request: a thread polls the service and
/// hands every waiting request its image (or its failure) through `deliver`.
pub struct Collector<S: ImageSource> {
    service: Arc<S>,
    done: Arc<Mutex<Done>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    deliver: Deliver,
}

/// Answers one request: its token, and the JPEG of the thumbnail or `None` when there is none.
pub type Deliver = Arc<dyn Fn(u64, Option<&[u8]>) + Send + Sync>;

impl<S: ImageSource> Collector<S> {
    pub fn new(service: S, deliver: Deliver) -> Self {
        let service = Arc::new(service);
        let done: Arc<Mutex<Done>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (service, done, stop, deliver) =
                (service.clone(), done.clone(), stop.clone(), deliver.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let arrived = service.arrived();
                    let failed = service.given_up();
                    if !arrived.is_empty() || !failed.is_empty() {
                        let mut answers: Vec<(u64, Option<Vec<u8>>)> = Vec::new();
                        {
                            let mut state = done.lock().unwrap();
                            if state.ready.len() > 4096 {
                                state.ready.clear();
                            }
                            for (id, aspect, bytes) in arrived {
                                for token in state.waiting.remove(&(id, aspect)).unwrap_or_default()
                                {
                                    answers.push((token, Some(bytes.clone())));
                                }
                                if S::KEEPS {
                                    state.ready.insert(id, bytes);
                                }
                            }
                            for (id, aspect) in failed {
                                for token in state.waiting.remove(&(id, aspect)).unwrap_or_default()
                                {
                                    answers.push((token, None));
                                }
                                if S::KEEPS {
                                    state.failed.insert(id);
                                }
                            }
                        }
                        for (token, jpeg) in answers {
                            deliver(token, jpeg.as_deref());
                        }
                    }
                    std::thread::sleep(Duration::from_millis(4));
                }
            })
        };
        Self {
            service,
            done,
            stop,
            thread: Some(thread),
            deliver,
        }
    }

    /// Asks for a photo's image (`aspect` 0) or overlay on behalf of the request `token`, answered through the
    /// collector's `deliver` (from this call, when it is already known, or later from the collector's thread).
    pub fn request(&self, id: PhotoId, aspect: u8, token: u64) {
        let known = {
            let mut state = self.done.lock().unwrap();
            if let Some(bytes) = state.ready.get(&id).filter(|_| aspect == 0) {
                Some(Some(bytes.clone()))
            } else if S::KEEPS && state.failed.contains(&id) {
                Some(None)
            } else {
                state.waiting.entry((id, aspect)).or_default().push(token);
                None
            }
        };
        match known {
            Some(jpeg) => (self.deliver)(token, jpeg.as_deref()),
            None => self.service.ask(id, aspect),
        }
    }

    /// The service the collector polls (for what is not an image: measures, marks, the order to prefetch).
    pub fn service(&self) -> &S {
        &self.service
    }
}

impl<S: ImageSource> Drop for Collector<S> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Collector<ThumbnailService> {
    /// Makes a photo's thumbnail ahead of anybody asking for it (a photo that just entered the
    /// catalogue), behind whatever the grid asks for. A photo that changed may have a thumbnail
    /// now, so an earlier failure is forgotten.
    pub fn warm(&self, id: PhotoId) {
        self.done.lock().unwrap().failed.remove(&id);
        self.service.warm(id);
    }
}

impl Collector<PreviewService> {
    /// Says which photos the image view will want next (see `PreviewService::prefetch`).
    pub fn prefetch(&self, ids: &[PhotoId]) {
        self.service.prefetch(ids);
    }
}
