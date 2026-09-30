// SPDX-License-Identifier: GPL-3.0-or-later
//! The GPU thread: the one owner of the device, the queue and, later, the stage caches and the
//! memory budget (architecture §4.2). It is not Qt's thread and not the engine coordinator's.
//!
//! Work reaches it as jobs on a channel (no async runtime, architecture §3.2 rule 7). After each
//! job it checks whether the device was lost, and **re-creates it before the next one**
//! (architecture §6.6). Replaying a request that was in flight is the render service's job, which
//! comes with render requests; here a job that meets a lost device simply reports it.

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use crate::adapter::{AdapterChoice, AdapterInfo, ChooseError, choose, enumerate};
use crate::gpu::{EngineLimits, Gpu};

thread_local! {
    /// Whether this thread is the GPU thread, so that a job calling back into the engine is
    /// refused instead of waiting for the thread it is running on.
    static ON_GPU_THREAD: Cell<bool> = const { Cell::new(false) };
}

/// How the engine is set up: the setting of architecture §6.6.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Which adapter to run on.
    pub adapter: AdapterChoice,
}

/// Why the engine could not start.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpenError {
    /// No adapter could be chosen: there is none at all, or the setting names none that exists.
    #[error(transparent)]
    Adapter(#[from] ChooseError),
    /// The adapter was chosen but would not give a device.
    #[error("cannot create a device on {adapter}: {reason}")]
    Device {
        /// The adapter that refused, as [`AdapterInfo::describe`] gives it.
        adapter: String,
        /// What the graphics API said.
        reason: String,
    },
    /// The GPU thread could not be started.
    #[error("the GPU thread could not be started: {0}")]
    Thread(String),
}

/// Why a job could not run on the GPU thread.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum RunError {
    /// The device was lost and a new one could not be created.
    #[error("the graphics device was lost and could not be re-created: {0}")]
    DeviceLost(String),
    /// The GPU thread has stopped.
    #[error("the GPU thread has stopped")]
    ThreadGone,
    /// The job panicked. The GPU thread survives it and re-creates the device before the next job,
    /// since the panic may have left the device's error scopes unbalanced.
    #[error("the job panicked: {0}")]
    Panicked(String),
    /// The job called the engine from the GPU thread, which would wait for itself.
    #[error("a job cannot run another job: it would wait for the GPU thread it is running on")]
    Reentrant,
}

type Job = Box<dyn FnOnce(Result<&Gpu, RunError>) + Send>;

/// What the handle and the thread both see.
struct Shared {
    adapter: Mutex<AdapterInfo>,
    /// The limits the current device was created with.
    limits: Mutex<EngineLimits>,
    /// How many times the device was re-created since the engine started.
    generation: AtomicU64,
    /// Set when a job panicked: the next job gets a fresh device.
    tainted: AtomicBool,
}

/// The image engine's handle. It owns the GPU thread and ends it when dropped.
///
/// Cheap to keep and to call from any thread; every call that touches the GPU is a message to the
/// GPU thread.
pub struct Pipeline {
    jobs: Option<Sender<Job>>,
    thread: Option<JoinHandle<()>>,
    shared: Arc<Shared>,
}

impl Pipeline {
    /// Starts the GPU thread on the adapter `config` chooses and waits until its device exists.
    ///
    /// Returns [`OpenError::Adapter`] (with [`ChooseError::NoAdapter`]) when the machine has no
    /// graphics adapter at all, so the caller can say so instead of failing later.
    pub fn open(config: Config) -> Result<Pipeline, OpenError> {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<Arc<Shared>, OpenError>>();
        // The device is created on the GPU thread, so every wgpu call stays on it.
        let choice = config.adapter;
        let thread = std::thread::Builder::new()
            .name("auroraw-gpu".into())
            .spawn(move || run_thread(&choice, &inbox, &ready_tx))
            .map_err(|e| OpenError::Thread(e.to_string()))?;
        let shared = ready_rx
            .recv()
            .map_err(|_| OpenError::Thread("it stopped before its device existed".into()))??;
        Ok(Pipeline {
            jobs: Some(jobs),
            thread: Some(thread),
            shared,
        })
    }

    /// The adapter the engine is running on now.
    pub fn adapter(&self) -> AdapterInfo {
        self.shared
            .adapter
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The limits the engine runs with on this adapter (its floor, [`EngineLimits::FLOOR`]), which a
    /// stage may rely on.
    pub fn limits(&self) -> EngineLimits {
        *self
            .shared
            .limits
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// How many times the device has been re-created (after a loss, or after a job panicked) since
    /// the engine started; `0` means never.
    pub fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::SeqCst)
    }

    /// Runs `f` on the GPU thread with the device and waits for its result.
    ///
    /// A panic in `f` comes back as [`RunError::Panicked`] and the GPU thread carries on. Calling
    /// this from inside a job is refused ([`RunError::Reentrant`]); so is dropping the last handle
    /// there, which would ask the thread to join itself, and the thread is left to end on its own.
    pub(crate) fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Gpu) -> T + Send + 'static,
    ) -> Result<T, RunError> {
        if ON_GPU_THREAD.get() {
            return Err(RunError::Reentrant);
        }
        let (reply, result) = mpsc::channel();
        let shared = Arc::clone(&self.shared);
        let job: Job = Box::new(move |gpu| {
            let outcome = gpu.and_then(|gpu| {
                catch_unwind(AssertUnwindSafe(|| f(gpu))).map_err(|payload| {
                    shared.tainted.store(true, Ordering::SeqCst);
                    RunError::Panicked(panic_message(payload.as_ref()))
                })
            });
            let _ = reply.send(outcome);
        });
        self.jobs
            .as_ref()
            .ok_or(RunError::ThreadGone)?
            .send(job)
            .map_err(|_| RunError::ThreadGone)?;
        result.recv().map_err(|_| RunError::ThreadGone)?
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        // Closing the channel ends the thread's loop.
        self.jobs = None;
        if let Some(thread) = self.thread.take() {
            // The last handle dropped by a job would join the thread it is on: let it end alone.
            // (Not exercised by a test: `run` blocks, so no job can hold the last handle yet; the
            // render service's fire-and-forget requests will.)
            if !ON_GPU_THREAD.get() {
                let _ = thread.join();
            }
        }
    }
}

/// The text of a panic, for [`RunError::Panicked`].
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(ToString::to_string)
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "without a message".to_string())
}

/// Creates a device on the adapter `choice` names.
fn create(choice: &AdapterChoice) -> Result<Gpu, OpenError> {
    let adapters = enumerate();
    let infos: Vec<AdapterInfo> = adapters.iter().map(|(_, info)| info.clone()).collect();
    let index = choose(&infos, choice)?;
    let (adapter, info) = &adapters[index];
    Gpu::create(adapter, info.clone()).map_err(|reason| OpenError::Device {
        adapter: info.describe(),
        reason,
    })
}

/// The GPU thread's body.
fn run_thread(
    choice: &AdapterChoice,
    inbox: &Receiver<Job>,
    ready: &Sender<Result<Arc<Shared>, OpenError>>,
) {
    let mut gpu = match create(choice) {
        Ok(gpu) => gpu,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    ON_GPU_THREAD.set(true);
    let shared = Arc::new(Shared {
        adapter: Mutex::new(gpu.info.clone()),
        limits: Mutex::new(gpu.limits),
        generation: AtomicU64::new(0),
        tainted: AtomicBool::new(false),
    });
    let _ = ready.send(Ok(Arc::clone(&shared)));

    // Why the last attempt to replace a lost device failed, if it did.
    let mut cannot_recreate: Option<String> = None;
    while let Ok(job) = inbox.recv() {
        if gpu.is_lost() || cannot_recreate.is_some() || shared.tainted.load(Ordering::SeqCst) {
            match create(choice) {
                Ok(fresh) => {
                    *shared
                        .adapter
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = fresh.info.clone();
                    *shared.limits.lock().unwrap_or_else(PoisonError::into_inner) = fresh.limits;
                    gpu = fresh;
                    shared.generation.fetch_add(1, Ordering::SeqCst);
                    shared.tainted.store(false, Ordering::SeqCst);
                    cannot_recreate = None;
                }
                Err(error) => cannot_recreate = Some(error.to_string()),
            }
        }
        match &cannot_recreate {
            Some(reason) => job(Err(RunError::DeviceLost(reason.clone()))),
            None => {
                job(Ok(&gpu));
                gpu.pump();
            }
        }
    }
}
