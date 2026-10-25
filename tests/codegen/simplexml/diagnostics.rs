//! Purpose:
//! Verifies isolated XPath registrations and PHP-visible native diagnostics.
//!
//! Called from:
//! - SimpleXML codegen CI with managed libxml2 and optional PHP reference checks.
//!
//! Key details:
//! - Handler callbacks observe native messages after native resources have been rooted or freed.

use crate::support::*;

/// XPath results, iteration and addChild return objects with independent XPath namespace contexts.
#[test]
fn test_simplexml_derived_xpath_namespace_bundle() {
    if skip_without_xml_native("test_simplexml_derived_xpath_namespace_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
function inspectDerivedXPath(SimpleXMLElement $root, SimpleXMLElement $node): void {
    var_dump($node->xpath('//q:v'));
    $node->registerXPathNamespace('q', 'urn:other');
    echo count($root->xpath('//q:v')), '|';
}
$root = new SimpleXMLElement('<r xmlns:p="urn:p"><p:v>one</p:v><c/></r>');
$root->registerXPathNamespace('q', 'urn:p');
inspectDerivedXPath($root, $root->xpath('//c')[0]);
foreach ($root as $node) { inspectDerivedXPath($root, $node); }
$added = $root->addChild('d');
if ($added instanceof SimpleXMLElement) { inspectDerivedXPath($root, $added); }
$iterator = new SimpleXMLIterator('<r xmlns:p="urn:p"><p:v>one</p:v><c/></r>');
$iterator->registerXPathNamespace('q', 'urn:p'); $iterator->rewind();
$current = $iterator->current();
if ($current instanceof SimpleXMLElement) { inspectDerivedXPath($iterator, $current); }
$alias = $root;
$alias->registerXPathNamespace('same', 'urn:p');
echo count($root->xpath('//same:v')), '|';
$source = $argc > 0 ? '$ev = new SimpleXMLElement("<r xmlns:p=\"urn:p\"><p:v>one</p:v><c/></r>"); $ev->registerXPathNamespace("q", "urn:p"); $derived = $ev->xpath("//c")[0]; var_dump($derived->xpath("//q:v")); $derived->registerXPathNamespace("q", "urn:other"); echo count($ev->xpath("//q:v")), "|";' : '';
eval($source);
restore_error_handler();
"###);
    let warning = "2:SimpleXMLElement::xpath(): Undefined namespace prefix: q|bool(false)\n1|";
    assert_eq!(out, format!("{}1|{}", warning.repeat(4), warning));
}

/// Parse failures deliver all three warnings, preserve false/throw behavior, and tolerate reentry.
#[test]
fn test_simplexml_parse_diagnostic_handlers_bundle() {
    if skip_without_xml_native("test_simplexml_parse_diagnostic_handlers_bundle") { return; }
    let out = compile_and_run_with_heap_debug(r###"<?php
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
var_dump(simplexml_load_string('<r>'));
var_dump(simplexml_load_string(''));
var_dump(simplexml_load_string('<r>', null, LIBXML_NOERROR));
echo (int)(simplexml_load_string('<?xml version="1.1"?><r/>', null, LIBXML_NOWARNING) instanceof SimpleXMLElement), '|';
try { new SimpleXMLElement('<r>'); } catch (Exception $error) { echo $error->getMessage(), '|'; }
restore_error_handler();
set_error_handler(function(int $level, string $message): bool {
    echo $level, ':', substr($message, 0, 22), '|';
    return true;
});
$path = __DIR__ . '/simplexml-broken.xml'; file_put_contents($path, '<r>');
var_dump(simplexml_load_file($path)); unlink($path);
restore_error_handler();
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException('caught'); });
try { simplexml_load_string('<?xml version="1.1"?><r/>'); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
try { new SimpleXMLElement('<?xml version="1.1"?><r/>'); } catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
restore_error_handler();
set_error_handler(function(int $level, string $message): bool {
    @simplexml_load_string('<nested>');
    echo $level, ':', $message, '|'; return true;
});
var_dump(simplexml_load_string('<r>'));
restore_error_handler();
set_error_handler(function(int $level, string $message): bool { echo $level, ':', $message, '|'; return true; });
$source = $argc > 0 ? 'var_dump(simplexml_load_string("<r>")); try { new SimpleXMLElement("<r>"); } catch (Exception $error) { echo $error->getMessage(), "|"; }' : '';
eval($source);
restore_error_handler();
"###);
    let malformed = "Entity: line 1: parser error : Premature end of data in tag r line 1";
    let empty = "Entity: line 1: parser error : Document is empty";
    let warning = |caller: &str, message: &str, source: &str, caret: &str| {
        format!("2:{caller}: {message}|2:{caller}: {source}|2:{caller}: {caret}|")
    };
    let load = warning("simplexml_load_string()", malformed, "<r>", "   ^");
    let constructor = warning("SimpleXMLElement::__construct()", malformed, "<r>", "   ^");
    assert_eq!(out, format!("{load}bool(false)\n{}bool(false)\nbool(false)\n1|{constructor}String could not be parsed as XML|{}bool(false)\ncaught|caught|{load}bool(false)\n{load}bool(false)\n{constructor}String could not be parsed as XML|",
        warning("simplexml_load_string()", empty, "", "^"),
        "2:simplexml_load_file():|".repeat(3)));
}
