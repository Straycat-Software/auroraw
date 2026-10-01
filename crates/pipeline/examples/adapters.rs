// SPDX-License-Identifier: GPL-3.0-or-later
//! Lists the graphics adapters of this machine and runs the smoke test on each.
//!
//! `cargo run -p auroraw-pipeline --example adapters`. Continuous integration runs it so that the
//! job log says which adapter each platform's runner actually offered (the tests' own output is
//! hidden when they pass), and it is the seed of `cargo xtask gpu-check` (CI doc §4).

use auroraw_pipeline::{AdapterChoice, Config, Pipeline, list_adapters};

fn main() {
    let adapters = list_adapters();
    if adapters.is_empty() {
        println!("no graphics adapter (the engine needs Vulkan, Metal or DirectX 12)");
        return;
    }
    let mut failed = false;
    for info in &adapters {
        println!("{}", info.describe());
        println!("  driver: {} {}", info.driver, info.driver_info);
        println!("  shaders: {}", info.backend.shader_route());
        println!(
            "  max buffer: {} MiB, max storage binding: {} MiB",
            info.max_buffer_size >> 20,
            info.max_storage_binding_size >> 20
        );
        let engine = Pipeline::open(Config {
            adapter: AdapterChoice::Named(format!("{} {}", info.backend, info.name)),
        });
        match engine.map(|engine| engine.smoke_test()) {
            Ok(Ok(report)) => {
                for shader in &report.shaders {
                    println!(
                        "  smoke {}: {}: {}",
                        shader.shader,
                        shader.describe(),
                        if shader.passed() { "ok" } else { "FAILED" }
                    );
                }
                failed |= !report.passed();
            }
            Ok(Err(error)) => {
                println!("  smoke test failed: {error}");
                failed = true;
            }
            Err(error) => {
                println!("  cannot open: {error}");
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
