//! Purpose:
//! The `--web` server entry point: parse the binary's runtime args, prefork N
//! worker processes, and supervise them. Each worker serves HTTP independently.
//!
//! Called from:
//! - The compiled `--web` binary's process entry, through one mode-specific C symbol.
//!
//! Key details:
//! - fork() happens BEFORE any tokio runtime is created (tokio does not survive
//!   fork); pool/request workers prestart a threadless handler broker, while
//!   worker isolation retains the original in-process serving path.
//! - --listen host:port or unix:/path is required; a Unix listener is bound
//!   once here, before the fork loop, and inherited by every worker.

mod args;

use std::ffi::c_char;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant};

use args::{parse_args, ParsedArgs, ServerArgs};
use crate::handler_broker::BrokerMode;
use crate::isolated_worker;
use crate::listener::{self, ListenAddr, ListenSource, SharedTcp};
use crate::worker;

/// Version-one generated callbacks; all PHP exceptions are contained before returning to Rust.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApplicationCallbacks {
    /// Executes the original top level once in the forked worker.
    pub bootstrap: extern "C" fn(),
    /// Returns one only after bootstrap registration has committed.
    pub ready: extern "C" fn() -> i64,
    /// Prepares, dispatches and cleans up one request.
    pub request: extern "C" fn(),
    /// Runs application shutdown and releases remaining PHP state.
    pub shutdown: extern "C" fn(),
}

/// Private adapter phase: bootstrap (0), compiler commit (1), requests (2), shutdown (3).
static APPLICATION_PHASE: AtomicI64 = AtomicI64::new(0);

/// Lets the PHP adapter validate phase methods without allowing application code to set its flag.
#[no_mangle]
pub extern "C" fn elephc_web_application_phase() -> i64 {
    APPLICATION_PHASE.load(Ordering::Relaxed)
}

/// Called only by the compiler's final bootstrap statements, before publishing ready.
#[no_mangle]
pub extern "C" fn elephc_web_application_commit_bootstrap() {
    let _ = APPLICATION_PHASE.compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed);
}

/// Compile-time model selected by the generated bridge symbol.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum IsolationMode {
    /// Execute PHP synchronously inside the prefork worker.
    Worker,
    /// Reuse a supervised pool of persistent handler processes.
    Pool,
    /// Fork one disposable handler process per request.
    Request,
}

impl IsolationMode {
    /// Returns the operator-facing name printed at server startup.
    const fn name(self) -> &'static str {
        match self {
            Self::Worker => "worker",
            Self::Pool => "pool",
            Self::Request => "request",
        }
    }

    /// Converts an isolated server mode to its broker lifecycle model.
    const fn broker_mode(self) -> Option<BrokerMode> {
        match self {
            Self::Worker => None,
            Self::Pool => Some(BrokerMode::Pool),
            Self::Request => Some(BrokerMode::Request),
        }
    }
}

/// A worker that dies within this window of being spawned counts as a crash-on-
/// startup; too many in a row (e.g. a bind failure or a handler that crashes on
/// every request) abort the master instead of fork-looping forever.
const FAST_DEATH: Duration = Duration::from_millis(1000);
/// Consecutive fast worker deaths tolerated before the master gives up.
const MAX_FAST_DEATHS: u32 = 10;
/// Failed application bootstraps count toward startup failure regardless of their duration.
const BOOTSTRAP_FAILURE_EXIT_CODE: i32 = 87;

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

/// Forks one worker child that serves until a planned `--max-requests` recycle
/// (then exits with `worker::RECYCLE_EXIT_CODE`), returning the child pid in the
/// master. The child restores default signal disposition and never returns. A
/// fork failure aborts the whole process. Used for both initial spawn and respawn.
fn spawn_worker(
    listen: &ListenAddr,
    master_unix: Option<&std::os::unix::net::UnixListener>,
    shared_tcp: Option<&SharedTcp>,
    worker_index: usize,
    handler: extern "C" fn(),
    args: &ServerArgs,
    isolation: IsolationMode,
    application: Option<ApplicationCallbacks>,
) -> libc::pid_t {
    // A SIGTERM can arrive between the master's shutdown check and the
    // child's handler reset. Block both signals across `fork()`, reset the
    // child dispositions, then unblock: the child never runs the master's
    // flag-only handler and swallows its own teardown signal.
    let mut blocked: libc::sigset_t = unsafe { std::mem::zeroed() };
    let mut previous: libc::sigset_t = unsafe { std::mem::zeroed() };
    unsafe {
        libc::sigemptyset(&mut blocked);
        libc::sigaddset(&mut blocked, libc::SIGINT);
        libc::sigaddset(&mut blocked, libc::SIGTERM);
        libc::pthread_sigmask(libc::SIG_BLOCK, &blocked, &mut previous);
    }
    match unsafe { libc::fork() } {
        -1 => {
            unsafe {
                libc::pthread_sigmask(
                    libc::SIG_SETMASK,
                    &previous,
                    std::ptr::null_mut(),
                );
            }
            eprintln!("error: fork failed");
            std::process::exit(1);
        }
        0 => {
            reset_signal_handlers_to_default();
            unsafe {
                libc::pthread_sigmask(
                    libc::SIG_SETMASK,
                    &previous,
                    std::ptr::null_mut(),
                );
            }
            let source = match ListenSource::for_worker(
                listen,
                master_unix,
                shared_tcp,
                worker_index,
            ) {
                Ok(source) => source,
                Err(error) => {
                    eprintln!("elephc-web: failed to inherit the listening socket: {error}");
                    std::process::exit(1);
                }
            };
            #[cfg(target_vendor = "apple")]
            if let Some(shared) = shared_tcp {
                // The receiver duplicate is installed; the listener, the other
                // workers' channels, and the master's pending connections are
                // master-only. An orphaned worker must not keep the port open.
                shared.close_in_worker();
            }
            if let Some(callbacks) = application {
                worker::install_application_shutdown();
                crate::request_state::reset_response();
                APPLICATION_PHASE.store(0, Ordering::Relaxed);
                worker::with_application_signals_blocked(|| (callbacks.bootstrap)());
                if (callbacks.ready)() != 1 {
                    eprintln!("elephc-web: application bootstrap failed");
                    APPLICATION_PHASE.store(3, Ordering::Relaxed);
                    worker::with_application_signals_blocked(|| (callbacks.shutdown)());
                    std::process::exit(BOOTSTRAP_FAILURE_EXIT_CODE);
                }
                APPLICATION_PHASE.store(2, Ordering::Relaxed);
                worker::serve_application(source, callbacks.request, args.worker_config());
                crate::request_state::set_capture(false);
                APPLICATION_PHASE.store(3, Ordering::Relaxed);
                worker::with_application_signals_blocked(|| (callbacks.shutdown)());
                std::process::exit(worker::RECYCLE_EXIT_CODE);
            }
            match isolation.broker_mode() {
                None => worker::serve(source, handler, args.worker_config()),
                Some(mode) => {
                    isolated_worker::serve(source, handler, args.isolated_worker_config(mode))
                }
            }
            // serve() only returns when the worker stopped accepting on purpose
            // after serving its --max-requests quota; exit with the recycle code
            // so the reaper skips the crash-loop accounting for this death.
            std::process::exit(worker::RECYCLE_EXIT_CODE);
        }
        pid => {
            unsafe {
                libc::pthread_sigmask(
                    libc::SIG_SETMASK,
                    &previous,
                    std::ptr::null_mut(),
                );
            }
            pid
        }
    }
}

/// Returns true when a reaped worker's `waitpid` status is a planned
/// `--max-requests` recycle: a normal exit with `worker::RECYCLE_EXIT_CODE`.
/// Signal deaths and every other exit code are real crashes and keep feeding
/// the fast-death accounting. Without this distinction, sustained traffic with
/// a small `--max-requests` recycles workers faster than `FAST_DEATH` and the
/// master mistakes the healthy recycle churn for a startup crash loop.
fn is_planned_recycle(status: libc::c_int) -> bool {
    if !libc::WIFEXITED(status) {
        return false;
    }
    let exit_code = libc::WEXITSTATUS(status);
    exit_code == worker::RECYCLE_EXIT_CODE
        || exit_code == isolated_worker::RECYCLE_EXIT_CODE
}

/// Removes one exact worker PID from the supervised set and returns its spawn time.
///
/// Reparented broker/handler descendants may also be returned by `waitpid(-1)`
/// when the master is PID 1; those PIDs are reaped but must not trigger a worker
/// replacement or change the configured pool size.
fn remove_tracked_worker(
    children: &mut Vec<(libc::pid_t, Instant, usize)>,
    pid: libc::pid_t,
) -> Option<(Instant, usize)> {
    let index = children.iter().position(|(child, _, _)| *child == pid)?;
    let (_, spawned_at, worker_index) = children.remove(index);
    Some((spawned_at, worker_index))
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
    run_server(argc, argv, handler, IsolationMode::Worker, None)
}

/// Pool-isolated server entry selected by `--web-isolation=pool` at compilation.
#[no_mangle]
pub extern "C" fn elephc_web_run_pool(
    argc: i32,
    argv: *const *const c_char,
    handler: extern "C" fn(),
) -> i32 {
    run_server(argc, argv, handler, IsolationMode::Pool, None)
}

/// Request-isolated server entry selected by `--web-isolation=request` at compilation.
#[no_mangle]
pub extern "C" fn elephc_web_run_request(
    argc: i32,
    argv: *const *const c_char,
    handler: extern "C" fn(),
) -> i32 {
    run_server(argc, argv, handler, IsolationMode::Request, None)
}

/// Selects the post-fork application lifecycle without changing the legacy bridge entries.
///
/// # Safety
/// `callbacks` points to the compiler's static four-word callback table for this binary.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_run_application(
    argc: i32,
    argv: *const *const c_char,
    callbacks: *const ApplicationCallbacks,
) -> i32 {
    let Some(callbacks) = callbacks.as_ref().copied() else { return 1 };
    run_server(argc, argv, callbacks.request, IsolationMode::Worker, Some(callbacks))
}

/// Parses runtime arguments and supervises the selected compile-time server model.
fn run_server(
    argc: i32,
    argv: *const *const c_char,
    handler: extern "C" fn(),
    isolation: IsolationMode,
    application: Option<ApplicationCallbacks>,
) -> i32 {
    let args = match parse_args(argc, argv, isolation) {
        ParsedArgs::Run(a) => a,
        ParsedArgs::Exit(code) => return code,
    };
    install_signal_handlers();
    // Bind the shared TCP socket before forking on Apple targets. On Linux
    // install_shared is a no-op and workers keep their per-process
    // SO_REUSEPORT sockets. A Unix listener must always be bound once before
    // the fork loop: SO_REUSEPORT does not balance accept() across AF_UNIX
    // listeners. This runs before any worker fork or Tokio runtime, so
    // bind_unix_master may temporarily narrow the process umask without racing
    // another thread. A bind failure is a master startup error, not a worker
    // crash loop.
    let (master_unix, shared_tcp) = match &args.listen {
        ListenAddr::Tcp(addr) => match listener::install_shared(*addr, args.workers) {
            Ok(shared) => (None, shared),
            Err(error) => {
                eprintln!("elephc-web: failed to bind {addr}: {error}");
                return 1;
            }
        },
        ListenAddr::Unix(path) => match listener::bind_unix_master(path) {
            Ok(listener) => (Some(listener), None),
            Err(error) => {
                eprintln!(
                    "elephc-web: failed to bind unix:{}: {error}",
                    path.display()
                );
                return 1;
            }
        },
    };
    // Make descriptor-map mutation safe across the forks below. A child forked
    // while the acceptor holds `pending` would inherit a locked pthread mutex.
    #[cfg(target_vendor = "apple")]
    listener::install_fork_gate();
    // Fork workers BEFORE creating any tokio runtime. Track each worker's spawn
    // time so a crash-on-startup loop (e.g. a failed bind) can be detected.
    let mut children: Vec<(libc::pid_t, Instant, usize)> = Vec::new();
    for worker_index in 0..args.workers {
        let pid = spawn_worker(
            &args.listen,
            master_unix.as_ref(),
            shared_tcp.as_deref(),
            worker_index,
            handler,
            &args,
            isolation,
            application,
        );
        children.push((pid, Instant::now(), worker_index));
    }
    #[cfg(target_vendor = "apple")]
    let accept_thread = shared_tcp
        .as_ref()
        .map(|shared| shared.start_accepting());
    #[cfg(not(target_vendor = "apple"))]
    let accept_thread: Option<std::thread::JoinHandle<()>> = None;
    eprintln!(
        "elephc-web: listening on {} ({} worker{}, isolation={})",
        args.listen.display(),
        args.workers,
        if args.workers == 1 { "" } else { "s" },
        isolation.name()
    );
    // Supervise: wait for any child; break on a shutdown request (SIGINT/SIGTERM).
    let mut fast_deaths: u32 = 0;
    let mut exit_code = 0;
    loop {
        if SHUTDOWN.load(Ordering::SeqCst) {
            break;
        }
        let mut status = 0;
        // A blocking `waitpid` can miss SIGTERM when the signal arrives after
        // the check above but before the call: the handler runs, then the
        // supervisor sleeps in `waitpid` with no child event to wake it. Poll
        // with `WNOHANG` so the shutdown flag is always re-checked.
        let pid = loop {
            if SHUTDOWN.load(Ordering::SeqCst) {
                break 0;
            }
            let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
            if pid != 0 {
                break pid;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        if SHUTDOWN.load(Ordering::SeqCst) {
            break;
        }
        if pid > 0 {
            let Some((spawned_at, worker_index)) =
                remove_tracked_worker(&mut children, pid)
            else {
                continue;
            };
            if SHUTDOWN.load(Ordering::SeqCst) {
                if children.is_empty() {
                    break;
                }
                continue;
            }
            // Crash-loop guard: if workers keep dying immediately after spawn,
            // stop respawning (otherwise a failed bind fork-loops forever). A
            // planned --max-requests recycle is exempt: it neither increments
            // nor resets the streak, so healthy recycle churn cannot trip the
            // guard yet also cannot mask a real crash loop interleaved with it.
            if !is_planned_recycle(status) {
                if libc::WIFSIGNALED(status) {
                    eprintln!(
                        "elephc-web: worker {pid} terminated by signal {}",
                        libc::WTERMSIG(status)
                    );
                } else if libc::WIFEXITED(status) {
                    eprintln!(
                        "elephc-web: worker {pid} exited with status {}",
                        libc::WEXITSTATUS(status)
                    );
                }
                let bootstrap_failed = application.is_some()
                    && libc::WIFEXITED(status)
                    && libc::WEXITSTATUS(status) == BOOTSTRAP_FAILURE_EXIT_CODE;
                if bootstrap_failed || spawned_at.elapsed() < FAST_DEATH {
                    fast_deaths += 1;
                    if fast_deaths >= MAX_FAST_DEATHS {
                        if application.is_some() { exit_code = 1; }
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
            }
            // The worker is gone (crash or recycle): replace it to keep the pool at N.
            let new_pid = spawn_worker(
                &args.listen,
                master_unix.as_ref(),
                shared_tcp.as_deref(),
                worker_index,
                handler,
                &args,
                isolation,
                application,
            );
            children.push((new_pid, Instant::now(), worker_index));
        } else if pid == -1 {
            // ECHILD: nothing left to wait for. EINTR: a signal arrived → re-loop
            // and re-check SHUTDOWN at the top.
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD) {
                break;
            }
        }
    }
    if let Some(shared) = &shared_tcp {
        shared.stop();
    }
    // Stop accepting before stopping the workers. Joining the acceptor also
    // gives a descriptor that was just handed off a moment to reach a worker's
    // accept loop, so shutdown does not reset the last accepted connection.
    if let Some(handle) = accept_thread {
        let _ = handle.join();
        std::thread::sleep(Duration::from_millis(50));
    }
    // Clean teardown: ask every still-tracked worker to terminate, then reap.
    for &(pid, _, _) in &children {
        unsafe { libc::kill(pid, libc::SIGTERM); }
    }
    // A child that missed SIGTERM must not hold the master forever and make a
    // supervisor wait for its stop timeout. Give the pool one bounded grace
    // period, then SIGKILL and reap what remains.
    let deadline = Instant::now() + Duration::from_secs(3);
    for &(pid, _, _) in &children {
        let mut status = 0;
        loop {
            let waited = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
            if waited == pid
                || (waited == -1
                    && std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD))
            {
                break;
            }
            if Instant::now() >= deadline {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                    libc::waitpid(pid, &mut status, 0);
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    if let ListenAddr::Unix(path) = &args.listen {
        listener::remove_unix_socket(path);
    }
    exit_code
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a `waitpid` status word for a normal exit with `code`. Both
    /// supported unix families (Linux and macOS) encode a normal exit as
    /// `code << 8` with zeroed low bits, which is what `WIFEXITED`/`WEXITSTATUS`
    /// decode.
    fn exited_status(code: i32) -> libc::c_int {
        (code & 0xff) << 8
    }

    /// A normal exit with the recycle code must be classified as a planned
    /// recycle so the crash-loop guard skips it (regression for issue #516:
    /// sustained traffic with a small --max-requests shut the server down).
    #[test]
    fn recycle_exit_is_planned() {
        assert!(is_planned_recycle(exited_status(worker::RECYCLE_EXIT_CODE)));
    }

    /// Clean exits and error exits are NOT planned recycles: they must keep
    /// feeding the fast-death accounting so real crash loops still trip the guard.
    #[test]
    fn other_exit_codes_are_not_planned() {
        assert!(!is_planned_recycle(exited_status(0)));
        assert!(!is_planned_recycle(exited_status(1)));
        assert!(!is_planned_recycle(exited_status(2)));
    }

    /// A signal death is never a planned recycle, even when the raw status bits
    /// could coincide numerically: `WIFEXITED` must gate the exit-code check.
    /// `waitpid` encodes "killed by signal N" as N in the low 7 bits.
    #[test]
    fn signal_death_is_not_planned() {
        assert!(!is_planned_recycle(libc::SIGSEGV));
        assert!(!is_planned_recycle(libc::SIGKILL));
        assert!(!is_planned_recycle(libc::SIGTERM));
    }

    /// A reparented descendant reaped by the master must not consume a tracked
    /// worker slot or cause the supervisor to spawn an extra worker.
    #[test]
    fn untracked_reaped_pid_does_not_change_worker_set() {
        let first = Instant::now();
        let second = Instant::now();
        let mut children = vec![(101, first, 0), (202, second, 1)];

        assert!(remove_tracked_worker(&mut children, 303).is_none());
        assert_eq!(children.len(), 2);
        assert!(remove_tracked_worker(&mut children, 101).is_some());
        assert_eq!(
            children.iter().map(|(pid, _, _)| *pid).collect::<Vec<_>>(),
            vec![202]
        );
    }

}
