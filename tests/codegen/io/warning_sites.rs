//! Purpose:
//! Regression tests for the source location published before PHP warnings raised by element
//! reads, including the hot-path deferral of the diagnostic site.
//!
//! Called from:
//! - `cargo test` through Rust's test harness via `tests/codegen/io.rs`.
//!
//! Key details:
//! - Warned element reads publish `_php_diagnostic_file`/`_php_diagnostic_line` only on their
//!   miss path, except for float keys whose conversion deprecation runs before any miss branch.
//! - The fixtures assert both the user-visible warning text and that it names the read's line.

use super::*;

/// A missing string key reports its warning at the read's source line after the site deferral.
#[test]
fn test_hash_miss_warning_keeps_its_source_line() {
    let out = compile_and_run_capture(
        r#"<?php
$a = ['k' => 1];
echo $a['missing'];
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains(r#"Warning: Undefined array key "missing""#)
            && out.stderr.contains("on line 3"),
        "warning must name line 3: {}",
        out.stderr
    );
}

/// A missing integer key reports its warning at the read's source line.
#[test]
fn test_indexed_miss_warning_keeps_its_source_line() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [1, 2];
echo $a[5];
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr.contains("Warning: Undefined array key 5")
            && out.stderr.contains("on line 3"),
        "warning must name line 3: {}",
        out.stderr
    );
}

/// A read from a null receiver reports PHP's null-offset warning at the read's line.
#[test]
fn test_null_receiver_warning_keeps_its_source_line() {
    let out = compile_and_run_capture(
        r#"<?php
$a = $argc > 1 ? ['k' => 1] : null;
echo $a['k'];
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "");
    assert!(
        out.stderr
            .contains("Warning: Trying to access array offset on")
            && out.stderr.contains("on line 3"),
        "warning must name line 3: {}",
        out.stderr
    );
}

/// A float key needs its diagnostic site before key normalization, so it stays eager.
#[test]
fn test_float_key_deprecation_keeps_its_source_line() {
    let out = compile_and_run_capture(
        r#"<?php
$a = [0 => 'zero', 1 => 'one'];
echo $a[1.5];
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "one");
    assert!(
        out.stderr
            .contains("Deprecated: Implicit conversion from float 1.5 to int loses precision")
            && out.stderr.contains("on line 3"),
        "deprecation must name line 3: {}",
        out.stderr
    );
}

/// A string-key hash read defers the diagnostic site to its miss path; the fast path pays no
/// file/line stores.
#[test]
fn test_hash_read_publishes_the_diagnostic_site_only_on_the_miss_path() {
    let dir = make_cli_test_dir("elephc_hash_warning_site");
    let (user_asm, _runtime_asm, _libs) = compile_source_to_asm_with_options(
        r#"<?php
$a = ['k' => 1, 'j' => 2];
$k = $argc > 1 ? 'j' : 'k';
echo $a[$k];
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    let read_start = user_asm
        .find("op=hash_get")
        .expect("fixture must lower a hash read");
    let read = &user_asm[read_start..];
    let call = read
        .find("__rt_hash_get")
        .expect("hash read must call the runtime getter");
    assert!(
        !read[..call].contains("_php_diagnostic_file"),
        "hash-read fast path must not publish the diagnostic site:\n{}",
        &read[..call]
    );
    let miss_start = user_asm
        .find("hash_get_miss")
        .expect("hash read must branch to a miss path");
    let miss = &user_asm[miss_start..];
    let miss_end = miss
        .find("hash_get_fallback")
        .expect("hash miss must join the null fallback");
    assert!(
        miss[..miss_end].contains("_php_diagnostic_file"),
        "hash-read miss path must publish the diagnostic site:\n{}",
        &miss[..miss_end]
    );
    let _ = fs::remove_dir_all(dir);
}

/// An indexed read with an integer key defers the site the same way; its null fallback already
/// published the file/line before the warning.
#[test]
fn test_indexed_read_defers_the_diagnostic_site() {
    let dir = make_cli_test_dir("elephc_indexed_warning_site");
    let (user_asm, _runtime_asm, _libs) = compile_source_to_asm_with_options(
        r#"<?php
$a = [1, 2];
$k = $argc > 1 ? 1 : 0;
echo $a[$k];
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    let read_start = user_asm
        .find("op=array_get")
        .expect("fixture must lower an indexed read");
    let read = &user_asm[read_start..];
    let null_branch = read
        .find("array_get_null")
        .expect("indexed read must branch to its null path");
    assert!(
        !read[..null_branch].contains("_php_diagnostic_file"),
        "indexed-read fast path must not publish the diagnostic site:\n{}",
        &read[..null_branch]
    );
    let _ = fs::remove_dir_all(dir);
}
