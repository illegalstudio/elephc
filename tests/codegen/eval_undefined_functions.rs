//! Purpose:
//! Verifies undefined eval calls use catchable PHP errors across the native boundary.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Runtime-selected strings force Magician instead of literal eval compilation.
//! - Tests cover eval-local and native catches, namespace fallback, and argument effects.

use crate::support::*;

/// Missing direct functions throw before arguments run and survive both eval and native catches.
#[test]
fn test_eval_undefined_functions_are_catchable() {
    let output = compile_and_run_with_heap_debug(r#"<?php
$source = $argc > 0 ? '
    $n = 0;
    function bump() { global $n; $n++; return $n; }
    try { isseet(bump()); } catch (Error $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
    echo $n, "\n";
' : '';
eval($source);
$source = $argc > 0 ? 'namespace replcheck; isseet();' : '';
try { eval($source); } catch (Error $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
$source = $argc > 0 ? 'namespace replcheck; echo strlen("ok"), "\n";' : '';
eval($source);
echo "alive\n";
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, concat!(
        "Error: Call to undefined function isseet()\n",
        "0\n",
        "Error: Call to undefined function replcheck\\isseet()\n",
        "2\nalive\n",
    ));
}
