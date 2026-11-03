//! Purpose:
//! Regression coverage for boxed JSON source ownership in `json_decode()`/`json_validate()`.
//!
//! Called from:
//! - The codegen JSON suite through `json`.
//!
//! Key details:
//! - A mixed string source borrows the live cell payload instead of persisting a copy.
//! - Non-string boxed sources still materialize a string and must release it after the call.
//! - The heap-debug run verifies equal allocation/free counts.

use crate::support::*;

/// Boxed JSON sources leave no persisted copy and keep PHP's decoded values.
#[test]
fn json_boxed_source_releases_its_materialized_string() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function mixedString(): mixed { return json_encode(['a' => 1, 'b' => [1, 2, 3]]); }
function mixedInt(): mixed { return 42; }
$s = mixedString();
for ($i = 0; $i < 3; $i++) { $n = count(json_decode($s, true)); }
echo $n, '|';
echo json_decode(mixedInt(), true), '|';
echo json_validate(mixedString()) ? 'T' : 'F', '|';
$decoded = json_decode(mixedString(), true);
echo $decoded['b'][2], "\n";
"#,
    );
    assert!(out.success, "program crashed: {}", out.stderr);
    assert_eq!(out.stdout, "2|42|T|3\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}
