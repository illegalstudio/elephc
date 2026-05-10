//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of strings formatting, including sprintf string, sprintf integer, and sprintf multiple.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_sprintf_string() {
    let out = compile_and_run(r#"<?php echo sprintf("Hello %s", "World");"#);
    assert_eq!(out, "Hello World");
}

#[test]
fn test_sprintf_int() {
    let out = compile_and_run(r#"<?php echo sprintf("Value: %d", 42);"#);
    assert_eq!(out, "Value: 42");
}

#[test]
fn test_sprintf_multiple() {
    let out = compile_and_run(r#"<?php echo sprintf("%s is %d", "age", 30);"#);
    assert_eq!(out, "age is 30");
}

#[test]
fn test_sprintf_percent() {
    let out = compile_and_run(r#"<?php echo sprintf("100%%");"#);
    assert_eq!(out, "100%");
}

#[test]
fn test_sprintf_precision_float() {
    let out = compile_and_run(r#"<?php echo sprintf("%.2f", 3.14159);"#);
    assert_eq!(out, "3.14");
}

#[test]
fn test_sprintf_width_string() {
    let out = compile_and_run(r#"<?php echo sprintf("%10s", "hi");"#);
    assert_eq!(out, "        hi");
}

#[test]
fn test_sprintf_left_align_string() {
    let out = compile_and_run(r#"<?php echo sprintf("%-10s|", "hi");"#);
    assert_eq!(out, "hi        |");
}

#[test]
fn test_sprintf_plus_sign() {
    let out = compile_and_run(r#"<?php echo sprintf("%+d", 42);"#);
    assert_eq!(out, "+42");
}

#[test]
fn test_sprintf_precision_float_trailing_zeros() {
    let out = compile_and_run(r#"<?php echo sprintf("%.5f", 1.0);"#);
    assert_eq!(out, "1.00000");
}

#[test]
fn test_sprintf_float_default() {
    let out = compile_and_run(r#"<?php echo sprintf("%f", 3.14);"#);
    assert_eq!(out, "3.140000");
}

#[test]
fn test_printf() {
    let out = compile_and_run(r#"<?php printf("Hello %s", "World");"#);
    assert_eq!(out, "Hello World");
}

// --- String interpolation ---

#[test]
fn test_sscanf_int() {
    let out = compile_and_run(
        r#"<?php
$result = sscanf("Age: 25", "Age: %d");
echo $result[0];
"#,
    );
    assert_eq!(out, "25");
}

#[test]
fn test_sscanf_string() {
    let out = compile_and_run(
        r#"<?php
$result = sscanf("Name: Alice", "Name: %s");
echo $result[0];
"#,
    );
    assert_eq!(out, "Alice");
}

#[test]
fn test_sscanf_multiple() {
    let out = compile_and_run(
        r#"<?php
$result = sscanf("John 30", "%s %d");
echo $result[0] . " " . $result[1];
"#,
    );
    assert_eq!(out, "John 30");
}
