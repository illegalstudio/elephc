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
