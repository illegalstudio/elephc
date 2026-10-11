//! Purpose:
//! The `--web` server entry point: parse the binary's runtime args and dispatch
//! to the Unix prefork supervisor or the Windows single-process event loop.
//!
//! Called from:
//! - The compiled `--web` binary's process entry, through one mode-specific C symbol.
//!
//! Key details:
//! - fork() happens BEFORE any tokio runtime is created (tokio does not survive
//!   fork); pool/request workers prestart a threadless handler broker, while
//!   worker isolation retains the original in-process serving path.
//! - --listen host:port is required; without it the process errors and exits.

mod args;

use std::ffi::c_char;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(unix)]
use std::time::{Duration, Instant};

use args::{parse_args, ParsedArgs, ServerArgs};
#[cfg(unix)]
use crate::handler_broker::BrokerMode;
#[cfg(unix)]
use crate::isolated_worker;
use crate::worker;

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

    /// Converts an isolated server mode to its Unix broker lifecycle model.
    #[cfg(unix)]
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
#[cfg(unix)]
const FAST_DEATH: Duration = Duration::from_millis(1000);
/// Consecutive fast worker deaths tolerated before the master gives up.
#[cfg(unix)]
const MAX_FAST_DEATHS: u32 = 10;

/// Set by the SIGINT/SIGTERM handler so the master supervision loop can break and
/// shut workers down cleanly. Async-signal-safe: the handler only stores to it.
pub(crate) static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Async-signal-safe SIGINT/SIGTERM handler: records the shutdown request only.
#[cfg(unix)]
extern "C" fn handle_shutdown_signal(_sig: libc::c_int) {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

/// Installs `handle_shutdown_signal` for SIGINT and SIGTERM WITHOUT `SA_RESTART`,
/// so a signal interrupts the master's blocking `waitpid` (returns EINTR) instead
/// of silently restarting it.
#[cfg(unix)]
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
#[cfg(unix)]
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
#[cfg(unix)]
fn spawn_worker(
    listen: &str,
    handler: extern "C" fn(),
    args: &ServerArgs,
    isolation: IsolationMode,
) -> libc::pid_t {
    match unsafe { libc::fork() } {
        -1 => {
            eprintln!("error: fork failed");
            std::process::exit(1);
        }
        0 => {
            reset_signal_handlers_to_default();
            match isolation.broker_mode() {
                None => worker::serve(listen, handler, args.worker_config(), None),
                Some(mode) => {
                    isolated_worker::serve(listen, handler, args.isolated_worker_config(mode))
                }
            }
            // serve() only returns when the worker stopped accepting on purpose
            // after serving its --max-requests quota; exit with the recycle code
            // so the reaper skips the crash-loop accounting for this death.
            std::process::exit(worker::RECYCLE_EXIT_CODE);
        }
        pid => pid,
    }
}

/// Returns true when a reaped worker's `waitpid` status is a planned
/// `--max-requests` recycle: a normal exit with `worker::RECYCLE_EXIT_CODE`.
/// Signal deaths and every other exit code are real crashes and keep feeding
/// the fast-death accounting. Without this distinction, sustained traffic with
/// a small `--max-requests` recycles workers faster than `FAST_DEATH` and the
/// master mistakes the healthy recycle churn for a startup crash loop.
#[cfg(unix)]
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
#[cfg(unix)]
fn remove_tracked_worker(
    children: &mut Vec<(libc::pid_t, Instant)>,
    pid: libc::pid_t,
) -> Option<Instant> {
    let index = children.iter().position(|(child, _)| *child == pid)?;
    Some(children.remove(index).1)
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
    run_server_entry(argc, argv, handler, IsolationMode::Worker)
}

/// Pool-isolated server entry selected by `--web-isolation=pool` at compilation.
#[no_mangle]
pub extern "C" fn elephc_web_run_pool(
    argc: i32,
    argv: *const *const c_char,
    handler: extern "C" fn(),
) -> i32 {
    run_server_entry(argc, argv, handler, IsolationMode::Pool)
}

/// Request-isolated server entry selected by `--web-isolation=request` at compilation.
#[no_mangle]
pub extern "C" fn elephc_web_run_request(
    argc: i32,
    argv: *const *const c_char,
    handler: extern "C" fn(),
) -> i32 {
    run_server_entry(argc, argv, handler, IsolationMode::Request)
}

/// Parses runtime arguments and supervises the selected compile-time server model.
fn run_server_entry(
    argc: i32,
    argv: *const *const c_char,
    handler: extern "C" fn(),
    isolation: IsolationMode,
) -> i32 {
    let args = match parse_args(argc, argv, isolation) {
        ParsedArgs::Run(a) => a,
        ParsedArgs::Exit(code) => return code,
    };
    SHUTDOWN.store(false, Ordering::SeqCst);
    run_server(args, handler, isolation)
}

/// Runs the Unix prefork supervisor while keeping every Unix-only API outside
/// Windows builds.
#[cfg(unix)]
fn run_server(args: ServerArgs, handler: extern "C" fn(), isolation: IsolationMode) -> i32 {
    install_signal_handlers();
    // Fork workers BEFORE creating any tokio runtime. Track each worker's spawn
    // time so a crash-on-startup loop (e.g. a failed bind) can be detected.
    let mut children: Vec<(libc::pid_t, Instant)> = Vec::new();
    for _ in 0..args.workers {
        let pid = spawn_worker(&args.listen, handler, &args, isolation);
        children.push((pid, Instant::now()));
    }
    eprintln!(
        "elephc-web: listening on http://{} ({} worker{}, isolation={})",
        args.listen,
        args.workers,
        if args.workers == 1 { "" } else { "s" },
        isolation.name()
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
            let Some(spawned_at) = remove_tracked_worker(&mut children, pid) else {
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
                if spawned_at.elapsed() < FAST_DEATH {
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
            }
            // The worker is gone (crash or recycle): replace it to keep the pool at N.
            let new_pid = spawn_worker(&args.listen, handler, &args, isolation);
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

/// Runs the Windows server in one process and one PHP execution thread.
///
/// Hyper still multiplexes connection I/O through local tasks, but the compiled
/// PHP handler and its process-global request state never execute concurrently.
/// `--max-requests` stops the Windows process cleanly so an external service
/// manager can restart with fresh state; Ctrl-C sets the shared shutdown flag
/// and lets the accept poll return cleanly.
#[cfg(windows)]
fn run_server(args: ServerArgs, handler: extern "C" fn(), isolation: IsolationMode) -> i32 {
    if let Err(error) = ctrlc::set_handler(|| SHUTDOWN.store(true, Ordering::SeqCst)) {
        eprintln!("elephc-web: failed to install Ctrl-C handler: {error}");
        return 1;
    }
    if args.workers != 1 {
        eprintln!(
            "elephc-web: Windows uses one event-loop worker; ignoring --workers {}",
            args.workers
        );
    }
    eprintln!(
        "elephc-web: listening on http://{} (Windows event-loop worker)",
        args.listen
    );
    if isolation != IsolationMode::Worker {
        eprintln!(
            "elephc-web: Windows currently runs --web-isolation={} in worker mode",
            isolation.name()
        );
    }
    worker::serve(&args.listen, handler, args.worker_config(), Some(&SHUTDOWN));
    0
}

/// Names the process model compiled for the current platform for diagnostics
/// and cfg-focused unit coverage.
#[cfg(test)]
fn worker_model_name() -> &'static str {
    if cfg!(windows) {
        "windows-event-loop"
    } else {
        "unix-prefork"
    }
}

#[cfg(all(test, unix))]
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
        let mut children = vec![(101, first), (202, second)];

        assert!(remove_tracked_worker(&mut children, 303).is_none());
        assert_eq!(children.len(), 2);
        assert!(remove_tracked_worker(&mut children, 101).is_some());
        assert_eq!(children.iter().map(|(pid, _)| *pid).collect::<Vec<_>>(), vec![202]);
    }

}

#[cfg(test)]
mod platform_tests {
    use super::*;

    /// Verifies cfg selection exposes exactly one documented server process model.
    #[test]
    fn worker_model_matches_target_family() {
        if cfg!(windows) {
            assert_eq!(worker_model_name(), "windows-event-loop");
        } else {
            assert_eq!(worker_model_name(), "unix-prefork");
        }
    }
}
