//! Purpose:
//! Parses the runtime command line of a compiled `--web` binary and produces
//! mode-appropriate worker configuration.
//!
//! Called from:
//! - `crate::server::run_server()` before the prefork supervisor starts.
//!
//! Key details:
//! - The isolation model is already fixed by the compiled entry symbol.
//! - Flags unavailable to that model fail with exit code 2 instead of being ignored.

use std::ffi::{c_char, CStr};

use crate::forwarded::TrustedProxies;
use crate::handler_broker::BrokerMode;
use crate::isolated_worker::WorkerConfig as IsolatedWorkerConfig;
use crate::listener::ListenAddr;
use crate::worker::WorkerConfig;

use super::IsolationMode;

/// `--help` text for the produced `--web` binary.
const WORKER_HELP: &str = "\
Usage: <binary> --listen HOST:PORT | unix:/PATH [options]

A standalone prefork HTTP server compiled with worker isolation.

Options:
  --listen HOST:PORT | unix:/PATH
                         Address to bind (required), e.g. 127.0.0.1:8080
                         or unix:/run/elephc/app.sock
  --trusted-proxy LIST   Comma-separated IPs/CIDRs (or 'unix') allowed to send
                         Forwarded / X-Forwarded-* headers; repeatable
  --workers N            Number of prefork worker processes (default: CPU count)
  --max-body-size BYTES  Max request body in bytes; 0 = unlimited (default: 8388608)
  --body-read-timeout N  Max seconds to receive a request body; 0 = unlimited (default: 30)
  --max-requests N       Drain and recycle after N completed requests; 0 = never (default: 0)
  --access-log           Log one line per request to stderr
  --max-execution-time N Kill and respawn a worker when its handler runs > N seconds; 0 = no limit
  --gzip                 Compress responses when the client sends Accept-Encoding: gzip
  --help                 Show this help and exit
  --version              Show the server version and exit";

/// `--help` text for binaries compiled with pool or request isolation.
const ISOLATED_HELP: &str = "\
Usage: <binary> --listen HOST:PORT | unix:/PATH [options]

A standalone prefork HTTP server compiled with isolated handlers.

Options:
  --listen HOST:PORT | unix:/PATH
                         Address to bind (required), e.g. 127.0.0.1:8080
                         or unix:/run/elephc/app.sock
  --trusted-proxy LIST   Comma-separated IPs/CIDRs (or 'unix') allowed to send
                         Forwarded / X-Forwarded-* headers; repeatable
  --workers N            Number of prefork web workers (default: CPU count)
  --handler-concurrency N
                         Handler processes per worker (default: 1)
  --max-handler-requests N
                         Pool requests before replacing one handler (default: 1000; pool only; 0 = never)
  --max-body-size BYTES  Max request body in bytes; 0 = unlimited (default: 8388608)
  --body-read-timeout N  Max seconds to receive a request body; 0 = unlimited (default: 30)
  --response-write-timeout N
                         Max seconds a response may wait on client backpressure; 0 = unlimited (default: 30)
  --max-requests N       Drain and recycle after N completed requests; 0 = never (default: 0)
  --access-log           Log one line per request to stderr
  --max-execution-time N Kill only the timed-out handler process; 0 = no limit
  --gzip                 Compress responses when the client sends Accept-Encoding: gzip
  --help                 Show this help and exit
  --version              Show the server version and exit";

/// Default request body cap in bytes (8 MiB), matching PHP's `post_max_size`.
const DEFAULT_MAX_BODY: usize = 8 * 1024 * 1024;
/// Default deadline for receiving a declared request body, in seconds.
const DEFAULT_BODY_READ_SECS: u64 = 30;
/// Default deadline for response writes stalled by client backpressure, in seconds.
const DEFAULT_RESPONSE_WRITE_SECS: u64 = 30;

/// Parsed server configuration from the binary's own argv.
pub(super) struct ServerArgs {
    pub(super) listen: ListenAddr,
    /// Peers allowed to supply Forwarded / X-Forwarded-* headers.
    trusted_proxies: TrustedProxies,
    pub(super) workers: usize,
    /// Handler processes supervised by each isolated web worker.
    handler_concurrency: usize,
    /// Requests served by one pool child before planned replacement.
    max_handler_requests: usize,
    /// Max request body in bytes; `0` means unlimited.
    max_body: usize,
    /// Body receive deadline in seconds; `0` means unlimited.
    body_read_secs: u64,
    /// Response backpressure deadline in seconds; `0` means unlimited.
    response_write_secs: u64,
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
    /// Builds the original in-process worker configuration.
    pub(super) fn worker_config(&self) -> WorkerConfig {
        WorkerConfig {
            max_body: self.max_body,
            max_requests: self.max_requests,
            access_log: self.access_log,
            max_exec_secs: self.max_exec_secs,
            gzip: self.gzip,
            body_read_secs: self.body_read_secs,
            trusted_proxies: self.trusted_proxies.clone(),
        }
    }

    /// Builds the broker-backed worker configuration for pool or request mode.
    pub(super) fn isolated_worker_config(&self, mode: BrokerMode) -> IsolatedWorkerConfig {
        IsolatedWorkerConfig {
            max_body: self.max_body,
            body_read_secs: self.body_read_secs,
            response_write_secs: self.response_write_secs,
            max_requests: self.max_requests,
            access_log: self.access_log,
            max_exec_secs: self.max_exec_secs,
            gzip: self.gzip,
            handler_mode: mode,
            handler_concurrency: self.handler_concurrency,
            max_handler_requests: self.max_handler_requests,
            trusted_proxies: self.trusted_proxies.clone(),
        }
    }
}

/// Outcome of argument parsing: runnable config, requested early exit, or usage error.
pub(super) enum ParsedArgs {
    /// Start the server with these validated arguments.
    Run(ServerArgs),
    /// Return the contained process exit code without starting workers.
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

/// Parses argv into runnable configuration or a specific early-exit code.
pub(super) fn parse_args(
    argc: i32,
    argv: *const *const c_char,
    isolation: IsolationMode,
) -> ParsedArgs {
    let args = collect_args(argc, argv);
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "{}",
            if isolation == IsolationMode::Worker {
                WORKER_HELP
            } else {
                ISOLATED_HELP
            }
        );
        return ParsedArgs::Exit(0);
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("elephc-web {}", env!("CARGO_PKG_VERSION"));
        return ParsedArgs::Exit(0);
    }
    // `--body-read-timeout` IS ACCEPTED IN WORKER MODE. It used to be listed here, so the
    // default isolation had no deadline for receiving a body at all and the flag that would
    // have set one was refused (issue #887). The in-process worker now enforces it on every
    // body path, exactly as the isolated one does.
    let isolated_only = [
        "--handler-concurrency",
        "--max-handler-requests",
        "--response-write-timeout",
    ];
    if isolation == IsolationMode::Worker {
        if let Some(flag) = args
            .iter()
            .find(|arg| isolated_only.contains(&arg.as_str()))
        {
            eprintln!(
                "error: {flag} requires a binary compiled with --web-isolation=pool or request"
            );
            return ParsedArgs::Exit(2);
        }
    }
    if isolation == IsolationMode::Request
        && args.iter().any(|arg| arg == "--max-handler-requests")
    {
        eprintln!("error: --max-handler-requests is available only with --web-isolation=pool");
        return ParsedArgs::Exit(2);
    }

    let mut listen: Option<ListenAddr> = None;
    let mut trusted_proxy_values: Vec<String> = Vec::new();
    let mut workers: usize = default_workers();
    let mut handler_concurrency: usize = 1;
    let mut max_handler_requests: usize = 1000;
    let mut max_body: usize = DEFAULT_MAX_BODY;
    let mut body_read_secs = DEFAULT_BODY_READ_SECS;
    let mut response_write_secs = DEFAULT_RESPONSE_WRITE_SECS;
    let mut max_requests: usize = 0;
    let mut access_log = false;
    let mut max_exec_secs: u32 = 0;
    let mut gzip = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--listen" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    eprintln!("error: --listen requires HOST:PORT or unix:/PATH");
                    return ParsedArgs::Exit(2);
                };
                match ListenAddr::parse(value) {
                    Ok(address) => listen = Some(address),
                    Err(message) => {
                        eprintln!("error: {message}");
                        return ParsedArgs::Exit(2);
                    }
                }
            }
            "--trusted-proxy" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    eprintln!("error: --trusted-proxy requires an IP, CIDR, or 'unix'");
                    return ParsedArgs::Exit(2);
                };
                trusted_proxy_values.push(value.clone());
            }
            "--workers" => {
                i += 1;
                workers = args
                    .get(i)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(workers);
            }
            "--handler-concurrency" => {
                i += 1;
                handler_concurrency = args
                    .get(i)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(handler_concurrency);
            }
            "--max-handler-requests" => {
                i += 1;
                max_handler_requests = args
                    .get(i)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(max_handler_requests);
            }
            "--max-body-size" => {
                i += 1;
                max_body = args
                    .get(i)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(max_body);
            }
            "--body-read-timeout" => {
                i += 1;
                body_read_secs = args
                    .get(i)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(body_read_secs);
            }
            "--response-write-timeout" => {
                i += 1;
                response_write_secs = args
                    .get(i)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(response_write_secs);
            }
            "--max-requests" => {
                i += 1;
                max_requests = args
                    .get(i)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(max_requests);
            }
            "--max-execution-time" => {
                i += 1;
                max_exec_secs = args
                    .get(i)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(max_exec_secs);
            }
            "--access-log" => access_log = true,
            "--gzip" => gzip = true,
            _ => {}
        }
        i += 1;
    }

    let trusted_proxies = match TrustedProxies::parse_values(&trusted_proxy_values) {
        Ok(trusted) => trusted,
        Err(message) => {
            eprintln!("error: {message}");
            return ParsedArgs::Exit(2);
        }
    };

    match listen {
        Some(listen) => ParsedArgs::Run(ServerArgs {
            listen,
            trusted_proxies,
            workers: workers.max(1),
            handler_concurrency: handler_concurrency.max(1),
            max_handler_requests,
            max_body,
            body_read_secs,
            response_write_secs,
            max_requests,
            access_log,
            max_exec_secs,
            gzip,
        }),
        None => {
            eprintln!(
                "error: --web binary requires --listen HOST:PORT or unix:/PATH (try --help)"
            );
            ParsedArgs::Exit(2)
        }
    }
}

/// Returns the default worker count (number of logical CPUs, minimum one).
fn default_workers() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    /// Parses test argv through the same C-compatible entry path.
    fn parse_test_args_for(args: &[&str], isolation: IsolationMode) -> ParsedArgs {
        let owned = args
            .iter()
            .map(|arg| CString::new(*arg).expect("test argument must not contain NUL"))
            .collect::<Vec<_>>();
        let pointers = owned.iter().map(|arg| arg.as_ptr()).collect::<Vec<_>>();
        parse_args(pointers.len() as i32, pointers.as_ptr(), isolation)
    }

    /// Verifies the shipped configuration bounds stalled isolated response writers.
    #[test]
    fn response_write_timeout_is_bounded_by_default() {
        let ParsedArgs::Run(args) = parse_test_args_for(
            &["app", "--listen", "127.0.0.1:0"],
            IsolationMode::Request,
        ) else {
            panic!("a valid listen address must produce runnable server arguments");
        };

        assert_eq!(
            args.response_write_secs, 30,
            "the default must reclaim handler slots held by stalled clients"
        );
    }

    /// Verifies mode-specific runtime flags fail instead of silently doing nothing.
    #[test]
    fn incompatible_isolation_runtime_flags_are_rejected() {
        assert!(matches!(
            parse_test_args_for(
                &[
                    "app",
                    "--listen",
                    "127.0.0.1:0",
                    "--handler-concurrency",
                    "2",
                ],
                IsolationMode::Worker,
            ),
            ParsedArgs::Exit(2)
        ));
        assert!(matches!(
            parse_test_args_for(
                &[
                    "app",
                    "--listen",
                    "127.0.0.1:0",
                    "--max-handler-requests",
                    "2",
                ],
                IsolationMode::Request,
            ),
            ParsedArgs::Exit(2)
        ));
        assert!(matches!(
            parse_test_args_for(
                &[
                    "app",
                    "--listen",
                    "127.0.0.1:0",
                    "--max-handler-requests",
                    "2",
                ],
                IsolationMode::Pool,
            ),
            ParsedArgs::Run(_)
        ));
    }

    /// Issue #887: `--body-read-timeout` is accepted in WORKER isolation, and the parsed
    /// value reaches the in-process worker's config.
    ///
    /// It used to be on the isolated-only reject list, so the default isolation had no body
    /// deadline at all AND refused the flag that would have set one.
    #[test]
    fn worker_isolation_accepts_and_threads_the_body_read_timeout() {
        let parsed = parse_test_args_for(
            &["app", "--listen", "127.0.0.1:0", "--body-read-timeout", "7"],
            IsolationMode::Worker,
        );
        let ParsedArgs::Run(args) = parsed else {
            panic!("--body-read-timeout must be accepted in worker isolation");
        };
        assert_eq!(args.body_read_secs, 7);
        assert_eq!(args.worker_config().body_read_secs, 7);
    }

    /// The default is a SAFE NONZERO value, so a binary that sets no flag is still bounded,
    /// and `0` stays available as the explicit unlimited setting.
    #[test]
    fn worker_isolation_body_read_timeout_defaults_nonzero_and_allows_explicit_zero() {
        let parsed = parse_test_args_for(
            &["app", "--listen", "127.0.0.1:0"],
            IsolationMode::Worker,
        );
        let ParsedArgs::Run(args) = parsed else {
            panic!("default worker args must parse");
        };
        assert_eq!(args.worker_config().body_read_secs, DEFAULT_BODY_READ_SECS);
        assert!(DEFAULT_BODY_READ_SECS > 0, "the default must bound the body read");

        let parsed = parse_test_args_for(
            &["app", "--listen", "127.0.0.1:0", "--body-read-timeout", "0"],
            IsolationMode::Worker,
        );
        let ParsedArgs::Run(args) = parsed else {
            panic!("an explicit 0 must parse");
        };
        assert_eq!(args.worker_config().body_read_secs, 0);
    }

    /// Verifies the runtime accepts the Unix-socket form and threads it to the server.
    #[test]
    fn listen_accepts_unix_socket_path() {
        let parsed = parse_test_args_for(
            &["app", "--listen", "unix:/tmp/elephc-test.sock"],
            IsolationMode::Worker,
        );
        let ParsedArgs::Run(args) = parsed else {
            panic!("a valid Unix socket path must produce runnable server arguments");
        };
        assert_eq!(
            args.listen,
            ListenAddr::Unix(std::path::PathBuf::from("/tmp/elephc-test.sock"))
        );
    }

    /// Verifies malformed Unix paths are usage errors, not panics or worker crashes.
    #[test]
    fn listen_rejects_empty_and_oversized_unix_paths() {
        assert!(matches!(
            parse_test_args_for(&["app", "--listen", "unix:"], IsolationMode::Worker),
            ParsedArgs::Exit(2)
        ));
        let long = format!("unix:/{}", "x".repeat(101));
        assert!(matches!(
            parse_test_args_for(&["app", "--listen", &long], IsolationMode::Worker),
            ParsedArgs::Exit(2)
        ));
    }

    /// Verifies a missing `--listen` value is a usage error.
    #[test]
    fn listen_requires_a_value() {
        assert!(matches!(
            parse_test_args_for(&["app", "--listen"], IsolationMode::Worker),
            ParsedArgs::Exit(2)
        ));
    }

    /// Verifies the trusted-proxy allowlist accepts IPv4/IPv6/CIDR/`unix` and
    /// rejects malformed or missing values as usage errors.
    #[test]
    fn trusted_proxy_flag_parses_and_rejects_malformed_values() {
        assert!(matches!(
            parse_test_args_for(
                &[
                    "app",
                    "--listen",
                    "127.0.0.1:0",
                    "--trusted-proxy",
                    "127.0.0.1/32,10.0.0.0/8,2001:db8::/32,unix",
                ],
                IsolationMode::Worker,
            ),
            ParsedArgs::Run(_)
        ));
        assert!(matches!(
            parse_test_args_for(
                &["app", "--listen", "127.0.0.1:0", "--trusted-proxy", "not-an-ip"],
                IsolationMode::Worker,
            ),
            ParsedArgs::Exit(2)
        ));
        assert!(matches!(
            parse_test_args_for(
                &["app", "--listen", "127.0.0.1:0", "--trusted-proxy"],
                IsolationMode::Worker,
            ),
            ParsedArgs::Exit(2)
        ));
    }
}
