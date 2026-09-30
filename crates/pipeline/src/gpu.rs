// SPDX-License-Identifier: GPL-3.0-or-later
//! The device and what the engine does with it that can fail: creating it, allocating on it,
//! reading a result back, and noticing that it is gone (architecture §6.3, §6.6).
//!
//! Running out of memory and losing the device are **ordinary events** here, not panics: a GPU is
//! shared with the desktop, and spike 1 saw the memory a process could allocate go from 1.3 GB to
//! 64 MB and back in one afternoon.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crate::adapter::AdapterInfo;

/// A device on one adapter, with the queue and what is known of the adapter.
pub(crate) struct Gpu {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) info: AdapterInfo,
    lost: Arc<LostState>,
}

/// Why and whether the device is gone, written by wgpu's callback and read by the GPU thread.
#[derive(Default)]
struct LostState {
    lost: AtomicBool,
    reason: Mutex<Option<String>>,
}

/// What can go wrong when allocating or using a buffer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum GpuError {
    /// The adapter has no memory left for this allocation: shrink the work and retry.
    #[error("the graphics memory is exhausted (asked for {size} bytes)")]
    OutOfMemory { size: u64 },
    /// The allocation is larger than the adapter can ever create.
    #[error("{size} bytes exceed the largest buffer this adapter can create ({max})")]
    TooLarge { size: u64, max: u64 },
    /// The graphics API refused the call for a reason of its own.
    #[error("the graphics API refused the call: {0}")]
    Rejected(String),
    /// The device is gone (a driver reset, a removed adapter): create a new one and replay.
    #[error("the graphics device was lost: {0}")]
    DeviceLost(String),
}

impl Gpu {
    /// Requests a device on `adapter`, with the adapter's own limits, and watches for its loss.
    pub(crate) fn create(adapter: &wgpu::Adapter, info: AdapterInfo) -> Result<Gpu, String> {
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("auroraw pipeline"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        let lost = Arc::new(LostState::default());
        let watcher = Arc::clone(&lost);
        device.set_device_lost_callback(move |reason, message| {
            *watcher
                .reason
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(format!("{reason:?}: {message}"));
            watcher.lost.store(true, Ordering::SeqCst);
        });
        Ok(Gpu {
            device,
            queue,
            info,
            lost,
        })
    }

    /// Whether wgpu has reported the device lost.
    pub(crate) fn is_lost(&self) -> bool {
        self.lost.lost.load(Ordering::SeqCst)
    }

    /// The reason wgpu gave for the loss, if the device is lost.
    pub(crate) fn lost_reason(&self) -> Option<String> {
        self.lost
            .reason
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Lets wgpu run its callbacks without waiting for the GPU. The GPU thread calls it after each
    /// job so that a loss is known before the next one starts.
    pub(crate) fn pump(&self) {
        // Polling a lost device is an error we are about to report by other means.
        let _ = self.device.poll(wgpu::PollType::Poll);
    }

    /// Waits until the GPU has finished what was submitted.
    pub(crate) fn wait(&self) -> Result<(), GpuError> {
        let polled = self.device.poll(wgpu::PollType::wait_indefinitely());
        if self.is_lost() {
            return Err(GpuError::DeviceLost(self.lost_reason().unwrap_or_default()));
        }
        polled
            .map(|_| ())
            .map_err(|e| GpuError::Rejected(e.to_string()))
    }

    /// Creates a buffer, turning "out of memory" and every other refusal into an error the caller
    /// can act on (design note 005 §6; architecture §6.3: allocate with error scopes).
    pub(crate) fn create_buffer(
        &self,
        size: u64,
        usage: wgpu::BufferUsages,
    ) -> Result<wgpu::Buffer, GpuError> {
        let max = self.info.max_buffer_size;
        if size > max {
            return Err(GpuError::TooLarge { size, max });
        }
        let validation = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage,
            mapped_at_creation: false,
        });
        // The scopes pop in the reverse order they were pushed.
        let out_of_memory = pollster::block_on(memory.pop());
        let rejected = pollster::block_on(validation.pop());
        if out_of_memory.is_some() {
            return Err(GpuError::OutOfMemory { size });
        }
        if let Some(error) = rejected {
            return Err(GpuError::Rejected(error.to_string()));
        }
        if self.is_lost() {
            return Err(GpuError::DeviceLost(self.lost_reason().unwrap_or_default()));
        }
        Ok(buffer)
    }

    /// A buffer with `bytes` in it, for a shader to read.
    pub(crate) fn upload(
        &self,
        bytes: &[u8],
        usage: wgpu::BufferUsages,
    ) -> Result<wgpu::Buffer, GpuError> {
        // A storage or uniform buffer's size is a multiple of four; pad with zeros.
        let padded = (bytes.len() as u64).div_ceil(4).max(1) * 4;
        let buffer = self.create_buffer(padded, usage | wgpu::BufferUsages::COPY_DST)?;
        self.queue.write_buffer(&buffer, 0, bytes);
        Ok(buffer)
    }

    /// Copies `len` bytes of `buffer` (which must have `COPY_SRC`) back to the CPU.
    pub(crate) fn read_back(&self, buffer: &wgpu::Buffer, len: u64) -> Result<Vec<u8>, GpuError> {
        let staging = self.create_buffer(
            len,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        )?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_buffer_to_buffer(buffer, 0, &staging, 0, Some(len));
        self.queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.wait()?;
        receiver
            .recv()
            .map_err(|_| GpuError::DeviceLost("the read-back was dropped".into()))?
            .map_err(|e| GpuError::Rejected(e.to_string()))?;
        let mapped = staging
            .slice(..)
            .get_mapped_range()
            .map_err(|e| GpuError::Rejected(e.to_string()))?;
        let bytes = mapped.to_vec();
        drop(mapped);
        staging.unmap();
        Ok(bytes)
    }
}
