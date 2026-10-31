//! Purpose:
//! End-to-end coverage for the raw `strlen((string) json_encode(...))` sink.
//!
//! Called from:
//! - The codegen JSON suite through `json`.
//!
//! Key details:
//! - Output lengths, JSON error state, partial output, pretty printing and a heap-grown
//!   result all match PHP; disabling EIR optimization keeps the boxed path.
//! - The heap-debug run verifies the raw encoder result is released exactly once.

use crate::support::*;

/// Length-only encodings match PHP, preserve json_last_error and stay heap-clean.
#[test]
fn json_encode_length_sink_matches_php() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ['a' => 1, 'b' => 2, 'c' => [1, 2, 3], 'd' => 'text'];
echo strlen(json_encode($a)), '|';
echo strlen((string)json_encode($a)), '|';
echo strlen(json_encode(['x' => "é\"\\/\n"])), '|';
echo strlen(json_encode([1.5, 2.0, -0.0, 1e20])), '|';
echo strlen(json_encode(['a' => 1], JSON_PRETTY_PRINT)), '|';
$bad = "\xFF\xFE";
echo strlen(json_encode($bad)), '|', json_last_error(), '|';
echo strlen(json_encode([1, NAN], JSON_PARTIAL_OUTPUT_ON_ERROR)), '|', json_last_error(), '|';
$big = array_fill(0, 5000, 'abcdefghij');
echo strlen(json_encode($big)), "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "36|36|22|18|14|0|5|5|7|65001\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// A non-length observer keeps the boxed encoder and its JSON string result.
#[test]
fn json_encode_boxed_result_survives_other_observers() {
    let out = compile_and_run(
        r#"<?php
$a = ['a' => 1, 'b' => [2, 3]];
$json = json_encode($a);
echo $json, '|', strlen($json), "\n";
"#,
    );
    assert_eq!(out, "{\"a\":1,\"b\":[2,3]}|17\n");
}

/// Disabling EIR optimization leaves the boxed encode/cast graph and produces the same lengths.
#[test]
fn json_encode_length_sink_no_ir_opt_parity() {
    without_ir_opt(|| {
        let out = compile_and_run(
            r#"<?php
$a = ['a' => 1, 'b' => 2, 'c' => [1, 2, 3], 'd' => 'text'];
echo strlen(json_encode($a)), '|', strlen(json_encode(['x' => "é\"\\/\n"])), "\n";
"#,
        );
        assert_eq!(out, "36|22\n");
    });
}
