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

/// Named variadic trace arguments keep entry values while func_get_args stays positional-only.
#[test]
fn test_core_backtrace_regression_named_tail_keeps_entry_snapshot_and_keys() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function captureNamedTrace(int $fixed = 7, mixed ...$rest): void {
    $rest[0] = 99;
    $rest["named"] = 88;
    foreach (debug_backtrace() as $frame) {
        if ($frame["function"] === "captureNamedTrace") {
            echo json_encode($frame["args"]), "|", json_encode(func_get_args()), "|";
        }
    }
    ob_start();
    debug_print_backtrace(0, 1);
    $printed = ob_get_clean();
    echo str_contains($printed, "named: 22") ? "named|" : "bad|";
}
captureNamedTrace(11, 12, named: 22);
captureNamedTrace(named: 22);
"#);
    assert_eq!(out.stdout,
        "{\"0\":11,\"1\":12,\"named\":22}|[11,12]|named|{\"named\":22}|[]|named|",
        "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Typed named string tails preserve entry values and include the final iterator entry.
#[test]
fn test_core_backtrace_regression_named_string_tail_keeps_entry_values() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function namedValues(string ...$values): mixed {
    $values["last"] = "changed";
    foreach (debug_backtrace() as $frame) {
        if ($frame["function"] === "namedValues") return $frame["args"];
    }
    return [];
}
echo json_encode(namedValues(first: "one", last: "two"));
"#);
    assert_eq!(out.stdout, "{\"first\":\"one\",\"last\":\"two\"}", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Reference tails reflect callee updates, but returned trace cells do not alias caller storage.
#[test]
fn test_core_backtrace_regression_named_reference_tail_returns_independent_cells() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function namedReferences(mixed &...$values): mixed {
    $values["named"] = 42;
    foreach (debug_backtrace() as $frame) {
        if ($frame["function"] === "namedReferences") return $frame["args"];
    }
    return [];
}
$value = 1;
$trace = namedReferences(named: $value);
echo json_encode($trace), "|", $value, "|";
$trace["named"] = 99;
echo $value;
"#);
    assert_eq!(out.stdout, "{\"named\":42}|42|42", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

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

/// A live argument ownership guard must never be invoked as a frame reader.
#[test]
fn test_core_backtrace_regression_skips_argument_ownership_guards() {
    let out = compile_and_run(r#"<?php
function tracedGuard($value): void {
    foreach (debug_backtrace() as $frame) echo $frame["function"], ":";
}
tracedGuard(["owned"]);
"#);
    assert_eq!(out, "tracedGuard:");
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
