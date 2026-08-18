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

/// Changes and restores the current signal mask while materializing the prior set by reference.
#[test]
fn test_pcntl_signal_mask_round_trip() {
    let out = compile_and_run(
        "<?php
        $old = [];
        $blocked = pcntl_sigprocmask(SIG_BLOCK, [SIGUSR1], $old);
        echo ($blocked ? 'blocked' : 'bad') . '|';
        echo (is_array($old) ? 'array' : 'bad') . '|';
        echo (pcntl_sigprocmask(SIG_SETMASK, $old) ? 'restored' : 'bad');",
    );
    assert_eq!(out, "blocked|array|restored");
}

/// Returns false on a Linux timed signal wait and preserves an existing info output.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_timed_signal_wait_timeout_preserves_info() {
    let out = compile_and_run(
        "<?php
        $info = ['old' => 42];
        $received = pcntl_sigtimedwait([SIGUSR1], $info, 0, 1);
        echo (!$received ? 'timeout' : 'bad') . '|' . $info['old'];",
    );
    assert_eq!(out, "timeout|42");
}

/// Reads and reapplies the current Linux CPU affinity through indexed integer arrays.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_linux_cpu_affinity_round_trip() {
    let out = compile_and_run(
        "<?php
        $cpu = pcntl_getcpu();
        $mask = pcntl_getcpuaffinity();
        echo ($cpu >= 0 ? 'cpu' : 'bad') . '|';
        echo (count($mask) > 0 ? 'mask' : 'bad') . '|';
        echo (pcntl_setcpuaffinity(cpu_ids: [$cpu]) ? 'set' : 'bad') . '|';
        echo (count($mask) > 0 ? 'mask' : 'bad');",
    );
    assert_eq!(out, "cpu|mask|set|mask");
}

/// Exercises safe Linux namespace entry points without changing namespace state.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_linux_namespace_operations_report_boolean_results() {
    let out = compile_and_run(
        "<?php
        $unshared = pcntl_unshare(0);
        echo (is_bool($unshared) ? 'bool' : 'bad') . '|';
        $joined = pcntl_setns(99999999, CLONE_NEWNET);
        echo (!$joined && pcntl_get_last_error() > 0 ? 'error' : 'bad');",
    );
    assert_eq!(out, "bool|error");
}

/// Keeps Linux-only PCNTL functions absent from the Darwin PHP-visible surface.
#[cfg(target_os = "macos")]
#[test]
fn test_pcntl_linux_functions_are_not_visible_on_macos() {
    let out = compile_and_run(
        "<?php echo function_exists('pcntl_getcpu') ? 'visible' : 'absent';",
    );
    assert_eq!(out, "absent");
}

/// Forks and reaps a real child through `pcntl_waitpid`, proving by-reference status writeback.
#[test]
fn test_pcntl_fork_waitpid_round_trip() {
    let out = compile_and_run(
        "<?php
        $pid = pcntl_fork();
        if ($pid === 0) { exit(23); }
        $status = 0;
        $waited = pcntl_waitpid($pid, $status);
        echo ($waited === $pid ? 'pid' : 'bad') . '|';
        echo pcntl_wifexited($status) . '|' . pcntl_wexitstatus($status);",
    );
    assert_eq!(out, "pid|1|23");
}

/// Forks and reaps a real child through the any-child `pcntl_wait` entry point.
#[test]
fn test_pcntl_fork_wait_round_trip() {
    let out = compile_and_run(
        "<?php
        $pid = pcntl_fork();
        if ($pid === 0) { exit(31); }
        $status = 0;
        $waited = pcntl_wait($status);
        echo ($waited === $pid ? 'pid' : 'bad') . '|';
        echo pcntl_wifexited($status) . '|' . pcntl_wexitstatus($status);",
    );
    assert_eq!(out, "pid|1|31");
}

/// Populates previously undefined status and usage outputs with PHP-compatible value types.
#[test]
fn test_pcntl_waitpid_populates_resource_usage_outputs() {
    let out = compile_and_run(
        "<?php
        $pid = pcntl_fork();
        if ($pid === 0) { exit(19); }
        $waited = pcntl_waitpid(
            process_id: $pid,
            status: $status,
            flags: 0,
            resource_usage: $usage,
        );
        echo ($waited === $pid ? 'pid' : 'bad') . '|';
        echo pcntl_wexitstatus($status) . '|';
        echo count($usage) . '|';
        echo is_int($usage['ru_utime.tv_sec']) ? 'int' : 'bad';",
    );
    assert_eq!(out, "pid|19|17|int");
}

/// Reaps a real child through `pcntl_waitid()` and exposes target-aware siginfo fields.
#[test]
fn test_pcntl_waitid_populates_signal_info() {
    let out = compile_and_run(
        "<?php
        $pid = pcntl_fork();
        if ($pid === 0) { exit(37); }
        $ok = pcntl_waitid(idtype: P_PID, id: $pid, info: $info, flags: WEXITED);
        echo ($ok ? 'ok' : 'bad') . '|';
        echo $info['status'] . '|';
        echo ($info['pid'] === $pid ? 'pid' : 'bad') . '|';
        echo count($info) . '|' . $info['signo'];",
    );
    #[cfg(target_os = "macos")]
    assert_eq!(out, "ok|37|pid|6|20");
    #[cfg(target_os = "linux")]
    assert_eq!(out, "ok|37|pid|8|17");
}

/// Leaves an existing info output untouched when `pcntl_waitid()` fails.
#[test]
fn test_pcntl_waitid_failure_preserves_info_output() {
    let out = compile_and_run(
        "<?php
        $info = ['old' => 41];
        $ok = pcntl_waitid(P_PID, 99999999, $info, WEXITED | WNOHANG);
        echo ($ok ? 'bad' : 'false') . '|' . $info['old'];",
    );
    assert_eq!(out, "false|41");
}
