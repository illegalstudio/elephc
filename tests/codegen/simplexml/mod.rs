//! Purpose:
//! PHP-oracle SimpleXML end-to-end coverage for the native document bridge.
//!
//! Called from:
//! - The codegen integration suite on Linux and macOS CI.
//!
//! Key details:
//! - Each fixture requires the managed libxml2 artifact, with explicit skip gating.

use crate::support::*;

/// Verifies SimpleXML parsing and properties against PHP's observable output.
#[test]
fn test_simplexml_parsing_and_properties() {
    if skip_without_xml_native("test_simplexml_parsing_and_properties") { return; }
    let out = compile_and_run(r###"<?php
$x = \SIMPLEXML_LOAD_STRING('<catalog><book id="1"><title>Compiler</title></book><book id="2"><title>XML</title></book></catalog>');
if ($x instanceof SimpleXMLElement) {
 echo get_class($x), ':', $x->getName(), ':', count($x), ':', count($x->book), "\n";
 echo (string)$x->book[0]->title, ':', (string)$x->book[1]['id'], "\n";
 var_dump(isset($x->book), isset($x->missing), isset($x->book[0]['id']), $x->missing->getName());
}
"###);
    assert_eq!(out, "SimpleXMLElement:catalog:2:2\nCompiler:2\nbool(true)\nbool(false)\nbool(true)\nstring(0) \"\"\n");
}

/// Verifies SimpleXML mutations are live against PHP's observable output.
#[test]
fn test_simplexml_mutations_are_live() {
    if skip_without_xml_native("test_simplexml_mutations_are_live") { return; }
    let out = compile_and_run(r###"<?php
$x = new SimpleXMLElement('<root><item>A</item><item>B</item></root>');
$items = $x->item;
$x->item[0] = 'A&B';
$x->item[] = 'C';
$x['id'] = 'x<y';
$x->nested->leaf = 'yes';
echo count($items), ':', (string)$items[0], ':', (string)$x['id'], ':', (string)$x->nested->leaf, "\n";
unset($x->item[1], $x['id']);
echo count($items), ':', (string)$items[1], ':', (int)isset($x['id']), "\n";
"###);
    assert_eq!(out, "3:A&B:x<y:yes\n2:C:0\n");
}

/// Verifies SimpleXML namespace filters and xpath against PHP's observable output.
#[test]
fn test_simplexml_namespace_filters_and_xpath() {
    if skip_without_xml_native("test_simplexml_namespace_filters_and_xpath") { return; }
    let out = compile_and_run(r###"<?php
$x = new SimpleXMLElement('<r xmlns:p="urn:p"><p:item p:id="7">one</p:item><item>two</item></r>');
echo count($x->children()), ':', count($x->children('urn:p')), ':', (string)$x->children('p', true)->item, "\n";
echo (string)$x->children('p', true)->item->attributes('p', true)['id'], "\n";
$x->registerXPathNamespace('q', 'urn:p');
foreach ($x->xpath('//q:item') as $node) { echo $node->getName(), ':', (string)$node, "\n"; }
echo json_encode($x->getDocNamespaces(true)), ':', json_encode($x->getNamespaces(true)), "\n";
"###);
    assert_eq!(out, "1:1:one\n7\nitem:one\n{\"p\":\"urn:p\"}:{\"p\":\"urn:p\"}\n");
}

/// Verifies SimpleXML iteration and recursive iterator against PHP's observable output.
#[test]
fn test_simplexml_iteration_and_recursive_iterator() {
    if skip_without_xml_native("test_simplexml_iteration_and_recursive_iterator") { return; }
    let out = compile_and_run(r###"<?php
$x = new SimpleXMLIterator('<r><a>one</a><b><c>two</c></b></r>');
foreach ($x as $key => $node) { echo $key, ':', (string)$node, ':', count($node), "\n"; }
$x->rewind(); echo $x->key(), ':', (int)$x->hasChildren(), "\n";
$x->next(); echo $x->key(), ':', (int)$x->hasChildren(), ':', $x->getChildren()->getName(), "\n";
$x->next(); var_dump($x->valid());
"###);
    assert_eq!(out, "a:one:0\nb::1\na:0\nb:1:b\nbool(false)\n");
}

/// Verifies SimpleXML add and serialize against PHP's observable output.
#[test]
fn test_simplexml_add_and_serialize() {
    if skip_without_xml_native("test_simplexml_add_and_serialize") { return; }
    let out = compile_and_run(r###"<?php
$x = new SimpleXMLElement('<root/>');
$child = $x->addChild('item', 'A &amp; B');
$child->addAttribute('id', 'x&y');
$x->addChild('p:tag', 'ok', 'urn:p');
echo $x->asXML();
echo $child->saveXML(), "\n";
"###);
    assert_eq!(out, "<?xml version=\"1.0\"?>\n<root><item id=\"x&amp;y\">A &amp; B</item><p:tag xmlns:p=\"urn:p\">ok</p:tag></root>\n<item id=\"x&amp;y\">A &amp; B</item>\n");
}

/// Verifies SimpleXML files and constructor url against PHP's observable output.
#[test]
fn test_simplexml_files_and_constructor_url() {
    if skip_without_xml_native("test_simplexml_files_and_constructor_url") { return; }
    let out = compile_and_run(r###"<?php
$file = __DIR__ . '/simplexml-test.xml';
file_put_contents($file, '<r><v>file</v></r>');
$x = simplexml_load_file($file);
if ($x instanceof SimpleXMLElement) { echo (string)$x->v, "\n"; }
$y = new SimpleXMLElement($file, 0, true);
$y->v = 'saved'; var_dump($y->asXML($file));
$z = simplexml_load_file($file);
if ($z instanceof SimpleXMLElement) { echo (string)$z->v, "\n"; }
unlink($file);
"###);
    assert_eq!(out, "file\nbool(true)\nsaved\n");
}

/// Verifies SimpleXML custom subclasses against PHP's observable output.
#[test]
fn test_simplexml_custom_subclasses() {
    if skip_without_xml_native("test_simplexml_custom_subclasses") { return; }
    let out = compile_and_run(r###"<?php
class CatalogElement extends SimpleXMLElement { public function label(): string { return $this->getName(); } }
$x = simplexml_load_string('<r><a>one</a></r>', CatalogElement::class);
if ($x instanceof CatalogElement) {
 echo get_class($x), ':', get_class($x->a), ':', $x->a->label(), "\n";
 echo get_class($x->addChild('b', 'two')), "\n";
}
"###);
    assert_eq!(out, "CatalogElement:CatalogElement:a\nCatalogElement\n");
}

/// Verifies SimpleXML parse errors and options against PHP's observable output.
#[test]
fn test_simplexml_parse_errors_and_options() {
    if skip_without_xml_native("test_simplexml_parse_errors_and_options") { return; }
    let out = compile_and_run(r###"<?php
var_dump(@simplexml_load_string('<r>'), @simplexml_load_string(''));
try { new SimpleXMLElement('<r>'); } catch (Exception $e) { echo $e->getMessage(), "\n"; }
$x = new SimpleXMLElement('<r>  <a>text</a>  </r>', LIBXML_NOBLANKS);
echo $x->asXML();
"###);
    assert_eq!(out, "bool(false)\nbool(false)\n\nWarning: SimpleXMLElement::__construct(): Entity: line 1: parser error : Premature end of data in tag r line 1 in Standard input code on line 3\n\nWarning: SimpleXMLElement::__construct(): <r> in Standard input code on line 3\n\nWarning: SimpleXMLElement::__construct():    ^ in Standard input code on line 3\nString could not be parsed as XML\n<?xml version=\"1.0\"?>\n<r><a>text</a></r>\n");
}

/// Verifies SimpleXML casts and empty selections against PHP's observable output.
#[test]
fn test_simplexml_casts_and_empty_selections() {
    if skip_without_xml_native("test_simplexml_casts_and_empty_selections") { return; }
    let out = compile_and_run(r###"<?php
$x = new SimpleXMLElement('<r><empty/><zero>0</zero></r>');
var_dump((bool)$x, (bool)$x->missing, (bool)$x->empty, (bool)$x->empty[0], (bool)$x->zero[0]);
echo '[', (string)$x->missing, ']:', count($x->missing), "\n";
"###);
    assert_eq!(out, "bool(true)\nbool(false)\nbool(true)\nbool(false)\nbool(true)\n[]:0\n");
}

/// Verifies SimpleXML eval uses native document against PHP's observable output.
#[test]
fn test_simplexml_eval_uses_native_document() {
    if skip_without_xml_native("test_simplexml_eval_uses_native_document") { return; }
    let out = compile_and_run(r###"<?php
$x = new SimpleXMLElement('<r><a>host</a></r>');
eval('$y = simplexml_load_string("<r><a>eval</a></r>"); echo (string)$y->a, "\n"; $x->a = "changed";');
echo (string)$x->a, "\n";
"###);
    assert_eq!(out, "eval\nchanged\n");
}

/// Verifies SimpleXML owners survive parent release against PHP's observable output.
#[test]
fn test_simplexml_owners_survive_parent_release() {
    if skip_without_xml_native("test_simplexml_owners_survive_parent_release") { return; }
    let out = compile_and_run(r###"<?php
$x = new SimpleXMLElement('<r><a>kept</a></r>');
$a = $x->a[0]; unset($x);
echo (string)$a, ':', $a->getName(), "\n";
$a['id'] = 'ok'; echo $a->asXML(), "\n";
"###);
    assert_eq!(out, "kept:a\n<a id=\"ok\">kept</a>\n");
}

/// Verifies SimpleXML dom import shares mutations using PHP's expected output.
#[test]
fn test_simplexml_dom_import_shares_mutations() {
    if skip_without_xml_native("test_simplexml_dom_import_shares_mutations") { return; }
    let out = compile_and_run(r###"<?php
$dom = new DOMDocument();
$dom->loadXML('<root id="one"><child>before</child></root>');
$root = $dom->documentElement;
$child = $dom->getElementsByTagName('child')->item(0);
$x = simplexml_import_dom($dom);
if ($x instanceof SimpleXMLElement) {
 $x->child = 'after'; $x['id'] = 'two';
 echo $child->nodeValue, ':', $root->getAttribute('id'), "\n";
 $child->nodeValue = 'from-dom'; $root->setAttribute('id', 'three');
 echo (string)$x->child, ':', (string)$x['id'], "\n";
 echo $dom->saveXML($root), "\n";
 $same = simplexml_import_dom($root);
 if ($same instanceof SimpleXMLElement) { echo (string)$same->child, "\n"; }
}
"###);
    assert_eq!(out, "after:two\nfrom-dom:three\n<root id=\"three\"><child>from-dom</child></root>\nfrom-dom\n");
}

/// Verifies SimpleXML clone and interfaces using PHP's expected output.
#[test]
fn test_simplexml_clone_and_interfaces() {
    if skip_without_xml_native("test_simplexml_clone_and_interfaces") { return; }
    let out = compile_and_run(r###"<?php
$x = new SimpleXMLElement('<root><a>one</a><a>two</a></root>');
$copy = clone $x;
$copy->a[0] = 'changed';
echo (string)$x->a[0], ':', (string)$copy->a[0], "\n";
var_dump($x instanceof ArrayAccess, $x instanceof RecursiveIterator, $x instanceof Countable, extension_loaded('SimpleXML'));
"###);
    assert_eq!(out, "one:changed\nbool(false)\nbool(true)\nbool(true)\nbool(true)\n");
}

/// Verifies SimpleXML entities and cdata options using PHP's expected output.
#[test]
fn test_simplexml_entities_and_cdata_options() {
    if skip_without_xml_native("test_simplexml_entities_and_cdata_options") { return; }
    let out = compile_and_run(r###"<?php
$x = new SimpleXMLElement('<!DOCTYPE r [<!ENTITY word "value">]><r><a><![CDATA[raw<text>]]>&word;</a></r>', LIBXML_NOCDATA | LIBXML_NOENT);
echo (string)$x->a, "\n";
echo (string)$x->a->asXML(), "\n";
"###);
    assert_eq!(out, "raw<text>value\n<a>raw&lt;text&gt;value</a>\n");
}

/// Numeric conversions parse the selected node text, including runtime-boxed offsets.
#[test]
fn test_simplexml_numeric_conversions() {
    if skip_without_xml_native("test_simplexml_numeric_conversions") { return; }
    let out = compile_and_run(r###"<?php
$x = new SimpleXMLElement('<r><number>42.5</number><zero>0</zero></r>');
echo (int)$x->number, ':', (float)$x->number, ':', (int)$x->number[0], ':', (int)$x->zero, "\n";
eval('echo (int)$x->number, ":", (float)$x->number, "\n";');
"###);
    assert_eq!(out, "42:42.5:42:0\n42:42.5\n");
}

/// XML declarations and legacy document encodings survive C-string bridge serialization.
#[test]
fn test_simplexml_legacy_encoding_serialization() {
    if skip_without_xml_native("test_simplexml_legacy_encoding_serialization") { return; }
    let out = compile_and_run(r###"<?php
$data = '<?xml version="1.0" encoding="ISO-8859-1"?><r>caf' . chr(233) . '</r>';
$x = new SimpleXMLElement($data);
echo bin2hex((string)$x), "\n", bin2hex((string)$x->asXML()), "\n";
"###);
    assert_eq!(out, "636166c3a9\n3c3f786d6c2076657273696f6e3d22312e302220656e636f64696e673d2249534f2d383835392d31223f3e0a3c723e636166e93c2f723e0a\n");
}

/// Attribute values count as scalar nodes and current() keeps a stable object until next().
#[test]
fn test_simplexml_attribute_counts_and_iterator_identity() {
    if skip_without_xml_native("test_simplexml_attribute_counts_and_iterator_identity") { return; }
    let out = compile_and_run(r###"<?php
$x = new SimpleXMLElement('<r a="" b="0"><c/><d>text</d></r>');
echo count($x['a']), ':', count($x->attributes()), ':', (int)(bool)$x['a'], "\n";
$x->rewind(); var_dump($x->current() === $x->current());
$x->getName(); echo $x->key(), "\n";
"###);
    assert_eq!(out, "0:2:1\nbool(true)\nc\n");
}

/// Native array and JSON projections expose XML data without adding PHP interfaces.
#[test]
fn test_simplexml_array_and_json_projections() {
    if skip_without_xml_native("test_simplexml_array_and_json_projections") { return; }
    let out = compile_and_run(r###"<?php
$x = new SimpleXMLElement('<r a="v"><item>one</item><item>two</item><nested><leaf>value</leaf></nested><empty/></r>');
echo json_encode($x), "\n";
echo json_encode((array)$x), "\n";
echo json_encode($x->item), "\n";
echo json_encode((array)$x->item), "\n";
echo json_encode($x->attributes()), "\n";
echo json_encode($x['a']), "\n";
var_dump($x instanceof JsonSerializable);
"###);
    assert_eq!(out, "{\"@attributes\":{\"a\":\"v\"},\"item\":[\"one\",\"two\"],\"nested\":{\"leaf\":\"value\"},\"empty\":{}}\n{\"@attributes\":{\"a\":\"v\"},\"item\":[\"one\",\"two\"],\"nested\":{\"leaf\":\"value\"},\"empty\":{}}\n{\"0\":\"one\",\"1\":\"two\"}\n[\"one\",\"two\"]\n{\"@attributes\":{\"a\":\"v\"}}\n{\"0\":\"v\"}\nbool(false)\n");
}
