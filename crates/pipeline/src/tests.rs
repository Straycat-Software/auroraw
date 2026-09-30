// SPDX-License-Identifier: GPL-3.0-or-later
//! Tests of the parts that need a device and the crate's own internals. The public behaviour on
//! every adapter is in `tests/smoke.rs`.

use crate::adapter::{AdapterChoice, AdapterKind, ChooseError};
use crate::gpu::GpuError;
use crate::smoke::SHADERS;
use crate::thread::{Config, OpenError, Pipeline};

/// Opens the engine on the best adapter, or returns `None` on a machine with no adapter at all,
/// which is a skip locally and a failure in continuous integration (`AUR_REQUIRE_GPU=1`), matching
/// `AUR_REQUIRE_EXIFTOOL` and the others.
fn engine() -> Option<Pipeline> {
    match Pipeline::open(Config::default()) {
        Ok(pipeline) => Some(pipeline),
        Err(OpenError::Adapter(ChooseError::NoAdapter)) => {
            assert!(
                std::env::var_os("AUR_REQUIRE_GPU").is_none(),
                "AUR_REQUIRE_GPU is set but the machine has no graphics adapter"
            );
            eprintln!(
                "skipped: no graphics adapter (set AUR_REQUIRE_GPU=1 to make this a failure)"
            );
            None
        }
        Err(other) => panic!("the engine did not open: {other}"),
    }
}

#[test]
fn every_wgsl_file_is_in_the_smoke_test() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shaders");
    let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
        .expect("src/shaders exists")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension()? == "wgsl").then(|| path.file_stem()?.to_str().map(String::from))?
        })
        .collect();
    on_disk.sort();
    let mut registered: Vec<&str> = SHADERS.iter().map(|s| s.name).collect();
    registered.sort_unstable();
    assert_eq!(
        on_disk, registered,
        "a shader in src/shaders/ must be in SHADERS (and have a smoke case), and the reverse"
    );
}

#[test]
fn a_buffer_survives_a_round_trip_through_the_device() {
    let Some(engine) = engine() else { return };
    let bytes: Vec<u8> = (0..=255).collect();
    let back = engine
        .run(move |gpu| {
            let buffer = gpu.upload(
                &bytes,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            )?;
            gpu.read_back(&buffer, bytes.len() as u64)
        })
        .expect("the job ran")
        .expect("the buffer read back");
    assert_eq!(back, (0..=255).collect::<Vec<u8>>());
}

#[test]
fn a_buffer_larger_than_the_adapter_allows_is_an_error_not_a_panic() {
    let Some(engine) = engine() else { return };
    let max = engine.adapter().max_buffer_size;
    let error = engine
        .run(move |gpu| {
            gpu.create_buffer(max + 4, wgpu::BufferUsages::STORAGE)
                .map(|_| ())
        })
        .expect("the job ran")
        .expect_err("more than the adapter's largest buffer");
    assert_eq!(error, GpuError::TooLarge { size: max + 4, max });
    // And the engine is still usable afterwards.
    engine.smoke_test().expect("the smoke test still runs");
}

/// An adapter to lose the device of: the software one, or a virtual GPU (a continuous-integration
/// runner's). On a machine with a real GPU it is `None` locally, so a developer's card is left
/// alone; in continuous integration (`AUR_REQUIRE_GPU`) the runner's adapter is used whatever its
/// kind, because a runner offers nothing else and the test must run on all three platforms.
fn disposable_adapter() -> Option<AdapterChoice> {
    let adapters = crate::adapter::list_adapters();
    if adapters.iter().any(|a| a.kind == AdapterKind::Software) {
        Some(AdapterChoice::Software)
    } else if adapters
        .first()
        .is_some_and(|a| a.kind == AdapterKind::Virtual)
        || (!adapters.is_empty() && std::env::var_os("AUR_REQUIRE_GPU").is_some())
    {
        Some(AdapterChoice::Best)
    } else {
        None
    }
}

#[test]
fn a_lost_device_is_recreated_before_the_next_job() {
    // Injects the loss the way testing strategy §4 asks: on the software adapter, where it is
    // cheap and cannot disturb a real GPU.
    let Some(adapter) = disposable_adapter() else {
        assert!(
            std::env::var_os("AUR_REQUIRE_GPU").is_none(),
            "AUR_REQUIRE_GPU is set but the machine has no adapter at all"
        );
        eprintln!("skipped: no software or virtual adapter to lose a device on");
        return;
    };
    let engine = Pipeline::open(Config { adapter }).expect("the engine opens");
    assert_eq!(engine.generation(), 0);
    engine
        .run(|gpu| gpu.device.destroy())
        .expect("the destroy job ran");
    // The next job finds the loss the callback recorded, and gets a fresh device.
    let report = engine
        .smoke_test()
        .expect("the smoke test runs on the new device");
    assert_eq!(
        engine.generation(),
        1,
        "the device was created a second time"
    );
    assert!(report.passed(), "{report:?}");
    assert_eq!(report.generation, 1);
}
