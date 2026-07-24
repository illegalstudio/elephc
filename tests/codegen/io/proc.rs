//! Purpose:
//! Integration tests for the `proc_open`/`proc_close` builtins (C1b parity).
//!
//! Called from:
//! - `cargo test` through the codegen test harness, via `tests/codegen/io.rs`.
//!
//! Key details:
//! - C1b ships a real pipe-only runtime on macOS-aarch64, Linux-aarch64, and
//!   Linux-x86_64 (`fork`/`pipe`/`execve`/`wait4`); `proc_open` returns a
//!   process resource and `proc_close` reaps the child and returns its exit code.
//! - Windows-x86_64 uses the C1c CreatePipe/CreateProcessW runtime; its
//!   compile-only structural coverage lives in `tests/codegen/windows_pe.rs`.

use super::*;

/// Verifies proc_open returns a process resource (not `false`) when given a
/// valid pipe descriptor spec. Parity flip of the former C1a stub test.
#[test]
fn test_proc_open_returns_resource() {
    let out = compile_and_run(
        r#"<?php
$pipes = [];
$r = proc_open("echo hi", [0 => ["pipe", "r"], 1 => ["pipe", "w"]], $pipes);
echo $r === false ? "false" : "resource";
"#,
    );
    assert_eq!(out, "resource");
}

/// Verifies binary pipe mode suffixes keep php-src's first-byte direction
/// semantics: `rb` behaves as read mode and `wb` behaves as write mode. The
/// Windows filter may choose CRLF line endings, so only its payload prefix is
/// part of the cross-platform assertion.
#[test]
fn test_proc_open_accepts_binary_pipe_modes() {
    let command = if target().platform == Platform::Windows {
        "findstr .*"
    } else {
        "cat"
    };
    let source =
        r#"<?php
$pipes = [];
$process = proc_open(
    "__COMMAND__",
    [0 => ["pipe", "rb"], 1 => ["pipe", "wb"]],
    $pipes,
);
if ($process === false) {
    echo "fail";
} else {
    fwrite($pipes[0], "binary-direction\n");
    fclose($pipes[0]);
    $output = fread($pipes[1], 100);
    fclose($pipes[1]);
    proc_close($process);
    echo substr($output, 0, 16) === "binary-direction" ? "ok" : "missing";
}
"#
        .replace("__COMMAND__", command);
    let out = compile_and_run(&source);
    assert_eq!(out, "ok");
}

/// Verifies PHP weakly coerces an integer pipe mode to a non-write-leading
/// string, so the parent receives the pipe's write end just as php-src does.
/// Windows filters may append CRLF, so the assertion compares the payload.
#[test]
fn test_proc_open_coerces_integer_pipe_mode_like_php() {
    let command = if target().platform == Platform::Windows {
        "findstr .*"
    } else {
        "cat"
    };
    let source =
        r#"<?php
$pipes = [];
$process = proc_open("__COMMAND__", [0 => ["pipe", 1], 1 => ["pipe", "w"]], $pipes);
if ($process === false) {
    echo "false";
} else {
    fwrite($pipes[0], "scalar-mode\n");
    fclose($pipes[0]);
    echo fread($pipes[1], 100);
    fclose($pipes[1]);
    proc_close($process);
}
"#
        .replace("__COMMAND__", command);
    let out = compile_and_run(&source);
    assert_eq!(
        out.trim_end_matches(&['\r', '\n'][..]),
        "scalar-mode"
    );
}

/// Verifies proc_close reaps the child and returns the exit status. `echo hi`
/// exits 0, so the close must return `0`. Replaces the former C1a compile-only
/// failure test.
#[test]
fn test_proc_close_returns_exit_status() {
    let out = compile_and_run(
        r#"<?php
$pipes = [];
$r = proc_open("echo hi", [0 => ["pipe", "r"], 1 => ["pipe", "w"]], $pipes);
if ($r === false) {
    echo "fail";
} else {
    fclose($pipes[0]);
    fread($pipes[1], 100);
    fclose($pipes[1]);
    echo proc_close($r);
}
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies a live process reports PHP's full non-cached status shape before
/// `proc_terminate` forwards SIGTERM on Unix. The explicit close then reaps the
/// child and exercises status-registry teardown after a prior status lookup.
#[test]
fn test_proc_get_status_then_terminate() {
    let output = compile_and_run_capture(
        r#"<?php
$pipes = [];
$process = proc_open("sleep 10", [1 => ["pipe", "w"]], $pipes);
if ($process === false) {
    echo "false";
} else {
    $status = proc_get_status($process);
    echo ($status["running"] ? "running" : "stopped");
    echo ":" . ($status["cached"] ? "cached" : "fresh");
    echo proc_terminate($process) ? ":terminated" : ":failed";
    proc_close($process);
}
"#,
    );
    assert!(
        output.success,
        "proc_get_status process exited unsuccessfully: stdout={:?}, stderr={:?}",
        output.stdout,
        output.stderr
    );
    assert_eq!(output.stdout, "running:fresh:terminated");
}

/// Verifies `proc_terminate` forwards its default SIGTERM without requiring a
/// preceding status query, then lets `proc_close` reap the terminated child.
#[test]
fn test_proc_terminate_reaps_live_child() {
    let out = compile_and_run(
        r#"<?php
$pipes = [];
$process = proc_open("sleep 10", [1 => ["pipe", "w"]], $pipes);
if ($process === false) {
    echo "false";
} else {
    echo proc_terminate($process) ? "terminated" : "failed";
    proc_close($process);
}
"#,
    );
    assert_eq!(out, "terminated");
}

/// Verifies weak PHP typing coerces a numeric string signal before terminating the child.
#[test]
fn test_proc_terminate_coerces_numeric_string_signal() {
    let out = compile_and_run(
        r#"<?php
$pipes = [];
$process = proc_open("sleep 10", [1 => ["pipe", "w"]], $pipes);
if ($process === false) {
    echo "false";
} else {
    echo proc_terminate($process, "15") ? "terminated" : "failed";
    proc_close($process);
}
"#,
    );
    assert_eq!(out, "terminated");
}

/// Verifies a runtime-produced numeric string uses strict PHP int-parameter coercion.
#[test]
fn test_proc_terminate_coerces_dynamic_numeric_string_signal() {
    let out = compile_and_run(
        r#"<?php
function process_signal(): string {
    return "15";
}

$pipes = [];
$process = proc_open("sleep 10", [1 => ["pipe", "w"]], $pipes);
if ($process === false) {
    echo "false";
} else {
    echo proc_terminate($process, process_signal()) ? "terminated" : "failed";
    proc_close($process);
}
"#,
    );
    assert_eq!(out, "terminated");
}

/// Verifies a runtime-produced non-numeric string throws PHP's catchable TypeError.
#[test]
fn test_proc_terminate_rejects_dynamic_non_numeric_string_signal() {
    let output = compile_and_run_capture(
        r#"<?php
function invalid_process_signal(): string {
    return "not-a-signal";
}

$pipes = [];
$process = proc_open("sleep 10", [1 => ["pipe", "w"]], $pipes);
if ($process === false) {
    echo "false";
} else {
    try {
        proc_terminate($process, invalid_process_signal());
        echo "missing-error";
    } catch (TypeError $error) {
        echo get_class($error) . ":" . $error->getMessage();
        proc_terminate($process);
    }
    proc_close($process);
}
"#,
    );
    assert!(
        output.success,
        "dynamic signal TypeError was not caught: stdout={:?}, stderr={:?}",
        output.stdout,
        output.stderr
    );
    assert_eq!(
        output.stdout,
        "TypeError:proc_terminate(): Argument #2 ($signal) must be of type int, string given"
    );
}

/// Verifies a normal exit remains available to `proc_close` after a status
/// query. Unix caches the reaped wait status, while Windows keeps the process
/// handle live and deliberately reports `cached => false`, matching php-src.
#[test]
fn test_proc_get_status_normal_exit_preserves_proc_close_code() {
    let out = compile_and_run(
        r#"<?php
$pipes = [];
$process = proc_open("exit 7", [1 => ["pipe", "w"]], $pipes);
if ($process === false) {
    echo "false";
} else {
    usleep(100000);
    $status = proc_get_status($process);
    echo ($status["cached"] ? "cached" : "fresh"), ":", $status["exitcode"], ":", proc_close($process);
}
"#,
    );
    assert_eq!(
        out,
        if cfg!(windows) {
            "fresh:7:7"
        } else {
            "cached:7:7"
        }
    );
}

/// Reads back a pipe populated through `proc_open`'s by-reference `$pipes`
/// parameter. This protects the checker flow fact that an empty input array
/// becomes an integer-keyed associative array of stream resources after the
/// call, matching descriptor keys that promote packed storage to a hash.
#[test]
fn test_proc_open_pipe_readback() {
    let out = compile_and_run(
        r#"<?php
$pipes = [];
$r = proc_open("echo hi", [1 => ["pipe", "w"]], $pipes);
if ($r === false) { echo "false"; }
else {
  $s = fread($pipes[1], 100);
  fclose($pipes[1]);
  proc_close($r);
  echo $s;
}
"#,
    );
    assert!(out.contains("hi"), "out was: {}", out);
}

/// Verifies the complete PHP parameter list accepts explicit null settings and
/// preserves named-argument mapping while using the implemented pipe runtime.
#[test]
fn test_proc_open_accepts_nullable_optional_settings() {
    let out = compile_and_run(
        r#"<?php
$pipes = [];
$r = proc_open(
    command: "exit 0",
    descriptor_spec: [0 => ["pipe", "r"]],
    pipes: $pipes,
    cwd: null,
    env_vars: null,
    options: null,
);
echo $r === false ? "false" : proc_close($r);
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies a descriptor set larger than the initial empty-array capacity
/// writes the reallocated `$pipes` container back through the by-ref local.
#[test]
fn test_proc_open_writes_back_reallocated_pipes() {
    let out = compile_and_run(
        r#"<?php
$pipes = [];
$process = proc_open(
    "exit 0",
    [
        ["pipe", "r"], ["pipe", "w"], ["pipe", "w"],
        ["pipe", "w"], ["pipe", "w"],
    ],
    $pipes,
);
echo count($pipes);
if ($process !== false) { proc_close($process); }
"#,
    );
    assert_eq!(out, "5");
}

/// Verifies proc_close closes unread child output before waiting, so a child
/// that fills its stdout pipe cannot deadlock the parent. The shell writes far
/// more than a pipe buffer while the PHP program deliberately never reads it.
#[test]
fn test_proc_close_drains_unread_full_pipe_before_waiting() {
    let command = if target().platform.php_os_family() == "Windows" {
        "for /L %i in (1,1,131072) do @echo 012345678901234567890123456789012345678901234567890123456789"
    } else {
        "dd if=/dev/zero bs=65536 count=32 2>/dev/null"
    };
    let out = compile_and_run(
        &format!(r#"<?php
$pipes = [];
$process = proc_open(
    {:?},
    [1 => ["pipe", "w"]],
    $pipes,
);
if ($process === false) {{ echo "fail"; }}
else {{
    proc_close($process);
    echo "done";
}}
"#, command),
    );
    assert_eq!(out, "done");
}
