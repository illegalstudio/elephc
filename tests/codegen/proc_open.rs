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

/// An out-of-range signal keeps only its low 32 bits, as php-src's `kill(pid, (int) $signal)`
/// does: `4294967311` and `PHP_INT_MIN + 15` send SIGTERM, `-4294967287` sends SIGKILL.
#[test]
fn test_proc_terminate_narrows_out_of_range_signals_like_php() {
    let out = compile_and_run(
        r#"<?php
foreach ([4294967311, -4294967287, PHP_INT_MIN + 15] as $signal) {
    $process = proc_open(['sleep', '5'], [], $pipes);
    var_dump(proc_terminate($process, $signal));
    $status = proc_get_status($process);
    for ($i = 0; $status['running'] && $i < 200; $i++) {
        usleep(10000);
        $status = proc_get_status($process);
    }
    echo $status['running'] ? 'running' : $status['termsig'], "\n";
    if ($status['running']) {
        proc_terminate($process, 9);
    }
    proc_close($process);
}
"#,
    );
    assert_eq!(out, "bool(true)\n15\nbool(true)\n9\nbool(true)\n15\n");
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
attempt(function () { proc_open(['', 'x'], [], $pipes); });
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
         ValueError: First element must contain a non-empty program name\n\
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

/// An increment outside the C `int` range keeps only its low 32 bits, as php-src's
/// `nice((int) $priority)` does: large negatives never flip to the lowest priority.
/// Only unprivileged-safe increments, so the result is the same under root. The level is
/// read from field 19 of `/proc/self/stat` on Linux (the Alpine test images ship BusyBox
/// `ps`, which has no `-p`; `fgets()` because `file_get_contents()` still reads procfs as
/// empty, #2190) and from `ps` elsewhere; an unreadable level, or a starting
/// level without room for the +2 the fixture expects, prints a line the assertion rejects.
#[test]
fn test_proc_nice_narrows_out_of_range_increments_like_php() {
    let out = compile_and_run(
        r#"<?php
function nice_level() {
    if (file_exists('/proc/self/stat')) {
        $handle = fopen('/proc/self/stat', 'r');
        $stat = (string) fgets($handle);
        fclose($handle);
        $close = strrpos($stat, ')');
        $fields = $close === false ? [] : explode(' ', trim(substr($stat, $close + 1)));
        $raw = $fields[16] ?? '';
    } else {
        $process = proc_open(['sh', '-c', 'ps -o nice= -p $$'], [1 => ['pipe', 'w']], $pipes);
        $raw = (string) stream_get_contents($pipes[1]);
        fclose($pipes[1]);
        proc_close($process);
    }
    $raw = trim($raw);
    if (!is_numeric($raw)) {
        echo "unreadable nice level: '", $raw, "'\n";
        exit(1);
    }
    return (int) $raw;
}
$base = nice_level();
if ($base > 17) {
    echo "starting nice level ", $base, " leaves no room for +2\n";
    exit(1);
}
foreach ([PHP_INT_MIN, -4294967296, 4294967296, -4294967295, 4294967297] as $increment) {
    var_dump(proc_nice($increment));
    echo nice_level() - $base, "\n";
}
"#,
    );
    assert_eq!(out, "bool(true)\n0\nbool(true)\n0\nbool(true)\n0\nbool(true)\n1\nbool(true)\n2\n");
}

/// Through a `callable` parameter, a first-class callable or `call_user_func()`, the signal
/// and priority reach the helper unconverted: `[9]` is php-src's TypeError (not SIGHUP), a
/// fractional float warns, an omitted signal is SIGTERM and an out-of-range one is narrowed.
#[test]
fn test_proc_terminate_and_nice_through_callables_keep_php_checks() {
    let out = compile_and_run_capture(
        r#"<?php
function call(callable $f, ...$args) {
    try { var_dump($f(...$args)); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
function termsig($process) {
    $status = proc_get_status($process);
    for ($i = 0; $status['running'] && $i < 200; $i++) {
        usleep(10000);
        $status = proc_get_status($process);
    }
    echo $status['running'] ? 'running' : $status['termsig'], "\n";
    if ($status['running']) {
        proc_terminate($process, 9);
    }
    proc_close($process);
}
$process = proc_open(['sleep', '5'], [], $pipes);
call('proc_terminate', $process, json_decode('[9]'));
call(proc_terminate(...), $process, json_decode('[9]'));
usleep(100000);
var_dump(proc_get_status($process)['running']);
call('proc_terminate', $process);
termsig($process);
$process = proc_open(['sleep', '5'], [], $pipes);
var_dump(call_user_func(proc_terminate(...), $process, 4294967305));
termsig($process);
$process = proc_open(['sleep', '5'], [], $pipes);
call('proc_terminate', $process, 9.5);
termsig($process);
call('proc_nice', json_decode('"x"'));
call(proc_nice(...), 0.5);
"#,
    );
    assert_eq!(
        out.stdout,
        "proc_terminate(): Argument #2 ($signal) must be of type int, array given\n\
         proc_terminate(): Argument #2 ($signal) must be of type int, array given\n\
         bool(true)\nbool(true)\n15\nbool(true)\n9\nbool(true)\n9\n\
         proc_nice(): Argument #1 ($priority) must be of type int, string given\n\
         bool(true)\n",
        "{}",
        out.stderr
    );
    assert_eq!(
        out.stderr.matches("Implicit conversion from float").count(),
        2,
        "{}",
        out.stderr
    );
}

/// Under `strict_types` a callable call keeps php-src's strict TypeErrors.
#[test]
fn test_proc_terminate_and_nice_through_callables_honor_strict_types() {
    let out = compile_and_run(
        r#"<?php
declare(strict_types=1);
function call(callable $f, ...$args) {
    try { var_dump($f(...$args)); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
$process = proc_open(['sleep', '5'], [], $pipes);
call('proc_terminate', $process, json_decode('"9"'));
call(proc_terminate(...), $process, 9.0);
call('proc_nice', json_decode('"0"'));
var_dump(proc_get_status($process)['running']);
proc_terminate($process, 9);
proc_close($process);
"#,
    );
    assert_eq!(
        out,
        "proc_terminate(): Argument #2 ($signal) must be of type int, string given\n\
         proc_terminate(): Argument #2 ($signal) must be of type int, float given\n\
         proc_nice(): Argument #1 ($priority) must be of type int, string given\n\
         bool(true)\n"
    );
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

/// Under `strict_types` a scalar command or cwd is a TypeError, not a converted string, so
/// nothing is spawned; coercive mode still converts (a `false` cwd becomes `""`).
#[test]
fn test_proc_open_rejects_scalar_command_and_cwd_under_strict_types() {
    let out = compile_and_run_capture(
        r#"<?php
declare(strict_types=1);
function open($command, $cwd): void {
    try {
        $process = proc_open($command, [], $pipes, $cwd);
        echo "spawned ", var_export($process === false ? false : proc_close($process), true), "\n";
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
open(5, null);
open(true, null);
open(1.5, null);
open('true', false);
open('true', 1);
open('true', 1.5);
open(['true'], true);
open('true', '/');
try { proc_open('true', [], $pipes, false); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out.stdout,
        "proc_open(): Argument #1 ($command) must be of type array|string, int given\n\
         proc_open(): Argument #1 ($command) must be of type array|string, true given\n\
         proc_open(): Argument #1 ($command) must be of type array|string, float given\n\
         proc_open(): Argument #4 ($cwd) must be of type ?string, false given\n\
         proc_open(): Argument #4 ($cwd) must be of type ?string, int given\n\
         proc_open(): Argument #4 ($cwd) must be of type ?string, float given\n\
         proc_open(): Argument #4 ($cwd) must be of type ?string, true given\n\
         spawned 0\n\
         proc_open(): Argument #4 ($cwd) must be of type ?string, false given\n",
        "{}",
        out.stderr
    );
    assert!(!out.stderr.contains("posix_spawn"), "{}", out.stderr);
    let coercive = compile_and_run_capture(
        "<?php $cwd = false; var_dump(proc_open('true', [], $pipes, $cwd));",
    );
    assert_eq!(coercive.stdout, "bool(false)\n");
    assert!(coercive.stderr.contains("posix_spawn() failed"), "{}", coercive.stderr);
}

/// Coercive mode converts a `Stringable` command or cwd (and scalar cwds) like PHP; strict
/// mode rejects the object, and a plain object is a TypeError in either mode.
#[test]
fn test_proc_open_converts_stringable_arguments_only_in_coercive_mode() {
    let fixture = r#"
class Text { public function __construct(private string $text) {} public function __toString(): string { return $this->text; } }
class Plain {}
function open($command, $cwd): void {
    try {
        $process = @proc_open($command, [1 => ['pipe', 'w']], $pipes, $cwd);
        if ($process === false) { echo "false\n"; return; }
        echo trim(stream_get_contents($pipes[1])), "|", proc_close($process), "\n";
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
open(new Text('echo object'), null);
open('echo cwd', new Text('/'));
open(['echo', 5, 1.5, true, new Text('x')], new Text('/'));
open('echo plain', new Plain());
open(new Plain(), null);
open('pwd', 1);
open('pwd', false);
$command = new Text('echo direct');
try {
    $process = proc_open($command, [1 => ['pipe', 'w']], $pipes);
    echo trim(stream_get_contents($pipes[1])), "|", proc_close($process), "\n";
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
"#;
    let coercive = compile_and_run(&format!("<?php{fixture}"));
    assert_eq!(
        coercive,
        "object|0\ncwd|0\n5 1.5 1 x|0\n\
         proc_open(): Argument #4 ($cwd) must be of type ?string, Plain given\n\
         proc_open(): Argument #1 ($command) must be of type array|string, Plain given\n\
         false\nfalse\ndirect|0\n"
    );
    let strict = compile_and_run(&format!("<?php\ndeclare(strict_types=1);{fixture}"));
    assert_eq!(
        strict,
        "proc_open(): Argument #1 ($command) must be of type array|string, Text given\n\
         proc_open(): Argument #4 ($cwd) must be of type ?string, Text given\n\
         proc_open(): Argument #4 ($cwd) must be of type ?string, Text given\n\
         proc_open(): Argument #4 ($cwd) must be of type ?string, Plain given\n\
         proc_open(): Argument #1 ($command) must be of type array|string, Plain given\n\
         proc_open(): Argument #4 ($cwd) must be of type ?string, int given\n\
         proc_open(): Argument #4 ($cwd) must be of type ?string, false given\n\
         proc_open(): Argument #1 ($command) must be of type array|string, Text given\n"
    );
}

/// A helper exception releases the operand boxes the lowering created, and leaves the
/// caller's own values (typed locals and `Mixed` elements passed through) intact.
#[test]
fn test_proc_open_caught_helper_errors_release_operand_boxes() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function attempt(string $command, array $spec, string $cwd): void {
    try { proc_open($command, $spec, $pipes, $cwd); } catch (ValueError $e) { echo "v"; }
}
$command = str_repeat('t', 1) . 'rue';
$cwd = sys_get_temp_dir();
$spec = [0 => 5];
$mixed = [$command, $spec];
for ($i = 0; $i < 3; $i++) {
    attempt($command, $spec, $cwd);
    try { proc_open($command . '', [0 => 5, 1 => [$i]], $pipes, $cwd . ''); } catch (ValueError $e) { echo "l"; }
    try { proc_open($mixed[0], $mixed[1], $pipes); } catch (ValueError $e) { echo "m"; }
    try { proc_terminate(STDIN, 15); } catch (TypeError $e) { echo "t"; }
}
echo "|", $command, "|", count($spec), "|", $mixed[0], "|", $cwd === sys_get_temp_dir() ? "same" : "changed", "\n";
"#,
    );
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "vlmtvlmtvlmt|true|1|true|same\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
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
/// the streams PHP cannot cast fail with its warnings. A `data:` URL ignores the spec's
/// mode, as php-src's rfc2397 wrapper does: a `'w'` spec still hands the child its payload.
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
    ['cat', [0 => ['file', 'data:,written%0A', 'w'], 1 => ['file', 'php://stdout', 'w']]],
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
        "stdout\n0\nfd\n0\n0\ndata\n0\na b!\n0\nwritten\n0\n0\nfalse\nfalse\nfalse\nfalse\nurl\n"
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

/// `['socket']` hands the child one end of a socket pair; the parent end is a stream.
#[test]
fn test_proc_open_socket_descriptor() {
    let out = compile_and_run(
        r#"<?php
$process = proc_open(['sh', '-c', 'read line <&3; echo "got $line" >&3'], [3 => ['socket']], $pipes);
echo get_resource_type($pipes[3]), "|";
fwrite($pipes[3], "ping\n");
echo fgets($pipes[3]);
fclose($pipes[3]);
echo proc_close($process);
"#,
    );
    assert_eq!(out, "stream|got ping\n0");
}

/// A warning turned into an exception by the error handler while a later descriptor is
/// staged still frees the descriptors staged before it.
#[test]
fn test_proc_open_throwing_staging_warning_releases_staged_descriptors() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $level, string $message): bool { throw new Exception($message); });
$before = count(scandir('/dev/fd'));
for ($i = 0; $i < 5; $i++) {
    try {
        proc_open(['true'], [0 => ['pipe', 'r'], 1 => ['file', '/nonexistent-dir/out', 'w']], $pipes);
    } catch (Exception $error) {
        if ($i === 0) {
            echo $error->getMessage(), "\n";
        }
    }
}
echo count(scandir('/dev/fd')) - $before;
"#,
    );
    assert_eq!(out, "proc_open(/nonexistent-dir/out): Failed to open stream: No such file or directory\n0");
}

/// A process resource dropped without `proc_close()` releases its pipes and reaps a child
/// that has already exited, like php-src's resource destructor, so no zombie is left.
#[test]
fn test_proc_open_dropped_process_is_reaped() {
    let out = compile_and_run(
        r#"<?php
function spawn_and_drop(): void {
    $process = proc_open(['true'], [1 => ['pipe', 'w']], $pipes);
    echo stream_get_contents($pipes[1]);
    usleep(200000);
}
for ($i = 0; $i < 3; $i++) {
    spawn_and_drop();
}
echo trim(shell_exec('ps -o stat= -o ppid= -A | awk -v parent=' . getmypid() . ' \'$2 == parent && $1 ~ /Z/\' | wc -l'));
"#,
    );
    assert_eq!(out, "0");
}
