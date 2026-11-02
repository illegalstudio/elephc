//! Purpose:
//! End-to-end parity tests for the `ext/gmp` surface through the runtime Magician eval bridge.
//!
//! Called from:
//! - `cargo test --test codegen_tests gmp_eval` through the codegen test module.
//!
//! Key details:
//! - The fixture forces bridge-run eval by building the fragment from a runtime-unknown
//!   `$argc` expression, so every GMP call goes through the public eval registry homes and
//!   the compiled prelude helpers rather than the literal-eval AOT planner.
//! - Expected output is byte-for-byte PHP 8.5 output. Object handles and deprecation lines are
//!   deliberately kept out of the fixture so the contract stays deterministic.
//! - GMP serialization is not covered here: Magician's `serialize()` / `unserialize()` remain
//!   a general pending eval surface, tracked separately from the GMP bridge delivery.

use crate::support::*;

/// Verifies functions, operators, casts, aggregates, named arguments, callables, and
/// diagnostics share one semantics between compiled and eval-run GMP code.
#[test]
fn test_gmp_eval_surface_matches_php() {
    if crate::support::skip_without_gmp_native("test_gmp_eval_surface_matches_php") {
        return;
    }
    let source = r#"<?php
$code = $argc > 0 ? <<<'__ELEPHC_EVAL__'
$g = gmp_add("40", 2);
echo gmp_strval($g), "\n";
echo gmp_strval(gmp_mul($g, 2)), "\n";
echo gmp_intval($g), "\n";
echo gmp_strval($g + "1"), "\n";
echo gmp_strval($g | "2"), "\n";
echo gmp_strval($g & "2"), "\n";
echo gmp_strval($g ^ "2"), "\n";
echo gmp_strval($g << "2"), "\n";
echo gmp_strval($g >> "2"), "\n";
echo gmp_strval(~$g), "\n";
echo gmp_strval(-$g), "\n";
var_dump((int) $g, (float) $g, (bool) $g, (string) $g);
var_dump($g > 1, $g < 100, $g <=> 42, $g == 42);
var_dump(gmp_invert(0, 7));
echo gmp_strval(gmp_invert(3, 11)), "\n";
$qr = gmp_div_qr(10, 3);
echo gmp_strval($qr[0]), ",", gmp_strval($qr[1]), "\n";
$sr = gmp_sqrtrem(16);
echo gmp_strval($sr[0]), ",", gmp_strval($sr[1]), "\n";
$gr = gmp_rootrem(27, 3);
echo gmp_strval($gr[0]), ",", gmp_strval($gr[1]), "\n";
$gd = gmp_gcdext(12, 18);
echo gmp_strval($gd["g"]), ",", gmp_strval($gd["s"]), ",", gmp_strval($gd["t"]), "\n";
echo gmp_strval(gmp_import(gmp_export(123456))), "\n";
$bits = gmp_init(5);
gmp_setbit($bits, 1);
echo gmp_strval($bits), "\n";
echo gmp_strval(gmp_root(27, 3)), "\n";
$arr = [gmp_init(0), -3, gmp_init(2), 1];
sort($arr);
foreach ($arr as $v) {
    echo is_object($v) ? gmp_strval($v) : $v, ",";
}
echo "\n";
var_dump(min(gmp_init(3), 4) instanceof GMP, max(gmp_init(3), 4));
var_dump(array_sum([gmp_init(3), 4]));
var_dump(extension_loaded("gmp"), in_array("gmp", get_loaded_extensions(), true), function_exists("gmp_add"));
var_dump(function_exists("__elephc_gmp_strval"));
$f = gmp_strval(...);
echo $f(new GMP(255), 16), "\n";
echo gmp_strval(num: new GMP(255), base: 16), "\n";
$s = serialize($g);
echo $s, "\n";
$u = unserialize($s);
echo gmp_strval($u), "\n";
var_dump(method_exists($g, "__debugInfo"), method_exists($g, "__serialize"), property_exists($g, "__elephc_num"));
$m = get_class_methods($g);
sort($m);
echo implode(",", $m), "\n";
try { unserialize('C:3:"GMP":0:{}'); } catch (Exception $e) { echo "caught:", $e->getMessage(), "\n"; }
echo print_r($g, true);
try {
    gmp_add("bad", 1);
} catch (ValueError $e) {
    echo "ValueError:", $e->getMessage(), "\n";
}
try {
    gmp_strval(new GMP(42), "x");
} catch (TypeError $e) {
    echo "TypeError:", $e->getMessage(), "\n";
}
try {
    gmp_add(1);
} catch (ArgumentCountError $e) {
    echo "ArgumentCountError:", $e->getMessage(), "\n";
}
var_dump(gmp_intval($assigned = gmp_init(42)));
var_dump(gmp_strval($assigned));
class EvalOperand {
    public function __toString() {
        return "1";
    }
}
$operand = new EvalOperand();
try {
    $g + $operand;
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
try {
    gmp_add($operand, 1);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
echo "done\n";
__ELEPHC_EVAL__
 : '';
eval($code);
"#;

    let out = compile_and_run_capture_merged_with_gmp(source);
    assert!(out.success, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "42\n84\n42\n43\n42\n2\n40\n168\n10\n-43\n-42\n\
int(42)\nfloat(42)\nbool(true)\nstring(2) \"42\"\n\
bool(true)\nbool(true)\nint(0)\nbool(true)\nbool(false)\n\
4\n3,1\n4,0\n3,0\n6,-1,1\n123456\n7\n3\n-3,0,1,2,\n\
bool(true)\nint(4)\nint(7)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\nff\nff\n\
O:3:\"GMP\":1:{i:0;s:2:\"2a\";}\n42\n\
bool(false)\nbool(true)\nbool(false)\n\
__construct,__serialize,__unserialize\n\
caught:Could not unserialize number\n\
GMP Object\n(\n    [num] => 42\n)\n\
ValueError:gmp_add(): Argument #1 ($num1) is not an integer string\n\
TypeError:gmp_strval(): Argument #2 ($base) must be of type int, string given\n\
ArgumentCountError:gmp_add() expects exactly 2 arguments, 1 given\n\
int(42)\nstring(2) \"42\"\n\
Number must be of type GMP|string|int, EvalOperand given\n\
gmp_add(): Argument #1 ($num1) must be of type GMP|string|int, EvalOperand given\n\
done\n"
    );
}
