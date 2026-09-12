//! Purpose:
//! Pins complete native/eval backtrace boundaries, limits, and argument ownership.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_backtrace_regression_`.
//!
//! Key details:
//! - Eval fragments depend on a runtime value so Magician executes the frame reader.
//! - Every frame-capture detector gets its OWN program. The gate is program-wide: one detector
//!   firing gives every frame in that program its hidden argument snapshot, so two detectors in
//!   one fixture would let the working one mask the broken one.
//! - Frames are selected by `function` name, never by index. `call_user_func` and friends may or
//!   may not contribute a frame of their own, and that is not what these tests are pinning; a
//!   fixture that indexed the trace would be asserting the wrong thing either way.

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

/// An eval-originated trace keeps omitted optionals out of the frame and keeps variadic tails in.
///
/// Both depend on the hidden argument snapshot, which only exists when the frame-capture gate
/// fires. The trace is taken by name rather than by index so a `call_user_func`-shaped frame
/// could not change what is asserted.
#[test]
fn test_core_backtrace_regression_eval_frame_respects_optional_and_variadic_args() {
    let out = compile_and_run(r#"<?php
function tracedOptionalTail($first, $second = 5, ...$rest) {
    foreach (debug_backtrace() as $frame) {
        if ($frame["function"] === "tracedOptionalTail") {
            return count($frame["args"]) . ":" . implode(",", $frame["args"]);
        }
    }
    return "missing";
}
$source = 'echo tracedOptionalTail(1), "|", tracedOptionalTail(1, 2, 3, 4);' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "1:1|4:1,2,3,4");
}

/// `eval()` alone arms the frame-capture gate, with no backtrace spelling anywhere in AOT source.
///
/// This is the case `crate::codegen::frame::module_uses_backtrace()` forces on: a program that
/// carries the eval bridge gets backtraces whether or not the AST mentions one, so a frame
/// lowered without its hidden argument snapshot would report stale or missing arguments. The
/// only occurrence of the builtin's name here is INSIDE a longer eval source string, which is
/// not a callable-name literal, and no AOT statement calls it.
#[test]
fn test_core_backtrace_regression_eval_is_the_only_frame_capture_gate() {
    let out = compile_and_run(r#"<?php
function evalOnlyGateFrame($first, $second = 5) {
    $probe = 'foreach (debug_backtrace() as $frame) {
        if ($frame["function"] === "evalOnlyGateFrame") { echo count($frame["args"]); }
    }';
    eval($probe . ' // ' . $first);
}
evalOnlyGateFrame(1);
echo "|";
evalOnlyGateFrame(1, 2);
"#);
    assert_eq!(out, "1|2");
}

/// A literal `call_user_func("debug_backtrace")` is the program's only frame-capture trigger.
#[test]
fn test_core_backtrace_regression_call_user_func_arms_the_gate() {
    let out = compile_and_run(r#"<?php
function tracedViaCallUserFunc($first, $second = 5) {
    foreach (call_user_func("debug_backtrace") as $frame) {
        if ($frame["function"] === "tracedViaCallUserFunc") { return count($frame["args"]); }
    }
    return -1;
}
echo tracedViaCallUserFunc(1), ":", tracedViaCallUserFunc(1, 2);
"#);
    assert_eq!(out, "1:2");
}

/// A literal `call_user_func_array("debug_backtrace", [])` is the program's only trigger.
#[test]
fn test_core_backtrace_regression_call_user_func_array_arms_the_gate() {
    let out = compile_and_run(r#"<?php
function tracedViaCallUserFuncArray($first, $second = 5) {
    foreach (call_user_func_array("debug_backtrace", []) as $frame) {
        if ($frame["function"] === "tracedViaCallUserFuncArray") { return count($frame["args"]); }
    }
    return -1;
}
echo tracedViaCallUserFuncArray(1), ":", tracedViaCallUserFuncArray(1, 2);
"#);
    assert_eq!(out, "1:2");
}

/// A first-class callable `debug_backtrace(...)` is the program's only trigger.
#[test]
fn test_core_backtrace_regression_first_class_callable_arms_the_gate() {
    let out = compile_and_run(r#"<?php
function tracedViaFirstClassCallable($first, $second = 5) {
    $reader = debug_backtrace(...);
    foreach ($reader() as $frame) {
        if ($frame["function"] === "tracedViaFirstClassCallable") { return count($frame["args"]); }
    }
    return -1;
}
echo tracedViaFirstClassCallable(1), ":", tracedViaFirstClassCallable(1, 2);
"#);
    assert_eq!(out, "1:2");
}

/// A callable held in a string variable is the program's only trigger.
#[test]
fn test_core_backtrace_regression_string_variable_callable_arms_the_gate() {
    let out = compile_and_run(r#"<?php
function tracedViaStringVariable($first, $second = 5) {
    $reader = "debug_backtrace";
    foreach ($reader() as $frame) {
        if ($frame["function"] === "tracedViaStringVariable") { return count($frame["args"]); }
    }
    return -1;
}
echo tracedViaStringVariable(1), ":", tracedViaStringVariable(1, 2);
"#);
    assert_eq!(out, "1:2");
}

/// A leading namespace separator in the callable string still names the Core builtin.
///
/// PHP strips exactly one leading `\` from a callable string, so `"\debug_backtrace"` resolves to
/// the same builtin and has to arm the same gate. This is its own program precisely because the
/// unprefixed spelling would otherwise arm it first.
#[test]
fn test_core_backtrace_regression_leading_backslash_callable_arms_the_gate() {
    let out = compile_and_run(r#"<?php
function tracedViaLeadingBackslash($first, $second = 5) {
    $reader = "\debug_backtrace";
    foreach ($reader() as $frame) {
        if ($frame["function"] === "tracedViaLeadingBackslash") { return count($frame["args"]); }
    }
    return -1;
}
echo tracedViaLeadingBackslash(1), ":", tracedViaLeadingBackslash(1, 2);
"#);
    assert_eq!(out, "1:2");
}

/// `debug_print_backtrace()` through `call_user_func` prints the caller's real argument list.
///
/// The expected text is written out, not rebuilt from `func_get_args()`: deriving it would make
/// the fixture agree with whatever the frame happens to hold, which is the very thing under test.
#[test]
fn test_core_backtrace_regression_call_user_func_print_shows_frame_args() {
    let out = compile_and_run(r#"<?php
function printedViaCallUserFunc($first, $second = 7) {
    ob_start(); call_user_func("debug_print_backtrace"); $printed = ob_get_clean();
    return str_contains($printed, "printedViaCallUserFunc(1, 2)") ? "ok" : $printed;
}
echo printedViaCallUserFunc(1, 2);
"#);
    assert_eq!(out, "ok");
}

/// `debug_print_backtrace()` through a string-variable callable prints the omitted-optional list.
#[test]
fn test_core_backtrace_regression_string_variable_print_shows_frame_args() {
    let out = compile_and_run(r#"<?php
function printedViaStringVariable($first, $second = 7) {
    $printer = "debug_print_backtrace";
    ob_start(); $printer(); $printed = ob_get_clean();
    return str_contains($printed, "printedViaStringVariable(1)") ? "ok" : $printed;
}
echo printedViaStringVariable(1);
"#);
    assert_eq!(out, "ok");
}
