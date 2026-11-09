//! Purpose:
//! Heap-debug coverage for bare `$GLOBALS` materialization and its array-typed values.
//!
//! Called from:
//! - `cargo test` through the runtime GC integration suite.
//!
//! Key details:
//! - The materialized hash owns one reference per initialized global, so each iteration must
//!   release the hash and its values, including a global array payload.
//! - The top-level global bindings themselves must also be retired at shutdown; the fixture
//!   intentionally creates loop variables so the epilogue must cover names interned later.

use crate::support::compile_and_run_with_heap_debug;

/// A materialized `$GLOBALS` snapshot keeps its values alive but leaves no heap blocks behind.
#[test]
fn test_globals_array_materialization_leaves_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [1, 2, 3];
$b = "text";
for ($i = 0; $i < 3; $i++) {
    $count = count($GLOBALS);
    foreach ($GLOBALS as $name => $value) {
        if ($name === "a") {
            echo count($value), ":";
        }
    }
}
echo $count > 0 ? "count-ok\n" : "count-bad\n";
"#,
    );
    assert_eq!(out.stdout, "3:3:3:count-ok\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}
