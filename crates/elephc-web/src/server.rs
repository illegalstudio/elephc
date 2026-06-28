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

use std::ffi::{c_char, CStr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::worker::{self, WorkerConfig};

/// `--help` text for the produced `--web` binary.
const HELP: &str = "\
Usage: <binary> --listen HOST:PORT [options]

A standalone prefork HTTP server compiled from PHP by `elephc --web`.

Options:
  --listen HOST:PORT     Address to bind (required), e.g. 127.0.0.1:8080
  --workers N            Number of prefork worker processes (default: CPU count)
  --max-body-size BYTES  Max request body in bytes; 0 = unlimited (default: 8388608)
  --max-requests N       Recycle a worker after N requests; 0 = never (default: 0)
  --access-log           Log one line per request to stderr
  --max-execution-time N Kill (and respawn) a worker whose handler runs > N seconds; 0 = no limit
  --gzip                 Compress responses when the client sends Accept-Encoding: gzip
  --help                 Show this help and exit
  --version              Show the server version and exit";

/// A worker that dies within this window of being spawned counts as a crash-on-
/// startup; too many in a row (e.g. a bind failure or a handler that crashes on
/// every request) abort the master instead of fork-looping forever.
const FAST_DEATH: Duration = Duration::from_millis(1000);
/// Consecutive fast worker deaths tolerated before the master gives up.
const MAX_FAST_DEATHS: u32 = 10;

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
    /// Recycle a worker after this many requests; `0` means never.
    max_requests: usize,
    /// When true, log one line per request to stderr.
    access_log: bool,
    /// Per-request handler time limit in seconds; `0` means no limit.
    max_exec_secs: u32,
    /// gzip the response when the client accepts it.
    gzip: bool,
}

impl ServerArgs {
    /// Builds the per-worker config handed to `worker::serve`.
    fn worker_config(&self) -> WorkerConfig {
        WorkerConfig {
            max_body: self.max_body,
            max_requests: self.max_requests,
            access_log: self.access_log,
            max_exec_secs: self.max_exec_secs,
            gzip: self.gzip,
        }
    }
}

/// Outcome of argument parsing: a runnable config, an early exit (`--help`/
/// `--version`, exit code 0), or a usage error (exit code 2).
enum ParsedArgs {
    Run(ServerArgs),
    Exit(i32),
}

/// Collects argv into owned strings.
fn collect_args(argc: i32, argv: *const *const c_char) -> Vec<String> {
    (0..argc as isize)
        .filter_map(|i| unsafe {
            let p = *argv.offset(i);
            if p.is_null() {
                return None;
            }
            Some(CStr::from_ptr(p).to_string_lossy().into_owned())
        })
        .collect()
}

/// Parses argv into a runnable config or an early-exit. Handles `--help` /
/// `--version` (print + exit 0) and a missing `--listen` (error + exit 2).
fn parse_args(argc: i32, argv: *const *const c_char) -> ParsedArgs {
    let args = collect_args(argc, argv);
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", HELP);
        return ParsedArgs::Exit(0);
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("elephc-web {}", env!("CARGO_PKG_VERSION"));
        return ParsedArgs::Exit(0);
    }
    let mut listen: Option<String> = None;
    let mut workers: usize = default_workers();
    let mut max_body: usize = DEFAULT_MAX_BODY;
    let mut max_requests: usize = 0;
    let mut access_log = false;
    let mut max_exec_secs: u32 = 0;
    let mut gzip = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--listen" => { i += 1; listen = args.get(i).cloned(); }
            "--workers" => { i += 1; workers = args.get(i).and_then(|w| w.parse().ok()).unwrap_or(workers); }
            "--max-body-size" => { i += 1; max_body = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(max_body); }
            "--max-requests" => { i += 1; max_requests = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(max_requests); }
            "--max-execution-time" => { i += 1; max_exec_secs = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(max_exec_secs); }
            "--access-log" => { access_log = true; }
            "--gzip" => { gzip = true; }
            _ => {}
        }
        i += 1;
    }
    match listen {
        Some(l) => ParsedArgs::Run(ServerArgs {
            listen: l,
            workers: workers.max(1),
            max_body,
            max_requests,
            access_log,
            max_exec_secs,
            gzip,
        }),
        None => {
            eprintln!("error: --web binary requires --listen host:port (try --help)");
            ParsedArgs::Exit(2)
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
fn spawn_worker(listen: &str, handler: extern "C" fn(), cfg: WorkerConfig) -> libc::pid_t {
    match unsafe { libc::fork() } {
        -1 => {
            eprintln!("error: fork failed");
            std::process::exit(1);
        }
        0 => {
            reset_signal_handlers_to_default();
            worker::serve(listen, handler, cfg);
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
    argv: *const *const c_char,
    handler: extern "C" fn(),
) -> i32 {
    let args = match parse_args(argc, argv) {
        ParsedArgs::Run(a) => a,
        ParsedArgs::Exit(code) => return code,
    };
    install_signal_handlers();
    // Fork workers BEFORE creating any tokio runtime. Track each worker's spawn
    // time so a crash-on-startup loop (e.g. a failed bind) can be detected.
    let mut children: Vec<(libc::pid_t, Instant)> = Vec::new();
    for _ in 0..args.workers {
        let pid = spawn_worker(&args.listen, handler, args.worker_config());
        children.push((pid, Instant::now()));
    }
    eprintln!(
        "elephc-web: listening on http://{} ({} worker{})",
        args.listen,
        args.workers,
        if args.workers == 1 { "" } else { "s" }
    );
    // Supervise: wait for any child; break on a shutdown request (SIGINT/SIGTERM).
    let mut fast_deaths: u32 = 0;
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
            let spawned_at = children
                .iter()
                .find(|(c, _)| *c == pid)
                .map(|(_, t)| *t);
            children.retain(|(c, _)| *c != pid);
            if SHUTDOWN.load(Ordering::SeqCst) {
                if children.is_empty() {
                    break;
                }
                continue;
            }
            // Crash-loop guard: if workers keep dying immediately after spawn,
            // stop respawning (otherwise a failed bind fork-loops forever).
            if spawned_at.map(|t| t.elapsed() < FAST_DEATH).unwrap_or(false) {
                fast_deaths += 1;
                if fast_deaths >= MAX_FAST_DEATHS {
                    eprintln!(
                        "elephc-web: {} workers died on startup (likely a bad --listen or a \
                         handler crashing every request); giving up",
                        fast_deaths
                    );
                    break;
                }
            } else {
                fast_deaths = 0;
            }
            // A worker died unexpectedly: replace it to keep the pool at N.
            let new_pid = spawn_worker(&args.listen, handler, args.worker_config());
            children.push((new_pid, Instant::now()));
        } else if pid == -1 {
            // ECHILD: nothing left to wait for. EINTR: a signal arrived → re-loop
            // and re-check SHUTDOWN at the top.
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD) {
                break;
            }
        }
    }
    // Clean teardown: ask every still-tracked worker to terminate, then reap.
    for &(pid, _) in &children {
        unsafe { libc::kill(pid, libc::SIGTERM); }
    }
    for &(pid, _) in &children {
        let mut status = 0;
        unsafe { libc::waitpid(pid, &mut status, 0); }
    }
    0
}
