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
