//! Purpose:
//! Covers propagation across array reads that can dispatch PHP warning handlers.
//!
//! Called from:
//! - The codegen integration test harness through the optimizer module.
//!
//! Key details:
//! - The handler declaration follows the observed read, so its global binding cannot
//!   pre-mark the variable volatile and hide an invalidation regression.

use crate::support::*;

/// A missing-key handler invalidates a top-level scalar fact created after registration.
#[test]
fn test_optimizer_array_warning_invalidates_global_scalar() {
    let source = r#"<?php
set_error_handler('changeWarningGlobal');
$warningScalar = 5;
$array = ['present' => 1];
$key = 'missing' . $argc;
$ignored = $array[$key];
echo $warningScalar + 1;
function changeWarningGlobal(int $level, string $message): bool {
    global $warningScalar;
    $warningScalar = 9;
    return true;
}
"#;
    assert_eq!(compile_and_run(source), "10");
    assert_eq!(compile_and_run_tagged(source), "10");
}
