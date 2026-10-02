// SPDX-License-Identifier: GPL-3.0-or-later
//! A plugin that misbehaves on purpose: everything a broken or malicious plugin could try, so
//! `plugin-host`'s hostile-plugin tests exercise a real sandbox boundary instead of a mock one
//! (testing strategy, `plugin-host`: "the hostile cases of spike 4 become permanent tests, on all
//! three platforms"). Never shipped as a real plugin; built only for those tests.

use std::io::ErrorKind;

/// Reserves `n` bytes in this instance's own memory (every plugin the host loads exports this).
#[unsafe(no_mangle)]
pub extern "C" fn alloc(n: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(n);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

fn text(ptr: *const u8, len: u32) -> String {
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len as usize) }).into_owned()
}

/// A negative code the host can read back: the OS error number if there is one, else 1000 plus a
/// coarse kind, so a refusal is distinguishable from an unrelated failure without depending on
/// exact errno values across platforms.
fn code(error: std::io::Error) -> i32 {
    match error.raw_os_error() {
        Some(n) => -n,
        None => {
            -1000
                - match error.kind() {
                    ErrorKind::Unsupported => 1,
                    ErrorKind::NotFound => 2,
                    ErrorKind::PermissionDenied => 3,
                    _ => 9,
                }
        }
    }
}

/// Panics immediately.
#[unsafe(no_mangle)]
pub extern "C" fn do_panic() -> u32 {
    panic!("this plugin panics on purpose")
}

/// Loops forever, spending real CPU (not blocked on anything the host could starve instead).
#[unsafe(no_mangle)]
pub extern "C" fn do_loop() -> u32 {
    let mut i = 0u64;
    loop {
        i = std::hint::black_box(i.wrapping_add(1));
    }
}

/// Allocates and touches `mb` megabytes, trying to grow past whatever memory limit the host set.
#[unsafe(no_mangle)]
pub extern "C" fn do_alloc_bomb(mb: u32) -> u32 {
    let mut kept: Vec<Vec<u8>> = Vec::new();
    for _ in 0..mb {
        let mut v = vec![0u8; 1 << 20];
        v[0] = 1;
        kept.push(v);
    }
    kept.len() as u32
}

/// Recurses without a base case, trying to overflow the guest's own stack.
#[unsafe(no_mangle)]
pub extern "C" fn do_stack_overflow(n: u32) -> u32 {
    let pad = [n as u8; 256];
    if n == u32::MAX {
        return pad[3] as u32;
    }
    do_stack_overflow(n + 1) + std::hint::black_box(pad[7] as u32)
}

/// Tries to read the file at the path given as `len` bytes at `ptr` (used both for a plain
/// denied path and for every path-traversal shape the test suite tries: `../`, an absolute path,
/// a Windows drive or UNC form, a symbolic link that escapes a granted folder).
#[unsafe(no_mangle)]
pub extern "C" fn do_read_file(ptr: *const u8, len: u32) -> i32 {
    match std::fs::read(text(ptr, len)) {
        Ok(bytes) => bytes.len() as i32,
        Err(e) => code(e),
    }
}

/// Tries to list the directory at the given path.
#[unsafe(no_mangle)]
pub extern "C" fn do_list_dir(ptr: *const u8, len: u32) -> i32 {
    match std::fs::read_dir(text(ptr, len)) {
        Ok(entries) => entries.count() as i32,
        Err(e) => code(e),
    }
}

/// Tries to write a file at the given path.
#[unsafe(no_mangle)]
pub extern "C" fn do_write_file(ptr: *const u8, len: u32) -> i32 {
    match std::fs::write(text(ptr, len), b"written by a plugin") {
        Ok(()) => 0,
        Err(e) => code(e),
    }
}

/// Tries to open a network connection.
#[unsafe(no_mangle)]
pub extern "C" fn do_connect() -> i32 {
    match std::net::TcpStream::connect("93.184.216.34:80") {
        Ok(_) => 0,
        Err(e) => code(e),
    }
}

/// Tries to start another process.
#[unsafe(no_mangle)]
pub extern "C" fn do_spawn_process() -> i32 {
    match std::process::Command::new("ls").output() {
        Ok(_) => 0,
        Err(e) => code(e),
    }
}

/// Tries to start a thread of its own.
#[unsafe(no_mangle)]
pub extern "C" fn do_spawn_thread() -> i32 {
    match std::thread::Builder::new().spawn(|| 1) {
        Ok(handle) => handle.join().unwrap_or(-2),
        Err(e) => code(e),
    }
}

/// Counts the environment variables it can see.
#[unsafe(no_mangle)]
pub extern "C" fn do_env_count() -> i32 {
    std::env::vars().count() as i32
}

/// Counts the process arguments it can see.
#[unsafe(no_mangle)]
pub extern "C" fn do_args_count() -> i32 {
    std::env::args().count() as i32
}

/// Reads the wall clock (granted to every plugin; architecture §8.2, spike 4: "a side channel,
/// but not for this use").
#[unsafe(no_mangle)]
pub extern "C" fn do_clock_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(-1)
}

/// A decoder that lies about what it returns, so that the host's checks of `WasmDecoder::decode` run against a real
/// sandbox (the real decoder plugin is honest, and nothing else could make them run). It exports `import` as the
/// decoder does and returns what its *input* says: the input is two little-endian words, the block length and the
/// samples length it claims, and then the bytes of a block, which stay where the host wrote them. The four words of
/// `out` point at them, with the claimed lengths; the samples' address is the start of the input, readable for as
/// many bytes as the input is long.
///
/// # Safety
///
/// `ptr` must be valid for `len` reads (at least 8) and `out` for 4 writes of `u32`: true of any address this
/// instance's own `alloc` returned, which is the only address the host passes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn import(ptr: *const u8, len: u32, out: *mut u32) -> i32 {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    let header = unsafe { std::slice::from_raw_parts_mut(out, 4) };
    header[0] = ptr as u32 + 8;
    header[1] = word(0);
    header[2] = ptr as u32;
    header[3] = word(1);
    0
}
