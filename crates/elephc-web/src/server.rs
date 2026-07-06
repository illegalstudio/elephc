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
use crate::{handler, worker_mode};

/// `--help` text for the produced `--web` binary.
const HELP: &str = "\
Usage: <binary> --listen HOST:PORT [options]

A standalone prefork HTTP server compiled from PHP by `elephc --web`.

Options:
  --listen HOST:PORT     Address to bind (required), e.g. 127.0.0.1:8080
  --workers N            Number of prefork worker processes (default: CPU count)
  --max-body-size BYTES  Max request body in bytes; 0 = unlimited (default: 8388608)
  --max-requests N       Recycle a WORKER PROCESS after N requests; 0 = never (default: 0; worker mode: 1000)
  --max-requests-per-connection N
                         Close a keep-alive CONNECTION after N responses (sends
                         \"Connection: close\" so the client reconnects and the
                         kernel re-picks a worker); 0 = unlimited (default: 0)
  --idle-timeout SECS    Close a keep-alive connection idle (no new request) for
                         more than SECS seconds; 0 = never (default: 0)
  --access-log           Log one line per request to stderr
  --max-execution-time N Kill (and respawn) a worker whose handler runs > N seconds; 0 = no limit
  --worker-gc-interval N Run the cycle collector every N requests; 0 = never, 1 = every request (worker mode default: 1)
  --gzip                 Compress responses when the client sends Accept-Encoding: gzip
  --tls-cert FILE        PEM certificate chain; enables TLS on --listen (requires --tls-key)
  --tls-key FILE         PEM private key matching --tls-cert (PKCS#8, PKCS#1 or SEC1)
  --help                 Show this help and exit
  --version              Show the server version and exit";

/// A worker that dies within this window of being spawned counts as a crash-on-
/// startup; too many in a row (e.g. a bind failure or a handler that crashes on
/// every request) abort the master instead of fork-looping forever. Used as the
/// fallback heuristic in classic `--web` mode (which has no boot-signal pipe).
const FAST_DEATH: Duration = Duration::from_millis(1000);
/// Consecutive fast worker deaths tolerated before the master gives up.
///
/// Crash-loop policy (startup vs runtime): in worker mode the master uses a
/// pipe-based boot signal — the child writes one byte when it reaches
/// `elephc_web_worker_register`, and the master inspects the pipe read end when a
/// child dies. A death with no boot signal (pipe empty) is a startup failure
/// (boot never completed) and counts toward `MAX_FAST_DEATHS`; a death after the
/// boot signal is a runtime crash (counter reset, immediate respawn). In classic
/// `--web` mode there is no boot pipe, so the timing heuristic (`FAST_DEATH`)
/// is used instead.
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
    /// Run the cycle collector every N requests in worker mode; `0` = never,
    /// `1` = every request. Ignored in classic `--web` mode.
    worker_gc_interval: u32,
    /// Close a keep-alive connection after this many responses; `0` = unlimited.
    /// Mode-independent, default `0` (opt-in; off preserves the original behavior).
    max_conn_requests: usize,
    /// Close a keep-alive connection idle for more than this many seconds;
    /// `0` = never. Mode-independent, default `0` (opt-in; off preserves the
    /// original behavior).
    idle_timeout_secs: u32,
    /// PEM certificate-chain path for TLS (`--tls-cert`); `None` = plaintext HTTP.
    /// Must be set together with `tls_key`. The acceptor is loaded in the master
    /// before fork and installed into the `tls` module's `OnceLock`, so it does
    /// not travel through the `Copy` `WorkerConfig`.
    tls_cert: Option<String>,
    /// PEM private-key path for TLS (`--tls-key`); `None` = plaintext HTTP. Must be
    /// set together with `tls_cert`.
    tls_key: Option<String>,
}

impl ServerArgs {
    /// Builds the per-worker config handed to `worker::serve` / `enter_worker_loop`.
    fn worker_config(&self) -> WorkerConfig {
        WorkerConfig {
            max_body: self.max_body,
            max_requests: self.max_requests,
            access_log: self.access_log,
            max_exec_secs: self.max_exec_secs,
            gzip: self.gzip,
            worker_gc_interval: self.worker_gc_interval,
            max_conn_requests: self.max_conn_requests,
            idle_timeout_secs: self.idle_timeout_secs,
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
///
/// `worker_mode` selects the worker-mode defaults: `--max-requests` defaults to
/// 1000 (vs 0 in classic mode) and `--worker-gc-interval` defaults to 1 (vs 0
/// in classic mode, where it is unused). An explicitly passed `--max-requests`
/// or `--worker-gc-interval` always overrides the default.
fn parse_args(argc: i32, argv: *const *const c_char, worker_mode: bool) -> ParsedArgs {
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
    let mut max_requests_set = false;
    let mut access_log = false;
    let mut max_exec_secs: u32 = 0;
    let mut gzip = false;
    let mut worker_gc_interval: u32 = if worker_mode { 1 } else { 0 };
    let mut worker_gc_interval_set = false;
    // Keep-alive rotation defaults are mode-independent (same in classic `--web`,
    // `--web-worker`, and `--web-worker=script`), so they are set here at init
    // rather than in the worker-mode override block below.
    let mut max_conn_requests: usize = 0;
    let mut idle_timeout_secs: u32 = 0;
    let mut tls_cert: Option<String> = None;
    let mut tls_key: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--listen" => { i += 1; listen = args.get(i).cloned(); }
            "--workers" => { i += 1; workers = args.get(i).and_then(|w| w.parse().ok()).unwrap_or(workers); }
            "--max-body-size" => { i += 1; max_body = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(max_body); }
            "--max-requests" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|v| v.parse().ok()) {
                    max_requests = v;
                    max_requests_set = true;
                }
            }
            "--max-requests-per-connection" => { i += 1; max_conn_requests = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(max_conn_requests); }
            "--idle-timeout" => { i += 1; idle_timeout_secs = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(idle_timeout_secs); }
            "--max-execution-time" => { i += 1; max_exec_secs = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(max_exec_secs); }
            "--worker-gc-interval" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|v| v.parse().ok()) {
                    worker_gc_interval = v;
                    worker_gc_interval_set = true;
                }
            }
            "--access-log" => { access_log = true; }
            "--gzip" => { gzip = true; }
            "--tls-cert" => { i += 1; tls_cert = args.get(i).cloned(); }
            "--tls-key" => { i += 1; tls_key = args.get(i).cloned(); }
            _ => {}
        }
        i += 1;
    }
    // TLS flags are paired: one without the other is a usage error. Checked before
    // the missing-`--listen` error so the TLS-specific mistake gets the precise
    // diagnostic. The acceptor itself is loaded later, in the master before fork.
    if tls_cert.is_some() != tls_key.is_some() {
        eprintln!("error: --tls-cert and --tls-key must be provided together (try --help)");
        return ParsedArgs::Exit(2);
    }
    // Worker-mode defaults: recycle after 1000 requests and collect cycles
    // every request, unless the operator explicitly overrode either flag.
    if worker_mode && !max_requests_set {
        max_requests = 1000;
    }
    let _ = worker_gc_interval_set;
    match listen {
        Some(l) => ParsedArgs::Run(ServerArgs {
            listen: l,
            workers: workers.max(1),
            max_body,
            max_requests,
            access_log,
            max_exec_secs,
            gzip,
            worker_gc_interval,
            max_conn_requests,
            idle_timeout_secs,
            tls_cert,
            tls_key,
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

/// Selects which child-entry path a forked worker takes.
enum WorkerKind {
    /// Classic `--web`: re-execute the top-level PHP handler per request.
    Classic { handler: extern "C" fn() },
    /// `--web-worker`: boot PHP once, then register + enter the Rust worker loop.
    Worker { boot: extern "C" fn() },
    /// `--web-worker=script`: register the top-level body directly as the
    /// per-request handler (void ABI) and enter the Rust worker loop, with no
    /// separate PHP boot/register phase.
    Script { handler: handler::ScriptHandler },
}

/// Forks one worker child that serves forever, returning the child pid and the
/// boot-pipe read fd (for worker mode) in the master. The child restores default
/// signal disposition, installs the per-mode worker config/listen address, and
/// never returns. A fork failure aborts the whole process. Used for both initial
/// spawn and respawn, in both modes.
///
/// For `WorkerKind::Worker` a pipe is created before fork; the child gets the
/// write end (closed in `elephc_web_worker_register` after signaling) and the
/// master gets the read end (stored alongside the pid so `supervise` can tell a
/// startup crash from a runtime crash by whether the boot signal arrived). For
/// `WorkerKind::Classic` the boot pipe read fd is `None`.
fn spawn_worker(listen: &str, kind: WorkerKind, cfg: WorkerConfig) -> (libc::pid_t, Option<i32>) {
    // Worker mode: create a boot-signal pipe before fork so the child can
    // signal boot completion and the master can detect startup vs runtime
    // crashes precisely instead of relying on the FAST_DEATH timing heuristic.
    let boot_pipe_rd = match &kind {
        WorkerKind::Worker { .. } | WorkerKind::Script { .. } => {
            let mut fds = [0i32; 2];
            // SAFETY: pipe(2) on a stack array; failure aborts the master.
            if unsafe { libc::pipe(fds.as_mut_ptr()) } == -1 {
                eprintln!("error: boot pipe creation failed");
                std::process::exit(1);
            }
            Some((fds[0], fds[1]))
        }
        WorkerKind::Classic { .. } => None,
    };
    match unsafe { libc::fork() } {
        -1 => {
            eprintln!("error: fork failed");
            std::process::exit(1);
        }
        0 => {
            reset_signal_handlers_to_default();
            match kind {
                WorkerKind::Classic { handler } => {
                    worker::serve(listen, handler, cfg);
                    std::process::exit(0);
                }
                WorkerKind::Worker { boot } => {
                    // Worker mode: publish config + listen address into the
                    // process-static slots BEFORE boot runs, so enter_worker_loop
                    // (reached from inside the boot via register) can read them.
                    worker_mode::set_worker_config(cfg);
                    worker_mode::set_worker_listen(listen.to_string());
                    handler::set_worker_boot(boot);
                    // Close the read end in the child; install the write end so
                    // elephc_web_worker_register can signal boot completion.
                    if let Some((rd, wr)) = boot_pipe_rd {
                        unsafe { libc::close(rd); }
                        handler::set_boot_pipe(wr);
                    }
                    boot();
                    // boot() is expected to call elephc_web_worker_register,
                    // which diverges into enter_worker_loop and never returns.
                    // Reaching here means the boot returned without registering.
                    eprintln!("elephc-web: worker boot returned without registering a handler");
                    std::process::exit(1);
                }
                WorkerKind::Script { handler } => {
                    // Script mode: publish config + listen address into the
                    // process-static slots, then register the handler directly —
                    // there is no separate PHP boot phase to run first.
                    worker_mode::set_worker_config(cfg);
                    worker_mode::set_worker_listen(listen.to_string());
                    if let Some((rd, wr)) = boot_pipe_rd {
                        unsafe { libc::close(rd); }
                        handler::set_boot_pipe(wr);
                    }
                    handler::register_script_handler(handler);
                    // register_script_handler is `-> !`: it diverges into
                    // enter_worker_loop and never returns.
                }
            }
        }
        pid => {
            // Master: close the write end, keep the read end with the pid so
            // supervise can check whether the boot signal arrived.
            let rd = match boot_pipe_rd {
                Some((rd, wr)) => {
                    // SAFETY: the write end belongs to the child; closing it in
                    // the master so a read on the read end returns EOF once the
                    // child closes its write end (after signaling or on crash).
                    unsafe { libc::close(wr); }
                    Some(rd)
                }
                None => None,
            };
            (pid, rd)
        }
    }
}

/// Supervises `children`: waits for any child to exit, breaks on a shutdown
/// request, respawns to keep the pool at its initial size, and aborts after too
/// many consecutive startup failures. Shared by `elephc_web_run` and
/// `elephc_web_run_worker`. Returns the master exit code.
///
/// Each entry is `(pid, boot_pipe_rd, spawned_at)`. For worker mode, `boot_pipe_rd`
/// is the read end of the boot-signal pipe; for classic mode it is `None`. When a
/// worker-mode child dies, the boot pipe distinguishes a startup crash (pipe had
/// no data: boot never reached `elephc_web_worker_register`) from a runtime crash
/// (pipe had data: boot completed and the crash happened during request serving).
/// Startup crashes count toward `MAX_FAST_DEATHS` with exponential backoff;
/// runtime crashes reset the counter and respawn immediately.
fn supervise(
    listen: &str,
    kind: WorkerKind,
    cfg: WorkerConfig,
    mut children: Vec<(libc::pid_t, Option<i32>, Instant)>,
) -> i32 {
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
            let (boot_pipe_rd, spawned_at) = children
                .iter()
                .find(|(c, _, _)| *c == pid)
                .map(|(_, rd, t)| (*rd, *t))
                .unwrap_or((None, Instant::now()));
            children.retain(|(c, _, _)| *c != pid);
            // Close the boot-pipe read fd now that the child is dead; for worker
            // mode it was used below to classify the crash, for classic mode it
            // is None.
            if let Some(rd) = boot_pipe_rd {
                // SAFETY: the read end is owned by the master; close(2) is safe.
                unsafe { libc::close(rd); }
            }
            if SHUTDOWN.load(Ordering::SeqCst) {
                if children.is_empty() {
                    break;
                }
                continue;
            }
            // Crash-loop guard: classify the crash as startup vs runtime.
            let is_startup_crash = classify_crash(boot_pipe_rd, spawned_at);
            if is_startup_crash {
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
            let (new_pid, new_rd) = spawn_worker(listen, kind.clone(), cfg);
            children.push((new_pid, new_rd, Instant::now()));
        } else if pid == -1 {
            // ECHILD: nothing left to wait for. EINTR: a signal arrived → re-loop
            // and re-check SHUTDOWN at the top.
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD) {
                break;
            }
        }
    }
    // Clean teardown: ask every still-tracked worker to terminate, then reap.
    // Close any leftover boot-pipe read fds so they do not leak.
    for &(pid, rd, _) in &children {
        unsafe { libc::kill(pid, libc::SIGTERM); }
        if let Some(rd) = rd {
            unsafe { libc::close(rd); }
        }
    }
    for &(pid, _, _) in &children {
        let mut status = 0;
        unsafe { libc::waitpid(pid, &mut status, 0); }
    }
    0
}

/// Classifies a worker death as a startup crash (counts toward
/// `MAX_FAST_DEATHS`) or a runtime crash (resets the counter, immediate
/// respawn).
///
/// For worker mode (`boot_pipe_rd` is `Some`): if the boot-signal pipe had data,
/// the child reached `elephc_web_worker_register` and the crash is a runtime
/// crash; if the pipe had no data, boot never completed and it is a startup
/// crash.
///
/// For classic mode (`boot_pipe_rd` is `None`): fall back to the timing
/// heuristic — a death within `FAST_DEATH` of spawn is a startup crash.
fn classify_crash(boot_pipe_rd: Option<i32>, spawned_at: Instant) -> bool {
    match boot_pipe_rd {
        // Worker mode: pipe-based boot signal. The read fd is still open here
        // (closed right after this call returns); check whether the child wrote
        // the boot byte before dying. A readable pipe means boot completed.
        Some(rd) => {
            // SAFETY: poll(2) on the read fd with a zero timeout to check for
            // pending data without blocking. The fd is owned by the master.
            let mut pfd = libc::pollfd { fd: rd, events: libc::POLLIN, revents: 0 };
            let n = unsafe { libc::poll(&mut pfd as *mut _, 1, 0) };
            // n > 0 and POLLIN set means there is data to read → boot completed.
            let booted = n > 0 && (pfd.revents & libc::POLLIN) != 0;
            !booted
        }
        // Classic mode: no boot pipe, so use the timing heuristic.
        None => spawned_at.elapsed() < FAST_DEATH,
    }
}

impl Clone for WorkerKind {
    fn clone(&self) -> Self {
        match self {
            WorkerKind::Classic { handler } => WorkerKind::Classic { handler: *handler },
            WorkerKind::Worker { boot } => WorkerKind::Worker { boot: *boot },
            WorkerKind::Script { handler } => WorkerKind::Script { handler: *handler },
        }
    }
}

/// Loads and installs the TLS acceptor when both `--tls-cert` and `--tls-key` are
/// set, BEFORE any worker is forked. On success the acceptor is stored in the
/// `tls` module's process-wide `OnceLock`, which every forked worker inherits.
/// Returns `Err(2)` (the fail-fast exit code) on a read/parse/mismatch error so
/// the master exits before forking a single worker; returns `Ok(())` when TLS is
/// off or configured successfully. Shared by all three run entry points.
fn install_tls_if_configured(args: &ServerArgs) -> Result<(), i32> {
    if let (Some(cert), Some(key)) = (&args.tls_cert, &args.tls_key) {
        match crate::tls::load_acceptor(cert, key) {
            Ok(acceptor) => crate::tls::set_tls_acceptor(acceptor),
            Err(cause) => {
                eprintln!("error: failed to load TLS certificate/key: {}", cause);
                return Err(2);
            }
        }
    }
    Ok(())
}

/// Returns the URL scheme (`"https"` when TLS is configured, else `"http"`) for
/// the startup log line.
fn listen_scheme(args: &ServerArgs) -> &'static str {
    if args.tls_cert.is_some() {
        "https"
    } else {
        "http"
    }
}
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
    let args = match parse_args(argc, argv, false) {
        ParsedArgs::Run(a) => a,
        ParsedArgs::Exit(code) => return code,
    };
    // Load the TLS acceptor (if configured) BEFORE fork so a bad cert/key fails
    // fast in the master instead of fork-looping.
    if let Err(code) = install_tls_if_configured(&args) {
        return code;
    }
    install_signal_handlers();
    let kind = WorkerKind::Classic { handler };
    let cfg = args.worker_config();
    let mut children: Vec<(libc::pid_t, Option<i32>, Instant)> = Vec::new();
    for _ in 0..args.workers {
        let (pid, rd) = spawn_worker(&args.listen, kind.clone(), cfg);
        children.push((pid, rd, Instant::now()));
    }
    eprintln!(
        "elephc-web: listening on {}://{} ({} worker{})",
        listen_scheme(&args),
        args.listen,
        args.workers,
        if args.workers == 1 { "" } else { "s" }
    );
    supervise(&args.listen, kind, cfg, children)
}

/// Entry point for the `--web-worker` mode: parse args, prefork workers that
/// each boot the PHP application once and then register a request handler, and
/// supervise them. Returns an exit code.
///
/// `boot_fn` is the compiler-emitted `_elephc_web_worker_boot` symbol: the
/// top-level PHP that initializes the app and calls
/// `elephc_web_worker_register`, which transfers control to the Rust worker
/// loop.
///
/// # Safety
/// `boot_fn` must be the compiler-emitted worker boot symbol; argv must point to
/// `argc` valid NUL-terminated C strings.
#[no_mangle]
pub extern "C" fn elephc_web_run_worker(
    argc: i32,
    argv: *const *const c_char,
    boot_fn: extern "C" fn(),
) -> i32 {
    let args = match parse_args(argc, argv, true) {
        ParsedArgs::Run(a) => a,
        ParsedArgs::Exit(code) => return code,
    };
    // Load the TLS acceptor (if configured) BEFORE fork so a bad cert/key fails
    // fast in the master instead of fork-looping.
    if let Err(code) = install_tls_if_configured(&args) {
        return code;
    }
    install_signal_handlers();
    let kind = WorkerKind::Worker { boot: boot_fn };
    let cfg = args.worker_config();
    let mut children: Vec<(libc::pid_t, Option<i32>, Instant)> = Vec::new();
    for _ in 0..args.workers {
        let (pid, rd) = spawn_worker(&args.listen, kind.clone(), cfg);
        children.push((pid, rd, Instant::now()));
    }
    eprintln!(
        "elephc-web: listening on {}://{} ({} worker{}, web-worker mode)",
        listen_scheme(&args),
        args.listen,
        args.workers,
        if args.workers == 1 { "" } else { "s" }
    );
    supervise(&args.listen, kind, cfg, children)
}

/// C-ABI entry for `--web-worker=script`: prefork server that runs the compiled
/// top-level (`handler`) once per request with persistent statics/globals, no
/// `elephc_worker_register` API. Mirrors `elephc_web_run_worker` but skips the
/// PHP boot phase — the handler is registered directly in each forked child.
///
/// # Safety
/// `handler` must be the compiler-emitted `_elephc_web_handler` symbol; argv
/// must point to `argc` valid NUL-terminated C strings.
#[no_mangle]
pub extern "C" fn elephc_web_run_script(
    argc: i32,
    argv: *const *const c_char,
    handler: handler::ScriptHandler,
) -> i32 {
    let args = match parse_args(argc, argv, true) {
        ParsedArgs::Run(a) => a,
        ParsedArgs::Exit(code) => return code,
    };
    // Load the TLS acceptor (if configured) BEFORE fork so a bad cert/key fails
    // fast in the master instead of fork-looping.
    if let Err(code) = install_tls_if_configured(&args) {
        return code;
    }
    install_signal_handlers();
    let kind = WorkerKind::Script { handler };
    let cfg = args.worker_config();
    let mut children: Vec<(libc::pid_t, Option<i32>, Instant)> = Vec::new();
    for _ in 0..args.workers {
        let (pid, rd) = spawn_worker(&args.listen, kind.clone(), cfg);
        children.push((pid, rd, Instant::now()));
    }
    eprintln!(
        "elephc-web: listening on {}://{} ({} worker{}, web-worker=script mode)",
        listen_scheme(&args),
        args.listen,
        args.workers,
        if args.workers == 1 { "" } else { "s" }
    );
    supervise(&args.listen, kind, cfg, children)
}
