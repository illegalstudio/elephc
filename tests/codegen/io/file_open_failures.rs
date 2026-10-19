//! Purpose:
//! Regression tests for filesystem helpers whose underlying syscall fails.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - On Darwin a failed `svc` leaves a POSITIVE errno in x0 with the carry set; Linux leaves
//!   `-errno`. `__rt_file_put_contents` used that value as a descriptor, so a destination it
//!   could not open (ENAMETOOLONG, ENOENT, EACCES) wrote the payload to stderr or to whatever
//!   fd matched the errno, and answered the write's errno as a byte count. Each fixture pins
//!   PHP 8.5's value and that nothing leaks to another descriptor.
//! - Fixtures work under `sys_get_temp_dir()` with a pid-suffixed directory they remove.

use super::*;

/// ENAMETOOLONG: every writer returns false and nothing is written anywhere.
#[test]
fn test_file_put_contents_name_too_long_is_false_and_writes_nothing() {
    let out = compile_and_run_capture(
        r#"<?php
$d = sys_get_temp_dir() . "/p41_t_long_" . getmypid();
@mkdir($d);
$p = $d . "/" . str_repeat("a", 300);
var_dump(@file_put_contents($p, "LEAK\n"));
var_dump(@file_put_contents($p, "LEAK\n", FILE_APPEND));
var_dump(@file_put_contents($p, "LEAK\n", LOCK_EX));
var_dump(@copy(__FILE__, $p));
var_dump(file_exists($p));
@rmdir($d);
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\n"
    );
    assert!(!out.stderr.contains("LEAK"), "payload leaked to stderr: {}", out.stderr);
}

/// ENOENT (missing parent directory): ENOENT is 2, the very descriptor of stderr, so the old
/// code wrote the payload straight to stderr. The failure now warns like PHP does.
#[test]
fn test_file_put_contents_missing_dir_warns_and_does_not_write_stderr() {
    let out = compile_and_run_capture(
        r#"<?php
$p = sys_get_temp_dir() . "/p41_t_missing_" . getmypid() . "/x.txt";
$r = file_put_contents($p, "LEAK\n");
var_dump($r);
var_dump($r === false);
var_dump(@file_put_contents($p, "LEAK\n"));
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(true)\nbool(false)\n");
    assert!(!out.stderr.contains("LEAK"), "payload leaked to stderr: {}", out.stderr);
    assert_eq!(
        out.stderr.matches("Warning: file_put_contents(): Failed to open stream").count(),
        1,
        "exactly the unsuppressed call warns: {}",
        out.stderr
    );
}

/// EACCES: a read-only file and a read-only directory refuse the write and keep their content.
#[test]
fn test_file_put_contents_without_write_permission_is_false() {
    let out = compile_and_run_capture(
        r#"<?php
$d = sys_get_temp_dir() . "/p41_t_ro_" . getmypid();
@mkdir($d);
$f = $d . "/ro.txt";
file_put_contents($f, "orig");
chmod($f, 0444);
var_dump(@file_put_contents($f, "LEAK"));
var_dump(@file_put_contents($f, "LEAK", FILE_APPEND));
var_dump(@copy(__FILE__, $f));
echo file_get_contents($f), "\n";
chmod($d, 0555);
var_dump(@file_put_contents($d . "/new.txt", "LEAK"));
chmod($d, 0755);
chmod($f, 0644);
unlink($f);
rmdir($d);
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(false)\nbool(false)\norig\nbool(false)\n"
    );
}

/// The successful result stays an int that arithmetic and `=== 0` see as before.
#[test]
fn test_file_put_contents_success_is_still_an_int() {
    let out = compile_and_run_capture(
        r#"<?php
$f = sys_get_temp_dir() . "/p41_t_ok_" . getmypid() . ".txt";
$n = file_put_contents($f, "abc");
var_dump($n);
echo $n + 1, "\n";
var_dump(file_put_contents($f, "") === 0);
var_dump(file_put_contents($f, "xy", FILE_APPEND));
var_dump(file_put_contents($f, "z", LOCK_EX));
unlink($f);
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "int(3)\n4\nbool(true)\nint(2)\nint(1)\n");
}

/// copy() from a missing source or a directory is false and creates no destination; an
/// empty source copies and is true. The warning names copy(), not its internal helpers.
#[test]
fn test_copy_failures_leave_no_destination_and_empty_copy_succeeds() {
    let out = compile_and_run_capture(
        r#"<?php
$d = sys_get_temp_dir() . "/p41_t_copy_" . getmypid();
@mkdir($d);
var_dump(copy($d . "/nope", $d . "/dst"));
var_dump(file_exists($d . "/dst"));
var_dump(@copy($d, $d . "/dst"));
var_dump(file_exists($d . "/dst"));
file_put_contents($d . "/empty", "");
var_dump(copy($d . "/empty", $d . "/empty2"));
var_dump(filesize($d . "/empty2"));
file_put_contents($d . "/src", "hola");
var_dump(copy($d . "/src", $d . "/dst2"));
echo file_get_contents($d . "/dst2"), "\n";
var_dump(@copy($d . "/src", $d . "/nope/dst3"));
foreach (["empty", "empty2", "src", "dst2"] as $n) { unlink($d . "/" . $n); }
rmdir($d);
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(false)\nbool(false)\nbool(false)\nbool(true)\nint(0)\nbool(true)\nhola\nbool(false)\n"
    );
    assert_eq!(out.stderr, "Warning: copy(): Failed to open stream\n");
}

/// file_get_contents() of a missing path is false; of a directory it is the empty string
/// (PHP's read fails after the open succeeds), never errno-many uninitialised heap bytes.
#[test]
fn test_file_get_contents_missing_is_false_and_directory_is_empty() {
    let out = compile_and_run_capture(
        r#"<?php
$d = sys_get_temp_dir() . "/p41_t_fgc_" . getmypid();
@mkdir($d);
var_dump(@file_get_contents($d . "/nope"));
var_dump(@file($d . "/nope"));
var_dump(@readfile($d . "/nope"));
var_dump(@file_get_contents($d));
rmdir($d);
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(false)\nbool(false)\nstring(0) \"\"\n"
    );
}

/// fwrite() on a read-only stream is false; Darwin's EBADF (9) used to read back as int(9).
#[test]
fn test_fwrite_on_read_only_stream_is_false() {
    let out = compile_and_run_capture(
        r#"<?php
$f = sys_get_temp_dir() . "/p41_t_fw_" . getmypid() . ".txt";
file_put_contents($f, "hello");
$h = fopen($f, "r");
var_dump(@fwrite($h, "abc"));
fclose($h);
echo file_get_contents($f), "\n";
unlink($f);
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nhello\n");
}

/// fwrite() on a full non-blocking socket is int(0), not false (php-src bug #79000): the
/// EAGAIN/EWOULDBLOCK failure is not an error.
#[test]
fn test_fwrite_would_block_on_non_blocking_socket_is_zero() {
    let out = compile_and_run_capture(
        r#"<?php
[$a, $b] = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, STREAM_IPPROTO_IP);
stream_set_blocking($a, false);
$s = str_repeat('a', 1000000);
$first = fwrite($a, $s);
var_dump(is_int($first) && $first > 0);
var_dump(fwrite($a, $s));
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nint(0)\n");
}

/// scandir() of a missing directory keeps its `array<string>` type (an `array|false` result
/// would stop `array_diff(scandir(...), ...)` compiling) but is no longer silent about it.
#[test]
fn test_scandir_missing_directory_warns() {
    let out = compile_and_run_capture(
        r#"<?php
$r = scandir(sys_get_temp_dir() . "/p41_t_sd_nope_" . getmypid());
echo count($r), "\n";
$r = @scandir(sys_get_temp_dir() . "/p41_t_sd_nope_" . getmypid());
echo count($r), "\n";
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "0\n0\n");
    assert_eq!(out.stderr, "Warning: scandir(): Failed to open directory\n");
}

/// A string callable dispatches through the builtin CONTRACT's
/// return type. The contract still said `int` while the lowering boxed `int|false`, so
/// `$cb = 'file_put_contents'; $cb(...)` printed the boxed cell's address as an integer.
#[test]
fn test_file_put_contents_through_dynamic_callables_returns_int_or_false() {
    let out = compile_and_run_capture(
        r#"<?php
$d = sys_get_temp_dir() . "/p41_t_dyn_" . getmypid();
@mkdir($d);
$p = $d . "/f.txt";
$cb = 'file_put_contents';
var_dump($cb($p, "1234"));
var_dump(@$cb($d . "/nope/x", "1234"));
var_dump(call_user_func('file_put_contents', $p, "12345"));
var_dump(@call_user_func('file_put_contents', $d . "/nope/x", "1"));
var_dump(call_user_func_array('file_put_contents', [$p, "123456"]));
var_dump(array_map(fn($x) => file_put_contents($p, $x), ["ab", "abc"]));
$f = file_put_contents(...);
var_dump($f($p, "1234567"));
$c = 'fputcsv';
$h = fopen($p, "w");
var_dump($c($h, ["a", "b"]));
fclose($h);
unlink($p);
rmdir($d);
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "int(4)\nbool(false)\nint(5)\nbool(false)\nint(6)\narray(2) {\n  [0]=>\n  int(2)\n  [1]=>\n  int(3)\n}\nint(7)\nint(4)\n"
    );
}

/// Fputcsv() on a read-only stream and hash_file() of a
/// directory are PHP `false`, not a negative sum of failed writes or md5("").
#[test]
fn test_fputcsv_read_only_and_hash_file_directory_are_false() {
    let out = compile_and_run_capture(
        r#"<?php
$d = sys_get_temp_dir() . "/p41_t_csv_" . getmypid();
@mkdir($d);
$p = $d . "/r.csv";
file_put_contents($p, "x");
$h = fopen($p, "r");
var_dump(@fputcsv($h, ["a", "b c", "d\"e"]));
fclose($h);
$w = fopen($p, "w");
var_dump(fputcsv($w, ["a", "b"]));
fclose($w);
var_dump(@hash_file("md5", $d));
var_dump(hash_file("md5", $p));
unlink($p);
rmdir($d);
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(false)\nint(4)\nbool(false)\nstring(32) \"f69f5b72bc79a92dc70c63c9aa142e36\"\n"
    );
}

/// The boxed `int|false` result is `Fresh`: storing, discarding, and dynamically calling
/// file_put_contents() in a loop must not leave live heap blocks behind.
#[test]
fn test_file_put_contents_boxed_result_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$p = sys_get_temp_dir() . "/p41_t_leak_" . getmypid() . ".txt";
$f = file_put_contents(...);
$t = 0;
for ($i = 0; $i < 50; $i++) {
    $t += file_put_contents($p, "abc");
    file_put_contents($p, "x");
    $t += $f($p, "a");
    $t += call_user_func('file_put_contents', $p, "abcd");
    if (@file_put_contents($p . "/nope", "z") === false) { $t++; }
}
echo $t, "\n";
unlink($p);
"#,
    );
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "450\n");
    assert!(
        out.stderr.contains("leak summary: clean"),
        "boxed file_put_contents() results leaked: {}",
        out.stderr
    );
}
