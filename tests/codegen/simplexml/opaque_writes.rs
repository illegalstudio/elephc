//! Purpose:
//! Covers checker and native write dispatch for opaque and static XML receivers.
//!
//! Called from:
//! - Codegen CI with managed libxml2 and PHP reference checks.
//!
//! Key details:
//! - Ordinary stdClass storage remains on the existing property-array runtime path.

use crate::support::*;

/// Enabling opaque writes preserves ordinary property append and COW without an XML capability.
#[test]
fn test_mixed_property_append_without_simplexml() {
    let out = compile_and_run_with_heap_debug(r###"<?php
function appendOrdinaryMixedProperty(mixed $node): void { $node->items[] = 'b'; }
$node = new stdClass(); $node->items = ['a'];
$snapshot = $node->items;
appendOrdinaryMixedProperty($node);
echo implode(',', $snapshot), '|', implode(',', $node->items);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "a|a,b");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Mixed, object and interface parameters preserve XML direct writes, indexed writes and append.
#[test]
fn test_simplexml_opaque_property_and_dimension_writes() {
    if skip_without_xml_native("test_simplexml_opaque_property_and_dimension_writes") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
function xmlWriteMixed(mixed $node): void { $node->item[0] = 'a'; $node->item[] = 'b'; }
function xmlWriteObject(object $node): void { $node->direct = 'c'; $node->item[1] = 'd'; }
function xmlWriteCountable(Countable $node): void { $node->direct = 'f'; $node->item[0] = 'e'; }
function xmlWriteRecursive(RecursiveIterator $node): void { $node->item[] = 'g'; }
$xml = new SimpleXMLElement('<r/>');
xmlWriteMixed($xml); xmlWriteObject($xml); xmlWriteCountable($xml); xmlWriteRecursive($xml);
echo count($xml->item), ':', (string)$xml->item[0], ':', (string)$xml->item[1], ':', (string)$xml->item[2], ':', (string)$xml->direct, '|';
$ordinary = new stdClass(); $ordinary->item = ['old'];
xmlWriteMixed($ordinary); echo implode(',', $ordinary->item), '|'; xmlWriteObject($ordinary);
echo implode(',', $ordinary->item), ':', $ordinary->direct;
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "3:e:d:g:f|a,b|a,d:c");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Opaque property access keeps native XML precedence over user hooks and ordinary storage.
#[test]
fn test_simplexml_opaque_property_adapters_ignore_hooks() {
    if skip_without_xml_native("test_simplexml_opaque_property_adapters_ignore_hooks") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
class OpaquePropertyXml extends SimpleXMLElement {
    public string $a = 'slot';
    public function __get(string $name): mixed { echo 'wrong|'; return 'wrong'; }
    public function __set(string $name, mixed $value): void { echo 'wrong|'; }
    public function __isset(string $name): bool { echo 'wrong|'; return false; }
    public function __unset(string $name): void { echo 'wrong|'; }
}
function mutateOpaqueXmlProperty(object $node): void {
    $node->a = 'changed';
    echo (string)$node->a, ':', (int)isset($node->a), '|';
    unset($node->a);
    echo (int)isset($node->a), '|';
}
$xml = new OpaquePropertyXml('<r><a>old</a></r>');
mutateOpaqueXmlProperty($xml);
$ordinary = new stdClass(); $ordinary->a = 'old';
mutateOpaqueXmlProperty($ordinary);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "changed:1|0|changed:1|0|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Static chains delay write-fetch until both key and value expressions finish.
#[test]
fn test_simplexml_static_write_fetch_order() {
    if skip_without_xml_native("test_simplexml_static_write_fetch_order") { return; }
    let out = compile_and_run(r###"<?php
class XmlStaticWriteHolder { public static SimpleXMLElement $xml; }
function xmlStaticWriteKey(): int {
    echo (int)isset(XmlStaticWriteHolder::$xml->item), '|';
    return isset(XmlStaticWriteHolder::$xml->item) ? 0 : 1;
}
XmlStaticWriteHolder::$xml = new SimpleXMLElement('<r/>');
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
XmlStaticWriteHolder::$xml->item[xmlStaticWriteKey()] = 'v';
restore_error_handler();
echo XmlStaticWriteHolder::$xml->asXML();
class PlainStaticWriteHolder { public static stdClass $node; }
function replacePlainStaticWrite(): string {
    PlainStaticWriteHolder::$node = new stdClass();
    PlainStaticWriteHolder::$node->items = ['new'];
    return 'rhs';
}
PlainStaticWriteHolder::$node = new stdClass(); PlainStaticWriteHolder::$node->items = ['old'];
$old = PlainStaticWriteHolder::$node;
PlainStaticWriteHolder::$node->items[0] = replacePlainStaticWrite();
echo $old->items[0], ':', PlainStaticWriteHolder::$node->items[0];
"###);
    assert_eq!(out, "0|2:main(): Cannot add element item number 1 when only 0 such elements exist|<?xml version=\"1.0\"?>\n<r><item/></r>\nold:rhs");
}

/// Only converted non-string offsets are trimmed before checking or creating an attribute name.
#[test]
fn test_simplexml_converted_dimension_key_trim() {
    if skip_without_xml_native("test_simplexml_converted_dimension_key_trim") { return; }
    let out = compile_and_run(r###"<?php
class XmlConvertedOffsetKey implements Stringable {
    private string $name;
    public function __construct(string $name) { $this->name = $name; }
    public function __toString(): string { return $this->name; }
}
$xml = new SimpleXMLElement('<r/>');
try { $xml[new XmlConvertedOffsetKey('  ')] = 'wrong'; }
catch (ValueError $error) { echo $error->getMessage(), '|'; }
$xml[new XmlConvertedOffsetKey(' id ')] = 'converted';
$xml[' id '] = 'literal';
echo (string)$xml['id'], ':', (string)$xml[' id '];
"###);
    assert_eq!(out, "Cannot create attribute with an empty name|converted:literal");
}
/// Runtime property getters release their cell owner after successful and throwing offset stores.
#[test]
fn test_mixed_property_indexed_write_retires_getter_cell() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class RejectPropertyOffset implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return false; }
    public function offsetGet(mixed $key): mixed { return null; }
    public function offsetSet(mixed $key, mixed $value): void { throw new RuntimeException('rejected'); }
    public function offsetUnset(mixed $key): void {}
}
function storePropertyOffset(mixed $holder, int $index): void {
    $holder->item[$index] = str_repeat('v', 40);
}
$holder = new stdClass();
$holder->item = [];
for ($i = 0; $i < 4; $i++) { storePropertyOffset($holder, $i); }
echo count($holder->item), '|';
$holder->item = new RejectPropertyOffset();
for ($i = 0; $i < 4; $i++) {
    try { storePropertyOffset($holder, $i); }
    catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
}
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "4|rejected|rejected|rejected|rejected|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
