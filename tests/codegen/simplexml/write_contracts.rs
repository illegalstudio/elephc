//! Purpose:
//! Verifies SimpleXML's PHP write-fetch, value validation, and attribute dimension contract.
//!
//! Called from:
//! - Codegen CI with managed libxml2 and PHP reference checks.
//!
//! Key details:
//! - Asserts both diagnostics and the resulting tree after rejected writes.

use crate::support::*;

/// PHP's add API preserves nonempty names without imposing the XMLWriter name validator.
#[test]
fn test_simplexml_add_nonempty_names_without_extra_validation() {
    if skip_without_xml_native("test_simplexml_add_nonempty_names_without_extra_validation") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r/>');
$child = $xml->addChild('1abc', 'v');
$xml->addAttribute('1abc', 'a');
echo $child->getName(), '|', $xml->asXML();
"###);
    assert_eq!(out, "1abc|<?xml version=\"1.0\"?>\n<r 1abc=\"a\"><1abc>v</1abc></r>\n");
}

/// Complex assignments throw before conversion or mutation, while exact SimpleXMLElement values work.
#[test]
fn test_simplexml_complex_assignment_preserves_tree() {
    if skip_without_xml_native("test_simplexml_complex_assignment_preserves_tree") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
class ComplexAssignedXml extends SimpleXMLElement {
    public function __toString(): string { echo 'wrong|'; return 'wrong'; }
}
$xml = new SimpleXMLElement('<r id="1"><item>old</item></r>');
$open = fopen('php://memory', 'r');
$closed = fopen('php://memory', 'r'); fclose($closed);
foreach ([['x'], new stdClass(), new ComplexAssignedXml('<v>wrong</v>'), $open, $closed] as $value) {
    try { $xml->item = $value; } catch (TypeError $error) { echo $error->getMessage(), '|'; }
    try { $xml['id'] = $value; } catch (TypeError $error) { echo $error->getMessage(), '|'; }
}
fclose($open);
echo (string)$xml->item, ':', (string)$xml['id'], '|';
$xml->item = new SimpleXMLElement('<v>accepted</v>');
echo (string)$xml->item;
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, concat!(
        "It's not possible to assign a complex type to properties, array given|",
        "It's not possible to assign a complex type to attributes, array given|",
        "It's not possible to assign a complex type to properties, stdClass given|",
        "It's not possible to assign a complex type to attributes, stdClass given|",
        "It's not possible to assign a complex type to properties, ComplexAssignedXml given|",
        "It's not possible to assign a complex type to attributes, ComplexAssignedXml given|",
        "It's not possible to assign a complex type to properties, resource given|",
        "It's not possible to assign a complex type to attributes, resource given|",
        "It's not possible to assign a complex type to properties, resource given|",
        "It's not possible to assign a complex type to attributes, resource given|old:1|accepted",
    ));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Assignment diagnostics identify their PHP caller and release backtrace storage after callbacks.
#[test]
fn test_simplexml_assignment_warning_caller() {
    if skip_without_xml_native("test_simplexml_assignment_warning_caller") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
function simpleXmlWarningCaller(SimpleXMLElement $xml): void { $xml->item[4] = 'v'; }
class SimpleXmlWarningCaller {
    public static function write(SimpleXMLElement $xml): void { $xml->item[4] = 'v'; }
}
class SimpleXmlUserWarningCaller extends SimpleXMLElement {
    public function __set(string $name, mixed $value): void { $this->item[4] = $value; }
}
$xml = new SimpleXMLElement('<r><item>a</item></r>');
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
simpleXmlWarningCaller($xml); SimpleXmlWarningCaller::write($xml);
$user = new SimpleXmlUserWarningCaller('<r/>'); $user->__set('ignored', 'v');
restore_error_handler();
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, concat!(
        "2:simpleXmlWarningCaller(): Cannot add element item number 4 when only 1 such elements exist|",
        "2:SimpleXmlWarningCaller::write(): Cannot add element item number 4 when only 2 such elements exist|",
        "2:SimpleXmlUserWarningCaller::__set(): Cannot add element item number 4 when only 0 such elements exist|",
    ));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Attribute appends throw, invalid indexes warn without mutation, and in-range writes remain valid.
#[test]
fn test_simplexml_attribute_dimension_write_errors() {
    if skip_without_xml_native("test_simplexml_attribute_dimension_write_errors") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r id="old"/>');
$attributes = $xml->attributes();
try { $attributes[] = 'wrong'; } catch (Error $error) { echo $error->getMessage(), '|'; }
try { $xml['id'][] = 'wrong'; } catch (Error $error) { echo $error->getMessage(), '|'; }
try { $xml['id'][0] = 'wrong'; } catch (Error $error) { echo $error->getMessage(), '|'; }
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
$attributes[5] = 'wrong'; $attributes[-1] = 'wrong';
restore_error_handler();
$attributes[0] = 'new';
echo $xml->asXML();
"###);
    assert_eq!(out, concat!(
        "Cannot append to an attribute list|Cannot create duplicate attribute|Cannot create duplicate attribute|",
        "2:main(): Cannot change attribute number 5 when only 1 attributes exist|",
        "2:main(): Cannot change attribute number -1 when only 0 attributes exist|",
        "<?xml version=\"1.0\"?>\n<r id=\"new\"/>\n",
    ));
}

/// Named offsets on non-element nodes are silent no-ops through direct, chained and eval writes.
#[test]
fn test_simplexml_non_element_named_dimension_bundle() {
    if skip_without_xml_native("test_simplexml_non_element_named_dimension_bundle") { return; }
    let source = r###"<?php
$doc = new SimpleXMLElement('<r><mark a="3">7<!--kept--><?p kept?></mark></r>');
$a = $doc->mark['a'];
$a['x'] = 'wrong'; echo $doc->asXML();
$a['k'] .= 'wrong'; echo $doc->asXML();
$doc->mark['a']['nested'] = 'wrong'; echo $doc->asXML();
$doc->mark['a']['compound'] .= 'wrong'; echo $doc->asXML();
$named = $doc->mark->attributes()->a;
$named['q'] = 'wrong'; echo $doc->asXML();
$nodes = $doc->xpath('//comment()'); $comment = $nodes[0];
$comment['x'] = 'wrong'; $comment['k'] .= 'wrong'; echo $doc->asXML();
$nodes = $doc->xpath('//processing-instruction()'); $pi = $nodes[0];
$pi['x'] = 'wrong'; $pi['k'] .= 'wrong'; echo $doc->asXML();
echo isset($a['k']) ? 'bad|' : 'absent|';
$nodes = $doc->xpath('//mark/text()'); $text = $nodes[0]; $text['x'] = 'y';
$doc->mark->attributes()['z'] = 'w';
$a->addAttribute('n', 'v'); echo $doc->asXML();
unset($doc, $a, $named, $nodes, $comment, $pi, $text);
"###;
    let body = source.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let expected = format!("{}absent|<?xml version=\"1.0\"?>\n<r><mark a=\"3\" x=\"y\" z=\"w\" n=\"v\">7<!--kept--><?p kept?></mark></r>\n",
        "<?xml version=\"1.0\"?>\n<r><mark a=\"3\">7<!--kept--><?p kept?></mark></r>\n".repeat(7));
    assert_bundle_with(&[
        BundleCase { name: "non_element_dimension_aot", source, expected: &expected },
        BundleCase { name: "non_element_dimension_eval", source: &eval, expected: &expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        assert!(!out.stderr.contains("Warning:") && !out.stderr.contains("Notice:"), "{}", out.stderr);
        out.stdout
    });
}
