//! Purpose:
//! Covers typed scalar, object, and interface continuations in native empty lowering.
//!
//! Called from:
//! - Codegen CI with managed libxml2.
//!
//! Key details:
//! - Ordinary scalar receivers must never produce a native XML method call.

use crate::support::*;

/// Scalar continuations preserve ordinary empty semantics and evaluate the final key.
#[test]
fn test_simplexml_empty_typed_scalar_continuations() {
    if skip_without_xml_native("test_simplexml_empty_typed_scalar_continuations") { return; }
    let source = r###"<?php
class EmptyScalarPeer { public int $child = 0; public string $text = '0'; }
function emptyScalarName(): string { echo 'name|'; return 'x'; }
$xml = new SimpleXMLElement('<r/>');
$peer = new EmptyScalarPeer();
echo (int)empty($peer?->child->{emptyScalarName()}), '|';
echo (int)empty($peer?->child[0]), '|', (int)empty($peer?->text[0]);
"###;
    assert_eq!(compile_and_run(source), "name|1|1|1");
    assert_eq!(without_ir_opt(|| compile_and_run(source)), "name|1|1|1");
}

/// XML content checks survive bare-object and interface hints, including borrowed boxed receivers.
#[test]
fn test_simplexml_empty_object_and_interface_parameters() {
    if skip_without_xml_native("test_simplexml_empty_object_and_interface_parameters") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
function emptyXmlObject(object $node): bool { return empty($node->zero); }
function emptyXmlCountable(Countable $node): bool { return empty($node->zero); }
function emptyXmlRecursive(RecursiveIterator $node): bool { return empty($node->zero); }
function emptyXmlIterator(Iterator $node): bool { return empty($node->zero); }
function emptyXmlTraversable(Traversable $node): bool { return empty($node->zero); }
function emptyXmlStringable(Stringable $node): bool { return empty($node->zero); }
$xml = new SimpleXMLElement('<r><zero>0</zero></r>');
echo (int)emptyXmlObject($xml), (int)emptyXmlCountable($xml), (int)emptyXmlRecursive($xml), (int)emptyXmlIterator($xml), (int)emptyXmlTraversable($xml), (int)emptyXmlStringable($xml), '|';
$ordinary = new stdClass(); $ordinary->zero = 'kept';
echo (int)emptyXmlObject($ordinary);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "111111|0");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Native unions use internal adapters for isset, empty and unset, including real null receivers.
#[test]
fn test_simplexml_native_union_dimension_probes() {
    if skip_without_xml_native("test_simplexml_native_union_dimension_probes") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
function nativeUnionProbe(SimpleXMLElement|SimpleXMLIterator|null $node): void {
    echo (int)isset($node['id']), ':', (int)empty($node['id']), '|';
    unset($node['id']);
    echo (int)isset($node['id']), '|';
}
nativeUnionProbe(null);
nativeUnionProbe(new SimpleXMLElement('<r id="x"/>'));
nativeUnionProbe(new SimpleXMLIterator('<r id="0"/>'));
"#);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "0:1|0|1:0|0|1:1|0|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
