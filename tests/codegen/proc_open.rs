//! Purpose:
//! End-to-end codegen coverage for `proc_open()`, `proc_get_status()`, `proc_close()`,
//! `proc_terminate()` and `proc_nice()` over the `elephc_proc` bridge.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext proc_` through Rust's test harness.
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

/// A command or environment value whose `__toString()` throws frees the staged builder
/// (probed through the bridge: a freed builder id refuses further staging).
#[test]
fn test_proc_open_throwing_conversion_frees_builder() {
    let out = compile_and_run(
        r#"<?php
class Boom { public function __toString(): string { throw new RuntimeException("boom"); } }
$probe = elephc_proc_builder_new();
elephc_proc_builder_free($probe);
try { proc_open(['echo', new Boom()], [], $pipes); } catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
try { proc_open('true', [], $pipes, null, ['A' => new Boom()]); } catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
var_dump(elephc_proc_builder_env_reset($probe + 1));
var_dump(elephc_proc_builder_env_reset($probe + 2));
"#,
    );
    assert_eq!(out, "boom\nboom\nint(0)\nint(0)\n");
}

/// A proc builtin named only by a string literal and invoked through a variable or a user
/// function's callable parameter still gets its prelude helper.
#[test]
fn test_proc_nice_through_a_variable_callable_keeps_helper() {
    let out = compile_and_run(
        r#"<?php
function run(callable $f) { var_dump($f(0)); }
run('proc_nice');
$f = 'PROC_NICE';
var_dump($f(0));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

/// Coercive mode follows php-src's `int` parameter rules: bools, numeric strings and
/// in-range floats are accepted, with the null and precision-loss deprecations.
#[test]
fn test_proc_nice_coerces_its_priority_like_php() {
    let out = compile_and_run_capture(
        r#"<?php
foreach (['0', ' 0', '0 ', false, null, 0.0, 0.5, '0.5', '1e30', 1e30, NAN, 'abc', '0abc', []] as $value) {
    try { var_dump(proc_nice($value)); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
"#,
    );
    let float_given = "proc_nice(): Argument #1 ($priority) must be of type int, float given\n";
    let string_given = "proc_nice(): Argument #1 ($priority) must be of type int, string given\n";
    assert_eq!(
        out.stdout,
        format!(
            "{}{string_given}{float_given}{float_given}{string_given}{string_given}\
             proc_nice(): Argument #1 ($priority) must be of type int, array given\n",
            "bool(true)\n".repeat(8)
        )
    );
    for deprecation in [
        "Deprecated: proc_nice(): Passing null to parameter #1 ($priority) of type int is deprecated",
        "Deprecated: Implicit conversion from float 0.5 to int loses precision",
        "Deprecated: Implicit conversion from float-string \"0.5\" to int loses precision",
    ] {
        assert!(out.stderr.contains(deprecation), "{}", out.stderr);
    }
}

/// Under `strict_types` only an int is accepted: the signal is never sent for `'15'`.
#[test]
fn test_proc_terminate_and_proc_nice_reject_non_ints_under_strict_types() {
    let out = compile_and_run(
        r#"<?php
declare(strict_types=1);
$process = proc_open(['sleep', '5'], [], $pipes);
foreach (['15', 15.0, true, null] as $signal) {
    try { var_dump(proc_terminate($process, $signal)); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
foreach (['0', 0.0, false, null] as $priority) {
    try { var_dump(proc_nice($priority)); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
var_dump(proc_get_status($process)['running']);
proc_terminate($process, 9);
proc_close($process);
"#,
    );
    assert_eq!(
        out,
        "proc_terminate(): Argument #2 ($signal) must be of type int, string given\n\
         proc_terminate(): Argument #2 ($signal) must be of type int, float given\n\
         proc_terminate(): Argument #2 ($signal) must be of type int, true given\n\
         proc_terminate(): Argument #2 ($signal) must be of type int, null given\n\
         proc_nice(): Argument #1 ($priority) must be of type int, string given\n\
         proc_nice(): Argument #1 ($priority) must be of type int, float given\n\
         proc_nice(): Argument #1 ($priority) must be of type int, false given\n\
         proc_nice(): Argument #1 ($priority) must be of type int, null given\n\
         bool(true)\n"
    );
}

/// A file spec's path goes through PHP's stream wrappers: `php://stdout`, `php://fd/1`,
/// `file://` URLs, `data:` payloads and `php://temp` reach the child as descriptors, and
/// the streams PHP cannot cast fail with its warnings.
#[test]
fn test_proc_open_file_specs_open_stream_urls() {
    let out = compile_and_run_capture(
        r#"<?php
$path = sys_get_temp_dir() . '/elephc_proc_url_' . getmypid() . '.txt';
foreach ([
    ['echo stdout', [1 => ['file', 'php://stdout', 'w']]],
    ['echo fd', [1 => ['file', 'PHP://fd/1', 'w']]],
    ['echo url', [1 => ['file', 'file://' . $path, 'w']]],
    ['cat', [0 => ['file', 'data:text/plain;base64,ZGF0YQo=', 'r'], 1 => ['file', 'php://stdout', 'w']]],
    ['cat', [0 => ['file', 'data:,a+b%21%0A', 'r'], 1 => ['file', 'php://stdout', 'w']]],
    ['echo temp', [1 => ['file', 'php://temp', 'w']]],
    ['true', [1 => ['file', 'php://memory', 'w']]],
    ['true', [0 => ['file', 'data:text,x', 'r']]],
    ['true', [1 => ['file', 'php://bogus', 'w']]],
    ['true', [1 => ['file', 'nope://x', 'w']]],
] as [$command, $spec]) {
    $process = proc_open($command, $spec, $pipes);
    echo $process === false ? "false\n" : proc_close($process) . "\n";
}
echo file_get_contents($path);
unlink($path);
"#,
    );
    assert_eq!(
        out.stdout,
        "stdout\n0\nfd\n0\n0\ndata\n0\na b!\n0\n0\nfalse\nfalse\nfalse\nfalse\nurl\n"
    );
    for warning in [
        "Warning: proc_open(): Cannot represent a stream of type MEMORY as a File Descriptor",
        "Warning: proc_open(data:text,x): Failed to open stream: rfc2397: illegal media type",
        "Warning: proc_open(): Invalid php:// URL specified",
        "Warning: proc_open(php://bogus): Failed to open stream: operation failed",
        "Warning: proc_open(): Unable to find the wrapper \"nope\" - did you forget to enable it when you configured PHP?",
        "Warning: proc_open(nope://x): Failed to open stream: No such file or directory",
    ] {
        assert!(out.stderr.contains(warning), "{}", out.stderr);
    }
}

/// A process resource is not a stream: reaching a stream builtin through `mixed`, it is
/// refused with php-src's TypeError instead of its bridge id being read as a descriptor.
/// (A statically typed `proc_open()` result is already rejected by the checker.)
#[test]
fn test_stream_builtins_reject_a_process_resource() {
    for (call, message) in [
        ("pclose($p)", "pclose(): Argument #1 ($handle) must be an open stream resource"),
        ("fclose($p)", "fclose(): Argument #1 ($stream) must be an open stream resource"),
        ("fwrite($p, 'x')", "fwrite(): Argument #1 ($stream) must be an open stream resource"),
        ("feof($p)", "feof(): Argument #1 ($stream) must be an open stream resource"),
        ("stream_get_contents($p)", "stream_get_contents(): Argument #1 ($stream) must be an open stream resource"),
    ] {
        let source = format!(
            "<?php\nfunction f(mixed $p) {{ return {call}; }}\necho \"start\\n\";\nf(proc_open('true', [], $pipes));\necho \"unreachable\\n\";\n"
        );
        let out = compile_and_run_capture(&source);
        assert!(!out.success, "{source}");
        assert_eq!(out.stdout, "start\n", "{source}");
        assert!(out.stderr.contains(message), "{source}\n{}", out.stderr);
    }
}
