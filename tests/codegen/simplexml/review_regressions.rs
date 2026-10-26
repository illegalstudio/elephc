//! Purpose:
//! Covers PHP compatibility and operand ownership found in the SimpleXML review.
//!
//! Called from:
//! - Codegen CI with managed libxml2 and the PHP reference interpreter.
//!
//! Key details:
//! - Distinguishes container empty tests, boolean casts, and XPath wrapper kinds.

use crate::support::*;

/// XPath attributes are filtered lists, whereas offset attributes remain stable scalar nodes.
#[test]
fn test_simplexml_xpath_attribute_views() {
    if skip_without_xml_native("test_simplexml_xpath_attribute_views") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r xmlns:p="urn:p" id="7" p:id="8"/>');
foreach ($xml->xpath('//@*') as $attribute) {
    echo $attribute->getName(), ':', (string)$attribute, ':', count($attribute), ':', json_encode($attribute), '|';
    foreach ($attribute as $key => $value) { echo $key, '=', (string)$value, '|'; }
}
$offset = $xml['id'];
echo count($offset), ':', json_encode($offset), '|';
"###);
    assert_eq!(out, "id:7:1:{\"@attributes\":{\"id\":\"7\"}}|id=7|id:8:1:{\"@attributes\":{\"id\":\"8\"}}|id=8|0:{\"0\":\"7\"}|");
}

/// XPath keeps comments and processing instructions with their own content and serialization.
#[test]
fn test_simplexml_xpath_comment_and_processing_instruction() {
    if skip_without_xml_native("test_simplexml_xpath_comment_and_processing_instruction") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r><!--note--><?work data?></r>');
foreach ($xml->xpath('comment() | processing-instruction()') as $node) {
    echo $node->getName(), ':', (string)$node, ':', $node->asXML(), '|';
}
"###);
    assert_eq!(out, "comment:note:<!--note-->|work:data:<?work data?>|");
}

/// Scalar XPath results warn with E_WARNING before returning false, including handler throws.
#[test]
fn test_simplexml_xpath_scalar_warning() {
    if skip_without_xml_native("test_simplexml_xpath_scalar_warning") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r><a/></r>');
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
var_dump($xml->xpath('count(//a)'), $xml->xpath('true()'), $xml->xpath('string(//a)'));
restore_error_handler();
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('caught'); });
try { $xml->xpath('count(//a)'); } catch (RuntimeException $exception) { echo $exception->getMessage(); }
restore_error_handler();
"###);
    assert_eq!(out, "2:SimpleXMLElement::xpath(): XPath expression must return a node set, number returned|2:SimpleXMLElement::xpath(): XPath expression must return a node set, bool returned|2:SimpleXMLElement::xpath(): XPath expression must return a node set, string returned|bool(false)\nbool(false)\nbool(false)\ncaught");
}

/// Native empty tests bypass subclass hooks, preserve ordinary slots, and differ from bool casts.
#[test]
fn test_simplexml_empty_native_container_content() {
    if skip_without_xml_native("test_simplexml_empty_native_container_content") { return; }
    let out = compile_and_run(r###"<?php
class EmptyNativeXml extends SimpleXMLElement {
    public string $zero = 'declared';
    public function __isset(string $name): bool { echo 'wrong'; return false; }
    public function offsetExists(mixed $key): bool { echo 'wrong'; return false; }
}
$xml = new EmptyNativeXml('<r z="0" n="00"><blank/><zero>0</zero><number>00</number><nested><a/></nested><cdata><![CDATA[0]]></cdata></r>');
echo (int)empty($xml->blank), (int)empty($xml->zero), (int)empty($xml->number), (int)empty($xml->nested), (int)empty($xml->cdata), (int)empty($xml->absent), '|';
echo (int)empty($xml['z']), (int)empty($xml['n']), (int)empty($xml['absent']), (int)empty($xml->zero[0]), '|';
$selected = $xml->zero; $attribute = $xml['z'];
echo (int)empty($selected), (int)empty($attribute), (int)(bool)$selected, (int)(bool)$attribute, '|';
function emptyNativeXmlBoxed(mixed $node, string $key): void {
    echo (int)empty($node->{$key}), (int)empty($node?->{$key}), '|';
}
emptyNativeXmlBoxed($xml, 'zero');
$ordinary = new stdClass(); $ordinary->zero = $selected;
emptyNativeXmlBoxed($ordinary, 'zero');
function emptyNativeXmlDimension(mixed $node): void { echo (int)empty($node['z']); }
emptyNativeXmlDimension($xml);
emptyNativeXmlDimension(['z' => '0']);
"###);
    assert_eq!(out, "110001|1011|0011|11|00|11");
}

/// Dynamic names run once, variable receivers load after the name, and nullsafe names stay lazy.
#[test]
fn test_simplexml_empty_operand_order_and_eval() {
    if skip_without_xml_native("test_simplexml_empty_operand_order_and_eval") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
$xml = new SimpleXMLElement('<r><zero>1</zero></r>');
function replaceEmptyXmlReceiver(): string {
    global $xml;
    echo 'key|';
    $xml = new SimpleXMLElement('<r z="0"><zero>0</zero></r>');
    return 'zero';
}
echo (int)empty($xml->{replaceEmptyXmlReceiver()}), '|';
function emptyXmlNullsafe(?SimpleXMLElement $node): void {
    echo (int)empty($node?->{replaceEmptyXmlReceiver()}), '|';
}
emptyXmlNullsafe(null);
$code = $argc > 0 ? 'echo (int)empty($xml->zero), (int)empty($xml["z"]), "|"; $selection = $xml->zero; echo (int)empty($selection);' : '';
eval($code);
unset($selection, $xml);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "key|1|1|11|0");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Upstream nullsafe exits skip terminal keys, while a real later null still evaluates them.
#[test]
fn test_simplexml_empty_long_nullsafe_chains() {
    if skip_without_xml_native("test_simplexml_empty_long_nullsafe_chains") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
$xml = new SimpleXMLElement('<r><child z="0"><item>0</item></child></r>');
function emptyChainName(): string { echo 'name|'; return 'item'; }
function emptyChainIndex(): string { echo 'index|'; return 'z'; }
function emptyChains(mixed $node): void {
    echo (int)empty($node?->child->{emptyChainName()}), '|';
    echo (int)empty($node?->child[emptyChainIndex()]), '|';
}
emptyChains(null);
emptyChains($xml);
$ordinary = new stdClass(); $ordinary->child = null;
emptyChains($ordinary);
$code = $argc > 0 ? '$null = null; echo (int)empty($null?->child->{emptyChainName()}), "|", (int)empty($null?->child[emptyChainIndex()]), "|"; echo (int)empty($xml?->child->{emptyChainName()}), "|", (int)empty($xml?->child[emptyChainIndex()]), "|"; echo (int)empty($ordinary?->child->{emptyChainName()}), "|", (int)empty($ordinary?->child[emptyChainIndex()]), "|";' : '';
eval($code);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "1|1|name|1|index|1|name|1|index|1|1|1|name|1|index|1|name|1|index|1|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Attribute views add to their containing element and existing namespace URIs keep their prefix.
#[test]
fn test_simplexml_add_attribute_parent_and_namespace_reuse() {
    if skip_without_xml_native("test_simplexml_add_attribute_parent_and_namespace_reuse") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r xmlns:p="urn:p" id="1"/>');
$xml['id']->addAttribute('offset', '2');
$xml->attributes()->addAttribute('list', '3');
$xml->addChild('q:child', 'v', 'urn:p');
$xml->addAttribute('q:a', '4', 'urn:p');
echo $xml->asXML();
"###);
    assert_eq!(out, "<?xml version=\"1.0\"?>\n<r xmlns:p=\"urn:p\" id=\"1\" offset=\"2\" list=\"3\" p:a=\"4\"><p:child>v</p:child></r>\n");
}

/// Write and add failures use E_WARNING, preserve appending, and validate names with ValueError.
#[test]
fn test_simplexml_mutation_warning_contract() {
    if skip_without_xml_native("test_simplexml_mutation_warning_contract") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r id="1"><item>a</item><item>b</item></r>');
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
$xml->item = 'wrong';
$xml->attributes()->addChild('blocked');
$xml->missing->addChild('blocked');
$xml->missing->addAttribute('blocked', 'v');
$xml->addAttribute('bare', 'v', 'urn:x');
$xml->addAttribute('id', '2');
$xml->item[5] = 'c';
echo count($xml->item), ':', (string)$xml->item[2], '|';
restore_error_handler();
try { $xml->addChild(''); } catch (ValueError $error) { echo $error->getMessage(), '|'; }
try { $xml->addAttribute('', 'v'); } catch (ValueError $error) { echo $error->getMessage(), '|'; }
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('stopped'); });
try { $xml->item[9] = 'not-added'; } catch (RuntimeException $error) { echo $error->getMessage(), ':', count($xml->item); }
restore_error_handler();
"###);
    assert_eq!(out, concat!(
        "2:main(): Cannot assign to an array of nodes (duplicate subnodes or attr detected)|",
        "2:SimpleXMLElement::addChild(): Cannot add element to attributes|",
        "2:SimpleXMLElement::addChild(): Cannot add child. Parent is not a permanent member of the XML tree|",
        "2:SimpleXMLElement::addAttribute(): Unable to locate parent Element|",
        "2:SimpleXMLElement::addAttribute(): Attribute requires prefix for namespace|",
        "2:SimpleXMLElement::addAttribute(): Attribute already exists|",
        "2:main(): Cannot add element item number 5 when only 2 such elements exist|3:c|",
        "SimpleXMLElement::addChild(): Argument #1 ($qualifiedName) must not be empty|",
        "SimpleXMLElement::addAttribute(): Argument #1 ($qualifiedName) must not be empty|stopped:3",
    ));
}

/// Eval empty releases fresh dimension keys, retains borrowed keys, and cleans a throwing cast.
#[test]
fn test_simplexml_eval_empty_dimension_key_owners() {
    if skip_without_xml_native("test_simplexml_eval_empty_dimension_key_owners") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
$xml = new SimpleXMLElement('<r z="0"/>');
$source = $argc > 0 ? '
class ThrowingXmlEmptyKey {
    public function __toString(): string { throw new RuntimeException("cast"); }
    public function __destruct() { echo "drop|"; }
}
function freshXmlEmptyKey(): mixed { return new ThrowingXmlEmptyKey(); }
$key = "z";
echo "probes|";
for ($i = 0; $i < 32; $i++) {
    if ($i === 0) { echo "empty-loop|"; }
    if (!empty($xml["missing" . str_repeat("x", 40)]) || !empty($xml[$key])) { echo "wrong|"; }
    if ($i === 0) { echo "isset-loop|"; }
    if (isset($xml["missing" . str_repeat("x", 40)]) || !isset($xml[$key])) { echo "wrong|"; }
    if ($i === 0) { echo "fresh-loop|"; }
    if (isset((new SimpleXMLElement("<r/>"))["missing"]) || !empty((new SimpleXMLElement("<r/>"))["missing"])) { echo "wrong|"; }
    if ($i === 0) { echo "chain-loop|"; }
    if (!empty($xml?->missing["key" . str_repeat("x", 40)])) { echo "wrong|"; }
}
echo "empty|";
try { empty($xml[freshXmlEmptyKey()]); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; }
echo "isset|";
try { isset($xml[freshXmlEmptyKey()]); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; }
echo $key;
' : '';
eval($source);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "probes|empty-loop|isset-loop|fresh-loop|chain-loop|empty|drop|cast|isset|drop|cast|z");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Ordinary eval ArrayAccess empty keeps a fresh key through both offsetExists and offsetGet.
#[test]
fn test_eval_arrayaccess_probe_key_owners() {
    let out = compile_and_run_with_heap_debug(r###"<?php
class ProbeIndexLeaseArray implements ArrayAccess {
    public function offsetExists(mixed $offset): bool { return strlen((string)$offset) > 0; }
    public function offsetGet(mixed $offset): mixed { return substr((string)$offset, 0, 1); }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void {}
}
$object = new ProbeIndexLeaseArray();
$source = $argc > 0 ? '
$checks = 0;
for ($i = 0; $i < 32; $i++) {
    $checks += (int)isset($object["key" . str_repeat("x", 40)]);
    $checks += (int)empty($object["0key" . str_repeat("x", 40)]);
}
echo $checks;
' : '';
eval($source);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "64");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
