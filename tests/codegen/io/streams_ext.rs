//! Purpose:
//! Integration tests for stream-extension builtins: fgetc, readfile, fpassthru,
//! flock, and tmpfile.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each test uses a fresh temporary directory; the helpers in `support` keep
//!   the working directory isolated for parallel test runs.

use super::*;

/// Verifies `fgetc` reads exactly one byte and advances the file pointer.
/// Fixture: a 3-byte file read three times sequentially.
#[test]
fn test_fgetc_reads_one_byte() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("c.txt", "abc");
$h = fopen("c.txt", "r");
echo fgetc($h) . fgetc($h) . fgetc($h);
fclose($h);
"#,
    );
    assert_eq!(out, "abc");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `fgetc` returns `false` once the stream reaches EOF.
/// Fixture: a 1-byte file read twice; the second call hits EOF.
#[test]
fn test_fgetc_returns_false_at_eof() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("eof.txt", "x");
$h = fopen("eof.txt", "r");
fgetc($h);
$tail = fgetc($h);
echo $tail === false ? "false" : "not false";
fclose($h);
"#,
    );
    assert_eq!(out, "false");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `fgetc` returns `false` when the underlying descriptor cannot be read
/// (e.g., opening a directory as a file).
#[test]
fn test_fgetc_returns_false_on_read_error() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("as-dir");
$h = fopen("as-dir", "r");
$c = fgetc($h);
echo $c === false ? "false" : "not-false:" . strlen($c);
fclose($h);
rmdir("as-dir");
"#,
    );
    assert_eq!(out, "false");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `readfile` writes file contents to stdout and returns the byte count.
#[test]
fn test_readfile_streams_to_stdout() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("rf.txt", "hello world");
$bytes = readfile("rf.txt");
echo "|" . $bytes;
"#,
    );
    assert_eq!(out, "hello world|11");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `readfile` returns `-1` when the file cannot be read (directory path).
#[test]
fn test_readfile_read_error_returns_minus_one() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("as-dir");
$bytes = readfile("as-dir");
echo "|" . $bytes;
rmdir("as-dir");
"#,
    );
    assert_eq!(out, "|-1");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `readfile` returns `false` when the file does not exist.
#[test]
fn test_readfile_missing_returns_false() {
    let out = compile_and_run(
        r#"<?php
$bytes = readfile("/nonexistent/path/xyz.txt");
echo $bytes === false ? "false" : "not false";
"#,
    );
    assert_eq!(out, "false");
}

/// Verifies `readfile` returns `0` and outputs nothing for an empty file.
#[test]
fn test_readfile_empty_file_returns_zero() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("empty.txt", "");
$bytes = readfile("empty.txt");
echo "|" . $bytes;
"#,
    );
    assert_eq!(out, "|0");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `readfile` correctly handles a file larger than the internal read buffer.
/// Regression: codegen was previously producing incorrect output for files ≥ ~4096 bytes.
#[test]
fn test_readfile_large_buffer_loop() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$payload = str_repeat("A", 5000);
file_put_contents("big.txt", $payload);
$bytes = readfile("big.txt");
echo "|" . $bytes;
"#,
    );
    assert!(out.starts_with(&"A".repeat(5000)), "got: {}", out);
    assert!(out.ends_with("|5000"), "got: {}", out);
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `fpassthru` outputs all remaining bytes from the current position to EOF
/// and returns the byte count.
#[test]
fn test_fpassthru_streams_remaining_bytes() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("pt.txt", "abcdefghij");
$h = fopen("pt.txt", "r");
fread($h, 4);
$bytes = fpassthru($h);
echo "|" . $bytes;
fclose($h);
"#,
    );
    assert_eq!(out, "efghij|6");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `fpassthru` sets EOF on the stream after a successful full read.
#[test]
fn test_fpassthru_sets_eof_after_success() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("pt-eof.txt", "abc");
$h = fopen("pt-eof.txt", "r");
$bytes = fpassthru($h);
echo "|" . $bytes . "|" . (feof($h) ? "eof" : "not-eof");
fclose($h);
"#,
    );
    assert_eq!(out, "abc|3|eof");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `fpassthru` returns `-1` when the underlying descriptor cannot be read.
#[test]
fn test_fpassthru_read_error_returns_minus_one() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("as-dir");
$h = fopen("as-dir", "r");
$bytes = fpassthru($h);
echo "|" . $bytes;
fclose($h);
rmdir("as-dir");
"#,
    );
    assert_eq!(out, "|-1");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `fpassthru` sets EOF on the stream after a read error.
#[test]
fn test_fpassthru_sets_eof_after_read_error() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("as-dir");
$h = fopen("as-dir", "r");
$bytes = fpassthru($h);
echo "|" . $bytes . "|" . (feof($h) ? "eof" : "not-eof");
fclose($h);
rmdir("as-dir");
"#,
    );
    assert_eq!(out, "|-1|eof");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `flock(LOCK_EX)` acquires an exclusive lock and `flock(LOCK_UN)` releases it.
#[test]
fn test_flock_exclusive_then_unlock() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("lock.txt", "data");
$h = fopen("lock.txt", "r+");
$got = flock($h, LOCK_EX);
$released = flock($h, LOCK_UN);
fclose($h);
echo ($got ? "y" : "n") . "|" . ($released ? "y" : "n");
"#,
    );
    assert_eq!(out, "y|y");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `flock(LOCK_SH)` acquires a shared lock.
#[test]
fn test_flock_shared() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ls.txt", "");
$h = fopen("ls.txt", "r");
echo flock($h, LOCK_SH) ? "y" : "n";
flock($h, LOCK_UN);
fclose($h);
"#,
    );
    assert_eq!(out, "y");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `flock` with `LOCK_NB` returns `false` and populates the would-block
/// output parameter when the lock cannot be acquired without blocking.
#[test]
fn test_flock_sets_would_block_output() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("block.txt", "x");
$first = fopen("block.txt", "r+");
$second = fopen("block.txt", "r+");
flock($first, LOCK_EX);
$would = 0;
$ok = flock($second, LOCK_EX | LOCK_NB, $would);
echo ($ok ? "locked" : "blocked") . "|" . gettype($would) . ":" . $would;
flock($first, LOCK_UN);
fclose($second);
fclose($first);
"#,
    );
    assert_eq!(out, "blocked|integer:1");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `flock` with named arguments (`stream:`, `operation:`, `would_block:`)
/// works correctly and the `would_block` output is set to `0` on success.
#[test]
fn test_flock_named_would_block_output_success() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("named-lock.txt", "x");
$h = fopen("named-lock.txt", "r+");
$would = 99;
$ok = flock(stream: $h, operation: LOCK_EX, would_block: $would);
echo ($ok ? "locked" : "blocked") . "|" . gettype($would) . ":" . $would;
flock($h, LOCK_UN);
fclose($h);
"#,
    );
    assert_eq!(out, "locked|integer:0");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `tmpfile` creates a writable stream, data can be written and read back.
#[test]
fn test_tmpfile_returns_writable_resource() {
    let out = compile_and_run(
        r#"<?php
$h = tmpfile();
$wrote = fwrite($h, "scratch");
fseek($h, 0);
$content = fread($h, 7);
fclose($h);
echo $wrote . "|" . $content;
"#,
    );
    assert_eq!(out, "7|scratch");
}

/// Verifies `tmpfile` returns a resource type (not `false`) and `gettype` reports "resource".
#[test]
fn test_tmpfile_returns_resource_type() {
    let out = compile_and_run(
        r#"<?php
$h = tmpfile();
echo gettype($h) . "|";
echo $h === false ? "false" : "resource";
fclose($h);
"#,
    );
    assert_eq!(out, "resource|resource");
}

/// Verifies `tmpfile` does not inherit EOF state from a previously-used descriptor.
/// Regression: a prior implementation reused file descriptors without clearing EOF flag.
#[test]
fn test_tmpfile_clears_stale_eof_for_reused_descriptor() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("seed.txt", "x");
$f = fopen("seed.txt", "r");
fread($f, 1);
fread($f, 1);
fclose($f);
$h = tmpfile();
echo feof($h) ? "eof" : "not-eof";
fclose($h);
unlink("seed.txt");
"#,
    );
    assert_eq!(out, "not-eof");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `tmpfile(...[])` accepts an empty variadic spread without error.
#[test]
fn test_tmpfile_accepts_empty_spread() {
    let out = compile_and_run(
r#"<?php
$h = tmpfile(...[]);
echo gettype($h);
fclose($h);
"#,
    );
    assert_eq!(out, "resource");
}

/// Verifies `LOCK_SH`, `LOCK_EX`, `LOCK_UN`, and `LOCK_NB` have the same integer
/// values as in PHP (1, 2, 3, 4 respectively).
#[test]
fn test_lock_constants_have_php_values() {
    let out = compile_and_run(
        r#"<?php echo LOCK_SH . "|" . LOCK_EX . "|" . LOCK_UN . "|" . LOCK_NB;"#,
    );
    assert_eq!(out, "1|2|3|4");
}

/// Verifies `function_exists` returns `true` for all stream extension builtins:
/// fgetc, readfile, fpassthru, flock, tmpfile.
#[test]
fn test_function_exists_streams_ext() {
    let out = compile_and_run(
        r#"<?php
echo function_exists('fgetc') ? "y" : "n";
echo function_exists('readfile') ? "y" : "n";
echo function_exists('fpassthru') ? "y" : "n";
echo function_exists('flock') ? "y" : "n";
echo function_exists('tmpfile') ? "y" : "n";
"#,
    );
    assert_eq!(out, "yyyyy");
}

/// Verifies stream extension builtins are callable with uppercase names (PHP case-insensitivity).
#[test]
fn test_streams_ext_case_insensitive_calls() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ci.txt", "ok");
$bytes = READFILE("ci.txt");
echo "|" . $bytes;
"#,
    );
    assert_eq!(out, "ok|2");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies stream extension builtins are resolved via PHP's namespace fallback
/// when called from within a namespace block.
#[test]
fn test_streams_ext_namespace_fallback() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
namespace App;
file_put_contents("ns.txt", "hi");
$bytes = readfile("ns.txt");
echo "|" . $bytes;
"#,
    );
    assert_eq!(out, "hi|2");
    let _ = fs::remove_dir_all(&dir);
}
