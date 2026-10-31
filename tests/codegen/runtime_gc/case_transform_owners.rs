//! Purpose:
//! Exercises the owned-heap allocation path shared by `strtolower()` and `strtoupper()`.
//!
//! Called from:
//! - The codegen integration suite through `runtime_gc`.
//!
//! Key details:
//! - Heap poisoning verifies that the directly written result is owned exactly once.
//! - Empty results still allocate their own block and must not alias the source.

use crate::support::compile_and_run_with_heap_debug;

/// Direct owned case transforms balance allocations for literals, empty strings and values.
#[test]
fn test_case_transform_owners_are_balanced() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$source = "  MiXeD 123 ABC xyz  ";
$lower = strtolower($source);
$upper = strtoupper($source);
echo $lower, '|', $upper, '|';
echo strlen(strtolower($source)), '|';
echo strtolower(''), '|', strtoupper(''), '|';
$high = strtoupper("caf\xC3\xA9");
echo bin2hex($high), '|';
$round = strtolower(strtoupper($source));
echo $round, "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "  mixed 123 abc xyz  |  MIXED 123 ABC XYZ  |21|||434146c3a9|  mixed 123 abc xyz  \n",
        "{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}
