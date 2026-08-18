//! Purpose:
//! Ownership regressions for PCNTL by-reference arrays returned by child-wait operations.
//!
//! Called from:
//! - `cargo test --test codegen_tests runtime_gc::pcntl` through Rust's test harness.
//!
//! Key details:
//! - Reusing `$usage` must release the prior hash, while final frame cleanup owns the last one.

use crate::support::compile_and_run_with_heap_debug;

/// Reaps two children into the same usage local without leaking or double-freeing either hash.
#[test]
fn test_pcntl_waitpid_resource_usage_rebind_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        "<?php
        $usage = [];
        $first = pcntl_fork();
        if ($first === 0) { exit(3); }
        pcntl_waitpid($first, $status, 0, $usage);
        echo count($usage) . '|';
        $second = pcntl_fork();
        if ($second === 0) { exit(5); }
        pcntl_waitpid($second, $status, 0, $usage);
        echo pcntl_wexitstatus($status) . '|' . count($usage);",
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "17|5|17", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr,
    );
}
