//! Purpose:
//! Ownership regressions for short-circuit conditions lowered directly to branches.
//!
//! Called from:
//! - `tests/codegen/runtime_gc.rs` through the codegen test harness.
//!
//! Key details:
//! - Owned string and boxed coalesce operands must be released on each evaluated path.

use crate::support::*;

/// Direct branches consume owned operands on taken and skipped short-circuit paths.
#[test]
fn test_branch_conditions_owned_operands_are_heap_clean() {
    let source = r#"<?php
function fresh(string $value): string { return str_repeat($value, 2); }
for ($i = 0; $i < 10; $i++) {
    $values = [$argc, 0];
    if ((fresh($i % 2 === 0 ? "x" : "") && ($values[0] ?? 0)) || !(fresh("") || ($values[1] ?? 1))) { echo "T"; }
    echo (fresh("") || ($values[0] ?? 0)) ? "Y" : "N";
}
"#;
    for optimized in [true, false] {
        let output = if optimized {
            compile_and_run_with_heap_debug(source)
        } else {
            without_ir_opt(|| compile_and_run_with_heap_debug(source))
        };
        assert!(output.success, "optimized={optimized}: {}", output.stderr);
        assert_eq!(output.stdout, "TYTYTYTYTYTYTYTYTYTY");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "optimized={optimized}: {}", output.stderr);
    }
}
