//! Purpose:
//! Checks native XML RW dimension selection independently of ordinary scalar writeback.
//!
//! Called from:
//! - The managed-libxml2 codegen integration suite.
//!
//! Key details:
//! - Indexed incdec is AOT-only; saved-view property writes also cover opaque eval.
//! - Expected notices are captured through the PHP error handler and checked in stdout.

use crate::support::*;

/// Computed XML indexed incdec keeps native no-writeback with one name conversion per update.
/// Opaque eval calls compiled incdec helpers and interprets supported indexed compound writes.
fn assert_computed_indexed_incdec(eval_mode: bool) {
    if skip_without_xml_native("test_simplexml_computed_indexed_incdec_bundle") { return; }
    let mut source = String::from(r#"<?php
class ComputedXmlChild extends SimpleXMLElement {}
class ComputedXmlName {
    public function __toString(): string { echo 's|'; return 'a'; }
    public function __destruct() { echo 'd|'; }
}
function computedXmlName(int $kind): mixed { echo 'f|'; return $kind === 0 ? 'a' : new ComputedXmlName(); }
function computedXmlTyped(SimpleXMLElement $node): SimpleXMLElement { echo 'g|'; return $node; }
function computedXmlMixed(mixed $node): mixed { echo 'g|'; return $node; }
function computedXmlChild(ComputedXmlChild $node): ComputedXmlChild { echo 'g|'; return $node; }
"#);
    for (suffix, ty) in [("Typed", "SimpleXMLElement"), ("Mixed", "mixed"), ("Child", "ComputedXmlChild")] {
        source.push_str(&format!(r#"
function updateComputedXml{suffix}({ty} $node, int $kind): void {{
    computedXml{suffix}($node)->{{computedXmlName($kind)}}[0]++;
    echo (string)$node->a, '|';
    --computedXml{suffix}($node)->{{computedXmlName($kind)}}[0];
    echo (string)$node->a, '|';
    computedXml{suffix}($node)->{{computedXmlName($kind)}}[0] += 2;
    echo (string)$node->a, '|';
}}
"#));
    }
    source.push_str(r#"
for ($kind = 0; $kind < 2; $kind++) {
    updateComputedXmlTyped(new SimpleXMLElement('<r><a>5</a></r>'), $kind);
    updateComputedXmlMixed(new SimpleXMLElement('<r><a>5</a></r>'), $kind);
    updateComputedXmlChild(new ComputedXmlChild('<r><a>5</a></r>'), $kind);
}
"#);
    let renamed = source.replace("ComputedXml", "EvalComputedXml").replace("computedXml", "evalComputedXml");
    let (setup, _) = renamed.split_once("for ($kind = 0; $kind < 2; $kind++)").unwrap();
    let mut setup = setup.to_string();
    let mut calls = String::from("for ($kind = 0; $kind < 2; $kind++) {\n");
    for (suffix, class) in [("Typed", "SimpleXMLElement"), ("Mixed", "SimpleXMLElement"), ("Child", "EvalComputedXmlChild")] {
        // The compiled helper keeps actual ++/--; the compound operation stays inside eval.
        let compound = format!("    evalComputedXml{suffix}($node)->{{evalComputedXmlName($kind)}}[0] += 2;\n    echo (string)$node->a, '|';\n");
        assert!(setup.contains(&compound));
        setup = setup.replace(&compound, "");
        calls.push_str(&format!(r#"
    $node = new {class}('<r><a>5</a></r>');
    updateEvalComputedXml{suffix}($node, $kind);
    evalComputedXml{suffix}($node)->{{evalComputedXmlName($kind)}}[0] += 2;
    echo (string)$node->a, '|';
"#));
    }
    calls.push_str("}\nunset($node);\n");
    let body = calls.replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("{setup}\n$code = $argc > 0 ? '{body}' : ''; eval($code);");
    let expected = ["g|f|5|g|f|5|g|f|7|".repeat(3),
        "g|f|s|d|5|g|f|s|d|5|g|f|s|d|7|".repeat(3)].concat();
    let source = if eval_mode { &eval } else { &source };
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Native indexed updates keep every typed, erased and subclass case in their own program.
#[test]
fn test_simplexml_computed_indexed_incdec_native_bundle() {
    assert_computed_indexed_incdec(false);
}

/// Eval still invokes compiled incdec helpers and performs its own compound writes.
#[test]
fn test_simplexml_computed_indexed_incdec_eval_bundle() {
    assert_computed_indexed_incdec(true);
}

/// Local, static and array-held XML incdec selects without writing its numeric result back.
#[test]
fn test_simplexml_dimension_incdec_places() {
    if skip_without_xml_native("test_simplexml_dimension_incdec_places") { return; }
    let source = r###"<?php
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
function DimensionUpdateOpaque(mixed $value): mixed { return $value; }
function DimensionUpdateKey(): int { echo 'key|'; return 0; }
class DimensionUpdateStatic {
    public static SimpleXMLElement $xml;
    public static array $bag = [];
    public static mixed $ordinary;
}
class DimensionUpdateHolder { public SimpleXMLElement $xml; }
$doc = new SimpleXMLElement('<r><mark>7</mark></r>');
$view = $doc->mark;
$view[0]++; ++$view[0]; $view[0]--; --$view[0];
$view[DimensionUpdateKey()]++;
$bag = ['x' => $doc->mark];
$bag['x'][0]++; ++$bag['x'][0]; $bag['x'][0]--; --$bag['x'][0];
DimensionUpdateStatic::$xml = $doc->mark;
DimensionUpdateStatic::$xml[0]++; ++DimensionUpdateStatic::$xml[0];
DimensionUpdateStatic::$xml[0]--; --DimensionUpdateStatic::$xml[0];
DimensionUpdateStatic::$bag = [$doc->mark];
DimensionUpdateStatic::$bag[0][0]++; --DimensionUpdateStatic::$bag[0][0];
$opaque = DimensionUpdateOpaque($doc->mark);
$opaque[0]++; --$opaque[0];
echo $doc->asXML();
$empty = $doc->other;
$empty[0]++; ++$empty[0]; $empty[0]--; --$empty[0];
echo $doc->asXML();
$holder = new DimensionUpdateHolder();
$holder->xml = $doc->other;
$holder->xml[0]++;
echo $doc->asXML();
$namespaced = new SimpleXMLElement('<r xmlns:p="urn:p"><p:a>kept</p:a></r>');
$orphan = clone $namespaced->children('urn:p')->a[0];
$orphanChildren = $orphan->children('urn:p');
if ($orphanChildren instanceof SimpleXMLElement) { $holder->xml = $orphanChildren; }
else { throw new Error('expected orphan child view'); }
$holder->xml[0]++;
echo $orphan->asXML(), "\n";
$ordinary = DimensionUpdateOpaque([4, 9]);
$ordinary[0]++; --$ordinary[1];
DimensionUpdateStatic::$ordinary = [1, 3];
$ordinaryAlias = DimensionUpdateStatic::$ordinary;
DimensionUpdateStatic::$ordinary[0]++; --DimensionUpdateStatic::$ordinary[1];
echo json_encode($ordinary), ':', json_encode(DimensionUpdateStatic::$ordinary), ':', json_encode($ordinaryAlias), "\n";
DimensionUpdateStatic::$bag = [];
DimensionUpdateStatic::$ordinary = null;
unset($doc, $view, $bag, $opaque, $empty, $holder, $namespaced, $orphan, $ordinary, $ordinaryAlias);
restore_error_handler();
"###;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:") && !out.stderr.contains("Notice:"), "{}", out.stderr);
    assert_eq!(out.stdout, "key|<?xml version=\"1.0\"?>\n<r><mark>7</mark></r>\n8:Indirect modification of overloaded element of SimpleXMLElement has no effect|8:Indirect modification of overloaded element of SimpleXMLElement has no effect|8:Indirect modification of overloaded element of SimpleXMLElement has no effect|2:Decrement on type null has no effect, this will change in the next major version of PHP|8:Indirect modification of overloaded element of SimpleXMLElement has no effect|2:Decrement on type null has no effect, this will change in the next major version of PHP|<?xml version=\"1.0\"?>\n<r><mark>7</mark></r>\n8:Indirect modification of overloaded element of SimpleXMLElement has no effect|<?xml version=\"1.0\"?>\n<r><mark>7</mark></r>\n<p:a xmlns:p=\"urn:p\">kept</p:a>\n[5,8]:[2,2]:[1,3]\n");
}

/// Static incdec returns its PHP result while each XML selection remains unchanged.
#[test]
fn test_simplexml_static_dimension_incdec_expression_results() {
    if skip_without_xml_native("test_simplexml_static_dimension_incdec_expression_results") { return; }
    let source = r#"<?php
class NativeStaticStep {
    public static SimpleXMLElement $typed;
    public static mixed $erased;
}
function nativeStaticStepKey(): int { echo 'key|'; return 0; }
function reportNativeStaticStep(mixed $value, SimpleXMLElement $doc): void {
    echo gettype($value), ':', (string)$value, ':', (string)$doc->a, '|';
}
$doc = new SimpleXMLElement('<r><a>7</a></r>');
NativeStaticStep::$typed = $doc->a;
NativeStaticStep::$erased = $doc->a;
reportNativeStaticStep(NativeStaticStep::$typed[nativeStaticStepKey()]++, $doc);
reportNativeStaticStep(++NativeStaticStep::$typed[0], $doc);
reportNativeStaticStep(NativeStaticStep::$typed[0]--, $doc);
reportNativeStaticStep(--NativeStaticStep::$typed[0], $doc);
reportNativeStaticStep(NativeStaticStep::$erased[0]++, $doc);
reportNativeStaticStep(++NativeStaticStep::$erased[0], $doc);
reportNativeStaticStep(NativeStaticStep::$erased[0]--, $doc);
reportNativeStaticStep(--NativeStaticStep::$erased[0], $doc);
NativeStaticStep::$typed[0]++;
echo 'discard:', (string)$doc->a, '|';
$old = NativeStaticStep::$typed[0]++;
$doc->a = 12;
echo 'view:', (string)$old, '|';
NativeStaticStep::$erased = [4, 9];
echo NativeStaticStep::$erased[0]++, ':', ++NativeStaticStep::$erased[1], ':', json_encode(NativeStaticStep::$erased), '|';
$created = NativeStaticStep::$typed[1]++;
echo 'created:', gettype($created), ':', count($doc->a), ':', (string)$doc->a[1], '|';
NativeStaticStep::$erased = null;
unset($old, $created, $doc);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    let operations = "object:7:7|integer:8:7|object:7:7|integer:6:7|".repeat(2);
    assert_eq!(output.stdout, format!("key|{operations}discard:7|view:12|4:10:[5,10]|created:object:2:|"));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Null decrement warnings can throw repeatedly without retaining selected XML views.
#[test]
fn test_simplexml_null_dimension_decrement_handler_unwind() {
    if skip_without_xml_native("test_simplexml_null_dimension_decrement_handler_unwind") { return; }
    let source = r#"<?php
set_error_handler(function(int $level, string $message): bool {
    if ($level === 2) { throw new RuntimeException('null-decrement'); }
    return true;
});
$doc = new SimpleXMLElement('<r/>');
$empty = $doc->absent;
for ($i = 0; $i < 3; $i++) {
    try { --$empty[0]; }
    catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
}
echo $doc->asXML();
unset($doc, $empty, $error);
restore_error_handler();
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "null-decrement|null-decrement|null-decrement|<?xml version=\"1.0\"?>\n<r/>\n");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Saved missing views report the indirect notice and leave the document unchanged on failure.
#[test]
fn test_simplexml_saved_empty_dimension_write_bundle() {
    if skip_without_xml_native("test_simplexml_saved_empty_dimension_write_bundle") { return; }
    let source = r###"<?php
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
class HeldDimensionHolder { public SimpleXMLElement $xml; }
$doc = new SimpleXMLElement('<r/>');
$view = $doc->other;
try { $view[0]->y = 1; } catch (Error $error) { echo $error->getMessage(), '|'; }
echo $doc->asXML();
$held = [$doc->other];
try { $held[0][0]->y = 1; } catch (Error $error) { echo $error->getMessage(), '|'; }
echo $doc->asXML();
$holder = new HeldDimensionHolder();
$holder->xml = $doc->other;
try { $holder->xml[0]->y = 1; } catch (Error $error) { echo $error->getMessage(), '|'; }
echo $doc->asXML();
$namespaced = new SimpleXMLElement('<r xmlns:p="urn:p"><p:a>kept</p:a></r>');
$orphan = clone $namespaced->children('urn:p')->a[0];
try { $orphan->children('urn:p')[0]->y = 1; } catch (Error $error) { echo $error->getMessage(), '|'; }
echo $orphan->asXML(), "\n";
$attached = new SimpleXMLElement('<r/>');
$attached->children()[0]->y = 1;
echo $attached->asXML();
unset($doc, $view, $held, $holder, $namespaced, $orphan, $attached, $error);
restore_error_handler();
"###;
    let renamed = source.replace("HeldDimensionHolder", "EvalHeldDimensionHolder");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let expected = "8:Indirect modification of overloaded element of SimpleXMLElement has no effect|Attempt to assign property \"y\" on null|<?xml version=\"1.0\"?>\n<r/>\n8:Indirect modification of overloaded element of SimpleXMLElement has no effect|Attempt to assign property \"y\" on null|<?xml version=\"1.0\"?>\n<r/>\n8:Indirect modification of overloaded element of SimpleXMLElement has no effect|Attempt to assign property \"y\" on null|<?xml version=\"1.0\"?>\n<r/>\nSimpleXMLElement is not properly initialized|<p:a xmlns:p=\"urn:p\">kept</p:a>\n<?xml version=\"1.0\"?>\n<r/>\n<r><y>1</y></r>\n";
    assert_bundle_with(&[
        BundleCase { name: "saved_dimension_aot", source, expected },
        BundleCase { name: "saved_dimension_eval", source: &eval, expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        assert!(!out.stderr.contains("Warning:") && !out.stderr.contains("Notice:"), "{}", out.stderr);
        out.stdout
    });
}

/// Every nested or opaque holder selects XML nodes without scalar writeback in all four forms.
#[test]
fn test_simplexml_nested_holder_dimension_incdec() {
    if skip_without_xml_native("test_simplexml_nested_holder_dimension_incdec") { return; }
    let source = r###"<?php
function NestedDimensionOpaque(mixed $value): mixed { return $value; }
function NestedDimensionKey(): int { echo 'key|'; return 0; }
class NestedDimensionHolder {
    public array $bag = [];
    public $untyped;
    public mixed $boxed;
}
$doc = new SimpleXMLElement('<r><mark>7</mark></r>');
$view = $doc->mark;
$view[0][0]++; echo (string)$doc->mark, '|';
++$view[0][0]; echo (string)$doc->mark, '|';
$view[0][0]--; echo (string)$doc->mark, '|';
--$view[0][0]; echo (string)$doc->mark, '|';
$doc->mark[0][0]++; echo (string)$doc->mark, '|';
++$doc->mark[0][0]; echo (string)$doc->mark, '|';
$doc->mark[0][0]--; echo (string)$doc->mark, '|';
--$doc->mark[0][0]; echo (string)$doc->mark, '|';
$holder = new NestedDimensionHolder();
$holder->bag['x'] = $doc->mark;
$holder->bag['x'][0]++; echo (string)$doc->mark, '|';
++$holder->bag['x'][0]; echo (string)$doc->mark, '|';
$holder->bag['x'][0]--; echo (string)$doc->mark, '|';
--$holder->bag['x'][0]; echo (string)$doc->mark, '|';
$holder->untyped = $doc->mark;
$holder->untyped[0]++; echo (string)$doc->mark, '|';
++$holder->untyped[0]; echo (string)$doc->mark, '|';
$holder->untyped[0]--; echo (string)$doc->mark, '|';
--$holder->untyped[0]; echo (string)$doc->mark, '|';
$holder->boxed = NestedDimensionOpaque($doc->mark);
$holder->boxed[0]++; echo (string)$doc->mark, '|';
++$holder->boxed[0]; echo (string)$doc->mark, '|';
$holder->boxed[0]--; echo (string)$doc->mark, '|';
--$holder->boxed[0]; echo (string)$doc->mark, '|';
$opaque = NestedDimensionOpaque($holder);
$opaque->untyped[0]++; echo (string)$doc->mark, '|';
++$opaque->untyped[0]; echo (string)$doc->mark, '|';
$opaque->untyped[0]--; echo (string)$doc->mark, '|';
--$opaque->untyped[0]; echo (string)$doc->mark, '|';
$view[NestedDimensionKey()][NestedDimensionKey()]++;
$holder->untyped[NestedDimensionKey()]++;
echo $doc->asXML();
$holder->bag['n'] = [4];
$holder->bag['n'][0]++; echo $holder->bag['n'][0], '|';
++$holder->bag['n'][0]; echo $holder->bag['n'][0], '|';
$holder->bag['n'][0]--; echo $holder->bag['n'][0], '|';
--$holder->bag['n'][0]; echo $holder->bag['n'][0], '|';
$holder->untyped = [4];
$holder->untyped[0]++; echo $holder->untyped[0], '|';
++$holder->untyped[0]; echo $holder->untyped[0], '|';
$holder->untyped[0]--; echo $holder->untyped[0], '|';
--$holder->untyped[0]; echo $holder->untyped[0], '|';
$holder->boxed = [[4]];
$holder->boxed[0][0]++; echo $holder->boxed[0][0], '|';
++$holder->boxed[0][0]; echo $holder->boxed[0][0], '|';
$holder->boxed[0][0]--; echo $holder->boxed[0][0], '|';
--$holder->boxed[0][0]; echo $holder->boxed[0][0], '|';
unset($doc, $view, $holder, $opaque);
"###;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:") && !out.stderr.contains("Notice:"), "{}", out.stderr);
    let expected = format!("{}key|key|key|<?xml version=\"1.0\"?>\n<r><mark>7</mark></r>\n{}", "7|".repeat(24), "5|6|5|4|".repeat(3));
    assert_eq!(out.stdout, expected);
}
