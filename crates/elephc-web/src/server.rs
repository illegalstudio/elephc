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
use std::sync::atomic::{AtomicBool, Ordering};

use crate::worker;

/// Set by the SIGINT/SIGTERM handler so the master supervision loop can break and
/// shut workers down cleanly. Async-signal-safe: the handler only stores to it.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Async-signal-safe SIGINT/SIGTERM handler: records the shutdown request only.
extern "C" fn handle_shutdown_signal(_sig: libc::c_int) {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

/// Installs `handle_shutdown_signal` for SIGINT and SIGTERM WITHOUT `SA_RESTART`,
/// so a signal interrupts the master's blocking `waitpid` (returns EINTR) instead
/// of silently restarting it.
fn install_signal_handlers() {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handle_shutdown_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = 0; // no SA_RESTART: waitpid returns EINTR on signal
        libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut());
    }
}

/// Restores the default disposition for SIGINT/SIGTERM. Each forked worker calls
/// this so it does NOT inherit the master's catch-and-flag handler — otherwise a
/// worker would catch the master's forwarded SIGTERM and never terminate, hanging
/// the master's reap. With SIG_DFL a forwarded SIGTERM terminates the worker.
fn reset_signal_handlers_to_default() {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = libc::SIG_DFL;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = 0;
        libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut());
    }
}

/// Default request body cap in bytes (8 MiB), matching PHP's `post_max_size`.
const DEFAULT_MAX_BODY: usize = 8 * 1024 * 1024;

/// Parsed server configuration from the binary's own argv.
struct ServerArgs {
    listen: String,
    workers: usize,
    /// Max request body in bytes; `0` means unlimited.
    max_body: usize,
}

/// Parses argc/argv into ServerArgs. Returns None (and prints to stderr) when
/// --listen is missing, which the caller turns into a nonzero exit.
fn parse_args(argc: i32, argv: *const *const u8) -> Option<ServerArgs> {
    let mut listen: Option<String> = None;
    let mut workers: usize = default_workers();
    let mut max_body: usize = DEFAULT_MAX_BODY;
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
            "--max-body-size" => { i += 1; max_body = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(max_body); }
            _ => {}
        }
        i += 1;
    }
    match listen {
        Some(l) => Some(ServerArgs { listen: l, workers: workers.max(1), max_body }),
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

/// Forks one worker child that serves forever, returning the child pid in the
/// master. The child restores default signal disposition and never returns. A
/// fork failure aborts the whole process. Used for both initial spawn and respawn.
fn spawn_worker(listen: &str, handler: extern "C" fn(), max_body: usize) -> libc::pid_t {
    match unsafe { libc::fork() } {
        -1 => {
            eprintln!("error: fork failed");
            std::process::exit(1);
        }
        0 => {
            reset_signal_handlers_to_default();
            worker::serve(listen, handler, max_body);
            std::process::exit(0);
        }
        pid => pid,
    }
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
    install_signal_handlers();
    // Fork workers BEFORE creating any tokio runtime.
    let mut children: Vec<libc::pid_t> = Vec::new();
    for _ in 0..args.workers {
        children.push(spawn_worker(&args.listen, handler, args.max_body));
    }
    // Supervise: wait for any child; break on a shutdown request (SIGINT/SIGTERM).
    loop {
        if SHUTDOWN.load(Ordering::SeqCst) {
            break;
        }
        let mut status = 0;
        let pid = unsafe { libc::waitpid(-1, &mut status, 0) };
        if SHUTDOWN.load(Ordering::SeqCst) {
            break;
        }
        if pid > 0 {
            children.retain(|&c| c != pid);
            if !SHUTDOWN.load(Ordering::SeqCst) {
                // A worker died unexpectedly: replace it to keep the pool at N.
                children.push(spawn_worker(&args.listen, handler, args.max_body));
            } else if children.is_empty() {
                break;
            }
        } else if pid == -1 {
            // ECHILD: nothing left to wait for. EINTR: a signal arrived → re-loop
            // and re-check SHUTDOWN at the top.
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD) {
                break;
            }
        }
    }
    // Clean teardown: ask every still-tracked worker to terminate, then reap.
    for &pid in &children {
        unsafe { libc::kill(pid, libc::SIGTERM); }
    }
    for &pid in &children {
        let mut status = 0;
        unsafe { libc::waitpid(pid, &mut status, 0); }
    }
    0
}
