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

/// Registers a callable, retrieves it, and dispatches SIGALRM with stable siginfo.
#[test]
fn test_pcntl_signal_handler_dispatch_and_lookup() {
    let out = compile_and_run(
        "<?php
        function handle_alarm(int $signal, array $info): void {
            echo 'handled:' . $signal . ':' . $info['signo'] . '|';
        }
        echo (pcntl_signal(SIGALRM, 'handle_alarm') ? 'set' : 'bad') . '|';
        $handler = pcntl_signal_get_handler(SIGALRM);
        echo (is_callable($handler) ? 'callable' : 'bad') . '|';
        pcntl_alarm(1);
        sleep(2);
        echo (pcntl_signal_dispatch() ? 'dispatched' : 'bad') . '|';
        echo (pcntl_signal(SIGALRM, SIG_DFL) ? 'reset' : 'bad');",
    );
    assert_eq!(out, "set|callable|handled:14:14|dispatched|reset");
}

/// Automatically dispatches a pending signal after a normal EIR safe point.
#[test]
fn test_pcntl_async_signals_dispatch_at_safe_points() {
    let out = compile_and_run(
        "<?php
        function handle_async_alarm(int $signal, array $info): void {
            echo 'async:' . $signal . '|';
        }
        pcntl_signal(SIGALRM, 'handle_async_alarm');
        echo (pcntl_async_signals(true) ? 'old-on' : 'old-off') . '|';
        pcntl_alarm(1);
        sleep(2);
        echo (pcntl_async_signals(false) ? 'was-on' : 'bad') . '|';
        pcntl_signal(SIGALRM, SIG_DFL);",
    );
    assert_eq!(out, "old-off|async:14|was-on|");
}

/// Treats an explicit nullable async-signals argument as a query without changing state.
#[test]
fn test_pcntl_async_signals_explicit_null_queries_state() {
    let out = compile_and_run(
        "<?php
        pcntl_async_signals(true);
        echo (pcntl_async_signals(null) ? 'old-on' : 'bad') . '|';
        echo (pcntl_async_signals() ? 'still-on' : 'bad') . '|';
        echo (pcntl_async_signals(false) ? 'reset' : 'bad');",
    );
    assert_eq!(out, "old-on|still-on|reset");
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

/// Exercises Darwin QoS enum injection, getter identity, setter selection, and defaulting.
#[cfg(target_os = "macos")]
#[test]
fn test_pcntl_macos_qos_class_round_trip() {
    let out = compile_and_run(
        "<?php
        echo (enum_exists('Pcntl\\\\QosClass') ? 'enum' : 'missing') . '|';
        echo count(Pcntl\\QosClass::cases()) . '|';
        $before = pcntl_getqos_class();
        pcntl_setqos_class($before);
        echo (pcntl_getqos_class() === $before ? 'same' : 'changed') . '|';
        pcntl_setqos_class();
        echo (pcntl_getqos_class() === Pcntl\\QosClass::Default ? 'default' : 'bad');",
    );
    assert_eq!(out, "enum|5|same|default");
}

/// Preserves php-src's catchable Error when Darwin rejects a QoS change after fork.
#[cfg(target_os = "macos")]
#[test]
fn test_pcntl_macos_qos_set_failure_throws_error() {
    let out = compile_and_run(
        "<?php
        $current = pcntl_getqos_class();
        $pid = pcntl_fork();
        if ($pid === 0) {
            try {
                pcntl_setqos_class($current);
                echo 'missing';
            } catch (Error $error) {
                echo $error->getMessage();
            }
            exit(0);
        }
        pcntl_waitpid($pid, $status);
        echo '|' . pcntl_wexitstatus($status);",
    );
    assert_eq!(out, "pcntl_setqos_class failed|0");
}

/// Keeps Darwin-only QoS functions and their enum absent from Linux PHP-visible metadata.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_macos_qos_surface_is_not_visible_on_linux() {
    let out = compile_and_run(
        "<?php
        echo function_exists('pcntl_getqos_class') ? 'visible' : 'absent';
        echo '|';
        echo function_exists('pcntl_setqos_class') ? 'visible' : 'absent';
        echo '|';
        echo enum_exists('Pcntl\\\\QosClass') ? 'visible' : 'absent';",
    );
    assert_eq!(out, "absent|absent|absent");
}

/// Replaces a forked child with `/bin/sh`, preserving argv order and the explicit environment.
#[test]
fn test_pcntl_exec_replaces_child_with_arguments_and_environment() {
    let out = compile_and_run(
        r#"<?php
        $pid = pcntl_fork();
        if ($pid === 0) {
            pcntl_exec(
                '/bin/sh',
                ['-c', 'printf "%s" "$PCNTL_EXEC_ENV"'],
                ['PCNTL_EXEC_ENV' => 'ready'],
            );
            exit(99);
        }
        pcntl_waitpid($pid, $status);
        echo '|' . pcntl_wexitstatus($status);"#,
    );
    assert_eq!(out, "ready|0");
}

/// Returns false and preserves the bridge errno when process replacement fails.
#[test]
fn test_pcntl_exec_failure_returns_false_and_records_errno() {
    let out = compile_and_run(
        "<?php
        $ok = pcntl_exec('/definitely/missing/elephc-pcntl');
        echo (!$ok ? 'false' : 'bad') . '|' . pcntl_get_last_error();",
    );
    assert_eq!(out, "false|2");
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

/// Executes scalar PCNTL adapters through runtime eval, including failure errno propagation.
#[test]
fn test_pcntl_eval_scalar_status_and_exec_failure() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            $status = 29 << 8;
            $failed = pcntl_exec("/definitely/missing/elephc-pcntl-eval");
            pcntl_async_signals(true);
            $nullable = pcntl_async_signals(null) && pcntl_async_signals();
            pcntl_async_signals(false);
            return (pcntl_wifexited($status) ? "exited" : "bad") . "|"
                . pcntl_wexitstatus($status) . "|"
                . (!$failed ? "false" : "bad") . "|"
                . pcntl_get_last_error() . "|"
                . ($nullable ? "nullable" : "bad");
        ');"#,
    );
    assert_eq!(out, "exited|29|false|2|nullable");
}

/// Forks and reaps a child inside runtime eval, proving by-reference status and usage writeback.
#[test]
fn test_pcntl_eval_fork_waitpid_writes_reference_outputs() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            $pid = pcntl_fork();
            if ($pid === 0) { exit(43); }
            $waited = pcntl_waitpid($pid, $status, 0, $usage);
            return ($waited === $pid ? "pid" : "bad") . "|"
                . pcntl_wexitstatus($status) . "|"
                . (is_int($usage["ru_utime.tv_sec"]) ? "usage" : "bad");
        ');"#,
    );
    assert_eq!(out, "pid|43|usage");
}

/// Registers and dispatches an eval closure through the signal-safe Magician queue.
#[test]
fn test_pcntl_eval_signal_handler_dispatch() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            $seen = 0;
            $handler = function($signal, $info) use (&$seen) {
                $seen = $signal === $info["signo"] ? $signal : -1;
            };
            pcntl_signal(SIGALRM, $handler);
            pcntl_alarm(1);
            sleep(2);
            $ok = pcntl_signal_dispatch();
            pcntl_signal(SIGALRM, SIG_DFL);
            return ($ok ? "dispatch" : "bad") . "|" . $seen;
        ');"#,
    );
    assert_eq!(out, "dispatch|14");
}

/// Exercises Linux-only eval adapters and confirms Darwin QoS metadata remains absent.
#[cfg(target_os = "linux")]
#[test]
fn test_pcntl_eval_linux_target_surface_and_affinity() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            $cpu = pcntl_getcpu();
            $mask = pcntl_getcpuaffinity();
            return (function_exists("pcntl_getcpu") ? "visible" : "missing") . "|"
                . (function_exists("pcntl_getqos_class") ? "qos" : "no-qos") . "|"
                . ($cpu >= 0 && count($mask) > 0 ? "affinity" : "bad");
        ');"#,
    );
    assert_eq!(out, "visible|no-qos|affinity");
}

/// Exercises Darwin QoS enum defaults through eval and confirms Linux-only metadata stays absent.
#[cfg(target_os = "macos")]
#[test]
fn test_pcntl_eval_macos_qos_default_and_target_surface() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            $before = pcntl_getqos_class();
            pcntl_setqos_class($before);
            pcntl_setqos_class();
            return (function_exists("pcntl_getqos_class") ? "visible" : "missing") . "|"
                . (function_exists("pcntl_getcpu") ? "linux" : "no-linux") . "|"
                . (pcntl_getqos_class() === Pcntl\\QosClass::Default ? "default" : "bad");
        ');"#,
    );
    assert_eq!(out, "visible|no-linux|default");
}
