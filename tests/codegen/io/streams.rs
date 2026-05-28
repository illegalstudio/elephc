//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O streams, including stdin constant, stdout constant, and stderr constant.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies STDIN constant evaluates to the expected resource display string.
#[test]
fn test_stdin_constant() {
    let out = compile_and_run("<?php echo STDIN;");
    assert_eq!(out, "Resource id #1");
}

/// Verifies STDOUT constant evaluates to the expected resource display string.
#[test]
fn test_stdout_constant() {
    let out = compile_and_run("<?php echo STDOUT;");
    assert_eq!(out, "Resource id #2");
}

/// Verifies STDERR constant evaluates to the expected resource display string.
#[test]
fn test_stderr_constant() {
    let out = compile_and_run("<?php echo STDERR;");
    assert_eq!(out, "Resource id #3");
}

/// Verifies all three standard stream constants are typed as resources via gettype().
#[test]
fn test_standard_stream_constants_are_resources() {
    let out = compile_and_run(
        r#"<?php
echo gettype(STDIN) . "|";
echo gettype(STDOUT) . "|";
echo gettype(STDERR);
"#,
    );
    assert_eq!(out, "resource|resource|resource");
}

/// Verifies standard stream constants are resolved from the global scope inside a namespace block.
#[test]
fn test_standard_stream_constants_resolve_from_namespace() {
    let out = compile_and_run(
        r#"<?php
namespace App;
echo gettype(STDOUT) . "|";
echo STDOUT;
"#,
    );
    assert_eq!(out, "resource|Resource id #2");
}

/// Verifies fopen() returns a stream resource and that resource-to-string coercion produces the PHP display string.
#[test]
fn test_fopen_returns_stream_resource() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("resource.txt", "w");
echo gettype($f) . "|";
echo $f;
fclose($f);
unlink("resource.txt");
"#,
    );
    assert!(out.starts_with("resource|Resource id #"), "unexpected output: {out}");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies fopen() returns false with a warning when opening a non-existent file for reading.
#[test]
fn test_fopen_missing_returns_false_and_warns() {
    let out = compile_and_run_capture(
        r#"<?php
$f = fopen("no_such_file.txt", "r");
echo $f === false ? "false" : "resource";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "false");
    assert!(
        out.stderr.contains("Warning: fopen()"),
        "expected fopen warning, got stderr={}",
        out.stderr
    );
}

/// Verifies @-suppression prevents the fopen() warning when opening a non-existent file.
#[test]
fn test_error_control_suppresses_fopen_missing_warning() {
    let out = compile_and_run_capture(
        r#"<?php
$f = @fopen("no_such_file.txt", "r");
echo gettype($f) . "|";
echo $f === false ? "false" : "resource";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "boolean|false");
    assert_eq!(out.stderr, "");
}

/// Verifies fopen() returns false for invalid or empty mode strings without emitting a warning.
#[test]
fn test_fopen_invalid_modes_return_false() {
    let out = compile_and_run_capture(
        r#"<?php
$bad = @fopen("bad_mode.txt", "z");
$empty = @fopen("empty_mode.txt", "");
echo ($bad === false ? "z" : "!");
echo ($empty === false ? "e" : "!");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ze");
    assert_eq!(out.stderr, "");
}

/// Verifies a stream resource passed through a mixed-type parameter preserves its resource type.
#[test]
fn test_mixed_file_handle_preserves_resource_type() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
function identity(mixed $value): mixed {
    return $value;
}
$f = fopen("mixed-resource.txt", "w");
$m = identity($f);
echo gettype($m) . "|";
echo $m;
fclose($f);
unlink("mixed-resource.txt");
"#,
    );
    assert!(out.starts_with("resource|Resource id #"), "unexpected output: {out}");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies stream resources use PHP's resource display string ("Resource id #N") in string concatenation.
#[test]
fn test_resource_concatenation_uses_php_display_string() {
    let out = compile_and_run("<?php echo \"stream=\" . STDOUT;");
    assert_eq!(out, "stream=Resource id #2");
}

/// Verifies stream resources are truthy and not empty according to PHP semantics, not raw file descriptor zero.
/// STDIN is always truthy even though its underlying fd is 0; regression for raw descriptor-based truthiness.
#[test]
fn test_resource_truthiness_does_not_use_raw_descriptor_zero() {
    let out = compile_and_run(
        r#"<?php
echo (bool)STDIN ? "truthy" : "falsy";
echo "|";
echo empty(STDIN) ? "empty" : "not-empty";
"#,
    );
    assert_eq!(out, "truthy|not-empty");
}

/// Verifies var_dump() emits the correct resource shape: "resource(N) of type (stream)".
#[test]
fn test_var_dump_resource_uses_stream_shape() {
    let out = compile_and_run("<?php var_dump(STDOUT);");
    assert_eq!(out, "resource(2) of type (stream)\n");
}

/// Verifies fopen/fwrite/fclose/fread round-trip: write "test data" to a file and read it back.
#[test]
fn test_fopen_fwrite_fclose_fread() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("rw.txt", "w");
fwrite($f, "test data");
fclose($f);
$f = fopen("rw.txt", "r");
$content = fread($f, 9);
fclose($f);
echo $content;
unlink("rw.txt");
"#,
    );
    assert_eq!(out, "test data");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies fgets() reads one line from STDIN when piped input is provided.
#[test]
fn test_fgets_stdin() {
    let out = compile_and_run_with_stdin(
        r#"<?php
$line = fgets(STDIN);
echo "got: " . $line;
"#,
        "hello\n",
    );
    assert_eq!(out, "got: hello\n");
}

/// Verifies fgets() raises a TypeError when passed false (e.g., from a failed fopen).
#[test]
fn test_fopen_false_stream_use_is_type_error() {
    let out = compile_and_run_capture(
        r#"<?php
 $f = @fopen("no_such_file.txt", "r");
$line = fgets($f);
echo "done";
"#,
    );
    assert!(!out.success, "program unexpectedly succeeded");
    assert!(
        out.stderr.contains("TypeError: fgets()") && out.stderr.contains("false given"),
        "expected fgets TypeError, got stderr={}",
        out.stderr
    );
}

/// Verifies fgets() TypeError reports the actual runtime type when a non-stream is passed.
#[test]
fn test_stream_type_error_reports_runtime_string_type() {
    let out = compile_and_run_capture(
        r#"<?php
function identity(mixed $value): mixed {
    return $value;
}
fgets(identity("not a stream"));
"#,
    );
    assert!(!out.success, "program unexpectedly succeeded");
    assert!(
        out.stderr.contains("TypeError: fgets()") && out.stderr.contains("string given"),
        "expected string TypeError, got stderr={}",
        out.stderr
    );
}

/// Verifies fopen() result can be guarded with a false check before reading from it.
#[test]
fn test_fopen_guarded_resource_path_can_read() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("guarded.txt", "safe");
$f = fopen("guarded.txt", "r");
if ($f === false) {
    echo "fail";
} else {
    echo fread($f, 4);
    fclose($f);
}
unlink("guarded.txt");
"#,
    );
    assert_eq!(out, "safe");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies feof() is not incorrectly set stale when a file descriptor is closed and reopened.
#[test]
fn test_fopen_clears_stale_eof_for_reused_descriptor() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("first.txt", "x");
file_put_contents("second.txt", "y");
$f = fopen("first.txt", "r");
fread($f, 1);
fread($f, 1);
fclose($f);
$g = fopen("second.txt", "r");
echo feof($g) ? "eof" : "not-eof";
fclose($g);
unlink("first.txt");
unlink("second.txt");
"#,
    );
    assert_eq!(out, "not-eof");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies fseek() positions and ftell() reports the correct offset; fread reads from the seek position.
#[test]
fn test_fseek_ftell() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("seek.txt", "abcdefghij");
$f = fopen("seek.txt", "r");
$result = fseek($f, 5);
echo $result;
echo ftell($f);
$data = fread($f, 5);
echo $data;
fclose($f);
unlink("seek.txt");
"#,
    );
    assert_eq!(out, "05fghij");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies fseek() returns 0 on success and SEEK_SET/SEEK_CUR/SEEK_END constant modes work correctly.
#[test]
fn test_fseek_return_value() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("seek2.txt", "hello world");
$f = fopen("seek2.txt", "r");
$r1 = fseek($f, 0);
echo $r1;
$r2 = fseek($f, 3, 0);
echo $r2;
$r3 = fseek($f, 2, 1);
echo $r3;
echo ftell($f);
fclose($f);
unlink("seek2.txt");
"#,
    );
    assert_eq!(out, "0005");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies fseek() clears the EOF flag after reading past end-of-file.
#[test]
fn test_fseek_clears_eof_after_successful_seek() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("seek-eof.txt", "x");
$f = fopen("seek-eof.txt", "r");
fread($f, 1);
fread($f, 1);
echo feof($f) ? "eof" : "not-eof";
fseek($f, 0);
echo "|" . (feof($f) ? "eof" : "not-eof");
fclose($f);
unlink("seek-eof.txt");
"#,
    );
    assert_eq!(out, "eof|not-eof");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies fgetcsv() parses a single CSV row and access to the first field.
#[test]
fn test_fgetcsv() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("data.csv", "alice,30,NY\n");
$f = fopen("data.csv", "r");
$row = fgetcsv($f);
echo $row[0];
fclose($f);
unlink("data.csv");
"#,
    );
    assert_eq!(out, "alice");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies fputcsv() writes a valid CSV line and file_get_contents() reads it back.
#[test]
fn test_fputcsv() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("out.csv", "w");
$data = ["hello", "world"];
fputcsv($f, $data);
fclose($f);
$content = file_get_contents("out.csv");
echo trim($content);
unlink("out.csv");
"#,
    );
    assert_eq!(out, "hello,world");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies rewind() resets the read position to the start and data can be re-read.
#[test]
fn test_rewind() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("rw.txt", "abcdef");
$f = fopen("rw.txt", "r");
$first = fread($f, 3);
rewind($f);
$again = fread($f, 3);
fclose($f);
echo $first . "|" . $again;
unlink("rw.txt");
"#,
    );
    assert_eq!(out, "abc|abc");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies rewind() clears the EOF flag after reading past end-of-file.
#[test]
fn test_rewind_clears_eof_after_successful_seek() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("rewind-eof.txt", "x");
$f = fopen("rewind-eof.txt", "r");
fread($f, 1);
fread($f, 1);
echo feof($f) ? "eof" : "not-eof";
rewind($f);
echo "|" . (feof($f) ? "eof" : "not-eof");
fclose($f);
unlink("rewind-eof.txt");
"#,
    );
    assert_eq!(out, "eof|not-eof");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies feof() returns true only after reading past the end of a file.
#[test]
fn test_feof() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("eof.txt", "hi");
$f = fopen("eof.txt", "r");
$data = fread($f, 2);
$data = fread($f, 1);
if (feof($f)) { echo "eof"; }
fclose($f);
unlink("eof.txt");
"#,
    );
    assert_eq!(out, "eof");
    let _ = fs::remove_dir_all(&dir);
}
