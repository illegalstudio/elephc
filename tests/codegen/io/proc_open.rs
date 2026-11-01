//! Purpose:
//! End-to-end tests for the `proc_open()` family (`proc_open`, `proc_close`, `proc_get_status`,
//! `proc_terminate`, `proc_nice`) and `exec()`'s by-reference output and status parameters.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value was measured on `php -n` 8.5.10, including the `proc_get_status()` key
//!   order and its caching rules (a normal exit is cached, a signal death is reported once).
//! - The family is an AST prelude over the `elephc_proc` bridge, so these tests cover both
//!   supported architectures through the same code path; the heap-debug fixtures assert that the
//!   pipe streams and the process handle leave no live block behind.
//! - Fixtures avoid printing pids and rely on `sh`, `cat`, `printf`, and `sleep`, which every
//!   supported host ships.

use super::*;

/// Asserts a heap-debug run succeeded with the expected stdout and no leaked block.
fn assert_clean(out: &ProgramOutput, expected: &str) {
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Pipes in both directions: the parent writes `cat`'s stdin and reads its stdout and stderr.
#[test]
fn test_proc_open_pipes_in_both_directions() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$spec = [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => ['pipe', 'w']];
$p = proc_open('cat', $spec, $pipes);
var_dump(is_resource($p), count($pipes), array_keys($pipes));
fwrite($pipes[0], "hello\nworld\n");
fclose($pipes[0]);
echo fgets($pipes[1]);
echo stream_get_contents($pipes[1]);
var_dump(stream_get_contents($pipes[2]));
fclose($pipes[1]);
fclose($pipes[2]);
var_dump(proc_close($p));
"#,
    );
    assert_clean(
        &out,
        "bool(true)\nint(3)\narray(3) {\n  [0]=>\n  int(0)\n  [1]=>\n  int(1)\n  [2]=>\n  int(2)\n}\nhello\nworld\nstring(0) \"\"\nint(0)\n",
    );
}

/// Only the requested pipes exist, an extra descriptor number works, and no spec means no pipes.
#[test]
fn test_proc_open_descriptor_numbers_and_empty_spec() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$p = proc_open('echo oops >&2; exit 5', [2 => ['pipe', 'w']], $pipes);
var_dump(array_keys($pipes), stream_get_contents($pipes[2]));
fclose($pipes[2]);
var_dump(proc_close($p));
$p = proc_open('true', [], $pipes);
var_dump($pipes, proc_close($p));
$p = proc_open('echo three >&3', [3 => ['pipe', 'w']], $pipes);
var_dump(array_keys($pipes), stream_get_contents($pipes[3]), proc_close($p));
"#,
    );
    assert_clean(
        &out,
        "array(1) {\n  [0]=>\n  int(2)\n}\nstring(5) \"oops\n\"\nint(5)\narray(0) {\n}\nint(0)\narray(1) {\n  [0]=>\n  int(3)\n}\nstring(6) \"three\n\"\nint(0)\n",
    );
}

/// `proc_close()` closes pipes the caller left open, so `cat` sees EOF and exits.
#[test]
fn test_proc_close_closes_pipes_left_open() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$p = proc_open('cat', [0 => ['pipe', 'r'], 1 => ['pipe', 'w']], $pipes);
fwrite($pipes[0], "x");
var_dump(proc_close($p));
"#,
    );
    assert_clean(&out, "int(1)\n");
}

/// File descriptors, `null`, `redirect`, and an existing stream resource as descriptor specs.
#[test]
fn test_proc_open_file_null_redirect_and_stream_descriptors() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents('in.txt', "line1\nline2\n");
$p = proc_open('cat; echo e1 >&2', [
    0 => ['file', 'in.txt', 'r'],
    1 => ['file', 'out.txt', 'w'],
    2 => ['file', 'err.txt', 'a'],
], $pipes);
var_dump($pipes, proc_close($p));
var_dump(file_get_contents('out.txt'), file_get_contents('err.txt'));
$p = proc_open('echo e2 >&2', [2 => ['file', 'err.txt', 'a']], $pipes);
proc_close($p);
var_dump(file_get_contents('err.txt'));
$p = proc_open('echo gone; echo gone2 >&2', [1 => ['null'], 2 => ['null']], $pipes);
var_dump(proc_close($p));
$p = proc_open('echo a; echo b >&2', [1 => ['pipe', 'w'], 2 => ['redirect', 1]], $pipes);
var_dump(array_keys($pipes), stream_get_contents($pipes[1]));
fclose($pipes[1]);
var_dump(proc_close($p));
$h = fopen('out.txt', 'w');
$p = proc_open('echo through-stream', [1 => $h], $pipes);
var_dump(proc_close($p));
fclose($h);
var_dump(file_get_contents('out.txt'));
"#,
    );
    assert_eq!(
        out,
        "array(0) {\n}\nint(0)\nstring(12) \"line1\nline2\n\"\nstring(3) \"e1\n\"\nstring(6) \"e1\ne2\n\"\nint(0)\narray(1) {\n  [0]=>\n  int(1)\n}\nstring(4) \"a\nb\n\"\nint(0)\nint(0)\nstring(15) \"through-stream\n\"\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The array command form runs without a shell; the string form goes through `sh -c`.
#[test]
fn test_proc_open_array_command_cwd_and_environment() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function capture(array|string $cmd, ?string $cwd = null, ?array $env = null): string {
    $p = proc_open($cmd, [1 => ['pipe', 'w']], $pipes, $cwd, $env);
    $text = stream_get_contents($pipes[1]);
    fclose($pipes[1]);
    proc_close($p);
    return $text;
}
var_dump(capture(['echo', 'a b', '$HOME', '*', ';x']));
var_dump(capture(['printf', '%s|%s', 'x', 'y']));
var_dump(capture('echo "$((1+2))" && echo two'));
var_dump(capture('pwd', '/'));
var_dump(capture('echo "$FOO|$BAR|${HOME-unset}"', null, ['FOO' => 'foo', 'BAR' => 7, 0 => 'ignored']));
var_dump(capture('echo "${FOO-unset}"', null, []));
putenv('AGP_PROC_ENV=inherited');
var_dump(capture('echo "$AGP_PROC_ENV"'));
"#,
    );
    assert_clean(
        &out,
        "string(15) \"a b $HOME * ;x\n\"\nstring(3) \"x|y\"\nstring(6) \"3\ntwo\n\"\nstring(2) \"/\n\"\nstring(12) \"foo|7|unset\n\"\nstring(6) \"unset\n\"\nstring(10) \"inherited\n\"\n",
    );
}

/// Failures: a missing program or directory returns false, bad specs throw `ValueError`.
#[test]
fn test_proc_open_failures_and_validation() {
    let out = compile_and_run(
        r#"<?php
var_dump(@proc_open(['/nonexistent/binary'], [], $pipes));
var_dump(@proc_open('echo x', [], $pipes, '/nonexistent/dir'));
function attempt(callable $run): void {
    try {
        var_dump($run());
    } catch (Throwable $e) {
        echo get_class($e), ': ', $e->getMessage(), "\n";
    }
}
attempt(fn() => proc_open([], [], $p));
attempt(fn() => proc_open('true', [1 => 'bad'], $p));
attempt(fn() => proc_open('true', [0 => []], $p));
attempt(fn() => proc_open('true', [0 => ['pipe']], $p));
attempt(fn() => proc_open('true', [0 => ['file']], $p));
attempt(fn() => proc_open('true', [0 => ['file', 'x']], $p));
attempt(fn() => proc_open('true', ['a' => ['pipe', 'r']], $p));
attempt(fn() => proc_open('true', [0 => ['redirect']], $p));
attempt(fn() => proc_open(['tr', "u\0e"], [], $p));
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(false)\nValueError: proc_open(): Argument #1 ($command) must not be empty\nValueError: proc_open(): Argument #2 ($descriptor_spec) must only contain arrays and streams\nValueError: Missing handle qualifier\nValueError: Missing mode parameter for 'pipe'\nValueError: Missing file name parameter for 'file'\nValueError: Missing mode parameter for 'file'\nValueError: proc_open(): Argument #2 ($descriptor_spec) must be an integer indexed array\nValueError: Missing redirection target\nValueError: Command array element 2 contains a null byte\n"
    );
}

/// `proc_get_status()` before and after exit, with php 8.5's key order and caching.
#[test]
fn test_proc_get_status_before_and_after_exit() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function show(array $s): void {
    unset($s['pid']);
    echo json_encode($s), "\n";
}
$p = proc_open('read x; exit 3', [0 => ['pipe', 'r']], $pipes);
$s = proc_get_status($p);
echo implode(',', array_keys($s)), "\n";
var_dump(is_int($s['pid']) && $s['pid'] > 0);
show($s);
fwrite($pipes[0], "go\n");
fclose($pipes[0]);
usleep(300000);
show(proc_get_status($p));
show(proc_get_status($p));
var_dump(proc_close($p));
"#,
    );
    assert_clean(
        &out,
        concat!(
            "command,pid,cached,running,signaled,stopped,exitcode,termsig,stopsig\n",
            "bool(true)\n",
            "{\"command\":\"read x; exit 3\",\"cached\":false,\"running\":true,\"signaled\":false,\"stopped\":false,\"exitcode\":-1,\"termsig\":0,\"stopsig\":0}\n",
            "{\"command\":\"read x; exit 3\",\"cached\":true,\"running\":false,\"signaled\":false,\"stopped\":false,\"exitcode\":3,\"termsig\":0,\"stopsig\":0}\n",
            "{\"command\":\"read x; exit 3\",\"cached\":true,\"running\":false,\"signaled\":false,\"stopped\":false,\"exitcode\":3,\"termsig\":0,\"stopsig\":0}\n",
            "int(3)\n"
        ),
    );
}

/// A signal death is reported once by `proc_get_status()`; `proc_close()` then answers -1.
#[test]
fn test_proc_terminate_and_signal_status() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function show(array $s): void {
    unset($s['pid']);
    echo json_encode($s), "\n";
}
$p = proc_open('sleep 30', [], $pipes);
show(proc_get_status($p));
var_dump(proc_terminate($p));
usleep(300000);
show(proc_get_status($p));
show(proc_get_status($p));
var_dump(proc_close($p));
// Closing without polling reports the raw wait status of a signal death.
$p = proc_open('sleep 30', [], $pipes);
var_dump(proc_terminate($p, 9));
usleep(300000);
var_dump(proc_close($p));
$p = proc_open('sleep 30', [], $pipes);
var_dump(proc_terminate($p));
usleep(300000);
var_dump(proc_close($p));
"#,
    );
    assert_clean(
        &out,
        concat!(
            "{\"command\":\"sleep 30\",\"cached\":false,\"running\":true,\"signaled\":false,\"stopped\":false,\"exitcode\":-1,\"termsig\":0,\"stopsig\":0}\n",
            "bool(true)\n",
            "{\"command\":\"sleep 30\",\"cached\":false,\"running\":false,\"signaled\":true,\"stopped\":false,\"exitcode\":-1,\"termsig\":15,\"stopsig\":0}\n",
            "{\"command\":\"sleep 30\",\"cached\":false,\"running\":false,\"signaled\":false,\"stopped\":false,\"exitcode\":-1,\"termsig\":0,\"stopsig\":0}\n",
            "int(-1)\n",
            "bool(true)\nint(9)\nbool(true)\nint(15)\n"
        ),
    );
}

/// Exit codes round-trip through `proc_close()`, and a closed or bad handle throws `TypeError`.
#[test]
fn test_proc_close_exit_codes_and_handle_errors() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
foreach ([0, 1, 7, 255] as $code) {
    $p = proc_open("exit $code", [], $pipes);
    echo proc_close($p), ",";
}
echo "\n";
$p = proc_open('true', [], $pipes);
proc_close($p);
foreach ([$p, 'nope'] as $bad) {
    try { proc_close($bad); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { proc_get_status($bad); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
    try { proc_terminate($bad); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
var_dump(proc_nice(0));
"#,
    );
    assert_clean(
        &out,
        concat!(
            "0,1,7,255,\n",
            "proc_close(): supplied resource is not a valid process resource\n",
            "proc_get_status(): supplied resource is not a valid process resource\n",
            "proc_terminate(): supplied resource is not a valid process resource\n",
            "proc_close(): Argument #1 ($process) must be of type resource, string given\n",
            "proc_get_status(): Argument #1 ($process) must be of type resource, string given\n",
            "proc_terminate(): Argument #1 ($process) must be of type resource, string given\n",
            "bool(true)\n"
        ),
    );
}

/// `stream_select()` and `stream_set_blocking()` work on the pipe streams.
#[test]
fn test_proc_open_pipes_work_with_stream_select_and_blocking_mode() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$p = proc_open('sleep 0.2; echo late; sleep 0.2', [1 => ['pipe', 'w']], $pipes);
stream_set_blocking($pipes[1], false);
$early = fread($pipes[1], 100);
var_dump($early === '' || $early === false);
$read = [$pipes[1]];
$write = [];
$except = [];
var_dump(stream_select($read, $write, $except, 5), count($read));
var_dump(fread($pipes[1], 100));
fclose($pipes[1]);
var_dump(proc_close($p));
$p = proc_open('while read l; do echo "got $l"; done', [0 => ['pipe', 'r'], 1 => ['pipe', 'w']], $pipes);
foreach (['one', 'two', 'three'] as $word) {
    fwrite($pipes[0], "$word\n");
    fflush($pipes[0]);
    echo fgets($pipes[1]);
}
fclose($pipes[0]);
var_dump(fgets($pipes[1]));
fclose($pipes[1]);
var_dump(proc_close($p));
"#,
    );
    assert_clean(
        &out,
        "bool(true)\nint(1)\nint(1)\nstring(5) \"late\n\"\nint(0)\ngot one\ngot two\ngot three\nbool(false)\nint(0)\n",
    );
}

/// An untyped property passed as `$pipes` is retyped, so `array_search()` over it type-checks.
#[test]
fn test_proc_open_untyped_property_receives_the_pipes_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Proc {
    protected $_pipes = null;
    protected $_process = null;
    public function run(): string {
        $this->_process = proc_open('echo hi; echo there >&2', [1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $this->_pipes);
        $out = '';
        foreach ($this->_pipes as $i => $pipe) {
            $where = array_search($pipe, $this->_pipes);
            $out .= $where . ':' . trim(stream_get_contents($pipe)) . ' ';
            fclose($pipe);
        }
        return $out . proc_close($this->_process);
    }
    public function count(): int {
        return count($this->_pipes);
    }
}
$p = new Proc;
echo $p->run(), "|", $p->count(), "\n";
"#,
    );
    assert_clean(&out, "1:hi 2:there 0|2\n");
}

/// Repeated open/close cycles leave no live block: pipes, handle, and static table all release.
#[test]
fn test_proc_open_cycles_do_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$total = 0;
for ($i = 0; $i < 5; $i++) {
    $p = proc_open('cat', [0 => ['pipe', 'r'], 1 => ['pipe', 'w']], $pipes);
    fwrite($pipes[0], "round $i\n");
    fclose($pipes[0]);
    $total += strlen(stream_get_contents($pipes[1]));
    fclose($pipes[1]);
    $total += proc_close($p);
    $status = null;
}
echo $total, "\n";
"#,
    );
    assert_clean(&out, "40\n");
}

/// `exec()` appends trimmed lines to `$output` and stores the exit status in `$result_code`.
#[test]
fn test_exec_output_and_result_code() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
var_dump(exec('printf "a\nb  \nc  "', $o, $rc), $o, $rc);
var_dump(exec('exit 4', $o2, $rc2), $o2, $rc2);
var_dump(exec('echo hi'));
var_dump(exec('printf ""', $o3), $o3);
var_dump(exec("printf 'tab\t\n\n\nlast \t \n'", $o4), $o4);
$arr = ['pre'];
var_dump(exec('echo more', $arr), $arr);
$code = 99;
var_dump(exec('exit 2', $out6, $code), $code);
var_dump(exec('printf "a\r\nb\r\n"', $o8), $o8);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "string(1) \"c\"\narray(3) {\n  [0]=>\n  string(1) \"a\"\n  [1]=>\n  string(1) \"b\"\n  [2]=>\n  string(1) \"c\"\n}\nint(0)\n",
            "string(0) \"\"\narray(0) {\n}\nint(4)\n",
            "string(2) \"hi\"\n",
            "string(0) \"\"\narray(0) {\n}\n",
            "string(4) \"last\"\narray(4) {\n  [0]=>\n  string(3) \"tab\"\n  [1]=>\n  string(0) \"\"\n  [2]=>\n  string(0) \"\"\n  [3]=>\n  string(4) \"last\"\n}\n",
            "string(4) \"more\"\narray(2) {\n  [0]=>\n  string(3) \"pre\"\n  [1]=>\n  string(4) \"more\"\n}\n",
            "string(0) \"\"\nint(2)\n",
            "string(1) \"b\"\narray(2) {\n  [0]=>\n  string(1) \"a\"\n  [1]=>\n  string(1) \"b\"\n}\n",
        ),
    );
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// `exec()` writes through function parameters and typed object properties.
#[test]
fn test_exec_output_through_function_and_property_targets() {
    let out = compile_and_run(
        r#"<?php
function run(string $cmd): array {
    exec($cmd, $lines, $status);
    return [$status, $lines];
}
echo json_encode(run('printf "1\n2\n"; exit 1')), "\n";
class Runner {
    public array $lines = [];
    public int $status = -1;
    public function go(string $cmd): int {
        exec($cmd, $this->lines, $this->status);
        return $this->status;
    }
}
$r = new Runner;
echo $r->go('printf "p\nq\n"; exit 6'), json_encode($r->lines), "\n";
"#,
    );
    assert_eq!(out, "[1,[\"1\",\"2\"]]\n6[\"p\",\"q\"]\n");
}

/// `stream_select()` takes `null` write/except sets and a hash read set (what Symfony Process's
/// `UnixPipes::readAndWrite()` passes after `unset($r[0])`), and keeps each ready entry's key.
#[test]
fn test_stream_select_accepts_null_sets_and_hash_with_original_keys() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$proc = proc_open(['sh', '-c', 'echo out; echo err 1>&2'], [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => ['pipe', 'w']], $pipes);
usleep(200000);
// Symfony shape: keys 1 and 2 after the unset, null write set, empty except set.
$r = $pipes;
unset($r[0]);
$w = null;
$e = [];
var_dump(($r || $w) && false === stream_select($r, $w, $e, 0, 0));
echo json_encode(array_keys($r)), " ", json_encode($w), " ", json_encode($e), "\n";
foreach ($r as $pipe) {
    echo "type ", array_search($pipe, $pipes, true), "\n";
}
// A single hash entry keeps its non-sequential key; null sets stay null.
$r = [5 => $pipes[1]];
$w = null;
$e = null;
var_dump(stream_select($r, $w, $e, 0, 0));
echo json_encode(array_keys($r)), " ", json_encode($w), " ", json_encode($e), "\n";
// A packed list with null write/except sets.
$q = [$pipes[1], $pipes[2]];
var_dump(stream_select($q, $w, $e, 0, 0), count($q));
// A null read set: only the write set is selected on.
$wr = [$pipes[0]];
$n = null;
var_dump(stream_select($n, $wr, $e, 0, 0));
echo json_encode(array_keys($wr)), " ", json_encode($n), "\n";
proc_close($proc);
"#,
    );
    assert_clean(
        &out,
        concat!(
            "bool(false)\n[1,2] null []\ntype 1\ntype 2\n",
            "int(1)\n[5] null null\n",
            "int(2)\nint(2)\n",
            "int(1)\n[0] null\n",
        ),
    );
}

/// Only the ready entries survive the select, under their original keys.
#[test]
fn test_stream_select_keeps_only_ready_entries_with_their_keys() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$p = proc_open('sleep 0.4', [1 => ['pipe', 'w']], $idle);
$q = proc_open('echo now', [1 => ['pipe', 'w']], $busy);
usleep(150000);
$r = ['slow' => $idle[1], 'fast' => $busy[1]];
$w = null;
$e = null;
var_dump(stream_select($r, $w, $e, 0, 0));
echo json_encode(array_keys($r)), "\n";
fclose($idle[1]);
fclose($busy[1]);
proc_close($p);
proc_close($q);
"#,
    );
    assert_clean(&out, "int(1)\n[\"fast\"]\n");
}
