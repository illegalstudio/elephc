//! Purpose:
//! End-to-end codegen coverage for `proc_open()`, `proc_get_status()`, `proc_close()`,
//! `proc_terminate()` and `proc_nice()` over the `elephc_proc` bridge.
//!
//! Called from:
//! - `cargo test --test codegen_tests proc_` through Rust's test harness.
//!
//! Key details:
//! - Every fixture spawns `/bin/sh` or a PATH tool, so it needs a Unix host. Each one
//!   prints the same text under PHP 8.5 (`ELEPHC_PHP_CHECK=1` compares them).
//! - Status loops poll with `usleep()` and a bound, never a fixed sleep, so a loaded
//!   machine only makes them slower.

use crate::support::*;

/// An array command's stdout pipe carries its output; `proc_close()` returns the exit code.
#[test]
fn test_proc_open_reads_echo_through_a_pipe() {
    let out = compile_and_run(
        r#"<?php
$process = proc_open(['echo', 'hello', 'world'], [1 => ['pipe', 'w']], $pipes);
echo get_resource_type($process), "|", count($pipes), "|";
echo stream_get_contents($pipes[1]);
fclose($pipes[1]);
echo proc_close($process), "|", get_resource_type($process);
"#,
    );
    assert_eq!(out, "process|1|hello world\n0|Unknown");
}

/// Writing to `cat` through its stdin pipe comes back on its stdout pipe.
#[test]
fn test_proc_open_round_trips_through_cat() {
    let out = compile_and_run(
        r#"<?php
$process = proc_open(['cat'], [0 => ['pipe', 'r'], 1 => ['pipe', 'w']], $pipes);
fwrite($pipes[0], "first line\n");
fwrite($pipes[0], "second line\n");
fclose($pipes[0]);
echo stream_get_contents($pipes[1]);
fclose($pipes[1]);
echo proc_close($process);
"#,
    );
    assert_eq!(out, "first line\nsecond line\n0");
}

/// A non-blocking stdout pipe is drained with `stream_select()` until EOF.
#[test]
fn test_proc_open_nonblocking_pipe_with_stream_select() {
    let out = compile_and_run(
        r#"<?php
$process = proc_open('echo one; sleep 0.2; echo two', [1 => ['pipe', 'w']], $pipes);
stream_set_blocking($pipes[1], false);
$buffer = '';
$rounds = 0;
while (!feof($pipes[1]) && $rounds < 200) {
    $read = [$pipes[1]];
    $write = null;
    $except = null;
    if (stream_select($read, $write, $except, 1, 0) > 0) {
        $buffer .= fread($pipes[1], 8192);
    }
    $rounds++;
}
fclose($pipes[1]);
echo $buffer, proc_close($process);
"#,
    );
    assert_eq!(out, "one\ntwo\n0");
}

/// `$cwd` and `$env_vars` reach the child, and the env array replaces the environment.
#[test]
fn test_proc_open_cwd_and_environment() {
    let out = compile_and_run(
        r#"<?php
$process = proc_open(
    'echo "$GREETING-${HOME:-unset}"; pwd',
    [1 => ['pipe', 'w'], 2 => ['redirect', 1]],
    $pipes,
    '/',
    ['GREETING' => 'hi', 'EMPTY' => ''],
);
echo stream_get_contents($pipes[1]);
fclose($pipes[1]);
echo proc_close($process);
"#,
    );
    assert_eq!(out, "hi-unset\n/\n0");
}

/// The exit code is reported through `proc_get_status()`, cached, and then by `proc_close()`.
#[test]
fn test_proc_get_status_caches_the_exit_code() {
    let out = compile_and_run(
        r#"<?php
$process = proc_open('exit 3', [], $pipes);
$status = proc_get_status($process);
for ($i = 0; $status['running'] && $i < 500; $i++) {
    usleep(10000);
    $status = proc_get_status($process);
}
echo implode(',', array_keys($status)), "\n";
echo $status['command'], '|', $status['pid'] > 0 ? 'pid' : 'nopid', '|';
var_dump($status['running'], $status['cached'], $status['exitcode']);
$again = proc_get_status($process);
var_dump($again['cached'], $again['exitcode']);
echo proc_close($process);
"#,
    );
    assert_eq!(
        out,
        "command,pid,cached,running,signaled,stopped,exitcode,termsig,stopsig\n\
         exit 3|pid|bool(false)\nbool(true)\nint(3)\nbool(true)\nint(3)\n3"
    );
}

/// `proc_terminate()` signals the child, which is reported once as signaled.
#[test]
fn test_proc_terminate_stops_a_sleeping_child() {
    let out = compile_and_run(
        r#"<?php
$process = proc_open(['sleep', '5'], [], $pipes);
var_dump(proc_terminate($process));
$status = proc_get_status($process);
for ($i = 0; $status['running'] && $i < 500; $i++) {
    usleep(10000);
    $status = proc_get_status($process);
}
var_dump($status['signaled'], $status['termsig'], $status['cached']);
echo proc_close($process);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nint(15)\nbool(false)\n-1");
}

/// File, null and existing-stream descriptors, plus a case-insensitive namespaced call.
#[test]
fn test_proc_open_file_null_and_stream_descriptors() {
    let out = compile_and_run(
        r#"<?php
namespace App;
$path = sys_get_temp_dir() . '/elephc_proc_open_' . getmypid() . '.txt';
$sink = fopen($path, 'w');
$process = \PROC_OPEN(
    ['sh', '-c', 'cat; echo out; echo err >&2'],
    [0 => ['null'], 1 => $sink, 2 => ['file', $path . '.err', 'w']],
    $pipes,
);
echo count($pipes), '|', Proc_Close($process), '|';
fclose($sink);
echo file_get_contents($path), file_get_contents($path . '.err');
unlink($path);
unlink($path . '.err');
"#,
    );
    assert_eq!(out, "0|0|out\nerr\n");
}

/// A spawn failure warns, answers false, and leaves `$pipes` as it was.
#[test]
fn test_proc_open_spawn_failure_warns_and_returns_false() {
    let out = compile_and_run_capture(
        r#"<?php
$pipes = 'untouched';
var_dump(proc_open(['elephc-definitely-missing-command'], [1 => ['pipe', 'w']], $pipes));
var_dump($pipes);
"#,
    );
    assert_eq!(out.stdout, "bool(false)\nstring(9) \"untouched\"\n");
    assert!(
        out.stderr.contains("proc_open(): posix_spawn() failed: No such file or directory"),
        "{}",
        out.stderr
    );
}

/// php-src's ValueError and TypeError texts for malformed arguments and closed processes.
#[test]
fn test_proc_open_argument_errors() {
    let out = compile_and_run(
        r#"<?php
function attempt(callable $call): void {
    try {
        $call();
        echo "no error\n";
    } catch (\Throwable $error) {
        echo get_class($error), ': ', $error->getMessage(), "\n";
    }
}
attempt(function () { proc_open([], [], $pipes); });
attempt(function () { proc_open(['a', "b\0c"], [], $pipes); });
attempt(function () { proc_open('true', ['x' => ['pipe', 'r']], $pipes); });
attempt(function () { proc_open('true', [0 => 5], $pipes); });
attempt(function () { proc_open('true', [0 => []], $pipes); });
attempt(function () { proc_open('true', [0 => ['pipe']], $pipes); });
attempt(function () { proc_open('true', [0 => ['redirect', 'x']], $pipes); });
$process = proc_open('true', [], $pipes);
proc_close($process);
attempt(fn() => proc_close($process));
attempt(fn() => proc_get_status(STDIN));
"#,
    );
    assert_eq!(
        out,
        "ValueError: proc_open(): Argument #1 ($command) must not be empty\n\
         ValueError: Command array element 2 contains a null byte\n\
         ValueError: proc_open(): Argument #2 ($descriptor_spec) must be an integer indexed array\n\
         ValueError: proc_open(): Argument #2 ($descriptor_spec) must only contain arrays and streams\n\
         ValueError: Missing handle qualifier\n\
         ValueError: Missing mode parameter for 'pipe'\n\
         ValueError: Redirection target must be of type int, string given\n\
         TypeError: proc_close(): supplied resource is not a valid process resource\n\
         TypeError: proc_get_status(): supplied resource is not a valid process resource\n"
    );
}

/// `proc_nice(0)` succeeds without changing the priority.
#[test]
fn test_proc_nice_zero_succeeds() {
    let out = compile_and_run("<?php var_dump(proc_nice(0));");
    assert_eq!(out, "bool(true)\n");
}

/// A proc builtin reached only through a string callable keeps its prelude helper.
#[test]
fn test_proc_nice_through_call_user_func_keeps_helper() {
    let out = compile_and_run(
        r#"<?php
var_dump(call_user_func('proc_nice', 0));
var_dump(call_user_func_array('proc_nice', [0]));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

/// The standard streams are accepted as descriptors, from a variable and from a literal list.
#[test]
fn test_proc_open_inherits_standard_streams() {
    let out = compile_and_run(
        r#"<?php
$stdout = STDOUT;
$process = proc_open(['echo', 'from a variable'], [1 => $stdout], $pipes);
echo proc_close($process), "\n";
$process = proc_open(['echo', 'from a list'], [STDIN, STDOUT, STDERR], $pipes);
echo proc_close($process), "\n";
"#,
    );
    assert_eq!(out, "from a variable\n0\nfrom a list\n0\n");
}

/// A pipe stream left open across `proc_close()` keeps its descriptor number, so a later
/// `fclose()` of it can never close a file opened after `proc_close()` (which would
/// otherwise reuse the freed number and lose its writes).
#[test]
fn test_proc_close_never_frees_a_pipe_number_a_stale_fclose_could_hit() {
    let out = compile_and_run(
        r#"<?php
$path = sys_get_temp_dir() . '/elephc_proc_stale_' . getmypid() . '.txt';
$process = proc_open(['echo', 'hi'], [1 => ['pipe', 'w']], $pipes);
echo stream_get_contents($pipes[1]);
echo proc_close($process), "\n";
$file = fopen($path, 'w');
fclose($pipes[1]);
var_dump(fwrite($file, 'kept'));
fclose($file);
echo file_get_contents($path), "\n";
unlink($path);
"#,
    );
    assert_eq!(out, "hi\n0\nint(4)\nkept\n");
}
