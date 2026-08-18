//! Purpose:
//! End-to-end codegen coverage for target-aware PCNTL constants and process-control builtins.
//!
//! Called from:
//! - `cargo test --test codegen_tests pcntl` through Rust's test harness.
//!
//! Key details:
//! - Constant expectations are selected for the host target because signal and errno values follow libc.

use crate::support::*;

/// Verifies common PCNTL constants resolve through namespaced PHP code and emit target values.
#[test]
fn test_pcntl_common_constants_are_target_aware() {
    let out = compile_and_run(
        "<?php namespace Demo; echo \\SIGCHLD . '|' . \\PCNTL_EAGAIN . '|' . \\WNOHANG;",
    );

    #[cfg(target_os = "macos")]
    assert_eq!(out, "20|35|1");
    #[cfg(target_os = "linux")]
    assert_eq!(out, "17|11|1");
}

/// Verifies Linux-only PCNTL namespace and siginfo constants compile to their libc values.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_linux_only_constants() {
    let out = compile_and_run(
        "<?php echo CLONE_NEWNS . '|' . SI_QUEUE . '|' . P_PIDFD . '|' . WNOWAIT;",
    );
    assert_eq!(out, "131072|-1|3|16777216");
}

/// Verifies macOS-only Darwin priority constants compile to their libc values.
#[cfg(target_os = "macos")]
#[test]
fn test_pcntl_macos_only_constants() {
    let out = compile_and_run("<?php echo PRIO_DARWIN_BG . '|' . PRIO_DARWIN_THREAD;");
    assert_eq!(out, "4096|3");
}

/// Verifies scalar PCNTL calls lower through the bridge and auto-load its extension identity.
#[test]
fn test_pcntl_scalar_bridge_and_extension_loading() {
    let out = compile_and_run(
        "<?php
        $message = pcntl_strerror(PCNTL_EINVAL);
        echo (strlen($message) > 0 ? 'message' : 'empty') . '|';
        echo pcntl_alarm(0) . '|';
        echo pcntl_errno() . ':' . pcntl_get_last_error() . '|';
        echo (extension_loaded('pcntl') ? 'loaded' : 'missing');",
    );
    assert_eq!(out, "message|0|0:0|loaded");
}

/// Verifies target-native wait status helpers preserve boolean and mixed result encodings.
#[test]
fn test_pcntl_wait_status_decoders() {
    let out = compile_and_run(
        "<?php
        $exit = 23 << 8;
        echo pcntl_wifexited($exit) . '|';
        echo pcntl_wexitstatus($exit) . '|';
        echo pcntl_wifsignaled(15) . '|';
        echo pcntl_wtermsig(15) . '|';
        echo pcntl_wifstopped(127) . '|';
        echo pcntl_wifcontinued(65535);",
    );
    #[cfg(target_os = "macos")]
    assert_eq!(out, "1|23|1|15|1|");
    #[cfg(target_os = "linux")]
    assert_eq!(out, "1|23|1|15|1|1");
}

/// Verifies priority lookup returns an integer without confusing a valid `-1` with failure.
#[test]
fn test_pcntl_getpriority_returns_int() {
    let out = compile_and_run(
        "<?php $priority = pcntl_getpriority(); echo is_int($priority) ? 'int' : 'failure';",
    );
    assert_eq!(out, "int");
}
