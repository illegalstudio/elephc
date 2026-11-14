//! Purpose:
//! Implements the minimal `elephc debug` launcher: build a debug source, then
//! run it with the interactive stdin debug session active.
//!
//! Called from:
//! - `crate::main()` through `cli::Command::Debug`.
//!
//! Key details:
//! - A source target is compiled by re-invoking this compiler with `--debug`,
//!   so the same CLI/config path owns the build.
//! - A binary target is launched as-is; it must have been built with `--debug`
//!   for the embedded metadata and site hook to exist.
//! - The interactive protocol is the runtime crate's stdin session.
//! - `--dap` creates a Unix socket pair, launches the debug binary with the
//!   socket on fd 3, and forwards DAP frames between the IDE's stdio and that
//!   socket. The program's own stdout is redirected to stderr so it cannot
//!   corrupt the protocol stream.
//! - `--attach <socket>` connects to a debug binary waiting on
//!   `ELEPHC_DEBUG_DAP_LISTEN` instead of launching one. Launch-mode `pause`
//!   frames are mirrored to SIGUSR1 for the runtime's next-site stop.

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::IntoRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// One parsed `elephc debug` invocation.
pub(crate) struct DebugCommand {
    /// Source file or already-built debug binary; empty for `--attach`.
    pub(crate) target: String,
    /// Optional `FILE:LINE` / `:LINE` breakpoint spec.
    pub(crate) break_at: Option<String>,
    /// Serve Debug Adapter Protocol on stdio instead of the human session.
    pub(crate) dap: bool,
    /// Optional DAP attach socket exposed by an already-running debug binary.
    pub(crate) attach: Option<String>,
    /// Optional loopback TCP attach address exposed by a debug binary.
    pub(crate) attach_tcp: Option<String>,
    /// Mode-0600 key file for `--attach-tcp`.
    pub(crate) key: Option<String>,
    /// Write a postmortem JSON document here on an uncaught exception.
    pub(crate) postmortem: Option<String>,
    /// Inspect an existing postmortem document and exit.
    pub(crate) last: Option<String>,
    /// Serve one route-selected request from a `--web` prefork server.
    pub(crate) web: bool,
    /// `--web` listen address (`host:port`).
    pub(crate) listen: Option<String>,
    /// Path filter for the selected web request (`*`, exact, or `prefix*`).
    pub(crate) route: Option<String>,
    /// Optional HTTP method filter for the selected web request.
    pub(crate) method: Option<String>,
    /// Optional `Name: value` header filter for the selected web request.
    pub(crate) header: Option<String>,
    /// Worker count passed to the web server.
    pub(crate) workers: Option<String>,
    /// Handler-child concurrency for pool/request isolation.
    pub(crate) handler_concurrency: Option<String>,
    /// Broker-owned session directory; defaults to a fresh temp directory.
    pub(crate) web_dir: Option<String>,
    /// Web isolation model used when compiling a source target.
    pub(crate) web_isolation: Option<String>,
}

/// Usage text for `elephc debug`.
pub(crate) const HELP: &str = "Usage: elephc debug <source.php|source.lfc|binary> [--break FILE:LINE] [--dap] [--postmortem FILE]
       elephc debug <source.php|source.lfc|binary> --web --listen HOST:PORT
           [--route PATH|PREFIX*] [--method METHOD] [--header NAME:VALUE]
           [--workers N] [--handler-concurrency N] [--web-dir DIR]
           [--web-isolation worker|pool|request]
       elephc debug --attach <socket>
       elephc debug --attach-tcp HOST:PORT --key FILE
       elephc debug --last FILE

Builds a source with --debug, then starts the interactive runtime session.
Without --break the program stops at its first executed site. Commands in the
session: continue|c, next|n, step|s, finish|f, bt, frame <index>, locals,
p <name>, help, quit|q.

--postmortem FILE writes a bounded JSON postmortem (PHP stack, recent calls and
exception fields) when the program exits with an uncaught exception. A normal
exit writes nothing. --last FILE prints that document; it does not launch a
program. Native crashes are not promised to flush from a signal handler.

`bt` walks the activation stack (current frame and callers). `frame <index>`
selects one of those frames for `locals` and `p`. `next` steps over calls,
`step` enters them, and `finish` returns to the caller.

With --dap the command serves the Debug Adapter Protocol on stdio instead of
starting the line-oriented session. The adapter supports launch, breakpoints,
configurationDone, threads, stackTrace, scopes, variables, evaluate and
continue/next/stepIn/stepOut. Program stdout is redirected to stderr, so the
protocol stream stays clean. With --attach <socket>, connects to a debug binary
started with ELEPHC_DEBUG_DAP=1 ELEPHC_DEBUG_DAP_LISTEN=<socket> and serves DAP
on stdio; the socket is a one-shot mode-0600 attach credential and the program
waits at its entry stop. In launch mode, `pause` is mirrored to SIGUSR1 and stops
at the next executed site. Data breakpoints and pause in attach mode are not
supported yet.

With --web, launches a --debug --web prefork server and attaches DAP to the first
request matching --route (default: any), --method and --header. The selected
worker advertises a mode-0600 Unix socket under a broker-owned 0700 directory,
accepts exactly one client, and stops at its first source site; the master and
every other worker keep serving. Disconnecting resumes that request and leaves
the server running. The v1 route broker supports the default worker isolation
model, not pool/request handler isolation.

--attach-tcp connects to a debug binary started with
ELEPHC_DEBUG_ATTACH_ADDR=127.0.0.1:PORT and ELEPHC_DEBUG_ATTACH_KEY=FILE. The
runtime writes FILE (mode 0600) and FILE.addr (the actual loopback address),
then proves the key with an HMAC challenge-response before speaking DAP. A
wrong key is rejected and the runtime keeps waiting for the correct one. The
existing --attach <socket> Unix path is unchanged.";

/// Parses the arguments after the `debug` selector.
pub(crate) fn parse_args(args: &[String]) -> Result<DebugCommand, String> {
    let mut target = None;
    let mut break_at = None;
    let mut dap = false;
    let mut attach = None;
    let mut attach_tcp = None;
    let mut key = None;
    let mut postmortem = None;
    let mut last = None;
    let mut web = false;
    let mut listen = None;
    let mut route = None;
    let mut method = None;
    let mut header = None;
    let mut workers = None;
    let mut handler_concurrency = None;
    let mut web_dir = None;
    let mut web_isolation = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            "--break" => {
                index += 1;
                break_at = Some(
                    args.get(index)
                        .ok_or("--break needs FILE:LINE")?
                        .clone(),
                );
            }
            value if value.starts_with("--break=") => {
                break_at = Some(value["--break=".len()..].to_string());
            }
            "--dap" => dap = true,
            "--web" => {
                web = true;
                dap = true;
            }
            "--listen" => {
                index += 1;
                listen = Some(args.get(index).ok_or("--listen needs HOST:PORT")?.clone());
            }
            value if value.starts_with("--listen=") => {
                listen = Some(value["--listen=".len()..].to_string());
            }
            "--route" => {
                index += 1;
                route = Some(args.get(index).ok_or("--route needs a path")?.clone());
            }
            value if value.starts_with("--route=") => {
                route = Some(value["--route=".len()..].to_string());
            }
            "--method" => {
                index += 1;
                method = Some(args.get(index).ok_or("--method needs a method")?.clone());
            }
            value if value.starts_with("--method=") => {
                method = Some(value["--method=".len()..].to_string());
            }
            "--header" => {
                index += 1;
                header = Some(args.get(index).ok_or("--header needs NAME:VALUE")?.clone());
            }
            value if value.starts_with("--header=") => {
                header = Some(value["--header=".len()..].to_string());
            }
            "--workers" => {
                index += 1;
                workers = Some(args.get(index).ok_or("--workers needs a count")?.clone());
            }
            value if value.starts_with("--workers=") => {
                workers = Some(value["--workers=".len()..].to_string());
            }
            "--handler-concurrency" => {
                index += 1;
                handler_concurrency = Some(
                    args.get(index)
                        .ok_or("--handler-concurrency needs a count")?
                        .clone(),
                );
            }
            value if value.starts_with("--handler-concurrency=") => {
                handler_concurrency = Some(value["--handler-concurrency=".len()..].to_string());
            }
            "--web-dir" => {
                index += 1;
                web_dir = Some(args.get(index).ok_or("--web-dir needs a directory")?.clone());
            }
            value if value.starts_with("--web-dir=") => {
                web_dir = Some(value["--web-dir=".len()..].to_string());
            }
            "--web-isolation" => {
                index += 1;
                web_isolation = Some(
                    args.get(index)
                        .ok_or("--web-isolation needs worker|pool|request")?
                        .clone(),
                );
            }
            value if value.starts_with("--web-isolation=") => {
                web_isolation = Some(value["--web-isolation=".len()..].to_string());
            }
            "--attach" => {
                index += 1;
                attach = Some(
                    args.get(index)
                        .ok_or("--attach needs a socket path")?
                        .clone(),
                );
            }
            value if value.starts_with("--attach=") => {
                attach = Some(value["--attach=".len()..].to_string());
            }
            "--attach-tcp" => {
                index += 1;
                attach_tcp = Some(
                    args.get(index)
                        .ok_or("--attach-tcp needs HOST:PORT")?
                        .clone(),
                );
            }
            value if value.starts_with("--attach-tcp=") => {
                attach_tcp = Some(value["--attach-tcp=".len()..].to_string());
            }
            "--key" => {
                index += 1;
                key = Some(args.get(index).ok_or("--key needs a file path")?.clone());
            }
            value if value.starts_with("--key=") => {
                key = Some(value["--key=".len()..].to_string());
            }
            "--postmortem" => {
                index += 1;
                postmortem = Some(
                    args.get(index)
                        .ok_or("--postmortem needs a file path")?
                        .clone(),
                );
            }
            value if value.starts_with("--postmortem=") => {
                postmortem = Some(value["--postmortem=".len()..].to_string());
            }
            "--last" => {
                index += 1;
                last = Some(
                    args.get(index)
                        .ok_or("--last needs a postmortem file path")?
                        .clone(),
                );
            }
            value if value.starts_with("--last=") => {
                last = Some(value["--last=".len()..].to_string());
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown debug option '{value}'"));
            }
            value => {
                if target.replace(value.to_string()).is_some() {
                    return Err("elephc debug accepts one target".to_string());
                }
            }
        }
        index += 1;
    }
    if last.is_some() {
        if target.is_some()
            || attach.is_some()
            || dap
            || break_at.is_some()
            || postmortem.is_some()
            || web
        {
            return Err("elephc debug --last inspects one file and takes no other option"
                .to_string());
        }
    } else if web {
        if target.is_none() {
            return Err("elephc debug --web needs a source or debug binary".to_string());
        }
        if attach.is_some() {
            return Err("elephc debug --web launches its own server; --attach is separate"
                .to_string());
        }
        if postmortem.is_some() {
            return Err(
                "--postmortem is not supported with --web: each worker has its own process"
                    .to_string(),
            );
        }
        if break_at.is_some() {
            return Err("elephc debug --web sets breakpoints over DAP; drop --break".to_string());
        }
        if listen.as_deref().is_none_or(str::is_empty) {
            return Err(
                "elephc debug --web needs --listen HOST:PORT for the prefork server".to_string(),
            );
        }
        if workers
            .as_deref()
            .is_some_and(|workers| workers.parse::<u32>().map_or(true, |count| count == 0))
        {
            return Err("--workers needs a positive count".to_string());
        }
        if handler_concurrency
            .as_deref()
            .is_some_and(|count| count.parse::<u32>().map_or(true, |count| count == 0))
        {
            return Err("--handler-concurrency needs a positive count".to_string());
        }
        if web_isolation
            .as_deref()
            .is_some_and(|mode| !matches!(mode, "worker" | "pool" | "request"))
        {
            return Err("--web-isolation must be worker, pool or request".to_string());
        }
    } else if handler_concurrency.is_some() || web_isolation.is_some() {
        return Err(
            "--handler-concurrency/--web-isolation are only valid with --web".to_string(),
        );
    } else if attach.is_some() || attach_tcp.is_some() {
        if attach.is_some() && attach_tcp.is_some() {
            return Err(
                "elephc debug takes either --attach <socket> or --attach-tcp HOST:PORT, not both"
                    .to_string(),
            );
        }
        if target.is_some() {
            return Err("elephc debug --attach does not take a target".to_string());
        }
        if postmortem.is_some() {
            return Err(
                "--postmortem must be configured when the debuggee starts, not by --attach"
                    .to_string(),
            );
        }
        if attach_tcp.is_some() && key.as_deref().is_none_or(str::is_empty) {
            return Err("--attach-tcp needs --key FILE from the debuggee".to_string());
        }
        if key.is_some() && attach_tcp.is_none() {
            return Err("--key is only valid with --attach-tcp".to_string());
        }
        dap = true;
    } else if key.is_some() {
        return Err("--key is only valid with --attach-tcp".to_string());
    }
    Ok(DebugCommand {
        target: target.unwrap_or_default(),
        break_at,
        dap,
        attach,
        attach_tcp,
        key,
        postmortem,
        last,
        web,
        listen,
        route,
        method,
        header,
        workers,
        handler_concurrency,
        web_dir,
        web_isolation,
    })
}

/// Runs one `elephc debug` invocation and returns the program's exit status.
pub(crate) fn run(command: DebugCommand) -> i32 {
    if let Some(file) = command.last.as_deref() {
        return run_last(file);
    }
    if let Some(address) = command.attach_tcp.as_deref() {
        return run_attach_tcp(
            address,
            command.key.as_deref().unwrap_or_default(),
        );
    }
    if let Some(socket) = command.attach.as_deref() {
        return run_attach(socket);
    }
    if command.web {
        return run_web_dap(command);
    }
    if command.dap {
        return run_dap(command);
    }
    let source = Path::new(&command.target);
    let binary = if is_source(source) {
        let compiler = match std::env::current_exe() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("elephc debug: cannot locate the compiler executable: {error}");
                return 1;
            }
        };
        let status = Command::new(compiler)
            .arg("--debug")
            .arg(&command.target)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status();
        match status {
            Ok(status) if status.success() => source.with_extension(""),
            Ok(_) => return 1,
            Err(error) => {
                eprintln!("elephc debug: failed to compile '{}': {error}", command.target);
                return 1;
            }
        }
    } else {
        PathBuf::from(&command.target)
    };

    if !binary.exists() {
        eprintln!(
            "elephc debug: '{}' does not exist; point at a source or a --debug binary",
            binary.display()
        );
        return 1;
    }
    // `Command::new("main")` searches PATH; a relative debug binary must be
    // resolved against the invocation directory instead.
    let binary = if binary.is_absolute() {
        binary
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(binary)
    };

    let mut process = Command::new(&binary);
    process
        .env("ELEPHC_DEBUG_STDIN", "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Some(spec) = command.break_at {
        process.env("ELEPHC_DEBUG_BREAK", spec);
    }
    if let Some(path) = command.postmortem.as_deref() {
        process.env("ELEPHC_DEBUG_POSTMORTEM", path);
    }
    match process.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("elephc debug: failed to run '{}': {error}", binary.display());
            1
        }
    }
}

/// Prints a human-readable summary of one postmortem document.
fn run_last(file: &str) -> i32 {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("elephc debug: cannot read postmortem '{file}': {error}");
            return 1;
        }
    };
    let document: serde_json::Value = match serde_json::from_str(&text) {
        Ok(document) => document,
        Err(error) => {
            eprintln!("elephc debug: invalid postmortem '{file}': {error}");
            return 1;
        }
    };
    if document.get("format").and_then(serde_json::Value::as_str)
        != Some("elephc-postmortem")
    {
        eprintln!("elephc debug: '{file}' is not an elephc postmortem document");
        return 1;
    }
    println!("postmortem: {file}");
    println!(
        "reason: {}",
        document
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
    );
    if let Some(exception) = document.get("exception") {
        println!(
            "exception: {}: {} (code {})",
            exception
                .get("class")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<unknown>"),
            exception
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
            exception
                .get("code")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0)
        );
        let path = exception
            .get("file")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let line = exception
            .get("line")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if !path.is_empty() && line > 0 {
            println!("throw site: {path}:{line}");
        }
    }
    if let Some(stack) = document.get("stack").and_then(serde_json::Value::as_array) {
        println!("stack:");
        for (index, frame) in stack.iter().enumerate() {
            let function = frame
                .get("function")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<unknown>");
            let path = frame
                .get("file")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let line = frame
                .get("line")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            if path.is_empty() || line == 0 {
                println!("  #{index} {function} at <no site>");
            } else {
                println!("  #{index} {function} at {path}:{line}");
            }
        }
    }
    let calls = document
        .get("calls")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let dropped = document
        .get("dropped")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    println!("recent calls: {} captured, {dropped} dropped", calls.len());
    for call in calls.iter().take(32) {
        let function = call
            .get("function")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<unknown>");
        let depth = call
            .get("depth")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);
        let indent = "  ".repeat(depth.saturating_sub(1) as usize);
        println!("  {indent}{function}");
    }
    0
}

/// One worker-advertised web debug session.
struct WebSession {
    /// Worker PID that owns the request.
    pid: u32,
    /// Mode-0600 socket the worker is waiting on.
    socket: PathBuf,
    /// Request method recorded in the announcement.
    method: String,
    /// Request path recorded in the announcement.
    path: String,
}

/// Returns whether a configured route matches one request path.
fn web_route_matches(pattern: &str, path: &str) -> bool {
    pattern == "*"
        || pattern == path
        || pattern
            .strip_suffix('*')
            .is_some_and(|prefix| path.starts_with(prefix))
}

/// Removes session announcements and the active claim left by a previous run.
fn clear_web_sessions(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
        let session = name.starts_with("session-")
            && path.extension().and_then(|extension| extension.to_str()) == Some("json");
        if session || name == "active-session" {
            let _ = fs::remove_file(path);
        }
    }
}

/// Returns whether an announced worker pid is still present.
fn web_session_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return true;
    }
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0
        || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Finds the newest advertised session matching the broker filters.
///
/// Announcements are sorted newest first, so a stale file left in a reused
/// `--web-dir` cannot win over the request that is waiting now. A missing
/// socket or a dead worker removes the announcement and keeps looking.
fn find_web_session(directory: &Path, command: &DebugCommand) -> Option<WebSession> {
    let entries = fs::read_dir(directory).ok()?;
    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                return None;
            }
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            Some((modified, path))
        })
        .collect();
    candidates.sort_by(|left, right| right.0.cmp(&left.0));
    for (_, path) in candidates {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(socket) = value.get("socket").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let pid = value
            .get("pid")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;
        if !Path::new(socket).exists() || !web_session_pid_alive(pid) {
            let _ = fs::remove_file(&path);
            continue;
        }
        let method = value
            .get("method")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let request_path = value
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if let Some(pattern) = command.route.as_deref() {
            if !web_route_matches(pattern, request_path) {
                continue;
            }
        }
        if let Some(expected) = command.method.as_deref() {
            if !method.eq_ignore_ascii_case(expected) {
                continue;
            }
        }
        return Some(WebSession {
            pid,
            socket: PathBuf::from(socket),
            method: method.to_string(),
            path: request_path.to_string(),
        });
    }
    None
}

/// Returns a fresh broker-owned session directory.
fn default_web_debug_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "elephc-web-debug-{}-{nanos}",
        std::process::id()
    ))
}

/// Compiles one source as a `--debug --web` prefork server.
fn compile_debug_web_source(target: &str, isolation: Option<&str>) -> Result<PathBuf, String> {
    let compiler = std::env::current_exe()
        .map_err(|error| format!("elephc debug: cannot locate the compiler: {error}"))?;
    let mut command = Command::new(compiler);
    command.arg("--debug").arg("--web");
    if let Some(isolation) = isolation {
        command.arg(format!("--web-isolation={isolation}"));
    }
    let output = command
        .arg(target)
        .output()
        .map_err(|error| format!("elephc debug: failed to compile '{target}': {error}"))?;
    if !output.status.success() {
        let mut message = String::new();
        message.push_str(&String::from_utf8_lossy(&output.stdout));
        message.push_str(&String::from_utf8_lossy(&output.stderr));
        return Err(message.trim_end().to_string());
    }
    if !output.stdout.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(Path::new(target).with_extension(""))
}

/// Runs `elephc debug --web`: launch a debug prefork server, wait for the first
/// request matching the route filters, then proxy DAP to that worker only.
fn run_web_dap(command: DebugCommand) -> i32 {
    let Some(listen) = command.listen.clone() else {
        eprintln!("elephc debug: --web needs --listen HOST:PORT");
        return 2;
    };
    let source = Path::new(&command.target);
    let binary = if is_source(source) {
        match compile_debug_web_source(&command.target, command.web_isolation.as_deref()) {
            Ok(binary) => binary,
            Err(error) => {
                eprintln!("{error}");
                return 1;
            }
        }
    } else {
        if command.web_isolation.is_some() {
            eprintln!(
                "elephc debug: warning: --web-isolation is baked into a prebuilt binary and is ignored"
            );
        }
        PathBuf::from(&command.target)
    };
    let binary = if binary.is_absolute() {
        binary
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(binary)
    };
    if !binary.exists() {
        eprintln!(
            "elephc debug: '{}' does not exist; point at a source or a --debug --web binary",
            binary.display()
        );
        return 1;
    }

    let directory = command
        .web_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(default_web_debug_dir);
    if let Err(error) = fs::create_dir_all(&directory) {
        eprintln!(
            "elephc debug: cannot create web session directory '{}': {error}",
            directory.display()
        );
        return 1;
    }
    let _ = fs::set_permissions(&directory, fs::Permissions::from_mode(0o700));
    clear_web_sessions(&directory);
    let _ = fs::remove_file(directory.join("server.pid"));

    let mut process = Command::new(&binary);
    process
        .arg("--listen")
        .arg(&listen)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .env("ELEPHC_DEBUG_WEB_DIR", &directory)
        .env(
            "ELEPHC_DEBUG_WEB_ROUTE",
            command.route.as_deref().unwrap_or("*"),
        )
        .env_remove("ELEPHC_DEBUG_DAP")
        .env_remove("ELEPHC_DEBUG_FD")
        .env_remove("ELEPHC_DEBUG_STDIN")
        .env_remove("ELEPHC_DEBUG_BREAK")
        .env_remove("ELEPHC_DEBUG_CALLS")
        .env_remove("ELEPHC_DEBUG_POSTMORTEM");
    if let Some(workers) = command.workers.as_deref() {
        process.arg("--workers").arg(workers);
    }
    if let Some(concurrency) = command.handler_concurrency.as_deref() {
        process.arg("--handler-concurrency").arg(concurrency);
    }
    if let Some(method) = command.method.as_deref() {
        process.env("ELEPHC_DEBUG_WEB_METHOD", method);
    }
    if let Some(header) = command.header.as_deref() {
        process.env("ELEPHC_DEBUG_WEB_HEADER", header);
    }
    unsafe {
        process.pre_exec(|| {
            // The server's own stdout is not the DAP channel; keep it on stderr
            // alongside the access log and bridge diagnostics.
            if libc::dup2(2, 1) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            eprintln!(
                "elephc debug: failed to run '{}': {error}",
                binary.display()
            );
            return 1;
        }
    };
    let pid_path = directory.join("server.pid");
    if fs::write(&pid_path, child.id().to_string()).is_ok() {
        let _ = fs::set_permissions(&pid_path, fs::Permissions::from_mode(0o600));
    }
    eprintln!(
        "elephc debug(web): server pid {} listening on {} (sessions in {})",
        child.id(),
        listen,
        directory.display()
    );

    let deadline = Instant::now() + Duration::from_secs(120);
    let session = loop {
        if let Some(session) = find_web_session(&directory, &command) {
            break session;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                eprintln!("elephc debug(web): server exited before a matching request: {status}");
                return 1;
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!("elephc debug(web): cannot poll the server: {error}");
                let _ = child.kill();
                let _ = child.wait();
                return 1;
            }
        }
        if Instant::now() >= deadline {
            eprintln!(
                "elephc debug(web): no request matched the route filters within 120s; stopping the server"
            );
            let _ = child.kill();
            let _ = child.wait();
            return 1;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stream = match UnixStream::connect(&session.socket) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!(
                "elephc debug(web): cannot attach to worker {} at '{}': {error}",
                session.pid,
                session.socket.display()
            );
            let _ = child.kill();
            let _ = child.wait();
            return 1;
        }
    };
    eprintln!(
        "elephc debug(web): attached to pid {} ({} {})",
        session.pid, session.method, session.path
    );
    let status = proxy_dap(stream, None, None);
    eprintln!(
        "elephc debug(web): detached; server pid {} is still serving on {}",
        child.id(),
        listen
    );
    status
}

/// Runs `elephc debug --dap`: compile when needed, then proxy DAP frames to a
/// debug binary that owns the runtime session on fd 3.
fn run_dap(command: DebugCommand) -> i32 {
    let source = Path::new(&command.target);
    let binary = if is_source(source) {
        match compile_debug_source(&command.target) {
            Ok(binary) => binary,
            Err(error) => {
                eprintln!("{error}");
                return 1;
            }
        }
    } else {
        PathBuf::from(&command.target)
    };
    if !binary.exists() {
        eprintln!(
            "elephc debug: '{}' does not exist; point at a source or a --debug binary",
            binary.display()
        );
        return 1;
    }
    let binary = if binary.is_absolute() {
        binary
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(binary)
    };
    let metadata = binary.with_extension("edbg.json");
    if !metadata.exists() {
        eprintln!(
            "elephc debug: warning: '{}' has no debug metadata sidecar; rebuild with --debug for full inspection",
            binary.display()
        );
    }

    let (adapter, child) = match UnixStream::pair() {
        Ok(pair) => pair,
        Err(error) => {
            eprintln!("elephc debug: cannot create the DAP socket: {error}");
            return 1;
        }
    };
    let child_fd = child.into_raw_fd();
    let mut process = Command::new(&binary);
    process
        .env("ELEPHC_DEBUG_DAP", "1")
        .env("ELEPHC_DEBUG_FD", "3")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Some(spec) = &command.break_at {
        process.env("ELEPHC_DEBUG_BREAK", spec);
    }
    if let Some(path) = command.postmortem.as_deref() {
        process.env("ELEPHC_DEBUG_POSTMORTEM", path);
    }
    unsafe {
        process.pre_exec(move || {
            // Program output goes to the adapter's stderr, never the DAP stream.
            if libc::dup2(2, 1) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::dup2(child_fd, 3) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::fcntl(3, libc::F_SETFD, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            unsafe {
                libc::close(child_fd);
            }
            eprintln!(
                "elephc debug: failed to run '{}': {error}",
                binary.display()
            );
            return 1;
        }
    };
    unsafe {
        libc::close(child_fd);
    }

    let child_pid = child.id();
    proxy_dap(adapter, Some(child), Some(child_pid))
}

/// Reads a 64-character hexadecimal attach key from `path`.
fn read_attach_key(path: &str) -> Result<[u8; 32], String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read attach key '{path}': {error}"))?;
    decode_hex_key(text.trim())
        .ok_or_else(|| format!("attach key '{path}' is not 64 hexadecimal characters"))
}

/// Decodes a 64-character hexadecimal key.
fn decode_hex_key(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut key = [0u8; 32];
    for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        let pair = std::str::from_utf8(pair).ok()?;
        key[index] = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(key)
}

/// Returns cryptographic random bytes from the OS entropy source.
fn random_bytes<const N: usize>() -> std::io::Result<[u8; N]> {
    let mut bytes = [0u8; N];
    let mut source = fs::File::open("/dev/urandom")?;
    source.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// Computes HMAC-SHA256 (RFC 2104) over `message` with `key`.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    let mut outer = Sha256::new();
    for byte in block {
        inner.update([byte ^ 0x36]);
        outer.update([byte ^ 0x5c]);
    }
    inner.update(message);
    outer.update(inner.finalize());
    outer.finalize().into()
}

/// Server proof over domain-separated `nonce_c || nonce_s`.
fn attach_server_tag(key: &[u8], nonce_c: &[u8], nonce_s: &[u8]) -> [u8; 32] {
    let mut message = Vec::with_capacity(1 + nonce_c.len() + nonce_s.len());
    message.push(b'S');
    message.extend_from_slice(nonce_c);
    message.extend_from_slice(nonce_s);
    hmac_sha256(key, &message)
}

/// Client proof over domain-separated `nonce_s || nonce_c`.
fn attach_client_tag(key: &[u8], nonce_s: &[u8], nonce_c: &[u8]) -> [u8; 32] {
    let mut message = Vec::with_capacity(1 + nonce_s.len() + nonce_c.len());
    message.push(b'C');
    message.extend_from_slice(nonce_s);
    message.extend_from_slice(nonce_c);
    hmac_sha256(key, &message)
}

/// Compares two tags without an early exit.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right) {
        difference |= a ^ b;
    }
    difference == 0
}

/// Connects to a loopback TCP attach endpoint and proves the mode-0600 key.
fn run_attach_tcp(address: &str, key_path: &str) -> i32 {
    let key = match read_attach_key(key_path) {
        Ok(key) => key,
        Err(error) => {
            eprintln!("elephc debug: {error}");
            return 1;
        }
    };
    let addr: SocketAddr = match address.parse() {
        Ok(addr) => addr,
        Err(_) => {
            eprintln!("elephc debug: attach address '{address}' must be IP:PORT");
            return 1;
        }
    };
    if !addr.ip().is_loopback() {
        eprintln!("elephc debug: TCP attach is loopback-only; '{address}' is not loopback");
        return 1;
    }
    let pause_pid = std::fs::read_to_string(format!("{key_path}.pid"))
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok())
        .filter(|pid| *pid > 0);
    if pause_pid.is_none() {
        eprintln!(
            "elephc debug: warning: no attach pid file at '{key_path}.pid'; pause is unavailable"
        );
    }
    let mut stream = match TcpStream::connect(addr) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("elephc debug: cannot attach to '{address}': {error}");
            return 1;
        }
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    let handshake = (|| -> std::io::Result<Result<(), String>> {
        let nonce_c = random_bytes::<32>()?;
        stream.write_all(&nonce_c)?;
        let mut response = [0u8; 64];
        stream.read_exact(&mut response)?;
        let (nonce_s, server_tag) = response.split_at(32);
        if !constant_time_eq(server_tag, &attach_server_tag(&key, &nonce_c, nonce_s)) {
            // Abort fast instead of making the listener wait out its read timeout.
            let _ = stream.write_all(&[0]);
            return Ok(Err("attach key rejected: server proof did not match".to_string()));
        }
        stream.write_all(&attach_client_tag(&key, nonce_s, &nonce_c))?;
        let mut status = [0u8; 1];
        stream.read_exact(&mut status)?;
        if status[0] != 1 {
            return Ok(Err("attach key rejected by the debuggee".to_string()));
        }
        Ok(Ok(()))
    })();
    match handshake {
        Ok(Ok(())) => {}
        Ok(Err(message)) => {
            eprintln!("elephc debug: {message}");
            return 1;
        }
        Err(error) => {
            eprintln!("elephc debug: TCP attach handshake failed: {error}");
            return 1;
        }
    }
    let _ = stream.set_read_timeout(None);
    let _ = stream.set_write_timeout(None);
    proxy_dap(stream, None, pause_pid)
}

/// Connects to a debug binary's one-shot DAP attach socket and proxies stdio.
fn run_attach(socket: &str) -> i32 {
    // Read the pid BEFORE connecting: the runtime writes `<socket>.pid` before
    // bind and removes it right after accept, so a post-connect read races the
    // cleanup. The pid file is mode 0600 next to the socket.
    let pid_path = format!("{socket}.pid");
    let pause_pid = std::fs::read_to_string(&pid_path)
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok())
        .filter(|pid| *pid > 0);
    if pause_pid.is_none() {
        eprintln!(
            "elephc debug: warning: no attach pid file at '{pid_path}'; pause is unavailable"
        );
    }
    let stream = match UnixStream::connect(socket) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("elephc debug: cannot attach to '{socket}': {error}");
            return 1;
        }
    };
    proxy_dap(stream, None, pause_pid)
}

/// One framed DAP transport usable by the launch, Unix-attach and TCP-attach paths.
trait DapStream: Read + Write + Send + 'static {
    /// Clones the stream for the read and write halves.
    fn try_clone_stream(&self) -> std::io::Result<Self>
    where
        Self: Sized;
    /// Half-closes the write side when stdin ends.
    fn shutdown_write(&self) -> std::io::Result<()>;
}

impl DapStream for UnixStream {
    /// Clones one Unix DAP stream.
    fn try_clone_stream(&self) -> std::io::Result<Self> {
        self.try_clone()
    }

    /// Half-closes one Unix DAP stream.
    fn shutdown_write(&self) -> std::io::Result<()> {
        self.shutdown(std::net::Shutdown::Write)
    }
}

impl DapStream for TcpStream {
    /// Clones one TCP DAP stream.
    fn try_clone_stream(&self) -> std::io::Result<Self> {
        self.try_clone()
    }

    /// Half-closes one TCP DAP stream.
    fn shutdown_write(&self) -> std::io::Result<()> {
        self.shutdown(std::net::Shutdown::Write)
    }
}

/// Forwards DAP frames between stdio and one runtime stream.
///
/// The child is optional: launch owns it, attach does not. A launch-mode
/// `pause` request is mirrored to SIGUSR1 so the runtime can stop at its next
/// site even while the program is running.
fn proxy_dap<S: DapStream>(
    adapter: S,
    mut child: Option<std::process::Child>,
    pause_pid: Option<u32>,
) -> i32 {
    let mut to_child = match adapter.try_clone_stream() {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("elephc debug: cannot clone the DAP socket: {error}");
            if let Some(child) = child.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
            return 1;
        }
    };
    let mut from_child = adapter;
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut interceptor = pause_pid.map(PauseInterceptor::new);
        let mut buffer = [0u8; 8192];
        loop {
            match stdin.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    if let Some(interceptor) = interceptor.as_mut() {
                        interceptor.observe(&buffer[..read]);
                    }
                    if to_child.write_all(&buffer[..read]).is_err() {
                        break;
                    }
                    let _ = to_child.flush();
                }
                Err(_) => break,
            }
        }
        let _ = to_child.shutdown_write();
    });
    let output = std::thread::spawn(move || {
        let mut stdout = std::io::stdout();
        let mut buffer = [0u8; 8192];
        loop {
            match from_child.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    if stdout.write_all(&buffer[..read]).is_err() {
                        break;
                    }
                    let _ = stdout.flush();
                }
                Err(_) => break,
            }
        }
    });
    match child.as_mut() {
        Some(child) => {
            let status = child.wait();
            let _ = output.join();
            match status {
                Ok(status) => status.code().unwrap_or(1),
                Err(error) => {
                    eprintln!("elephc debug: failed to wait for the debuggee: {error}");
                    1
                }
            }
        }
        None => {
            let _ = output.join();
            0
        }
    }
}

/// Inspects a copy of the client-to-adapter byte stream for `pause` requests.
struct PauseInterceptor {
    pid: u32,
    pending: Vec<u8>,
}

impl PauseInterceptor {
    /// Records the launched debuggee pid for the pause signal.
    fn new(pid: u32) -> Self {
        Self {
            pid,
            pending: Vec::new(),
        }
    }

    /// Buffers bytes, emits SIGUSR1 for each complete pause request, and drops
    /// consumed frames. The bytes themselves are forwarded unchanged elsewhere.
    fn observe(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        loop {
            let Some(header_end) = find_bytes(&self.pending, b"\r\n\r\n") else {
                break;
            };
            let header = String::from_utf8_lossy(&self.pending[..header_end]);
            let length = header.lines().find_map(|line| {
                line.strip_prefix("Content-Length:")
                    .and_then(|value| value.trim().parse::<usize>().ok())
            });
            let Some(length) = length else {
                self.pending.drain(..header_end + 4);
                continue;
            };
            let body_start = header_end + 4;
            if self.pending.len() < body_start + length {
                break;
            }
            let body = &self.pending[body_start..body_start + length];
            if let Ok(message) = serde_json::from_slice::<serde_json::Value>(body) {
                if message.get("command").and_then(|value| value.as_str()) == Some("pause") {
                    unsafe {
                        libc::kill(self.pid as libc::pid_t, libc::SIGUSR1);
                    }
                }
            }
            self.pending.drain(..body_start + length);
        }
    }
}

/// Returns the first index of `needle` inside `haystack`.
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Builds one source with `--debug` and captures compiler output so the DAP
/// stream on stdout stays protocol-only.
fn compile_debug_source(target: &str) -> Result<PathBuf, String> {
    let compiler = std::env::current_exe()
        .map_err(|error| format!("elephc debug: cannot locate the compiler: {error}"))?;
    let output = Command::new(compiler)
        .arg("--debug")
        .arg(target)
        .output()
        .map_err(|error| format!("elephc debug: failed to compile '{target}': {error}"))?;
    if !output.status.success() {
        let mut message = String::new();
        message.push_str(&String::from_utf8_lossy(&output.stdout));
        message.push_str(&String::from_utf8_lossy(&output.stderr));
        return Err(message.trim_end().to_string());
    }
    if !output.stdout.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(Path::new(target).with_extension(""))
}

/// Returns whether the target is a source file handled by the compiler.
fn is_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("php" | "lfc")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the `--web` broker parses its route/method/header/worker options
    /// and refuses a launch without a listen address or with postmortem capture.
    #[test]
    fn parses_web_broker_options() {
        let command = parse_args(&[
            "main.php".to_string(),
            "--web".to_string(),
            "--listen".to_string(),
            "127.0.0.1:9000".to_string(),
            "--route".to_string(),
            "/api*".to_string(),
            "--method".to_string(),
            "POST".to_string(),
            "--header".to_string(),
            "X-Debug: 1".to_string(),
            "--workers".to_string(),
            "2".to_string(),
            "--handler-concurrency".to_string(),
            "2".to_string(),
            "--web-dir".to_string(),
            "/tmp/sessions".to_string(),
            "--web-isolation=request".to_string(),
        ])
        .expect("web broker options parse");
        assert!(command.web);
        assert!(command.dap);
        assert_eq!(command.listen.as_deref(), Some("127.0.0.1:9000"));
        assert_eq!(command.route.as_deref(), Some("/api*"));
        assert_eq!(command.method.as_deref(), Some("POST"));
        assert_eq!(command.header.as_deref(), Some("X-Debug: 1"));
        assert_eq!(command.workers.as_deref(), Some("2"));
        assert_eq!(command.handler_concurrency.as_deref(), Some("2"));
        assert_eq!(command.web_dir.as_deref(), Some("/tmp/sessions"));
        assert_eq!(command.web_isolation.as_deref(), Some("request"));

        assert!(
            parse_args(&["main.php".to_string(), "--web".to_string()]).is_err(),
            "--web needs --listen"
        );
        assert!(
            parse_args(&[
                "main.php".to_string(),
                "--web".to_string(),
                "--listen".to_string(),
                "127.0.0.1:9000".to_string(),
                "--postmortem".to_string(),
                "pm.json".to_string(),
            ])
            .is_err(),
            "--web cannot combine with --postmortem"
        );
        assert!(
            parse_args(&[
                "--web".to_string(),
                "--listen".to_string(),
                "127.0.0.1:9000".to_string(),
            ])
            .is_err(),
            "--web needs a target"
        );
        assert!(
            parse_args(&[
                "main.php".to_string(),
                "--web".to_string(),
                "--listen".to_string(),
                "127.0.0.1:9000".to_string(),
                "--workers".to_string(),
                "0".to_string(),
            ])
            .is_err(),
            "--workers must be positive"
        );
        assert!(
            parse_args(&[
                "main.php".to_string(),
                "--web".to_string(),
                "--listen".to_string(),
                "127.0.0.1:9000".to_string(),
                "--web-isolation".to_string(),
                "banana".to_string(),
            ])
            .is_err(),
            "--web-isolation must be worker, pool or request"
        );
        assert!(
            parse_args(&[
                "main.php".to_string(),
                "--web-isolation=request".to_string(),
            ])
            .is_err(),
            "--web-isolation requires --web"
        );
    }

    /// Verifies session discovery ignores stale announcements by picking the
    /// newest live request and clearing old files between runs.
    #[test]
    fn web_session_discovery_prefers_newest_announcement() {
        let dir = std::env::temp_dir().join(format!(
            "elephc-web-session-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("session dir");
        let old_socket = dir.join("old.sock");
        let new_socket = dir.join("new.sock");
        fs::write(&old_socket, b"").expect("old socket");
        fs::write(&new_socket, b"").expect("new socket");
        let announcement = |socket: &Path| {
            serde_json::json!({
                "pid": std::process::id(),
                "method": "GET",
                "path": "/selected",
                "socket": socket.to_string_lossy(),
            })
            .to_string()
        };
        fs::write(dir.join("session-old.json"), announcement(&old_socket))
            .expect("old announcement");
        std::thread::sleep(Duration::from_millis(20));
        fs::write(dir.join("session-new.json"), announcement(&new_socket))
            .expect("new announcement");
        fs::write(dir.join("server.pid"), b"123").expect("server pid");

        let command = parse_args(&[
            "main.php".to_string(),
            "--web".to_string(),
            "--listen".to_string(),
            "127.0.0.1:9000".to_string(),
            "--route".to_string(),
            "/selected".to_string(),
        ])
        .expect("web command");
        let session = find_web_session(&dir, &command).expect("newest session");
        assert_eq!(session.socket, new_socket);

        clear_web_sessions(&dir);
        assert!(!dir.join("session-old.json").exists());
        assert!(!dir.join("session-new.json").exists());
        assert!(dir.join("server.pid").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Verifies the loopback TCP attach form parses its address and key, implies
    /// DAP, and refuses a missing key or a mixed Unix/TCP attach.
    #[test]
    fn parses_tcp_attach_address_and_key() {
        let command = parse_args(&[
            "--attach-tcp".to_string(),
            "127.0.0.1:9000".to_string(),
            "--key".to_string(),
            "attach.key".to_string(),
        ])
        .expect("tcp attach parses");
        assert_eq!(command.attach_tcp.as_deref(), Some("127.0.0.1:9000"));
        assert_eq!(command.key.as_deref(), Some("attach.key"));
        assert!(command.dap);
        assert!(command.attach.is_none());

        assert!(
            parse_args(&[
                "--attach-tcp".to_string(),
                "127.0.0.1:9000".to_string(),
            ])
            .is_err(),
            "--attach-tcp needs --key"
        );
        assert!(
            parse_args(&["--key".to_string(), "attach.key".to_string()]).is_err(),
            "--key is only valid with --attach-tcp"
        );
        assert!(
            parse_args(&[
                "--attach".to_string(),
                "app.sock".to_string(),
                "--attach-tcp".to_string(),
                "127.0.0.1:9000".to_string(),
                "--key".to_string(),
                "attach.key".to_string(),
            ])
            .is_err(),
            "--attach and --attach-tcp are mutually exclusive"
        );
        assert!(
            parse_args(&[
                "main.php".to_string(),
                "--attach-tcp".to_string(),
                "127.0.0.1:9000".to_string(),
                "--key".to_string(),
                "attach.key".to_string(),
            ])
            .is_err(),
            "--attach-tcp does not take a target"
        );
    }

    /// Verifies the postmortem launch flag and the `--last` inspector parse
    /// without a target, and that incompatible combinations are refused.
    #[test]
    fn parses_postmortem_launch_and_inspector_forms() {
        let command = parse_args(&[
            "main.php".to_string(),
            "--postmortem=pm.json".to_string(),
        ])
        .expect("--postmortem= parses");
        assert_eq!(command.target, "main.php");
        assert_eq!(command.postmortem.as_deref(), Some("pm.json"));

        let command = parse_args(&[
            "--postmortem".to_string(),
            "pm.json".to_string(),
            "main.php".to_string(),
        ])
        .expect("--postmortem parses");
        assert_eq!(command.postmortem.as_deref(), Some("pm.json"));

        let command = parse_args(&["--last".to_string(), "pm.json".to_string()])
            .expect("--last parses");
        assert_eq!(command.last.as_deref(), Some("pm.json"));
        assert!(command.target.is_empty());

        assert!(
            parse_args(&[
                "--attach".to_string(),
                "app.sock".to_string(),
                "--postmortem".to_string(),
                "pm.json".to_string(),
            ])
            .is_err(),
            "--attach cannot configure a postmortem on an already-running process"
        );
        assert!(
            parse_args(&[
                "main.php".to_string(),
                "--last".to_string(),
                "pm.json".to_string(),
            ])
            .is_err(),
            "--last must not launch a target"
        );
        assert!(
            parse_args(&["--postmortem".to_string()]).is_err(),
            "--postmortem needs a path"
        );
        assert!(
            parse_args(&["--last".to_string()]).is_err(),
            "--last needs a path"
        );
    }
}
