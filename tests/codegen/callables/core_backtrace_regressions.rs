//! Purpose:
//! Pins complete native/eval backtrace boundaries, limits, and argument ownership.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_backtrace_regression_`.
//!
//! Key details:
//! - Eval fragments depend on a runtime value so Magician executes the frame reader.

use crate::support::*;

/// Opaque eval traces include eval and suspended AOT frames, while options and limits compose.
#[test]
fn test_core_backtrace_regression_mixed_boundary() {
    let out = compile_and_run(r#"<?php
function outerInventoryTrace(int $n): void {
    $source = 'function innerInventoryTrace(): void {
        $trace = debug_backtrace();
        foreach ($trace as $frame) { echo $frame["function"], ":"; }
        echo $trace[2]["args"][0], ":";
        echo count(debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 2)), ":";
        echo isset(debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS)[2]["args"]) ? "bad" : "noargs";
        ob_start(); debug_print_backtrace(0); $printed = ob_get_clean();
        echo str_contains($printed, "outerInventoryTrace(7)") ? ":printed" : ":bad";
    } innerInventoryTrace();' . ' // ' . $n;
    eval($source);
}
outerInventoryTrace(7);
"#);
    assert_eq!(out, "innerInventoryTrace:eval:outerInventoryTrace:7:2:noargs:printed");
}

/// Nested eval adds boundaries without shadowing func_num_args in its enclosing function.
#[test]
fn test_core_backtrace_regression_nested_eval_keeps_argument_scope() {
    let out = compile_and_run(r#"<?php
$source = 'function nestedInventoryTrace($arg): void {
    eval(\'echo func_num_args(), ":"; foreach (debug_backtrace(2) as $f) echo $f["function"], ":";\');
} nestedInventoryTrace(9);' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "1:eval:nestedInventoryTrace:eval:");
}
