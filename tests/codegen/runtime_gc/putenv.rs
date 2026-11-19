//! Purpose:
//! Checks cleanup of temporary C strings used to remove environment variables.
//!
//! Called from:
//! - `cargo test --test codegen_tests_runtime` through the runtime-GC suite.
//!
//! Key details:
//! - Unsetting absent names avoids the persistent allocations retained by assignments.
//! - Heap debug checks both the temporary argument and the copied C string are released.

use crate::support::*;

/// Verifies repeated removals release copied short and long names and preserve success results.
#[test]
fn test_putenv_unset_temporary_names_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$successes = 0;
for ($i = 0; $i < 32; $i++) {
    if (putenv("ELEPHC_PUTENV_ABSENT_SHORT_907")) {
        $successes++;
    }
    if (putenv(str_repeat("ELEPHC_PUTENV_ABSENT_LONG_907", 320))) {
        $successes++;
    }
}
echo $successes;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "64");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected unset name buffers to leave a clean heap, got: {}",
        out.stderr
    );
}
