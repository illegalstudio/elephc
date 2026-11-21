//! Purpose:
//! End-to-end tests for `preg_filter()`, the injected PCRE prelude function.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext preg_filter`.
//!
//! Key details:
//! - Expected output is PHP 8.5's.
//! - The eval fixture builds its source at runtime so it runs through Magician.

use crate::support::*;

/// The PHP calls every fixture runs, compiled and through eval.
const FILTER_CALLS: &str = r#"var_dump(preg_filter('/(\d+)/', '<$1>', 'a12b'));
var_dump(preg_filter('/\d/', 'x', 'abc'));
var_dump(PREG_FILTER('/b/', '', 'abc'));
"#;

/// PHP 8.5's output for [`FILTER_CALLS`].
const FILTER_OUTPUT: &str = "string(6) \"a<12>b\"\nNULL\nstring(2) \"ac\"\n";

/// Replaces like `preg_replace()` and answers `null` for no match or an invalid pattern.
#[test]
fn test_preg_filter_matches_php() {
    let out = compile_and_run_capture_with_regex(&format!(
        "<?php\n{FILTER_CALLS}var_dump(@preg_filter('/(/', 'x', 'abc'));\n"
    ));
    assert_eq!(out.stdout, format!("{FILTER_OUTPUT}NULL\n"));
}

/// Eval binds `preg_filter()` with the same results. An invalid pattern stays the runtime
/// fatal every eval `preg_*` builtin but `preg_grep()` raises, so it is not exercised here.
#[test]
fn test_preg_filter_in_eval() {
    let quoted = FILTER_CALLS.replace('\\', "\\\\").replace('\'', "\\'");
    let out = compile_and_run_capture_with_regex(&format!(
        "<?php\neval('{quoted}' . ' // ' . $argc);\n"
    ));
    assert_eq!(out.stdout, FILTER_OUTPUT);
}
