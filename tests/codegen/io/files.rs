//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of I/O files, including file put get contents, file get contents missing emits runtime warning, and file get contents missing is strict false.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

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
