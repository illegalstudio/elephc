//! Purpose:
//! Provides JSON string-encoding reservation tests for large inputs.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - The string encoder sizes its output in a first pass: plain printable bytes cost one byte, and
//!   only escapable or non-ASCII bytes are budgeted at the six-byte `\u00XX` worst case.

use super::*;

/// Verifies a long plain string encodes on a heap that holds the input and its encoding but not
/// six times the input: the old up-front `6 * len + 2` reservation exhausted the 8 MB test heap
/// for a 1.3 MB string, where PHP succeeds.
#[test]
fn test_json_encode_large_plain_string_reserves_only_what_it_needs() {
    let out = compile_and_run(
        r#"<?php
$s = json_encode(str_repeat("a", 1300000 + $argc - 1));
echo strlen($s), ":", substr($s, 0, 3), ":", substr($s, -3);
"#,
    );
    assert_eq!(out, "1300002:\"aa:aa\"");
}

/// Verifies the sizing pass still budgets every escapable byte: a string made only of bytes that
/// expand under the active flags encodes completely and byte-for-byte.
#[test]
fn test_json_encode_large_escaped_string_keeps_every_escape() {
    let out = compile_and_run(
        r#"<?php
$s = json_encode(str_repeat("\"/<&'" . chr(1) . "\u{e9}", 20000 + $argc - 1), JSON_HEX_TAG | JSON_HEX_AMP | JSON_HEX_APOS | JSON_HEX_QUOT);
echo strlen($s), ":", substr($s, 0, 45);
"#,
    );
    assert_eq!(
        out,
        "760002:\"\\u0022\\/\\u003C\\u0026\\u0027\\u0001\\u00e9\\u0022"
    );
}
