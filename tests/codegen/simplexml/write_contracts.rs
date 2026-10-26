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
foreach ([['x'], new stdClass(), new ComplexAssignedXml('<v>wrong</v>')] as $value) {
    try { $xml->item = $value; } catch (TypeError $error) { echo $error->getMessage(), '|'; }
    try { $xml['id'] = $value; } catch (TypeError $error) { echo $error->getMessage(), '|'; }
}
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
        "It's not possible to assign a complex type to attributes, ComplexAssignedXml given|old:1|accepted",
    ));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Write-fetch creates missing nodes after keys and values run, while a saved empty read stays lazy.
#[test]
fn test_simplexml_missing_child_write_fetch() {
    if skip_without_xml_native("test_simplexml_missing_child_write_fetch") { return; }
    let out = compile_and_run(r###"<?php
$xml = new SimpleXMLElement('<r/>');
function missingXmlWriteIndex(): int { global $xml; echo (int)isset($xml->positive), '|'; return 1; }
function missingXmlWriteValue(): string { global $xml; echo (int)isset($xml->positive), '|'; return 'ignored'; }
function missingEvalXmlWriteIndex(): int { global $xml; echo (int)isset($xml->evaluated), '|'; return 1; }
function missingEvalXmlWriteValue(): string { global $xml; echo (int)isset($xml->evaluated), '|'; return 'ignored'; }
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
$xml->positive[missingXmlWriteIndex()] = missingXmlWriteValue();
$xml->negative[-1] = 'ignored';
$xml->zero[0] = 'c';
$xml->appended[] = 'd';
$selection = $xml->absent;
$selection[0] = 'ignored'; $selection[1] = 'ignored'; $selection[] = 'ignored';
restore_error_handler();
echo $xml->asXML();
$code = $argc > 0 ? '$xml->evaluated[missingEvalXmlWriteIndex()] = missingEvalXmlWriteValue();' : '';
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
eval($code);
restore_error_handler();
echo count($xml->evaluated), ':', (string)$xml->evaluated;
"###);
    assert_eq!(out, concat!(
        "0|0|2:main(): Cannot add element positive number 1 when only 0 such elements exist|",
        "2:main(): Cannot add element negative number -1 when only 0 such elements exist|",
        "<?xml version=\"1.0\"?>\n<r><positive/><negative/><zero>c</zero><appended>d</appended></r>\n",
        "0|0|2:Unknown: Cannot add element evaluated number 1 when only 0 such elements exist|1:",
    ));
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

/// Runtime-eval writes report the active PHP callable instead of a suspended native frame.
#[test]
fn test_simplexml_eval_assignment_warning_callers() {
    if skip_without_xml_native("test_simplexml_eval_assignment_warning_callers") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
set_error_handler(function(int $level, string $message): bool { echo $message, '|'; return true; });
$source = $argc > 0 ? '
function evaluatedXmlWarning(SimpleXMLElement $xml): void { $xml->item[4] = "f"; }
class EvaluatedXmlWarning {
    public function instanceWrite(SimpleXMLElement $xml): void { $xml->item[4] = "i"; }
    public static function staticWrite(SimpleXMLElement $xml): void { $xml->item[4] = "s"; }
}
$xml = new SimpleXMLElement("<r><item>a</item><item>b</item></r>");
$xml->item = "duplicate";
$xml->item[4] = "top";
evaluatedXmlWarning(new SimpleXMLElement("<r><item>a</item></r>"));
(new EvaluatedXmlWarning())->instanceWrite(new SimpleXMLElement("<r><item>a</item></r>"));
EvaluatedXmlWarning::staticWrite(new SimpleXMLElement("<r><item>a</item></r>"));
' : '';
eval($source);
restore_error_handler();
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, concat!(
        "Unknown: Cannot assign to an array of nodes (duplicate subnodes or attr detected)|",
        "Unknown: Cannot add element item number 4 when only 2 such elements exist|",
        "evaluatedXmlWarning(): Cannot add element item number 4 when only 1 such elements exist|",
        "EvaluatedXmlWarning::instanceWrite(): Cannot add element item number 4 when only 1 such elements exist|",
        "EvaluatedXmlWarning::staticWrite(): Cannot add element item number 4 when only 1 such elements exist|",
    ));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
