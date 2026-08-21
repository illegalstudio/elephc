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
    // php-src puts the PATH inside the parentheses and the reason after it; the bare
    // `fopen()` this used to assert named neither.
    assert!(
        out.diagnostics.contains(
            "Warning: fopen(no_such_file.txt): Failed to open stream: No such file or directory"
        ),
        "expected the path and reason in the warning, got diagnostics={}",
        out.diagnostics
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
    assert_eq!(out.diagnostics, "");
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
    assert_eq!(out.diagnostics, "");
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
fn test_fgets_returns_false_at_eof() {
    // Regression: fgets() used to return PhpType::Str unconditionally,
    // so `while (($l = fgets($f)) !== false)` looped forever — the
    // !== false comparison always saw a string. fgets() now boxes its
    // result as Mixed: string on success, PHP false on zero-byte read
    // (EOF with no bytes accumulated).
    let out = compile_and_run(
        r#"<?php
$f = fopen("php://memory", "r+");
fwrite($f, "line1\nline2\nline3\n");
rewind($f);
$count = 0;
while (($l = fgets($f)) !== false) {
    echo $l;
    $count++;
    if ($count > 10) { echo "OVERRUN"; break; }
}
echo "count=$count";
"#,
    );
    assert_eq!(out, "line1\nline2\nline3\ncount=3");
}

/// Verifies compiled PHP output for fgets stdin.
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

/// Verifies the PHP manual's own `fgetcsv()` read loop terminates and yields every row.
///
/// `while (($row = fgetcsv($h)) !== false)` ran forever: `fgetcsv()` was declared to return
/// `array<string>` and answered an empty array at end of file, which is never `!== false`.
/// Three things had to agree for the idiom to work — the runtime signalling EOF distinctly,
/// the declared type carrying a `false` arm (as `False`, not `Bool`, so the guard can strip
/// it), and the guard narrowing seeing through the assignment inside the condition. The
/// `count($row)` in the body is the part that fails when the narrowing does not apply, and
/// the empty third line is the part that fails if EOF is confused with an empty row.
#[test]
fn test_fgetcsv_manual_read_loop_terminates_and_narrows() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("rows.csv", "a,b,c\n1,2,3\n\nx,y,z\n");
$h = fopen("rows.csv", "r");
$n = 0;
while (($row = fgetcsv($h)) !== false) {
    $n += 1;
    echo "[", count($row), ":", $row[0], "]";
}
fclose($h);
echo "|rows=", $n;
unlink("rows.csv");
"#,
    );
    assert_eq!(out, "[3:a][3:1][1:][3:x]|rows=4");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the `while` guard narrowing does not outlive its loop.
///
/// The narrowing added for the read-loop idiom applies to every `while` in the language, so
/// the interesting cases are the ones where the loop is left with the guard still TRUE. After
/// a normal exit the guarded variable holds `false` and must read back as such — a narrowing
/// that leaked would have codegen treat that `false` as an array. After a `break` it holds an
/// array, which the conservative restore has to keep working too.
#[test]
fn test_while_guard_narrowing_does_not_outlive_the_loop() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("w.csv", "a,b\nc,d\n");
$h = fopen("w.csv", "r");
$seen = 0;
while (($row = fgetcsv($h)) !== false) {
    $seen += count($row);
}
fclose($h);
echo "seen=", $seen, " after=", var_export($row, true), "|";
$h = fopen("w.csv", "r");
while (($r2 = fgetcsv($h)) !== false) {
    break;
}
fclose($h);
echo "broke=", ($r2 === false) ? "false" : "array";
unlink("w.csv");
"#,
    );
    assert_eq!(out, "seen=4 after=false|broke=array");
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

/// Verifies fgetcsv() honors a custom separator.
#[test]
fn test_fgetcsv_custom_separator() {
    let (out, _dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("php://memory", "r+");
fwrite($f, "a;b;c\n1;2;3\n");
rewind($f);
$row1 = fgetcsv($f, 0, ";");
$row2 = fgetcsv($f, 0, ";");
echo $row1[0] . $row1[1] . $row1[2] . "\n";
echo $row2[0] . $row2[1] . $row2[2] . "\n";
"#,
    );
    assert_eq!(out, "abc\n123\n");
}

/// Verifies fgetcsv() honors a custom enclosure character.
#[test]
fn test_fgetcsv_custom_enclosure() {
    let (out, _dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("php://memory", "r+");
fwrite($f, "'a','b,c','d'\n");
rewind($f);
$row = fgetcsv($f, 0, ",", "'");
echo $row[0] . "|" . $row[1] . "|" . $row[2] . "\n";
"#,
    );
    assert_eq!(out, "a|b,c|d\n");
}

/// Verifies fgetcsv() with PHP 8.4 doubling mode (escape="").
#[test]
fn test_fgetcsv_php84_doubling() {
    let (out, _dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("php://memory", "r+");
fwrite($f, "\"a\"\"b\",\"c\"\n");
rewind($f);
$row = fgetcsv($f, 0, ",", "\"", "");
echo $row[0] . "|" . $row[1] . "\n";
"#,
    );
    assert_eq!(out, "a\"b|c\n");
}

/// Verifies fputcsv() honors custom separator and enclosure.
#[test]
fn test_fputcsv_custom_separator_enclosure() {
    let (out, _dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("php://memory", "r+");
fputcsv($f, ["a", "b;c", "d"], ";", "'");
rewind($f);
echo fread($f, 100);
"#,
    );
    assert_eq!(out, "a;'b;c';d\n");
}

/// Verifies fputcsv() honors a custom end-of-line string.
#[test]
fn test_fputcsv_custom_eol() {
    let (out, _dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("php://memory", "r+");
fputcsv($f, ["a", "b"], ",", "\"", "\\", "\r\n");
rewind($f);
echo bin2hex(fread($f, 100));
"#,
    );
    assert_eq!(out, "612c620d0a");
}

/// Verifies fputcsv+fgetcsv round-trip with custom delimiters and doubling mode.
#[test]
fn test_fputcsv_fgetcsv_roundtrip_custom() {
    let (out, _dir) = compile_and_run_in_dir(
        r##"<?php
$f = fopen("php://memory", "r+");
fputcsv($f, ["a;b", 'c"d'], ";", "#", "", "\n");
rewind($f);
$r = fgetcsv($f, 0, ";", "#", "");
echo $r[0] . "|" . $r[1] . "\n";
"##,
    );
    assert_eq!(out, "a;b|c\"d\n");
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

// --- resource & stream introspection (streams/sockets phase 1) ---

/// Verifies compiled PHP output for is resource true for stream.
#[test]
fn test_is_resource_true_for_stream() {
    let out = compile_and_run("<?php var_dump(is_resource(STDIN));");
    assert_eq!(out, "bool(true)\n");
}

/// Verifies compiled PHP output for is resource false for non resource.
#[test]
fn test_is_resource_false_for_non_resource() {
    let out = compile_and_run(
        r#"<?php
echo is_resource(42) ? "y" : "n";
echo is_resource("s") ? "y" : "n";
echo is_resource(null) ? "y" : "n";
"#,
    );
    assert_eq!(out, "nnn");
}

/// Verifies compiled PHP output for get resource type returns stream.
#[test]
fn test_get_resource_type_returns_stream() {
    let out = compile_and_run("<?php echo get_resource_type(STDOUT);");
    assert_eq!(out, "stream");
}

/// Verifies compiled PHP output for get resource id matches display marker.
#[test]
fn test_get_resource_id_matches_display_marker() {
    let out = compile_and_run(
        r#"<?php echo get_resource_id(STDIN) . "|" . get_resource_id(STDOUT) . "|" . get_resource_id(STDERR);"#,
    );
    assert_eq!(out, "1|2|3");
}

/// Verifies compiled PHP output for resource introspection is case insensitive.
#[test]
fn test_resource_introspection_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php echo IS_RESOURCE(STDIN) ? "y" : "n"; echo Get_Resource_Type(STDIN);"#,
    );
    assert_eq!(out, "ystream");
}

/// Verifies compiled PHP output for stream isatty false for regular file.
#[test]
fn test_stream_isatty_false_for_regular_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("tty_probe.txt", "w");
var_dump(stream_isatty($f));
fclose($f);
unlink("tty_probe.txt");
"#,
    );
    assert_eq!(out, "bool(false)\n");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream is local and supports lock are true.
#[test]
fn test_stream_is_local_and_supports_lock_are_true() {
    let out = compile_and_run(
        r#"<?php echo stream_is_local(STDIN) ? "L" : "l"; echo stream_supports_lock(STDIN) ? "S" : "s";"#,
    );
    assert_eq!(out, "LS");
}

/// Verifies `fgetcsv()` ends the manual's own read loop instead of spinning on it.
///
/// The runtime signals end-of-input with a null array pointer. Storing that raw left it
/// reading as `null`, and `null !== false` holds, so
/// `while (($row = fgetcsv($h)) !== false)` — the loop PHP's manual shows — ran forever;
/// a loop that guarded itself fatalled on `count(null)` instead. The counter here is the
/// point: a test that only checked the parsed fields passed throughout.
#[test]
fn test_fgetcsv_reports_false_at_end_of_input() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("csv_eof.csv", "a,b\nc,d\n");
$f = fopen("csv_eof.csv", "r");
$rows = 0;
while (($row = fgetcsv($f, 0, ",", "\"", "\\")) !== false) {
    $rows = $rows + 1;
    if ($rows > 8) { echo "RUNAWAY"; break; }
}
fclose($f);
echo $rows;
unlink("csv_eof.csv");
"#,
    );
    assert_eq!(out, "2");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a row read by `fgetcsv()` can be written straight back by `fputcsv()`.
///
/// This is the pair's whole point, and it is the shape that broke when `fgetcsv()` started
/// reporting `array<string>|false`: the row is stored boxed, and the writer accepted only
/// an unboxed string array, so the read-transform-write pipeline stopped COMPILING. The
/// union is what makes unwrapping safe — it guarantees the payload is a string array.
#[test]
fn test_fgetcsv_row_can_be_written_back_by_fputcsv() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("pipe_in.csv", "1,x\n2,\"y,z\"\n");
$in = fopen("pipe_in.csv", "r");
$out = fopen("pipe_out.csv", "w");
while (($rec = fgetcsv($in, 0, ",", "\"", "\\")) !== false) {
    fputcsv($out, $rec, ",", "\"", "\\");
}
fclose($in);
fclose($out);
echo file_get_contents("pipe_out.csv");
unlink("pipe_in.csv");
unlink("pipe_out.csv");
"#,
    );
    assert_eq!(out, "1,x\n2,\"y,z\"\n");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies writing an end-of-input `fgetcsv()` result raises php-src's own `TypeError`.
#[test]
fn test_fputcsv_rejects_a_false_fields_argument() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("empty_in.csv", "");
$in = fopen("empty_in.csv", "r");
$out = fopen("t_out.csv", "w");
$rec = fgetcsv($in, 0, ",", "\"", "\\");
try {
    fputcsv($out, $rec, ",", "\"", "\\");
} catch (TypeError $e) {
    echo $e->getMessage();
}
fclose($in);
fclose($out);
unlink("empty_in.csv");
unlink("t_out.csv");
"#,
    );
    assert_eq!(
        out,
        "fputcsv(): Argument #2 ($fields) must be of type array, false given"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a BLANK LINE reads back as php's `[null]` record rather than `[""]`.
///
/// php-src decides "no line at all" and "a line with no fields" in two different places:
/// `PHP_FUNCTION(fgetcsv)` answers `false` when `php_stream_get_line()` returns NULL, and only
/// then calls `php_fgetcsv()`, whose own NULL (`first_field && bptr == line_end`) becomes
/// `php_bc_fgetcsv_empty_line()` — one element holding null. elephc collapsed both onto one null
/// pointer, so a blank line came back as a one-element array holding the EMPTY STRING. The
/// record COUNT is half the point: `[""]` and `[null]` both have one element, so a test that
/// only counted rows passed throughout. Measured on `php -n` 8.5.6.
#[test]
fn test_fgetcsv_reads_a_blank_line_as_a_null_record() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("blank_mid.csv", "a,b\n\nc,d\n");
$f = fopen("blank_mid.csv", "r");
$seen = "";
$rows = 0;
while (($row = fgetcsv($f, 0, ",", "\"", "\\")) !== false) {
    $seen = $seen . json_encode($row) . ";";
    $rows = $rows + 1;
    if ($rows > 8) { echo "RUNAWAY"; break; }
}
fclose($f);
echo $seen, "|", $rows;
unlink("blank_mid.csv");
"#,
    );
    assert_eq!(out, "[\"a\",\"b\"];[null];[\"c\",\"d\"];|3");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a file ENDING in a blank line yields a trailing `[null]`, then `false`.
///
/// This is the case that proves the two markers stayed apart: the blank record and end of input
/// arrive back to back, so collapsing them either loses the last row or spins the manual's loop.
#[test]
fn test_fgetcsv_blank_last_line_is_a_null_record_then_false() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("blank_end.csv", "a,b\nc,d\n\n");
$f = fopen("blank_end.csv", "r");
$seen = "";
$rows = 0;
while (($row = fgetcsv($f, 0, ",", "\"", "\\")) !== false) {
    $seen = $seen . json_encode($row) . ";";
    $rows = $rows + 1;
    if ($rows > 8) { echo "RUNAWAY"; break; }
}
fclose($f);
echo $seen, "|", $rows;
unlink("blank_end.csv");
"#,
    );
    assert_eq!(out, "[\"a\",\"b\"];[\"c\",\"d\"];[null];|3");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies only the LINE TERMINATOR is stripped before the blank test — whitespace is a field.
///
/// `php_fgetcsv_lookup_trailing_spaces()` drops one `\r\n`, `\n` or `\r` and nothing else despite
/// its name, so `"   \n"` and `"\t\n"` are one field of whitespace while `"\n"` and `"\r\n"` are
/// no record at all. Without this control a "trim the line" rule looks equally correct and
/// silently turns every whitespace-only row into `[null]`.
#[test]
fn test_fgetcsv_treats_a_whitespace_only_line_as_a_field_not_a_blank() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ws.csv", "a,b\n   \n\t\n\nc,d\n");
$f = fopen("ws.csv", "r");
$seen = "";
$rows = 0;
while (($row = fgetcsv($f, 0, ",", "\"", "\\")) !== false) {
    $seen = $seen . json_encode($row) . ";";
    $rows = $rows + 1;
    if ($rows > 8) { echo "RUNAWAY"; break; }
}
fclose($f);
echo $seen, "|", $rows;
unlink("ws.csv");
"#,
    );
    assert_eq!(out, "[\"a\",\"b\"];[\"   \"];[\"\\t\"];[null];[\"c\",\"d\"];|5");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a file of nothing but one blank line yields ONE `[null]` row, not zero and not two.
///
/// The sharpest test of the split markers: the very first read is a blank record and the very
/// next is end of input, so a single marker answers one of them wrongly whichever way it leans.
/// A `\r\n` blank line is the same record, since one terminator is stripped as a unit.
#[test]
fn test_fgetcsv_separates_a_blank_record_from_end_of_input() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("only_blank.csv", "\n");
file_put_contents("empty.csv", "");
file_put_contents("crlf.csv", "a,b\n\r\nc,d\n");
$seen = "";
foreach (["only_blank.csv", "empty.csv", "crlf.csv"] as $name) {
    $f = fopen($name, "r");
    $rows = 0;
    while (($row = fgetcsv($f, 0, ",", "\"", "\\")) !== false) {
        $seen = $seen . json_encode($row) . ";";
        $rows = $rows + 1;
        if ($rows > 8) { echo "RUNAWAY"; break; }
    }
    fclose($f);
    $seen = $seen . "|" . $rows . " ";
    unlink($name);
}
echo $seen;
"#,
    );
    assert_eq!(
        out,
        "[null];|1 |0 [\"a\",\"b\"];[null];[\"c\",\"d\"];|3 "
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `fputcsv()` writes a `[null]` row back as the blank line it came from.
///
/// The round-trip is the pair's whole point and the consumer most exposed to the row's element
/// type changing from `string` to `mixed`: the writer now receives a boxed cell holding null
/// where it used to receive a string slot, and php writes that as an empty line.
#[test]
fn test_fputcsv_writes_back_a_null_record_read_by_fgetcsv() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("bp_in.csv", "a,b\n\nc,d\n");
$in = fopen("bp_in.csv", "r");
$out = fopen("bp_out.csv", "w");
while (($rec = fgetcsv($in, 0, ",", "\"", "\\")) !== false) {
    fputcsv($out, $rec, ",", "\"", "\\");
}
fclose($in);
fclose($out);
echo json_encode(file_get_contents("bp_out.csv"));
unlink("bp_in.csv");
unlink("bp_out.csv");
"#,
    );
    assert_eq!(out, "\"a,b\\n\\nc,d\\n\"");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `SplFileObject::fgetcsv()` reports a blank line as `[null]` too.
///
/// The SPL method body is synthesized and has no checked call-site type, so it reads the row
/// through the EIR fallback rather than the checker's union — a second authority that has to
/// agree about the boxed-`Mixed` cells, and the one that silently handed back header words as
/// integers the last time `fgetcsv()`'s representation moved.
#[test]
fn test_spl_file_object_fgetcsv_reads_a_blank_line_as_null() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("spl_blank.csv", "a,b\n\nc,d\n");
$f = new SplFileObject("spl_blank.csv");
$seen = "";
$rows = 0;
while (!$f->eof()) {
    $row = $f->fgetcsv(",", "\"", "\\");
    if ($row === false) { break; }
    $seen = $seen . json_encode($row) . ";";
    $rows = $rows + 1;
    if ($rows > 8) { echo "RUNAWAY"; break; }
}
unset($f);
echo $seen;
unlink("spl_blank.csv");
"#,
    );
    assert_eq!(out, "[\"a\",\"b\"];[null];[\"c\",\"d\"];");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an UNTYPED `public $context;` receives its context — the spelling the manual shows.
///
/// `public mixed $context;` already worked. The untyped form was read as declaring nothing, so
/// the wrapper never got its context and collected the dynamic-property deprecation meant for
/// classes that really declared none. The two spellings are the same PHP null and differ only in
/// elephc's representation: an untyped property is initialised to the in-band tagged null rather
/// than to a cell pointer, which the context injection was freeing as though it were one.
#[test]
fn test_an_untyped_context_property_receives_its_context() {
    let out = compile_and_run_capture(
        r#"<?php
class W {
    public $context;
    public function stream_open($path, $mode, $options, &$opened) { return true; }
    public function stream_read($n) { return ""; }
    public function stream_eof() { return true; }
    public function stream_stat() { return []; }
    public function stream_close() {}
}
stream_wrapper_register("w", "W");
$h = fopen("w://x", "r");
echo $h === false ? "false" : "resource";
fclose($h);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "resource");
    assert!(
        !out.diagnostics.contains("dynamic property"),
        "a declared $context must not be deprecated as invented, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies the dynamic-property deprecation still fires for a wrapper that declares NO context.
///
/// The guard above widens which spellings count as declared, so this pins the other side of it:
/// PHP assigns the context whether or not the class declared a property for it, and deprecates
/// the invented assignment.
#[test]
fn test_a_wrapper_without_a_context_property_is_still_deprecated() {
    let out = compile_and_run_capture(
        r#"<?php
class N {
    public function stream_open($path, $mode, $options, &$opened) { return true; }
    public function stream_read($n) { return ""; }
    public function stream_eof() { return true; }
    public function stream_stat() { return []; }
    public function stream_close() {}
}
stream_wrapper_register("n", "N");
$h = fopen("n://x", "r");
echo $h === false ? "false" : "resource";
fclose($h);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "resource");
    assert!(
        out.diagnostics
            .contains("Creation of dynamic property N::$context is deprecated"),
        "expected PHP 8.2's deprecation, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies an unknown scheme reports the MISSING WRAPPER, which is the reason php gives first.
///
/// php-src emits two warnings here. elephc emitted only the second, which says "No such file or
/// directory" — true of the path, and silent about the cause.
#[test]
fn test_unknown_wrapper_names_itself_like_php() {
    let out = compile_and_run_capture(
        r#"<?php
$h = fopen("bogus://x", "r");
echo $h === false ? "false" : "resource";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "false");
    assert!(
        out.diagnostics.contains(
            "Warning: fopen(): Unable to find the wrapper \"bogus\" - did you forget to enable it when you configured PHP?"
        ),
        "missing the unknown-wrapper warning, got diagnostics={}",
        out.diagnostics
    );
    assert!(
        out.diagnostics.contains("Warning: fopen(bogus://x): Failed to open stream:"),
        "the failed-open warning must still follow it, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies the unknown-wrapper warning stays silent for every scheme that DOES have a wrapper.
///
/// The check has to run at run time, not at lowering: `stream_wrapper_register()` is a runtime
/// call, so a scheme the compiler never heard of can be perfectly valid by the time an open
/// happens. Both authorities are consulted, and a path with no scheme at all is not a wrapper.
#[test]
fn test_a_known_wrapper_does_not_report_itself_missing() {
    let out = compile_and_run_capture(
        r#"<?php
class Mem {
    public $context;
    public $pos = 0;
    public function stream_open($path, $mode, $options, &$opened) { return true; }
    public function stream_read($n) { $this->pos = $this->pos + 1; return $this->pos > 1 ? "" : "hi"; }
    public function stream_eof() { return $this->pos > 1; }
    public function stream_stat() { return []; }
    public function stream_close() {}
}
stream_wrapper_register("mine", "Mem");
$h = fopen("mine://x", "r");
fclose($h);
$p = fopen("php://memory", "w+");
fclose($p);
$m = @fopen("/no/such/file", "r");
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert!(
        !out.diagnostics.contains("Unable to find the wrapper"),
        "a registered wrapper, a built-in scheme and a plain path must all stay quiet, got diagnostics={}",
        out.diagnostics
    );
    assert_eq!(out.stdout, "done");
}

/// Verifies `fputcsv()` casts each element LAYOUT, as `php_fputcsv` does per field.
///
/// One case per layout rather than one test for all six. The layout is what the writer has to
/// read — 16-byte (ptr, len) slots for strings, 8-byte payloads for int/float/bool, 8-byte cell
/// pointers for a gradual array — so a single combined test can only report that one of six is
/// wrong, which is useless on an architecture this host cannot run. Every expectation was
/// measured against `php -n` 8.5.6.
fn fputcsv_layout_case(row: &str, expected: &str) {
    let source = format!(
        r#"<?php
$out = fopen("cast_out.csv", "w");
fputcsv($out, {row}, ",", "\"", "\\");
fclose($out);
echo file_get_contents("cast_out.csv");
unlink("cast_out.csv");
"#
    );
    let (out, dir) = compile_and_run_in_dir(&source);
    assert_eq!(out, expected, "row {row} rendered wrongly");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_fputcsv_string_layout() {
    fputcsv_layout_case(r#"["a", "b"]"#, "a,b\n");
}

#[test]
fn test_fputcsv_int_layout() {
    fputcsv_layout_case("[1, 2, 3]", "1,2,3\n");
}

#[test]
fn test_fputcsv_float_layout() {
    fputcsv_layout_case("[1.5, 2.25]", "1.5,2.25\n");
}

#[test]
fn test_fputcsv_bool_layout() {
    fputcsv_layout_case("[true, false]", "1,\n");
}

#[test]
fn test_fputcsv_boxed_mixed_layout() {
    fputcsv_layout_case(r#"["name", 42, 3.5, true, null]"#, "name,42,3.5,1,\n");
}

#[test]
fn test_fputcsv_boxed_layout_still_quotes() {
    fputcsv_layout_case(r#"["with,comma", 7]"#, "\"with,comma\",7\n");
}

#[test]
fn test_fputcsv_empty_row() {
    fputcsv_layout_case("[]", "\n");
}

/// Verifies a `foreach` row reaches the writer as its ARRAY, not as the Mixed cell carrying it.
///
/// A gradually-typed row arrives boxed. Writing the box would not merely mis-render a field: the
/// cell's tag word reads as a length, so this two-field row came out as four fields of raw header
/// bytes before the writer unwrapped it.
#[test]
fn test_fputcsv_writes_a_foreach_row_not_its_box() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$out = fopen("rows_out.csv", "w");
foreach ([[1, 2], [3, 4]] as $row) {
    fputcsv($out, $row, ",", "\"", "\\");
}
fclose($out);
echo file_get_contents("rows_out.csv");
unlink("rows_out.csv");
"#,
    );
    assert_eq!(out, "1,2\n3,4\n");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a long numeric run hands its formatting scratch back, row by row.
///
/// `__rt_itoa` formats into the shared 64 KiB concat arena and advances its cursor. A writer that
/// never reclaimed the row's scratch would walk off the arena long before this loop ends, so the
/// failure this pins is a silent memory overrun rather than a wrong field.
#[test]
fn test_fputcsv_reclaims_its_cast_scratch_across_many_rows() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$out = fopen("many.csv", "w");
for ($i = 0; $i < 4000; $i++) {
    fputcsv($out, [$i, $i * 2, $i * 3], ",", "\"", "\\");
}
fclose($out);
$lines = file("many.csv");
echo count($lines), "|", trim($lines[0]), "|", trim($lines[3999]);
unlink("many.csv");
"#,
    );
    assert_eq!(out, "4000|0,0,0|3999,7998,11997");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a `php://filter` chain runs EVERY filter, in order.
///
/// Only the first name was applied, so `read=a|b` silently produced `a`'s output — which
/// looks plausible and is wrong. `convert.base64-encode` and `string.toupper` do not
/// commute, so swapping them proves the ORDER is right rather than just the count.
#[test]
fn test_php_filter_chain_applies_every_filter_in_order() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("fchain.txt", "Hello World");
$a = fopen("php://filter/read=convert.base64-encode|string.toupper/resource=fchain.txt", "r");
echo stream_get_contents($a), "|";
fclose($a);
$b = fopen("php://filter/read=string.toupper|convert.base64-encode/resource=fchain.txt", "r");
echo stream_get_contents($b), "|";
fclose($b);
$c = fopen("php://filter/read=string.toupper|no.such.filter/resource=fchain.txt", "r");
echo stream_get_contents($c);
fclose($c);
unlink("fchain.txt");
"#,
    );
    // The third case pins what an UNKNOWN name does: `php -n` skips it, keeps its
    // neighbours, and still opens. Cancelling the whole chain reads as just as plausible,
    // which is why it is measured rather than reasoned about.
    assert_eq!(out, "SGVSBG8GV29YBGQ=|SEVMTE8gV09STEQ=|HELLO WORLD");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a failed open names WHICH path failed and WHY, as php-src does.
///
/// The message was a bare `fopen(): Failed to open stream` — neither the path nor the
/// reason, which is most of what it exists for when several opens share a line. The
/// remaining difference from PHP is the ` in FILE on line N` suffix elephc never adds.
#[test]
fn test_failed_open_warning_names_the_path_and_the_reason() {
    let out = compile_and_run_capture(
        r#"<?php
$f = fopen("/no/such/dir/missing.txt", "r");
echo $f === false ? "false" : "open";
$c = file_get_contents("/no/such/dir/other.txt");
echo $c === false ? "|false" : "|read";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "false|false");
    assert!(
        out.diagnostics.contains(
            "Warning: fopen(/no/such/dir/missing.txt): Failed to open stream: No such file or directory"
        ),
        "fopen warning lost the path or the reason, got diagnostics={}",
        out.diagnostics
    );
    assert!(
        out.diagnostics.contains(
            "Warning: file_get_contents(/no/such/dir/other.txt): Failed to open stream: No such file or directory"
        ),
        "file_get_contents warning lost the path or the reason, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies an invalid `fopen()` mode is reported in php's words, not as a bogus errno.
///
/// No syscall runs for a mode php refuses, so there is no errno to describe. The failure shared
/// the errno path anyway, which read `x0`/`rax` — still carrying the PATH POINTER on that branch
/// — and handed it to `strerror`.
///
/// RED before the fix, `php -n` 8.5.6 on the left and elephc on the right:
///   fopen(F,"z")  ``Failed to open stream: `z' is not a valid mode for fopen``
///                 vs `Failed to open stream: Unknown error: 80792944`
///   fopen(F,"")   ``Failed to open stream: `' is not a valid mode for fopen``
///                 vs the same garbage
///
/// The quoting is php-src's own and is NOT symmetrical: an opening backtick, a closing
/// apostrophe. The empty mode is included because it takes a different branch in `__rt_fopen`
/// (a length test, before the first byte is ever read) and shares only the wording.
#[test]
fn test_invalid_fopen_mode_is_reported_in_phps_words() {
    let out = compile_and_run_capture(
        r#"<?php
$f = "invalid_mode_probe.txt";
file_put_contents($f, "hello");
var_dump(fopen($f, "z"));
var_dump(fopen($f, ""));
$m = "br";
var_dump(fopen($f, $m));
unlink($f);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\nbool(false)\n");
    for reason in ["`z'", "`'", "`br'"] {
        assert!(
            out.diagnostics.contains(&format!(
                "Warning: fopen(invalid_mode_probe.txt): Failed to open stream: {reason} is not a valid mode for fopen"
            )),
            "missing php's wording for mode {reason}, got diagnostics={}",
            out.diagnostics
        );
    }
    assert!(
        !out.diagnostics.contains("Unknown error"),
        "the mode failure still went through the errno path, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies php validates only the FIRST byte of an `fopen()` mode, and the rest is free.
///
/// php-src's `php_stream_parse_fopen_modes` switches on `mode[0]` and then only ever asks
/// `strchr(mode, '+')`. So `rz`, `rbz`, `rw`, `ra` and even `"r "` all open, while `br`, `tr`,
/// `+r` and `" r"` do not — the letters are not a SET, they are a first character. Measured
/// across 20 spellings on `php -n` 8.5.6; this pins the eight that make the rule visible.
///
/// It guards the fix above rather than the parse: rewording the failure must not move the line
/// between accept and reject, and `rz`/`rw`/`ra` are exactly the spellings a "valid letters"
/// reading would start rejecting.
#[test]
fn test_only_the_first_fopen_mode_byte_decides_validity() {
    let out = compile_and_run(
        r#"<?php
$f = "first_byte_mode_probe.txt";
file_put_contents($f, "hello");
foreach (["rz", "rbz", "rw", "ra", "r ", "br", "+r", " r"] as $m) {
    $h = @fopen($f, $m);
    echo $h === false ? "-" : "+";
    if ($h) fclose($h);
}
unlink($f);
"#,
    );
    assert_eq!(out, "+++++---");
}

/// Verifies every wrapper-refusal diagnostic still obeys `@`.
///
/// These lines reach stderr by four different routes — a compile-time literal interned whole, a
/// run-time composition inside `__rt_data_stream_dynamic`, another inside `__rt_php_wrapper_open`,
/// and three `__rt_diag_warning` fragments emitted by the `glob://` lowering — and only the last
/// of those goes through the path the older diagnostics used. A route that reached `write(2)`
/// without consulting the suppression depth would make `@fopen(...)` noisy, which is a silent
/// break of the one thing `@` is for.
///
/// The unsuppressed line at the end is the control: without it, a fix that disabled the
/// diagnostics entirely would pass.
#[test]
fn test_wrapper_refusal_diagnostics_obey_the_error_suppression_operator() {
    let out = compile_and_run_capture(
        r#"<?php
file_put_contents("sup_probe.txt", "x");
var_dump(@fopen("php://bogus", "r"));
var_dump(@fopen("glob://*.php", "r"));
var_dump(@fopen("data://text/plain;base64,!!!bad!!!", "r"));
var_dump(@fopen("sup_probe.txt", "z"));
$u = "php://bogus"; var_dump(@fopen($u, "r"));
$g = "glob://*.php"; var_dump(@fopen($g, "r"));
$d = "data://nocomma"; var_dump(@fopen($d, "r"));
var_dump(fopen("glob://*.php", "r"));
unlink("sup_probe.txt");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\n".repeat(8));
    assert_eq!(
        out.diagnostics.trim(),
        "Warning: fopen(glob://*.php): Failed to open stream: wrapper does not support stream open",
        "`@` let a wrapper-refusal line through, or the control line went missing"
    );
}

/// Verifies `stream_get_meta_data()` returns php's keys in php's ORDER.
///
/// A PHP array remembers insertion order and this one is routinely dumped whole, so an array with
/// identical contents in a different order still prints differently under `print_r()`,
/// `var_export()`, `json_encode()` or `foreach`. php-src fills it in `_php_stream_get_metadata`:
/// the three fallback flags, then `wrapper_type`, `stream_type`, `mode`, `unread_bytes`,
/// `seekable`, and `uri` last.
///
/// RED before the fix — elephc put `unread_bytes` third and `stream_type` ahead of
/// `wrapper_type`:
///   php     timed_out,blocked,eof,wrapper_type,stream_type,mode,unread_bytes,seekable,uri
///   elephc  timed_out,blocked,eof,unread_bytes,stream_type,wrapper_type,mode,seekable,uri
///
/// Three stream kinds are checked because the order comes from one shared builder and a
/// per-wrapper divergence would otherwise hide behind whichever one the test happened to pick.
#[test]
fn test_stream_get_meta_data_keys_are_in_phps_order() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("meta_order.txt", "hi");
foreach ([fopen("meta_order.txt", "r"), fopen("php://memory", "w+"), fopen("php://stdout", "w")] as $h) {
    echo implode(",", array_keys(stream_get_meta_data($h))), "\n";
    fclose($h);
}
unlink("meta_order.txt");
"#,
    );
    let expected =
        "timed_out,blocked,eof,wrapper_type,stream_type,mode,unread_bytes,seekable,uri\n";
    assert_eq!(out, expected.repeat(3));
}

/// Verifies `seekable` names the descriptor's TYPE rather than whether `lseek` happened to work.
///
/// php-src decides it once, at open: `php_stream_fopen_from_fd` sets `is_pipe` from
/// `!S_ISREG(sb.st_mode)` and that becomes `PHP_STREAM_FLAG_NO_SEEK`, which is what
/// `_php_stream_get_metadata` reports. elephc asked a different question — `lseek(fd, 0,
/// SEEK_CUR)` — and the two answers only agree for regular files, sockets and FIFOs. They part
/// company on a CHARACTER DEVICE, which is seekable to the kernel and not a file to PHP.
///
/// Measured with `php -n` 8.5.6:
///
/// ```text
/// fopen('/dev/null', 'r')  seekable => bool(false)   elephc said bool(true)
/// fopen('/dev/zero', 'r')  seekable => bool(false)   elephc said bool(true)
/// popen('echo hi', 'r')    seekable => bool(false)   elephc agreed
/// ```
///
/// The same divergence is what made `php://stdin` answer `true` under `< /dev/null` where php
/// answers `false`; a regular file on stdin makes BOTH answer `true`, so the descriptor kind —
/// not the wrapper name — is the thing under test.
#[test]
fn test_stream_get_meta_data_seekable_follows_the_descriptor_kind() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("seekable_probe.txt", "hi");
foreach (["seekable_probe.txt", "/dev/null", "/dev/zero"] as $path) {
    $h = fopen($path, "r");
    echo $path, "=", var_export(stream_get_meta_data($h)["seekable"], true), "\n";
    fclose($h);
}
unlink("seekable_probe.txt");
"#,
    );
    assert_eq!(
        out,
        "seekable_probe.txt=true\n/dev/null=false\n/dev/zero=false\n"
    );
}

/// Verifies the three fallback flags appear only on the streams php puts them on.
///
/// `timed_out`, `blocked` and `eof` are not unconditional in php-src: `_php_stream_get_metadata`
/// emits them only when the stream answers `PHP_STREAM_OPTION_META_DATA_API`, and the `php://`
/// wrapper answers it for `memory` but not for `temp`. `data:` never answers it at all. elephc
/// wrote all three onto every stream, so `php://temp` reported nine keys where php reports six.
///
/// Measured with `php -n` 8.5.6 — `implode(",", array_keys(...))`:
///
/// ```text
/// php://memory   timed_out,blocked,eof,wrapper_type,stream_type,mode,unread_bytes,seekable,uri
/// php://temp     wrapper_type,stream_type,mode,unread_bytes,seekable,uri
/// data://...     mediatype,base64,wrapper_type,stream_type,mode,unread_bytes,seekable,uri
/// ```
///
/// `php://temp/maxmemory:1024` is included because it is the same sub-wrapper reached through a
/// longer URI, and a check that only looked at the exact string `php://temp` would miss it.
#[test]
fn test_stream_get_meta_data_omits_the_fallback_flags_php_omits() {
    let out = compile_and_run(
        r#"<?php
foreach ([
    fopen("php://memory", "r+"),
    fopen("php://temp", "r+"),
    fopen("php://temp/maxmemory:1024", "r+"),
] as $h) {
    $keys = array_keys(stream_get_meta_data($h));
    echo implode(",", $keys), "\n";
    fclose($h);
}
"#,
    );
    assert_eq!(
        out,
        "timed_out,blocked,eof,wrapper_type,stream_type,mode,unread_bytes,seekable,uri\n\
         wrapper_type,stream_type,mode,unread_bytes,seekable,uri\n\
         wrapper_type,stream_type,mode,unread_bytes,seekable,uri\n"
    );
}

/// Verifies a `php://filter` stream names PHP as its wrapper, not the resource it wraps.
///
/// The URL is resolved by `php_stream_url_wrap_php`, so the stream php hands back belongs to the
/// `php` wrapper however ordinary the thing behind it is. elephc opened the inner resource and
/// left ITS identity on the handle, so a filter over a plain file called itself `plainfile`.
///
/// Measured with `php -n` 8.5.6 on `php://filter/read=string.toupper/resource=<file>`:
///
/// ```text
/// wrapper_type => "PHP"          elephc said "plainfile"
/// stream_type  => "STDIO"        (the INNER identity, which php keeps)
/// uri          => the whole php://filter/... URL
/// ```
///
/// `stream_type` is asserted alongside because it is the half php does NOT move: the two names
/// disagree on purpose, and a fix that dragged both to `PHP` would trade one divergence for
/// another. That is also why only a plain-path resource is re-stamped — a filter over
/// `php://memory` reports `MEMORY`, and the name is derived from the recorded URI.
///
/// Scope: the LITERAL URL route. A URL computed at run time still reports the inner wrapper,
/// because the dynamic route swaps the URL for its resource before the open and stamps the
/// resource opener's own id; that half is measured and unfixed.
#[test]
fn test_stream_get_meta_data_names_php_as_the_filter_wrapper() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("filter_meta.txt", "hi\n");
$m = stream_get_meta_data(fopen("php://filter/read=string.toupper/resource=filter_meta.txt", "r"));
echo $m["wrapper_type"], "|", $m["stream_type"], "|", $m["uri"], "\n";
unlink("filter_meta.txt");
"#,
    );
    assert_eq!(
        out,
        "PHP|STDIO|php://filter/read=string.toupper/resource=filter_meta.txt\n"
    );
}

/// Verifies a `data:` stream's metadata carries the URI's own media type, its parameters and the
/// base64 flag, each as its own key ahead of `wrapper_type`.
///
/// php-src's `php_stream_url_wrap_rfc2397` builds the metadata array while it PARSES the URI, so
/// every `name=value` before the comma lands as a separate key in the order it was written, the
/// media type lands under `mediatype`, and `base64` is a bool that is present even when false.
/// elephc reported none of them.
///
/// Measured with `php -n` 8.5.6:
///
/// ```text
/// data://text/plain,hello                 mediatype=text/plain base64=false
/// data://text/plain;charset=utf-8,x       mediatype=text/plain charset=utf-8 base64=false
/// data://text/plain;base64,aGVsbG8=       mediatype=text/plain base64=true
/// data://text/plain;charset=utf-8;foo=bar,x
///                                         mediatype/charset/foo then base64=false
/// data:,justtext                          no mediatype key at all, base64=false
/// ```
///
/// The bare `data:,justtext` row is the one that pins the key as OPTIONAL: php emits `base64`
/// unconditionally but `mediatype` only when the URI spells one.
#[test]
fn test_stream_get_meta_data_exposes_the_data_uri_parameters() {
    let out = compile_and_run(
        r#"<?php
foreach ([
    "data://text/plain,hello",
    "data://text/plain;charset=utf-8,x",
    "data://text/plain;base64,aGVsbG8=",
    "data://text/plain;charset=utf-8;foo=bar,x",
    "data:,justtext",
] as $uri) {
    $m = stream_get_meta_data(fopen($uri, "r"));
    $parts = [];
    foreach ($m as $key => $value) {
        if ($key === "wrapper_type") {
            break;
        }
        $parts[] = $key . "=" . var_export($value, true);
    }
    echo implode(" ", $parts), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "mediatype='text/plain' base64=false\n\
         mediatype='text/plain' charset='utf-8' base64=false\n\
         mediatype='text/plain' base64=true\n\
         mediatype='text/plain' charset='utf-8' foo='bar' base64=false\n\
         base64=false\n"
    );
}

/// Verifies the eval interpreter accepts every `fopen()` mode php accepts.
///
/// `EvalOpenMode::parse` refused any mode carrying a byte outside `rwaxc+bte`. php has no such
/// rule: `php_stream_parse_fopen_modes` switches on `mode[0]` and afterwards only ever asks
/// `strchr(mode, '+')`. So the interpreter refused three spellings that `php -n` 8.5.6 AND
/// elephc's own AOT backend both open — the backend and the interpreter disagreeing about the
/// same PHP is worse than either being wrong alone.
///
/// RED, over `["r","rb","rn","rz","r ","rt","w","x","br","+r","q",""]`:
///   php / AOT backend  +++++++-----
///   eval interpreter   ++---++-----
/// (`x` is `-` on both because the file already exists.) The `_ => return None` arm on the first
/// character is the whole of php's check, so the extra filter could only refuse too much.
#[test]
fn test_eval_fopen_accepts_every_mode_php_accepts() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("evalmode_probe.txt", "hello");
eval('
foreach (["r", "rb", "rn", "rz", "r ", "rt", "w", "x", "br", "+r", "q", ""] as $m) {
    $h = @fopen("evalmode_probe.txt", $m);
    echo $h === false ? "-" : "+";
    if ($h) fclose($h);
}
');
unlink("evalmode_probe.txt");
"#,
    );
    assert_eq!(out, "+++++++-----");
}

/// Verifies a `data:` URI php refuses is refused, and named with php's own `rfc2397:` sentence.
///
/// The `unable to decode` case was not just undiagnosed: the run-time opener asked
/// `__rt_base64_decode` for its LAX mode — and asked in the wrong register, since the flag is
/// `x3`/`rdi` and it wrote `x0`/`edi` — so `data://text/plain;base64,!!!not-base64!!!` opened a
/// stream over the lax decoder's salvage. php answers false. That is a silent wrong VALUE, not a
/// missing message, which is why it leads here.
///
/// Measured on `php -n` 8.5.6; each of the four sentences is a different php-src call site, and
/// which one applies is not guessable from the URI's shape — `;,` and `;BASE64,` are `illegal
/// parameter`, not `illegal media type`, because the TYPE is only the first `;`-segment.
///
/// RED before the fix (dynamic form, `$u` a loop variable):
///   `!!!not-base64!!!`  php `false` + `rfc2397: unable to decode`   vs elephc a stream of `''`
///   `data://`           php `rfc2397: no comma in URL`              vs elephc silent `false`
#[test]
fn test_refused_data_uris_carry_phps_rfc2397_reason() {
    let out = compile_and_run_capture(
        r#"<?php
foreach ([
    "data://text/plain;base64,!!!not-base64!!!",
    "data://nocomma",
    "data://text;base64,SGk=",
    "data://text/plain;,hi",
] as $u) {
    var_dump(fopen($u, "r"));
}
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\nbool(false)\nbool(false)\n");
    for (url, reason) in [
        ("data://text/plain;base64,!!!not-base64!!!", "rfc2397: unable to decode"),
        ("data://nocomma", "rfc2397: no comma in URL"),
        ("data://text;base64,SGk=", "rfc2397: illegal media type"),
        ("data://text/plain;,hi", "rfc2397: illegal parameter"),
    ] {
        assert!(
            out.diagnostics.contains(&format!(
                "Warning: fopen({url}): Failed to open stream: {reason}"
            )),
            "missing php's reason for {url}, got diagnostics={}",
            out.diagnostics
        );
    }
}

/// Verifies the `data:` scheme opens with or without the `//`, as php-src special-cases it.
///
/// `php_stream_locate_url_wrapper` normally demands `://`, but its test is
/// `!strncmp("//", p+1, 2) || (n == 4 && !memcmp("data:", path, 5))` — `data` is the one scheme
/// exempted. elephc matched `data://` only, so `data:text/plain,hi` went to the FILE opener and
/// reported `No such file or directory`.
///
/// RED before the fix: php `'hi'` / `'Hi'`, elephc `false` twice with a filesystem errno.
/// Both the compile-time literal and the run-time value are covered — they are separate dispatches.
#[test]
fn test_data_scheme_opens_without_the_double_slash() {
    let out = compile_and_run(
        r#"<?php
echo stream_get_contents(fopen("data:text/plain,hi", "r"));
echo "|";
$u = "data:text/plain;base64,SGk=";
echo stream_get_contents(fopen($u, "r"));
echo "|";
echo stream_get_contents(fopen("data://text/plain,slashes", "r"));
"#,
    );
    assert_eq!(out, "hi|Hi|slashes");
}

/// Verifies an unrecognised `php://` target prints php's TWO lines instead of nothing.
///
/// The pair is structural, not decorative. `php_stream_url_wrap_php` reports the first with a
/// DIRECT `php_error_docref`, so it prints at once as `fopen(): …` and leaves the wrapper error
/// stack empty — which is exactly why the generic failed-open line that follows has nothing left
/// to say but `operation failed`. Getting one without the other would be wrong twice over.
///
/// `php://fd/` is the exception and is asserted with them: it goes through
/// `php_stream_wrapper_log_error` like an ordinary wrapper, so it prints ONE line carrying its own
/// sentence. Measured on `php -n` 8.5.6; elephc answered a silent `false` for every case here.
///
/// Both dispatches are covered: a literal URL is refused during lowering, a run-time one inside
/// `__rt_php_wrapper_open`, and the two compose the same text by different means.
#[test]
fn test_unknown_php_target_prints_both_of_phps_lines() {
    let out = compile_and_run_capture(
        r#"<?php
var_dump(fopen("php://bogus", "r"));
$u = "php://foo/bar";
var_dump(fopen($u, "r"));
var_dump(fopen("php://fd/", "r"));
$v = "php://fd/";
var_dump(fopen($v, "r"));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\nbool(false)\nbool(false)\n");
    assert_eq!(
        out.diagnostics.matches("Warning: fopen(): Invalid php:// URL specified").count(),
        2,
        "the direct php_error_docref line is missing or duplicated, got diagnostics={}",
        out.diagnostics
    );
    for url in ["php://bogus", "php://foo/bar"] {
        assert!(
            out.diagnostics.contains(&format!(
                "Warning: fopen({url}): Failed to open stream: operation failed"
            )),
            "missing the failed-open line for {url}, got diagnostics={}",
            out.diagnostics
        );
    }
    assert_eq!(
        out.diagnostics
            .matches(
                "Warning: fopen(php://fd/): Failed to open stream: \
                 php://fd/ stream must be specified in the form php://fd/<orig fd>"
            )
            .count(),
        2,
        "php://fd/ lost its own sentence on one of the two dispatches, got diagnostics={}",
        out.diagnostics
    );
    assert!(
        !out.diagnostics.contains("No such file or directory"),
        "a php:// URL reached the file opener, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies `php://fd/N` says WHY it could not open the descriptor, the three ways php does.
///
/// This is the only diagnostic in php that prints an errno NUMBER in brackets AND its `strerror`
/// text; every other failed open prints the text alone. Measured on `php -n` 8.5.6, with
/// `getdtablesize()` answering 61440 on the measuring host:
///
/// ```text
/// fopen("php://fd/99")     Error duping file descriptor 99; possibly it doesn't exist:
///                          [9]: Bad file descriptor
/// fopen("php://fd/-1")     The file descriptors must be non-negative numbers smaller than 61440
/// fopen("php://fd/61440")  the same sentence: the bound is exclusive
/// fopen("php://fd/abc")    php://fd/ stream must be specified in the form php://fd/<orig fd>
/// ```
///
/// The bound itself is asserted only as a PREFIX: it is `getdtablesize()`, a property of the
/// running process, and the number differs between this host and CI.
///
/// RED before the fix: elephc answered a silent `false` for the first three and reported the
/// filesystem's `No such file or directory` for `abc`, about a path nothing had looked for.
/// Both dispatches are covered — a literal URL is opened during lowering, a run-time one inside
/// `__rt_php_wrapper_open` — because they parse the descriptor by different means.
#[test]
fn test_php_fd_refusals_carry_phps_two_sentences() {
    let out = compile_and_run_capture(
        r#"<?php
var_dump(fopen("php://fd/99", "r"));
var_dump(fopen("php://fd/-1", "r"));
var_dump(fopen("php://fd/abc", "r"));
$a = "php://fd/99";
var_dump(fopen($a, "r"));
$b = "php://fd/-1";
var_dump(fopen($b, "r"));
$c = "php://fd/abc";
var_dump(fopen($c, "r"));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\n".repeat(6));
    assert_eq!(
        out.diagnostics
            .matches(
                "Warning: fopen(php://fd/99): Failed to open stream: \
                 Error duping file descriptor 99; possibly it doesn't exist: \
                 [9]: Bad file descriptor"
            )
            .count(),
        2,
        "the duping refusal is missing on one of the two dispatches, got diagnostics={}",
        out.diagnostics
    );
    assert_eq!(
        out.diagnostics
            .matches(
                "Warning: fopen(php://fd/-1): Failed to open stream: \
                 The file descriptors must be non-negative numbers smaller than "
            )
            .count(),
        2,
        "the range refusal is missing on one of the two dispatches, got diagnostics={}",
        out.diagnostics
    );
    assert_eq!(
        out.diagnostics
            .matches(
                "Warning: fopen(php://fd/abc): Failed to open stream: \
                 php://fd/ stream must be specified in the form php://fd/<orig fd>"
            )
            .count(),
        2,
        "a descriptor that is not a number lost php's form sentence, got diagnostics={}",
        out.diagnostics
    );
    assert!(
        !out.diagnostics.contains("No such file or directory"),
        "a php://fd/ URL reached the file opener, got diagnostics={}",
        out.diagnostics
    );
    assert!(
        !out.diagnostics.contains("Invalid php:// URL specified"),
        "php://fd/abc was reported as an unknown php:// target, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies a `php://fd/N` naming a descriptor that DOES exist still opens, and says nothing.
///
/// The refusal wording above is only correct if it stays off the ordinary path: php duplicates
/// the descriptor and hands the copy out, so writing to `php://fd/1` reaches standard output and
/// no diagnostic is printed. Measured on `php -n` 8.5.6.
#[test]
fn test_php_fd_opens_an_existing_descriptor_quietly() {
    let out = compile_and_run_capture(
        r#"<?php
$h = fopen("php://fd/1", "w");
var_dump($h !== false);
fwrite($h, "literal\n");
$u = "php://fd/1";
$g = fopen($u, "w");
var_dump($g !== false);
fwrite($g, "runtime\n");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nliteral\nbool(true)\nruntime\n");
    assert!(
        !out.diagnostics.contains("Warning"),
        "a descriptor that exists warned about itself, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies `fopen("glob://…")` is refused for the reason php gives, with no filesystem consulted.
///
/// php-src registers `glob` with NO `stream_opener`, so the generic caller reports the absence
/// itself. elephc sent the URL to the file opener, which answered `No such file or directory`
/// about a path nothing had ever looked for — a message that would send a reader hunting for a
/// missing file when the wrapper simply has no such operation.
///
/// RED before the fix: `wrapper does not support stream open` vs `No such file or directory`,
/// on both the literal and the run-time dispatch.
#[test]
fn test_fopen_on_glob_is_refused_by_the_wrapper_not_the_filesystem() {
    let out = compile_and_run_capture(
        r#"<?php
var_dump(fopen("glob://*.php", "r"));
$u = "glob:///tmp/*";
var_dump(fopen($u, "r"));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\n");
    for url in ["glob://*.php", "glob:///tmp/*"] {
        assert!(
            out.diagnostics.contains(&format!(
                "Warning: fopen({url}): Failed to open stream: wrapper does not support stream open"
            )),
            "missing php's reason for {url}, got diagnostics={}",
            out.diagnostics
        );
    }
    assert!(
        !out.diagnostics.contains("No such file or directory"),
        "a glob:// URL reached the file opener, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies a filter name that resolves to nothing is REPORTED, naming the filter.
///
/// Returning `false` silently left a misspelled filter indistinguishable from one that
/// attached — the caller's data simply came through untransformed. php-src names both the
/// function and the filter, and `@` suppresses it like any warning.
#[test]
fn test_stream_filter_attach_warns_and_names_an_unknown_filter() {
    let out = compile_and_run_capture(
        r#"<?php
$h = fopen("php://memory", "w+");
var_dump(stream_filter_append($h, "no.such.filter"));
var_dump(stream_filter_prepend($h, "also.missing"));
var_dump(@stream_filter_append($h, "suppressed.one"));
fclose($h);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\nbool(false)\n");
    assert!(
        out.diagnostics
            .contains("Warning: stream_filter_append(): Unable to locate filter \"no.such.filter\""),
        "missing the append warning, got diagnostics={}",
        out.diagnostics
    );
    assert!(
        out.diagnostics
            .contains("Warning: stream_filter_prepend(): Unable to locate filter \"also.missing\""),
        "missing the prepend warning, got diagnostics={}",
        out.diagnostics
    );
    assert!(
        !out.diagnostics.contains("suppressed.one"),
        "`@` must suppress the warning, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies the CSV family deprecates an OMITTED `$escape`, and only an omitted one.
///
/// PHP 8.5 raises it because 9.0 changes the default from `"\\"` to `""`, which silently
/// changes how existing files parse. It keys on the argument being absent, so passing the
/// default explicitly stays quiet — the count is what pins that: three calls omit it and
/// three pass it, and exactly three notices come out.
#[test]
fn test_csv_family_deprecates_an_omitted_escape_argument() {
    let out = compile_and_run_capture(
        r#"<?php
file_put_contents("dep.csv", "a,b\n");
$r = fopen("dep.csv", "r");
fgetcsv($r);
fgetcsv($r, 0, ",", "\"", "\\");
fclose($r);
$w = fopen("dep_out.csv", "w");
fputcsv($w, ["a"]);
fputcsv($w, ["a"], ",", "\"", "\\");
fclose($w);
str_getcsv("a,b");
str_getcsv("a,b", ",", "\"", "\\");
echo "done";
unlink("dep.csv");
unlink("dep_out.csv");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    let notices = out.diagnostics.matches("the $escape parameter must be provided").count();
    assert_eq!(notices, 3, "expected three notices, got diagnostics={}", out.diagnostics);
    for name in ["fgetcsv", "fputcsv", "str_getcsv"] {
        assert!(
            out.diagnostics
                .contains(&format!("Deprecated: {name}(): the $escape parameter")),
            "missing the {name} notice, got diagnostics={}",
            out.diagnostics
        );
    }
}

/// Verifies the `$escape` deprecation is VERSION-GATED, as the rest of the notice surface is.
///
/// PHP 8.4 introduced it; 8.2 and 8.3 print nothing. elephc emitted it at every
/// `--php-version`, which makes a program built for 8.3 noisier than the interpreter it is
/// asked to imitate. The DIAGNOSTIC stream is what has to be inspected — the notice never
/// reaches the program's own output, so a stdout-only check reads the same for a gate that
/// works and a gate that is missing.
#[test]
fn test_csv_escape_deprecation_is_gated_by_php_version() {
    let source = r#"<?php
$h = fopen("php://memory", "r+");
fputcsv($h, ["a"]);
str_getcsv("a,b");
echo "done";
"#;
    let modern =
        compile_and_run_capture_with_php_version(source, elephc::php_version::PhpVersion::Php84);
    assert!(modern.success, "8.4 run failed: {}", modern.stderr);
    assert_eq!(modern.stdout, "done");
    assert_eq!(
        modern
            .diagnostics
            .matches("the $escape parameter must be provided")
            .count(),
        2,
        "8.4 must still raise both notices, got diagnostics={}",
        modern.diagnostics
    );

    for version in [
        elephc::php_version::PhpVersion::Php82,
        elephc::php_version::PhpVersion::Php83,
    ] {
        let old = compile_and_run_capture_with_php_version(source, version);
        assert!(old.success, "{version:?} run failed: {}", old.stderr);
        assert_eq!(old.stdout, "done");
        assert!(
            !old.diagnostics.contains("$escape parameter"),
            "{version:?} must print nothing, got diagnostics={}",
            old.diagnostics
        );
    }
}

/// Verifies an OMITTED `$escape` writes with `"\\"`, not with RFC 4180 doubling.
///
/// `fgetcsv()` and `str_getcsv()` already defaulted to the backslash; `fputcsv()` defaulted to
/// the zero byte the helper reads as doubling mode, so the very row `fgetcsv()` would read back
/// came out differently depending on whether the argument was spelled. Measured on `php -n`
/// 8.5.6: with an escape in force the quote is NOT doubled, because the escape already
/// neutralizes it. The bytes are compared in hex because the difference is one `"` character.
#[test]
fn test_fputcsv_default_escape_is_the_backslash_not_doubling() {
    let out = compile_and_run(
        r#"<?php
$h = fopen("php://memory", "r+");
$n = fputcsv($h, ['a\\"b']);
rewind($h);
echo bin2hex(stream_get_contents($h)), "|", $n, "\n";
fclose($h);
$h = fopen("php://memory", "r+");
$n = fputcsv($h, ['a\\"b'], ",", "\"", "\\");
rewind($h);
echo bin2hex(stream_get_contents($h)), "|", $n, "\n";
fclose($h);
$h = fopen("php://memory", "r+");
$n = fputcsv($h, ['a\\"b'], ",", "\"", "");
rewind($h);
echo bin2hex(stream_get_contents($h)), "|", $n, "\n";
"#,
    );
    assert_eq!(
        out,
        "22615c2262220a|7\n22615c2262220a|7\n22615c222262220a|8\n",
        "an omitted $escape must write exactly what the explicit backslash writes"
    );
}

/// Verifies `str_getcsv()`'s omitted `$escape` is the backslash the manual documents.
///
/// The lowering pushed a zero byte for every absent control and let the runtime pick, which is
/// right for the separator and the enclosure and WRONG for the escape: zero is doubling mode
/// there, php's 9.0 default, not today's `"\\"`.
#[test]
fn test_str_getcsv_default_escape_matches_the_explicit_backslash() {
    let out = compile_and_run(
        r#"<?php
$s = "\"a\\\"b\",c";
echo json_encode(str_getcsv($s)), "|", json_encode(str_getcsv($s, ",", "\"", "\\")), "\n";
"#,
    );
    let (omitted, explicit) = out.trim_end().split_once('|').expect("two records");
    assert_eq!(
        omitted, explicit,
        "an omitted $escape must parse exactly like the explicit backslash"
    );
}

/// Verifies an EMPTY `$eol` writes no terminator, while an ABSENT one still writes `"\n"`.
///
/// Measured on `php -n` 8.5.6: `fputcsv($h, ["a", "b"], ",", '"', "\\", "")` answers 3 and
/// leaves `a,b`; omitting the argument answers 4 and leaves `a,b\n`. A zero LENGTH cannot tell
/// the two apart, so the helper used to substitute the newline for both.
#[test]
fn test_fputcsv_empty_eol_writes_no_terminator() {
    let out = compile_and_run(
        r#"<?php
$h = fopen("php://memory", "r+");
$n = fputcsv($h, ["a", "b"], ",", "\"", "\\", "");
rewind($h);
echo bin2hex(stream_get_contents($h)), "|", $n, "\n";
fclose($h);
$h = fopen("php://memory", "r+");
$n = fputcsv($h, ["a", "b"], ",", "\"", "\\");
rewind($h);
echo bin2hex(stream_get_contents($h)), "|", $n, "\n";
fclose($h);
$h = fopen("php://memory", "r+");
$n = fputcsv($h, ["a", "b"], ",", "\"", "\\", "\r\n");
rewind($h);
echo bin2hex(stream_get_contents($h)), "|", $n, "\n";
"#,
    );
    assert_eq!(out, "612c62|3\n612c620a|4\n612c620d0a|5\n");
}

/// Verifies every CSV control argument raises php-src's own `ValueError` unless it is one byte.
///
/// elephc read the first byte and dropped the rest in silence, so `fgetcsv($h, 0, "::")` parsed
/// on `:`; an EMPTY separator or enclosure quietly selected the default. php rejects all of
/// them, and only `$escape` accepts the empty string. Each function names its OWN argument
/// position, which is why one rule cannot cover the three: the reader counts a `$length` first.
/// Every message below is `php -n` 8.5.6 verbatim.
#[test]
fn test_csv_controls_must_be_a_single_character() {
    let out = compile_and_run(
        r#"<?php
function t(callable $c): void {
    try { $c(); echo "NO-THROW\n"; }
    catch (ValueError $e) { echo $e->getMessage(), "\n"; }
}
$r = fopen("php://memory", "r+");
fwrite($r, "a,b,c\n");
rewind($r);
$w = fopen("php://memory", "r+");
t(fn() => str_getcsv("a,b", ",,", "\"", "\\"));
t(fn() => str_getcsv("a,b", "", "\"", "\\"));
t(fn() => str_getcsv("a,b", ",", "''", "\\"));
t(fn() => str_getcsv("a,b", ",", "", "\\"));
t(fn() => str_getcsv("a,b", ",", "\"", "\\\\"));
t(fn() => fgetcsv($r, 0, ",,", "\"", "\\"));
t(fn() => fgetcsv($r, 0, ",", "", "\\"));
t(fn() => fgetcsv($r, 0, ",", "\"", "ab"));
t(fn() => fputcsv($w, ["a"], ",,", "\"", "\\"));
t(fn() => fputcsv($w, ["a"], ",", "", "\\"));
t(fn() => fputcsv($w, ["a"], ",", "\"", "\\\\"));
echo json_encode(str_getcsv("a,b", ",", "\"", "")), "\n";
"#,
    );
    assert_eq!(
        out,
        "str_getcsv(): Argument #2 ($separator) must be a single character\n\
         str_getcsv(): Argument #2 ($separator) must be a single character\n\
         str_getcsv(): Argument #3 ($enclosure) must be a single character\n\
         str_getcsv(): Argument #3 ($enclosure) must be a single character\n\
         str_getcsv(): Argument #4 ($escape) must be empty or a single character\n\
         fgetcsv(): Argument #3 ($separator) must be a single character\n\
         fgetcsv(): Argument #4 ($enclosure) must be a single character\n\
         fgetcsv(): Argument #5 ($escape) must be empty or a single character\n\
         fputcsv(): Argument #3 ($separator) must be a single character\n\
         fputcsv(): Argument #4 ($enclosure) must be a single character\n\
         fputcsv(): Argument #5 ($escape) must be empty or a single character\n\
         [\"a\",\"b\"]\n"
    );
}

/// Verifies `str_getcsv()` parses one record, with a newline as DATA rather than a break.
///
/// It is not `fgetcsv()` over a line, and the difference is not obvious: only a trailing
/// newline is structural, and php-src strips one in two separate places. `"a\nb"` is one
/// field containing a newline; `"a,b\n\n"` still yields two fields because both trailing
/// newlines go. The expectations come from `php -n` 8.5.6.
#[test]
fn test_str_getcsv_treats_an_interior_newline_as_data() {
    let out = compile_and_run(
        r#"<?php
$cases = ["a,b,\"c,d\"", "a,\"b\"\"c\",d", "a\nb", "a,b\n", "a,b\n\n", "\na,b", " \n", "a,b\r\n"];
foreach ($cases as $c) { echo json_encode(str_getcsv($c, ",", "\"", "\\")), "|"; }
"#,
    );
    assert_eq!(
        out,
        "[\"a\",\"b\",\"c,d\"]|[\"a\",\"b\\\"c\",\"d\"]|[\"a\\nb\"]|[\"a\",\"b\"]|[\"a\",\"b\"]|[\"\\na\",\"b\"]|[\" \"]|[\"a\",\"b\"]|"
    );
}

/// Verifies `str_getcsv()` answers the same through `eval()` as it does compiled.
#[test]
fn test_str_getcsv_matches_between_compiled_and_eval() {
    let out = compile_and_run(
        r#"<?php
echo json_encode(str_getcsv("a,\"b,c\",d", ",", "\"", "\\")), "|";
eval('echo json_encode(str_getcsv("a,\"b,c\",d", ",", "\"", "\\\\"));');
"#,
    );
    assert_eq!(out, "[\"a\",\"b,c\",\"d\"]|[\"a\",\"b,c\",\"d\"]");
}

/// Verifies a quoted CSV field may span newlines, as one field of one record.
///
/// The reader took one line at a time, so `1,"line one\nline two"` came back as two
/// records with the field cut in half and a stray quote left on the second — silent
/// corruption of a legal, common export shape. The record count is what pins it: a test
/// that only inspected the first row saw nothing wrong.
#[test]
fn test_fgetcsv_continues_a_quoted_field_across_newlines() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ml.csv", "id,note\n1,\"line one\nline two\"\n2,plain\n");
$f = fopen("ml.csv", "r");
$rows = 0;
$note = "";
while (($row = fgetcsv($f, 0, ",", "\"", "\\")) !== false) {
    $rows = $rows + 1;
    if ($rows > 8) { echo "RUNAWAY"; break; }
    if ($rows == 2) { $note = $row[1]; }
}
fclose($f);
echo $rows, "|", strlen($note), "|", $note;
unlink("ml.csv");
"#,
    );
    assert_eq!(out, "3|17|line one\nline two");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `fputcsv()` doubles an embedded enclosure instead of backslash-escaping it.
///
/// elephc wrote `"with\"quote"` where PHP writes `"with""quote"` — not valid CSV, and PHP
/// itself reads it back as a different value. php-src also tracks whether the escape
/// character shielded the enclosure: `back\"quote` keeps its single quote rather than
/// gaining a doubled one, and the escape character is never doubled on output. The whole
/// existing fputcsv suite passed either way, because none of it wrote an embedded quote.
#[test]
fn test_fputcsv_doubles_an_embedded_enclosure() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = fopen("fp_dq.csv", "w");
fputcsv($h, ["with\"quote"], ",", "\"", "\\");
fputcsv($h, ["a\"b\"c"], ",", "\"", "\\");
fputcsv($h, ["back\\slash"], ",", "\"", "\\");
fputcsv($h, ["back\\\"shielded"], ",", "\"", "\\");
fclose($h);
echo file_get_contents("fp_dq.csv");
unlink("fp_dq.csv");
"#,
    );
    assert_eq!(
        out,
        "\"with\"\"quote\"\n\"a\"\"b\"\"c\"\n\"back\\slash\"\n\"back\\\"shielded\"\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `SplFileObject::fgetcsv()` still yields strings after `fgetcsv()` began boxing.
///
/// The SPL method body is synthesized, so it has no checked call-site type and takes the
/// EIR fallback instead. While that fallback still claimed `array<string>`, the boxed
/// `array|false` cell was read as a raw array pointer and every field came back as an
/// integer — a silent corruption no `fgetcsv()` test could see.
#[test]
fn test_spl_file_object_fgetcsv_reads_fields_not_pointers() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("spl_csv.csv", "a,b\nc,d\n");
$f = new SplFileObject("spl_csv.csv");
$seen = "";
while (!$f->eof()) {
    $row = $f->fgetcsv(",", "\"", "\\");
    if ($row === false) { break; }
    foreach ($row as $field) { $seen = $seen . $field; }
}
unset($f);
echo $seen;
unlink("spl_csv.csv");
"#,
    );
    assert_eq!(out, "abcd");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a refused write reports failure rather than its errno.
///
/// macOS returns a failed `write` as the POSITIVE errno with the carry flag set, which is
/// indistinguishable from a byte count: writing to a read-only handle answered `int(9)`
/// — EBADF — where PHP answers `false`. Asserting on the exact value matters, because
/// `9` is truthy and every `if (fwrite(...))` guard read it as success.
#[test]
fn test_fwrite_to_a_read_only_stream_reports_false() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("fw_ro.txt", "seed");
$h = fopen("fw_ro.txt", "r");
var_dump(@fwrite($h, "XY"));
fclose($h);
echo file_get_contents("fw_ro.txt");
unlink("fw_ro.txt");
"#,
    );
    assert_eq!(out, "bool(false)\nseed");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `stream_is_local()` classifies a path that only exists at run time.
///
/// A literal is folded at compile time, so the loop is what exercises the runtime
/// classifier — before it existed this failed to compile rather than answering wrongly.
/// The expectations are `php -n` 8.5.6's: `data:` is remote with or without slashes,
/// scheme matching folds case, and the scheme needs its full `://`.
#[test]
fn test_stream_is_local_classifies_a_runtime_path() {
    let out = compile_and_run(
        r#"<?php
$cases = [
    "plain.txt", "/etc/hosts", "file:///etc/hosts",
    "http://example.com/x", "https://example.com/x",
    "ftp://example.com/x", "ftps://example.com/x",
    "php://memory", "glob://*.txt", "phar://a.phar/b.txt",
    "compress.zlib://a.gz", "data://text/plain,hello", "data:text/plain,hello",
    "HTTP://example.com/x", "hTTps://example.com", "FTP://x",
    "httpx://x", "http:/one-slash", "http", "my.http://x", "",
];
foreach ($cases as $c) { echo stream_is_local($c) ? "L" : "r"; }
"#,
    );
    assert_eq!(out, "LLLrrrrLLLLrrrrrLLLLL");
}

/// Verifies `stream_supports_lock()` answers per wrapper rather than always true.
///
/// php-src answers from the stream's ops: a descriptor-backed stream carries the lock
/// option, the memory and output wrappers do not. elephc answered a blanket `true`, which
/// told a caller that `flock()` on `php://memory` would serialise something. A descriptor
/// test cannot decide it, because elephc backs `php://memory` with a real temporary file.
#[test]
fn test_stream_supports_lock_is_false_for_the_memory_wrappers() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("lk.txt", "x");
echo stream_supports_lock(fopen("lk.txt", "r")) ? "L" : "n";
echo stream_supports_lock(fopen("php://memory", "w+")) ? "L" : "n";
echo stream_supports_lock(fopen("php://temp", "w+")) ? "L" : "n";
echo stream_supports_lock(fopen("php://output", "w")) ? "L" : "n";
echo stream_supports_lock(fopen("php://stdout", "w")) ? "L" : "n";
echo stream_supports_lock(tmpfile()) ? "L" : "n";
echo stream_supports_lock(STDIN) ? "L" : "n";
unlink("lk.txt");
"#,
    );
    assert_eq!(out, "LnnnLLL");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream get wrappers lists known wrappers.
#[test]
fn test_stream_get_wrappers_lists_known_wrappers() {
    // php's registration order, measured on php 8.5.6 and frozen in
    // `tests/php_oracle/manifests/streams`:
    //   https, ftps, compress.zlib, compress.bzip2, php, file, glob, data,
    //   http, ftp, phar, zip
    // php's probe reads `12:https,compress.bzip2,file` and so does elephc's: `zip` is now
    // really readable through the elephc-phar bridge, so advertising it is no longer a lie.
    // This assertion read `11:https,compress.bzip2,file` while the wrapper was missing, and
    // before that the list started at `file` and the probe read `11:file,ftp,https`.
    let out = compile_and_run(
        r#"<?php $w = stream_get_wrappers(); echo count($w) . ":" . $w[0] . "," . $w[3] . "," . $w[5] . "," . $w[11];"#,
    );
    assert_eq!(out, "12:https,compress.bzip2,file,zip");
}

/// Verifies compiled PHP output for stream get transports and filters.
#[test]
fn test_stream_get_transports_and_filters() {
    // The transport list is php-src's exactly: ten entries, tlsv1.0/1.1/1.2/1.3 routing
    // through the same enable_crypto path. `sslv2`/`sslv3` used to be listed and are not
    // any more — PHP 8.5.6 does not publish them and the protocols are dead.
    //
    // The filter list is now php's too: php publishes nine FAMILIES (`zlib.*`,
    // `bzip2.*`, `convert.*`, `convert.iconv.*`) rather than the concrete names
    // behind them. Publishing fourteen concrete names both over-promised
    // (`string.strip_tags` has not existed since php 8.0) and mis-shaped the
    // list. Measured `10,9`; this assertion read `10,14`.
    let out = compile_and_run(
        r#"<?php echo count(stream_get_transports()) . "," . count(stream_get_filters());"#,
    );
    assert_eq!(out, "10,9");
}

/// Verifies the published filter list matches php's families, in php's order.
#[test]
fn test_stream_get_filters_publishes_php_families_in_order() {
    // `php -n -r 'var_export(stream_get_filters());'` on 8.5.6.
    let out = compile_and_run(r#"<?php echo implode(",", stream_get_filters());"#);
    assert_eq!(
        out,
        "zlib.*,bzip2.*,convert.iconv.*,string.rot13,string.toupper,string.tolower,convert.*,consumed,dechunk"
    );
}

/// Verifies compiled PHP output for stream filter rot13 on read.
#[test]
fn test_stream_filter_rot13_on_read() {
    // A read-direction filter transforms bytes as they leave the stream.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
fwrite($m, "Hello World");
rewind($m);
stream_filter_append($m, "string.rot13", STREAM_FILTER_READ);
echo fread($m, 32);
fclose($m);
"#,
    );
    assert_eq!(out, "Uryyb Jbeyq");
}

/// Verifies compiled PHP output for stream filter toupper on write.
#[test]
fn test_stream_filter_toupper_on_write() {
    // A write-direction filter transforms bytes as they enter the stream.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
stream_filter_append($m, "string.toupper", STREAM_FILTER_WRITE);
fwrite($m, "written lower");
rewind($m);
echo fread($m, 32);
fclose($m);
"#,
    );
    assert_eq!(out, "WRITTEN LOWER");
}

/// Verifies compiled PHP output for php filter read toupper over temp.
#[test]
fn test_php_filter_read_toupper_over_temp() {
    // php://filter/read=F/resource=R opens R and attaches F to the read side.
    let out = compile_and_run(
        r#"<?php
$f = fopen("php://filter/read=string.toupper/resource=php://temp", "r+");
fwrite($f, "hello temp");
rewind($f);
echo fread($f, 64);
fclose($f);
"#,
    );
    assert_eq!(out, "HELLO TEMP");
}

/// Verifies compiled PHP output for php filter write rot13 over temp.
#[test]
fn test_php_filter_write_rot13_over_temp() {
    // php://filter/write=F transforms bytes as they enter the stream; reading
    // back raw (no filter) shows the rot13-encoded payload.
    let out = compile_and_run(
        r#"<?php
$f = fopen("php://filter/write=string.rot13/resource=php://temp", "r+");
fwrite($f, "hello");
rewind($f);
echo fread($f, 64);
fclose($f);
"#,
    );
    assert_eq!(out, "uryyb");
}

/// Verifies compiled PHP output for php filter bare filter applies to read.
#[test]
fn test_php_filter_bare_filter_applies_to_read() {
    // A bare filter (no read=/write=) is STREAM_FILTER_ALL, so it applies on read.
    let out = compile_and_run(
        r#"<?php
$f = fopen("php://filter/string.toupper/resource=php://temp", "r+");
fwrite($f, "both ways");
rewind($f);
echo fread($f, 64);
fclose($f);
"#,
    );
    assert_eq!(out, "BOTH WAYS");
}

/// Verifies compiled PHP output for php filter unknown filter returns unfiltered stream.
#[test]
fn test_php_filter_unknown_filter_returns_unfiltered_stream() {
    // PHP emits a warning but still returns the unfiltered stream for an unknown
    // filter (not false); reads pass through untransformed.
    let out = compile_and_run(
        r#"<?php
$f = fopen("php://filter/read=nope.bad/resource=php://temp", "r+");
echo ($f === false) ? "false" : "resource";
fwrite($f, "raw bytes");
rewind($f);
echo "|" . fread($f, 64);
fclose($f);
"#,
    );
    assert_eq!(out, "resource|raw bytes");
}

/// Verifies a FAILED filtered open names the whole filter URL, not the resource inside it.
///
/// elephc opened the resource the URL wrapped and let THAT opener warn, so the message named a
/// path the program never wrote and a reason php never gives —
/// `Warning: fopen(absent_abc.txt): Failed to open stream: No such file or directory`.
/// `php -n` 8.5.6 prints, for the same call, `Warning:
/// fopen(php://filter/read=string.toupper/resource=absent_abc.txt): Failed to open stream:
/// operation failed`.
/// php-src's `php_stream_url_wrap_php` returns NULL the moment the inner open fails, so the
/// generic caller composes the line from the URL it was HANDED and the wrapper's fixed reason —
/// the inner errno never reaches the user. The write direction is probed too because the two
/// spellings take different openers underneath and only one of them was ever measured.
#[test]
fn test_failed_filter_open_names_the_url_not_the_wrapped_resource() {
    let out = compile_and_run_capture(
        r#"<?php
$a = fopen("php://filter/read=string.toupper/resource=absent_abc.txt", "r");
var_dump($a);
$b = @fopen("php://filter/read=string.toupper/resource=absent_abc.txt", "r");
var_dump($b);
$c = fopen("php://filter/write=string.rot13/resource=missing_dir_abc/out.txt", "w");
var_dump($c);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\nbool(false)\n");
    assert_eq!(
        out.diagnostics,
        "Warning: fopen(php://filter/read=string.toupper/resource=absent_abc.txt): \
         Failed to open stream: operation failed\n\
         Warning: fopen(php://filter/write=string.rot13/resource=missing_dir_abc/out.txt): \
         Failed to open stream: operation failed\n",
        "php's wording and the WHOLE URL; `@` still silences it like any other warning"
    );
}

/// Verifies an unknown `php://filter` name warns TWICE and still hands back the stream.
///
/// elephc resolved the chain, quietly dropped the name it did not know and said nothing, so a
/// typo in a filter name became a silently unfiltered read. `php -n` 8.5.6 prints two lines per
/// failed creation — `php_stream_filter_create` cannot locate it, then
/// `php_stream_apply_filter_list` cannot create it — and neither cancels the open:
///
/// ```text
/// Warning: fopen(): Unable to locate filter "no.such.filter"
/// Warning: fopen(): Unable to create filter (no.such.filter)
/// resource(5) of type (stream)
/// ```
///
/// Four things beyond the bare pair are pinned, each measured rather than reasoned about:
/// - the chain CONTINUES, so `one.bad|string.toupper|two.bad` still uppercases and warns for
///   both unknown names, in chain order;
/// - a name with no `read=`/`write=` prefix is tried once per DIRECTION the mode names, so the
///   same URL warns twice over on `r+` and NOT AT ALL on `x`, which names neither;
/// - `@` silences the pair, since it goes through the same depth counter as every warning;
/// - a FAILED open never reaches the filters, so it prints the failed-open line ALONE — php
///   returns NULL before a single filter is created.
#[test]
fn test_unknown_php_filter_name_warns_twice_and_keeps_the_stream() {
    let out = compile_and_run_capture(
        r#"<?php
$a = fopen("php://filter/read=no.such.filter/resource=data://text/plain,hi", "r");
echo is_resource($a) ? "resource" : "false", "|", stream_get_contents($a), "|";
$b = fopen("php://filter/read=one.bad|string.toupper|two.bad/resource=data://text/plain,hi", "r");
echo stream_get_contents($b), "|";
$c = fopen("php://filter/only.bad/resource=php://temp", "r+");
echo is_resource($c) ? "resource" : "false", "|";
$d = fopen("php://filter/only.bad/resource=php://temp", "x");
echo is_resource($d) ? "resource" : "false", "|";
$e = @fopen("php://filter/read=quiet.bad/resource=php://temp", "r");
echo is_resource($e) ? "resource" : "false", "|";
$f = fopen("php://filter/read=never.reached/resource=absent_abc.txt", "r");
var_dump($f);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "resource|hi|HI|resource|resource|resource|bool(false)\n"
    );
    assert_eq!(
        out.diagnostics,
        "Warning: fopen(): Unable to locate filter \"no.such.filter\"\n\
         Warning: fopen(): Unable to create filter (no.such.filter)\n\
         Warning: fopen(): Unable to locate filter \"one.bad\"\n\
         Warning: fopen(): Unable to create filter (one.bad)\n\
         Warning: fopen(): Unable to locate filter \"two.bad\"\n\
         Warning: fopen(): Unable to create filter (two.bad)\n\
         Warning: fopen(): Unable to locate filter \"only.bad\"\n\
         Warning: fopen(): Unable to create filter (only.bad)\n\
         Warning: fopen(): Unable to locate filter \"only.bad\"\n\
         Warning: fopen(): Unable to create filter (only.bad)\n\
         Warning: fopen(php://filter/read=never.reached/resource=absent_abc.txt): \
         Failed to open stream: operation failed\n",
        "two lines per failed creation, once per applied direction, never on a failed open"
    );
}

/// Verifies compiled PHP output for fprintf formats and writes to stream.
#[test]
fn test_fprintf_formats_and_writes_to_stream() {
    // fprintf = sprintf + fwrite: it formats the arguments and writes the result
    // to the stream, returning the byte count.
    let out = compile_and_run(
        r#"<?php
$f = fopen("php://temp", "r+");
$n = fprintf($f, "%s=%d (%.2f)", "x", 42, 3.14159);
rewind($f);
echo "n=$n|[" . stream_get_contents($f) . "]";
fclose($f);
"#,
    );
    assert_eq!(out, "n=11|[x=42 (3.14)]");
}

/// Verifies compiled PHP output for fscanf float via shared sscanf engine.
#[test]
fn test_fscanf_float_via_shared_sscanf_engine() {
    // fscanf shares the injected scanf prelude, so %f must work through it too.
    let out = compile_and_run(
        r#"<?php
$g = fopen("php://temp", "r+");
fwrite($g, "9.99\n");
rewind($g);
$row = fscanf($g, "%f");
echo $row[0];
fclose($g);
"#,
    );
    assert_eq!(out, "9.99");
}

/// Verifies compiled PHP output for fscanf reads and parses line by line.
#[test]
fn test_fscanf_reads_and_parses_line_by_line() {
    // fscanf reads one line per call and parses it with the sscanf engine,
    // returning the matched fields as an array (2-argument form).
    let out = compile_and_run(
        r#"<?php
$g = fopen("php://temp", "r+");
fwrite($g, "alice 30\nbob 25\n");
rewind($g);
$r1 = fscanf($g, "%s %d");
echo $r1[0] . "=" . $r1[1] . "|";
$r2 = fscanf($g, "%s %d");
echo $r2[0] . "=" . $r2[1];
fclose($g);
"#,
    );
    assert_eq!(out, "alice=30|bob=25");
}

/// Verifies compiled PHP output for fprintf inside function returns int.
#[test]
fn test_fprintf_inside_function_returns_int() {
    // Exercises local-type inference: the fprintf result assigned to a local
    // inside a function must be an 8-byte Int slot (not a 16-byte str slot).
    let out = compile_and_run(
        r#"<?php
function emit($f): int { $n = fprintf($f, "[%d]", 7); return $n; }
$f = fopen("php://temp", "r+");
$c = emit($f);
rewind($f);
echo $c . ":" . stream_get_contents($f);
fclose($f);
"#,
    );
    assert_eq!(out, "3:[7]");
}

/// Verifies compiled PHP output for stream filter prepend and remove.
#[test]
fn test_stream_filter_prepend_and_remove() {
    // stream_filter_prepend attaches a filter; stream_filter_remove drops that one
    // filter and leaves the rest of the chain attached.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
stream_filter_prepend($m, "string.tolower", STREAM_FILTER_READ);
fwrite($m, "FIRST PASS");
rewind($m);
echo fread($m, 32);
echo "|";
$f = stream_filter_append($m, "string.rot13", STREAM_FILTER_READ);
stream_filter_remove($f);
rewind($m);
echo fread($m, 32);
fclose($m);
"#,
    );
    // The prepended `string.tolower` survives removing the appended `string.rot13`,
    // so the second read is still lowercased. The previous expectation of
    // "FIRST PASS" encoded the old two-slot table, whose removal cleared every
    // slot on the descriptor and so detached unrelated filters. Verified against
    // the PHP 8.5.6 CLI, which prints "first pass|first pass".
    assert_eq!(out, "first pass|first pass");
}

/// Verifies compiled PHP output for stream filter zlib deflate compresses.
#[test]
fn test_stream_filter_zlib_deflate_compresses() {
    // The zlib.deflate write filter deflate-compresses data into the stream;
    // the compressed output is non-empty and shorter than the input.
    let out = compile_and_run(
        r#"<?php
$w = fopen("zlib_filter_out.bin", "w");
stream_filter_append($w, "zlib.deflate", STREAM_FILTER_WRITE);
$data = str_repeat("stream filter compression test ", 30);
fwrite($w, $data);
fclose($w);
$packed = file_get_contents("zlib_filter_out.bin");
echo (strlen($packed) > 0 && strlen($packed) < strlen($data)) ? "compressed" : "FAIL";
"#,
    );
    assert_eq!(out, "compressed");
}

/// Verifies compiled PHP output for compress zlib wrapper round trips through deflate.
#[test]
fn test_compress_zlib_wrapper_round_trips_through_deflate() {
    // compress.zlib:// opens a file and attaches the zlib.inflate read filter
    // so subsequent reads see decompressed bytes. Pairs with zlib.deflate
    // write to round-trip a payload through the filesystem.
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$w = fopen("czlib_rt.bin", "w");
stream_filter_append($w, "zlib.deflate", STREAM_FILTER_WRITE);
fwrite($w, "elephc compress.zlib round-trip payload");
fclose($w);
$r = fopen("compress.zlib://czlib_rt.bin", "r");
echo stream_get_contents($r);
fclose($r);
"#,
    );
    assert_eq!(out, "elephc compress.zlib round-trip payload");
    let _ = fs::remove_dir_all(&dir);
}

/// `compress.zlib://` was READ-ONLY: an open in `w` mode silently wrote plain bytes.
///
/// php's wrapper is `gzopen`-backed and writes in BOTH directions. `fopen(..., 'w')` +
/// `fwrite()` produces a real GZIP member — header, deflate body, CRC/ISIZE trailer — that
/// `gzdecode()` and `gunzip` both read. elephc opened the underlying file read-only whatever the
/// mode said, attached the DEcompressor, and let the writes through untouched, so the file was
/// never compressed at all and the `.gz` name was a lie.
///
/// MEASURED on `php -n` 8.5.6, writing `"abc"` through the wrapper:
///
/// ```text
/// bin2hex(substr($raw, 0, 4))     1f8b0800
/// bin2hex(substr($raw, 9, 1))     13          <- the OS byte, PLATFORM-dependent
/// bin2hex(substr($raw, 10))       4a4c4a06000000ffff0300c241243503000000
/// strlen($raw)                    29
/// gzinflate(substr($raw, 10, -8)) "abc"
/// fwrite("ab") + fwrite("c")      byte-identical to the single write
/// fopen("compress.zlib://…","r+") false
/// fopen("compress.zlib://…","x")  false
/// ```
///
/// The assertion deliberately skips bytes 4..10. Four of them are MTIME and one is zlib's
/// `OS_CODE`, which is `0x13` on Apple and `0x03` on Linux — pinning the whole header would pass
/// on the macOS shards and fail on the x86 ones for a reason that has nothing to do with this
/// change. Everything that carries meaning is pinned: the magic, the deflate body, and the
/// CRC32/ISIZE trailer.
///
/// The body hex is worth reading. `4a4c4a0600` is `gzdeflate("abc")` with BFINAL clear (`0x4a`
/// where `gzdeflate` has `0x4b`), then `0000ffff` is a `Z_SYNC_FLUSH` marker, then `0300` is the
/// empty final block from `Z_FINISH`. php's wrapper flushes twice like that, which is why its
/// output is six bytes longer than `gzencode()` of the same payload — and why the close helper
/// grew a sync pass that the `zlib.deflate` FILTER must not have. The filter's own output stays
/// `4b4c4a0600`, measured, and its test above still pins it.
///
/// The mode rule is php's own: the wrapper reads the FIRST character only and refuses any `+`,
/// so `rw` READS and `x`/`c` are refused outright.
///
/// The read half had to move for the round trip to close: elephc's attach inflated with raw
/// windowBits, which cannot read a gzip header, so a file this very test writes was unreadable
/// through the wrapper that wrote it. The attach now picks its framing from the payload's two
/// magic bytes, which keeps the `zlib.deflate` pairing above working unchanged.
#[test]
fn test_compress_zlib_wrapper_writes_a_real_gzip_member() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = fopen("compress.zlib://czw.gz", "w");
var_dump($h !== false);
var_dump(fwrite($h, "abc"));
fclose($h);
$raw = file_get_contents("czw.gz");
echo "head=", bin2hex(substr($raw, 0, 4)), "\n";
echo "body=", bin2hex(substr($raw, 10)), "\n";
var_dump(strlen($raw));
var_dump(gzinflate(substr($raw, 10, -8)) === "abc");

$m = fopen("compress.zlib://czm.gz", "w");
fwrite($m, "ab");
fwrite($m, "c");
fclose($m);
var_dump(file_get_contents("czm.gz") === $raw);

$r = fopen("compress.zlib://czw.gz", "r");
var_dump(stream_get_contents($r) === "abc");
fclose($r);

var_dump(@fopen("compress.zlib://czw.gz", "r+"));
var_dump(@fopen("compress.zlib://czx.gz", "x"));
unlink("czw.gz");
unlink("czm.gz");
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "int(3)\n",
            "head=1f8b0800\n",
            "body=4a4c4a06000000ffff0300c241243503000000\n",
            "int(29)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(false)\n",
            "bool(false)\n",
        ),
        "the wrapper writes php's own gzip bytes, and reads them back through itself"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The `compress.zlib://` wrapper ignored the stream context's `zlib.level`.
///
/// php reads it in `ext/zlib/zlib_fopen_wrapper.c` and hands it straight to `deflateInit2_`, so
/// the option is observable in the output SIZE. MEASURED on `php -n` 8.5.6 over
/// `str_repeat("The quick brown fox jumps over the lazy dog. ", 200)`:
///
/// ```text
/// zlib.level => 1     147 bytes
/// zlib.level => 9     113 bytes
/// no context          113 bytes    (Z_DEFAULT_COMPRESSION, -1, which is level 6's tree here)
/// ```
///
/// The level is only knowable at RUN time: `stream_context_create(['zlib' => ['level' => 9]])`
/// builds a live hash that the compiler never reads, and the `$context` reaching
/// `file_put_contents()` is a variable. The opener walks the context and publishes the answer to
/// `_zlib_wrapper_level`, which the inline deflate initialization loads.
///
/// An out-of-range level is a DELIBERATE divergence: php passes it through, `deflateInit2_`
/// refuses it, and the stream then writes nothing at all (measured: `level => 12` leaves a
/// 0-byte file and `fwrite()` answers 0). elephc's deflate helpers loop until zlib consumes
/// their input, so an uninitialized stream would spin forever instead of writing zero bytes.
/// Clamping to -1..9 keeps the absurd input producing a correct file; every level php accepts
/// passes through untouched, which is what the two sizes above prove.
#[test]
fn test_compress_zlib_wrapper_honours_the_context_zlib_level() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$data = str_repeat("The quick brown fox jumps over the lazy dog. ", 200);
$fast = stream_context_create(["zlib" => ["level" => 1]]);
$best = stream_context_create(["zlib" => ["level" => 9]]);

$h = fopen("compress.zlib://lvl1.gz", "w", false, $fast);
fwrite($h, $data);
fclose($h);
$h = fopen("compress.zlib://lvl9.gz", "w", false, $best);
fwrite($h, $data);
fclose($h);
$h = fopen("compress.zlib://lvld.gz", "w");
fwrite($h, $data);
fclose($h);

var_dump(filesize("lvl1.gz"));
var_dump(filesize("lvl9.gz"));
var_dump(filesize("lvld.gz"));

$r = fopen("compress.zlib://lvl1.gz", "r");
var_dump(stream_get_contents($r) === $data);
fclose($r);
$r = fopen("compress.zlib://lvl9.gz", "r");
var_dump(stream_get_contents($r) === $data);
fclose($r);

unlink("lvl1.gz"); unlink("lvl9.gz"); unlink("lvld.gz");
"#,
    );
    assert_eq!(
        out,
        concat!(
            "int(147)\n",
            "int(113)\n",
            "int(113)\n",
            "bool(true)\n",
            "bool(true)\n",
        ),
        "level 1 and level 9 disagree by php's own byte counts, and both still read back"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `file_put_contents("compress.zlib://out.gz", …)` created a file NAMED after the URL.
///
/// The one-shot writer never recognised the scheme, so the wrapper prefix became part of the
/// filename and the bytes landed uncompressed. php opens the wrapper, deflates through it and
/// closes, answering the INPUT byte count — not the compressed one.
///
/// MEASURED on `php -n` 8.5.6:
///
/// ```text
/// file_put_contents("compress.zlib://fpc.gz", $data)   1175   <- strlen($data)
/// bin2hex(substr($raw, 0, 4))                          1f8b0800
/// gzinflate(substr($raw, 10, -8)) === $data            true
/// stream_get_contents(fopen("compress.zlib://…","r"))  === $data
/// zlib.level => 1 / => 9                               147 / 113 bytes
/// file_put_contents("compress.zlib://nodir/x.gz", "x") false
/// ```
///
/// The route deliberately reuses the `fopen()` wrapper open rather than growing a second
/// compressor: the framing, the context's `zlib.level` and the sync-flushed tail all come from
/// one place, so the two entry points cannot drift apart.
#[test]
fn test_file_put_contents_writes_through_the_compress_zlib_wrapper() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$data = str_repeat("elephc file_put_contents through compress.zlib\n", 25);
var_dump(file_put_contents("compress.zlib://fpc.gz", $data));
$raw = file_get_contents("fpc.gz");
echo "head=", bin2hex(substr($raw, 0, 4)), "\n";
var_dump(strlen($raw) < strlen($data));
var_dump(gzinflate(substr($raw, 10, -8)) === $data);
$r = fopen("compress.zlib://fpc.gz", "r");
var_dump(stream_get_contents($r) === $data);
fclose($r);

$ctx1 = stream_context_create(["zlib" => ["level" => 1]]);
$ctx9 = stream_context_create(["zlib" => ["level" => 9]]);
$big = str_repeat("The quick brown fox jumps over the lazy dog. ", 200);
file_put_contents("compress.zlib://f1.gz", $big, 0, $ctx1);
file_put_contents("compress.zlib://f9.gz", $big, 0, $ctx9);
var_dump(filesize("f1.gz"));
var_dump(filesize("f9.gz"));
var_dump(@file_put_contents("compress.zlib://nodir/x.gz", "x"));
unlink("fpc.gz"); unlink("f1.gz"); unlink("f9.gz");
"#,
    );
    assert_eq!(
        out,
        concat!(
            "int(1175)\n",
            "head=1f8b0800\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "int(147)\n",
            "int(113)\n",
            "bool(false)\n",
        ),
        "the one-shot writer now goes through the wrapper, level and all"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for compress bzip2 wrapper decompresses file.
#[test]
fn test_compress_bzip2_wrapper_decompresses_file() {
    // compress.bzip2:// slurps the underlying file and runs libbz2's
    // BZ2_bzBuffToBuffDecompress over it before exposing the bytes through
    // the file descriptor. The hex payload below is `bzip2 -c < "elephc
    // bzip2 round-trip"` captured at fixture-generation time.
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$raw = hex2bin("425a6839314159265359814f1ef10000039980400210001e65d610200031434d300050f440c9ea7a8c1e5b5022c8cab9a05c297c5dc914e14242053c7bc4");
file_put_contents("cbz2_rt.bin", $raw);
$f = fopen("compress.bzip2://cbz2_rt.bin", "r");
echo stream_get_contents($f);
fclose($f);
"#,
    );
    assert_eq!(out, "elephc bzip2 round-trip");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream filter bzip2 compress then decompress roundtrip.
#[test]
fn test_stream_filter_bzip2_compress_then_decompress_roundtrip() {
    // bzip2.compress (write) streams the payload through libbz2's BZ2_bzCompress
    // and flushes the tail at fclose; bzip2.decompress (read) one-shot
    // decompresses it back. The compressed file must be smaller and the restored
    // bytes must match the original exactly.
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$payload = str_repeat("bzip2 stream filter round-trip. ", 12);
$w = fopen("bz2rt.bin", "w");
stream_filter_append($w, "bzip2.compress", STREAM_FILTER_WRITE);
fwrite($w, $payload);
fclose($w);
$comp = filesize("bz2rt.bin");
$r = fopen("bz2rt.bin", "r");
stream_filter_append($r, "bzip2.decompress", STREAM_FILTER_READ);
$restored = stream_get_contents($r);
fclose($r);
echo (($comp < strlen($payload)) ? "smaller" : "NOTSMALLER");
echo ($restored === $payload) ? "|match" : "|MISMATCH";
"#,
    );
    assert_eq!(out, "smaller|match");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream filter params compression level round trips.
#[test]
fn test_stream_filter_params_compression_level_round_trips() {
    // The 4th stream_filter_append $params arg sets the compression level
    // (zlib.deflate) / blockSize (bzip2.compress). A bare int literal is honored
    // at codegen; both filters must still produce a valid stream that the matching
    // decompressor restores exactly. zlib uses level 9, bzip2 blockSize 1.
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$payload = str_repeat("stream filter params round-trip. ", 16);

$zw = fopen("zp.bin", "w");
stream_filter_append($zw, "zlib.deflate", STREAM_FILTER_WRITE, 9);
fwrite($zw, $payload);
fclose($zw);
$zr = fopen("compress.zlib://zp.bin", "r");
$zrestored = stream_get_contents($zr);
fclose($zr);

$bw = fopen("bp.bin", "w");
stream_filter_append($bw, "bzip2.compress", STREAM_FILTER_WRITE, 1);
fwrite($bw, $payload);
fclose($bw);
$br = fopen("bp.bin", "r");
stream_filter_append($br, "bzip2.decompress", STREAM_FILTER_READ);
$brestored = stream_get_contents($br);
fclose($br);

echo ($zrestored === $payload) ? "zok" : "zBAD";
echo ($brestored === $payload) ? "|bok" : "|bBAD";
"#,
    );
    assert_eq!(out, "zok|bok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream filter params array form round trips.
#[test]
fn test_stream_filter_params_array_form_round_trips() {
    // PHP's canonical $params shape is an associative array, not a bare int:
    // zlib.deflate reads ['level' => N] and bzip2.compress reads
    // ['blocks' => N, 'work' => N]. Both array forms must be honored at codegen
    // and still produce a stream the matching decompressor restores exactly.
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$payload = str_repeat("array-form stream filter params round-trip. ", 16);

$zw = fopen("zp.bin", "w");
stream_filter_append($zw, "zlib.deflate", STREAM_FILTER_WRITE, ['level' => 9]);
fwrite($zw, $payload);
fclose($zw);
$zr = fopen("compress.zlib://zp.bin", "r");
$zrestored = stream_get_contents($zr);
fclose($zr);

$bw = fopen("bp.bin", "w");
stream_filter_append($bw, "bzip2.compress", STREAM_FILTER_WRITE, ['blocks' => 1, 'work' => 30]);
fwrite($bw, $payload);
fclose($bw);
$br = fopen("bp.bin", "r");
stream_filter_append($br, "bzip2.decompress", STREAM_FILTER_READ);
$brestored = stream_get_contents($br);
fclose($br);

echo ($zrestored === $payload) ? "zok" : "zBAD";
echo ($brestored === $payload) ? "|bok" : "|bBAD";
"#,
    );
    assert_eq!(out, "zok|bok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream filter bzip2 decompress reads real bzip2.
#[test]
fn test_stream_filter_bzip2_decompress_reads_real_bzip2() {
    // bzip2.decompress (the FILTER path, distinct from the compress.bzip2://
    // wrapper) must decode a genuine bzip2 stream. The hex payload is
    // `bzip2 -c < "elephc bzip2 round-trip"` captured at fixture-generation time.
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$raw = hex2bin("425a6839314159265359814f1ef10000039980400210001e65d610200031434d300050f440c9ea7a8c1e5b5022c8cab9a05c297c5dc914e14242053c7bc4");
file_put_contents("bz2fix.bin", $raw);
$f = fopen("bz2fix.bin", "r");
stream_filter_append($f, "bzip2.decompress", STREAM_FILTER_READ);
echo stream_get_contents($f);
fclose($f);
"#,
    );
    assert_eq!(out, "elephc bzip2 round-trip");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for compress bzip2 wrapper missing file returns false.
#[test]
fn test_compress_bzip2_wrapper_missing_file_returns_false() {
    // compress.bzip2:// surfaces a missing-file failure as PHP false,
    // mirroring the compress.zlib:// fallback path.
    let out = compile_and_run(
        r#"<?php
$r = @fopen("compress.bzip2:///nonexistent/elephc/file.bz2", "r");
echo ($r === false) ? "FALSE" : "OTHER";
"#,
    );
    assert_eq!(out, "FALSE");
}

/// Verifies compiled PHP output for compress zlib wrapper missing file returns false.
#[test]
fn test_compress_zlib_wrapper_missing_file_returns_false() {
    // compress.zlib:// must surface a missing-file failure as PHP `false`,
    // not as a half-attached inflate stream.
    let out = compile_and_run(
        r#"<?php
$r = @fopen("compress.zlib:///nonexistent/elephc/file.bin", "r");
echo ($r === false) ? "FALSE" : "OTHER";
"#,
    );
    assert_eq!(out, "FALSE");
}

/// Verifies compiled PHP output for stream filter zlib inflate decompresses.
#[test]
fn test_stream_filter_zlib_inflate_decompresses() {
    // The zlib.inflate read filter decompresses a zlib.deflate-compressed
    // stream; the two filters round-trip a payload through a file.
    let out = compile_and_run(
        r#"<?php
$data = str_repeat("zlib stream filter round-trip ", 24);
$w = fopen("zlib_rt.bin", "w");
stream_filter_append($w, "zlib.deflate", STREAM_FILTER_WRITE);
fwrite($w, $data);
fclose($w);
$r = fopen("zlib_rt.bin", "r");
stream_filter_append($r, "zlib.inflate", STREAM_FILTER_READ);
$got = stream_get_contents($r);
fclose($r);
echo ($got === $data) ? "roundtrip-ok" : "FAIL";
"#,
    );
    assert_eq!(out, "roundtrip-ok");
}

/// Verifies compiled PHP output for stream filter iconv utf8 to utf16le.
#[test]
fn test_stream_filter_iconv_utf8_to_utf16le() {
    // convert.iconv.UTF-8/UTF-16LE transcodes the stream at attach time via
    // libc iconv. "Hi" → 4 bytes UTF-16LE: 'H',0,'i',0. UTF-8↔UTF-16LE is in
    // the charset set even musl's limited iconv supports.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
fwrite($m, "Hi");
rewind($m);
stream_filter_append($m, "convert.iconv.UTF-8/UTF-16LE", STREAM_FILTER_READ);
$u = fread($m, 64);
echo strlen($u) . ":" . ord($u[0]) . "," . ord($u[1]) . "," . ord($u[2]) . "," . ord($u[3]);
fclose($m);
"#,
    );
    assert_eq!(out, "4:72,0,105,0");
}

/// Verifies compiled PHP output for stream filter iconv utf16le to utf8 roundtrips.
#[test]
fn test_stream_filter_iconv_utf16le_to_utf8_roundtrips() {
    // The reverse direction: UTF-16LE bytes decode back to the UTF-8 source.
    // The UTF-16LE input is built with chr() since elephc's lexer does not
    // process \xHH escapes.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
fwrite($m, chr(72) . chr(0) . chr(105) . chr(0) . chr(33) . chr(0));
rewind($m);
stream_filter_append($m, "convert.iconv.UTF-16LE/UTF-8", STREAM_FILTER_READ);
echo fread($m, 64);
fclose($m);
"#,
    );
    assert_eq!(out, "Hi!");
}

/// Verifies compiled PHP output for stream filter iconv write transcodes on fwrite.
#[test]
fn test_stream_filter_iconv_write_transcodes_on_fwrite() {
    // STREAM_FILTER_WRITE installs a streaming per-fwrite transcoder: "Hi"
    // written as UTF-8 lands in the stream as UTF-16LE (48 00 69 00).
    // stream_get_contents reads the raw stored bytes (it bypasses read filters),
    // so it returns the transcoded UTF-16LE form.
    let out = compile_and_run(
        r#"<?php
$f = fopen("php://temp", "r+");
stream_filter_append($f, "convert.iconv.UTF-8/UTF-16LE", STREAM_FILTER_WRITE);
fwrite($f, "Hi");
rewind($f);
echo bin2hex(stream_get_contents($f));
fclose($f);
"#,
    );
    assert_eq!(out, "48006900");
}

/// Verifies compiled PHP output for stream filter iconv write then read roundtrips.
#[test]
fn test_stream_filter_iconv_write_then_read_roundtrips() {
    // Write through the UTF-8->UTF-16LE write filter, then read back through the
    // UTF-16LE->UTF-8 read filter: the original text is recovered.
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$w = fopen("ic.bin", "w");
stream_filter_append($w, "convert.iconv.UTF-8/UTF-16LE", STREAM_FILTER_WRITE);
fwrite($w, "Hello");
fclose($w);
$r = fopen("ic.bin", "r");
stream_filter_append($r, "convert.iconv.UTF-16LE/UTF-8", STREAM_FILTER_READ);
echo fread($r, 64);
fclose($r);
"#,
    );
    assert_eq!(out, "Hello");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream filter iconv read still default on all mode.
#[test]
fn test_stream_filter_iconv_read_still_default_on_all_mode() {
    // Regression for the new mode dispatch: a bare append (no 3rd arg = ALL)
    // must keep the attach-time READ transform, not switch to write.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
fwrite($m, "Hi");
rewind($m);
stream_filter_append($m, "convert.iconv.UTF-8/UTF-16LE");
echo strlen(fread($m, 64));
fclose($m);
"#,
    );
    assert_eq!(out, "4");
}

/// Verifies compiled PHP output for stream filter base64 encode pads correctly.
#[test]
fn test_stream_filter_base64_encode_pads_correctly() {
    // The convert.base64-encode write filter encodes 3-byte groups into 4
    // base64 chars and pads the tail with '=' bytes. Tests all three
    // remainder cases (0/1/2 bytes leftover).
    let out = compile_and_run(
        r#"<?php
$m1 = fopen("php://memory", "r+");
stream_filter_append($m1, "convert.base64-encode", STREAM_FILTER_WRITE);
fwrite($m1, "Hello World");
rewind($m1);
echo fread($m1, 64);
fclose($m1);
echo "|";
$m2 = fopen("php://memory", "r+");
stream_filter_append($m2, "convert.base64-encode", STREAM_FILTER_WRITE);
fwrite($m2, "ab");
rewind($m2);
echo fread($m2, 64);
fclose($m2);
echo "|";
$m3 = fopen("php://memory", "r+");
stream_filter_append($m3, "convert.base64-encode", STREAM_FILTER_WRITE);
fwrite($m3, "a");
rewind($m3);
echo fread($m3, 64);
fclose($m3);
"#,
    );
    assert_eq!(out, "SGVsbG8gV29ybGQ=|YWI=|YQ==");
}

/// Verifies compiled PHP output for stream filter qp encode escapes non printables.
#[test]
fn test_stream_filter_qp_encode_escapes_non_printables() {
    // The convert.quoted-printable-encode write filter escapes bytes outside
    // ASCII 33..126 (and '=') as '=XX' hex escapes. Pass-through ASCII is
    // copied verbatim.
    let out = compile_and_run(
        r#"<?php
$s = "abc" . chr(195) . chr(169) . chr(10) . "=";
$m = fopen("php://memory", "r+");
stream_filter_append($m, "convert.quoted-printable-encode", STREAM_FILTER_WRITE);
fwrite($m, $s);
rewind($m);
echo fread($m, 64);
fclose($m);
"#,
    );
    assert_eq!(out, "abc=C3=A9=0A=3D");
}

/// Verifies the quoted-printable encoder leaves SPACE and TAB literal, as php's default does.
///
/// php escapes whitespace only under the filter's `binary` option; the default answers
/// `a b=3Dc d` for `a b=c d`. elephc passed through 33..126 only, which escaped both — php's
/// BINARY rule applied to every call, so `a b` came back as `a=20b` and a plain sentence round
/// -tripped into something php never writes. Measured on `php -n` 8.5.6.
#[test]
fn test_stream_filter_qp_encode_keeps_space_and_tab_literal() {
    let out = compile_and_run(
        r#"<?php
function qp(string $data): string {
    $m = fopen("php://memory", "r+");
    stream_filter_append($m, "convert.quoted-printable-encode", STREAM_FILTER_WRITE);
    fwrite($m, $data);
    rewind($m);
    $out = (string) stream_get_contents($m);
    fclose($m);
    return $out;
}
echo qp("a b=c d"), "|";
echo qp("Hello World!"), "|";
echo qp("a\tb"), "|";
echo qp(" "), "|";
// A newline is still escaped: only SPACE and TAB are exempt, and `=` still becomes `=3D`.
echo qp("x\ny"), "|";
echo qp("caf\xe9 au lait");
"#,
    );
    assert_eq!(
        out,
        "a b=3Dc d|Hello World!|a\tb| |x=0Ay|caf=E9 au lait"
    );
}

/// Verifies `Foo::class` names a stream wrapper/filter class as well as a string literal does.
///
/// `Foo::class` does not lower to `Op::ConstStr` — it is its own opcode indexing the class-name
/// table — so the reachability rule that decides which classes keep their runtime metadata could
/// not see it. The registration still SUCCEEDED and the scheme still appeared in
/// `stream_get_wrappers()`, but the class carried no vtable, so every `fopen()` through it failed
/// with no diagnostic at all. `Foo::class` is the refactor-safe spelling the manual and php-src's
/// own tests use, so it has to bind the class exactly as `'Foo'` does.
#[test]
fn test_class_constant_names_a_registered_stream_class() {
    let out = compile_and_run(
        r#"<?php
class ClassConstWrapper {
    public $context;
    private int $pos = 0;
    private string $data = "wrapped!";
    function stream_open($path, $mode, $options, &$opened): bool { return true; }
    function stream_read($n): string {
        $out = substr($this->data, $this->pos, $n);
        $this->pos += strlen($out);
        return $out;
    }
    function stream_eof(): bool { return $this->pos >= strlen($this->data); }
    function stream_stat(): array { return []; }
}
class ClassConstFilter extends php_user_filter {
    public function filter($in, $out, &$consumed, $closing): int {
        while ($b = stream_bucket_make_writeable($in)) {
            $b->data = strtoupper($b->data);
            $consumed += $b->datalen;
            stream_bucket_append($out, $b);
        }
        return PSFS_PASS_ON;
    }
}
echo var_export(stream_wrapper_register("ccw", ClassConstWrapper::class), true), "|";
$h = fopen("ccw://x", "r");
echo ($h === false ? "OPEN-FAILED" : stream_get_contents($h)), "|";

echo var_export(stream_filter_register("cc.up", ClassConstFilter::class), true), "|";
$m = fopen("php://memory", "r+");
$f = stream_filter_append($m, "cc.up", STREAM_FILTER_WRITE);
echo ($f === false ? "ATTACH-FAILED" : ""), "";
fwrite($m, "hello");
rewind($m);
echo stream_get_contents($m);
fclose($m);
"#,
    );
    assert_eq!(out, "true|wrapped!|true|HELLO");
}

/// Verifies `fflush()` is a flush point for a `zlib.deflate` filter, as it is in php.
///
/// A deflate stream holds its bytes until zlib's own window fills, so nothing reached the stream
/// until it CLOSED: a long-lived stream — a socket, say — compressed everything and sent none of
/// it. php pushes a `Z_SYNC_FLUSH` pass on `fflush()`, which closes the current block and emits the
/// `00 00 ff ff` marker. Measured on `php -n` 8.5.6 over 400 bytes to a file, `filesize()` reads 0
/// after the write, 12 after `fflush()` and 14 after `fclose()` — the close adds only the finishing
/// block; elephc read 0, 0, then 8.
///
/// The pass belongs to `fflush()` and NOT to the write path: with `Z_NO_FLUSH` per write, a
/// write-then-close stream still answers exactly `gzdeflate()`, which is what php answers for the
/// same program. Both are asserted, and so is the round trip through a mid-stream flush.
#[test]
fn test_fflush_pushes_the_deflate_sync_flush() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$data = str_repeat("a", 400);
$p = "zflushtest.bin";
$h = fopen($p, "wb");
stream_filter_append($h, "zlib.deflate", STREAM_FILTER_WRITE);
fwrite($h, $data);
clearstatcache();
printf("after write: %d\n", filesize($p));
fflush($h);
clearstatcache();
printf("after fflush: %d\n", filesize($p));
fclose($h);
clearstatcache();
printf("after close: %d\n", filesize($p));
unlink($p);
// Without a flush the stream is byte-for-byte gzdeflate(), which the write path must not change.
$p2 = "zflushtest2.bin";
$h = fopen($p2, "wb");
stream_filter_append($h, "zlib.deflate", STREAM_FILTER_WRITE);
fwrite($h, $data);
fclose($h);
$raw = file_get_contents($p2);
unlink($p2);
printf("no flush: %d equals gzdeflate=%s\n", strlen($raw), var_export($raw === gzdeflate($data), true));
// A payload written across a flush still round-trips whole.
$p3 = "zflushtest3.bin";
$h = fopen($p3, "wb");
stream_filter_append($h, "zlib.deflate", STREAM_FILTER_WRITE);
fwrite($h, $data);
fflush($h);
fwrite($h, $data);
fclose($h);
$raw = file_get_contents($p3);
unlink($p3);
printf("round trip: %s\n", var_export(gzinflate($raw) === $data . $data, true));
// fflush on a stream carrying no filter is untouched.
$p4 = "zflushtest4.bin";
$h = fopen($p4, "wb");
fwrite($h, "plain");
fflush($h);
clearstatcache();
printf("unfiltered: %d\n", filesize($p4));
fclose($h);
unlink($p4);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "after write: 0\n",
            "after fflush: 12\n",
            "after close: 14\n",
            "no flush: 8 equals gzdeflate=true\n",
            "round trip: true\n",
            "unfiltered: 5\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the closing dispatch delivers an EMPTY BRIGADE, so a mutating filter runs once.
///
/// php gives a filter one final `filter(..., $closing = true)` with no buckets at all. elephc built
/// a bucket whatever the input length, so `while ($b = stream_bucket_make_writeable($in))` ran a
/// second time over an empty `$b->data` and the filter applied TWICE: `$b->data = "<" . $b->data .
/// ">"` over "abc" answered "<<abc>>" where php answers "<abc>".
///
/// A filter that only FORWARDS its buckets cannot see the difference — an extra empty bucket
/// concatenates to nothing — which is why every existing test passed. The withholding filter is
/// here because it is the case the empty brigade must not break: it answers `PSFS_FEED_ME` until
/// `$closing`, and only then emits, so removing the bucket must not remove the dispatch.
#[test]
fn test_closing_dispatch_delivers_an_empty_brigade() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
class Mark extends php_user_filter {
    public function filter($in, $out, &$consumed, $closing): int {
        while ($b = stream_bucket_make_writeable($in)) {
            $b->data = "<" . $b->data . ">";
            $consumed += $b->datalen;
            stream_bucket_append($out, $b);
        }
        return PSFS_PASS_ON;
    }
}
stream_filter_register("mark2", "Mark");
$h = fopen("php://memory", "w+");
fwrite($h, "abc");
rewind($h);
stream_filter_append($h, "mark2", STREAM_FILTER_READ);
var_dump(stream_get_contents($h));
fclose($h);
$h = fopen("php://memory", "w+");
stream_filter_append($h, "mark2", STREAM_FILTER_WRITE);
fwrite($h, "xyz");
rewind($h);
var_dump(stream_get_contents($h));
fclose($h);
class Hold extends php_user_filter {
    private string $buf = "";
    public function filter($in, $out, &$consumed, $closing): int {
        while ($b = stream_bucket_make_writeable($in)) {
            $this->buf .= $b->data;
            $consumed += $b->datalen;
        }
        if ($closing) {
            $b = stream_bucket_new($this->stream, strrev($this->buf));
            stream_bucket_append($out, $b);
            return PSFS_PASS_ON;
        }
        return PSFS_FEED_ME;
    }
}
stream_filter_register("hold", "Hold");
$h = fopen("php://memory", "w+");
fwrite($h, "abcdef");
rewind($h);
stream_filter_append($h, "hold", STREAM_FILTER_READ);
var_dump(stream_get_contents($h));
fclose($h);
"#,
    );
    assert_eq!(
        out,
        "string(5) \"<abc>\"\nstring(5) \"<xyz>\"\nstring(6) \"fedcba\"\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `php_user_filter::$stream` is the stream being filtered, for the duration of `filter()`.
///
/// The property stayed null, so a filter could not reach the stream it was filtering — the manual's
/// own example does. php publishes it for the DURATION of each `filter()` call and nowhere else:
/// measured on `php -n` 8.5.6 it is UNSET inside `onCreate()`, a live resource inside `filter()`,
/// and NULL again inside `onClose()`. All three are asserted, because publishing it permanently
/// would be as wrong as never publishing it.
#[test]
fn test_user_filter_stream_property_is_live_during_filter() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
class Probe extends php_user_filter {
    public function onCreate(): bool {
        printf("onCreate %s\n", isset($this->stream) ? gettype($this->stream) : "unset");
        return true;
    }
    public function filter($in, $out, &$consumed, $closing): int {
        printf("filter %s %s %s\n",
            gettype($this->stream),
            var_export(is_resource($this->stream), true),
            is_resource($this->stream) ? stream_get_meta_data($this->stream)["stream_type"] : "-");
        while ($b = stream_bucket_make_writeable($in)) {
            $consumed += $b->datalen;
            stream_bucket_append($out, $b);
        }
        return PSFS_PASS_ON;
    }
    public function onClose(): void {
        printf("onClose %s\n", gettype($this->stream));
    }
}
stream_filter_register("probe", "Probe");
$h = fopen("php://memory", "w+");
fwrite($h, "hello");
rewind($h);
stream_filter_append($h, "probe", STREAM_FILTER_READ);
var_dump(stream_get_contents($h));
fclose($h);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "onCreate unset\n",
            "filter resource true MEMORY\n",
            "filter resource true MEMORY\n",
            "string(5) \"hello\"\n",
            "onClose NULL\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies php's two explanations for a stream `stream_select()` cannot represent.
///
/// The `ValueError` that follows was already right, but it arrived with nothing to say WHICH stream
/// caused it. php names the class when it defines no `stream_cast()` — `W::stream_cast is not
/// implemented!` — and then always reports `Cannot represent a stream of type user-space as a
/// select()able descriptor`. A class that DOES define the method and simply answers `false` gets
/// only the second, which is what separates the two here. Measured on `php -n` 8.5.6.
#[test]
fn test_stream_select_explains_an_uncastable_stream() {
    let missing = compile_and_run_expect_failure(
        r#"<?php
class W {
    public $context;
    public function stream_open($p, $m, $o, &$op) { return true; }
    public function stream_read($n) { return ""; }
    public function stream_eof() { return true; }
}
stream_wrapper_register("nocast2", "W");
$h = fopen("nocast2://x", "rb");
$r = [$h]; $w = null; $e = null;
stream_select($r, $w, $e, 0, 0);
"#,
    );
    assert!(
        missing.contains("Warning: stream_select(): W::stream_cast is not implemented!"),
        "missing-method warning absent: {missing}"
    );
    assert!(
        missing.contains(
            "Warning: stream_select(): Cannot represent a stream of type user-space \
             as a select()able descriptor"
        ),
        "unrepresentable warning absent: {missing}"
    );

    // A class that defines the method and refuses gets ONLY the second warning.
    let refusing = compile_and_run_expect_failure(
        r#"<?php
class C {
    public $context;
    public function stream_open($p, $m, $o, &$op) { return true; }
    public function stream_read($n) { return ""; }
    public function stream_eof() { return true; }
    public function stream_cast($as) { return false; }
}
stream_wrapper_register("hascast3", "C");
$h = fopen("hascast3://x", "rb");
$r = [$h]; $w = null; $e = null;
stream_select($r, $w, $e, 0, 0);
"#,
    );
    assert!(
        !refusing.contains("stream_cast is not implemented"),
        "named a method the class defines: {refusing}"
    );
    assert!(
        refusing.contains(
            "Warning: stream_select(): Cannot represent a stream of type user-space \
             as a select()able descriptor"
        ),
        "unrepresentable warning absent: {refusing}"
    );
}

/// Verifies a wrapper is READ in chunks, and that php's last `stream_read()` is not skipped.
///
/// `fgets()` asked the wrapper for ONE BYTE per iteration, so reading 100 bytes cost a HUNDRED
/// calls into user code where php makes six. php reads a chunk and keeps what the line does not
/// need, and that buffer survives the call — which is why the byte count below is reached with six
/// reads however many `fgets()` calls consume it.
///
/// `stream_get_contents()` had the opposite problem: it asked `stream_eof()` first and skipped the
/// final `stream_read()` when the answer was true. php does not gate on eof at all — it keeps
/// calling until one call answers an EMPTY string, which is the seventh here.
#[test]
fn test_user_wrapper_reads_are_chunked_like_php() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
class R {
    public $context;
    public static array $reads = [];
    public int $pos = 0;
    public function stream_open($p, $m, $o, &$op) { return true; }
    public function stream_read($n) {
        self::$reads[] = $n;
        $r = substr(str_repeat("a", 100), $this->pos, $n);
        $this->pos += strlen($r);
        return $r;
    }
    public function stream_write($d) { return strlen($d); }
    public function stream_eof() { return $this->pos >= 100; }
    public function stream_tell() { return $this->pos; }
    public function stream_seek($o, $w) { return false; }
    public function stream_stat() { return []; }
    public function stream_close() {}
}
stream_wrapper_register("chunkread", "R");
// stream_get_contents: the seventh call is the empty one that stops php's loop.
$h = fopen("chunkread://x", "rb");
stream_set_chunk_size($h, 17);
$s = stream_get_contents($h);
fclose($h);
printf("contents len=%d reads=%s\n", strlen($s), implode(",", R::$reads));
// fgets: six reads for the whole file, not one per byte.
R::$reads = [];
$h = fopen("chunkread://x", "rb");
stream_set_chunk_size($h, 17);
$n = 0;
while (($l = fgets($h)) !== false) {
    $n += strlen($l);
}
fclose($h);
printf("fgets len=%d reads=%d\n", $n, count(R::$reads));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "contents len=100 reads=17,17,17,17,17,17,17\n",
            "fgets len=100 reads=6\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a user wrapper's `stream_write()` receives CHUNKS, not the whole payload.
///
/// php hands a userspace wrapper at most `chunk_size` bytes per call, so 70 bytes to a stream whose
/// chunk size is 42 calls `stream_write()` twice, with 42 then 28. elephc made one call with all
/// 70, which a wrapper that counts or frames its writes observes directly.
///
/// Two details had to be measured rather than assumed. The default here is 8192 — the value
/// `stream_set_chunk_size()` itself reports as the previous one — not the 4096
/// `__rt_stream_chunk_size` answers, which is a read-loop fallback. And a SHORT write is not the
/// end: php re-offers from the new position, so a wrapper accepting four bytes of every ten still
/// receives the whole payload, as `10,10,10,10,10,10,6,2` for 30 bytes at chunk 10.
#[test]
fn test_user_wrapper_write_is_split_at_the_chunk_size() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
class W {
    public $context;
    public static array $writes = [];
    public static int $accept = -1;   // -1 = take everything
    public function stream_open($path, $mode, $options, &$opened) { return true; }
    public function stream_write($data) {
        self::$writes[] = strlen($data);
        return self::$accept < 0 ? strlen($data) : min(self::$accept, strlen($data));
    }
    public function stream_read($n) { return ""; }
    public function stream_eof() { return true; }
    public function stream_tell() { return 0; }
    public function stream_seek($o, $w) { return false; }
    public function stream_stat() { return []; }
    public function stream_close() {}
}
stream_wrapper_register("chunked", "W");
function run(int $chunk, int $bytes, int $accept): void {
    W::$writes = [];
    W::$accept = $accept;
    $h = fopen("chunked://x", "wb");
    stream_set_chunk_size($h, $chunk);
    $n = fwrite($h, str_repeat("a", $bytes));
    fclose($h);
    printf("chunk=%d bytes=%d accept=%d -> returned=%s writes=%s\n",
        $chunk, $bytes, $accept, var_export($n, true), implode(",", W::$writes));
}
run(42, 70, -1);
run(10, 25, -1);
run(100, 25, -1);
run(1, 3, -1);
run(10, 30, 4);
// The default chunk size is 8192, which is what stream_set_chunk_size() reports as the previous.
W::$writes = [];
W::$accept = -1;
$h = fopen("chunked://x", "wb");
var_dump(stream_set_chunk_size($h, 42));
fclose($h);
$h = fopen("chunked://x", "wb");
$n = fwrite($h, str_repeat("b", 9000));
fclose($h);
printf("default -> returned=%s writes=%s\n", var_export($n, true), implode(",", W::$writes));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "chunk=42 bytes=70 accept=-1 -> returned=70 writes=42,28\n",
            "chunk=10 bytes=25 accept=-1 -> returned=25 writes=10,10,5\n",
            "chunk=100 bytes=25 accept=-1 -> returned=25 writes=25\n",
            "chunk=1 bytes=3 accept=-1 -> returned=3 writes=1,1,1\n",
            "chunk=10 bytes=30 accept=4 -> returned=30 writes=10,10,10,10,10,10,6,2\n",
            "int(8192)\n",
            "default -> returned=9000 writes=8192,808\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an INCOMPLETE line is refused and kept on the stream, not handed back.
///
/// php's `stream_get_line()` answers `false` when it finds neither the delimiter nor the length cap
/// and the stream is not at EOF, and the bytes it read stay ON the stream. elephc consumed them and
/// answered them as a line php never breaks — so a reader assembling records off a non-blocking
/// socket saw a record split wherever the packets happened to land.
///
/// EOF is NOT that case, which is why the file half is here: a blocking file whose last line has no
/// delimiter still answers that line. Nor is the length cap: `stream_get_line($h, 4, "\n")` answers
/// four bytes with no delimiter in sight.
///
/// The bytes go back into the stream's read buffer, which php shares with every read function — a
/// refused `stream_get_line()` followed by `fread()` sees them, and so does one followed by
/// `fgets()`. `fgets()` takes them one at a time rather than in bulk because they CAN contain a
/// newline: `stream_get_line()` refuses on ITS delimiter, not on `\n`.
#[test]
fn test_stream_get_line_keeps_an_incomplete_line_on_the_stream() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
stream_set_blocking($pair[0], false);
fwrite($pair[1], "abc");
var_dump(stream_get_line($pair[0], 100, "
"));
fwrite($pair[1], "def
ghi");
var_dump(stream_get_line($pair[0], 100, "
"));
var_dump(stream_get_line($pair[0], 100, "
"));
fclose($pair[0]);
fclose($pair[1]);
// The retained bytes belong to the stream, so every reader sees them.
$p2 = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
stream_set_blocking($p2[0], false);
fwrite($p2[1], "abc");
var_dump(stream_get_line($p2[0], 100, "
"), fread($p2[0], 10));
fclose($p2[0]);
fclose($p2[1]);
$p3 = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
stream_set_blocking($p3[0], false);
fwrite($p3[1], "abc");
var_dump(stream_get_line($p3[0], 100, "
"), fgets($p3[0]));
fclose($p3[0]);
fclose($p3[1]);
// EOF still hands back a last line with no delimiter, and the cap still wins.
$f = "sglkeep.txt";
file_put_contents($f, "one
two
three");
$h = fopen($f, "rb");
var_dump(stream_get_line($h, 100, "
"), stream_get_line($h, 100, "
"));
var_dump(stream_get_line($h, 100, "
"), stream_get_line($h, 100, "
"));
fclose($h);
unlink($f);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(false)\nstring(6) \"abcdef\"\nbool(false)\n",
            "bool(false)\nstring(3) \"abc\"\n",
            "bool(false)\nstring(3) \"abc\"\n",
            "string(3) \"one\"\nstring(3) \"two\"\n",
            "string(5) \"three\"\nbool(false)\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies removing a `zlib.deflate` STOPS it, and flushes its tail where php flushes it.
///
/// The filter runs as an inline shape keyed on the descriptor, so unlinking its node retired the
/// resource and left the shape running: the stream went on compressing after
/// `stream_filter_remove()` had reported success, and a following `fwrite("plain text here")`
/// landed as deflate output.
///
/// The ORDER is the second half of the rule. php flushes the encoder's tail when the filter is
/// REMOVED, so the two-byte deflate sync marker precedes the plain text; elephc emitted it at
/// `fclose()`, which put the same bytes out back to front. Measured on `php -n` 8.5.6.
#[test]
fn test_removing_an_inline_shape_filter_stops_it_and_flushes_its_tail() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$p = "zdefremove.bin";
$h = fopen($p, "wb");
$z = stream_filter_append($h, "zlib.deflate", STREAM_FILTER_WRITE);
stream_filter_remove($z);
fwrite($h, "plain text here");
fclose($h);
echo bin2hex(file_get_contents($p)), "\n";
unlink($p);
// The tail reaches a memory stream too, and only once the filter is removed.
$h = fopen("php://memory", "w+");
$z = stream_filter_append($h, "zlib.deflate", STREAM_FILTER_WRITE);
$n = fwrite($h, str_repeat("a", 400));
var_dump($n, ftell($h));
stream_filter_remove($z);
rewind($h);
var_dump(strlen((string) stream_get_contents($h)));
fclose($h);
"#,
    );
    assert_eq!(
        out,
        concat!(
            // the deflate sync marker, THEN "plain text here"
            "0300", "706c61696e2074657874206865726", "5\n",
            "int(400)\nint(0)\n",
            "int(8)\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the filters compiled as an inline shape still hand back the resource php documents.
///
/// `zlib.*`, `bzip2.*` and `convert.iconv.*` filter through code emitted over the DESCRIPTOR rather
/// than through a chain node, so they filtered but minted nothing: `is_resource()` on the result
/// answered false and `get_resource_type()` answered "Unknown", where php answers a live
/// `stream filter`. Nothing observed the filter's lifetime — neither `stream_filter_remove()` nor
/// the invalidation php performs when the owning stream closes.
///
/// The node minted for them is INERT: no built-in id and no `php_user_filter`, which is what makes
/// the chain applier pass it by while the inline shape keeps doing the filtering. It joins the
/// chain all the same, because that is what makes `fclose()` close it — the case this test pins
/// with `closed`.
#[test]
fn test_inline_shape_filters_still_mint_their_resource() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
function shape($r): string {
    return var_export(is_resource($r), true) . " " . (is_resource($r) ? get_resource_type($r) : "-");
}
$h = fopen("php://memory", "w+");
$z = stream_filter_append($h, "zlib.deflate", STREAM_FILTER_WRITE);
echo "deflate  ", shape($z), "\n";
echo "remove   ", var_export(stream_filter_remove($z), true), " ", shape($z), "\n";
fclose($h);
$h = fopen("php://memory", "w+");
$i = stream_filter_append($h, "zlib.inflate", STREAM_FILTER_READ);
echo "inflate  ", shape($i), "\n";
fclose($h);
echo "closed   ", shape($i), "\n";
$h = fopen("php://memory", "w+");
echo "bzip2    ", shape(stream_filter_append($h, "bzip2.compress", STREAM_FILTER_WRITE)), "\n";
fclose($h);
$h = fopen("php://memory", "w+");
echo "iconv    ", shape(stream_filter_append($h, "convert.iconv.utf-8/utf-8", STREAM_FILTER_WRITE)), "\n";
fclose($h);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "deflate  true stream filter\n",
            "remove   true false -\n",
            "inflate  true stream filter\n",
            "closed   false -\n",
            "bzip2    true stream filter\n",
            "iconv    true stream filter\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `stream_filter_remove()` refuses a resource that is not a live filter, in php's words.
///
/// Passing an ordinary stream reported SUCCESS: the chain lookup rejected the handle, the legacy
/// per-descriptor path cleared four already-empty table slots and answered `true`. php throws
/// there, and again for a filter that was already removed. Its wording is not a variation on the
/// generic one either — `supplied resource is not a valid stream filter resource`, with no
/// argument name, for every resource it will not accept. Measured on `php -n` 8.5.6.
///
/// The legacy path stays reachable for the handles that DO own a per-descriptor filter, which is
/// what `zlib.*` and `bzip2.*` still use; the guard only refuses a descriptor whose four slots are
/// all empty.
///
/// A value that is not a resource AT ALL is not exercised here: php raises its `Argument #1
/// ($stream_filter) must be of type resource` at run time, and elephc's checker refuses the same
/// call at compile time, so no program reaches that run-time branch.
#[test]
fn test_stream_filter_remove_refuses_a_resource_that_is_not_a_filter() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = fopen("php://memory", "w+");
try {
    stream_filter_remove($h);
} catch (Throwable $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
$f = stream_filter_append($h, "string.toupper", STREAM_FILTER_WRITE);
var_dump(get_resource_type($f), stream_filter_remove($f), is_resource($f));
// Removing it a second time is the same refusal: the resource is no longer a live filter.
try {
    stream_filter_remove($f);
} catch (Throwable $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
fclose($h);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "TypeError: stream_filter_remove(): supplied resource is not a valid stream filter resource\n",
            "string(13) \"stream filter\"\nbool(true)\nbool(false)\n",
            "TypeError: stream_filter_remove(): supplied resource is not a valid stream filter resource\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a `$params` the filter cannot read is REFUSED, and only by the filters that read it.
///
/// php's four `convert.*` filters parse `$params` as an array and reject anything else with two
/// warnings and a `false`; `string.*`, `dechunk`, `zlib.*` and `bzip2.*` accept a null, an int or a
/// string without complaint, because they never look at it. elephc attached a working filter in
/// every case and said nothing.
///
/// OMITTING the argument is the case that pins the rule: php tests the zval POINTER, which is NULL
/// only when nothing was supplied, so a three-argument call SUCCEEDS on the very filters that
/// refuse an explicit `null`.
#[test]
fn test_builtin_filter_refuses_a_params_it_cannot_read() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$names = ["string.toupper", "dechunk", "convert.base64-encode", "convert.base64-decode",
          "convert.quoted-printable-encode", "convert.quoted-printable-decode"];
foreach ($names as $n) {
    $h = fopen("php://memory", "w+");
    printf("%s null=%s int=%s none=%s arr=%s\n", $n,
        var_export(is_resource(@stream_filter_append($h, $n, STREAM_FILTER_WRITE, null)), true),
        var_export(is_resource(@stream_filter_append($h, $n, STREAM_FILTER_WRITE, 7)), true),
        var_export(is_resource(stream_filter_append($h, $n, STREAM_FILTER_WRITE)), true),
        var_export(is_resource(stream_filter_append($h, $n, STREAM_FILTER_WRITE, [])), true));
    fclose($h);
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "string.toupper null=true int=true none=true arr=true\n",
            "dechunk null=true int=true none=true arr=true\n",
            "convert.base64-encode null=false int=false none=true arr=true\n",
            "convert.base64-decode null=false int=false none=true arr=true\n",
            "convert.quoted-printable-encode null=false int=false none=true arr=true\n",
            "convert.quoted-printable-decode null=false int=false none=true arr=true\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies php's built-in encoders read `$params`: `line-length`, `line-break-chars`, `binary`.
///
/// elephc retained `$params` only for a USER filter, where `filter()` reads it off the instance, and
/// passed 0 for a built-in — so `["line-length" => 8]` produced one unbroken line where php
/// produces wrapped ones, and `["binary" => true]` left SPACE and TAB literal where php escapes
/// them. The array is now parsed once at attach, into plain words on the filter node.
///
/// The default is NO wrapping for both encoders, which is why the unparameterized cases are here:
/// they are the common path and must stay byte-identical. The default break is CRLF, not a lone
/// newline — measured on `php -n` 8.5.6, `["line-length" => 8]` over "hello world" answers 18
/// bytes, not 17.
#[test]
fn test_builtin_filters_read_their_params() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
function through(string $filter, array $params, string $data): string {
    $h = fopen("php://memory", "w+");
    stream_filter_append($h, $filter, STREAM_FILTER_WRITE, $params);
    fwrite($h, $data);
    rewind($h);
    $out = (string) stream_get_contents($h);
    fclose($h);
    return $out;
}
echo bin2hex(through("convert.base64-encode", ["line-length" => 8], "hello world")), "\n";
echo through("convert.base64-encode", ["line-length" => 8, "line-break-chars" => "|"], "hello world"), "\n";
echo through("convert.base64-encode", [], "hello world"), "\n";
echo through("convert.quoted-printable-encode", ["binary" => true], "a b\tc"), "\n";
echo through("convert.quoted-printable-encode", [], "a b\tc"), "\n";
// The soft break costs a column of its own, and never falls inside an `=XX` triplet.
echo bin2hex(through("convert.quoted-printable-encode", ["line-length" => 12], "aaaaaaaaaaaaaaaaaaaa")), "\n";
echo bin2hex(through("convert.quoted-printable-encode", ["line-length" => 10], "aaaaaa\xE9bbbbbb")), "\n";
echo bin2hex(through("convert.quoted-printable-encode", ["line-length" => 8], "aaaaaaaa\xE9bbbbbb")), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            // "aGVsbG8g" CRLF "d29ybGQ=" — the default break is CRLF, not a lone newline
            "61475673624738670d0a643239796247513d\n",
            "aGVsbG8g|d29ybGQ=\n",
            "aGVsbG8gd29ybGQ=\n",
            "a=20b=09c\n",
            "a b\tc\n",
            // 11 a's, then the soft `=` taking the twelfth column, CRLF, then the remaining 9
            "61616161616161616161613d0d0a616161616161616161\n",
            // the `=E9` FITS at column 6 with line-length 10, so no break precedes it
            "6161616161613d45393d0d0a626262626262\n",
            // at line-length 8 the eighth `a` already needs a break, and the triplet stays whole
            "616161616161613d0d0a613d45396262623d0d0a626262\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a SOCKET carries no `wrapper_type` and does carry the address it was opened on.
///
/// php reaches every transport through `php_stream_xport_create`, which never assigns
/// `stream->wrapper`, and `_php_stream_get_metadata` writes `wrapper_type` only `if
/// (stream->wrapper)`. elephc left the wrapper id at its unset value, which maps to "plainfile", so
/// every socket claimed to have been opened by the plain-files wrapper. The `uri` moved the other
/// way: php stores the address in `stream->orig_path` and reports it, and elephc recorded the
/// transport but not the text, so the key php provides was missing.
///
/// A socket PAIR names no address, php leaves `orig_path` NULL for it, and the key stays absent —
/// which is why the pair is checked here alongside the two openers that do name one.
#[test]
fn test_socket_metadata_has_no_wrapper_and_keeps_its_address() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$m = stream_get_meta_data($pair[0]);
var_dump(array_key_exists("wrapper_type", $m), array_key_exists("uri", $m), $m["stream_type"]);
$path = "sockmeta.sock";
$srv = stream_socket_server("unix://" . $path);
$m2 = stream_get_meta_data($srv);
var_dump(array_key_exists("wrapper_type", $m2), $m2["uri"], $m2["stream_type"]);
$cli = stream_socket_client("unix://" . $path);
$m3 = stream_get_meta_data($cli);
var_dump(array_key_exists("wrapper_type", $m3), $m3["uri"]);
// An accepted connection names no address of its own either.
$acc = stream_socket_accept($srv);
$m4 = stream_get_meta_data($acc);
var_dump(array_key_exists("wrapper_type", $m4), array_key_exists("uri", $m4), $m4["stream_type"]);
fclose($acc);
fclose($cli);
fclose($srv);
fclose($pair[0]);
fclose($pair[1]);
unlink($path);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(false)\nbool(false)\nstring(14) \"generic_socket\"\n",
            "bool(false)\nstring(20) \"unix://sockmeta.sock\"\nstring(11) \"unix_socket\"\n",
            "bool(false)\nstring(20) \"unix://sockmeta.sock\"\n",
            "bool(false)\nbool(false)\nstring(11) \"unix_socket\"\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `fsockopen()` takes a lone hostname, honours the transport that hostname names, and
/// reports the address php reports.
///
/// php's `$port` defaults to -1, and `php_stream_xport_create` reads the transport out of the
/// address, falling back to TCP only when there is no `://`. elephc required the port AND prepended
/// `tcp://` unconditionally, so `fsockopen("unix:///tmp/s.sock")` did not compile at all — and once
/// it did, the address became `tcp://unix:///tmp/s.sock`, which resolves as a HOSTNAME.
///
/// The `uri` is checked against the port because php records the string it composed, `host:port`,
/// with no scheme: the `tcp://` elephc adds for a schemeless host is elephc's, and php never saw
/// it. The `tcp://`-spelled call is here to pin the other side of that rule — a hostname that
/// names its own transport keeps every byte of it.
#[test]
fn test_fsockopen_takes_a_lone_address_and_keeps_its_transport() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$path = "fsock.sock";
$srv = stream_socket_server("unix://" . $path);
$c = fsockopen("unix://" . $path);
$m = stream_get_meta_data($c);
var_dump(is_resource($c), $m["uri"], $m["stream_type"], array_key_exists("wrapper_type", $m));
fclose($c);
fclose($srv);
unlink($path);
$tcp = stream_socket_server("tcp://127.0.0.1:0");
$name = stream_socket_get_name($tcp, false);
$port = (int) explode(":", $name)[1];
// A schemeless host gets `tcp://` for the connect, but php's `uri` is the bare `host:port`.
$c2 = fsockopen("127.0.0.1", $port);
$m2 = stream_get_meta_data($c2);
var_dump($m2["uri"] === "127.0.0.1:" . $port, $m2["stream_type"]);
fclose($c2);
// A host that names the transport keeps it, whatever letter it starts with.
$c3 = fsockopen("tcp://127.0.0.1", $port);
$m3 = stream_get_meta_data($c3);
var_dump($m3["uri"] === "tcp://127.0.0.1:" . $port);
fclose($c3);
fclose($tcp);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\nstring(17) \"unix://fsock.sock\"\nstring(11) \"unix_socket\"\nbool(false)\n",
            "bool(true)\nstring(14) \"tcp_socket/ssl\"\n",
            "bool(true)\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a resource NESTED in a container renders, and that php's resource numbering matches.
///
/// `__rt_var_dump_value` sent runtime tag 9 to its NULL arm, so a resource inside an array, a
/// hash or an object printed as `NULL` while the same resource dumped on its own printed
/// correctly. That is one renderer, so every container shape was wrong at once — a
/// `stream_socket_pair()` result looked like `[NULL, NULL]` even though both ends were live.
///
/// The NUMBER is checked alongside it because the two defects hid each other: php's
/// `file_get_contents()` and `file_put_contents()` open a stream internally and therefore consume
/// one resource id apiece, while elephc used raw syscalls and consumed none — so every id after
/// such a call was one lower than php's.
#[test]
fn test_nested_resource_renders_and_numbers_like_php() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
// Each whole-file call costs one id in php, so the handle below must be numbered past them.
file_put_contents("resnest.txt", "x");
file_get_contents("resnest.txt");
$f = fopen("resnest.txt", "r");
var_dump($f);
var_dump([$f]);
var_dump(["h" => $f]);
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
var_dump(is_resource($pair[0]), $pair);
fclose($pair[0]);
fclose($pair[1]);
fclose($f);
unlink("resnest.txt");
"#,
    );
    assert_eq!(
        out,
        concat!(
            "resource(7) of type (stream)\n",
            "array(1) {\n  [0]=>\n  resource(7) of type (stream)\n}\n",
            "array(1) {\n  [\"h\"]=>\n  resource(7) of type (stream)\n}\n",
            "bool(true)\n",
            "array(2) {\n  [0]=>\n  resource(8) of type (stream)\n  [1]=>\n",
            "  resource(9) of type (stream)\n}\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a read filter answering `PSFS_ERR_FATAL` fails the read instead of emptying it.
///
/// php separates "the stream is exhausted" from "a filter refused the data": `fread()` and
/// `stream_copy_to_stream()` answer `false`, while `stream_get_contents()` answers `""` and
/// `fgets()` `false`. elephc reported `""` and `int(0)` for the first two, which read as an empty
/// stream rather than a failure — the whole point of the return value a filter uses to say the
/// data is unusable.
///
/// The published code is reset to `PSFS_PASS_ON` before each filtered read and before a copy,
/// because the slot lives in BSS and starts at ZERO — which IS `PSFS_ERR_FATAL`, so without the
/// reset an ordinary EOF on the first filtered read would look like a refusal.
#[test]
fn test_filter_fatal_fails_the_read() {
    let out = compile_and_run(
        r#"<?php
class FatalFilter extends php_user_filter {
    public function filter($in, $out, &$consumed, $closing): int {
        stream_bucket_make_writeable($in);
        return PSFS_ERR_FATAL;
    }
}
class PassFilter extends php_user_filter {
    public function filter($in, $out, &$consumed, $closing): int {
        while ($b = stream_bucket_make_writeable($in)) {
            $consumed += $b->datalen;
            stream_bucket_append($out, $b);
        }
        return PSFS_PASS_ON;
    }
}
stream_filter_register('fatalf', 'FatalFilter');
stream_filter_register('passf', 'PassFilter');
function src(string $filter) {
    $s = fopen('php://memory', 'rb+');
    fwrite($s, 'Test data');
    rewind($s);
    stream_filter_prepend($s, $filter, STREAM_FILTER_READ);
    return $s;
}
$a = src('fatalf'); echo var_export(stream_copy_to_stream($a, fopen('php://memory', 'wb')), true), "|"; fclose($a);
$b = src('fatalf'); echo var_export(fread($b, 32), true), "|"; fclose($b);
$c = src('fatalf'); echo var_export(stream_get_contents($c), true), "|"; fclose($c);
$d = src('fatalf'); echo var_export(fgets($d), true), "|"; fclose($d);
// A filter that PASSES must still report the bytes, and its EOF must still be "" — the
// reset is what keeps the exhausted read from inheriting the previous stream's refusal.
$e = src('passf'); echo var_export(fread($e, 32), true), "|";
echo var_export(fread($e, 32), true), "|";
fclose($e);
$f = src('passf'); echo var_export(stream_copy_to_stream($f, fopen('php://memory', 'wb')), true); fclose($f);
"#,
    );
    assert_eq!(
        out,
        "false|false|''|false|'Test data'|''|9"
    );
}

/// Verifies the `data:` scheme is read by every reader, with or without the `//`.
///
/// RFC 2397 has no `//` and php makes it optional, so the canonical spelling is `data:,abc` /
/// `data:text/plain;base64,...`. `fopen()` tested the five-byte scheme and read it, but
/// `file_get_contents()` tested `data://` — so the canonical form fell through to the FILE reader
/// and answered `false` with "No such file or directory". A URL built at run time missed as well:
/// the dynamic route knows http/https/ftp/ftps and then reads a file, so nothing decoded it.
#[test]
fn test_data_uri_is_read_with_or_without_the_double_slash() {
    let out = compile_and_run(
        r#"<?php
echo var_export(file_get_contents("data:,abc"), true), "|";
echo var_export(file_get_contents("data://,abc"), true), "|";
echo var_export(file_get_contents("data:text/plain,abc"), true), "|";
$h = fopen("data:,abc", "r");
echo var_export(stream_get_contents($h), true), "|";
fclose($h);
// Built at run time, so the dynamic route decides it.
$u = "data:," . str_repeat("A", 100);
echo strlen((string) file_get_contents($u)), "|";
echo var_export(file_get_contents("data://text/plain;base64,YWJj"), true);
"#,
    );
    assert_eq!(out, "'abc'|'abc'|'abc'|'abc'|100|'abc'");
}

/// Verifies `stream_context_create()` enforces php's option-array shape.
///
/// php keeps only entries whose key is a STRING and whose value is an ARRAY, raising a catchable
/// `ValueError` otherwise — measured: `['ssl' => "abc"]`, `['ssl' => 1]` and `[0 => ['a' => 1]]`
/// all raise it, while `[]` and an absent argument do not. elephc stored the malformed map in
/// silence, so a typo in a context array produced a context that simply carried nothing.
///
/// The empty array is its own case because `[]` is a PACKED array, not a hash: walking it as one
/// would read a header that is not there, and a packed array with elements can only have integer
/// keys, which php refuses.
#[test]
fn test_stream_context_create_enforces_the_option_shape() {
    let out = compile_and_run(
        r#"<?php
function t(string $label, callable $fn): void {
    echo $label, "=";
    try { $fn(); echo "OK|"; }
    catch (ValueError $e) { echo "ValueError|"; }
}
t("good",    fn() => stream_context_create(['http' => ['method' => 'POST']]));
t("string",  fn() => stream_context_create(['ssl' => "abc"]));
t("int",     fn() => stream_context_create(['ssl' => 1]));
t("intkey",  fn() => stream_context_create([0 => ['a' => 1]]));
t("empty",   fn() => stream_context_create([]));
t("none",    fn() => stream_context_create());
t("nested",  fn() => stream_context_create(['http' => [0 => 'v']]));
echo (string) stream_context_get_options(stream_context_create(['http' => ['m' => 'v']]))["http"]["m"];
"#,
    );
    assert_eq!(
        out,
        "good=OK|string=ValueError|int=ValueError|intkey=ValueError|empty=OK|none=OK|nested=OK|v"
    );
}

/// Verifies `php_user_filter::$filtername` carries the ATTACHED name and `$closing` is a bool.
///
/// php seeds `$filtername` before `onCreate()` with the name the filter was attached under, so
/// one class registered under two names reports each in turn; elephc left the property null.
/// `$closing` is documented `bool`: an untyped parameter otherwise infers Int, so `var_dump()`
/// printed `int(0)` where php prints `bool(false)` and `$closing === true` could never hold.
#[test]
fn test_user_filter_inherited_properties() {
    let out = compile_and_run(
        r#"<?php
class NameProbe extends php_user_filter {
    public function onCreate(): bool {
        echo "create:", var_export($this->filtername, true), "|";
        return true;
    }
    public function filter($in, $out, &$consumed, $closing): int {
        echo "filter:", var_export($this->filtername, true),
             ":", var_export($closing, true),
             ":", var_export($closing === true, true), "|";
        while ($b = stream_bucket_make_writeable($in)) {
            $consumed += $b->datalen;
            stream_bucket_append($out, $b);
        }
        return PSFS_PASS_ON;
    }
}
stream_filter_register("np.one", "NameProbe");
stream_filter_register("np.two", "NameProbe");
$h = fopen("php://memory", "r+");
stream_filter_append($h, "np.one", STREAM_FILTER_WRITE);
fwrite($h, "x");
fclose($h);
$g = fopen("php://memory", "r+");
stream_filter_append($g, "np.two", STREAM_FILTER_WRITE);
fwrite($g, "y");
fclose($g);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "create:'np.one'|filter:'np.one':false:false|filter:'np.one':true:true|",
            "create:'np.two'|filter:'np.two':false:false|filter:'np.two':true:true|"
        )
    );
}

/// Verifies a non-zero `$microseconds` beside a null `$seconds` raises php's `ValueError`.
///
/// A null `$seconds` is php's "block forever", which no microsecond count can refine, so php
/// refuses the pair rather than ignoring one half of it — `stream_select($r, $w, $e, null, 5)`
/// throws. A microsecond count of exactly ZERO is still allowed, because php only rejects a
/// non-zero one. Measured on `php -n` 8.5.6.
#[test]
fn test_stream_select_microseconds_require_seconds() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("select_pair.txt", "x");
$f = fopen("select_pair.txt", "r");
$r = [$f]; $w = null; $e = null;
try { stream_select($r, $w, $e, null, 5); echo "no throw|"; }
catch (ValueError $x) { echo $x->getMessage(), "|"; }
// Zero microseconds beside a null $seconds is accepted.
$r2 = [$f]; $w2 = null; $e2 = null;
echo var_export(stream_select($r2, $w2, $e2, null, 0), true);
fclose($f);
unlink("select_pair.txt");
"#,
    );
    assert_eq!(
        out,
        "stream_select(): Argument #5 ($microseconds) must be null when argument #4 ($seconds) is null|1"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `stream_filter_register()` refuses a name that is already taken, and a filter
/// resource names itself.
///
/// php answers `false` rather than replacing a registration: a name php itself owns
/// (`string.toupper`, `zlib.deflate`) is never replaceable, and the second registration of a
/// fresh name is false too. elephc stored into the first EMPTY slot without comparing names, so
/// both answered `true` and a program branching on the result took the wrong path. An empty name
/// or class is php's own catchable `ValueError`, raised before the registry is consulted — and
/// php does NOT check that the class exists here at all.
///
/// The filter resource is checked in the same test because it is the same registry: php gives it
/// its own type, `stream filter`, which `var_dump()` and `get_resource_type()` both report.
#[test]
fn test_stream_filter_register_refuses_a_taken_name() {
    let out = compile_and_run(
        r#"<?php
class RegUpper extends php_user_filter {
    public function filter($in, $out, &$consumed, $closing): int {
        while ($b = stream_bucket_make_writeable($in)) {
            $b->data = strtoupper($b->data);
            $consumed += $b->datalen;
            stream_bucket_append($out, $b);
        }
        return PSFS_PASS_ON;
    }
}
// A name php owns EXACTLY is never replaceable; a WILDCARD family name is registrable, which
// is why `zlib.deflate` and `convert.base64-encode` answer true and `string.toupper` does not.
echo var_export(stream_filter_register("string.toupper", "RegUpper"), true), "|";
echo var_export(stream_filter_register("consumed", "RegUpper"), true), "|";
echo var_export(stream_filter_register("zlib.deflate", "RegUpper"), true), "|";
echo var_export(stream_filter_register("convert.base64-encode", "RegUpper"), true), "|";
// A fresh name registers once, and only once.
echo var_export(stream_filter_register("reg.upper", "RegUpper"), true), "|";
echo var_export(stream_filter_register("reg.upper", "RegUpper"), true), "|";
// Empty arguments are catchable ValueErrors, not registrations.
try { stream_filter_register("", "RegUpper"); } catch (ValueError $e) { echo $e->getMessage(), "|"; }
try { stream_filter_register("reg.other", ""); } catch (ValueError $e) { echo $e->getMessage(), "|"; }
// The registered filter still works, and its resource names itself.
$m = fopen("php://memory", "r+");
$f = stream_filter_append($m, "reg.upper", STREAM_FILTER_WRITE);
echo get_resource_type($f), "|";
fwrite($m, "hello");
rewind($m);
echo stream_get_contents($m);
fclose($m);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "false|false|true|true|true|false|",
            "stream_filter_register(): Argument #1 ($filter_name) must be a non-empty string|",
            "stream_filter_register(): Argument #2 ($class) must be a non-empty string|",
            "stream filter|HELLO"
        )
    );
}

/// Verifies an `a` mode on `php://memory`/`php://temp` sends every write to the END.
///
/// php ignores the seek position for a write in append mode: writing `hello`, seeking to 0 and
/// writing `world` answers `helloworld`. A real file gets that from `O_APPEND` at `open()`, but
/// the in-memory backend is a `tmpfile()` descriptor created with no mode at all, so the second
/// write OVERWROTE the first and the stream silently lost data. `php://temp` and `php://memory`
/// are separate cases because they are separate sub-wrappers, and the URL is checked once as a
/// literal and once built at run time because those are two different openers.
#[test]
fn test_append_mode_memory_stream_writes_at_the_end() {
    let out = compile_and_run(
        r#"<?php
$fp = fopen("php://temp", "a+");
fwrite($fp, "hello");
fseek($fp, 0, SEEK_SET);
fwrite($fp, "world");
echo stream_get_contents($fp, -1, 0), "|";
fclose($fp);

$m = fopen("php://memory", "a+");
fwrite($m, "abc");
rewind($m);
fwrite($m, "XY");
echo stream_get_contents($m, -1, 0), "|";
fclose($m);

// The same URL built at run time takes the dynamic opener, which needs the flag too.
$path = "php://" . "temp";
$d = fopen($path, "a+");
fwrite($d, "one");
fseek($d, 0, SEEK_SET);
fwrite($d, "two");
echo stream_get_contents($d, -1, 0), "|";
fclose($d);

// A `w+` mode still overwrites, which is the case the append flag must not capture.
$w = fopen("php://temp", "w+");
fwrite($w, "hello");
fseek($w, 0, SEEK_SET);
fwrite($w, "world");
echo stream_get_contents($w, -1, 0);
fclose($w);
"#,
    );
    assert_eq!(out, "helloworld|abcXY|onetwo|world");
}

/// Verifies `stream_get_meta_data()` omits `uri` for a pathless stream and calls a directory
/// seekable.
///
/// php guards the key with `if (stream->orig_path)`, so a directory handle answers EIGHT keys;
/// elephc inserted `["uri"] => ""` and reported nine, which made every `count()` over the result
/// disagree. `seekable` is `stream->ops->seek != NULL` in php, not a live probe: the plain-files
/// directory ops carry `rewinddir`, so php says `true` where `S_ISREG` on the descriptor — the
/// question elephc asked — says `false`.
#[test]
fn test_stream_get_meta_data_directory_shape() {
    let out = compile_and_run(
        r#"<?php
$d = opendir(sys_get_temp_dir());
$m = stream_get_meta_data($d);
echo count($m), "|", var_export(isset($m["uri"]), true), "|", var_export($m["seekable"], true);
echo "|", $m["stream_type"], "|", $m["wrapper_type"];
closedir($d);
"#,
    );
    assert_eq!(out, "8|false|true|dir|plainfile");
}

/// Verifies `fwrite()`'s third argument caps the write, and that a null cap writes everything.
///
/// php's signature is `fwrite($stream, string $data, ?int $length = null)`: the write is capped
/// at `max(0, min($length, strlen($data)))`, a non-positive cap writes nothing WITHOUT raising,
/// and null means no cap. elephc accepted only two arguments. The cap is applied to the byte
/// count the runtime write helper already takes, so an attached write filter sees exactly the
/// bytes php gives it rather than the whole string.
#[test]
fn test_fwrite_length_argument_caps_the_write() {
    let out = compile_and_run(
        r#"<?php
function w(string $data, $length): string {
    $m = fopen("php://memory", "r+");
    $n = $length === "omit" ? fwrite($m, $data) : fwrite($m, $data, $length);
    rewind($m);
    $got = (string) stream_get_contents($m);
    fclose($m);
    return $n . ":" . $got;
}
echo w("hello", 3), "|";       // shorter than the data
echo w("hello", 5), "|";       // exactly the data
echo w("hello", 9), "|";       // longer than the data clamps to it
echo w("hello", 0), "|";       // zero writes nothing, and is not an error
echo w("hello", -1), "|";      // neither is a negative
echo w("hello", null), "|";    // null is "no cap"
echo w("hello", "omit"), "|";  // as is omitting it
// A write filter must see the CAPPED bytes, not the whole string.
$m = fopen("php://memory", "r+");
stream_filter_append($m, "string.toupper", STREAM_FILTER_WRITE);
$n = fwrite($m, "abcdef", 4);
rewind($m);
echo $n, ":", stream_get_contents($m);
fclose($m);
"#,
    );
    assert_eq!(
        out,
        "3:hel|5:hello|5:hello|0:|0:|5:hello|5:hello|4:ABCD"
    );
}

/// Verifies a `?int` argument arriving as a boxed null keeps php's "no bound" meaning.
///
/// `__rt_mixed_cast_int` flattens a null payload to `0` — the same answer a real `0` gives — so
/// forwarding one through an untyped parameter turned `fgets($h, null)` into
/// `ValueError: Argument #2 ($length) must be greater than 0` and made `fwrite($h, $d, null)`
/// write nothing. php reads the whole line and writes every byte for both.
#[test]
fn test_nullable_length_through_an_untyped_parameter() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("nullable_len.txt", "hello world\n");
function grabLine($h, $len) { return fgets($h, $len); }
function writeAll($h, $data, $len) { return fwrite($h, $data, $len); }
$h = fopen("nullable_len.txt", "r");
echo var_export(grabLine($h, null), true), "|";
fclose($h);
$m = fopen("php://memory", "r+");
echo writeAll($m, "abcdef", null), "|";
rewind($m);
echo stream_get_contents($m);
fclose($m);
unlink("nullable_len.txt");
"#,
    );
    assert_eq!(out, "'hello world\n'|6|abcdef");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream filter base64 decode decompacts.
#[test]
fn test_stream_filter_base64_decode_decompacts() {
    // The convert.base64-decode read filter decodes 4-byte base64 quads
    // into 3 raw bytes. The runtime overwrites the buffer in place and
    // returns the shrunk byte count.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
fwrite($m, "SGVsbG8gV29ybGQ=");
rewind($m);
stream_filter_append($m, "convert.base64-decode", STREAM_FILTER_READ);
$s = fread($m, 64);
fclose($m);
echo "'" . $s . "' len=" . strlen($s);
"#,
    );
    assert_eq!(out, "'Hello World' len=11");
}

/// Verifies compiled PHP output for stream filter qp decode handles escapes and soft breaks.
#[test]
fn test_stream_filter_qp_decode_handles_escapes_and_soft_breaks() {
    // The convert.quoted-printable-decode read filter expands "=XX" hex
    // escapes into raw bytes and drops "=\r\n" / "=\n" soft line breaks.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
fwrite($m, "Caf=C3=A9 br=\n=C3=BBl=C3=A9");
rewind($m);
stream_filter_append($m, "convert.quoted-printable-decode", STREAM_FILTER_READ);
$s = fread($m, 64);
fclose($m);
echo "'" . $s . "' len=" . strlen($s);
"#,
    );
    assert_eq!(out, "'Café brûlé' len=13");
}

/// Verifies `string.strip_tags` is refused: php removed the filter in 8.0.
#[test]
fn test_stream_filter_strip_tags_is_not_a_php_filter() {
    // This test used to pin the opposite — `assert_eq!(out, "Hello World")` —
    // because elephc shipped a strip-tags state machine php has not had since
    // 8.0. php-src ext/standard/filters.c registers no `strip_tags` factory, so
    // the name must miss like any other unknown one.
    //
    // php 8.5.6 on this exact program:
    //   Warning: stream_filter_append(): Unable to locate filter "string.strip_tags" in ... on line 5
    //   bool(false)
    //   <p>Hello <b>World</b></p>
    // i.e. false, nothing attached, and the read comes back untouched.
    let out = compile_and_run_capture(
        r#"<?php
$m = fopen("php://memory", "r+");
fwrite($m, "<p>Hello <b>World</b></p>");
rewind($m);
var_dump(stream_filter_append($m, "string.strip_tags", STREAM_FILTER_READ));
echo fread($m, 64);
fclose($m);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\n<p>Hello <b>World</b></p>");
    assert!(
        out.diagnostics
            .contains("Unable to locate filter \"string.strip_tags\""),
        "expected php's unknown-filter warning, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies `consumed` attaches and passes every byte through, like php's filter.
#[test]
fn test_stream_filter_consumed_passes_bytes_through() {
    // php's `consumed` filter appends each bucket to its output brigade
    // unchanged and only counts bytes (php-src ext/standard/filters.c:1649-1653),
    // so a read that does not out-run the file comes back byte-for-byte. Before
    // this landed the attach warned `Unable to locate filter "consumed"` and
    // returned false, while `stream_get_filters()` advertised the name.
    //
    // php 8.5.6 on this exact program: `hel|lo |abcdef`.
    //
    // `ftell()` is deliberately not probed here: on a filtered stream elephc
    // reports the descriptor's own position (11) where php subtracts what it
    // still holds buffered (6). That gap is the chain's read-ahead, not this
    // filter's — `string.toupper` reproduces it identically — so it belongs to
    // its own fix.
    let out = compile_and_run(
        r#"<?php
$f = tempnam(sys_get_temp_dir(), "cns");
file_put_contents($f, "hello world");
$s = fopen($f, "r");
stream_filter_append($s, "consumed", STREAM_FILTER_READ);
echo fread($s, 3), "|", fread($s, 3), "|";
fclose($s);
unlink($f);
$o = tempnam(sys_get_temp_dir(), "cnw");
$w = fopen($o, "w");
stream_filter_append($w, "consumed", STREAM_FILTER_WRITE);
fwrite($w, "abcdef");
fclose($w);
echo file_get_contents($o);
unlink($o);
"#,
    );
    assert_eq!(out, "hel|lo |abcdef");
}

/// Verifies php's `$mode = 0` default reads the direction off the stream itself.
#[test]
fn test_stream_filter_mode_zero_deduces_direction_from_the_stream() {
    // php's default is 0, and 0 is not "no chain": php examines `stream->mode`
    // and enables the chains the stream can use (php-src
    // streamsfuncs.c:1202-1214). elephc passed 0 straight through, so an
    // explicit 0 linked the node into neither chain and this program printed
    // `abc|abc|xyz|` — the unfiltered bytes.
    //
    // php 8.5.6 on this exact program: `ABC|ABC|XYZ|`.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
fwrite($m, "abc");
rewind($m);
stream_filter_append($m, "string.toupper", 0);
echo fread($m, 10), "|";
fclose($m);
$f = tempnam(sys_get_temp_dir(), "md0");
file_put_contents($f, "abc");
$s = fopen($f, "r");
stream_filter_append($s, "string.toupper", 0);
echo fread($s, 10), "|";
fclose($s);
unlink($f);
$o = tempnam(sys_get_temp_dir(), "md0w");
$w = fopen($o, "w");
stream_filter_append($w, "string.toupper", 0);
fwrite($w, "xyz");
fclose($w);
echo file_get_contents($o), "|";
unlink($o);
"#,
    );
    assert_eq!(out, "ABC|ABC|XYZ|");
}

/// Verifies the deduced default follows the mode string, not the descriptor.
#[test]
fn test_stream_filter_default_mode_follows_the_open_mode_string() {
    // php reads `stream->mode`, so `a` selects the WRITE chain and `rb` the READ
    // chain even though both spellings collapse under `fcntl(F_GETFL)`.
    //
    // php 8.5.6 on this exact program: `APPENDED|rb-read|`.
    let out = compile_and_run(
        r#"<?php
$f = tempnam(sys_get_temp_dir(), "dms");
file_put_contents($f, "");
$a = fopen($f, "a");
stream_filter_append($a, "string.toupper");
fwrite($a, "appended");
fclose($a);
echo file_get_contents($f), "|";
file_put_contents($f, "RB-READ");
$r = fopen($f, "rb");
stream_filter_append($r, "string.tolower");
echo fread($r, 32), "|";
fclose($r);
unlink($f);
"#,
    );
    assert_eq!(out, "APPENDED|rb-read|");
}

/// Verifies every reader pulls through the read-filter chain, not past it.
#[test]
fn test_line_readers_apply_the_read_filter_chain() {
    // php attaches a read filter to the STREAM, so every reader that pulls
    // bytes out of it sees the filtered output: php-src `php_stream_read`
    // drains `readfilters` into `readbuf`, and `php_stream_get_line`,
    // `php_stream_getc`, `php_stream_passthru` and the CSV reader all consume
    // that same buffer.
    //
    // elephc had exactly one filtered reader. `__rt_fread` went through
    // `fread_filtered.rs`, so `fread`, `fgetc` and `stream_get_contents` were
    // right; `__rt_fgets` and `__rt_stream_get_line` issued their own
    // one-byte `read()` against the descriptor and `__rt_fpassthru` its own
    // chunked `read()`, so all of them handed back the RAW bytes. `fgetcsv`
    // and `fscanf` are built on `__rt_fgets` and inherited the same gap.
    // Nothing warned: the bytes were simply unfiltered.
    //
    // `feof()` came out wrong for the same reason. `__rt_stream_eof_get`
    // holds a filtered stream not-at-EOF until the chain has had its closing
    // dispatch, and only `__rt_fread` ever runs that dispatch — so a stream
    // drained purely by `fgets()` reported `false` forever.
    //
    // php 8.5.6 on this exact program:
    //   AB,CD
    //   |6|EF,GH
    //   |12|false|true|AB;CD|6|EF;GH|AB,CD|6|EF,GH|AB,CD
    //   EF,GH
    //   12|12|AB,CD|6|AB|2|
    let out = compile_and_run(
        r#"<?php
$f = tempnam(sys_get_temp_dir(), "flg");
file_put_contents($f, "ab,cd\nef,gh\n");
$s = fopen($f, "r");
stream_filter_append($s, "string.toupper", STREAM_FILTER_READ);
echo fgets($s), "|", ftell($s), "|", fgets($s), "|", ftell($s), "|";
echo var_export(fgets($s), true), "|", var_export(feof($s), true), "|";
fclose($s);
$s = fopen($f, "r");
stream_filter_append($s, "string.toupper", STREAM_FILTER_READ);
echo implode(";", fgetcsv($s, 0, ",", "\"", "")), "|", ftell($s), "|";
echo implode(";", fgetcsv($s, 0, ",", "\"", "")), "|";
fclose($s);
$s = fopen($f, "r");
stream_filter_append($s, "string.toupper", STREAM_FILTER_READ);
echo stream_get_line($s, 100, "\n"), "|", ftell($s), "|", stream_get_line($s, 100, "\n"), "|";
fclose($s);
$s = fopen($f, "r");
stream_filter_append($s, "string.toupper", STREAM_FILTER_READ);
echo fpassthru($s), "|", ftell($s), "|";
fclose($s);
$s = fopen($f, "r");
stream_filter_append($s, "string.toupper", STREAM_FILTER_READ);
echo implode(";", fscanf($s, "%s")), "|", ftell($s), "|";
fclose($s);
$s = fopen($f, "r");
stream_filter_append($s, "string.toupper", STREAM_FILTER_READ);
echo fgetc($s), fgetc($s), "|", ftell($s), "|";
fclose($s);
unlink($f);
"#,
    );
    assert_eq!(
        out,
        "AB,CD\n|6|EF,GH\n|12|false|true|AB;CD|6|EF;GH|AB,CD|6|EF,GH|AB,CD\nEF,GH\n12|12|AB,CD|6|AB|2|"
    );
}

/// Verifies a filtered line reader's position counts bytes SERVED, not consumed.
#[test]
fn test_filtered_line_reader_position_counts_bytes_served() {
    // `string.toupper` emits one byte per byte, so it cannot tell the two
    // rules apart: reading the descriptor and counting what the caller got
    // agree. `convert.base64-encode` does not — 8 input bytes become 12 —
    // and there php reports 12, the bytes it HANDED BACK.
    //
    // That is the rule `__rt_fread` already follows through
    // `STREAM_FILTERED_POS_OFFSET`. `fgets()` and `stream_get_line()` agreed
    // with php only by accident, because they read the descriptor directly;
    // routing them through the chain has to move them onto the same counter
    // or the accident becomes a divergence.
    //
    // The caller's `$length` bound still applies to the FILTERED bytes.
    //
    // php 8.5.6 on this exact program: `b25lCnR3bwo=|12|true|b25lCnR3bwo=|12|ON|2|E\n|4|`.
    let out = compile_and_run(
        r#"<?php
$f = tempnam(sys_get_temp_dir(), "flp");
file_put_contents($f, "one\ntwo\n");
$s = fopen($f, "r");
stream_filter_append($s, "convert.base64-encode", STREAM_FILTER_READ);
echo fgets($s), "|", ftell($s), "|", var_export(feof($s), true), "|";
fclose($s);
$s = fopen($f, "r");
stream_filter_append($s, "convert.base64-encode", STREAM_FILTER_READ);
echo stream_get_line($s, 100, "\n"), "|", ftell($s), "|";
fclose($s);
$s = fopen($f, "r");
stream_filter_append($s, "string.toupper", STREAM_FILTER_READ);
echo fgets($s, 3), "|", ftell($s), "|", fgets($s), "|", ftell($s), "|";
fclose($s);
unlink($f);
"#,
    );
    assert_eq!(out, "b25lCnR3bwo=|12|true|b25lCnR3bwo=|12|ON|2|E\n|4|");
}

/// Verifies a filtered `fgets()` respects chain order, direction and attach time.
#[test]
fn test_filtered_fgets_honours_chain_order_direction_and_attach_time() {
    // Three properties that a reader which merely "applies the filter" can
    // still get wrong, and that the `fread` path already holds:
    //   - the chain runs head-to-tail, so `one` uppercases to `ONE` and then
    //     rot13s to `BAR`, never the other way round;
    //   - a filter attached with STREAM_FILTER_WRITE is not on the read chain
    //     and must leave reads untouched;
    //   - a filter appended after bytes were already read applies from that
    //     point on, so line one stays `one` while line two becomes `TWO`.
    //
    // php 8.5.6 on this exact program: `BAR\n|GJB\n|one\n|one\n|TWO\n|`.
    let out = compile_and_run(
        r#"<?php
$f = tempnam(sys_get_temp_dir(), "flc");
file_put_contents($f, "one\ntwo\n");
$s = fopen($f, "r");
stream_filter_append($s, "string.toupper", STREAM_FILTER_READ);
stream_filter_append($s, "string.rot13", STREAM_FILTER_READ);
echo fgets($s), "|", fgets($s), "|";
fclose($s);
$s = fopen($f, "r");
stream_filter_append($s, "string.toupper", STREAM_FILTER_WRITE);
echo fgets($s), "|";
fclose($s);
$s = fopen($f, "r");
echo fgets($s), "|";
stream_filter_append($s, "string.toupper", STREAM_FILTER_READ);
echo fgets($s), "|";
fclose($s);
unlink($f);
"#,
    );
    assert_eq!(out, "BAR\n|GJB\n|one\n|one\n|TWO\n|");
}

/// Verifies a read filter on a USERSPACE-WRAPPER stream reaches the line readers too.
#[test]
fn test_read_filter_applies_to_a_userspace_wrapper_stream() {
    // A filter chain belongs to the stream, not to its backend, so it has to
    // outrank the backend when a reader picks where to pull bytes from. Each
    // line reader had a wrapper branch that read through `stream_read` and a
    // native branch that read the descriptor, and neither ran the chain: the
    // filtered branch has to be chosen FIRST, and then `__rt_fread_raw`
    // resolves descriptor-versus-wrapper underneath it.
    //
    // The unfiltered wrapper read is in here on purpose: routing filtered
    // streams away from the wrapper branch must not take the plain wrapper
    // reads with them.
    //
    // php 8.5.6 on this exact program: `ONE\n|TWO\n|one\n|ONE|ONE\nTWO\n8|`.
    let out = compile_and_run(
        r#"<?php
class W {
    public $context;
    public $pos = 0;
    public $data = "one\ntwo\n";
    public function stream_open($p, $m, $o, &$op) { return true; }
    public function stream_read($n) { $r = substr($this->data, $this->pos, $n); $this->pos += strlen($r); return $r; }
    public function stream_eof() { return $this->pos >= strlen($this->data); }
    public function stream_stat() { return []; }
    public function stream_tell() { return $this->pos; }
}
stream_wrapper_register("wtst", "W");
$s = fopen("wtst://x", "r");
stream_filter_append($s, "string.toupper", STREAM_FILTER_READ);
echo fgets($s), "|", fgets($s), "|";
fclose($s);
$s = fopen("wtst://x", "r");
echo fgets($s), "|";
fclose($s);
$s = fopen("wtst://x", "r");
stream_filter_append($s, "string.toupper", STREAM_FILTER_READ);
echo stream_get_line($s, 100, "\n"), "|";
fclose($s);
$s = fopen("wtst://x", "r");
stream_filter_append($s, "string.toupper", STREAM_FILTER_READ);
echo fpassthru($s), "|";
fclose($s);
"#,
    );
    assert_eq!(out, "ONE\n|TWO\n|one\n|ONE|ONE\nTWO\n8|");
}

/// Verifies compiled PHP output for stream filter dechunk parses chunked encoding.
#[test]
fn test_stream_filter_dechunk_parses_chunked_encoding() {
    // The dechunk read filter parses HTTP/1.1 chunked-transfer encoding:
    // hex size line, CRLF, payload, CRLF, then a zero chunk terminator.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
fwrite($m, "5\r\nHello\r\n6\r\n World\r\n0\r\n\r\n");
rewind($m);
stream_filter_append($m, "dechunk", STREAM_FILTER_READ);
echo fread($m, 64);
fclose($m);
"#,
    );
    assert_eq!(out, "Hello World");
}

/// Verifies compiled PHP output for stream get contents reads whole stream.
#[test]
fn test_stream_get_contents_reads_whole_stream() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("sgc.txt", "elephc stream contents");
$f = fopen("sgc.txt", "r");
echo stream_get_contents($f);
fclose($f);
unlink("sgc.txt");
"#,
    );
    assert_eq!(out, "elephc stream contents");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream get contents reads from current position.
#[test]
fn test_stream_get_contents_reads_from_current_position() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("sgc_pos.txt", "HEADERbody");
$f = fopen("sgc_pos.txt", "r");
fread($f, 6);
echo stream_get_contents($f);
fclose($f);
unlink("sgc_pos.txt");
"#,
    );
    assert_eq!(out, "body");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream get contents empty at eof.
#[test]
fn test_stream_get_contents_empty_at_eof() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("sgc_eof.txt", "x");
$f = fopen("sgc_eof.txt", "r");
fread($f, 10);
$rest = stream_get_contents($f);
echo "[" . $rest . "]" . strlen($rest);
fclose($f);
unlink("sgc_eof.txt");
"#,
    );
    assert_eq!(out, "[]0");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the optional `$length` and `$offset` arguments of
/// `stream_get_contents()`: a finite `$length` caps the read (`Hello`); an
/// `$offset >= 0` seeks before reading (`World` for length 5 from offset 7,
/// `World!` for read-all from offset 7); a negative/omitted `$length` reads to
/// EOF; and a capped read honors the current position after a prior `fread`
/// (`llo`). Output matches PHP 8.5 byte-for-byte (verified via `php -r`).
#[test]
fn test_stream_get_contents_length_and_offset() {
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
fwrite($m, "Hello, World!");
rewind($m);
echo "[" . stream_get_contents($m, 5) . "]";
rewind($m);
echo "[" . stream_get_contents($m, 5, 7) . "]";
rewind($m);
echo "[" . stream_get_contents($m, -1, 7) . "]";
rewind($m);
echo "[" . stream_get_contents($m) . "]";
rewind($m);
fread($m, 2);
echo "[" . stream_get_contents($m, 3) . "]";
fclose($m);
"#,
    );
    assert_eq!(out, "[Hello][World][World!][Hello, World!][llo]");
}

/// Verifies `stream_get_contents()` returns `false` when a positive offset
/// fails through a user wrapper's `stream_seek`, matching PHP's failure result.
#[test]
fn test_stream_get_contents_offset_seek_failure_is_false() {
    let out = compile_and_run(
        r#"<?php
class NoSeekGetW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
    public function stream_seek(int $offset, int $whence): bool { return false; }
    public function stream_read(int $n): string { return "abc"; }
    public function stream_eof(): bool { return true; }
}
stream_wrapper_register("noseekget", "NoSeekGetW");
$f = fopen("noseekget://x", "r");
$r = stream_get_contents($f, null, 1);
echo $r === false ? "false" : "got";
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies finite `stream_get_contents()` on a user wrapper keeps reading
/// smaller chunks until the requested length is filled without draining the
/// rest of the wrapper stream.
#[test]
fn test_stream_get_contents_bounded_wrapper_read_fills_length() {
    let out = compile_and_run(
        r#"<?php
class SlowW {
    public $data; public $pos;
    public function stream_open($p,$m,$o,&$op): bool { $this->data="abcdefghi"; $this->pos=0; return true; }
    public function stream_read($n): string { $c=substr($this->data,$this->pos,min(2,$n)); $this->pos+=strlen($c); return $c; }
    public function stream_eof(): bool { return $this->pos>=strlen($this->data); }
    public function stream_close(): void {}
}
stream_wrapper_register("slow","SlowW");
$f=fopen("slow://x","r");
echo stream_get_contents($f,5);
echo "|" . stream_get_contents($f);
fclose($f);
"#,
    );
    assert_eq!(out, "abcde|fghi");
}

/// Verifies a runtime-computed negative length follows PHP's read-all contract
/// instead of being treated as a finite negative cap.
#[test]
fn test_stream_get_contents_runtime_negative_length_reads_all() {
    let out = compile_and_run(
        r#"<?php
function neg_one(): int { return -1; }
$m = fopen("php://memory", "r+");
fwrite($m, "runtime-all");
rewind($m);
echo stream_get_contents($m, neg_one());
fclose($m);
"#,
    );
    assert_eq!(out, "runtime-all");
}

/// Verifies compiled PHP output for stream copy to stream copies all bytes.
#[test]
fn test_stream_copy_to_stream_copies_all_bytes() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("scts_src.txt", "copy me through a stream");
$from = fopen("scts_src.txt", "r");
$to = fopen("scts_dst.txt", "w");
$n = stream_copy_to_stream($from, $to);
fclose($from);
fclose($to);
echo $n . ":" . file_get_contents("scts_dst.txt");
unlink("scts_src.txt");
unlink("scts_dst.txt");
"#,
    );
    assert_eq!(out, "24:copy me through a stream");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream copy to stream resumes from position.
#[test]
fn test_stream_copy_to_stream_resumes_from_position() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("scts_p_src.txt", "SKIPkeep");
$from = fopen("scts_p_src.txt", "r");
fread($from, 4);
$to = fopen("scts_p_dst.txt", "w");
$n = stream_copy_to_stream($from, $to);
fclose($from);
fclose($to);
echo $n . ":" . file_get_contents("scts_p_dst.txt");
unlink("scts_p_src.txt");
unlink("scts_p_dst.txt");
"#,
    );
    assert_eq!(out, "4:keep");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream copy to stream empty source.
#[test]
fn test_stream_copy_to_stream_empty_source() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("scts_e_src.txt", "");
$from = fopen("scts_e_src.txt", "r");
$to = fopen("scts_e_dst.txt", "w");
echo stream_copy_to_stream($from, $to);
fclose($from);
fclose($to);
unlink("scts_e_src.txt");
unlink("scts_e_dst.txt");
"#,
    );
    assert_eq!(out, "0");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the optional `$length` and `$offset` arguments of
/// `stream_copy_to_stream()`: a finite `$length` caps the copy (`Hello`, 5
/// bytes); an `$offset >= 0` seeks the source first (`World` for length 5 from
/// offset 7); and a negative `$length` from an offset copies to EOF (`World!`,
/// 6 bytes). Byte counts and contents match PHP 8.5 (verified via `php -r`).
#[test]
fn test_stream_copy_to_stream_length_and_offset() {
    let out = compile_and_run(
        r#"<?php
$s = fopen("php://memory", "r+"); fwrite($s, "Hello, World!"); rewind($s);
$d = fopen("php://memory", "r+");
$n = stream_copy_to_stream($s, $d, 5);
rewind($d);
echo "[" . $n . ":" . stream_get_contents($d) . "]";

$s2 = fopen("php://memory", "r+"); fwrite($s2, "Hello, World!"); rewind($s2);
$d2 = fopen("php://memory", "r+");
$n2 = stream_copy_to_stream($s2, $d2, 5, 7);
rewind($d2);
echo "[" . $n2 . ":" . stream_get_contents($d2) . "]";

$s3 = fopen("php://memory", "r+"); fwrite($s3, "Hello, World!"); rewind($s3);
$d3 = fopen("php://memory", "r+");
$n3 = stream_copy_to_stream($s3, $d3, -1, 7);
rewind($d3);
echo "[" . $n3 . ":" . stream_get_contents($d3) . "]";
"#,
    );
    assert_eq!(out, "[5:Hello][5:World][6:World!]");
}

/// Verifies `stream_copy_to_stream()` returns `false` when a positive offset
/// fails through a user wrapper's `stream_seek`, matching PHP's failure result.
#[test]
fn test_stream_copy_to_stream_offset_seek_failure_is_false() {
    let out = compile_and_run(
        r#"<?php
class NoSeekCopyW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
    public function stream_seek(int $offset, int $whence): bool { return false; }
    public function stream_read(int $n): string { return "abc"; }
    public function stream_eof(): bool { return true; }
}
stream_wrapper_register("noseekcopy", "NoSeekCopyW");
$src = fopen("noseekcopy://x", "r");
$dst = fopen("php://memory", "r+");
$n = stream_copy_to_stream($src, $dst, null, 1);
echo $n === false ? "false" : "got";
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies a runtime-computed negative length copies to EOF, matching PHP's
/// default `-1` length semantics.
#[test]
fn test_stream_copy_to_stream_runtime_negative_length_copies_all() {
    let out = compile_and_run(
        r#"<?php
function neg_one(): int { return -1; }
$s = fopen("php://memory", "r+");
$d = fopen("php://memory", "r+");
fwrite($s, "copy-runtime-all");
rewind($s);
$n = stream_copy_to_stream($s, $d, neg_one());
rewind($d);
echo $n . ":" . stream_get_contents($d);
fclose($s);
fclose($d);
"#,
    );
    assert_eq!(out, "16:copy-runtime-all");
}

/// Verifies finite `stream_copy_to_stream()` copies from a wrapper source that
/// returns smaller chunks than requested.
#[test]
fn test_stream_copy_to_stream_bounded_wrapper_read_fills_length() {
    let out = compile_and_run(
        r#"<?php
class SlowCopyW {
    public $data; public $pos;
    public function stream_open($p,$m,$o,&$op): bool { $this->data="abcdefghi"; $this->pos=0; return true; }
    public function stream_read($n): string { $c=substr($this->data,$this->pos,2); $this->pos+=strlen($c); return $c; }
    public function stream_eof(): bool { return $this->pos>=strlen($this->data); }
    public function stream_close(): void {}
}
stream_wrapper_register("slowcopy","SlowCopyW");
$src=fopen("slowcopy://x","r");
$dst=fopen("php://memory","r+");
$n=stream_copy_to_stream($src,$dst,5);
rewind($dst);
echo $n . ":" . stream_get_contents($dst);
fclose($src);
fclose($dst);
"#,
    );
    assert_eq!(out, "5:abcde");
}

/// Verifies compiled PHP output for fopen php stdout writes to stdout.
#[test]
fn test_fopen_php_stdout_writes_to_stdout() {
    let out =
        compile_and_run(r#"<?php $h = fopen("php://stdout", "w"); fwrite($h, "via php-wrapper");"#);
    assert_eq!(out, "via php-wrapper");
}

/// Verifies closing a `php://stdout` handle leaves the program's own stdout usable.
///
/// The wrapper used to hand back descriptor 1 itself, so `fclose()` closed the process's
/// standard output: `after` was written to a closed descriptor and vanished, while the
/// program still exited 0 — output loss with no diagnostic anywhere. php-src duplicates
/// the descriptor in `php_fopen_wrapper.c`, and reference PHP 8.5.6 prints both lines.
///
/// The `before` line is asserted too: a wrapper that failed to open at all would drop
/// only the `via-handle` write and still print `after`, passing a test that pinned the
/// tail alone.
#[test]
fn test_closing_php_stdout_leaves_the_process_stdout_open() {
    let out = compile_and_run(
        r#"<?php
$h = fopen("php://stdout", "w");
echo "before\n";
fwrite($h, "via-handle\n");
fclose($h);
echo "after\n";
"#,
    );
    assert_eq!(out, "before\nvia-handle\nafter\n");
}

/// Verifies `php://output` reaches the terminal when no output buffer is active.
///
/// Renamed from `..._is_stdout_alias`: the old name asserted a relationship that php does not
/// have. `php://output` and `php://stdout` agree ONLY while the output-buffer stack is empty,
/// which is all this case exercises; the two tests below pin where they part company.
#[test]
fn test_fopen_php_output_reaches_the_terminal_unbuffered() {
    let out = compile_and_run(r#"<?php $h = fopen("php://output", "w"); fwrite($h, "aliased");"#);
    assert_eq!(out, "aliased");
}

/// Verifies `php://output` writes travel the OUTPUT-BUFFER stack, and `php://stdout` does not.
///
/// php-src gives `php://output` its own `php_stream_output_ops`, whose write is `php_output_write`
/// — the sink `echo` uses — while `php://stdout` is a `dup()` of descriptor 1. elephc aliased both
/// onto descriptor 1, so `ob_start()` never saw a `php://output` write.
///
/// RED before the fix (`php -n` 8.5.6 on the left, elephc on the right):
///   A: `string(15) "CAPTURED-OUTPUT"`  vs  `CAPTURED-OUTPUT` printed, then `string(0) ""`
///   D: `string(13) "BEFORE-HANDLE"`    vs  `BEFORE-HANDLE`   printed, then `string(0) ""`
/// `php://stdout` (B) already matched and must keep matching, which is why it is asserted here
/// rather than in a test of its own: the fix has to move ONE of the two.
#[test]
fn test_php_output_is_captured_by_ob_and_php_stdout_is_not() {
    let out = compile_and_run(
        r#"<?php
ob_start();
$o = fopen("php://output", "w");
fwrite($o, "CAPTURED");
fclose($o);
echo "[out=" . ob_get_clean() . "]";

ob_start();
$s = fopen("php://stdout", "w");
fwrite($s, "DIRECT");
fclose($s);
echo "[std=" . ob_get_clean() . "]";
"#,
    );
    assert_eq!(out, "[out=CAPTURED]DIRECT[std=]");
}

/// Verifies a `php://output` handle opened BEFORE `ob_start()` is still captured.
///
/// The sink is a property of the stream, not of the moment it was opened, so php captures a
/// handle that predates the buffer. Measured: `string(13) "BEFORE-HANDLE"`. A fix that only
/// consulted the buffer depth at open time would pass the test above and fail this one.
#[test]
fn test_php_output_handle_opened_before_ob_start_is_still_captured() {
    let out = compile_and_run(
        r#"<?php
$h = fopen("php://output", "w");
ob_start();
fwrite($h, "BEFORE-HANDLE");
echo "[" . ob_get_clean() . "]";
fclose($h);
"#,
    );
    assert_eq!(out, "[BEFORE-HANDLE]");
}

/// Verifies php never gates a write on a DESCRIPTOR-backed `php://` target's mode string.
///
/// php-src's `_php_stream_write` refuses only when the stream's ops have no write function; the
/// mode is never read back. So `fopen("php://stdout","r")` writes, and so does `php://fd/1`
/// opened `"rb"`. elephc's read-only gate refused both.
///
/// RED before the fix: `php -n` 8.5.6 answers `2` for every line below; elephc answered
/// `false` for the two `r`-flavoured opens and printed nothing for them.
///
/// The in-memory targets deliberately keep the gate — php builds `php://memory` read-only when
/// the mode names none of `w`, `a`, `+` — and the last two lines pin that they still do.
#[test]
fn test_read_mode_does_not_gate_writes_to_descriptor_backed_php_targets() {
    let out = compile_and_run(
        r#"<?php
foreach (["r", "rb", "w"] as $m) {
    $h = fopen("php://stdout", $m);
    echo "[", var_export(fwrite($h, "S"), true), "]";
    fclose($h);
}
foreach (["r", "rb"] as $m) {
    $h = fopen("php://fd/1", $m);
    echo "[", var_export(fwrite($h, "F"), true), "]";
    fclose($h);
}
$mem = fopen("php://memory", "r");
echo "[", var_export(fwrite($mem, "M"), true), "]";
fclose($mem);
$t = fopen("php://temp", "rb");
echo "[", var_export(fwrite($t, "T"), true), "]";
fclose($t);
"#,
    );
    assert_eq!(out, "[S1][S1][S1][F1][F1][false][false]");
}

/// Verifies compiled PHP output for fopen php stream yields resource.
#[test]
fn test_fopen_php_stream_yields_resource() {
    let out = compile_and_run(
        r#"<?php $h = fopen("php://stderr", "w"); echo is_resource($h) ? "y" : "n"; echo get_resource_type($h);"#,
    );
    assert_eq!(out, "ystream");
}

/// Verifies compiled PHP output for fopen php memory round trip.
#[test]
fn test_fopen_php_memory_round_trip() {
    // php://memory is a writable, seekable in-memory stream.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
fwrite($m, "memory contents");
rewind($m);
echo fread($m, 64);
fclose($m);
"#,
    );
    assert_eq!(out, "memory contents");
}

/// Verifies compiled PHP output for fopen php temp seek and tell.
#[test]
fn test_fopen_php_temp_seek_and_tell() {
    // php://temp behaves like php://memory; fseek/ftell work on it.
    let out = compile_and_run(
        r#"<?php
$t = fopen("php://temp", "w+");
fwrite($t, "0123456789");
fseek($t, 4);
echo fread($t, 3);
echo "|";
echo ftell($t);
fclose($t);
"#,
    );
    assert_eq!(out, "456|7");
}

/// Verifies compiled PHP output for fopen data uri base64.
#[test]
fn test_fopen_data_uri_base64() {
    // data:// with ;base64 decodes the payload at compile time.
    let out = compile_and_run(
        r#"<?php
$d = fopen("data://text/plain;base64,SGVsbG8gd29ybGQ=", "r");
echo fread($d, 64);
fclose($d);
"#,
    );
    assert_eq!(out, "Hello world");
}

/// Verifies compiled PHP output for fopen data uri percent encoded.
#[test]
fn test_fopen_data_uri_percent_encoded() {
    // A non-base64 data:// payload is percent-decoded (%HH and + → space).
    let out = compile_and_run(
        r#"<?php
$d = fopen("data://text/plain,Hello%20raw%2Bworld", "r");
echo fread($d, 64);
fclose($d);
"#,
    );
    assert_eq!(out, "Hello raw+world");
}

/// Verifies compiled PHP output for fopen data uri invalid returns false.
#[test]
fn test_fopen_data_uri_invalid_returns_false() {
    // A data:// URI without the mandatory comma fails like any bad fopen().
    let out = compile_and_run(
        r#"<?php $d = fopen("data://no-comma-here", "r"); echo is_bool($d) ? "false" : "resource";"#,
    );
    assert_eq!(out, "false");
}

/// One PHAR entry for the test builder: archive name, recorded uncompressed
/// size, the bytes as stored in the data section, and the entry flag word.
struct TestPharEntry<'a> {
    name: &'a str,
    uncompressed_size: u32,
    stored: &'a [u8],
    flags: u32,
}

// Precomputed bzip2 blob for `"bzip2-compressed phar entry. "` repeated eight
// times. bzip2-rs is decode-only, so tests keep this stable fixture inline.
const BZIP2_PHAR_BLOB: &[u8] = &[
    0x42, 0x5a, 0x68, 0x39, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59, 0x61, 0x39,
    0xa6, 0xe8, 0x00, 0x00, 0x1f, 0x99, 0x80, 0x40, 0x03, 0x10, 0x00, 0x3e,
    0x63, 0xdc, 0x30, 0x20, 0x00, 0x70, 0x53, 0x09, 0xa6, 0x80, 0xd3, 0x10,
    0x2a, 0xa8, 0x0c, 0x43, 0x46, 0x1a, 0x9b, 0x0b, 0x0a, 0x0e, 0x46, 0x45,
    0xc5, 0x44, 0xc5, 0x05, 0x46, 0x06, 0xe3, 0xa1, 0x21, 0x03, 0x22, 0x42,
    0xc2, 0xe2, 0x63, 0x02, 0xe2, 0x82, 0x07, 0x82, 0x82, 0x05, 0x44, 0x0f,
    0xc5, 0xdc, 0x91, 0x4e, 0x14, 0x24, 0x18, 0x4e, 0x69, 0xba, 0x00,
];

/// Builds a native-format PHAR (PHP stub + manifest + data section) from
/// explicit per-entry stored bytes and flags, matching the byte layout PHP's
/// `Phar` class produces. crc32 and signature are omitted because the reader
/// ignores them. Lets the `phar://` codegen tests exercise uncompressed and
/// gzip (raw-DEFLATE) entries as deterministic, php-free fixtures.
fn build_phar(entries: &[TestPharEntry]) -> Vec<u8> {
    let mut manifest = Vec::new();
    manifest.extend_from_slice(&(entries.len() as u32).to_le_bytes()); // num_files
    manifest.extend_from_slice(&[0x11, 0x00]); // api version (1.1.0)
    manifest.extend_from_slice(&0u32.to_le_bytes()); // global bitmapped flags
    manifest.extend_from_slice(&0u32.to_le_bytes()); // alias length (none)
    manifest.extend_from_slice(&0u32.to_le_bytes()); // manifest metadata length (none)
    for e in entries {
        manifest.extend_from_slice(&(e.name.len() as u32).to_le_bytes());
        manifest.extend_from_slice(e.name.as_bytes());
        manifest.extend_from_slice(&e.uncompressed_size.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes()); // timestamp
        manifest.extend_from_slice(&(e.stored.len() as u32).to_le_bytes()); // compressed size
        manifest.extend_from_slice(&0u32.to_le_bytes()); // crc32 (ignored by the reader)
        manifest.extend_from_slice(&e.flags.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes()); // entry metadata length (none)
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"<?php __HALT_COMPILER(); ?>\r\n");
    out.extend_from_slice(&(manifest.len() as u32).to_le_bytes()); // manifest length
    out.extend_from_slice(&manifest);
    for e in entries {
        out.extend_from_slice(e.stored); // data section: entries in manifest order
    }
    out
}

/// Convenience over [`build_phar`] for plain uncompressed entries (mode 0644).
fn build_minimal_phar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let raw: Vec<TestPharEntry> = entries
        .iter()
        .map(|(name, content)| TestPharEntry {
            name,
            uncompressed_size: content.len() as u32,
            stored: content,
            flags: 0x0000_01a4,
        })
        .collect();
    build_phar(&raw)
}

/// Builds a minimal POSIX tar archive with regular-file entries.
fn build_tar_phar_container(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, content) in entries {
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        let size = format!("{:011o}\0", content.len());
        header[124..124 + size.len()].copy_from_slice(size.as_bytes());
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        for byte in &mut header[148..156] {
            *byte = b' ';
        }
        let checksum: u32 = header.iter().map(|&b| b as u32).sum();
        let checksum = format!("{:06o}\0 ", checksum);
        header[148..156].copy_from_slice(checksum.as_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(content);
        let padded_len = ((content.len() + 511) / 512) * 512;
        out.resize(out.len() + padded_len - content.len(), 0);
    }
    out.extend_from_slice(&[0u8; 1024]);
    out
}

/// Builds a ZIP archive with ordinary store/deflate entries and a central directory.
fn build_zip_phar_container(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, content, deflate) in entries {
        let local_offset = out.len() as u32;
        let stored = if *deflate {
            let mut encoder =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            std::io::Write::write_all(&mut encoder, content).unwrap();
            encoder.finish().unwrap()
        } else {
            content.to_vec()
        };
        let method = if *deflate { 8u16 } else { 0u16 };
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(stored.len() as u32).to_le_bytes());
        out.extend_from_slice(&(content.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&stored);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&(stored.len() as u32).to_le_bytes());
        central.extend_from_slice(&(content.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&local_offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let central_offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// Verifies compiled PHP output for fopen phar reads uncompressed entry.
#[test]
fn test_fopen_phar_reads_uncompressed_entry() {
    // fopen("phar://archive/entry") reads the named uncompressed entry out of the
    // archive at compile time and serves it as a readable stream. Covers a
    // top-level entry, a nested entry (exercising the cumulative data-offset
    // walk), and a missing entry lowering to false. The archive path must be a
    // literal, so the fixture is written to an absolute temp path embedded below.
    let phar = build_minimal_phar(&[
        ("hello.txt", b"Hello from phar!\n"),
        ("dir/inner.txt", b"inner content here"),
    ]);
    let path = std::env::temp_dir().join(format!("elephc_phar_m1_read_{}.phar", std::process::id()));
    std::fs::write(&path, &phar).unwrap();
    let src = format!(
        r#"<?php
$f = fopen("phar://{p}/hello.txt", "r");
echo fread($f, 100);
fclose($f);
$g = fopen("phar://{p}/dir/inner.txt", "r");
echo "[" . fread($g, 100) . "]";
fclose($g);
$m = @fopen("phar://{p}/nope.txt", "r");
echo "|" . ($m === false ? "false" : "open");
"#,
        p = path.display()
    );
    let out = compile_and_run(&src);
    std::fs::remove_file(&path).ok();
    assert_eq!(out, "Hello from phar!\n[inner content here]|false");
}

/// Verifies a literal `phar://` `file_get_contents()` honors PHP's `$offset`/`$length` window.
///
/// The entry bytes are extracted at COMPILE time and served from read-only `.data`, so the
/// windowing path — which trims its input in place and frees a failed read — must copy them into
/// an owned string first. Without that copy the trim would move and free a rodata pointer.
#[test]
fn test_file_get_contents_literal_phar_entry_honors_offset_and_length() {
    let phar = build_minimal_phar(&[("hello.txt", b"Hello from phar!\n")]);
    let path =
        std::env::temp_dir().join(format!("elephc_phar_fgc_range_{}.phar", std::process::id()));
    std::fs::write(&path, &phar).unwrap();
    let src = format!(
        r#"<?php
var_dump(file_get_contents("phar://{p}/hello.txt"));
var_dump(file_get_contents("phar://{p}/hello.txt", false, null, 6, 4));
var_dump(file_get_contents("phar://{p}/hello.txt", false, null, -6, 5));
var_dump(@file_get_contents("phar://{p}/hello.txt", false, null, -99));
"#,
        p = path.display()
    );
    let out = compile_and_run(&src);
    std::fs::remove_file(&path).ok();
    assert_eq!(
        out,
        "string(17) \"Hello from phar!\n\"\nstring(4) \"from\"\nstring(5) \"phar!\"\nbool(false)\n"
    );
}

/// Runtime phar:// read: when the archive path arrives via a variable (not a
/// compile-time literal), `fopen` routes through `__rt_fopen_maybe_phar` →
/// `__rt_phar_read_entry`, which reads and parses the archive at run time and
/// materializes the entry as a readable stream. Reads the nested (2nd) entry to
/// validate the cumulative data-offset walk, and a missing entry → false.
#[test]
fn test_fopen_phar_runtime_path_reads_entry() {
    let phar = build_minimal_phar(&[
        ("hello.txt", b"Hello from phar!\n"),
        ("dir/inner.txt", b"inner content here"),
    ]);
    let path = std::env::temp_dir().join(format!("elephc_phar_m2_rt_{}.phar", std::process::id()));
    std::fs::write(&path, &phar).unwrap();
    let src = format!(
        r#"<?php
$p = "{p}";
$f = fopen("phar://" . $p . "/dir/inner.txt", "r");
echo fread($f, 100);
fclose($f);
$m = @fopen("phar://" . $p . "/nope.txt", "r");
echo "|" . ($m === false ? "false" : "open");
"#,
        p = path.display()
    );
    let out = compile_and_run(&src);
    std::fs::remove_file(&path).ok();
    assert_eq!(out, "inner content here|false");
}

/// phar:// write Milestone 1: `fopen("phar://...","w")` + `fwrite` + `fclose`
/// assembles a valid single-entry uncompressed phar that sets the
/// PHAR_HDR_SIGNATURE (0x10000) global flag and appends a SHA1 signature
/// trailer (`raw-sha1 ++ LE32(0x0002) ++ "GBMB"`), so real PHP — which requires
/// a hash by default — accepts the archive. elephc's own phar reader is
/// compile-time (it reads the archive during compilation), so a runtime-written
/// archive can't be read back in the same program; this test verifies the
/// on-disk bytes directly. (Manually confirmed that real PHP's `new Phar(...)`
/// reads the entry back.)
#[test]
fn test_fopen_phar_write_signs_single_entry() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("phar://out.phar/hello.txt", "w");
$n = fwrite($f, "payload-data");
echo (fclose($f) ? "ok" : "fail") . $n;
"#,
    );
    assert_eq!(out, "ok12");
    let bytes = fs::read(dir.join("out.phar")).expect("phar archive written");
    let _ = fs::remove_dir_all(&dir);
    // Global manifest flags carry PHAR_HDR_SIGNATURE (0x00010000) at offset 39
    // (29-byte stub + manifest_len(4) + num_files(4) + api_version(2)).
    assert_eq!(
        &bytes[39..43],
        &[0x00, 0x00, 0x01, 0x00],
        "PHAR_HDR_SIGNATURE flag not set"
    );
    // Signature trailer: <20 raw SHA1 bytes> ++ LE32(0x0002 = Phar::SHA1) ++ "GBMB".
    let tail = &bytes[bytes.len() - 8..];
    assert_eq!(&tail[0..4], &[0x02, 0x00, 0x00, 0x00], "signature type not SHA1");
    assert_eq!(&tail[4..8], b"GBMB", "phar magic missing");
}

/// `file_put_contents("phar://archive/entry", $data)` writes a signed
/// single-entry phar in one call (reusing the fopen-write runtime), returning
/// the byte count. Verifies the returned count and the on-disk signature bytes.
/// (Manually confirmed real PHP reads the entry back.)
#[test]
fn test_file_put_contents_phar_writes_signed_entry() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
echo file_put_contents("phar://out.phar/note.txt", "via fpc");
"#,
    );
    assert_eq!(out, "7"); // strlen("via fpc")
    let bytes = fs::read(dir.join("out.phar")).expect("phar archive written");
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(
        &bytes[39..43],
        &[0x00, 0x00, 0x01, 0x00],
        "PHAR_HDR_SIGNATURE flag not set"
    );
    let tail = &bytes[bytes.len() - 8..];
    assert_eq!(&tail[0..4], &[0x02, 0x00, 0x00, 0x00], "signature type not SHA1");
    assert_eq!(&tail[4..8], b"GBMB", "phar magic missing");
}

/// EIR phar:// write streams seed the runtime PHAR writer instead of falling
/// through to a literal filesystem path named `phar://...`.
#[test]
fn test_fopen_phar_write_runtime_readback() {
    let out = compile_and_run(
        r#"<?php
$f = fopen("phar://streamed.phar/hello.txt", "w");
echo fwrite($f, "streamed") . "|";
echo (fclose($f) ? "closed" : "failed") . "|";
$archive = "streamed.phar";
echo file_get_contents("phar://" . $archive . "/hello.txt");
"#,
    );
    assert_eq!(out, "8|closed|streamed");
}

/// EIR one-shot phar:// writes use the same signed archive runtime as
/// `fopen()` + `fwrite()` + `fclose()` and are readable through a runtime URL.
#[test]
fn test_file_put_contents_phar_runtime_readback() {
    let out = compile_and_run(
        r#"<?php
echo file_put_contents("phar://single.phar/note.txt", "via fpc") . "|";
$archive = "single.phar";
echo file_get_contents("phar://" . $archive . "/note.txt");
"#,
    );
    assert_eq!(out, "7|via fpc");
}

/// Repeated phar:// file_put_contents() calls update a native PHAR in place,
/// preserving previously written entries instead of rewriting a single-entry archive.
#[test]
fn test_file_put_contents_phar_preserves_existing_entries() {
    let out = compile_and_run(
        r#"<?php
echo file_put_contents("phar://multi.phar/one.txt", "alpha") . "|";
echo file_put_contents("phar://multi.phar/dir/two.txt", "bravo") . "|";
echo file_put_contents("phar://multi.phar/one.txt", "updated") . "|";
$archive = "multi.phar";
echo file_get_contents("phar://" . $archive . "/one.txt") . "|";
echo file_get_contents("phar://" . $archive . "/dir/two.txt");
"#,
    );
    assert_eq!(out, "5|5|7|updated|bravo");
}

/// fopen()+fwrite()+fclose() phar:// writes also use the native PHAR
/// read-modify-write bridge, so stream writes preserve earlier entries.
#[test]
fn test_fopen_phar_write_preserves_existing_entries() {
    let out = compile_and_run(
        r#"<?php
echo file_put_contents("phar://stream_multi.phar/one.txt", "alpha") . "|";
$f = fopen("phar://stream_multi.phar/two.txt", "w");
echo fwrite($f, "stream") . "|";
echo (fclose($f) ? "closed" : "failed") . "|";
$archive = "stream_multi.phar";
echo file_get_contents("phar://" . $archive . "/one.txt") . "|";
echo file_get_contents("phar://" . $archive . "/two.txt");
"#,
    );
    assert_eq!(out, "5|6|closed|alpha|stream");
}

/// Runtime-built phar:// URLs passed to file_put_contents() route through the
/// native PHAR URL bridge instead of writing a literal filesystem path.
#[test]
fn test_file_put_contents_dynamic_phar_url_preserves_existing_entries() {
    let out = compile_and_run(
        r#"<?php
$archive = "dynamic_multi.phar";
echo file_put_contents("phar://" . $archive . "/one.txt", "alpha") . "|";
echo file_put_contents("phar://" . $archive . "/dir/two.txt", "bravo") . "|";
echo file_get_contents("phar://" . $archive . "/one.txt") . "|";
echo file_get_contents("phar://" . $archive . "/dir/two.txt");
"#,
    );
    assert_eq!(out, "5|5|alpha|bravo");
}

/// Runtime-built phar:// URLs passed to write-mode fopen() preserve the full URL
/// until fclose(), then update the native PHAR through the URL bridge.
#[test]
fn test_fopen_dynamic_phar_write_preserves_existing_entries() {
    let out = compile_and_run(
        r#"<?php
$archive = "dynamic_stream.phar";
echo file_put_contents("phar://" . $archive . "/one.txt", "alpha") . "|";
$f = fopen("phar://" . $archive . "/dir/two.txt", "w");
echo fwrite($f, "stream") . "|";
echo (fclose($f) ? "closed" : "failed") . "|";
echo file_get_contents("phar://" . $archive . "/one.txt") . "|";
echo file_get_contents("phar://" . $archive . "/dir/two.txt");
"#,
    );
    assert_eq!(out, "5|6|closed|alpha|stream");
}

/// Concurrent phar:// write streams keep independent payload buffers and
/// finalize through their own descriptors, including mixed literal/dynamic URLs.
#[test]
fn test_fopen_concurrent_phar_write_streams_preserve_entries() {
    let out = compile_and_run(
        r#"<?php
$archive = "concurrent_streams.phar";
$one = fopen("phar://concurrent_streams.phar/one.txt", "w");
$two = fopen("phar://" . $archive . "/two.txt", "w");
echo fwrite($two, "bravo") . "|";
echo fwrite($one, "alpha") . "|";
echo (fclose($one) ? "one" : "fail-one") . "|";
echo (fclose($two) ? "two" : "fail-two") . "|";
echo file_get_contents("phar://" . $archive . "/one.txt") . "|";
echo file_get_contents("phar://" . $archive . "/two.txt");
"#,
    );
    assert_eq!(out, "5|5|one|two|alpha|bravo");
}

/// `phar://` writes to a `.tar` archive create/update a tar container through
/// the Rust bridge, and the runtime reader can read both entries back.
#[test]
fn test_file_put_contents_phar_tar_archive_runtime_readback() {
    let out = compile_and_run(
        r#"<?php
echo file_put_contents("phar://out.tar/one.txt", "alpha") . "|";
echo file_put_contents("phar://out.tar/dir/two.txt", "bravo") . "|";
$archive = "out.tar";
echo file_get_contents("phar://" . $archive . "/one.txt") . "|";
echo file_get_contents("phar://" . $archive . "/dir/two.txt");
"#,
    );
    assert_eq!(out, "5|5|alpha|bravo");
}

/// `phar://` writes to a `.zip` archive create/update a ZIP container through
/// the Rust bridge, and the runtime reader can read both entries back.
#[test]
fn test_file_put_contents_phar_zip_archive_runtime_readback() {
    let out = compile_and_run(
        r#"<?php
echo file_put_contents("phar://out.zip/one.txt", "alpha") . "|";
echo file_put_contents("phar://out.zip/dir/two.txt", "bravo") . "|";
$archive = "out.zip";
echo file_get_contents("phar://" . $archive . "/one.txt") . "|";
echo file_get_contents("phar://" . $archive . "/dir/two.txt");
"#,
    );
    assert_eq!(out, "5|5|alpha|bravo");
}

/// `unlink("phar://...")` removes one archive entry while preserving sibling
/// entries across native PHAR, tar, and ZIP containers.
#[test]
fn test_unlink_phar_entries_preserves_siblings() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("phar://delete.phar/one.txt", "alpha");
file_put_contents("phar://delete.phar/two.txt", "bravo");
echo (unlink("phar://delete.phar/one.txt") ? "u|" : "bad|");
$phar = "delete.phar";
echo (file_get_contents("phar://" . $phar . "/one.txt") === false ? "missing|" : "bad|");
echo file_get_contents("phar://" . $phar . "/two.txt") . "|";
file_put_contents("phar://delete.tar/one.txt", "tar-one");
file_put_contents("phar://delete.tar/two.txt", "tar-two");
echo (unlink("phar://delete.tar/one.txt") ? "u|" : "bad|");
$tar = "delete.tar";
echo file_get_contents("phar://" . $tar . "/two.txt") . "|";
file_put_contents("phar://delete.zip/one.txt", "zip-one");
file_put_contents("phar://delete.zip/two.txt", "zip-two");
echo (unlink("phar://delete.zip/one.txt") ? "u|" : "bad|");
$zip = "delete.zip";
echo file_get_contents("phar://" . $zip . "/two.txt") . "|";
echo (unlink("phar://delete.zip/missing.txt") ? "bad" : "missing");
"#,
    );
    assert_eq!(
        out,
        "u|missing|bravo|u|tar-two|u|zip-two|missing"
    );
}

/// Verifies the `zip://archive.zip#entry` stream wrapper against `php -n` 8.5.6.
///
/// php's `ext/zip` wrapper takes a URL shape nothing else uses — a single `#` separates the
/// archive from the entry — and reads the member as a plain ZIP file with no phar semantics.
/// Measured, in this order:
///
/// ```text
/// file_get_contents("zip://a.zip#f.txt")       => string(12) "hello world\n"   (deflated)
/// file_get_contents("zip://a.zip#sub/n.txt")   => string(20) "nested content here\n"
/// strlen(file_get_contents("...#stored.txt"))  => int(200)                     (stored)
/// file_get_contents("zip://a.zip#a#b.txt")     => string(9) "hashname\n"       (splits at the FIRST #)
/// file_get_contents("zip://a.zip#nope.txt")    => Warning + bool(false)
/// file_get_contents("zip://ghost.zip#f.txt")   => Warning + bool(false)
/// file_get_contents("zip://a.zip")             => Warning + bool(false)
/// file_get_contents("zip://a.zip#/f.txt")      => Warning + bool(false)   (no leading-slash stripping)
/// file_get_contents("zip://a.zip#sub")         => Warning + bool(false)   (a directory names nothing)
/// fopen("zip://a.zip#f.txt", "w")              => Warning + bool(false)   (the wrapper is read-only)
/// ```
///
/// EVERY failure is the same line — `Failed to open stream: operation failed` — because
/// `ext/zip` stashes no wrapper error and the generic caller has only its fallback to print.
///
/// Before this test the wrapper did not exist: elephc answered
/// `Warning: fopen(): Unable to find the wrapper "zip"` followed by
/// `Failed to open stream: No such file or directory`, and `stream_get_wrappers()` listed 11
/// entries where php lists 12.
#[test]
fn test_zip_wrapper_reads_entries_and_refuses_like_php() {
    let archive = std::env::temp_dir().join(format!("elephc_zip_w1_{}.zip", std::process::id()));
    std::fs::write(
        &archive,
        build_zip_phar_container(&[
            ("f.txt", b"hello world\n", true),
            ("sub/n.txt", b"nested content here\n", true),
            ("stored.txt", &[b'x'; 200], false),
            ("a#b.txt", b"hashname\n", false),
        ]),
    )
    .unwrap();
    let src = format!(
        r#"<?php
var_dump(file_get_contents("zip://{p}#f.txt"));
var_dump(file_get_contents("zip://{p}#sub/n.txt"));
var_dump(strlen(file_get_contents("zip://{p}#stored.txt")));
var_dump(file_get_contents("zip://{p}#a#b.txt"));
var_dump(file_get_contents("zip://{p}#nope.txt"));
var_dump(file_get_contents("zip://{p}.ghost#f.txt"));
var_dump(file_get_contents("zip://{p}"));
var_dump(file_get_contents("zip://{p}#/f.txt"));
var_dump(file_get_contents("zip://{p}#sub"));
var_dump(fopen("zip://{p}#f.txt", "w"));
"#,
        p = archive.display()
    );
    let out = compile_and_run_capture(&src);
    std::fs::remove_file(&archive).ok();
    let p = archive.display();
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string(12) \"hello world\n\"\n\
         string(20) \"nested content here\n\"\n\
         int(200)\n\
         string(9) \"hashname\n\"\n\
         bool(false)\n\
         bool(false)\n\
         bool(false)\n\
         bool(false)\n\
         bool(false)\n\
         bool(false)\n"
    );
    // One wording for every failure, and the URL php names is the WHOLE url, `#` included.
    for expected in [
        format!("Warning: file_get_contents(zip://{p}#nope.txt): Failed to open stream: operation failed"),
        format!("Warning: file_get_contents(zip://{p}.ghost#f.txt): Failed to open stream: operation failed"),
        format!("Warning: file_get_contents(zip://{p}): Failed to open stream: operation failed"),
        format!("Warning: file_get_contents(zip://{p}#/f.txt): Failed to open stream: operation failed"),
        format!("Warning: file_get_contents(zip://{p}#sub): Failed to open stream: operation failed"),
        format!("Warning: fopen(zip://{p}#f.txt): Failed to open stream: operation failed"),
    ] {
        assert!(
            out.diagnostics.contains(&expected),
            "missing php's failed-open line {expected:?}, got diagnostics={}",
            out.diagnostics
        );
    }
    // php's zip wrapper EXISTS, so none of these may claim otherwise.
    assert!(
        !out.diagnostics.contains("Unable to find the wrapper"),
        "the zip wrapper is registered now, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies a `zip://` stream opened through `fopen()` reads, ends, and names itself as php does.
///
/// A RUN-TIME filename is used for the reads so the dynamic route is covered too: php reads the
/// archive when the program runs, so — unlike `phar://`, whose literal entry is extracted during
/// lowering — a literal `zip://` URL must NOT be resolved at compile time either.
///
/// Measured on `php -n` 8.5.6:
///
/// ```text
/// $h = fopen("zip://a.zip#f.txt", "r");
/// fread($h, 5)   => string(5) "hello"
/// fread($h, 100) => string(7) " world\n"
/// feof($h)       => bool(true)
/// stream_get_meta_data($h) => wrapper_type "zip wrapper", stream_type "zip", seekable false
/// fread($h,3); rewind($h) => Warning: rewind(): Stream does not support seeking + bool(false)
/// ftell($h)               => int(3)   (the refused seek moved nothing)
/// fseek($h, 0, SEEK_SET)  => Warning: fseek(): Stream does not support seeking  + int(-1)
/// ```
///
/// The metadata and the seek refusal are the same fact twice: `ext/zip`'s stream ops leave
/// `seek` NULL. elephc serves the entry from a regular temp file, which seeks perfectly well,
/// so both had to be keyed off the recorded wrapper identity — before that this read
/// `plainfile` / `STDIO` / `bool(true)` and the seeks quietly succeeded.
#[test]
fn test_zip_wrapper_stream_reads_and_refuses_to_seek() {
    let archive = std::env::temp_dir().join(format!("elephc_zip_w2_{}.zip", std::process::id()));
    std::fs::write(
        &archive,
        build_zip_phar_container(&[("f.txt", b"hello world\n", true)]),
    )
    .unwrap();
    let src = format!(
        r#"<?php
$url = "zip://{p}#f.txt";
$h = fopen($url, "r");
var_dump(fread($h, 5));
var_dump(fread($h, 100));
var_dump(feof($h));
fclose($h);
$m = stream_get_meta_data(fopen("zip://{p}#f.txt", "r"));
var_dump($m["wrapper_type"], $m["stream_type"], $m["seekable"], $m["uri"]);
$g = fopen($url, "r");
var_dump(fread($g, 3));
var_dump(rewind($g));
var_dump(ftell($g));
var_dump(fseek($g, 0, SEEK_SET));
var_dump(stream_get_contents(fopen($url, "r")));
"#,
        p = archive.display()
    );
    let out = compile_and_run_capture(&src);
    std::fs::remove_file(&archive).ok();
    let uri = format!("zip://{}#f.txt", archive.display());
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        format!(
            "string(5) \"hello\"\n\
             string(7) \" world\n\"\n\
             bool(true)\n\
             string(11) \"zip wrapper\"\n\
             string(3) \"zip\"\n\
             bool(false)\n\
             string({uri_len}) \"{uri}\"\n\
             string(3) \"hel\"\n\
             bool(false)\n\
             int(3)\n\
             int(-1)\n\
             string(12) \"hello world\n\"\n",
            uri_len = uri.len()
        )
    );
    assert!(
        out.diagnostics
            .contains("Warning: rewind(): Stream does not support seeking"),
        "expected php's rewind refusal, got diagnostics={}",
        out.diagnostics
    );
    assert!(
        out.diagnostics
            .contains("Warning: fseek(): Stream does not support seeking"),
        "expected php's fseek refusal, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies the `ZipArchive` read surface against `php -n` 8.5.6, method by method.
///
/// Measured on an archive holding `f.txt` (deflated, 12 bytes), `sub/n.txt`
/// (deflated, 20) and `stored.txt` (stored, 200):
///
/// ```text
/// $z->open("a.zip")                      => bool(true)
/// $z->numFiles, status, statusSys        => int(3), int(0), int(0)
/// $z->comment                            => string(0) ""
/// getNameIndex(0) / (2)                  => "f.txt" / "stored.txt"
/// getNameIndex(3) / (-1)                 => bool(false)   (out of range, silent)
/// locateName("f.txt") / ("stored.txt")   => int(0) / int(2)
/// locateName("nope") / ("F.TXT")         => bool(false)
/// locateName("F.TXT", FL_NOCASE)         => int(0)
/// statName("nope") / statIndex(99)       => bool(false)
/// getFromName("f.txt")                   => string(12) "hello world\n"
/// getFromName("nope")                    => bool(false)   (NO warning)
/// getStream("f.txt")                     => a readable stream
/// getStream("nope")                      => bool(false)   (NO warning)
/// $z->close()                            => bool(true), and numFiles returns to int(0)
/// ```
///
/// Every failing accessor is SILENT — only `open()` reports anything, through its
/// return value — which is why the reads go through `@`-suppressed wrapper calls
/// rather than bare ones.
#[test]
fn test_zip_archive_reads_entries_like_php() {
    let archive = std::env::temp_dir().join(format!("elephc_zip_oop_{}.zip", std::process::id()));
    std::fs::write(
        &archive,
        build_zip_phar_container(&[
            ("f.txt", b"hello world\n", true),
            ("sub/n.txt", b"nested content here\n", true),
            ("stored.txt", &[b'x'; 200], false),
        ]),
    )
    .unwrap();
    let src = format!(
        r#"<?php
$z = new ZipArchive();
var_dump($z->open("{p}"));
var_dump($z->numFiles, $z->status, $z->statusSys, $z->comment);
var_dump($z->getNameIndex(0), $z->getNameIndex(2), $z->getNameIndex(3), $z->getNameIndex(-1));
var_dump($z->locateName("f.txt"), $z->locateName("stored.txt"), $z->locateName("nope"));
var_dump($z->locateName("F.TXT"), $z->locateName("F.TXT", ZipArchive::FL_NOCASE));
var_dump($z->statName("nope"), $z->statIndex(99));
var_dump($z->getFromName("f.txt"), $z->getFromName("nope"));
var_dump($z->getFromIndex(2) === str_repeat("x", 200), $z->getFromIndex(99));
var_dump(stream_get_contents($z->getStream("f.txt")));
var_dump($z->getStream("nope"));
var_dump($z->count());
var_dump($z->close());
var_dump($z->numFiles, $z->filename);
"#,
        p = archive.display()
    );
    let out = compile_and_run_capture(&src);
    std::fs::remove_file(&archive).ok();
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(true)\n\
         int(3)\nint(0)\nint(0)\nstring(0) \"\"\n\
         string(5) \"f.txt\"\nstring(10) \"stored.txt\"\nbool(false)\nbool(false)\n\
         int(0)\nint(2)\nbool(false)\n\
         bool(false)\nint(0)\n\
         bool(false)\nbool(false)\n\
         string(12) \"hello world\n\"\nbool(false)\n\
         bool(true)\nbool(false)\n\
         string(12) \"hello world\n\"\n\
         bool(false)\n\
         int(3)\n\
         bool(true)\n\
         int(0)\nstring(0) \"\"\n"
    );
    // Not one accessor may report a failure: php's do not.
    assert_eq!(out.stderr, "", "the read surface must be silent");
}

/// Verifies `ZipArchive::statIndex()` reports php's eight keys, in php's order and values.
///
/// Measured on `php -n` 8.5.6 — the whole array for the first entry, plus the
/// stored entry's method:
///
/// ```text
/// statIndex(0) => ["name" => "f.txt", "index" => 0, "crc" => 2936552237,
///                  "size" => 12, "mtime" => <unix>, "comp_size" => 14,
///                  "comp_method" => 8, "encryption_method" => 0]
/// statIndex(1)["comp_method"] => int(0)   (stored)
/// statName("f.txt") == statIndex(0)
/// ```
///
/// `crc` reads `0` here BECAUSE the shared fixture builder writes no CRC field —
/// measured: php reports `int(0)` for exactly these bytes too, and
/// `$s["crc"] === crc32("hello world\n")` is `bool(false)` on php as well. A real
/// archive's CRC is pinned in the ZipCrypto test below, whose fixture is a genuine
/// `zip(1)` archive.
///
/// `mtime` is asserted structurally rather than as a fixed number: php derives it
/// from the entry's DOS date/time in the PROCESS timezone, so any literal here
/// would pin the machine's timezone instead of the unpacking. The unpacking itself
/// is pinned in the bridge's own unit test, and the exact value was differenced
/// against `php -n` under both the local zone and `TZ=UTC`.
#[test]
fn test_zip_archive_stat_index_reports_php_fields() {
    let archive = std::env::temp_dir().join(format!("elephc_zip_stat_{}.zip", std::process::id()));
    std::fs::write(
        &archive,
        build_zip_phar_container(&[
            ("f.txt", b"hello world\n", true),
            ("stored.txt", &[b'x'; 200], false),
        ]),
    )
    .unwrap();
    let src = format!(
        r#"<?php
$z = new ZipArchive();
$z->open("{p}");
$s = $z->statIndex(0);
var_dump(array_keys($s));
var_dump($s["name"], $s["index"], $s["size"], $s["comp_method"], $s["encryption_method"]);
var_dump($s["crc"]);
var_dump($s["comp_size"] > 0, $s["comp_size"] < 200);
var_dump($z->statIndex(1)["comp_method"], $z->statIndex(1)["comp_size"]);
var_dump($z->statName("f.txt") === $s);
var_dump(is_int($s["mtime"]), $s["mtime"] === $z->statIndex(1)["mtime"]);
$z->close();
"#,
        p = archive.display()
    );
    let out = compile_and_run_capture(&src);
    std::fs::remove_file(&archive).ok();
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "array(8) {\n  [0]=>\n  string(4) \"name\"\n  [1]=>\n  string(5) \"index\"\n  \
         [2]=>\n  string(3) \"crc\"\n  [3]=>\n  string(4) \"size\"\n  [4]=>\n  \
         string(5) \"mtime\"\n  [5]=>\n  string(9) \"comp_size\"\n  [6]=>\n  \
         string(11) \"comp_method\"\n  [7]=>\n  string(17) \"encryption_method\"\n}\n\
         string(5) \"f.txt\"\nint(0)\nint(12)\nint(8)\nint(0)\n\
         int(0)\n\
         bool(true)\nbool(true)\n\
         int(0)\nint(200)\n\
         bool(true)\n\
         bool(true)\nbool(true)\n"
    );
}

/// Verifies `ZipArchive::open()`'s flag matrix and its error codes, measured one by one.
///
/// On `php -n` 8.5.6, with `m.zip` an existing archive of three entries:
///
/// ```text
/// open("m.zip")                  => bool(true),  numFiles 3
/// open("ghost.zip")              => int(9)   ER_NOENT
/// open("ghost.zip", RDONLY)      => int(9)   ER_NOENT
/// open("n1.zip", CREATE)         => bool(true),  numFiles 0 — and NO file is created
/// open("m.zip", CREATE)          => bool(true),  numFiles 3 — opens the existing one
/// open("m.zip", CREATE|EXCL)     => int(10)  ER_EXISTS — EXCL wins over CREATE
/// open("notzip.txt")             => int(19)  ER_NOZIP
/// open("")                       => ValueError: ZipArchive::open(): Argument #1
///                                   ($filename) must not be empty
/// open("m.zip", OVERWRITE) then close() => the archive is DELETED, because libzip
///                                   removes an archive that would hold nothing
/// ```
#[test]
fn test_zip_archive_open_flag_matrix_matches_php() {
    let dir = std::env::temp_dir().join(format!("elephc_zip_flags_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let archive = dir.join("m.zip");
    let plain = dir.join("notzip.txt");
    std::fs::write(
        &archive,
        build_zip_phar_container(&[
            ("f.txt", b"hello world\n", true),
            ("sub/n.txt", b"nested content here\n", true),
            ("stored.txt", &[b'x'; 200], false),
        ]),
    )
    .unwrap();
    std::fs::write(&plain, b"not a zip\n").unwrap();
    let src = format!(
        r#"<?php
function t(string $label, string $f, int $fl): void {{
    $z = new ZipArchive();
    $r = $z->open($f, $fl);
    echo $label, ": ";
    var_dump($r);
    if ($r === true) {{ echo "  numFiles=", $z->numFiles, "\n"; var_dump($z->close()); }}
}}
t("existing", "{d}/m.zip", 0);
t("missing", "{d}/ghost.zip", 0);
t("missing RDONLY", "{d}/ghost.zip", ZipArchive::RDONLY);
t("missing CREATE", "{d}/n1.zip", ZipArchive::CREATE);
t("existing CREATE", "{d}/m.zip", ZipArchive::CREATE);
t("existing EXCL", "{d}/m.zip", ZipArchive::CREATE | ZipArchive::EXCL);
t("not a zip", "{d}/notzip.txt", 0);
var_dump(file_exists("{d}/n1.zip"));
try {{ $e = new ZipArchive(); $e->open(""); }} catch (ValueError $x) {{ echo $x->getMessage(), "\n"; }}
$o = new ZipArchive();
var_dump($o->open("{d}/m.zip", ZipArchive::OVERWRITE));
var_dump($o->numFiles);
var_dump($o->close());
var_dump(file_exists("{d}/m.zip"));
"#,
        d = dir.display()
    );
    let out = compile_and_run_capture(&src);
    std::fs::remove_dir_all(&dir).ok();
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "existing: bool(true)\n  numFiles=3\nbool(true)\n\
         missing: int(9)\n\
         missing RDONLY: int(9)\n\
         missing CREATE: bool(true)\n  numFiles=0\nbool(true)\n\
         existing CREATE: bool(true)\n  numFiles=3\nbool(true)\n\
         existing EXCL: int(10)\n\
         not a zip: int(19)\n\
         bool(false)\n\
         ZipArchive::open(): Argument #1 ($filename) must not be empty\n\
         bool(true)\n\
         int(0)\n\
         bool(true)\n\
         bool(false)\n"
    );
}

/// Verifies `ZipArchive` on a ZipCrypto archive and on directory entries.
///
/// The archive is the same real `zip --encrypt -P hunter2` fixture the PharData
/// password test uses. Measured on `php -n` 8.5.6 against an equivalent archive:
///
/// ```text
/// getFromName(...) before setPassword() => bool(false)   (and NO warning)
/// setPassword("hunter2")                => bool(true)
/// getFromName(...) after                => the plaintext
/// statIndex(0) => ["crc" => 3275747770, "size" => 25, "comp_size" => 37,
///                  "comp_method" => 0, "encryption_method" => 1]
/// ```
///
/// That CRC is the one field the synthetic fixtures cannot pin: this archive is a
/// genuine `zip(1)` one, so its central directory carries a real CRC-32 and
/// `$s["crc"] === crc32("secret zipcrypto payload\n")` holds.
///
/// A directory member is a member like any other: `zip -r` writes `dd/` and
/// `dd/sub/` entries, `numFiles` counts them, and reading one answers `""`.
#[test]
fn test_zip_archive_encrypted_and_directory_entries() {
    let dir = std::env::temp_dir().join(format!("elephc_zip_enc_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dirs = dir.join("d.zip");
    std::fs::write(
        &dirs,
        build_zip_phar_container(&[
            ("dd/", b"", false),
            ("dd/sub/", b"", false),
            ("dd/sub/x.txt", b"hi\n", false),
        ]),
    )
    .unwrap();
    let src = format!(
        r#"<?php
$bytes = base64_decode("UEsDBAoACQAAACWR1Fy68T/DJQAAABkAAAAMABwAemNfcGxhaW4udHh0VVQJAAMluzZqJbs2anV4CwABBPUBAAAEAAAAAIX9cegIcalT/zcAGsBrKLo1vP/AI2DJ71z0w4OcxvSzLXaea0tQSwcIuvE/wyUAAAAZAAAAUEsBAh4DCgAJAAAAJZHUXLrxP8MlAAAAGQAAAAwAGAAAAAAAAQAAAKSBAAAAAHpjX3BsYWluLnR4dFVUBQADJbs2anV4CwABBPUBAAAEAAAAAFBLBQYAAAAAAQABAFIAAAB7AAAAAAA=");
file_put_contents("{d}/enc.zip", $bytes);
$e = new ZipArchive();
var_dump($e->open("{d}/enc.zip"));
var_dump($e->numFiles, $e->getNameIndex(0));
$st = $e->statIndex(0);
var_dump($st["crc"] === crc32("secret zipcrypto payload\n"));
var_dump($st["size"], $st["comp_size"], $st["comp_method"], $st["encryption_method"]);
var_dump($e->getFromName("zc_plain.txt"));
var_dump($e->setPassword("hunter2"));
var_dump($e->getFromName("zc_plain.txt"));
$e->close();

$d = new ZipArchive();
var_dump($d->open("{d}/d.zip"));
var_dump($d->numFiles);
var_dump($d->getNameIndex(0), $d->getNameIndex(1), $d->getNameIndex(2));
var_dump($d->statIndex(0)["size"]);
var_dump($d->getFromName("dd/"), $d->getFromName("dd/sub/x.txt"));
var_dump($d->locateName("DD/SUB/X.TXT"), $d->locateName("DD/SUB/X.TXT", ZipArchive::FL_NOCASE));
$d->close();
"#,
        d = dir.display()
    );
    let out = compile_and_run_capture(&src);
    std::fs::remove_dir_all(&dir).ok();
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(true)\n\
         int(1)\nstring(12) \"zc_plain.txt\"\n\
         bool(true)\n\
         int(25)\nint(37)\nint(0)\nint(1)\n\
         bool(false)\n\
         bool(true)\n\
         string(25) \"secret zipcrypto payload\n\"\n\
         bool(true)\n\
         int(3)\n\
         string(3) \"dd/\"\nstring(7) \"dd/sub/\"\nstring(12) \"dd/sub/x.txt\"\n\
         int(0)\n\
         string(0) \"\"\nstring(3) \"hi\n\"\n\
         bool(false)\nint(2)\n"
    );
    // A locked entry answers `false`, it does not complain.
    assert_eq!(out.stderr, "", "the read surface must be silent");
}

/// Verifies `ZipArchive::extractTo()` against `php -n` 8.5.6, selection and all.
///
/// ```text
/// extractTo("ex1")                        => bool(true), the whole archive
/// extractTo("ex2", "f.txt")               => bool(true), that one entry
/// extractTo("ex3", ["f.txt","sub/n.txt"]) => bool(true), those two
/// extractTo("a.zip/x")                    => bool(false)  and NO warning
/// extractTo("ex4", "nope.txt")            => bool(false)
/// extractTo("ex4", [])                    => bool(false)
/// extractTo("")                           => bool(false)
/// ```
///
/// An existing file is overwritten, the extracted file carries the ENTRY's mtime
/// rather than the extraction time, and a directory member (`dd/`) becomes a
/// directory instead of an empty file.
///
/// THE UNCREATABLE DESTINATION IS A PATH THROUGH A FILE, not an unwritable root.
/// `/no/such/root/x` is only uncreatable for an unprivileged user: CI's linux shards run
/// inside a container as root, where `mkdir -p /no/such/root/x` SUCCEEDS and the case flips
/// to `true`. A component that is a regular file is `ENOTDIR` for every user, root included,
/// and `php -n` 8.5.6 answers `false` for it just the same.
#[test]
fn test_zip_archive_extract_to_matches_php() {
    let dir = std::env::temp_dir().join(format!("elephc_zip_extract_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("a.zip"),
        build_zip_phar_container(&[
            ("f.txt", b"hello world\n", true),
            ("sub/n.txt", b"nested content here\n", true),
            ("stored.txt", &[b'x'; 200], false),
        ]),
    )
    .unwrap();
    std::fs::write(
        dir.join("d.zip"),
        build_zip_phar_container(&[("dd/", b"", false), ("dd/sub/x.txt", b"hi\n", false)]),
    )
    .unwrap();
    let src = format!(
        r#"<?php
$z = new ZipArchive();
$z->open("{d}/a.zip");
var_dump($z->extractTo("{d}/ex1"));
var_dump($z->extractTo("{d}/ex2", "f.txt"));
var_dump($z->extractTo("{d}/ex3", ["f.txt", "sub/n.txt"]));
var_dump($z->extractTo("{d}/a.zip/x"));
var_dump($z->extractTo("{d}/ex4", "nope.txt"));
var_dump($z->extractTo("{d}/ex4", []));
var_dump($z->extractTo(""));
var_dump(file_get_contents("{d}/ex1/f.txt"));
var_dump(strlen(file_get_contents("{d}/ex1/stored.txt")));
var_dump(file_get_contents("{d}/ex1/sub/n.txt"));
var_dump(file_exists("{d}/ex2/sub/n.txt"), file_exists("{d}/ex3/stored.txt"));
var_dump(filemtime("{d}/ex1/f.txt") === $z->statName("f.txt")["mtime"]);
$z->close();
$dd = new ZipArchive();
$dd->open("{d}/d.zip");
var_dump($dd->extractTo("{d}/ex5"));
var_dump(is_dir("{d}/ex5/dd"), is_dir("{d}/ex5/dd/sub"), file_get_contents("{d}/ex5/dd/sub/x.txt"));
$dd->close();
"#,
        d = dir.display()
    );
    let out = compile_and_run_capture(&src);
    std::fs::remove_dir_all(&dir).ok();
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "bool(true)\nbool(true)\nbool(true)\n\
         bool(false)\nbool(false)\nbool(false)\nbool(false)\n\
         string(12) \"hello world\n\"\n\
         int(200)\n\
         string(20) \"nested content here\n\"\n\
         bool(false)\nbool(false)\n\
         bool(true)\n\
         bool(true)\n\
         bool(true)\nbool(true)\nstring(3) \"hi\n\"\n"
    );
    assert_eq!(out.stderr, "", "a failed extractTo is silent in php too");
}

/// Verifies an entry name cannot write outside the destination `extractTo()` was given.
///
/// php does not REJECT such a name, it NORMALIZES it, and the normalization is a
/// plain path walk. Every case below was measured by extracting a real archive
/// with `php -n` 8.5.6 and listing what appeared:
///
/// ```text
/// "../up.txt"          => "up.txt"       "a/../b.txt"         => "b.txt"
/// "a/b/../c.txt"       => "a/c.txt"      "a/b/../../../d.txt" => "d.txt"
/// "./dot.txt"          => "dot.txt"      "/abs.txt"           => "abs.txt"
/// "..//e.txt"          => "e.txt"        "x/./y.txt"          => "x/y.txt"
/// "f..g.txt"           => "f..g.txt"     "a/..b/h.txt"        => "a/..b/h.txt"
/// "..\\win.txt"        => "..\\win.txt"
/// ```
///
/// `a/b/../c.txt` is the case that fixes the rule: `..` pops ONE segment, it does
/// not reset the whole path. `a/b/../../../d.txt` is the case that fixes the other
/// half: popping an empty stack is a no-op, never an escape. Only a WHOLE `..`
/// segment counts, and a backslash is not a separator.
#[test]
fn test_zip_archive_extract_to_cannot_escape_the_destination() {
    let dir = std::env::temp_dir().join(format!("elephc_zip_travers_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let names: &[&str] = &[
        "../up.txt",
        "a/../b.txt",
        "a/b/../c.txt",
        "a/b/../../../d.txt",
        "./dot.txt",
        "/abs.txt",
        "..//e.txt",
        "x/./y.txt",
        "f..g.txt",
        "a/..b/h.txt",
        "..\\win.txt",
    ];
    let entries: Vec<(&str, &[u8], bool)> =
        names.iter().map(|name| (*name, &b"x\n"[..], false)).collect();
    std::fs::write(dir.join("t.zip"), build_zip_phar_container(&entries)).unwrap();
    let src = format!(
        r#"<?php
$z = new ZipArchive();
$z->open("{d}/t.zip");
var_dump($z->extractTo("{d}/out"));
$found = [];
$it = new RecursiveIteratorIterator(new RecursiveDirectoryIterator("{d}/out", FilesystemIterator::SKIP_DOTS));
foreach ($it as $file) {{ $found[] = substr($file->getPathname(), strlen("{d}/out/")); }}
sort($found);
foreach ($found as $one) {{ echo $one, "\n"; }}
$z->close();
"#,
        d = dir.display()
    );
    let out = compile_and_run_capture(&src);
    let escaped = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "t.zip" && name != "out")
        .collect::<Vec<_>>();
    std::fs::remove_dir_all(&dir).ok();
    assert!(out.success, "program failed: {}", out.stderr);
    assert!(
        escaped.is_empty(),
        "an entry name wrote outside the destination: {escaped:?}"
    );
    assert_eq!(
        out.stdout,
        "bool(true)\n\
         ..\\win.txt\n\
         a/..b/h.txt\n\
         a/c.txt\n\
         abs.txt\n\
         b.txt\n\
         d.txt\n\
         dot.txt\n\
         e.txt\n\
         f..g.txt\n\
         up.txt\n\
         x/y.txt\n"
    );
}

/// `Phar` and `PharData` expose a minimal OOP ArrayAccess surface that maps
/// bracket reads/writes/isset to the existing runtime `phar://` reader/writer.
#[test]
fn test_phar_oop_array_access_read_write() {
    let out = compile_and_run(
        r#"<?php
$p = new Phar("oop.phar");
$p["one.txt"] = "alpha";
$p["dir/two.txt"] = "bravo";
echo class_exists("phar") ? "class|" : "missing|";
echo class_exists("pharfileinfo") ? "info-class|" : "missing-info|";
echo ($p instanceof ArrayAccess) ? "aa|" : "no-aa|";
$info = $p["one.txt"];
echo ($info instanceof SplFileInfo) ? "spl-info|" : "bad-info|";
echo get_class($info) . "|";
echo $info->getContent() . "|";
echo $info->getFilename() . "|";
echo $info->getPathname() . "|";
echo $p["dir/two.txt"]->getContent() . "|";
echo ($p["missing.txt"]->getContent() === false ? "missing|" : "bad|");
echo (isset($p["one.txt"]) ? "yes|" : "no|");
echo (isset($p["missing.txt"]) ? "bad|" : "no|");
$pd = new PharData("oop.tar");
$pd["note.txt"] = "tar";
echo $pd["note.txt"]->getContent() . "|";
echo Phar::GZ . "|" . PharData::TAR;
"#,
    );
    assert_eq!(
        out,
        "class|info-class|aa|spl-info|PharFileInfo|alpha|one.txt|phar://oop.phar/one.txt|bravo|missing|yes|no|tar|4096|2"
    );
}

/// `Phar::addFromString()` and `PharData::addFromString()` use the same runtime
/// writer as ArrayAccess assignment for native PHAR and tar containers.
#[test]
fn test_phar_oop_add_from_string_writes_entries() {
    let out = compile_and_run(
        r#"<?php
$p = new Phar("add.phar");
$p->addFromString("one.txt", "alpha");
$p->addFromString("dir/two.txt", "bravo");
echo $p["one.txt"]->getContent() . "|";
echo $p["dir/two.txt"]->getContent() . "|";
$pd = new PharData("add.tar");
$pd->addFromString("note.txt", "tar");
echo $pd["note.txt"]->getContent();
"#,
    );
    assert_eq!(out, "alpha|bravo|tar");
}

/// `Phar` and `PharData` expose object-level metadata, stub, and path helpers.
#[test]
fn test_phar_oop_metadata_stub_and_path_helpers() {
    let out = compile_and_run(
        r#"<?php
$p = new Phar("meta.phar");
echo ($p->hasMetadata() ? "bad|" : "no-meta|");
echo ($p->getMetadata() === null ? "null|" : "bad|");
$p->setMetadata("app:3");
echo ($p->hasMetadata() ? "has-meta|" : "bad|");
echo $p->getMetadata() . "|";
$p->setMetadata(["kind" => "app", "version" => 3]);
$meta = $p->getMetadata();
echo $meta["kind"] . ":" . $meta["version"] . "|";
$p->setMetadata(42);
echo $p->getMetadata() . "|";
$p->setMetadata(null);
echo ($p->hasMetadata() ? "has-null|" : "bad|");
echo ($p->getMetadata() === null ? "null-meta|" : "bad|");
echo ($p->delMetadata() ? "cleared|" : "bad|");
echo ($p->hasMetadata() ? "bad|" : "no-meta|");
$p->setStub("<?php echo 'stub'; __HALT_COMPILER(); ?>");
echo $p->getStub() . "|";
echo $p->getPath() . "|" . $p->getPathname() . "|" . $p->getFilename() . "|";
$pd = new PharData("meta.tar");
$pd->setMetadata("tar-meta");
echo $pd->getMetadata() . "|" . $pd->__toString();
"#,
    );
    assert_eq!(
        out,
        "no-meta|null|has-meta|app:3|app:3|42|has-null|null-meta|cleared|no-meta|<?php echo 'stub'; __HALT_COMPILER(); ?>|meta.phar|meta.phar|meta.phar|tar-meta|meta.tar"
    );
}

/// Global metadata and the stub persist into the archive and are read back by a fresh
/// `Phar`/`PharData` object across all three families (native, tar, zip).
#[test]
fn test_phar_oop_metadata_stub_persist_across_objects() {
    let out = compile_and_run(
        r#"<?php
$p = new Phar("persist.phar");
$p->addFromString("a.txt", "alpha");
$p->setMetadata(["v" => "1.0", "n" => 5]);
$q = new Phar("persist.phar");
$m = $q->getMetadata();
echo $m["v"], ":", $m["n"], ":", ($q->hasMetadata() ? "y" : "n"), "|";
$t = new PharData("persist.tar");
$t->addFromString("b.txt", "bravo");
$t->setMetadata("tar-meta");
$t->setStub("<?php __HALT_COMPILER(); ?>");
$t2 = new PharData("persist.tar");
echo $t2->getMetadata(), ":", $t2->getStub(), "|";
echo $t2->count(), "|";
$z = new PharData("persist.zip");
$z->addFromString("c.txt", "charlie");
$z->setMetadata(["zip" => 1]);
$z2 = new PharData("persist.zip");
$zm = $z2->getMetadata();
echo $zm["zip"];
"#,
    );
    assert_eq!(
        out,
        "1.0:5:y|tar-meta:<?php __HALT_COMPILER(); ?>|1|1"
    );
}

/// `PharFileInfo::setMetadata()`/`getMetadata()`/`hasMetadata()`/`delMetadata()`
/// persist per-file metadata into the archive and round-trip across fresh objects,
/// for native PHAR, tar, and zip, including a nested entry path and scalar metadata.
#[test]
fn test_phar_oop_per_file_metadata_persist() {
    let out = compile_and_run(
        r#"<?php
$p = new Phar("perfile.phar");
$p->addFromString("a.txt", "alpha");
$p->addFromString("dir/b.txt", "bravo");
$p["a.txt"]->setMetadata(["role" => "first", "n" => 3]);
$p["dir/b.txt"]->setMetadata("nested");
$q = new Phar("perfile.phar");
$am = $q["a.txt"]->getMetadata();
echo $am["role"], ":", $am["n"], "|";
echo $q["dir/b.txt"]->getMetadata(), "|";
echo ($q["a.txt"]->hasMetadata() ? "y" : "n"), ($q["dir/b.txt"]->hasMetadata() ? "y" : "n"), "|";
$t = new PharData("perfile.tar");
$t->addFromString("c.txt", "charlie");
$t->addFromString("d.txt", "delta");
$t["c.txt"]->setMetadata(["t" => 9]);
$t2 = new PharData("perfile.tar");
$tm = $t2["c.txt"]->getMetadata();
echo $tm["t"], ":", ($t2["c.txt"]->hasMetadata() ? "y" : "n"), ($t2["d.txt"]->hasMetadata() ? "y" : "n"), "|";
$z = new PharData("perfile.zip");
$z->addFromString("e.txt", "echo");
$z["e.txt"]->setMetadata(["z" => "v"]);
$z["e.txt"]->delMetadata();
$z2 = new PharData("perfile.zip");
echo ($z2["e.txt"]->hasMetadata() ? "y" : "n");
unlink("perfile.phar");
unlink("perfile.tar");
unlink("perfile.zip");
"#,
    );
    assert_eq!(out, "first:3|nested|yy|9:yn|n");
}

/// Verifies persisted archive metadata cannot instantiate application classes
/// or run wakeup hooks while a fresh Phar object loads an untrusted archive.
#[test]
fn test_phar_persisted_metadata_blocks_class_hydration() {
    let out = compile_and_run(
        r#"<?php
class ArchiveMetadata {
    public static int $wakeups = 0;
    public function __wakeup(): void {
        self::$wakeups = self::$wakeups + 1;
    }
}
$p = new Phar("metadata-policy.phar");
$p->addFromString("entry.txt", "payload");
$p->setMetadata(new ArchiveMetadata());
$p["entry.txt"]->setMetadata(new ArchiveMetadata());

$q = new Phar("metadata-policy.phar");
echo get_class($q->getMetadata()), "|";
echo get_class($q["entry.txt"]->getMetadata()), "|";
echo ArchiveMetadata::$wakeups;
unlink("metadata-policy.phar");
"#,
    );
    assert_eq!(
        out,
        "__PHP_Incomplete_Class|__PHP_Incomplete_Class|0",
        "archive metadata must deserialize with allowed_classes=false"
    );
}

/// `PharData::compress()` produces a whole-archive `.tar.gz`/`.tar.bz2` that is read
/// back transparently, and `decompress()` reverses it — entries (including a nested
/// path) survive each step.
#[test]
fn test_phar_oop_tar_whole_archive_compression() {
    let out = compile_and_run(
        r#"<?php
$p = new PharData("wac.tar");
$p->addFromString("a.txt", "alpha");
$p->addFromString("dir/b.txt", "bravo");
$gz = $p->compress(Phar::GZ);
echo $gz->count(), ":", $gz["a.txt"]->getContent(), ":", $gz["dir/b.txt"]->getContent(), "|";
$bz = $p->compress(Phar::BZ2);
echo $bz->count(), ":", $bz["a.txt"]->getContent(), "|";
$back = $gz->decompress();
echo $back->count(), ":", $back["dir/b.txt"]->getContent();
unlink("wac.tar");
unlink("wac.tar.gz");
unlink("wac.tar.bz2");
"#,
    );
    assert_eq!(out, "2:alpha:bravo|2:alpha|2:bravo");
}

/// `Phar::setSignatureAlgorithm(Phar::OPENSSL, $key)` signs the archive with RSA-SHA1
/// and `getSignature()` reads it back as an OpenSSL signature; a hash algorithm
/// (`Phar::SHA256`) rewrites the trailer and reads back as SHA-256.
#[test]
fn test_phar_oop_signature_algorithms() {
    let out = compile_and_run(
        r#"<?php
$key = <<<'KEYEOF'
-----BEGIN PRIVATE KEY-----
MIICdgIBADANBgkqhkiG9w0BAQEFAASCAmAwggJcAgEAAoGBAOuAP7xZaVfhwn9l
BaMgxKPU1ODBpuT7Ybu6Fav03TJp1BKc1wUMiXnUPraUUI2R2JxoattDe7R/LcGk
jVoPiBGGPoxxTaByd5LJZJk6MJAiGBhzQT7bkK3OMDHLQqhziefqDFfnDLt/TN7+
umuMCPtLmuF6UUXiebMzyH21x7jvAgMBAAECgYBBhL+2rgVxzrxm5vsnhEFQ9zB2
i0ncYNey+7V1zr0PfoPi3cGwhOlmfJcqAp9ak534/c/kyqSK9esL+bTdvn5zIQqC
Swt2znffaW9nC6lM/pkZcvGLETt2m0L71n6pZVkMewsGBm9YrBQFA1krC7BV674U
mlOmmYpM3LPgzmRLwQJBAPm/G7O4Stmzu5xV5qtvYX1dNZ2gydkVyfK/AwCYpfbK
8ZXntKeWCt1BER1hNBSMPacHKb0LotK3j3LNNteLHCECQQDxZdNsXNLTHylWKA/X
dyM3SH9mM6ESZP07cU7Ifq6t9zJdTfGdiyxsAjaaXxDmShL+bAjU16iwaTAGcYTB
NrMPAkEAoUGwVV7Nlbvji5I7mr4UKKoikGDdc/oJp1+GRMBLiQqI6s3ta7gJ08rL
jjjRM+NJe6u4W4RD4eL8EJhIrOv5gQJAK4Tm+8c0PtmEU0L/sCGLWMEaLquqIy3P
tXK0+FJWXYiOLOILaBKaHJK9k1EGM+4wxGtnoC+M+tjLzq2SeF7LIwJAPdLUn2Qq
eGMK12chOVcx41RxYctqsOlEKCIt011yGsV2/Mdm9ljTXeyXvNXCVOVcnHaf1v5w
rNiobfy8sSb6iw==
-----END PRIVATE KEY-----
KEYEOF;
$p = new Phar("signed.phar");
$p->addFromString("a.txt", "alpha");
$p->setSignatureAlgorithm(Phar::OPENSSL, $key);
$archive = "signed.phar";
echo (file_get_contents("phar://" . $archive . "/a.txt") === false ? "closed:" : "open:");
echo ($p->getSignature() === false ? "closed|" : "metadata|");
$publicKey = <<<'KEYEOF'
-----BEGIN PUBLIC KEY-----
MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDrgD+8WWlX4cJ/ZQWjIMSj1NTg
wabk+2G7uhWr9N0yadQSnNcFDIl51D62lFCNkdicaGrbQ3u0fy3BpI1aD4gRhj6M
cU2gcneSyWSZOjCQIhgYc0E+25CtzjAxy0Koc4nn6gxX5wy7f0ze/rprjAj7S5rh
elFF4nmzM8h9tce47wIDAQAB
-----END PUBLIC KEY-----
KEYEOF;
file_put_contents("signed.phar.pubkey", $publicKey);
$s = $p->getSignature();
echo $s["hash_type"], ":", strlen($s["hash"]), ":";
echo file_get_contents("phar://" . $archive . "/a.txt"), "|";
$p->setSignatureAlgorithm(Phar::SHA256);
$s2 = $p->getSignature();
echo $s2["hash_type"], ":", strlen($s2["hash"]);
unlink("signed.phar");
unlink("signed.phar.pubkey");
"#,
    );
    // 1024-bit RSA signature = 128 bytes = 256 uppercase-hex chars; SHA-256 = 32 bytes = 64 hex.
    assert_eq!(out, "closed:closed|OpenSSL:256:alpha|SHA-256:64");
}

/// Tar and zip phars carry their signature in a `.phar/signature.bin` entry rather
/// than a trailer. `PharData::setSignatureAlgorithm()` signs both families (hash and
/// OpenSSL), `getSignature()` reads them back, and the signed archive still reads.
#[test]
fn test_phar_oop_tar_zip_signatures() {
    let out = compile_and_run(
        r#"<?php
$key = <<<'KEYEOF'
-----BEGIN PRIVATE KEY-----
MIICdgIBADANBgkqhkiG9w0BAQEFAASCAmAwggJcAgEAAoGBAOuAP7xZaVfhwn9l
BaMgxKPU1ODBpuT7Ybu6Fav03TJp1BKc1wUMiXnUPraUUI2R2JxoattDe7R/LcGk
jVoPiBGGPoxxTaByd5LJZJk6MJAiGBhzQT7bkK3OMDHLQqhziefqDFfnDLt/TN7+
umuMCPtLmuF6UUXiebMzyH21x7jvAgMBAAECgYBBhL+2rgVxzrxm5vsnhEFQ9zB2
i0ncYNey+7V1zr0PfoPi3cGwhOlmfJcqAp9ak534/c/kyqSK9esL+bTdvn5zIQqC
Swt2znffaW9nC6lM/pkZcvGLETt2m0L71n6pZVkMewsGBm9YrBQFA1krC7BV674U
mlOmmYpM3LPgzmRLwQJBAPm/G7O4Stmzu5xV5qtvYX1dNZ2gydkVyfK/AwCYpfbK
8ZXntKeWCt1BER1hNBSMPacHKb0LotK3j3LNNteLHCECQQDxZdNsXNLTHylWKA/X
dyM3SH9mM6ESZP07cU7Ifq6t9zJdTfGdiyxsAjaaXxDmShL+bAjU16iwaTAGcYTB
NrMPAkEAoUGwVV7Nlbvji5I7mr4UKKoikGDdc/oJp1+GRMBLiQqI6s3ta7gJ08rL
jjjRM+NJe6u4W4RD4eL8EJhIrOv5gQJAK4Tm+8c0PtmEU0L/sCGLWMEaLquqIy3P
tXK0+FJWXYiOLOILaBKaHJK9k1EGM+4wxGtnoC+M+tjLzq2SeF7LIwJAPdLUn2Qq
eGMK12chOVcx41RxYctqsOlEKCIt011yGsV2/Mdm9ljTXeyXvNXCVOVcnHaf1v5w
rNiobfy8sSb6iw==
-----END PRIVATE KEY-----
KEYEOF;
$tar = new PharData("sig.tar");
$tar->addFromString("doc.txt", "tarbody");
$tar->setSignatureAlgorithm(Phar::SHA256);
$ts = $tar->getSignature();
echo $ts["hash_type"], ":", strlen($ts["hash"]), "|";
$zip = new PharData("sig.zip");
$zip->addFromString("doc.txt", "zipbody");
$zip->setSignatureAlgorithm(Phar::OPENSSL, $key);
$publicKey = <<<'KEYEOF'
-----BEGIN PUBLIC KEY-----
MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDrgD+8WWlX4cJ/ZQWjIMSj1NTg
wabk+2G7uhWr9N0yadQSnNcFDIl51D62lFCNkdicaGrbQ3u0fy3BpI1aD4gRhj6M
cU2gcneSyWSZOjCQIhgYc0E+25CtzjAxy0Koc4nn6gxX5wy7f0ze/rprjAj7S5rh
elFF4nmzM8h9tce47wIDAQAB
-----END PUBLIC KEY-----
KEYEOF;
file_put_contents("sig.zip.pubkey", $publicKey);
$zs = $zip->getSignature();
echo $zs["hash_type"], ":", strlen($zs["hash"]), "|";
echo $tar["doc.txt"]->getContent(), ":", $zip["doc.txt"]->getContent();
unlink("sig.tar");
unlink("sig.zip");
unlink("sig.zip.pubkey");
"#,
    );
    // SHA-256 digest = 32 bytes = 64 hex; OpenSSL 1024-bit RSA = 128 bytes = 256 hex.
    assert_eq!(out, "SHA-256:64|OpenSSL:256|tarbody:zipbody");
}

/// `PharData::setZipPassword()` decrypts traditional-PKWARE (ZipCrypto) encrypted
/// ZIP entries (a compiler extension). The embedded fixture was produced by the
/// `zip --encrypt` CLI; the correct password reads the payload, a wrong one yields
/// nothing.
#[test]
fn test_phar_oop_zipcrypto_password() {
    // A real `zip --encrypt -P hunter2` archive of "secret zipcrypto payload\n".
    let out = compile_and_run(
        r#"<?php
$zip = base64_decode("UEsDBAoACQAAACWR1Fy68T/DJQAAABkAAAAMABwAemNfcGxhaW4udHh0VVQJAAMluzZqJbs2anV4CwABBPUBAAAEAAAAAIX9cegIcalT/zcAGsBrKLo1vP/AI2DJ71z0w4OcxvSzLXaea0tQSwcIuvE/wyUAAAAZAAAAUEsBAh4DCgAJAAAAJZHUXLrxP8MlAAAAGQAAAAwAGAAAAAAAAQAAAKSBAAAAAHpjX3BsYWluLnR4dFVUBQADJbs2anV4CwABBPUBAAAEAAAAAFBLBQYAAAAAAQABAFIAAAB7AAAAAAA=");
file_put_contents("enc.zip", $zip);
$p = new PharData("enc.zip");
$p->setZipPassword("hunter2");
echo $p["zc_plain.txt"]->getContent();
$wrong = new PharData("enc.zip");
$wrong->setZipPassword("nope");
echo "|len=", strlen($wrong["zc_plain.txt"]->getContent());
unlink("enc.zip");
"#,
    );
    assert_eq!(out, "secret zipcrypto payload\n|len=0");
}

/// `PharData::setZipPassword()` also encrypts on write (a compiler extension): with a
/// password set before `addFromString`, the entry is ZipCrypto-encrypted on disk and
/// round-trips back through a fresh object with the correct password, while a fresh
/// object with a wrong password cannot decrypt it.
#[test]
fn test_phar_oop_zipcrypto_write_roundtrip() {
    let out = compile_and_run(
        r#"<?php
$p = new PharData("encw.zip");
$p->setZipPassword("hunter2");
$p->addFromString("a.txt", "secret payload");
// A fresh object with the correct password decrypts the written entry.
$ok = new PharData("encw.zip");
$ok->setZipPassword("hunter2");
echo $ok["a.txt"]->getContent();
// A fresh object with a wrong password cannot decrypt it.
$bad = new PharData("encw.zip");
$bad->setZipPassword("nope");
echo "|len=", strlen($bad["a.txt"]->getContent());
unlink("encw.zip");
"#,
    );
    assert_eq!(out, "secret payload|len=0");
}

/// `Phar` and `PharData` iterate over entries written through the OOP surface.
#[test]
fn test_phar_oop_iteration_tracks_written_entries() {
    let out = compile_and_run(
        r#"<?php
$p = new Phar("iter.phar");
$p->addFromString("one.txt", "alpha");
$p["two.txt"] = "bravo";
$p->addFromString("one.txt", "alpha2");
echo ($p instanceof Iterator) ? "iter|" : "no-iter|";
echo ($p instanceof Countable) ? "countable|" : "no-count|";
echo $p->count() . "|";
foreach ($p as $name => $info) {
    echo $name . "=" . $info->getContent() . "|";
}
$p->rewind();
echo get_class($p->current()) . "|";
unset($p["two.txt"]);
echo $p->count() . "|";
foreach ($p as $name => $info) {
    echo $name . "=" . $info->getContent() . "|";
}
$pd = new PharData("iter.tar");
$pd->addFromString("tar.txt", "tar");
foreach ($pd as $name => $info) {
    echo $name . "=" . $info->getContent();
}
unlink("iter.phar");
unlink("iter.tar");
"#,
    );
    assert_eq!(
        out,
        "iter|countable|2|one.txt=alpha2|two.txt=bravo|PharFileInfo|1|one.txt=alpha2|tar.txt=tar"
    );
}

/// `Phar` and `PharData` seed iteration from archives that already exist.
#[test]
fn test_phar_oop_iteration_scans_existing_archives() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("phar://scan.phar/one.txt", "alpha");
file_put_contents("phar://scan.phar/two.txt", "bravo");
$p = new Phar("scan.phar");
echo $p->count() . "|";
foreach ($p as $name => $info) {
    echo $name . "=" . $info->getContent() . "|";
}
file_put_contents("phar://scan.tar/tar.txt", "tar");
$tar = new PharData("scan.tar");
echo $tar->count() . "|";
foreach ($tar as $name => $info) {
    echo $name . "=" . $info->getContent() . "|";
}
file_put_contents("phar://scan.zip/zip.txt", "zip");
$zip = new PharData("scan.zip");
echo $zip->count() . "|";
foreach ($zip as $name => $info) {
    echo $name . "=" . $info->getContent();
}
unlink("scan.phar");
unlink("scan.tar");
unlink("scan.zip");
"#,
    );
    assert_eq!(
        out,
        "2|one.txt=alpha|two.txt=bravo|1|tar.txt=tar|1|zip.txt=zip"
    );
}

/// `Phar::compressFiles()` and `decompressFiles()` rewrite native PHAR entry
/// compression while preserving readable payloads.
#[test]
fn test_phar_oop_compress_and_decompress_files() {
    let out = compile_and_run(
        r#"<?php
$p = new Phar("compress.phar");
$p->addFromString("one.txt", "alpha alpha alpha");
$p->addFromString("two.txt", "bravo bravo bravo");
$p->compressFiles(Phar::GZ);
echo $p["one.txt"]->getContent() . "|";
echo ($p->decompressFiles() ? "plain|" : "bad|");
echo $p["two.txt"]->getContent() . "|";
$zip = new PharData("compress.zip");
$zip->addFromString("zip.txt", "zip zip zip");
$zip->compressFiles(Phar::GZ);
echo $zip["zip.txt"]->getContent() . "|";
echo ($zip->decompressFiles() ? "zip-plain|" : "zip-bad|");
echo $zip["zip.txt"]->getContent() . "|";
echo (function_exists("__elephc_phar_set_compression") ? "visible" : "hidden");
"#,
    );
    assert_eq!(
        out,
        "alpha alpha alpha|plain|bravo bravo bravo|zip zip zip|zip-plain|zip zip zip|hidden"
    );
}

/// `Phar::delete()` and `PharData::delete()` remove archive entries through the
/// same PHAR-aware unlink path as ArrayAccess unset.
#[test]
fn test_phar_oop_delete_method_removes_entries() {
    let out = compile_and_run(
        r#"<?php
$p = new Phar("delete-method.phar");
$p->addFromString("one.txt", "alpha");
$p->addFromString("two.txt", "bravo");
echo ($p->delete("one.txt") ? "deleted|" : "bad|");
echo (isset($p["one.txt"]) ? "bad|" : "missing|");
echo $p["two.txt"]->getContent() . "|";
$pd = new PharData("delete-method.tar");
$pd->addFromString("one.txt", "tar-one");
$pd->addFromString("two.txt", "tar-two");
echo ($pd->delete("one.txt") ? "deleted|" : "bad|");
echo $pd["two.txt"]->getContent();
"#,
    );
    assert_eq!(out, "deleted|missing|bravo|deleted|tar-two");
}

/// ArrayAccess `unset()` on `Phar` and `PharData` deletes the archive entry and
/// leaves other entries readable.
#[test]
fn test_phar_oop_array_access_unset_deletes_entry() {
    let out = compile_and_run(
        r#"<?php
$p = new Phar("unset.phar");
$p["one.txt"] = "alpha";
$p["two.txt"] = "bravo";
unset($p["one.txt"]);
echo (isset($p["one.txt"]) ? "bad|" : "missing|");
echo $p["two.txt"]->getContent() . "|";
$pd = new PharData("unset.tar");
$pd["one.txt"] = "tar-one";
$pd["two.txt"] = "tar-two";
unset($pd["one.txt"]);
echo (isset($pd["one.txt"]) ? "bad|" : "missing|");
echo $pd["two.txt"]->getContent();
"#,
    );
    assert_eq!(out, "missing|bravo|missing|tar-two");
}

/// `file_get_contents()` of a literal `phar://` URL decodes the entry at compile
/// time (like the fopen read fast path) and returns its bytes as a string; a
/// missing entry returns `false`.
#[test]
fn test_file_get_contents_phar_literal_entry() {
    let phar = build_minimal_phar(&[
        ("hello.txt", b"Hello from phar!\n"),
        ("dir/inner.txt", b"inner content here"),
    ]);
    let path = std::env::temp_dir().join(format!("elephc_phar_fgc_{}.phar", std::process::id()));
    std::fs::write(&path, &phar).unwrap();
    let src = format!(
        r#"<?php
echo file_get_contents("phar://{p}/dir/inner.txt");
echo "|" . (file_get_contents("phar://{p}/nope.txt") === false ? "false" : "open");
"#,
        p = path.display()
    );
    let out = compile_and_run(&src);
    std::fs::remove_file(&path).ok();
    assert_eq!(out, "inner content here|false");
}

/// `file_get_contents()` of a NON-literal `phar://` URL reads the entry at run
/// time (via the `__rt_file_get_contents_maybe_phar` gate → runtime reader →
/// `stream_get_contents`): write a phar literally, then read it back through a
/// runtime path; a missing entry returns `false`.
#[test]
fn test_file_get_contents_phar_runtime_path() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("phar://fg.phar/data.txt", "w");
fwrite($f, "runtime fgc");
fclose($f);
$p = "fg.phar";
echo file_get_contents("phar://" . $p . "/data.txt");
echo "|" . (file_get_contents("phar://" . $p . "/missing.txt") === false ? "false" : "open");
"#,
    );
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(out, "runtime fgc|false");
}

/// Verifies compiled PHP output for fopen phar missing archive returns false.
#[test]
fn test_fopen_phar_missing_archive_returns_false() {
    // A phar:// URL whose archive file does not exist lowers to PHP false,
    // matching a failed fopen().
    let out = compile_and_run(
        r#"<?php $f = @fopen("phar:///nonexistent/elephc-missing.phar/x.txt", "r"); echo $f === false ? "false" : "open";"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for fopen phar reads gzip entry.
#[test]
fn test_fopen_phar_reads_gzip_entry() {
    // PHP stores gzip-compressed phar entries as raw DEFLATE; the compiler
    // inflates them at compile time. The fixture is compressed with the same
    // flate2 encoder the compiler decodes, so the round-trip is version-stable.
    let content = b"gzip-compressed phar entry payload, repeated for ratio. ".repeat(8);
    let mut encoder =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut encoder, &content).unwrap();
    let stored = encoder.finish().unwrap();
    assert!(stored.len() < content.len(), "fixture should actually compress");
    let phar = build_phar(&[TestPharEntry {
        name: "z.txt",
        uncompressed_size: content.len() as u32,
        stored: &stored,
        flags: 0x0000_11a4, // gzip (0x1000) | 0644
    }]);
    let path = std::env::temp_dir().join(format!("elephc_phar_m2_gz_{}.phar", std::process::id()));
    std::fs::write(&path, &phar).unwrap();
    let src = format!(
        r#"<?php $f = fopen("phar://{p}/z.txt", "r"); $s = fread($f, 8192); fclose($f); echo strlen($s) . "|" . substr($s, 0, 4);"#,
        p = path.display()
    );
    let out = compile_and_run(&src);
    std::fs::remove_file(&path).ok();
    assert_eq!(out, format!("{}|gzip", content.len()));
}

/// Verifies compiled PHP output for dynamic fopen phar reads gzip entry.
#[test]
fn test_fopen_phar_runtime_path_reads_gzip_entry() {
    // The runtime phar reader must inflate gzip entries when the archive path
    // arrives through string concatenation instead of the compile-time literal
    // fast path.
    let content = b"gzip-compressed phar entry payload, repeated for ratio. ".repeat(8);
    let mut encoder =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut encoder, &content).unwrap();
    let stored = encoder.finish().unwrap();
    assert!(stored.len() < content.len(), "fixture should actually compress");
    let phar = build_phar(&[TestPharEntry {
        name: "z.txt",
        uncompressed_size: content.len() as u32,
        stored: &stored,
        flags: 0x0000_11a4, // gzip (0x1000) | 0644
    }]);
    let path = std::env::temp_dir().join(format!("elephc_phar_rt_gz_{}.phar", std::process::id()));
    std::fs::write(&path, &phar).unwrap();
    let src = format!(
        r#"<?php $p = "{p}"; $f = fopen("phar://" . $p . "/z.txt", "r"); $s = fread($f, 8192); fclose($f); echo strlen($s) . "|" . substr($s, 0, 4);"#,
        p = path.display()
    );
    let out = compile_and_run(&src);
    std::fs::remove_file(&path).ok();
    assert_eq!(out, format!("{}|gzip", content.len()));
}

/// Verifies compiled PHP output for fopen phar reads bzip2 entry.
#[test]
fn test_fopen_phar_reads_bzip2_entry() {
    // PHP stores bzip2 phar entries as a standard bzip2 stream ("BZh..."); the
    // compiler decompresses them at compile time via the pure-Rust bzip2-rs. A
    // pure-Rust decoder can't compress, so the fixture is a precomputed bzip2
    // blob of a known 232-byte string (`"bzip2-compressed phar entry. "` x8).
    const BZIP2_BLOB: &[u8] = &[
        0x42, 0x5a, 0x68, 0x39, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59, 0x61, 0x39,
        0xa6, 0xe8, 0x00, 0x00, 0x1f, 0x99, 0x80, 0x40, 0x03, 0x10, 0x00, 0x3e,
        0x63, 0xdc, 0x30, 0x20, 0x00, 0x70, 0x53, 0x09, 0xa6, 0x80, 0xd3, 0x10,
        0x2a, 0xa8, 0x0c, 0x43, 0x46, 0x1a, 0x9b, 0x0b, 0x0a, 0x0e, 0x46, 0x45,
        0xc5, 0x44, 0xc5, 0x05, 0x46, 0x06, 0xe3, 0xa1, 0x21, 0x03, 0x22, 0x42,
        0xc2, 0xe2, 0x63, 0x02, 0xe2, 0x82, 0x07, 0x82, 0x82, 0x05, 0x44, 0x0f,
        0xc5, 0xdc, 0x91, 0x4e, 0x14, 0x24, 0x18, 0x4e, 0x69, 0xba, 0x00,
    ];
    let phar = build_phar(&[TestPharEntry {
        name: "b.txt",
        uncompressed_size: 232,
        stored: BZIP2_BLOB,
        flags: 0x0000_21a4, // bzip2 (0x2000) | 0644
    }]);
    let path = std::env::temp_dir().join(format!("elephc_phar_m2_bz_{}.phar", std::process::id()));
    std::fs::write(&path, &phar).unwrap();
    let src = format!(
        r#"<?php $f = fopen("phar://{p}/b.txt", "r"); $s = fread($f, 4096); fclose($f); echo strlen($s) . "|" . substr($s, 0, 26);"#,
        p = path.display()
    );
    let out = compile_and_run(&src);
    std::fs::remove_file(&path).ok();
    assert_eq!(out, "232|bzip2-compressed phar entr");
}

/// Verifies compiled PHP output for dynamic file_get_contents phar reads bzip2 entry.
#[test]
fn test_file_get_contents_phar_runtime_path_reads_bzip2_entry() {
    // Dynamic file_get_contents() routes through the runtime phar reader, so it
    // must publish libbz2 and decompress bzip2-compressed entry payloads there.
    let phar = build_phar(&[TestPharEntry {
        name: "b.txt",
        uncompressed_size: 232,
        stored: BZIP2_PHAR_BLOB,
        flags: 0x0000_21a4, // bzip2 (0x2000) | 0644
    }]);
    let path = std::env::temp_dir().join(format!("elephc_phar_rt_bz_{}.phar", std::process::id()));
    std::fs::write(&path, &phar).unwrap();
    let src = format!(
        r#"<?php $p = "{p}"; $s = file_get_contents("phar://" . $p . "/b.txt"); echo strlen($s) . "|" . substr($s, 0, 26);"#,
        p = path.display()
    );
    let out = compile_and_run(&src);
    std::fs::remove_file(&path).ok();
    assert_eq!(out, "232|bzip2-compressed phar entr");
}

/// Verifies a literal `fopen("phar://...")` URL can read a tar-based PHAR container.
#[test]
fn test_fopen_phar_literal_tar_entry() {
    let archive = build_tar_phar_container(&[
        ("plain.txt", b"plain"),
        ("dir/tar.txt", b"tar payload"),
    ]);
    let path = std::env::temp_dir().join(format!("elephc_phar_tar_lit_{}.tar", std::process::id()));
    std::fs::write(&path, &archive).unwrap();
    let src = format!(
        r#"<?php $f = fopen("phar://{p}/dir/tar.txt", "r"); echo fread($f, 64); fclose($f);"#,
        p = path.display()
    );
    let out = compile_and_run(&src);
    std::fs::remove_file(&path).ok();
    assert_eq!(out, "tar payload");
}

/// Verifies a literal `file_get_contents("phar://...")` URL can read a deflated ZIP PHAR entry.
#[test]
fn test_file_get_contents_phar_literal_zip_deflate_entry() {
    let archive = build_zip_phar_container(&[
        ("plain.txt", b"stored", false),
        ("deflated.txt", b"deflated zip payload", true),
    ]);
    let path = std::env::temp_dir().join(format!("elephc_phar_zip_lit_{}.zip", std::process::id()));
    std::fs::write(&path, &archive).unwrap();
    let src = format!(
        r#"<?php echo file_get_contents("phar://{p}/deflated.txt");"#,
        p = path.display()
    );
    let out = compile_and_run(&src);
    std::fs::remove_file(&path).ok();
    assert_eq!(out, "deflated zip payload");
}

/// Verifies a dynamic `file_get_contents()` PHAR URL uses the runtime bridge for tar containers.
#[test]
fn test_file_get_contents_phar_runtime_tar_entry() {
    let archive = build_tar_phar_container(&[
        ("plain.txt", b"plain"),
        ("dir/runtime.txt", b"runtime tar payload"),
    ]);
    let path = std::env::temp_dir().join(format!("elephc_phar_tar_rt_{}.tar", std::process::id()));
    std::fs::write(&path, &archive).unwrap();
    let src = format!(
        r#"<?php $p = "{p}"; echo file_get_contents("phar://" . $p . "/dir/runtime.txt");"#,
        p = path.display()
    );
    let out = compile_and_run(&src);
    std::fs::remove_file(&path).ok();
    assert_eq!(out, "runtime tar payload");
}

/// Verifies a dynamic `fopen()` PHAR URL uses the runtime bridge for deflated ZIP entries.
#[test]
fn test_fopen_phar_runtime_zip_deflate_entry() {
    let archive = build_zip_phar_container(&[
        ("plain.txt", b"stored", false),
        ("dir/deflated.txt", b"runtime zip payload", true),
    ]);
    let path = std::env::temp_dir().join(format!("elephc_phar_zip_rt_{}.zip", std::process::id()));
    std::fs::write(&path, &archive).unwrap();
    let src = format!(
        r#"<?php $p = "{p}"; $f = fopen("phar://" . $p . "/dir/deflated.txt", "r"); echo fread($f, 64); fclose($f);"#,
        p = path.display()
    );
    let out = compile_and_run(&src);
    std::fs::remove_file(&path).ok();
    assert_eq!(out, "runtime zip payload");
}

/// Verifies compiled PHP output for stream socket server creates listening socket.
#[test]
fn test_stream_socket_server_creates_listening_socket() {
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:0");
echo is_resource($srv) ? "r" : "x";
echo get_resource_type($srv);
"#,
    );
    assert_eq!(out, "rstream");
}

/// Verifies compiled PHP output for stream socket client tcp nodelay does not crash.
#[test]
fn test_stream_socket_client_tcp_nodelay_does_not_crash() {
    // socket.tcp_nodelay = 1 triggers __rt_apply_socket_client_opts after
    // connect, which sets TCP_NODELAY via setsockopt. The setsockopt result
    // isn't observable from PHP (best-effort) but the helper must not blow
    // up the connection sequence.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:0");
$addr = stream_socket_get_name($srv, false);
stream_context_set_option(stream_context_get_default(), "socket", "tcp_nodelay", 1);
$client = stream_socket_client("tcp://" . $addr);
echo is_resource($client) ? "ok" : "fail";
if ($client) { fclose($client); }
fclose($srv);
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for stream socket client so broadcast does not crash.
#[test]
fn test_stream_socket_client_so_broadcast_does_not_crash() {
    // socket.so_broadcast = 1 triggers __rt_apply_socket_client_opts, which sets
    // SO_BROADCAST on the UDP socket via setsockopt. Not observable from PHP
    // (best-effort) but the option must be accepted without breaking the socket.
    // STREAM_SERVER_BIND is required for any datagram server; see
    // test_datagram_server_refuses_the_default_listen_flags.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("udp://127.0.0.1:0", $e, $m, STREAM_SERVER_BIND);
$addr = stream_socket_get_name($srv, false);
stream_context_set_option(stream_context_get_default(), "socket", "so_broadcast", 1);
$client = stream_socket_client("udp://" . $addr);
echo is_resource($client) ? "ok" : "fail";
if ($client) { fclose($client); }
fclose($srv);
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for stream socket client bindto binds local address.
#[test]
fn test_stream_socket_client_bindto_binds_local_address() {
    // socket.bindto = "127.0.0.1:0" routes through __rt_apply_socket_bindto
    // before connect(). After connect, the local end of the client socket
    // must report 127.0.0.1 as its address. The :0 lets the kernel pick
    // any free local port — we only assert on the host prefix.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:0");
$addr = stream_socket_get_name($srv, false);
stream_context_set_option(stream_context_get_default(), "socket", "bindto", "127.0.0.1:0");
$client = stream_socket_client("tcp://" . $addr);
$local = stream_socket_get_name($client, false);
echo strpos($local, "127.0.0.1:") === 0 ? "ok" : "bad";
fclose($client);
fclose($srv);
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for stream socket server ipv6 v6only does not crash.
#[test]
fn test_stream_socket_server_ipv6_v6only_does_not_crash() {
    // socket.ipv6_v6only = 1 is best-effort: the option only matters for
    // IPv6 sockets, and setsockopt fails silently on a v4 socket (EINVAL).
    // The bind/listen should still succeed.
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "socket", "ipv6_v6only", 1);
$srv = stream_socket_server("tcp://127.0.0.1:0");
echo is_resource($srv) ? "ok" : "fail";
if ($srv) { fclose($srv); }
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for stream socket server so reuseport does not crash.
#[test]
fn test_stream_socket_server_so_reuseport_does_not_crash() {
    // socket.so_reuseport = 1 triggers __rt_apply_socket_server_opts after
    // the socket() call but before bind(). The setsockopt call is best-
    // effort; this test only verifies the server still binds successfully.
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "socket", "so_reuseport", 1);
$srv = stream_socket_server("tcp://127.0.0.1:0");
echo is_resource($srv) ? "ok" : "fail";
if ($srv) { fclose($srv); }
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for stream socket server backlog accepts connection.
#[test]
fn test_stream_socket_server_backlog_accepts_connection() {
    // socket.backlog (read as a string, like ftp.resume_pos) feeds the listen()
    // backlog via __rt_socket_backlog instead of the hardcoded 128. A small
    // backlog must still bind, listen, and accept at least one connection.
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "socket", "backlog", "5");
$srv = stream_socket_server("tcp://127.0.0.1:0");
$addr = stream_socket_get_name($srv, false);
$client = stream_socket_client("tcp://" . $addr);
$conn = stream_socket_accept($srv);
echo is_resource($conn) ? "accepted" : "fail";
if ($conn) { fclose($conn); }
fclose($client);
fclose($srv);
"#,
    );
    assert_eq!(out, "accepted");
}

/// Verifies compiled PHP output for stream socket server backlog default when unset.
#[test]
fn test_stream_socket_server_backlog_default_when_unset() {
    // No backlog option set: __rt_socket_backlog falls back to the default 128
    // and the server still binds (regression for the miss path).
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:0");
echo is_resource($srv) ? "ok" : "fail";
if ($srv) { fclose($srv); }
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for unix socket server backlog does not crash.
#[test]
fn test_unix_socket_server_backlog_does_not_crash() {
    // Exercises the unix_socket_server backlog site (whose ARM64 path is a leaf
    // that now spills x30 around the __rt_socket_backlog call).
    let out = compile_and_run(
        r#"<?php
$path = "/tmp/elephc_backlog_test.sock";
@unlink($path);
stream_context_set_option(stream_context_get_default(), "socket", "backlog", "3");
$srv = stream_socket_server("unix://" . $path);
echo is_resource($srv) ? "ok" : "fail";
if ($srv) { fclose($srv); }
@unlink($path);
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for stream socket server rejects bad address.
#[test]
fn test_stream_socket_server_rejects_bad_address() {
    let out = compile_and_run(
        r#"<?php
echo stream_socket_server("garbage") === false ? "a" : "A";
echo stream_socket_server("tcp://999.1.2.3:80") === false ? "b" : "B";
"#,
    );
    assert_eq!(out, "ab");
}

/// Verifies compiled PHP output for stream socket client connects to server.
#[test]
fn test_stream_socket_client_connects_to_server() {
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:54731");
$cli = stream_socket_client("tcp://127.0.0.1:54731");
echo is_resource($cli) ? "connected" : "failed";
"#,
    );
    assert_eq!(out, "connected");
}

/// Mechanism guard for the enable_crypto SNI auto-default (#84): stream_socket_client
/// now records the transport host per fd via __rt_stash_connect_host before boxing
/// the result. This must not disturb the normal connect path — verify a full
/// client→server→client round-trip still works over a named-loopback address, and
/// that a failed connect (fd = -1, stash passthrough) still returns false.
#[test]
fn test_stream_socket_client_host_stash_does_not_break_connect() {
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:54838");
$cli = stream_socket_client("tcp://127.0.0.1:54838");
$conn = stream_socket_accept($srv);
fwrite($cli, "ping");
echo fread($conn, 4);
echo is_resource($cli) ? ":ok" : ":no";
$bad = stream_socket_client("tcp://127.0.0.1:1");
echo ($bad === false) ? ":closed" : ":open";
"#,
    );
    assert_eq!(out, "ping:ok:closed");
}

/// Verifies the socket error out-parameters carry the real failure, not a fixed guess.
///
/// The two outputs used to be a hardcoded `ECONNREFUSED` / `"Connection refused"` pair on
/// `fsockopen()` and nothing at all on `stream_socket_client()`. A `unix://` path that does not
/// exist pins the distinction: the answer must be `ENOENT`, which is 2 on both supported
/// platforms, rather than the connection-refused text a fixed guess would produce.
#[test]
fn test_socket_error_outputs_report_the_real_failure() {
    let out = compile_and_run(
        r#"<?php
$c = @stream_socket_client("unix:///nonexistent/elephc-probe.sock", $errno, $errstr, 1);
echo var_export($c === false, true), "|", $errno, "|", $errstr;
"#,
    );
    assert_eq!(out, "true|2|No such file or directory");
}

/// Verifies a successful call leaves the out-parameters at PHP's "nothing went wrong" values.
#[test]
fn test_socket_error_outputs_are_empty_after_a_successful_connect() {
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:0", $se, $ss);
$cli = stream_socket_client("tcp://" . stream_socket_get_name($srv, false), $ce, $cs, 5);
echo var_export($cli !== false, true), "|", $se, "|", var_export($ss, true);
echo "|", $ce, "|", var_export($cs, true);
fclose($cli);
fclose($srv);
"#,
    );
    assert_eq!(out, "true|0|''|0|''");
}

/// Verifies a server can rebind a port its own previous run has left in TIME_WAIT.
///
/// php-src sets `SO_REUSEADDR` on every socket it binds; elephc set it on the IPv6 path
/// only, so an IPv4 server that restarted answered `false` for roughly a minute. That is
/// the ordinary lifecycle of any server — stop it, change something, start it again — and
/// it is also why `test_stream_set_timeout_on_socket` failed four runs out of five when
/// run back to back, which is what surfaced this.
///
/// Binding, connecting and closing inside one program leaves the port in TIME_WAIT for the
/// second bind, which is the state SO_REUSEADDR exists for. A LIVE listener is a different
/// case and must still be refused — `test_stream_socket_server_reports_a_bind_failure_...`
/// above pins that, and the two together say SO_REUSEADDR without saying SO_REUSEPORT.
#[test]
fn test_stream_socket_server_rebinds_a_port_left_in_time_wait() {
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:0", $e1, $s1);
$addr = stream_socket_get_name($srv, false);
$cli = stream_socket_client("tcp://" . $addr);
$conn = stream_socket_accept($srv);
fclose($conn);
fclose($cli);
fclose($srv);
$again = @stream_socket_server("tcp://" . $addr, $e2, $s2);
echo var_export($again !== false, true), "|", $s2;
"#,
    );
    assert_eq!(out, "true|");
}

/// Verifies `stream_socket_server()` describes a bind failure the way php-src does.
///
/// php-src is measurably the odd one out here: it leaves `&$error_code` at `0` for every bind
/// and listen failure and puts the reason in `&$error_message` alone. Reporting the real `errno`
/// would be more informative and would not be PHP.
#[test]
fn test_stream_socket_server_reports_a_bind_failure_through_the_message_only() {
    let out = compile_and_run(
        r#"<?php
$first = stream_socket_server("tcp://127.0.0.1:0", $e1, $s1);
$taken = stream_socket_get_name($first, false);
$second = @stream_socket_server("tcp://" . $taken, $e2, $s2);
echo var_export($second === false, true), "|", $e2, "|", $s2;
fclose($first);
"#,
    );
    assert_eq!(out, "true|0|Address already in use");
}

/// Verifies an error number does not survive into the NEXT socket call.
///
/// The failure reason lives in one process-global, so a helper that never records one — the
/// `unix://` and IPv6 paths are reached by a tail call — would otherwise hand back whatever the
/// previous failure left there. The entry of each socket helper clears it for that reason.
#[test]
fn test_socket_error_outputs_do_not_leak_between_calls() {
    let out = compile_and_run(
        r#"<?php
$a = @stream_socket_client("tcp://127.0.0.1:1", $e1, $s1, 1);
$srv = stream_socket_server("tcp://127.0.0.1:0", $e2, $s2);
echo var_export($e1 !== 0, true), "|", $e2, "|", var_export($s2, true);
fclose($srv);
"#,
    );
    assert_eq!(out, "true|0|''");
}

/// Verifies compiled PHP output for stream socket client rejects closed port.
#[test]
fn test_stream_socket_client_rejects_closed_port() {
    let out =
        compile_and_run(r#"<?php var_dump(stream_socket_client("tcp://127.0.0.1:1") === false);"#);
    assert_eq!(out, "bool(true)\n");
}

/// Verifies compiled PHP output for stream socket accept exchanges data.
#[test]
fn test_stream_socket_accept_exchanges_data() {
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:54732");
$cli = stream_socket_client("tcp://127.0.0.1:54732");
$conn = stream_socket_accept($srv);
echo is_resource($conn) ? "a" : "x";
fwrite($cli, "ping");
echo fread($conn, 16);
"#,
    );
    assert_eq!(out, "aping");
}

/// Verifies compiled PHP output for stream socket accept timeout returns false.
#[test]
fn test_stream_socket_accept_timeout_returns_false() {
    // With no client connecting, stream_socket_accept() must respect the
    // timeout and return false instead of blocking forever. 0 seconds
    // (poll) is the strictest test of the select() gate.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:54933");
$conn = stream_socket_accept($srv, 0);
echo is_bool($conn) ? "timeout" : "got_conn";
"#,
    );
    assert_eq!(out, "timeout");
}

/// Verifies compiled PHP output for stream socket accept peer name inet.
#[test]
fn test_stream_socket_accept_peer_name_inet() {
    // The optional 3rd argument receives the peer A.B.C.D:port string for
    // IPv4 connections. The client's source port is ephemeral but the
    // host part is deterministic, so check the prefix.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:54934");
$cli = stream_socket_client("tcp://127.0.0.1:54934");
$peer = "";
$conn = stream_socket_accept($srv, -1, $peer);
echo is_resource($conn) ? "ok|" : "fail|";
echo substr($peer, 0, 10);
"#,
    );
    assert_eq!(out, "ok|127.0.0.1:");
}

/// Verifies compiled PHP output for stream socket accept peer name unix.
#[test]
fn test_stream_socket_accept_peer_name_unix() {
    // Unix-domain peers are anonymous unless the client bound a name first,
    // which stream_socket_client() does not do — so the peer_name slot ends
    // up as an empty string (matching PHP for unnamed Unix peers).
    let out = compile_and_run(
        r#"<?php
$path = "/tmp/elephc_accept_peer_test.sock";
unlink($path);
$srv = stream_socket_server("unix://" . $path);
$cli = stream_socket_client("unix://" . $path);
$peer = "preseed";
$conn = stream_socket_accept($srv, -1, $peer);
echo is_resource($conn) ? "ok|" : "fail|";
echo strlen($peer);
unlink($path);
"#,
    );
    assert_eq!(out, "ok|0");
}

/// Verifies compiled PHP output for stream get line splits on delimiter.
#[test]
fn test_stream_get_line_splits_on_delimiter() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("sgl.txt", "alpha\nbeta\ngamma");
$f = fopen("sgl.txt", "r");
echo stream_get_line($f, 100, "\n") . "|";
echo stream_get_line($f, 100, "\n") . "|";
echo stream_get_line($f, 100, "\n");
fclose($f);
unlink("sgl.txt");
"#,
    );
    assert_eq!(out, "alpha|beta|gamma");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream get line respects length cap.
#[test]
fn test_stream_get_line_respects_length_cap() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("sgl_cap.txt", "0123456789");
$f = fopen("sgl_cap.txt", "r");
echo stream_get_line($f, 4, "\n");
fclose($f);
unlink("sgl_cap.txt");
"#,
    );
    assert_eq!(out, "0123");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream get line loop terminates at eof.
///
/// The trailing newline leaves the stream positioned before EOF, so the loop runs a
/// third time and that read returns `false`. `false !== ""` holds, so reference PHP
/// counts three — the count is 3, not 2, and `php -n` agrees.
#[test]
fn test_stream_get_line_loop_terminates_at_eof() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("sgl_eof.txt", "x\ny\n");
$f = fopen("sgl_eof.txt", "r");
$count = 0;
while (!feof($f)) {
    $line = stream_get_line($f, 100, "\n");
    if ($line !== "") { $count = $count + 1; }
}
echo $count;
fclose($f);
unlink("sgl_eof.txt");
"#,
    );
    assert_eq!(out, "3");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `stream_get_line()` tells an empty segment apart from an exhausted stream.
///
/// A delimiter sitting at the read position strips the segment to nothing, which PHP
/// still reports as a string; only a stream with no byte left is false. Testing this
/// with `var_dump` rather than `.` concatenation is deliberate — string coercion turns
/// both answers into "" and the divergence disappears.
#[test]
fn test_stream_get_line_returns_false_only_once_nothing_remains() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("sgl_empty.txt", "a||||b");
$f = fopen("sgl_empty.txt", "r");
var_dump(stream_get_line($f, 100, "||"));
var_dump(stream_get_line($f, 100, "||"));
var_dump(stream_get_line($f, 100, "||"));
var_dump(stream_get_line($f, 100, "||"));
fclose($f);
unlink("sgl_empty.txt");
"#,
    );
    assert_eq!(
        out,
        "string(1) \"a\"\nstring(0) \"\"\nstring(1) \"b\"\nbool(false)\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a zero `$length` reads php-src's default chunk instead of nothing.
#[test]
fn test_stream_get_line_treats_zero_length_as_the_default_chunk() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("sgl_zero.txt", str_repeat("z", 9000));
$f = fopen("sgl_zero.txt", "r");
echo strlen(stream_get_line($f, 0)), "|", ftell($f);
fclose($f);
unlink("sgl_zero.txt");
"#,
    );
    assert_eq!(out, "8192|8192");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a negative `$length` raises php-src's verbatim `ValueError`.
#[test]
fn test_stream_get_line_rejects_a_negative_length() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("sgl_neg.txt", "data");
$f = fopen("sgl_neg.txt", "r");
try {
    stream_get_line($f, -1);
} catch (ValueError $e) {
    echo $e->getMessage();
}
fclose($f);
unlink("sgl_neg.txt");
"#,
    );
    assert_eq!(
        out,
        "stream_get_line(): Argument #2 ($length) must be greater than or equal to 0"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies compiled PHP output for stream set blocking toggles mode.
#[test]
fn test_stream_set_blocking_toggles_mode() {
    let out = compile_and_run(
        r#"<?php
echo stream_set_blocking(STDIN, false) ? "n" : "N";
echo stream_set_blocking(STDIN, true) ? "b" : "B";
"#,
    );
    assert_eq!(out, "nb");
}

/// Verifies nonblocking fread/fgets misses do not mark the stream EOF.
#[test]
fn test_nonblocking_socket_reads_do_not_mark_eof() {
    let out = compile_and_run(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
stream_set_blocking($pair[0], false);
$first = fread($pair[0], 5);
echo $first === "" ? "empty" : "data";
echo "|";
echo feof($pair[0]) ? "eof" : "open";
echo "|";
$line = fgets($pair[0]);
echo $line === false ? "false" : "line";
echo "|";
echo feof($pair[0]) ? "eof" : "open";
echo "|";
$char = fgetc($pair[0]);
echo $char === false ? "false" : "char";
echo "|";
echo feof($pair[0]) ? "eof" : "open";
echo "|";
fwrite($pair[1], "hi\n");
echo fgets($pair[0]);
echo feof($pair[0]) ? "eof" : "open";
"#,
    );
    assert_eq!(out, "empty|open|false|open|false|open|hi\nopen");
}

/// Verifies `stream_get_line()` treats a nonblocking miss as transient instead of EOF.
#[test]
fn test_nonblocking_stream_get_line_does_not_mark_eof() {
    let out = compile_and_run(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
stream_set_blocking($pair[0], false);
$miss = stream_get_line($pair[0], 8);
echo $miss === "" ? "empty" : "data";
echo "|";
echo feof($pair[0]) ? "eof" : "open";
echo "|";
fwrite($pair[1], "ready\n");
echo stream_get_line($pair[0], 8, "\n");
"#,
    );
    // A nonblocking miss consumed no byte, so it reads as false rather than "" — the
    // point of the test is the middle field: the miss must NOT latch EOF.
    assert_eq!(out, "data|open|ready");
}

/// Verifies compiled PHP output for stream socket shutdown on connection.
#[test]
fn test_stream_socket_shutdown_on_connection() {
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:54733");
$cli = stream_socket_client("tcp://127.0.0.1:54733");
$conn = stream_socket_accept($srv);
echo stream_socket_shutdown($conn, 2) ? "down" : "fail";
"#,
    );
    assert_eq!(out, "down");
}

/// Verifies compiled PHP output for gethostname returns nonempty string.
#[test]
fn test_gethostname_returns_nonempty_string() {
    let out = compile_and_run(r#"<?php echo strlen(gethostname()) > 0 ? "named" : "empty";"#);
    assert_eq!(out, "named");
}

/// Verifies compiled PHP output for gethostbyname resolves localhost.
#[test]
fn test_gethostbyname_resolves_localhost() {
    // gethostbyname() resolves a host name to its IPv4 address; a numeric
    // address resolves to itself.
    let out = compile_and_run(
        r#"<?php echo gethostbyname("localhost"); echo "|"; echo gethostbyname("127.0.0.1");"#,
    );
    assert_eq!(out, "127.0.0.1|127.0.0.1");
}

/// Verifies compiled PHP output for gethostbyname unresolved returns input.
#[test]
fn test_gethostbyname_unresolved_returns_input() {
    // PHP returns the host name unchanged when it cannot be resolved.
    let out = compile_and_run(r#"<?php echo gethostbyname("no-such-host.invalid");"#);
    assert_eq!(out, "no-such-host.invalid");
}

/// Verifies compiled PHP output for gethostbyaddr resolves valid address.
#[test]
fn test_gethostbyaddr_resolves_valid_address() {
    // gethostbyaddr() reverse-resolves a valid IPv4 address to a host name,
    // or returns the address unchanged when no record exists.
    let out = compile_and_run(
        r#"<?php echo strlen(gethostbyaddr("127.0.0.1")) > 0 ? "named" : "empty";"#,
    );
    assert_eq!(out, "named");
}

/// Verifies compiled PHP output for gethostbyaddr malformed address is false.
#[test]
fn test_gethostbyaddr_malformed_address_is_false() {
    // A malformed address yields PHP false.
    let out = compile_and_run(
        r#"<?php echo gethostbyaddr("not-an-ip-address") === false ? "false" : "?";"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for getprotobyname known protocols.
#[test]
fn test_getprotobyname_known_protocols() {
    let out = compile_and_run(
        r#"<?php
echo getprotobyname("tcp");
echo "|";
echo getprotobyname("udp");
echo "|";
echo getprotobyname("icmp");
"#,
    );
    assert_eq!(out, "6|17|1");
}

/// Verifies compiled PHP output for getprotobyname alias and missing.
#[test]
fn test_getprotobyname_alias_and_missing() {
    let out = compile_and_run(
        r#"<?php
echo getprotobyname("TCP");
echo "|";
echo getprotobyname("no_such_protocol") === false ? "false" : "?";
"#,
    );
    assert_eq!(out, "6|false");
}

/// Verifies compiled PHP output for getprotobynumber known numbers.
#[test]
fn test_getprotobynumber_known_numbers() {
    let out = compile_and_run(
        r#"<?php
echo getprotobynumber(6);
echo "|";
echo getprotobynumber(17);
echo "|";
echo getprotobynumber(1);
"#,
    );
    assert_eq!(out, "tcp|udp|icmp");
}

/// Verifies protocol zero and its host-defined name resolve in both directions.
#[test]
fn test_protocol_zero_host_name_round_trip() {
    // Protocol zero is named "ip" on some systems and "hopopt" on others.
    let out = compile_and_run(
        r#"<?php
$name = getprotobynumber(0);
echo $name . "|" . getprotobyname($name);
"#,
    );
    let (name, number) = out
        .split_once('|')
        .expect("expected protocol zero output in name|number format");
    assert!(!name.is_empty(), "expected a non-empty protocol name");
    assert_eq!(number, "0", "expected protocol name to round-trip to zero");
}

/// Verifies compiled PHP output for getprotobynumber persists across calls.
#[test]
fn test_getprotobynumber_persists_across_calls() {
    let out = compile_and_run(
        r#"<?php
$n = getprotobynumber(6);
$m = getprotobynumber(17);
echo $n . "/" . $m;
echo "|";
echo getprotobynumber(999) === false ? "false" : "?";
"#,
    );
    assert_eq!(out, "tcp/udp|false");
}

/// Verifies compiled PHP output for getservbyname known services.
#[test]
fn test_getservbyname_known_services() {
    let out = compile_and_run(
        r#"<?php
echo getservbyname("http", "tcp");
echo "|";
echo getservbyname("https", "tcp");
echo "|";
echo getservbyname("domain", "udp");
"#,
    );
    assert_eq!(out, "80|443|53");
}

/// Verifies compiled PHP output for getservbyname alias and missing.
#[test]
fn test_getservbyname_alias_and_missing() {
    let out = compile_and_run(
        r#"<?php
echo getservbyname("www", "tcp");
echo "|";
echo getservbyname("no_such_service", "tcp") === false ? "false" : "?";
"#,
    );
    assert_eq!(out, "80|false");
}

/// Verifies compiled PHP output for getservbyport known ports.
#[test]
fn test_getservbyport_known_ports() {
    let out = compile_and_run(
        r#"<?php
echo getservbyport(80, "tcp");
echo "|";
echo getservbyport(443, "tcp");
echo "|";
echo getservbyport(53, "udp");
"#,
    );
    assert_eq!(out, "http|https|domain");
}

/// Verifies compiled PHP output for getservbyport persists and missing.
#[test]
fn test_getservbyport_persists_and_missing() {
    let out = compile_and_run(
        r#"<?php
$a = getservbyport(80, "tcp");
$b = getservbyport(22, "tcp");
echo $a . "/" . $b;
echo "|";
echo getservbyport(80, "no_such_proto") === false ? "false" : "?";
"#,
    );
    assert_eq!(out, "http/ssh|false");
}

/// Verifies compiled PHP output for stream set timeout on socket.
#[test]
fn test_stream_set_timeout_on_socket() {
    // A short receive timeout makes the no-data fread() return instead of
    // blocking forever — the test completing at all proves it took effect.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:54734");
$cli = stream_socket_client("tcp://127.0.0.1:54734");
$conn = stream_socket_accept($srv);
echo stream_set_timeout($conn, 0, 50000) ? "set" : "fail";
echo "|";
$data = fread($conn, 10);
echo "done";
"#,
    );
    assert_eq!(out, "set|done");
}

/// Verifies compiled PHP output for stream socket sendto connected.
#[test]
fn test_stream_socket_sendto_connected() {
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:54736");
$cli = stream_socket_client("tcp://127.0.0.1:54736");
$conn = stream_socket_accept($srv);
echo stream_socket_sendto($cli, "ping");
echo "|";
echo fread($conn, 16);
"#,
    );
    assert_eq!(out, "4|ping");
}

/// Verifies compiled PHP output for stream socket recvfrom connected.
#[test]
fn test_stream_socket_recvfrom_connected() {
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:54738");
$cli = stream_socket_client("tcp://127.0.0.1:54738");
$conn = stream_socket_accept($srv);
stream_socket_sendto($cli, "first");
$a = stream_socket_recvfrom($conn, 32);
stream_socket_sendto($cli, "second");
$b = stream_socket_recvfrom($conn, 32);
echo $a . "/" . $b;
"#,
    );
    assert_eq!(out, "first/second");
}

/// Verifies compiled PHP output for stream socket recvfrom address out param.
#[test]
fn test_stream_socket_recvfrom_address_out_param() {
    // The optional 4th argument receives the sender address by reference.
    //
    // STREAM_SERVER_BIND is not decoration: PHP's default flags also ask for listen(), which no
    // datagram transport accepts, so a udp:// server opened without it is `false` in PHP too.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("udp://127.0.0.1:54745", $e, $m, STREAM_SERVER_BIND);
$cli = stream_socket_client("udp://127.0.0.1:54745");
fwrite($cli, "hello");
$addr = "";
$data = stream_socket_recvfrom($srv, 32, 0, $addr);
echo $data . "|" . substr($addr, 0, 10);
"#,
    );
    assert_eq!(out, "hello|127.0.0.1:");
}

/// Verifies compiled PHP output for stream socket recvfrom address overwrites slot.
#[test]
fn test_stream_socket_recvfrom_address_overwrites_slot() {
    // Regression: the address write-back must overwrite the variable's
    // string slot fully — pointer and length — so a pre-seeded value of a
    // different length cannot leak into the result.
    //
    // A `socketpair`-created Unix-domain socket has no bound name, so the
    // PHP-compatible sender address is the empty string. The pre-seeded
    // "PRESEED" length still has to be reset to 0 by the writeback.
    let out = compile_and_run(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
fwrite($pair[0], "hi");
$addr = "PRESEED";
$data = stream_socket_recvfrom($pair[1], 8, 0, $addr);
echo $data . "|" . $addr . "|" . strlen($addr);
"#,
    );
    assert_eq!(out, "hi||0");
}

/// Verifies compiled PHP output for udp socket round trip.
#[test]
fn test_udp_socket_round_trip() {
    // See test_stream_socket_recvfrom_address_out_param on STREAM_SERVER_BIND.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("udp://127.0.0.1:54740", $e, $m, STREAM_SERVER_BIND);
$cli = stream_socket_client("udp://127.0.0.1:54740");
fwrite($cli, "udp datagram");
echo fread($srv, 32);
"#,
    );
    assert_eq!(out, "udp datagram");
}

/// Verifies compiled PHP output for stream socket sendto to udp address.
#[test]
fn test_stream_socket_sendto_to_udp_address() {
    let out = compile_and_run(
        r#"<?php
$a = stream_socket_server("udp://127.0.0.1:54741", $e1, $m1, STREAM_SERVER_BIND);
$b = stream_socket_server("udp://127.0.0.1:54742", $e2, $m2, STREAM_SERVER_BIND);
echo stream_socket_sendto($b, "abc", 0, "udp://127.0.0.1:54741");
echo "|";
echo fread($a, 16);
"#,
    );
    assert_eq!(out, "3|abc");
}

/// A datagram transport cannot listen, so PHP fails the server its default flags ask for.
///
/// `$flags` defaults to `STREAM_SERVER_BIND|STREAM_SERVER_LISTEN`, and `listen()` is meaningless on
/// a datagram socket, so `stream_socket_server("udp://…")` is `false` in PHP unless the caller
/// passes `STREAM_SERVER_BIND` alone. elephc skipped `listen()` for udp and handed back a working
/// socket, so a script written against PHP took a branch PHP never takes.
///
/// Both datagram transports are checked, because the refusal has to sit ahead of the dispatch that
/// separates them; and the bind-only server is checked in the same program, because a refusal that
/// also broke the legitimate call would look just as green in the first assertion.
#[test]
fn test_datagram_server_refuses_the_default_listen_flags() {
    let out = compile_and_run_capture(
        r#"<?php
$e = 12345;
$m = "untouched";
$s = stream_socket_server("udp://127.0.0.1:54906", $e, $m);
echo ($s === false ? "false" : "resource"), "|", $e, "|[", $m, "]";
$ok = stream_socket_server("udp://127.0.0.1:54907", $e2, $m2, STREAM_SERVER_BIND);
echo "|", (is_resource($ok) ? "bind-only-open" : "bind-only-failed");
$g = stream_socket_server("udg://" . sys_get_temp_dir() . "/elephc_dgram_listen.sock");
echo "|", ($g === false ? "udg-false" : "udg-open");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "false|0|[]|bind-only-open|udg-false");
    // PHP words a failure nothing described as "Unknown error", not as an empty pair of
    // parentheses, and still leaves `&$error_message` empty — the two come from different places.
    assert!(
        out.diagnostics.contains(
            "Warning: stream_socket_server(): Unable to connect to udp://127.0.0.1:54906 (Unknown error)"
        ),
        "expected PHP's wording for a failure with no reason, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies compiled PHP output for unix socket round trip.
#[test]
fn test_unix_socket_round_trip() {
    let out = compile_and_run(
        r#"<?php
$path = "/tmp/elephc_unix_codegen_test.sock";
unlink($path);
$srv = stream_socket_server("unix://" . $path);
$cli = stream_socket_client("unix://" . $path);
$conn = stream_socket_accept($srv);
fwrite($cli, "unix payload");
echo fread($conn, 32);
unlink($path);
"#,
    );
    assert_eq!(out, "unix payload");
}

/// Verifies compiled PHP output for udg socket round trip.
#[test]
fn test_udg_socket_round_trip() {
    // udg:// is the Unix-domain datagram transport: the server binds (no
    // listen/accept, since datagrams are connectionless), and the client's
    // connect() sets the default destination so fwrite can send a datagram.
    // Being connectionless is also why STREAM_SERVER_BIND is required — PHP's
    // default flags ask for a listen() the transport cannot perform.
    let out = compile_and_run(
        r#"<?php
$path = "/tmp/elephc_udg_codegen_test.sock";
unlink($path);
$srv = stream_socket_server("udg://" . $path, $e, $m, STREAM_SERVER_BIND);
$cli = stream_socket_client("udg://" . $path);
fwrite($cli, "udg datagram");
echo fread($srv, 32);
unlink($path);
"#,
    );
    assert_eq!(out, "udg datagram");
}

/// Verifies compiled PHP output for stream socket sendto to udg address.
#[test]
fn test_stream_socket_sendto_to_udg_address() {
    // stream_socket_sendto() accepts a udg:// target: the sender must be a
    // bound Unix-domain datagram socket, but it doesn't have to be connected
    // to the receiver. The kernel routes the datagram by sockaddr_un path.
    let out = compile_and_run(
        r#"<?php
$srv_path = "/tmp/elephc_udg_sendto_srv.sock";
$cli_path = "/tmp/elephc_udg_sendto_cli.sock";
unlink($srv_path);
unlink($cli_path);
$srv = stream_socket_server("udg://" . $srv_path, $e1, $m1, STREAM_SERVER_BIND);
$cli = stream_socket_server("udg://" . $cli_path, $e2, $m2, STREAM_SERVER_BIND);
$n = stream_socket_sendto($cli, "udg-via-sendto", 0, "udg://" . $srv_path);
echo $n . "|" . fread($srv, 32);
unlink($srv_path);
unlink($cli_path);
"#,
    );
    assert_eq!(out, "14|udg-via-sendto");
}

/// Verifies compiled PHP output for stream socket sendto to unix address.
#[test]
fn test_stream_socket_sendto_to_unix_address() {
    // stream_socket_sendto() can also target a unix:// (SOCK_STREAM) listener
    // for connectionless writes from a separately-opened socket. The kernel
    // requires the sender's socket type and the target's type to be
    // compatible, so this exercises the Unix-domain sockaddr_un build through
    // the existing socketpair (SOCK_STREAM) sender.
    let out = compile_and_run(
        r#"<?php
$path = "/tmp/elephc_unix_sendto_test.sock";
unlink($path);
$srv = stream_socket_server("unix://" . $path);
$cli = stream_socket_client("unix://" . $path);
$conn = stream_socket_accept($srv);
$n = stream_socket_sendto($cli, "unix-via-sendto", 0, "");
echo $n . "|" . fread($conn, 32);
unlink($path);
"#,
    );
    assert_eq!(out, "15|unix-via-sendto");
}

/// Minimal one-shot passive-mode FTP server for the `ftp://` codegen test.
/// Binds the control port immediately, then serves one client on a thread by
/// dispatching on each command verb (so any login command order is accepted).
fn spawn_ftp_server(port: u16, content: &'static [u8]) -> std::thread::JoinHandle<()> {
    use std::io::{Read, Write};
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", port)).expect("ftp test: bind control port");
    std::thread::spawn(move || {
        let (mut ctrl, _) = listener.accept().expect("ftp test: accept control");
        let read_line = |s: &mut std::net::TcpStream| {
            let mut buf = Vec::new();
            let mut byte = [0u8; 1];
            while s.read(&mut byte).unwrap_or(0) == 1 {
                buf.push(byte[0]);
                if buf.ends_with(b"\r\n") {
                    break;
                }
            }
            buf
        };
        ctrl.write_all(b"220 ready\r\n").unwrap();
        let mut data_listener: Option<std::net::TcpListener> = None;
        loop {
            let cmd = read_line(&mut ctrl);
            if cmd.is_empty() {
                break;
            }
            let verb = cmd
                .split(|&b| b == b' ' || b == b'\r')
                .next()
                .unwrap_or(b"")
                .to_ascii_uppercase();
            match verb.as_slice() {
                b"USER" => ctrl.write_all(b"331 need password\r\n").unwrap(),
                b"PASS" => ctrl.write_all(b"230 logged in\r\n").unwrap(),
                b"TYPE" => ctrl.write_all(b"200 type set\r\n").unwrap(),
                b"PASV" => {
                    let dl = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
                    let dport = dl.local_addr().unwrap().port();
                    ctrl.write_all(
                        format!(
                            "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                            dport >> 8,
                            dport & 0xff
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                    data_listener = Some(dl);
                }
                b"RETR" => {
                    let dl = data_listener.take().expect("ftp test: RETR before PASV");
                    let (mut data, _) = dl.accept().unwrap();
                    ctrl.write_all(b"150 opening data connection\r\n").unwrap();
                    data.write_all(content).unwrap();
                    drop(data);
                    ctrl.write_all(b"226 transfer complete\r\n").unwrap();
                }
                b"QUIT" => {
                    ctrl.write_all(b"221 bye\r\n").unwrap();
                    break;
                }
                _ => ctrl.write_all(b"200 ok\r\n").unwrap(),
            }
        }
    })
}

/// FTP server variant that captures every control-channel command and
/// returns the captured-command log as the data-channel body so tests
/// can assert that specific commands (REST, etc.) were sent.
fn spawn_ftp_command_echo_server(port: u16) -> std::thread::JoinHandle<()> {
    use std::io::{Read, Write};
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", port)).expect("ftp test: bind control port");
    std::thread::spawn(move || {
        let (mut ctrl, _) = listener.accept().expect("ftp test: accept control");
        let read_line = |s: &mut std::net::TcpStream| {
            let mut buf = Vec::new();
            let mut byte = [0u8; 1];
            while s.read(&mut byte).unwrap_or(0) == 1 {
                buf.push(byte[0]);
                if buf.ends_with(b"\r\n") {
                    break;
                }
            }
            buf
        };
        ctrl.write_all(b"220 ready\r\n").unwrap();
        let mut data_listener: Option<std::net::TcpListener> = None;
        let mut commands: Vec<u8> = Vec::new();
        loop {
            let cmd = read_line(&mut ctrl);
            if cmd.is_empty() {
                break;
            }
            commands.extend_from_slice(&cmd);
            let verb = cmd
                .split(|&b| b == b' ' || b == b'\r')
                .next()
                .unwrap_or(b"")
                .to_ascii_uppercase();
            match verb.as_slice() {
                b"USER" => ctrl.write_all(b"331 need password\r\n").unwrap(),
                b"PASS" => ctrl.write_all(b"230 logged in\r\n").unwrap(),
                b"TYPE" => ctrl.write_all(b"200 type set\r\n").unwrap(),
                b"PASV" => {
                    let dl = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
                    let dport = dl.local_addr().unwrap().port();
                    ctrl.write_all(
                        format!(
                            "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                            dport >> 8,
                            dport & 0xff
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                    data_listener = Some(dl);
                }
                b"REST" => ctrl.write_all(b"350 restarting\r\n").unwrap(),
                b"RETR" => {
                    let dl = data_listener.take().expect("ftp test: RETR before PASV");
                    let (mut data, _) = dl.accept().unwrap();
                    ctrl.write_all(b"150 opening data connection\r\n").unwrap();
                    data.write_all(&commands).unwrap();
                    drop(data);
                    ctrl.write_all(b"226 transfer complete\r\n").unwrap();
                }
                b"QUIT" => {
                    ctrl.write_all(b"221 bye\r\n").unwrap();
                    break;
                }
                _ => ctrl.write_all(b"200 ok\r\n").unwrap(),
            }
        }
    })
}

/// Verifies compiled PHP output for fopen ftp resume pos sends rest command.
#[test]
fn test_fopen_ftp_resume_pos_sends_rest_command() {
    // Phase 11 B2: stream_context_create(['ftp' => ['resume_pos' => '1024']])
    // makes __rt_ftp_open send "REST 1024\r\n" between PASV and RETR.
    // The echo server captures every command and returns the log as
    // the data-channel body, so the test sees REST in the response.
    let _server = spawn_ftp_command_echo_server(54994);
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "ftp", "resume_pos", "1024");
$f = fopen("ftp://127.0.0.1:54994/pub/file.txt", "r");
$log = stream_get_contents($f);
fclose($f);
echo strpos($log, "REST 1024\r\n") !== false ? "has-rest" : "no-rest";
"#,
    );
    assert_eq!(out, "has-rest");
}

/// Verifies compiled PHP output for fopen ftp no resume pos skips rest.
#[test]
fn test_fopen_ftp_no_resume_pos_skips_rest() {
    // With no resume_pos in context, the runtime must NOT send REST.
    // (Sending REST 0 would still work but pollutes the protocol — the
    // builder skips the call entirely on a missed context lookup.)
    let _server = spawn_ftp_command_echo_server(54993);
    let out = compile_and_run(
        r#"<?php
$f = fopen("ftp://127.0.0.1:54993/pub/file.txt", "r");
$log = stream_get_contents($f);
fclose($f);
echo strpos($log, "REST") !== false ? "has-rest" : "no-rest";
"#,
    );
    assert_eq!(out, "no-rest");
}

/// Verifies compiled PHP output for fopen ftp retrieves file.
#[test]
fn test_fopen_ftp_retrieves_file() {
    // fopen("ftp://...") performs the anonymous passive-mode handshake and
    // returns the data connection as a readable stream.
    let _server = spawn_ftp_server(54965, b"contents fetched over ftp");
    let out = compile_and_run(
        r#"<?php
$f = fopen("ftp://127.0.0.1:54965/pub/file.txt", "r");
echo fread($f, 64);
fclose($f);
"#,
    );
    assert_eq!(out, "contents fetched over ftp");
}

/// `file_get_contents($url)` routes a runtime `ftp://` URL through the FTP
/// wrapper open path, then slurps the returned data connection.
#[test]
fn test_file_get_contents_dynamic_ftp_url() {
    let _server = spawn_ftp_server(54966, b"dynamic contents fetched over ftp");
    let out = compile_and_run(
        r#"<?php
$url = "ftp://127.0.0.1:54966/pub/file.txt";
echo file_get_contents($url);
"#,
    );
    assert_eq!(out, "dynamic contents fetched over ftp");
}

/// `file_get_contents($url)` routes a runtime `ftps://` URL through the FTP
/// TLS path; an unreachable control port deterministically returns PHP false
/// while still exercising TLS linkage and dynamic scheme dispatch.
#[test]
fn test_file_get_contents_dynamic_ftps_unreachable_is_false() {
    let out = compile_and_run(
        r#"<?php
$url = "ftps://127.0.0.1:1/pub/file.txt";
$r = @file_get_contents($url);
echo $r === false ? "false" : "got";
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for fopen ftp invalid url is false.
#[test]
fn test_fopen_ftp_invalid_url_is_false() {
    // An ftp:// URL without a path component fails like any bad fopen().
    let out = compile_and_run(
        r#"<?php $f = fopen("ftp://host-without-path", "r"); echo is_bool($f) ? "false" : "resource";"#,
    );
    assert_eq!(out, "false");
}

/// Minimal one-shot HTTP/1.0 server for the `http://` codegen test. Binds an
/// ephemeral port immediately (returned alongside the handle, so parallel and
/// orphaned test processes can never collide), then serves one client on a
/// thread: it drains the request headers and writes a close-framed response
/// whose body is `content`.
fn spawn_http_server(content: &'static [u8]) -> (std::thread::JoinHandle<()>, u16) {
    use std::io::{Read, Write};
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("http test: bind port");
    let port = listener.local_addr().expect("http test: local addr").port();
    let handle = std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().expect("http test: accept");
        // Drain the request up to the blank line that ends the headers.
        let mut req = Vec::new();
        let mut byte = [0u8; 1];
        while sock.read(&mut byte).unwrap_or(0) == 1 {
            req.push(byte[0]);
            if req.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let header = format!(
            "HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n",
            content.len()
        );
        sock.write_all(header.as_bytes()).unwrap();
        sock.write_all(content).unwrap();
        // Dropping the socket closes the connection so the client sees EOF.
    });
    (handle, port)
}

/// Same shape as `spawn_http_server` but echoes the received request bytes
/// back as the response body so tests can assert on the exact wire format
/// (method, path, headers, AND body) the elephc-built request produced.
fn spawn_http_echo_server() -> (std::thread::JoinHandle<()>, u16) {
    use std::io::{Read, Write};
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("http test: bind port");
    let port = listener.local_addr().expect("http test: local addr").port();
    let handle = std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().expect("http test: accept");
        let mut req = Vec::new();
        let mut byte = [0u8; 1];
        while sock.read(&mut byte).unwrap_or(0) == 1 {
            req.push(byte[0]);
            if req.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        // If a Content-Length header is present, also drain that many body
        // bytes — otherwise POST-style requests would never have their body
        // bytes echoed back, masking real propagation bugs in the client.
        if let Some(idx) = twoway_find(&req, b"\r\nContent-Length: ") {
            let start = idx + b"\r\nContent-Length: ".len();
            let end = req[start..]
                .iter()
                .position(|&b| b == b'\r')
                .map(|p| start + p)
                .unwrap_or(req.len());
            if let Ok(n) = std::str::from_utf8(&req[start..end])
                .unwrap_or("0")
                .trim()
                .parse::<usize>()
            {
                let mut body = vec![0u8; n];
                let _ = sock.read_exact(&mut body);
                req.extend_from_slice(&body);
            }
        }
        let header = format!(
            "HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n",
            req.len()
        );
        sock.write_all(header.as_bytes()).unwrap();
        sock.write_all(&req).unwrap();
    });
    (handle, port)
}

/// Serves two HTTP responses on the same port: the first is a 302 with a
/// `Location:` header pointing to `final_path` on the same `127.0.0.1:port`,
/// the second is a 200 with `body`. Used to exercise the follow_location
/// path through both relative and absolute Location values.
fn spawn_http_redirect_server(
    location: &str,
    final_path: &'static str,
    body: &'static [u8],
) -> (std::thread::JoinHandle<()>, u16) {
    use std::io::{Read, Write};
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("http redirect: bind port");
    let port = listener.local_addr().expect("http redirect: local addr").port();
    // the absolute-URL fixture needs the ephemeral port inside the Location header
    let location = location.replace("{PORT}", &port.to_string());
    let handle = std::thread::spawn(move || {
        let read_until_double_crlf = |sock: &mut std::net::TcpStream| {
            let mut req = Vec::new();
            let mut byte = [0u8; 1];
            while sock.read(&mut byte).unwrap_or(0) == 1 {
                req.push(byte[0]);
                if req.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            req
        };
        // Hop 1: respond 302 redirecting to `location`.
        let (mut s1, _) = listener.accept().expect("http redirect: accept hop 1");
        let _ = read_until_double_crlf(&mut s1);
        let r1 = format!(
            "HTTP/1.0 302 Found\r\nLocation: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            location
        );
        let _ = s1.write_all(r1.as_bytes());
        drop(s1);
        // Hop 2: serve the final body. Reject any unexpected path so the
        // assertion below pinpoints redirect-target bugs.
        let (mut s2, _) = listener.accept().expect("http redirect: accept hop 2");
        let req = read_until_double_crlf(&mut s2);
        let expected = format!("GET {} HTTP/1.0", final_path);
        if !req.starts_with(expected.as_bytes()) {
            let r2 = b"HTTP/1.0 400 Bad Request\r\nContent-Length: 0\r\n\r\n";
            let _ = s2.write_all(r2);
            return;
        }
        let r2 = format!(
            "HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        let _ = s2.write_all(r2.as_bytes());
        let _ = s2.write_all(body);
    });
    (handle, port)
}

/// Naive bytes-substring search — avoids pulling in extra crates for the
/// http test fixture.
fn twoway_find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
}

const TEST_HTTPS_CERT_PEM: &str = "\
-----BEGIN CERTIFICATE-----
MIIDDTCCAfWgAwIBAgIUYwEnFCptGtZ9bISKGHSDDyDeR78wDQYJKoZIhvcNAQEL
BQAwFjEUMBIGA1UEAwwLZWxlcGhjLXRlc3QwHhcNMjYwNjAxMTQzMzMzWhcNMzYw
NTI5MTQzMzMzWjAWMRQwEgYDVQQDDAtlbGVwaGMtdGVzdDCCASIwDQYJKoZIhvcN
AQEBBQADggEPADCCAQoCggEBALEueBZ5lUAbSBPd5gj6DdreVaIUC1sTKaOtK32f
gEgo8f+OvI7x0xZSB75t07Kz4luusaq1iYKegF61P8gI0ZpaNkj6uLVowj+Pu8/+
AMPrr11i38P701YLNvcOf4QWOnoDlRsjyzR+w4XbQmeNRrT1yUwkUQf64rZ3OkrD
tk4+VLizdj/eeoEXezGO/HzEY4vyFHA0ZC4GDT0yfjh77NOi7rY+7yr1DdbYzon/
JkPw3fV25m7StGsgr/a3i4ghVXUze88XSAYHWANUMmyJc2kxX33EAWB30n5yy0DN
ikN8emJqsRhpVU4MwlnD+5tPVBz9rgdXE8++I5i5uUvX65UCAwEAAaNTMFEwHQYD
VR0OBBYEFKx0E1bLjEIQqIzIzj0qhgpMIg0WMB8GA1UdIwQYMBaAFKx0E1bLjEIQ
qIzIzj0qhgpMIg0WMA8GA1UdEwEB/wQFMAMBAf8wDQYJKoZIhvcNAQELBQADggEB
AKeskQbHp//yz/LEJWqa2uCKB+05Uutg/yauByw2JGvFIdpGMXtOeFYh6PlbhVQL
rijdbW0mI0W2slefK6xsCJxFGfQY3daL2pLgoJSU0nkW7WkZh0ao292letIR9vFR
8cULtOtZZUSl8lq6Xt51mdUcCvAJgNctEI/+58YyDZBrUf0hKSjAQ2MGuZsHr8xT
S5TYFmrdKicmU53hVXsNgsCDmqENsZqP99zgqikvcrd1qfJQ95N/7thuSJtBJydk
IxMlsDmy7cFWp8ts9w+WvdxpGeZAs1M7I2N2SqTuHYVh3SJCrdA1rwtJZKTsctUJ
rmggbINQyJdm1RdcppwbOqA=
-----END CERTIFICATE-----
";

const TEST_HTTPS_KEY_PEM: &str = "\
-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCxLngWeZVAG0gT
3eYI+g3a3lWiFAtbEymjrSt9n4BIKPH/jryO8dMWUge+bdOys+JbrrGqtYmCnoBe
tT/ICNGaWjZI+ri1aMI/j7vP/gDD669dYt/D+9NWCzb3Dn+EFjp6A5UbI8s0fsOF
20JnjUa09clMJFEH+uK2dzpKw7ZOPlS4s3Y/3nqBF3sxjvx8xGOL8hRwNGQuBg09
Mn44e+zTou62Pu8q9Q3W2M6J/yZD8N31duZu0rRrIK/2t4uIIVV1M3vPF0gGB1gD
VDJsiXNpMV99xAFgd9J+cstAzYpDfHpiarEYaVVODMJZw/ubT1Qc/a4HVxPPviOY
ublL1+uVAgMBAAECggEAKW0fAMo+njWCvbplHXYxpRnU1cdv/ERXuQA1KfMQEE8a
fdEGvzlFTHOzgc+17pNmel83BR3a3+JlSz9/gSqmrzsmdBvC8g9jU28sz22pCiXh
46jJfs4zVGvc1xjZsa1s0LhjtWvCCC0XVAW22fVLMeZBwX7AP2hmd5ka1P47csF2
aDIPRPuWWCMse7u/31bJIpLOTJwLe1KmOsrk8IaQcjPUYC+WCA84N3QUwVUMVXvR
31bYy2s2fLZ/pO4EYCHJ2TDXuUSL4JYQ9ru7FPNWyGQo8cuTBexDWMiRb8qxFYNl
U5pAJuk4Om2v3CqIgCLK2PQB/lPrJkcUPEN4P5SGgQKBgQDeZux9GFcYpwZKTAr2
4rPU7ovCNTgAGyNh+5u/xaJ/6zNYDKH+EQujM35JhZR114nHYvigTzUj2VyTPMEq
ncyYoG+7sj99QqMNqIXK+d22UeYWmbSw/jf1XDzC7UHWXASViw/kL1y/jP4NXSjf
dAxSahyRnP+aYYNXAsmRWsV2YQKBgQDL8rUFs1nzX6WfHRQ5zzcPAF9XAGwkVKzQ
OKHCHfyLN9sfCnJrSOd1DU3JEwWZ6Qzl+BwAavaqDHY8PsV0pMtKSfO77yDZVFeE
ZdrJeQMv44DszZjZK/J9Vd7JDR+6Yg49+P4l438KrMsbIp/PaEe34ApgwfzU1LB5
XOORMcPZtQKBgQCk7CAc1+rmbh19BQzwbca7dTYQi1R+x6EibOnfeRh60Zieh6es
90jw+iOBM9yW0oHqaJtEjdgzQGGlEd2Q07m/yOFyh8kLA1pUq46jqUzfgbYlNlBH
HA21FnQ8fKJg6pW/q4LaTMDzjwNqN5YytiTZDLUoygrFmeBCqt98uZpKoQKBgB7W
5pSkGDf7AJpc1VAgi1zTW5dWUwPzYeZiieNGkYejvJinBcI/VfCXQGnlXHV3jiHA
MMvHYOE53S8i9sy6lpr3L8n9UORMIqe8lybcC6VUK4yjUjeUs6hMMdIJEAEpDqpE
Wnn0OqOsmVHTHINKa33cfPVAoDC2sLDJYQf1lH35AoGAd0pIqclrFb1a4Fbpq8TM
jgOspoq2Sjj+5724t8sFeg7SRMdTkA/8M1t4FsY9TNhDSI2vi6cu9013EcfVGlUB
MYQgldWOaXCRMQsHgapn+orK7iF89zA+4UDACVNiHEYS9q8CGynLckruklWdiyi3
6NdfPEjH08mFJU5npyEEa7Q=
-----END PRIVATE KEY-----
";

/// Minimal one-shot HTTPS server for deterministic `https://` wrapper tests.
/// Binds an ephemeral port and returns it alongside the handle.
fn spawn_https_server(content: &'static [u8]) -> (std::thread::JoinHandle<()>, u16) {
    use std::io::{Read, Write};
    use std::sync::Arc;

    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("https test: bind port");
    let port = listener.local_addr().expect("https test: local addr").port();
    let handle = std::thread::spawn(move || {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut cert_reader = TEST_HTTPS_CERT_PEM.as_bytes();
        let certs = rustls_pemfile::certs(&mut cert_reader)
            .collect::<Result<Vec<_>, _>>()
            .expect("https test: parse cert");
        let mut key_reader = TEST_HTTPS_KEY_PEM.as_bytes();
        let key = rustls_pemfile::private_key(&mut key_reader)
            .expect("https test: parse private key")
            .expect("https test: private key present");
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .expect("https test: build server config");

        let (tcp, _) = listener.accept().expect("https test: accept");
        tcp.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("https test: set read timeout");
        let conn =
            rustls::ServerConnection::new(Arc::new(config)).expect("https test: new connection");
        let mut tls = rustls::StreamOwned::new(conn, tcp);
        let mut request = [0u8; 1024];
        let _ = tls.read(&mut request);
        let headers = format!("HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n", content.len());
        tls.write_all(headers.as_bytes()).expect("https test: write headers");
        tls.write_all(content).expect("https test: write body");
        tls.flush().expect("https test: flush response");
    });
    (handle, port)
}

/// Verifies compiled PHP output for fopen http method default is get.
#[test]
fn test_fopen_http_method_default_is_get() {
    // Without a stream context, the request method falls back to "GET".
    // The echo server reflects the request bytes; the response body must
    // start with "GET /path HTTP/1.0\r\n".
    let (_server, port) = spawn_http_echo_server();
    let out = compile_and_run(
        &r#"<?php
$f = fopen("http://127.0.0.1:PHP_TEST_PORT/echo", "r");
$req = stream_get_contents($f);
fclose($f);
echo substr($req, 0, 19);
"#
        .replace("PHP_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "GET /echo HTTP/1.0\r");
}

/// Verifies compiled PHP output for fopen http method overrides via context.
#[test]
fn test_fopen_http_method_overrides_via_context() {
    // Phase 11 B2: stream_context_create(['http' => ['method' => 'POST']])
    // propagates through __rt_http_build_request → the request line
    // starts with "POST" instead of the default "GET".
    let (_server, port) = spawn_http_echo_server();
    let out = compile_and_run(
        &r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "method", "POST");
$f = fopen("http://127.0.0.1:PHP_TEST_PORT/api", "r");
$req = stream_get_contents($f);
fclose($f);
echo substr($req, 0, 21);
"#
        .replace("PHP_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "POST /api HTTP/1.0\r\nH");
}

/// Verifies compiled PHP output for fopen http header inserted via context.
#[test]
fn test_fopen_http_header_inserted_via_context() {
    // Phase 11 B2: stream_context_create(['http' => ['header' => ...]])
    // propagates through __rt_http_build_request — the supplied header
    // line lands between the Host: line and the Connection: close line.
    let (_server, port) = spawn_http_echo_server();
    let out = compile_and_run(
        &r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "header", "X-Trace: abc");
$f = fopen("http://127.0.0.1:PHP_TEST_PORT/path", "r");
$req = stream_get_contents($f);
fclose($f);
echo strpos($req, "\r\nX-Trace: abc\r\n") !== false ? "has-header" : "no-header";
"#
        .replace("PHP_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "has-header");
}

/// Verifies compiled PHP output for fopen http content only emits body.
#[test]
fn test_fopen_http_content_only_emits_body() {
    // Reduced repro of the POST + content gap: set only ['http']['content']
    // without 'method'. If this passes, the bug is in set_option_4's two-call
    // sub-hash merge; if this fails, it's in the content lookup or emission.
    let (_server, port) = spawn_http_echo_server();
    let out = compile_and_run(
        &r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "content", "x=y");
$f = fopen("http://127.0.0.1:PHP_TEST_PORT/p", "r");
$req = stream_get_contents($f);
fclose($f);
$has_clen = strpos($req, "\r\nContent-Length: 3\r\n") !== false;
$has_body = strpos($req, "\r\n\r\nx=y") !== false;
echo ($has_clen ? "clen-ok" : "clen-MISSING") . "|" . ($has_body ? "body-ok" : "body-MISSING");
"#
        .replace("PHP_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "clen-ok|body-ok");
}

/// Verifies compiled PHP output for fopen http content post body with content length.
#[test]
fn test_fopen_http_content_post_body_with_content_length() {
    // Phase 11 B2 + post-deliverable: setting ['http']['content'] alongside
    // ['method' => 'POST'] propagates a Content-Length: N header and writes
    // the body bytes after the blank line. The echo server reflects the
    // raw request bytes so we can grep for both the header and the body.
    let (_server, port) = spawn_http_echo_server();
    let out = compile_and_run(
        &r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "method", "POST");
stream_context_set_option(stream_context_get_default(), "http", "content", "foo=bar&baz=qux");
$f = fopen("http://127.0.0.1:PHP_TEST_PORT/submit", "r");
$req = stream_get_contents($f);
fclose($f);
$has_clen = strpos($req, "\r\nContent-Length: 15\r\n") !== false;
$has_body = strpos($req, "\r\n\r\nfoo=bar&baz=qux") !== false;
echo ($has_clen ? "clen-ok" : "clen-MISSING") . "|" . ($has_body ? "body-ok" : "body-MISSING");
"#
        .replace("PHP_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "clen-ok|body-ok");
}

/// Verifies compiled PHP output for fopen http retrieves body.
#[test]
fn test_fopen_http_retrieves_body() {
    // fopen("http://...") issues an HTTP GET and exposes the response body
    // with the headers stripped as a readable stream.
    let (_server, port) = spawn_http_server(b"body delivered over http");
    let out = compile_and_run(&format!(
        r#"<?php
$f = fopen("http://127.0.0.1:{port}/page.txt", "r");
echo stream_get_contents($f);
fclose($f);
"#
    ));
    assert_eq!(out, "body delivered over http");
}

/// Verifies `stream_get_meta_data()` on an `http://` stream carries `wrapper_data`.
///
/// php-src's `php_stream_url_wrap_http` stores the response header lines in
/// `stream->wrapperdata` and `_php_stream_get_metadata` copies them out under `wrapper_data` —
/// the SAME array it publishes as `$http_response_header`, status line first, in the order the
/// server sent them, written after the three fallback flags and before `wrapper_type`. Measured
/// on `php -n` 8.5.6 against a local server:
///
/// ```text
/// [timed_out] =>            [wrapper_data] => Array
/// [blocked] => 1                ( [0] => HTTP/1.1 200 OK
/// [eof] =>                        [1] => Host: 127.0.0.1:8933
///                                 [2] => Date: …
///                                 [5] => Content-Length: 11 )
/// [wrapper_type] => http    [stream_type] => tcp_socket/ssl    [mode] => r
/// ```
///
/// RED before the fix: elephc published the global and left the metadata key out entirely, and
/// the global itself carried a SEVENTH, empty entry — the blank line that closes the header
/// block sits inside the scanned region and was being pushed as a header of its own.
#[test]
fn test_http_meta_data_carries_the_response_headers() {
    let (_server, port) = spawn_http_server(b"metabody");
    let out = compile_and_run(&format!(
        r#"<?php
$f = fopen("http://127.0.0.1:{port}/page.txt", "r");
$m = stream_get_meta_data($f);
echo implode(",", array_keys($m)) . "\n";
echo count($m["wrapper_data"]) . "\n";
echo $m["wrapper_data"][0] . "\n";
echo $m["wrapper_data"][2] . "\n";
echo count($http_response_header) . "\n";
$plain = stream_get_meta_data(fopen("php://memory", "r+"));
echo array_key_exists("wrapper_data", $plain) ? "leaked" : "absent";
"#
    ));
    assert_eq!(
        out,
        "timed_out,blocked,eof,wrapper_data,wrapper_type,stream_type,mode,unread_bytes,seekable,uri\n\
         3\n\
         HTTP/1.0 200 OK\n\
         Content-Length: 8\n\
         3\n\
         absent"
    );
}

/// `file_get_contents("http://...")` opens the `http://` wrapper, slurps the
/// whole response body (headers stripped) into an owned string, and returns it
/// — equivalent to `fopen()` + `stream_get_contents()` + `fclose()` on the URL.
/// The owned-heap copy (via `__rt_str_persist`) survives the concat below.
#[test]
fn test_file_get_contents_over_http() {
    let (_server, port) = spawn_http_server(b"fgc over http body");
    let out = compile_and_run(&format!(
        r#"<?php
echo "[" . file_get_contents("http://127.0.0.1:{port}/page.txt") . "]";
"#
    ));
    assert_eq!(out, "[fgc over http body]");
}

/// Verifies php 8.4's `http_get_last_response_headers()` / `http_clear_last_response_headers()`.
///
/// MEASURED on `php -n` 8.5.6 against a local server: the getter answers `NULL`
/// before any request, the response's header lines (status line first) after one,
/// and `NULL` again after a clear. The `NULL` is the point — it is a different
/// answer from the empty array the shared header builder produces, which is why
/// the getter is a wrapper and not the builder itself.
#[test]
fn test_http_get_last_response_headers_is_null_around_the_request() {
    let (_server, port) = spawn_http_server(b"lastheaders");
    let out = compile_and_run(&format!(
        r#"<?php
var_dump(http_get_last_response_headers());
$f = fopen("http://127.0.0.1:{port}/page.txt", "r");
$h = http_get_last_response_headers();
echo is_array($h) ? "array" : "not-array", "\n";
echo count($h), "\n";
echo $h[0], "\n";
http_clear_last_response_headers();
var_dump(http_get_last_response_headers());
fclose($f);
"#
    ));
    assert_eq!(
        out,
        "NULL\n\
         array\n\
         3\n\
         HTTP/1.0 200 OK\n\
         NULL\n"
    );
}

/// Verifies the `$http_response_header` deprecation NAMES ITS LINE, as every php diagnostic does.
///
/// MEASURED on `php -n` 8.5.6, on this exact program:
///
/// ```text
/// Deprecated: The predefined locally scoped $http_response_header variable is deprecated, call
/// http_get_last_response_headers() instead in /path/test.php on line 3
/// x
///
/// Warning: Undefined variable $http_response_header in /path/test.php on line 3
/// y
/// ```
///
/// elephc printed the deprecation with NO location at all. The suffix is published per
/// instruction by the lowering, and this notice is emitted from the main prologue, where there is
/// no instruction to read a span from — so the channel had nothing to append.
///
/// KNOWN GAP, deliberately not asserted: php also raises `Warning: Undefined variable
/// $http_response_header` at the read itself, which elephc does not emit at all — a separate
/// hole, in the undefined-variable path rather than in the location one this test pins. Asserting
/// the deprecation line alone keeps the test honest about what was fixed; when the missing
/// warning lands, the expectation below grows a second line.
#[test]
fn test_http_response_header_deprecation_names_the_mention_line() {
    let out = compile_and_run_capture(
        r#"<?php
echo "x\n";
$v = $http_response_header;
echo "y\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "x\ny\n");
    assert_eq!(
        out.located_diagnostics,
        concat!(
            "Deprecated: The predefined locally scoped $http_response_header variable is ",
            "deprecated, call http_get_last_response_headers() instead in test.php on line 3\n",
        )
    );
}

/// Verifies php 8.5's `$http_response_header` deprecation is version-gated.
///
/// php raises it while COMPILING a file that names the variable, so it fires once
/// per file and before any script output — MEASURED on `php -n` 8.5.6, including
/// for a mention inside `if (false)`. elephc emits it from the main prologue for
/// the same reason, and only when the program actually names the variable, so a
/// program that uses `http_get_last_response_headers()` instead stays quiet.
#[test]
fn test_http_response_header_deprecation_is_gated_on_php_85() {
    use std::fs;
    for (version, expected) in [("8.4", false), ("8.5", true)] {
        let dir = make_cli_test_dir("elephc_http_response_header_dep");
        let php_path = dir.join("main.php");
        fs::write(
            &php_path,
            r#"<?php
$f = fopen("http://127.0.0.1:9/page.txt", "r");
if ($f !== false) { echo count($http_response_header); }
echo "done";
"#,
        )
        .unwrap();
        let output = elephc_cli_command(&dir)
            .arg("--php-version")
            .arg(version)
            .arg("--emit-asm")
            .arg(&php_path)
            .output()
            .expect("failed to emit assembly for the deprecation gate");
        assert!(
            output.status.success(),
            "{version}: --emit-asm failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let asm = fs::read_to_string(dir.join("main.s")).expect("emitted assembly");
        let present = asm.contains("locally scoped $http_response_header variable is deprecated");
        assert_eq!(
            present, expected,
            "{version}: the $http_response_header deprecation must be emitted only from 8.5"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    // A program that never names the variable must not carry the notice at all.
    let dir = make_cli_test_dir("elephc_http_response_header_quiet");
    let php_path = dir.join("main.php");
    fs::write(
        &php_path,
        r#"<?php
var_dump(http_get_last_response_headers());
"#,
    )
    .unwrap();
    let output = elephc_cli_command(&dir)
        .arg("--php-version")
        .arg("8.5")
        .arg("--emit-asm")
        .arg(&php_path)
        .output()
        .expect("failed to emit assembly for the quiet case");
    assert!(output.status.success());
    let asm = fs::read_to_string(dir.join("main.s")).expect("emitted assembly");
    assert!(
        !asm.contains("locally scoped $http_response_header variable is deprecated"),
        "the replacement function must not drag the deprecation in"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `file_get_contents($url)` routes a runtime string beginning with `http://`
/// through the HTTP wrapper instead of the plain filesystem reader.
#[test]
fn test_file_get_contents_dynamic_http_url() {
    let (_server, port) = spawn_http_server(b"dynamic fgc over http");
    let out = compile_and_run(&format!(
        r#"<?php
$url = "http://127.0.0.1:{port}/page.txt";
echo "[" . file_get_contents($url) . "]";
"#
    ));
    assert_eq!(out, "[dynamic fgc over http]");
}

/// `file_get_contents("https://...")` succeeds against a local TLS server,
/// proving the literal HTTPS wrapper path returns an owned response body.
#[test]
fn test_file_get_contents_over_https_local_server() {
    let (_server, port) = spawn_https_server(b"fgc over local https");
    let out = compile_and_run(&format!(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "ssl", "verify_peer", "0");
echo "[" . file_get_contents("https://127.0.0.1:{port}/page.txt") . "]";
"#
    ));
    assert_eq!(out, "[fgc over local https]");
}

/// Returns the SHA-1 of the test server's leaf certificate DER, lowercase hex.
///
/// This is the value a program would write as a bare `ssl.peer_fingerprint`
/// string: php-src infers the digest from the string's LENGTH, and 40 hex
/// characters means SHA-1.
fn test_https_cert_sha1_hex() -> String {
    let mut reader = TEST_HTTPS_CERT_PEM.as_bytes();
    let der = rustls_pemfile::certs(&mut reader)
        .next()
        .expect("fingerprint test: a certificate in the fixture")
        .expect("fingerprint test: parse the fixture certificate");
    let mut hasher = <sha1::Sha1 as sha1::Digest>::new();
    sha1::Digest::update(&mut hasher, der.as_ref());
    let digest = sha1::Digest::finalize(hasher);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Verifies `ssl.peer_fingerprint` pins the peer's leaf certificate.
///
/// MEASURED on `php -n` 8.5.6: a matching pin lets the request through, and a
/// mismatch prints `peer_fingerprint match failure` and then fails the open.
/// A BARE string is matched by length — 32 hex is MD5 and 40 is SHA-1 — so the
/// 40-character SHA-1 below is the spelling php recognizes without an array.
///
/// The pin is checked after the handshake, against the certificate the peer
/// actually presented, which is why it composes with `verify_peer => "0"`:
/// relaxing chain verification must not relax the pin.
#[test]
fn test_https_peer_fingerprint_pins_the_peer_certificate() {
    let sha1 = test_https_cert_sha1_hex();
    assert_eq!(sha1.len(), 40, "a SHA-1 hex digest is 40 characters");
    let (_server, port) = spawn_https_server(b"pinned body");
    let out = compile_and_run(&format!(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "ssl", "verify_peer", "0");
stream_context_set_option(stream_context_get_default(), "ssl", "peer_fingerprint", "{sha1}");
echo "[" . file_get_contents("https://127.0.0.1:{port}/page.txt") . "]";
"#
    ));
    assert_eq!(out, "[pinned body]");
}

/// Verifies a WRONG `ssl.peer_fingerprint` refuses the connection.
///
/// Without this the option was accepted and never checked, which is the worst
/// shape a security control can take: the program reads as pinned and is not.
#[test]
fn test_https_peer_fingerprint_mismatch_fails_the_open() {
    let (_server, port) = spawn_https_server(b"never delivered");
    let wrong = "0".repeat(40);
    let out = compile_and_run(&format!(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "ssl", "verify_peer", "0");
stream_context_set_option(stream_context_get_default(), "ssl", "peer_fingerprint", "{wrong}");
echo (@file_get_contents("https://127.0.0.1:{port}/page.txt") === false) ? "refused" : "served";
"#
    ));
    assert_eq!(out, "refused");
}

/// Verifies a bare 64-character SHA-256 pin is refused, as it is in php.
///
/// php-src recognizes only two BARE lengths (32 = MD5, 40 = SHA-1); a 64-hex
/// string has no inferred algorithm and fails the match even when it is the
/// correct SHA-256 of the peer certificate — MEASURED on `php -n` 8.5.6 against
/// a public endpoint whose SHA-256 had been captured through `capture_peer_cert`.
#[test]
fn test_https_peer_fingerprint_bare_sha256_is_refused_like_php() {
    let (_server, port) = spawn_https_server(b"never delivered");
    let sha256_shaped = "a".repeat(64);
    let out = compile_and_run(&format!(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "ssl", "verify_peer", "0");
stream_context_set_option(stream_context_get_default(), "ssl", "peer_fingerprint", "{sha256_shaped}");
echo (@file_get_contents("https://127.0.0.1:{port}/page.txt") === false) ? "refused" : "served";
"#
    ));
    assert_eq!(out, "refused");
}

/// `file_get_contents($url)` also succeeds when the runtime string uses
/// `https://`, covering the non-literal dynamic URL dispatcher.
#[test]
fn test_file_get_contents_dynamic_https_local_server() {
    let (_server, port) = spawn_https_server(b"dynamic fgc over local https");
    let out = compile_and_run(&format!(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "ssl", "verify_peer", "0");
$url = "https://127.0.0.1:{port}/page.txt";
echo "[" . file_get_contents($url) . "]";
"#
    ));
    assert_eq!(out, "[dynamic fgc over local https]");
}

/// `file_get_contents($url)` routes a runtime `https://` URL through the HTTPS
/// wrapper dispatcher. A bad cafile fails before network I/O, making the TLS
/// path deterministic while still covering dynamic HTTPS linkage and parsing.
#[test]
fn test_file_get_contents_dynamic_https_cafile_bad_path_is_false() {
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "ssl", "cafile", "/nonexistent/elephc/ca.pem");
$url = "https://127.0.0.1:9/";
$r = @file_get_contents($url);
echo $r === false ? "false" : "got";
"#,
    );
    assert_eq!(out, "false");
}

/// `file_get_contents()` of an unreachable `http://` URL returns PHP `false`
/// (the wrapper open fails, so the result boxes bool false).
#[test]
fn test_file_get_contents_over_http_failure_is_false() {
    let out = compile_and_run(
        r#"<?php
$r = file_get_contents("http://127.0.0.1:1/nope");
echo $r === false ? "false" : "got";
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for fopen http follow location relative path.
#[test]
fn test_fopen_http_follow_location_relative_path() {
    // 302 with a Location: /new redirects to the same host. The redirect
    // loop in __rt_http_open re-issues GET /new and serves the second body.
    let (_server, port) = spawn_http_redirect_server("/new", "/new", b"after-relative-redirect");
    let out = compile_and_run(&format!(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "follow_location", "1");
stream_context_set_option(stream_context_get_default(), "http", "max_redirects", "5");
$f = fopen("http://127.0.0.1:{port}/start", "r");
echo stream_get_contents($f);
fclose($f);
"#
    ));
    assert_eq!(out, "after-relative-redirect");
}

/// Verifies compiled PHP output for fopen http follow location absolute same host.
#[test]
fn test_fopen_http_follow_location_absolute_same_host() {
    // 302 with a Location: http://127.0.0.1:53902/final — same-host absolute
    // URLs are rewritten to /final and followed exactly like a relative
    // redirect. The fixture rejects any path other than /final, so this
    // test fails if the host:port parsing leaves stray prefix bytes in the
    // redirect path buffer.
    let (_server, port) = spawn_http_redirect_server(
        "http://127.0.0.1:{PORT}/final",
        "/final",
        b"after-absolute-redirect",
    );
    let out = compile_and_run(&format!(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "follow_location", "1");
stream_context_set_option(stream_context_get_default(), "http", "max_redirects", "5");
$f = fopen("http://127.0.0.1:{port}/start", "r");
echo stream_get_contents($f);
fclose($f);
"#
    ));
    assert_eq!(out, "after-absolute-redirect");
}

/// Verifies compiled PHP output for fopen http follow location cross host is not followed.
#[test]
fn test_fopen_http_follow_location_cross_host_is_not_followed() {
    // 302 with a Location: pointing to a different host:port is NOT followed
    // (cross-host redirect requires reconnecting, deferred for v1). The
    // initial 302 response is surfaced as-is; the body is empty because the
    // redirect response itself has Content-Length: 0.
    let (_server, port) = spawn_http_redirect_server(
        "http://other-host.invalid:80/whatever",
        "/never-reached",
        b"unreachable",
    );
    let out = compile_and_run(&format!(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "follow_location", "1");
stream_context_set_option(stream_context_get_default(), "http", "max_redirects", "5");
stream_context_set_option(stream_context_get_default(), "http", "ignore_errors", "1");
$f = fopen("http://127.0.0.1:{port}/start", "r");
echo strlen(stream_get_contents($f));
fclose($f);
"#
    ));
    assert_eq!(out, "0");
}

/// Verifies compiled PHP output for fopen ftps invalid url is false.
#[test]
fn test_fopen_ftps_invalid_url_is_false() {
    // An ftps:// URL with no authority fails at compile-time URL parsing,
    // mirroring the existing https:// invalid-URL test. The binary still
    // links elephc-tls, so a passing test exercises the whole linkage path
    // (TLS function-pointer slots, the runtime helper, and the runner's
    // -L target/debug wiring) before any real network IO.
    let out = compile_and_run(
        r#"<?php $f = fopen("ftps://", "r"); echo is_bool($f) ? "false" : "resource";"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for fopen ftps unreachable host is false.
#[test]
fn test_fopen_ftps_unreachable_host_is_false() {
    // ftps://127.0.0.1:1/foo — port 1 is unbound so __rt_stream_socket_client
    // returns -1 and __rt_ftp_open falls into the fail path. Returns false
    // without exploding the AUTH TLS dance.
    let out = compile_and_run(
        r#"<?php $f = @fopen("ftps://127.0.0.1:1/x", "r"); echo is_bool($f) ? "false" : "resource";"#,
    );
    assert_eq!(out, "false");
}

/// `file_get_contents("ftps://...")` reuses the ftps:// wrapper open plus the
/// shared slurp path; an unreachable host fails the open so the result is PHP
/// false. Also exercises the elephc-tls linkage the checker requires for ftps.
#[test]
fn test_file_get_contents_over_ftps_unreachable_is_false() {
    let out = compile_and_run(
        r#"<?php $r = @file_get_contents("ftps://127.0.0.1:1/x"); echo $r === false ? "false" : "got";"#,
    );
    assert_eq!(out, "false");
}

/// `file_get_contents("ftp://...")` over an unreachable host returns PHP false
/// (the ftp:// wrapper open fails), completing the URL-scheme coverage next to
/// the http:// success test.
#[test]
fn test_file_get_contents_over_ftp_unreachable_is_false() {
    let out = compile_and_run(
        r#"<?php $r = @file_get_contents("ftp://127.0.0.1:1/x"); echo $r === false ? "false" : "got";"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for fopen http invalid url is false.
#[test]
fn test_fopen_http_invalid_url_is_false() {
    // An http:// URL with no authority fails like any bad fopen().
    let out = compile_and_run(
        r#"<?php $f = fopen("http://", "r"); echo is_bool($f) ? "false" : "resource";"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for fopen https invalid url is false.
#[test]
fn test_fopen_https_invalid_url_is_false() {
    // An https:// URL with no authority fails at compile-time URL parsing.
    // The binary still links against the elephc-tls staticlib, so a passing
    // test here verifies the whole linkage path (TLS function pointer slots,
    // the runtime helper, the runner's -L target/debug wiring) before any
    // real network IO is involved.
    let out = compile_and_run(
        r#"<?php $f = fopen("https://", "r"); echo is_bool($f) ? "false" : "resource";"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for fopen https cafile bad path is false.
#[test]
fn test_fopen_https_cafile_bad_path_is_false() {
    // ssl.cafile routes the connect through elephc_tls_connect_cafile, which
    // loads the CA bundle BEFORE any TCP connect. A nonexistent cafile fails to
    // load → the connect returns -1 → fopen() returns false. This exercises the
    // cafile dispatch branch + the elephc-tls linkage deterministically (no
    // network), since the failure happens during cafile load.
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "ssl", "cafile", "/nonexistent/elephc/ca.pem");
$f = @fopen("https://127.0.0.1:9/", "r");
echo ($f === false) ? "false" : "open";
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for fopen https capath bad path is false.
#[test]
fn test_fopen_https_capath_bad_path_is_false() {
    // OOS Phase C: ssl.capath routes the connect through elephc_tls_connect_capath,
    // which scans the directory for CA certs BEFORE any TCP connect. A nonexistent
    // directory yields no certs → the connect returns -1 → fopen() returns false.
    // Exercises the capath dispatch branch + linkage deterministically (no network).
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "ssl", "capath", "/nonexistent/elephc/cadir");
$f = @fopen("https://127.0.0.1:9/", "r");
echo ($f === false) ? "false" : "open";
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for fopen https peer name and relaxed options fail closed.
#[test]
fn test_fopen_https_peer_name_and_relaxed_options_fail_closed() {
    // OOS Phase C: ssl.peer_name routes through elephc_tls_connect_peer_name
    // (verify the cert for a different name), and ssl.allow_self_signed /
    // ssl.verify_peer_name = "0" route through the relaxed (insecure) verifier.
    // Each connects to an unreachable port, so the connect fails and fopen()
    // returns false — this exercises the new dispatch branches + the elephc-tls
    // linkage deterministically (no live TLS server needed).
    let out = compile_and_run(
        r#"<?php
$d = stream_context_get_default();
stream_context_set_option($d, "ssl", "peer_name", "example.com");
echo (@fopen("https://127.0.0.1:9/", "r") === false) ? "P" : "p";
stream_context_set_option($d, "ssl", "peer_name", "");
stream_context_set_option($d, "ssl", "allow_self_signed", "1");
echo (@fopen("https://127.0.0.1:9/", "r") === false) ? "S" : "s";
stream_context_set_option($d, "ssl", "allow_self_signed", "");
stream_context_set_option($d, "ssl", "verify_peer_name", "0");
echo (@fopen("https://127.0.0.1:9/", "r") === false) ? "V" : "v";
"#,
    );
    assert_eq!(out, "PSV");
}

/// End-to-end smoke against a real HTTPS host pinned to a custom CA bundle via
/// `ssl.cafile`. Requires outbound network plus a CA file on disk that signs
/// the host's chain, so it is `#[ignore]`d; it documents the manual
/// verification path for the cafile connect variant.
#[test]
#[ignore]
fn test_fopen_https_cafile_custom_bundle() {
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "ssl", "cafile", "/etc/ssl/cert.pem");
$f = fopen("https://example.com/", "r");
echo substr(stream_get_contents($f), 0, 15);
fclose($f);
"#,
    );
    assert_eq!(out, "<!doctype html>");
}

/// End-to-end smoke against a real HTTPS host with `ssl.verify_peer = false`.
/// example.com obviously has a valid cert, so this just exercises the
/// dispatcher: with verify_peer disabled the runtime must pick the insecure
/// connect path and still return a usable body. `#[ignore]` because it
/// requires outbound network access.
#[test]
#[ignore]
fn test_fopen_https_real_example_com_with_verify_peer_disabled() {
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "ssl", "verify_peer", "0");
$f = fopen("https://example.com/", "r");
$body = stream_get_contents($f);
fclose($f);
echo substr($body, 0, 15);
"#,
    );
    assert_eq!(out, "<!doctype html>");
}

/// End-to-end smoke against a real HTTPS host. The test is `#[ignore]`d
/// because it needs outbound network access, just like the rustls-level test
/// in `crates/elephc-tls`; run with `cargo test -- --ignored` to exercise it.
#[test]
#[ignore]
fn test_fopen_https_real_example_com() {
    let out = compile_and_run(
        r#"<?php
$f = fopen("https://example.com/", "r");
$body = stream_get_contents($f);
fclose($f);
echo substr($body, 0, 15);
"#,
    );
    assert_eq!(out, "<!doctype html>");
}

/// End-to-end smoke for `file_get_contents("https://...")` against a real
/// HTTPS host. Ignored because it needs outbound network access and a currently
/// trusted public certificate chain.
#[test]
#[ignore]
fn test_file_get_contents_https_real_example_com() {
    let out = compile_and_run(
        r#"<?php
$body = file_get_contents("https://example.com/");
echo substr($body, 0, 15);
"#,
    );
    assert_eq!(out, "<!doctype html>");
}

/// End-to-end smoke for dynamic `file_get_contents($url)` over HTTPS. Ignored
/// for the same outbound-network reason as the fopen HTTPS smoke tests.
#[test]
#[ignore]
fn test_file_get_contents_dynamic_https_real_example_com() {
    let out = compile_and_run(
        r#"<?php
$url = "https://example.com/";
$body = file_get_contents($url);
echo substr($body, 0, 15);
"#,
    );
    assert_eq!(out, "<!doctype html>");
}

/// End-to-end real-TLS handshake through `stream_socket_enable_crypto`: open a
/// plain TCP socket to a real HTTPS host, promote it to TLS in place (SNI /
/// cert-name taken from the `ssl.peer_name` context), then exchange an encrypted
/// HTTP request/response over the upgraded fd. Proves the rustls
/// `elephc_tls_attach_fd` path and the fread/fwrite TLS routing actually work,
/// not just the return-shape mechanism the non-ignored tests pin. `#[ignore]`d
/// because it needs outbound network access; run with `cargo test -- --ignored`.
#[test]
#[ignore]
fn test_stream_socket_enable_crypto_real_tls_handshake() {
    let out = compile_and_run(
        r#"<?php
stream_context_create(["ssl" => ["peer_name" => "example.com"]]);
$fp = stream_socket_client("tcp://example.com:443");
$ok = stream_socket_enable_crypto($fp, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
fwrite($fp, "GET / HTTP/1.0\r\nHost: example.com\r\nConnection: close\r\n\r\n");
$status = substr(fread($fp, 64), 0, 12);
fclose($fp);
echo ($ok ? "1" : "0") . "|" . $status;
"#,
    );
    assert_eq!(out, "1|HTTP/1.1 200");
}

/// End-to-end real-TLS teardown through `stream_socket_enable_crypto(false)`.
/// It upgrades a TCP socket to TLS, proves encrypted I/O works, then disables
/// crypto and closes the descriptor. Ignored because it needs outbound network.
#[test]
#[ignore]
fn test_stream_socket_enable_crypto_real_tls_disable_teardown() {
    let out = compile_and_run(
        r#"<?php
stream_context_create(["ssl" => ["peer_name" => "example.com"]]);
$fp = stream_socket_client("tcp://example.com:443");
$enabled = stream_socket_enable_crypto($fp, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
fwrite($fp, "GET / HTTP/1.0\r\nHost: example.com\r\nConnection: close\r\n\r\n");
$status = substr(fread($fp, 64), 0, 12);
$disabled = stream_socket_enable_crypto($fp, false);
fclose($fp);
echo ($enabled ? "1" : "0") . "|" . $status . "|" . ($disabled ? "1" : "0");
"#,
    );
    assert_eq!(out, "1|HTTP/1.1 200|1");
}

/// Minimal one-shot TCP server for the `fsockopen` codegen test. Binds the
/// port immediately, then serves one client on a thread by writing `content`
/// and closing the connection.
fn spawn_tcp_server(port: u16, content: &'static [u8]) -> std::thread::JoinHandle<()> {
    use std::io::Write;
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", port)).expect("tcp test: bind port");
    std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().expect("tcp test: accept");
        sock.write_all(content).unwrap();
        // Dropping the socket closes the connection so the client sees EOF.
    })
}

/// Minimal TCP server that writes two payload fragments with a pause between
/// them, forcing clients that request more bytes than the first fragment to
/// observe a short read before the rest of the payload arrives.
fn spawn_chunked_tcp_server(
    port: u16,
    first: &'static [u8],
    second: &'static [u8],
) -> std::thread::JoinHandle<()> {
    use std::io::Write;
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", port)).expect("tcp test: bind port");
    std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().expect("tcp test: accept");
        sock.write_all(first).unwrap();
        sock.flush().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(150));
        sock.write_all(second).unwrap();
    })
}

/// Verifies finite `stream_get_contents()` loops across short socket reads
/// until the requested length is filled, then leaves the remaining socket bytes
/// available for the next read.
#[test]
fn test_stream_get_contents_bounded_socket_read_fills_length() {
    let _server = spawn_chunked_tcp_server(54989, b"ab", b"cdefghi");
    let out = compile_and_run(
        r#"<?php
$s = stream_socket_client("tcp://127.0.0.1:54989");
echo stream_get_contents($s, 5);
echo "|" . stream_get_contents($s);
fclose($s);
"#,
    );
    assert_eq!(out, "abcde|fghi");
}

/// Verifies compiled PHP output for fsockopen connects and reads.
#[test]
fn test_fsockopen_connects_and_reads() {
    // fsockopen() connects a TCP socket; on success the error outputs are
    // cleared and the connected stream is readable.
    let _server = spawn_tcp_server(54990, b"data over fsockopen");
    let out = compile_and_run(
        r#"<?php
$errno = -1;
$errstr = "unset";
$s = fsockopen("127.0.0.1", 54990, $errno, $errstr);
echo ($s === false) ? "FAIL" : "ok";
echo "|errno=" . $errno;
echo "|errstr=[" . $errstr . "]";
echo "|" . stream_get_contents($s);
fclose($s);
"#,
    );
    assert_eq!(out, "ok|errno=0|errstr=[]|data over fsockopen");
}

/// Verifies compiled PHP output for fsockopen refused sets error.
#[test]
fn test_fsockopen_refused_sets_error() {
    // A refused connection returns false and fills the by-reference error
    // outputs; the error code is non-zero and the message is set.
    let out = compile_and_run(
        r#"<?php
$errno = 0;
$errstr = "";
$s = fsockopen("127.0.0.1", 54991, $errno, $errstr);
echo ($s === false) ? "false" : "resource";
echo "|" . ($errno !== 0 ? "errno-set" : "errno-zero");
echo "|" . $errstr;
"#,
    );
    assert_eq!(out, "false|errno-set|Connection refused");
}

/// Verifies compiled PHP output for pfsockopen connects and reads.
#[test]
fn test_pfsockopen_connects_and_reads() {
    // pfsockopen() is an alias of fsockopen() — persistence is meaningless in a
    // standalone compiled binary, so it connects, reads, and clears the
    // by-reference error outputs identically to fsockopen().
    let _server = spawn_tcp_server(54992, b"data over pfsockopen");
    let out = compile_and_run(
        r#"<?php
$errno = -1;
$errstr = "unset";
$s = pfsockopen("127.0.0.1", 54992, $errno, $errstr);
echo ($s === false) ? "FAIL" : "ok";
echo "|errno=" . $errno;
echo "|errstr=[" . $errstr . "]";
echo "|" . stream_get_contents($s);
fclose($s);
"#,
    );
    assert_eq!(out, "ok|errno=0|errstr=[]|data over pfsockopen");
}

/// Verifies compiled PHP output for stream wrapper register records class.
#[test]
fn test_stream_wrapper_register_records_class() {
    // stream_wrapper_register() stores the user wrapper registration. v1
    // accepts up to 16 entries and returns true; the wrapper class is not
    // yet invoked by fopen.
    let out = compile_and_run(
        r#"<?php
class CustomWrapper {}
echo stream_wrapper_register("custom", "CustomWrapper") ? "true" : "false";
echo "|";
echo stream_wrapper_register("alt", "CustomWrapper", 0) ? "true" : "false";
"#,
    );
    assert_eq!(out, "true|true");
}

/// Verifies a registration keeps its own copy of the scheme and class name.
///
/// The registry stored the caller's pointers verbatim, and a registration outlives the call:
/// reassigning the variable afterwards rewrote what had been registered. Measured before the
/// fix as `aa=0 bb=1 zz=1` where reference PHP answers `aa=1 bb=1 zz=0` — the first scheme
/// became unroutable and `zz://`, never registered by anyone, dispatched into the wrapper.
/// Every existing test passed a literal, which lives in rodata and never moves, so the whole
/// suite pinned the one case that could not fail.
#[test]
fn test_wrapper_registration_owns_its_scheme_after_the_caller_reassigns_it() {
    let out = compile_and_run(
        r#"<?php
class W {
    public function url_stat(string $path, int $flags) {
        return ['dev'=>0,'ino'=>0,'mode'=>33188,'nlink'=>1,'uid'=>0,'gid'=>0,
                'rdev'=>0,'size'=>7,'atime'=>0,'mtime'=>0,'ctime'=>0,
                'blksize'=>4096,'blocks'=>1];
    }
}
$s = "aa";
stream_wrapper_register($s, "W");
$s = "bb";
stream_wrapper_register($s, "W");
$s = "zz";
echo file_exists("aa://p") ? 1 : 0;
echo file_exists("bb://p") ? 1 : 0;
echo file_exists("zz://p") ? 1 : 0;
"#,
    );
    assert_eq!(out, "110");
}

/// Verifies which schemes `stream_wrapper_register()` accepts, and which ever dispatch.
///
/// Two separate rules, both read off reference PHP rather than inferred. Registration
/// refuses a protocol holding anything outside `[A-Za-z0-9+.-]`, and refuses one that is
/// already registered. Dispatch additionally ignores a ONE-LETTER scheme — PHP's
/// `php_stream_locate_url_wrapper` requires `n > 1`, because `f:` is a Windows drive
/// letter — so `f` registers successfully and still never routes anywhere.
///
/// Before this, every one of these was accepted: `x_y` and `x y` registered, a protocol
/// could be registered twice, and `f://` reached the wrapper.
#[test]
fn test_wrapper_scheme_acceptance_matches_php() {
    let out = compile_and_run_capture(
        r#"<?php
class W {
    public function url_stat(string $path, int $flags) {
        return ['dev'=>0,'ino'=>0,'mode'=>33188,'nlink'=>1,'uid'=>0,'gid'=>0,
                'rdev'=>0,'size'=>7,'atime'=>0,'mtime'=>0,'ctime'=>0,
                'blksize'=>4096,'blocks'=>1];
    }
}
foreach (["f", "fo", "x+y", "x-y", "x.y", "x_y", "x y"] as $scheme) {
    echo @stream_wrapper_register($scheme, "W") ? "1" : "0";
    echo @file_exists("$scheme://p") ? "1" : "0";
    echo " ";
}
echo "|", @stream_wrapper_register("fo", "W") ? "1" : "0";
"#,
    );
    // f registers but never dispatches; fo/x+y/x-y/x.y do both; x_y and x y do neither;
    // re-registering fo is refused.
    assert_eq!(out.stdout, "10 11 11 11 11 00 00 |0");
}

/// Verifies the minimum scheme length reaches EVERY dispatch path, not just one.
///
/// The rule is enforced by starting each wrapper-dispatch scan at index 2, and there are twelve
/// such scans across `fopen`, `url_stat`, the directory helpers, and the path-op family. A test
/// that only exercised `file_exists()` would pass with eleven of them still starting at zero,
/// so this drives one builtin per scan and pins both answers: a one-letter scheme reaches
/// nothing, a two-letter one reaches everything. Both rows are reference PHP's own output.
#[test]
fn test_minimum_scheme_length_applies_to_every_dispatch_path() {
    let out = compile_and_run_capture(
        r#"<?php
class W {
    public $context;
    public function stream_open($path, $mode, $options, &$opened): bool { return true; }
    public function stream_read($count): string { return "DATA"; }
    public function stream_eof(): bool { return true; }
    public function stream_close(): void {}
    public function url_stat($path, $flags) { return ['dev'=>0,'ino'=>0,'mode'=>33188,'nlink'=>1,
        'uid'=>0,'gid'=>0,'rdev'=>0,'size'=>99,'atime'=>0,'mtime'=>0,'ctime'=>0,
        'blksize'=>4096,'blocks'=>1]; }
    public function dir_opendir($path, $options): bool { return true; }
    public function dir_readdir() { return false; }
    public function dir_closedir(): void {}
    public function unlink($path): bool { return true; }
    public function mkdir($path, $mode, $options): bool { return true; }
    public function rmdir($path, $options): bool { return true; }
    public function rename($from, $to): bool { return true; }
}
stream_wrapper_register("q", "W");
stream_wrapper_register("qq", "W");
foreach (["q", "qq"] as $s) {
    echo @fopen("$s://x", "r") === false ? 0 : 1;
    echo @filesize("$s://x") === 99 ? 1 : 0;
    echo @opendir("$s://d") === false ? 0 : 1;
    echo @unlink("$s://a") ? 1 : 0;
    echo @mkdir("$s://d") ? 1 : 0;
    echo @rmdir("$s://d") ? 1 : 0;
    echo @rename("$s://a", "$s://b") ? 1 : 0;
    echo "|";
}
"#,
    );
    assert_eq!(out.stdout, "0000000|1111111|");
}

/// Verifies `stream_open`, `dir_opendir` and the path ops unbox an undeclared boolean result.
///
/// A boxed `false` is a NON-NULL pointer, so a helper that reads the result register raw turns
/// every refusal into a success. When these slots were wired in I could not build a wrapper
/// whose body made them infer `Mixed`, and said so rather than claim a fix I had not shown:
/// `return false;` infers `bool`, and returning an INITIALISED property infers that property's
/// type. The shape that does it is a property with NO initialiser assigned two different types,
/// which is what widens it — the same thing that made `stream_tell()` return a pointer.
///
/// Removing those slots from the boxed-result mask flips all six answers from 0 to 1.
#[test]
fn test_undeclared_boolean_refusals_are_unboxed_on_every_wrapper_path() {
    let out = compile_and_run_capture(
        r#"<?php
class Deny {
    public $context;
    private $state;
    public function seed(int $n) { $this->state = $n > 0 ? "no" : false; }
    public function stream_open($path, $mode, $options, &$opened) { $this->seed(0); return $this->state; }
    public function dir_opendir($path, $options) { $this->seed(0); return $this->state; }
    public function unlink($path) { $this->seed(0); return $this->state; }
    public function mkdir($path, $mode, $options) { $this->seed(0); return $this->state; }
    public function rmdir($path, $options) { $this->seed(0); return $this->state; }
    public function rename($from, $to) { $this->seed(0); return $this->state; }
}
stream_wrapper_register("wd", "Deny");
echo @fopen("wd://x", "r") === false ? 0 : 1;
echo @opendir("wd://d") === false ? 0 : 1;
echo @unlink("wd://a") ? 1 : 0;
echo @mkdir("wd://d") ? 1 : 0;
echo @rmdir("wd://d") ? 1 : 0;
echo @rename("wd://a", "wd://b") ? 1 : 0;
"#,
    );
    assert_eq!(out.stdout, "000000");
}

/// Verifies compiled PHP output for stream wrapper unregister round trip.
#[test]
fn test_stream_wrapper_unregister_round_trip() {
    // unregister removes a previously-registered protocol, then a fresh
    // register of the same protocol succeeds; unregistering an unknown
    // protocol returns false.
    let out = compile_and_run(
        r#"<?php
class W {}
stream_wrapper_register("foo", "W");
echo stream_wrapper_unregister("foo") ? "true" : "false";
echo "|";
echo stream_wrapper_unregister("foo") ? "true" : "false";
echo "|";
echo stream_wrapper_register("foo", "W") ? "true" : "false";
"#,
    );
    assert_eq!(out, "true|false|true");
}

/// Verifies `stream_wrapper_restore()` answers PHP's three cases, diagnostics included.
///
/// php 8.5.6 distinguishes them: a built-in that `stream_wrapper_unregister()` disabled is
/// restored silently and reports `true`; a built-in that was never disabled reports `true`
/// with a Notice; a scheme that never existed reports `false` with a Warning. The return
/// values already matched — the two diagnostics were missing.
///
/// Both travel on the diagnostic stream, in the order the calls run: PHP CLI writes every
/// severity to stdout through the output buffer, and elephc now does the same. The three return
/// values print on their own.
#[test]
fn test_stream_wrapper_restore_reports_phps_three_cases() {
    let out = compile_and_run_capture(
        r#"<?php
var_dump(stream_wrapper_restore("file"));
var_dump(stream_wrapper_restore("nosuch"));
stream_wrapper_unregister("file");
var_dump(stream_wrapper_restore("file"));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(true)\nbool(false)\nbool(true)\n");
    assert_eq!(
        out.diagnostics,
        "Notice: stream_wrapper_restore(): file:// was never changed, nothing to restore\n\
         Warning: stream_wrapper_restore(): nosuch:// never existed, nothing to restore\n"
    );
}

/// Verifies `@` suppresses the unknown-scheme Warning, as it does every runtime warning.
#[test]
fn test_stream_wrapper_restore_warning_is_suppressible() {
    let out = compile_and_run_capture(
        r#"<?php var_dump(@stream_wrapper_restore("nosuch"));"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\n");
    assert_eq!(out.stderr, "");
    assert_eq!(out.diagnostics, "");
}

/// Verifies compiled PHP output for stream socket enable crypto reads peer name from context.
#[test]
fn test_stream_socket_enable_crypto_reads_peer_name_from_context() {
    // Phase 11 B3 follow-up: enable_crypto navigates
    // _stream_context_options["ssl"]["peer_name"] for the SNI hint via
    // __rt_get_ssl_peer_name. We can't reach a real TLS server in tests
    // (the rustls handshake needs a live remote), so the contract pinned
    // here is "this code path doesn't crash and still returns a bool" —
    // exercising the helper's two nested hash_get's plus its hit branch
    // (peer_name is in context). Also asserts the options round-trip
    // through stream_context_get_options.
    let out = compile_and_run(
        r#"<?php
$ctx = stream_context_create(["ssl" => ["peer_name" => "example.com"]]);
$m = fopen("php://memory", "r+");
$r = stream_socket_enable_crypto($m, true);
echo is_bool($r) ? "bool|" : "non-bool|";
echo count(stream_context_get_options($ctx));
fclose($m);
"#,
    );
    assert_eq!(out, "bool|1");
}

/// Verifies compiled PHP output for stream socket enable crypto returns bool.
#[test]
fn test_stream_socket_enable_crypto_returns_bool() {
    // Phase 11 B3: stream_socket_enable_crypto invokes elephc_tls_attach_fd
    // on the fd. The rustls ClientConnection::new completes synchronously
    // (no I/O yet), so attach reports success even on degenerate fds like
    // php://memory; the failure surfaces on the first fread/fwrite when the
    // handshake actually runs. The shape of the return is the contract this
    // test pins — production code should also verify by attempting a read.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
$r = stream_socket_enable_crypto($m, true);
echo is_bool($r) ? "bool" : "non-bool";
fclose($m);
"#,
    );
    assert_eq!(out, "bool");
}

/// `stream_socket_enable_crypto($s, false)` unwinds a live TLS session: the
/// disable path reloads the fd and runs the shared `emit_tls_session_teardown`,
/// which (because the prior enable installed a non-zero `_tls_sessions[fd]`
/// handle) calls `_elephc_tls_close_fn` to send `close_notify` and zeroes the
/// slot, then reports `true`. The contract pinned here is that the enable→disable
/// sequence runs the real teardown branch without crashing and returns a `bool`
/// `true`; a plain-stream read-back is intentionally not asserted because the
/// `close_notify` record pollutes a degenerate `php://memory` backing buffer.
#[test]
fn test_stream_socket_enable_crypto_disable_tears_down_session() {
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
$a = stream_socket_enable_crypto($m, true);
$b = stream_socket_enable_crypto($m, false);
echo (is_bool($a) && is_bool($b) && $b === true) ? "ok" : "bad";
fclose($m);
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that the shared signature accepts the fourth named `session_stream` arg.
#[test]
fn test_stream_socket_enable_crypto_accepts_named_session_stream() {
    let out = compile_and_run(
        r#"<?php
function session_arg($stream) {
    echo "S";
    return $stream;
}
$m = fopen("php://memory", "r+");
$r = stream_socket_enable_crypto(stream: $m, enable: false, session_stream: session_arg($m));
echo $r ? "T" : "F";
fclose($m);
"#,
    );
    assert_eq!(out, "ST");
}

/// `ssl.local_cert` + `ssl.local_pk` select the mutual-TLS (client-certificate)
/// attach variant. A bogus cert/key path fails the client-auth config load
/// before any network I/O, so enable_crypto returns `false` — unlike the plain
/// server-auth attach, which reports `true` synchronously (see
/// `test_stream_socket_enable_crypto_returns_bool`). This pins that the
/// client-cert path is selected from the context and fails gracefully. A
/// successful client-cert handshake needs a client-auth-requiring server, so it
/// is covered by the `elephc-tls` crate unit tests instead.
#[test]
fn test_stream_socket_enable_crypto_client_cert_bad_path_fails() {
    let out = compile_and_run(
        r#"<?php
$ctx = stream_context_create(['ssl' => ['local_cert' => '/nonexistent/elephc-cc.pem', 'local_pk' => '/nonexistent/elephc-cc-key.pem']]);
$m = fopen("php://memory", "r+");
$r = stream_socket_enable_crypto($m, true);
echo $r === false ? "no" : "yes";
fclose($m);
"#,
    );
    assert_eq!(out, "no");
}

/// Verifies compiled PHP output for stream context create returns resource.
#[test]
fn test_stream_context_create_returns_resource() {
    // Context creation and the lazy default each return a registry resource
    // whose ContextState owns its independently persisted options and notifier.
    let out = compile_and_run(
        r#"<?php
$c = stream_context_create(["http" => ["method" => "POST"]]);
$d = stream_context_get_default();
echo is_resource($c) ? "ok" : "FAIL";
echo "|";
echo is_resource($d) ? "ok" : "FAIL";
echo "|";
echo stream_context_set_option($c, "http", "method", "GET") ? "set-ok" : "FAIL";
"#,
    );
    assert_eq!(out, "ok|ok|set-ok");
}

/// Verifies compiled PHP output for stream context get options returns array.
#[test]
fn test_stream_context_get_options_returns_array() {
    // get_options returns the addressed ContextState's live COW snapshot, while
    // get_params reconstructs the exact notification/options parameter map.
    let out = compile_and_run(
        r#"<?php
$c = stream_context_create(["http" => ["method" => "POST"]]);
echo gettype(stream_context_get_options($c));
echo "|" . count(stream_context_get_options($c));
echo "|";
echo gettype(stream_context_get_params($c));
"#,
    );
    assert_eq!(out, "array|1|array");
}

/// Verifies compiled PHP output for fopen accepts 4 arg form with context.
#[test]
fn test_fopen_accepts_4_arg_form_with_context() {
    // Phase 11 B2: fopen($file, $mode, $use_include_path, $context) compiles
    // and runs. The 3rd and 4th args are evaluated for their side effects
    // (so e.g. dynamic-context PHP code typechecks) but the open path still
    // uses the global _stream_context_options slot for any consumer logic.
    let out = compile_and_run(
        r#"<?php
$ctx = stream_context_create(["http" => ["method" => "GET"]]);
$m = fopen("php://memory", "r+", false, $ctx);
echo is_resource($m) ? "ok" : "fail";
fclose($m);
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that fopen() exposes its optional PHP parameter names to call planning.
#[test]
fn test_fopen_accepts_named_optional_args() {
    let out = compile_and_run(
        r#"<?php
$ctx = stream_context_create(["http" => ["method" => "GET"]]);
$m = fopen(filename: "php://memory", mode: "r+", use_include_path: false, context: $ctx);
echo is_resource($m) ? "ok" : "fail";
fclose($m);
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that literal fopen wrappers evaluate ignored optional args before opening.
#[test]
fn test_fopen_literal_wrapper_evaluates_optional_args_in_source_order() {
    let out = compile_and_run(
        r#"<?php
function mode_arg(): string { echo "M"; return "r+"; }
function use_include_path_arg(): bool { echo "U"; return false; }
function context_arg($ctx) { echo "C"; return $ctx; }
$ctx = stream_context_create();
$m = fopen("php://memory", mode_arg(), use_include_path_arg(), context_arg($ctx));
echo is_resource($m) ? "R" : "F";
fclose($m);
"#,
    );
    assert_eq!(out, "MUCR");
}

/// Verifies that non-literal fopen paths evaluate optional args before the open side effect.
#[test]
fn test_fopen_dynamic_path_evaluates_optional_args_before_open() {
    let out = compile_and_run(
        r#"<?php
function create_before_open(string $path): bool {
    echo "O";
    file_put_contents($path, "x");
    return false;
}
$path = tempnam(sys_get_temp_dir(), "elephc_fopen_order_");
unlink($path);
$f = fopen($path, "r", create_before_open($path));
echo is_resource($f) ? "R" : "F";
if ($f !== false) { fclose($f); }
unlink($path);
"#,
    );
    assert_eq!(out, "OR");
}

/// Verifies compiled PHP output for stream context set option four arg per option updates.
#[test]
fn test_stream_context_set_option_four_arg_per_option_updates() {
    // Phase 11 B2: the 4-arg form
    // stream_context_set_option(ctx, wrapper, opt, val) mutates the
    // persisted options[wrapper][opt] = val structure. Multiple calls
    // for the same wrapper accumulate options on the same sub-hash;
    // distinct wrappers grow the top-level hash.
    let out = compile_and_run(
        r#"<?php
$ctx = stream_context_create();
stream_context_set_option($ctx, "http", "method", "POST");
stream_context_set_option($ctx, "http", "header", "X-Trace: 1");
stream_context_set_option($ctx, "ssl", "peer_name", "example.com");
$opts = stream_context_get_options($ctx);
$out = "wrappers:" . count($opts);
foreach ($opts as $w => $sub) {
    $out .= "|" . $w . ":" . count($sub);
}
echo $out;
"#,
    );
    assert_eq!(out, "wrappers:2|http:2|ssl:1");
}

/// Verifies compiled PHP output for TLS cipher/security-level options accepted as no-ops.
#[test]
fn test_stream_context_ssl_cipher_options_are_accepted_noops() {
    let out = compile_and_run(
        r#"<?php
$ctx = stream_context_create();
$a = stream_context_set_option($ctx, "ssl", "ciphers", "DEFAULT@SECLEVEL=1");
$b = stream_context_set_option($ctx, "ssl", "security_level", "1");
$count = 0;
foreach (stream_context_get_options($ctx) as $wrapper => $sub) {
    if ($wrapper === "ssl") {
        $count = count($sub);
    }
}
echo ($a && $b ? "ok" : "FAIL") . "|" . $count;
"#,
    );
    assert_eq!(out, "ok|2");
}

/// Verifies the two-argument stream context option form merges wrapper maps.
#[test]
fn test_stream_context_set_option_two_arg_merges_options() {
    // The two-argument form merges incoming wrappers and each wrapper's option
    // map into the addressed ContextState, preserving entries absent from the patch.
    let out = compile_and_run(
        r#"<?php
$ctx = stream_context_create(["http" => ["method" => "POST"]]);
echo count(stream_context_get_options($ctx)) . "|";
stream_context_set_option($ctx, ["ssl" => ["verify_peer" => false], "http" => ["method" => "GET"]]);
echo count(stream_context_get_options($ctx));
"#,
    );
    assert_eq!(out, "1|2");
}

/// Verifies compiled PHP output for stream context get options empty when no create.
#[test]
fn test_stream_context_get_options_empty_when_no_create() {
    // Before any stream_context_create, the persisted-options slot is
    // null; stream_context_get_options falls back to an empty hash.
    let out = compile_and_run(
        r#"<?php
$d = stream_context_get_default();
echo count(stream_context_get_options($d));
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies compiled PHP output for the stream buffer setters on a non-wrapper stream.
#[test]
fn test_stream_set_buffer_stubs() {
    // stream_set_chunk_size returns the previous chunk size (8192 default on the
    // first call). The buffer setters do NOT both answer 0: measured on php 8.5.6,
    // `php://memory` answers 0 for the read buffer and -1 for the write buffer, the
    // same split a real file gives. This assertion used to read "8192|0|0", which was
    // the no-op lowering writing down its own return value.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
echo stream_set_chunk_size($m, 4096);
echo "|";
echo stream_set_read_buffer($m, 0);
echo "|";
echo stream_set_write_buffer($m, 0);
fclose($m);
"#,
    );
    assert_eq!(out, "8192|0|-1");
}

/// `stream_set_chunk_size` returns the PREVIOUS per-fd chunk size (PHP's
/// observable contract): the first call reports the 8192 default, and each
/// subsequent call reports the value set by the previous call.
#[test]
fn test_stream_set_chunk_size_returns_previous() {
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
echo stream_set_chunk_size($m, 4096);
echo "|";
echo stream_set_chunk_size($m, 2048);
echo "|";
echo stream_set_chunk_size($m, 1024);
fclose($m);
"#,
    );
    assert_eq!(out, "8192|4096|2048");
}

/// Pins PHP's own out-parameter idiom: `&$errno` / `&$errstr` passed undeclared.
///
/// PHP auto-vivifies a variable bound to a by-reference parameter, which is why every manual
/// example writes the call this way and never declares the two error variables. The parameters
/// are declared `ref(Int)` / `ref(Str)` in the registry, so the checker treats those argument
/// positions as definition sites and gives each variable the type the builtin writes.
#[test]
fn test_socket_out_parameters_may_be_undeclared() {
    let out = compile_and_run(
        r#"<?php
$s = @stream_socket_client("tcp://127.0.0.1:1", $errno, $errstr, 1);
echo var_export($s === false, true), "|", gettype($errno), "|", gettype($errstr);
"#,
    );
    assert_eq!(out, "true|integer|string");
}

/// Pins that a NAMED out-parameter binds the parameter it names, not the one sharing its index.
///
/// `error_message:` is the third parameter but the second argument here, so resolving by position
/// would type `$why` as `int` and the runtime would then write a string pointer into an integer
/// slot. It also pins that omitting `$error_code` is allowed: normalization materialises the
/// parameter's `null` default at that position, which a by-reference argument check must accept.
#[test]
fn test_named_out_parameter_binds_the_parameter_it_names() {
    let out = compile_and_run(
        r#"<?php
$c = @stream_socket_client("unix:///nonexistent/elephc-probe.sock", error_message: $why);
echo gettype($why), "=", $why;
"#,
    );
    assert_eq!(out, "string=No such file or directory");
}

/// Pins that a by-ref output still refuses an argument with nowhere to write back into.
#[test]
fn test_out_parameter_rejects_an_argument_without_storage() {
    let error = compile_expect_type_error(
        r#"<?php
$c = @stream_socket_client("tcp://127.0.0.1:1", 0, $errstr, 1);
"#,
    );
    assert!(
        error.contains("parameter $error_code must be passed a variable"),
        "expected the by-reference storage diagnostic, got: {error}"
    );
}

/// Pins that the undeclared out-parameter also works in statement position, where the call's
/// result is discarded — `flock()` is the non-socket member of the same family.
#[test]
fn test_flock_would_block_out_parameter_may_be_undeclared() {
    let out = compile_and_run(
        r#"<?php
$h = fopen("php://memory", "r+");
flock($h, LOCK_SH, $would);
echo gettype($would), "=", var_export($would, true);
fclose($h);
"#,
    );
    assert_eq!(out, "integer=0");
}

/// Pins that a by-ref out-parameter whose variable already holds an incompatible type reports
/// elephc's ordinary reassignment error.
///
/// The write used to go straight into the caller's slot without consulting its representation:
/// an `int` landing in a `string` slot overwrote the pointer half with a small integer, and the
/// program segfaulted on the next read. Binding the out-parameter through the normal assignment
/// merge is what turns that silent corruption into a diagnostic.
#[test]
fn test_by_ref_out_parameter_rejects_an_incompatible_variable() {
    let error = compile_expect_type_error(
        r#"<?php
$would = "untouched";
$h = fopen("php://memory", "r+");
flock($h, LOCK_SH, $would);
"#,
    );
    assert!(
        error.contains("cannot reassign $would from string to int"),
        "expected a reassignment diagnostic, got: {error}"
    );
}

/// Verifies `fopen()` honours a `php://` scheme in a path built at RUN TIME, not only in a
/// literal.
///
/// The wrapper dispatch is a compile-time chain over the constant-folded filename, and the
/// dynamic path used to recognise `http://` alone — so every other scheme opened as a plain file
/// name, failed to find it, and answered `false`. That is the shape real code takes: a function
/// receives its path as a parameter, so the literal-only dispatch was invisible until a caller
/// passed one in. `__rt_php_wrapper_open` now makes the same choices from the run-time bytes.
///
/// Measured against php 8.5.6, which opens all of these.
#[test]
fn test_fopen_honours_a_php_scheme_built_at_run_time() {
    let out = compile_and_run(
        r#"<?php
function probe(string $label, string $path, string $mode): void {
    $h = @fopen($path, $mode);
    echo $label, "=", var_export($h !== false, true), " ";
    if ($h !== false) { fclose($h); }
}
$p = "php://";
probe("memory", $p . "memory", "r+");
probe("temp", $p . "temp", "r+");
probe("stdout", $p . "stdout", "w");
probe("stderr", $p . "stderr", "w");
probe("input", $p . "input", "r");
probe("output", $p . "output", "w");
probe("fd1", $p . "fd/1", "w");
probe("maxmemory", $p . "temp/maxmemory:16", "r+");
echo "|";
$m = fopen($p . "memory", "r+");
fwrite($m, "round trip");
rewind($m);
echo stream_get_contents($m);
fclose($m);
"#,
    );
    assert_eq!(
        out,
        "memory=true temp=true stdout=true stderr=true input=true output=true fd1=true \
         maxmemory=true |round trip"
    );
}

/// Pins that a run-time `php://` URL naming no stream answers `false` rather than opening
/// something.
///
/// The dispatcher walks a table and reports `-1` for anything it does not recognise, which boxes
/// as PHP's `false`. Without this the unknown case would be indistinguishable from the schemes
/// that work.
#[test]
fn test_fopen_rejects_an_unknown_php_scheme_built_at_run_time() {
    let out = compile_and_run(
        r#"<?php
$p = "php://";
echo var_export(@fopen($p . "nosuchstream", "r"), true), "|";
echo var_export(@fopen($p . "fd/notanumber", "r"), true), "|";
echo var_export(@fopen($p, "r"), true);
"#,
    );
    assert_eq!(out, "false|false|false");
}

/// Verifies a run-time `php://` handle behaves like a literal one in the ways most likely to
/// break: descriptor ownership, filters, and independence.
///
/// A descriptor-backed scheme must hand out a `dup()` — closing a `php://stdout` handle that WAS
/// descriptor 1 would take the program's own output with it. A run-time handle must also accept a
/// filter and honour the filtered-read buffer, and two handles to `php://temp` must not share a
/// buffer.
#[test]
fn test_a_run_time_php_handle_behaves_like_a_literal_one() {
    let out = compile_and_run(
        r#"<?php
$p = "php://";
$o = fopen($p . "stdout", "w");
fwrite($o, "via-handle ");
fclose($o);
echo "still-alive|";

$f = fopen($p . "memory", "r+");
fwrite($f, "abcdef");
rewind($f);
stream_filter_append($f, "string.toupper", STREAM_FILTER_READ);
$parts = [];
while (!feof($f)) {
    $c = fread($f, 2);
    if ($c === "") { break; }
    $parts[] = $c;
}
echo implode(",", $parts), "|";
fclose($f);

$a = fopen($p . "temp", "r+");
$b = fopen($p . "temp", "r+");
fwrite($a, "AAA");
fwrite($b, "BBB");
rewind($a);
rewind($b);
echo fread($a, 3), fread($b, 3);
fclose($a);
fclose($b);
"#,
    );
    assert_eq!(out, "via-handle still-alive|AB,CD,EF|AAABBB");
}

/// Verifies a `php://filter` URL built at RUN TIME opens and filters.
///
/// A filter URL is "open this, then filter it", so the parse hands the open path the RESOURCE and
/// the named filter is attached once the stream exists. That keeps the resource on whatever open
/// path it deserves — this covers both a plain file and a nested `php://temp`, and checks that a
/// plain open afterwards does not inherit the filter.
#[test]
fn test_php_filter_url_built_at_run_time_opens() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("pf.txt", "hello");
$url = "php://filter/read=string.toupper/resource=" . "pf.txt";
$f = fopen($url, "r");
echo "file=", stream_get_contents($f), "|";
fclose($f);

$nested = "php://filter/read=string.toupper/resource=php://" . "temp";
$g = fopen($nested, "r+");
fwrite($g, "abc");
rewind($g);
echo "nested=", stream_get_contents($g), "|";
fclose($g);

$h = fopen("pf" . ".txt", "r");
echo "plain=", stream_get_contents($h);
fclose($h);
"#,
    );
    assert_eq!(out, "file=HELLO|nested=ABC|plain=hello");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a run-time filter URL that names nothing usable opens the resource unfiltered,
/// throws, or fails — each the way php does it.
///
/// An unknown filter name is what php-src tolerates by opening the resource plain. A URL with
/// no `/resource=` at all is answered with `Error: No URL resource specified` — a THROW, not a
/// warning, and `@` does not soften it; the same Error covers the literal spelling, where the
/// decision is made at compile time. The NESTED case still pins a KNOWN DIVERGENCE: php
/// recurses into a `resource=php://filter/...` and applies both levels, elephc refuses the
/// open — loudly, as `false` — until the parses learn to recurse.
#[test]
fn test_run_time_filter_url_edge_cases() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("pf.txt", "hello");
$unknown = "php://filter/read=no.such.filter/resource=" . "pf.txt";
$a = @fopen($unknown, "r");
echo "unknown=", var_export($a !== false, true);
if ($a !== false) { echo ":", stream_get_contents($a); fclose($a); }
$nores = "php://filter/read=string." . "toupper";
try {
    @fopen($nores, "r");
    echo " noresource=unreached";
} catch (Error $e) {
    echo " noresource=", $e->getMessage();
}
$nested = "php://filter/read=string.toupper/resource=php://filter/read=string." . "tolower";
echo " nested=", var_export(@fopen($nested, "r"), true);
"#,
    );
    assert_eq!(
        out,
        "unknown=true:hello noresource=No URL resource specified nested=false"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a RUN-TIME `php://filter` chain runs every filter, in order.
///
/// The literal path resolved the whole `|` chain; the run-time parse stopped at the first name and
/// said nothing, so `read=a|b` answered `a`'s output. That is the worst shape a wrong answer takes
/// — plausible bytes, no diagnostic — and it only reached this path when the URL was assembled
/// rather than written out, which is why the literal test above stayed green throughout.
///
/// `convert.base64-encode` and `string.toupper` do not commute, so swapping them proves the ORDER
/// is right rather than just the count. The third case pins that an unrecognised name is SKIPPED
/// and its neighbours still apply — the same reading the literal path was measured against.
#[test]
fn test_run_time_filter_chain_applies_every_filter_in_order() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("rtchain.txt", "Hello World");
$res = "rtchain" . ".txt";
$a = fopen("php://filter/read=convert.base64-encode|string.toupper/resource=" . $res, "r");
echo stream_get_contents($a), "|";
fclose($a);
$b = fopen("php://filter/read=string.toupper|convert.base64-encode/resource=" . $res, "r");
echo stream_get_contents($b), "|";
fclose($b);
$c = fopen("php://filter/read=string.toupper|no.such.filter/resource=" . $res, "r");
echo stream_get_contents($c), "|";
fclose($c);
$d = fopen("php://filter/read=string.tolower|string.rot13|string.toupper/resource=" . $res, "r");
echo stream_get_contents($d);
fclose($d);
unlink("rtchain.txt");
"#,
    );
    // The same four expectations `php -n` 8.5.6 produces for these URLs. The fourth runs THREE
    // names, because a two-slot hand-off would pass a two-filter test and still drop the tail.
    assert_eq!(
        out,
        "SGVSBG8GV29YBGQ=|SEVMTE8gV09STEQ=|HELLO WORLD|URYYB JBEYQ"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `file_get_contents()` reads a RUN-TIME `php://filter` URL through the chain.
///
/// The literal spelling worked and `fopen()` on the same dynamic URL worked; the byte reader
/// was the one consumer left out, because it never creates a stream and a filter chain has
/// nowhere to attach. The route opens the RESOURCE through the same runtime openers `fopen()`
/// dispatches to — a plain file and a data:// URI are both covered here — attaches the parked
/// chain, and reads through it.
///
/// The failure wording is part of the assertion: php names `file_get_contents` and the WHOLE
/// URL with the wrapper's generic `operation failed`, not the inner opener and the bare
/// resource path — the inner warning is suppressed through the same depth counter `@` uses,
/// so the `@`-suppressed probe must print nothing at all.
///
/// The `no.such|missing.too` read once expected NO output at all, which was this test reading
/// the implementation back to itself: the run-time parse dropped a name it could not resolve
/// without a word. `php -n` 8.5.6 on this exact script prints four lines for it — two per name,
/// `Unable to locate filter` then `Unable to create filter`, in chain order — and still returns
/// the file's bytes, so the expectation below is php's, not elephc's.
#[test]
fn test_file_get_contents_reads_a_run_time_filter_url() {
    let out = compile_and_run_capture(
        r#"<?php
file_put_contents("fgcrt.txt", "Hello World");
$res = "fgcrt" . ".txt";
echo file_get_contents("php://filter/read=string.toupper|string.rot13/resource=" . $res), "|";
echo file_get_contents("php://filter/read=string.toupper/resource=data://text/plain," . "abc"), "|";
var_dump(@file_get_contents("php://filter/read=string.toupper/resource=" . "absent.txt"));
echo file_get_contents("php://filter/read=no.such|missing.too/resource=" . $res), "|";
var_dump(file_get_contents("php://filter/read=string.toupper/resource=" . "absent.txt"));
unlink("fgcrt.txt");
"#,
    );
    assert!(out.success);
    assert_eq!(
        out.stdout,
        "URYYB JBEYQ|ABC|bool(false)\nHello World|bool(false)\n"
    );
    assert_eq!(
        out.diagnostics,
        "Warning: file_get_contents(): Unable to locate filter \"no.such\"\n\
         Warning: file_get_contents(): Unable to create filter (no.such)\n\
         Warning: file_get_contents(): Unable to locate filter \"missing.too\"\n\
         Warning: file_get_contents(): Unable to create filter (missing.too)\n\
         Warning: file_get_contents(php://filter/read=string.toupper/resource=absent.txt): \
         Failed to open stream: operation failed\n",
        "php's wording throughout: two lines per unresolvable name, then the unsuppressed failure"
    );
}

/// Verifies `file_put_contents()` writes THROUGH a `php://filter/write=...` chain.
///
/// The one-shot writer has nowhere to attach a chain, so a filter URL used to reach it as a
/// FILENAME — and before the writer checked its open result, the payload went out through a
/// garbage descriptor. The route opens the resource, attaches the parked write chain, writes
/// through it and closes; php answers the INPUT byte count, which is what the filtered write
/// helper returns. One spelling serves both forms (the URL is probed at run time), so the
/// literal and the assembled URL are asserted against the same expectations:
/// `rot13|toupper` proves order, FILE_APPEND proves the mode bit, and the unopenable resource
/// proves the failure warns in php's words — naming `file_put_contents` and the WHOLE URL —
/// and answers false.
#[test]
fn test_file_put_contents_writes_through_a_filter_chain() {
    let out = compile_and_run_capture(
        r#"<?php
var_dump(file_put_contents("php://filter/write=string.rot13|string.toupper/resource=wf1.txt", "hello"));
echo file_get_contents("wf1.txt"), "|";
unlink("wf1.txt");
file_put_contents("wf2.txt", "AB");
var_dump(file_put_contents("php://filter/write=string.rot13/resource=" . "wf2" . ".txt", "cd", FILE_APPEND));
echo file_get_contents("wf2.txt"), "|";
unlink("wf2.txt");
var_dump(file_put_contents("php://filter/write=string.rot13/resource=/no/such/wf.txt", "data"));
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "int(5)\nURYYB|int(2)\nABpq|bool(false)\n");
    assert_eq!(
        out.diagnostics,
        "Warning: file_put_contents(php://filter/write=string.rot13/resource=/no/such/wf.txt): \
         Failed to open stream: operation failed\n",
        "php's wording, the whole URL, and no inner-opener leak"
    );
}

/// Verifies `readfile()` and `file()` read through a `php://filter` chain, both spellings.
///
/// Every path-taking reader now consults the same run-time filter route: `readfile()` streams
/// the filtered bytes to the output sink and answers the byte count; `file()` splits the
/// filtered bytes through `__rt_file`'s second entry — the ordinary entry performs its own
/// read and cannot be handed bytes that were already read through a chain.
#[test]
fn test_readfile_and_file_read_through_a_filter_chain() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("rfl.txt", "Hello World\n");
$res = "rfl" . ".txt";
var_dump(readfile("php://filter/read=string.toupper/resource=rfl.txt"));
var_dump(readfile("php://filter/read=string.toupper/resource=" . $res));
var_dump(file("php://filter/read=string.toupper/resource=rfl.txt"));
var_dump(file("php://filter/read=string.rot13/resource=" . $res, FILE_IGNORE_NEW_LINES));
unlink("rfl.txt");
"#,
    );
    assert_eq!(
        out,
        "HELLO WORLD\nint(12)\nHELLO WORLD\nint(12)\narray(1) {\n  [0]=>\n  \
         string(12) \"HELLO WORLD\n\"\n}\narray(1) {\n  [0]=>\n  string(11) \"Uryyb Jbeyq\"\n}\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a NESTED literal filter URL recurses, as php does.
///
/// The inner level sits closest to the bytes, so its chain applies FIRST and the outer chain
/// sees what the inner produced: toupper-then-rot13 for the double, and the triple proves the
/// order is depth-driven rather than a two-level accident. The ASSEMBLED spelling still pins
/// the loud refusal in `test_run_time_filter_url_edge_cases` — the run-time parse does not
/// recurse yet, and that divergence stays recorded there.
#[test]
fn test_a_nested_literal_filter_url_recurses_like_php() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("nf.txt", "Hello World");
echo file_get_contents("php://filter/read=string.rot13/resource=php://filter/read=string.toupper/resource=nf.txt"), "|";
$h = fopen("php://filter/read=string.rot13/resource=php://filter/read=string.toupper/resource=nf.txt", "r");
echo stream_get_contents($h), "|";
fclose($h);
echo file_get_contents("php://filter/read=string.tolower/resource=php://filter/read=string.rot13/resource=php://filter/read=string.toupper/resource=nf.txt");
unlink("nf.txt");
"#,
    );
    assert_eq!(out, "URYYB JBEYQ|URYYB JBEYQ|uryyb jbeyq");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a run-time filter chain whose names are ALL unrecognised opens the resource plain.
///
/// The direction is published from the resolved count, so this is the case that distinguishes
/// "no filter matched" from "the URL named no filters at all" — both must open unfiltered rather
/// than fail, which is what `php -n` does.
#[test]
fn test_run_time_filter_chain_of_unknown_names_opens_unfiltered() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("rtunk.txt", "Hello");
$res = "rtunk" . ".txt";
$a = @fopen("php://filter/read=no.such|also.missing/resource=" . $res, "r");
echo var_export($a !== false, true), ":", stream_get_contents($a);
fclose($a);
unlink("rtunk.txt");
"#,
    );
    assert_eq!(out, "true:Hello");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a `data://` URI built at RUN TIME decodes and opens.
///
/// A literal URI is decoded during lowering and its bytes embedded, which left a run-time one
/// with no path at all. Decoding needed nothing new in the runtime: `__rt_base64_decode` and
/// `__rt_urldecode` already exist, and the latter's `+`-as-space rule is what the compile-time
/// decoder applies to these URIs too.
#[test]
fn test_fopen_honours_a_data_url_built_at_run_time() {
    let out = compile_and_run(
        r#"<?php
function probe(string $label, string $uri): void {
    $h = @fopen($uri, "r");
    echo $label, "=", var_export($h !== false, true);
    if ($h !== false) { echo ":", stream_get_contents($h); fclose($h); }
    echo " ";
}
$d = "data://";
probe("plain", $d . "text/plain,hi");
probe("pct", $d . "text/plain,a%20b%21");
probe("b64", $d . "text/plain;base64,aGVsbG8=");
probe("empty", $d . "text/plain,");
probe("nocomma", $d . "text/plain");
"#,
    );
    assert_eq!(out, "plain=true:hi pct=true:a b! b64=true:hello empty=true: nocomma=false ");
}

/// Verifies PHP's optional `fgets($handle, $length)`, which bounds the line.
///
/// php 8.5.6 reads at most `$length - 1` bytes, leaves the remainder for the next read, answers
/// `false` when the bound leaves room for nothing, and rejects a non-positive bound with a
/// `ValueError`. The builtin used to take a single parameter, so `fgets($conn, 1024)` — the
/// ordinary way to read a request line — did not compile at all.
#[test]
fn test_fgets_accepts_phps_length_bound() {
    let out = compile_and_run(
        r#"<?php
$h = fopen("php://memory", "r+");
fwrite($h, "abcdefghij\nsecond\n");
rewind($h);
echo var_export(fgets($h, 5), true), "|", var_export(fgets($h), true), "|";
rewind($h);
echo var_export(fgets($h, 2), true), "|", var_export(fgets($h, 1), true), "|";
rewind($h);
echo var_export(fgets($h, 100), true);
fclose($h);
"#,
    );
    assert_eq!(out, "'abcd'|'efghij\n'|'a'|false|'abcdefghij\n'");
}

/// Verifies a non-positive `$length` raises php-src's `ValueError` rather than reading unbounded.
///
/// Zero is what an omitted argument means to the runtime helper, so a caller-supplied zero has to
/// be rejected before it reaches it — otherwise `fgets($h, 0)` would quietly read a whole line.
#[test]
fn test_fgets_rejects_a_non_positive_length() {
    let out = compile_and_run(
        r#"<?php
$h = fopen("php://memory", "r+");
fwrite($h, "abcdefghij\n");
rewind($h);
foreach ([0, -1] as $len) {
    try {
        fgets($h, $len);
        echo "no-throw|";
    } catch (ValueError $e) {
        echo $e->getMessage(), "|";
    }
}
fclose($h);
"#,
    );
    assert_eq!(
        out,
        "fgets(): Argument #2 ($length) must be greater than 0|\
         fgets(): Argument #2 ($length) must be greater than 0|"
    );
}

/// Verifies a string that arrives as a boxed `Mixed` can still be indexed.
///
/// `fgets()` and `fread()` report `string|false`, which is carried as a boxed Mixed, and the
/// boxed reader knew arrays, hashes, stdClass and null — but not strings, so `$s[0]` fell
/// through to NULL. Nothing announced it: `ord(null)` is 0 and `null` prints as nothing, so
/// `$s = fgets($h); echo $s[0];` simply produced empty output.
///
/// The out-of-range rows matter as much as the in-range ones: php answers `""` there, not
/// null, and it counts a negative offset back from the end.
#[test]
fn test_indexing_a_boxed_mixed_string_reads_the_byte() {
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
fwrite($m, "Hello");
rewind($m);
$s = fgets($m);              // string|false, so the value is boxed
foreach ([0, 4, -1, -5] as $i) {
    echo var_export($s[$i], true), ",";
}
foreach ([5, -6] as $i) {    // out of range in both directions
    echo var_export(@$s[$i], true), ",";
}
var_dump($s[0] === "H");
fclose($m);
"#,
    );
    assert_eq!(out, "'H','o','o','H','','',bool(true)\n");
}

/// Verifies a stream opened read-only refuses a write, and that every writable mode still
/// writes.
///
/// `php://memory` and `php://temp` are backed by a temporary FILE that elephc opens
/// read-write whatever the caller asked, so `fopen("php://memory", "r")` accepted writes and
/// the bytes were really there to read back. A file opened `'r'` was already refused, but by
/// the OS rather than by elephc.
///
/// The mode elephc records on the stream is the authority — the same string
/// `stream_get_meta_data()['mode']` reports — so the two cannot disagree about what a stream
/// allows. The second half of the test is the one that matters: a guard that refuses too much
/// would break every ordinary write, and `a`/`c`/`x` do not start with `r` while `r+` does.
#[test]
fn test_a_read_only_stream_refuses_writes() {
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r");
echo var_export(@fwrite($m, "X"), true), ",";
rewind($m);
echo var_export(fread($m, 10), true), "|";
fclose($m);

$t = fopen("php://temp", "r");
echo var_export(@fwrite($t, "X"), true), "|";
fclose($t);

$p = tempnam(sys_get_temp_dir(), "wr");
foreach (["w", "a", "r+", "w+", "a+", "c"] as $mode) {
    @unlink($p);
    file_put_contents($p, "seed");
    $h = fopen($p, $mode);
    echo var_export(fwrite($h, "Z"), true), ",";
    fclose($h);
}
$mm = fopen("php://memory", "w+");
echo var_export(fwrite($mm, "ok"), true);
fclose($mm);
@unlink($p);
"#,
    );
    assert_eq!(out, "false,''|false|1,1,1,1,1,1,2");
}

/// Verifies `data://` reports itself as neither local nor lockable, and that the wrappers
/// around it keep their own answers.
///
/// `data://` carries its payload inside the URI, and php answers false to both questions for
/// it. elephc answered true to both: the URL-identity test covered the remote wrappers
/// (HTTP/HTTPS/FTP/FTPS) and `data://` is not one of them, while the lock test only knew the
/// `php://` family.
///
/// The other four rows are the point of the test as much as the `data://` one — `php://temp`
/// is local but not lockable, `php://stdout` is both, and a plain file is both, so this
/// cannot pass by answering false more often.
#[test]
fn test_data_wrapper_is_neither_local_nor_lockable() {
    let out = compile_and_run(
        r#"<?php
$p = tempnam(sys_get_temp_dir(), "wl");
file_put_contents($p, "x");
$file = fopen($p, "r");
$mem  = fopen("php://memory", "r+");
$tmp  = fopen("php://temp", "r+");
$out  = fopen("php://stdout", "w");
$data = fopen("data://text/plain,abc", "r");
foreach (["file" => $file, "mem" => $mem, "tmp" => $tmp, "out" => $out, "data" => $data] as $k => $h) {
    echo $k, ":", stream_supports_lock($h) ? "L" : "-", stream_is_local($h) ? "l" : "-", " ";
}
fclose($file); fclose($mem); fclose($tmp); fclose($out); fclose($data);
unlink($p);
"#,
    );
    assert_eq!(out, "file:Ll mem:-l tmp:-l out:Ll data:-- ");
}

/// Verifies `stream_select()` accepts `null` for the sets a caller does not watch.
///
/// This is the call shape php.net documents — `stream_select($read, $write, $except, 0)` with
/// the unused sets passed as null — and it killed the process with SIGSEGV. Passing empty
/// arrays worked, which is why no existing test caught it.
///
/// A null set is not a null POINTER: elephc's tagged null is an in-band sentinel, so the
/// guards have to go through `emit_branch_if_null_container` rather than test for zero. Three
/// places dereferenced it per set — the length read, the header read, and the compacted
/// length written BACK after the loop, which a guard branching to the loop's own exit label
/// still ran into.
#[test]
fn test_stream_select_accepts_null_for_the_unwatched_sets() {
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:0");
$addr = stream_socket_get_name($srv, false);
$cli = stream_socket_client("tcp://" . $addr);
$conn = stream_socket_accept($srv, 5);

$r = [$conn];
$w = null;
$x = null;
echo "ready=", var_export(stream_select($r, $w, $x, 0, 1000), true), "|";

fwrite($cli, "hi");
$r2 = [$conn];
$w2 = null;
$x2 = null;
echo "after write=", var_export(stream_select($r2, $w2, $x2, 1, 0), true), "|";
echo "kept=", count($r2);

fclose($conn);
fclose($cli);
fclose($srv);
"#,
    );
    assert_eq!(out, "ready=0|after write=1|kept=1");
}

/// Verifies an out-of-range offset on a boxed string warns, and that the silent readers stay
/// silent AND still see the offset as absent.
///
/// These two halves have to be pinned together. php answers `""` for an ordinary read of a
/// missing offset but reports it as ABSENT to `isset()` and `??` — so returning `""` on every
/// path makes `isset($s[9])` true and `$s[9] ?? "d"` answer `""`, which is how the first
/// version of this fix was wrong. The warning flag the reader already receives is what
/// separates the two callers.
///
/// The offset is named as the caller WROTE it: `$s[-9]` reports `-9`, not the resolved index.
#[test]
fn test_out_of_range_offset_on_a_boxed_string_warns_and_reads_as_absent() {
    let out = compile_and_run_capture(
        r#"<?php
$m = fopen("php://memory", "r+");
fwrite($m, "Hello");
rewind($m);
$s = fgets($m);
echo "[", $s[9], "]";
echo "[", $s[-9], "]";
echo "at:", @$s[9], ":";
echo "isset:", isset($s[9]) ? "y" : "n", ":";
echo "coalesce:", $s[9] ?? "dflt";
fclose($m);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "[][]at::isset:n:coalesce:dflt");
    assert!(
        out.diagnostics
            .contains("Warning: Uninitialized string offset 9"),
        "expected the offset warning, got diagnostics={}",
        out.diagnostics
    );
    assert!(
        out.diagnostics
            .contains("Warning: Uninitialized string offset -9"),
        "expected the negative offset reported as written, got diagnostics={}",
        out.diagnostics
    );
    // Exactly two: `@`, isset() and `??` must not add a third.
    assert_eq!(
        out.diagnostics.matches("Uninitialized string offset").count(),
        2,
        "silent readers must not warn, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies a FAILED `fread()` answers `false` while an empty one still answers `""`.
///
/// Reading a handle opened `'w'` fails at the OS, and php-src reports that as `false`;
/// elephc answered `""`, so `fread(...) !== false` read the failure as an empty read. The
/// hard part is that an exhausted stream answers `""` too and both carry zero bytes, so
/// the cases have to be separated by more than emptiness.
///
/// The `php://memory` line is the other half of the rule and is what stops this being
/// "anything empty is false": a memory stream has no OS read to fail, so php answers `""`
/// there even though the handle is write-only.
#[test]
fn test_fread_returns_false_only_when_the_read_actually_fails() {
    let out = compile_and_run(
        r#"<?php
// Two files on purpose: opening the first "w" TRUNCATES it, so reusing it would leave the
// short-read case reading an empty file and quietly stop testing anything.
$a = tempnam(sys_get_temp_dir(), "fra");
$b = tempnam(sys_get_temp_dir(), "frb");
file_put_contents($b, "hello");

$w = fopen($a, "w");
echo var_export(@fread($w, 5), true), "|";   // the read fails at the OS
fclose($w);

$r = fopen($b, "r");
echo var_export(fread($r, 100), true), "|";  // a short read is not a failure
echo var_export(fread($r, 5), true), "|";    // exhausted: "" and not false
fclose($r);

$m = fopen("php://memory", "w");
echo var_export(@fread($m, 5), true);        // no OS read to fail
fclose($m);
unlink($a);
unlink($b);
"#,
    );
    assert_eq!(out, "false|'hello'|''|''");
}

/// Verifies `fread()` rejects a non-positive length the way php-src does.
///
/// elephc answered `""` for both, which is what a legitimate empty read looks like, so a
/// caller could not tell a rejected argument from an exhausted stream. php-src refuses
/// before it reads anything.
#[test]
fn test_fread_rejects_a_non_positive_length() {
    let out = compile_and_run(
        r#"<?php
$h = fopen("php://memory", "r+");
fwrite($h, "abcdefghij");
rewind($h);
foreach ([0, -1] as $len) {
    try {
        fread($h, $len);
        echo "no-throw|";
    } catch (ValueError $e) {
        echo $e->getMessage(), "|";
    }
}
echo fread($h, 3);
fclose($h);
"#,
    );
    assert_eq!(
        out,
        "fread(): Argument #2 ($length) must be greater than 0|\
         fread(): Argument #2 ($length) must be greater than 0|abc"
    );
}

/// Verifies `data://` refuses a media type php-src does not accept, and reads `;base64` the way
/// php-src reads it.
///
/// elephc used to accept ANY media type and look for a `;base64` suffix case-insensitively, so it
/// opened URIs php-src refuses and base64-decoded a `;BASE64` php-src would not. Measuring the
/// real rule was the point of this fixture, and it is narrower than "charset is special":
///
/// - the type is empty, or it must carry a `/` — `text` alone is refused;
/// - every parameter must be `name=value`, whatever the name — `;bogus=1` is ACCEPTED, `;bogus`
///   and a trailing empty `;` are not;
/// - `base64` counts only as the LAST parameter and only in lower case, so
///   `;charset=utf-8;base64` decodes while `;base64;charset=utf-8` is refused outright.
///
/// The rule lives twice — in `data_uri_media_type_shape` for a literal URI resolved at compile
/// time, and in `__rt_data_uri_meta_ok` for one built at run time. Neither can serve both, so both
/// forms are exercised here and a divergence fails this test.
#[test]
fn test_data_url_rejects_a_media_type_php_refuses() {
    let out = compile_and_run(
        r#"<?php
function probe(string $label, string $uri): void {
    $h = @fopen($uri, "r");
    echo $label, "=", var_export($h !== false, true);
    if ($h !== false) { echo ":", stream_get_contents($h); fclose($h); }
    echo " ";
}
// Run-time URIs go through the runtime validator.
$d = "data://";
probe("noslash", $d . "text,aGVsbG8=");
probe("emptyparam", $d . "text/plain;,aGVsbG8=");
probe("b64notlast", $d . "text/plain;base64;charset=utf-8,aGVsbG8=");
probe("upper", $d . "text/plain;BASE64,aGVsbG8=");
probe("namedparam", $d . "text/plain;bogus=1,aGVsbG8=");
probe("b64last", $d . "text/plain;charset=utf-8;base64,aGVsbG8=");
echo "|";
// The same shapes as literals, which the compile-time decoder resolves instead.
probe("lit-noslash", "data://text,aGVsbG8=");
probe("lit-b64notlast", "data://text/plain;base64;charset=utf-8,aGVsbG8=");
probe("lit-namedparam", "data://text/plain;bogus=1,aGVsbG8=");
probe("lit-b64last", "data://text/plain;charset=utf-8;base64,aGVsbG8=");
"#,
    );
    assert_eq!(
        out,
        "noslash=false emptyparam=false b64notlast=false upper=false \
         namedparam=true:aGVsbG8= b64last=true:hello \
         |lit-noslash=false lit-b64notlast=false \
         lit-namedparam=true:aGVsbG8= lit-b64last=true:hello "
    );
}

/// Pins that `fread($f, $n)` never hands back more than `$n` bytes through a filter.
///
/// IGNORED because elephc has no filtered-read buffer: a read filter that expands its input
/// has its whole output returned in one go, so `fread($f, 2)` over a filter tripling `"ab"`
/// answers the 6-byte `"ababab"` where php 8.5.6 answers `ab`, `ab`, `ab` — it caps the
/// result at `$n` and keeps the remainder on the stream for the next read.
///
/// This is INDEPENDENT of [`test_user_filter_psfs_feed_me_buffers_across_dispatches`]: the
/// filter here answers `PSFS_PASS_ON` on every dispatch, so no FEED_ME handling is involved.
/// It is also that fixture's prerequisite — without somewhere to park the remainder, a
/// FEED_ME fix cannot hand back the right chunk sizes either.
///
/// Returning more bytes than requested is a contract break in its own right: a caller that
/// sized a buffer from `$n` gets more than it asked for.
#[test]
fn test_fread_caps_a_filtered_read_at_the_requested_length() {
    let out = compile_and_run(
        r#"<?php
class ExpandThrice extends php_user_filter {
    public function filter($in, $out, &$consumed, $closing): int {
        while ($b = stream_bucket_make_writeable($in)) {
            $consumed += $b->datalen;
            $ob = stream_bucket_new($this->stream, str_repeat($b->data, 3));
            stream_bucket_append($out, $ob);
        }
        return PSFS_PASS_ON;
    }
}
stream_filter_register("expand.thrice", "ExpandThrice");
$f = fopen("php://memory", "r+");
fwrite($f, "ab");
rewind($f);
stream_filter_append($f, "expand.thrice", STREAM_FILTER_READ);
$parts = [];
while (!feof($f)) {
    $c = fread($f, 2);
    if ($c === "" || $c === false) { break; }
    $parts[] = $c;
}
echo implode("|", $parts);
"#,
    );
    assert_eq!(out, "ab|ab|ab");
}

/// Pins PHP's `PSFS_FEED_ME` contract for a filter that buffers across dispatches.
///
/// IGNORED because elephc does not implement it yet, and the current behaviour is a
/// SILENT one: `PSFS_FEED_ME` passes the RAW input through, so this filter leaks
/// unfiltered bytes to the caller — `<abc><ABCDEF><ghi>` where php 8.5.6 answers
/// `<ABC><DEF><GHI>`. A filter that returns PSFS_PASS_ON on every dispatch is
/// unaffected, which is why the rest of the filter suite stays green.
///
/// Fixing it takes THREE changes that must land together:
///   1. `PSFS_FEED_ME` must return nothing rather than the original input;
///   2. `__rt_fread` must then fetch more input and dispatch again instead of
///      reporting a short read — with (1) alone, `fread()` returns "" and every
///      caller written as `if ($chunk === "") break;` stops early, turning a data
///      LEAK into data LOSS;
///   3. the StreamState needs a filtered-read buffer plus a closing flush at EOF.
///      Measured against php 8.5.6: a filter that triples `"ab"` answers three
///      `fread($f, 2)` calls with `ab|ab|ab`, so PHP caps the filtered result at
///      `$length` and keeps the remainder; and a filter still holding bytes when the
///      stream ends gets a `$closing` dispatch whose output reaches the reader. With
///      only (1)+(2) this fixture prints `<ABCDEF>` — the leak becomes a loss.
#[test]
fn test_user_filter_psfs_feed_me_buffers_across_dispatches() {
    let out = compile_and_run(
        r#"<?php
class FeedMeCollect extends php_user_filter {
    private string $buf = "";
    public function filter($in, $out, &$consumed, $closing): int {
        while ($b = stream_bucket_make_writeable($in)) {
            $consumed += $b->datalen;
            $this->buf .= $b->data;
        }
        if (strlen($this->buf) < 6) {
            return PSFS_FEED_ME;
        }
        $ob = stream_bucket_new($this->stream, strtoupper($this->buf));
        stream_bucket_append($out, $ob);
        $this->buf = "";
        return PSFS_PASS_ON;
    }
}
stream_filter_register("feedme.collect", "FeedMeCollect");
$f = fopen("php://memory", "r+");
fwrite($f, "abcdefghi");
rewind($f);
stream_filter_append($f, "feedme.collect", STREAM_FILTER_READ);
$out = "";
while (!feof($f)) {
    $chunk = fread($f, 3);
    if ($chunk === "" || $chunk === false) { break; }
    $out .= "<" . $chunk . ">";
}
echo $out;
"#,
    );
    assert_eq!(out, "<ABC><DEF><GHI>");
}

/// Pins the third measured property of PHP's filtered reads: end of input triggers a `$closing`
/// dispatch whose output reaches the reader.
///
/// IGNORED because nothing flushes a read filter at EOF. A filter holding every byte until
/// `$closing` therefore never emits its result — and because `PSFS_FEED_ME` currently passes its
/// input through, the reader gets the RAW `xyz` instead of the filter's `[xyz]`. Measured against
/// php 8.5.6.
///
/// Kept separate from [`test_user_filter_psfs_feed_me_buffers_across_dispatches`] so the three
/// properties can be fixed and verified one at a time: FEED_ME returning nothing, `fread()`
/// capping and parking the remainder, and this closing flush. Landing the first two without this
/// one turns the leak into silent data loss, so all three ship together.
#[test]
fn test_read_filter_is_flushed_when_the_stream_ends() {
    let out = compile_and_run(
        r#"<?php
class HoldUntilClose extends php_user_filter {
    private string $buf = "";
    public function filter($in, $out, &$consumed, $closing): int {
        while ($b = stream_bucket_make_writeable($in)) {
            $consumed += $b->datalen;
            $this->buf .= $b->data;
        }
        if (!$closing) {
            return PSFS_FEED_ME;
        }
        stream_bucket_append($out, stream_bucket_new($this->stream, "[" . $this->buf . "]"));
        return PSFS_PASS_ON;
    }
}
stream_filter_register("hold.until.close", "HoldUntilClose");
$f = fopen("php://memory", "r+");
fwrite($f, "xyz");
rewind($f);
stream_filter_append($f, "hold.until.close", STREAM_FILTER_READ);
echo stream_get_contents($f);
fclose($f);
"#,
    );
    assert_eq!(out, "[xyz]");
}

/// Regression: `ftell()` on a filtered read stream reported the READ-AHEAD position.
///
/// php advances `stream->position` by the bytes each read RETURNED TO THE CALLER, never by the
/// bytes it pulled from the descriptor. elephc's filtered `fread()` reads whole 8192-byte chunks
/// so the filter has something to work on, caps the result at what was asked for, and parks the
/// rest — and `ftell()` probed `lseek(SEEK_CUR)`, which reports where that read-ahead stopped.
/// Measured with `php -n` (8.5.6) on a 26-byte file through `string.toupper`: three reads of 3, 3
/// and 5 answer `3`, `6`, `11`; elephc answered `26`, `26`, `26` — the whole file, every time.
/// An unfiltered control and a `fgets()` read are in the same program: neither engages the
/// buffered path, and both must keep the descriptor probe.
#[test]
fn test_ftell_on_a_filtered_read_counts_the_bytes_handed_to_the_caller() {
    let base = std::env::temp_dir().join(format!("elephc_ftellf_{}", std::process::id()));
    let base = base.display().to_string();
    let out = compile_and_run(&format!(
        r#"<?php
$p = "{base}_a";
@unlink($p);
file_put_contents($p, "abcdefghijklmnopqrstuvwxyz");
$f = fopen($p, "r");
stream_filter_append($f, "string.toupper", STREAM_FILTER_READ);
echo "start:", ftell($f), "\n";
echo "r1:", fread($f, 3), ":", ftell($f), "\n";
echo "r2:", fread($f, 3), ":", ftell($f), "\n";
echo "r3:", fread($f, 5), ":", ftell($f), "\n";
while (!feof($f)) {{ fread($f, 4); }}
echo "drained:", ftell($f), "\n";
fclose($f);
$f = fopen($p, "r");
fread($f, 3);
echo "plain:", ftell($f), "\n";
fclose($f);
$q = "{base}_b";
@unlink($q);
file_put_contents($q, "one\ntwo\n");
$f = fopen($q, "r");
stream_filter_append($f, "string.toupper", STREAM_FILTER_READ);
fgets($f);
echo "fgets1:", ftell($f), "\n";
fgets($f);
echo "fgets2:", ftell($f), "\n";
fclose($f);
@unlink($p);
@unlink($q);
"#
    ));
    assert_eq!(
        out,
        "start:0\nr1:ABC:3\nr2:DEF:6\nr3:GHIJK:11\ndrained:26\nplain:3\nfgets1:4\nfgets2:8\n"
    );
}

/// Pins what the filtered position COUNTS, and where it restarts.
///
/// An expanding filter settles the first question: through `convert.base64-encode`, two
/// `fread($f, 4)` answer `4` and `8` — the FILTERED bytes handed out, not the source bytes
/// consumed to make them, so the number cannot be derived from the descriptor at all. `fseek()`
/// and `rewind()` settle the second: php's position restarts from wherever the seek landed and
/// advances from there, so `fread(3)`, `fseek(10)`, `fread(4)` answers `3`, `10`, `14`. Two
/// filtered streams open at once pin that the count lives on the stream, not in a global.
/// Measured with `php -n` (8.5.6).
#[test]
fn test_filtered_ftell_counts_filtered_bytes_and_restarts_at_a_seek() {
    let path = std::env::temp_dir().join(format!("elephc_ftellf2_{}.txt", std::process::id()));
    let path = path.display().to_string();
    let out = compile_and_run(&format!(
        r#"<?php
$p = "{path}";
@unlink($p);
file_put_contents($p, "abcdefghijklmnopqrstuvwxyz");
$f = fopen($p, "r");
stream_filter_append($f, "convert.base64-encode", STREAM_FILTER_READ);
echo "b64:", fread($f, 4), ":", ftell($f), "\n";
echo "b64:", fread($f, 4), ":", ftell($f), "\n";
fclose($f);
$f = fopen($p, "r");
stream_filter_append($f, "string.toupper", STREAM_FILTER_READ);
echo "r:", fread($f, 3), ":", ftell($f), "\n";
fseek($f, 10);
echo "seek:", ftell($f), "\n";
echo "r:", fread($f, 4), ":", ftell($f), "\n";
rewind($f);
echo "rewind:", ftell($f), "\n";
echo "r:", fread($f, 2), ":", ftell($f), "\n";
fclose($f);
$f = fopen($p, "r");
$g = fopen($p, "r");
stream_filter_append($f, "string.toupper", STREAM_FILTER_READ);
stream_filter_append($g, "string.rot13", STREAM_FILTER_READ);
fread($f, 3);
fread($g, 7);
echo "two:", ftell($f), ":", ftell($g), "\n";
fread($f, 2);
echo "two:", ftell($f), ":", ftell($g), "\n";
fclose($f);
fclose($g);
@unlink($p);
"#
    ));
    assert_eq!(
        out,
        "b64:YWJj:4\nb64:ZGVm:8\n\
         r:ABC:3\nseek:10\nr:KLMN:14\nrewind:0\nr:AB:2\n\
         two:3:7\ntwo:5:7\n"
    );
}

/// Regression: `fclose()` ran no closing flush, so a buffering WRITE filter's bytes were lost.
///
/// php gives every attached filter one last `filter($in, $out, &$consumed, $closing = true)` call
/// before the stream goes away. A filter that answered `PSFS_FEED_ME` until then has been
/// ACCUMULATING, and that dispatch is the only chance its payload has to reach the file.
/// `_user_filter_closing` was raised on the read path and by `stream_filter_remove()`, never on
/// close. Measured with `php -n` (8.5.6): the file is EMPTY before `fclose()` and holds
/// `[hello world]` after it; elephc left it empty in both places.
#[test]
fn test_write_filter_is_flushed_when_the_stream_is_closed() {
    let path = std::env::temp_dir().join(format!("elephc_wclose_{}.txt", std::process::id()));
    let path = path.display().to_string();
    let out = compile_and_run(&format!(
        r#"<?php
class HoldUntilCloseW extends php_user_filter {{
    private string $buf = "";
    public function filter($in, $out, &$consumed, $closing): int {{
        while ($b = stream_bucket_make_writeable($in)) {{
            $consumed += $b->datalen;
            $this->buf .= $b->data;
        }}
        if (!$closing) {{
            return PSFS_FEED_ME;
        }}
        stream_bucket_append($out, stream_bucket_new($this->stream, "[" . $this->buf . "]"));
        return PSFS_PASS_ON;
    }}
}}
stream_filter_register("hold.until.close.w", "HoldUntilCloseW");
$p = "{path}";
@unlink($p);
$h = fopen($p, "w");
stream_filter_append($h, "hold.until.close.w", STREAM_FILTER_WRITE);
fwrite($h, "hello ");
fwrite($h, "world");
echo "before:[", (file_exists($p) ? file_get_contents($p) : "?"), "]\n";
fclose($h);
echo "after:[", file_get_contents($p), "]\n";
@unlink($p);
"#
    ));
    assert_eq!(out, "before:[]\nafter:[[hello world]]\n");
}

/// Guard: the closing flush must not add bytes where php adds none, on any other filter shape.
///
/// An unfiltered stream, a pass-through write filter that already emitted on every dispatch, a
/// READ-only filter on a stream that is also written, a built-in write filter, a two-node chain
/// and a `STREAM_FILTER_ALL` node all keep exactly the bytes php writes. `STREAM_FILTER_ALL`
/// matters most: the node sits in BOTH chains, and flushing per chain would emit its payload
/// twice. Measured with `php -n` (8.5.6).
#[test]
fn test_close_flush_leaves_other_filter_shapes_byte_identical() {
    let base = std::env::temp_dir().join(format!("elephc_wcf_{}", std::process::id()));
    let base = base.display().to_string();
    let out = compile_and_run(&format!(
        r#"<?php
class PassThroughW extends php_user_filter {{
    public function filter($in, $out, &$consumed, $closing): int {{
        while ($b = stream_bucket_make_writeable($in)) {{
            $b->data = strtoupper($b->data);
            $consumed += $b->datalen;
            stream_bucket_append($out, $b);
        }}
        return PSFS_PASS_ON;
    }}
}}
class BufferingW extends php_user_filter {{
    private string $buf = "";
    public function filter($in, $out, &$consumed, $closing): int {{
        while ($b = stream_bucket_make_writeable($in)) {{
            $this->buf .= $b->data;
            $consumed += $b->datalen;
        }}
        if ($closing) {{
            stream_bucket_append($out, stream_bucket_new($this->stream, "<" . $this->buf . ">"));
            return PSFS_PASS_ON;
        }}
        return PSFS_FEED_ME;
    }}
}}
stream_filter_register("pt.w", "PassThroughW");
stream_filter_register("buf.w", "BufferingW");

$p = "{base}_1"; @unlink($p);
$h = fopen($p, "w"); fwrite($h, "plain"); fclose($h);
echo "nofilter:[", file_get_contents($p), "]\n"; @unlink($p);

$p = "{base}_2"; @unlink($p);
$h = fopen($p, "w");
stream_filter_append($h, "pt.w", STREAM_FILTER_WRITE);
fwrite($h, "abc"); fwrite($h, "def"); fclose($h);
echo "passthru:[", file_get_contents($p), "]\n"; @unlink($p);

$p = "{base}_3"; @unlink($p);
file_put_contents($p, "seed");
$h = fopen($p, "a");
stream_filter_append($h, "buf.w", STREAM_FILTER_READ);
fwrite($h, "+tail"); fclose($h);
echo "readonly:[", file_get_contents($p), "]\n"; @unlink($p);

$p = "{base}_4"; @unlink($p);
$h = fopen($p, "w");
stream_filter_append($h, "string.toupper", STREAM_FILTER_WRITE);
fwrite($h, "mixed Case"); fclose($h);
echo "builtin:[", file_get_contents($p), "]\n"; @unlink($p);

$p = "{base}_5"; @unlink($p);
$h = fopen($p, "w");
stream_filter_append($h, "buf.w", STREAM_FILTER_WRITE);
stream_filter_append($h, "pt.w", STREAM_FILTER_WRITE);
fwrite($h, "one"); fclose($h);
echo "chained:[", file_get_contents($p), "]\n"; @unlink($p);

$p = "{base}_6"; @unlink($p);
$h = fopen($p, "w+");
stream_filter_append($h, "buf.w", STREAM_FILTER_ALL);
fwrite($h, "both"); fclose($h);
echo "all:[", file_get_contents($p), "]\n"; @unlink($p);
"#
    ));
    assert_eq!(
        out,
        "nofilter:[plain]\n\
         passthru:[ABCDEF]\n\
         readonly:[seed+tail]\n\
         builtin:[MIXED CASE]\n\
         chained:[<ONE>]\n\
         all:[<both>]\n"
    );
}

/// Regression: `stream_filter_remove()` ran the closing flush but THREW ITS BYTES AWAY.
///
/// `__rt_filter_node_closing_flush` observed only the PSFS code the filter answered with and
/// dropped the pair `__rt_user_filter_brigade_invoke` returned, so a filter that accumulated until
/// `$closing` lost its whole payload at removal — and `fclose()` afterwards could not recover it,
/// because the node was already off the chain. php's `php_stream_filter_remove(…, call_dtor)`
/// hands the flushed buckets to the stream. Measured with `php -n` (8.5.6): `<xy>`, written once.
#[test]
fn test_stream_filter_remove_writes_the_bytes_its_flush_produced() {
    let path = std::env::temp_dir().join(format!("elephc_wrm_{}.txt", std::process::id()));
    let path = path.display().to_string();
    let out = compile_and_run(&format!(
        r#"<?php
class BufferingRm extends php_user_filter {{
    private string $buf = "";
    public function filter($in, $out, &$consumed, $closing): int {{
        while ($b = stream_bucket_make_writeable($in)) {{
            $this->buf .= $b->data;
            $consumed += $b->datalen;
        }}
        if ($closing) {{
            stream_bucket_append($out, stream_bucket_new($this->stream, "<" . $this->buf . ">"));
            return PSFS_PASS_ON;
        }}
        return PSFS_FEED_ME;
    }}
}}
stream_filter_register("buf.rm", "BufferingRm");
$p = "{path}";
@unlink($p);
$h = fopen($p, "w");
$f = stream_filter_append($h, "buf.rm", STREAM_FILTER_WRITE);
fwrite($h, "xy");
var_dump(stream_filter_remove($f));
fclose($h);
echo "after:[", file_get_contents($p), "]\n";
@unlink($p);
"#
    ));
    assert_eq!(out, "bool(true)\nafter:[<xy>]\n");
}

/// Verifies `php_user_filter` declares the properties PHP declares.
///
/// Only `$params` existed, so the manual's own filter idiom — building an output bucket
/// with `stream_bucket_new($this->stream, ...)` — did not compile at all.
#[test]
fn test_user_filter_base_class_declares_filtername_and_stream() {
    let out = compile_and_run(
        r#"<?php
class PropProbeFilter extends php_user_filter {
    public function filter($in, $out, &$consumed, $closing): int {
        while ($b = stream_bucket_make_writeable($in)) {
            $consumed += $b->datalen;
            $ob = stream_bucket_new($this->stream, strtoupper($b->data));
            stream_bucket_append($out, $ob);
        }
        return PSFS_PASS_ON;
    }
}
stream_filter_register("prop.probe", "PropProbeFilter");
$f = fopen("php://memory", "r+");
fwrite($f, "hello");
rewind($f);
stream_filter_append($f, "prop.probe", STREAM_FILTER_READ);
echo stream_get_contents($f);
echo "|", var_export(property_exists("PropProbeFilter", "filtername"), true);
"#,
    );
    assert_eq!(out, "HELLO|true");
}

/// Verifies compiled PHP output for user stream filter write transforms payload.
#[test]
fn test_user_stream_filter_write_transforms_payload() {
    // Phase 10 tier 3: a user-registered filter class attached in write
    // direction transforms fwrite payloads. The filter's filter() method
    // receives the raw bytes and returns the bytes that actually hit the
    // underlying stream — so reading them back yields the transformed
    // payload.
    let out = compile_and_run(
        r#"<?php
class UpperFilter {
    public function filter(string $data): string {
        return strtoupper($data);
    }
}
stream_filter_register("user.upper", "UpperFilter");
$f = fopen("php://memory", "r+");
stream_filter_append($f, "user.upper", STREAM_FILTER_WRITE);
fwrite($f, "hello world");
rewind($f);
echo fread($f, 64);
"#,
    );
    assert_eq!(out, "HELLO WORLD");
}

/// Verifies compiled PHP output for user stream filter registered class is case insensitive.
#[test]
fn test_user_stream_filter_registered_class_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
class CaseFilter {
    public function filter(string $data): string {
        return strtoupper($data);
    }
}
stream_filter_register("case.upper", "casefilter");
$f = fopen("php://memory", "r+");
stream_filter_append($f, "case.upper", STREAM_FILTER_WRITE);
fwrite($f, "hello");
rewind($f);
echo fread($f, 64);
"#,
    );
    assert_eq!(out, "HELLO");
}

/// Verifies compiled PHP output for user stream filter read transforms payload.
#[test]
fn test_user_stream_filter_read_transforms_payload() {
    // Phase 10 tier 3: a user-registered filter class attached in read
    // direction transforms bytes returned by fread. The raw on-stream
    // bytes are unchanged; only the read path sees the filtered result.
    let out = compile_and_run(
        r#"<?php
class LowerFilter {
    public function filter(string $data): string {
        return strtolower($data);
    }
}
stream_filter_register("user.lower", "LowerFilter");
$f = fopen("php://memory", "r+");
fwrite($f, "HELLO WORLD");
rewind($f);
stream_filter_append($f, "user.lower", STREAM_FILTER_READ);
echo fread($f, 64);
"#,
    );
    assert_eq!(out, "hello world");
}

/// Verifies compiled PHP output for user stream filter params exposed on `$this`.
#[test]
fn test_user_stream_filter_params_are_exposed_on_this() {
    let out = compile_and_run(
        r#"<?php
class ParamFilter extends php_user_filter {
    public function onCreate(): bool {
        echo $this->params["prefix"];
        return true;
    }

    public function filter(string $data): string {
        return $data . $this->params["suffix"];
    }
}
stream_filter_register("user.params", "ParamFilter");
$f = fopen("php://memory", "r+");
stream_filter_append($f, "user.params", STREAM_FILTER_WRITE, ["prefix" => "<", "suffix" => ">"]);
fwrite($f, "hello");
rewind($f);
echo "|" . fread($f, 64);
"#,
    );
    assert_eq!(out, "<|hello>");
}

/// Verifies compiled PHP output for user stream filter unknown name returns false.
#[test]
fn test_user_stream_filter_unknown_name_returns_false() {
    // stream_filter_append on an unknown user-filter name resolves the
    // ID to 0 through the registry scan; the helper short-circuits and
    // the builtin emitter boxes PHP false. No state mutation happens.
    let out = compile_and_run(
        r#"<?php
$f = fopen("php://memory", "r+");
$r = stream_filter_append($f, "this.does.not.exist");
echo $r === false ? "false" : "open";
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for stream filter user onclose fires on remove.
#[test]
fn test_stream_filter_user_onclose_fires_on_remove() {
    // Phase 11 B4 (partial): stream_filter_remove() now shares the same
    // onClose-then-clear teardown as fclose(). Removing a filter that
    // declared onClose fires the hook before subsequent fwrites bypass
    // the (now-detached) filter.
    let out = compile_and_run(
        r#"<?php
class TraceFilter {
    public function filter(string $data): string {
        return strtoupper($data);
    }
    public function onClose(): void {
        echo "|closed";
    }
}
stream_filter_register("trace.upper", "TraceFilter");
$m = fopen("php://memory", "r+");
$f = stream_filter_append($m, "trace.upper", STREAM_FILTER_WRITE);
fwrite($m, "a");
stream_filter_remove($f);
fwrite($m, "b");
rewind($m);
echo stream_get_contents($m);
fclose($m);
"#,
    );
    // Filtered "a" → "A", then onClose fires before the second write
    // bypasses the filter, so the final memory holds "Ab" and the
    // closed-marker lands between them in the output.
    assert_eq!(out, "|closedAb");
}

/// Verifies compiled PHP output for stream bucket new returns object with data and datalen.
#[test]
fn test_stream_bucket_new_returns_object_with_data_and_datalen() {
    // Phase 11 B4 (API-surface delivery): stream_bucket_new($stream, $data)
    // returns a real PHP object (stdClass-backed) with public `data` and
    // `datalen` properties, matching PHP's documented bucket shape. The
    // bucket is decoupled from the filter dispatch — it's a stand-alone
    // primitive that filter() implementations using the PHP-standard
    // 4-arg signature can call (the dispatch refactor itself is the
    // separate increment).
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
$b = stream_bucket_new($m, "hello world");
echo gettype($b) . "|" . $b->data . "|" . $b->datalen;
fclose($m);
"#,
    );
    assert_eq!(out, "object|hello world|11");
}

/// Verifies compiled PHP output for stream bucket make writeable returns null for empty brigade.
#[test]
fn test_stream_bucket_make_writeable_returns_null_for_empty_brigade() {
    // Phase 11 B4: stream_bucket_make_writeable on an empty brigade
    // returns null per PHP's documented behaviour. v1 always returns
    // null since the filter dispatch hasn't been wired to seed brigade
    // state yet.
    let out = compile_and_run(
        r#"<?php
$brigade = new stdClass();
$b = stream_bucket_make_writeable($brigade);
echo is_null($b) ? "null" : "non-null";
"#,
    );
    assert_eq!(out, "null");
}

/// Verifies compiled PHP output for stream filter user oncreate refusal blocks attach.
#[test]
fn test_stream_filter_user_oncreate_refusal_blocks_attach() {
    // Phase 11 B4 (partial): if a user-filter class's onCreate() returns
    // false, the filter is refused and stream_filter_append returns false.
    // No filter is recorded against the fd, so subsequent fwrites pass
    // through unchanged.
    let out = compile_and_run(
        r#"<?php
class RefuseFilter {
    public function onCreate(): bool {
        return false;
    }
    public function filter(string $data): string {
        return "should not run";
    }
}
stream_filter_register("trace.refuse", "RefuseFilter");
$m = fopen("php://memory", "r+");
$r = stream_filter_append($m, "trace.refuse", STREAM_FILTER_WRITE);
echo "attach=" . ($r === false ? "false" : "ok") . "|";
fwrite($m, "hi");
rewind($m);
echo stream_get_contents($m);
fclose($m);
"#,
    );
    assert_eq!(out, "attach=false|hi");
}

/// Verifies compiled PHP output for stream filter user oncreate and onclose fire.
#[test]
fn test_stream_filter_user_oncreate_and_onclose_fire() {
    // Phase 11 B4 (partial): onCreate() runs at attach time (so its
    // side effect of pre-loading state is visible to the first filter()
    // call), and onClose() runs at fclose() time (so cleanup like a
    // final flush can happen). When the method is absent in the class,
    // the attach / close still works — only the implemented hooks
    // fire.
    let out = compile_and_run(
        r#"<?php
class CountingFilter {
    public string $prefix = "";
    public function onCreate(): bool {
        $this->prefix = ">>";
        return true;
    }
    public function filter(string $data): string {
        return $this->prefix . $data;
    }
    public function onClose(): void {
        echo "|closed";
    }
}
stream_filter_register("count.upper", "CountingFilter");
$m = fopen("php://memory", "r+");
stream_filter_append($m, "count.upper", STREAM_FILTER_WRITE);
fwrite($m, "x");
rewind($m);
echo stream_get_contents($m);
fclose($m);
"#,
    );
    assert_eq!(out, ">>x|closed");
}

/// Verifies compiled PHP output for stream filter register accepts registration.
#[test]
fn test_stream_filter_register_accepts_registration() {
    // v1 stub: stream_filter_register() accepts the registration and reports
    // true. The user-defined filter class is not yet invoked on read/write.
    let out = compile_and_run(
        r#"<?php
class CustomFilter {}
echo stream_filter_register("custom.filter", "CustomFilter") ? "true" : "false";
"#,
    );
    assert_eq!(out, "true");
}

/// Verifies a filter registration keeps its own copy of the name — the twin of the wrapper case.
///
/// `_user_filter_registry` stored the caller's pointer, so reassigning the variable rewrote the
/// registered name. Measured before the fix as both registrations resolving to the variable's
/// LAST value: `aa` and `bb` were unusable and `zz`, never registered, filtered.
#[test]
fn test_filter_registration_owns_its_name_after_the_caller_reassigns_it() {
    let out = compile_and_run(
        r#"<?php
class Up extends php_user_filter {
    public function filter($in, $out, &$consumed, bool $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) {
            $bucket->data = strtoupper($bucket->data);
            $consumed += $bucket->datalen;
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }
}
$n = "aa";
stream_filter_register($n, "Up");
$n = "bb";
stream_filter_register($n, "Up");
$n = "zz";
foreach (["aa", "bb", "zz"] as $name) {
    $f = fopen("php://memory", "w+");
    stream_filter_append($f, $name, STREAM_FILTER_WRITE);
    fwrite($f, "x");
    rewind($f);
    echo fread($f, 4);
    fclose($f);
}
"#,
    );
    assert_eq!(out, "XXx");
}

/// Verifies a wrapper whose scalar methods carry NO return type behaves like one that does.
///
/// A method with no declared return type has codegen representation `Mixed`, so it hands back
/// a boxed cell where the helper reads a raw integer or boolean — and leaving the return type
/// off is how ordinary wrapper code is written, so the broken shape was the common one.
/// Measured before the fix: `ftell()` answered 4329450168, a pointer, where PHP answers 5.
///
/// The expectation is the output of the SAME wrapper with every return type declared, which is
/// the property that matters: the declaration must not change the answer. `ftell()` reporting
/// the wrapper's own position rather than PHP's write-advanced one is a separate, pre-existing
/// divergence — it shows identically in both forms, which is how it was told apart from this.
#[test]
fn test_undeclared_scalar_returns_behave_like_declared_ones() {
    let source = r#"<?php
class S {
    public $context;
    private $buf = "abcdefghij";
    private $pos = 0;
    public function stream_open($path, $mode, $options, &$opened): bool { $this->pos = 0; return true; }
    public function stream_read($count): string { $c = substr($this->buf, $this->pos, $count); $this->pos += strlen($c); return $c; }
    public function stream_write(string $data)RET_INT { $this->buf .= $data; return strlen($data); }
    public function stream_eof()RET_BOOL { return $this->pos >= strlen($this->buf); }
    public function stream_tell()RET_INT { return $this->pos; }
    public function stream_seek($offset, $whence)RET_BOOL { $this->pos = $offset; return true; }
    public function stream_flush()RET_BOOL { return true; }
    public function stream_lock($op)RET_BOOL { return true; }
    public function stream_truncate($size)RET_BOOL { $this->buf = substr($this->buf, 0, $size); return true; }
    public function stream_close() {}
}
stream_wrapper_register("slots", "S");
$f = fopen("slots://x", "r+");
echo "w", fwrite($f, "XY");
echo "s", fseek($f, 3);
echo "t", ftell($f);
echo "r", fread($f, 4);
echo "f", fflush($f) ? 1 : 0;
echo "l", flock($f, LOCK_EX) ? 1 : 0;
echo "u", ftruncate($f, 4) ? 1 : 0;
echo "e", feof($f) ? 1 : 0;
fclose($f);
"#;
    let declared = compile_and_run(&source.replace("RET_INT", ": int").replace("RET_BOOL", ": bool"));
    let undeclared = compile_and_run(&source.replace("RET_INT", "").replace("RET_BOOL", ""));
    assert_eq!(
        undeclared, declared,
        "omitting the return type must not change what the wrapper reports"
    );
    assert_eq!(declared, "w2s0t3rdefgf1l1u1e1");
}

/// Verifies compiled PHP output for fopen silent fail for registered user wrapper.
#[test]
fn test_fopen_silent_fail_for_registered_user_wrapper() {
    // Phase 10 dispatch v1: __rt_fopen recognises paths whose scheme matches
    // a registered user wrapper. When the wrapper class does not implement
    // `stream_open`, the runtime fails silently (no "Failed to open stream"
    // warning) instead of attempting to open the literal path.
    let out = compile_and_run_capture(
        r#"<?php
class CustomWrapper {}
stream_wrapper_register("custom", "CustomWrapper");
$f = fopen("custom://anywhere", "r");
echo $f === false ? "false" : "open";
"#,
    );
    assert_eq!(out.stdout, "false");
    assert!(
        !out.diagnostics.contains("Failed to open"),
        "registered user wrapper should not produce the failed-to-open warning, got diagnostics: {:?}",
        out.diagnostics,
    );
}

/// Verifies compiled PHP output for fopen user wrapper stream open true returns resource.
#[test]
fn test_fopen_user_wrapper_stream_open_true_returns_resource() {
    // Phase 10 step 3: when the wrapper class implements `stream_open` and
    // returns true, fopen() returns a resource backed by a synthetic
    // descriptor stored in `_user_wrapper_handles`. The wrapper object
    // itself is retained for later fread/fwrite/fclose dispatch.
    let out = compile_and_run(
        r#"<?php
class MyW {
    public function stream_open($path, $mode, $options, &$opened): bool {
        return true;
    }
}
stream_wrapper_register("my", "MyW");
$f = fopen("my://anywhere", "r");
echo is_resource($f) ? "ok" : "fail";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for fopen user wrapper registered class is case insensitive.
#[test]
fn test_fopen_user_wrapper_registered_class_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
class CaseWrapper {
    public function stream_open($path, $mode, $options, &$opened): bool {
        return true;
    }
}
stream_wrapper_register("casew", "casewrapper");
$f = fopen("casew://anywhere", "r");
echo is_resource($f) ? "ok" : "fail";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for fopen user wrapper round trip read write close.
#[test]
fn test_fopen_user_wrapper_round_trip_read_write_close() {
    // Phase 10 step 4: fread/fwrite/fclose dispatch into the wrapper class's
    // stream_read/stream_write/stream_close on a synthetic fd. The method
    // contracts are: stream_read returns string, stream_write returns int,
    // stream_close returns void, stream_eof returns bool.
    let out = compile_and_run(
        r#"<?php
class MyW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
    public function stream_read(int $count): string { return "hello"; }
    public function stream_write(string $data): int { return strlen($data); }
    public function stream_close(): void {}
    public function stream_eof(): bool { return false; }
}
stream_wrapper_register("my", "MyW");
$f = fopen("my://x", "r");
echo fread($f, 100);
echo "|";
echo fwrite($f, "abc");
echo "|";
echo feof($f) ? "1" : "0";
echo "|";
echo fclose($f) ? "1" : "0";
"#,
    );
    assert_eq!(out, "hello|3|0|1");
}

/// Verifies the final owner of an abandoned wrapper stream closes it on unset.
#[test]
fn test_fopen_user_wrapper_closes_on_final_owner_unset() {
    let out = compile_and_run(
        r#"<?php
class ScopeCloseWrapper {
    public function stream_open($path, $mode, $options, &$openedPath): bool {
        return true;
    }

    public function stream_close(): void {
        echo "closed|";
    }
}

stream_wrapper_register("scopecl", "ScopeCloseWrapper");
$stream = fopen("scopecl://resource", "r");
echo is_resource($stream) ? "open|" : "failed|";
unset($stream);
echo "after";
"#,
    );
    assert_eq!(out, "open|closed|after");
}

/// Verifies compiled PHP output for fopen user wrapper fputcsv routes through stream write.
#[test]
fn test_fopen_user_wrapper_fputcsv_routes_through_stream_write() {
    // fputcsv() on a userspace-wrapper resource must route its field/separator/
    // quote/newline segments into the wrapper's stream_write (via __rt_fd_write's
    // synthetic-fd dispatch) instead of a raw write to a real fd. The wrapper
    // echoes each chunk, so stdout reconstructs the exact CSV bytes: a plain row,
    // then a row whose first field embeds a comma and is therefore CSV-quoted.
    let out = compile_and_run(
        r#"<?php
class CsvW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
    public function stream_write(string $data): int { echo $data; return strlen($data); }
    public function stream_close(): void {}
}
stream_wrapper_register("csv", "CsvW");
$f = fopen("csv://x", "w");
fputcsv($f, ["a", "b", "c"]);
fputcsv($f, ["x,y", "z"]);
fclose($f);
"#,
    );
    assert_eq!(out, "a,b,c\n\"x,y\",z\n");
}

/// A user wrapper's negative `stream_write()` result is the runtime failure
/// sentinel and must surface from PHP `fwrite()` as boolean false, never integer
/// `-1`; successful writes remain integer byte counts.
#[test]
fn test_fwrite_user_wrapper_negative_result_is_false() {
    let out = compile_and_run(
        r#"<?php
class FailWriteWrapper {
    public function stream_open($path, $mode, $options, &$opened): bool { return true; }
    public function stream_write(string $data): int { return -1; }
}
stream_wrapper_register("failwrite", "FailWriteWrapper");
$stream = fopen("failwrite://x", "r+");
$result = fwrite($stream, "x");
echo ($result === false) ? "false" : gettype($result) . ":" . $result;
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for fopen user wrapper fgetc and rewind dispatch.
#[test]
fn test_fopen_user_wrapper_fgetc_and_rewind_dispatch() {
    // fgetc() reads a single byte via the wrapper's stream_read; rewind()
    // dispatches stream_seek(0, SEEK_SET) so a subsequent read restarts from
    // the beginning. (rewind previously lseek'd the synthetic fd and no-op'd.)
    let out = compile_and_run(
        r#"<?php
class W {
    public $data; public $pos;
    public function stream_open($p,$m,$o,&$op): bool { $this->data="ABCDE"; $this->pos=0; return true; }
    public function stream_read($n): string { $c=substr($this->data,$this->pos,$n); $this->pos+=strlen($c); return $c; }
    public function stream_seek($o,$w): bool { $this->pos=$o; return true; }
    public function stream_eof(): bool { return $this->pos>=strlen($this->data); }
    public function stream_close(): void {}
}
stream_wrapper_register("ww","W");
$f=fopen("ww://x","r");
echo fgetc($f) . fgetc($f);
rewind($f);
echo fgetc($f);
fclose($f);
"#,
    );
    assert_eq!(out, "ABA");
}

/// Verifies compiled PHP output for fopen user wrapper applies property defaults.
#[test]
fn test_fopen_user_wrapper_applies_property_defaults() {
    // A registered wrapper instantiated by __rt_new_by_name now receives its
    // declared property defaults (via the _class_propinit_<id> thunk), so a
    // stream_open that relies on a default without assigning it works.
    let out = compile_and_run(
        r#"<?php
class W {
    public string $prefix = "PFX:";
    public string $data;
    public int $pos;
    public function stream_open($p, $m, $o, &$op): bool { $this->data = $this->prefix . "body"; $this->pos = 0; return true; }
    public function stream_read($n): string { $c = substr($this->data, $this->pos, $n); $this->pos += strlen($c); return $c; }
    public function stream_eof(): bool { return $this->pos >= strlen($this->data); }
    public function stream_close(): void {}
}
stream_wrapper_register("ww", "W");
$h = fopen("ww://x", "r");
echo fread($h, 100);
fclose($h);
"#,
    );
    assert_eq!(out, "PFX:body");
}

/// Verifies compiled PHP output for fopen user wrapper stream get contents drains.
#[test]
fn test_fopen_user_wrapper_stream_get_contents_drains() {
    // stream_get_contents() on a synthetic wrapper fd drains via a compiled,
    // feof-gated fread loop: it checks the wrapper's stream_eof before each
    // read, so it never makes the EOF read whose empty substr result freed the
    // caller's resource cell. The result is assigned and the stream closed —
    // the exact pattern that previously SIGSEGV'd / corrupted $f.
    let out = compile_and_run(
        r#"<?php
class W {
    public $data; public $pos;
    public function stream_open($p,$m,$o,&$op): bool { $this->data="hello, world!"; $this->pos=0; return true; }
    public function stream_read($n): string { $c=substr($this->data,$this->pos,$n); $this->pos+=strlen($c); return $c; }
    public function stream_eof(): bool { return $this->pos>=strlen($this->data); }
    public function stream_close(): void {}
}
stream_wrapper_register("ww","W");
$f=fopen("ww://x","r");
$x = stream_get_contents($f);
echo "[$x]";
fclose($f);
echo "|t=" . gettype($f);
"#,
    );
    assert_eq!(out, "[hello, world!]|t=resource");
}

/// Verifies compiled PHP output for fopen user wrapper fpassthru writes and counts.
#[test]
fn test_fopen_user_wrapper_fpassthru_writes_and_counts() {
    // fpassthru() on a wrapper fd uses the same feof-gated loop: it streams each
    // chunk to stdout, returns the byte count, and leaves the resource intact so
    // a following fclose() still sees a resource (not a freed/int cell).
    let out = compile_and_run(
        r#"<?php
class W {
    public $data; public $pos;
    public function stream_open($p,$m,$o,&$op): bool { $this->data="Hello, world!"; $this->pos=0; return true; }
    public function stream_read($n): string { $c=substr($this->data,$this->pos,$n); $this->pos+=strlen($c); return $c; }
    public function stream_eof(): bool { return $this->pos>=strlen($this->data); }
    public function stream_close(): void {}
}
stream_wrapper_register("ww","W");
$f=fopen("ww://x","r");
$n=fpassthru($f);
echo "|n=$n";
fclose($f);
echo "|t=" . gettype($f);
"#,
    );
    assert_eq!(out, "Hello, world!|n=13|t=resource");
}

/// Verifies compiled PHP output for fopen user wrapper fgets reads lines.
#[test]
fn test_fopen_user_wrapper_fgets_reads_lines() {
    // fgets() on a wrapper fd reads one line at a time through a feof-gated
    // 1-byte loop, keeping the trailing newline and stopping at EOF. The
    // `!== false` loop must terminate cleanly and leave the resource intact.
    let out = compile_and_run(
        r#"<?php
class W {
    public $data; public $pos;
    public function stream_open($p,$m,$o,&$op): bool { $this->data="line1\nline2\nlast"; $this->pos=0; return true; }
    public function stream_read($n): string { $c=substr($this->data,$this->pos,$n); $this->pos+=strlen($c); return $c; }
    public function stream_eof(): bool { return $this->pos>=strlen($this->data); }
    public function stream_close(): void {}
}
stream_wrapper_register("ww","W");
$f=fopen("ww://x","r");
while (($l = fgets($f)) !== false) { echo "[" . rtrim($l, "\n") . "]"; }
fclose($f);
echo "|t=" . gettype($f);
"#,
    );
    assert_eq!(out, "[line1][line2][last]|t=resource");
}

/// Verifies compiled PHP output for fopen user wrapper fscanf reads through stream read.
#[test]
fn test_fopen_user_wrapper_fscanf_reads_through_stream_read() {
    // fscanf() reads its line via __rt_fgets, which gained a wrapper-fd branch in
    // the userspace-wrapper coverage work, so fscanf() transparently parses a line
    // drained from the wrapper's stream_read. The conformant wrapper honors $count.
    let out = compile_and_run(
        r#"<?php
class W {
    public $data; public $pos;
    public function stream_open($p,$m,$o,&$op): bool { $this->data="42 3.14 hi\n"; $this->pos=0; return true; }
    public function stream_read($n): string { $c=substr($this->data,$this->pos,$n); $this->pos+=strlen($c); return $c; }
    public function stream_eof(): bool { return $this->pos>=strlen($this->data); }
    public function stream_close(): void {}
}
stream_wrapper_register("ww","W");
$f=fopen("ww://x","r");
$r = fscanf($f, "%d %f %s");
echo $r[0] . "|" . $r[1] . "|" . $r[2];
fclose($f);
"#,
    );
    assert_eq!(out, "42|3.14|hi");
}

/// Verifies compiled PHP output for fopen user wrapper stream copy to stream drains.
#[test]
fn test_fopen_user_wrapper_stream_copy_to_stream_drains() {
    // stream_copy_to_stream() with a wrapper source uses the feof-gated loop:
    // each chunk is read via __rt_fread and written to the destination via
    // __rt_fwrite (here a real php://temp fd). The source resource must survive.
    let out = compile_and_run(
        r#"<?php
class W {
    public $data; public $pos;
    public function stream_open($p,$m,$o,&$op): bool { $this->data="copy-me-over!"; $this->pos=0; return true; }
    public function stream_read($n): string { $c=substr($this->data,$this->pos,$n); $this->pos+=strlen($c); return $c; }
    public function stream_eof(): bool { return $this->pos>=strlen($this->data); }
    public function stream_close(): void {}
}
stream_wrapper_register("ww","W");
$src=fopen("ww://x","r");
$dst=fopen("php://temp","r+");
$n=stream_copy_to_stream($src,$dst);
rewind($dst);
echo "n=$n|got=[" . stream_get_contents($dst) . "]";
fclose($src); fclose($dst);
echo "|st=" . gettype($src);
"#,
    );
    assert_eq!(out, "n=13|got=[copy-me-over!]|st=resource");
}

/// Verifies compiled PHP output for fopen user wrapper ftell dispatches to stream tell.
#[test]
fn test_fopen_user_wrapper_ftell_does_not_dispatch_to_stream_tell() {
    // The old name and expectation were both fiction: `42|-1`, on the belief that ftell()
    // dispatches into the wrapper. `php -n` answers `0|0` for this exact program. php-src has no
    // tell op for userspace wrappers — `main/streams/userspace.c` calls `stream_tell` only from
    // inside `php_userstreamop_seek` — so a freshly opened stream is at 0 whatever the method
    // says, and a wrapper without the method is at 0 too rather than at a failure sentinel.
    let out = compile_and_run(
        r#"<?php
class TellW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
    public function stream_tell(): int { return 42; }
}
class NoTellW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
}
stream_wrapper_register("tellw", "TellW");
stream_wrapper_register("notell", "NoTellW");
$f = fopen("tellw://x", "r");
echo ftell($f);
echo "|";
$g = fopen("notell://x", "r");
echo ftell($g);
"#,
    );
    assert_eq!(out, "0|0");
}

/// Verifies compiled PHP output for fopen user wrapper fstat dispatches to stream stat.
#[test]
fn test_fopen_user_wrapper_fstat_dispatches_to_stream_stat() {
    // OOS Phase E: fstat() on a synthetic wrapper fd dispatches into the
    // wrapper's stream_stat() (vtable slot 8) and returns the associative stat
    // array it builds, so fstat($f)['size'] / ['mode'] read through the boxed
    // Mixed cell. The stat method is declared WITHOUT a return type so its
    // assoc array round-trips as a Mixed (a `: array` return would be
    // integer-keyed and reject the string keys). A wrapper without stream_stat
    // falls through to boxed false, matching PHP's fstat() failure.
    let out = compile_and_run(
        r#"<?php
class StatW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
    public function stream_read($c): string { return ""; }
    public function stream_eof(): bool { return true; }
    public function stream_stat() {
        return ['dev'=>0,'ino'=>0,'mode'=>33188,'nlink'=>1,'uid'=>0,'gid'=>0,
                'rdev'=>0,'size'=>5,'atime'=>0,'mtime'=>0,'ctime'=>0,
                'blksize'=>4096,'blocks'=>1];
    }
}
class NoStatW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
    public function stream_read($c): string { return ""; }
    public function stream_eof(): bool { return true; }
}
stream_wrapper_register("statw", "StatW");
stream_wrapper_register("nostatw", "NoStatW");
$f = fopen("statw://x", "r");
$s = fstat($f);
echo gettype($s) . ":" . $s['size'] . ":" . $s['mode'];
fclose($f);
echo "|";
$g = fopen("nostatw://y", "r");
$r = fstat($g);
echo ($r === false) ? "false" : "arr";
fclose($g);
"#,
    );
    assert_eq!(out, "array:5:33188|false");
}

/// Verifies compiled PHP output for file exists dispatches to wrapper url stat.
#[test]
fn test_file_exists_dispatches_to_wrapper_url_stat() {
    // OOS Phase E: file_exists("scheme://...") on a registered userspace wrapper
    // routes through __rt_user_wrapper_url_stat, instantiates the class, and
    // calls url_stat(string $path, int $flags). The path exists iff url_stat
    // returns a stat array (not false). A non-wrapper path falls back to the
    // real filesystem stat. url_stat must declare `string $path` (PHP's actual
    // signature) — an untyped param infers as Mixed and rejects string ops.
    let out = compile_and_run(
        r#"<?php
class SW {
    public function url_stat(string $path, int $flags) {
        if (strpos($path, "yes") !== false) {
            return ['dev'=>0,'ino'=>0,'mode'=>33188,'nlink'=>1,'uid'=>0,'gid'=>0,
                    'rdev'=>0,'size'=>10,'atime'=>0,'mtime'=>0,'ctime'=>0,
                    'blksize'=>4096,'blocks'=>1];
        }
        return false;
    }
}
stream_wrapper_register("sw", "SW");
file_put_contents("probe.txt", "x");
echo file_exists("sw://yes") ? "Y" : "N";
echo file_exists("sw://no") ? "Y" : "N";
echo file_exists("probe.txt") ? "Y" : "N";
echo file_exists("no_such_elephc_probe.txt") ? "Y" : "N";
"#,
    );
    assert_eq!(out, "YNYN");
}

/// Pins how many times the stat family reaches a userspace wrapper's `url_stat()`.
///
/// php keeps a ONE-entry stat cache keyed by the exact path string, so consecutive stat-family
/// calls on the same path cost a single `url_stat()`. elephc has no such cache and re-asks every
/// time. MEASURED on `php -n` 8.5.6, against what elephc answers today:
///
/// ```text
///                                                       php   elephc
/// file_exists($p); file_exists($p);                      1      2
/// file_exists($p); filesize($p); is_file($p);            1      3
/// file_exists($p); clearstatcache(); file_exists($p);    2      2
/// is_file($p);                                           1      1
/// file_exists($e); file_exists($f); file_exists($e);     3      3
/// ```
///
/// Only the first two rows diverge, and only because php's cache HITS there. The last three
/// agree by construction: with no cache, elephc always pays N calls, which is what php also pays
/// whenever its single entry misses — after `clearstatcache()`, for a lone call, and for any
/// alternation that keeps evicting the one slot.
///
/// This is a DELIBERATE gap, pinned so it stays visible. php's cache is invalidated by very
/// nearly everything: MEASURED, a stat of ANY other path evicts it, and `touch`, `unlink`,
/// `rename`, `chmod`, `mkdir`, `rmdir`, `file_put_contents`, `file_get_contents`, a bare
/// `fopen()`/`fclose()` pair and even `shell_exec()` all clear it outright, while only pure
/// computation and `opendir()`/`closedir()` leave it standing. `clearstatcache()` clears it in
/// ALL FOUR argument shapes — `clearstatcache(true, '/other/path')` included, because php-src
/// drops `CurrentStatFile`/`CurrentLStatFile` whatever filename it was handed.
///
/// Reproducing that by enumerating invalidation points is the wrong shape of risk: missing ONE
/// of them returns a stale stat silently, which is strictly worse than the extra syscall it
/// saves, and the win is observable only through a wrapper that counts its own `url_stat()`
/// calls. The safe shape is the opposite default — an intra-block reuse that treats every call
/// it cannot prove pure as an invalidation, so a miss costs a lost optimisation rather than a
/// wrong answer. Until that exists, `clearstatcache()` correctly stays the ordered no-op it is
/// today (`lower_clearstatcache`), because there is nothing to clear; it has to grow teeth in
/// the same change that grows the cache.
#[test]
fn test_stat_family_url_stat_call_counts() {
    let out = compile_and_run(
        r#"<?php
class W {
    public $context;
    public static int $n = 0;
    public function url_stat(string $path, int $flags) {
        W::$n = W::$n + 1;
/// Verifies `stat()` and `lstat()` reach a registered wrapper's `url_stat()`, with the flags
/// PHP hands them, and still fall back to the filesystem for an ordinary path.
///
/// `stat()` was the one member of the stat family that never consulted a wrapper — the others
/// all probed `url_stat()` first — so `stat("scheme://x")` returned the filesystem's answer for
/// a path that only exists inside the wrapper. The flag values are read off reference PHP with
/// a wrapper that echoes its `$flags`, not inferred from the two documented
/// `STREAM_URL_STAT_*` constants: PHP also sets an internal no-cache bit, so `stat()` arrives
/// as 4 and `lstat()` as 4|1. A wrapper that branches on the link bit needs that exact value.
#[test]
fn test_stat_and_lstat_dispatch_to_wrapper_url_stat() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
class StatW {
    public function url_stat(string $path, int $flags) {
        echo "[", $flags, "]";
        return ['dev'=>0,'ino'=>0,'mode'=>33188,'nlink'=>1,'uid'=>0,'gid'=>0,
                'rdev'=>0,'size'=>77,'atime'=>0,'mtime'=>5,'ctime'=>0,
                'blksize'=>4096,'blocks'=>1];
    }
}
stream_wrapper_register("statw", "StatW");
file_put_contents("statw_probe.txt", "abcd");
$a = stat("statw://x");
echo $a["size"], ":", $a["mtime"], "|";
$b = lstat("statw://y");
echo $b["size"], "|";
$c = stat("statw_probe.txt");
echo $c["size"];
unlink("statw_probe.txt");
"#,
    );
    assert_eq!(out, "[4]77:5|[5]77|4");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies every stat-family builtin that consults a wrapper hands it the flags PHP hands it.
///
/// The values are not derivable from the two documented `STREAM_URL_STAT_*` constants: PHP also
/// sets an internal no-cache bit, so the observed table is `stat 4 · lstat 5 · filesize 4 ·
/// file_exists 6 · is_file 6` — NOCACHE everywhere, plus LINK for `lstat` and QUIET for the
/// existence predicates. All five passed 0 or nothing at all before, so a wrapper deciding from
/// the quiet bit whether to emit its own warning never saw it.
///
/// The one echo per builtin also pins the call COUNT: PHP invokes `url_stat()` exactly once per
/// builtin, so the permission predicates — which need `mode`, `uid` and `gid` together — have to
/// read all three out of a single result rather than one field per call.
#[test]
fn test_stat_family_hands_the_wrapper_the_flags_php_hands_it() {
    let out = compile_and_run(
        r#"<?php
class FlagW {
    public function url_stat(string $path, int $flags) {
        echo substr($path, 8), "=", $flags, " ";
        return ['dev'=>0,'ino'=>0,'mode'=>33188,'nlink'=>1,'uid'=>0,'gid'=>0,
                'rdev'=>0,'size'=>7,'atime'=>0,'mtime'=>0,'ctime'=>0,
                'blksize'=>4096,'blocks'=>1];
    }
}
stream_wrapper_register("cnt", "W");

W::$n = 0;
file_exists("cnt://a");
file_exists("cnt://a");
echo "same=", W::$n, "\n";

W::$n = 0;
file_exists("cnt://b");
filesize("cnt://b");
is_file("cnt://b");
echo "three=", W::$n, "\n";

W::$n = 0;
file_exists("cnt://c");
clearstatcache();
file_exists("cnt://c");
echo "cleared=", W::$n, "\n";

W::$n = 0;
is_file("cnt://d");
echo "single=", W::$n, "\n";

W::$n = 0;
file_exists("cnt://e");
file_exists("cnt://f");
file_exists("cnt://e");
echo "alternating=", W::$n, "\n";
stream_wrapper_register("flagw", "FlagW");
stat("flagw://stat");
lstat("flagw://lstat");
file_exists("flagw://exists");
filesize("flagw://size");
is_file("flagw://isfile");
is_readable("flagw://readable");
is_writable("flagw://writable");
is_executable("flagw://executable");
"#,
    );
    assert_eq!(
        out,
        "same=2\nthree=3\ncleared=2\nsingle=1\nalternating=3\n",
        "elephc re-asks where php's one-entry cache would have answered; php gives 1/1/2/1/3"
    );
}

        "stat=4 lstat=5 exists=6 size=4 isfile=6 readable=6 writable=6 executable=6 "
    );
}

/// Verifies the permission predicates apply PHP's triad-selection rule to a wrapper's stat.
///
/// PHP does not mask the mode against `S_IRUSR|S_IRGRP|S_IROTH`: it picks ONE triad — owner when
/// the reported uid is the process uid, group when the reported gid is the process gid or one of
/// its supplementary groups, world otherwise — and then ignores the other two. Measured against
/// reference PHP, which answers `is_readable() === false` for a `mode 0700` file owned by someone
/// else even though the owner read bit is set.
///
/// The uid/gid here are values no process can hold, so the world triad is selected on every host
/// and the expectation does not depend on who runs the suite. Before this dispatch existed the
/// three predicates ran a real `access()` on the literal `perm://…` path and answered false for
/// all six cases, including the ones PHP answers true for.
#[test]
fn test_permission_predicates_apply_the_php_triad_rule_to_wrapper_stat() {
    let out = compile_and_run(
        r#"<?php
class PermW {
    public function url_stat(string $path, int $flags) {
        $mode = (int) substr($path, 7);
        return ['dev'=>0,'ino'=>0,'mode'=>$mode,'nlink'=>1,
                'uid'=>2000000001,'gid'=>2000000002,
                'rdev'=>0,'size'=>1,'atime'=>0,'mtime'=>0,'ctime'=>0,
                'blksize'=>4096,'blocks'=>1];
    }
}
stream_wrapper_register("perm", "PermW");
foreach ([0644, 0700, 0007, 0002, 0070] as $mode) {
    echo is_readable("perm://$mode") ? "r" : "-";
    echo is_writable("perm://$mode") ? "w" : "-";
    echo is_executable("perm://$mode") ? "x" : "-";
    echo " ";
}
"#,
    );
    assert_eq!(out, "r-- --- rwx -w- --- ");
}

/// Verifies `is_dir()` and `filemtime()` reach a registered wrapper's `url_stat()`.
///
/// `is_dir()` had no wrapper dispatch at all while its twin `is_file()` did — the two differ
/// only in the `S_IFMT` value they compare against, and writing them as separate code paths is
/// how one came to be wired and the other not. `filemtime()` needed a third field selector in
/// the shared runtime helper, which until now could extract only `size` and `mode`.
#[test]
fn test_is_dir_and_filemtime_dispatch_to_wrapper_url_stat() {
    let out = compile_and_run(
        r#"<?php
class TypeW {
    public function url_stat(string $path, int $flags) {
        $mode = strpos($path, "dir") !== false ? 16877 : 33188;
        return ['dev'=>0,'ino'=>0,'mode'=>$mode,'nlink'=>1,'uid'=>0,'gid'=>0,
                'rdev'=>0,'size'=>3,'atime'=>0,'mtime'=>4321,'ctime'=>0,
                'blksize'=>4096,'blocks'=>1];
    }
}
stream_wrapper_register("typew", "TypeW");
echo is_dir("typew://dir") ? "D" : "-";
echo is_dir("typew://file") ? "D" : "-";
echo is_file("typew://file") ? "F" : "-";
echo is_file("typew://dir") ? "F" : "-";
echo "|", filemtime("typew://file");
"#,
    );
    assert_eq!(out, "D-F-|4321");
}

/// Verifies compiled PHP output for filesize and is file dispatch to wrapper url stat.
#[test]
fn test_filesize_and_is_file_dispatch_to_wrapper_url_stat() {
    // OOS Phase E: filesize()/is_file() on a registered wrapper route through
    // __rt_user_wrapper_url_stat_field, which calls url_stat(string $path, int
    // $flags) and extracts the int 'size' (filesize) or 'mode' (is_file, then a
    // S_IFMT==S_IFREG check). Non-wrapper paths fall back to the real
    // filesystem. The url_stat result is a Mixed array; ['size']/['mode'] are
    // read via __rt_mixed_array_get and the boxes are released.
    let out = compile_and_run(
        r#"<?php
class SW {
    public function url_stat(string $path, int $flags) {
        if (strpos($path, "file") !== false) { return ['size'=>123, 'mode'=>33188]; }
        if (strpos($path, "dir")  !== false) { return ['size'=>0,   'mode'=>16877]; }
        return false;
    }
}
stream_wrapper_register("sw", "SW");
file_put_contents("real.txt", "abcde");
echo filesize("sw://file");
echo ":" . filesize("real.txt");
echo ":" . (is_file("sw://file") ? "Y" : "N");
echo ":" . (is_file("sw://dir") ? "Y" : "N");
echo ":" . (is_file("sw://nope") ? "Y" : "N");
echo ":" . (is_file("real.txt") ? "Y" : "N");
echo ":" . (is_file("no_such_elephc_probe") ? "Y" : "N");
"#,
    );
    assert_eq!(out, "123:5:Y:N:N:Y:N");
}

/// Verifies compiled PHP output for readfile dispatches to wrapper.
#[test]
fn test_readfile_dispatches_to_wrapper() {
    // OOS Phase E: readfile("scheme://...") on a registered wrapper routes
    // through __rt_readfile_wrapper (fopen + feof-gated fread drain to stdout +
    // close), echoing the wrapper's contents and returning the byte count. A
    // non-wrapper path falls back to __rt_readfile (raw open + stream), which
    // preserves the directory read-error semantics.
    let out = compile_and_run(
        r#"<?php
class RW {
    public $pos = 0;
    public function stream_open(string $p, string $m, int $o, &$op): bool { return true; }
    public function stream_read(int $count): string { if ($this->pos >= 5) { return ""; } $this->pos = 5; return "HELLO"; }
    public function stream_eof(): bool { return $this->pos >= 5; }
}
stream_wrapper_register("rw", "RW");
file_put_contents("rfr.txt", "abc");
$n = readfile("rw://x");
echo "|" . $n . "|";
$m = readfile("rfr.txt");
echo "|" . $m;
"#,
    );
    assert_eq!(out, "HELLO|5|abc|3");
}

/// Verifies compiled PHP output for fgetcsv and stream get line on wrapper.
#[test]
fn test_fgetcsv_and_stream_get_line_on_wrapper() {
    // OOS Phase E: fgetcsv() and stream_get_line() read from a wrapper fd.
    // fgetcsv goes through __rt_fgetcsv -> __rt_fgets, and stream_get_line
    // through __rt_stream_get_line; both gained a feof-gated, 1-byte __rt_fread
    // loop that accumulates into _user_wrapper_drain_buf (NOT _concat_buf, which
    // each __rt_fread result may occupy). The wrapper's stream_read honors
    // $count (returns a substr), matching PHP's stream_read contract.
    let out = compile_and_run(
        r#"<?php
class LW {
    public $data = "a,b,c\n1,2,3\n";
    public $pos = 0;
    public function stream_open(string $p, string $m, int $o, &$op): bool { $this->pos = 0; return true; }
    public function stream_read(int $count): string {
        $chunk = substr($this->data, $this->pos, $count);
        $this->pos = $this->pos + strlen($chunk);
        return $chunk;
    }
    public function stream_eof(): bool { return $this->pos >= strlen($this->data); }
}
stream_wrapper_register("lw", "LW");
$g = fopen("lw://x", "r");
$r1 = fgetcsv($g);
$r2 = fgetcsv($g);
echo implode("|", $r1) . ":" . implode("|", $r2);
fclose($g);
echo "/";
$h = fopen("lw://y", "r");
echo trim(stream_get_line($h, 100, "\n"));
echo ",";
echo trim(stream_get_line($h, 100, "\n"));
fclose($h);
"#,
    );
    assert_eq!(out, "a|b|c:1|2|3/a,b,c,1,2,3");
}

/// Verifies compiled PHP output for fopen user wrapper fflush dispatches to stream flush.
#[test]
fn test_fopen_user_wrapper_fflush_dispatches_to_stream_flush() {
    // fflush() dispatches into the wrapper's stream_flush and returns its bool
    // result. Without stream_flush php answers FALSE, measured on 8.5.6 — the
    // "nothing to flush is a benign success" default this used to assert was the
    // helper's own convention, not php's.
    let out = compile_and_run(
        r#"<?php
class FlushW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
    public function stream_flush(): bool { return true; }
}
class NoFlushW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
}
stream_wrapper_register("flushw", "FlushW");
stream_wrapper_register("noflush", "NoFlushW");
$f = fopen("flushw://x", "r");
echo fflush($f) ? "1" : "0";
echo "|";
$g = fopen("noflush://x", "r");
echo fflush($g) ? "1" : "0";
"#,
    );
    assert_eq!(out, "1|0");
}

/// Verifies compiled PHP output for fopen user wrapper fseek dispatches to stream seek.
#[test]
fn test_fopen_user_wrapper_fseek_dispatches_to_stream_seek() {
    // Phase 10 step 4: fseek dispatches into the wrapper's stream_seek and
    // maps a `true` return to 0, anything else (including a missing method)
    // to -1 — matching PHP's int fseek() result.
    let out = compile_and_run(
        r#"<?php
class SeekW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
    public function stream_seek(int $offset, int $whence): bool { return true; }
}
stream_wrapper_register("seek", "SeekW");
$f = fopen("seek://x", "r");
echo fseek($f, 10);
echo "|";
echo fseek($f, 0, 2);
"#,
    );
    assert_eq!(out, "0|0");
}

/// Verifies compiled PHP output for fopen user wrapper fseek missing method returns minus one.
#[test]
fn test_fopen_user_wrapper_fseek_missing_method_returns_minus_one() {
    // Phase 10 step 4: when the wrapper class does not implement stream_seek,
    // the user-wrapper helper falls through to the PHP -1 failure sentinel.
    let out = compile_and_run(
        r#"<?php
class NoSeekW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
}
stream_wrapper_register("noseek", "NoSeekW");
$f = fopen("noseek://x", "r");
echo fseek($f, 10);
"#,
    );
    assert_eq!(out, "-1");
}

/// Verifies stream_set_blocking() and stream_set_timeout() on a registered
/// userspace-wrapper stream dispatch into the wrapper's stream_set_option(),
/// threading the option code and value; a wrapper without stream_set_option
/// returns false.
#[test]
fn test_stream_set_option_wrapper_dispatch() {
    // G1: stream_set_blocking($fp, $mode) → stream_set_option(STREAM_OPTION_BLOCKING=1,
    // mode?1:0, 0); stream_set_timeout($fp, $sec) → stream_set_option(
    // STREAM_OPTION_READ_TIMEOUT=4, sec, 0) — both via vtable slot 13 on a
    // synthetic wrapper fd. A wrapper missing stream_set_option yields false.
    let out = compile_and_run(
        r#"<?php
class OptW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
    public function stream_set_option(int $option, int $arg1, int $arg2): bool {
        if ($option === STREAM_OPTION_BLOCKING)     return $arg1 === 0;
        if ($option === STREAM_OPTION_READ_TIMEOUT) return $arg1 === 7;
        return false;
    }
}
class NoOptW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
}
stream_wrapper_register("opt", "OptW");
stream_wrapper_register("noopt", "NoOptW");
$f = fopen("opt://x", "r");
echo stream_set_blocking($f, false) ? "1" : "0";
echo stream_set_blocking($f, true)  ? "1" : "0";
echo stream_set_timeout($f, 7)      ? "1" : "0";
echo stream_set_timeout($f, 3)      ? "1" : "0";
echo "|";
$g = fopen("noopt://x", "r");
echo stream_set_blocking($g, false) ? "1" : "0";
"#,
    );
    assert_eq!(out, "1010|0");
}

/// Verifies chmod() on a registered userspace-wrapper scheme dispatches into the
/// wrapper's stream_metadata($path, STREAM_META_ACCESS, $mode), threading the
/// option and mode through; a wrapper without stream_metadata returns false.
#[test]
fn test_chmod_wrapper_dispatches_to_stream_metadata() {
    // G1: chmod("scheme://path", $mode) on a registered wrapper routes to
    // stream_metadata (vtable slot 14) with option STREAM_META_ACCESS (6) and
    // value = $mode. A non-wrapper path keeps the libc chmod; a wrapper missing
    // stream_metadata yields false.
    let out = compile_and_run(
        r#"<?php
class MetaW {
    public function stream_metadata(string $path, int $option, mixed $value): bool {
        return $path === "mw://f" && $option === STREAM_META_ACCESS && $value === 0644;
    }
}
class NoMetaW {}
stream_wrapper_register("mw", "MetaW");
stream_wrapper_register("nm", "NoMetaW");
echo chmod("mw://f", 0644) ? "1" : "0";
echo chmod("mw://f", 0700) ? "1" : "0";
echo chmod("nm://f", 0644) ? "1" : "0";
"#,
    );
    assert_eq!(out, "100");
}

/// Verifies unlink()/mkdir()/rmdir() on a registered userspace-wrapper scheme
/// dispatch into the wrapper's matching path method, and that a wrapper without
/// the method (or a non-wrapper path) does not take the wrapper branch.
#[test]
fn test_user_wrapper_path_ops_dispatch() {
    // G1: unlink/mkdir/rmdir on a "scheme://" path matching a registered wrapper
    // route to the wrapper's unlink()/mkdir()/rmdir() (vtable slots 15/17/18),
    // returning their bool result; a wrapper missing the method yields false.
    let out = compile_and_run(
        r#"<?php
class PathW {
    public function unlink(string $path): bool { return $path === "pw://gone"; }
    public function mkdir(string $path): bool { return $path === "pw://newdir"; }
    public function rmdir(string $path): bool { return $path === "pw://olddir"; }
}
class BareW {}
stream_wrapper_register("pw", "PathW");
stream_wrapper_register("bare", "BareW");
echo unlink("pw://gone") ? "1" : "0";
echo mkdir("pw://newdir") ? "1" : "0";
echo rmdir("pw://olddir") ? "1" : "0";
echo "|";
echo unlink("pw://other") ? "1" : "0";
echo unlink("bare://x") ? "1" : "0";
"#,
    );
    assert_eq!(out, "111|00");
}

/// Verifies rename() on a registered userspace-wrapper source scheme dispatches
/// into the wrapper's rename(), threading both the source and destination URLs,
/// and that a wrapper without rename() returns false.
#[test]
fn test_user_wrapper_rename_dispatch() {
    // G1: rename($from, $to) where $from is a registered "scheme://" path routes
    // to the wrapper's rename() (vtable slot 16), passing both full URLs.
    let out = compile_and_run(
        r#"<?php
class MoveW {
    public function rename(string $from, string $to): bool {
        return $from === "mw://a" && $to === "mw://b";
    }
}
class NoMoveW {}
stream_wrapper_register("mw", "MoveW");
stream_wrapper_register("nm", "NoMoveW");
echo rename("mw://a", "mw://b") ? "1" : "0";
echo rename("mw://a", "mw://wrong") ? "1" : "0";
echo rename("nm://a", "nm://b") ? "1" : "0";
"#,
    );
    assert_eq!(out, "100");
}

/// Verifies flock() on a userspace-wrapper stream dispatches into the wrapper's
/// stream_lock(), threading the lock operation through, and returns its bool
/// result; a wrapper that does not implement stream_lock yields false.
#[test]
fn test_fopen_user_wrapper_flock_dispatches_to_stream_lock() {
    // G1: flock($fp, $op) on a synthetic wrapper fd routes to stream_lock($op).
    // The wrapper reports whether it received LOCK_EX, proving the operation is
    // threaded through; a wrapper missing stream_lock falls through to false.
    let out = compile_and_run(
        r#"<?php
class LockW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
    public function stream_lock(int $operation): bool { return $operation === LOCK_EX; }
}
class NoLockW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
}
stream_wrapper_register("lockw", "LockW");
stream_wrapper_register("nolock", "NoLockW");
$f = fopen("lockw://x", "r");
echo flock($f, LOCK_EX) ? "1" : "0";
echo "|";
echo flock($f, LOCK_SH) ? "1" : "0";
echo "|";
$g = fopen("nolock://x", "r");
echo flock($g, LOCK_EX) ? "1" : "0";
"#,
    );
    assert_eq!(out, "1|0|0");
}

/// Verifies ftruncate() on a userspace-wrapper stream dispatches into the
/// wrapper's stream_truncate(), threading the new size through, and returns its
/// bool result; a wrapper that does not implement stream_truncate yields false.
#[test]
fn test_fopen_user_wrapper_ftruncate_dispatches_to_stream_truncate() {
    // G1: ftruncate($fp, $size) on a synthetic wrapper fd routes to
    // stream_truncate($new_size). The wrapper reports whether it received 42,
    // proving the size is threaded; a wrapper missing stream_truncate is false.
    let out = compile_and_run(
        r#"<?php
class TruncW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
    public function stream_truncate(int $new_size): bool { return $new_size === 42; }
}
class NoTruncW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
}
stream_wrapper_register("truncw", "TruncW");
stream_wrapper_register("notrunc", "NoTruncW");
$f = fopen("truncw://x", "w");
echo ftruncate($f, 42) ? "1" : "0";
echo "|";
echo ftruncate($f, 7) ? "1" : "0";
echo "|";
$g = fopen("notrunc://x", "w");
echo ftruncate($g, 42) ? "1" : "0";
"#,
    );
    assert_eq!(out, "1|0|0");
}

/// Verifies compiled PHP output for fopen user wrapper stream open receives opened path arg.
#[test]
fn test_fopen_user_wrapper_stream_open_receives_opened_path_arg() {
    // Phase 10 follow-up: stream_open is now called with the 5th
    // `?string &$opened_path` argument (a writable scratch slot). Wrappers
    // that declare the PHP-faithful 5-arg signature must dispatch
    // correctly. The value the wrapper writes back is not surfaced to the
    // caller (v1 limitation), but the wrapper must be able to write
    // without crashing.
    let out = compile_and_run(
        r#"<?php
class OpenedW {
    public bool $touched_opened_path = false;
    public function stream_open(string $path, string $mode, int $options, ?string &$opened_path): bool {
        $opened_path = "/resolved/" . $path;
        $this->touched_opened_path = true;
        return true;
    }
    public function stream_eof(): bool { return false; }
}
stream_wrapper_register("opened", "OpenedW");
$f = fopen("opened://x", "r");
echo is_resource($f) ? "ok" : "fail";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for fopen user wrapper handles above old cap.
#[test]
fn test_fopen_user_wrapper_handles_above_old_cap() {
    // Phase 10 follow-up: bumped USER_WRAPPER_HANDLES_CAP from 64 to 256.
    // Opens 100 concurrent wrapper handles, each backed by a no-op stream_open
    // that returns true. Used to overflow the 64-slot table; now succeeds.
    let out = compile_and_run(
        r#"<?php
class CapW {
    public function stream_open($p, $m, $o, &$op): bool { return true; }
}
stream_wrapper_register("cap", "CapW");
$handles = [];
for ($i = 0; $i < 100; $i++) {
    $h = fopen("cap://x", "r");
    if (!is_resource($h)) { echo "fail@" . $i; return; }
    $handles[] = $h;
}
echo "ok-" . count($handles);
"#,
    );
    assert_eq!(out, "ok-100");
}

/// Verifies compiled PHP output for fopen user wrapper failure does not leak.
#[test]
fn test_fopen_user_wrapper_failure_does_not_leak() {
    // Phase 10 follow-up: after stream_open returns false, the runtime
    // helper releases the wrapper object via __rt_object_free_deep so
    // long-running programs that probe many failing wrappers do not
    // accumulate one heap object per attempt. Loops 256 fopen calls and
    // checks the loop completes (a stress signal — the leak path itself
    // is verified by the deep-free call being on the path).
    let out = compile_and_run(
        r#"<?php
class MyW {
    public function stream_open($p, $m, $o, &$op): bool { return false; }
}
stream_wrapper_register("leak", "MyW");
for ($i = 0; $i < 256; $i++) {
    $f = fopen("leak://x", "r");
    if ($f !== false) {
        echo "leaked"; return;
    }
}
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for fopen user wrapper stream open false returns false.
#[test]
fn test_fopen_user_wrapper_stream_open_false_returns_false() {
    // Phase 10 step 3: when the wrapper class's stream_open returns false,
    // fopen() reports failure (PHP `false`) without emitting the standard
    // "Failed to open stream" warning.
    let out = compile_and_run_capture(
        r#"<?php
class MyW {
    public function stream_open($path, $mode, $options, &$opened): bool {
        return false;
    }
}
stream_wrapper_register("my", "MyW");
$f = fopen("my://anywhere", "r");
echo $f === false ? "false" : "open";
"#,
    );
    assert_eq!(out.stdout, "false");
    assert!(
        !out.diagnostics.contains("Failed to open"),
        "wrapper stream_open returning false should not emit the failed-to-open warning, got diagnostics: {:?}",
        out.diagnostics,
    );
}

/// Verifies compiled PHP output for stream socket get name.
#[test]
fn test_stream_socket_get_name() {
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:54743");
echo stream_socket_get_name($srv, false);
echo "|";
$cli = stream_socket_client("tcp://127.0.0.1:54743");
echo stream_socket_get_name($cli, true);
"#,
    );
    assert_eq!(out, "127.0.0.1:54743|127.0.0.1:54743");
}

/// Verifies compiled PHP output for stream socket client resolves hostname.
#[test]
fn test_stream_socket_client_resolves_hostname() {
    // A non-numeric host in a socket address is resolved through gethostbyname.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://127.0.0.1:54920");
$cli = stream_socket_client("tcp://localhost:54920");
$conn = stream_socket_accept($srv);
fwrite($cli, "resolved");
echo fread($conn, 16);
"#,
    );
    assert_eq!(out, "resolved");
}

/// Verifies compiled PHP output for stream socket server resolves hostname.
#[test]
fn test_stream_socket_server_resolves_hostname() {
    // Host-name resolution applies to the server bind address too.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://localhost:54921");
$cli = stream_socket_client("tcp://127.0.0.1:54921");
$conn = stream_socket_accept($srv);
fwrite($cli, "bound by name");
echo fread($conn, 32);
"#,
    );
    assert_eq!(out, "bound by name");
}

/// Verifies compiled PHP output for stream socket client ipv6 hostname via dns.
#[test]
fn test_stream_socket_client_ipv6_hostname_via_dns() {
    // Phase 11 B1: tcp://[hostname]:port now resolves the bracketed token
    // through getaddrinfo with AF_INET6 hint when inet_pton rejects the
    // literal. `localhost` resolves to ::1 on every supported system, so
    // a server bound to [::1] accepts the client built from
    // [localhost]:port end-to-end without any literal-IPv6 input.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://[::1]:55821");
echo is_resource($srv) ? "srv|" : "srv_fail|";
$cli = stream_socket_client("tcp://[localhost]:55821");
echo is_resource($cli) ? "cli|" : "cli_fail|";
$conn = stream_socket_accept($srv);
fwrite($cli, "v6-dns");
echo fread($conn, 16);
fclose($conn); fclose($cli); fclose($srv);
"#,
    );
    assert_eq!(out, "srv|cli|v6-dns");
}

/// Verifies compiled PHP output for stream socket server ipv6 literal roundtrip.
#[test]
fn test_stream_socket_server_ipv6_literal_roundtrip() {
    // Full PHP-side IPv6 round-trip: stream_socket_server binds [::1]:port,
    // stream_socket_client connects, fwrite/fread carry the payload. This
    // exercises both __rt_stream_socket_server_v6 and the client's IPv6
    // dispatch in the same binary.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://[::1]:54937");
echo is_resource($srv) ? "srv|" : "srv_fail|";
$cli = stream_socket_client("tcp://[::1]:54937");
echo is_resource($cli) ? "cli|" : "cli_fail|";
$conn = stream_socket_accept($srv);
fwrite($cli, "v6-ping");
echo fread($conn, 16);
"#,
    );
    assert_eq!(out, "srv|cli|v6-ping");
}

/// Verifies compiled PHP output for udp ipv6 round trip.
#[test]
fn test_udp_ipv6_round_trip() {
    // UDP over IPv6: stream_socket_server binds [::1]:port with SOCK_DGRAM
    // (no listen), stream_socket_client connects (sets default target),
    // fwrite/fread carry one datagram each way. This exercises the
    // udp:// scheme detection in both v6 dispatchers.
    //
    // STREAM_SERVER_BIND is required: PHP's default flags ask for listen() too, and a datagram
    // transport refuses it. The port is left to the kernel because the fixed one this test used to
    // name is owned by a macOS system service on some machines, which failed the bind outright.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("udp://[::1]:0", $e, $m, STREAM_SERVER_BIND);
echo is_resource($srv) ? "srv|" : "srv_fail|";
$cli = stream_socket_client("udp://" . stream_socket_get_name($srv, false));
echo is_resource($cli) ? "cli|" : "cli_fail|";
fwrite($cli, "v6-udp");
echo fread($srv, 16);
"#,
    );
    assert_eq!(out, "srv|cli|v6-udp");
}

/// A wrapper's untyped contract parameters carry the types PHP documents for them.
///
/// `stream_write($data) { return strlen($data); }` is the signature the manual shows, and it
/// failed to compile: a wrapper's methods are reached through a runtime vtable with raw
/// fixed-ABI arguments, so they are deliberately excluded from the pass that widens untyped
/// parameters to boxed Mixed — and they kept the `Int` an untyped parameter is seeded with.
///
/// Every contract method here uses its parameter AS its documented type, so a wrong seeding
/// fails the build rather than the assertion. The plain class at the end is the control: a
/// method named `stream_write` on something that is not a wrapper must keep its own inference,
/// because a method name is not a contract.
///
/// The second `fread()` is what pins `stream_read($count)`'s parameter: the wrapper slices with
/// `substr(self::$data, $this->pos, $count)`, so a count seeded as anything but an integer would
/// hand back the wrong window rather than fail the build. `ftell()` is deliberately absent — it
/// answers garbage on a wrapper stream today, which this test discovered and which is tracked
/// separately; asserting it here would tie an unrelated defect to this one.
#[test]
fn test_wrapper_contract_params_carry_their_documented_types() {
    let out = compile_and_run(
        r#"<?php
class Mem {
    public $pos = 0;
    public static string $data = "wrapped payload";
    public function stream_open($path, $mode, $opts, &$opened) {
        $this->pos = 0;
        return strlen($path) > 0 && strlen($mode) > 0;
    }
    public function stream_read($count) {
        $r = substr(self::$data, $this->pos, $count);
        $this->pos += strlen($r);
        return $r;
    }
    public function stream_write($d) { return strlen($d); }
    public function stream_eof() { return $this->pos >= strlen(self::$data); }
    public function stream_seek($offset, $whence) { $this->pos = $offset; return true; }
    public function stream_tell() { return $this->pos; }
    public function stream_close() {}
}
stream_wrapper_register("memc", "Mem");
$h = fopen("memc://x", "r");
echo fread($h, 7), "|";
echo fread($h, 8), "|";
fclose($h);

class NotAWrapper {
    public function stream_write($d) { return $d + 1; }
}
$n = new NotAWrapper();
echo $n->stream_write(41);
"#,
    );
    assert_eq!(out, "wrapped| payload|42");
}

/// `ftell()` on a wrapper stream reports PHP's position, not whatever `stream_tell()` says.
///
/// php-src has no tell op for userspace wrappers: `main/streams/userspace.c` calls `stream_tell`
/// only from inside `php_userstreamop_seek`, to reconcile after a seek. The position is PHP's
/// own, advanced by whatever each read moved. elephc asked the method on every `ftell()`, and
/// since an undeclared return hands back a boxed cell, it printed a pointer — a different number
/// each run.
///
/// Fixing only the boxing would have been worse than the crash-shaped answer: it would have
/// reported what the wrapper CLAIMS. The sequence here separates the two — after seven bytes the
/// answer must be 7, and after `fseek(3)` it must follow the seek.
#[test]
fn test_wrapper_ftell_reports_phps_position_not_stream_tell() {
    let out = compile_and_run(
        r#"<?php
class Pos {
    public $pos = 0;
    public static string $data = "wrapped payload";
    public function stream_open($p, $m, $o, &$op) { $this->pos = 0; return true; }
    public function stream_read($count) {
        $r = substr(self::$data, $this->pos, $count);
        $this->pos += strlen($r);
        return $r;
    }
    public function stream_eof() { return $this->pos >= strlen(self::$data); }
    public function stream_seek($offset, $whence) { $this->pos = $offset; return true; }
    public function stream_tell() { return $this->pos; }
    public function stream_close() {}
}
stream_wrapper_register("memp", "Pos");
$h = fopen("memp://x", "r");
echo ftell($h), "|";
echo fread($h, 7), "|";
echo ftell($h), "|";
fseek($h, 3);
echo ftell($h), "|";
echo fread($h, 4), "|";
echo ftell($h);
fclose($h);
"#,
    );
    assert_eq!(out, "0|wrapped|7|3|pped|7");
}

/// `rewind()` reconciles the wrapper position the same way `fseek()` does.
///
/// `rewind($h)` IS `fseek($h, 0)`, and it needed the same reconciliation — which NEITHER
/// architecture had. The wrapper's own `$this->pos` went back to zero, so the read after the
/// rewind returned the right bytes and only the number `ftell()` reported was wrong: `php -n`
/// answers `wrapped|7|0|wrap|4`, elephc answered `wrapped|7|7|wrap|11`. Reading the correct
/// bytes while reporting the wrong offset is what let this sit behind the `fseek()` test.
///
/// The read after the rewind is part of the assertion on purpose: a fix that reset the tracked
/// position without leaving the stream usable would satisfy the `0` and fail here.
#[test]
fn test_rewind_resets_the_position_ftell_reports_for_a_wrapper() {
    let out = compile_and_run(
        r#"<?php
class Rew {
    public $pos = 0;
    public static string $data = "wrapped payload";
    public function stream_open($p, $m, $o, &$op) { $this->pos = 0; return true; }
    public function stream_read($count) {
        $r = substr(self::$data, $this->pos, $count);
        $this->pos += strlen($r);
        return $r;
    }
    public function stream_eof() { return $this->pos >= strlen(self::$data); }
    public function stream_seek($offset, $whence) { $this->pos = $offset; return true; }
    public function stream_tell() { return $this->pos; }
    public function stream_close() {}
}
stream_wrapper_register("memrw", "Rew");
$h = fopen("memrw://x", "r");
echo fread($h, 7), "|";
echo ftell($h), "|";
rewind($h);
echo ftell($h), "|";
echo fread($h, 4), "|";
echo ftell($h);
fclose($h);
"#,
    );
    assert_eq!(out, "wrapped|7|0|wrap|4");
}

/// `file_get_contents()` reads through a registered wrapper, as `fopen()` already did.
///
/// php-src has no separate reader here — `file_get_contents` is `php_stream_open_wrapper`
/// followed by `_php_stream_copy_to_mem` — so every scheme the opener knows is readable by
/// definition. elephc had a hand-rolled scheme ladder that knew `http`, `https` and `ftp` and
/// then fell back to a filename, so a registered wrapper answered `Failed to open stream` from
/// `file_get_contents()` while `fopen()` on the very same URI worked.
///
/// The unknown scheme at the end is the other half: delegating to the opener has to keep
/// reporting a scheme nobody registered, rather than turn it into a silent empty read.
#[test]
fn test_file_get_contents_reads_through_a_registered_wrapper() {
    let out = compile_and_run_capture(
        r#"<?php
class Src {
    public $pos = 0;
    public static string $data = "wrapped payload";
    public function stream_open($p, $m, $o, &$op) { $this->pos = 0; return true; }
    public function stream_read($count) {
        $r = substr(self::$data, $this->pos, $count);
        $this->pos += strlen($r);
        return $r;
    }
    public function stream_eof() { return $this->pos >= strlen(self::$data); }
    public function stream_close() {}
}
stream_wrapper_register("memg", "Src");
echo var_export(file_get_contents("memg://y"), true), "|";
echo var_export(@file_get_contents("nosuchscheme://y"), true);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "'wrapped payload'|false");
}

/// A wrapper that declares no `$context` gets PHP 8.2's dynamic-property deprecation.
///
/// PHP assigns the stream context onto the wrapper object whether or not the class declared a
/// property for it, and since 8.2 the invented assignment is deprecated. elephc simply skipped
/// the injection and said nothing, so a program that would be told to declare its property under
/// PHP heard nothing here.
///
/// The declaring class is the control: naming a `$context` property must stay silent, which is
/// what separates this from a notice fired on every wrapper open.
#[test]
fn test_wrapper_without_declared_context_gets_phps_deprecation() {
    let out = compile_and_run_capture(
        r#"<?php
class NoCtx {
    public function stream_open($p, $m, $o, &$op) { return true; }
    public function stream_read($count) { return ""; }
    public function stream_eof() { return true; }
    public function stream_close() {}
}
class HasCtx {
    public mixed $context;
    public function stream_open($p, $m, $o, &$op) { return true; }
    public function stream_read($count) { return ""; }
    public function stream_eof() { return true; }
    public function stream_close() {}
}
stream_wrapper_register("noctx", "NoCtx");
stream_wrapper_register("hasctx", "HasCtx");
$a = fopen("noctx://x", "r");
fclose($a);
$b = fopen("hasctx://x", "r");
fclose($b);
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.diagnostics
            .contains("Deprecated: Creation of dynamic property NoCtx::$context is deprecated"),
        "expected php's wording, got diagnostics={}",
        out.diagnostics
    );
    // The declaring class is the control, and it declares `mixed $context` rather than a bare
    // `$context` on purpose. An UNTYPED property is not typed `Mixed` here, and the vtable slot
    // that carries the context offset only records a property it can see as Mixed — so
    // `public $context;`, the spelling the manual shows, still reads as undeclared, never
    // receives its context, and collects this deprecation. That is tracked on its own; pinning
    // it here would tie an unrelated typing defect to this notice.
    assert!(
        !out.diagnostics.contains("HasCtx::$context"),
        "a declared property must not be deprecated, got diagnostics={}",
        out.diagnostics
    );
}

/// A failing IPv6 server has to say why, like its IPv4 sibling.
///
/// The IPv6 helper is tail-called from the dispatcher, which clears the error stash before jumping;
/// the helper then failed its `bind()` without recording anything, so `&$error_message` came back
/// empty and the warning read `()`. PHP reports `Address already in use` for exactly this, and it
/// is the difference between a script that logs why it could not start and one that logs nothing.
#[test]
fn test_ipv6_server_reports_why_the_bind_failed() {
    let out = compile_and_run_capture(
        r#"<?php
$held = stream_socket_server("tcp://[::1]:54897");
echo is_resource($held) ? "held|" : "hold_failed|";
$e = 0;
$m = "";
$dup = @stream_socket_server("tcp://[::1]:54897", $e, $m);
echo ($dup === false ? "false" : "resource"), "|", $m;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "held|false|Address already in use");
}

/// Verifies compiled PHP output for stream socket get name ipv6.
#[test]
fn test_stream_socket_get_name_ipv6() {
    // stream_socket_get_name on an AF_INET6 socket should surface the peer
    // as `[ipv6]:port`. The local server's bound port is deterministic; the
    // client's source port is ephemeral, so check that the result starts
    // with the bracketed IPv6 prefix.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("tcp://[::1]:54938");
echo stream_socket_get_name($srv, false) . "\n";
$cli = stream_socket_client("tcp://[::1]:54938");
echo stream_socket_get_name($cli, true) . "\n";
echo substr(stream_socket_get_name($cli, false), 0, 5);
"#,
    );
    assert_eq!(out, "[::1]:54938\n[::1]:54938\n[::1]");
}

/// Verifies compiled PHP output for stream socket client ipv6 literal roundtrip.
#[test]
fn test_stream_socket_client_ipv6_literal_roundtrip() {
    // tcp://[::1]:port routes through the IPv6 dispatch: __rt_inet6_pton
    // parses the bracketed literal, the helper builds a sockaddr_in6, and
    // connects via AF_INET6. The Rust-side listener binds to ::1 so we
    // exercise the full IPv6 socket stack without any DNS dependency.
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("[::1]:54936")
        .expect("ipv6 test: bind [::1]:54936");
    let handle = std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().expect("ipv6 test: accept");
        let mut buf = [0u8; 4];
        sock.read_exact(&mut buf).expect("ipv6 test: read");
        sock.write_all(b"PONG").expect("ipv6 test: write");
        buf
    });
    let out = compile_and_run(
        r#"<?php
$cli = stream_socket_client("tcp://[::1]:54936");
echo is_resource($cli) ? "ok|" : "fail|";
fwrite($cli, "PING");
echo fread($cli, 4);
"#,
    );
    let read_buf = handle.join().expect("ipv6 test: join");
    assert_eq!(&read_buf, b"PING");
    assert_eq!(out, "ok|PONG");
}

/// Verifies compiled PHP output for stream socket client unresolvable host is false.
#[test]
fn test_stream_socket_client_unresolvable_host_is_false() {
    // An unresolvable host fails the connection like any bad address.
    let out = compile_and_run(
        r#"<?php $c = stream_socket_client("tcp://no-such-host.invalid:1234"); echo is_bool($c) ? "false" : "resource";"#,
    );
    assert_eq!(out, "false");
}

/// Verifies compiled PHP output for stream socket pair unsupported domain is false.
#[test]
fn test_stream_socket_pair_unsupported_domain_is_false() {
    // socketpair() refuses STREAM_PF_INET on every platform we target.
    // PHP's contract is `array|false`, so the return must be strictly
    // false (not an empty array) for === comparisons to work.
    let out = compile_and_run(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_INET, STREAM_SOCK_STREAM, 0);
echo gettype($pair);
echo "|";
echo ($pair === false) ? "strict_false" : "not_false";
"#,
    );
    assert_eq!(out, "boolean|strict_false");
}

/// Verifies compiled PHP output for stream socket pair round trip.
#[test]
fn test_stream_socket_pair_round_trip() {
    // Also a regression test for indexed reads of an array<resource>:
    // $pair[0] / $pair[1] must yield the stored descriptors, not the index.
    let out = compile_and_run(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
echo count($pair);
echo "|";
fwrite($pair[0], "ping");
echo fread($pair[1], 16);
echo "|";
fwrite($pair[1], "pong");
echo fread($pair[0], 16);
"#,
    );
    assert_eq!(out, "2|ping|pong");
}

/// Verifies socket-pair elements own opaque registry handles after the result array is released.
#[test]
fn test_stream_socket_pair_handles_survive_result_array_release() {
    let out = compile_and_run(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$left = $pair[0];
$right = $pair[1];
$distinct = get_resource_id($left) !== get_resource_id($right);
unset($pair);
echo get_resource_type($left) . "|" . get_resource_type($right) . "|";
echo $distinct ? "distinct|" : "same|";
fwrite($left, "owned");
echo fread($right, 5);
"#,
    );
    assert_eq!(out, "stream|stream|distinct|owned");
}

/// Verifies compiled PHP output for stream socket get name udp.
#[test]
fn test_stream_socket_get_name_udp() {
    // Phase 5 audit: stream_socket_get_name on a UDP socket must format the
    // bound address as A.B.C.D:port, just like the TCP case. Both the local
    // (server) and peer (client) sides should report the bound port.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("udp://127.0.0.1:54928", $e, $m, STREAM_SERVER_BIND);
echo stream_socket_get_name($srv, false);
echo "|";
$cli = stream_socket_client("udp://127.0.0.1:54928");
echo stream_socket_get_name($cli, true);
"#,
    );
    assert_eq!(out, "127.0.0.1:54928|127.0.0.1:54928");
}

/// Verifies compiled PHP output for stream socket get name unix.
#[test]
fn test_stream_socket_get_name_unix() {
    // Phase 5 audit: stream_socket_get_name on a Unix-domain socket must
    // surface the filesystem path, not garbage parsed out of a sockaddr_in.
    // Use a process-unique path so parallel tests do not collide.
    let out = compile_and_run(
        r#"<?php
$path = "/tmp/elephc_unix_getname_test.sock";
unlink($path);
$srv = stream_socket_server("unix://" . $path);
echo stream_socket_get_name($srv, false);
unlink($path);
"#,
    );
    assert_eq!(out, "/tmp/elephc_unix_getname_test.sock");
}

/// Verifies compiled PHP output for popen read mode.
#[test]
fn test_popen_read_mode() {
    let out = compile_and_run(
        r#"<?php
$p = popen("printf abc", "r");
echo fread($p, 16);
echo "|";
echo pclose($p);
"#,
    );
    assert_eq!(out, "abc|0");
}

/// Verifies compiled PHP output for opendir readdir iterates directory.
#[test]
fn test_opendir_readdir_iterates_directory() {
    let out = compile_and_run(
        r#"<?php
mkdir("sub");
file_put_contents("sub/alpha.txt", "a");
$d = opendir("sub");
$count = 0;
$found = 0;
while (($e = readdir($d)) !== false) {
    $count = $count + 1;
    if ($e === "alpha.txt") { $found = 1; }
}
closedir($d);
echo $count . ":" . $found;
"#,
    );
    assert_eq!(out, "3:1");
}

/// Verifies compiled PHP output for opendir invalid path returns false.
#[test]
fn test_opendir_invalid_path_returns_false() {
    let out = compile_and_run(
        r#"<?php
var_dump(opendir("/nonexistent/path/elephc-xyz") === false);
"#,
    );
    assert_eq!(out, "bool(true)\n");
}

/// Verifies compiled PHP output for readdir returns false at end of directory.
#[test]
fn test_readdir_returns_false_at_end_of_directory() {
    let out = compile_and_run(
        r#"<?php
mkdir("ed");
$d = opendir("ed");
$a = readdir($d);
$b = readdir($d);
$x = readdir($d);
closedir($d);
echo (is_string($a) ? "s" : "?");
echo (is_string($b) ? "s" : "?");
echo ($x === false ? "F" : "?");
"#,
    );
    assert_eq!(out, "ssF");
}

/// Verifies compiled PHP output for rewinddir restarts iteration.
#[test]
fn test_rewinddir_restarts_iteration() {
    let out = compile_and_run(
        r#"<?php
mkdir("rd");
$d = opendir("rd");
$first = readdir($d);
readdir($d);
$end = readdir($d);
rewinddir($d);
$again = readdir($d);
closedir($d);
echo ($end === false ? "1" : "0");
echo ($again === $first ? "1" : "0");
"#,
    );
    assert_eq!(out, "11");
}

/// Verifies `closedir` invalidates the old PHP resource while a new handle remains usable.
#[test]
fn test_closedir_allows_directory_handle_reuse() {
    let out = compile_and_run(
        r#"<?php
mkdir("cd");
$d1 = opendir("cd");
closedir($d1);
$d2 = opendir("cd");
$e = readdir($d2);
closedir($d2);
echo (is_resource($d2) ? "r" : "?");
echo (is_string($e) ? "ok" : "no");
"#,
    );
    assert_eq!(out, "?ok");
}

/// Verifies compiled PHP output for array literal of resources round trips.
#[test]
fn test_array_literal_of_resources_round_trips() {
    let out = compile_and_run(
        r#"<?php
$arr = [STDIN, STDOUT, STDERR];
echo $arr[0] . "|" . $arr[1] . "|" . $arr[2];
"#,
    );
    assert_eq!(out, "Resource id #1|Resource id #2|Resource id #3");
}

/// Verifies associative array literals preserve resource value metadata.
#[test]
fn test_assoc_array_literal_of_resources_round_trips() {
    let out = compile_and_run(
        r#"<?php
$arr = ["in" => STDIN, "out" => STDOUT, "err" => STDERR];
echo $arr["in"] . "|" . $arr["out"] . "|" . $arr["err"];
"#,
    );
    assert_eq!(out, "Resource id #1|Resource id #2|Resource id #3");
}

/// Verifies compiled PHP output for stream get meta data describes file stream.
#[test]
fn test_stream_get_meta_data_describes_file_stream() {
    let out = compile_and_run(
        r#"<?php
$f = fopen("meta.txt", "w");
$m = stream_get_meta_data($f);
echo "mode=" . $m["mode"];
echo " seekable=" . ($m["seekable"] ? "1" : "0");
echo " eof=" . ($m["eof"] ? "1" : "0");
echo " type=" . $m["stream_type"];
echo " wrap=" . $m["wrapper_type"];
echo " blocked=" . ($m["blocked"] ? "1" : "0");
echo " unread=" . $m["unread_bytes"];
echo " timed_out=" . ($m["timed_out"] ? "1" : "0");
fclose($f);
"#,
    );
    assert_eq!(
        out,
        "mode=w seekable=1 eof=0 type=STDIO wrap=plainfile blocked=1 unread=0 timed_out=0"
    );
}

/// Verifies the `data:` wrapper reports PHP's name for it, `RFC2397`.
///
/// elephc answered `data` — the scheme, not the wrapper. Reference PHP 8.5.6 names it
/// after the RFC that defines `data:` URLs, and a program branching on `wrapper_type`
/// (as PSR-7 and Flysystem adapters do) saw a name that exists nowhere in PHP.
#[test]
fn test_stream_get_meta_data_names_the_data_wrapper_rfc2397() {
    let out = compile_and_run(
        r#"<?php
$d = fopen("data://text/plain,hi", "r");
echo stream_get_meta_data($d)["wrapper_type"];
"#,
    );
    assert_eq!(out, "RFC2397");
}

/// Verifies compiled PHP output for stream get meta data reports eof consistently with feof.
#[test]
fn test_stream_get_meta_data_reports_eof_consistently_with_feof() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("meta2.txt", "ab");
$f = fopen("meta2.txt", "r");
fread($f, 10);
fread($f, 10);
$m = stream_get_meta_data($f);
echo ($m["eof"] ? "eof" : "no");
echo ":";
echo ($m["eof"] === feof($f) ? "consistent" : "differ");
fclose($f);
"#,
    );
    assert_eq!(out, "eof:consistent");
}

/// Verifies compiled PHP output for readdir loop collects results into array.
#[test]
fn test_readdir_loop_collects_results_into_array() {
    // Regression: appending a string|false value to an array inside a loop
    // re-ran the indexed-to-mixed conversion every iteration, corrupting the
    // already-boxed earlier elements.
    let out = compile_and_run(
        r#"<?php
mkdir("collectdir");
file_put_contents("collectdir/x.txt", "1");
$d = opendir("collectdir");
$names = [];
while (($e = readdir($d)) !== false) { $names[] = $e; }
closedir($d);
echo count($names);
echo is_string($names[0]) ? "s" : "?";
echo is_string($names[1]) ? "s" : "?";
echo is_string($names[2]) ? "s" : "?";
"#,
    );
    assert_eq!(out, "3sss");
}

/// Verifies compiled PHP output for stream select detects ready socket.
#[test]
fn test_stream_select_detects_ready_socket() {
    let out = compile_and_run(
        r#"<?php
$pair = stream_socket_pair(1, 1, 0);
$a = $pair[0];
$b = $pair[1];
fwrite($a, "ping");
$r1 = [$b]; $w1 = []; $e1 = [];
$n1 = stream_select($r1, $w1, $e1, 0, 0);
$r2 = [$a]; $w2 = []; $e2 = [];
$n2 = stream_select($r2, $w2, $e2, 0, 0);
echo "n1=" . $n1 . " r1=" . count($r1) . " n2=" . $n2 . " r2=" . count($r2);
"#,
    );
    assert_eq!(out, "n1=1 r1=1 n2=0 r2=0");
}

/// Verifies compiled PHP output for stream select compacts to ready subset.
#[test]
fn test_stream_select_compacts_to_ready_subset() {
    let out = compile_and_run(
        r#"<?php
$p1 = stream_socket_pair(1, 1, 0);
$p2 = stream_socket_pair(1, 1, 0);
fwrite($p1[0], "x");
$r = [$p1[1], $p2[1]];
$w = [];
$e = [];
$n = stream_select($r, $w, $e, 0, 0);
echo $n . ":" . count($r);
"#,
    );
    assert_eq!(out, "1:1");
}

/// Verifies compiled PHP output for stream bucket append then pop in order.
#[test]
fn test_stream_bucket_append_then_pop_in_order() {
    // Phase 11 B4 v2: stream_bucket_append actually appends to the
    // brigade's _buckets indexed-array property; stream_bucket_make_writeable
    // actually pops the head. With three appends and three pops in a row
    // we should observe FIFO order matching what PHP's bucket brigade
    // semantics guarantee.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
$brigade = new stdClass();
stream_bucket_append($brigade, stream_bucket_new($m, "alpha"));
stream_bucket_append($brigade, stream_bucket_new($m, "beta"));
stream_bucket_append($brigade, stream_bucket_new($m, "gamma"));
while (true) {
    $b = stream_bucket_make_writeable($brigade);
    if (is_null($b)) break;
    echo "[" . $b->data . "]";
}
echo "|done";
fclose($m);
"#,
    );
    assert_eq!(out, "[alpha][beta][gamma]|done");
}

/// Verifies prepend order and brigade growth beyond the initial bucket-array capacity.
#[test]
fn test_stream_bucket_prepend_then_pop_in_reverse_insertion_order() {
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
$brigade = new stdClass();
stream_bucket_prepend($brigade, stream_bucket_new($m, "alpha"));
stream_bucket_prepend($brigade, stream_bucket_new($m, "beta"));
stream_bucket_prepend($brigade, stream_bucket_new($m, "gamma"));
stream_bucket_prepend($brigade, stream_bucket_new($m, "delta"));
stream_bucket_prepend($brigade, stream_bucket_new($m, "epsilon"));
stream_bucket_prepend($brigade, stream_bucket_new($m, "zeta"));
while (true) {
    $b = stream_bucket_make_writeable($brigade);
    if (is_null($b)) break;
    echo "[" . $b->data . "]";
}
echo "|done";
fclose($m);
"#,
    );
    assert_eq!(out, "[zeta][epsilon][delta][gamma][beta][alpha]|done");
}

/// Verifies compiled PHP output for user filter 4arg brigade dispatch.
#[test]
fn test_user_filter_4arg_brigade_dispatch() {
    // Phase 11 B4 v2: when a user filter class's filter() method has 4
    // parameters, the runtime dispatcher seeds an input brigade with one
    // bucket (the just-read stream bytes), calls
    // `filter($in, $out, &$consumed, $closing)`, then walks the output
    // brigade's `_buckets` indexed-array and concatenates each
    // `$bucket->data` string into the post-filter buffer.
    //
    // Simplest end-to-end check: a "pass-through" filter that pops the
    // input bucket and appends it to the output brigade. The fread()
    // result is the original file bytes routed through the brigade
    // pipeline.
    let out = compile_and_run(
        r#"<?php
class PassThrough {
    public function filter($in, $out, $consumed, $closing): int {
        $b = stream_bucket_make_writeable($in);
        stream_bucket_append($out, $b);
        return 2;  // PSFS_PASS_ON
    }
}
stream_filter_register("pass.test", "PassThrough");
$path = tempnam(sys_get_temp_dir(), "elephc_brigade_e2e_");
file_put_contents($path, "hello brigade");
$f = fopen($path, "r");
stream_filter_append($f, "pass.test");
$content = fread($f, 64);
echo $content;
fclose($f);
unlink($path);
"#,
    );
    assert_eq!(out, "hello brigade");
}

/// Verifies compiled PHP output for user filter 4arg brigade transforms via while loop.
#[test]
fn test_user_filter_4arg_brigade_transforms_via_while_loop() {
    // Regression for two pre-existing Mixed bugs that blocked the canonical
    // PHP brigade-filter idiom (both fixed alongside this test):
    //   1. `while ($b = stream_bucket_make_writeable($in))` — the loop
    //      condition evaluates a Mixed(object) for truthiness;
    //      __rt_mixed_cast_bool used to treat tag-6 (object) as falsy, so the
    //      loop body never ran.
    //   2. `strtoupper($b->data)` — strtoupper/strtolower read a Mixed operand
    //      via a bare emit_expr and left a boxed cell in x0 with stale string
    //      registers, yielding an empty result; they now route through
    //      emit_string_arg (coerce_to_string → __rt_mixed_cast_string).
    // Together they make a transforming 4-arg brigade filter round-trip.
    let out = compile_and_run(
        r#"<?php
class UpBrigade {
    public $context;
    public function filter($in, $out, &$consumed, $closing): int {
        while ($b = stream_bucket_make_writeable($in)) {
            $b->data = strtoupper($b->data);
            $consumed += $b->datalen;
            stream_bucket_append($out, $b);
        }
        return PSFS_PASS_ON;
    }
}
stream_filter_register("up.brigade", "UpBrigade");
$w = fopen("php://temp", "w+");
stream_filter_append($w, "up.brigade", STREAM_FILTER_WRITE);
fwrite($w, "hello brigade");
rewind($w);
echo fread($w, 64);
"#,
    );
    assert_eq!(out, "HELLO BRIGADE");
}

/// Verifies a user filter returning PSFS_ERR_FATAL yields an empty read result.
#[test]
fn test_user_filter_psfs_err_fatal() {
    let out = compile_and_run(
        r#"<?php
class FatalFilter extends php_user_filter {
    public function filter($in, $out, &$consumed, $closing): int {
        return PSFS_ERR_FATAL;
    }
}
stream_filter_register("fatal", "FatalFilter");
$f = fopen("php://memory", "r+");
fwrite($f, "hello\n");
rewind($f);
stream_filter_append($f, "fatal");
$r = fread($f, 100);
echo "len=" . strlen($r) . "|";
"#,
    );
    assert_eq!(out, "len=0|");
}

/// Verifies a user filter that only ever answers `PSFS_FEED_ME` yields NOTHING.
///
/// This fixture used to assert `"hello\n"` — it pinned the defect. `PSFS_FEED_ME` means the
/// filter took the input and has no output yet, so passing the input through handed the caller
/// raw, unfiltered bytes. Measured against php 8.5.6, which answers the empty string here (plus
/// a "Unprocessed filter buckets remaining on input brigade" warning elephc does not emit).
#[test]
fn test_user_filter_psfs_feed_me() {
    let out = compile_and_run(
        r#"<?php
class FeedMeFilter extends php_user_filter {
    public function filter($in, $out, &$consumed, $closing): int {
        return PSFS_FEED_ME;
    }
}
stream_filter_register("feedme", "FeedMeFilter");
$f = fopen("php://memory", "r+");
fwrite($f, "hello\n");
rewind($f);
stream_filter_append($f, "feedme");
$r = fread($f, 100);
echo "len=", strlen($r);
"#,
    );
    assert_eq!(out, "len=0");
}

/// Verifies a user filter returning PSFS_PASS_ON transforms the output (control).
#[test]
fn test_user_filter_psfs_pass_on_control() {
    let out = compile_and_run(
        r#"<?php
class UpperFilter extends php_user_filter {
    public function filter($in, $out, &$consumed, $closing): int {
        while ($b = stream_bucket_make_writeable($in)) {
            $b->data = strtoupper($b->data);
            stream_bucket_append($out, $b);
        }
        return PSFS_PASS_ON;
    }
}
stream_filter_register("upper", "UpperFilter");
$f = fopen("php://memory", "r+");
fwrite($f, "hello\n");
rewind($f);
stream_filter_append($f, "upper");
echo fread($f, 100);
"#,
    );
    assert_eq!(out, "HELLO\n");
}

/// Verifies compiled PHP output for mixed object is truthy.
#[test]
fn test_mixed_object_is_truthy() {
    // Regression: a Mixed cell holding an object (tag 6) must be truthy in a
    // boolean context, matching PHP. __rt_mixed_cast_bool previously fell
    // through to the falsy default for tag 6 (only int/string/float/bool/
    // array/resource were handled). A Mixed(null) stays falsy.
    let out = compile_and_run(
        r#"<?php
class C { public $x = 1; }
function mk(): mixed { return new C(); }
function nope(): mixed { return null; }
$o = mk();
echo ($o ? "obj-truthy" : "obj-falsy");
$n = nope();
echo ($n ? "|null-truthy" : "|null-falsy");
"#,
    );
    assert_eq!(out, "obj-truthy|null-falsy");
}

/// Verifies compiled PHP output for fopen http content emits content length header.
#[test]
#[ignore = "test is reliable standalone but flakes in parallel sweep (port-binding race); the underlying Content-Length emission is verified by ad-hoc Ruby + standalone elephc runs — see the http_build_request.rs commit body for the reproduction"]
fn test_fopen_http_content_emits_content_length_header() {
    // Phase 11 B2 polish: when $ctx['http']['content'] is set, the request
    // line carries a `Content-Length: <N>\r\n` header so the receiving
    // server knows how many body bytes to read. (The earlier B2 commit
    // landed the body append but left the Content-Length emission stubbed
    // with a TEMPORARILY-DISABLED branch on ARM64; this verifies the
    // re-enabled path puts the right bytes on the wire.)
    let (_server, port) = spawn_http_echo_server();
    let out = compile_and_run(
        &r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "method", "POST");
stream_context_set_option(stream_context_get_default(), "http", "content", "hello body");
$f = fopen("http://127.0.0.1:PHP_TEST_PORT/", "r");
$req = stream_get_contents($f);
fclose($f);
// The echo server replays the request headers (bytes up to the blank
// line) as the response body. Substr-based search instead of strpos
// to dodge any `!== false` quirks on Mixed return values.
$found = false;
$needle = "Content-Length: 10";
$nlen = strlen($needle);
for ($i = 0; $i + $nlen <= strlen($req); $i++) {
    if (substr($req, $i, $nlen) === $needle) { $found = true; break; }
}
echo $found ? "ok" : "MISS:" . strlen($req);
"#
        .replace("PHP_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for stream context set default returns resource.
#[test]
fn test_stream_context_set_default_returns_resource() {
    let out = compile_and_run(
        r#"<?php
$r = stream_context_set_default(["http" => ["method" => "POST"]]);
echo is_resource($r) ? "resource" : "no";
"#,
    );
    assert_eq!(out, "resource");
}

/// Verifies compiled PHP output for stream context set params returns true.
#[test]
fn test_stream_context_set_params_returns_true() {
    let out = compile_and_run(
        r#"<?php
$ctx = stream_context_create();
echo stream_context_set_params($ctx, []) ? "ok" : "FAIL";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for stream resolve include path existing and missing.
#[test]
fn test_stream_resolve_include_path_existing_and_missing() {
    let out = compile_and_run(
        r#"<?php
$r = stream_resolve_include_path("/tmp");
$miss = stream_resolve_include_path("/non/existent/xyz");
echo (is_string($r) ? "s" : "n") . "|" . ($miss === false ? "f" : "x");
"#,
    );
    assert_eq!(out, "s|f");
}

/// Verifies compiled PHP output for fopen http user agent in request.
#[test]
fn test_fopen_http_user_agent_in_request() {
    let (_server, port) = spawn_http_echo_server();
    let out = compile_and_run(
        &r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "user_agent", "MyApp/2.0");
$f = fopen("http://127.0.0.1:PHP_TEST_PORT/", "r");
$req = stream_get_contents($f);
fclose($f);
$needle = "User-Agent: MyApp/2.0";
$nlen = strlen($needle);
$found = false;
for ($i = 0; $i + $nlen <= strlen($req); $i++) {
    if (substr($req, $i, $nlen) === $needle) { $found = true; break; }
}
echo $found ? "ok" : "MISS";
"#
        .replace("PHP_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for fopen http protocol version 1 1.
#[test]
fn test_fopen_http_protocol_version_1_1() {
    let (_server, port) = spawn_http_echo_server();
    let out = compile_and_run(
        &r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "protocol_version", "1.1");
$f = fopen("http://127.0.0.1:PHP_TEST_PORT/", "r");
$req = stream_get_contents($f);
fclose($f);
$needle = "HTTP/1.1";
$nlen = strlen($needle);
$found = false;
for ($i = 0; $i + $nlen <= strlen($req); $i++) {
    if (substr($req, $i, $nlen) === $needle) { $found = true; break; }
}
echo $found ? "ok" : "MISS";
"#
        .replace("PHP_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for fopen php fd n writes to descriptor.
#[test]
fn test_fopen_php_fd_n_writes_to_descriptor() {
    let out = compile_and_run(
        r#"<?php
$f = fopen("php://fd/1", "w");
fwrite($f, "fd-route");
fclose($f);
"#,
    );
    assert_eq!(out, "fd-route");
}

/// Verifies compiled PHP output for fopen http request fulluri in request line.
#[test]
fn test_fopen_http_request_fulluri_in_request_line() {
    let (_server, port) = spawn_http_echo_server();
    let out = compile_and_run(
        &r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "request_fulluri", "1");
$f = fopen("http://127.0.0.1:PHP_TEST_PORT/path", "r");
$req = stream_get_contents($f);
fclose($f);
$needle = "GET http://127.0.0.1:PHP_TEST_PORT/path HTTP/1.0";
$nlen = strlen($needle);
$found = false;
for ($i = 0; $i + $nlen <= strlen($req); $i++) {
    if (substr($req, $i, $nlen) === $needle) { $found = true; break; }
}
echo $found ? "ok" : "MISS";
"#
        .replace("PHP_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "ok");
}

/// Verifies opendir()/readdir()/rewinddir()/closedir() on a registered userspace
/// wrapper dispatch to dir_opendir/dir_readdir/dir_rewinddir/dir_closedir (vtable
/// slots 19-22) through a synthetic handle fd, with object state (the read
/// cursor) persisting across the readdir() calls and surviving a rewinddir().
#[test]
fn test_opendir_readdir_wrapper_dispatch() {
    let out = compile_and_run(
        r#"<?php
class MyDir {
    public $context;
    public $pos = 0;
    public function dir_opendir($path, $options): bool {
        $this->pos = 0;
        return true;
    }
    public function dir_readdir(): string {
        $names = ["a.txt", "b.txt"];
        if ($this->pos >= 2) {
            return "";
        }
        $n = $names[$this->pos];
        $this->pos = $this->pos + 1;
        return $n;
    }
    public function dir_rewinddir(): bool {
        $this->pos = 0;
        return true;
    }
    public function dir_closedir(): bool {
        echo "closed\n";
        return true;
    }
}
stream_wrapper_register("mydir", "MyDir");
$dh = opendir("mydir://x");
while (($f = readdir($dh)) !== false) {
    echo "$f\n";
}
rewinddir($dh);
$g = readdir($dh);
echo "rewound:$g\n";
closedir($dh);
echo "done\n";
"#,
    );
    assert_eq!(out, "a.txt\nb.txt\nrewound:a.txt\nclosed\ndone\n");
}

/// A registered wrapper that does not implement dir_opendir makes opendir()
/// return false (the matched-but-failed path) rather than a directory handle.
#[test]
fn test_opendir_wrapper_without_dir_opendir_returns_false() {
    let out = compile_and_run(
        r#"<?php
class NoDir {
    public $context;
    public function stream_open($path, $mode, $options, &$opened): bool {
        return true;
    }
}
stream_wrapper_register("ndir", "NoDir");
$dh = opendir("ndir://x");
if ($dh === false) {
    echo "false\n";
} else {
    echo "opened\n";
}
"#,
    );
    assert_eq!(out, "false\n");
}

/// chown()/chgrp() with an integer uid/gid on a registered userspace wrapper
/// dispatch to the wrapper's stream_metadata($path, STREAM_META_OWNER/GROUP,
/// $value) (vtable slot 14) instead of libc chown(2).
#[test]
fn test_chown_chgrp_int_wrapper_dispatch() {
    let out = compile_and_run(
        r#"<?php
class MetaWrapper {
    public $context;
    public function stream_metadata(string $path, int $option, mixed $value): bool {
        echo "meta:" . $option . ":" . $value . "\n";
        return true;
    }
}
stream_wrapper_register("metaw", "MetaWrapper");
$a = chown("metaw://x", 1000);
$b = chgrp("metaw://y", 2000);
echo ($a ? "ok" : "no") . "\n";
echo ($b ? "ok" : "no") . "\n";
"#,
    );
    assert_eq!(out, "meta:3:1000\nmeta:5:2000\nok\nok\n");
}

/// chown()/chgrp() with a STRING user/group name on a registered userspace wrapper
/// dispatch to stream_metadata($path, STREAM_META_OWNER_NAME/GROUP_NAME, $value)
/// (vtable slot 14) with the name boxed as a mixed value, instead of libc
/// getpwnam/getgrnam. A non-wrapper path keeps the libc name-resolving helpers.
#[test]
fn test_chown_chgrp_name_wrapper_dispatch() {
    let out = compile_and_run(
        r#"<?php
class NameWrapper {
    public $context;
    public function stream_metadata(string $path, int $option, mixed $value): bool {
        echo "meta:" . $option . ":" . $value . "\n";
        return true;
    }
}
stream_wrapper_register("namew", "NameWrapper");
$a = chown("namew://x", "www-data");
$b = chgrp("namew://y", "staff");
echo ($a ? "ok" : "no") . "\n";
echo ($b ? "ok" : "no") . "\n";
"#,
    );
    assert_eq!(out, "meta:2:www-data\nmeta:4:staff\nok\nok\n");
}

/// touch() on a registered userspace wrapper dispatches to
/// stream_metadata($path, STREAM_META_TOUCH, [mtime, atime]); the value is a
/// 2-element int array. A non-wrapper path keeps libc touch.
#[test]
fn test_touch_wrapper_dispatch() {
    let out = compile_and_run(
        r#"<?php
class TouchW {
    public $context;
    public function stream_metadata(string $path, int $option, mixed $value): bool {
        echo "opt=" . $option . " n=" . count($value) . " m=" . $value[0] . " a=" . $value[1] . "\n";
        return true;
    }
}
stream_wrapper_register("touchw", "TouchW");
$r = touch("touchw://f", 100, 200);
echo ($r ? "ok" : "no") . "\n";
"#,
    );
    assert_eq!(out, "opt=1 n=2 m=100 a=200\nok\n");
}

/// stream_metadata() declared the way the MANUAL shows it — with no type hints —
/// must still receive its arguments intact.
///
/// The contract-seeding table covered every hook that takes parameters except
/// this one, so an untyped `$path` kept the `Int` an untyped parameter is seeded
/// with. `Int` occupies ONE register, but the caller hands a string as a
/// (ptr,len) PAIR, so `$path` swallowed the pointer alone and every later
/// argument slid down one slot — `$option` read the length, `$value` read the
/// option. Measured against php 8.5.6, which prints the values below.
#[test]
fn test_untyped_stream_metadata_receives_its_arguments() {
    let out = compile_and_run(
        r#"<?php
class UntypedMetaW {
    public $context;
    public function stream_open($path, $mode, $options, &$opened) { $opened = $path; return true; }
    public function stream_metadata($path, $option, $value) {
        echo "p=" . $path . " o=" . $option . " t=" . gettype($value) . "\n";
        return true;
    }
}
stream_wrapper_register("umw", "UntypedMetaW");
echo chmod("umw://f", 0644) ? "1" : "0";
echo "\n";
$r = touch("umw://g", 100, 200);
echo ($r ? "ok" : "no") . "\n";
"#,
    );
    assert_eq!(out, "p=umw://f o=6 t=integer\n1\np=umw://g o=1 t=array\nok\n");
}

/// A wrapper that serves ONLY path operations still gets the wrapper ABI.
///
/// Both gates that ask "is this a wrapper?" — the checker's contract seeding and
/// the EIR normalizer — used `stream_open` as the marker. A wrapper reached only
/// through `chmod()`/`touch()` never declares `stream_open`, so it failed both:
/// the body was normalized to boxed Mixed while the runtime dispatcher kept
/// handing it a raw (ptr,len) pair, and the program SEGFAULTED. Same php output
/// as the typed form.
#[test]
fn test_path_only_wrapper_without_stream_open_seeds_its_contract() {
    let out = compile_and_run(
        r#"<?php
class PathOnlyW {
    public $context;
    public function stream_metadata($path, $option, $value) {
        echo "p=" . $path . " o=" . $option . " t=" . gettype($value) . "\n";
        return true;
    }
}
stream_wrapper_register("pow", "PathOnlyW");
echo chmod("pow://f", 0644) ? "1" : "0";
echo "\n";
$r = touch("pow://g", 100, 200);
echo ($r ? "ok" : "no") . "\n";
"#,
    );
    assert_eq!(out, "p=pow://f o=6 t=integer\n1\np=pow://g o=1 t=array\nok\n");
}

/// `stream_set_write_buffer()`/`stream_set_read_buffer()` reach the wrapper's `stream_set_option()`.
///
/// Both were lowered as a no-op that always returned 0, so a userspace wrapper never learned the
/// buffer changed and every stream claimed success. Measured on php 8.5.6: `$option` is 3 for write
/// and 2 for read, `$arg1` is `PHP_STREAM_BUFFER_NONE` (0) for size 0 and `_FULL` (2) otherwise,
/// and `$arg2` is the size — except for size 0, where php substitutes the default chunk size 1024.
/// The call answers 0 when the hook returns true and -1 otherwise. A stream that is not a wrapper
/// never reaches the hook at all: `php://memory` answers -1 for write and 0 for read.
#[test]
fn test_stream_set_buffer_dispatches_to_the_wrapper_option_hook() {
    let out = compile_and_run(
        r#"<?php
class BufW {
    public $context;
    public function stream_open($p, $m, $o, &$op) { $op = $p; return true; }
    public function stream_set_option($option, $arg1, $arg2) {
        echo "opt=" . $option . " mode=" . $arg1 . " size=" . $arg2 . "\n";
        return $option === 3;
    }
}
stream_wrapper_register("buf", "BufW");
$f = fopen("buf://x", "r");
var_dump(stream_set_write_buffer($f, 0));
var_dump(stream_set_write_buffer($f, 512));
var_dump(stream_set_read_buffer($f, 0));
var_dump(stream_set_read_buffer($f, 256));
fclose($f);
$m = fopen("php://memory", "r+");
var_dump(stream_set_write_buffer($m, 0));
var_dump(stream_set_read_buffer($m, 0));
fclose($m);
"#,
    );
    assert_eq!(
        out,
        "opt=3 mode=0 size=1024\nint(0)\nopt=3 mode=2 size=512\nint(0)\n\
         opt=2 mode=0 size=1024\nint(-1)\nopt=2 mode=2 size=256\nint(-1)\nint(-1)\nint(0)\n"
    );
}

/// `fwrite()` on a user-filtered write stream answers with the filter's `&$consumed`.
///
/// Two bugs met here. The dispatcher handed `&$consumed` a Mixed CELL, but an untyped by-ref
/// parameter is an Int by-ref, so the method read the cell's tag word as its starting value —
/// which is 0 for an int, so it looked right — and then wrote its count straight over that tag.
/// And the filtered-write helper ignored the parameter regardless, always reporting the payload
/// length. Measured on php 8.5.6: a filter maintaining `$consumed` answers 10, one that assigns 7
/// answers 7, and one that never touches the parameter answers 0 even though the bytes were
/// written. Built-in filters are unaffected — they publish nothing, and php reports the payload
/// length for them, which is what the sentinel preserves.
#[test]
fn test_fwrite_reports_the_user_filters_consumed_count() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
class Maintains extends php_user_filter {
    function filter($in, $out, &$consumed, $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) {
            $consumed += $bucket->datalen;
            $bucket->data = strtoupper($bucket->data);
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }
}
class Assigns extends php_user_filter {
    function filter($in, $out, &$consumed, $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) { stream_bucket_append($out, $bucket); }
        $consumed = 7;
        return PSFS_PASS_ON;
    }
}
class Ignores extends php_user_filter {
    function filter($in, $out, &$consumed, $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) {
            $bucket->data = strtoupper($bucket->data);
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }
}
stream_filter_register("maintains", "Maintains");
stream_filter_register("assigns", "Assigns");
stream_filter_register("ignores", "Ignores");
foreach (["maintains", "assigns", "ignores"] as $name) {
    $f = fopen("out_$name", "w");
    stream_filter_append($f, $name, STREAM_FILTER_WRITE);
    var_dump(fwrite($f, "abcdefghij"));
    fclose($f);
    echo file_get_contents("out_$name"), "\n";
}
$b = fopen("out_builtin", "w");
stream_filter_append($b, "string.toupper", STREAM_FILTER_WRITE);
var_dump(fwrite($b, "abcdefghij"));
fclose($b);
"#,
    );
    assert_eq!(
        out,
        "int(10)\nABCDEFGHIJ\nint(7)\nabcdefghij\nint(0)\nABCDEFGHIJ\nint(10)\n"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// php warns, by name, for every `streamWrapper` hook the registered class does not implement.
///
/// elephc answered silently, and two of the answers were wrong as well: `fwrite()` reported 0
/// bytes where php reports false, and `fflush()` reported success where php reports false.
/// Every wording below was measured against php 8.5.6 on a wrapper declaring only `stream_open`;
/// note that `feof()` alone also says what php assumed instead. `@` still silences all of them.
///
/// `stream_read` is deliberately absent from this list: php 8.5.6 emits NO warning for a missing
/// `stream_read`, measured with `stream_eof` present so the read is genuinely attempted.
#[test]
fn test_missing_wrapper_hooks_warn_by_class_and_method() {
    let out = compile_and_run_capture(
        r#"<?php
class Bare {
    public $context;
    function stream_open($p, $m, $o, &$op) { $op = $p; return true; }
}
stream_wrapper_register("bare", "Bare");
$f = fopen("bare://x", "r+");
var_dump(fwrite($f, "abc"));
var_dump(feof($f));
var_dump(fstat($f));
var_dump(flock($f, LOCK_EX));
var_dump(fflush($f));
echo "--- suppressed ---\n";
var_dump(@fwrite($f, "abc"));
var_dump(@feof($f));
var_dump(@fstat($f));
var_dump(@flock($f, LOCK_EX));
fclose($f);
"#,
    );
    assert!(out.success, "the diagnostics must not disturb the program");
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(true)\nbool(false)\nbool(false)\nbool(false)\n\
         --- suppressed ---\nbool(false)\nbool(true)\nbool(false)\nbool(false)\n"
    );
    assert_eq!(
        out.diagnostics,
        "Warning: fwrite(): Bare::stream_write is not implemented!\n\
         Warning: feof(): Bare::stream_eof is not implemented! Assuming EOF\n\
         Warning: fstat(): Bare::stream_stat is not implemented!\n\
         Warning: flock(): Bare::stream_lock is not implemented!\n",
        "one line per missing hook, naming the class, and nothing from the suppressed calls"
    );
}

/// A wrapper missing `stream_read`/`url_stat` answers FALSE, not an empty string or -1.
///
/// Both were silent wrong values rather than missing diagnostics. `fread()` handed back `""`,
/// which reads as a successful empty read; php answers false. `filesize()` handed back the -1 the
/// field lookup uses as its sentinel, because a matched SCHEME was being treated as a successful
/// stat — a wrapper with no `url_stat()` matches the scheme and produces nothing. Measured on
/// php 8.5.6, including the two controls: a wrapper that does implement `url_stat()` keeps its
/// size, and an absent ordinary file keeps its own false.
#[test]
fn test_missing_read_and_stat_hooks_answer_false_not_a_value() {
    let out = compile_and_run(
        r#"<?php
class Bare {
    public $context;
    function stream_open($p, $m, $o, &$op) { $op = $p; return true; }
}
class HasStat {
    public $context;
    function stream_open($p, $m, $o, &$op) { $op = $p; return true; }
    function url_stat($path, $flags) { return ["size" => 42]; }
}
stream_wrapper_register("bare", "Bare");
stream_wrapper_register("hs", "HasStat");
var_dump(@filesize("bare://y"));
var_dump(@filesize("hs://y"));
var_dump(@filesize("/definitely/not/here"));
$f = fopen("bare://x", "r");
var_dump(fread($f, 5));
fclose($f);
"#,
    );
    assert_eq!(out, "bool(false)\nint(42)\nbool(false)\nbool(false)\n");
}

/// The path operations warn by class and method too, naming the BUILTIN that called them.
///
/// These share one runtime helper across unlink/rename/mkdir/rmdir and the whole
/// `stream_metadata` family, so the helper cannot know which builtin reached it — php names the
/// caller, and `chmod`/`touch`/`chown` all name `stream_metadata` rather than a method of their
/// own. Each wording measured against php 8.5.6.
#[test]
fn test_missing_wrapper_path_hooks_warn_naming_their_caller() {
    let out = compile_and_run_capture(
        r#"<?php
class Bare {
    public $context;
    function stream_open($p, $m, $o, &$op) { $op = $p; return true; }
}
stream_wrapper_register("bare", "Bare");
var_dump(unlink("bare://a"));
var_dump(rename("bare://a", "bare://b"));
var_dump(mkdir("bare://d"));
var_dump(rmdir("bare://d"));
var_dump(chmod("bare://a", 0644));
var_dump(touch("bare://a"));
var_dump(chown("bare://a", 501));
var_dump(@unlink("bare://a"));
"#,
    );
    assert!(out.success, "the diagnostics must not disturb the program");
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(false)\nbool(false)\nbool(false)\n\
         bool(false)\nbool(false)\nbool(false)\nbool(false)\n"
    );
    assert_eq!(
        out.diagnostics,
        "Warning: unlink(): Bare::unlink is not implemented!\n\
         Warning: rename(): Bare::rename is not implemented!\n\
         Warning: mkdir(): Bare::mkdir is not implemented!\n\
         Warning: rmdir(): Bare::rmdir is not implemented!\n\
         Warning: chmod(): Bare::stream_metadata is not implemented!\n\
         Warning: touch(): Bare::stream_metadata is not implemented!\n\
         Warning: chown(): Bare::stream_metadata is not implemented!\n",
        "the caller's own name on every line, and nothing from the suppressed call"
    );
}

/// The stat builtins warn when a wrapper has no `url_stat()`, and `filesize()` adds php's second line.
///
/// Measured on php 8.5.6: each caller names itself and the missing `url_stat`, and `filesize()`
/// alone follows with "stat failed for <path>" — which php prints for ANY failed stat, so an
/// absent ordinary file gets that line too while `is_file()`/`file_exists()` stay silent. A
/// wrapper that does implement `url_stat()` warns about nothing and keeps its answers.
#[test]
fn test_missing_url_stat_warns_and_filesize_adds_its_second_line() {
    let out = compile_and_run_capture(
        r#"<?php
class Bare {
    public $context;
    function stream_open($p, $m, $o, &$op) { $op = $p; return true; }
}
class HasStat {
    public $context;
    function stream_open($p, $m, $o, &$op) { $op = $p; return true; }
    function url_stat($path, $flags) { return ["size" => 42, "mode" => 0100644]; }
}
stream_wrapper_register("bare", "Bare");
stream_wrapper_register("hs", "HasStat");
var_dump(file_exists("bare://a"));
var_dump(filesize("bare://a"));
var_dump(is_file("bare://a"));
var_dump(@file_exists("bare://a"));
var_dump(file_exists("hs://a"));
var_dump(filesize("hs://a"));
var_dump(is_file("hs://a"));
var_dump(filesize("/definitely/not/here"));
var_dump(is_file("/definitely/not/here"));
"#,
    );
    assert!(out.success, "the diagnostics must not disturb the program");
    assert_eq!(
        out.stdout,
        "bool(false)\nbool(false)\nbool(false)\nbool(false)\n\
         bool(true)\nint(42)\nbool(true)\nbool(false)\nbool(false)\n"
    );
    assert_eq!(
        out.diagnostics,
        "Warning: file_exists(): Bare::url_stat is not implemented!\n\
         Warning: filesize(): Bare::url_stat is not implemented!\n\
         Warning: filesize(): stat failed for bare://a\n\
         Warning: is_file(): Bare::url_stat is not implemented!\n\
         Warning: filesize(): stat failed for /definitely/not/here\n",
        "the wrapper that implements url_stat warns about nothing, the suppressed call is silent, \
         and only filesize() reports the failed stat"
    );
}

/// The wrapper marker must NOT fire on a class that merely owns generic names.
///
/// `mkdir`/`rmdir`/`unlink`/`rename` are ordinary method names on ordinary
/// classes — Symfony's `Filesystem::mkdir($path, $mode)` is not a stream
/// wrapper. Seeding those on name alone would force the raw (ptr,len) ABI onto a
/// plain method call, so they take the wrapper contract only when the class also
/// declares one of the protocol's RESERVED names.
#[test]
fn test_generic_method_names_alone_do_not_make_a_wrapper() {
    let out = compile_and_run(
        r#"<?php
class Filesystem {
    public function mkdir($path, $mode) { return $path . "/" . $mode; }
    public function unlink($path) { return strtoupper($path); }
}
$fs = new Filesystem();
echo $fs->mkdir("a", 5), "\n";
echo $fs->unlink("b"), "\n";
"#,
    );
    assert_eq!(out, "a/5\nB\n");
}

/// Regression: two `stream_context_create` calls in one program must
/// assemble. The no-options clear path previously used a fixed
/// `scc_store_zero` label that was defined twice (once per call), so any
/// program creating more than one context failed to assemble.
#[test]
fn test_stream_context_create_twice_assembles() {
    let out = compile_and_run(
        r#"<?php
$a = stream_context_create([]);
$b = stream_context_create([]);
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// An explicitly supplied stream-context notifier fires STREAM_NOTIFY_CONNECT
/// (code 2) while opening a successful loopback HTTP stream.
#[test]
fn test_stream_notification_callback_fires_connect_for_explicit_context() {
    let (_server, port) = spawn_http_server(b"ok");
    let out = compile_and_run(
        &r#"<?php
$ctx = stream_context_create([], ['notification' => function($code, $sev, $msg, $mc, $bt, $bm) {
    if ($code === 2) echo "N" . $code . ";";
}]);
$f = fopen('http://127.0.0.1:PHP_TEST_PORT/', 'r', false, $ctx);
echo $f === false ? "closed" : "open";
fclose($f);
"#
        .replace("PHP_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "N2;open");
}

/// v1 captures only a literal closure / first-class-callable `notification`
/// value. A string function-name callback is not a callable descriptor (no
/// invoker at offset 56), so it is not fired and the global is cleared
/// instead; the refused open must still complete without crashing.
#[test]
fn test_stream_notification_string_callback_not_fired_in_v1() {
    let out = compile_and_run(
        r#"<?php
function my_notify($code) { echo "S" . $code; }
$ctx = stream_context_create([], ['notification' => 'my_notify']);
$f = fopen('http://127.0.0.1:1/', 'r', false, $ctx);
echo $f === false ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

/// An explicit empty context masks the request-default notification callback.
#[test]
fn test_stream_notification_empty_explicit_context_masks_default() {
    let (_server, port) = spawn_http_server(b"ok");
    let out = compile_and_run(
        &r#"<?php
$default = stream_context_get_default();
stream_context_set_params($default, ['notification' => function($code) {
    if ($code === 2) echo "default-fired";
}]);
$empty = stream_context_create([], ['other' => 1]);
$f = fopen('http://127.0.0.1:PHP_TEST_PORT/', 'r', false, $empty);
echo $f === false ? "bad" : "ok";
fclose($f);
"#
        .replace("PHP_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "ok");
}

/// `stream_context_set_params` updates the explicitly addressed context notifier.
#[test]
fn test_stream_notification_callback_via_set_params() {
    let (_server, port) = spawn_http_server(b"ok");
    let out = compile_and_run(
        &r#"<?php
$ctx = stream_context_create([]);
stream_context_set_params($ctx, ['notification' => function($code) {
    if ($code === 2) echo "P" . $code . ";";
}]);
$f = fopen('http://127.0.0.1:PHP_TEST_PORT/', 'r', false, $ctx);
echo $f === false ? "closed" : "open";
fclose($f);
"#
        .replace("PHP_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "P2;open");
}

/// A userspace wrapper whose `stream_cast()` (vtable slot 10) returns a real
/// underlying socket fd becomes select()-able: `stream_select` resolves the
/// synthetic wrapper fd to that real fd (STREAM_CAST_FOR_SELECT) and reports it
/// ready once data arrives. The wrapper connects to a same-process server
/// inside `stream_open`, and the server side writes to make it readable.
#[test]
fn test_stream_select_wrapper_stream_cast_detects_ready() {
    let out = compile_and_run(
        r#"<?php
class SockW {
    public $context;
    public $inner;
    public function stream_open($path, $mode, $options, &$opened): bool {
        $this->inner = stream_socket_client("tcp://127.0.0.1:55050");
        return $this->inner !== false;
    }
    public function stream_cast($cast_as) { return $this->inner; }
    public function stream_eof(): bool { return false; }
    public function stream_read(int $n): string { return ""; }
}
stream_wrapper_register("sockw", "SockW");
$srv = stream_socket_server("tcp://127.0.0.1:55050");
$w = fopen("sockw://x", "r");
$conn = stream_socket_accept($srv);
fwrite($conn, "ping");
$r = [$w]; $wr = []; $e = [];
$n = stream_select($r, $wr, $e, 1, 0);
echo "n=" . $n . " kept=" . count($r);
"#,
    );
    assert_eq!(out, "n=1 kept=1");
}

/// A resource keeps its PHP kind name when it travels through an untyped parameter.
///
/// `stream_context_create()` is statically `Resource`, so passing it to `mixed $r` boxes it
/// through the generic value boxer — which writes ownership marker 0. The registry lookup
/// only ran for markers 1/3/4/9, so a context answered `"stream"` while a filter (boxed by
/// the legacy fd path, marker 3) answered correctly. Same emitted code for both, so the
/// divergence was purely the marker. Oracle: php 8.5.6.
#[test]
fn test_resource_kind_name_survives_an_untyped_parameter() {
    let out = compile_and_run(
        r#"<?php
function kind($r) { return get_resource_type($r); }
function open_p($r) { return var_export(is_resource($r), true); }
$ctx = stream_context_create([]);
$f   = fopen("php://memory", "r+");
$fl  = stream_filter_append($f, "string.toupper", STREAM_FILTER_WRITE);
echo kind($ctx), "|", kind($fl), "|", kind($f), "|", open_p($ctx);
fclose($f);
echo "|", kind($f), "|", open_p($f);
"#,
    );
    assert_eq!(out, "stream-context|stream filter|stream|true|Unknown|false");
}

/// `stream_select()` must actually wait for its timeout.
///
/// The timeout arrives in caller-saved registers (x3/x4, rcx/r8) and the pollfd build
/// calls `__rt_stream_fd` for every entry, so the computed timeout was whatever those
/// registers happened to hold afterwards. On macOS that garbage rounded to zero and the
/// call returned instantly; on Linux it hit the "negative seconds means infinite" arm and
/// `poll(-1)` blocked forever, which is what timed this suite's wrapper test out at 60s.
/// The lower bound is deliberately loose — the bug produced 0 ms, not 190 ms.
#[test]
fn test_stream_select_waits_for_its_timeout() {
    let out = compile_and_run(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$r = [$pair[0]]; $w = []; $e = [];
$t0 = microtime(true);
$n = stream_select($r, $w, $e, 0, 200000);
$ms = (int) round((microtime(true) - $t0) * 1000);
echo "n=", var_export($n, true), " waited=", var_export($ms >= 150, true);
"#,
    );
    assert_eq!(out, "n=0 waited=true");
}

/// A userspace wrapper that does not implement `stream_cast` cannot be represented as a
/// select()-able descriptor, so it contributes nothing to the descriptor set.
///
/// This test used to assert `n=0 kept=0`, which is not php's answer: php counts the streams it
/// could cast and raises `ValueError: No stream arrays were passed` when that count is zero, so
/// the only entry here leaves it at zero and the call THROWS. Measured on `php -n` 8.5.6, which
/// also prints `NoCast::stream_cast is not implemented!` and `Cannot represent a stream of type
/// user-space as a select()able descriptor` first; both are asserted by
/// `test_stream_select_explains_an_uncastable_stream`, which reads the diagnostic channel this
/// test does not.
#[test]
fn test_stream_select_wrapper_without_stream_cast_excluded() {
    let out = compile_and_run(
        r#"<?php
class NoCast {
    public $context;
    public function stream_open($path, $mode, $options, &$opened): bool { return true; }
    public function stream_eof(): bool { return false; }
    public function stream_read(int $n): string { return ""; }
}
stream_wrapper_register("nocast", "NoCast");
$w = fopen("nocast://x", "r");
$r = [$w]; $wr = []; $e = [];
try { $n = stream_select($r, $wr, $e, 0, 0); echo "n=" . $n . " kept=" . count($r); }
catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(); }
"#,
    );
    assert_eq!(out, "ValueError: No stream arrays were passed");
}

/// Verifies `fread()` of a payload larger than the 64 KiB concat scratch buffer returns the whole
/// string AND leaves the stream-handle table intact, so the following `fclose()` still sees a
/// valid resource. Before the reservation fix the read ran past `_concat_buf` into the adjacent
/// BSS globals and `fclose()` failed with a bogus TypeError.
#[test]
fn test_fread_larger_than_concat_scratch_keeps_stream_table_intact() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$payload = str_repeat("0123456789", 10000);
file_put_contents("big_fread.bin", $payload);
$f = fopen("big_fread.bin", "r");
$data = fread($f, 100000);
echo strlen($data), "|", substr($data, 0, 5), "|", substr($data, -5), "|";
echo ($data === $payload ? "same" : "DIFF"), "|";
echo (is_resource($f) ? "res" : "broken"), "|";
fclose($f);
unlink("big_fread.bin");
echo "closed";
"#,
    );
    assert_eq!(out, "100000|01234|56789|same|res|closed");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `stream_get_contents()` drains a stream larger than the 64 KiB concat scratch buffer
/// into one contiguous, byte-exact result through the growable reservation.
#[test]
fn test_stream_get_contents_larger_than_concat_scratch() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$payload = str_repeat("abcdefghij", 10000);
file_put_contents("big_sgc.bin", $payload);
$f = fopen("big_sgc.bin", "r");
$data = stream_get_contents($f);
fclose($f);
unlink("big_sgc.bin");
echo strlen($data), "|", ($data === $payload ? "same" : "DIFF");
"#,
    );
    assert_eq!(out, "100000|same");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the bounded `stream_get_contents($f, $length)` form also honours a cap larger than the
/// 64 KiB concat scratch buffer without overrunning it.
#[test]
fn test_stream_get_contents_bounded_larger_than_concat_scratch() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$payload = str_repeat("abcdefghij", 10000);
file_put_contents("big_sgc_b.bin", $payload);
$f = fopen("big_sgc_b.bin", "r");
$data = stream_get_contents($f, 70000);
fclose($f);
unlink("big_sgc_b.bin");
echo strlen($data), "|", ($data === substr($payload, 0, 70000) ? "same" : "DIFF");
"#,
    );
    assert_eq!(out, "70000|same");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `fgets()` returns a line longer than the 64 KiB concat scratch buffer intact: the line
/// accumulator grows into owned heap storage instead of writing past `_concat_buf`.
#[test]
fn test_fgets_line_larger_than_concat_scratch() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$w = fopen("big_line.txt", "w");
fwrite($w, "first\n");
fwrite($w, str_repeat("Z", 200000));
fwrite($w, "\nlast\n");
fclose($w);
$f = fopen("big_line.txt", "r");
$a = fgets($f);
$b = fgets($f);
$c = fgets($f);
fclose($f);
unlink("big_line.txt");
echo rtrim($a), "|", strlen($b), "|", substr($b, 0, 3), "|", rtrim($c);
"#,
    );
    assert_eq!(out, "first|200001|ZZZ|last");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `stream_get_line()` honours a byte budget larger than the 64 KiB concat scratch
/// buffer, returning the full delimiter-stripped line from the reserved destination.
#[test]
fn test_stream_get_line_budget_larger_than_concat_scratch() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$long = str_repeat("Q", 150000);
file_put_contents("big_sgl.txt", $long . "|tail");
$f = fopen("big_sgl.txt", "r");
$a = stream_get_line($f, 200000, "|");
$b = stream_get_line($f, 200000, "|");
fclose($f);
unlink("big_sgl.txt");
echo strlen($a), "|", substr($a, 0, 3), "|", $b;
"#,
    );
    assert_eq!(out, "150000|QQQ|tail");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `stream_get_meta_data()` reports the mode string the caller passed, not one derived
/// from the descriptor's access bits.
///
/// The derivation could only ever answer `r`, `w` or `r+`: it read `F_GETFL`, which knows nothing
/// of `a` (reported `w`), of `+` past a `b` flag, or of the `b` flag itself. A library that
/// branches on `$meta['mode'][0] === 'a'` to decide whether a handle appends saw `w` and rewound.
#[test]
fn test_stream_get_meta_data_reports_the_mode_the_caller_passed() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("modes.txt", "seed");
foreach (["r", "rb", "r+", "r+b", "w", "w+", "a", "a+", "c"] as $mode) {
    $h = fopen("modes.txt", $mode);
    echo stream_get_meta_data($h)["mode"], " ";
    fclose($h);
}
"#,
    );
    assert_eq!(out, "r rb r+ r+b w w+ a a+ c ");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the memory wrappers report the mode of the stream PHP built for them.
///
/// `php://memory` and `php://temp` do not echo the caller's mode: a read-only mode answers `rb`,
/// an append mode `a+b`, and anything asking for write access `w+b`. Reference PHP 8.5.6 was the
/// oracle for each of these.
#[test]
fn test_stream_get_meta_data_maps_the_memory_wrapper_modes() {
    let out = compile_and_run(
        r#"<?php
foreach (["r", "rb", "r+", "w", "w+", "a", "c"] as $mode) {
    $h = fopen("php://memory", $mode);
    echo stream_get_meta_data($h)["mode"], " ";
    fclose($h);
}
$t = fopen("php://temp", "r");
echo stream_get_meta_data($t)["mode"], " ";
fclose($t);
$o = fopen("php://output", "w");
echo stream_get_meta_data($o)["mode"];
"#,
    );
    assert_eq!(out, "rb rb w+b w+b w+b a+b rb rb wb");
}

/// Verifies repeated `stream_get_meta_data()` calls keep reporting the same URI.
///
/// The array releases its string values, so handing it the StreamState's own URI allocation freed
/// the state's copy. The first two calls still read the right bytes; by the third, the hash keys
/// of the arrays built in between had reused the block, and `uri` came back as a fragment of
/// `seekable` or `blocked`. The state's pointer was also left dangling for its own teardown.
#[test]
fn test_stream_get_meta_data_uri_survives_repeated_reads() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("uri_meta.txt", "seed");
$h = fopen("uri_meta.txt", "r");
echo stream_get_meta_data($h)["uri"], "|";
echo stream_get_meta_data($h)["uri"], "|";
echo stream_get_meta_data($h)["uri"], "|";
echo stream_get_meta_data($h)["uri"];
fclose($h);
"#,
    );
    assert_eq!(out, "uri_meta.txt|uri_meta.txt|uri_meta.txt|uri_meta.txt");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `c` opens a file for writing without truncating it, and creates it when absent.
///
/// The mode parser accepted only `r`, `w` and `a`, so `c` — which PHP added precisely to let a
/// caller take an advisory lock before deciding to truncate — returned `false` with a warning.
#[test]
fn test_fopen_c_mode_creates_without_truncating() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("c_mode.txt", "abcdef");
$h = fopen("c_mode.txt", "c");
fwrite($h, "XY");
fclose($h);
echo file_get_contents("c_mode.txt"), "|";
$fresh = fopen("c_mode_new.txt", "c");
echo ($fresh === false ? "false" : "resource");
fclose($fresh);
"#,
    );
    assert_eq!(out, "XYcdef|resource");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `x` creates a file exclusively and refuses one that already exists.
#[test]
fn test_fopen_x_mode_refuses_an_existing_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$fresh = fopen("x_mode.txt", "x");
echo ($fresh === false ? "false" : "resource"), "|";
fwrite($fresh, "new");
fclose($fresh);
$again = @fopen("x_mode.txt", "x");
echo ($again === false ? "false" : "resource"), "|";
echo file_get_contents("x_mode.txt");
"#,
    );
    assert_eq!(out, "resource|false|new");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a `+` after the `b` flag still opens the file for both reading and writing.
///
/// The parser only inspected the second mode byte, so `rb+` — an idiom PHP accepts and the manual
/// spells out — stayed read-only and its writes failed silently.
#[test]
fn test_fopen_plus_is_honoured_after_the_b_flag() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("plus_after_b.txt", "abcdef");
$h = fopen("plus_after_b.txt", "rb+");
fwrite($h, "ZZ");
fclose($h);
echo file_get_contents("plus_after_b.txt");
"#,
    );
    assert_eq!(out, "ZZcdef");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `stream_socket_client()` warns when the connection is refused.
///
/// PHP raises this Warning whether or not the caller passed `&$errno`/`&$errstr`; elephc filled
/// the out-parameters and printed nothing, so a script that watched the warning to notice a dead
/// endpoint saw a silent `false`. Port 9 (discard) is not served on a CI host.
#[test]
fn test_stream_socket_client_warns_when_the_connection_is_refused() {
    let out = compile_and_run_capture(
        r#"<?php
$c = stream_socket_client("tcp://127.0.0.1:9");
echo ($c === false ? "false" : "resource");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "false");
    assert!(
        out.diagnostics
            .contains("Warning: stream_socket_client(): Unable to connect to tcp://127.0.0.1:9 ("),
        "expected PHP's connect warning, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies `@` suppresses the connect-failure warning, as it does every other PHP diagnostic.
#[test]
fn test_error_control_suppresses_the_connect_failure_warning() {
    let out = compile_and_run_capture(
        r#"<?php
$c = @stream_socket_client("tcp://127.0.0.1:9");
echo ($c === false ? "false" : "resource");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "false");
    assert_eq!(out.stderr, "");
    assert_eq!(out.diagnostics, "");
}

/// Verifies `stream_get_meta_data()['stream_type']` names the wrapper, not the descriptor.
///
/// It was derived from whether `lseek` worked, which is not what php-src reports: a memory stream
/// came back as STDIO, `php://output` as a socket, and a `popen()` pipe as a socket too. The name
/// is a wrapper and backend identity, so it comes from the recorded identity now.
#[test]
fn test_stream_get_meta_data_names_the_wrapper_not_the_descriptor() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("stype.txt", "seed");
$names = [];
$h = fopen("stype.txt", "r");
$names[] = stream_get_meta_data($h)["stream_type"];
fclose($h);
$h = fopen("php://memory", "r+");
$names[] = stream_get_meta_data($h)["stream_type"];
fclose($h);
$h = fopen("php://temp", "r+");
$names[] = stream_get_meta_data($h)["stream_type"];
fclose($h);
$h = fopen("php://output", "w");
$names[] = stream_get_meta_data($h)["stream_type"];
$h = fopen("php://input", "r");
$names[] = stream_get_meta_data($h)["stream_type"];
$h = fopen("data://text/plain,hi", "r");
$names[] = stream_get_meta_data($h)["stream_type"];
fclose($h);
$p = popen("printf hi", "r");
$names[] = stream_get_meta_data($p)["stream_type"];
pclose($p);
$d = opendir(".");
$names[] = stream_get_meta_data($d)["stream_type"];
closedir($d);
echo implode("|", $names);
"#,
    );
    assert_eq!(out, "STDIO|MEMORY|TEMP|Output|Input|RFC2397|STDIO|dir");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies each socket transport is named the way php-src names it.
///
/// A TCP, UDP, Unix-domain and paired socket are all non-seekable descriptors, so nothing about
/// them distinguishes the four names php-src gives them. The transport is recorded from the
/// address the caller wrote, and an accepted connection takes its listener's.
#[test]
fn test_stream_get_meta_data_names_each_socket_transport() {
    let out = compile_and_run(
        r#"<?php
$names = [];
$s = stream_socket_server("tcp://127.0.0.1:0");
$names[] = stream_get_meta_data($s)["stream_type"];
$c = stream_socket_client("tcp://" . stream_socket_get_name($s, false));
$names[] = stream_get_meta_data($c)["stream_type"];
$a = stream_socket_accept($s);
$names[] = stream_get_meta_data($a)["stream_type"];
fclose($a);
fclose($c);
fclose($s);
$u = stream_socket_server("udp://127.0.0.1:0", $e, $m, STREAM_SERVER_BIND);
$names[] = stream_get_meta_data($u)["stream_type"];
fclose($u);
$path = "/tmp/elephc_stype_transport.sock";
@unlink($path);
$x = stream_socket_server("unix://" . $path);
$names[] = stream_get_meta_data($x)["stream_type"];
fclose($x);
@unlink($path);
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, STREAM_IPPROTO_IP);
$names[] = stream_get_meta_data($pair[0])["stream_type"];
fclose($pair[0]);
fclose($pair[1]);
echo implode("|", $names);
"#,
    );
    assert_eq!(
        out,
        "tcp_socket/ssl|tcp_socket/ssl|tcp_socket/ssl|udp_socket|unix_socket|generic_socket"
    );
}

/// Verifies an unresolvable host produces the message php-src composes for it.
///
/// This failure has no `errno` — php-src builds the text itself, which is why `&$error_code` stays
/// `0` — so elephc, which only ever described an `errno`, left `&$error_message` empty and the
/// caller had nothing but `false` to go on. `.invalid` is reserved by RFC 2606 and never resolves.
#[test]
fn test_socket_error_outputs_describe_an_unresolvable_host() {
    let out = compile_and_run_capture(
        r#"<?php
$c = @stream_socket_client("tcp://no-such-host.invalid:80", $errno, $errstr);
echo ($c === false ? "false" : "resource"), "|", $errno, "|", $errstr;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert!(
        out.stdout.starts_with(
            "false|0|php_network_getaddresses: getaddrinfo for no-such-host.invalid failed: "
        ),
        "expected php-src's composed resolver message, got stdout={}",
        out.stdout
    );
}

/// Verifies an unresolvable host raises the two Warnings PHP raises, in PHP's order.
///
/// php-src reports the resolver's own message first, then the connect line that repeats it as the
/// reason.
#[test]
fn test_unresolvable_host_warns_twice_like_php() {
    let out = compile_and_run_capture(
        r#"<?php
$c = stream_socket_client("tcp://no-such-host.invalid:80");
echo ($c === false ? "false" : "resource");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "false");
    let lines: Vec<&str> = out.diagnostics.lines().collect();
    assert_eq!(lines.len(), 2, "expected two warnings, got diagnostics={}", out.diagnostics);
    assert!(
        lines[0].starts_with(
            "Warning: stream_socket_client(): php_network_getaddresses: getaddrinfo for \
             no-such-host.invalid failed: "
        ),
        "unexpected first warning: {}",
        lines[0]
    );
    assert!(
        lines[1].starts_with(
            "Warning: stream_socket_client(): Unable to connect to tcp://no-such-host.invalid:80 \
             (php_network_getaddresses: getaddrinfo for no-such-host.invalid failed: "
        ),
        "unexpected second warning: {}",
        lines[1]
    );
}

/// Verifies `fsockopen()` spells its refused endpoint the way PHP does, as `host:port`.
#[test]
fn test_fsockopen_warns_with_the_host_and_port() {
    let out = compile_and_run_capture(
        r#"<?php
$c = fsockopen("127.0.0.1", 9);
echo ($c === false ? "false" : "resource");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "false");
    assert!(
        out.diagnostics
            .contains("Warning: fsockopen(): Unable to connect to 127.0.0.1:9 ("),
        "expected PHP's connect warning, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies a FAILED open of a RUN-TIME `php://filter` URL names the URL, not the resource.
///
/// The literal spelling was fixed first; a URL assembled at run time still named the swapped
/// RESOURCE with the inner opener's errno, because the swap replaces the filename before any
/// opener runs and nothing downstream remembered what the program had written:
/// `Warning: fopen(absent_dyn.txt): Failed to open stream: No such file or directory`.
/// `php -n` 8.5.6 prints, for the same call, `Warning:
/// fopen(php://filter/read=string.toupper/resource=absent_dyn.txt): Failed to open stream:
/// operation failed` — php-src's `php_stream_url_wrap_php` returns NULL the moment the inner
/// open fails, BEFORE a single filter is created, and the generic caller composes one fixed
/// line from the URL it was handed.
///
/// `_php_filter_pending_mode` cannot gate this: it reads 0 exactly when the URL IS a filter URL
/// whose every name failed to resolve, which is the second probe here. The parse publishes the
/// URL itself and that pointer is the flag. The third probe pins that a PLAIN dynamic open still
/// names itself with its own errno — the suppression the filter path opens must not leak.
#[test]
fn test_failed_run_time_filter_open_names_the_url_not_the_wrapped_resource() {
    let out = compile_and_run_capture(
        r#"<?php
$u = "php://filter/read=string.toupper/resource=" . "absent_dyn.txt";
var_dump(fopen($u, "r"));
$v = "php://filter/read=no.such/resource=" . "absent_dyn2.txt";
var_dump(fopen($v, "r"));
$p = "no_such_plain" . ".txt";
var_dump(fopen($p, "r"));
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\nbool(false)\n");
    assert_eq!(
        out.diagnostics,
        "Warning: fopen(php://filter/read=string.toupper/resource=absent_dyn.txt): \
         Failed to open stream: operation failed\n\
         Warning: fopen(php://filter/read=no.such/resource=absent_dyn2.txt): \
         Failed to open stream: operation failed\n\
         Warning: fopen(no_such_plain.txt): Failed to open stream: No such file or directory\n",
        "the URL for both filter URLs; the plain path keeps its own name and errno"
    );
}

/// Verifies an unknown name in a RUN-TIME `php://filter` URL warns TWICE and keeps the stream.
///
/// The run-time parse published only the ids it HAD resolved, so a name it could not resolve was
/// dropped in complete silence and nothing downstream could report it — a typo in a filter name
/// became a silently unfiltered read. `php -n` 8.5.6 prints two lines per failed creation, one
/// from `php_stream_filter_create` (main/streams/filter.c) and one from
/// `php_stream_apply_filter_list`, and neither cancels the open:
///
/// ```text
/// Warning: fopen(): Unable to locate filter "no.such.filter"
/// Warning: fopen(): Unable to create filter (no.such.filter)
/// bool(true)
/// hello
/// ```
///
/// The chain CONTINUES past a failure, which the second probe pins: it still uppercases while
/// warning for both unknown names, in chain order.
#[test]
fn test_run_time_filter_unknown_name_warns_twice_and_keeps_the_stream() {
    let out = compile_and_run_capture(
        r#"<?php
file_put_contents("rtu.txt", "hello");
$res = "rtu" . ".txt";
$u = "php://filter/read=no.such.filter/resource=" . $res;
$h = fopen($u, "r");
var_dump(is_resource($h));
echo fread($h, 100), "|";
$c = "php://filter/read=one.bad|string.toupper|two.bad/resource=" . $res;
$g = fopen($c, "r");
echo fread($g, 100), "\n";
unlink("rtu.txt");
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "bool(true)\nhello|HELLO\n");
    assert_eq!(
        out.diagnostics,
        "Warning: fopen(): Unable to locate filter \"no.such.filter\"\n\
         Warning: fopen(): Unable to create filter (no.such.filter)\n\
         Warning: fopen(): Unable to locate filter \"one.bad\"\n\
         Warning: fopen(): Unable to create filter (one.bad)\n\
         Warning: fopen(): Unable to locate filter \"two.bad\"\n\
         Warning: fopen(): Unable to create filter (two.bad)\n",
        "two lines per unresolvable name, in chain order, with the known filter still applied"
    );
}

/// Verifies the run-time report counts the DIRECTIONS php applies, not the names.
///
/// php-src walks the filter list once per direction it applies and reaches
/// `php_stream_filter_create` again on the second walk, so the count is not one pair per name.
/// Measured on `php -n` 8.5.6 with a prefix-less chain: `"r"` warns once per name, `"r+"` twice,
/// and `"x"` — a mode naming neither direction — not at all, while the open still succeeds. An
/// explicit `read=` list is applied exactly once whatever the mode, so the same name opened
/// `"r+"` behind a `read=` prefix warns once. Six pairs in total.
///
/// The mode is read at RUN TIME, which the `$m = "r" . "+"` probe is here to force: `fopen($url,
/// $mode)` reaches the dynamic path with BOTH assembled at run time, so a rule that only ever
/// looked at a compile-time-literal mode would answer that line with half the warnings php
/// prints and no test would notice.
#[test]
fn test_run_time_filter_warning_count_follows_the_open_mode_directions() {
    let out = compile_and_run_capture(
        r#"<?php
file_put_contents("rtd.txt", "x");
$res = "rtd" . ".txt";
$plain = "php://filter/no.such/resource=" . $res;
fclose(fopen($plain, "r"));
echo "-r\n";
fclose(fopen($plain, "r+"));
echo "-rplus\n";
$fresh = "php://filter/no.such/resource=" . "rtdx.txt";
fclose(fopen($fresh, "x"));
echo "-x\n";
$m = "r" . "+";
fclose(fopen($plain, $m));
echo "-dynmode\n";
$explicit = "php://filter/read=no.such/resource=" . $res;
fclose(fopen($explicit, "r+"));
echo "-explicit\n";
unlink("rtd.txt");
unlink("rtdx.txt");
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "-r\n-rplus\n-x\n-dynmode\n-explicit\n");
    let pair = "Warning: fopen(): Unable to locate filter \"no.such\"\n\
                Warning: fopen(): Unable to create filter (no.such)\n";
    assert_eq!(
        out.diagnostics,
        pair.repeat(6),
        "one pair for `r`, two for `r+`, NONE for `x`, two for the run-time `r+`, one for `read=`"
    );
}

/// Verifies a failed run-time filtered open prints its line ALONE, and `@` silences everything.
///
/// php never reaches the filters when the inner open fails — `php_stream_url_wrap_php` returns
/// before creating any — so `php -n` 8.5.6 answers a URL that is BOTH unopenable and names an
/// unresolvable filter with the failed-open line and nothing else. An empty segment names
/// nothing and is skipped in silence on the success path, as `php_strtok_r` does, and `@`
/// suppresses every one of these through the shared depth counter rather than a rule of its own.
#[test]
fn test_run_time_filter_failed_open_warns_alone_and_at_suppresses_everything() {
    let out = compile_and_run_capture(
        r#"<?php
file_put_contents("rta.txt", "ok");
$res = "rta" . ".txt";
$bad = "php://filter/read=no.such/resource=" . "absent_rta.txt";
var_dump(fopen($bad, "r"));
var_dump(@fopen($bad, "r"));
$u = "php://filter/read=no.such/resource=" . $res;
var_dump(is_resource(@fopen($u, "r")));
$empty = "php://filter/read=/resource=" . $res;
$h = fopen($empty, "r");
echo fread($h, 10), "\n";
unlink("rta.txt");
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\nbool(true)\nok\n");
    assert_eq!(
        out.diagnostics,
        "Warning: fopen(php://filter/read=no.such/resource=absent_rta.txt): \
         Failed to open stream: operation failed\n",
        "the failed open speaks alone; `@` and an empty segment say nothing at all"
    );
}

/// Verifies the path readers NAME THEMSELVES in the unresolvable-filter warnings.
///
/// php words these with the CALLING function — `file_get_contents(): Unable to locate filter`,
/// `readfile(): Unable to create filter` — and every one of these routes said nothing at all.
/// The literal `file_get_contents` route wrapped the shared emitter in diagnostic suppression,
/// which silenced the unresolvable-name warnings along with the inner opener's it was aimed at;
/// the run-time routes had no channel for the names the parse dropped. All five verdicts
/// measured on `php -n` 8.5.6, and the first two lines pin that the LITERAL and the assembled
/// spelling of the same URL now answer alike.
#[test]
fn test_path_readers_name_themselves_in_unresolvable_filter_warnings() {
    let out = compile_and_run_capture(
        r#"<?php
file_put_contents("rtc.txt", "hi\n");
$res = "rtc" . ".txt";
echo file_get_contents("php://filter/read=no.such/resource=rtc.txt");
echo file_get_contents("php://filter/read=no.such/resource=" . $res);
readfile("php://filter/read=no.such/resource=" . $res);
var_dump(count(file("php://filter/read=no.such/resource=" . $res)));
var_dump(file_put_contents("php://filter/write=no.such/resource=" . "rtw.txt", "abc"));
unlink("rtc.txt");
unlink("rtw.txt");
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "hi\nhi\nhi\nint(1)\nint(3)\n");
    let lines = |callee: &str| {
        format!(
            "Warning: {callee}(): Unable to locate filter \"no.such\"\n\
             Warning: {callee}(): Unable to create filter (no.such)\n"
        )
    };
    assert_eq!(
        out.diagnostics,
        format!(
            "{}{}{}{}{}",
            lines("file_get_contents"),
            lines("file_get_contents"),
            lines("readfile"),
            lines("file"),
            lines("file_put_contents"),
        ),
        "each route names itself, for the literal URL and the assembled one alike"
    );
}

/// Verifies a filtered open NESTED inside another open does not swallow later warnings.
///
/// Silencing the inner opener needs a suppression scope, and gating that scope on "did the parse
/// see a filter URL" makes the pop depend on a global the resource's own open can republish: a
/// user wrapper's `stream_open` is PHP and may `fopen()` something itself, and a non-literal
/// inner path runs the parse, which clears that flag. The outer open then never popped what it
/// had pushed, and EVERY later warning in the program vanished — the two below among them. Each
/// open now saves what it needs on the way in and reads its own frame on the way out, so the pop
/// can never disagree with the push.
///
/// Both remaining lines are `php -n` 8.5.6's, and the empty filter segment is deliberate: the
/// outer chain has nothing to lose to the inner open's parse, which keeps this test about the
/// suppression pairing rather than the single-slot hand-off it shares with the pending ids.
#[test]
fn test_a_nested_open_does_not_leak_the_filter_suppression_scope() {
    let out = compile_and_run_capture(
        r#"<?php
class W {
    public $context;
    public function stream_open($path, $mode, $options, &$opened) {
        $p = "definitely_absent" . "_inner8.txt";
        $inner = @fopen($p, "r");
        return true;
    }
    public function stream_read($n) { return ""; }
    public function stream_eof() { return true; }
    public function stream_stat() { return array(); }
}
stream_wrapper_register("w8", "W");
$u = "php://filter/read=/resource=w8://x";
var_dump(is_resource(fopen($u, "r")));
$q = "absent_after" . "_t8.txt";
var_dump(fopen($q, "r"));
$b = "php://filter/read=string.toupper/resource=" . "absent_t8.txt";
var_dump(fopen($b, "r"));
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "bool(true)\nbool(false)\nbool(false)\n");
    assert_eq!(
        out.diagnostics,
        "Warning: fopen(absent_after_t8.txt): Failed to open stream: No such file or directory\n\
         Warning: fopen(php://filter/read=string.toupper/resource=absent_t8.txt): \
         Failed to open stream: operation failed\n",
        "the nested open must leave the suppression depth exactly as it found it"
    );
}

/// Verifies a filtered open NESTED inside another open does not STEAL the outer chain.
///
/// The parse hands its results to the attach through fixed globals, and the open that sits
/// between them can run PHP: a user wrapper's `stream_open` is a PHP method, and a method that
/// `fopen()`s anything re-enters the parse, which publishes over every one of those globals. The
/// outer open then attached whatever the INNER URL left behind — nothing, because the inner
/// open's own attach had already consumed and cleared it — and the outer chain vanished.
///
/// Both chains below are real and DIFFERENT, so the answer names which one ran: `php -n` 8.5.6
/// prints `string(3) "NOP"` — the wrapper serves `abc` uppercased by the chain its own
/// `stream_open` opened, and the outer chain then rot13s `ABC`. This branch's parent answered
/// `ABC`: the inner filter applied, the outer one silently did not.
#[test]
fn test_a_nested_open_does_not_steal_the_outer_filter_chain() {
    let out = compile_and_run_capture(
        r#"<?php
class W1 {
    public $context;
    public $fh;
    public function stream_open($path, $mode, $options, &$opened) {
        $u = "php://filter/read=string.toupper/resource=data://text/plain,abc";
        $this->fh = fopen($u, "r");
        return true;
    }
    public function stream_read($n) { return fread($this->fh, $n); }
    public function stream_eof() { return feof($this->fh); }
    public function stream_stat() { return array(); }
}
stream_wrapper_register("w1", "W1");
$u = "php://filter/read=string.rot13/resource=w1://x";
var_dump(stream_get_contents(fopen($u, "r")));
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "string(3) \"NOP\"\n");
    assert_eq!(out.stderr, "", "both chains resolve, so neither warns");
}

/// Verifies the names the OUTER URL could not resolve survive a nested open too.
///
/// The unresolved-name spans travel in the same hand-off as the filter ids, so the inner parse
/// took those with it: the outer URL's typo went unreported, which is the silence the whole
/// channel exists to end. The inner open is `@`-silenced deliberately — php reports the inner
/// names in the inner open's own words, and elephc's filter suppression still swallows them,
/// which is a separate defect this test must not pin either way.
///
/// `php -n` 8.5.6 on this script prints exactly the two lines below and `string(3) "abc"`: a
/// chain whose every name is unknown attaches nothing and the open still succeeds.
#[test]
fn test_a_nested_open_does_not_steal_the_outer_unresolved_names() {
    let out = compile_and_run_capture(
        r#"<?php
class W6 {
    public $context;
    public $buf;
    public $pos = 0;
    public function stream_open($path, $mode, $options, &$opened) {
        $u = "php://filter/read=inner.absent/resource=data://text/plain,zzz";
        $inner = @fopen($u, "r");
        $this->buf = "abc";
        return true;
    }
    public function stream_read($n) {
        $s = substr($this->buf, $this->pos, $n);
        $this->pos += strlen($s);
        return $s;
    }
    public function stream_eof() { return $this->pos >= strlen($this->buf); }
    public function stream_stat() { return array(); }
}
stream_wrapper_register("w6", "W6");
$u = "php://filter/read=outer.absent/resource=w6://x";
var_dump(stream_get_contents(fopen($u, "r")));
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "string(3) \"abc\"\n");
    assert_eq!(
        out.diagnostics,
        "Warning: fopen(): Unable to locate filter \"outer.absent\"\n\
         Warning: fopen(): Unable to create filter (outer.absent)\n",
        "the outer URL's own skipped name must survive the inner open's parse"
    );
}

/// Verifies the PATH readers keep their filter chain across a nested open as well.
///
/// `file_get_contents()` and `file_put_contents()` reach the same parse and the same attach
/// through their own routes, so the hand-off has to be parked on all three or the defect simply
/// moves. Measured on `php -n` 8.5.6: the read answers `string(3) "ABC"` and the write hands the
/// wrapper `ABC`, then answers the INPUT byte count.
#[test]
fn test_the_path_readers_keep_their_filter_chain_across_a_nested_open() {
    let out = compile_and_run_capture(
        r#"<?php
class WR {
    public $context;
    public $buf;
    public $pos = 0;
    public function stream_open($path, $mode, $options, &$opened) {
        $u = "php://filter/read=string.toupper/resource=data://text/plain,zzz";
        $inner = fopen($u, "r");
        $this->buf = "abc";
        return true;
    }
    public function stream_read($n) {
        $s = substr($this->buf, $this->pos, $n);
        $this->pos += strlen($s);
        return $s;
    }
    public function stream_write($data) { echo "wrote:[", $data, "]\n"; return strlen($data); }
    public function stream_eof() { return $this->pos >= strlen($this->buf); }
    public function stream_stat() { return array(); }
}
stream_wrapper_register("wr", "WR");
stream_wrapper_register("ww", "WR");
$r = "php://filter/read=string.toupper/resource=wr://x";
var_dump(file_get_contents($r));
$w = "php://filter/write=string.toupper/resource=ww://x";
var_dump(file_put_contents($w, "abc"));
"#,
    );
    assert!(out.success);
    assert_eq!(
        out.stdout,
        "string(3) \"ABC\"\nwrote:[ABC]\nint(3)\n",
        "both path routes must attach their OWN chain, not the nested open's"
    );
    assert_eq!(out.stderr, "");
    assert_eq!(out.diagnostics, "");
}

/// Verifies nesting PAST the parked-frame bound stays quiet instead of corrupting the outer open.
///
/// php-src imposes no limit on how many filtered opens can be in flight; elephc parks each one's
/// hand-off in a fixed BSS frame and keeps 8. The twelve below therefore overflow it by four, and
/// what must hold is that the frames INSIDE the bound are untouched: the outermost open sits at
/// depth 0 and its chain is the one the answer names. `php -n` 8.5.6 prints the twelve levels and
/// `string(3) "ABC"`; this branch's parent printed `string(3) "abc"`, having lost the outermost
/// chain to the very first nested open — the bound is not what this ever depended on.
#[test]
fn test_filtered_opens_nested_past_the_parked_frame_bound_keep_the_outer_chain() {
    let out = compile_and_run_capture(
        r#"<?php
class D {
    public $context;
    public $fh;
    public $buf;
    public $pos = 0;
    public function stream_open($path, $mode, $options, &$opened) {
        $scheme = substr($path, 0, strpos($path, "://"));
        $n = (int) substr($scheme, 1);
        if ($n > 0) {
            $u = "php://filter/read=/resource=d" . ($n - 1) . "://x";
            $this->fh = fopen($u, "r");
            $this->buf = stream_get_contents($this->fh);
        } else {
            $this->buf = "abc";
        }
        return true;
    }
    public function stream_read($n) {
        $s = substr($this->buf, $this->pos, $n);
        $this->pos += strlen($s);
        return $s;
    }
    public function stream_eof() { return $this->pos >= strlen($this->buf); }
    public function stream_stat() { return array(); }
}
for ($i = 0; $i < 12; $i++) { stream_wrapper_register("d" . $i, "D"); }
$u = "php://filter/read=string.toupper/resource=d11://x";
var_dump(stream_get_contents(fopen($u, "r")));
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "string(3) \"ABC\"\n");
    assert_eq!(
        out.stderr,
        "",
        "an empty filter segment is skipped in silence, at every depth"
    );
}

/// Verifies a literal `file_get_contents()` filter URL resolving NO filter still returns bytes.
///
/// `emit_open_read_close_tail` called `__rt_stream_get_contents` without staging its second
/// argument, the read-loop chunk size, so the loop ran with whatever the preceding code happened
/// to leave in that register. A URL naming a KNOWN filter left the stamp sequence's value there
/// and read correctly; a URL whose chain resolved to nothing left a rodata address, and the read
/// died with `Fatal error: Possible integer overflow in memory allocation`. Both spellings below
/// reached that, so a single typo in a filter name was a fatal. `php -n` 8.5.6 returns the
/// file's bytes for both, and says nothing about an empty segment.
#[test]
fn test_literal_filter_read_with_no_resolvable_filter_still_returns_the_bytes() {
    let out = compile_and_run_capture(
        r#"<?php
file_put_contents("rtn.txt", "bytes");
var_dump(file_get_contents("php://filter/read=/resource=rtn.txt"));
var_dump(file_get_contents("php://filter/read=string.toupper/resource=rtn.txt"));
unlink("rtn.txt");
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "string(5) \"bytes\"\nstring(5) \"BYTES\"\n");
    assert_eq!(out.stderr, "", "an empty segment is skipped in silence");
}

/// Verifies a user wrapper's OWN unresolvable-filter warnings survive the outer filter scope.
///
/// A filtered open silences its inner opener, because php-src returns NULL from
/// `php_stream_url_wrap_php` the moment the resource fails and composes one line from the whole
/// URL instead. elephc opened that silence with `__rt_diag_push_suppression` — the counter `@`
/// uses — around the WHOLE inner opener, and a user wrapper's `stream_open` is PHP running inside
/// it. Every warning that PHP raised therefore vanished with the inner opener's.
///
/// Written RED first. `php -n` 8.5.6 prints FOUR unresolvable-name lines here, the inner open's
/// pair before the outer's, and `string(3) "abc"`; at base elephc printed only the outer pair:
///
///     stdout: string(3) "abc"                                     # matched
///     stderr: Warning: fopen(): Unable to locate filter "outer.absent"
///             Warning: fopen(): Unable to create filter (outer.absent)
///             # the two `inner.absent` lines php prints FIRST were missing
#[test]
fn test_a_nested_open_reports_the_wrapper_s_own_unresolved_names() {
    let out = compile_and_run_capture(
        r#"<?php
class W6 {
    public $context;
    public $buf;
    public $pos = 0;
    public function stream_open($path, $mode, $options, &$opened) {
        $u = "php://filter/read=inner.absent/resource=data://text/plain,zzz";
        $inner = fopen($u, "r");
        $this->buf = "abc";
        return true;
    }
    public function stream_read($n) {
        $s = substr($this->buf, $this->pos, $n);
        $this->pos += strlen($s);
        return $s;
    }
    public function stream_eof() { return $this->pos >= strlen($this->buf); }
    public function stream_stat() { return array(); }
}
stream_wrapper_register("w6", "W6");
$u = "php://filter/read=outer.absent/resource=w6://x";
var_dump(stream_get_contents(fopen($u, "r")));
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "string(3) \"abc\"\n");
    assert_eq!(
        out.diagnostics,
        "Warning: fopen(): Unable to locate filter \"inner.absent\"\n\
         Warning: fopen(): Unable to create filter (inner.absent)\n\
         Warning: fopen(): Unable to locate filter \"outer.absent\"\n\
         Warning: fopen(): Unable to create filter (outer.absent)\n",
        "the wrapper's own PHP warns inside the outer open, and the outer names follow it"
    );
}

/// Verifies a FAILED open inside a user wrapper's `stream_open` still names itself.
///
/// The same swallow as [`test_a_nested_open_reports_the_wrapper_s_own_unresolved_names`], reached
/// by the other route: what the wrapper's PHP loses here is an ordinary failed-open line, not a
/// filter name, which pins that the scope silenced every warning raised under it rather than one
/// family. The outer chain resolves and applies, so the answer also says the swallow was never
/// the price of the chain.
///
/// Written RED first. `php -n` 8.5.6 prints the wrapper's own failed-open line and `string(3)
/// "ABC"`; at base elephc printed the same stdout and an EMPTY stderr:
///
///     stdout: string(3) "ABC"                                     # matched
///     stderr: <empty>                                             # php prints one Warning here
#[test]
fn test_a_failed_open_inside_stream_open_still_warns_under_a_filter_url() {
    let out = compile_and_run_capture(
        r#"<?php
class W7 {
    public $context;
    public $buf;
    public $pos = 0;
    public function stream_open($path, $mode, $options, &$opened) {
        $p = "definitely_absent" . "_inner7.txt";
        $inner = fopen($p, "r");
        $this->buf = "abc";
        return true;
    }
    public function stream_read($n) {
        $s = substr($this->buf, $this->pos, $n);
        $this->pos += strlen($s);
        return $s;
    }
    public function stream_eof() { return $this->pos >= strlen($this->buf); }
    public function stream_stat() { return array(); }
}
stream_wrapper_register("w7", "W7");
$u = "php://filter/read=string.toupper/resource=w7://x";
var_dump(stream_get_contents(fopen($u, "r")));
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "string(3) \"ABC\"\n");
    assert_eq!(
        out.diagnostics,
        "Warning: fopen(definitely_absent_inner7.txt): \
         Failed to open stream: No such file or directory\n",
        "the wrapper's own failed open speaks, and the outer chain still applies"
    );
}

/// The CONTROL for the two above: `@` on a filtered open still silences EVERYTHING under it.
///
/// Splitting the filter scope off the `@` counter is only correct if `@` keeps reaching the same
/// distance it always did — into the wrapper's PHP, into the inner open's filter names, and into
/// the outer URL's own. This script raises all three under one `@` and then warns OUTSIDE it, so
/// a scope left standing would be visible on the following line rather than merely suspected.
///
/// `php -n` 8.5.6 prints `string(3) "abc"`, `after` and `bool(false)` with a single Warning, for
/// the LAST open only. This one was GREEN at base and must stay so: it is what the split may not
/// cost.
#[test]
fn test_at_silences_a_filtered_open_including_the_wrapper_s_own_php() {
    let out = compile_and_run_capture(
        r#"<?php
class W9 {
    public $context;
    public $buf;
    public $pos = 0;
    public function stream_open($path, $mode, $options, &$opened) {
        $u = "php://filter/read=inner.absent/resource=data://text/plain,zzz";
        $inner = fopen($u, "r");
        $p = "definitely_absent" . "_inner9.txt";
        $miss = fopen($p, "r");
        $this->buf = "abc";
        return true;
    }
    public function stream_read($n) {
        $s = substr($this->buf, $this->pos, $n);
        $this->pos += strlen($s);
        return $s;
    }
    public function stream_eof() { return $this->pos >= strlen($this->buf); }
    public function stream_stat() { return array(); }
}
stream_wrapper_register("w9", "W9");
$u = "php://filter/read=outer.absent/resource=w9://x";
var_dump(stream_get_contents(@fopen($u, "r")));
echo "after\n";
$q = "absent_after" . "_c9.txt";
var_dump(fopen($q, "r"));
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "string(3) \"abc\"\nafter\nbool(false)\n");
    assert_eq!(
        out.diagnostics,
        "Warning: fopen(absent_after_c9.txt): Failed to open stream: No such file or directory\n",
        "`@` still covers the whole open, and gives the depth back when it ends"
    );
}

/// Verifies a throw out of `stream_open` UNWINDS the filter depths instead of spending them.
///
/// A filtered open counts itself in `_php_filter_open_depth` on the way in — which is also what
/// records that it opened a suppression scope for its inner opener — and in
/// `_php_filter_pending_depth` when it parks its hand-off. Both are given back by helpers that
/// run after the opener RETURNS, and an exception thrown out of a user wrapper's `stream_open`
/// reaches neither: the depth stays up, and the eight parked frames are spent one throw at a
/// time. The try handler already saves and restores `_rt_diag_suppression` for exactly this
/// reason, and the filter depths now travel in the same set.
///
/// The witness is the LAST open's wording. Past the bound `__rt_php_filter_suppress_begin` parks
/// nothing and so opens no scope, which stops silencing the inner opener — and php-src never
/// lets that opener speak, because `php_stream_url_wrap_php` returns NULL and the caller composes
/// one line naming the WHOLE URL. So a leaked depth turns php's line into the inner opener's,
/// naming `absent_x7.txt`: a path the program never wrote. That is a plain failed open with no
/// nesting, so it says the same thing on both arches; an earlier draft of this test ended on a
/// nested rot13 chain instead, which is x86-red at BASE for an unrelated reason and would have
/// pinned that defect here by accident.
///
/// Written RED first. `php -n` 8.5.6 prints twelve `caught` lines, `bool(false)`, and exactly one
/// Warning. At base elephc printed the same stdout and the WRONG warning — measured identically
/// on aarch64 natively and on x86_64 under qemu:
///
///     stderr: Warning: fopen(absent_x7.txt): Failed to open stream: No such file or directory
#[test]
fn test_a_throw_out_of_stream_open_gives_the_filter_depths_back() {
    let out = compile_and_run_capture(
        r#"<?php
class TH {
    public $context;
    public function stream_open($path, $mode, $options, &$opened) {
        throw new Exception("boom");
    }
    public function stream_read($n) { return ""; }
    public function stream_eof() { return true; }
    public function stream_stat() { return array(); }
}
stream_wrapper_register("th", "TH");
$t = "php://filter/read=string.toupper/resource=th://x";
for ($i = 0; $i < 12; $i++) {
    try { $h = fopen($t, "r"); } catch (Exception $e) { echo "caught\n"; }
}
$bad = "php://filter/read=no.such/resource=" . "absent_x7.txt";
var_dump(fopen($bad, "r"));
"#,
    );
    assert!(out.success);
    assert_eq!(
        out.stdout,
        "caught\ncaught\ncaught\ncaught\ncaught\ncaught\n\
         caught\ncaught\ncaught\ncaught\ncaught\ncaught\n\
         bool(false)\n"
    );
    assert_eq!(
        out.diagnostics,
        "Warning: fopen(php://filter/read=no.such/resource=absent_x7.txt): \
         Failed to open stream: operation failed\n",
        "twelve throws must cost no filter frame, so the last open still names the whole URL"
    );
}

/// The WHOLE stat family reaches a wrapper's `url_stat()`, with the flags php passes each caller.
///
/// Only `file_exists()`/`filesize()`/`is_file()` ever dispatched; every other stat builtin ran the
/// real filesystem against a `scheme://` path that is not a file, so on the wrapper below elephc
/// measured `is_dir`, `is_link`, `filemtime`, `fileatime`, `filectime`, `filetype`, `fileperms`,
/// `fileowner`, `filegroup`, `fileinode`, `stat` and `lstat` ALL as `bool(false)` — twelve silent
/// wrong answers.
///
/// The `[n]` markers are the `$flags` argument, which php varies per caller and elephc hard-coded
/// to 0. Measured one call at a time on php 8.5.6 with `clearstatcache(true)` between them — php
/// keeps a ONE-entry stat cache, so without that the second read of a path never reaches the
/// wrapper at all and the flags never show. `PHP_STREAM_URL_STAT_NOCACHE`(4) alone for the value
/// readers, `|LINK`(5) for the two that do not follow a symlink, `|QUIET`(6) for the silent
/// predicates, and all three (7) for `is_link()`.
///
/// `stat()` rebuilds php's canonical 26 entries rather than handing back the wrapper's own array,
/// which is why the numeric keys read as well as the string ones.
#[test]
fn test_whole_stat_family_dispatches_to_wrapper_url_stat_with_php_flags() {
    let out = compile_and_run(
        r#"<?php
class SW {
    public $context;
    function url_stat($path, $flags) {
        echo "[", $flags, "]";
        if (strpos($path, "dir") !== false) {
            return ["dev"=>7,"ino"=>11,"mode"=>040755,"nlink"=>2,"uid"=>501,"gid"=>20,
                    "rdev"=>0,"size"=>96,"atime"=>1000000001,"mtime"=>1000000002,
                    "ctime"=>1000000003,"blksize"=>4096,"blocks"=>0];
        }
        return ["dev"=>7,"ino"=>11,"mode"=>0100644,"nlink"=>1,"uid"=>501,"gid"=>20,
                "rdev"=>0,"size"=>1234,"atime"=>1000000001,"mtime"=>1000000002,
                "ctime"=>1000000003,"blksize"=>4096,"blocks"=>8];
    }
}
stream_wrapper_register("sw", "SW");
$f = "sw://a.txt";
$d = "sw://x/dir";
echo "is_dir "; echo is_dir($f) ? "Y" : "N"; clearstatcache(true);
echo is_dir($d) ? "Y" : "N"; echo "\n"; clearstatcache(true);
echo "is_link "; echo is_link($f) ? "Y" : "N"; echo "\n"; clearstatcache(true);
echo "filemtime ", filemtime($f), "\n"; clearstatcache(true);
echo "fileatime ", fileatime($f), "\n"; clearstatcache(true);
echo "filectime ", filectime($f), "\n"; clearstatcache(true);
echo "fileperms ", fileperms($f), "\n"; clearstatcache(true);
echo "fileowner ", fileowner($f), "\n"; clearstatcache(true);
echo "filegroup ", filegroup($f), "\n"; clearstatcache(true);
echo "fileinode ", fileinode($f), "\n"; clearstatcache(true);
echo "filetype ", filetype($f); clearstatcache(true);
echo " ", filetype($d), "\n"; clearstatcache(true);
$s = stat($f);
echo "stat ", $s["mode"], " ", $s[2], " ", $s["size"], " ", $s[7], " ", $s["blocks"], " ", $s[0], "\n";
clearstatcache(true);
$s = lstat($d);
echo "lstat ", $s["mode"], " ", $s["nlink"], " ", $s[12], "\n";
"#,
    );
    assert_eq!(
        out,
        "is_dir [6]N[6]Y\n\
         is_link [7]N\n\
         filemtime [4]1000000002\n\
         fileatime [4]1000000001\n\
         filectime [4]1000000003\n\
         fileperms [4]33188\n\
         fileowner [4]501\n\
         filegroup [4]20\n\
         fileinode [4]11\n\
         filetype [5]file [5]dir\n\
         [4]stat 33188 33188 1234 1234 8 7\n\
         [5]lstat 16877 2 0\n",
        "every stat builtin must reach url_stat with php's own flags and read php's own field"
    );
}

/// A wrapper array that does not NAME a field answers zero, not false.
///
/// php builds a `php_stream_statbuf` from the array and `statbuf_from_array` zeroes it first, so
/// only `url_stat()` answering `false` is a failed stat. Measured on php 8.5.6: a wrapper returning
/// just `['mode' => 0100644]` gives `filesize()` `int(0)` and `filemtime()` `int(0)`, while
/// `stat()` still measures as a full 26-entry array whose unnamed fields are all `0`. Conflating
/// "absent field" with "failed stat" would turn each of those into `bool(false)`.
///
/// A mode of zero matches no `S_IFMT`, so the type predicates read false and `filetype()` reads
/// `"unknown"` — after php's notice, which names the mode MASKED to its file-type bits.
#[test]
fn test_wrapper_url_stat_absent_field_reads_zero_not_false() {
    let out = compile_and_run_capture(
        r#"<?php
class Sparse {
    public $context;
    function url_stat($path, $flags) { return ["mode" => 0100644]; }
}
class Bald {
    public $context;
    function url_stat($path, $flags) { return []; }
}
stream_wrapper_register("sp", "Sparse");
stream_wrapper_register("bd", "Bald");
var_dump(filesize("sp://x"));
clearstatcache(true);
var_dump(filemtime("sp://x"));
clearstatcache(true);
var_dump(fileowner("sp://x"));
clearstatcache(true);
var_dump(is_file("sp://x"));
clearstatcache(true);
var_dump(filetype("sp://x"));
clearstatcache(true);
$s = stat("sp://x");
echo $s["mode"], ",", $s["size"], ",", $s[8], ",", $s[12], "\n";
clearstatcache(true);
var_dump(filetype("bd://x"));
clearstatcache(true);
var_dump(is_file("bd://x"));
clearstatcache(true);
var_dump(is_dir("bd://x"));
clearstatcache(true);
var_dump(filesize("bd://x"));
"#,
    );
    assert!(out.success);
    assert_eq!(
        out.stdout,
        "int(0)\nint(0)\nint(0)\nbool(true)\nstring(4) \"file\"\n\
         33188,0,0,0\n\
         string(7) \"unknown\"\nbool(false)\nbool(false)\nint(0)\n",
        "an unnamed stat field is php's zero; only url_stat() answering false is a failed stat"
    );
    assert_eq!(
        out.diagnostics,
        "Notice: filetype(): Unknown file type (0)\n",
        "a successful stat that names few fields warns about nothing, but a mode php cannot \
         classify still gets its notice"
    );
}

/// The access checks SELECT a permission triad out of the wrapper's mode; they do not `access(2)`.
///
/// php never asks the kernel about a `scheme://` path — it compares the array's `uid`/`gid` against
/// the process and then tests ONE bit of the mode, so the answer can contradict the filesystem
/// entirely. elephc ran `access(2)` on the URL as a literal path, which cannot exist, so
/// `is_readable()`/`is_writable()`/`is_writeable()`/`is_executable()` measured false for every
/// wrapper.
///
/// Measured on php 8.5.6, with `$mode` decimal because the wrapper reads it back out of the path:
/// owner-matched `0400`(256) reads readable and `0040`(32) does not; unmatched, `0040` does not and
/// `0004`(4) does. The GROUP triad — `st_gid == getgid()` or `st_gid` among `getgroups()` — is the
/// third branch and measures the same way (`--- --- --- r-- -w- --x --- --- ---` for a file owned
/// by another user in the process's primary group); it is not asserted here because a compiled test
/// has no portable way to name the runner's gid.
#[test]
fn test_wrapper_access_checks_select_phps_permission_triad() {
    let out = compile_and_run(
        r#"<?php
file_put_contents("triad_probe.txt", "x");
$me = fileowner("triad_probe.txt");
class TW {
    public $context;
    public static $owner = 0;
    function url_stat($path, $flags) {
        $mode = (int) substr($path, strrpos($path, "/") + 1);
        if (strpos($path, "/mine/") !== false) {
            return ["mode" => $mode, "uid" => TW::$owner, "gid" => 4242];
        }
        return ["mode" => $mode, "uid" => 4242, "gid" => 4242];
    }
}
TW::$owner = $me;
stream_wrapper_register("tw", "TW");
function probe($p) {
    clearstatcache(true);
    echo is_readable($p) ? "r" : "-";
    clearstatcache(true);
    echo is_writable($p) ? "w" : "-";
    clearstatcache(true);
    echo is_writeable($p) ? "W" : "-";
    clearstatcache(true);
    echo is_executable($p) ? "x" : "-";
    echo " ";
}
foreach ([256, 128, 64, 32, 16, 8, 4, 2, 1, 511] as $mode) { probe("tw:///mine/" . $mode); }
echo "|";
foreach ([256, 128, 64, 32, 16, 8, 4, 2, 1, 511] as $mode) { probe("tw:///other/" . $mode); }
echo "\n";
unlink("triad_probe.txt");
"#,
    );
    assert_eq!(
        out,
        "r--- -wW- ---x ---- ---- ---- ---- ---- ---- rwWx \
         |---- ---- ---- ---- ---- ---- r--- -wW- ---x rwWx \n",
        "the owner triad wins on a uid match and the world triad answers when nothing matches"
    );
}

/// Every stat caller names ITSELF in the missing-hook warning, and the value readers add php's
/// second line.
///
/// One runtime dispatcher serves them all, so it cannot know which builtin reached it; the lowering
/// publishes the caller's name. `is_writeable()` names the ALIAS the program called, not
/// `is_writable`. Measured on php 8.5.6: the value readers follow with `stat failed for`, the two
/// link-free ones with php's capitalized `Lstat failed for`, and the six PREDICATES print nothing
/// beyond the missing-hook line.
#[test]
fn test_every_stat_caller_names_itself_when_url_stat_is_missing() {
    let out = compile_and_run_capture(
        r#"<?php
class Bare {
    public $context;
    function stream_open($p, $m, $o, &$op) { $op = $p; return true; }
}
stream_wrapper_register("bare", "Bare");
var_dump(is_dir("bare://a"));
var_dump(is_link("bare://a"));
var_dump(is_readable("bare://a"));
var_dump(is_writable("bare://a"));
var_dump(is_writeable("bare://a"));
var_dump(is_executable("bare://a"));
var_dump(filemtime("bare://a"));
var_dump(fileatime("bare://a"));
var_dump(filectime("bare://a"));
var_dump(filetype("bare://a"));
var_dump(fileperms("bare://a"));
var_dump(fileowner("bare://a"));
var_dump(filegroup("bare://a"));
var_dump(fileinode("bare://a"));
var_dump(stat("bare://a"));
var_dump(lstat("bare://a"));
var_dump(@is_dir("bare://a"));
"#,
    );
    assert!(out.success, "the diagnostics must not disturb the program");
    assert_eq!(out.stdout, "bool(false)\n".repeat(17));
    assert_eq!(
        out.diagnostics,
        "Warning: is_dir(): Bare::url_stat is not implemented!\n\
         Warning: is_link(): Bare::url_stat is not implemented!\n\
         Warning: is_readable(): Bare::url_stat is not implemented!\n\
         Warning: is_writable(): Bare::url_stat is not implemented!\n\
         Warning: is_writeable(): Bare::url_stat is not implemented!\n\
         Warning: is_executable(): Bare::url_stat is not implemented!\n\
         Warning: filemtime(): Bare::url_stat is not implemented!\n\
         Warning: filemtime(): stat failed for bare://a\n\
         Warning: fileatime(): Bare::url_stat is not implemented!\n\
         Warning: fileatime(): stat failed for bare://a\n\
         Warning: filectime(): Bare::url_stat is not implemented!\n\
         Warning: filectime(): stat failed for bare://a\n\
         Warning: filetype(): Bare::url_stat is not implemented!\n\
         Warning: filetype(): Lstat failed for bare://a\n\
         Warning: fileperms(): Bare::url_stat is not implemented!\n\
         Warning: fileperms(): stat failed for bare://a\n\
         Warning: fileowner(): Bare::url_stat is not implemented!\n\
         Warning: fileowner(): stat failed for bare://a\n\
         Warning: filegroup(): Bare::url_stat is not implemented!\n\
         Warning: filegroup(): stat failed for bare://a\n\
         Warning: fileinode(): Bare::url_stat is not implemented!\n\
         Warning: fileinode(): stat failed for bare://a\n\
         Warning: stat(): Bare::url_stat is not implemented!\n\
         Warning: stat(): stat failed for bare://a\n\
         Warning: lstat(): Bare::url_stat is not implemented!\n\
         Warning: lstat(): Lstat failed for bare://a\n",
        "each caller's own name, php's second line only for the value readers, \
         and nothing at all from the suppressed call"
    );
}

/// The value readers report a FAILED stat on an ordinary path too, not just through a wrapper.
///
/// Only `filesize()` printed php's `stat failed for` line; the other ten failed in silence.
/// Measured on php 8.5.6 against an absent path — note that `filetype()` and `lstat()` capitalize
/// it as `Lstat failed for`, and that the predicates print nothing at all.
#[test]
fn test_stat_value_readers_report_a_failed_stat_on_an_ordinary_path() {
    let out = compile_and_run_capture(
        r#"<?php
$p = "/definitely/not/here";
var_dump(filesize($p));
var_dump(filemtime($p));
var_dump(fileatime($p));
var_dump(filectime($p));
var_dump(fileperms($p));
var_dump(fileowner($p));
var_dump(filegroup($p));
var_dump(fileinode($p));
var_dump(filetype($p));
var_dump(stat($p));
var_dump(lstat($p));
var_dump(is_dir($p));
var_dump(is_link($p));
var_dump(is_readable($p));
var_dump(file_exists($p));
"#,
    );
    assert!(out.success);
    assert_eq!(out.stdout, "bool(false)\n".repeat(15));
    assert_eq!(
        out.diagnostics,
        "Warning: filesize(): stat failed for /definitely/not/here\n\
         Warning: filemtime(): stat failed for /definitely/not/here\n\
         Warning: fileatime(): stat failed for /definitely/not/here\n\
         Warning: filectime(): stat failed for /definitely/not/here\n\
         Warning: fileperms(): stat failed for /definitely/not/here\n\
         Warning: fileowner(): stat failed for /definitely/not/here\n\
         Warning: filegroup(): stat failed for /definitely/not/here\n\
         Warning: fileinode(): stat failed for /definitely/not/here\n\
         Warning: filetype(): Lstat failed for /definitely/not/here\n\
         Warning: stat(): stat failed for /definitely/not/here\n\
         Warning: lstat(): Lstat failed for /definitely/not/here\n",
        "eleven readers report the failure and the four predicates stay silent"
    );
}

/// Verifies the argument-range `ValueError`s php-src raises across the `stream_*` surface.
///
/// Each wording below was MEASURED against `php -n` 8.5.6 before this test was written:
///
/// ```text
/// stream_get_contents($f, -5)     ValueError: stream_get_contents(): Argument #2 ($length) must be greater than or equal to -1
/// stream_get_contents($f, -1)     reads to EOF — -1 is the documented "read all" sentinel, NOT an error
/// stream_set_chunk_size($f, 0)    ValueError: stream_set_chunk_size(): Argument #2 ($size) must be greater than 0
/// stream_socket_shutdown($f, 9)   ValueError: stream_socket_shutdown(): Argument #2 ($mode) must be one of STREAM_SHUT_RD, STREAM_SHUT_WR, or STREAM_SHUT_RDWR
/// ```
///
/// All three are catchable `ValueError`s, so a `try`/`catch` observes the message instead
/// of the process dying; each used to answer a VALUE (`""`, `int(8192)`, `false`).
#[test]
fn test_stream_builtins_raise_php_argument_range_value_errors() {
    let out = compile_and_run(
        r#"<?php
function probe(callable $c): string {
    try { $v = $c(); return "no-throw:" . var_export($v, true); }
    catch (ValueError $e) { return $e->getMessage(); }
}
$m = fopen("php://memory", "r+");
fwrite($m, "payload");
rewind($m);
echo probe(fn() => stream_get_contents($m, -5)), "\n";
echo probe(fn() => stream_set_chunk_size($m, 0)), "\n";
echo probe(fn() => stream_set_chunk_size($m, -3)), "\n";
echo probe(fn() => stream_socket_shutdown($m, 9)), "\n";
echo probe(fn() => stream_socket_shutdown($m, -1)), "\n";
rewind($m);
echo stream_get_contents($m, -1), "\n";
fclose($m);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "stream_get_contents(): Argument #2 ($length) must be greater than or equal to -1\n",
            "stream_set_chunk_size(): Argument #2 ($size) must be greater than 0\n",
            "stream_set_chunk_size(): Argument #2 ($size) must be greater than 0\n",
            "stream_socket_shutdown(): Argument #2 ($mode) must be one of STREAM_SHUT_RD, \
             STREAM_SHUT_WR, or STREAM_SHUT_RDWR\n",
            "stream_socket_shutdown(): Argument #2 ($mode) must be one of STREAM_SHUT_RD, \
             STREAM_SHUT_WR, or STREAM_SHUT_RDWR\n",
            "payload\n",
        )
    );
}

/// Verifies `stream_socket_shutdown()` still accepts every mode php-src enumerates.
///
/// The `ValueError` guard sits between the argument and the runtime helper, so the three
/// legal modes must keep reaching it. MEASURED: php answers `true` for all three on a
/// connected socket pair.
#[test]
fn test_stream_socket_shutdown_accepts_the_three_php_modes() {
    let out = compile_and_run(
        r#"<?php
foreach ([STREAM_SHUT_RD, STREAM_SHUT_WR, STREAM_SHUT_RDWR] as $mode) {
    $pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
    echo stream_socket_shutdown($pair[0], $mode) ? "y" : "n";
    fclose($pair[0]);
    fclose($pair[1]);
}
"#,
    );
    assert_eq!(out, "yyy");
}

/// Verifies `stream_context_get_options()` names php-src's own parameter in its `TypeError`,
/// and that the error is CATCHABLE rather than an immediate fatal.
///
/// MEASURED on `php -n` 8.5.6 — the parameter is `$stream_or_context`, not `$stream`:
///
/// ```text
/// stream_context_get_options("nope") TypeError: stream_context_get_options(): Argument #1 ($stream_or_context) must be of type resource, string given
/// stream_context_get_options(1)      TypeError: stream_context_get_options(): Argument #1 ($stream_or_context) must be of type resource, int given
/// ```
#[test]
fn test_stream_context_get_options_type_error_is_catchable_and_names_its_parameter() {
    let out = compile_and_run(
        r#"<?php
$values = ["nope", 1, 1.5, null, []];
foreach ($values as $value) {
    try { stream_context_get_options($value); echo "no-throw\n"; }
    catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "stream_context_get_options(): Argument #1 ($stream_or_context) must be of type resource, string given\n",
            "stream_context_get_options(): Argument #1 ($stream_or_context) must be of type resource, int given\n",
            "stream_context_get_options(): Argument #1 ($stream_or_context) must be of type resource, float given\n",
            "stream_context_get_options(): Argument #1 ($stream_or_context) must be of type resource, null given\n",
            "stream_context_get_options(): Argument #1 ($stream_or_context) must be of type resource, array given\n",
        )
    );
}

/// Verifies `stream_copy_to_stream()` only seeks for a STRICTLY POSITIVE `$offset`.
///
/// php-src's `streamsfuncs.c` guards the seek with `pos > 0`, so its documented default
/// (`$offset = 0`) copies from the source's CURRENT position — it does not rewind.
/// MEASURED on `php -n` 8.5.6 with the source parked at byte 4 of `"0123456789"`:
///
/// ```text
/// stream_copy_to_stream($src, $dst)          6 bytes, "456789"
/// stream_copy_to_stream($src, $dst, null,  0) 6 bytes, "456789"   <- 0 does NOT rewind
/// stream_copy_to_stream($src, $dst, null, -1) 6 bytes, "456789"
/// stream_copy_to_stream($src, $dst, null,  2) 8 bytes, "23456789"
/// ```
#[test]
fn test_stream_copy_to_stream_zero_offset_keeps_the_source_position() {
    let out = compile_and_run(
        r#"<?php
function run(int $offset): string {
    $src = fopen("php://memory", "r+");
    fwrite($src, "0123456789");
    fseek($src, 4);
    $dst = fopen("php://memory", "r+");
    $n = stream_copy_to_stream($src, $dst, null, $offset);
    rewind($dst);
    $payload = stream_get_contents($dst);
    fclose($src);
    fclose($dst);
    return $n . ":" . $payload;
}
function run_default(): string {
    $src = fopen("php://memory", "r+");
    fwrite($src, "0123456789");
    fseek($src, 4);
    $dst = fopen("php://memory", "r+");
    $n = stream_copy_to_stream($src, $dst);
    rewind($dst);
    $payload = stream_get_contents($dst);
    fclose($src);
    fclose($dst);
    return $n . ":" . $payload;
}
echo run_default(), "\n";
echo run(0), "\n";
echo run(-1), "\n";
echo run(2), "\n";
"#,
    );
    assert_eq!(
        out,
        "6:456789\n6:456789\n6:456789\n8:23456789\n",
        "only a strictly positive offset seeks, exactly like php-src's `pos > 0`"
    );
}

/// Verifies every shape of `stream_context_set_option()` php refuses, plus its 8.3 notice.
///
/// The stub's fourth parameter carries NO default — it is `UNKNOWN`, not `null` — and the
/// second is `array|string`, so the arity alone does not decide: what php accepts depends on
/// whether `$wrapper_or_options` is an array or a string. MEASURED on `php -n` 8.5.6:
///
/// ```text
/// ($c, ['http' => [...]])          E_DEPRECATED, then bool(true)
/// ($c, ['http' => [...]], null)    bool(true), and NO deprecation — the notice counts arguments
/// ($c, ['http' => [...]], 'x')     ValueError: Argument #3 ($option_name) must be null when argument #2 ($wrapper_or_options) is an array
/// ($c, 'http')                     E_DEPRECATED, then ValueError: Argument #3 ($option_name) cannot be null when argument #2 ($wrapper_or_options) is a string
/// ($c, 'http', 'header')           ValueError: Argument #4 ($value) must be provided when argument #2 ($wrapper_or_options) is a string
/// ($c, 'http', null)               ValueError: Argument #3 ($option_name) cannot be null when argument #2 ($wrapper_or_options) is a string
/// ($c, 'http', 'header', 'X: 1')   bool(true)
/// ```
///
/// The three-argument form used to answer a silent `bool(true)` and store nothing, so a caller
/// who forgot the value read the refusal as a successful write.
#[test]
fn test_stream_context_set_option_refuses_phps_invalid_shapes() {
    let out = compile_and_run_capture(
        r#"<?php
$c1 = stream_context_create();
try { echo stream_context_set_option($c1, ['http' => ['a' => 1]]) === true ? "true" : "other", "\n"; }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
$c2 = stream_context_create();
try { echo stream_context_set_option($c2, ['http' => ['a' => 1]], null) === true ? "true" : "other", "\n"; }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
$c3 = stream_context_create();
try { echo stream_context_set_option($c3, ['http' => ['a' => 1]], 'x') === true ? "true" : "other", "\n"; }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
$c4 = stream_context_create();
try { echo stream_context_set_option($c4, 'http') === true ? "true" : "other", "\n"; }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
$c5 = stream_context_create();
try { echo stream_context_set_option($c5, 'http', 'header') === true ? "true" : "other", "\n"; }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
$c6 = stream_context_create();
try { echo stream_context_set_option($c6, 'http', null) === true ? "true" : "other", "\n"; }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
$c7 = stream_context_create();
try { echo stream_context_set_option($c7, 'http', 'header', 'X: 1') === true ? "true" : "other", "\n"; }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
echo json_encode(stream_context_get_options($c7)), "\n";
"#,
    );
    assert!(out.success);
    assert_eq!(
        out.stdout,
        concat!(
            "true\n",
            "true\n",
            "stream_context_set_option(): Argument #3 ($option_name) must be null when \
             argument #2 ($wrapper_or_options) is an array\n",
            "stream_context_set_option(): Argument #3 ($option_name) cannot be null when \
             argument #2 ($wrapper_or_options) is a string\n",
            "stream_context_set_option(): Argument #4 ($value) must be provided when \
             argument #2 ($wrapper_or_options) is a string\n",
            "stream_context_set_option(): Argument #3 ($option_name) cannot be null when \
             argument #2 ($wrapper_or_options) is a string\n",
            "true\n",
            "{\"http\":{\"header\":\"X: 1\"}}\n",
        )
    );
    assert_eq!(
        out.diagnostics,
        concat!(
            "Deprecated: Calling stream_context_set_option() with 2 arguments is deprecated, \
             use stream_context_set_options() instead\n",
            "Deprecated: Calling stream_context_set_option() with 2 arguments is deprecated, \
             use stream_context_set_options() instead\n",
        ),
        "the notice fires on the ARITY, so the three-argument array form stays quiet"
    );
}

/// Verifies `stream_select()` rejects php-src's two negative timeout components.
///
/// MEASURED on `php -n` 8.5.6 against a live `stream_socket_pair()`:
///
/// ```text
/// stream_select($r, $w, $e, -1)     ValueError: stream_select(): Argument #4 ($seconds) must be greater than or equal to 0
/// stream_select($r, $w, $e, 0, -1)  ValueError: stream_select(): Argument #5 ($microseconds) must be greater than or equal to 0
/// ```
#[test]
fn test_stream_select_rejects_negative_timeout_components() {
    let out = compile_and_run(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
fwrite($pair[1], "hi");
$w = null;
$x = null;
$r = [$pair[0]];
try { stream_select($r, $w, $x, -1); echo "no-throw\n"; }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
$r = [$pair[0]];
try { stream_select($r, $w, $x, 0, -1); echo "no-throw\n"; }
catch (ValueError $e) { echo $e->getMessage(), "\n"; }
$r = [$pair[0]];
echo stream_select($r, $w, $x, 0), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "stream_select(): Argument #4 ($seconds) must be greater than or equal to 0\n",
            "stream_select(): Argument #5 ($microseconds) must be greater than or equal to 0\n",
            "1\n",
        )
    );
}

/// Verifies the CSV reader KEEPS the escape byte, which php never removes.
///
/// `"a\"b"` reads back as `a\"b` on `php -n` 8.5.6 — four bytes, exactly what `fputcsv()` wrote.
/// All the escape character does is stop the next byte from closing the field; both bytes land in
/// the value. The parser dropped it when it preceded the ENCLOSURE and kept it everywhere else,
/// so every round trip through a quoted field containing one silently lost a byte. The three
/// other rows pin the cases that already worked, so the fix cannot be a swap of which byte is
/// lost. `str_getcsv()` shares the state machine and is checked alongside.
#[test]
fn test_fgetcsv_keeps_the_escape_byte_it_reads() {
    let out = compile_and_run(
        r#"<?php
$cases = ["\"a\\\"b\"\n", "\"x\\\"\",y\n", "\"a\\\\b\"\n", "\"a\\,b\"\n"];
foreach ($cases as $text) {
    $h = fopen("php://memory", "r+");
    fwrite($h, $text);
    rewind($h);
    echo json_encode(fgetcsv($h, 0, ",", "\"", "\\")), "|";
    fclose($h);
    echo json_encode(str_getcsv(rtrim($text, "\n"), ",", "\"", "\\")), "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "[\"a\\\\\\\"b\"]|[\"a\\\\\\\"b\"]\n",
            "[\"x\\\\\\\"\",\"y\"]|[\"x\\\\\\\"\",\"y\"]\n",
            "[\"a\\\\\\\\b\"]|[\"a\\\\\\\\b\"]\n",
            "[\"a\\\\,b\"]|[\"a\\\\,b\"]\n",
        )
    );
}

/// Verifies the enclosure that CLOSES a field is consumed, even when data follows it.
///
/// php reads `"ab"cd` as `abcd`: the closing quote is gone and everything after it is ordinary
/// data, quotes included — `"ab"c"d"` reads back as `abc"d"`. The parser wrote the quote back
/// before resuming, which added a byte php never keeps. The doubled-quote row is here because it
/// runs through the same state and must NOT change: `"ab""cd"` is still `ab"cd`.
#[test]
fn test_fgetcsv_drops_the_closing_enclosure_when_data_follows_it() {
    let out = compile_and_run(
        r#"<?php
$cases = ["\"ab\"cd\n", "\"ab\"cd,e\n", "\"ab\"c\"d\"\n", "\"ab\" cd\n", "\"ab\"\"cd\"\n"];
foreach ($cases as $text) {
    $h = fopen("php://memory", "r+");
    fwrite($h, $text);
    rewind($h);
    echo json_encode(fgetcsv($h, 0, ",", "\"", "\\")), "\n";
    fclose($h);
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "[\"abcd\"]\n",
            "[\"abcd\",\"e\"]\n",
            "[\"abc\\\"d\\\"\"]\n",
            "[\"ab cd\"]\n",
            "[\"ab\\\"cd\"]\n",
        )
    );
}

/// Verifies whitespace in FRONT of an opening enclosure is skipped, and only then.
///
/// php looks ahead from the start of a field: if the first byte that is neither the separator nor
/// whitespace is the enclosure, the field starts there and the whitespace is dropped. So
/// `" \"a\",b"` reads as `a`, while `" a,b"` — no enclosure ahead — keeps the space and reads as
/// `" a"`. The reader had no lookahead at all and kept the space in both.
///
/// The last row is the reason the lookahead is bounded by the BUFFER rather than by a newline
/// test: `str_getcsv()` holds the whole subject, so it CAN reach a quote past a newline and php
/// answers `["a"]`, while `fgetcsv()` holds one line and cannot. One bound gives both.
#[test]
fn test_fgetcsv_skips_whitespace_before_an_opening_enclosure() {
    let out = compile_and_run(
        r#"<?php
$cases = [" \"a\",b\n", "\t\"a\",b\n", " a,b\n", "a, \"b\"\n", " x\"a\",b\n", " \"a\" ,b\n"];
foreach ($cases as $text) {
    $h = fopen("php://memory", "r+");
    fwrite($h, $text);
    rewind($h);
    echo json_encode(fgetcsv($h, 0, ",", "\"", "\\")), "\n";
    fclose($h);
}
echo json_encode(str_getcsv(" \n\"a\"", ",", "\"", "\\")), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "[\"a\",\"b\"]\n",
            "[\"a\",\"b\"]\n",
            "[\" a\",\"b\"]\n",
            "[\"a\",\"b\"]\n",
            "[\" x\\\"a\\\"\",\"b\"]\n",
            "[\"a \",\"b\"]\n",
            "[\"a\"]\n",
        )
    );
}

/// Verifies the `$escape` deprecation comes AFTER the control characters are validated.
///
/// php checks the separator, enclosure and escape for being a single character before it reaches
/// the notice, so a call that throws `ValueError` never prints one. elephc emitted the notice
/// first, which made every rejected call two lines where php prints one — and on the CSV family
/// that is the whole diagnostic. The successful call at the end is what proves the notice was
/// moved rather than lost.
#[test]
fn test_csv_escape_deprecation_comes_after_the_control_validation() {
    let out = compile_and_run_capture(
        r#"<?php
$h = fopen("php://memory", "r+");
fwrite($h, "a,b\n");
rewind($h);
try { fgetcsv($h, 0, ";;"); } catch (ValueError $e) { echo "1:", $e->getMessage(), "\n"; }
try { fputcsv($h, ["a"], ";;"); } catch (ValueError $e) { echo "2:", $e->getMessage(), "\n"; }
try { str_getcsv("a,b", ";;"); } catch (ValueError $e) { echo "3:", $e->getMessage(), "\n"; }
rewind($h);
echo "4:", json_encode(fgetcsv($h)), "\n";
fclose($h);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "1:fgetcsv(): Argument #3 ($separator) must be a single character\n",
            "2:fputcsv(): Argument #3 ($separator) must be a single character\n",
            "3:str_getcsv(): Argument #2 ($separator) must be a single character\n",
            "4:[\"a\",\"b\"]\n",
        )
    );
    let notices = out
        .diagnostics
        .matches("the $escape parameter must be provided")
        .count();
    assert_eq!(
        notices, 1,
        "only the call that survived validation may warn, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies a filter name held in a VARIABLE reaches the same filters a literal one does.
///
/// `$name = "zlib.deflate"; stream_filter_append($h, $name);` attached NOTHING and answered
/// `false`, while the identical call with the literal compresses. php makes no such distinction,
/// and a name in a variable is ordinary PHP — a config value, a loop over a list, exactly what
/// this test does. The five are unreachable through the run-time name table on purpose: that
/// table lists what a chain node can apply, and each of these installs a per-fd handle and a
/// program-local helper thunk instead, so the lowering now emits the attach SEQUENCES at the
/// call site and picks between them by comparing the name.
///
/// The refusals matter as much as the attaches. `convert.iconv.` and `convert.iconv.UTF-8` carry
/// no separator, so php has no filter for them and answers `false`; `convert.iconv.nope/alsonope`
/// names a conversion `iconv_open()` cannot open, and php finds that out when it CREATES the
/// filter, so that is `false` too. An EMPTY half is none of those — iconv reads it as the current
/// locale's charset, and php attaches. All measured on `php -n` 8.5.6.
#[test]
fn test_stream_filter_append_resolves_a_run_time_filter_name() {
    let out = compile_and_run(
        r#"<?php
$names = ["zlib.deflate", "zlib.inflate", "bzip2.compress", "bzip2.decompress", "convert.iconv.UTF-8/ISO-8859-1"];
foreach ($names as $n) {
    $h = fopen("php://memory", "w+");
    echo $n, "=", var_export(@stream_filter_append($h, $n, STREAM_FILTER_WRITE) !== false, true), "\n";
    fclose($h);
}
$h = fopen("php://memory", "w+");
echo "literal=", var_export(@stream_filter_append($h, "zlib.deflate", STREAM_FILTER_WRITE) !== false, true), "\n";
fclose($h);
$bad = ["convert.iconv.", "convert.iconv.UTF-8", "convert.iconv.nope/alsonope", "nosuchfilter"];
foreach ($bad as $n) {
    $h = fopen("php://memory", "w+");
    echo $n, "=", var_export(@stream_filter_append($h, $n, STREAM_FILTER_WRITE), true), "\n";
    fclose($h);
}
$h = fopen("php://memory", "w+");
echo "empty-half=", var_export(@stream_filter_append($h, "convert.iconv.UTF-8/", STREAM_FILTER_WRITE) !== false, true), "\n";
fclose($h);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "zlib.deflate=true\n",
            "zlib.inflate=true\n",
            "bzip2.compress=true\n",
            "bzip2.decompress=true\n",
            "convert.iconv.UTF-8/ISO-8859-1=true\n",
            "literal=true\n",
            "convert.iconv.=false\n",
            "convert.iconv.UTF-8=false\n",
            "convert.iconv.nope/alsonope=false\n",
            "nosuchfilter=false\n",
            "empty-half=true\n",
        )
    );
}

/// Verifies php's "create or locate" wording reaches the `convert.iconv.*` refusals.
///
/// php has two verbs and picks by WHY the attach failed: a name no factory claims gets
/// `Unable to locate filter "nosuchfilter"`, while one a factory claims and then refuses gets
/// `Unable to create or locate filter "convert.iconv."`. Every `convert.iconv.` name reaches the
/// second, the prefix being what selects the factory. elephc reported success for both of these
/// and warned about neither.
#[test]
fn test_iconv_filter_refusal_uses_the_create_or_locate_wording() {
    let out = compile_and_run_capture(
        r#"<?php
$h = fopen("php://memory", "w+");
var_dump(stream_filter_append($h, "convert.iconv.", STREAM_FILTER_WRITE));
var_dump(stream_filter_append($h, "convert.iconv.nope/alsonope", STREAM_FILTER_WRITE));
var_dump(stream_filter_append($h, "nosuchfilter", STREAM_FILTER_WRITE));
fclose($h);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\nbool(false)\n");
    for expected in [
        "Unable to create or locate filter \"convert.iconv.\"",
        "Unable to create or locate filter \"convert.iconv.nope/alsonope\"",
        "Unable to locate filter \"nosuchfilter\"",
    ] {
        assert!(
            out.diagnostics.contains(expected),
            "missing {expected}, got diagnostics={}",
            out.diagnostics
        );
    }
}

/// Verifies `stream_select()` answers the READY COUNT when `$write`/`$except` are null.
///
/// `stream_select($r, $w, $e, 0)` with null write and except sets is the shape every read loop
/// in PHP is written in, and it answered 15 where php answers 1 — a constant, because the null
/// sets reached the runtime as boxed Mixed cells whose header it read as an array length, so
/// `poll()` was handed fourteen uninitialized entries and counted every one of them.
///
/// The empty-array row is the contrast that isolates it: passing `[]` instead of `null` was
/// correct throughout, which is why no existing test caught the null form. The last row keeps a
/// non-null `$write` in the mix so the two shapes cannot be conflated, and every row also checks
/// the arrays are compacted to the ready subset, which is the other half of the contract.
#[test]
fn test_stream_select_counts_ready_streams_with_null_sets() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("sel.txt", "hello");
$a = fopen("sel.txt", "r");
$b = fopen("sel.txt", "r");

$r = [$a];
$w = null;
$e = null;
echo stream_select($r, $w, $e, 0), "|", count($r), "\n";

$r = [$a, $b];
$w = null;
$e = null;
echo stream_select($r, $w, $e, 0), "|", count($r), "\n";

$r = [$a];
$w = [];
$e = [];
echo stream_select($r, $w, $e, 0), "|", count($r), "\n";

$r = [$a];
$w = [$b];
$e = null;
echo stream_select($r, $w, $e, 0), "|", count($r), "|", count($w), "\n";

fclose($a);
fclose($b);
"#,
    );
    assert_eq!(out, "1|1\n2|2\n1|1\n2|1|1\n");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `stream_select()` refuses a `php://memory` stream, as php does.
///
/// A MEMORY stream is bytes in the heap: there is no operating-system descriptor to poll. php
/// names the type and drops the entry — `Cannot represent a stream of type MEMORY as a
/// select()able descriptor` — and raises `ValueError: No stream arrays were passed` when that
/// leaves nothing selectable. elephc polled the stream's backing descriptor and reported it
/// READY, so a select loop that blocks forever on php returned immediately here.
///
/// Only MEMORY is refused, and the other rows are what pin that: `php://temp` selects fine, being
/// backed by a real file, and so do `data:` and a plain file. The mixed row is the one that shows
/// the entry is DROPPED rather than fatal — a memory stream beside a real one leaves the real one
/// selectable, and the answer counts only it.
#[test]
fn test_stream_select_refuses_a_memory_stream() {
    let out = compile_and_run_capture(
        r#"<?php
file_put_contents("selmem.txt", "hello");
// Each label is built and printed only once the call has resolved, so where the warnings go
// cannot interleave with stdout — the diagnostic stream is asserted separately below.
function probe(string $label, $s): string {
    $r = [$s];
    $w = null;
    $e = null;
    try {
        return $label . "=" . var_export(stream_select($r, $w, $e, 0), true);
    } catch (ValueError $ex) {
        return $label . "=VE:" . $ex->getMessage();
    }
}
$out = [];
$out[] = probe("file", fopen("selmem.txt", "r"));
$out[] = probe("memory", fopen("php://memory", "w+"));
$out[] = probe("temp", fopen("php://temp", "w+"));
$out[] = probe("data", fopen("data://text/plain,hi", "r"));
$f = fopen("selmem.txt", "r");
$m = fopen("php://memory", "w+");
$r = [$f, $m];
$w = null;
$e = null;
$n = stream_select($r, $w, $e, 0);
$out[] = "mixed=" . $n . "|" . count($r);
fclose($f);
fclose($m);
echo implode("\n", $out), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "file=1\n",
            "memory=VE:No stream arrays were passed\n",
            "temp=1\n",
            "data=1\n",
            "mixed=1|1\n",
        )
    );
    let notices = out
        .diagnostics
        .matches("Cannot represent a stream of type MEMORY as a select()able descriptor")
        .count();
    // Three, not two: php walks the arrays TWICE — once to build the descriptor sets and once to
    // translate the result back — and names the stream on both passes, so the mixed call warns
    // twice while the memory-only one warns once, its ValueError landing before the second pass.
    assert_eq!(
        notices, 3,
        "one per pass that reached the memory stream, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies a `compress.zlib://` URL assembled at RUN time opens like the literal spelling.
///
/// `$name = "compress.zlib://out.gz"; fopen($name, "w");` answered `false` where the identical
/// call with the literal compresses — in both directions. The wrapper was reachable only from a
/// compile-time literal, because that is what the split into "wrapper" and "underlying path"
/// needed, and a URL built with `sys_get_temp_dir()` or read from config is ordinary PHP.
///
/// The literal rows are kept beside the computed ones so a fix that merely moves which spelling
/// works still fails this test, and the round trip is checked through the OTHER spelling each
/// time — a literal write read back by a computed open, and the reverse — which is what proves
/// the two produce the same bytes rather than two self-consistent formats.
#[test]
fn test_compress_zlib_wrapper_accepts_a_run_time_url() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$literal = "compress.zlib://a.gz";
$computed = "compress.zlib://b.gz";

$w = fopen("compress.zlib://a.gz", "w");
var_dump(fwrite($w, "payload payload payload"));
fclose($w);

$r = fopen($literal, "r");
echo "computed read of literal write: ";
var_dump(stream_get_contents($r));
fclose($r);

$w2 = fopen($computed, "w");
var_dump(fwrite($w2, "second second second"));
fclose($w2);

$r2 = fopen("compress.zlib://b.gz", "r");
echo "literal read of computed write: ";
var_dump(stream_get_contents($r2));
fclose($r2);

// The raw file must NOT be the payload: a wrapper that merely passed bytes through
// would round-trip just as happily.
echo "compressed: ";
var_dump(file_get_contents("a.gz") !== "payload payload payload");
"#,
    );
    assert_eq!(
        out,
        concat!(
            "int(23)\n",
            "computed read of literal write: string(23) \"payload payload payload\"\n",
            "int(20)\n",
            "literal read of computed write: string(20) \"second second second\"\n",
            "compressed: bool(true)\n",
        )
    );
    let _ = fs::remove_dir_all(&dir);
/// Verifies a wrapper declaring the PHP manual's `stream_read(): string|false` returns its
/// actual bytes.
///
/// A wrapper method is called through the ABI its own return type produces: `: string` hands
/// back the raw pointer/length pair, while the manual's union has codegen representation
/// `Mixed` and hands back a single boxed cell. The runtime helper read the pair either way, so
/// the documented signature yielded the right LENGTH and the wrong bytes — `fread()` answered
/// five spaces where PHP answers "hello". Every other wrapper test in this file declares
/// `: string`, so the suite pinned the limitation and not one of them could fail on this.
#[test]
fn test_wrapper_stream_read_declared_string_or_false_returns_its_bytes() {
    let out = compile_and_run(
        r#"<?php
class UnionReadW {
    public $context;
    private $data = "hello world";
    private $pos = 0;
    public function stream_open($p, $m, $o, &$op): bool { return true; }
    public function stream_read(int $count): string|false {
        if ($this->pos >= strlen($this->data)) { return false; }
        $chunk = substr($this->data, $this->pos, $count);
        $this->pos = $this->pos + strlen($chunk);
        return $chunk;
    }
    public function stream_eof(): bool { return $this->pos >= strlen($this->data); }
}
stream_wrapper_register("unionread", "UnionReadW");
$f = fopen("unionread://x", "r");
echo "[", fread($f, 5), "][", fread($f, 6), "]";
fclose($f);
"#,
    );
    assert_eq!(out, "[hello][ world]");
}

/// Verifies a wrapper declaring the manual's `dir_readdir(): string|false` yields its entries
/// and stops on `false`.
///
/// Same boxed-return mismatch as the read slot above, and the reason `examples/dir-wrapper`
/// used to declare `: string` and signal the end of the directory with an empty string: that
/// spelling is not PHP's, and reference php loops forever on it because `"" !== false`.
#[test]
fn test_wrapper_dir_readdir_declared_string_or_false_ends_on_false() {
    let out = compile_and_run(
        r#"<?php
class UnionDirW {
    public $context;
    private $entries = ["alpha.txt", "beta.md"];
    private $pos = 0;
    public function dir_opendir($path, $options): bool { $this->pos = 0; return true; }
    public function dir_readdir(): string|false {
        if ($this->pos >= count($this->entries)) { return false; }
        $name = $this->entries[$this->pos];
        $this->pos = $this->pos + 1;
        return $name;
    }
    public function dir_rewinddir(): bool { $this->pos = 0; return true; }
    public function dir_closedir(): bool { return true; }
}
stream_wrapper_register("uniondir", "UnionDirW");
$dh = opendir("uniondir://x");
while (($entry = readdir($dh)) !== false) { echo "[", $entry, "]"; }
rewinddir($dh);
echo "|", readdir($dh);
closedir($dh);
"#,
    );
    assert_eq!(out, "[alpha.txt][beta.md]|alpha.txt");
}

/// Verifies the boxed return path releases the cell without freeing the string it hands back.
///
/// `__rt_mixed_cast_string` routes an already-persisted payload through `__rt_str_persist`,
/// which DUPLICATES it — but takes a concat temporary over IN PLACE and returns that same
/// pointer. Releasing the box afterwards would then free the entry name being returned, so the
/// box is retagged as a scalar first and only its own storage is released. A method returning a
/// concatenation is what makes that difference observable.
#[test]
fn test_wrapper_boxed_return_of_a_concatenation_survives_the_box_release() {
    let out = compile_and_run(
        r#"<?php
class ConcatDirW {
    public $context;
    private $names = ["one", "two", "three"];
    private $pos = 0;
    public function dir_opendir($path, $options): bool { $this->pos = 0; return true; }
    public function dir_readdir(): string|false {
        if ($this->pos >= count($this->names)) { return false; }
        $name = "e-" . $this->names[$this->pos] . ".txt";
        $this->pos = $this->pos + 1;
        return $name;
    }
    public function dir_closedir(): bool { return true; }
}
stream_wrapper_register("concatdir", "ConcatDirW");
$dh = opendir("concatdir://x");
while (($entry = readdir($dh)) !== false) { echo "[", $entry, "]"; }
closedir($dh);
"#,
    );
    assert_eq!(out, "[e-one.txt][e-two.txt][e-three.txt]");
}

/// Verifies the raw-pair path still works, so the conversion is selected and not applied to
/// everything.
///
/// The boxed path is chosen by a per-class mask, and a mask that was always set would pass
/// every test above while breaking every wrapper that declares `: string` — the spelling all
/// the other tests in this file use.
#[test]
fn test_wrapper_stream_read_declared_string_still_uses_the_raw_pair() {
    let out = compile_and_run(
        r#"<?php
class PlainReadW {
    public $context;
    private $data = "abcdef";
    private $pos = 0;
    public function stream_open($p, $m, $o, &$op): bool { return true; }
    public function stream_read(int $count): string {
        $chunk = substr($this->data, $this->pos, $count);
        $this->pos = $this->pos + strlen($chunk);
        return $chunk;
    }
    public function stream_eof(): bool { return $this->pos >= strlen($this->data); }
}
stream_wrapper_register("plainread", "PlainReadW");
$f = fopen("plainread://x", "r");
echo "[", fread($f, 3), "][", fread($f, 3), "]";
fclose($f);
"#,
    );
    assert_eq!(out, "[abc][def]");
}
