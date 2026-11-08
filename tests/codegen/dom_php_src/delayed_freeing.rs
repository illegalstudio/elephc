//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `delayed/freeing` folder, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/delayed_freeing/attribute_declaration.phpt`: Delayed freeing attribute declaration
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_attribute_declaration() {
    if skip_without_xml_native("test_attribute_declaration") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXML(<<<'XML'
<?xml version="1.0"?>
<!DOCTYPE book [
<!ELEMENT book (#PCDATA)>
<!ATTLIST book
title CDATA #REQUIRED
>
]>
<book title="book title"/>
XML);
echo $doc->saveXML(), "\n";
// Note: no way to query the attribute declaration, but this at least tests destruction order
$doc->removeChild($doc->doctype);
echo $doc->saveXML(), "\n";
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<!DOCTYPE book [
<!ELEMENT book (#PCDATA)>
<!ATTLIST book title CDATA #REQUIRED>
]>
<book title="book title"/>

<?xml version="1.0"?>
<book title="book title"/>"#);
}

/// php-src `ext/dom/tests/delayed_freeing/comment_node.phpt`: Delayed freeing comment node
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createComment"#]
fn test_comment_node() {
    if skip_without_xml_native("test_comment_node") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
$comment = $doc->appendChild($doc->createElement('container'))
    ->appendChild($doc->createComment('my comment'));
echo $doc->saveXML(), "\n";
$comment->parentNode->remove();
echo $doc->saveXML(), "\n";
echo $doc->saveXML($comment), "\n";
var_dump($comment->parentNode);
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<container><!--my comment--></container>

<?xml version="1.0"?>

<!--my comment-->
NULL"#);
}

/// php-src `ext/dom/tests/delayed_freeing/direct_construction.phpt`: Tests with direction construction
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMCharacterData::__construct' expects 0 arguments, got 1"#]
fn test_direct_construction() {
    if skip_without_xml_native("test_direct_construction") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
function node_alike_test($test) {
    try {
        var_dump($test->parentNode);
        var_dump($test->nodeValue);
    } catch (Throwable $e) {
        echo $e->getMessage(), "\n";
    }
    try {
        var_dump($test->appendChild($test));
    } catch (Throwable $e) {
        echo $e->getMessage(), "\n";
    }
}

echo "-- Test DOMCharacterData --\n";
$test = new DOMCharacterData("test");
try {
    var_dump($test->textContent);
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}
try {
    var_dump($test->appendData('test'));
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

echo "-- Test DOMCdataSection --\n";
$test = new DOMCdataSection("test");
var_dump($test->textContent);
var_dump($test->appendData('test'));

echo "-- Test DOMText --\n";
$test = new DOMText("test");
try {
    var_dump($test->wholeText);
    var_dump($test->parentNode);
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}
try {
    var_dump($test->isWhitespaceInElementContent());
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

echo "-- Test DOMComment --\n";
$test = new DOMComment("my comment");
var_dump($test->textContent);
var_dump($test->parentNode);
var_dump($test->getLineNo());

echo "-- Test DOMElement --\n";
$test = new DOMElement("qualifiedName", "test");
var_dump($test->textContent);
var_dump($test->parentNode);
try {
    var_dump($test->appendChild($test));
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

echo "-- Test DOMNode --\n";
node_alike_test(new DOMNode());

echo "-- Test DOMNotation --\n";
node_alike_test(new DOMNotation());

echo "-- Test DOMProcessingInstruction --\n";
node_alike_test(new DOMProcessingInstruction("name", "value"));

echo "-- Test DOMEntity --\n";
$test = new DOMEntity();
try {
    var_dump($test->nodeValue);
    var_dump($test->parentNode);
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}
try {
    var_dump($test->appendChild($test));
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

echo "-- Test DOMAttr --\n";
$test = new DOMAttr("attr", "value");
var_dump($test->nodeValue);
var_dump($test->parentNode);
try {
    var_dump($test->appendChild($test));
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}
"#);
    assert_exact(&out, r#"-- Test DOMCharacterData --
Invalid State Error
Couldn't fetch DOMCharacterData
-- Test DOMCdataSection --
string(4) "test"
bool(true)
-- Test DOMText --
string(4) "test"
NULL
bool(false)
-- Test DOMComment --
string(10) "my comment"
NULL
int(0)
-- Test DOMElement --
string(4) "test"
NULL
No Modification Allowed Error
-- Test DOMNode --
Invalid State Error
Couldn't fetch DOMNode
-- Test DOMNotation --
Invalid State Error
Couldn't fetch DOMNotation
-- Test DOMProcessingInstruction --
NULL
string(5) "value"
bool(false)
-- Test DOMEntity --
Invalid State Error
Couldn't fetch DOMEntity
-- Test DOMAttr --
string(5) "value"
NULL
No Modification Allowed Error"#);
}

/// php-src `ext/dom/tests/delayed_freeing/document_fragment.phpt`: Delayed freeing document fragment
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createDocumentFragment"#]
fn test_document_fragment() {
    if skip_without_xml_native("test_document_fragment") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
$frag = $doc->createDocumentFragment();
$frag->appendChild($doc->createElementNS('some:ns', 'child', 'text content'));
$child = $doc->appendChild($doc->createElement('root'))->appendChild($frag);
var_dump($doc->textContent);
$doc->documentElement->remove();
var_dump($doc->textContent);
unset($doc);
var_dump($child->textContent);

$doc = new DOMDocument;
$doc->appendChild($doc->createElement('container'));
$doc->documentElement->appendChild($doc->importNode($frag));
unset($frag);
var_dump($doc->textContent);

var_dump($child->parentNode);
"#);
    assert_format(&out, r#"string(12) "text content"
string(0) ""
string(12) "text content"

Warning: DOMNode::appendChild(): Document Fragment is empty in %s on line %d
string(0) ""
NULL"#);
}

/// php-src `ext/dom/tests/delayed_freeing/dom_character_data.phpt`: Delayed freeing character data
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_dom_character_data() {
    if skip_without_xml_native("test_dom_character_data") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXML(<<<'XML'
<?xml version="1.0"?>
<container><![CDATA[This is a CDATA section<p>test</p>]]></container>
XML);
$cdata = $doc->documentElement->firstChild;
var_dump($cdata->wholeText, $cdata->parentNode->tagName);
$cdata->parentNode->remove();
var_dump($cdata->wholeText, $cdata->parentNode->tagName);
"#);
    assert_format(&out, r#"string(34) "This is a CDATA section<p>test</p>"
string(9) "container"

Warning: Attempt to read property "tagName" on null in %s on line %d
string(34) "This is a CDATA section<p>test</p>"
NULL"#);
}

/// php-src `ext/dom/tests/delayed_freeing/dtd_node.phpt`: Delayed freeing dtd node
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::implementation"#]
fn test_dtd_node() {
    if skip_without_xml_native("test_dtd_node") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
$dtd = $doc->implementation->createDocumentType('qualified name', 'public', 'system');
$doc = $doc->implementation->createDocument('', '', $dtd);
echo $doc->saveXML(), "\n";
unset($doc);
echo $dtd->ownerDocument->saveXML();
$dtd->ownerDocument->removeChild($dtd);
var_dump($dtd->ownerDocument->nodeName);
"#);
    assert_exact(&out, r##"<?xml version="1.0"?>
<!DOCTYPE qualified name PUBLIC "public" "system">

<?xml version="1.0"?>
<!DOCTYPE qualified name PUBLIC "public" "system">
string(9) "#document""##);
}

/// php-src `ext/dom/tests/delayed_freeing/element_declaration.phpt`: Delayed freeing element declaration
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_element_declaration() {
    if skip_without_xml_native("test_element_declaration") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXML(<<<'XML'
<?xml version="1.0"?>
<!DOCTYPE books [
<!ELEMENT parent (child1, child2)>
<!ELEMENT child1 (#PCDATA)>
<!ELEMENT child2 (#PCDATA)>
]>
<container><parent/></container>
XML, LIBXML_NOENT);
$element = $doc->documentElement->firstElementChild;
echo $doc->saveXML(), "\n";
var_dump($element->tagName);
var_dump($element->textContent);

$doc->removeChild($doc->doctype);
echo $doc->saveXML(), "\n";
var_dump($element->tagName);
var_dump($element->textContent);
?>
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<!DOCTYPE books [
<!ELEMENT parent (child1 , child2)>
<!ELEMENT child1 (#PCDATA)>
<!ELEMENT child2 (#PCDATA)>
]>
<container><parent/></container>

string(6) "parent"
string(0) ""
<?xml version="1.0"?>
<container><parent/></container>

string(6) "parent"
string(0) """#);
}

/// php-src `ext/dom/tests/delayed_freeing/element_uaf_crash.phpt`: Delayed freeing should not cause a UAF
#[test]
fn test_element_uaf_crash() {
    if skip_without_xml_native("test_element_uaf_crash") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$xml = new DomDocument();
$d = $xml->createElement("div");
$d->appendChild($b = $xml->createElement("b"));
$ret = $d->appendChild($xml->createElement("xxx"));
echo $xml->saveXML($d), "\n";
unset($d);

echo $ret->textContent, "Done\n";
"#);
    assert_exact(&out, r#"<div><b/><xxx/></div>
Done"#);
}

/// php-src `ext/dom/tests/delayed_freeing/entity_declaration.phpt`: Delayed freeing entity declaration
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createEntityReference"#]
fn test_entity_declaration() {
    if skip_without_xml_native("test_entity_declaration") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXML(<<<'XML'
<?xml version="1.0"?>
<!DOCTYPE books [
<!ENTITY test "entity is only for test purposes">
<!ENTITY myimage PUBLIC "-" "mypicture.gif" NDATA GIF>
]>
<container/>
XML);
$ref1 = $doc->createEntityReference("test");
$ref2 = $doc->createEntityReference("myimage");
$entity1 = $doc->doctype->entities[0];
$entity2 = $doc->doctype->entities[1];
if (strcmp($entity1->nodeName, $entity2->nodeName) < 0) {
    // Entity ordering depends on the addresses
    [$entity1, $entity2] = [$entity2, $entity1];
}
var_dump($entity1->nodeName, $entity1->parentNode->nodeName);
var_dump($entity2->nodeName, $entity2->parentNode->nodeName);
$doc->removeChild($doc->doctype);
var_dump($entity1->nodeName, $entity1->parentNode);
var_dump($entity2->nodeName, $entity2->parentNode);
"#);
    assert_exact(&out, r#"string(4) "test"
string(5) "books"
string(7) "myimage"
string(5) "books"
string(4) "test"
NULL
string(7) "myimage"
NULL"#);
}

/// php-src `ext/dom/tests/delayed_freeing/entity_reference.phpt`: Delayed freeing entity reference
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createElementNS"#]
fn test_entity_reference() {
    if skip_without_xml_native("test_entity_reference") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
$entityRef = $doc->appendChild($doc->createElementNS('some:ns', 'container'))
    ->appendChild($doc->createEntityReference('nbsp'));
echo $doc->saveXML(), "\n";
$entityRef->parentNode->remove();
echo $doc->saveXML(), "\n";
var_dump($entityRef->parentNode);
var_dump($entityRef->nodeName);
var_dump($entityRef->textContent);

$doc = new DOMDocument;
$doc->loadXML(<<<'XML'
<?xml version="1.0"?>
<!DOCTYPE books [
<!ENTITY test "entity is only for test purposes">
]>
<div/>
XML);
$entityRef = $doc->documentElement->appendChild($doc->createEntityReference('test'));
echo $doc->saveXML(), "\n";
$entityRef->parentNode->remove();
unset($doc);
var_dump($entityRef->nodeName);
var_dump($entityRef->textContent);
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<container xmlns="some:ns">&nbsp;</container>

<?xml version="1.0"?>

NULL
string(4) "nbsp"
string(0) ""
<?xml version="1.0"?>
<!DOCTYPE books [
<!ENTITY test "entity is only for test purposes">
]>
<div>&test;</div>

string(4) "test"
string(0) """#);
}

/// php-src `ext/dom/tests/delayed_freeing/gh9628_1.phpt`: GH-9628 (Implicitly removing nodes from \DOMDocument breaks existing references) - simple variation
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::importNode"#]
fn test_gh9628_1() {
    if skip_without_xml_native("test_gh9628_1") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$html = <<<'HTML'
<p xmlns="some:ns">
    <span id="1" xmlns:test="some:ns2">
        <strong>
            <span id="2">Test <test:test/></span>
        </strong>
    </span>
</p>
HTML;

$doc = new DOMDocument('1.0', 'UTF-8');
$doc->loadXML($html);

$outer_span = $doc->documentElement->firstElementChild;
$inner_span = $outer_span->firstElementChild->firstElementChild;
var_dump($inner_span->namespaceURI);

// Remove strong
$outer_span->firstElementChild->remove();

var_dump($inner_span->getAttribute('id'));
var_dump($inner_span->namespaceURI);

// Import test
$doc = new DOMDocument();
$doc->append($doc->importNode($outer_span, true), $doc->importNode($inner_span, true));
echo $doc->saveXML();

var_dump($inner_span->getAttribute('id'));
var_dump($inner_span->namespaceURI);
"#);
    assert_exact(&out, r#"string(7) "some:ns"
string(1) "2"
string(7) "some:ns"
<?xml version="1.0"?>
<span xmlns:test="some:ns2" xmlns="some:ns" id="1">
        
    </span>
<default:span xmlns:default="some:ns" xmlns:test="some:ns2" id="2">Test <test:test/></default:span>
string(1) "2"
string(7) "some:ns""#);
}

/// php-src `ext/dom/tests/delayed_freeing/gh9628_2.phpt`: GH-9628 (Implicitly removing nodes from \DOMDocument breaks existing references) - advanced variation
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_gh9628_2() {
    if skip_without_xml_native("test_gh9628_2") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$html = <<<'HTML'
<p>
    <span>
        <strong>
            <span>
                Test
            </span>
        </strong>
    </span>
</p>
HTML;

$doc = new \DOMDocument('1.0', 'UTF-8');
$doc->loadHTML(
    '<html><head><meta http-equiv="Content-Type" content="text/html; charset=utf-8" /></head><body>' . $html . '</body></html>'
);
$xpath = new \DOMXPath($doc);

$spans = [];
foreach ($xpath->query('//span') as $span) {
    $spans[] = $span;
}

\assert(\count($spans) === 2);

foreach ($spans as $span) {
    \assert($span instanceof \DOMElement);
    \assert($span->ownerDocument === $doc);
}

foreach ($spans as $span) {
    \assert($span instanceof \DOMElement);

    // This call will fail for the second `<span>`. It does not really
    // matter what is accessed here, the error message will be the same.
    $span->hasAttribute('test');

    while ($span->childNodes->length) {
        $span->removeChild($span->childNodes[0]);
    }
    $span->appendChild(
        $span->ownerDocument->createTextNode('Hello World')
    );
}

echo $spans[0]->textContent . "\n"; // Hello World;"#);
    assert_exact(&out, r#"Hello World"#);
}

/// php-src `ext/dom/tests/delayed_freeing/namespace_definition_crash.phpt`: Delayed freeing namespace definition should not crash
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createElementNS"#]
fn test_namespace_definition_crash() {
    if skip_without_xml_native("test_namespace_definition_crash") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
$doc->appendChild($doc->createElement('container'));
$child = $doc->documentElement->appendChild($doc->createElementNS('some:ns', 'child'));
$child_child = $child->appendChild($doc->createElementNS('some:ns', 'x'));

echo $doc->saveXML();

$child->remove();
echo $doc->saveXML();

unset($child);
var_dump($child_child->namespaceURI);
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<container><child xmlns="some:ns"><x/></child></container>
<?xml version="1.0"?>
<container/>
string(7) "some:ns""#);
}

/// php-src `ext/dom/tests/delayed_freeing/namespace_definition_crash_in_attribute.phpt`: Delayed freeing namespace definition should not crash in attribute
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createElementNS"#]
fn test_namespace_definition_crash_in_attribute() {
    if skip_without_xml_native("test_namespace_definition_crash_in_attribute") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
$doc->appendChild($doc->createElement('container'));
$child = $doc->documentElement->appendChild($doc->createElementNS('some:ns', 'child'));
$child_attr1 = $child->appendChild($doc->createAttributeNS('some:ns2', 'hello'));
$child_attr1->textContent = 'hello content 1';
$child_attr2 = $child->appendChild($doc->createAttribute('hello2'));
$child_attr1->textContent = 'hello content 2';
$attr3 = $child->appendChild($doc->createElementNS('some:ns', 'childcontainer'))
        ->appendChild($doc->createAttributeNS('some:ns', 'hello'));

echo $doc->saveXML();

$child->remove();
echo $doc->saveXML();

unset($child);

var_dump($child_attr1->textContent);
var_dump($child_attr2->textContent);
var_dump($child_attr1->namespaceURI);
var_dump($child_attr2->namespaceURI);
var_dump($attr3->textContent);
var_dump($attr3->namespaceURI);

$doc->documentElement->remove();

unset($child_attr1);
unset($child_attr2);
var_dump($attr3->namespaceURI);

echo $doc->saveXML($attr3), "\n";
echo $doc->saveXML($attr3->parentNode), "\n";
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<container xmlns:default="some:ns2" xmlns:default1="some:ns"><child xmlns="some:ns" default:hello="hello content 2" hello2=""><childcontainer default1:hello=""/></child></container>
<?xml version="1.0"?>
<container xmlns:default="some:ns2" xmlns:default1="some:ns"/>
string(15) "hello content 2"
string(0) ""
string(8) "some:ns2"
NULL
string(0) ""
string(7) "some:ns"
string(7) "some:ns"
 default1:hello=""
<?xml version="1.0"?>"#);
}

/// php-src `ext/dom/tests/delayed_freeing/namespace_xmlns_declaration.phpt`: Delayed freeing namespace xmlns declaration
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createElementNS"#]
fn test_namespace_xmlns_declaration() {
    if skip_without_xml_native("test_namespace_xmlns_declaration") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXML('<?xml version="1.0"?><container xmlns="http://php.net"/>');
$doc->documentElement->appendChild($el = $doc->createElementNS('http://php.net', 'example'));
echo $doc->saveXML(), "\n";

$doc->documentElement->remove();
echo $doc->saveXML(), "\n";

var_dump($el->namespaceURI);
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<container xmlns="http://php.net"><example/></container>

<?xml version="1.0"?>

string(14) "http://php.net""#);
}

/// php-src `ext/dom/tests/delayed_freeing/namespace_xmlns_declaration_attribute_variation.phpt`: Delayed freeing namespace xmlns declaration - attribute variation
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createElementNS"#]
fn test_namespace_xmlns_declaration_attribute_variation() {
    if skip_without_xml_native("test_namespace_xmlns_declaration_attribute_variation") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXML('<?xml version="1.0"?><container xmlns="http://php.net"/>');
$doc->documentElement->appendChild($doc->createElementNS('http://php.net', 'example'));
echo $doc->saveXML(), "\n";

$declaration = $doc->documentElement->getAttributeNode('xmlns');
var_dump($declaration->nodeValue);

$doc->documentElement->remove();
echo $doc->saveXML(), "\n";

var_dump($declaration->nodeValue);
unset($doc);
var_dump($declaration->parentNode->nodeName);
var_dump($declaration->parentNode->childNodes[0]->nodeName);

$declaration->parentNode->childNodes[0]->remove();
var_dump($declaration->parentNode->nodeName);
var_dump($declaration->parentNode->childNodes[0]);
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<container xmlns="http://php.net"><example/></container>

string(14) "http://php.net"
<?xml version="1.0"?>

string(14) "http://php.net"
string(9) "container"
string(7) "example"
string(9) "container"
NULL"#);
}

/// php-src `ext/dom/tests/delayed_freeing/notation_declaration.phpt`: Delayed freeing notation declaration
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_notation_declaration() {
    if skip_without_xml_native("test_notation_declaration") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXML(<<<'XML'
<?xml version="1.0"?>
<!DOCTYPE books [
<!NOTATION myNotation SYSTEM "test.dtd">
]>
<container/>
XML);
$notation = $doc->doctype->notations[0];
var_dump($notation->nodeName, $notation->publicId, $notation->systemId);
$doc->removeChild($doc->doctype);
var_dump($notation->nodeName, $notation->publicId, $notation->systemId);
unset($doc);
var_dump($notation->nodeName, $notation->publicId, $notation->systemId);
"#);
    assert_exact(&out, r#"string(10) "myNotation"
string(0) ""
string(8) "test.dtd"
string(10) "myNotation"
string(0) ""
string(8) "test.dtd"
string(10) "myNotation"
string(0) ""
string(8) "test.dtd""#);
}

/// php-src `ext/dom/tests/delayed_freeing/processing_instruction.phpt`: Delayed freeing processing instruction
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createElementNS"#]
fn test_processing_instruction() {
    if skip_without_xml_native("test_processing_instruction") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
$pi = $doc->appendChild($doc->createElementNS('some:ns', 'container'))
    ->appendChild($doc->createProcessingInstruction('hello', 'world'));
echo $doc->saveXML(), "\n";
$pi->parentNode->remove();
echo $doc->saveXML(), "\n";
var_dump($pi->parentNode);
var_dump($pi->nodeValue);
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<container xmlns="some:ns"><?hello world?></container>

<?xml version="1.0"?>

NULL
string(5) "world""#);
}

/// php-src `ext/dom/tests/delayed_freeing/text_node.phpt`: Delayed freeing text node
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createTextNode"#]
fn test_text_node() {
    if skip_without_xml_native("test_text_node") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$doc = new DOMDocument;
// Text nodes are special as the underlying library has a tendency to merge adjacent ones
$text1 = $doc->appendChild($doc->createElement('container'))
    ->appendChild($doc->createTextNode('my text 1'));
$text2 = $doc->documentElement->appendChild($doc->createTextNode('my text 2'));
echo $doc->saveXML(), "\n";
$text1->parentNode->remove();
echo $doc->saveXML(), "\n";
echo $doc->saveXML($text1), "\n";
echo $doc->saveXML($text2), "\n";
var_dump($text1->parentNode, $text2->parentNode);
var_dump($text1->nextSibling, $text2->previousSibling);
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<container>my text 1my text 2</container>

<?xml version="1.0"?>

my text 1
my text 2
NULL
NULL
NULL
NULL"#);
}

/// php-src `ext/dom/tests/delayed_freeing/without_contructor.phpt`: Tests without running the constructor
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::importNode"#]
fn test_without_contructor() {
    if skip_without_xml_native("test_without_contructor") {
        return;
    }
    let out = run_case(r#"delayed_freeing"#, &[], r#"<?php
$rc = new ReflectionClass('DOMNode');
$node = $rc->newInstanceWithoutConstructor();

// Property read test
try {
    var_dump($node);
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

// Call test
try {
    $node->removeChild($node);
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

// Import test
$doc = new DOMDocument;
try {
    $doc->appendChild($doc->importNode($node));
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"object(DOMNode)#2 (0) {
}
Invalid State Error
Couldn't fetch DOMNode
Couldn't fetch DOMNode"#);
}
