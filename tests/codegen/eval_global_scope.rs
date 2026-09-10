//! Purpose:
//! Checks global symbol-table identity across opaque eval and declared functions.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Runtime-selected source keeps these cases on the interpreter bridge.

use crate::support::*;

/// Shares names created by main-scope eval with eval-declared function globals.
#[test]
fn test_eval_main_global_scope_dynamic_names() {
    let source = r#"<?php
$source = $argc > 0 ? '$number = 7; function increment_dynamic_global() { global $number; $number++; } increment_dynamic_global(); echo $number, "\n";' : '';
eval($source);
$next = $argc > 0 ? 'increment_dynamic_global(); echo $number, "\n";' : '';
eval($next);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "8\n9\n");
}

/// Keeps function-local eval variables separate from a declared global binding.
#[test]
fn test_eval_function_global_scope_keeps_locals_separate() {
    let source = r#"<?php
$number = 7;
function read_number(): int { global $number; return $number; }
function invoke_local_eval(string $source): void {
    $number = 20;
    eval($source);
    echo $number, "\n";
}
$source = $argc > 0 ? 'function increment_local_global() { global $number; $number++; } increment_local_global(); echo $number, "\n";' : '';
invoke_local_eval($source);
echo read_number(), "\n";
"#;
    assert_eq!(compile_and_run(source), "20\n20\n8\n");
}
