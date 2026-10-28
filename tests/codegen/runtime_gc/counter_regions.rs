//! Purpose:
//! Checks ownership when a numeric binding created in a conditional arm ends at a later region.
//!
//! Called from:
//! - The focused runtime GC codegen suite.
//!
//! Key details:
//! - Both EIR modes cover written and unwritten branch slots under strict heap debugging.

use crate::support::{compile_and_run_with_heap_debug_tagged, without_ir_opt};

/// Ending a maybe-written boxed counter binding releases its occupant before clearing the slot.
#[test]
fn test_counter_regions_conditional_rebind_releases_owner() {
    let source = r#"<?php
function counterArms(int $n, bool $c): int {
    $count = 0;
    for ($r = 0; $r < $n; $r++) { $count += 1; }
    if ($c) { for ($r = $n - 1; $r >= 0; $r--) { $count += 1; } }
    else { for ($r = $n - 2; $r >= 0; $r--) { $count += 1; } }
    for ($r = 0; $r < $n; $r++) { $count += 1; }
    return $count;
}
function counterOptionalArm(int $n, bool $c): int {
    $count = 0;
    for ($r = 0; $r < $n; $r++) { $count += 1; }
    if ($c) { for ($r = $n - 1; $r >= 0; $r--) { $count += 1; } }
    for ($r = 0; $r < $n; $r++) { $count += 1; }
    return $count;
}
echo counterArms($argc + 2, true), ':', counterArms($argc + 2, false), '|';
echo counterOptionalArm($argc + 2, true), ':', counterOptionalArm($argc + 2, false);
"#;
    for optimized in [false, true] {
        let output = if optimized { compile_and_run_with_heap_debug_tagged(source) }
            else { without_ir_opt(|| compile_and_run_with_heap_debug_tagged(source)) };
        assert_eq!(output.stdout, "9:8|9:6", "optimized={optimized}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "optimized={optimized}: {}", output.stderr);
    }
}
