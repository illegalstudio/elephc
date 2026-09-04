//! Purpose:
//! Integration tests for handing a DIRECTORY to the functions that read files: `copy()` refuses
//! it with a sentence of its own, and `file_get_contents()` answers the empty string after
//! saying why.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The read failed and its RESULT WAS USED AS A LENGTH. On macOS a failed `read(2)` answers the
//!   errno itself, so `EISDIR` became a 21-byte string of uninitialised heap:
//!   `file_get_contents()` handed it back, and `copy()` wrote it out and answered TRUE.
//! - php refuses the directory in `copy()` before it opens anything, so the destination is never
//!   touched — a separate rule from the read failure, with its own wording.
//! - php sizes its read `st_size + CHUNK` so ONE read can also see the end of the file, and the
//!   Notice names that number. Asking for exactly `st_size` reported a count php never asks for.
//! - The byte count is the only part of the Notice that is not asserted here: it is
//!   `st_size + 8192`, and an empty directory's `st_size` is 64 on macOS and 4096 on Linux.
//! - Every expectation was measured on `php -n` 8.5.6.

use crate::support::*;

/// Verifies `copy()` refuses a directory source, says why, and leaves the destination alone.
#[test]
fn test_copy_refuses_a_directory_source() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("srcdir");
file_put_contents("dst.txt", "untouched");
var_dump(copy("srcdir", "dst.txt"));
echo file_get_contents("dst.txt"), "\n";
unlink("dst.txt");
rmdir("srcdir");
"#,
    );
    assert_eq!(out, "bool(false)\nuntouched\n");
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies the refusal is php's own sentence, and that `@` suppresses it like any warning.
#[test]
fn test_the_copy_refusal_is_phps_wording() {
    let out = compile_and_run_capture(
        r#"<?php
mkdir("srcdir");
var_dump(copy("srcdir", "dst.txt"));
var_dump(@copy("srcdir", "dst.txt"));
rmdir("srcdir");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nbool(false)\n");
    assert_eq!(
        out.diagnostics,
        "Warning: copy(): The first argument to copy() function cannot be a directory\n"
    );
}

/// Verifies `file_get_contents()` answers the EMPTY STRING for a directory, not garbage.
///
/// The open succeeds — a directory can be opened — so php has a string to answer, and it is
/// empty. This used to be a 21-byte string of uninitialised heap, `EISDIR` read as a length.
#[test]
fn test_file_get_contents_of_a_directory_is_the_empty_string() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("readme");
$content = @file_get_contents("readme");
var_dump($content, $content === "", $content === false);
rmdir("readme");
"#,
    );
    assert_eq!(out, "string(0) \"\"\nbool(true)\nbool(false)\n");
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies php's Notice for the failed read, in php's own words.
///
/// The byte count is left out of the assertion on purpose: php reports `st_size + 8192`, and an
/// empty directory's `st_size` is not the same number on macOS and Linux.
#[test]
fn test_the_failed_read_says_why_in_phps_words() {
    let out = compile_and_run_capture(
        r#"<?php
mkdir("readme");
file_get_contents("readme");
rmdir("readme");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let notice = out.diagnostics.trim_end();
    assert!(
        notice.starts_with("Notice: file_get_contents(): Read of "),
        "unexpected diagnostic: {notice:?}"
    );
    assert!(
        notice.ends_with(" bytes failed with errno=21 Is a directory"),
        "unexpected diagnostic: {notice:?}"
    );
}

/// Verifies an ORDINARY file still reads whole, now that the read asks for a chunk more.
///
/// The control for the sizing change: the answer is the file's bytes and nothing else, and a
/// file larger than one chunk is not truncated.
#[test]
fn test_an_ordinary_file_still_reads_whole() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("small.txt", "hello");
file_put_contents("big.txt", str_repeat("ab", 20000));
$small = file_get_contents("small.txt");
$big = file_get_contents("big.txt");
var_dump($small, strlen($big), $big[0], $big[39999]);
unlink("small.txt");
unlink("big.txt");
"#,
    );
    assert_eq!(
        out,
        "string(5) \"hello\"\nint(40000)\nstring(1) \"a\"\nstring(1) \"b\"\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}
