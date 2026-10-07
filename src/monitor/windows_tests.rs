//! Purpose:
//! Windows-only unit coverage for monitor's explicit local-mode boundary.
//!
//! Called from:
//! - `crate::monitor` under `cfg(test, target_os = "windows")`.
//!
//! Key details:
//! - Windows must preserve remote TCP/TLS monitoring while refusing Unix-only local paths.

use super::*;

/// A local path must not be mistaken for a Unix endpoint when the target is
/// Windows; `monitor::run` then emits the actionable local-mode diagnostic.
#[test]
fn local_socket_paths_are_not_claimed_as_windows_probe_endpoints() {
    assert!(!is_socket_path(r"C:\work\service.sock"));
}

/// The refusal explains both the unavailable modes and the portable recovery.
#[test]
fn local_monitor_refusal_points_to_a_remote_endpoint() {
    let diagnostic = windows_local_monitor_diagnostic();
    assert!(diagnostic.contains("--live"));
    assert!(diagnostic.contains("--attach"));
    assert!(diagnostic.contains("host:port"));
}
