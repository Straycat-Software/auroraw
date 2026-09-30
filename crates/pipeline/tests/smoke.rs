// SPDX-License-Identifier: GPL-3.0-or-later
//! The smoke test on every adapter of the machine, through the public interface: what continuous
//! integration runs on the software adapters of its three platforms (blocking), and what
//! `cargo xtask gpu-check` will run on the real ones.
//!
//! On a machine with no graphics adapter these tests skip, except where `AUR_REQUIRE_GPU` is set
//! (continuous integration), where they fail.

use auroraw_pipeline::{
    AdapterChoice, AdapterKind, ChooseError, Config, OpenError, Pipeline, list_adapters,
};

/// Whether the machine has any adapter; fails instead of skipping when one is required.
fn adapters_or_skip() -> bool {
    let any = !list_adapters().is_empty();
    if !any {
        assert!(
            std::env::var_os("AUR_REQUIRE_GPU").is_none(),
            "AUR_REQUIRE_GPU is set but the machine has no graphics adapter"
        );
        eprintln!("skipped: no graphics adapter (set AUR_REQUIRE_GPU=1 to make this a failure)");
    }
    any
}

#[test]
fn the_smoke_test_passes_on_every_adapter() {
    if !adapters_or_skip() {
        return;
    }
    for info in list_adapters() {
        let engine = Pipeline::open(Config {
            adapter: AdapterChoice::Named(format!("{} {}", info.backend, info.name)),
        })
        .unwrap_or_else(|e| panic!("cannot open {}: {e}", info.describe()));
        assert_eq!(engine.adapter().name, info.name);
        let report = engine
            .smoke_test()
            .unwrap_or_else(|e| panic!("smoke test on {}: {e}", info.describe()));
        assert!(
            report.passed(),
            "{} disagrees with the CPU reference: {report:#?}",
            info.describe()
        );
        eprintln!("{}: {:?}", info.describe(), report.shaders);
    }
}

#[test]
fn the_best_adapter_is_a_real_gpu_when_the_machine_has_one() {
    if !adapters_or_skip() {
        return;
    }
    let adapters = list_adapters();
    let engine = Pipeline::open(Config::default()).expect("the engine opens");
    let has_real = adapters
        .iter()
        .any(|a| !matches!(a.kind, AdapterKind::Software | AdapterKind::Virtual));
    if has_real {
        assert!(!matches!(
            engine.adapter().kind,
            AdapterKind::Software | AdapterKind::Virtual
        ));
    }
}

#[test]
fn an_adapter_that_does_not_exist_is_a_typed_error_that_lists_the_ones_that_do() {
    if !adapters_or_skip() {
        return;
    }
    let Err(error) = Pipeline::open(Config {
        adapter: AdapterChoice::Named("no such adapter".into()),
    }) else {
        panic!("an adapter named \"no such adapter\" opened");
    };
    assert!(
        matches!(error, OpenError::Adapter(ChooseError::NoMatch { .. })),
        "{error}"
    );
}
