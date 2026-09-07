//! Purpose:
//! Regressions for error-handler suspension and nullable reporting state.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_error_regression_`.
//!
//! Key details:
//! - Opaque eval source exercises the native/Magician dispatch boundary.

use crate::support::*;

/// A nested user warning uses default handling while the current callback is suspended.
#[test]
fn test_core_error_regression_handler_is_not_reentrant() {
    let out = compile_and_run(r#"<?php
function recursiveHandler(int $level, string $message): bool {
    echo $message, ':';
    if ($message === 'outer') { trigger_error('inner', E_USER_WARNING); }
    return true;
}
error_reporting(0);
set_error_handler('recursiveHandler');
trigger_error('outer', E_USER_WARNING);
trigger_error('later', E_USER_WARNING);
"#);
    assert_eq!(out, "outer:later:");
}

/// Throwing out of a native handler restores it before the surrounding catch continues.
#[test]
fn test_core_error_regression_handler_restored_after_throw() {
    let out = compile_and_run(r#"<?php
function throwingHandler(int $level, string $message): bool {
    echo $message, ':';
    if ($message === 'first') { throw new Exception('handler'); }
    return true;
}
set_error_handler('throwingHandler');
try { trigger_error('first', E_USER_WARNING); } catch (Exception $e) { echo 'caught:'; }
trigger_error('second', E_USER_WARNING);
"#);
    assert_eq!(out, "first:caught:second:");
}

/// Replacing the handler inside a callback sees null and preserves the replacement.
#[test]
fn test_core_error_regression_callback_can_replace_handler() {
    let out = compile_and_run(r#"<?php
function replacementHandler(int $level, string $message): bool { echo 'new:', $message, ':'; return true; }
function installingHandler(int $level, string $message): bool {
    echo is_null(set_error_handler('replacementHandler')) ? 'null:' : 'bad:';
    trigger_error('inner', E_USER_WARNING);
    return true;
}
set_error_handler('installingHandler');
trigger_error('outer', E_USER_WARNING);
trigger_error('later', E_USER_WARNING);
"#);
    assert_eq!(out, "null:new:inner:new:later:");
}

/// Eval handlers share the native reentrancy guard and can be invoked again afterward.
#[test]
fn test_core_error_regression_eval_handler_is_not_reentrant() {
    let out = compile_and_run(r#"<?php
error_reporting(0);
$code = 'function evalRecursive($level, $message) {
    echo $message, ":";
    if ($message === "outer") { trigger_error("inner", E_USER_WARNING); }
    return true;
}
set_error_handler("evalRecursive");
trigger_error("outer", E_USER_WARNING);' . ' // ' . $argc;
eval($code);
trigger_error('later', E_USER_WARNING);
"#);
    assert_eq!(out, "outer:later:");
}

/// Runtime nullable and boxed null arguments query without modifying the reporting mask.
#[test]
fn test_core_error_regression_reporting_nullable_and_mixed_null() {
    let source = r#"<?php
function nullableLevel(): ?int { return null; }
function mixedLevel(): mixed { return null; }
function nullableNumber(): ?int { return 456; }
error_reporting(123);
echo error_reporting(nullableLevel()), ':', error_reporting(), ':';
echo error_reporting(mixedLevel()), ':', error_reporting(), ':';
echo error_reporting(nullableNumber()), ':', error_reporting();
"#;
    assert_eq!(compile_and_run(source), "123:123:123:123:123:456");
    assert_eq!(compile_and_run_tagged(source), "123:123:123:123:123:456");
}
