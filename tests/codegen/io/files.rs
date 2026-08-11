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
    assert!(
        out.stderr.contains("Warning: file_get_contents()"),
        "expected runtime warning, got stderr={}",
        out.stderr
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
        out.stderr
            .contains("Warning: file_get_contents(): Failed to seek to position -30 in the stream"),
        "expected the php-src seek warning, got stderr={}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("position -11"),
        "the @-suppressed read must not warn, got stderr={}",
        out.stderr
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
        !out.stderr.contains("Failed to open stream"),
        "the negative-length ValueError must precede the open, got stderr={}",
        out.stderr
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

/// Verifies a non-null `$context` is refused with a diagnostic that names the parameter, rather
/// than being silently ignored: elephc has no stream-context plumbing on the read path, so a
/// caller's context options could not be honored.
#[test]
fn test_file_get_contents_rejects_a_non_null_stream_context() {
    let error = compile_source_expect_backend_error(
        r#"<?php
$context = stream_context_create([]);
echo file_get_contents("x.txt", false, $context);
"#,
    );
    assert!(
        error.contains("file_get_contents() $context argument"),
        "expected a diagnostic naming $context, got: {error}"
    );
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
