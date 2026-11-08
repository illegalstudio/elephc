//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for the legacy node classes (`DOMNode`, `DOMElement`, `DOMAttr`, character data, ...), as Elephc codegen tests.
//!
//! Called from:
//! - `cargo test` through `tests/codegen/dom_php_src/mod.rs`.
//!
//! Key details:
//! - Each test keeps the php-src program and expected output verbatim; its doc
//!   comment names the PHPT it was migrated from.
//! - A test Elephc does not pass yet is `#[ignore]`d with the missing behaviour, and
//!   php-src's own environment skips keep their `SKIPIF` message.

use super::*;

/// php-src `ext/dom/tests/DOM4_ChildNode_wrong_document.phpt`: DOMChildNode::after(), before, replaceWith with DOMNode from wrong document throws exception
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_child_node_wrong_document() {
    if skip_without_xml_native("test_dom4_child_node_wrong_document") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

function test($method) {
    $dom1 = new DOMDocument;
    $dom1->loadXML('<test/>');

    $dom2 = new DOMDocument;
    $dom2->loadXML('<test><foo /></test>');

    $element = $dom1->documentElement;

    try {
        $element->$method($dom2->documentElement->firstChild);
        echo "FAIL";
    } catch (DOMException $e) {
        echo $e->getMessage() . "\n";
    }
}

test("after");
test("before");
test("replaceWith");

"#);
    assert_exact(&out, r#"Wrong Document Error
Wrong Document Error
Wrong Document Error"#);
}

/// php-src `ext/dom/tests/DOM4_DOMNode_ElementSiblings.phpt`: DOMNode: Element Siblings
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_domnode_element_siblings() {
    if skip_without_xml_native("test_dom4_domnode_element_siblings") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$dom = new DOMDocument;
$dom->loadXML('<test>foo<bar>FirstElement</bar><bar>LastElement</bar>bar</test>');

$element = $dom->documentElement;
print_node($element->firstElementChild->nextElementSibling);
print_node($element->lastElementChild->previousElementSibling);
"#);
    assert_exact(&out, r#"Node Name: bar
Node Type: 1
Num Children: 1
Node Content: LastElement

Node Name: bar
Node Type: 1
Num Children: 1
Node Content: FirstElement"#);
}

/// php-src `ext/dom/tests/DOM4_DOMNode_after.phpt`: DOMNode::after()
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_domnode_after() {
    if skip_without_xml_native("test_dom4_domnode_after") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$dom = new DOMDocument;
$dom->loadXML('<test><mark>first</mark><mark>second</mark></test>');

$element = $dom->documentElement->firstElementChild;
$secondMark = $dom->documentElement->lastElementChild;

$element->after(
  'text inserted after',
  $dom->createElement('inserted-after', 'content')
);

$secondMark->after('text inserted after second');

print_node_list_compact($dom->documentElement->childNodes);
"#);
    assert_exact(&out, r#"<mark>
  first
</mark>
text inserted after
<inserted-after>
  content
</inserted-after>
<mark>
  second
</mark>
text inserted after second"#);
}

/// php-src `ext/dom/tests/DOM4_DOMNode_after_ns.phpt`: DOMNode::after() with namespace
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_domnode_after_ns() {
    if skip_without_xml_native("test_dom4_domnode_after_ns") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$doc  = new DOMDocument('1.0', 'utf-8');
$doc->formatOutput = true;

$root = $doc->createElementNS('http://www.w3.org/2005/Atom', 'element');
$doc->appendChild($root);
$root->setAttributeNS('http://www.w3.org/2000/xmlns/' ,'xmlns:g', 'http://base.google.com/ns/1.0');

$item = $doc->createElementNS('http://base.google.com/ns/1.0', 'g:item_type', 'house');
$root->append($item);

$item2 = $doc->createElementNS('http://base.google.com/ns/1.0', 'g:item_type', 'street');
$item->after($item2);

echo $doc->saveXML(), "\n";
"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="utf-8"?>
<element xmlns="http://www.w3.org/2005/Atom" xmlns:g="http://base.google.com/ns/1.0">
  <g:item_type>house</g:item_type>
  <g:item_type>street</g:item_type>
</element>"#);
}

/// php-src `ext/dom/tests/DOM4_DOMNode_append_ns.phpt`: DOMNode::append() with namespace
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_domnode_append_ns() {
    if skip_without_xml_native("test_dom4_domnode_append_ns") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$doc  = new DOMDocument('1.0', 'utf-8');
$doc->formatOutput = true;

$root = $doc->createElementNS('http://www.w3.org/2005/Atom', 'element');
$doc->appendChild($root);
$root->setAttributeNS('http://www.w3.org/2000/xmlns/' ,'xmlns:g', 'http://base.google.com/ns/1.0');

$item = $doc->createElementNS('http://base.google.com/ns/1.0', 'g:item_type', 'house');
$root->append($item);

echo $doc->saveXML(), "\n";
"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="utf-8"?>
<element xmlns="http://www.w3.org/2005/Atom" xmlns:g="http://base.google.com/ns/1.0">
  <g:item_type>house</g:item_type>
</element>"#);
}

/// php-src `ext/dom/tests/DOM4_DOMNode_before.phpt`: DOMNode::before()
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_domnode_before() {
    if skip_without_xml_native("test_dom4_domnode_before") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$dom = new DOMDocument;
$dom->loadXML('<test><mark>first</mark><mark>second</mark></test>');

$element = $dom->documentElement->firstElementChild;
$secondMark = $dom->documentElement->lastElementChild;

$element->before(
  $dom->createElement('inserted-before', 'content'),
  'text inserted before'
);

$secondMark->before('text inserted before second');

print_node_list_compact($dom->documentElement->childNodes);
"#);
    assert_exact(&out, r#"<inserted-before>
  content
</inserted-before>
text inserted before
<mark>
  first
</mark>
text inserted before second
<mark>
  second
</mark>"#);
}

/// php-src `ext/dom/tests/DOM4_DOMNode_before_ns.phpt`: DOMNode::before() with namespace
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_domnode_before_ns() {
    if skip_without_xml_native("test_dom4_domnode_before_ns") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$doc  = new DOMDocument('1.0', 'utf-8');
$doc->formatOutput = true;

$root = $doc->createElementNS('http://www.w3.org/2005/Atom', 'element');
$doc->appendChild($root);
$root->setAttributeNS('http://www.w3.org/2000/xmlns/' ,'xmlns:g', 'http://base.google.com/ns/1.0');

$item = $doc->createElementNS('http://base.google.com/ns/1.0', 'g:item_type', 'house');
$root->append($item);

$item2 = $doc->createElementNS('http://base.google.com/ns/1.0', 'g:item_type', 'street');
$item->before($item2);

echo $doc->saveXML(), "\n";
"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="utf-8"?>
<element xmlns="http://www.w3.org/2005/Atom" xmlns:g="http://base.google.com/ns/1.0">
  <g:item_type>street</g:item_type>
  <g:item_type>house</g:item_type>
</element>"#);
}

/// php-src `ext/dom/tests/DOM4_DOMNode_prepend_ns.phpt`: DOMNode::prepend() with namespace
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_domnode_prepend_ns() {
    if skip_without_xml_native("test_dom4_domnode_prepend_ns") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$doc  = new DOMDocument('1.0', 'utf-8');
$doc->formatOutput = true;

$root = $doc->createElementNS('http://www.w3.org/2005/Atom', 'element');
$doc->appendChild($root);
$root->setAttributeNS('http://www.w3.org/2000/xmlns/' ,'xmlns:g', 'http://base.google.com/ns/1.0');

$item = $doc->createElementNS('http://base.google.com/ns/1.0', 'g:item_type', 'house');
$root->prepend($item);

echo $doc->saveXML(), "\n";
"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="utf-8"?>
<element xmlns="http://www.w3.org/2005/Atom" xmlns:g="http://base.google.com/ns/1.0">
  <g:item_type>house</g:item_type>
</element>"#);
}

/// php-src `ext/dom/tests/DOM4_DOMNode_remove.phpt`: DOMNode::remove()
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_domnode_remove() {
    if skip_without_xml_native("test_dom4_domnode_remove") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$dom = new DOMDocument;
$dom->loadXML('<test><one>first</one><two>second</two></test>');

$element = $dom->documentElement;
print_node($element->firstChild);
$returnValue = $element->firstChild->remove();
print_node($element->firstChild);
var_dump($returnValue);
"#);
    assert_exact(&out, r#"Node Name: one
Node Type: 1
Num Children: 1
Node Content: first

Node Name: two
Node Type: 1
Num Children: 1
Node Content: second

NULL"#);
}

/// php-src `ext/dom/tests/DOM4_DOMNode_removeDanglingElement.phpt`: DOMNode::remove() dangling element
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMElement::remove"#]
fn test_dom4_domnode_remove_dangling_element() {
    if skip_without_xml_native("test_dom4_domnode_remove_dangling_element") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();

$element = $dom->createElement('test');

try {
    $element->remove();
} catch (DOMException $e) {
    echo $e->getMessage();
}
"#);
    assert_exact(&out, r#"Not Found Error"#);
}

/// php-src `ext/dom/tests/DOM4_DOMNode_replaceWith.phpt`: DOMNode::replaceWith()
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_domnode_replace_with() {
    if skip_without_xml_native("test_dom4_domnode_replace_with") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$dom = new DOMDocument;
$dom->loadXML('<test><mark>first</mark><mark>second</mark></test>');

$element = $dom->documentElement->firstChild;
$element->replaceWith(
  $dom->createElement('element', 'content'),
  'content'
);

print_node_list_compact($dom->documentElement->childNodes);
"#);
    assert_exact(&out, r#"<element>
  content
</element>
content
<mark>
  second
</mark>"#);
}

/// php-src `ext/dom/tests/DOM4_ParentNode.phpt`: DOMParentNode: Child Element Handling
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_parent_node() {
    if skip_without_xml_native("test_dom4_parent_node") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$dom = new DOMDocument;
$dom->loadXML('<test>foo<bar>FirstElement</bar><bar>LastElement</bar>bar</test>');

var_dump($dom instanceof DOMParentNode);
print_node($dom->firstElementChild);
print_node($dom->lastElementChild);
var_dump($dom->childElementCount);

$element = $dom->documentElement;
var_dump($element instanceof DOMParentNode);
print_node($element->firstElementChild);
print_node($element->lastElementChild);
var_dump($element->childElementCount);
var_dump($element->lastElementChild->firstElementChild);
var_dump($element->lastElementChild->lastElementChild);
var_dump($element->lastElementChild->childElementCount);
"#);
    assert_exact(&out, r#"bool(true)
Node Name: test
Node Type: 1
Num Children: 4

Node Name: test
Node Type: 1
Num Children: 4

int(1)
bool(true)
Node Name: bar
Node Type: 1
Num Children: 1
Node Content: FirstElement

Node Name: bar
Node Type: 1
Num Children: 1
Node Content: LastElement

int(2)
NULL
NULL
int(0)"#);
}

/// php-src `ext/dom/tests/DOM4_ParentNode_Fragment.phpt`: DOMParentNode: Child Element Handling
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_parent_node_fragment() {
    if skip_without_xml_native("test_dom4_parent_node_fragment") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$dom = new DOMDocument;
$dom->loadXML('<test></test>');

$fragment = $dom->createDocumentFragment();
$fragment->appendChild($dom->createTextNode('foo'));
$fragment->appendChild($dom->createElement('bar', 'FirstElement'));
$fragment->appendChild($dom->createElement('bar', 'LastElement'));
$fragment->appendChild($dom->createTextNode('bar'));

var_dump($fragment instanceof DOMParentNode);
print_node($fragment->firstElementChild);
print_node($fragment->lastElementChild);
var_dump($fragment->childElementCount);
var_dump($fragment->lastElementChild->firstElementChild);
var_dump($fragment->lastElementChild->lastElementChild);
var_dump($fragment->lastElementChild->childElementCount);
"#);
    assert_exact(&out, r#"bool(true)
Node Name: bar
Node Type: 1
Num Children: 1
Node Content: FirstElement

Node Name: bar
Node Type: 1
Num Children: 1
Node Content: LastElement

int(2)
NULL
NULL
int(0)"#);
}

/// php-src `ext/dom/tests/DOM4_ParentNode_append.phpt`: DOMParentNode::append()
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_parent_node_append() {
    if skip_without_xml_native("test_dom4_parent_node_append") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
declare(strict_types=1);

require_once("dom_test.inc");

$dom = new DOMDocument;
$dom->loadXML('<test><mark/><mark /><mark /></test>');

$element = $dom->documentElement;
$element->append(
  $dom->createElement('element', 'content'),
  'content'
);

var_dump($dom->documentElement->childElementCount);
print_node_list_compact($element->childNodes);

$element->append(
  $dom->createElement('element', 'content'),
  'content'
);
var_dump($dom->documentElement->childElementCount);
"#);
    assert_exact(&out, r#"int(4)
<mark>
</mark>
<mark>
</mark>
<mark>
</mark>
<element>
  content
</element>
content
int(5)"#);
}

/// php-src `ext/dom/tests/DOM4_ParentNode_append_invalidtypes.phpt`: DOMParentNode::append() exception on invalid argument
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_parent_node_append_invalidtypes() {
    if skip_without_xml_native("test_dom4_parent_node_append_invalidtypes") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$dom = new DOMDocument;
$dom->loadXML('<test />');

try {
    $dom->documentElement->append(array());
} catch(TypeError $e) {
    echo "OK! {$e->getMessage()}";
}
"#);
    assert_exact(&out, r#"OK! DOMElement::append(): Argument #1 must be of type DOMNode|string, array given"#);
}

/// php-src `ext/dom/tests/DOM4_ParentNode_append_with_attributes.phpt`: DOMParentNode::append() with attributes
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_parent_node_append_with_attributes() {
    if skip_without_xml_native("test_dom4_parent_node_append_with_attributes") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$dom = new DOMDocument;
$dom->loadXML('<test attr-one="21"/>');

$replacement = $dom->createAttribute('attr-one');
$replacement->value ='42';
$addition = $dom->createAttribute('attr-two');
$addition->value = '23';

$element = $dom->documentElement;

try {
    $element->append($replacement, $addition);
} catch (DOMException $e) {
    echo $e->getMessage() . "\n";
}

echo $dom->saveXML();
"#);
    assert_exact(&out, r#"Hierarchy Request Error
<?xml version="1.0"?>
<test attr-one="21"/>"#);
}

/// php-src `ext/dom/tests/DOM4_ParentNode_append_wrong_document.phpt`: DOMParentNode::append() with DOMNode from wrong document throws exception
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_parent_node_append_wrong_document() {
    if skip_without_xml_native("test_dom4_parent_node_append_wrong_document") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

function test($method) {
    $dom1 = new DOMDocument;
    $dom1->loadXML('<test/>');

    $dom2 = new DOMDocument;
    $dom2->loadXML('<test><foo /></test>');

    $element = $dom1->documentElement;

    try {
        $element->$method($dom2->documentElement->firstChild);
        echo "FAIL";
    } catch (DOMException $e) {
        echo $e->getMessage() . "\n";
    }
}

test("append");
test("prepend");
"#);
    assert_exact(&out, r#"Wrong Document Error
Wrong Document Error"#);
}

/// php-src `ext/dom/tests/DOM4_ParentNode_prepend.phpt`: DOMParentNode::prepend()
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_dom4_parent_node_prepend() {
    if skip_without_xml_native("test_dom4_parent_node_prepend") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
declare(strict_types=1);

require_once("dom_test.inc");

$dom = new DOMDocument;
$dom->loadXML('<test><mark/><mark><nested /></mark><mark/></test>');

$element = $dom->documentElement;
$firstMark = $element->childNodes[0];
$element->prepend(
  $dom->createElement('element', 'content'),
  'content'
);

var_dump($element->childElementCount);
print_node_list_compact($element->childNodes);

$element = $dom->documentElement;
$element->prepend(
  $dom->createElement('element', 'content'),
  'content'
);
var_dump($element->childElementCount);

$firstMark->prepend('content');
print_node_list_compact($firstMark->childNodes);
"#);
    assert_exact(&out, r#"int(4)
<element>
  content
</element>
content
<mark>
</mark>
<mark>
  <nested>
  </nested>
</mark>
<mark>
</mark>
int(5)
content"#);
}

/// php-src `ext/dom/tests/DOMAttr_construct_error_001.phpt`: DOMAttr __construct() with no arguments.
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMAttr"#]
fn test_domattr_construct_error_001() {
    if skip_without_xml_native("test_domattr_construct_error_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
try {
    $attr = new DOMAttr();
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
"#);
    assert_exact(&out, r#"DOMAttr::__construct() expects at least 1 argument, 0 given"#);
}

/// php-src `ext/dom/tests/DOMAttr_name_basic_001.phpt`: DOMAttr read $name property.
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMAttr"#]
fn test_domattr_name_basic_001() {
    if skip_without_xml_native("test_domattr_name_basic_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$attr = new DOMAttr('category', 'books');
print $attr->name;
"#);
    assert_exact(&out, r#"category"#);
}

/// php-src `ext/dom/tests/DOMAttr_ownerElement_error_001.phpt`: Read $ownerElement with null parent.
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_domattr_owner_element_error_001() {
    if skip_without_xml_native("test_domattr_owner_element_error_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$document = new DOMDocument;
$root = $document->createElement('root');
$document->appendChild($root);
$attr = $root->setAttribute('category', 'books');
$document->removeChild($root);
$root = null;
var_dump($attr->ownerElement);
"#);
    assert_exact(&out, r#"NULL"#);
}

/// php-src `ext/dom/tests/DOMAttr_value_basic_001.phpt`: Read empty $value.
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMAttr"#]
fn test_domattr_value_basic_001() {
    if skip_without_xml_native("test_domattr_value_basic_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$attr = new DOMAttr('category');
print $attr->value."\n";
echo '===DONE===
';"#);
    assert_exact(&out, r#"===DONE==="#);
}

/// php-src `ext/dom/tests/DOMAttr_value_basic_002.phpt`: Write non-string $value property
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMAttr"#]
fn test_domattr_value_basic_002() {
    if skip_without_xml_native("test_domattr_value_basic_002") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$attr = new DOMAttr('category');
$attr->value = 1;
print $attr->value;
"#);
    assert_exact(&out, r#"1"#);
}

/// php-src `ext/dom/tests/DOMCDATASection_construct_error_001.phpt`: __construct() with no arguments.
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMCDataSection"#]
fn test_domcdatasection_construct_error_001() {
    if skip_without_xml_native("test_domcdatasection_construct_error_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
    try {
        $section = new DOMCDataSection();
    } catch (TypeError $e) {
        echo $e->getMessage();
    }
"#);
    assert_exact(&out, r#"DOMCdataSection::__construct() expects exactly 1 argument, 0 given"#);
}

/// php-src `ext/dom/tests/DOMCharacterData_appendData_basic.phpt`: DOMCharacterData::appendData basic functionality test
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createCDATASection"#]
fn test_domcharacter_data_append_data_basic() {
    if skip_without_xml_native("test_domcharacter_data_append_data_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$document = new DOMDocument;
$root = $document->createElement('root');
$document->appendChild($root);

$cdata = $document->createElement('cdata');
$root->appendChild($cdata);

$cdatanode = $document->createCDATASection('');
$cdata->appendChild($cdatanode);
$cdatanode->appendData('data');
echo "CDATA Length (one append): " . $cdatanode->length . "\n";

$cdatanode->appendData('><&"');
echo "CDATA Length (two appends): " . $cdatanode->length . "\n";

echo "CDATA Content: " . $cdatanode->data . "\n";

echo "\n" . $document->saveXML();

"#);
    assert_exact(&out, r#"CDATA Length (one append): 4
CDATA Length (two appends): 8
CDATA Content: data><&"

<?xml version="1.0"?>
<root><cdata><![CDATA[data><&"]]></cdata></root>"#);
}

/// php-src `ext/dom/tests/DOMCharacterData_data_basic_002.phpt`: Create CDATA section and change it using DOMcreateCDATASection
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createCDATASection"#]
fn test_domcharacter_data_data_basic_002() {
    if skip_without_xml_native("test_domcharacter_data_data_basic_002") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$document = new DOMDocument;
$root = $document->createElement('root');
$document->appendChild($root);

$cdata = $document->createCDATASection('t');
$root->appendChild($cdata);
print $document->saveXML()."\n";

$cdata->data = 100;
print $document->saveXML()."\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<root><![CDATA[t]]></root>

<?xml version="1.0"?>
<root><![CDATA[100]]></root>"#);
}

/// php-src `ext/dom/tests/DOMCharacterData_data_error_002.phpt`: Invalid State Error when getting data on DOMCharacterData out of content.
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMCharacterData::data"#]
fn test_domcharacter_data_data_error_002() {
    if skip_without_xml_native("test_domcharacter_data_data_error_002") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$character_data = new DOMCharacterData();
try {
    print $character_data->data;
} catch (DOMException $exception) {
    echo $exception->getMessage() . "\n";
}
"#);
    assert_exact(&out, r#"Invalid State Error"#);
}

/// php-src `ext/dom/tests/DOMCharacterData_deleteData_basic_001.phpt`: DOMCharacterData::deleteData() with count exceeding string size.
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createCDATASection"#]
fn test_domcharacter_data_delete_data_basic_001() {
    if skip_without_xml_native("test_domcharacter_data_delete_data_basic_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$document = new DOMDocument;
$root = $document->createElement('root');
$document->appendChild($root);

$cdata = $document->createCDATASection('test');
$root->appendChild($cdata);
$cdata->deleteData(1, 10);
var_dump($cdata->data);
"#);
    assert_format(&out, r#"string(%d) "t""#);
}

/// php-src `ext/dom/tests/DOMCharacterData_deleteData_error_002.phpt`: DOMCharacterData::deleteData() with offset exceeding string size.
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createCDATASection"#]
fn test_domcharacter_data_delete_data_error_002() {
    if skip_without_xml_native("test_domcharacter_data_delete_data_error_002") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$document = new DOMDocument;
$root = $document->createElement('root');
$document->appendChild($root);

$cdata = $document->createCDATASection('test');
$root->appendChild($cdata);
try {
    $cdata->deleteData(5, 1);
} catch (DOMException $e) {
    echo $e->getMessage();
}
"#);
    assert_exact(&out, r#"Index Size Error"#);
}

/// php-src `ext/dom/tests/DOMCharacterData_length_error_001.phpt`: Invalid State Error when getting length on DOMCharacterData out of content.
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMCharacterData::length"#]
fn test_domcharacter_data_length_error_001() {
    if skip_without_xml_native("test_domcharacter_data_length_error_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$character_data = new DOMCharacterData();
try {
    print $character_data->length;
} catch (DOMException $exception) {
    echo $exception->getMessage() . "\n";
}
"#);
    assert_exact(&out, r#"Invalid State Error"#);
}

/// php-src `ext/dom/tests/DOMCharacterData_replaceWith_self.phpt`: DOMCharacterData::replaceWith() with itself
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domcharacter_data_replace_with_self() {
    if skip_without_xml_native("test_domcharacter_data_replace_with_self") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument;
$dom->loadXML('<?xml version="1.0"?><container><![CDATA[Hello]]></container>');
$cdata = $dom->documentElement->firstChild;
$cdata->replaceWith($cdata);
echo $dom->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<container><![CDATA[Hello]]></container>"#);
}

/// php-src `ext/dom/tests/DOMCharacterData_substringData_basic_001.phpt`: __DOMCharacterData::substringData pull mid section of string
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createCDATASection"#]
fn test_domcharacter_data_substring_data_basic_001() {
    if skip_without_xml_native("test_domcharacter_data_substring_data_basic_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$document = new DOMDocument;
$root = $document->createElement('root');
$document->appendChild($root);

$cdata = $document->createCDATASection('testfest');
$root->appendChild($cdata);
print $cdata->substringData(1, 6);

"#);
    assert_exact(&out, r#"estfes"#);
}

/// php-src `ext/dom/tests/DOMChildNode_methods_without_parent.phpt`: DOMChildNode methods without a parent
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domchild_node_methods_without_parent() {
    if skip_without_xml_native("test_domchild_node_methods_without_parent") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXML(<<<XML
<?xml version="1.0"?>
<container>
    <child/>
</container>
XML);

$container = $doc->documentElement;
$child = $container->firstElementChild;

$test = $doc->createElement('foo');
foreach (['before', 'after', 'replaceWith'] as $method) {
    echo "--- $method ---\n";
    $test->$method($child);
    echo $doc->saveXML();
    echo $doc->saveXML($test), "\n";
}
"#);
    assert_exact(&out, r#"--- before ---
<?xml version="1.0"?>
<container>
    <child/>
</container>
<foo/>
--- after ---
<?xml version="1.0"?>
<container>
    <child/>
</container>
<foo/>
--- replaceWith ---
<?xml version="1.0"?>
<container>
    <child/>
</container>
<foo/>"#);
}

/// php-src `ext/dom/tests/DOMComment_appendData_basic.phpt`: Test adding data to a DOMComment
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createComment"#]
fn test_domcomment_append_data_basic() {
    if skip_without_xml_native("test_domcomment_append_data_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DomDocument();
$comment = $dom->createComment('test-comment');
$comment->appendData('-more-data');
$dom->appendChild($comment);
$dom->saveXML();
echo $dom->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<!--test-comment-more-data-->"#);
}

/// php-src `ext/dom/tests/DOMComment_appendData_basic_Sullivan.phpt`: DOMComment::appendData basic functionality test
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createComment"#]
fn test_domcomment_append_data_basic_sullivan() {
    if skip_without_xml_native("test_domcomment_append_data_basic_sullivan") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$document = new DOMDocument;
$root = $document->createElement('root');
$document->appendChild($root);

$comment = $document->createElement('comment');
$root->appendChild($comment);

$commentnode = $document->createComment('');
$comment->appendChild($commentnode);
$commentnode->appendData('data');
echo "Comment Length (one append): " . $commentnode->length . "\n";

$commentnode->appendData('><&"');
echo "Comment Length (two appends): " . $commentnode->length . "\n";

echo "Comment Content: " . $commentnode->data . "\n";

echo "\n" . $document->saveXML();

"#);
    assert_exact(&out, r#"Comment Length (one append): 4
Comment Length (two appends): 8
Comment Content: data><&"

<?xml version="1.0"?>
<root><comment><!--data><&"--></comment></root>"#);
}

/// php-src `ext/dom/tests/DOMComment_construct_basic_001.phpt`: DOMComment::__construct() with constructor called twice.
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMElement::__construct' expects 0 arguments, got 1"#]
fn test_domcomment_construct_basic_001() {
    if skip_without_xml_native("test_domcomment_construct_basic_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument('1.0', 'UTF-8');
$element = $dom->appendChild(new DOMElement('root'));
$comment = new DOMComment("This is the first comment.");
$comment->__construct("This is the second comment.");
$comment = $element->appendChild($comment);
print $dom->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root><!--This is the second comment.--></root>"#);
}

/// php-src `ext/dom/tests/DOMComment_construct_error_001.phpt`: DOMComment::__construct() with more arguments than acceptable.
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMComment::__construct' expects 0 arguments, got 2"#]
fn test_domcomment_construct_error_001() {
    if skip_without_xml_native("test_domcomment_construct_error_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
try {
    $comment = new DOMComment("comment1", "comment2");
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
"#);
    assert_exact(&out, r#"DOMComment::__construct() expects at most 1 argument, 2 given"#);
}

/// php-src `ext/dom/tests/DOMComment_insertData_basic.phpt`: Test inserting data into a DOMComment basic test
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createComment"#]
fn test_domcomment_insert_data_basic() {
    if skip_without_xml_native("test_domcomment_insert_data_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

//correct offset
$dom = new DomDocument();
$comment = $dom->createComment('test-comment');
$comment->insertData(4,'-inserted');
$dom->appendChild($comment);
echo $dom->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<!--test-inserted-comment-->"#);
}

/// php-src `ext/dom/tests/DOMComment_insertData_error1.phpt`: Test inserting data into a DOMComment basic test
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createComment"#]
fn test_domcomment_insert_data_error1() {
    if skip_without_xml_native("test_domcomment_insert_data_error1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

//Negative offset
$dom = new DomDocument();
$comment = $dom->createComment('test-comment');
try {
  $comment->insertData(-1,'-inserted');
} catch (DOMException $e ) {
  if ($e->getMessage() == 'Index Size Error'){
    echo "Throws DOMException for -ve offset\n";
  }
}

"#);
    assert_exact(&out, r#"Throws DOMException for -ve offset"#);
}

/// php-src `ext/dom/tests/DOMComment_insertData_error2.phpt`: Test inserting data into a DOMComment basic test
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createComment"#]
fn test_domcomment_insert_data_error2() {
    if skip_without_xml_native("test_domcomment_insert_data_error2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

//offset to large
$dom = new DomDocument();
$comment = $dom->createComment('test-comment');
try {
  $comment->insertData(999,'-inserted');
} catch (DOMException $e ) {
  if ($e->getMessage() == 'Index Size Error'){
    echo "Throws DOMException for offset too large\n";
  }
}

"#);
    assert_exact(&out, r#"Throws DOMException for offset too large"#);
}

/// php-src `ext/dom/tests/DOMComment_replaceData_basic.phpt`: Test replacing data into a DOMComment basic test
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createComment"#]
fn test_domcomment_replace_data_basic() {
    if skip_without_xml_native("test_domcomment_replace_data_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DomDocument();
$comment = $dom->createComment('test-comment');
$comment->replaceData(4,1,'replaced');
$dom->appendChild($comment);
echo $dom->saveXML();

// Replaces rest of string if count is greater than length of existing string
$dom = new DomDocument();
$comment = $dom->createComment('test-comment');
$comment->replaceData(0,50,'replaced');
$dom->appendChild($comment);
echo $dom->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<!--testreplacedcomment-->
<?xml version="1.0"?>
<!--replaced-->"#);
}

/// php-src `ext/dom/tests/DOMComment_replaceData_error1.phpt`: Test replacing data into a DOMComment basic test
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createComment"#]
fn test_domcomment_replace_data_error1() {
    if skip_without_xml_native("test_domcomment_replace_data_error1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

//Negative offset
$dom = new DomDocument();
$comment = $dom->createComment('test-comment');
try {
  $comment->replaceData(-1,4,'-inserted');
} catch (DOMException $e ) {
  if ($e->getMessage() == 'Index Size Error'){
    echo "Throws DOMException for -ve offset\n";
  }
}

"#);
    assert_exact(&out, r#"Throws DOMException for -ve offset"#);
}

/// php-src `ext/dom/tests/DOMComment_replaceData_error2.phpt`: Test replacing data into a DOMComment basic test
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createComment"#]
fn test_domcomment_replace_data_error2() {
    if skip_without_xml_native("test_domcomment_replace_data_error2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

//offset to large
$dom = new DomDocument();
$comment = $dom->createComment('test-comment');
try {
  $comment->replaceData(999,4,'-inserted');
} catch (DOMException $e ) {
  if ($e->getMessage() == 'Index Size Error'){
    echo "Throws DOMException for offset too large\n";
  }
}

"#);
    assert_exact(&out, r#"Throws DOMException for offset too large"#);
}

/// php-src `ext/dom/tests/DOMElement_append_hierarchy_test.phpt`: DOMElement::append() with hierarchy changes and errors
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::firstChild"#]
fn test_domelement_append_hierarchy_test() {
    if skip_without_xml_native("test_domelement_append_hierarchy_test") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom_original = new DOMDocument;
$dom_original->loadXML('<p><b>hello</b><b><i>world</i></b></p>');

echo "-- Append hello with world --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
$b_world = $b_hello->nextSibling;
$b_hello->append($b_world);
var_dump($dom->saveHTML());

echo "-- Append hello with world's child --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
$b_world = $b_hello->nextSibling;
$b_hello->append($b_world->firstChild);
var_dump($dom->saveHTML());

echo "-- Append hello with world's child and text --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
$b_world = $b_hello->nextSibling;
$b_hello->append($b_world->firstChild, "foo");
var_dump($dom->saveHTML());

echo "-- Append world's child with hello --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
$b_world = $b_hello->nextSibling;
$b_world->firstChild->append($b_hello);
var_dump($dom->saveHTML());

echo "-- Append hello with itself --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
try {
    $b_hello->append($b_hello);
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}
var_dump($dom->saveHTML());

echo "-- Append hello with itself and text --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
try {
    $b_hello->append($b_hello, "foo");
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}
var_dump($dom->saveHTML());

echo "-- Append world's i tag with the parent --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
$b_world = $b_hello->nextSibling;
try {
    $b_world->firstChild->append($b_world);
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}
var_dump($dom->saveHTML());

echo "-- Append from another document --\n";
$dom = clone $dom_original;
$dom2 = new DOMDocument;
$dom2->loadXML('<p>other</p>');
try {
    $dom->firstChild->firstChild->prepend($dom2->firstChild);
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}
var_dump($dom2->saveHTML());
var_dump($dom->saveHTML());

"#);
    assert_exact(&out, r#"-- Append hello with world --
string(39) "<p><b>hello<b><i>world</i></b></b></p>
"
-- Append hello with world's child --
string(39) "<p><b>hello<i>world</i></b><b></b></p>
"
-- Append hello with world's child and text --
string(42) "<p><b>hello<i>world</i>foo</b><b></b></p>
"
-- Append world's child with hello --
string(39) "<p><b><i>world<b>hello</b></i></b></p>
"
-- Append hello with itself --
Hierarchy Request Error
string(39) "<p><b>hello</b><b><i>world</i></b></p>
"
-- Append hello with itself and text --
Hierarchy Request Error
string(27) "<p><b><i>world</i></b></p>
"
-- Append world's i tag with the parent --
Hierarchy Request Error
string(39) "<p><b>hello</b><b><i>world</i></b></p>
"
-- Append from another document --
Wrong Document Error
string(13) "<p>other</p>
"
string(39) "<p><b>hello</b><b><i>world</i></b></p>
""#);
}

/// php-src `ext/dom/tests/DOMElement_className.phpt`: DOMElement::className
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domelement_class_name() {
    if skip_without_xml_native("test_domelement_class_name") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class MyStringable {
    public function __toString(): string {
        throw new Exception("foo");
    }
}

$dom = new DOMDocument();
$dom->loadXML('<html/>');

var_dump($dom->documentElement->className);
$dom->documentElement->className = "hello & world<>";
var_dump($dom->documentElement->className);
$dom->documentElement->className = "";
var_dump($dom->documentElement->className);
$dom->documentElement->className = "é";
var_dump($dom->documentElement->className);
$dom->documentElement->className = "\0";
var_dump($dom->documentElement->className);
$dom->documentElement->className = 12345;
var_dump($dom->documentElement->className);
try {
    $dom->documentElement->className = new MyStringable();
} catch (Throwable $e) {
    echo "Error: ", $e->getMessage(), "\n";
}
var_dump($dom->documentElement->className);
echo $dom->saveXML();

"#);
    assert_exact(&out, r#"string(0) ""
string(15) "hello & world<>"
string(0) ""
string(2) "é"
string(0) ""
string(5) "12345"
Error: foo
string(5) "12345"
<?xml version="1.0"?>
<html class="12345"/>"#);
}

/// php-src `ext/dom/tests/DOMElement_getAttributeNames.phpt`: DOMElement::getAttributeNames()
#[test]
#[ignore = r#"DOM port pending: Undefined function: assert"#]
fn test_domelement_get_attribute_names() {
    if skip_without_xml_native("test_domelement_get_attribute_names") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

function test($str) {
    $dom = new DOMDocument();
    $dom->loadXML($str);
    var_dump($dom->documentElement->getAttributeNames());
    foreach ($dom->documentElement->getAttributeNames() as $name) {
        assert($dom->documentElement->getAttributeNode($name)->nodeName === $name);
    }
}

test('<html xmlns:some="some:ns" some:test="a" test2="b"/>');
test('<html test="b" test3="c"/>');
test('<html/>');

"#);
    assert_exact(&out, r#"array(3) {
  [0]=>
  string(10) "xmlns:some"
  [1]=>
  string(9) "some:test"
  [2]=>
  string(5) "test2"
}
array(2) {
  [0]=>
  string(4) "test"
  [1]=>
  string(5) "test3"
}
array(0) {
}"#);
}

/// php-src `ext/dom/tests/DOMElement_getElementsByTagName_without_document.phpt`: Node list cache should not break on DOMElement::getElementsByTagName() without document
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMElement::__construct' expects 0 arguments, got 1"#]
fn test_domelement_get_elements_by_tag_name_without_document() {
    if skip_without_xml_native("test_domelement_get_elements_by_tag_name_without_document") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$element = new DOMElement("b");
foreach ($element->getElementsByTagName("b") as $x) {
    var_dump($x);
}

echo 'Done
';"#);
    assert_exact(&out, r#"Done"#);
}

/// php-src `ext/dom/tests/DOMElement_get_or_has_attribute_ns_xmlns.phpt`: DOMElement::{get,has}AttributeNS() with xmlns
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domelement_get_or_has_attribute_ns_xmlns() {
    if skip_without_xml_native("test_domelement_get_or_has_attribute_ns_xmlns") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML('<root xmlns:a="urn:a"/>');

var_dump($dom->documentElement->getAttributeNS('http://www.w3.org/2000/xmlns/', 'a'));
var_dump($dom->documentElement->getAttributeNS('http://www.w3.org/2000/xmlns/', 'b'));
var_dump($dom->documentElement->hasAttributeNS('http://www.w3.org/2000/xmlns/', 'a'));
var_dump($dom->documentElement->hasAttributeNS('http://www.w3.org/2000/xmlns/', 'b'));

"#);
    assert_exact(&out, r#"string(5) "urn:a"
string(0) ""
bool(true)
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMElement_hasAttributes_basic.phpt`: DOMNode: hasAttributes()
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_domelement_has_attributes_basic() {
    if skip_without_xml_native("test_domelement_has_attributes_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$dom = new DOMDocument;
$dom->loadXML($xmlstr);
if(!$dom) {
  echo "Error while parsing the document\n";
  exit(0);
}

$element = $dom->documentElement;

echo "Verify that we have a DOMElement object:\n";
echo get_class($element), "\n";

echo "\nElement should have attributes:\n";
var_dump($element->hasAttributes());

$nodelist=$dom->getElementsByTagName('tbody') ;
$element = $nodelist->item(0);

echo "\nVerify that we have a DOMElement object:\n";
echo get_class($element), "\n";

echo "\nElement should have no attributes:\n";
var_dump($element->hasAttributes());


"#);
    assert_exact(&out, r#"Verify that we have a DOMElement object:
DOMElement

Element should have attributes:
bool(true)

Verify that we have a DOMElement object:
DOMElement

Element should have no attributes:
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMElement_id.phpt`: DOMElement::id
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::getElementById"#]
fn test_domelement_id() {
    if skip_without_xml_native("test_domelement_id") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class MyStringable {
    public function __toString(): string {
        throw new Exception("foo");
    }
}

$dom = new DOMDocument();
$dom->loadXML('<html><div/></html>');
$div = $dom->documentElement->firstChild;

var_dump($div->id);
$div->id = "hello & world<>";
var_dump($div->id);
$div->id = "";
var_dump($div->id);
$div->id = "é";
var_dump($div->id);
$div->id = "\0";
var_dump($div->id);
$div->id = 12345;
var_dump($div->id);
try {
    $div->id = new MyStringable();
} catch (Throwable $e) {
    echo "Error: ", $e->getMessage(), "\n";
}
var_dump($div->id);
echo $dom->saveXML();

var_dump($dom->getElementById("12345") === $div);

"#);
    assert_exact(&out, r#"string(0) ""
string(15) "hello & world<>"
string(0) ""
string(2) "é"
string(0) ""
string(5) "12345"
Error: foo
string(5) "12345"
<?xml version="1.0"?>
<html><div id="12345"/></html>
bool(true)"#);
}

/// php-src `ext/dom/tests/DOMElement_insertAdjacentElement.phpt`: DOMElement::insertAdjacentElement()
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMElement::insertAdjacentElement"#]
fn test_domelement_insert_adjacent_element() {
    if skip_without_xml_native("test_domelement_insert_adjacent_element") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML('<?xml version="1.0"?><container><p>foo</p></container>');
$container = $dom->documentElement;
$p = $container->firstElementChild;

echo "--- Edge cases ---\n";

var_dump($dom->createElement('free')->insertAdjacentElement("beforebegin", $dom->createElement('element')));
var_dump($dom->createElement('free')->insertAdjacentElement("afterend", $dom->createElement('element')));

try {
    var_dump($dom->createElement('free')->insertAdjacentElement("bogus", $dom->createElement('element')));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Hierarchy test ---\n";

$element = $dom->createElement('free');
$child = $element->appendChild($dom->createElement('child'));
foreach (['beforebegin', 'afterbegin', 'beforeend', 'afterend'] as $where) {
    try {
        var_dump($child->insertAdjacentElement($where, $element)->tagName);
    } catch (DOMException $e) {
        echo $e->getMessage(), "\n";
    }
}

function testNormalCases($dom, $uppercase) {
    $container = $dom->documentElement;
    $p = $container->firstElementChild;
    $transform = fn ($s) => $uppercase ? strtoupper($s) : $s;

    var_dump($p->insertAdjacentElement($transform("beforebegin"), $dom->createElement('A'))->tagName);
    echo $dom->saveXML();

    var_dump($p->insertAdjacentElement($transform("afterbegin"), $dom->createElement('B'))->tagName);
    echo $dom->saveXML();

    var_dump($p->insertAdjacentElement($transform("beforeend"), $dom->createElement('C'))->tagName);
    echo $dom->saveXML();

    var_dump($p->insertAdjacentElement($transform("afterend"), $dom->createElement('D'))->tagName);
    echo $dom->saveXML();
}

echo "--- Normal cases uppercase ---\n";

testNormalCases(clone $dom, true);

echo "--- Normal cases lowercase ---\n";

testNormalCases($dom, false);

$empty = $dom->createElement('empty');
var_dump($empty->insertAdjacentElement("afterbegin", $dom->createElement('A'))->tagName);
echo $dom->saveXML($empty), "\n";

echo "--- Namespace test ---\n";

$dom->loadXML('<?xml version="1.0"?><container xmlns:foo="some:ns"/>');
$dom->documentElement->insertAdjacentElement("afterbegin", $dom->createElementNS("some:ns", "bar"));
echo $dom->saveXML();

echo "--- Two document test ---\n";

$dom1 = new DOMDocument();
$dom1->loadXML('<?xml version="1.0"?><container><div/></container>');
$dom2 = new DOMDocument();
$dom2->loadXML('<?xml version="1.0"?><container><p/></container>');
$dom1->documentElement->firstChild->insertAdjacentElement('afterbegin', $dom2->documentElement->firstChild);
echo $dom1->saveXML();
echo $dom2->saveXML();

"#);
    assert_exact(&out, r#"--- Edge cases ---
NULL
NULL
Syntax Error
--- Hierarchy test ---
Hierarchy Request Error
Hierarchy Request Error
Hierarchy Request Error
Hierarchy Request Error
--- Normal cases uppercase ---
string(1) "A"
<?xml version="1.0"?>
<container><A/><p>foo</p></container>
string(1) "B"
<?xml version="1.0"?>
<container><A/><p><B/>foo</p></container>
string(1) "C"
<?xml version="1.0"?>
<container><A/><p><B/>foo<C/></p></container>
string(1) "D"
<?xml version="1.0"?>
<container><A/><p><B/>foo<C/></p><D/></container>
--- Normal cases lowercase ---
string(1) "A"
<?xml version="1.0"?>
<container><A/><p>foo</p></container>
string(1) "B"
<?xml version="1.0"?>
<container><A/><p><B/>foo</p></container>
string(1) "C"
<?xml version="1.0"?>
<container><A/><p><B/>foo<C/></p></container>
string(1) "D"
<?xml version="1.0"?>
<container><A/><p><B/>foo<C/></p><D/></container>
string(1) "A"
<empty><A/></empty>
--- Namespace test ---
<?xml version="1.0"?>
<container xmlns:foo="some:ns"><foo:bar/></container>
--- Two document test ---
<?xml version="1.0"?>
<container><div><p/></div></container>
<?xml version="1.0"?>
<container/>"#);
}

/// php-src `ext/dom/tests/DOMElement_insertAdjacentText.phpt`: DOMElement::insertAdjacentText()
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMElement::insertAdjacentText"#]
fn test_domelement_insert_adjacent_text() {
    if skip_without_xml_native("test_domelement_insert_adjacent_text") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML('<?xml version="1.0"?><container><p>foo</p></container>');

echo "--- Edge cases ---\n";

try {
    $dom->createElement('free')->insertAdjacentText("bogus", "bogus");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

function testNormalCases($dom, $uppercase) {
    $container = $dom->documentElement;
    $p = $container->firstElementChild;
    $transform = fn ($s) => $uppercase ? strtoupper($s) : $s;

    $p->insertAdjacentText("beforebegin", 'A');
    echo $dom->saveXML();

    $p->insertAdjacentText("afterbegin", 'B');
    echo $dom->saveXML();

    $p->insertAdjacentText("beforeend", 'C');
    echo $dom->saveXML();

    $p->insertAdjacentText("afterend", 'D');
    echo $dom->saveXML();
}

echo "--- Normal cases uppercase ---\n";

testNormalCases(clone $dom, true);

echo "--- Normal cases lowercase ---\n";

testNormalCases($dom, false);

echo "--- Normal cases starting from empty element ---\n";

$empty = $dom->createElement('empty');
$empty->insertAdjacentText("afterbegin", 'A');
echo $dom->saveXML($empty), "\n";

$AText = $empty->firstChild;
$empty->insertAdjacentText("afterbegin", 'B');
echo $dom->saveXML($empty), "\n";
var_dump($AText->textContent);

"#);
    assert_exact(&out, r#"--- Edge cases ---
Syntax Error
--- Normal cases uppercase ---
<?xml version="1.0"?>
<container>A<p>foo</p></container>
<?xml version="1.0"?>
<container>A<p>Bfoo</p></container>
<?xml version="1.0"?>
<container>A<p>BfooC</p></container>
<?xml version="1.0"?>
<container>A<p>BfooC</p>D</container>
--- Normal cases lowercase ---
<?xml version="1.0"?>
<container>A<p>foo</p></container>
<?xml version="1.0"?>
<container>A<p>Bfoo</p></container>
<?xml version="1.0"?>
<container>A<p>BfooC</p></container>
<?xml version="1.0"?>
<container>A<p>BfooC</p>D</container>
--- Normal cases starting from empty element ---
<empty>A</empty>
<empty>BA</empty>
string(1) "A""#);
}

/// php-src `ext/dom/tests/DOMElement_prefix_empty.phpt`: DOMElement->prefix with empty string creates bogus prefix
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domelement_prefix_empty() {
    if skip_without_xml_native("test_domelement_prefix_empty") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML("<hello:container xmlns:conflict=\"urn:foo\" xmlns:hello=\"http://www.w3.org/1999/xhtml\"/>");

$container = $dom->documentElement;

echo "--- Changing the prefix to an empty string ---\n";

$container->prefix = "";
echo $dom->saveXML();

echo "--- Changing the prefix to an empty C-style string ---\n";

$container->prefix = "\0foobar";
echo $dom->saveXML();

echo "--- Changing the prefix to \"hello\" ---\n";

$container->prefix = "hello";
echo $dom->saveXML();

echo "--- Changing the prefix to that of a conflicting namespace (\"conflict\") ---\n";

try {
    $container->prefix = "conflict";
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveXML();

"#);
    assert_exact(&out, r#"--- Changing the prefix to an empty string ---
<?xml version="1.0"?>
<container xmlns:conflict="urn:foo" xmlns:hello="http://www.w3.org/1999/xhtml" xmlns="http://www.w3.org/1999/xhtml"/>
--- Changing the prefix to an empty C-style string ---
<?xml version="1.0"?>
<container xmlns:conflict="urn:foo" xmlns:hello="http://www.w3.org/1999/xhtml" xmlns="http://www.w3.org/1999/xhtml"/>
--- Changing the prefix to "hello" ---
<?xml version="1.0"?>
<hello:container xmlns:conflict="urn:foo" xmlns:hello="http://www.w3.org/1999/xhtml" xmlns="http://www.w3.org/1999/xhtml"/>
--- Changing the prefix to that of a conflicting namespace ("conflict") ---
Namespace Error
<?xml version="1.0"?>
<hello:container xmlns:conflict="urn:foo" xmlns:hello="http://www.w3.org/1999/xhtml" xmlns="http://www.w3.org/1999/xhtml"/>"#);
}

/// php-src `ext/dom/tests/DOMElement_prepend_hierarchy_test.phpt`: DOMElement::prepend() with hierarchy changes and errors
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::firstChild"#]
fn test_domelement_prepend_hierarchy_test() {
    if skip_without_xml_native("test_domelement_prepend_hierarchy_test") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom_original = new DOMDocument;
$dom_original->loadXML('<p><b>hello</b><b><i>world</i></b></p>');

echo "-- Prepend hello with world --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
$b_world = $b_hello->nextSibling;
$b_hello->prepend($b_world);
var_dump($dom->saveHTML());

echo "-- Prepend hello with world's child --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
$b_world = $b_hello->nextSibling;
$b_hello->prepend($b_world->firstChild);
var_dump($dom->saveHTML());

echo "-- Prepend hello with world's child and text --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
$b_world = $b_hello->nextSibling;
$b_hello->prepend($b_world->firstChild, "foo");
var_dump($dom->saveHTML());

echo "-- Prepend world's child with hello --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
$b_world = $b_hello->nextSibling;
$b_world->firstChild->prepend($b_hello);
var_dump($dom->saveHTML());

echo "-- Prepend world's child with hello and text --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
$b_world = $b_hello->nextSibling;
$b_world->firstChild->prepend($b_hello, "foo");
var_dump($dom->saveHTML());

echo "-- Prepend hello with itself --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
try {
    $b_hello->prepend($b_hello);
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}
var_dump($dom->saveHTML());

echo "-- Prepend hello with itself and text --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
try {
    $b_hello->prepend($b_hello, "foo");
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}
var_dump($dom->saveHTML());

echo "-- Prepend world's i tag with the parent --\n";
$dom = clone $dom_original;
$b_hello = $dom->firstChild->firstChild;
$b_world = $b_hello->nextSibling;
try {
    $b_world->firstChild->prepend($b_world);
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}
var_dump($dom->saveHTML());

echo "-- Append from another document --\n";
$dom = clone $dom_original;
$dom2 = new DOMDocument;
$dom2->loadXML('<p>other</p>');
try {
    $dom->firstChild->firstChild->prepend($dom2->firstChild);
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}
var_dump($dom2->saveHTML());
var_dump($dom->saveHTML());

"#);
    assert_exact(&out, r#"-- Prepend hello with world --
string(39) "<p><b><b><i>world</i></b>hello</b></p>
"
-- Prepend hello with world's child --
string(39) "<p><b><i>world</i>hello</b><b></b></p>
"
-- Prepend hello with world's child and text --
string(42) "<p><b><i>world</i>foohello</b><b></b></p>
"
-- Prepend world's child with hello --
string(39) "<p><b><i><b>hello</b>world</i></b></p>
"
-- Prepend world's child with hello and text --
string(42) "<p><b><i><b>hello</b>fooworld</i></b></p>
"
-- Prepend hello with itself --
Hierarchy Request Error
string(39) "<p><b>hello</b><b><i>world</i></b></p>
"
-- Prepend hello with itself and text --
Hierarchy Request Error
string(27) "<p><b><i>world</i></b></p>
"
-- Prepend world's i tag with the parent --
Hierarchy Request Error
string(39) "<p><b>hello</b><b><i>world</i></b></p>
"
-- Append from another document --
Wrong Document Error
string(13) "<p>other</p>
"
string(39) "<p><b>hello</b><b><i>world</i></b></p>
""#);
}

/// php-src `ext/dom/tests/DOMElement_replaceChildren.phpt`: DOMParentNode::replaceChildren()
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createDocumentFragment"#]
fn test_domelement_replace_children() {
    if skip_without_xml_native("test_domelement_replace_children") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadHTML('<!DOCTYPE HTML><html><p>hi</p> test <p>hi2</p></html>');

echo "--- Edge cases ---\n";

try {
    $dom->documentElement->replaceChildren($dom->documentElement);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

try {
    $dom->documentElement->firstElementChild->replaceChildren($dom->documentElement);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Normal cases ---\n";

$dom->documentElement->replaceChildren('foo', $dom->createElement('p'), 'bar');
echo $dom->saveXML();

$fragment1 = $dom->createDocumentFragment();
$fragment1->appendChild($dom->createElement('a'));
$fragment1->appendChild($dom->createElement('b'));
$fragment2 = $dom->createDocumentFragment();
$fragment2->append('text');
$fragment3 = $dom->createDocumentFragment();
$dom->documentElement->replaceChildren($fragment1, $fragment2, $fragment3);
echo $dom->saveXML();

echo "--- Fragment case ---\n";

$fragment = $dom->createDocumentFragment();
$fragment->replaceChildren();
var_dump($dom->saveXML($fragment));

$old = $fragment->appendChild($dom->createElement('p', 'test'));
$fragment->replaceChildren($dom->createElement('b', 'test'));
echo $dom->saveXML($fragment), "\n";
var_dump($old->nodeValue);

echo "--- Idempotent case ---\n";

$dom->replaceChildren($dom->documentElement);
echo $dom->saveXML();

echo "--- Removal shortcut ---\n";

$dom->documentElement->replaceChildren();
echo $dom->saveXML();

"#);
    assert_exact(&out, r#"--- Edge cases ---
Hierarchy Request Error
Hierarchy Request Error
--- Normal cases ---
<?xml version="1.0" standalone="yes"?>
<!DOCTYPE HTML>
<html>foo<p/>bar</html>
<?xml version="1.0" standalone="yes"?>
<!DOCTYPE HTML>
<html><a/><b/>text</html>
--- Fragment case ---
string(0) ""
<b>test</b>
string(4) "test"
--- Idempotent case ---
<?xml version="1.0" standalone="yes"?>
<html><a/><b/>text</html>
--- Removal shortcut ---
<?xml version="1.0" standalone="yes"?>
<html/>"#);
}

/// php-src `ext/dom/tests/DOMElement_setAttributeNS_prefix_conflict.phpt`: DOMElement::setAttributeNS() with prefix name conflict
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domelement_set_attribute_ns_prefix_conflict() {
    if skip_without_xml_native("test_domelement_set_attribute_ns_prefix_conflict") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
echo "--- Non-default namespace test case ---\n";

$dom = new DOMDocument();
$dom->loadXML('<?xml version="1.0"?><container xmlns:foo="http://php.net" foo:bar="yes"/>');
$dom->documentElement->setAttributeNS('http://php.net/2', 'foo:bar', 'no1');
echo $dom->saveXML();
$dom->documentElement->setAttributeNS('http://php.net/2', 'bar', 'no2');
echo $dom->saveXML();

echo "--- Default namespace test case ---\n";

$dom = new DOMDocument();
$dom->loadXML('<?xml version="1.0"?><container xmlns="http://php.net" bar="yes"/>');
$dom->documentElement->setAttributeNS('http://php.net/2', 'bar', 'no1');
echo $dom->saveXML();
$dom->documentElement->setAttributeNS('http://php.net/2', 'bar', 'no2');
echo $dom->saveXML();
"#);
    assert_exact(&out, r#"--- Non-default namespace test case ---
<?xml version="1.0"?>
<container xmlns:foo="http://php.net" xmlns:default="http://php.net/2" foo:bar="yes" default:bar="no1"/>
<?xml version="1.0"?>
<container xmlns:foo="http://php.net" xmlns:default="http://php.net/2" foo:bar="yes" default:bar="no2"/>
--- Default namespace test case ---
<?xml version="1.0"?>
<container xmlns="http://php.net" xmlns:default="http://php.net/2" bar="yes" default:bar="no1"/>
<?xml version="1.0"?>
<container xmlns="http://php.net" xmlns:default="http://php.net/2" bar="yes" default:bar="no2"/>"#);
}

/// php-src `ext/dom/tests/DOMElement_toggleAttribute.phpt`: DOMElement::toggleAttribute()
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::saveHTML"#]
fn test_domelement_toggle_attribute() {
    if skip_without_xml_native("test_domelement_toggle_attribute") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$html = new DOMDocument();
$html->loadHTML('<!DOCTYPE HTML><html id="test"></html>');
$xml = new DOMDocument();
$xml->loadXML('<?xml version="1.0"?><html id="test"></html>');

try {
    var_dump($html->documentElement->toggleAttribute("\0"));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Selected attribute tests (HTML) ---\n";

var_dump($html->documentElement->toggleAttribute("SELECTED", false));
echo $html->saveHTML();
var_dump($html->documentElement->toggleAttribute("SELECTED"));
echo $html->saveHTML();
var_dump($html->documentElement->toggleAttribute("selected", true));
echo $html->saveHTML();
var_dump($html->documentElement->toggleAttribute("selected"));
echo $html->saveHTML();

echo "--- Selected attribute tests (XML) ---\n";

var_dump($xml->documentElement->toggleAttribute("SELECTED", false));
echo $xml->saveXML();
var_dump($xml->documentElement->toggleAttribute("SELECTED"));
echo $xml->saveXML();
var_dump($xml->documentElement->toggleAttribute("selected", true));
echo $xml->saveXML();
var_dump($xml->documentElement->toggleAttribute("selected"));
echo $xml->saveXML();

echo "--- id attribute tests ---\n";

var_dump($html->getElementById("test") === NULL);
var_dump($html->documentElement->toggleAttribute("id"));
var_dump($html->getElementById("test") === NULL);

echo "--- Namespace tests ---\n";

$dom = new DOMDocument();
$dom->loadXML("<?xml version='1.0'?><container xmlns='some:ns' xmlns:foo='some:ns2' xmlns:anotherone='some:ns3'><foo:bar/><baz/></container>");

echo "Toggling namespaces:\n";
var_dump($dom->documentElement->toggleAttribute('xmlns'));
echo $dom->saveXML();
var_dump($dom->documentElement->toggleAttribute('xmlns:anotherone'));
echo $dom->saveXML();
var_dump($dom->documentElement->toggleAttribute('xmlns:anotherone'));
echo $dom->saveXML();
var_dump($dom->documentElement->toggleAttribute('xmlns:foo'));
echo $dom->saveXML();
var_dump($dom->documentElement->toggleAttribute('xmlns:nope', false));
echo $dom->saveXML();

echo "Toggling namespaced attributes:\n";
var_dump($dom->documentElement->toggleAttribute('test:test'));
var_dump($dom->documentElement->firstElementChild->toggleAttribute('foo:test'));
var_dump($dom->documentElement->firstElementChild->toggleAttribute('doesnotexist:test'));
var_dump($dom->documentElement->firstElementChild->toggleAttribute('doesnotexist:test2', false));
echo $dom->saveXML();

echo "namespace of test:test = ";
var_dump($dom->documentElement->getAttributeNode('test:test')->namespaceURI);
echo "namespace of foo:test = ";
var_dump($dom->documentElement->firstElementChild->getAttributeNode('foo:test')->namespaceURI);
echo "namespace of doesnotexist:test = ";
var_dump($dom->documentElement->firstElementChild->getAttributeNode('doesnotexist:test')->namespaceURI);

echo "Toggling namespaced attributes:\n";
var_dump($dom->documentElement->toggleAttribute('test:test'));
var_dump($dom->documentElement->firstElementChild->toggleAttribute('foo:test'));
var_dump($dom->documentElement->firstElementChild->toggleAttribute('doesnotexist:test'));
var_dump($dom->documentElement->firstElementChild->toggleAttribute('doesnotexist:test2', true));
var_dump($dom->documentElement->firstElementChild->toggleAttribute('doesnotexist:test3', false));
echo $dom->saveXML();

echo "Checking toggled namespace:\n";
var_dump($dom->documentElement->getAttribute('xmlns:anotheron'));

"#);
    assert_format(&out, r#"Invalid Character Error
--- Selected attribute tests (HTML) ---
bool(false)
<!DOCTYPE HTML>
<html id="test"></html>
bool(true)
<!DOCTYPE HTML>
<html id="test" selected%r(="")?%r></html>
bool(true)
<!DOCTYPE HTML>
<html id="test" selected%r(="")?%r></html>
bool(false)
<!DOCTYPE HTML>
<html id="test"></html>
--- Selected attribute tests (XML) ---
bool(false)
<?xml version="1.0"?>
<html id="test"/>
bool(true)
<?xml version="1.0"?>
<html id="test" SELECTED=""/>
bool(true)
<?xml version="1.0"?>
<html id="test" SELECTED="" selected=""/>
bool(false)
<?xml version="1.0"?>
<html id="test" SELECTED=""/>
--- id attribute tests ---
bool(false)
bool(false)
bool(true)
--- Namespace tests ---
Toggling namespaces:
bool(false)
<?xml version="1.0"?>
<container xmlns:foo="some:ns2" xmlns:anotherone="some:ns3" xmlns="some:ns"><foo:bar/><baz/></container>
bool(false)
<?xml version="1.0"?>
<container xmlns:foo="some:ns2" xmlns="some:ns"><foo:bar/><baz/></container>
bool(true)
<?xml version="1.0"?>
<container xmlns:foo="some:ns2" xmlns="some:ns" xmlns:anotherone=""><foo:bar/><baz/></container>
bool(false)
<?xml version="1.0"?>
<container xmlns="some:ns" xmlns:anotherone=""><foo:bar xmlns:foo="some:ns2"/><baz/></container>
bool(false)
<?xml version="1.0"?>
<container xmlns="some:ns" xmlns:anotherone=""><foo:bar xmlns:foo="some:ns2"/><baz/></container>
Toggling namespaced attributes:
bool(true)
bool(true)
bool(true)
bool(false)
<?xml version="1.0"?>
<container xmlns="some:ns" xmlns:anotherone="" test:test=""><foo:bar xmlns:foo="some:ns2" foo:test="" doesnotexist:test=""/><baz/></container>
namespace of test:test = NULL
namespace of foo:test = string(8) "some:ns2"
namespace of doesnotexist:test = NULL
Toggling namespaced attributes:
bool(false)
bool(false)
bool(false)
bool(true)
bool(false)
<?xml version="1.0"?>
<container xmlns="some:ns" xmlns:anotherone=""><foo:bar xmlns:foo="some:ns2" doesnotexist:test2=""/><baz/></container>
Checking toggled namespace:
string(0) """#);
}

/// php-src `ext/dom/tests/DOMElement_toggleAttribute_without_document.phpt`: DOMElement::toggleAttribute() without a document
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMElement::__construct' expects 0 arguments, got 1"#]
fn test_domelement_toggle_attribute_without_document() {
    if skip_without_xml_native("test_domelement_toggle_attribute_without_document") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$element = new DOMElement("container");
$element->toggleAttribute('foo', true);

$dom = new DOMDocument;
$element = $dom->importNode($element, true);
echo $dom->saveXML($element), "\n";

"#);
    assert_exact(&out, r#"<container foo=""/>"#);
}

/// php-src `ext/dom/tests/DOMEntityReference_basic.phpt`: DOMEntityReference - read $nodeName property
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMEntityReference"#]
fn test_domentity_reference_basic() {
    if skip_without_xml_native("test_domentity_reference_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$der = new DOMEntityReference('nbsp');
var_dump($der->nodeName);
"#);
    assert_exact(&out, r#"string(4) "nbsp""#);
}

/// php-src `ext/dom/tests/DOMEntityReference_error1.phpt`: DOMEntityReference - DOM_INVALID_CHARACTER_ERR raised if name contains an invalid character
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMEntityReference"#]
fn test_domentity_reference_error1() {
    if skip_without_xml_native("test_domentity_reference_error1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
try {
    new DOMEntityReference('!');
} catch (DOMException $e) {
    var_dump($e->getCode() === DOM_INVALID_CHARACTER_ERR);
    echo $e->getMessage();
}
"#);
    assert_exact(&out, r#"bool(true)
Invalid Character Error"#);
}

/// php-src `ext/dom/tests/DOMEntityReference_predefined_free.phpt`: Freeing of a predefined DOMEntityReference
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMEntityReference"#]
fn test_domentity_reference_predefined_free() {
    if skip_without_xml_native("test_domentity_reference_predefined_free") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$ref = new DOMEntityReference("amp");
var_dump($ref);
"#);
    assert_format(&out, r#"object(DOMEntityReference)#1 (17) {
  ["nodeName"]=>
  string(3) "amp"
  ["nodeValue"]=>
  NULL
  ["nodeType"]=>
  int(5)
  ["parentNode"]=>
  NULL
  ["parentElement"]=>
  NULL
  ["childNodes"]=>
  string(22) "(object value omitted)"
  ["firstChild"]=>
  string(22) "(object value omitted)"
  ["lastChild"]=>
  string(22) "(object value omitted)"
  ["previousSibling"]=>
  NULL
  ["nextSibling"]=>
  NULL
  ["attributes"]=>
  NULL
  ["isConnected"]=>
  bool(false)
  ["namespaceURI"]=>
  NULL
  ["prefix"]=>
  string(0) ""
  ["localName"]=>
  NULL
  ["baseURI"]=>
  NULL
  ["textContent"]=>
  string(%d) "%S"
}"#);
}

/// php-src `ext/dom/tests/DOMEntity_fields.phpt`: DOMEntity fields
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_domentity_fields() {
    if skip_without_xml_native("test_domentity_fields") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xmlString = <<<XML
<?xml version="1.0"?>
<!DOCTYPE test [
    <!ENTITY sampleInternalEntity "This is a sample entity value.">
    <!ENTITY sampleExternalSystemWithNotationName SYSTEM "external.stuff" NDATA stuff>
    <!ENTITY sampleExternalSystemWithoutNotationName SYSTEM "external.stuff" NDATA >
    <!ENTITY sampleExternalPublicWithNotationName1 PUBLIC "public id" "external.stuff" NDATA stuff>
    <!ENTITY sampleExternalPublicWithNotationName2 PUBLIC "" "external.stuff" NDATA stuff>
    <!ENTITY sampleExternalPublicWithoutNotationName1 PUBLIC "public id" "external.stuff" NDATA >
    <!ENTITY sampleExternalPublicWithoutNotationName2 PUBLIC "" "external.stuff" NDATA >
]>
<root/>
XML;

$dom = new DOMDocument();
$dom->loadXML($xmlString);

// Sort them, the iteration order isn't defined
$entities = iterator_to_array($dom->doctype->entities);
ksort($entities);

foreach ($entities as $entity) {
    echo "Entity name: {$entity->nodeName}\n";
    echo "publicId: ";
    var_dump($entity->publicId);
    echo "systemId: ";
    var_dump($entity->systemId);
    echo "notationName: ";
    var_dump($entity->notationName);
    echo "actualEncoding: ";
    var_dump($entity->actualEncoding);
    echo "encoding: ";
    var_dump($entity->encoding);
    echo "version: ";
    var_dump($entity->version);
    echo "\n";
}
"#);
    assert_format(&out, r#"Entity name: sampleExternalPublicWithNotationName1
publicId: string(9) "public id"
systemId: string(14) "external.stuff"
notationName: string(5) "stuff"
actualEncoding: 
Deprecated: Property DOMEntity::$actualEncoding is deprecated in %s on line %d
NULL
encoding: 
Deprecated: Property DOMEntity::$encoding is deprecated in %s on line %d
NULL
version: 
Deprecated: Property DOMEntity::$version is deprecated in %s on line %d
NULL

Entity name: sampleExternalPublicWithNotationName2
publicId: string(0) ""
systemId: string(14) "external.stuff"
notationName: string(5) "stuff"
actualEncoding: 
Deprecated: Property DOMEntity::$actualEncoding is deprecated in %s on line %d
NULL
encoding: 
Deprecated: Property DOMEntity::$encoding is deprecated in %s on line %d
NULL
version: 
Deprecated: Property DOMEntity::$version is deprecated in %s on line %d
NULL

Entity name: sampleExternalPublicWithoutNotationName1
publicId: string(9) "public id"
systemId: string(14) "external.stuff"
notationName: string(0) ""
actualEncoding: 
Deprecated: Property DOMEntity::$actualEncoding is deprecated in %s on line %d
NULL
encoding: 
Deprecated: Property DOMEntity::$encoding is deprecated in %s on line %d
NULL
version: 
Deprecated: Property DOMEntity::$version is deprecated in %s on line %d
NULL

Entity name: sampleExternalPublicWithoutNotationName2
publicId: string(0) ""
systemId: string(14) "external.stuff"
notationName: string(0) ""
actualEncoding: 
Deprecated: Property DOMEntity::$actualEncoding is deprecated in %s on line %d
NULL
encoding: 
Deprecated: Property DOMEntity::$encoding is deprecated in %s on line %d
NULL
version: 
Deprecated: Property DOMEntity::$version is deprecated in %s on line %d
NULL

Entity name: sampleExternalSystemWithNotationName
publicId: NULL
systemId: string(14) "external.stuff"
notationName: string(5) "stuff"
actualEncoding: 
Deprecated: Property DOMEntity::$actualEncoding is deprecated in %s on line %d
NULL
encoding: 
Deprecated: Property DOMEntity::$encoding is deprecated in %s on line %d
NULL
version: 
Deprecated: Property DOMEntity::$version is deprecated in %s on line %d
NULL

Entity name: sampleExternalSystemWithoutNotationName
publicId: NULL
systemId: string(14) "external.stuff"
notationName: string(0) ""
actualEncoding: 
Deprecated: Property DOMEntity::$actualEncoding is deprecated in %s on line %d
NULL
encoding: 
Deprecated: Property DOMEntity::$encoding is deprecated in %s on line %d
NULL
version: 
Deprecated: Property DOMEntity::$version is deprecated in %s on line %d
NULL

Entity name: sampleInternalEntity
publicId: NULL
systemId: NULL
notationName: NULL
actualEncoding: 
Deprecated: Property DOMEntity::$actualEncoding is deprecated in %s on line %d
NULL
encoding: 
Deprecated: Property DOMEntity::$encoding is deprecated in %s on line %d
NULL
version: 
Deprecated: Property DOMEntity::$version is deprecated in %s on line %d
NULL"#);
}

/// php-src `ext/dom/tests/DOMImplementation_createDocumentType_basic.phpt`: DOMImplementation::createDocumentType()
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMImplementation"#]
fn test_domimplementation_create_document_type_basic() {
    if skip_without_xml_native("test_domimplementation_create_document_type_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$imp = new DOMImplementation();
$doctype = $imp->createDocumentType("html",
    "-//W3C//DTD XHTML 1.0 Strict//EN",
    "http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd");
$doc = $imp->createDocument(null, 'html', $doctype);
echo $doc->saveHTML();

$doctype = $imp->createDocumentType("html");
$doc = $imp->createDocument(null, 'html', $doctype);
echo $doc->saveHTML();

$doctype = $imp->createDocumentType("html", "", "");
$doc = $imp->createDocument(null, 'html', $doctype);
echo $doc->saveHTML();

"#);
    assert_exact(&out, r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Strict//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd">
<html></html>
<!DOCTYPE html>
<html></html>
<!DOCTYPE html>
<html></html>"#);
}

/// php-src `ext/dom/tests/DOMImplementation_createDocument_basic.phpt`: DOMImplementation::createDocument()
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMImplementation"#]
fn test_domimplementation_create_document_basic() {
    if skip_without_xml_native("test_domimplementation_create_document_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
declare(strict_types=1);
$x = new DOMImplementation();
$doc = $x->createDocument(null, 'html');
echo $doc->saveHTML();
"#);
    assert_exact(&out, r#"<html></html>"#);
}

/// php-src `ext/dom/tests/DOMImplementation_hasFeature_basic.phpt`: DOMImplementation::hasFeature()
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMImplementation"#]
fn test_domimplementation_has_feature_basic() {
    if skip_without_xml_native("test_domimplementation_has_feature_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$imp = new DOMImplementation();
var_dump($imp->hasFeature('Core', '1.0'));
var_dump($imp->hasFeature('XML', '2.0'));
"#);
    assert_exact(&out, r#"bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/DOMNamedNodeMap_count.phpt`: Test count nodes in DOMNamedNodeMap
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMElement::attributes"#]
fn test_domnamed_node_map_count() {
    if skip_without_xml_native("test_domnamed_node_map_count") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$document = new DomDocument();
$root = $document->createElement('root');
$document->appendChild($root);
for($i = 0; $i < 5; $i++) {
    $root->setAttribute('attribute-' . $i, 'value-' . $i);
}
for($i = 0; $i < 7; $i++) {
    $item = $document->createElement('item');
    $root->appendChild($item);
}
var_dump($root->attributes->length);
var_dump($root->attributes->count());
var_dump(count($root->attributes));

"#);
    assert_exact(&out, r#"int(5)
int(5)
int(5)"#);
}

/// php-src `ext/dom/tests/DOMNamedNodeMap_edge_case_offset.phpt`: Test edge case offsets in DOMNamedNodeMap
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMElement::attributes"#]
fn test_domnamed_node_map_edge_case_offset() {
    if skip_without_xml_native("test_domnamed_node_map_edge_case_offset") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$document = new DomDocument();
$root = $document->createElement('root');
$document->appendChild($root);
$root->setAttribute('attrib', 'value');
var_dump($root->attributes->length);
// Consistent with the method call
try {
    var_dump($root->attributes[-1]);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $root->attributes[][] = null;
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"int(1)
must be between 0 and 2147483647
Cannot access DOMNamedNodeMap without offset"#);
}

/// php-src `ext/dom/tests/DOMNamedNodeMap_string_references.phpt`: DOMNamedNodeMap string references
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domnamed_node_map_string_references() {
    if skip_without_xml_native("test_domnamed_node_map_string_references") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML('<a href="hi" foo="bar"/>');

$attributes = $dom->documentElement->attributes;

var_dump(isset($attributes['href']), $attributes['href']->value);

var_dump(isset($attributes['foo']), $attributes['foo']->value);

$str = 'href';
$ref =& $str;
var_dump(isset($attributes[$ref]), $attributes[$ref]->value);

$str = 'foo';
$ref =& $str;
var_dump(isset($attributes[$ref]), $attributes[$ref]->value);

$str = 'this does not exist';
$ref =& $str;
var_dump(isset($attributes[$ref]), $attributes[$ref]);

$str = '0';
$ref =& $str;
var_dump(isset($attributes[$ref]), $attributes[$ref]->value);

$str = '1';
$ref =& $str;
var_dump(isset($attributes[$ref]), $attributes[$ref]->value);

$int = 0;
$ref =& $int;
var_dump(isset($attributes[$ref]), $attributes[$ref]->value);

$int = 1;
$ref =& $int;
var_dump(isset($attributes[$ref]), $attributes[$ref]->value);

"#);
    assert_exact(&out, r#"bool(true)
string(2) "hi"
bool(true)
string(3) "bar"
bool(true)
string(2) "hi"
bool(true)
string(3) "bar"
bool(false)
NULL
bool(true)
string(2) "hi"
bool(true)
string(3) "bar"
bool(true)
string(2) "hi"
bool(true)
string(3) "bar""#);
}

/// php-src `ext/dom/tests/DOMNode_C14NFile_basic.phpt`: DOMNode::C14NFile()
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMNode::C14NFile"#]
fn test_domnode_c14_nfile_basic() {
    if skip_without_xml_native("test_domnode_c14_nfile_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<< XML
<?xml version="1.0" ?>
<books>
 <book>
  <title>The Grapes of Wrath</title>
  <author>John Steinbeck</author>
 </book>
 <book>
  <title>The Pearl</title>
  <author>John Steinbeck</author>
 </book>
</books>
XML;

$output = __DIR__.'/DOMNode_C14NFile_basic.tmp';
$doc = new DOMDocument();
$doc->loadXML($xml);
$node = $doc->getElementsByTagName('title')->item(0);
var_dump($node->C14NFile($output));
$content = file_get_contents($output);
var_dump($content);
try {
    var_dump($node->C14NFile($output, false, false, []));
} catch (\ValueError $e) {
    echo $e->getMessage() . \PHP_EOL;
}
try {
    var_dump($node->C14NFile($output, false, false, ['query' => []]));
} catch (\TypeError $e) {
    echo $e->getMessage() . \PHP_EOL;
}
"#);
    assert_exact(&out, r#"int(34)
string(34) "<title>The Grapes of Wrath</title>"
DOMNode::C14NFile(): Argument #4 ($xpath) must have a "query" key
DOMNode::C14NFile(): Argument #4 ($xpath) "query" option must be a string, array given"#);
}

/// php-src `ext/dom/tests/DOMNode_C14N_basic.phpt`: DOMNode::C14N()
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMNode::C14N"#]
fn test_domnode_c14_n_basic() {
    if skip_without_xml_native("test_domnode_c14_n_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<< XML
<?xml version="1.0" ?>
<books>
 <book>
  <title>The Grapes of Wrath</title>
  <author>John Steinbeck</author>
 </book>
 <book>
  <title>The Pearl</title>
  <author>John Steinbeck</author>
 </book>
</books>
XML;

$doc = new DOMDocument();
$doc->loadXML($xml);
$node = $doc->getElementsByTagName('title')->item(0);
var_dump($node->C14N());

try {
    var_dump($node->C14N(false, false, []));
} catch (\ValueError $e) {
    echo $e->getMessage() . \PHP_EOL;
}
try {
    var_dump($node->C14N(false, false, ['query' => []]));
} catch (\TypeError $e) {
    echo $e->getMessage() . \PHP_EOL;
}
"#);
    assert_exact(&out, r#"string(34) "<title>The Grapes of Wrath</title>"
DOMNode::C14N(): Argument #3 ($xpath) must have a "query" key
DOMNode::C14N(): Argument #3 ($xpath) "query" option must be a string, array given"#);
}

/// php-src `ext/dom/tests/DOMNode_C14N_references.phpt`: Test: Canonicalization - C14N() with references
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domnode_c14_n_references() {
    if skip_without_xml_native("test_domnode_c14_n_references") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
// Adapted from canonicalization.phpt

$xml = <<<EOXML
<?xml version="1.0" encoding="ISO-8859-1" ?>
<foo xmlns="http://www.example.com/ns/foo"
     xmlns:fubar="http://www.example.com/ns/fubar" xmlns:test="urn::test"><contain>
  <bar><test1 /></bar>
  <bar><test2 /></bar>
  <fubar:bar xmlns:fubar="http://www.example.com/ns/fubar"><test3 /></fubar:bar>
  <fubar:bar><test4 /></fubar:bar>
</contain>
</foo>
EOXML;

$dom = new DOMDocument();
$dom->loadXML($xml);
$doc = $dom->documentElement->firstChild;

$xpath = [
    'query' => '(//a:contain | //a:bar | .//namespace::*)',
    'namespaces' => ['a' => 'http://www.example.com/ns/foo'],
];
$prefixes = ['test'];

foreach ($xpath['namespaces'] as $k => &$v);
unset($v);
foreach ($xpath as $k => &$v);
unset($v);
foreach ($prefixes as $k => &$v);
unset($v);

echo $doc->C14N(true, false, $xpath, $prefixes);
"#);
    assert_exact(&out, r#"<contain xmlns="http://www.example.com/ns/foo" xmlns:test="urn::test"><bar></bar><bar></bar></contain>"#);
}

/// php-src `ext/dom/tests/DOMNode_DOMNameSpaceNode_isConnected.phpt`: DOMNode::isConnected and DOMNameSpaceNode::isConnected
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMElement::isConnected"#]
fn test_domnode_domname_space_node_is_connected() {
    if skip_without_xml_native("test_domnode_domname_space_node_is_connected") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML('<!DOCTYPE html><html><head/><body/></html>');

$docElement = $dom->documentElement;
$head = $docElement->firstChild;
$body = $head->nextSibling;

echo "--- Created element not connected yet ---\n";

$p = $dom->createElement('p');
var_dump($p->isConnected);

echo "--- Appending and checking connection isn't broken for parents ---\n";

$body->appendChild($p);
var_dump($body->isConnected);
var_dump($p->isConnected);
$document = $docElement->parentNode;
var_dump($document->isConnected);
var_dump($dom->doctype->isConnected);

echo "--- Indirect removal should set isConnected to false for affected nodes ---\n";

$body->remove();
var_dump($p->isConnected);
var_dump($docElement->isConnected);
var_dump($body->isConnected);
var_dump($head->isConnected);
var_dump($dom->doctype->isConnected);

echo "--- Empty document test ---\n";

$dom = new DOMDocument();
var_dump($dom->isConnected);

"#);
    assert_exact(&out, r#"--- Created element not connected yet ---
bool(false)
--- Appending and checking connection isn't broken for parents ---
bool(true)
bool(true)
bool(true)
bool(true)
--- Indirect removal should set isConnected to false for affected nodes ---
bool(false)
bool(true)
bool(false)
bool(true)
bool(true)
--- Empty document test ---
bool(true)"#);
}

/// php-src `ext/dom/tests/DOMNode_DOMNameSpaceNode_parentElement.phpt`: DOMNode::parentElement and DOMNameSpaceNode::parentElement
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_domnode_domname_space_node_parent_element() {
    if skip_without_xml_native("test_domnode_domname_space_node_parent_element") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<!DOCTYPE HTML>
<html>
    <body/>
</html>
XML);

echo "--- body test ---\n";

$body = $dom->documentElement->firstElementChild;
var_dump($body->parentNode->localName);
var_dump($body->parentElement->localName);

echo "--- document test ---\n";

var_dump(get_class($dom->documentElement->parentNode));
var_dump($dom->documentElement->parentElement);

var_dump(get_class($dom->doctype->parentNode));
var_dump($dom->doctype->parentElement);

echo "--- fragment test ---\n";

$fragment = $dom->createDocumentFragment();
$p = $fragment->appendChild($dom->createElement('p'));

var_dump(get_class($p->parentNode));
var_dump($p->parentElement);

$body->appendChild($fragment);

var_dump($p->parentNode->localName);
var_dump($p->parentElement->localName);

"#);
    assert_exact(&out, r#"--- body test ---
string(4) "html"
string(4) "html"
--- document test ---
string(11) "DOMDocument"
NULL
string(11) "DOMDocument"
NULL
--- fragment test ---
string(19) "DOMDocumentFragment"
NULL
string(4) "body"
string(4) "body""#);
}

/// php-src `ext/dom/tests/DOMNode_cloneNode_basic.phpt`: DOM cloneNode : Basic Functionality
#[test]
#[ignore = r#"DOM port pending: Undefined constant: XML_TEXT_NODE"#]
fn test_domnode_clone_node_basic() {
    if skip_without_xml_native("test_domnode_clone_node_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$xml = <<< EOXML
<?xml version="1.0" encoding="ISO-8859-1"?>
<courses>
    <course title="one">
        <notes>
            <note>c1n1</note>
            <note>c1n2</note>
        </notes>
    </course>
    <course title="two">
        <notes>
            <note>c2n1</note>
            <note>c2n2</note>
        </notes>
    </course>
</courses>
EOXML;

function dumpcourse($current) {
    $title = ($current->nodeType != XML_TEXT_NODE && $current->hasAttribute('title')) ? $current->getAttribute('title'):"no title";
    echo "Course: $title:";echo(get_class($current)), "\n";
    echo "~";var_dump($current->textContent);
}

$dom = new DOMDocument();
$dom->loadXML($xml);
$root = $dom->documentElement;

// strip all text nodes from this tree
$children = $root->childNodes;
$len = $children->length;
for ($index = $children->length - 1; $index >=0; $index--) {
    $current = $children->item($index);
    if ($current->nodeType == XML_TEXT_NODE) {
        $noderemoved = $root->removeChild($current);
    }
}

echo "Start cloneNode test\n";
$first_course = $children->item(0);
$cloned_first_course_default = $first_course->cloneNode();
$first_course->setAttribute('title', 'new title1');

$cloned_first_course_true = $first_course->cloneNode(true);
$first_course->setAttribute('title', 'new title2');

$cloned_first_course_false = $first_course->cloneNode(false);
$first_course->setAttribute('title', 'new title3');

$cloned_first_course_default->setAttribute('title', 'new title default');
$cloned_first_course_true->setAttribute('title', 'new title true');
$cloned_first_course_false->setAttribute('title', 'new title false');

$root->appendChild($cloned_first_course_default);
$root->appendChild($cloned_first_course_true);
$root->appendChild($cloned_first_course_false);

$children = $root->childNodes;
for ($index = 0; $index < $children->length; $index++) {
    echo "node $index\n";
    dumpcourse($children->item($index));
}
"#);
    assert_exact(&out, r#"Start cloneNode test
node 0
Course: new title3:DOMElement
~string(57) "
        
            c1n1
            c1n2
        
    "
node 1
Course: two:DOMElement
~string(57) "
        
            c2n1
            c2n2
        
    "
node 2
Course: new title default:DOMElement
~string(0) ""
node 3
Course: new title true:DOMElement
~string(57) "
        
            c1n1
            c1n2
        
    "
node 4
Course: new title false:DOMElement
~string(0) """#);
}

/// php-src `ext/dom/tests/DOMNode_contains.phpt`: DOMNode::contains()
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_domnode_contains() {
    if skip_without_xml_native("test_domnode_contains") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<!DOCTYPE HTML>
<html xmlns:test="some:ns">
    <head>
        <title>my title</title>
    </head>
    <body>
        <main>
            <p>Hello, world!</p>
            <p>Second paragraph</p>
            <div><p>container</p></div>
            <!-- comment -->
        </main>
    </body>
</html>
XML);

$xpath = new DOMXPath($dom);
$head = $xpath->query("//head")[0];
$main = $xpath->query("//main")[0];
$div = $xpath->query("//div")[0];

echo "--- False edge cases ---\n";

var_dump($dom->documentElement->contains(null));

try {
    var_dump($dom->contains(new stdClass));
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}

echo "--- True cases ---\n";
var_dump($dom->documentElement->contains($head));
var_dump($dom->documentElement->contains($main));
var_dump($dom->contains($dom));
var_dump($main->contains($main));
var_dump($div->contains($div));
var_dump($main->contains($div));
var_dump($main->contains($main->firstElementChild));
var_dump($main->contains($div->firstElementChild));
var_dump($div->contains($div->firstElementChild));
var_dump($main->contains($main->firstElementChild->firstChild));
var_dump($dom->contains($dom->doctype));
var_dump($dom->contains($dom->doctype));
var_dump($dom->contains($dom->documentElement->getAttributeNode('xmlns:test')));
var_dump($dom->contains($main->lastChild));

echo "--- False cases ---\n";
var_dump($main->firstElementChild->contains($main));
var_dump($main->contains($head));
var_dump($div->contains($main));
var_dump($main->contains($head->firstElementChild));
var_dump($div->contains($main->firstElementChild));
var_dump($div->contains($main->firstElementChild->nextElementSibling));
var_dump($div->contains($main->lastChild));

echo "--- False, create element case ---\n";

$newElement = $dom->createElement('x');
var_dump($dom->documentElement->contains($newElement));

echo "--- Removal case ---\n";

$main->remove();
var_dump($main->contains($main));
var_dump($dom->contains($main));

"#);
    assert_exact(&out, r#"--- False edge cases ---
bool(false)
DOMNode::contains(): Argument #1 ($other) must be of type DOMNode|DOMNameSpaceNode|null, stdClass given
--- True cases ---
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
--- False cases ---
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
--- False, create element case ---
bool(false)
--- Removal case ---
bool(true)
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMNode_getLineNo_basic.phpt`: DOMNode::getLineNo()
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domnode_get_line_no_basic() {
    if skip_without_xml_native("test_domnode_get_line_no_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$file = __DIR__.'/book.xml';
$doc = new DOMDocument();
$doc->load($file);
$nodes = $doc->getElementsByTagName('title');
foreach($nodes as $node) {
    var_dump($node->getLineNo());
}
"#);
    assert_exact(&out, r#"int(4)
int(8)"#);
}

/// php-src `ext/dom/tests/DOMNode_getNodePath_basic.phpt`: DOMNode::getNodePath()
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domnode_get_node_path_basic() {
    if skip_without_xml_native("test_domnode_get_node_path_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$file = __DIR__.'/book.xml';
$doc = new DOMDocument();
$doc->load($file);
$nodes = $doc->getElementsByTagName('title');
foreach($nodes as $node) {
    var_dump($node->getNodePath());
}
"#);
    assert_exact(&out, r#"string(20) "/books/book[1]/title"
string(20) "/books/book[2]/title""#);
}

/// php-src `ext/dom/tests/DOMNode_getRootNode.phpt`: DOMNode::getRootNode()
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMElement::getRootNode"#]
fn test_domnode_get_root_node() {
    if skip_without_xml_native("test_domnode_get_root_node") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML('<?xml version="1.0"?><html><body/></html>');

var_dump($dom->documentElement->firstElementChild->getRootNode() === $dom);
$p = $dom->createElement('p');
var_dump($p->getRootNode() === $p);
$dom->documentElement->appendChild($p);
var_dump($p->getRootNode() === $dom);
$dom->documentElement->remove();
var_dump($p->getRootNode() === $p);

$fragment = $dom->createDocumentFragment();
var_dump($fragment->getRootNode() === $fragment);
$div = $fragment->appendChild($dom->createElement('div'));
$div->appendChild($p);
var_dump($p->getRootNode() === $fragment);

"#);
    assert_exact(&out, r#"bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/DOMNode_hasChildNodes.phpt`: Tests DOMNode::hasChildNodes()
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domnode_has_child_nodes() {
    if skip_without_xml_native("test_domnode_has_child_nodes") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();

$dom->loadXML('<root/>');

echo $dom->saveXML();

echo "Document has child nodes\n";
var_dump($dom->documentElement->hasChildNodes());

echo "Document has child nodes\n";
$dom->loadXML('<root><a/></root>');
var_dump($dom->documentElement->hasChildNodes());

echo "Remove node and save\n";
$dom->documentElement->removeChild($dom->documentElement->firstChild);
echo $dom->saveXML();

echo "Document has child nodes\n";
var_dump($dom->documentElement->hasChildNodes());

echo "Document with 2 child nodes\n";
$dom->loadXML('<root><a/><b/></root>');
var_dump($dom->documentElement->hasChildNodes());

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<root/>
Document has child nodes
bool(false)
Document has child nodes
bool(true)
Remove node and save
<?xml version="1.0"?>
<root/>
Document has child nodes
bool(false)
Document with 2 child nodes
bool(true)"#);
}

/// php-src `ext/dom/tests/DOMNode_hasChildNodes_basic.phpt`: Test whether a node has child nodes: hasChildNodes()
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createTextNode"#]
fn test_domnode_has_child_nodes_basic() {
    if skip_without_xml_native("test_domnode_has_child_nodes_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

/* Create an XML document
 * with structure
 * <book>
 *  <title>This is the title</title>
 * </book>
 * Check for child nodes of the <book>, <title> and This is the title
 *
*/

$doc = new DOMDocument();

$root = $doc->createElement('book');
$doc->appendChild($root);

$title = $doc->createElement('title');
$root->appendChild($title);

$text = $doc->createTextNode('This is the title');
$title->appendChild($text);

echo "Root has child nodes: ";
var_dump($root->hasChildNodes());

echo "Title has child nodes: ";
var_dump($title->hasChildNodes());

echo "Text has child nodes: ";
var_dump($text->hasChildNodes());

"#);
    assert_exact(&out, r#"Root has child nodes: bool(true)
Title has child nodes: bool(true)
Text has child nodes: bool(false)"#);
}

/// php-src `ext/dom/tests/DOMNode_insertBefore.phpt`: Tests DOMNode::insertBefore()
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domnode_insert_before() {
    if skip_without_xml_native("test_domnode_insert_before") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML('<root/>');
echo $dom->saveXML();

$e1 = $dom->createElement("A");
$e2 = $dom->documentElement->appendChild($dom->createElement("B"));

echo "Add new node B\n";
echo $dom->saveXML();

echo "Add new node A before B\n";
$e2->parentNode->insertBefore($e1, $e2);
echo $dom->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<root/>
Add new node B
<?xml version="1.0"?>
<root><B/></root>
Add new node A before B
<?xml version="1.0"?>
<root><A/><B/></root>"#);
}

/// php-src `ext/dom/tests/DOMNode_insertBefore_error1.phpt`: DOMNode::insertBefore() should fail if node belongs to another document
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMElement::insertBefore"#]
fn test_domnode_insert_before_error1() {
    if skip_without_xml_native("test_domnode_insert_before_error1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc1 = new DOMDocument();
$doc2 = new DOMDocument();

$node_in_doc1 = $doc1->createElement("foo");
$node_in_doc2 = $doc2->createElement("bar");

try {
    $node_in_doc2->insertBefore($node_in_doc1);
} catch(DOMException $e) {
    echo $e->getMessage();
}

"#);
    assert_exact(&out, r#"Wrong Document Error"#);
}

/// php-src `ext/dom/tests/DOMNode_insertBefore_error2.phpt`: Test DOMNode::insertBefore() check the error code DOM_NOT_FOUND is raised
#[test]
#[ignore = r#"DOM port pending: Undefined function: assert"#]
fn test_domnode_insert_before_error2() {
    if skip_without_xml_native("test_domnode_insert_before_error2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument();

$doc = $dom->load(__DIR__ . "/book.xml", LIBXML_NOBLANKS);
assert($doc === true);

$parent_node = $dom->getElementsByTagName("book")->item(0);
assert(!is_null($parent_node));

$new_node = $dom->createElement('newnode');
assert($new_node !== false);

// getting a sibling as reference node to insert

$ref_node = $dom->getElementsByTagName("book")->item(1);

try {
    $parent_node->insertBefore($new_node, $ref_node);
} catch(DOMException $e) {
    echo $e->getMessage();
}

"#);
    assert_exact(&out, r#"Not Found Error"#);
}

/// php-src `ext/dom/tests/DOMNode_insertBefore_error3.phpt`: Test DOMNode::insertBefore() check the error code DOM_NOT_FOUND is raised
#[test]
#[ignore = r#"DOM port pending: Undefined function: assert"#]
fn test_domnode_insert_before_error3() {
    if skip_without_xml_native("test_domnode_insert_before_error3") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument();

$doc = $dom->load(__DIR__ . "/book.xml", LIBXML_NOBLANKS);
assert($doc === true);

$parent_node = $dom->getElementsByTagName("book")->item(0);
assert(!is_null($parent_node));

$new_node = $dom->createElement('newnode');
assert($new_node !== false);

// getting the parent node as reference node to insert

$ref_node = $dom->getElementsByTagName("book")->item(0)->parentNode;
assert(!is_null($ref_node));

try {
    $parent_node->insertBefore($new_node, $ref_node);
} catch(DOMException $e) {
    echo $e->getMessage();
}

"#);
    assert_exact(&out, r#"Not Found Error"#);
}

/// php-src `ext/dom/tests/DOMNode_insertBefore_error4.phpt`: Test DOMNode::insertBefore() check the error code DOM_NOT_FOUND is raised
#[test]
#[ignore = r#"DOM port pending: Undefined function: assert"#]
fn test_domnode_insert_before_error4() {
    if skip_without_xml_native("test_domnode_insert_before_error4") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument();

$doc = $dom->load(__DIR__ . "/book.xml", LIBXML_NOBLANKS);
assert($doc === true);

$parent_node = $dom->getElementsByTagName("book")->item(0);
assert(!is_null($parent_node));

$new_node = $dom->createElement('newnode');
assert($new_node !== false);

// could be a brand new node

$ref_node = $dom->createElement('newnode2');

try {
    $parent_node->insertBefore($new_node, $ref_node);
} catch(DOMException $e) {
    echo $e->getMessage();
}

"#);
    assert_exact(&out, r#"Not Found Error"#);
}

/// php-src `ext/dom/tests/DOMNode_insertBefore_error5.phpt`: Test DOMNode::insertBefore() check the error code DOM_NOT_FOUND is raised
#[test]
#[ignore = r#"DOM port pending: Undefined function: assert"#]
fn test_domnode_insert_before_error5() {
    if skip_without_xml_native("test_domnode_insert_before_error5") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument();

$doc = $dom->load(__DIR__ . "/book.xml", LIBXML_NOBLANKS);
assert($doc === true);

$parent_node = $dom->getElementsByTagName("book")->item(0);
assert(!is_null($parent_node));

$new_node = $dom->createElement('newnode');
assert($new_node !== false);

// creating a new node (descendant) and getting it as the refnode

$ref_node = $dom->createElement('newnode3');
$parent_node->childNodes->item(0)->appendChild($ref_node);
$dom->saveXML();

try {
    $parent_node->insertBefore($new_node, $ref_node);
} catch(DOMException $e) {
    echo $e->getMessage();
}

"#);
    assert_exact(&out, r#"Not Found Error"#);
}

/// php-src `ext/dom/tests/DOMNode_insertBefore_error6.phpt`: Test DOMNode::insertBefore() check the error code DOM_NOT_FOUND is raised
#[test]
#[ignore = r#"DOM port pending: Undefined function: assert"#]
fn test_domnode_insert_before_error6() {
    if skip_without_xml_native("test_domnode_insert_before_error6") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument();

$doc = $dom->load(__DIR__ . "/book.xml", LIBXML_NOBLANKS);
assert($doc === true);

$parent_node = $dom->getElementsByTagName("book")->item(0);
assert(!is_null($parent_node));
$ref_node = $parent_node;

$new_node = $dom->createElement('newnode');
assert($new_node !== false);

try {
    $parent_node->insertBefore($new_node, $ref_node);
} catch(DOMException $e) {
    echo $e->getMessage();
}

"#);
    assert_exact(&out, r#"Not Found Error"#);
}

/// php-src `ext/dom/tests/DOMNode_isEqualNode.phpt`: DOMNode::isEqualNode()
#[test]
#[ignore = r#"DOM port pending: iterator_to_array() first argument must be a statically known array or Traversable"#]
fn test_domnode_is_equal_node() {
    if skip_without_xml_native("test_domnode_is_equal_node") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom1 = new DOMDocument();
$dom2 = new DOMDocument();

$dom1->loadXML(<<<XML
<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd" [
    <!ENTITY bar '<bar>bartext</bar>'>
    <!ENTITY foo '<foo/>'>
    <!NOTATION myNotation SYSTEM "test.dtd">
]>
<html>
    <body>
        <pi><?test 123?><?test2 123?><?test2 1234?><?test 123?></pi>
        <ps><p>text</p><p>text</p><p>other text</p></ps>
        <psattrs><p align="center" xmlns:foo="some:bar">text</p><p xmlns:foo="some:bar">text</p><p align="center">text</p><p align="left" xmlns:foo="some:bar">text</p><p align="center" xmlns:foo="some:bar2">text</p><p align="center" xmlns:foo="some:bar">text</p></psattrs>
        <comments><!-- comment 1 --><!-- comment 2 --><!-- comment 1 --></comments>
        <texts>abc<i/>def<i/>abc</texts>
        <cdatas><![CDATA[test]]> <![CDATA[test2]]> <![CDATA[test]]></cdatas>
        <tree>
            <div>
                <p>A</p>
                <div foo="bar">
                    <p>B</p>
                </div>
            </div>
            <p>A</p>
        </tree>
    </body>
</html>
XML);

$xpath = new DOMXPath($dom1);

function foreach_comparator($query) {
    global $xpath;
    $container = $xpath->query($query)[0];
    $childNodes = iterator_to_array($container->childNodes);
    $firstChild = $childNodes[0];
    foreach ($childNodes as $child) {
        var_dump($child->isEqualNode($firstChild));
    }
}

function comparePairs($list1, $list2) {
    $list1 = iterator_to_array($list1);
    $list2 = iterator_to_array($list2);
    usort($list1, function ($a, $b) {
        return strcmp($a->nodeName, $b->nodeName);
    });
    usort($list2, function ($a, $b) {
        return strcmp($a->nodeName, $b->nodeName);
    });
    foreach ($list1 as $entity1) {
        foreach ($list2 as $entity2) {
            echo "Comparing {$entity1->nodeName} with {$entity2->nodeName}\n";
            var_dump($entity1->isEqualNode($entity2));
        }
    }
}

echo "--- Test edge cases ---\n";

var_dump($dom1->doctype->isEqualNode(null));
var_dump((new DOMDocument())->isEqualNode(new DOMDocument()));

echo "--- Test doctype ---\n";

var_dump($dom1->doctype->isEqualNode($dom1->doctype));
$dom2->loadXML('<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/x.dtd"><html/>');
var_dump($dom1->doctype->isEqualNode($dom2->doctype));
$dom2->loadXML('<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN2" "http://www.w3.org/TR/html4/strict.dtd"><html/>');
var_dump($dom1->doctype->isEqualNode($dom2->doctype));
$dom2->loadXML('<!DOCTYPE HTML2 PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd"><html/>');
var_dump($dom1->doctype->isEqualNode($dom2->doctype));
$dom2->loadXML('<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd"><html/>');
var_dump($dom1->doctype->isEqualNode($dom2->doctype));

echo "--- Test processing instruction ---\n";

foreach_comparator("//pi");

echo "--- Test comments ---\n";

foreach_comparator("//comments");

echo "--- Test texts ---\n";

foreach_comparator("//texts");

echo "--- Test CDATA ---\n";

foreach_comparator("//cdatas");

echo "--- Test attribute ---\n";

var_dump((new DOMAttr("name", "value"))->isEqualNode(new DOMAttr("name", "value")));
var_dump((new DOMAttr("name", "value"))->isEqualNode(new DOMAttr("name", "value2")));
var_dump((new DOMAttr("name", "value"))->isEqualNode(new DOMAttr("name2", "value")));
var_dump((new DOMAttr("name", "value"))->isEqualNode(new DOMAttr("name2", "value2")));
var_dump((new DOMAttr("name", "value"))->isEqualNode(new DOMAttr("ns:name", "value")));

echo "--- Test entity reference ---\n";

var_dump((new DOMEntityReference("ref"))->isEqualNode(new DOMEntityReference("ref")));
var_dump((new DOMEntityReference("ref"))->isEqualNode(new DOMEntityReference("ref2")));

echo "--- Test entity declaration ---\n";

$dom2->loadXML(<<<XML
<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd" [
    <!ENTITY barbar '<bar>bartext</bar>'>
    <!ENTITY foo '<foo2/>'>
    <!ENTITY bar '<bar>bartext</bar>'>
]>
<html/>
XML);

comparePairs($dom1->doctype->entities, $dom2->doctype->entities);

echo "--- Test notation declaration ---\n";

$dom2->loadXML(<<<XML
<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd" [
    <!NOTATION myNotation SYSTEM "test.dtd">
    <!NOTATION myNotation2 SYSTEM "test2.dtd">
    <!NOTATION myNotation3 SYSTEM "test.dtd">
]>
<html/>
XML);

comparePairs($dom1->doctype->notations, $dom2->doctype->notations);

echo "--- Test element without attributes ---\n";

foreach_comparator("//ps");

echo "--- Test element with attributes ---\n";

foreach_comparator("//psattrs");

echo "--- Test element tree ---\n";

$tree = $xpath->query("//tree")[0];
$dom2->loadXML(<<<XML
        <tree>
            <div>
                <p>A</p>
                <div foo="bar">
                    <p>B</p>
                </div>
            </div>
            <p>A</p>
        </tree>
XML);
var_dump($tree->isEqualNode($dom2->documentElement));
$dom2->loadXML(<<<XML
        <tree>
            <p>A</p>
            <div>
                <p>A</p>
                <div foo="bar">
                    <p>B</p>
                </div>
            </div>
        </tree>
XML);
var_dump($tree->isEqualNode($dom2->documentElement));
$dom2->loadXML(<<<XML
        <tree>
            <div>
                <p>A</p>
                <div foo="bar">
                </div>
            </div>
            <p>A</p>
        </tree>
XML);
var_dump($tree->isEqualNode($dom2->documentElement));
$dom2->loadXML(<<<XML
        <tree>
            <div>
                <p>A</p>
                <div foo="bar" extra="attr">
                    <p>B</p>
                </div>
            </div>
            <p>A</p>
        </tree>
XML);
var_dump($tree->isEqualNode($dom2->documentElement));

echo "--- Test documents ---\n";

$dom1Clone = clone $dom1;
var_dump($dom1->documentElement->isEqualNode($dom1Clone->documentElement));
var_dump($dom1->documentElement->isEqualNode($dom2->documentElement));
var_dump($dom1->isEqualNode($dom1Clone));
var_dump($dom1->isEqualNode($dom2));
var_dump($dom1->documentElement->isEqualNode($dom1Clone));
var_dump($dom1->documentElement->isEqualNode($dom2));

echo "--- Test document fragments ---\n";

$fragment1 = $dom1->createDocumentFragment();
$fragment1->appendChild($dom1->createElement('em'));
$fragment2 = $dom1->createDocumentFragment();
$fragment2->appendChild($dom1->createElement('em'));
$fragment3 = $dom1->createDocumentFragment();
$fragment3->appendChild($dom1->createElement('b'));
$emptyFragment1 = $dom1->createDocumentFragment();
$emptyFragment2 = $dom1->createDocumentFragment();

var_dump($fragment1->isEqualNode($fragment2));
var_dump($fragment1->isEqualNode($fragment3));
var_dump($emptyFragment1->isEqualNode($fragment1));
var_dump($emptyFragment1->isEqualNode($emptyFragment2));

echo "--- Test document fragments with multiple child nodes ---\n";

$fragment1 = $dom1->createDocumentFragment();
$fragment1->appendChild($dom1->createElement('a'));
$fragment1->appendChild($dom1->createElement('b'));
$fragment1->appendChild($dom1->createElement('c'));
$fragment2 = $dom2->createDocumentFragment();
$fragment2->appendChild($dom2->createElement('a'));
$fragment2->appendChild($dom2->createElement('b'));
$fragment2->appendChild($dom2->createElement('c'));

var_dump($fragment1->isEqualNode($fragment2));

$fragment2->firstChild->nextSibling->nextSibling->remove();
var_dump($fragment1->isEqualNode($fragment2));

echo "--- Test x:includes ---\n";

// Adapted from https://www.php.net/manual/en/domdocument.xinclude.php
$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0" ?>
<chapter xmlns:xi="http://www.w3.org/2001/XInclude">
 <p>Hello</p>
 <para>
  <xi:include href="book.xml">
   <xi:fallback>
    <p>xinclude: book.xml not found</p>
   </xi:fallback>
  </xi:include>
 </para>
 <para><p>xinclude: book.xml not found</p></para>
</chapter>
XML);
@$dom->xinclude();
$xpath = new DOMXPath($dom);

$firstPara = $dom->documentElement->firstElementChild->nextElementSibling;
$secondPara = $dom->documentElement->firstElementChild->nextElementSibling->nextElementSibling;
var_dump($firstPara->isEqualNode($secondPara));
var_dump($firstPara->firstElementChild->isEqualNode($secondPara->firstElementChild));

"#);
    assert_exact(&out, r#"--- Test edge cases ---
bool(false)
bool(true)
--- Test doctype ---
bool(true)
bool(false)
bool(false)
bool(false)
bool(true)
--- Test processing instruction ---
bool(true)
bool(false)
bool(false)
bool(true)
--- Test comments ---
bool(true)
bool(false)
bool(true)
--- Test texts ---
bool(true)
bool(false)
bool(false)
bool(false)
bool(true)
--- Test CDATA ---
bool(true)
bool(false)
bool(false)
bool(false)
bool(true)
--- Test attribute ---
bool(true)
bool(false)
bool(false)
bool(false)
bool(false)
--- Test entity reference ---
bool(true)
bool(false)
--- Test entity declaration ---
Comparing bar with bar
bool(true)
Comparing bar with barbar
bool(false)
Comparing bar with foo
bool(false)
Comparing foo with bar
bool(false)
Comparing foo with barbar
bool(false)
Comparing foo with foo
bool(true)
--- Test notation declaration ---
Comparing myNotation with myNotation
bool(true)
Comparing myNotation with myNotation2
bool(false)
Comparing myNotation with myNotation3
bool(false)
--- Test element without attributes ---
bool(true)
bool(true)
bool(false)
--- Test element with attributes ---
bool(true)
bool(false)
bool(false)
bool(false)
bool(false)
bool(true)
--- Test element tree ---
bool(true)
bool(false)
bool(false)
bool(false)
--- Test documents ---
bool(true)
bool(false)
bool(true)
bool(false)
bool(false)
bool(false)
--- Test document fragments ---
bool(true)
bool(false)
bool(false)
bool(true)
--- Test document fragments with multiple child nodes ---
bool(true)
bool(false)
--- Test x:includes ---
bool(false)
bool(true)"#);
}

/// php-src `ext/dom/tests/DOMNode_issamenode_basic.phpt`: DOMNode: isSameNode()
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_domnode_issamenode_basic() {
    if skip_without_xml_native("test_domnode_issamenode_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$dom = new DOMDocument;
$dom->loadXML($xmlstr);
if(!$dom) {
  echo "Error while parsing the document\n";
  exit(0);
}

$node = $dom->documentElement;
if($node->isSameNode($node))
    echo "EXPECTING SAME NODE, PASSED\n" ;
else
    echo "EXPECTING SAME NODE, FAILED\n" ;

$nodelist=$dom->getElementsByTagName('tbody') ;

if($nodelist->item(0)->isSameNode($node))
    echo "EXPECTING NOT SAME NODE, FAILED\n" ;
else
    echo "EXPECTING NOT SAME NODE, PASSED\n" ;

"#);
    assert_exact(&out, r#"EXPECTING SAME NODE, PASSED
EXPECTING NOT SAME NODE, PASSED"#);
}

/// php-src `ext/dom/tests/DOMNode_normalize_advanced.phpt`: DOMNode::normalize() advanced
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createTextNode"#]
fn test_domnode_normalize_advanced() {
    if skip_without_xml_native("test_domnode_normalize_advanced") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$root = $dom->appendChild($dom->createElement("root"));
$child1 = $root->appendChild($dom->createElement("child1"));
$child2 = $root->appendChild($dom->createElement("child2"));

$child1->appendChild($dom->createTextNode("abc"));
$child1->appendChild($dom->createTextNode(""));
$child1->appendChild($dom->createTextNode("def"));

$child2->appendChild($dom->createTextNode(""));
$child2->setAttribute("foo", "bar");

echo $dom->saveXML();

$root->normalize();

var_dump($child1->childNodes[0]->textContent);
var_dump($child2->childNodes[0]);

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<root><child1>abcdef</child1><child2 foo="bar"></child2></root>
string(6) "abcdef"
NULL"#);
}

/// php-src `ext/dom/tests/DOMNode_normalize_basic.phpt`: DOMNode::normalize()
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createTextNode"#]
fn test_domnode_normalize_basic() {
    if skip_without_xml_native("test_domnode_normalize_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

/* Create an XML document
 * with structure
 * <book>
 *  <author></author>
 *  <title>This is the title</title>
 * </book>
 * Calculate the number of title text nodes (1).
 * Add another text node to title. Calculate the number of title text nodes (2).
 * Normalize author. Calculate the number of title text nodes (2).
 * Normalize title. Calculate the number of title text nodes (1).
*/

$doc = new DOMDocument();

$root = $doc->createElement('book');
$doc->appendChild($root);

$title = $doc->createElement('title');
$root->appendChild($title);

$author = $doc->createElement('author');
$root->appendChild($author);

$text = $doc->createTextNode('This is the first title');
$title->appendChild($text);

echo "Number of child nodes of title = ";
var_dump($title->childNodes->length);

// add a second text node to title
$text = $doc->createTextNode('This is the second title');
$title->appendChild($text);

echo "Number of child nodes of title after adding second title = ";
var_dump($title->childNodes->length);

// should do nothing
$author->normalize();

echo "Number of child nodes of title after normalizing author = ";
var_dump($title->childNodes->length);


// should concatenate first and second title text nodes
$title->normalize();

echo "Number of child nodes of title after normalizing title = ";
var_dump($title->childNodes->length);

"#);
    assert_exact(&out, r#"Number of child nodes of title = int(1)
Number of child nodes of title after adding second title = int(2)
Number of child nodes of title after normalizing author = int(2)
Number of child nodes of title after normalizing title = int(1)"#);
}

/// php-src `ext/dom/tests/DOMNode_removeChild_basic.phpt`: DOM removeChild : Basic Functionality
#[test]
#[ignore = r#"DOM port pending: Undefined constant: XML_TEXT_NODE"#]
fn test_domnode_remove_child_basic() {
    if skip_without_xml_native("test_domnode_remove_child_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$xml = <<< EOXML
<?xml version="1.0" encoding="ISO-8859-1"?>
<courses>
    <course title="one">
        <notes>
            <note>c1n1</note>
            <note>c1n2</note>
        </notes>
    </course>
    <course title="two">
        <notes>
            <note>c2n1</note>
            <note>c2n2</note>
        </notes>
    </course>
</courses>
EOXML;

function dumpcourse($current) {
    $title = ($current->nodeType != XML_TEXT_NODE && $current->hasAttribute('title')) ? $current->getAttribute('title'):"no title";
    echo "Course: $title:";echo get_class($current), "\n";
    echo "~";var_dump($current->textContent);
}

$dom = new DOMDocument();
$dom->loadXML($xml);
$root = $dom->documentElement;

$children = $root->childNodes;
$len = $children->length;
echo "original has $len nodes\n";
for ($index = $children->length - 1; $index >=0; $index--) {
    echo "node $index\n";
    $current = $children->item($index);
    dumpcourse($current);
    if ($current->nodeType == XML_TEXT_NODE) {
        $noderemoved = $root->removeChild($current);
    }
}
$children = $root->childNodes;
$len = $children->length;
echo "after text removed it now has $len nodes\n";
for ($index = 0; $index < $children->length; $index++) {
    echo "node $index\n";
    $current = $children->item($index);
    dumpcourse($current);
}
"#);
    assert_exact(&out, r#"original has 5 nodes
node 4
Course: no title:DOMText
~string(1) "
"
node 3
Course: two:DOMElement
~string(57) "
        
            c2n1
            c2n2
        
    "
node 2
Course: no title:DOMText
~string(5) "
    "
node 1
Course: one:DOMElement
~string(57) "
        
            c1n1
            c1n2
        
    "
node 0
Course: no title:DOMText
~string(5) "
    "
after text removed it now has 2 nodes
node 0
Course: one:DOMElement
~string(57) "
        
            c1n1
            c1n2
        
    "
node 1
Course: two:DOMElement
~string(57) "
        
            c2n1
            c2n2
        
    ""#);
}

/// php-src `ext/dom/tests/DOMNode_removeChild_error1.phpt`: removeChild() where the node is not a child
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domnode_remove_child_error1() {
    if skip_without_xml_native("test_domnode_remove_child_error1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$document = new DOMDocument();
$real_parent = $document->createElement('real');
$parent = $document->createElement('p');
$child1 = $document->createElement('child1');
$child2 = $document->createElement('child2');
$real_parent->appendChild($child1);
$parent->appendChild($child2);
try {
	$parent->removeChild($child1);
} catch (DOMException $e) {
	echo "DOMException: " . $e->getMessage();
}
"#);
    assert_exact(&out, r#"DOMException: Not Found Error"#);
}

/// php-src `ext/dom/tests/DOMNode_replaceChild_basic.phpt`: Replacing a child node
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMNode::firstChild"#]
fn test_domnode_replace_child_basic() {
    if skip_without_xml_native("test_domnode_replace_child_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$document = new DOMDocument();
$document->loadXML('<?xml version="1.0" encoding="utf-8"?>
<root><foo><bar/><baz/></foo><spam><eggs/><eggs/></spam></root>');

// Replaces the child node oldChild with newChild in the list of children, and
// returns the oldChild node.
$parent = $document->getElementsByTagName('foo')->item(0);
$new_child = $document->createElement('qux');
$old_child = $parent->replaceChild($new_child, $parent->firstChild);
echo "New child replaces old child:\n" . $document->saveXML();
echo "Old child is returned:\n" . $old_child->tagName . "\n";

// If the newChild is already in the tree, it is first removed.
$parent = $document->getElementsByTagName('spam')->item(0);
$parent->replaceChild($new_child, $parent->firstChild);
echo "Existing child is removed from tree:\n" . $document->saveXML();

// Children are inserted in the correct order.
$new_child = $document->getElementsByTagName('spam')->item(0);
$parent = $document->getElementsByTagName('foo')->item(0);
$parent->replaceChild($new_child, $parent->firstChild);
echo "Children are inserted in order:\n" . $document->saveXML();
"#);
    assert_exact(&out, r#"New child replaces old child:
<?xml version="1.0" encoding="utf-8"?>
<root><foo><qux/><baz/></foo><spam><eggs/><eggs/></spam></root>
Old child is returned:
bar
Existing child is removed from tree:
<?xml version="1.0" encoding="utf-8"?>
<root><foo><baz/></foo><spam><qux/><eggs/></spam></root>
Children are inserted in order:
<?xml version="1.0" encoding="utf-8"?>
<root><foo><spam><qux/><eggs/></spam></foo></root>"#);
}

/// php-src `ext/dom/tests/DOMNode_replaceChild_error1.phpt`: replaceChild() where the old node is not a child
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMElement::replaceChild"#]
fn test_domnode_replace_child_error1() {
    if skip_without_xml_native("test_domnode_replace_child_error1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$document = new DOMDocument();
$real_parent = $document->createElement('real');
$parent = $document->createElement('p');
$child1 = $document->createElement('child1');
$child2 = $document->createElement('child2');
$child3 = $document->createElement('child3');
$real_parent->appendChild($child1);
$parent->appendChild($child2);
try {
	$parent->replaceChild($child3, $child1);
} catch (DOMException $e) {
	echo "DOMException: " . $e->getMessage();
}
"#);
    assert_exact(&out, r#"DOMException: Not Found Error"#);
}

/// php-src `ext/dom/tests/DOMNode_replaceChild_error2.phpt`: replaceChild() where the new node is a grandparent of the old node
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMElement::replaceChild"#]
fn test_domnode_replace_child_error2() {
    if skip_without_xml_native("test_domnode_replace_child_error2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$document = new DOMDocument();
$a = $document->createElement('a');
$b = $document->createElement('b');
$c = $document->createElement('c');
$a->appendChild($b);
$b->appendChild($c);
try {
	$b->replaceChild($a, $c);
} catch (DOMException $e) {
	echo "DOMException: " . $e->getMessage();
}
"#);
    assert_exact(&out, r#"DOMException: Hierarchy Request Error"#);
}

/// php-src `ext/dom/tests/DOMParentNode_empty_argument.phpt`: DOMParentNode functions with empty argument
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createDocumentFragment"#]
fn test_domparent_node_empty_argument() {
    if skip_without_xml_native("test_domparent_node_empty_argument") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument();
$dom->loadXML('<?xml version="1.0"?><root><node/></root>');

$emptyFragment = $dom->createDocumentFragment();

echo "--- DOMElement test ---\n";

$dom->documentElement->after(...$emptyFragment->childNodes);
$dom->documentElement->before(...$emptyFragment->childNodes);
$dom->documentElement->append(...$emptyFragment->childNodes);
$dom->documentElement->prepend(...$emptyFragment->childNodes);
$dom->documentElement->after();
$dom->documentElement->before();
$dom->documentElement->append();
$dom->documentElement->prepend();
echo $dom->saveXML();

$dom->documentElement->firstChild->replaceWith(...$emptyFragment->childNodes);
echo $dom->saveXML();

$dom->documentElement->replaceWith(...$emptyFragment->childNodes);
echo $dom->saveXML();

echo "--- DOMDocumentFragment test ---\n";

$fragment = $dom->createDocumentFragment();
$fragment->append($dom->createElement('foo'));
$fragment->append(...$emptyFragment->childNodes);
$fragment->prepend(...$emptyFragment->childNodes);
$fragment->append();
$fragment->prepend();
echo $dom->saveXML($fragment), "\n";

echo "--- DOMDocument test ---\n";

$dom->append(...$emptyFragment->childNodes);
$dom->prepend(...$emptyFragment->childNodes);
$dom->append();
$dom->prepend();
echo $dom->saveXML(), "\n";

echo "--- DOMCharacterData test ---\n";

$cdata = $dom->createCDATASection('foo');
$dom->appendChild($cdata);

$cdata->after(...$emptyFragment->childNodes);
$cdata->before(...$emptyFragment->childNodes);
$cdata->after();
$cdata->before();
echo $dom->saveXML(), "\n";
$cdata->replaceWith(...$emptyFragment->childNodes);
echo $dom->saveXML(), "\n";

$cdata = $dom->createCDATASection('foo');
$dom->appendChild($cdata);
$cdata->replaceWith(...$emptyFragment->childNodes);
echo $dom->saveXML(), "\n";
"#);
    assert_exact(&out, r#"--- DOMElement test ---
<?xml version="1.0"?>
<root><node/></root>
<?xml version="1.0"?>
<root/>
<?xml version="1.0"?>
--- DOMDocumentFragment test ---
<foo/>
--- DOMDocument test ---
<?xml version="1.0"?>

--- DOMCharacterData test ---
<?xml version="1.0"?>
<![CDATA[foo]]>

<?xml version="1.0"?>

<?xml version="1.0"?>"#);
}

/// php-src `ext/dom/tests/DOMProcessingInstruction.phpt`: Tests for DOMProcessingInstruction class
#[test]
#[ignore = r#"DOM port pending: Property DOMProcessingInstruction::$data expects string, got FailingStringable"#]
fn test_domprocessing_instruction() {
    if skip_without_xml_native("test_domprocessing_instruction") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class FailingStringable {
    public function __toString(): string {
        throw new Exception("failed in __toString");
    }
}

echo "--- Test construction ---\n";

try {
    $pi = new DOMProcessingInstruction("\0");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

$pi = new DOMProcessingInstruction("test");

echo "--- Test fields ---\n";

var_dump($pi->target);
var_dump($pi->data);
$pi->data = "ok";
var_dump($pi->data);
try {
    $pi->data = new FailingStringable;
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}
var_dump($pi->data);
$pi->data = 12345;
var_dump($pi->data);
$pi->data = "my data <>";
var_dump($pi->data);

echo "--- Test appending ---\n";

$doc = new DOMDocument;
$doc->appendChild($doc->createElement('root'));
$doc->documentElement->appendChild($doc->adoptNode($pi));
echo $doc->saveXML();

echo "--- Test construction with __construct by reflection and fields ---\n";

$class = new ReflectionClass('DOMProcessingInstruction');
$instance = $class->newInstanceWithoutConstructor();

try {
    var_dump($instance->target);
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}
try {
    var_dump($instance->data);
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}
try {
    $instance->data = "hello";
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"--- Test construction ---
Invalid Character Error
--- Test fields ---
string(4) "test"
string(0) ""
string(2) "ok"
failed in __toString
string(2) "ok"
string(5) "12345"
string(10) "my data <>"
--- Test appending ---
<?xml version="1.0"?>
<root><?test my data <>?></root>
--- Test construction with __construct by reflection and fields ---
Invalid State Error
Invalid State Error
Invalid State Error"#);
}

/// php-src `ext/dom/tests/DOMText_appendData_basic.phpt`: DOMText::appendData basic functionality test
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createTextNode"#]
fn test_domtext_append_data_basic() {
    if skip_without_xml_native("test_domtext_append_data_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$document = new DOMDocument;
$root = $document->createElement('root');
$document->appendChild($root);

$text = $document->createElement('text');
$root->appendChild($text);

$textnode = $document->createTextNode('');
$text->appendChild($textnode);
$textnode->appendData('data');
echo "Text Length (one append): " . $textnode->length . "\n";

$textnode->appendData('><&"');
echo "Text Length (two appends): " . $textnode->length . "\n";

echo "Text Content: " . $textnode->data . "\n";

echo "\n" . $document->saveXML();

"#);
    assert_exact(&out, r#"Text Length (one append): 4
Text Length (two appends): 8
Text Content: data><&"

<?xml version="1.0"?>
<root><text>data&gt;&lt;&amp;"</text></root>"#);
}

/// php-src `ext/dom/tests/DOM_PHP_ERR_deprecated.phpt`: Test deprecation emitted on accessing DOM_PHP_ERR
#[test]
#[ignore = r#"DOM port pending: Undefined constant: DOM_PHP_ERR"#]
fn test_dom_php_err_deprecated() {
    if skip_without_xml_native("test_dom_php_err_deprecated") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
var_dump(DOM_PHP_ERR);
"#);
    assert_format(&out, r#"Deprecated: Constant DOM_PHP_ERR is deprecated since 8.4, as it is no longer used in %s on line %d
int(0)"#);
}
