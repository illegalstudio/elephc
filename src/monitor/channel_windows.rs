//! Purpose:
//! Provides the Windows boundary for `elephc monitor`'s Unix inherited-socket
//! control channel without pretending that `fork`, fd passing, or `ptrace` exist.
//!
//! Called from:
//! - `crate::monitor::run()` after it has accepted remote TCP/TLS endpoints.
//!
//! Key details:
//! - Local monitor modes are rejected before these stubs can launch a target.
//! - Remote probe authentication remains portable through `remote::run_probe_host`.

use super::*;

/// Answers one HTTP request with the current bytes of a live HTML export.
/// This transport is ordinary TCP and therefore remains usable on Windows.
pub(crate) fn serve_one_request(
    mut stream: std::net::TcpStream,
    path: &str,
) -> std::io::Result<()> {
    use std::io::{BufRead, BufReader, Write};

    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    {
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        loop {
            let mut header = String::new();
            let read = reader.read_line(&mut header)?;
            if read == 0 || header == "\r\n" || header == "\n" {
                break;
            }
        }
    }
    let body = std::fs::read(path).unwrap_or_default();
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&body)?;
    stream.flush()
}

/// Keeps program stderr filtering consistent with Unix monitor output.
pub(crate) fn is_profiler_line(line: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "elephc-instr:",
        "elephc-probe:",
        "elephc-probe-alloc:",
        "elephc-probe-io:",
        "elephc-probe-samples:",
    ];
    line.split_whitespace()
        .next()
        .is_some_and(|first| PREFIXES.contains(&first))
}

/// Compiles a PHP source with monitoring metadata when called by shared command
/// plumbing. Windows local execution is rejected before this path is selected.
pub(crate) fn compile_php_monitored(source: &str) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| format!("cannot locate elephc: {error}"))?;
    let status = process::Command::new(exe)
        .args(["--with-monitoring", source])
        .status()
        .map_err(|error| format!("cannot run elephc: {error}"))?;
    if !status.success() {
        return Err(format!("compiling {source} with --with-monitoring failed"));
    }
    Ok(spawnable_path(source.trim_end_matches(".php")))
}

/// Windows does not have the inherited Unix socket contract used by a launched
/// monitor target, so no channel is ever opened.
pub(crate) fn open_control_channel() -> Option<ControlChannel> {
    None
}

/// `--live` has the same unsupported local-channel boundary as exact capture.
pub(crate) fn open_polled_control_channel() -> Option<ControlChannel> {
    None
}

/// A Windows process cannot acknowledge a channel that was never created.
pub(crate) fn control_channel_activated(_channel: &ControlChannel) -> bool {
    false
}

/// The shared loop only reaches this through local modes, which Windows rejects.
pub(crate) fn await_activation(_channel: &ControlChannel, _timeout: std::time::Duration) -> bool {
    false
}

/// Result shape retained for shared monitor rendering code; no Windows local
/// transport can produce an answer.
#[allow(dead_code)]
pub(crate) enum Snapshot {
    /// No inherited control channel exists on Windows.
    Gone,
    /// Retained for the common loop's exhaustive match.
    Late { activation_seen: bool },
    /// Retained for the common loop's exhaustive match.
    Answered(String),
}

/// A request cannot be sent because local channel monitoring is unsupported.
pub(crate) fn request_snapshot(_channel: &ControlChannel) -> Snapshot {
    Snapshot::Gone
}

/// Does not configure a fake child channel. `monitor::run` emits the platform
/// diagnostic before it could spawn this command.
pub(crate) fn attach_control_channel(
    _command: &mut process::Command,
    _channel: &ControlChannel,
) {
}

/// Checks the portable marker in a regular file without attempting to inspect a
/// running process.
pub(crate) fn carries_monitoring(path: &std::path::Path) -> bool {
    if !std::fs::metadata(path).map(|metadata| metadata.is_file()).unwrap_or(false) {
        return false;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes
        .windows(MONITORING_MARKER.len())
        .any(|window| window == MONITORING_MARKER)
}

/// Keeps marker diagnostics coherent for code that validates a target before
/// reaching the Windows-only local-monitoring refusal.
pub(crate) fn require_monitoring(path: &std::path::Path) -> Result<(), String> {
    match std::fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err(format!("{} is not a file, so there is nothing to run.", path.display()));
        }
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
        Ok(_) => {}
    }
    if carries_monitoring(path) {
        Ok(())
    } else {
        Err(format!(
            "{} was not built with --with-monitoring, so there is nothing to monitor.",
            path.display()
        ))
    }
}

/// Resolves the remote endpoint's HMAC key exactly as the Unix channel module
/// does; TCP/TLS probe reads are supported on Windows.
pub(crate) fn resolve_probe_key(cmd: &MonitorCommand, socket: &str) -> Result<[u8; 32], String> {
    if let Some(path) = &cmd.probe_key {
        let hex = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read probe key {path}: {error}"))?;
        return parse_hex_key(hex.trim())
            .ok_or_else(|| format!("probe key {path} is not 64 hex characters"));
    }
    if let Ok(hex) = std::env::var("ELEPHC_PROBE_KEY") {
        return parse_hex_key(hex.trim())
            .ok_or_else(|| "ELEPHC_PROBE_KEY is not 64 hex characters".to_string());
    }
    let candidates = [
        format!("{}.key", socket.trim_end_matches(".sock")),
        format!("{socket}.key"),
    ];
    for candidate in &candidates {
        if let Ok(hex) = std::fs::read_to_string(candidate) {
            return parse_hex_key(hex.trim()).ok_or_else(|| {
                format!("probe key sidecar {candidate} is not 64 hex characters")
            });
        }
    }
    Err(format!(
        "no build key: pass --key <file>, set ELEPHC_PROBE_KEY, or place a .key file next to {socket}"
    ))
}
