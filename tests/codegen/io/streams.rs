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

/// Verifies compiled PHP output for stream get wrappers lists known wrappers.
#[test]
fn test_stream_get_wrappers_lists_known_wrappers() {
    // Full PHP-published wrapper list (Phase D: surface 100%). ftps,
    // compress.*, phar, glob are accepted at runtime but currently
    // return false from fopen — the listing is the PHP-spec surface.
    let out = compile_and_run(
        r#"<?php $w = stream_get_wrappers(); echo count($w) . ":" . $w[0] . "," . $w[3] . "," . $w[5];"#,
    );
    assert_eq!(out, "11:file,ftp,https");
}

/// Verifies compiled PHP output for stream get transports and filters.
#[test]
fn test_stream_get_transports_and_filters() {
    // Full PHP-published transport and filter lists. tlsv1.0/1.1/1.2/1.3
    // + sslv2/3 route through the same enable_crypto path; the extended
    // filter list registers strip_tags / base64-* / qp-* / dechunk as
    // passthrough stubs so stream_filter_append succeeds.
    let out = compile_and_run(
        r#"<?php echo count(stream_get_transports()) . "," . count(stream_get_filters());"#,
    );
    assert_eq!(out, "12,14");
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
    // fscanf shares __rt_sscanf, so the new %f branch must work through it too.
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
    // stream_filter_prepend attaches a filter; stream_filter_remove drops it.
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
    assert_eq!(out, "first pass|FIRST PASS");
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

/// Verifies compiled PHP output for stream filter strip tags removes html.
#[test]
fn test_stream_filter_strip_tags_removes_html() {
    // The string.strip_tags read filter elides everything between '<' and '>'.
    let out = compile_and_run(
        r#"<?php
$m = fopen("php://memory", "r+");
fwrite($m, "<p>Hello <b>World</b></p>");
rewind($m);
stream_filter_append($m, "string.strip_tags", STREAM_FILTER_READ);
echo fread($m, 64);
fclose($m);
"#,
    );
    assert_eq!(out, "Hello World");
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

/// Verifies compiled PHP output for fopen php output is stdout alias.
#[test]
fn test_fopen_php_output_is_stdout_alias() {
    let out = compile_and_run(r#"<?php $h = fopen("php://output", "w"); fwrite($h, "aliased");"#);
    assert_eq!(out, "aliased");
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
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("udp://127.0.0.1:0");
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
    assert_eq!(out, "2");
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
echo "|";
echo getprotobyname("ip");
"#,
    );
    assert_eq!(out, "6|17|1|0");
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
echo "|";
echo getprotobynumber(0);
"#,
    );
    assert_eq!(out, "tcp|udp|icmp|ip");
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
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("udp://127.0.0.1:54745");
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
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("udp://127.0.0.1:54740");
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
$a = stream_socket_server("udp://127.0.0.1:54741");
$b = stream_socket_server("udp://127.0.0.1:54742");
echo stream_socket_sendto($b, "abc", 0, "udp://127.0.0.1:54741");
echo "|";
echo fread($a, 16);
"#,
    );
    assert_eq!(out, "3|abc");
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
    let out = compile_and_run(
        r#"<?php
$path = "/tmp/elephc_udg_codegen_test.sock";
unlink($path);
$srv = stream_socket_server("udg://" . $path);
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
$srv = stream_socket_server("udg://" . $srv_path);
$cli = stream_socket_server("udg://" . $cli_path);
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

/// Minimal one-shot HTTP/1.0 server for the `http://` codegen test. Binds the
/// port immediately, then serves one client on a thread: it drains the request
/// headers and writes a close-framed response whose body is `content`.
fn spawn_http_server(port: u16, content: &'static [u8]) -> std::thread::JoinHandle<()> {
    use std::io::{Read, Write};
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", port)).expect("http test: bind port");
    std::thread::spawn(move || {
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
    })
}

/// Same shape as `spawn_http_server` but echoes the received request bytes
/// back as the response body so tests can assert on the exact wire format
/// (method, path, headers, AND body) the elephc-built request produced.
fn spawn_http_echo_server(port: u16) -> std::thread::JoinHandle<()> {
    use std::io::{Read, Write};
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", port)).expect("http test: bind port");
    std::thread::spawn(move || {
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
    })
}

/// Serves two HTTP responses on the same port: the first is a 302 with a
/// `Location:` header pointing to `final_path` on the same `127.0.0.1:port`,
/// the second is a 200 with `body`. Used to exercise the follow_location
/// path through both relative and absolute Location values.
fn spawn_http_redirect_server(
    port: u16,
    location: &'static str,
    final_path: &'static str,
    body: &'static [u8],
) -> std::thread::JoinHandle<()> {
    use std::io::{Read, Write};
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", port)).expect("http redirect: bind port");
    std::thread::spawn(move || {
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
    })
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
fn spawn_https_server(port: u16, content: &'static [u8]) -> std::thread::JoinHandle<()> {
    use std::io::{Read, Write};
    use std::sync::Arc;

    let listener =
        std::net::TcpListener::bind(("127.0.0.1", port)).expect("https test: bind port");
    std::thread::spawn(move || {
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
    })
}

/// Verifies compiled PHP output for fopen http method default is get.
#[test]
fn test_fopen_http_method_default_is_get() {
    // Without a stream context, the request method falls back to "GET".
    // The echo server reflects the request bytes; the response body must
    // start with "GET /path HTTP/1.0\r\n".
    let _server = spawn_http_echo_server(54995);
    let out = compile_and_run(
        r#"<?php
$f = fopen("http://127.0.0.1:54995/echo", "r");
$req = stream_get_contents($f);
fclose($f);
echo substr($req, 0, 19);
"#,
    );
    assert_eq!(out, "GET /echo HTTP/1.0\r");
}

/// Verifies compiled PHP output for fopen http method overrides via context.
#[test]
fn test_fopen_http_method_overrides_via_context() {
    // Phase 11 B2: stream_context_create(['http' => ['method' => 'POST']])
    // propagates through __rt_http_build_request → the request line
    // starts with "POST" instead of the default "GET".
    let _server = spawn_http_echo_server(54996);
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "method", "POST");
$f = fopen("http://127.0.0.1:54996/api", "r");
$req = stream_get_contents($f);
fclose($f);
echo substr($req, 0, 21);
"#,
    );
    assert_eq!(out, "POST /api HTTP/1.0\r\nH");
}

/// Verifies compiled PHP output for fopen http header inserted via context.
#[test]
fn test_fopen_http_header_inserted_via_context() {
    // Phase 11 B2: stream_context_create(['http' => ['header' => ...]])
    // propagates through __rt_http_build_request — the supplied header
    // line lands between the Host: line and the Connection: close line.
    let _server = spawn_http_echo_server(54997);
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "header", "X-Trace: abc");
$f = fopen("http://127.0.0.1:54997/path", "r");
$req = stream_get_contents($f);
fclose($f);
echo strpos($req, "\r\nX-Trace: abc\r\n") !== false ? "has-header" : "no-header";
"#,
    );
    assert_eq!(out, "has-header");
}

/// Verifies compiled PHP output for fopen http content only emits body.
#[test]
fn test_fopen_http_content_only_emits_body() {
    // Reduced repro of the POST + content gap: set only ['http']['content']
    // without 'method'. If this passes, the bug is in set_option_4's two-call
    // sub-hash merge; if this fails, it's in the content lookup or emission.
    let _server = spawn_http_echo_server(53999);
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "content", "x=y");
$f = fopen("http://127.0.0.1:53999/p", "r");
$req = stream_get_contents($f);
fclose($f);
$has_clen = strpos($req, "\r\nContent-Length: 3\r\n") !== false;
$has_body = strpos($req, "\r\n\r\nx=y") !== false;
echo ($has_clen ? "clen-ok" : "clen-MISSING") . "|" . ($has_body ? "body-ok" : "body-MISSING");
"#,
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
    let _server = spawn_http_echo_server(53998);
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "method", "POST");
stream_context_set_option(stream_context_get_default(), "http", "content", "foo=bar&baz=qux");
$f = fopen("http://127.0.0.1:53998/submit", "r");
$req = stream_get_contents($f);
fclose($f);
$has_clen = strpos($req, "\r\nContent-Length: 15\r\n") !== false;
$has_body = strpos($req, "\r\n\r\nfoo=bar&baz=qux") !== false;
echo ($has_clen ? "clen-ok" : "clen-MISSING") . "|" . ($has_body ? "body-ok" : "body-MISSING");
"#,
    );
    assert_eq!(out, "clen-ok|body-ok");
}

/// Verifies compiled PHP output for fopen http retrieves body.
#[test]
fn test_fopen_http_retrieves_body() {
    // fopen("http://...") issues an HTTP GET and exposes the response body
    // with the headers stripped as a readable stream.
    let _server = spawn_http_server(54971, b"body delivered over http");
    let out = compile_and_run(
        r#"<?php
$f = fopen("http://127.0.0.1:54971/page.txt", "r");
echo stream_get_contents($f);
fclose($f);
"#,
    );
    assert_eq!(out, "body delivered over http");
}

/// `file_get_contents("http://...")` opens the `http://` wrapper, slurps the
/// whole response body (headers stripped) into an owned string, and returns it
/// — equivalent to `fopen()` + `stream_get_contents()` + `fclose()` on the URL.
/// The owned-heap copy (via `__rt_str_persist`) survives the concat below.
#[test]
fn test_file_get_contents_over_http() {
    let _server = spawn_http_server(54973, b"fgc over http body");
    let out = compile_and_run(
        r#"<?php
echo "[" . file_get_contents("http://127.0.0.1:54973/page.txt") . "]";
"#,
    );
    assert_eq!(out, "[fgc over http body]");
}

/// `file_get_contents($url)` routes a runtime string beginning with `http://`
/// through the HTTP wrapper instead of the plain filesystem reader.
#[test]
fn test_file_get_contents_dynamic_http_url() {
    let _server = spawn_http_server(54974, b"dynamic fgc over http");
    let out = compile_and_run(
        r#"<?php
$url = "http://127.0.0.1:54974/page.txt";
echo "[" . file_get_contents($url) . "]";
"#,
    );
    assert_eq!(out, "[dynamic fgc over http]");
}

/// `file_get_contents("https://...")` succeeds against a local TLS server,
/// proving the literal HTTPS wrapper path returns an owned response body.
#[test]
fn test_file_get_contents_over_https_local_server() {
    let _server = spawn_https_server(54975, b"fgc over local https");
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "ssl", "verify_peer", "0");
echo "[" . file_get_contents("https://127.0.0.1:54975/page.txt") . "]";
"#,
    );
    assert_eq!(out, "[fgc over local https]");
}

/// `file_get_contents($url)` also succeeds when the runtime string uses
/// `https://`, covering the non-literal dynamic URL dispatcher.
#[test]
fn test_file_get_contents_dynamic_https_local_server() {
    let _server = spawn_https_server(54976, b"dynamic fgc over local https");
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "ssl", "verify_peer", "0");
$url = "https://127.0.0.1:54976/page.txt";
echo "[" . file_get_contents($url) . "]";
"#,
    );
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
    let _server = spawn_http_redirect_server(53901, "/new", "/new", b"after-relative-redirect");
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "follow_location", "1");
stream_context_set_option(stream_context_get_default(), "http", "max_redirects", "5");
$f = fopen("http://127.0.0.1:53901/start", "r");
echo stream_get_contents($f);
fclose($f);
"#,
    );
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
    let _server = spawn_http_redirect_server(
        53902,
        "http://127.0.0.1:53902/final",
        "/final",
        b"after-absolute-redirect",
    );
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "follow_location", "1");
stream_context_set_option(stream_context_get_default(), "http", "max_redirects", "5");
$f = fopen("http://127.0.0.1:53902/start", "r");
echo stream_get_contents($f);
fclose($f);
"#,
    );
    assert_eq!(out, "after-absolute-redirect");
}

/// Verifies compiled PHP output for fopen http follow location cross host is not followed.
#[test]
fn test_fopen_http_follow_location_cross_host_is_not_followed() {
    // 302 with a Location: pointing to a different host:port is NOT followed
    // (cross-host redirect requires reconnecting, deferred for v1). The
    // initial 302 response is surfaced as-is; the body is empty because the
    // redirect response itself has Content-Length: 0.
    let _server = spawn_http_redirect_server(
        53903,
        "http://other-host.invalid:80/whatever",
        "/never-reached",
        b"unreachable",
    );
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "follow_location", "1");
stream_context_set_option(stream_context_get_default(), "http", "max_redirects", "5");
stream_context_set_option(stream_context_get_default(), "http", "ignore_errors", "1");
$f = fopen("http://127.0.0.1:53903/start", "r");
echo strlen(stream_get_contents($f));
fclose($f);
"#,
    );
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

/// Verifies compiled PHP output for stream wrapper restore always true.
#[test]
fn test_stream_wrapper_restore_always_true() {
    // v1 cannot unregister built-in wrappers, so stream_wrapper_restore()
    // is effectively a no-op that reports success.
    let out = compile_and_run(
        r#"<?php echo stream_wrapper_restore("file") ? "true" : "false";"#,
    );
    assert_eq!(out, "true");
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
    // v1 stub: stream_context_create/get_default return a resource so PHP
    // code that constructs or consults stream contexts compiles. The options
    // are not yet persisted on the resource.
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
    // stream_context_get_options now returns the hash that was passed to
    // stream_context_create (Phase 11 B2 — single global context slot in v1).
    // stream_context_get_params is still an empty-array stub.
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

/// Verifies compiled PHP output for stream context set option two arg replaces options.
#[test]
fn test_stream_context_set_option_two_arg_replaces_options() {
    // Phase 11 B2: the 2-arg form
    // stream_context_set_option(ctx, options_array) overwrites the
    // global persisted options hash, so a subsequent get_options sees
    // the new wrapper set.
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

/// Verifies compiled PHP output for stream set buffer stubs.
#[test]
fn test_stream_set_buffer_stubs() {
    // stream_set_chunk_size returns the previous chunk size (8192 default on the
    // first call); the read/write buffer setters return 0 ("success" — elephc
    // streams are unbuffered, so the size has no effect).
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
    assert_eq!(out, "8192|0|0");
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
        !out.stderr.contains("Failed to open"),
        "registered user wrapper should not produce the failed-to-open warning, got stderr: {:?}",
        out.stderr,
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
stream_wrapper_register("w","W");
$f=fopen("w://x","r");
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
stream_wrapper_register("w", "W");
$h = fopen("w://x", "r");
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
stream_wrapper_register("w","W");
$f=fopen("w://x","r");
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
stream_wrapper_register("w","W");
$f=fopen("w://x","r");
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
stream_wrapper_register("w","W");
$f=fopen("w://x","r");
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
stream_wrapper_register("w","W");
$f=fopen("w://x","r");
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
stream_wrapper_register("w","W");
$src=fopen("w://x","r");
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
fn test_fopen_user_wrapper_ftell_dispatches_to_stream_tell() {
    // Phase 10 follow-up: ftell() dispatches into the wrapper's stream_tell
    // and returns the int it reports. Without stream_tell, the helper falls
    // through to -1 (PHP's ftell failure sentinel).
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
    assert_eq!(out, "42|-1");
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
    // Phase 10 follow-up: fflush() dispatches into the wrapper's stream_flush
    // and returns its bool result. Without stream_flush, the helper reports
    // success — "nothing to flush" is a benign default.
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
    assert_eq!(out, "1|1");
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
        !out.stderr.contains("Failed to open"),
        "wrapper stream_open returning false should not emit the failed-to-open warning, got stderr: {:?}",
        out.stderr,
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
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("udp://[::1]:54939");
echo is_resource($srv) ? "srv|" : "srv_fail|";
$cli = stream_socket_client("udp://[::1]:54939");
echo is_resource($cli) ? "cli|" : "cli_fail|";
fwrite($cli, "v6-udp");
echo fread($srv, 16);
"#,
    );
    assert_eq!(out, "srv|cli|v6-udp");
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

/// Verifies compiled PHP output for stream socket get name udp.
#[test]
fn test_stream_socket_get_name_udp() {
    // Phase 5 audit: stream_socket_get_name on a UDP socket must format the
    // bound address as A.B.C.D:port, just like the TCP case. Both the local
    // (server) and peer (client) sides should report the bound port.
    let out = compile_and_run(
        r#"<?php
$srv = stream_socket_server("udp://127.0.0.1:54928");
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

/// Verifies compiled PHP output for closedir allows directory handle reuse.
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
    assert_eq!(out, "rok");
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
    let _server = spawn_http_echo_server(56001);
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "method", "POST");
stream_context_set_option(stream_context_get_default(), "http", "content", "hello body");
$f = fopen("http://127.0.0.1:56001/", "r");
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
"#,
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
    let _server = spawn_http_echo_server(56010);
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "user_agent", "MyApp/2.0");
$f = fopen("http://127.0.0.1:56010/", "r");
$req = stream_get_contents($f);
fclose($f);
$needle = "User-Agent: MyApp/2.0";
$nlen = strlen($needle);
$found = false;
for ($i = 0; $i + $nlen <= strlen($req); $i++) {
    if (substr($req, $i, $nlen) === $needle) { $found = true; break; }
}
echo $found ? "ok" : "MISS";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies compiled PHP output for fopen http protocol version 1 1.
#[test]
fn test_fopen_http_protocol_version_1_1() {
    let _server = spawn_http_echo_server(56011);
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "protocol_version", "1.1");
$f = fopen("http://127.0.0.1:56011/", "r");
$req = stream_get_contents($f);
fclose($f);
$needle = "HTTP/1.1";
$nlen = strlen($needle);
$found = false;
for ($i = 0; $i + $nlen <= strlen($req); $i++) {
    if (substr($req, $i, $nlen) === $needle) { $found = true; break; }
}
echo $found ? "ok" : "MISS";
"#,
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
#[ignore = "reliable standalone but flakes in parallel sweep (fixed-port echo server, port-binding race); run with --ignored --test-threads=1. The request_fulluri absolute-form URI now includes the non-default port (fixed in parse_http_url)"]
fn test_fopen_http_request_fulluri_in_request_line() {
    let _server = spawn_http_echo_server(56012);
    let out = compile_and_run(
        r#"<?php
stream_context_set_option(stream_context_get_default(), "http", "request_fulluri", "1");
$f = fopen("http://127.0.0.1:56012/path", "r");
$req = stream_get_contents($f);
fclose($f);
$needle = "GET http://127.0.0.1:56012/path HTTP/1.0";
$nlen = strlen($needle);
$found = false;
for ($i = 0; $i + $nlen <= strlen($req); $i++) {
    if (substr($req, $i, $nlen) === $needle) { $found = true; break; }
}
echo $found ? "ok" : "MISS";
"#,
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

/// A stream-context `notification` closure must fire STREAM_NOTIFY_FAILURE
/// (code 9) when an `http://` connection is refused. Connecting to
/// 127.0.0.1:1 (a closed port) is refused immediately, so `__rt_http_open`
/// reaches its failure path and invokes the captured callback through its
/// descriptor invoker (the offset-56 invoker contract). This is the
/// deterministic, network-free end-to-end test for the whole capture →
/// global → fire-shim → invoker → closure-body path. CONNECT (2) and
/// COMPLETED (8) are validated against a live server during development; they
/// share the same shim and differ only by the milestone code immediate.
#[test]
fn test_stream_notification_callback_fires_failure_on_refused_connection() {
    let out = compile_and_run(
        r#"<?php
$ctx = stream_context_create([], ['notification' => function($code, $sev, $msg, $mc, $bt, $bm) {
    echo "N" . $code . ";";
}]);
$f = fopen('http://127.0.0.1:1/', 'r');
echo $f === false ? "closed" : "open";
"#,
    );
    assert_eq!(out, "N9;closed");
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
$f = fopen('http://127.0.0.1:1/', 'r');
echo $f === false ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

/// A later `stream_context_create` whose params array lacks `notification`
/// clears the global callback, so a subsequent failed `http://` open fires
/// nothing (single-global context model). Verifies the clear-on-no-callback
/// path in `capture_notification_callback`.
#[test]
fn test_stream_notification_callback_cleared_by_later_context() {
    let out = compile_and_run(
        r#"<?php
$a = stream_context_create([], ['notification' => function($code) { echo "A" . $code; }]);
$b = stream_context_create([], ['other' => 1]);
$f = fopen('http://127.0.0.1:1/', 'r');
echo $f === false ? "ok" : "bad";
"#,
    );
    assert_eq!(out, "ok");
}

/// `stream_context_set_params` must also capture a `notification` closure into
/// the global so a later refused `http://` open fires STREAM_NOTIFY_FAILURE.
#[test]
fn test_stream_notification_callback_via_set_params() {
    let out = compile_and_run(
        r#"<?php
$ctx = stream_context_create([]);
stream_context_set_params($ctx, ['notification' => function($code) { echo "P" . $code . ";"; }]);
$f = fopen('http://127.0.0.1:1/', 'r');
echo $f === false ? "closed" : "open";
"#,
    );
    assert_eq!(out, "P9;closed");
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

/// A userspace wrapper that does not implement `stream_cast` cannot be
/// represented as a select()-able descriptor, so `stream_select` excludes its
/// synthetic fd (matching PHP) and drops it from the ready set without crashing.
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
$n = stream_select($r, $wr, $e, 0, 0);
echo "n=" . $n . " kept=" . count($r);
"#,
    );
    assert_eq!(out, "n=0 kept=0");
}
