//! Purpose:
//! End-to-end codegen coverage for the ext/posix builtins served by the PCNTL bridge:
//! `posix_kill`, `posix_isatty`, `posix_geteuid`, `posix_getpgrp`, `posix_get_last_error`,
//! `posix_errno` and `posix_strerror`, natively and inside `eval()`.
//!
//! Called from:
//! - `cargo test --test codegen_tests posix` through Rust's test harness.
//!
//! Key details:
//! - Only errno values shared by Darwin and Linux are asserted (EPERM 1, ESRCH 3, EBADF 9,
//!   ENOTTY 25), and descriptors are regular files so results never depend on the test's tty.

use crate::support::*;

/// Delivers SIGTERM to a forked child and observes it through the wait status.
#[test]
fn test_posix_kill_terminates_forked_child() {
    let out = compile_and_run(
        r#"<?php
        $child = pcntl_fork();
        if ($child === 0) {
            sleep(30);
            exit(0);
        }
        echo \PoSiX_KiLl($child, SIGTERM) ? 'sent' : 'failed';
        pcntl_waitpid($child, $status);
        echo '|', pcntl_wifsignaled($status) && pcntl_wtermsig($status) === SIGTERM ? 'terminated' : 'survived';
        echo '|', posix_kill(process_id: getmypid(), signal: 0) ? 'alive' : 'missing';"#,
    );
    assert_eq!(out, "sent|terminated|alive");
}

/// Records ESRCH in the ext/posix error state only, never in PCNTL's, and keeps it on success.
#[test]
fn test_posix_kill_failure_sets_posix_last_error_only() {
    let out = compile_and_run(
        r#"<?php
        echo posix_get_last_error(), '|';
        var_dump(posix_kill(2147483647, 0));
        echo posix_get_last_error(), '|', posix_errno(), '|', pcntl_get_last_error(), '|';
        echo posix_strerror(posix_get_last_error()), '|';
        posix_kill(getmypid(), 0);
        echo posix_get_last_error();"#,
    );
    assert_eq!(out, "0|bool(false)\n3|3|0|No such process|3");
}

/// Raises PHP's `ValueError` for process ids outside the C `pid_t` range.
#[test]
fn test_posix_kill_rejects_out_of_range_process_id() {
    let out = compile_and_run(
        r#"<?php
        try {
            posix_kill(PHP_INT_MAX, 0);
            echo 'no-error';
        } catch (ValueError $error) {
            echo $error->getMessage();
        }"#,
    );
    assert_eq!(
        out,
        "posix_kill(): Argument #1 ($process_id) must be between 2147483648 and 2147483647"
    );
}

/// Probes stream resources, integer descriptors and boxed values, recording ENOTTY and EBADF.
#[test]
fn test_posix_isatty_accepts_streams_and_descriptors() {
    let out = compile_and_run(
        r#"<?php
        function probe(mixed $fd): string {
            return (posix_isatty($fd) ? 'tty' : 'notty') . ':' . posix_get_last_error();
        }
        $path = tempnam(sys_get_temp_dir(), 'posix');
        $file = fopen($path, 'w');
        echo \PoSiX_IsAtTy($file) ? 'tty' : 'notty', ':', posix_get_last_error(), '|';
        echo posix_isatty(2147483647) ? 'tty' : 'notty', ':', posix_get_last_error(), '|';
        echo probe($file), '|', probe(2147483647), '|';
        echo posix_isatty(file_descriptor: $file) ? 'tty' : 'notty', '|';
        echo pcntl_get_last_error();
        fclose($file);
        unlink($path);"#,
    );
    assert_eq!(out, "notty:25|notty:9|notty:25|notty:9|notty|0");
}

/// Warns and records EBADF for descriptors outside `0..=INT_MAX`, as PHP 8.5 does.
#[test]
fn test_posix_isatty_warns_for_out_of_range_descriptor() {
    let out = compile_and_run_capture(
        r#"<?php
        var_dump(posix_isatty(-1));
        echo posix_get_last_error();"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\n9");
    assert!(
        out.stderr.contains(
            "posix_isatty(): Argument #1 ($file_descriptor) must be between 0 and 2147483647"
        ),
        "missing range warning, stderr: {}",
        out.stderr
    );
}

/// Throws PHP's `TypeError` for a closed stream, natively and inside `eval()`, without a warning.
#[test]
fn test_posix_isatty_rejects_closed_stream() {
    let out = compile_and_run_capture(
        r#"<?php
        function probe(mixed $fd): string {
            try {
                return posix_isatty($fd) ? 'tty' : 'notty';
            } catch (TypeError $error) {
                return $error->getMessage();
            }
        }
        $path = tempnam(sys_get_temp_dir(), 'posix');
        $file = fopen($path, 'w');
        fclose($file);
        echo probe($file), '|';
        echo eval('
            $other = fopen($path, "r");
            fclose($other);
            try { posix_isatty($other); return "no-error"; }
            catch (TypeError $error) { return $error->getMessage(); }
        '), '|', posix_get_last_error();
        unlink($path);"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "posix_isatty(): supplied resource is not a valid stream resource|\
         posix_isatty(): supplied resource is not a valid stream resource|0"
    );
    assert!(!out.stderr.contains("Warning"), "unexpected warning: {}", out.stderr);
}

/// Reports the effective user id and the process group a forked child creates for itself.
#[test]
fn test_posix_geteuid_and_getpgrp_report_process_identity() {
    let out = compile_and_run(
        r#"<?php
        echo posix_geteuid() === (int) trim(shell_exec('id -u')) ? 'euid' : 'euid-mismatch';
        echo '|', posix_getpgrp() > 0 ? 'pgrp' : 'pgrp-missing';
        $child = pcntl_fork();
        if ($child === 0) {
            $session = posix_setsid();
            echo '|', posix_getpgrp() === $session && $session === getmypid() ? 'leader' : 'not-leader';
            exit(0);
        }
        pcntl_waitpid($child, $status);"#,
    );
    assert_eq!(out, "euid|pgrp|leader");
}

/// Returns libc's error text for known and unknown errno values.
#[test]
fn test_posix_strerror_formats_errno_values() {
    let out = compile_and_run(
        r#"<?php
        echo posix_strerror(1), '|', posix_strerror(error_code: 3), '|', strlen(posix_strerror(99999)) > 0 ? 'unknown' : 'empty';"#,
    );
    assert_eq!(out, "Operation not permitted|No such process|unknown");
}

/// Exposes the ext/posix builtins inside eval with the same shared errno state.
#[test]
fn test_posix_eval_builtins_share_bridge_state() {
    let out = compile_and_run(
        r#"<?php
        posix_kill(2147483647, 0);
        echo eval('
            $path = tempnam(sys_get_temp_dir(), "posix");
            $file = fopen($path, "w");
            $parts = [posix_get_last_error()];
            $parts[] = posix_isatty($file) ? "tty" : "notty";
            $parts[] = posix_errno();
            $parts[] = posix_isatty(2147483647) ? "tty" : "notty";
            $parts[] = posix_get_last_error();
            $parts[] = posix_geteuid() === (int) trim(shell_exec("id -u")) ? "euid" : "bad";
            $parts[] = posix_getpgrp() > 0 ? "pgrp" : "bad";
            $parts[] = posix_kill(getmypid(), 0) ? "alive" : "bad";
            $parts[] = posix_strerror(3);
            try { posix_kill(PHP_INT_MAX, 0); $parts[] = "no-error"; }
            catch (ValueError $error) { $parts[] = "value-error"; }
            fclose($file);
            unlink($path);
            return implode("|", $parts);
        ');
        echo '|', posix_get_last_error();"#,
    );
    assert_eq!(
        out,
        "3|notty|25|notty|9|euid|pgrp|alive|No such process|value-error|9"
    );
}

/// Keeps a store observed only by the catch block alive across a throwing `posix_isatty()`.
#[test]
fn test_posix_isatty_closed_stream_throw_preserves_catch_visible_store() {
    let out = compile_and_run(
        r#"<?php
        function probe($file): int {
            $state = 1;
            try {
                $state = 2;
                posix_isatty($file);
                $state = 3;
            } catch (TypeError $error) {
                return $state;
            }
            return $state;
        }
        $path = tempnam(sys_get_temp_dir(), 'posix');
        $file = fopen($path, 'w');
        fclose($file);
        echo probe($file);
        unlink($path);"#,
    );
    assert_eq!(out, "2");
}

/// Probes an open socket listener inside `eval()` instead of reporting it as closed.
///
/// The probe errno is ENOTTY on Linux and ENXIO on Darwin, so only "not EBADF" is asserted.
#[test]
fn test_posix_isatty_eval_probes_socket_listener() {
    let out = compile_and_run(
        r#"<?php
        echo eval('
            $server = stream_socket_server("tcp://127.0.0.1:0");
            try { $result = posix_isatty($server) ? "tty" : "notty"; }
            catch (TypeError $error) { $result = $error->getMessage(); }
            $errno = posix_get_last_error();
            $result .= $errno !== 0 && $errno !== 9 ? ":probed" : ":errno-" . $errno;
            fclose($server);
            return $result;
        ');"#,
    );
    assert_eq!(out, "notty:probed");
}
