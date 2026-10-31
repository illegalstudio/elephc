//! Purpose:
//! Exercises the `parse_str` prelude declaration and its `filter.default` post-pass.
//!
//! Called from:
//! - `cargo test` through the strings integration suite.
//!
//! Key details:
//! - Parsing reuses the native `mb_parse_str` query engine, including bracketed keys.
//! - The untyped by-reference result parameter must be populated through the caller's variable.
//! - A configured `filter.default` filters every parsed leaf and turns rejected values into `""`.

use crate::support::*;

/// Parses flat keys, indexed lists, and named nested keys into the caller's variable.
#[test]
fn test_parse_str_populates_nested_output() {
    let out = compile_and_run(
        r#"<?php
parse_str("a=1&b[]=2&b[]=3&c[x]=4", $out);
echo $out["a"], "|", $out["b"][0], "|", $out["b"][1], "|", $out["c"]["x"];
"#,
    );
    assert_eq!(out, "1|2|3|4");
}

/// Named arguments bind to `string` and the by-reference `result` parameter.
#[test]
fn test_parse_str_named_arguments() {
    let out = compile_and_run(
        r#"<?php
parse_str(result: $out, string: "a=1&b=2");
echo $out["a"], "|", $out["b"];
"#,
    );
    assert_eq!(out, "1|2");
}

/// `filter.default=int` rejects `b=notint` into the empty string, matching PHP's input filter.
#[test]
fn test_parse_str_applies_filter_default_and_rejects_invalid_values() {
    let output = compile_and_run_capture_with_ini(
        r#"<?php
parse_str("a=1&b=notint", $out);
var_dump($out);
"#,
        &[("filter.default".to_string(), "int".to_string())],
    );
    assert!(output.stdout.contains("string(1) \"1\""), "stdout: {:?}", output.stdout);
    assert!(output.stdout.contains("string(0) \"\""), "stdout: {:?}", output.stdout);
    assert!(!output.stdout.contains("bool(false)"), "stdout: {:?}", output.stdout);
}

/// A configured sanitizer filters nested values recursively.
#[test]
fn test_parse_str_filter_default_walks_nested_arrays() {
    let output = compile_and_run_capture_with_ini(
        r#"<?php
parse_str("a=%3Cb%3E&list[]=%3Ci%3E", $out);
echo $out["a"], "|", $out["list"][0];
"#,
        &[("filter.default".to_string(), "special_chars".to_string())],
    );
    assert_eq!(output.stdout, "&#60;b&#62;|&#60;i&#62;");
}
