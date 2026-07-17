//! Purpose:
//! End-to-end regressions for eval ReflectionFunction/ReflectionMethod invocation.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_reflection_invocation` through Rust's test harness.
//!
//! Key details:
//! - Fixtures distinguish PHP's by-value `invoke()` forwarding from by-reference
//!   `invokeArgs([&$value])` forwarding for eval-declared and generated/AOT callables.

use crate::support::compile_and_run_capture;

/// Verifies ReflectionFunction preserves PHP by-ref semantics for invoke and invokeArgs.
#[test]
fn test_eval_reflection_function_invoke_by_ref_matches_php_ref_semantics() {
    let out = compile_and_run_capture(
        r#"<?php
function eval_reflect_aot_invoke_ref_fn(int &$value): int {
    $value = $value + 7;
    return $value;
}

echo eval('function eval_reflect_invoke_ref_fn(int &$value): int {
    $value = $value + 3;
    return $value;
}

$evalRef = new ReflectionFunction("eval_reflect_invoke_ref_fn");
$direct = "1";
echo $evalRef->invoke($direct) . ":" . gettype($direct) . ":" . $direct . "|";

$argsValue = "2";
echo $evalRef->invokeArgs([&$argsValue]) . ":" . gettype($argsValue) . ":" . $argsValue . "|";

$aotRef = new ReflectionFunction("eval_reflect_aot_invoke_ref_fn");
$aotDirect = "3";
echo $aotRef->invoke($aotDirect) . ":" . gettype($aotDirect) . ":" . $aotDirect . "|";

$aotArgsValue = "4";
return $aotRef->invokeArgs([&$aotArgsValue]) . ":" . gettype($aotArgsValue) . ":" . $aotArgsValue;');
"#,
    );

    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "4:string:1|5:integer:5|10:string:3|11:integer:11");
    for warning in [
        "eval_reflect_invoke_ref_fn(): Argument #1 ($value) must be passed by reference, value given",
        "eval_reflect_aot_invoke_ref_fn(): Argument #1 ($value) must be passed by reference, value given",
    ] {
        assert!(
            out.stderr.contains(warning),
            "missing by-ref warning {warning:?}: {}",
            out.stderr
        );
    }
}

/// Verifies ReflectionMethod preserves PHP by-ref semantics for invoke and invokeArgs.
#[test]
fn test_eval_reflection_method_invoke_by_ref_matches_php_ref_semantics() {
    let out = compile_and_run_capture(
        r#"<?php
class EvalReflectAotInvokeRefMethodBox {
    public function bump(int &$value): int {
        $value = $value + 11;
        return $value;
    }

    public static function add(int &$value): int {
        $value = $value + 13;
        return $value;
    }
}

echo eval('class EvalReflectInvokeRefMethodBox {
    public function bump(int &$value): int {
        $value = $value + 5;
        return $value;
    }

    public static function add(int &$value): int {
        $value = $value + 9;
        return $value;
    }
}

$evalBox = new EvalReflectInvokeRefMethodBox();
$evalMethod = new ReflectionMethod("EvalReflectInvokeRefMethodBox", "bump");
$evalDirect = "1";
echo $evalMethod->invoke($evalBox, $evalDirect) . ":" . gettype($evalDirect) . ":" . $evalDirect . "|";

$evalArgsValue = "2";
echo $evalMethod->invokeArgs($evalBox, [&$evalArgsValue]) . ":" . gettype($evalArgsValue) . ":" . $evalArgsValue . "|";

$evalStatic = new ReflectionMethod("EvalReflectInvokeRefMethodBox", "add");
$evalStaticDirect = "3";
echo $evalStatic->invoke(null, $evalStaticDirect) . ":" . gettype($evalStaticDirect) . ":" . $evalStaticDirect . "|";

$evalStaticArgsValue = "4";
echo $evalStatic->invokeArgs(null, [&$evalStaticArgsValue]) . ":" . gettype($evalStaticArgsValue) . ":" . $evalStaticArgsValue . "|";

$aotBox = new EvalReflectAotInvokeRefMethodBox();
$aotMethod = new ReflectionMethod("EvalReflectAotInvokeRefMethodBox", "bump");
$aotDirect = "5";
echo $aotMethod->invoke($aotBox, $aotDirect) . ":" . gettype($aotDirect) . ":" . $aotDirect . "|";

$aotArgsValue = "6";
echo $aotMethod->invokeArgs($aotBox, [&$aotArgsValue]) . ":" . gettype($aotArgsValue) . ":" . $aotArgsValue . "|";

$aotStatic = new ReflectionMethod("EvalReflectAotInvokeRefMethodBox", "add");
$aotStaticDirect = "7";
echo $aotStatic->invoke(null, $aotStaticDirect) . ":" . gettype($aotStaticDirect) . ":" . $aotStaticDirect . "|";

$aotStaticArgsValue = "8";
return $aotStatic->invokeArgs(null, [&$aotStaticArgsValue]) . ":" . gettype($aotStaticArgsValue) . ":" . $aotStaticArgsValue;');
"#,
    );

    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "6:string:1|7:integer:7|12:string:3|13:integer:13|16:string:5|17:integer:17|20:string:7|21:integer:21"
    );
    for warning in [
        "EvalReflectInvokeRefMethodBox::bump(): Argument #1 ($value) must be passed by reference, value given",
        "EvalReflectInvokeRefMethodBox::add(): Argument #1 ($value) must be passed by reference, value given",
        "EvalReflectAotInvokeRefMethodBox::bump(): Argument #1 ($value) must be passed by reference, value given",
        "EvalReflectAotInvokeRefMethodBox::add(): Argument #1 ($value) must be passed by reference, value given",
    ] {
        assert!(
            out.stderr.contains(warning),
            "missing by-ref warning {warning:?}: {}",
            out.stderr
        );
    }
}
