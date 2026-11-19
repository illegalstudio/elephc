//! Purpose:
//! Runs PHP 8.6's imprecise base-conversion notice fixtures.
//!
//! Called from:
//! - `cargo test --test codegen_tests base_conversion_notice_php86`.
//!
//! Key details:
//! - `base_convert()`, `bindec()`, `hexdec()` and `octdec()` report an `E_NOTICE` when the
//!   integer accumulator widens to `double`; 8.5 and earlier stay silent.
//! - The notice is emitted by the shared `__rt_base_to_number` helper, so every entry point
//!   that composes it is covered.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// The php-src 8.6 wording, including its leading category prefix as elephc stores it.
const NOTICE: &str = "Notice: Input number is larger than PHP_INT_MAX, precision has been lost in conversion";

/// Compiles and runs one fixture with the PHP 8.6 profile, merging stderr into stdout.
fn run86(source: &str) -> String {
    compile_and_run_capture_merged_with_php_version(source, PhpVersion::Php86)
}

/// Compiles and runs one fixture with the PHP 8.5 profile, merging stderr into stdout.
fn run85(source: &str) -> String {
    compile_and_run_capture_merged_with_php_version(source, PhpVersion::Php85)
}

/// Verifies `bindec()` reports the 8.6 precision-loss notice once it widens to float.
#[test]
fn test_bindec_overflow_notice_php86() {
    let source = r#"<?php var_dump(bindec(str_repeat('1', 64)));"#;
    let out86 = run86(source);
    assert!(out86.contains(NOTICE), "PHP 8.6 must notice the widening: {out86}");
    assert!(out86.contains("float("), "the widened value is a float: {out86}");
    let out85 = run85(source);
    assert!(!out85.contains(NOTICE), "PHP 8.5 must stay silent: {out85}");
}

/// Verifies `hexdec()` reports the 8.6 precision-loss notice once it widens to float.
#[test]
fn test_hexdec_overflow_notice_php86() {
    let source = r#"<?php var_dump(hexdec('ffffffffffffffff'));"#;
    let out86 = run86(source);
    assert!(out86.contains(NOTICE), "PHP 8.6 must notice the widening: {out86}");
    assert!(out86.contains("float("), "the widened value is a float: {out86}");
    let out85 = run85(source);
    assert!(!out85.contains(NOTICE), "PHP 8.5 must stay silent: {out85}");
}

/// Verifies `octdec()` reports the 8.6 precision-loss notice once it widens to float.
#[test]
fn test_octdec_overflow_notice_php86() {
    let source = r#"<?php var_dump(octdec(str_repeat('7', 22)));"#;
    let out86 = run86(source);
    assert!(out86.contains(NOTICE), "PHP 8.6 must notice the widening: {out86}");
    assert!(out86.contains("float("), "the widened value is a float: {out86}");
    let out85 = run85(source);
    assert!(!out85.contains(NOTICE), "PHP 8.5 must stay silent: {out85}");
}

/// Verifies `base_convert()` reports the 8.6 precision-loss notice through the same helper.
#[test]
fn test_base_convert_overflow_notice_php86() {
    let source = r#"<?php echo base_convert('ffffffffffffffff', 16, 10), "\n";"#;
    let out86 = run86(source);
    assert!(out86.contains(NOTICE), "PHP 8.6 must notice the widening: {out86}");
    // php-src renders the widened value through its lossy float loop, so the digits are
    // `18446744073709552046`, not the exact 2^64-1.
    assert!(out86.contains("18446744073709552046"), "widened conversion value: {out86}");
    let out85 = run85(source);
    assert!(!out85.contains(NOTICE), "PHP 8.5 must stay silent: {out85}");
}

/// Verifies exact conversions stay silent on the 8.6 profile.
#[test]
fn test_base_conversion_exact_values_stay_quiet_php86() {
    let source = r#"<?php
var_dump(bindec('111'));
var_dump(hexdec('ff'));
var_dump(octdec('777'));
echo base_convert('ff', 16, 10), "\n";
var_dump(bindec(str_repeat('1', 63)));"#;
    let out = run86(source);
    assert!(!out.contains(NOTICE), "exact conversions must not notice: {out}");
    assert!(out.contains("int(7)"), "{out}");
    assert!(out.contains("int(255)"), "{out}");
    assert!(out.contains("int(511)"), "{out}");
    assert!(out.contains("9223372036854775807"), "{out}");
}
