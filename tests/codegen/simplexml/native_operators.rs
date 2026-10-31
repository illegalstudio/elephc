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

/// Typed ordinary updates preserve scalar writes and expression results while XML is loaded.
#[test]
fn test_simplexml_ordinary_typed_update_bridge_bundle() {
    if skip_without_xml_native("test_simplexml_ordinary_typed_update_bridge_bundle") { return; }
    let setup = r#"<?php
class OrdinaryXmlEnabledCounter {
    public int $number = 3;
    public string $text = 'a';
    public function receiver(): self { echo 'r|'; return $this; }
}
class OrdinaryXmlEnabledName {
    public function __toString(): string { echo 'n|'; return 'number'; }
}
function updateOrdinaryXmlEnabledCounter(OrdinaryXmlEnabledCounter $counter): void {
    $counter->receiver()->number += 2; echo $counter->number, '|';
    echo ++$counter->receiver()->number, '|';
    echo $counter->receiver()->number++, '|';
    echo ($counter->receiver()->text .= 'b'), '|';
    $counter->{new OrdinaryXmlEnabledName()} += 1;
    echo $counter->number, '|';
}
$xml = new SimpleXMLElement('<r/>');
"#;
    let body = r#"
$counter = new OrdinaryXmlEnabledCounter();
updateOrdinaryXmlEnabledCounter($counter);
unset($counter, $xml);
"#;
    let aot = format!("{setup}{body}");
    let eval_setup = setup.replace("OrdinaryXmlEnabled", "EvalOrdinaryXmlEnabled");
    let eval_body = body.replace("OrdinaryXmlEnabled", "EvalOrdinaryXmlEnabled")
        .replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("{eval_setup}\n$code = $argc > 0 ? '{eval_body}' : ''; eval($code);");
    let expected = "r|5|r|6|r|6|r|ab|n|8|";
    assert_bundle_with(&[
        BundleCase { name: "ordinary_xml_enabled_aot", source: &aot, expected },
        BundleCase { name: "ordinary_xml_enabled_eval_bridge", source: &eval, expected },
    ], |source| {
        let output = compile_and_run_with_heap_debug(source);
        assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
        output.stdout
    });
}

/// Mixed native refusal keeps scalar byte complements, conversion diagnostics and fresh owners.
#[test]
fn test_simplexml_bitnot_scalar_fallback_bundle() {
    if skip_without_xml_native("test_simplexml_bitnot_scalar_fallback_bundle") { return; }
    let source = r#"<?php
$xml = new SimpleXMLElement('<r/>');
function scalarNativeBitnot(mixed $value): mixed { return ~$value; }
function reportScalarNativeBitnot(mixed $value): void {
    try {
        $result = scalarNativeBitnot($value);
        echo is_string($result) ? bin2hex($result) : $result, '|';
    } catch (TypeError $error) { echo $error->getMessage(), '|'; }
}
set_error_handler(function(int $severity, string $message): bool { echo $message, '|'; return true; });
reportScalarNativeBitnot(5); reportScalarNativeBitnot('A'); reportScalarNativeBitnot(1.5);
reportScalarNativeBitnot(true); reportScalarNativeBitnot(false); reportScalarNativeBitnot(null);
reportScalarNativeBitnot([]); reportScalarNativeBitnot(new stdClass());
restore_error_handler();
set_error_handler(function(int $severity, string $message): bool { throw new RuntimeException('bitnot-handler'); });
try { scalarNativeBitnot(1.5); }
catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
restore_error_handler();
$source = str_repeat('A', 70000);
$result = scalarNativeBitnot($source);
echo strlen($result), ':', ord($result[0]), ':', ord($result[69999]), ':', ord($source[0]), '|';
unset($xml, $source, $result);
"#;
    let renamed = source.replace("ScalarNativeBitnot", "EvalScalarNativeBitnot")
        .replace("scalarNativeBitnot", "evalScalarNativeBitnot");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $code = $argc > 0 ? '{body}' : ''; eval($code);");
    let expected = concat!(
        "-6|be|Implicit conversion from float 1.5 to int loses precision|-2|",
        "Cannot perform bitwise not on true|Cannot perform bitwise not on false|",
        "Cannot perform bitwise not on null|Cannot perform bitwise not on array|",
        "Cannot perform bitwise not on stdClass|bitnot-handler|70000:190:190:65|",
    );
    assert_bundle_with(&[
        BundleCase { name: "bitnot_scalar_aot", source, expected },
        BundleCase { name: "bitnot_scalar_eval", source: &eval, expected },
    ], |source| {
        let output = compile_and_run_with_heap_debug(source);
        assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
        output.stdout
    });
}

/// Bitwise NOT refuses native XML and subclasses through typed AOT and runtime eval dispatch.
#[test]
fn test_simplexml_bitnot_refusal_bundle() {
    if skip_without_xml_native("test_simplexml_bitnot_refusal_bundle") { return; }
    let setup = r#"<?php
class BitnotNativeXml extends SimpleXMLElement {}
$xml = new SimpleXMLElement('<n>5</n>');
$iterator = new SimpleXMLIterator('<n>5</n>');
$child = new BitnotNativeXml('<n>5</n>');
"#;
    let body = r#"
function rejectNativeBitnot(mixed $value): void {
    try { ~$value; echo 'unexpected|'; }
    catch (TypeError $error) { echo $error->getMessage(), '|'; }
}
try { ~$xml; echo 'unexpected|'; }
catch (TypeError $error) { echo $error->getMessage(), '|'; }
try { $unused = ~$iterator; echo 'unexpected|'; }
catch (TypeError $error) { echo $error->getMessage(), '|'; }
try { $unused = ~$child; echo 'unexpected|'; }
catch (TypeError $error) { echo $error->getMessage(), '|'; }
rejectNativeBitnot($xml); rejectNativeBitnot($iterator); rejectNativeBitnot($child);
"#;
    let expected = concat!(
        "Cannot perform bitwise not on SimpleXMLElement|Cannot perform bitwise not on SimpleXMLIterator|Cannot perform bitwise not on BitnotNativeXml|",
        "Cannot perform bitwise not on SimpleXMLElement|Cannot perform bitwise not on SimpleXMLIterator|Cannot perform bitwise not on BitnotNativeXml|",
    );
    let aot = format!("{setup}{body}");
    let eval_setup = setup.replace("BitnotNativeXml", "EvalBitnotNativeXml");
    let eval_body = body.replace("rejectNativeBitnot", "rejectEvalNativeBitnot");
    let eval_expected = expected.replace("BitnotNativeXml", "EvalBitnotNativeXml");
    let eval = format!("{eval_setup}\n$code = $argc > 0 ? <<<'BITNOT_NATIVE'\n{eval_body}\nBITNOT_NATIVE\n : ''; eval($code);");
    assert_bundle_with(&[
        BundleCase { name: "bitnot_xml_aot", source: &aot, expected },
        BundleCase { name: "bitnot_xml_eval", source: &eval, expected: &eval_expected },
    ], |source| {
        let output = compile_and_run_with_heap_debug(source);
        assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
        output.stdout
    });
}

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
    let eval_body = body.replace("nativeForeignNumberAdd", "evalForeignNumberAdd");
    let eval = format!("{setup}\n$code = $argc > 0 ? <<<'FOREIGN_NUMBER'\n{eval_body}\nFOREIGN_NUMBER\n : ''; eval($code);");
    assert_bundle_with(&[
        BundleCase { name: "foreign_number_aot", source: &aot, expected },
        BundleCase { name: "foreign_number_eval", source: &eval, expected },
    ], |source| {
        let output = compile_and_run_capture_merged(source);
        assert!(output.success, "{}", output.stderr);
        output.stdout
    });
}
