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
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::dispatch::{self, DispatchConfig, DispatchMode, MasterWorker};
use crate::worker::{self, WorkerConfig};
use crate::{handler, worker_mode};

/// `--help` text for the produced `--web` binary.
const HELP: &str = "\
Usage: <binary> --listen HOST:PORT [options]

A standalone prefork HTTP server compiled from PHP by `elephc --web`.

Options:
  --listen HOST:PORT     Address to bind (required), e.g. 127.0.0.1:8080
  --workers N            Number of prefork worker processes (default: CPU count)
  --dispatch MODE        Connection dispatch: kernel = SO_REUSEPORT per-worker
                         listeners (default), master = the master accepts and
                         passes each connection to an idle worker (shared queue).
                         Pairs with a short --max-requests-per-connection to
                         approach request-grained balancing on keep-alive clients
  --dispatch-backlog N   Master mode only: max accepted connections queued while
                         all workers are busy (default: 1024)
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
  --handler-offload      Run the PHP handler on a dedicated thread so request/response
                         I/O overlaps handler execution (handlers still never overlap)
  --max-pending N        With --handler-offload: max parsed requests queued for the
                         handler before new requests get 503; queued-body memory is
                         bounded by N x --max-body-size. 0 is rejected (default: 16)
  --http2                Opt in to HTTP/2 (h2c prior-knowledge on plaintext; h2
                         over TLS via ALPN when --tls-cert + --tls-key are set).
                         REQUIRES --handler-offload (without offload, h2
                         multiplexed streams all stall on the single inline
                         handler). Default off: the server speaks HTTP/1.1 only
                         (byte-for-byte the h1 path)
  --http2-max-streams N  Max concurrent h2 streams per connection (default: 8). The
                         per-connection memory bound is N x --max-body-size, so 8
                         caps a single connection at 8 x --max-body-size of buffered
                         bodies before the handler drains them. N < 1 is rejected
                         (omit --http2 to disable HTTP/2)
  --http2-max-header-size N
                         Max h2 header block in BYTES (HPACK header-bomb clamp,
                         default: 65536 = 64 KiB). h1 is unaffected (h1 headers are
                         bounded by --max-body-size). Generous for JWT+cookies+tracing,
                         far below h2's 16 MiB default
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
pub(crate) const MAX_FAST_DEATHS: u32 = 10;

/// Set by the SIGINT/SIGTERM handler so the master supervision loop can break and
/// shut workers down cleanly. Async-signal-safe: the handler only stores to it.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Returns whether a shutdown (SIGINT/SIGTERM) has been requested. Read by both
/// `supervise` (kernel) and `dispatch::master_loop` (master) so the two loops
/// share one shutdown flag.
pub(crate) fn shutdown_requested() -> bool {
    SHUTDOWN.load(Ordering::SeqCst)
}

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

/// Restores the default disposition for SIGINT/SIGTERM (and SIGCHLD). Each forked
/// worker calls this so it does NOT inherit the master's catch-and-flag handler —
/// otherwise a worker would catch the master's forwarded SIGTERM and never
/// terminate, hanging the master's reap. With SIG_DFL a forwarded SIGTERM
/// terminates the worker. SIGCHLD is reset too so a master-mode worker respawned
/// after `dispatch::install_sigchld_handler` does not inherit the master's SIGCHLD
/// handler (the worker has no children; a no-op in kernel mode).
fn reset_signal_handlers_to_default() {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = libc::SIG_DFL;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = 0;
        libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGCHLD, &sa, std::ptr::null_mut());
    }
}

/// Default request body cap in bytes (8 MiB), matching PHP's `post_max_size`.
const DEFAULT_MAX_BODY: usize = 8 * 1024 * 1024;

/// Default `--max-pending` queue depth (parsed requests waiting for the handler
/// thread) when `--handler-offload` is on. Kept low (16, not the spec's 64)
/// because each queued job pins up to `--max-body-size` of body, so the worst-case
/// queued-body memory is `16 × --max-body-size` per worker.
const DEFAULT_MAX_PENDING: usize = 16;

/// Default `--http2-max-streams`: max concurrent h2 streams per connection. Kept
/// at 8 (not the spec's 16): 16 × `--max-body-size` (8 MiB default) = 128 MiB/conn
/// worst case is reckless for a feature with ZERO parallelism payoff until ZTS;
/// 8 → 64 MiB/conn and matches the "no parallelism, just batching" reality.
const DEFAULT_HTTP2_MAX_STREAMS: u32 = 8;

/// Default `--http2-max-header-size` in BYTES (GAP-B): the HPACK header-bomb clamp.
/// 64 KiB is generous for JWT+cookies+tracing, far below h2's 16 MiB default.
const DEFAULT_HTTP2_MAX_HEADER_SIZE: u32 = 64 * 1024;

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
    /// Connection-dispatch backend (`--dispatch`); `Kernel` (default) preserves the
    /// SO_REUSEPORT path, `Master` selects the fd-passing master loop.
    dispatch_mode: DispatchMode,
    /// Master-mode internal fd-queue cap (`--dispatch-backlog`, default 1024).
    /// Ignored in kernel mode.
    dispatch_backlog: usize,
    /// Run the PHP handler on a dedicated `php-handler` thread fed a bounded job
    /// queue (`--handler-offload`), so request/response I/O overlaps handler
    /// execution; default `false` (synchronous inline handler). All three web modes.
    handler_offload: bool,
    /// Max parsed requests queued for the handler thread before new requests get
    /// `503` (`--max-pending`, default 16). Bounds queued-body memory to
    /// `max_pending × --max-body-size`. Only meaningful with `--handler-offload`.
    max_pending: usize,
    /// Opt in to HTTP/2 (`--http2`, default off). When off, the server speaks
    /// HTTP/1.1 only via `auto::Builder::http1_only()` — one code path. When on,
    /// `--handler-offload` is required (validated at parse time).
    http2: bool,
    /// Max concurrent h2 streams per connection (`--http2-max-streams`, default
    /// 8). Acts as hyper's `max_concurrent_streams` cap AND the per-connection
    /// stream budget (GAP-A). `0` only valid when `http2` is off.
    http2_max_streams: u32,
    /// Max h2 header block in bytes (`--http2-max-header-size`, default 64 KiB;
    /// GAP-B). Sets hyper's `max_header_list_size`. h1 is unaffected.
    http2_max_header_size: u32,
}

impl ServerArgs {
    /// Builds the per-worker config handed to `worker::serve` / `enter_worker_loop`.
    /// Kept SEPARATE from `dispatch_config` so the `Copy` `WorkerConfig` does not
    /// carry master-only fields.
    fn worker_config(&self) -> WorkerConfig {
        // GAP-A: per-connection h2 stream budget. `max_conn_requests` is the
        // operator's per-CONNECTION cap (closest semantic match for a per-conn h2
        // budget); `max_requests` is the per-WORKER recycle cap, used as a fallback
        // so `--max-requests` alone still bounds h2 connections; 0 = unbounded.
        let h2_stream_budget = if self.max_conn_requests > 0 {
            self.max_conn_requests
        } else if self.max_requests > 0 {
            self.max_requests
        } else {
            0
        };
        WorkerConfig {
            max_body: self.max_body,
            max_requests: self.max_requests,
            access_log: self.access_log,
            max_exec_secs: self.max_exec_secs,
            gzip: self.gzip,
            worker_gc_interval: self.worker_gc_interval,
            max_conn_requests: self.max_conn_requests,
            idle_timeout_secs: self.idle_timeout_secs,
            handler_offload: self.handler_offload,
            max_pending: self.max_pending,
            h2: worker::Http2Config {
                http2: self.http2,
                max_streams: self.http2_max_streams,
                max_header_size: self.http2_max_header_size,
            },
            h2_stream_budget,
        }
    }

    /// Builds the dispatch config (mode + master backlog) threaded into the run
    /// entry points to select `supervise` (kernel) vs `dispatch::master_loop`.
    fn dispatch_config(&self) -> DispatchConfig {
        DispatchConfig {
            mode: self.dispatch_mode,
            backlog: self.dispatch_backlog,
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
    let mut dispatch_mode = DispatchMode::Kernel;
    let mut dispatch_backlog: usize = 1024;
    let mut dispatch_backlog_set = false;
    let mut handler_offload = false;
    let mut max_pending: usize = DEFAULT_MAX_PENDING;
    let mut max_pending_set = false;
    // HTTP/2 opt-in (`--http2`, default off) and its tunables. `http2` is the
    // on/off switch; `http2_max_streams` is the per-connection concurrent
    // stream cap (also the GAP-A stream budget); `http2_max_header_size` is the
    // HPACK header-bomb guard (GAP-B). The `_set` flag distinguishes "operator
    // passed the flag" from "default value" so inert-flag warnings fire only
    // when the operator actually opted into a tunable without `--http2`.
    let mut http2 = false;
    let mut http2_max_streams: u32 = DEFAULT_HTTP2_MAX_STREAMS;
    let mut http2_max_streams_set = false;
    let mut http2_max_header_size: u32 = DEFAULT_HTTP2_MAX_HEADER_SIZE;
    let mut http2_max_header_size_set = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--listen" => { i += 1; listen = args.get(i).cloned(); }
            "--workers" => { i += 1; workers = args.get(i).and_then(|w| w.parse().ok()).unwrap_or(workers); }
            "--dispatch" => {
                i += 1;
                match args.get(i).map(|s| s.as_str()) {
                    Some("kernel") => dispatch_mode = DispatchMode::Kernel,
                    Some("master") => dispatch_mode = DispatchMode::Master,
                    other => {
                        eprintln!(
                            "error: --dispatch must be 'kernel' or 'master' (got {:?}; try --help)",
                            other.unwrap_or("")
                        );
                        return ParsedArgs::Exit(2);
                    }
                }
            }
            "--dispatch-backlog" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|v| v.parse().ok()) {
                    dispatch_backlog = v;
                    dispatch_backlog_set = true;
                }
            }
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
            "--handler-offload" => { handler_offload = true; }
            "--max-pending" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|v| v.parse().ok()) {
                    max_pending = v;
                    max_pending_set = true;
                }
            }
            "--tls-cert" => { i += 1; tls_cert = args.get(i).cloned(); }
            "--tls-key" => { i += 1; tls_key = args.get(i).cloned(); }
            "--http2" => { http2 = true; }
            "--http2-max-streams" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|v| v.parse().ok()) {
                    http2_max_streams = v;
                    http2_max_streams_set = true;
                }
            }
            "--http2-max-header-size" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|v| v.parse().ok()) {
                    http2_max_header_size = v;
                    http2_max_header_size_set = true;
                }
            }
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
    // `--dispatch-backlog` only means anything in master mode; warn (do not error)
    // when it is set in kernel mode so a stray flag is not silently misleading.
    if dispatch_backlog_set && dispatch_mode == DispatchMode::Kernel {
        eprintln!(
            "warning: --dispatch-backlog is ignored without --dispatch master"
        );
    }
    // `--max-pending 0` is rejected: an unbounded queue turns a slow handler into
    // unbounded memory (each queued job pins up to --max-body-size of body).
    if max_pending_set && max_pending == 0 {
        eprintln!(
            "error: --max-pending must be greater than 0 (an unbounded queue lets a \
             slow handler exhaust memory; try --help)"
        );
        return ParsedArgs::Exit(2);
    }
    // `--max-pending` only means anything with `--handler-offload`; warn (do not
    // error) when it is set without it so a stray flag is not silently misleading.
    if max_pending_set && !handler_offload {
        eprintln!("warning: --max-pending is ignored without --handler-offload");
    }
    // `--http2` REQUIRES `--handler-offload`: without offload, h2's multiplexed
    // streams all stall on the single inline handler (one PHP call at a time per
    // worker), which defeats the point of h2 and can deadlock under backpressure.
    // Hard error (exit 2) — not a warning — because the misconfiguration is
    // silent at runtime and looks like a hang.
    if http2 && !handler_offload {
        eprintln!(
            "error: --http2 requires --handler-offload (without offload, h2 multiplexed \
             streams all stall on the single inline handler; see --help)"
        );
        return ParsedArgs::Exit(2);
    }
    // `--http2-max-streams` must be at least 1 when set; 0 streams means no h2
    // traffic can ever flow. Hard error (exit 2). The operator disables h2 by
    // simply not passing `--http2` (it is opt-in, default off).
    if http2_max_streams_set && http2_max_streams < 1 {
        eprintln!(
            "error: --http2-max-streams must be greater than 0 (omit --http2 to \
             disable HTTP/2; try --help)"
        );
        return ParsedArgs::Exit(2);
    }
    // Inert h2 tunables: warn (do not error) when the operator passed one of the
    // tunables without `--http2`, so a stray flag is not silently misleading.
    if http2_max_streams_set && !http2 {
        eprintln!("warning: --http2-max-streams is ignored without --http2");
    }
    if http2_max_header_size_set && !http2 {
        eprintln!("warning: --http2-max-header-size is ignored without --http2");
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
            dispatch_mode,
            dispatch_backlog: dispatch_backlog.max(1),
            handler_offload,
            max_pending: max_pending.max(1),
            http2,
            http2_max_streams,
            http2_max_header_size,
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
pub(crate) enum WorkerKind {
    /// Classic `--web`: re-execute the top-level PHP handler per request.
    Classic { handler: extern "C" fn() },
    /// `--web-worker`: boot PHP once, then register + enter the Rust worker loop.
    Worker { boot: extern "C" fn() },
    /// `--web-worker=script`: register the top-level body directly as the
    /// per-request handler (void ABI) and enter the Rust worker loop, with no
    /// separate PHP boot/register phase.
    Script { handler: handler::ScriptHandler },
}

/// Forks one worker child that serves forever, returning `(pid, boot_pipe_rd,
/// master_chan)` in the master. `boot_pipe_rd` is the boot-signal pipe read end
/// (worker/script modes) or `None` (classic). `master_chan` is the master end of
/// the dispatch socketpair when `master_dispatch` is set (`--dispatch master`),
/// else `None`. The child restores default signal disposition, installs the
/// per-mode config/listen address, and never returns. A fork failure aborts the
/// whole process. Used for both initial spawn and respawn, in both dispatch modes.
///
/// In master mode a socketpair is created before fork (like the boot pipe): the
/// child keeps its end (installed via `dispatch::set_child_dispatch_chan`, which
/// makes the serve loop use `ConnSource::Master` and NOT bind a listener) and
/// closes `child_close_fds` (sibling master ends + the listener fd) so it does not
/// inherit descriptors it must not hold; the master keeps the other end.
pub(crate) fn spawn_worker(
    listen: &str,
    kind: WorkerKind,
    cfg: WorkerConfig,
    master_dispatch: bool,
    child_close_fds: &[i32],
) -> (libc::pid_t, Option<i32>, Option<i32>) {
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
    // Master dispatch: create the socketpair before fork (child end `b`, master
    // end `a`). Failure aborts the master (a partial pool would deadlock).
    let dispatch_pair = if master_dispatch {
        match dispatch::socketpair_cloexec() {
            Ok(pair) => Some(pair),
            Err(e) => {
                eprintln!("error: dispatch socketpair creation failed: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        None
    };
    match unsafe { libc::fork() } {
        -1 => {
            eprintln!("error: fork failed");
            std::process::exit(1);
        }
        0 => {
            reset_signal_handlers_to_default();
            // Master dispatch: keep the child end, close the master end + every
            // inherited sibling master end + the listener fd, and install the child
            // end so the serve loop selects `ConnSource::Master` (no bind).
            if let Some((master_end, child_end)) = dispatch_pair {
                // SAFETY: the master end and the passed fds belong to the master; the
                // child must not hold them (they would break master-crash EOF and
                // leak the listener into a worker that never accepts).
                unsafe {
                    libc::close(master_end);
                    for &fd in child_close_fds {
                        libc::close(fd);
                    }
                }
                dispatch::set_child_dispatch_chan(child_end);
            }
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
            // Master dispatch: close the child end, keep the master end.
            let chan = dispatch_pair.map(|(master_end, child_end)| {
                // SAFETY: the child end belongs to the worker; close the master's copy.
                unsafe { libc::close(child_end); }
                master_end
            });
            (pid, rd, chan)
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
            let (new_pid, new_rd, _) = spawn_worker(listen, kind.clone(), cfg, false, &[]);
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
pub(crate) fn classify_crash(boot_pipe_rd: Option<i32>, spawned_at: Instant) -> bool {
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
        match crate::tls::load_acceptor(cert, key, args.http2) {
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

/// Runs the `--dispatch master` fd-dispatch path, shared by all three web modes.
/// Forks every worker with its own socketpair (each worker closes the master ends
/// of its already-forked siblings so no worker inherits another's dispatch
/// channel), binds the SINGLE plain listener AFTER the fork loop (so no worker
/// inherits it), logs the startup line, and drives `dispatch::master_loop` in
/// place of `supervise`. `mode_label` names the mode in the startup line. Returns
/// the master exit code. The kernel path is unaffected — `master_loop` is a
/// strictly parallel alternative selected only here.
fn run_master(
    args: &ServerArgs,
    kind: WorkerKind,
    cfg: WorkerConfig,
    dispatch: DispatchConfig,
    mode_label: &str,
) -> i32 {
    let mut workers: Vec<MasterWorker> = Vec::with_capacity(args.workers);
    for _ in 0..args.workers {
        // A worker forked at position k must close the master ends AND boot-pipe
        // read ends of workers 0..k, so it inherits no live sibling fd. The listener
        // does not exist yet, so it is not in the close set at initial spawn
        // (respawns add it and the queued backlog fds — see `dispatch::master_loop`).
        let mut close_fds: Vec<i32> = Vec::new();
        for w in &workers {
            close_fds.push(w.chan());
            if let Some(rd) = w.boot_pipe_rd() {
                close_fds.push(rd);
            }
        }
        let (pid, rd, chan) = spawn_worker(&args.listen, kind.clone(), cfg, true, &close_fds);
        let chan = chan.expect("master dispatch spawn must return a socketpair master end");
        workers.push(MasterWorker::new(pid, chan, rd));
    }
    let addr: SocketAddr = match args.listen.parse() {
        Ok(a) => a,
        Err(_) => {
            eprintln!("error: invalid --listen address {:?}", args.listen);
            dispatch::reap_workers(&workers);
            return 2;
        }
    };
    let listener = match crate::worker::plain_listener(addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("elephc-web: failed to bind {}: {}", addr, e);
            dispatch::reap_workers(&workers);
            return 1;
        }
    };
    eprintln!(
        "elephc-web: listening on {}://{} ({} worker{}{}, master dispatch)",
        listen_scheme(args),
        args.listen,
        args.workers,
        if args.workers == 1 { "" } else { "s" },
        mode_label,
    );
    dispatch::master_loop(listener, &args.listen, kind, cfg, dispatch, workers)
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
    let dispatch = args.dispatch_config();
    // Master dispatch is a strictly parallel path; the kernel path below is
    // unchanged.
    if dispatch.mode == DispatchMode::Master {
        return run_master(&args, kind, cfg, dispatch, "");
    }
    let mut children: Vec<(libc::pid_t, Option<i32>, Instant)> = Vec::new();
    for _ in 0..args.workers {
        let (pid, rd, _) = spawn_worker(&args.listen, kind.clone(), cfg, false, &[]);
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
    let dispatch = args.dispatch_config();
    if dispatch.mode == DispatchMode::Master {
        return run_master(&args, kind, cfg, dispatch, ", web-worker mode");
    }
    let mut children: Vec<(libc::pid_t, Option<i32>, Instant)> = Vec::new();
    for _ in 0..args.workers {
        let (pid, rd, _) = spawn_worker(&args.listen, kind.clone(), cfg, false, &[]);
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
    let dispatch = args.dispatch_config();
    if dispatch.mode == DispatchMode::Master {
        return run_master(&args, kind, cfg, dispatch, ", web-worker=script mode");
    }
    let mut children: Vec<(libc::pid_t, Option<i32>, Instant)> = Vec::new();
    for _ in 0..args.workers {
        let (pid, rd, _) = spawn_worker(&args.listen, kind.clone(), cfg, false, &[]);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    // Purpose:
    // Unit tests for runtime-arg parsing of the offload flags
    // (`--handler-offload`, `--max-pending`) and their validation.
    //
    // Called from:
    // - `cargo test` through Rust's test harness.
    //
    // Key details:
    // - `parse_args` takes a C `argv`; each test builds one from owned `CString`s
    //   kept alive for the call. `--listen` is always supplied so a missing-listen
    //   exit-2 never masks the assertion under test.

    /// Builds a C `argv` from `["prog", args...]` and runs `parse_args` in classic
    /// mode, keeping the backing `CString`s alive for the duration of the call.
    fn parse(args: &[&str]) -> ParsedArgs {
        let owned: Vec<CString> = std::iter::once("prog")
            .chain(args.iter().copied())
            .map(|s| CString::new(s).unwrap())
            .collect();
        let ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        parse_args(ptrs.len() as i32, ptrs.as_ptr(), false)
    }

    /// Verifies offload defaults: off, with the default 16-deep queue.
    #[test]
    fn offload_defaults_off_pending_16() {
        match parse(&["--listen", "127.0.0.1:0"]) {
            ParsedArgs::Run(a) => {
                assert!(!a.handler_offload, "offload must default off");
                assert_eq!(a.max_pending, 16, "default queue depth must be 16");
            }
            ParsedArgs::Exit(c) => panic!("expected Run, got Exit({c})"),
        }
    }

    /// Verifies `--handler-offload` enables offload and `--max-pending N` threads
    /// the queue depth through to the config.
    #[test]
    fn offload_flag_and_max_pending_parse() {
        match parse(&[
            "--listen",
            "127.0.0.1:0",
            "--handler-offload",
            "--max-pending",
            "32",
        ]) {
            ParsedArgs::Run(a) => {
                assert!(a.handler_offload, "--handler-offload must enable offload");
                assert_eq!(a.max_pending, 32, "--max-pending must be applied");
            }
            ParsedArgs::Exit(c) => panic!("expected Run, got Exit({c})"),
        }
    }

    /// Verifies `--max-pending 0` is a usage error (exit 2): an unbounded queue is
    /// an OOM vector, so it is rejected rather than silently clamped.
    #[test]
    fn max_pending_zero_is_exit_2() {
        match parse(&[
            "--listen",
            "127.0.0.1:0",
            "--handler-offload",
            "--max-pending",
            "0",
        ]) {
            ParsedArgs::Exit(2) => {}
            ParsedArgs::Exit(c) => panic!("expected Exit(2) for --max-pending 0, got Exit({c})"),
            ParsedArgs::Run(_) => panic!("expected Exit(2) for --max-pending 0, got Run"),
        }
    }

    /// Verifies `--max-pending` without `--handler-offload` still yields a runnable
    /// config (warning only, not an error), with the value applied.
    #[test]
    fn max_pending_without_offload_warns_but_runs() {
        match parse(&["--listen", "127.0.0.1:0", "--max-pending", "8"]) {
            ParsedArgs::Run(a) => {
                assert!(!a.handler_offload, "offload stays off");
                assert_eq!(a.max_pending, 8, "value still applied");
            }
            ParsedArgs::Exit(c) => panic!("expected Run, got Exit({c})"),
        }
    }

    /// Verifies HTTP/2 defaults: opt-in OFF, 8 streams, 64 KiB header budget —
    /// so a plain `--listen` invocation never silently turns h2 on.
    #[test]
    fn http2_defaults_off_streams_8_header_64kib() {
        match parse(&["--listen", "127.0.0.1:0"]) {
            ParsedArgs::Run(a) => {
                assert!(!a.http2, "--http2 must default off (opt-in)");
                assert_eq!(a.http2_max_streams, 8, "default max_streams must be 8");
                assert_eq!(
                    a.http2_max_header_size, 64 * 1024,
                    "default max_header_size must be 64 KiB"
                );
                assert!(!a.handler_offload, "offload stays off when --http2 absent");
            }
            ParsedArgs::Exit(c) => panic!("expected Run, got Exit({c})"),
        }
    }

    /// Verifies `--http2` + `--handler-offload` parse through to the config and
    /// that the tunables are honored when overridden.
    #[test]
    fn http2_flags_parse_with_offload() {
        match parse(&[
            "--listen",
            "127.0.0.1:0",
            "--handler-offload",
            "--http2",
            "--http2-max-streams",
            "4",
            "--http2-max-header-size",
            "32768",
        ]) {
            ParsedArgs::Run(a) => {
                assert!(a.http2, "--http2 must be on");
                assert!(a.handler_offload, "--handler-offload must be on");
                assert_eq!(a.http2_max_streams, 4, "--http2-max-streams override");
                assert_eq!(
                    a.http2_max_header_size, 32768,
                    "--http2-max-header-size override"
                );
            }
            ParsedArgs::Exit(c) => panic!("expected Run, got Exit({c})"),
        }
    }

    /// Verifies `--http2-max-streams 0` is a usage error (exit 2): zero streams
    /// means no h2 traffic can ever flow, so it is rejected rather than clamped.
    #[test]
    fn http2_max_streams_zero_is_exit_2() {
        match parse(&[
            "--listen",
            "127.0.0.1:0",
            "--handler-offload",
            "--http2",
            "--http2-max-streams",
            "0",
        ]) {
            ParsedArgs::Exit(2) => {}
            ParsedArgs::Exit(c) => {
                panic!("expected Exit(2) for --http2-max-streams 0, got Exit({c})")
            }
            ParsedArgs::Run(_) => panic!("expected Exit(2) for --http2-max-streams 0, got Run"),
        }
    }

    /// Verifies `--http2` without `--handler-offload` is a hard exit 2: without
    /// offload, h2 multiplexed streams stall on the single inline handler, so
    /// the misconfiguration is rejected up front rather than hanging at runtime.
    #[test]
    fn http2_without_offload_is_exit_2() {
        match parse(&["--listen", "127.0.0.1:0", "--http2"]) {
            ParsedArgs::Exit(2) => {}
            ParsedArgs::Exit(c) => panic!("expected Exit(2) for --http2 w/o offload, got Exit({c})"),
            ParsedArgs::Run(_) => panic!("expected Exit(2) for --http2 w/o offload, got Run"),
        }
    }
}
