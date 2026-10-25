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
