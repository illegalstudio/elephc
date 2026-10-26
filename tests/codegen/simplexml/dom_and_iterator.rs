//! Purpose:
//! Covers SimpleXML DOM import failures, namespace contexts, and iterator diagnostics.
//!
//! Called from:
//! - Focused codegen CI with managed libxml2 and optional PHP reference checks.
//!
//! Key details:
//! - Bundles compiled and runtime-eval cases with heap-debug ownership checks.

use crate::support::*;

/// Dynamic method syntax and callback arrays inject the XML loader used by DOMDocument.
#[test]
fn test_simplexml_dynamic_dom_load_xml() {
    if skip_without_xml_native("test_simplexml_dynamic_dom_load_xml") { return; }
    let out = compile_and_run(r#"<?php
$document = new DOMDocument();
$method = 'loadXML';
var_dump($document->$method('<r>dynamic</r>'));
var_dump($document?->{'loadXML'}('<r>nullsafe</r>'));
var_dump(call_user_func([$document, 'loadXML'], '<r>callback</r>'));
echo $document->documentElement->textContent;
"#);
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\ncallback");
}

/// A by-reference callback can replace a non-DOM local before its first XML load.
#[test]
fn test_simplexml_dom_load_after_captured_reassignment() {
    if skip_without_xml_native("test_simplexml_dom_load_after_captured_reassignment") { return; }
    let out = compile_and_run(r#"<?php
class InitialConfig {}
function initialDocumentConfig(): mixed { return new InitialConfig(); }
$document = initialDocumentConfig();
$replace = function () use (&$document): void { $document = new DOMDocument(); };
$replace();
var_dump($document->loadXML('<r>captured</r>'));
echo $document->documentElement->textContent;
$alias =& $document;
$document = initialDocumentConfig();
$alias = new DOMDocument();
var_dump($document->loadXML('<r>alias</r>'));
echo $document->documentElement->textContent;
"#);
    assert_eq!(out, "bool(true)\ncapturedbool(true)\nalias");
}

/// Loading HTML replaces a document's XML binding without invalidating retained old nodes.
#[test]
fn test_simplexml_dom_html_reload_bundle() {
    if skip_without_xml_native("test_simplexml_dom_html_reload_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
$document = new DOMDocument();
$document->loadXML('<old>before</old>');
$old = simplexml_import_dom($document);
$document->loadHTML('<html><body><p>after</p></body></html>');
var_dump($document->documentElement->nodeName, str_contains($document->saveXML(), 'after'), str_contains($document->saveXML(), '<old>'));
echo $old->getName(), ':', (string)$old, '|';
$document->loadHTML('<html><body><p>first</p></body></html>');
$imported = simplexml_import_dom($document);
$document->loadHTML('<html><body><p>second</p></body></html>');
var_dump(str_contains($document->saveXML(), 'second'), str_contains($document->saveXML(), 'first'));
$source = $argc > 0 ? '$ed = new DOMDocument(); $ed->loadXML("<old/>"); $ed->loadHTML("<html><body><p>new</p></body></html>"); var_dump($ed->documentElement->nodeName, str_contains($ed->saveXML(), "new"), str_contains($ed->saveXML(), "<old"));' : '';
eval($source);
"###);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(out.stdout, "string(4) \"html\"\nbool(true)\nbool(false)\nold:before|bool(true)\nbool(false)\nstring(4) \"html\"\nbool(true)\nbool(false)\n");
}

/// DOM failures warn before returning, imported wrappers isolate prefixes, and iterator errors match PHP.
#[test]
fn test_simplexml_dom_namespace_and_iterator_bundle() {
    if skip_without_xml_native("test_simplexml_dom_namespace_and_iterator_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
$doc = new DOMDocument(); $doc->loadXML('<r xmlns:p="urn:p" p:id="8">text<c/></r>');
$text = $doc->documentElement->childNodes[0];
var_dump(simplexml_import_dom($text));
var_dump(simplexml_import_dom(new DOMElement('a')));
var_dump(simplexml_import_dom(new DOMDocument()));
try { simplexml_import_dom(new stdClass()); } catch (TypeError $error) { echo $error->getMessage(), '|'; }
$first = simplexml_import_dom($doc);
$first->registerXPathNamespace('q', 'urn:p');
$second = simplexml_import_dom($doc);
var_dump($second->xpath('//@q:id'));
$second->registerXPathNamespace('q', 'urn:other');
echo count($first->xpath('//@q:id')), '|';
echo json_encode($first->attributes('p', true)->id->getNamespaces()), '|';
echo json_encode($first->attributes('p', true)->id->getNamespaces(true)), '|';
echo json_encode($first->attributes()->getNamespaces()), '|';
$iterator = new SimpleXMLElement('<r><a/></r>');
try { $iterator->current(); } catch (Error $error) { echo $error->getMessage(), '|'; }
try { $iterator->key(); } catch (Error $error) { echo $error->getMessage(), '|'; }
$iterator->rewind(); $iterator->next();
try { $iterator->current(); } catch (Error $error) { echo $error->getMessage(), '|'; }
try { $iterator->key(); } catch (Error $error) { echo $error->getMessage(), '|'; }
$bad = new DOMDocument();
var_dump($bad->loadXML('<r>'));
var_dump($bad->loadXML('<?xml version="1.1"?><r/>'));
try { $bad->loadXML(''); } catch (ValueError $error) { echo $error->getMessage(), '|'; }
restore_error_handler();
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('caught'); });
try { simplexml_import_dom($text); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
try { $bad->loadXML('<?xml version="1.1"?><r/>'); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
restore_error_handler();
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
$source = $argc > 0 ? '$ed = new DOMDocument(); var_dump(simplexml_import_dom($ed)); var_dump($ed->loadXML("<r>")); $ei = new SimpleXMLElement("<r/>"); try { $ei->key(); } catch (Error $error) { echo $error->getMessage(), "|"; }' : '';
eval($source);
restore_error_handler();
"###);
    let invalid = "2:simplexml_import_dom(): Invalid Nodetype to import|NULL\n";
    let missing = "2:simplexml_import_dom(): Imported Node must have associated Document|NULL\n";
    let consumed = "Iterator not initialized or already consumed|";
    let parse = "2:DOMDocument::loadXML(): Premature end of data in tag r line 1 in Entity, line: 1|bool(false)\n";
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(out.stdout, format!("{invalid}{missing}{invalid}simplexml_import_dom(): Argument #1 ($node) must be a valid XML node|2:SimpleXMLElement::xpath(): Undefined namespace prefix: q|bool(false)\n1|{{\"p\":\"urn:p\"}}|{{\"p\":\"urn:p\"}}|[]|{}{parse}8:DOMDocument::loadXML(): Unsupported version '1.1' in Entity, line: 1|bool(true)\nDOMDocument::loadXML(): Argument #1 ($source) must not be empty|caught|caught|{invalid}{parse}{consumed}", consumed.repeat(4)));
}
