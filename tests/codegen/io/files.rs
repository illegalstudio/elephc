//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O files, including file put get contents, file get contents missing emits runtime warning, and file get contents missing is strict false.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies `file_put_contents` writes data and `file_get_contents` reads it back identically.
/// Fixture: creates `test.txt` with "hello world" via put, reads it back, asserts equality.
/// Cleans up the temp directory after the test.
#[test]
fn test_file_put_get_contents() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("test.txt", "hello world");
echo file_get_contents("test.txt");
"#,
    );
    assert_eq!(out, "hello world");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the disk-space family answers `false` for a path it cannot stat.
///
/// Both answered `float(0)`, which is a legitimate reading for a full filesystem — so
/// `disk_free_space($d) === false` never fired and arithmetic silently used zero.
///
/// The success half is the control, and it is the half a `float|false` change can break:
/// declaring the union changes how the value is carried, so the result still has to be a
/// float that adds, divides and compares.
#[test]
fn test_disk_space_reports_false_for_an_unstattable_path() {
    let out = compile_and_run(
        r#"<?php
echo var_export(@disk_free_space("/no/such/dir"), true), "|";
echo var_export(@disk_total_space("/no/such/dir"), true), "|";
$f = disk_free_space("/");
echo var_export(is_float($f), true), ",";
echo var_export($f > 0, true), ",";
echo var_export($f <= disk_total_space("/"), true);
"#,
    );
    assert_eq!(out, "false|false|true,true,true");
}

/// An append stream reports the position PHP maintains, not the descriptor's.
///
/// `O_APPEND` puts every write at the end of the file, so after writing one byte to a four-byte
/// file the descriptor is at 5 — but PHP answers 1, because it advances a position of its own by
/// the bytes written, wherever they land. elephc reported the descriptor's offset.
///
/// Every case here is a `php -n` witness. The `a+` read matters most: it is the one the fix could
/// have broken, since a read moves the descriptor and PHP's position by the same amount and must
/// therefore be left alone. The seek matters next: it puts the two back in agreement, and without
/// clearing the running total the following write answers a negative number.
#[test]
fn test_append_stream_reports_phps_position_not_the_descriptors() {
    let out = compile_and_run(
        r#"<?php
$p = sys_get_temp_dir() . "/elephc_append_tell.txt";
@unlink($p);
file_put_contents($p, "seed");
$h = fopen($p, "a");
echo ftell($h), ",";
fwrite($h, "X");
echo ftell($h), ",";
fwrite($h, "YZ");
echo ftell($h), ",";
fseek($h, 0);
echo ftell($h), ",";
fwrite($h, "Q");
echo ftell($h), "|";
fclose($h);
echo file_get_contents($p), "|";

@unlink($p);
file_put_contents($p, "seed");
$g = fopen($p, "a+");
echo ftell($g), ",";
fread($g, 2);
echo ftell($g), ",";
fwrite($g, "X");
echo ftell($g), "|";
fclose($g);

@unlink($p);
$w = fopen($p, "w");
fwrite($w, "abc");
echo ftell($w);
fclose($w);
@unlink($p);
"#,
    );
    assert_eq!(out, "0,1,3,0,1|seedXYZQ|0,2,3|3");
}

/// A disk-space failure names itself and the reason, as php does.
///
/// Answering `false` was only half of it: php also prints `disk_free_space(): No such file or
/// directory`, so a script that watched the warning to notice a bad path saw a silent `false`.
/// php names NEITHER the path here nor a fixed middle, which is why this does not go through the
/// failed-open composer.
///
/// The `@` half is the control. A diagnostic that ignores suppression is as wrong as a missing
/// one, and it is the half that a hand-written warning path gets wrong.
#[test]
fn test_disk_space_failure_names_itself_and_the_reason() {
    let out = compile_and_run_capture(
        r#"<?php
echo var_export(disk_free_space("/no/such/dir"), true), "|";
echo var_export(disk_total_space("/no/such/dir"), true), "|";
echo var_export(@disk_free_space("/no/such/dir"), true), "|";
echo var_export(is_float(disk_free_space("/")), true);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "false|false|false|true");
    assert!(
        out.diagnostics
            .contains("Warning: disk_free_space(): No such file or directory\n"),
        "expected php's wording, got diagnostics={}",
        out.diagnostics
    );
    assert!(
        out.diagnostics
            .contains("Warning: disk_total_space(): No such file or directory\n"),
        "expected the total-space wording too, got diagnostics={}",
        out.diagnostics
    );
    // Three calls fail, one of them suppressed: exactly two lines may be printed.
    assert_eq!(
        out.diagnostics.matches("No such file or directory").count(),
        2,
        "@ must suppress the third, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies `sys_get_temp_dir()` derives its answer from `TMPDIR`, as php does.
///
/// It used to answer a hardcoded `/tmp`. On macOS php hands out a private per-user directory,
/// so a program falling back to the shared `/tmp` changed behaviour and not merely its output.
///
/// The assertion is a RELATIONSHIP rather than a literal, because the right answer depends on
/// the machine: whatever `TMPDIR` holds, minus exactly one trailing slash — php removes one,
/// not all, which is why `/tmp///` must not collapse to `/tmp`. With `TMPDIR` unset the test
/// falls back to checking the answer is a usable directory, since the constant differs
/// between macOS (`/var/tmp/`) and Linux (`/tmp`).
#[test]
fn test_sys_get_temp_dir_follows_tmpdir() {
    let out = compile_and_run(
        r#"<?php
$env = getenv("TMPDIR");
$tmp = sys_get_temp_dir();
if ($env === false || $env === "") {
    echo var_export(is_dir($tmp), true);
} else {
    // Copy every byte but a single trailing slash. Deliberately NOT substr($env, 0, -1):
    // a negative substr length is itself wrong on this branch, so using it here would make
    // this test measure that defect instead of this one.
    $keep = strlen($env);
    if ($env[$keep - 1] === "/") {
        $keep--;
    }
    $expected = "";
    for ($i = 0; $i < $keep; $i++) {
        $expected .= $env[$i];
    }
    echo var_export($tmp === $expected, true);
}
echo "|", var_export(is_dir($tmp), true);
"#,
    );
    assert_eq!(out, "true|true");
}

/// Verifies `file_get_contents()` reads a literal `data://` URI.
///
/// `fopen("data://…")` already decoded these at compile time, but `file_get_contents()` went
/// through the filesystem helper and answered `false` with `No such file or directory` —
/// naming a "path" that was never meant to be one. The `$offset`/`$length` window applies to
/// the decoded bytes, and a malformed URI still answers `false`, both as php does.
#[test]
fn test_file_get_contents_reads_a_literal_data_uri() {
    let out = compile_and_run(
        r#"<?php
echo var_export(file_get_contents("data://text/plain,hello"), true), "|";
echo var_export(file_get_contents("data://text/plain;base64,aGVsbG8="), true), "|";
echo var_export(file_get_contents("data://text/plain,a%20b"), true), "|";
echo var_export(@file_get_contents("data://bogus"), true), "|";
echo var_export(file_get_contents("data://text/plain,offset", false, null, 2, 3), true);
"#,
    );
    assert_eq!(out, "'hello'|'hello'|'a b'|false|'fse'");
}

/// Verifies `filesize()` and `filemtime()` answer `false` for a path they cannot stat.
///
/// Seven of the nine stat readers already did; these two were left behind. `filesize()`
/// answered `0` — a legitimate size for an empty file, so `filesize($f) === false` never
/// fired and arithmetic silently used zero. `filemtime()` was worse: the AArch64 helper read
/// the stat buffer WITHOUT checking whether the syscall had filled it, so a missing path
/// returned uninitialised stack — a different large integer each run.
///
/// The success half is the control: both must still behave as plain ints, since declaring
/// them `int|false` changes how the value is carried.
#[test]
fn test_filesize_and_filemtime_report_false_for_an_unstattable_path() {
    let out = compile_and_run(
        r#"<?php
echo var_export(@filesize("/no/such/file"), true), "|";
echo var_export(@filemtime("/no/such/file"), true), "|";

$p = tempnam(sys_get_temp_dir(), "sz");
file_put_contents($p, "0123456789");
$s = filesize($p);
echo $s, ",", $s + 1, ",", var_export(is_int($s), true), "|";
echo var_export(filemtime($p) > 0, true);
unlink($p);
"#,
    );
    assert_eq!(out, "false|false|10,11,true|true");
}

/// Verifies `FILE_APPEND` extends the file instead of replacing it.
///
/// The flag was accepted by the arity check and then discarded, so the one call whose entire
/// purpose is to EXTEND a file truncated it — and still returned the byte count, so a caller
/// checking the result saw a success while the previous contents were gone. Nothing covered
/// FILE_APPEND on a file that already had content, which is the only way to see it.
///
/// The second write is the control: WITHOUT the flag the call must still truncate, so this
/// cannot pass by making every write append.
#[test]
fn test_file_put_contents_append_extends_the_file() {
    let out = compile_and_run(
        r#"<?php
$p = tempnam(sys_get_temp_dir(), "ap");
file_put_contents($p, "xy");
$n = file_put_contents($p, "z", FILE_APPEND);
echo $n, ":", file_get_contents($p), "|";
file_put_contents($p, "w");
echo file_get_contents($p);
unlink($p);
"#,
    );
    assert_eq!(out, "1:xyz|w");
}

/// Verifies `file_get_contents` on a missing file emits a runtime warning to stderr and continues execution.
/// Fixture: tries to read "missing.txt" which does not exist.
/// Asserts: program exits successfully, stdout is "after" (execution continued), stderr contains the PHP warning.
/// This is a regression check for missing-file handling to ensure no fatal error is raised.
#[test]
fn test_file_get_contents_missing_emits_runtime_warning() {
    let out = compile_and_run_capture(
        r#"<?php
echo file_get_contents("missing.txt");
echo "after";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "after");
    // php-src puts the PATH inside the parentheses and the reason after it; the bare
    // `file_get_contents()` this used to assert named neither.
    assert!(
        out.diagnostics.contains(
            "Warning: file_get_contents(missing.txt): Failed to open stream: No such file or directory"
        ),
        "expected the path and reason in the warning, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies `file_get_contents` on a missing file returns strict `false` (not a falsy value).
/// Fixture: reads "missing.txt" with error suppression (`@`), stores result, compares with `=== false`.
/// Asserts: stdout is "false" (identity check passes), stderr is empty.
/// Covers the PHP semantics where missing file returns `false` not `""` or `0`.
#[test]
fn test_file_get_contents_missing_is_strict_false() {
    let out = compile_and_run_capture(
        r#"<?php
$value = @file_get_contents("missing.txt");
echo $value === false ? "false" : "string";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "false");
    assert_eq!(out.stderr, "");
    assert_eq!(out.diagnostics, "");
}

/// Verifies `file_get_contents` on an existing file returns a truthy value, not `false`.
/// Fixture: creates `test.txt` with empty string via `file_put_contents`, then reads it back.
/// Asserts: identity comparison `$value === false` is false, confirming a string (not false) is returned.
/// Regression check: success path must not incorrectly return `false`.
#[test]
fn test_file_get_contents_success_is_not_false() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("test.txt", "");
$value = file_get_contents("test.txt");
echo $value === false ? "false" : "string";
"#,
    );
    assert_eq!(out, "string");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `file_exists` returns true for existing files and false for non-existent files.
/// Fixture: creates "exists.txt" with data, checks it; checks "nope.txt" which does not exist.
/// Asserts: "exists.txt" → yes, "nope.txt" → no, combined output is "yesno".
/// Cleans up the temp directory after the test.
#[test]
fn test_file_exists() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("exists.txt", "data");
if (file_exists("exists.txt")) {
    echo "yes";
}
if (!file_exists("nope.txt")) {
    echo "no";
}
"#,
    );
    assert_eq!(out, "yesno");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `filesize` returns the byte length of a file's content.
/// Fixture: creates "size.txt" containing "12345" (5 bytes).
/// Asserts: `filesize("size.txt")` equals 5.
/// Cleans up the temp directory after the test.
#[test]
fn test_filesize() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("size.txt", "12345");
echo filesize("size.txt");
"#,
    );
    assert_eq!(out, "5");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `is_file` and `is_dir` return correct booleans for files and directories.
/// Fixture: creates "afile.txt" and "adir" directory; checks both with is_file/is_dir and their negations.
/// Asserts: is_file("afile.txt")=true, is_dir("afile.txt")=false, is_dir("adir")=true, is_file("adir")=false.
/// Output sequence: "F!DD!F" (file→F, not dir→!D, dir→D, not file→!F).
/// Cleans up the directory (rmdir "adir") after the test.
#[test]
fn test_is_file_is_dir() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("afile.txt", "x");
mkdir("adir");
if (is_file("afile.txt")) { echo "F"; }
if (!is_dir("afile.txt")) { echo "!D"; }
if (is_dir("adir")) { echo "D"; }
if (!is_file("adir")) { echo "!F"; }
rmdir("adir");
"#,
    );
    assert_eq!(out, "F!DD!F");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `file()` reads a file and returns an array of lines (without newlines).
/// Fixture: creates "lines.txt" with "one\ntwo\nthree\n" (3 lines + trailing newline).
/// Asserts: `count($lines)` equals 3. Uses `unlink` to remove the file, then cleans up the temp dir.
#[test]
fn test_file_lines() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("lines.txt", "one\ntwo\nthree\n");
$lines = file("lines.txt");
echo count($lines);
unlink("lines.txt");
"#,
    );
    assert_eq!(out, "3");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `is_readable` and `is_writable` return true for a file the process can access.
/// Fixture: creates "perm.txt" with content, checks both predicates, then deletes it.
/// Asserts: "R" (readable) and "W" (writable) are both printed.
/// Platform assumption: current user has read/write permissions on the temp file.
/// Cleans up after the test by deleting the file.
#[test]
fn test_is_readable_writable() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("perm.txt", "x");
if (is_readable("perm.txt")) { echo "R"; }
if (is_writable("perm.txt")) { echo "W"; }
unlink("perm.txt");
"#,
    );
    assert_eq!(out, "RW");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `filemtime` returns a Unix timestamp greater than 1 billion for a recently created file.
/// Fixture: creates "ts.txt" with content, reads its modification time, asserts it is > 1,000,000,000.
/// Asserts: output is "ok". Uses `unlink` to remove the file, then cleans up the temp directory.
/// Regression check: filemtime must not return -1 or an invalid value for a freshly created file.
#[test]
fn test_filemtime() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ts.txt", "x");
$t = filemtime("ts.txt");
if ($t > 1000000000) { echo "ok"; }
unlink("ts.txt");
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `file()` answers `false` for a read it could not perform, and an EMPTY ARRAY for a
/// file that is genuinely empty.
///
/// Both used to be the same empty array, so no caller could tell them apart — the shape PHP
/// gives a `false` return exists precisely to separate them. The empty-file half is the one
/// that constrains the implementation: the failure signal has to be the payload pointer from
/// `__rt_file_get_contents`, because an empty file and a missing one both produce zero LINES.
///
/// The `count()` calls are load-bearing. Giving `file()` its union return type is what made
/// this conversion fail twice before: `count($lines)` stopped compiling, since `count()`
/// refused a union unless every member was countable. That rule was standing in for a missing
/// run-time `TypeError`, which now exists, so the ordinary shape compiles again.
#[test]
fn test_file_reports_a_failed_read_as_false_and_an_empty_file_as_an_empty_array() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("two.txt", "a\nb\n");
file_put_contents("none.txt", "");
$lines = file("two.txt");
$empty = file("none.txt");
$absent = @file("absent.txt");
var_dump($lines === false);
echo count($lines), "|";
var_dump($empty === false);
echo count($empty), "|";
var_dump($absent === false);
"#,
    );
    assert_eq!(
        out,
        "bool(false)\n2|bool(false)\n0|bool(true)\n",
        "an empty file is an empty array; only a failed read is false"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `file()`'s `$flags` bitmask over every combination PHP distinguishes.
///
/// The fixture writes a file with two empty lines so `FILE_IGNORE_NEW_LINES` and
/// `FILE_SKIP_EMPTY_LINES` are separable: PHP applies the newline trimming FIRST, so
/// `FILE_SKIP_EMPTY_LINES` alone drops nothing (a bare `"\n"` line still has length 1). Each line
/// is reported as `index/strlen/trimmed-content` so the trailing-terminator handling is visible.
/// `FILE_USE_INCLUDE_PATH` is accepted and has no effect, matching PHP's default empty
/// `include_path`. The expected values are verbatim `LC_ALL=C php` output from PHP 8.4.20.
#[test]
fn test_file_flags_combinations() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("f1.txt", "alpha\n\nbeta\n\ngamma");
function dump($label, $lines) { echo $label, ":", count($lines); foreach ($lines as $i => $l) { echo " ", $i, "/", strlen($l), "/", rtrim($l, "\r\n"); } echo "|"; }
dump("plain", file("f1.txt"));
dump("ignore", file("f1.txt", FILE_IGNORE_NEW_LINES));
dump("skip", file("f1.txt", FILE_SKIP_EMPTY_LINES));
dump("both", file("f1.txt", FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES));
dump("incpath", file("f1.txt", FILE_USE_INCLUDE_PATH));
unlink("f1.txt");
"#,
    );
    assert_eq!(
        out,
        "plain:5 0/6/alpha 1/1/ 2/5/beta 3/1/ 4/5/gamma|\
ignore:5 0/5/alpha 1/0/ 2/4/beta 3/0/ 4/5/gamma|\
skip:5 0/6/alpha 1/1/ 2/5/beta 3/1/ 4/5/gamma|\
both:3 0/5/alpha 1/4/beta 2/5/gamma|\
incpath:5 0/6/alpha 1/1/ 2/5/beta 3/1/ 4/5/gamma|"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `file()` accepts its `$flags` as a named argument and as a run-time value.
///
/// The flag is a plain bitmask rather than a shape-changing literal, so a variable must work.
#[test]
fn test_file_flags_named_and_runtime() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("f1.txt", "alpha\n\nbeta\n\ngamma");
function dump($label, $lines) { echo $label, ":", count($lines); foreach ($lines as $i => $l) { echo " ", $i, "/", strlen($l), "/", rtrim($l, "\r\n"); } echo "|"; }
dump("named", file(filename: "f1.txt", flags: FILE_IGNORE_NEW_LINES));
$f = FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES;
dump("runtime", file("f1.txt", $f));
unlink("f1.txt");
"#,
    );
    assert_eq!(
        out,
        "named:5 0/5/alpha 1/0/ 2/4/beta 3/0/ 4/5/gamma|runtime:3 0/5/alpha 1/4/beta 2/5/gamma|"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `FILE_IGNORE_NEW_LINES` removes a CRLF pair, not just the line feed.
#[test]
fn test_file_flags_strip_crlf() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("f2.txt", "a\r\nb\r\n\r\nc");
function dump($label, $lines) { echo $label, ":", count($lines); foreach ($lines as $i => $l) { echo " ", $i, "/", strlen($l), "/", rtrim($l, "\r\n"); } echo "|"; }
dump("crlf", file("f2.txt", FILE_IGNORE_NEW_LINES));
dump("crlfboth", file("f2.txt", FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES));
unlink("f2.txt");
"#,
    );
    assert_eq!(out, "crlf:4 0/1/a 1/1/b 2/0/ 3/1/c|crlfboth:3 0/1/a 1/1/b 2/1/c|");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies PHP's `$offset`/`$length` window on `file_get_contents()` for every shape reference
/// PHP accepts: a positive offset, an offset plus a length, a negative offset counted from the
/// end, a negative offset with a length, a length that runs past EOF, an offset past EOF, an
/// offset exactly at EOF, and a zero length.
///
/// The expected block is verbatim `LC_ALL=C php` 8.4 output for the same fixture.
#[test]
fn test_file_get_contents_offset_and_length_match_php() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("range.txt", "ABCDEFGHIJ");
var_dump(file_get_contents("range.txt"));
var_dump(file_get_contents("range.txt", false, null, 3));
var_dump(file_get_contents("range.txt", false, null, 3, 4));
var_dump(file_get_contents("range.txt", false, null, -3));
var_dump(file_get_contents("range.txt", false, null, -3, 2));
var_dump(file_get_contents("range.txt", false, null, -10));
var_dump(file_get_contents("range.txt", false, null, 0, 100));
var_dump(file_get_contents("range.txt", false, null, 20));
var_dump(file_get_contents("range.txt", false, null, 10));
var_dump(file_get_contents("range.txt", false, null, 0, 0));
unlink("range.txt");
"#,
    );
    assert_eq!(
        out,
        r#"string(10) "ABCDEFGHIJ"
string(7) "DEFGHIJ"
string(4) "DEFG"
string(3) "HIJ"
string(2) "HI"
string(10) "ABCDEFGHIJ"
string(10) "ABCDEFGHIJ"
string(0) ""
string(0) ""
string(0) ""
"#
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `$offset` and `$length` also work when they only become known at run time, so the
/// window is not folded away by the frontend before the backend sees it.
#[test]
fn test_file_get_contents_runtime_offset_and_length_match_php() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("dyn.txt", "ABCDEFGHIJ");
$offset = 2 * $argc;
$length = 3 * $argc;
var_dump(file_get_contents("dyn.txt", false, null, $offset, $length));
$none = null;
var_dump(file_get_contents("dyn.txt", false, null, $offset, $none));
unlink("dyn.txt");
"#,
    );
    assert_eq!(
        out,
        r#"string(3) "CDE"
string(8) "CDEFGHIJ"
"#
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a negative `$offset` whose magnitude exceeds the file size reproduces php-src's
/// "Failed to seek to position N in the stream" warning and returns `false`, instead of
/// clamping to the start of the file.
#[test]
fn test_file_get_contents_unreachable_negative_offset_warns_and_returns_false() {
    let out = compile_and_run_capture(
        r#"<?php
file_put_contents("seek.txt", "ABCDEFGHIJ");
var_dump(file_get_contents("seek.txt", false, null, -30));
var_dump(@file_get_contents("seek.txt", false, null, -11));
unlink("seek.txt");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\n");
    assert!(
        out.diagnostics
            .contains("Warning: file_get_contents(): Failed to seek to position -30 in the stream"),
        "expected the php-src seek warning, got diagnostics={}",
        out.diagnostics
    );
    assert!(
        !out.diagnostics.contains("position -11"),
        "the @-suppressed read must not warn, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies a `$length` far larger than the file is bounded by the bytes that are actually
/// there, and that `PHP_INT_MAX` neither wraps the kept byte count nor sizes an allocation.
///
/// This is the H4 bound: the buffer and the copy are sized by the same value, so a huge
/// `$length` can only ever mean "to the end of the data".
#[test]
fn test_file_get_contents_huge_length_is_bounded_by_the_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("huge.txt", "ABCDEFGHIJ");
echo strlen(file_get_contents("huge.txt", false, null, 0, PHP_INT_MAX)), "\n";
echo strlen(file_get_contents("huge.txt", false, null, 5, PHP_INT_MAX)), "\n";
echo strlen(file_get_contents("huge.txt", false, null, -4, PHP_INT_MAX)), "\n";
var_dump(file_get_contents("huge.txt", false, null, PHP_INT_MAX));
var_dump(file_get_contents("huge.txt", false, null, PHP_INT_MAX, PHP_INT_MAX));
unlink("huge.txt");
"#,
    );
    assert_eq!(
        out,
        r#"10
5
4
string(0) ""
string(0) ""
"#
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a negative `$length` raises php-src's catchable `ValueError` with its exact wording
/// and argument number, BEFORE the file is opened — a missing file plus a negative length still
/// throws instead of warning about the missing file.
#[test]
fn test_file_get_contents_negative_length_raises_value_error() {
    let out = compile_and_run_capture(
        r#"<?php
file_put_contents("neg.txt", "ABCDEFGHIJ");
try {
    file_get_contents("neg.txt", false, null, 0, -1);
} catch (ValueError $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
try {
    file_get_contents("does-not-exist.txt", false, null, 0, -5);
} catch (Throwable $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
unlink("neg.txt");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "ValueError: file_get_contents(): Argument #5 ($length) must be greater than or equal to 0\nValueError: file_get_contents(): Argument #5 ($length) must be greater than or equal to 0\n"
    );
    assert!(
        !out.diagnostics.contains("Failed to open stream"),
        "the negative-length ValueError must precede the open, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies `$use_include_path = true` is accepted and reads the same file.
///
/// elephc resolves paths against the current directory only — the same thing an include path of
/// `"."` does — so `true` and `false` cannot differ here.
#[test]
fn test_file_get_contents_use_include_path_reads_the_same_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("incl.txt", "ABCDEFGHIJ");
var_dump(file_get_contents("incl.txt", true));
var_dump(file_get_contents("incl.txt", true, null, 4, 3));
unlink("incl.txt");
"#,
    );
    assert_eq!(
        out,
        r#"string(10) "ABCDEFGHIJ"
string(3) "EFG"
"#
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a non-null `$context` compiles and reaches the read.
///
/// This used to be a compile error — the read path had no context plumbing, so refusing was
/// better than silently dropping the caller's options. The context is published for the
/// duration of the read now, so the same program has to compile and run. What the options
/// actually do to a request is covered by `stream_context_propagation`.
#[test]
fn test_file_get_contents_accepts_a_non_null_stream_context() {
    let out = compile_and_run_capture(
        r#"<?php
$context = stream_context_create([]);
var_dump(@file_get_contents("still-missing.txt", false, $context));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\n");
}

/// Verifies the 1-argument form is unchanged: a literal `null` context and an omitted one both
/// compile, and a missing file still returns `false` with the open warning.
#[test]
fn test_file_get_contents_null_context_and_missing_file_are_unchanged() {
    let out = compile_and_run_capture(
        r#"<?php
var_dump(@file_get_contents("still-missing.txt", false, null));
var_dump(@file_get_contents("still-missing.txt", false, null, 2, 3));
echo "after";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\nafter");
}
