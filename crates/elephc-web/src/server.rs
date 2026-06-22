//! Purpose:
//! The `--web` server entry point: parse the binary's runtime args, prefork N
//! worker processes, and supervise them. Each worker serves HTTP independently.
//!
//! Called from:
//! - The compiled `--web` binary's process entry (tail-call to elephc_web_run).
//!
//! Key details:
//! - fork() happens BEFORE any tokio runtime is created (tokio does not survive
//!   fork); each child builds its own current-thread runtime in worker::serve.
//! - --listen host:port is required; without it the process errors and exits.

use std::ffi::CStr;

use crate::worker;

/// Parsed server configuration from the binary's own argv.
struct ServerArgs {
    listen: String,
    workers: usize,
}

/// Parses argc/argv into ServerArgs. Returns None (and prints to stderr) when
/// --listen is missing, which the caller turns into a nonzero exit.
fn parse_args(argc: i32, argv: *const *const u8) -> Option<ServerArgs> {
    let mut listen: Option<String> = None;
    let mut workers: usize = default_workers();
    let args: Vec<String> = (0..argc as isize)
        .filter_map(|i| unsafe {
            let p = *argv.offset(i);
            if p.is_null() { return None; }
            Some(CStr::from_ptr(p as *const i8).to_string_lossy().into_owned())
        })
        .collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--listen" => { i += 1; listen = args.get(i).cloned(); }
            "--workers" => { i += 1; workers = args.get(i).and_then(|w| w.parse().ok()).unwrap_or(workers); }
            _ => {}
        }
        i += 1;
    }
    match listen {
        Some(l) => Some(ServerArgs { listen: l, workers: workers.max(1) }),
        None => {
            eprintln!("error: --web binary requires --listen host:port");
            None
        }
    }
}

/// Returns the default worker count (number of logical CPUs, min 1).
fn default_workers() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

/// Server entry: parse args, prefork workers, supervise. Returns an exit code.
///
/// # Safety
/// `handler` must be the compiler-emitted `_elephc_web_handler` symbol; argv
/// must point to `argc` valid NUL-terminated C strings.
#[no_mangle]
pub extern "C" fn elephc_web_run(
    argc: i32,
    argv: *const *const u8,
    handler: extern "C" fn(),
) -> i32 {
    let args = match parse_args(argc, argv) {
        Some(a) => a,
        None => return 2,
    };
    // Fork workers BEFORE creating any tokio runtime.
    let mut children = Vec::new();
    for _ in 0..args.workers {
        match unsafe { libc::fork() } {
            -1 => { eprintln!("error: fork failed"); return 1; }
            0 => {
                // Child: serve forever; never returns to the master loop.
                worker::serve(&args.listen, handler);
                std::process::exit(0);
            }
            pid => children.push(pid),
        }
    }
    // Master: wait for children. (Signal propagation / respawn: Phase 4.)
    for pid in children {
        let mut status = 0;
        unsafe { libc::waitpid(pid, &mut status, 0); }
    }
    0
}
