//! Purpose:
//! Integration tests for symbolic / hard link builtins: symlink, link,
//! readlink, linkinfo.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Tests run in isolated tmp directories so concurrent test runs do not
//!   clash on link/file names.

use super::*;

/// Verifies `symlink()` creates a valid symbolic link and that reads through
/// the link return the original file's contents.
#[test]
fn test_symlink_creates_link() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("orig.txt", "hi");
$ok = symlink("orig.txt", "link.txt");
echo $ok ? "y" : "n";
echo "|" . file_get_contents("link.txt");
unlink("link.txt");
unlink("orig.txt");
"#,
    );
    assert_eq!(out, "y|hi");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `link()` creates a hard link and that reads through the hard link
/// return the original file's contents.
#[test]
fn test_link_creates_hard_link() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("orig.txt", "payload");
$ok = link("orig.txt", "hard.txt");
echo $ok ? "y" : "n";
echo "|" . file_get_contents("hard.txt");
unlink("hard.txt");
unlink("orig.txt");
"#,
    );
    assert_eq!(out, "y|payload");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `readlink()` returns the target path string for an existing
/// symbolic link.
#[test]
fn test_readlink_returns_target_path() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("orig.txt", "data");
symlink("orig.txt", "soft.txt");
$target = readlink("soft.txt");
echo $target;
unlink("soft.txt");
unlink("orig.txt");
"#,
    );
    assert_eq!(out, "orig.txt");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `readlink()` returns `false` (PHP `=== false` check) when the
/// path does not exist or is not a symlink.
#[test]
fn test_readlink_missing_returns_false() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$res = readlink("/nonexistent/missing-link");
echo $res === false ? "false" : "ok";
"#,
    );
    assert_eq!(out, "false");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `linkinfo()` returns a positive (non-zero) device number for an
/// existing symlink.
#[test]
fn test_linkinfo_returns_nonzero_dev_for_existing_link() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("orig.txt", "x");
symlink("orig.txt", "soft.txt");
$info = linkinfo("soft.txt");
echo $info > 0 ? "y" : "n";
unlink("soft.txt");
unlink("orig.txt");
"#,
    );
    assert_eq!(out, "y");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `linkinfo()` returns `-1` when the path does not exist.
#[test]
fn test_linkinfo_returns_minus_one_for_missing() {
    let out = compile_and_run(
        r#"<?php
echo linkinfo("/nonexistent/missing-path");
"#,
    );
    assert_eq!(out, "-1");
}

/// Verifies all four link-related builtins (`symlink`, `link`, `readlink`,
/// `linkinfo`) are registered and return `true` from `function_exists()`.
#[test]
fn test_function_exists_symlinks() {
    let out = compile_and_run(
        r#"<?php
echo function_exists('symlink') ? "y" : "n";
echo function_exists('link') ? "y" : "n";
echo function_exists('readlink') ? "y" : "n";
echo function_exists('linkinfo') ? "y" : "n";
"#,
    );
    assert_eq!(out, "yyyy");
}

/// Verifies PHP's case-insensitive builtin function name resolution: calling
/// `SYMLINK()` in uppercase still resolves to the `symlink` builtin.
#[test]
fn test_symlinks_case_insensitive_calls() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("orig.txt", "ok");
SYMLINK("orig.txt", "link.txt");
echo file_get_contents("link.txt");
unlink("link.txt");
unlink("orig.txt");
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that inside a namespace, `symlink` and `readlink` are reachable
/// without a `use` statement via PHP's builtin namespace fallback.
#[test]
fn test_symlinks_namespace_fallback() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
namespace App;
file_put_contents("orig.txt", "ns");
symlink("orig.txt", "link.txt");
echo readlink("link.txt");
unlink("link.txt");
unlink("orig.txt");
"#,
    );
    assert_eq!(out, "orig.txt");
    let _ = fs::remove_dir_all(&dir);
}
