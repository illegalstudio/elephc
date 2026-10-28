//! Purpose:
//! PHP-oracle SimpleXML end-to-end coverage for the native document bridge.
//!
//! Called from:
//! - The codegen integration suite on Linux and macOS CI.
//!
//! Key details:
//! - Each fixture requires the managed libxml2 artifact, with explicit skip gating.

use crate::support::*;

mod review_regressions;
mod review_type_boundaries;
mod eval_bundles;
mod write_contracts;
mod typed_empty;
mod opaque_writes;
mod dimension_writes;
mod dimension_owners;
mod diagnostics;
mod dom_and_iterator;
mod dom_activation;
mod comparisons_and_files;
mod stream_urls;
mod projections;
mod namespace_queries;
mod debug;
mod debug_owners;

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

/// Native iteration and projections retain the namespace selected by constructors or children().
#[test]
fn test_simplexml_namespace_projection_and_indexed_iteration() {
    if skip_without_xml_native("test_simplexml_namespace_projection_and_indexed_iteration") { return; }
    let out = compile_and_run(r###"<?php
$source = '<r xmlns:p="urn:p" id="plain" p:id="ns"><p:a id="plain-a" p:id="ns-a"><p:b>value</p:b><b>plain</b></p:a><a>other</a></r>';
$xml = new SimpleXMLElement($source);
$namespaced = $xml->children('urn:p');
echo json_encode($namespaced), "\n", json_encode((array)$namespaced), "\n";
echo json_encode($namespaced->a), "\n";
foreach ($namespaced->a[0] as $child) { echo (string)$child, '|'; }
foreach ($namespaced->a->children() as $child) { echo (string)$child, '|'; }
echo "\n";
$filtered = new SimpleXMLElement($source, 0, false, 'urn:p');
echo json_encode($filtered), "\n";
echo (string)$namespaced->a['id'], ':', (string)$namespaced->a->attributes()['id'], "\n";
"###);
    assert_eq!(out, "{\"a\":{\"@attributes\":{\"id\":\"ns-a\"},\"b\":\"value\"}}\n{\"a\":{\"@attributes\":{\"id\":\"ns-a\"},\"b\":\"value\"}}\n{\"@attributes\":{\"id\":\"ns-a\"},\"b\":\"value\"}\nvalue|plain|\n{\"@attributes\":{\"id\":\"ns\"},\"a\":{\"@attributes\":{\"id\":\"ns-a\"},\"b\":\"value\"}}\nns-a:plain-a\n");
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
$directory = __DIR__ . '/simplexml-base';
mkdir($directory);
file_put_contents($directory . '/entity.dtd', '<!ENTITY word "relative">');
file_put_contents($directory . '/data.xml', '<!DOCTYPE r SYSTEM "entity.dtd"><r>&word;</r>');
$loaded = simplexml_load_file($directory . '/data.xml', options: LIBXML_DTDLOAD | LIBXML_NOENT);
if ($loaded instanceof SimpleXMLElement) { echo (string)$loaded, "\n"; }
$constructed = new SimpleXMLElement($directory . '/data.xml', LIBXML_DTDLOAD | LIBXML_NOENT, true);
echo (string)$constructed, "\n";
unlink($directory . '/entity.dtd'); unlink($directory . '/data.xml'); rmdir($directory);
"###);
    assert_eq!(out, "file\nbool(true)\nsaved\nrelative\nrelative\n");
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

/// XML property views take precedence over subclass slots while ordinary objects keep their storage.
#[test]
fn test_simplexml_mixed_property_reads() {
    if skip_without_xml_native("test_simplexml_mixed_property_reads") { return; }
    let out = compile_and_run(r###"<?php
class DeclaredPropertyXml extends SimpleXMLElement { public string $child = 'declared'; }
function readSimpleXmlChild(mixed $node): void { echo (string)$node->child, '|'; }
readSimpleXmlChild(new SimpleXMLElement('<r><child>native</child></r>'));
readSimpleXmlChild(new DeclaredPropertyXml('<r><child>hidden</child></r>'));
$ordinary = new stdClass(); $ordinary->child = 'ordinary';
readSimpleXmlChild($ordinary);
"###);
    assert_eq!(out, "native|hidden|ordinary|");
}

/// Nullable ordinary objects retain their own property protocol when XML is enabled.
#[test]
fn test_simplexml_nullable_ordinary_property_dispatch() {
    if skip_without_xml_native("test_simplexml_nullable_ordinary_property_dispatch") { return; }
    let out = compile_and_run(r###"<?php
class NullableOrdinaryXmlPeer {
    public string $value = 'plain';
    public string $hooked = 'seed' {
        get { echo 'get:'; return $this->hooked; }
        set { echo 'set:'; $this->hooked = $value; }
    }
}
function nullableOrdinaryXmlPeer(int $n): ?NullableOrdinaryXmlPeer {
    return $n > 0 ? new NullableOrdinaryXmlPeer() : null;
}
function unsetHookedXmlPeer(mixed $peer): void {
    try { unset($peer->hooked); }
    catch (Error $error) { echo 'blocked:'; }
}
$xml = new SimpleXMLElement('<r/>');
$peer = nullableOrdinaryXmlPeer($argc);
echo $peer->value, ':';
$peer->value = 'changed';
echo $peer->value, ':';
$peer->hooked = 'next';
echo $peer->hooked, ':';
unsetHookedXmlPeer($peer);
var_dump(isset($peer->value));
echo $peer->value ?? 'missing', ':';
unset($peer->value);
var_dump(isset($peer->value));
"###);
    assert_eq!(out, "plain:changed:set:get:next:blocked:bool(true)\nchanged:bool(false)\n");
}

/// Native XML properties bypass user slots and hooks in typed, boxed, and eval operations.
#[test]
fn test_simplexml_native_properties_ignore_user_hooks() {
    if skip_without_xml_native("test_simplexml_native_properties_ignore_user_hooks") { return; }
    let out = compile_and_run(r###"<?php
class HookedPropertyXml extends SimpleXMLElement {
    public string $a = 'slot';
    public function __get(string $name): mixed { echo 'get-hook|'; return 'wrong'; }
    public function __set(string $name, mixed $value): void { echo 'set-hook|'; }
    public function __isset(string $name): bool { echo 'isset-hook|'; return false; }
    public function __unset(string $name): void { echo 'unset-hook|'; }
}
$xml = new HookedPropertyXml('<r><a>native</a></r>');
echo (string)$xml->a, '|';
$xml->a = 'typed'; echo (string)$xml->a, '|';
var_dump(isset($xml->a), isset($xml->missing));
echo $xml->missing ?? 'fallback', '|';
function mutateNativePropertyMixed(mixed $node): void {
    $node->a = 'mixed'; echo (string)$node->a, '|';
    var_dump(isset($node->a), isset($node->missing));
    echo $node->missing ?? 'boxed-fallback', '|';
    unset($node->a); var_dump(isset($node->a));
}
mutateNativePropertyMixed($xml);
eval('$xml->a = "eval"; echo (string)$xml->a, "|"; var_dump(isset($xml->a)); unset($xml->a); var_dump(isset($xml->a));');
"###);
    assert_eq!(out, "native|typed|bool(true)\nbool(false)\nfallback|mixed|bool(true)\nbool(false)\nboxed-fallback|bool(false)\neval|bool(true)\nbool(false)\n");
}

/// Verifies SimpleXML parse errors and options against PHP's observable output.
#[test]
fn test_simplexml_parse_errors_and_options() {
    if skip_without_xml_native("test_simplexml_parse_errors_and_options") { return; }
    let out = compile_and_run(r###"<?php
var_dump(@simplexml_load_string('<r>'), @simplexml_load_string(''));
try { @(new SimpleXMLElement('<r>')); } catch (Exception $e) { echo $e->getMessage(), "\n"; }
$x = new SimpleXMLElement('<r>  <a>text</a>  </r>', LIBXML_NOBLANKS);
echo $x->asXML();
"###);
    assert_eq!(out, "bool(false)\nbool(false)\nString could not be parsed as XML\n<?xml version=\"1.0\"?>\n<r><a>text</a></r>\n");
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
 if ($root instanceof DOMNode) {
  $same = simplexml_import_dom($root);
  if ($same instanceof SimpleXMLElement) { echo (string)$same->child, "\n"; }
 }
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
$iterator = new SimpleXMLIterator('<r><a/><b/></r>');
$iterator->rewind(); $iterator->next();
$clonedIterator = clone $iterator;
var_dump($clonedIterator->valid());
class CloneHookXml extends SimpleXMLElement { public function __clone(): void { $this->v = 'copy'; } }
function cloneXmlMixed(mixed $value): mixed { return clone $value; }
$original = new CloneHookXml('<r><v>original</v></r>');
$customCopy = cloneXmlMixed($original);
echo (string)$original->v, ':', (string)$customCopy->v, "\n";
$customCopy->v = 'changed';
echo (string)$original->v, ':', (string)$customCopy->v, "\n";
"###);
    assert_eq!(out, "one:changed\nbool(false)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\noriginal:original\noriginal:changed\n");
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

/// The public debug projection matches PHP for element and attribute wrappers.
#[test]
fn test_simplexml_debug_info_projection() {
    if skip_without_xml_native("test_simplexml_debug_info_projection") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r id="x"><a>v</a></r>');
function debugXmlMixed(mixed $node): mixed { return $node->__debugInfo(); }
echo json_encode($xml->__debugInfo()), "\n";
echo json_encode($xml['id']->__debugInfo()), "\n";
echo json_encode(debugXmlMixed($xml)), "\n";
"###);
    assert_eq!(out, "{\"@attributes\":{\"id\":\"x\"},\"a\":\"v\"}\n[\"x\"]\n{\"@attributes\":{\"id\":\"x\"},\"a\":\"v\"}\n");
}

/// Repeated projections and child aliases release compiler-owned values under heap debugging.
#[test]
fn test_simplexml_projection_and_alias_heap_debug() {
    if skip_without_xml_native("test_simplexml_projection_and_alias_heap_debug") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
for ($i = 0; $i < 2; $i++) {
    $x = new SimpleXMLElement('<r id="x"><child>kept</child></r>');
    $child = $x->child[0];
    $json = json_encode($x);
    $properties = (array)$x;
    unset($x);
    echo (string)$child, "|";
    unset($child, $json, $properties);
}
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "kept|kept|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Compiles one isolated ownership stage so each CI test has its own timeout and heap counters.
fn assert_simplexml_heap_stage(label: &str, action: &str) {
    if skip_without_xml_native(label) { return; }
    let source = format!("<?php $x = new SimpleXMLElement('<r id=\"x\"><child>kept</child></r>'); {action} unset($x); echo 'done';");
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "{label}: stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "done", "{label}: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{label}: {}", out.stderr);
}

/// A native root and its lease release their heap storage independently.
#[test]
fn test_simplexml_heap_root() { assert_simplexml_heap_stage("root", ""); }

/// Child aliases retire both the wrapper and its independent native lease.
#[test]
fn test_simplexml_heap_alias() {
    assert_simplexml_heap_stage("alias", "$child = $x->child[0]; unset($child);");
}

/// XML writes do not restamp the storage of an ignored PHP subclass property.
#[test]
fn test_simplexml_heap_declared_slot() {
    assert_simplexml_heap_stage("declared", "class HeapStorageXml extends SimpleXMLElement { public $a = []; } $x = new HeapStorageXml('<r/>'); $x->a = 'text';");
}

/// The array projection releases its temporary wrappers, keys, and result hash.
#[test]
fn test_simplexml_heap_array_projection() {
    assert_simplexml_heap_stage("array", "$properties = (array)$x; unset($properties);");
}

/// The JSON projection releases its temporary wrappers, keys, and encoded string.
#[test]
fn test_simplexml_heap_json_projection() {
    assert_simplexml_heap_stage("json", "$json = json_encode($x); unset($json);");
}

/// Nullable self returns are covariant while a class's interface schema is still being built.
#[test]
fn test_simplexml_nullable_self_interface_covariance() {
    let out = compile_and_run(r###"<?php
interface NullableNextNode { public function nextNode(): ?NullableNextNode; }
class NullableSelfNode implements NullableNextNode {
    public function nextNode(): ?NullableSelfNode { return null; }
}
$node = new NullableSelfNode(); var_dump($node->nextNode());
"###);
    assert_eq!(out, "NULL\n");
}

/// Invalid result classes fail before parsing or invoking an unrelated class constructor.
#[test]
fn test_simplexml_invalid_result_classes() {
    if skip_without_xml_native("test_simplexml_invalid_result_classes") { return; }
    let out = compile_and_run(r###"<?php
foreach (['stdClass', 'MissingSimpleXmlClass', ''] as $class) {
    try { simplexml_load_string('<r/>', $class); }
    catch (TypeError $error) { echo "type-error|"; }
}
"###);
    assert_eq!(out, "type-error|type-error|type-error|");
}

/// Numeric aliases remain bound to the same native node when siblings are removed.
#[test]
fn test_simplexml_indexed_alias_after_unset() {
    if skip_without_xml_native("test_simplexml_indexed_alias_after_unset") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r><i>a</i><i>b</i></r>');
$first = $xml->i[0];
unset($xml->i[-1]);
echo count($xml->i), ':';
unset($xml->i[0]);
echo (string)$first, ':', (string)$xml->i[0], "\n";
unset($first[0]); unset($first[0]);
$missing = $xml->absent[0];
function missingXmlKey(): int { echo 'key'; return 0; }
unset($missing[missingXmlKey()]);
"###);
    assert_eq!(out, "2:a:b\nkey");
}

/// Removed element and attribute aliases retain their data and clone into independent storage.
#[test]
fn test_simplexml_clone_removed_aliases() {
    if skip_without_xml_native("test_simplexml_clone_removed_aliases") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r id="old"><a>one</a></r>');
$element = $xml->a[0]; $attribute = $xml['id'];
unset($xml->a, $xml['id']);
$copy = clone $element; $copy[0] = 'two';
echo (string)$element, ':', (string)$copy, "\n";
$attributeCopy = clone $attribute;
echo (string)$attribute, ':', (string)$attributeCopy, "\n";
"###);
    assert_eq!(out, "one:two\nold:old\n");
}

/// Indirect child writes use the XML getter and native dimensions on inherited classes.
#[test]
fn test_simplexml_child_dimensions_and_append() {
    if skip_without_xml_native("test_simplexml_child_dimensions_and_append") { return; }
    let out = compile_and_run(r###"<?php
class DimensionXml extends SimpleXMLIterator {}
$xml = new DimensionXml('<r><item>a</item></r>');
$xml->item['id'] = 'v';
$xml->item[0] = 'changed';
$xml->item[] = 'added';
echo (string)$xml->item['id'], ':', count($xml->item), ':', (string)$xml->item[0], ':', (string)$xml->item[1], "\n";
class DimensionHolder { public SimpleXMLElement $xml; public function __construct(SimpleXMLElement $xml) { $this->xml = $xml; } }
$holder = new DimensionHolder($xml->item);
$holder->xml[] = 'third';
echo count($xml->item), ':', (string)$xml->item[2], "\n";
"###);
    assert_eq!(out, "v:2:changed:added\n3:third\n");
}

/// A dynamic property expression can replace the variable before PHP fetches its receiver.
#[test]
fn test_simplexml_dynamic_property_receiver_evaluation_order() {
    if skip_without_xml_native("test_simplexml_dynamic_property_receiver_evaluation_order") { return; }
    let out = compile_and_run(r###"<?php
$receiver = new SimpleXMLElement('<r><a>old</a></r>');
function alterXmlReceiver(): string {
    global $receiver;
    $receiver = new SimpleXMLElement('<r><a>new</a></r>');
    return 'a';
}
echo (string)$receiver->{alterXmlReceiver()}, ':', (string)$receiver->a;
"###);
    assert_eq!(out, "new:new");
}

/// Native document leases survive overrides of the PHP destructor and release independently.
#[test]
fn test_simplexml_subclass_destructor_owns_native_lease() {
    if skip_without_xml_native("test_simplexml_subclass_destructor_owns_native_lease") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
class DestructXml extends SimpleXMLElement { public function __destruct() { echo "drop|"; } }
$root = new DestructXml('<r><child>kept</child></r>');
$child = $root->child;
unset($root);
echo (string)$child, "|";
unset($child);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "drop|kept|drop|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
