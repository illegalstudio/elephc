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

/// Probes `?int` and `int|false` descriptors, whose inline storage is not a boxed `Mixed` cell.
#[test]
fn test_posix_isatty_accepts_nullable_and_scalar_union_descriptors() {
    let out = compile_and_run_capture(
        r#"<?php
        function nullable(?int $fd): string {
            $result = posix_isatty($fd);
            return gettype($result) . ':' . posix_get_last_error();
        }
        function falsable(int|false $fd): string {
            return (posix_isatty($fd) ? 'tty' : 'notty') . ':' . posix_get_last_error();
        }
        echo nullable(2147483647), '|', falsable(2147483647), '|';
        echo explode(':', nullable(null))[0];"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "boolean:9|notty:9|boolean");
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

/// Throws PHP's `TypeError` for refused `posix_kill()` operands before any signal is sent.
///
/// Every refused operand would otherwise coerce to pid 0 (the caller's process group), so the
/// probes use signal 0 and a `posix_get_last_error()` sentinel shows the bridge never ran.
#[test]
fn test_posix_kill_rejects_non_integer_operands_before_signalling() {
    let out = compile_and_run_capture(
        r#"<?php
        function kill(mixed $pid, mixed $signal): string {
            try {
                return var_export(posix_kill($pid, $signal), true);
            } catch (TypeError $error) {
                return $error->getMessage();
            }
        }
        posix_kill(2147483647, 0);
        echo kill([], 0), "\n";
        echo kill('abc', 0), "\n";
        echo kill('12abc', 0), "\n";
        echo kill(new stdClass(), 0), "\n";
        echo kill(getmypid(), []), "\n";
        echo kill(getmypid(), 'abc'), "\n";
        echo posix_get_last_error(), "\n";
        try { posix_kill('abc', 0); } catch (TypeError $error) { echo $error->getMessage(), "\n"; }
        echo kill(' ' . getmypid() . ' ', '0'), '|', kill((string) getmypid(), 0.0), "\n";
        echo eval('
            $parts = [];
            foreach ([[], "abc", new stdClass()] as $pid) {
                try { $parts[] = var_export(posix_kill($pid, 0), true); }
                catch (TypeError $error) { $parts[] = $error->getMessage(); }
            }
            foreach ([[], "abc"] as $signal) {
                try { $parts[] = var_export(posix_kill(getmypid(), $signal), true); }
                catch (TypeError $error) { $parts[] = $error->getMessage(); }
            }
            $parts[] = posix_get_last_error();
            $parts[] = var_export(posix_kill(" " . getmypid() . " ", "0"), true);
            return implode("\n", $parts);
        ');"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "posix_kill(): Argument #1 ($process_id) must be of type int, array given\n\
         posix_kill(): Argument #1 ($process_id) must be of type int, string given\n\
         posix_kill(): Argument #1 ($process_id) must be of type int, string given\n\
         posix_kill(): Argument #1 ($process_id) must be of type int, stdClass given\n\
         posix_kill(): Argument #2 ($signal) must be of type int, array given\n\
         posix_kill(): Argument #2 ($signal) must be of type int, string given\n\
         3\n\
         posix_kill(): Argument #1 ($process_id) must be of type int, string given\n\
         true|true\n\
         posix_kill(): Argument #1 ($process_id) must be of type int, array given\n\
         posix_kill(): Argument #1 ($process_id) must be of type int, string given\n\
         posix_kill(): Argument #1 ($process_id) must be of type int, stdClass given\n\
         posix_kill(): Argument #2 ($signal) must be of type int, array given\n\
         posix_kill(): Argument #2 ($signal) must be of type int, string given\n\
         3\n\
         true"
    );
}

/// Applies PHP's `int` parameter check to `posix_strerror()`, natively and inside `eval()`.
#[test]
fn test_posix_strerror_rejects_non_integer_operands() {
    let out = compile_and_run_capture(
        r#"<?php
        function message(mixed $code): string {
            try {
                return posix_strerror($code);
            } catch (TypeError $error) {
                return $error->getMessage();
            }
        }
        echo message([]), '|', message('abc'), '|', message(new stdClass()), '|', message(' 3 '), '|';
        try { echo posix_strerror('abc'); } catch (TypeError $error) { echo $error->getMessage(); }
        echo '|', eval('
            $parts = [];
            foreach ([[], "abc", new stdClass(), " 3 "] as $code) {
                try { $parts[] = posix_strerror($code); }
                catch (TypeError $error) { $parts[] = $error->getMessage(); }
            }
            return implode("|", $parts);
        ');"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let refused = "posix_strerror(): Argument #1 ($error_code) must be of type int";
    assert_eq!(
        out.stdout,
        format!(
            "{refused}, array given|{refused}, string given|{refused}, stdClass given|No such process|\
             {refused}, string given|\
             {refused}, array given|{refused}, string given|{refused}, stdClass given|No such process"
        )
    );
}

/// Warns and returns false for `posix_isatty()` operands outside `int|resource`, as PHP does.
#[test]
fn test_posix_isatty_warns_for_non_integer_operands() {
    let out = compile_and_run_capture(
        r#"<?php
        function probe(mixed $fd): string {
            return var_export(posix_isatty($fd), true);
        }
        echo probe([]), '|', probe('abc'), '|', probe(new stdClass()), '|', var_export(posix_isatty('abc'), true), '|';
        echo eval('return var_export(posix_isatty([]), true) . "|" . var_export(posix_isatty(new stdClass()), true);');"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "false|false|false|false|false|false");
    let warning = "posix_isatty(): Argument #1 ($file_descriptor) must be of type int|resource";
    for given in ["array given", "string given", "stdClass given"] {
        assert!(
            out.stderr.contains(&format!("{warning}, {given}")),
            "missing {given} warning, stderr: {}",
            out.stderr
        );
    }
}

/// Warns and returns false for unrepresentable float descriptors instead of throwing, as PHP does.
///
/// NaN, the infinities and floats outside PHP's `int` range fail the `int|resource` check;
/// direct and boxed floats must both warn, while a lossy in-range float still only deprecates.
#[test]
fn test_posix_isatty_warns_for_unrepresentable_float_operands() {
    let out = compile_and_run_capture(
        r#"<?php
        function probe(mixed $fd): string {
            return var_export(posix_isatty($fd), true);
        }
        $inf = INF;
        echo var_export(posix_isatty($inf), true), '|', var_export(posix_isatty(1e30), true), '|';
        echo probe(NAN), '|', probe(-INF), '|', probe(1e30), '|';
        echo var_export(posix_isatty(1.5), true), '|', probe(1.0), '|done';"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "false|false|false|false|false|false|false|done");
    let warning = "posix_isatty(): Argument #1 ($file_descriptor) must be of type int|resource, float given";
    assert_eq!(out.stderr.matches(warning).count(), 5, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("Implicit conversion from float 1.5 to int loses precision"),
        "stderr: {}",
        out.stderr
    );
}

/// Keeps stores read only by a catch block alive across refused `int` operands.
///
/// The int guard throws before the bridge runs, so each call must be treated as throwing:
/// otherwise dead-store elimination drops `$state = 2` and the catch reads the stale 1.
#[test]
fn test_refused_int_operand_throw_preserves_catch_visible_store() {
    let out = compile_and_run(
        r#"<?php
        function strerror_literal(): int {
            $state = 1;
            try {
                $state = 2;
                posix_strerror('abc');
                $state = 3;
            } catch (\TypeError $error) {
                return $state;
            }
            return $state;
        }
        function pcntl_strerror_boxed(mixed $code): int {
            $state = 1;
            try {
                $state = 2;
                pcntl_strerror($code);
                $state = 3;
            } catch (\TypeError $error) {
                return $state;
            }
            return $state;
        }
        function kill_literal(): int {
            $state = 1;
            try {
                $state = 2;
                posix_kill('abc', 0);
                $state = 3;
            } catch (\TypeError $error) {
                return $state;
            }
            return $state;
        }
        echo strerror_literal(), pcntl_strerror_boxed([]), kill_literal();"#,
    );
    assert_eq!(out, "222");
}

/// Refuses numeric strings outside PHP's `int` range instead of collapsing them to pid 0.
///
/// `"1e999"` used to pass the grammar check and convert through infinity to 0, which would
/// signal the caller's process group; probes use signal 0 so a regression stays harmless.
#[test]
fn test_posix_int_operands_reject_out_of_range_numeric_strings() {
    let out = compile_and_run_capture(
        r#"<?php
        function kill(mixed $pid): string {
            try { return var_export(posix_kill($pid, 0), true); }
            catch (Throwable $error) { return get_class($error) . ': ' . $error->getMessage(); }
        }
        function message(mixed $code): string {
            try { return posix_strerror($code); }
            catch (Throwable $error) { return get_class($error) . ': ' . $error->getMessage(); }
        }
        foreach (['1e999', '9223372036854775808', ' 1e20 ', '9223372036854775807'] as $value) {
            echo kill($value), "\n";
        }
        echo message('-1e999'), "\n";
        try { posix_kill('1e999', 0); } catch (TypeError $error) { echo $error->getMessage(), "\n"; }
        try { posix_strerror(' 1e20 '); } catch (TypeError $error) { echo $error->getMessage(), "\n"; }
        echo eval('
            $parts = [];
            foreach (["1e999", "9223372036854775808", " 1e20 "] as $value) {
                try { $parts[] = var_export(posix_kill($value, 0), true); }
                catch (TypeError $error) { $parts[] = $error->getMessage(); }
            }
            return implode("\n", $parts);
        ');"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let kill = "posix_kill(): Argument #1 ($process_id) must be of type int, string given";
    let message = "posix_strerror(): Argument #1 ($error_code) must be of type int, string given";
    assert_eq!(
        out.stdout,
        format!(
            "TypeError: {kill}\nTypeError: {kill}\nTypeError: {kill}\n\
             ValueError: posix_kill(): Argument #1 ($process_id) must be between 2147483648 and 2147483647\n\
             TypeError: {message}\n{kill}\n{message}\n{kill}\n{kill}\n{kill}"
        )
    );
}

/// Refuses every non-int `posix_*` operand inside an `eval()` that declares `strict_types`,
/// while a weak-mode eval keeps coercing numeric strings and integral floats.
///
/// The eval adapters used to apply weak conversion regardless of the declaration, so a strict
/// `posix_kill("0", 0)` probed the caller's process group instead of throwing. The sources are
/// runtime-selected so the interpreter, not literal AOT lowering, runs them.
#[test]
fn test_posix_eval_int_operands_follow_strict_types() {
    let out = compile_and_run_capture(
        r#"<?php
$strict = $argc > 0 ? <<<'__ELEPHC_EVAL__'
declare(strict_types=1);
$parts = [];
foreach (["0", 1.0, true, null] as $pid) {
    try { $parts[] = posix_kill($pid, 0) ? "true" : "false"; }
    catch (TypeError $error) { $parts[] = $error->getMessage(); }
}
try { $parts[] = posix_kill(getmypid(), "0") ? "true" : "false"; }
catch (TypeError $error) { $parts[] = $error->getMessage(); }
foreach (["3", 3.0] as $code) {
    try { $parts[] = posix_strerror($code); }
    catch (TypeError $error) { $parts[] = $error->getMessage(); }
    try { $parts[] = pcntl_strerror($code); }
    catch (TypeError $error) { $parts[] = $error->getMessage(); }
}
$parts[] = posix_isatty("0") ? "true" : "false";
$parts[] = posix_isatty(1.0) ? "true" : "false";
return implode("\n", $parts);
__ELEPHC_EVAL__ : '';
$weak = $argc > 0 ? 'return (posix_kill((string) getmypid(), "0") ? "true" : "false") . "|" . posix_strerror("3") . "|" . pcntl_strerror(3.0);' : '';
echo eval($strict), "\n", eval($weak), "\n";"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let pid = "posix_kill(): Argument #1 ($process_id) must be of type int";
    let signal = "posix_kill(): Argument #2 ($signal) must be of type int, string given";
    let posix = "posix_strerror(): Argument #1 ($error_code) must be of type int";
    let pcntl = "pcntl_strerror(): Argument #1 ($error_code) must be of type int";
    assert_eq!(
        out.stdout,
        format!(
            "{pid}, string given\n{pid}, float given\n{pid}, true given\n{pid}, null given\n{signal}\n\
             {posix}, string given\n{pcntl}, string given\n{posix}, float given\n{pcntl}, float given\n\
             false\nfalse\ntrue|No such process|No such process\n"
        )
    );
    let warning = "posix_isatty(): Argument #1 ($file_descriptor) must be of type int|resource";
    for given in ["string given", "float given"] {
        assert!(
            out.stderr.contains(&format!("{warning}, {given}")),
            "missing {given} warning, stderr: {}",
            out.stderr
        );
    }
}
