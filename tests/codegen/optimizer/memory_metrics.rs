//! Purpose:
//! Verifies that heap samples and unused peak resets survive optimization.
//!
//! Called from:
//! - The optimizer codegen integration suite.
//!
//! Key details:
//! - Runtime-sized arrays keep the measured lifetime beyond constant folding.

use crate::support::*;

/// Peak reset side effects and heap reads behave identically with EIR optimization off or on.
#[test]
fn test_memory_metrics_optimizer_barriers() {
    let source = r#"<?php
$base = memory_get_usage();
$values = array_fill(0, $argc + 32, 7);
$held = memory_get_usage();
unset($values);
$before = memory_get_peak_usage();
memory_reset_peak_usage();
$after = memory_get_peak_usage();
echo $held > $base && $before >= $held && $after < $before ? "measured" : "bad";
"#;
    assert_eq!(compile_and_run(source), "measured");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "measured");
}
