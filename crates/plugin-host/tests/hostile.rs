// SPDX-License-Identifier: GPL-3.0-or-later
//! The hostile-plugin tests of spike 4, made permanent (testing strategy, `plugin-host`: "the
//! hostile cases of spike 4 become permanent tests, on all three platforms: panic, endless loop,
//! memory bomb, deep recursion, denied file, path traversal (`../`, absolute paths, Windows drive
//! and UNC forms, symbolic links), network, process, thread, environment. After each case, a
//! normal plugin runs to prove the host is unharmed."
//!
//! Skipped locally when `plugins/hostile` has not been built (`tools/build-plugins.sh`); required
//! in CI (`AUR_REQUIRE_PLUGINS=1`).

use auroraw_plugin_host::{Grants, PluginHost};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn plugin_wasm() -> Option<Vec<u8>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins/target/wasm32-wasip1/release/hostile.wasm");
    if path.is_file() {
        return Some(std::fs::read(&path).expect("read the compiled plugin"));
    }
    if std::env::var_os("AUR_REQUIRE_PLUGINS").is_some() {
        panic!("hostile.wasm is required here but was not built; run tools/build-plugins.sh");
    }
    eprintln!("hostile.wasm not found: skipping (run tools/build-plugins.sh)");
    None
}

/// Proof the host is still fine (spike 4: "host alive: yes" after every hostile case): a fresh
/// instance of the same module still starts and answers a harmless call correctly.
fn host_alive(host: &PluginHost, wasm: &[u8]) -> bool {
    let plugin = host.load(wasm).unwrap();
    let mut instance = host.instantiate(&plugin, &Grants::default()).unwrap();
    let clock: i64 = instance
        .call("do_clock_ms", (), Duration::from_secs(5))
        .unwrap_or(-1);
    clock > 0
}

fn put(instance: &mut auroraw_plugin_host::Instance, text: &str) -> (u32, u32) {
    let at = instance.alloc(text.len()).unwrap();
    instance.write(at, text.as_bytes()).unwrap();
    (at, text.len() as u32)
}

#[test]
fn a_panic_traps_the_call_and_leaves_the_host_alive() {
    let Some(wasm) = plugin_wasm() else { return };
    let host = PluginHost::new().unwrap();
    let plugin = host.load(&wasm).unwrap();
    let mut instance = host.instantiate(&plugin, &Grants::default()).unwrap();
    let result: auroraw_plugin_host::Result<u32> =
        instance.call("do_panic", (), Duration::from_secs(5));
    assert!(
        result.is_err(),
        "a panic must fail the call, not the host process"
    );
    assert!(host_alive(&host, &wasm));
}

#[test]
fn an_infinite_loop_is_interrupted_by_its_time_budget() {
    let Some(wasm) = plugin_wasm() else { return };
    let host = PluginHost::new().unwrap();
    let plugin = host.load(&wasm).unwrap();
    for deadline_ms in [10u64, 100, 300] {
        let mut instance = host.instantiate(&plugin, &Grants::default()).unwrap();
        let deadline = Duration::from_millis(deadline_ms);
        let t = Instant::now();
        let result: auroraw_plugin_host::Result<u32> = instance.call("do_loop", (), deadline);
        let took = t.elapsed();
        assert!(
            result.is_err(),
            "an infinite loop must be interrupted, not returned from"
        );
        // The reason is in the message, not only the backtrace: a decode stopped by its budget was once reported
        // by CI as a backtrace that could not be told from a crash.
        let message = result.unwrap_err().to_string();
        assert!(message.contains("interrupt"), "{message}");
        // The host's timer is a free-running 1 ms tick (`plugin::EPOCH_TICK`), not a fresh timer
        // per call: the first tick counted toward this call's deadline can land anywhere up to a
        // full tick after the deadline was set, so a call can finish up to one tick early.
        assert!(
            took + Duration::from_millis(1) >= deadline,
            "interrupted much before its own deadline ({took:?} < {deadline:?})"
        );
        // A shared CI runner can starve the host's own timer thread for a long stretch under
        // contention from every other test running in parallel (observed: several real seconds
        // for a 100 ms budget on a busy runner, 13 s for 300 ms on a Windows one once the suite grew) without that meaning anything is broken: the
        // ticker is a plain thread, not a real-time one, and nothing here promises it never
        // shares a core with 187 other tests. What must still hold is "eventually", not "close to
        // on time": a genuinely stuck timer (the ticker thread panicked or was never started)
        // would still be caught by this, just with a lot of headroom instead of none.
        assert!(
            took < deadline + Duration::from_secs(30),
            "interrupted far later than its deadline ({took:?} for a {deadline:?} budget)"
        );
        assert!(host_alive(&host, &wasm));
    }
}

#[test]
fn a_memory_bomb_is_refused_past_the_limit_but_allowed_under_it() {
    let Some(wasm) = plugin_wasm() else { return };
    let host = PluginHost::new().unwrap();
    let plugin = host.load(&wasm).unwrap();
    let limit = 256usize << 20;

    let mut under = host
        .instantiate(
            &plugin,
            &Grants {
                memory_limit: Some(limit),
                ..Default::default()
            },
        )
        .unwrap();
    let got: auroraw_plugin_host::Result<u32> =
        under.call("do_alloc_bomb", 100u32, Duration::from_secs(10));
    assert_eq!(got.unwrap(), 100, "100 MB must fit under a 256 MB limit");
    assert!(host_alive(&host, &wasm));

    let mut over = host
        .instantiate(
            &plugin,
            &Grants {
                memory_limit: Some(limit),
                ..Default::default()
            },
        )
        .unwrap();
    let refused: auroraw_plugin_host::Result<u32> =
        over.call("do_alloc_bomb", 2000u32, Duration::from_secs(10));
    assert!(
        refused.is_err(),
        "2000 MB must be refused under a 256 MB limit"
    );
    assert!(host_alive(&host, &wasm));
}

#[test]
fn endless_recursion_traps_instead_of_corrupting_the_host() {
    let Some(wasm) = plugin_wasm() else { return };
    let host = PluginHost::new().unwrap();
    let plugin = host.load(&wasm).unwrap();
    let mut instance = host.instantiate(&plugin, &Grants::default()).unwrap();
    let result: auroraw_plugin_host::Result<u32> =
        instance.call("do_stack_overflow", 0u32, Duration::from_secs(5));
    assert!(result.is_err());
    assert!(host_alive(&host, &wasm));
}

#[test]
fn a_plugin_reaches_for_files_it_was_not_granted() {
    let Some(wasm) = plugin_wasm() else { return };
    let host = PluginHost::new().unwrap();
    let plugin = host.load(&wasm).unwrap();
    let temp = auroraw_testkit::temp_dir();
    let granted = temp.path().join("granted");
    std::fs::create_dir_all(&granted).unwrap();
    std::fs::write(granted.join("hello.txt"), "hi").unwrap();
    let secret = temp.path().join("secret.txt");
    std::fs::write(&secret, "the host's private file").unwrap();

    let call_i32 = |instance: &mut auroraw_plugin_host::Instance, name: &str, path: &str| -> i32 {
        let (a, l) = put(instance, path);
        instance.call(name, (a, l), Duration::from_secs(5)).unwrap()
    };

    // Nothing granted: every path is refused, including one inside a folder that exists on the
    // host but was never preopened.
    {
        let mut instance = host.instantiate(&plugin, &Grants::default()).unwrap();
        assert!(call_i32(&mut instance, "do_read_file", secret.to_str().unwrap()) < 0);
        assert!(call_i32(&mut instance, "do_read_file", "/etc/passwd") < 0);
        assert!(call_i32(&mut instance, "do_read_file", "/granted/0/hello.txt") < 0);
        assert!(host_alive(&host, &wasm));
    }

    // One folder granted, read only: inside it is allowed; escaping it, by any shape, is not.
    {
        let grants = Grants {
            read_dirs: vec![granted.clone()],
            ..Default::default()
        };
        let mut instance = host.instantiate(&plugin, &grants).unwrap();
        assert!(
            call_i32(&mut instance, "do_read_file", "/granted/0/hello.txt") >= 0,
            "a file inside the granted folder must be readable"
        );
        assert!(
            call_i32(&mut instance, "do_list_dir", "/granted/0") >= 0,
            "the granted folder itself must be listable"
        );
        assert!(
            call_i32(&mut instance, "do_read_file", "/granted/0/../secret.txt") < 0,
            "`../` must not escape the granted folder"
        );
        assert!(
            call_i32(&mut instance, "do_read_file", secret.to_str().unwrap()) < 0,
            "an absolute host path must not escape the granted folder"
        );
        assert!(
            call_i32(
                &mut instance,
                "do_read_file",
                "C:\\Windows\\System32\\config\\SAM"
            ) < 0,
            "a Windows drive form must not resolve to anything, on any host"
        );
        assert!(
            call_i32(&mut instance, "do_read_file", "\\\\server\\share\\file") < 0,
            "a UNC form must not resolve to anything, on any host"
        );
        let write_path = "/granted/0/new.txt";
        assert!(
            call_i32(&mut instance, "do_write_file", write_path) < 0,
            "the folder was granted read-only"
        );
        assert!(
            !granted.join("new.txt").exists(),
            "no file must have been created"
        );
        assert!(host_alive(&host, &wasm));
    }

    // A symbolic link inside the granted folder that points outside it must not be followed past
    // the boundary either. Best effort: skipped if this host cannot create a symlink at all (an
    // unprivileged Windows account without Developer Mode), which is a host-environment
    // limitation, not something this test is checking.
    #[cfg(unix)]
    {
        let link = granted.join("escape");
        if std::os::unix::fs::symlink(&secret, &link).is_ok() {
            let grants = Grants {
                read_dirs: vec![granted.clone()],
                ..Default::default()
            };
            let mut instance = host.instantiate(&plugin, &grants).unwrap();
            assert!(
                call_i32(&mut instance, "do_read_file", "/granted/0/escape") < 0,
                "a symbolic link out of the granted folder must not be followed"
            );
            assert!(host_alive(&host, &wasm));
        }
    }
}

#[test]
fn a_plugin_reaches_for_the_network_a_process_and_a_thread() {
    let Some(wasm) = plugin_wasm() else { return };
    let host = PluginHost::new().unwrap();
    let plugin = host.load(&wasm).unwrap();
    let mut instance = host.instantiate(&plugin, &Grants::default()).unwrap();
    let timeout = Duration::from_secs(5);
    let connect: i32 = instance.call("do_connect", (), timeout).unwrap();
    assert!(connect < 0, "a network connection must not be available");
    let spawn_process: i32 = instance.call("do_spawn_process", (), timeout).unwrap();
    assert!(
        spawn_process < 0,
        "starting a process must not be available"
    );
    let spawn_thread: i32 = instance.call("do_spawn_thread", (), timeout).unwrap();
    assert!(spawn_thread < 0, "starting a thread must not be available");
    assert!(host_alive(&host, &wasm));
}

#[test]
fn a_plugin_sees_no_environment_but_can_read_the_clock() {
    let Some(wasm) = plugin_wasm() else { return };
    let host = PluginHost::new().unwrap();
    let plugin = host.load(&wasm).unwrap();
    let mut instance = host.instantiate(&plugin, &Grants::default()).unwrap();
    let timeout = Duration::from_secs(5);
    let env_count: i32 = instance.call("do_env_count", (), timeout).unwrap();
    assert_eq!(env_count, 0, "no environment variables must be visible");
    let clock: i64 = instance.call("do_clock_ms", (), timeout).unwrap();
    assert!(
        clock > 0,
        "the clock is granted to every plugin (architecture §8.2)"
    );
    assert!(host_alive(&host, &wasm));
}
