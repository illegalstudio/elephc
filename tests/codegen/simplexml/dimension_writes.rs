//! Purpose:
//! Covers native dimension dispatch, explicit null offsets and delayed assignment receivers.
//!
//! Called from:
//! - Focused SimpleXML codegen CI with managed libxml2 and optional PHP reference checks.
//!
//! Key details:
//! - Actual runtime eval sources exercise the interpreter, and heap checks cover throwing appends.

use crate::support::*;

/// Explicit null is an empty attribute name, while every native append surface stays distinct.
#[test]
fn test_simplexml_dimension_null_and_append_bundle() {
    if skip_without_xml_native("test_simplexml_dimension_null_and_append_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
function appendNativeXmlMixed(mixed $view): void { $view[] = 'b'; }
function freshNativeXmlAppend(): string { return str_repeat('x', 3); }
function freshMixedNativeXmlAppend(string $text): mixed { return str_repeat($text, 1); }
class NativeXmlAppendHolder { public static mixed $view; }
$xml = new SimpleXMLElement('<r id="old"><item>keep</item></r>');
$node = $xml->item[0];
try { $node[null] = 'wrong'; } catch (ValueError $e) { echo $e->getMessage(), '|'; }
try { $xml[null] = 'wrong'; } catch (ValueError $e) { echo $e->getMessage(), '|'; }
try { $xml->item[null] = 'wrong'; } catch (ValueError $e) { echo $e->getMessage(), '|'; }
$node[] = 'a'; appendNativeXmlMixed($xml->item);
$holder = new stdClass(); $holder->view = $xml->item; $holder->view[] = 'c';
$parts = [$xml->item]; $parts[0][] = 'd';
NativeXmlAppendHolder::$view = $xml->item; NativeXmlAppendHolder::$view[] = 'e';
$attributes = $xml->attributes();
try { $attributes[] = freshNativeXmlAppend(); } catch (Error $e) { echo $e->getMessage(), '|'; }
try { $attributes[] = freshMixedNativeXmlAppend('x'); } catch (Error $e) { echo $e->getMessage(), '|'; }
$source = $argc > 0 ? '
$evaluated = $xml->item;
try { $evaluated[null] = "wrong"; } catch (ValueError $e) { echo $e->getMessage(), "|"; }
$evaluated[] = "f";
' : '';
eval($source);
$tail = $xml->item; $tail[] = freshMixedNativeXmlAppend('g');
NativeXmlAppendHolder::$view[] = freshMixedNativeXmlAppend('h');
echo count($xml->item), ':';
foreach ($xml->item as $item) { echo (string)$item, ','; }
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, concat!(
        "Cannot create attribute with an empty name|Cannot create attribute with an empty name|",
        "Cannot create attribute with an empty name|Cannot append to an attribute list|Cannot append to an attribute list|",
        "Cannot create attribute with an empty name|8:a,b,c,d,e,f,g,h,",
    ));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Keys and RHS precede target fetches, including rebinds and compounds in AOT and runtime eval.
#[test]
fn test_simplexml_dimension_write_timing_bundle() {
    if skip_without_xml_native("test_simplexml_dimension_write_timing_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
$timingXml = new SimpleXMLElement('<r/>');
function nativeXmlCompoundKey(): int { global $timingXml; echo (int)isset($timingXml->item), '|'; return 1; }
function nativeXmlCompoundRhs(): int { global $timingXml; echo (int)isset($timingXml->item), '|'; return 1; }
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
$timingXml->item[nativeXmlCompoundKey()] += nativeXmlCompoundRhs();
restore_error_handler();
echo count($timingXml->item), ':', (string)$timingXml->item, '|';
$xml = new SimpleXMLElement('<old/>'); $old = $xml;
$xml->item[0] = (function() use (&$xml): string { $xml = new SimpleXMLElement('<new/>'); return 'v'; })();
echo $old->getName(), ':', (int)isset($old->item), '|', $xml->getName(), ':', (string)$xml->item, '|';
$xml = new SimpleXMLElement('<old><item>1</item></old>'); $old = $xml;
$xml->item[0] += (function() use (&$xml): int { $xml = new SimpleXMLElement('<new><item>5</item></new>'); return 1; })();
echo (string)$old->item, ':', (string)$xml->item, '|';
class NativeXmlTimingStatic { public static SimpleXMLElement $xml; }
NativeXmlTimingStatic::$xml = new SimpleXMLElement('<sold/>'); $old = NativeXmlTimingStatic::$xml;
NativeXmlTimingStatic::$xml->item[0] = (function(): string { NativeXmlTimingStatic::$xml = new SimpleXMLElement('<snew/>'); return 's'; })();
echo $old->getName(), ':', (int)isset($old->item), '|', NativeXmlTimingStatic::$xml->getName(), ':', (string)NativeXmlTimingStatic::$xml->item, '|';
$source = $argc > 0 ? '
$evalTimingXml = new SimpleXMLElement("<r/>");
function evalNativeXmlCompoundKey(): int { global $evalTimingXml; echo (int)isset($evalTimingXml->item), "|"; return 1; }
function evalNativeXmlCompoundRhs(): int { global $evalTimingXml; echo (int)isset($evalTimingXml->item), "|"; return 1; }
set_error_handler(function(int $level, string $message): bool { echo $level, ":", $message, "|"; return true; });
$evalTimingXml->item[evalNativeXmlCompoundKey()] += evalNativeXmlCompoundRhs();
restore_error_handler();
echo count($evalTimingXml->item), ":", (string)$evalTimingXml->item, "|";
$evalXml = new SimpleXMLElement("<old/>"); $evalOld = $evalXml;
$evalXml->item[0] = (function() use (&$evalXml): string { $evalXml = new SimpleXMLElement("<new/>"); return "v"; })();
echo $evalOld->getName(), ":", (int)isset($evalOld->item), "|", $evalXml->getName(), ":", (string)$evalXml->item, "|";
$evalXml = new SimpleXMLElement("<old><item>1</item></old>"); $evalOld = $evalXml;
$evalXml->item[0] += (function() use (&$evalXml): int { $evalXml = new SimpleXMLElement("<new><item>5</item></new>"); return 1; })();
echo (string)$evalOld->item, ":", (string)$evalXml->item, "|";
class EvalNativeXmlTimingStatic { public static SimpleXMLElement $xml; }
EvalNativeXmlTimingStatic::$xml = new SimpleXMLElement("<sold/>"); $evalOld = EvalNativeXmlTimingStatic::$xml;
EvalNativeXmlTimingStatic::$xml->item[0] = (function(): string { EvalNativeXmlTimingStatic::$xml = new SimpleXMLElement("<snew/>"); return "s"; })();
echo $evalOld->getName(), ":", (int)isset($evalOld->item), "|", EvalNativeXmlTimingStatic::$xml->getName(), ":", (string)EvalNativeXmlTimingStatic::$xml->item, "|";
' : '';
eval($source);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, concat!(
        "0|0|2:main(): Cannot add element item number 1 when only 0 such elements exist|1:|old:0|new:v|1:6|sold:0|snew:s|",
        "0|0|2:Unknown: Cannot add element item number 1 when only 0 such elements exist|1:|old:0|new:v|1:6|sold:0|snew:s|",
    ));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Native dimensions bypass same-named user methods, while explicit method calls remain ordinary.
#[test]
fn test_simplexml_native_dimension_adapters_bundle() {
    if skip_without_xml_native("test_simplexml_native_dimension_adapters_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
class UserDimensionXml extends SimpleXMLElement implements ArrayAccess {
    public function offsetGet(mixed $key): mixed { echo 'public-get|'; return 'public'; }
    public function offsetSet(mixed $key, mixed $value): void { echo 'public-set|'; }
    public function offsetExists(mixed $key): bool { echo 'public-exists|'; return false; }
    public function offsetUnset(mixed $key): void { echo 'public-unset|'; }
}
function nativeMixedDimensionWrite(mixed $xml): void { $xml['id'] = 'mixed'; echo (string)$xml['id'], ':', (int)isset($xml['id']), '|'; }
$xml = new UserDimensionXml('<r id="a"><item>old</item></r>');
$xml['id'] = 'b'; echo (string)$xml['id'], ':', (int)isset($xml['id']), '|';
unset($xml['id']); echo (int)isset($xml['id']), '|';
$xml->item[0] = 'new'; $xml->item[] = 'tail';
echo count($xml->item), ':', (string)$xml->item[0], ':', (string)$xml->item[1], '|';
nativeMixedDimensionWrite($xml);
$xml->offsetGet('id'); $xml->offsetSet('id', 'ignored'); $xml->offsetExists('id'); $xml->offsetUnset('id');
echo (string)$xml['id'], '|';
foreach (['offsetGet', 'offsetSet', 'offsetExists', 'offsetUnset', '__get', '__set', '__isset', '__unset'] as $name) {
    echo (int)method_exists('SimpleXMLElement', $name);
}
$source = $argc > 0 ? '
$evalDimensions = new UserDimensionXml("<r id=\"a\"/>");
$evalDimensions["id"] = "eval";
echo "|", (string)$evalDimensions["id"], ":", (int)isset($evalDimensions["id"]);
unset($evalDimensions["id"]);
echo ":", (int)isset($evalDimensions["id"]);
' : '';
eval($source);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "b:1|0|2:new:tail|mixed:1|public-get|public-set|public-exists|public-unset|mixed|00000000|eval:1:0");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Compound arithmetic uses PHP's quiet numeric node conversion and ignores string overrides.
#[test]
fn test_simplexml_numeric_dimension_values_bundle() {
    if skip_without_xml_native("test_simplexml_numeric_dimension_values_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
foreach (['3', '3.5', 'abc', '12abc', '', '1e3'] as $text) {
    $node = new SimpleXMLElement('<n/>'); $node[0] = $text;
    $value = $node + 1;
    echo gettype($value), ':', $value, '|';
}
class NativeNumericXml extends SimpleXMLElement {
    public function __toString(): string { return '99'; }
}
$node = new NativeNumericXml('<n>2.5</n>');
echo (string)$node, ':', (int)$node, ':', (float)$node, ':', $node + 1, '|';
$node = new SimpleXMLElement('<n>8</n>');
echo $node - 1, ':', $node * 2, ':', $node / 4, ':', $node % 3, ':', $node ** 2;
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "integer:4|double:4.5|integer:1|integer:13|integer:1|double:1001|99:2:2.5:3.5|7:16:2:2:64");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
