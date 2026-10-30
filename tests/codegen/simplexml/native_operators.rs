//! Purpose:
//! Covers SimpleXML scalar operators while GMP's native adapters are also available.
//!
//! Called from:
//! - Managed libxml2 and GMP codegen CI through the SimpleXML test module.
//!
//! Key details:
//! - Mixed casts must inspect both native families before using the scalar fallback.
//! - Opaque eval retains the XML integer coercion and GMP arbitrary precision dispatch.

use crate::support::*;

/// Both native families retain their casts and operator result domains in AOT and opaque eval.
#[test]
fn test_simplexml_gmp_mixed_operator_dispatch_bundle() {
    if skip_without_xml_native("test_simplexml_gmp_mixed_operator_dispatch_bundle")
        || skip_without_gmp_native("test_simplexml_gmp_mixed_operator_dispatch_bundle")
    {
        return;
    }
    let setup = r#"<?php
$xml = new SimpleXMLElement('<n>7.5</n>');
$integer = gmp_init('42');
"#;
    let body = r#"
function nativeScalarCasts(mixed $value): void { var_dump((int)$value, (float)$value); }
function nativeAdd(mixed $value): mixed { return $value + 2; }
function nativeNegate(mixed $value): mixed { return -$value; }
function nativeAnd(mixed $value): mixed { return $value & 3; }
function nativeShift(mixed $value): mixed { return $value << 1; }
nativeScalarCasts($xml);
nativeScalarCasts($integer);
nativeScalarCasts(3.5);
class NativeOperatorXml extends SimpleXMLElement {}
$subclass = new NativeOperatorXml('<n>9.5</n>');
var_dump((int)$subclass, (float)$subclass);
nativeScalarCasts($subclass);
echo nativeAdd($xml), ':', gmp_strval(nativeAdd($integer)), "\n";
echo nativeNegate($xml), ':', gmp_strval(nativeNegate($integer)), "\n";
echo nativeAnd($xml), ':', gmp_strval(nativeAnd($integer)), "\n";
echo nativeShift($xml), ':', gmp_strval(nativeShift($integer)), "\n";
$inverted = gmp_invert(3, 11);
echo gmp_strval($integer * $inverted), "\n";
$maybe = $argc > 0 ? 2 : null;
echo gmp_strval($integer + $maybe), "\n";
function nativeForeignOperator(mixed $left, mixed $right, int $operation): void {
    try {
        if ($operation === 0) { $result = $left + $right; }
        elseif ($operation === 1) { $result = $left & $right; }
        else { $result = $left << $right; }
        echo 'unexpected-result|';
    } catch (TypeError $error) { echo $error->getMessage(), '|'; }
}
nativeForeignOperator($xml, $integer, 0); nativeForeignOperator($integer, $xml, 0);
nativeForeignOperator($xml, $integer, 1); nativeForeignOperator($integer, $xml, 1);
nativeForeignOperator($xml, $integer, 2); nativeForeignOperator($integer, $xml, 2);
"#;
    let expected = concat!(
        "int(7)\nfloat(7.5)\nint(42)\nfloat(42)\nint(3)\nfloat(3.5)\n",
        "int(9)\nfloat(9.5)\nint(9)\nfloat(9.5)\n",
        "9.5:44\n-7.5:-42\n3:2\n14:84\n168\n44\n",
        "Number must be of type GMP|string|int, SimpleXMLElement given|",
        "Number must be of type GMP|string|int, SimpleXMLElement given|",
        "Number must be of type GMP|string|int, SimpleXMLElement given|",
        "Number must be of type GMP|string|int, SimpleXMLElement given|",
        "Number must be of type GMP|string|int, SimpleXMLElement given|",
        "Unsupported operand types: GMP << SimpleXMLElement|",
    );
    for (name, source) in [
        ("aot", format!("{setup}{body}")),
        ("opaque-eval", format!("{setup}\n$code = $argc > 0 ? <<<'NATIVE_OPERATORS'\n{body}\nNATIVE_OPERATORS\n : '';\neval($code);\n")),
    ] {
        let output = compile_and_run_capture_merged_with_gmp(&source);
        assert!(output.success, "{name}: {}", output.stderr);
        assert_eq!(output.stdout, expected, "{name}: {}", output.stderr);
    }
}

/// BcMath's native handler sees the original XML operand before numeric conversion in either order.
#[test]
fn test_simplexml_bcmath_foreign_operand_dispatch_bundle() {
    if skip_without_xml_native("test_simplexml_bcmath_foreign_operand_dispatch_bundle") { return; }
    let setup = "<?php $xml = new SimpleXMLElement('<n>5</n>'); $number = new BcMath\\Number('3');";
    let body = r#"
function nativeForeignNumberAdd(mixed $left, mixed $right): void {
    try { $result = $left + $right; echo 'unexpected-result|'; }
    catch (TypeError $error) { echo $error->getMessage(), '|'; }
}
nativeForeignNumberAdd($xml, $number); nativeForeignNumberAdd($number, $xml);
"#;
    let expected = "Unsupported operand types: SimpleXMLElement + BcMath\\Number|Unsupported operand types: BcMath\\Number + SimpleXMLElement|";
    let aot = format!("{setup}{body}");
    let eval = format!("{setup}\n$code = $argc > 0 ? <<<'FOREIGN_NUMBER'\n{body}\nFOREIGN_NUMBER\n : ''; eval($code);");
    assert_bundle_with(&[
        BundleCase { name: "foreign_number_aot", source: &aot, expected },
        BundleCase { name: "foreign_number_eval", source: &eval, expected },
    ], |source| {
        let output = compile_and_run_capture_merged(source);
        assert!(output.success, "{}", output.stderr);
        output.stdout
    });
}
