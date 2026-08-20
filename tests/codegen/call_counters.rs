//! Purpose:
//! Integration coverage for the `--counters` exit dump: exact per-function call
//! counts embedded as BSS slots and printed to stderr when main returns.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Counts are exact, not sampled — the assertions pin precise values.
//! - A fully inlined call site leaves its counter at zero: the dead body is
//!   emitted but never entered. That zero is intentional, observable behavior.

use crate::support::*;

/// A recursive function cannot be inlined away, so its counter records every
/// activation: `tick(3)` recurses through 4 calls, and program output is
/// untouched by the instrumentation.
#[test]
fn test_counters_report_exact_recursive_call_counts() {
    let out = compile_and_run_with_counters(
        "<?php
        function tick(int $n): int {
            if ($n <= 0) {
                return 0;
            }
            return 1 + tick($n - 1);
        }
        echo tick(3);
        ",
    );
    assert_eq!(out.stdout, "3");
    assert!(
        out.stderr.contains("elephc-counters: tick 4"),
        "stderr should carry the exact recursive count: {}",
        out.stderr
    );
}

/// Methods are counted under their `Class::method` name, and a helper the
/// inliner erases keeps a zero counter — the counter dump makes inlining
/// visible by difference instead of miscounting.
#[test]
fn test_counters_name_methods_and_expose_inlined_zeroes() {
    let out = compile_and_run_with_counters(
        "<?php
        class Wheel {
            public function spin(int $n): int {
                if ($n <= 0) {
                    return 0;
                }
                return 1 + $this->spin($n - 1);
            }
        }
        function shortcut(int $x): int { return $x + 1; }
        $w = new Wheel();
        echo $w->spin(2), \"|\", shortcut(4);
        ",
    );
    assert_eq!(out.stdout, "2|5");
    assert!(
        out.stderr.contains("elephc-counters: Wheel::spin 3"),
        "methods should be counted under Class::method: {}",
        out.stderr
    );
    // shortcut() is trivially inlinable; whether its counter reads 1 (called)
    // or 0 (inlined) it must be REPORTED — the dump covers every PHP function.
    assert!(
        out.stderr.contains("elephc-counters: shortcut "),
        "every PHP function appears in the dump: {}",
        out.stderr
    );
}
