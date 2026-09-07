//! Purpose:
//! Regressions for error-handler suspension and nullable reporting state.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_error_regression_`.
//!
//! Key details:
//! - Opaque eval source exercises the native/Magician dispatch boundary.

use crate::support::*;

/// Native and eval warnings enter an eval-registered handler through the shared dispatcher.
#[test]
fn test_core_error_regression_ordinary_warning_eval_handler() {
    let out = compile_and_run(r#"<?php
error_reporting(0);
$source = 'function evalOrdinaryWarning($level, $message) { echo $level, ":"; return true; }
set_error_handler("evalOrdinaryWarning");
fopen("/elephc-missing-directory/eval-file", "r");' . ' // ' . $argc;
eval($source);
fopen('/elephc-missing-directory/missing-file', 'r');
echo 'done';
"#);
    assert_eq!(out, "2:2:done");
}

/// A warning handler's writes remain observable after a discarded warning-producing read.
#[test]
fn test_core_error_regression_ordinary_warning_mutates_state() {
    let out = compile_and_run(r#"<?php
$seen = 0;
set_error_handler(function(int $level, string $message) use (&$seen): bool { $seen = 9; return true; });
$array = ['present' => 1];
$key = 'missing' . $argc;
$ignored = $array[$key];
echo $seen;
"#);
    assert_eq!(out, "9");
}

/// Reporting masks suppress ordinary default stderr output, including handler false fallthrough.
#[test]
fn test_core_error_regression_ordinary_warning_reporting_mask() {
    let output = compile_and_run_capture(r#"<?php
function fallthroughWarning(int $level, string $message): bool { echo 'handler:'; return false; }
error_reporting(0);
fopen('/elephc-missing-directory/missing-file', 'r');
set_error_handler('fallthroughWarning');
fopen('/elephc-missing-directory/missing-file', 'r');
echo 'done';
"#);
    assert!(output.success);
    assert_eq!(output.stdout, "handler:done");
    assert!(output.stderr.is_empty(), "{}", output.stderr);
}

/// Fragmented missing-key diagnostics invoke the callback once with its complete message.
#[test]
fn test_core_error_regression_ordinary_warning_fragments_and_reentrancy() {
    let out = compile_and_run(r#"<?php
function fragmentedWarning(int $level, string $message): bool {
    echo $message, ':';
    fopen('/elephc-missing-directory/inner', 'r');
    return true;
}
error_reporting(0);
set_error_handler('fragmentedWarning');
$array = ['present' => 7];
$key = 'missing' . $argc;
$value = $array[$key];
echo 'done';
"#);
    assert_eq!(out, "Undefined array key \"missing1\":done");
}

/// Ordinary runtime warnings reach handlers even when default reporting is disabled.
#[test]
fn test_core_error_regression_ordinary_warning_dispatch() {
    let out = compile_and_run(r#"<?php
function ordinaryWarning(int $level, string $message, string $file, int $line): bool {
    echo $level, ':', str_starts_with($message, 'Warning:') ? 'bad' : 'message';
    echo ':', strlen($file) > 0 && $line > 0 ? 'site:' : 'bad:';
    return true;
}
error_reporting(0);
set_error_handler('ordinaryWarning', E_WARNING);
$result = fopen('/elephc-missing-directory/missing-file', 'r');
echo $result === false ? 'failed' : 'bad';
"#);
    assert_eq!(out, "2:message:site:failed");
}

/// A throwing ordinary-warning handler releases its arguments and restores the active handler.
#[test]
fn test_core_error_regression_ordinary_warning_unwind() {
    let out = compile_and_run(r#"<?php
function ordinaryThrow(int $level, string $message): bool {
    echo 'handler:';
    throw new Exception('warning');
}
set_error_handler('ordinaryThrow');
try { fopen('/elephc-missing-directory/missing-file', 'r'); } catch (Exception $e) { echo 'caught:'; }
try { fopen('/elephc-missing-directory/missing-file', 'r'); } catch (Exception $e) { echo 'caught'; }
"#);
    assert_eq!(out, "handler:caught:handler:caught");
}

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
