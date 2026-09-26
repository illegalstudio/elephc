//! Purpose:
//! Heap-debug regressions for large strings returned by unary string runtime helpers.
//!
//! Called from:
//! - `tests/codegen/runtime_gc.rs` in the codegen integration test suite.
//!
//! Key details:
//! - Results larger than the concat scratch buffer are owned heap blocks and must be released
//!   when a consumer such as `strlen()` discards them.
//! - Repeated calls expose ownership errors that a single temporary allocation can hide.

use crate::support::compile_and_run_with_heap_debug;

/// Large unary-string results remain balanced after repeated consuming calls.
#[test]
fn test_large_addslashes_results_do_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$big = str_repeat("a'b", 30000);
for ($i = 0; $i < 5; $i++) { $length = strlen(addslashes($big)); }
echo $length;
"#,
    );
    assert_eq!(out.stdout, "120000", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "large unary-string results leaked: {}",
        out.stderr
    );
}
