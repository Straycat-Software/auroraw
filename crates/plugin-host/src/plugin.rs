// SPDX-License-Identifier: GPL-3.0-or-later
//! The wasmtime host itself: loading, instantiating and calling into a plugin through the
//! hand-made C interface (architecture §8.2b): the host reserves memory in the plugin, writes
//! bytes, calls, reads the result back. Productizes spike 4's `plugin-host` crate.

use crate::error::{HostError, Result};
use crate::grants::Grants;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use wasmtime::{
    Config, Engine, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc, WasmParams,
    WasmResults,
};
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::p1::{WasiP1Ctx, add_to_linker_sync};

/// How often the host's background timer advances the epoch (architecture §8.2: "a time budget
/// is enforced by the epoch timer"). A call's own timeout is rounded up to this granularity.
const EPOCH_TICK: Duration = Duration::from_millis(1);

/// The wasmtime engine, a cache of compiled modules, and the background timer every instance's
/// time budget is measured against. One host serves every plugin instance in the process; it is
/// cheap to hand out clones of the engine internally but expensive to create, so applications
/// keep one for their lifetime (architecture §8.2, "a plugin is compiled once").
pub struct PluginHost {
    engine: Engine,
    cache: Mutex<HashMap<blake3::Hash, Module>>,
    ticker_stop: Arc<AtomicBool>,
    ticker: Option<JoinHandle<()>>,
}

/// A module the host has compiled, ready to be started as many times as needed
/// ([`PluginHost::instantiate`]). Compiling is the expensive step (spike 4: 8 ms to 3 s);
/// starting an instance from an already-compiled module is not (spike 4: 0.07 ms).
#[derive(Clone)]
pub struct CompiledPlugin {
    module: Module,
}

struct Ctx {
    wasi: WasiP1Ctx,
    limits: StoreLimits,
}

/// One running plugin: its own memory, its own limits, isolated from every other instance and
/// from the host unless a [`Grants`] said otherwise.
pub struct Instance {
    store: Store<Ctx>,
    instance: wasmtime::Instance,
    memory: Memory,
    alloc_fn: TypedFunc<u32, u32>,
}

impl PluginHost {
    /// Starts the host: a wasmtime engine configured for epoch interruption, and the background
    /// timer that drives it.
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(|e| HostError::Load(e.to_string()))?;
        let ticker_stop = Arc::new(AtomicBool::new(false));
        let ticker = {
            let engine = engine.clone();
            let stop = ticker_stop.clone();
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(EPOCH_TICK);
                    engine.increment_epoch();
                }
            })
        };
        Ok(Self {
            engine,
            cache: Mutex::new(HashMap::new()),
            ticker_stop,
            ticker: Some(ticker),
        })
    }

    /// Compiles `wasm`, or returns the already-compiled module if this host has seen these exact
    /// bytes before (the compiled cache, architecture §8.2; keyed by content, not by a plugin
    /// identifier, so identical bytes are never compiled twice regardless of where they came
    /// from).
    pub fn load(&self, wasm: &[u8]) -> Result<CompiledPlugin> {
        let hash = blake3::hash(wasm);
        let mut cache = self.cache.lock().expect("the module cache is not poisoned");
        if let Some(module) = cache.get(&hash) {
            return Ok(CompiledPlugin {
                module: module.clone(),
            });
        }
        let module =
            Module::from_binary(&self.engine, wasm).map_err(|e| HostError::Load(e.to_string()))?;
        cache.insert(hash, module.clone());
        Ok(CompiledPlugin { module })
    }

    /// Starts a fresh instance of `plugin`, with no access beyond `grants` (architecture §8.2:
    /// "no access to anything unless granted"). Every WASI import required by the module is
    /// wired to an empty context otherwise: no environment, no arguments, no standard streams, no
    /// network, no processes, no threads.
    pub fn instantiate(&self, plugin: &CompiledPlugin, grants: &Grants) -> Result<Instance> {
        let mut wasi = WasiCtxBuilder::new();
        for (i, dir) in grants.read_dirs.iter().enumerate() {
            wasi.preopened_dir(
                dir,
                format!("/granted/{i}"),
                wasmtime_wasi::FsPerms::ReadOnly,
            )
            .map_err(|e| HostError::Instantiate(e.to_string()))?;
        }
        let limits = StoreLimitsBuilder::new()
            .memory_size(grants.memory_limit_or_default())
            .trap_on_grow_failure(true)
            .build();
        let mut store = Store::new(
            &self.engine,
            Ctx {
                wasi: wasi.build_p1(),
                limits,
            },
        );
        store.limiter(|c| &mut c.limits);
        // No call has run yet: give the store a deadline far in the future so instantiation
        // itself (which runs no plugin code, but starts the epoch clock ticking for this store)
        // never traps for want of one (spike 4: "a new store has no fuel and an epoch deadline of
        // zero").
        store.set_epoch_deadline(u64::MAX / 2);
        let mut linker = wasmtime::Linker::new(&self.engine);
        add_to_linker_sync(&mut linker, |c: &mut Ctx| &mut c.wasi)
            .map_err(|e| HostError::Instantiate(e.to_string()))?;
        let instance = linker
            .instantiate(&mut store, &plugin.module)
            .map_err(|e| HostError::Instantiate(e.to_string()))?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| HostError::Instantiate("the module exports no memory".into()))?;
        let alloc_fn = instance
            .get_typed_func::<u32, u32>(&mut store, "alloc")
            .map_err(|e| HostError::Instantiate(format!("no `alloc` export: {e}")))?;
        Ok(Instance {
            store,
            instance,
            memory,
            alloc_fn,
        })
    }
}

impl Drop for PluginHost {
    fn drop(&mut self) {
        self.ticker_stop.store(true, Ordering::Relaxed);
        if let Some(ticker) = self.ticker.take() {
            let _ = ticker.join();
        }
    }
}

impl Instance {
    /// Reserves `n` bytes in the plugin's own memory and returns their address there, so the
    /// host can write into a place the plugin itself chose to allocate (never guessing at an
    /// address inside someone else's sandbox).
    pub fn alloc(&mut self, n: usize) -> Result<u32> {
        self.alloc_fn
            .call(&mut self.store, n as u32)
            .map_err(|e| HostError::Call(e.to_string()))
    }

    /// Writes `bytes` into the plugin's memory at `at` (from a prior [`Self::alloc`]).
    pub fn write(&mut self, at: u32, bytes: &[u8]) -> Result<()> {
        self.memory
            .write(&mut self.store, at as usize, bytes)
            .map_err(|e| HostError::Call(e.to_string()))
    }

    /// Reads `out.len()` bytes from the plugin's memory at `at`.
    pub fn read(&mut self, at: u32, out: &mut [u8]) -> Result<()> {
        self.memory
            .read(&self.store, at as usize, out)
            .map_err(|e| HostError::Call(e.to_string()))
    }

    /// Calls the export named `name`, interrupting it if it runs longer than `timeout`
    /// (architecture §8.2). A call that traps, that is interrupted, or that exceeds a memory
    /// limit fails with [`HostError::Call`]: this instance must not be called again afterwards
    /// (its state after a trap is undefined), but the host and every other instance are
    /// unaffected (spike 4: "host alive" in every hostile case).
    pub fn call<P, R>(&mut self, name: &str, params: P, timeout: Duration) -> Result<R>
    where
        P: WasmParams,
        R: WasmResults,
    {
        let ticks = (timeout.as_secs_f64() / EPOCH_TICK.as_secs_f64())
            .ceil()
            .max(1.0) as u64;
        self.store.set_epoch_deadline(ticks);
        let func = self
            .instance
            .get_typed_func::<P, R>(&mut self.store, name)
            .map_err(|e| HostError::Call(format!("no `{name}` export: {e}")))?;
        // `{:#}`: the trap's reason ("interrupt", "out of bounds memory access", ...) is the error's cause, and
        // the plain text is only the backtrace, which cannot tell a timeout from a crash.
        func.call(&mut self.store, params)
            .map_err(|e| HostError::Call(format!("{e:#}")))
    }
}
