//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for the remaining legacy DOM behaviours (cloning, canonicalization, entities, ...), as Elephc codegen tests.
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

/// php-src `ext/dom/tests/DomNodeList_count.phpt`: Test count nodes in DOMNodeList
#[test]
fn test_dom_node_list_count() {
    if skip_without_xml_native("test_dom_node_list_count") {
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
var_dump($root->childNodes->length);
var_dump($root->childNodes->count());
var_dump(count($root->childNodes));

"#);
    assert_exact(&out, r#"int(7)
int(7)
int(7)"#);
}

/// php-src `ext/dom/tests/ParentNode_append_fragment_text_coalesce.phpt`: Text coalesce bug when appending fragment with text nodes
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createDocumentFragment"#]
fn test_parent_node_append_fragment_text_coalesce() {
    if skip_without_xml_native("test_parent_node_append_fragment_text_coalesce") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$document = new DOMDocument();
$document->loadXML('<root/>');

$sut = $document->createDocumentFragment();
for($i = 0; $i < 10; $i++) {
	$textNode = $document->createTextNode("Node$i");
	$sut->append($textNode);
}

$document->documentElement->append($sut);
echo $document->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<root>Node0Node1Node2Node3Node4Node5Node6Node7Node8Node9</root>"#);
}

/// php-src `ext/dom/tests/bug_lifetime_parentNode_getAttributeNodeNS.phpt`: Lifetime issue with parentNode on getAttributeNodeNS()
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_bug_lifetime_parent_node_get_attribute_node_ns() {
    if skip_without_xml_native("test_bug_lifetime_parent_node_get_attribute_node_ns") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xmlString = '<?xml version="1.0" encoding="utf-8" ?>
<root xmlns="http://ns" xmlns:ns2="http://ns2">
    <ns2:child />
</root>';

$xml=new DOMDocument();
$xml->loadXML($xmlString);
$ns2 = $xml->documentElement->getAttributeNodeNS("http://www.w3.org/2000/xmlns/", "ns2");
$ns2->parentNode->remove();
var_dump($ns2->parentNode->localName);

"#);
    assert_exact(&out, r#"string(4) "root""#);
}

/// php-src `ext/dom/tests/c14n_no_nodeset_returned.phpt`: Not returning a node set for C14N
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_c14n_no_nodeset_returned() {
    if skip_without_xml_native("test_c14n_no_nodeset_returned") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument;
$dom->loadXML('<root>test</root>');
try {
    $dom->documentElement->C14N(true, false, ['query' => 'string(./root/text())']);
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
"#);
    assert_exact(&out, r#"XPath query did not return a nodeset"#);
}

/// php-src `ext/dom/tests/canonicalization.phpt`: Test: Canonicalization - C14N()
#[test]
#[ignore = r#"DOM port pending: unsupported EIR backend feature: array_push for PHP type Void (function main, instruction InstId(511))"#]
fn test_canonicalization() {
    if skip_without_xml_native("test_canonicalization") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<<EOXML
<?xml version="1.0" encoding="ISO-8859-1" ?>
<foo xmlns="http://www.example.com/ns/foo"
     xmlns:fubar="http://www.example.com/ns/fubar" xmlns:test="urn::test"><contain>
  <bar><test1 /></bar>
  <bar><test2 /></bar>
  <fubar:bar xmlns:fubar="http://www.example.com/ns/fubar"><test3 /></fubar:bar>
  <fubar:bar><test4 /></fubar:bar>
<!-- this is a comment -->
</contain>
</foo>
EOXML;

$dom = new DOMDocument();
$dom->loadXML($xml);
$doc = $dom->documentElement->firstChild;

$newDom = Dom\XMLDocument::createFromString($xml);
$newDoc = $newDom->documentElement->firstChild;
$counter = 0;

function check($doc, $newDoc, ...$args) {
  global $counter;
  $counter++;
  echo $doc->C14N(...$args)."\n\n";
  if ($doc->C14N(...$args) !== $newDoc->C14N(...$args)) {
    var_dump($doc->C14N(...$args), $newDoc->C14N(...$args));
    throw new Error("mismatch: $counter");
  }
}

/* inclusive/without comments first child element of doc element is context. */
check($doc, $newDoc);

/* exclusive/without comments first child element of doc element is context. */
check($doc, $newDoc, TRUE);

/* inclusive/with comments first child element of doc element is context. */
check($doc, $newDoc, FALSE, TRUE);

/* exclusive/with comments first child element of doc element is context. */
check($doc, $newDoc, TRUE, TRUE);

/* exclusive/without comments using xpath query. */
check($doc, $newDoc, TRUE, FALSE, array('query'=>'(//. | //@* | //namespace::*)'))."\n\n";

/* exclusive/without comments first child element of doc element is context.
   using xpath query with registered namespace.
   test namespace prefix is also included. */
check($doc, $newDoc, TRUE, FALSE,
                array('query'=>'(//a:contain | //a:bar | .//namespace::*)',
                      'namespaces'=>array('a'=>'http://www.example.com/ns/foo')),
                array('test'));

/* exclusive/without comments first child element of doc element is context.
   test namespace prefix is also included */
check($doc, $newDoc, TRUE, FALSE, NULL, array('test'));
"#);
    assert_exact(&out, r#"<contain xmlns="http://www.example.com/ns/foo" xmlns:fubar="http://www.example.com/ns/fubar" xmlns:test="urn::test">
  <bar><test1></test1></bar>
  <bar><test2></test2></bar>
  <fubar:bar><test3></test3></fubar:bar>
  <fubar:bar><test4></test4></fubar:bar>

</contain>

<contain xmlns="http://www.example.com/ns/foo">
  <bar><test1></test1></bar>
  <bar><test2></test2></bar>
  <fubar:bar xmlns:fubar="http://www.example.com/ns/fubar"><test3></test3></fubar:bar>
  <fubar:bar xmlns:fubar="http://www.example.com/ns/fubar"><test4></test4></fubar:bar>

</contain>

<contain xmlns="http://www.example.com/ns/foo" xmlns:fubar="http://www.example.com/ns/fubar" xmlns:test="urn::test">
  <bar><test1></test1></bar>
  <bar><test2></test2></bar>
  <fubar:bar><test3></test3></fubar:bar>
  <fubar:bar><test4></test4></fubar:bar>
<!-- this is a comment -->
</contain>

<contain xmlns="http://www.example.com/ns/foo">
  <bar><test1></test1></bar>
  <bar><test2></test2></bar>
  <fubar:bar xmlns:fubar="http://www.example.com/ns/fubar"><test3></test3></fubar:bar>
  <fubar:bar xmlns:fubar="http://www.example.com/ns/fubar"><test4></test4></fubar:bar>
<!-- this is a comment -->
</contain>

<foo xmlns="http://www.example.com/ns/foo"><contain>
  <bar><test1></test1></bar>
  <bar><test2></test2></bar>
  <fubar:bar xmlns:fubar="http://www.example.com/ns/fubar"><test3></test3></fubar:bar>
  <fubar:bar xmlns:fubar="http://www.example.com/ns/fubar"><test4></test4></fubar:bar>

</contain>
</foo>

<contain xmlns="http://www.example.com/ns/foo" xmlns:test="urn::test"><bar></bar><bar></bar></contain>

<contain xmlns="http://www.example.com/ns/foo" xmlns:test="urn::test">
  <bar><test1></test1></bar>
  <bar><test2></test2></bar>
  <fubar:bar xmlns:fubar="http://www.example.com/ns/fubar"><test3></test3></fubar:bar>
  <fubar:bar xmlns:fubar="http://www.example.com/ns/fubar"><test4></test4></fubar:bar>

</contain>"#);
}

/// php-src `ext/dom/tests/canonicalization_special_nodes.phpt`: Test: Canonicalization with special nodes
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_canonicalization_special_nodes() {
    if skip_without_xml_native("test_canonicalization_special_nodes") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<<EOXML
<?xml version="1.0"?>
<!DOCTYPE doc [
]>
<doc xmlns="">
    <![CDATA[bar]]>
    <!-- x -->
    <temp xmlns=""/>
    <?pi-no-data          ?>
</doc>
EOXML;

$dom = new DOMDocument();
$dom->loadXML($xml);
$doc = $dom->documentElement;
echo $doc->C14N(withComments: true);
echo $doc->C14N(withComments: false);

"#);
    assert_exact(&out, r#"<doc>
    bar
    <!-- x -->
    <temp></temp>
    <?pi-no-data?>
</doc><doc>
    bar
    
    <temp></temp>
    <?pi-no-data?>
</doc>"#);
}

/// php-src `ext/dom/tests/childNodes_current_crash.phpt`: Crash in childNodes iterator current()
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_child_nodes_current_crash() {
    if skip_without_xml_native("test_child_nodes_current_crash") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML('<foo>foo1</foo>');

$nodes = $dom->documentElement->childNodes;
$iter = $nodes->getIterator();

var_dump($iter->valid());
var_dump($iter->current()?->wholeText);
$iter->next();
var_dump($iter->valid());
var_dump($iter->current()?->wholeText);

"#);
    assert_exact(&out, r#"bool(true)
string(4) "foo1"
bool(false)
NULL"#);
}

/// php-src `ext/dom/tests/clone_attribute_namespace_01.phpt`: Cloning an attribute should retain its namespace 01
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_clone_attribute_namespace_01() {
    if skip_without_xml_native("test_clone_attribute_namespace_01") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

function createTestDocument() {
    $dom = new DOMDocument;
    $dom->loadXML('<?xml version="1.0"?><container/>');
    $dom->documentElement->setAttributeNs("some:ns", "foo:bar", "value");

    $attr = $dom->documentElement->getAttributeNodeNs("some:ns", "bar");
    $clone = $attr->cloneNode(true);

    return [$dom, $clone];
}

[$dom, $clone] = createTestDocument();
var_dump($clone->prefix, $clone->namespaceURI);

echo "--- Re-adding a namespaced attribute ---\n";

[$dom, $clone] = createTestDocument();
$dom->documentElement->removeAttributeNs("some:ns", "bar");
echo $dom->saveXML();
$dom->documentElement->setAttributeNodeNs($clone);
echo $dom->saveXML();

echo "--- Re-adding a namespaced attribute, with the namespace deleted (setAttributeNodeNs variation) ---\n";

function readd_test(string $method) {
    [$dom, $clone] = createTestDocument();
    $dom->documentElement->removeAttributeNs("some:ns", "bar");
    $dom->documentElement->removeAttribute("xmlns:foo");
    echo $dom->saveXML();
    $child = $dom->documentElement->appendChild($dom->createElement("child"));
    $child->{$method}($clone);
    echo $dom->saveXML();
}

readd_test("setAttributeNodeNs");

echo "--- Re-adding a namespaced attribute, with the namespace deleted (setAttributeNode variation) ---\n";

readd_test("setAttributeNode");

echo "--- Re-adding a namespaced attribute, with the namespace deleted (appendChild variation) ---\n";

readd_test("appendChild");

echo "--- Removing the document reference should not crash ---\n";

[$dom, $clone] = createTestDocument();
unset($dom);
var_dump($clone->prefix, $clone->namespaceURI);

"#);
    assert_exact(&out, r#"string(3) "foo"
string(7) "some:ns"
--- Re-adding a namespaced attribute ---
<?xml version="1.0"?>
<container xmlns:foo="some:ns"/>
<?xml version="1.0"?>
<container xmlns:foo="some:ns" foo:bar="value"/>
--- Re-adding a namespaced attribute, with the namespace deleted (setAttributeNodeNs variation) ---
<?xml version="1.0"?>
<container/>
<?xml version="1.0"?>
<container><child xmlns:foo="some:ns" foo:bar="value"/></container>
--- Re-adding a namespaced attribute, with the namespace deleted (setAttributeNode variation) ---
<?xml version="1.0"?>
<container/>
<?xml version="1.0"?>
<container><child xmlns:foo="some:ns" foo:bar="value"/></container>
--- Re-adding a namespaced attribute, with the namespace deleted (appendChild variation) ---
<?xml version="1.0"?>
<container/>
<?xml version="1.0"?>
<container><child xmlns:foo="some:ns" foo:bar="value"/></container>
--- Removing the document reference should not crash ---
string(3) "foo"
string(7) "some:ns""#);
}

/// php-src `ext/dom/tests/clone_attribute_namespace_02.phpt`: Cloning an attribute should retain its namespace 02
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_clone_attribute_namespace_02() {
    if skip_without_xml_native("test_clone_attribute_namespace_02") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<container xmlns:foo="some:ns" foo:bar="1">
    <child xmlns:foo="some:other"/>
</container>
XML);

$clone = $dom->documentElement->getAttributeNodeNs("some:ns", "bar")->cloneNode(true);
$dom->documentElement->firstElementChild->setAttributeNodeNs($clone);

echo $dom->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<container xmlns:foo="some:ns" foo:bar="1">
    <child xmlns:foo="some:other" xmlns:foo1="some:ns" foo1:bar="1"/>
</container>"#);
}

/// php-src `ext/dom/tests/clone_list_map_collection.phpt`: Cloning node lists, maps, and collections should fail
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_clone_list_map_collection() {
    if skip_without_xml_native("test_clone_list_map_collection") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML('<root a="1"><a/></root>');
try {
    clone $dom->documentElement->attributes;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
try {
    clone $dom->documentElement->childNodes;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

$dom = Dom\XMLDocument::createFromString('<!DOCTYPE root [<!ENTITY foo "">]><root a="1"><a/></root>');
try {
    clone $dom->documentElement->attributes;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
try {
    clone $dom->documentElement->childNodes;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
try {
    clone $dom->documentElement->children;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
try {
    clone $dom->doctype->entities;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Trying to clone an uncloneable object of class DOMNamedNodeMap
Trying to clone an uncloneable object of class DOMNodeList
Trying to clone an uncloneable object of class Dom\NamedNodeMap
Trying to clone an uncloneable object of class Dom\NodeList
Trying to clone an uncloneable object of class Dom\HTMLCollection
Trying to clone an uncloneable object of class Dom\DtdNamedNodeMap"#);
}

/// php-src `ext/dom/tests/clone_nodes.phpt`: Clone nodes
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::firstElementChild"#]
fn test_clone_nodes() {
    if skip_without_xml_native("test_clone_nodes") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

echo "-- Clone DOMNameSpaceNode --\n";

$doc = new DOMDocument;
$doc->loadXML('<foo xmlns="http://php.net/test" xmlns:foo="urn:foo" />');

$attr = $doc->documentElement->getAttributeNode('xmlns');
var_dump($attr);

$attrClone = clone $attr;
var_dump($attrClone->nodeValue);
var_dump($attrClone->parentNode->nodeName);

unset($doc);
unset($attr);

var_dump($attrClone->nodeValue);
var_dump($attrClone->parentNode->nodeName);

echo "-- Clone DOMNode --\n";

$doc = new DOMDocument;
$doc->loadXML('<foo><bar/></foo>');

$bar = $doc->documentElement->firstChild;
$barClone = clone $bar;
$bar->remove();
unset($bar);

var_dump($barClone->nodeName);

$doc->firstElementChild->remove();
unset($doc);

var_dump($barClone->nodeName);
var_dump($barClone->parentNode);

"#);
    assert_exact(&out, r#"-- Clone DOMNameSpaceNode --
object(DOMNameSpaceNode)#3 (10) {
  ["nodeName"]=>
  string(5) "xmlns"
  ["nodeValue"]=>
  string(19) "http://php.net/test"
  ["nodeType"]=>
  int(18)
  ["prefix"]=>
  string(0) ""
  ["localName"]=>
  string(5) "xmlns"
  ["namespaceURI"]=>
  string(19) "http://php.net/test"
  ["isConnected"]=>
  bool(true)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["parentNode"]=>
  string(22) "(object value omitted)"
  ["parentElement"]=>
  string(22) "(object value omitted)"
}
string(19) "http://php.net/test"
string(3) "foo"
string(19) "http://php.net/test"
string(3) "foo"
-- Clone DOMNode --
string(3) "bar"
string(3) "bar"
NULL"#);
}

/// php-src `ext/dom/tests/dom_comment_basic.phpt`: DOM Comment : Basic Functionality
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_dom_comment_basic() {
    if skip_without_xml_native("test_dom_comment_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$xml = <<< EOXML
<?xml version="1.0" encoding="ISO-8859-1"?>
<courses>
    <!-- Hello World! -->
</courses>
EOXML;

$dom = new DOMDocument();
$dom->loadXML($xml);
$root = $dom->documentElement;
var_dump($root->hasChildNodes());
$children = $root->childNodes;

for ($index = 0; $index < $children->length; $index++) {
    echo "--- child $index ---\n";
    $current = $children->item($index);
    echo get_class($current), "\n";
    var_dump($current->textContent);
}
"#);
    assert_exact(&out, r#"bool(true)
--- child 0 ---
DOMText
string(5) "
    "
--- child 1 ---
DOMComment
string(14) " Hello World! "
--- child 2 ---
DOMText
string(1) "
""#);
}

/// php-src `ext/dom/tests/dom_comment_variation.phpt`: DOM Comment : Variation
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_dom_comment_variation() {
    if skip_without_xml_native("test_dom_comment_variation") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$xml = <<< EOXML
<?xml version="1.0" encoding="ISO-8859-1"?><courses><!-- Hello World! --></courses>
EOXML;

$dom = new DOMDocument();
$dom->loadXML($xml);
$root = $dom->documentElement;
var_dump($root->hasChildNodes());
$children = $root->childNodes;

for ($index = 0; $index < $children->length; $index++) {
    echo "--- child $index ---\n";
    $current = $children->item($index);
    echo get_class($current), "\n";
    var_dump($current->textContent);
}
"#);
    assert_exact(&out, r#"bool(true)
--- child 0 ---
DOMComment
string(14) " Hello World! ""#);
}

/// php-src `ext/dom/tests/dom_create_element.phpt`: Test 1: Creating Elements with and without Namespaces
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createElementNS"#]
fn test_dom_create_element() {
    if skip_without_xml_native("test_dom_create_element") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

print " 1 DOMDocument::createElement('valid')\n";
try {
    $dom = new domDocument;
    $dom->createElement('valid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print " 2 DOMDocument::createElement('-invalid')\n";
try {
    $dom = new domDocument;
    $dom->createElement('-invalid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print " 3 DOMDocument::createElement(' ')\n";
try {
    $dom = new domDocument;
    $dom->createElement(' ');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print " 4 DOMDocument::createElement('prefix:valid')\n";
try {
    $dom = new domDocument;
    $dom->createElement('prefix:valid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print " 5 DOMDocument::createElementNS('http://valid.com', 'valid')\n";
try {
    $dom = new domDocument;
    $dom->createElementNS('http://valid.com', 'valid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print " 6 DOMDocument::createElementNS('http://valid.com', 'prefix:valid')\n";
try {
    $dom = new domDocument;
    $dom->createElementNS('http://valid.com', 'prefix:valid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print " 7 DOMDocument::createElementNS('http://valid.com', '-invalid')\n";
try {
    $dom = new domDocument;
    $dom->createElementNS('http://valid.com', '-invalid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print " 8 DOMDocument::createElementNS('http://valid.com', 'prefix:-invalid')\n";
try {
    $dom = new domDocument;
    $dom->createElementNS('http://valid.com', 'prefix:-invalid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print " 9 DOMDocument::createElementNS('', 'prefix:invalid')\n";
try {
    $dom = new domDocument;
    $dom->createElementNS('', 'prefix:invalid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "10 DOMDocument::createElementNS('http://valid.com', 'prefix:valid:invalid')\n";
try {
    $dom = new domDocument;
    $dom->createElementNS('http://valid.com', 'prefix:valid:invalid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "11 DOMDocument::createElementNS('http://valid.com', '-prefix:valid')\n";
try {
    $dom = new domDocument;
    $dom->createElementNS('http://valid.com', '-prefix:valid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "12 DOMDocument::createElementNS('-', 'prefix:valid')\n";
try {
    $dom = new domDocument;
    $dom->createElementNS('-', 'prefix:valid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}


print "13 DOMElement::__construct('valid')\n";
try {
    $element = new DomElement('valid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "14 DOMElement::__construct('-invalid')\n";
try {
    $element = new DomElement('-invalid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "15 DOMElement::__construct(' ')\n";
try {
    $element = new DomElement(' ');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "16 DOMElement::__construct('prefix:valid')\n";
try {
    $element = new DomElement('prefix:valid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "17 DOMElement::__construct('valid', '', 'http://valid.com')\n";
try {
    $element = new DomElement('valid', '', 'http://valid.com');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "18 DOMElement::__construct('prefix:valid', '', 'http://valid.com')\n";
try {
    $element = new DomElement('prefix:valid', '', 'http://valid.com');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "19 DOMElement::__construct('-invalid', '', 'http://valid.com')\n";
try {
    $element = new DomElement('-invalid', '', 'http://valid.com');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "20 DOMElement::__construct('prefix:-invalid', '', 'http://valid.com')\n";
try {
    $element = new DomElement('prefix:-invalid', '', 'http://valid.com');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "21 DOMElement::__construct('prefix:invalid', '', '')\n";
try {
    $element = new DomElement('prefix:invalid', '', '');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "22 DOMElement::__construct('prefix:valid:invalid', '', 'http://valid.com')\n";
try {
    $element = new DomElement('prefix:valid:invalid', '', 'http://valid.com');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "23 DOMElement::__construct('-prefix:valid', '', 'http://valid.com')\n";
try {
    $element = new DomElement('-prefix:valid', '', 'http://valid.com');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "24 DOMElement::__construct('prefix:valid', '', '-')\n";
try {
    $element = new DomElement('prefix:valid', '', '-');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

/* the qualifiedName has a prefix and the  namespaceURI is null */

print "25 DOMDocument::createElementNS('', 'prefix:valid')\n";
try {
    $dom = new domDocument;
    $dom->createElementNS('', 'prefix:valid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

/* the qualifiedName has a prefix that is "xml" and the  namespaceURI
   is different from "http://www.w3.org/XML/1998/namespace" [XML Namespaces] */

print "26 DOMDocument::createElementNS('http://wrong.namespaceURI.com', 'xml:valid')\n";
try {
    $dom = new domDocument;
    $dom->createElementNS('http://wrong.namespaceURI.com', 'xml:valid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "27 DOMElement::__construct('xml:valid', '', 'http://wrong.namespaceURI.com')\n";
try {
    $element = new DomElement('xml:valid', '', 'http://wrong.namespaceURI.com');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

/* This is okay because we reuse the xml namespace from the document */
print "28 DOMDocument::createElementNS('http://www.w3.org/XML/1998/namespace', 'xml:valid')\n";
try {
    $dom = new domDocument;
    $dom->createElementNS('http://www.w3.org/XML/1998/namespace', 'xml:valid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

/* There used to be a 29 here that tested DOMElement::__construct('xml:valid', '', 'http://www.w3.org/XML/1998/namespace').
 * In libxml2 version 2.12 or prior this didn't work because the xml namespace isn't there and you can't create it without
 * a document. Starting from libxml2 version 2.13 it does actually work because the XML namespace is statically defined.
 * The behaviour from version 2.13 is actually the desired behaviour anyway. */


/* the qualifiedName or its prefix is "xmlns" and the  namespaceURI is
   different from  "http://www.w3.org/2000/xmlns/" */

print "30 DOMDocument::createElementNS('http://wrong.namespaceURI.com', 'xmlns:valid')\n";
try {
    $dom = new domDocument;
    $dom->createElementNS('http://wrong.namespaceURI.com', 'xmlns:valid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "31 DOMElement::__construct('xmlns:valid', '', 'http://wrong.namespaceURI.com')\n";
try {
    $element = new DomElement('xmlns:valid', '', 'http://wrong.namespaceURI.com');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "32 DOMDocument::createElementNS('http://www.w3.org/2000/xmlns/', 'xmlns:valid')\n";
try {
    $dom = new domDocument;
    $dom->createElementNS('http://www.w3.org/2000/xmlns/', 'xmlns:valid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "33 DOMElement::__construct('xmlns:valid', '', 'http://www.w3.org/2000/xmlns/')\n";
try {
    $element = new DomElement('xmlns:valid', '', 'http://www.w3.org/2000/xmlns/');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

/* the namespaceURI is "http://www.w3.org/2000/xmlns/" and neither the
   qualifiedName nor its prefix is "xmlns". */

print "34 DOMDocument::createElementNS('http://www.w3.org/2000/xmlns/', 'wrongprefix:valid')\n";
try {
    $dom = new domDocument;
    $dom->createElementNS('http://www.w3.org/2000/xmlns/', 'wrongprefix:valid');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}

print "35 DOMElement::__construct('wrongprefix:valid', '', 'http://www.w3.org/2000/xmlns/')\n";
try {
    $element = new DomElement('wrongprefix:valid', '', 'http://www.w3.org/2000/xmlns/');
    print "valid\n";
} catch (Exception $e) {
    print $e->getMessage() . "\n";
}



"#);
    assert_exact(&out, r#"1 DOMDocument::createElement('valid')
valid
 2 DOMDocument::createElement('-invalid')
Invalid Character Error
 3 DOMDocument::createElement(' ')
Invalid Character Error
 4 DOMDocument::createElement('prefix:valid')
valid
 5 DOMDocument::createElementNS('http://valid.com', 'valid')
valid
 6 DOMDocument::createElementNS('http://valid.com', 'prefix:valid')
valid
 7 DOMDocument::createElementNS('http://valid.com', '-invalid')
Namespace Error
 8 DOMDocument::createElementNS('http://valid.com', 'prefix:-invalid')
Namespace Error
 9 DOMDocument::createElementNS('', 'prefix:invalid')
Namespace Error
10 DOMDocument::createElementNS('http://valid.com', 'prefix:valid:invalid')
Namespace Error
11 DOMDocument::createElementNS('http://valid.com', '-prefix:valid')
Namespace Error
12 DOMDocument::createElementNS('-', 'prefix:valid')
valid
13 DOMElement::__construct('valid')
valid
14 DOMElement::__construct('-invalid')
Invalid Character Error
15 DOMElement::__construct(' ')
Invalid Character Error
16 DOMElement::__construct('prefix:valid')
Namespace Error
17 DOMElement::__construct('valid', '', 'http://valid.com')
valid
18 DOMElement::__construct('prefix:valid', '', 'http://valid.com')
valid
19 DOMElement::__construct('-invalid', '', 'http://valid.com')
Invalid Character Error
20 DOMElement::__construct('prefix:-invalid', '', 'http://valid.com')
Namespace Error
21 DOMElement::__construct('prefix:invalid', '', '')
Namespace Error
22 DOMElement::__construct('prefix:valid:invalid', '', 'http://valid.com')
Namespace Error
23 DOMElement::__construct('-prefix:valid', '', 'http://valid.com')
Invalid Character Error
24 DOMElement::__construct('prefix:valid', '', '-')
valid
25 DOMDocument::createElementNS('', 'prefix:valid')
Namespace Error
26 DOMDocument::createElementNS('http://wrong.namespaceURI.com', 'xml:valid')
Namespace Error
27 DOMElement::__construct('xml:valid', '', 'http://wrong.namespaceURI.com')
Namespace Error
28 DOMDocument::createElementNS('http://www.w3.org/XML/1998/namespace', 'xml:valid')
valid
30 DOMDocument::createElementNS('http://wrong.namespaceURI.com', 'xmlns:valid')
Namespace Error
31 DOMElement::__construct('xmlns:valid', '', 'http://wrong.namespaceURI.com')
Namespace Error
32 DOMDocument::createElementNS('http://www.w3.org/2000/xmlns/', 'xmlns:valid')
valid
33 DOMElement::__construct('xmlns:valid', '', 'http://www.w3.org/2000/xmlns/')
valid
34 DOMDocument::createElementNS('http://www.w3.org/2000/xmlns/', 'wrongprefix:valid')
Namespace Error
35 DOMElement::__construct('wrongprefix:valid', '', 'http://www.w3.org/2000/xmlns/')
Namespace Error"#);
}

/// php-src `ext/dom/tests/dom_import_simplexml.phpt`: Interop Test: Import from SimpleXML
#[test]
#[ignore = r#"DOM port pending: Undefined function: dom_import_simplexml"#]
fn test_dom_import_simplexml() {
    if skip_without_xml_native("test_dom_import_simplexml") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$s = simplexml_load_file(__DIR__."/book.xml");
if(!$s) {
  echo "Error while loading the document\n";
  exit(0);
}
$dom = dom_import_simplexml($s);
print $dom->ownerDocument->saveXML();

// This should fail because it has been imported already above in legacy DOM
try {
    Dom\import_simplexml($s);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
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
Dom\import_simplexml(): Argument #1 ($node) must not be already imported as a DOMNode"#);
}

/// php-src `ext/dom/tests/dom_set_attr_node.phpt`: Test: setAttributeNode()
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_dom_set_attr_node() {
    if skip_without_xml_native("test_dom_set_attr_node") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$xml = <<<HERE
<?xml version="1.0" ?>
<root a="b" />
HERE;

$xml2 = <<<HERE
<?xml version="1.0" ?>
<doc2 />
HERE;

$dom = new DOMDocument();
$dom->loadXML($xml);
$root = $dom->documentElement;
$attr = $root->getAttributeNode('a');

$dom2 = new DOMDocument();
$dom2->loadXML($xml2);
$root2 = $dom2->documentElement;
try {
    $root2->setAttributeNode($attr);
} catch (domexception $e) {
ob_start();
    var_dump($e);
    $contents = ob_get_contents();
    ob_end_clean();
    echo preg_replace('/object\(DOMAttr\).+\{.*?\}/s', 'DOMAttr', $contents);
}

"#);
    assert_format(&out, r#"object(DOMException)#%d (7) {
  ["message":protected]=>
  string(20) "Wrong Document Error"
  ["string":"Exception":private]=>
  string(0) ""
  ["code"]=>
  int(4)
  ["file":protected]=>
  string(%d) "%stest.php"
  ["line":protected]=>
  int(%d)
  ["trace":"Exception":private]=>
  array(1) {
    [0]=>
    array(6) {
      ["file"]=>
      string(%d) "%stest.php"
      ["line"]=>
      int(%d)
      ["function"]=>
      string(16) "setAttributeNode"
      ["class"]=>
      string(10) "DOMElement"
      ["type"]=>
      string(2) "->"
      ["args"]=>
      array(1) {
        [0]=>
        DOMAttr
      }
    }
  }
  ["previous":"Exception":private]=>
  NULL
}"#);
}

/// php-src `ext/dom/tests/dom_xinclude.phpt`: Test: Xinclude and Streams
#[test]
#[ignore = r#"DOM port pending: Method DOMDocument::loadXML parameter $source expects string, got string|false"#]
fn test_dom_xinclude() {
    if skip_without_xml_native("test_dom_xinclude") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$data = file_get_contents(__DIR__."/xinclude.xml");
$reldir = str_replace(getcwd(),".",__DIR__);
if (DIRECTORY_SEPARATOR == '\\') {
    $reldir = str_replace('\\',"/", $reldir);
}
$data = str_replace('compress.zlib://ext/dom/tests/','compress.zlib://'.$reldir."/", $data);

$dom = new domdocument;
$dom->loadXML($data);
$dom->xinclude();
print $dom->saveXML()."\n";
foreach ($dom->documentElement->childNodes as $node) {
    print $node->nodeName."\n";
}

$dom = Dom\XMLDocument::createFromString($data);
$dom->xinclude();
print $dom->saveXML()."\n";
foreach ($dom->documentElement->childNodes as $node) {
    print $node->nodeName."\n";
}
"#);
    assert_format(&out, r#"<?xml version="1.0"?>
<foo xmlns:xi="http://www.w3.org/2001/XInclude">
    <book xml:base="compress.zlib://%sbook.xml">
  <title>The Grapes of Wrath</title>
  <author>John Steinbeck</author>
 </book><book xml:base="compress.zlib://%sbook.xml">
  <title>The Pearl</title>
  <author>John Steinbeck</author>
 </book>
 </foo>

#text
book
book
#text
<?xml version="1.0" encoding="UTF-8"?>
<foo xmlns:xi="http://www.w3.org/2001/XInclude">
    <book xml:base="compress.zlib://./ext/dom/tests/book.xml">
  <title>The Grapes of Wrath</title>
  <author>John Steinbeck</author>
 </book><book xml:base="compress.zlib://./ext/dom/tests/book.xml">
  <title>The Pearl</title>
  <author>John Steinbeck</author>
 </book>
 </foo>
#text
book
book
#text"#);
}

/// php-src `ext/dom/tests/dom_xinclude_errors.phpt`: Test: Xinclude errors
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::xinclude"#]
fn test_dom_xinclude_errors() {
    if skip_without_xml_native("test_dom_xinclude_errors") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument;
$dom->xinclude(PHP_INT_MAX);

$dom = Dom\XMLDocument::createEmpty();
try {
    $dom->xinclude(PHP_INT_MAX);
} catch (ValueError $e) {
    echo $e->getMessage();
}
"#);
    assert_format(&out, r#"Warning: DOMDocument::xinclude(): Invalid flags in %s on line %d
Dom\XMLDocument::xinclude(): Argument #1 ($options) is too large"#);
}

/// php-src `ext/dom/tests/domattributes.phpt`: Attributes: DOMAttribute functionality
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_domattributes() {
    if skip_without_xml_native("test_domattributes") {
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

$lang = $node->getAttributeNode('language');
echo "Language: ".$lang->value."\n";

$lang->value = 'en-US';
echo "Language: ".$lang->value."\n";

$parent = $lang->ownerElement;

$chapter = new DOMAttr("num", "1");
$parent->setAttributeNode($chapter);

echo "Is ID?: ".($chapter->isId()?'YES':'NO')."\n";

$top_element = $node->cloneNode();

print $dom->saveXML($top_element);


"#);
    assert_exact(&out, r#"Language: en
Language: en-US
Is ID?: NO
<chapter language="en-US" num="1"/>"#);
}

/// php-src `ext/dom/tests/domchardata.phpt`: CharData: DOMCharacterData and related functionality
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_domchardata() {
    if skip_without_xml_native("test_domchardata") {
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

$charnode = $dom->createElement('charnode');
$node->appendChild($charnode);

/* DOMComment */
$comment = new DOMComment('Testing character data and extending nodes');
$charnode->appendChild($comment);

echo "Comment Length: ".$comment->length."\n";

$comment->data = 'Updated comment';
echo "New Comment Length: ".$comment->length."\n";
echo "New Comment Data: ".$comment->data."\n";

/* DOMCDataSection */
$cdata = new DOMCDataSection('Chars: <>&"');
$charnode->appendChild($cdata);

echo "Substring: ".$cdata->substringData(7, 4)."\n";
$cdata->replaceData(10, 1, "'");
echo "New Substring: ".$cdata->substringData(7, 4)."\n";

/* DOMCharacterData using DOMComment */
$comment = new DOMComment('instructions');
echo "Comment Value: ".$comment->data."\n";
$comment->data = 'some more instructions';
echo "New Comment Value: ".$comment->data."\n";

$comment->insertData(10, 'pi ');
$comment->replaceData(18, 5, 'i');
$comment->insertData(20, 'g');
$comment->deleteData(13, 2);
$comment->deleteData(10, 3);
$comment->insertData(10, 'comment ');
echo "Updated Comment Value: ".$comment->data."\n";

/* DOMText */
$text = new DOMText('some text characters');

echo "Whole Text: ".$text->wholeText."\n";
$text2 = $text->splitText(9);
echo "Is Whitespace?: ", $text2->isElementContentWhitespace()?'YES':'NO', "\n";

echo "Split text: ".$text2->wholeText."\n";
$text3 = $text2->splitText(1);

echo "Is Whitespace?: ".($text2->isElementContentWhitespace()?'YES':'NO');
"#);
    assert_exact(&out, r#"Comment Length: 42
New Comment Length: 15
New Comment Data: Updated comment
Substring: <>&"
New Substring: <>&'
Comment Value: instructions
New Comment Value: some more instructions
Updated Comment Value: some more comment strings
Whole Text: some text characters
Is Whitespace?: NO
Split text:  characters
Is Whitespace?: YES"#);
}

/// php-src `ext/dom/tests/domdocument_createentityreference_001.phpt`: DomDocument::CreateEntityReference() - Creates an entity reference with the appropriate name
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createEntityReference"#]
fn test_domdocument_createentityreference_001() {
    if skip_without_xml_native("test_domdocument_createentityreference_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
    $objDoc = new DomDocument();

    $objRef = $objDoc->createEntityReference('Test');
    echo $objRef->nodeName . "\n";
"#);
    assert_exact(&out, r#"Test"#);
}

/// php-src `ext/dom/tests/domelement.phpt`: Elements: DOMElement functionality
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_domelement() {
    if skip_without_xml_native("test_domelement") {
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
echo "Tag Name: ".$node->tagName."\n";


$node->setAttribute('num', '1');
echo "Chapter: ".$node->getAttribute('num')."\n";
echo 'Attribute num exists?: '.($node->hasAttribute('num')?'Yes':'No')."\n";
$node->removeAttribute('num');
echo "Chapter: ".$node->getAttribute('num')."\n";
echo 'Attribute num exists?: '.($node->hasAttribute('num')?'Yes':'No')."\n";

echo "Language: ".$node->getAttribute('language')."\n";
$lang = $node->getAttributeNode('language');
$lang->nodeValue = 'en-US';
$node->setAttributeNode($lang);
echo "Language: ".$node->getAttribute('language')."\n";
$node->removeAttributeNode($lang);
echo "Language: ".$node->getAttribute('language')."\n";

echo "\n-- xml:lang --\n";
$node->setAttributeNS('http://www.w3.org/XML/1998/namespace', 'xml:lang', 'en');
echo "Language: ".$node->getAttributeNS('http://www.w3.org/XML/1998/namespace', 'lang')."\n";
echo 'Attribute xml:lang exists?: '.($node->hasAttributeNS('http://www.w3.org/XML/1998/namespace', 'lang')?'Yes':'No')."\n";

$node->removeAttributeNS('http://www.w3.org/XML/1998/namespace', 'lang');
echo "Language: ".$node->getAttributeNS('http://www.w3.org/XML/1998/namespace', 'lang')."\n";
echo 'Attribute xml:lang exists?: '.($node->hasAttributeNS('http://www.w3.org/XML/1998/namespace', 'lang')?'Yes':'No')."\n";

$lang = $dom->createAttributeNS('http://www.w3.org/XML/1998/namespace', 'xml:lang');
$lang->nodeValue = 'en-GB';
$node->setAttributeNodeNS($lang);
unset($lang);
echo "Language: ".$node->getAttributeNS('http://www.w3.org/XML/1998/namespace', 'lang')."\n";
$lang = $node->getAttributeNodeNS('http://www.w3.org/XML/1998/namespace', 'lang');
echo "Language: ".$lang->value."\n";

echo "\n-- Elements --\n";
$rows = $node->getElementsByTagName('row');
echo "Row Count: ".$rows->length."\n";

$element_ns = new DOMElement('newns:myelement', 'default content', 'urn::dummyns');
$node->appendChild($element_ns);
$element_ns = new DOMElement('newns2:myelement', 'second default content', 'urn::dummyns');
$node->appendChild($element_ns);

$myelements = $node->getElementsByTagNameNS('urn::dummyns', 'myelement');
$mylen = $myelements->length;
echo "myelements Count: ".$mylen."\n";

echo "\n-- IDs --\n";
$node->setAttribute('idatt', 'n1');
$node->setIdAttribute('idatt', TRUE);

for ($x = 0; $x < $mylen; $x++) {
    $current = $myelements->item($x);
    $current->setAttributeNS('urn::dummyns', 'newns:idatt', 'n'.($x+2))."\n";
    $current->setIdAttributeNS('urn::dummyns', 'idatt', TRUE);
}

echo 'Element Name: '.(($elem = $dom->getElementByID('n1'))?$elem->localName:'Not Found')."\n";
$idatt = $node->getAttributeNode('idatt');
$node->setIdAttributeNode($idatt, FALSE);
echo 'Element Name: '.(($elem = $dom->getElementByID('n1'))?$elem->localName:'Not Found')."\n";

echo 'Element Name: '.(($elem = $dom->getElementByID('n3'))?$elem->nodeName:'Not Found')."\n";
for ($x = 0; $x < $mylen; $x++) {
    $node = $myelements->item($x);
    $node->setIdAttributeNS('urn::dummyns', 'idatt', FALSE);
}
echo 'Element Name: '.(($elem = $dom->getElementByID('n3'))?$elem->nodeName:'Not Found')."\n";
"#);
    assert_exact(&out, r#"Tag Name: chapter
Chapter: 1
Attribute num exists?: Yes
Chapter: 
Attribute num exists?: No
Language: en
Language: en-US
Language: 

-- xml:lang --
Language: en
Attribute xml:lang exists?: Yes
Language: 
Attribute xml:lang exists?: No
Language: en-GB
Language: en-GB

-- Elements --
Row Count: 3
myelements Count: 2

-- IDs --
Element Name: chapter
Element Name: Not Found
Element Name: newns2:myelement
Element Name: Not Found"#);
}

/// php-src `ext/dom/tests/domobject_debug_handler.phpt`: Objects of DOM extension: debug info object handler.
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::dynamicProperty"#]
fn test_domobject_debug_handler() {
    if skip_without_xml_native("test_domobject_debug_handler") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<<XML
<foo>
    <bar>foobar</bar>
</foo>
XML;
$d = new domdocument;
$d->dynamicProperty = new stdclass;
$d->loadXML($xml);
var_dump($d);
"#);
    assert_format(&out, r##"Deprecated: Creation of dynamic property DOMDocument::$dynamicProperty is deprecated in %s on line %d
object(DOMDocument)#1 (41) {
  ["dynamicProperty"]=>
  object(stdClass)#2 (0) {
  }
  ["doctype"]=>
  NULL
  ["implementation"]=>
  string(22) "(object value omitted)"
  ["documentElement"]=>
  string(22) "(object value omitted)"
  ["actualEncoding"]=>
  NULL
  ["encoding"]=>
  NULL
  ["xmlEncoding"]=>
  NULL
  ["standalone"]=>
  bool(false)
  ["xmlStandalone"]=>
  bool(false)
  ["version"]=>
  string(3) "1.0"
  ["xmlVersion"]=>
  string(3) "1.0"
  ["strictErrorChecking"]=>
  bool(true)
  ["documentURI"]=>
  string(%d) "%s"
  ["config"]=>
  NULL
  ["formatOutput"]=>
  bool(false)
  ["validateOnParse"]=>
  bool(false)
  ["resolveExternals"]=>
  bool(false)
  ["preserveWhiteSpace"]=>
  bool(true)
  ["recover"]=>
  bool(false)
  ["substituteEntities"]=>
  bool(false)
  ["firstElementChild"]=>
  string(22) "(object value omitted)"
  ["lastElementChild"]=>
  string(22) "(object value omitted)"
  ["childElementCount"]=>
  int(1)
  ["nodeName"]=>
  string(9) "#document"
  ["nodeValue"]=>
  NULL
  ["nodeType"]=>
  int(9)
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
  bool(true)
  ["ownerDocument"]=>
  NULL
  ["namespaceURI"]=>
  NULL
  ["prefix"]=>
  string(0) ""
  ["localName"]=>
  NULL
  ["baseURI"]=>
  string(%d) %s
  ["textContent"]=>
  string(12) "
    foobar
"
}"##);
}

/// php-src `ext/dom/tests/domxpath.phpt`: DOMXPath Tests
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_domxpath() {
    if skip_without_xml_native("test_domxpath") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

function MyAverage($nodelist) {
    $count = 0;
    $val = 0;
    foreach ($nodelist AS $node) {
        $count++;
        $val += $node->textContent;
    }
    if ($val > 0) {
        return $val/$count;
    } else {
        return 0;
    }
}

$dom = new DOMDocument;
$dom->loadXML('<root xmlns="urn::default"><child>myval</child></root>');

$xpath = new DOMXPath($dom);

$xpath->registerPHPFunctions('MyAverage');
$xpath->registerNamespace("php", "http://php.net/xpath");

$xpath->registerNamespace("def", "urn::default");
$nodelist = $xpath->query("//def:child");
if ($node = $nodelist->item(0)) {
    print $node->textContent."\n";
}

$count = $xpath->evaluate("count(//def:child)");

var_dump($count);

$xpathdoc = $xpath->document;

var_dump($xpathdoc instanceof DOMDocument);

$root = $dom->documentElement;
$root->appendChild($dom->createElementNS("urn::default", "testnode", 3));
$root->appendChild($dom->createElementNS("urn::default", "testnode", 4));
$root->appendChild($dom->createElementNS("urn::default", "testnode", 4));
$root->appendChild($dom->createElementNS("urn::default", "testnode", 5));

$avg = $xpath->evaluate('number(php:function("MyAverage", //def:testnode))');
var_dump($avg);

try {
    $xpath->registerPHPFunctions('non_existent');
    $avg = $xpath->evaluate('number(php:function("non_existent", //def:testnode))');
} catch (\Error $e) {
    echo $e->getMessage() . \PHP_EOL;
}
try {
    $xpath->registerPhpFunctions(['non_existant']);
    $avg = $xpath->evaluate('number(php:function("non_existent", //def:testnode))');
} catch (\Error $e) {
    echo $e->getMessage() . \PHP_EOL;
}
"#);
    assert_exact(&out, r#"myval
float(1)
bool(true)
float(4)
DOMXPath::registerPhpFunctions(): Argument #1 ($restrict) must be a callable, function "non_existent" not found or invalid function name
DOMXPath::registerPhpFunctions(): Argument #1 ($restrict) must be an array with valid callbacks as values, function "non_existant" not found or invalid function name"#);
}

/// php-src `ext/dom/tests/element_child_and_parent_node_without_document.phpt`: DOMElement: DOMChildNode, DOMParentNode modifications without a document
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMElement::__construct' expects 0 arguments, got 2"#]
fn test_element_child_and_parent_node_without_document() {
    if skip_without_xml_native("test_element_child_and_parent_node_without_document") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$element = new DOMElement("p", " Hello World! ");
$element->append("APPENDED");
$element->prepend("PREPENDED");
$element->after("AFTER");
$element->before("BEFORE");
$element->replaceWith("REPLACE");

$doc = new DOMDocument();
$doc->adoptNode($element);
echo $doc->saveXML($element), "\n";

"#);
    assert_exact(&out, r#"<p>PREPENDED Hello World! APPENDED</p>"#);
}

/// php-src `ext/dom/tests/entity_reference_stale_01.phpt`: Entity references with stale entity declaration 01
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_entity_reference_stale_01() {
    if skip_without_xml_native("test_entity_reference_stale_01") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML(<<<XML
<!DOCTYPE foo [
<!ENTITY foo "bar">
]>
<foo>&foo;</foo>
XML);

$ref = $dom->documentElement->firstChild;
$decl = $ref->firstChild;

$nodes = $ref->childNodes;
$dom->removeChild($dom->doctype);
unset($decl);

var_dump($nodes);
var_dump($ref->firstChild);
var_dump($ref->lastChild);
var_dump($ref->textContent);
var_dump($ref->childNodes);

"#);
    assert_exact(&out, r#"object(DOMNodeList)#4 (1) {
  ["length"]=>
  int(0)
}
NULL
NULL
string(0) ""
object(DOMNodeList)#2 (1) {
  ["length"]=>
  int(0)
}"#);
}

/// php-src `ext/dom/tests/entity_reference_stale_02.phpt`: Entity references with stale entity declaration 02
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_entity_reference_stale_02() {
    if skip_without_xml_native("test_entity_reference_stale_02") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML(<<<XML
<!DOCTYPE foo [
<!ENTITY foo1 "bar1">
<!ENTITY foo2 "bar2">
<!ENTITY foo3 "bar3">
]>
<foo>&foo1;</foo>
XML);

$ref = $dom->documentElement->firstChild;
$decl = $ref->firstChild;

$nodes = $ref->childNodes;
$iter = $nodes->getIterator();
$iter->next();
$dom->removeChild($dom->doctype);
unset($decl);

try {
    var_dump($iter->current()->publicId);
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"NULL"#);
}

/// php-src `ext/dom/tests/entity_reference_stale_03.phpt`: Entity references with stale entity declaration 03
#[test]
#[ignore = r#"DOM port pending: Undefined property: Dom\XMLDocument::doctype"#]
fn test_entity_reference_stale_03() {
    if skip_without_xml_native("test_entity_reference_stale_03") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

foreach (['firstChild', 'lastChild', 'textContent', 'childNodes'] as $prop) {
    $dom = Dom\XMLDocument::createFromString(<<<XML
    <!DOCTYPE foo [
    <!ENTITY foo "bar">
    ]>
    <foo>&foo;</foo>
    XML);

    $ref = $dom->documentElement->firstChild;
    $decl = $ref->firstChild;

    $nodes = $ref->childNodes;
    $dom->removeChild($dom->doctype);
    unset($decl);

    var_dump($ref->$prop);
}

"#);
    assert_exact(&out, r#"NULL
NULL
NULL
object(Dom\NodeList)#1 (1) {
  ["length"]=>
  int(0)
}"#);
}

/// php-src `ext/dom/tests/fragments_multiple_nodes_DOMParentNode.phpt`: Handling fragments of multiple nodes for DOMParentNode
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createDocumentFragment"#]
fn test_fragments_multiple_nodes_domparent_node() {
    if skip_without_xml_native("test_fragments_multiple_nodes_domparent_node") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML('<!DOCTYPE HTML><html><container/></html>');

$container = $dom->documentElement->firstElementChild;

$fragment = $dom->createDocumentFragment();
$fragment->appendChild($dom->createElement('p', '1'));
$fragment->appendChild($dom->createElement('b', '2'));
$container->replaceWith($fragment);
echo $dom->saveXML();

$dom->documentElement->append('foo');
echo $dom->saveXML();

$fragment = $dom->createDocumentFragment();
$fragment->appendChild($dom->createElement('p', '3'));
$fragment->appendChild($dom->createElement('b', '4'));
$dom->documentElement->prepend($fragment);
echo $dom->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<!DOCTYPE HTML>
<html><p>1</p><b>2</b></html>
<?xml version="1.0"?>
<!DOCTYPE HTML>
<html><p>1</p><b>2</b>foo</html>
<?xml version="1.0"?>
<!DOCTYPE HTML>
<html><p>3</p><b>4</b><p>1</p><b>2</b>foo</html>"#);
}

/// php-src `ext/dom/tests/getElementsByTagName_liveness_deallocated_document.phpt`: getElementsByTagName() liveness with deallocated document
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_get_elements_by_tag_name_liveness_deallocated_document() {
    if skip_without_xml_native("test_get_elements_by_tag_name_liveness_deallocated_document") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<container>
    <p>1</p><p>2</p><p>3</p>
</container>
XML);

$ps = $dom->documentElement->getElementsByTagName('p');
$second = $ps->item(1);
var_dump($second->textContent);
var_dump($ps->length);

unset($dom);
$dom = $second->ownerDocument;

$second->parentNode->appendChild($dom->createElement('p', '4'));
var_dump($ps->length);

"#);
    assert_exact(&out, r#"string(1) "2"
int(3)
int(4)"#);
}

/// php-src `ext/dom/tests/import_attribute_namespace.phpt`: Cloning an attribute should retain its namespace 02
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::importNode"#]
fn test_import_attribute_namespace() {
    if skip_without_xml_native("test_import_attribute_namespace") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<container xmlns:foo="some:ns" foo:bar="1"/>
XML);

$dom2 = new DOMDocument;
$dom2->loadXML(<<<XML
<?xml version="1.0"?>
<container xmlns:foo="some:other"/>
XML);

$imported = $dom2->importNode($dom->documentElement->getAttributeNodeNs("some:ns", "bar"));
var_dump($imported->prefix, $imported->namespaceURI);
$dom2->documentElement->setAttributeNodeNs($imported);
var_dump($imported->prefix, $imported->namespaceURI);

echo $dom->saveXML();
echo $dom2->saveXML();

"#);
    assert_exact(&out, r#"string(7) "default"
string(7) "some:ns"
string(7) "default"
string(7) "some:ns"
<?xml version="1.0"?>
<container xmlns:foo="some:ns" foo:bar="1"/>
<?xml version="1.0"?>
<container xmlns:foo="some:other" xmlns:default="some:ns" default:bar="1"/>"#);
}

/// php-src `ext/dom/tests/libxml_global_state_entity_loader_bypass.phpt`: GHSA-3qrf-m4j2-pcrr (libxml global state entity loader bypass)
#[test]
#[ignore = r#"requires the test-only zend_test extension"#]
fn test_libxml_global_state_entity_loader_bypass() {
    if skip_without_xml_native("test_libxml_global_state_entity_loader_bypass") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$xml = "<?xml version='1.0'?><!DOCTYPE root [<!ENTITY % bork SYSTEM \"php://nope\"> %bork;]><nothing/>";

libxml_use_internal_errors(true);

function parseXML($xml) {
  $doc = new DOMDocument();
  @$doc->loadXML($xml);
  $doc->createDocumentFragment()->appendXML("&bork;");
  foreach (libxml_get_errors() as $error) {
    var_dump(trim($error->message));
  }
}

parseXML($xml);
zend_test_override_libxml_global_state();
parseXML($xml);

echo "Done\n";

"#);
    assert_exact(&out, r#"string(25) "Entity 'bork' not defined"
string(25) "Entity 'bork' not defined"
string(25) "Entity 'bork' not defined"
Done"#);
}

/// php-src `ext/dom/tests/node_textcontent.phpt`: Testing reading and writing to DOMNode::textContent
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::getElementById"#]
fn test_node_textcontent() {
    if skip_without_xml_native("test_node_textcontent") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
/*
 If this test is failing it is because the libxml2 library being used does
 not have this bug fix from 2009:

   https://github.com/GNOME/libxml2/commit/f3c06692e0d200ae0d35b5b3c31de8c56aa99ac6

 The workaround if you are being hit by this is to add a <!DOCTYPE html> tag
*/
$html = <<<HTML
<div id="test"><span>hi there</span></div>
HTML;

$text = '<p>hello world &trade;</p>';

$dom = new DOMDocument('1.0', 'UTF-8');
$dom->loadHTML($html);

$node = $dom->getElementById('test');
var_dump($node->textContent);
$node->textContent = $text;
var_dump($node->textContent == $text);

var_dump($dom->saveHTML($node));

"#);
    assert_exact(&out, r#"string(8) "hi there"
bool(true)
string(63) "<div id="test">&lt;p&gt;hello world &amp;trade;&lt;/p&gt;</div>""#);
}

/// php-src `ext/dom/tests/not_serializable.phpt`: DOM classes are not serializable
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_not_serializable() {
    if skip_without_xml_native("test_not_serializable") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$doc->loadXML('<root><node/></root>');
try {
    serialize($doc);
} catch (Exception $e) {
    echo $e->getMessage(), "\n";
}

$node = $doc->documentElement;
try {
    serialize($node);
} catch (Exception $e) {
    echo $e->getMessage(), "\n";
}

$xpath = new DOMXPath($doc);
try {
    serialize($xpath);
} catch (Exception $e) {
    echo $e->getMessage(), "\n";
}

$ns = $xpath->query('//namespace::*')->item(0);
try {
    serialize($ns);
} catch (Exception $e) {
    echo $e->getMessage(), "\n";
}

$dom = Dom\XMLDocument::createEmpty();
try {
    serialize($dom);
} catch (Exception $e) {
    echo $e->getMessage(), "\n";
}

try {
    serialize(new Dom\XPath($dom));
} catch (Exception $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Serialization of 'DOMDocument' is not allowed, unless serialization methods are implemented in a subclass
Serialization of 'DOMElement' is not allowed, unless serialization methods are implemented in a subclass
Serialization of 'DOMXPath' is not allowed
Serialization of 'DOMNameSpaceNode' is not allowed, unless serialization methods are implemented in a subclass
Serialization of 'Dom\XMLDocument' is not allowed, unless serialization methods are implemented in a subclass
Serialization of 'Dom\XPath' is not allowed"#);
}

/// php-src `ext/dom/tests/not_unserializable.phpt`: DOM classes are not unserializable
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_not_unserializable() {
    if skip_without_xml_native("test_not_unserializable") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$classes = [
    "DOMXPath",
    "DOMDocument",
    "DOMNode",
    "DOMNameSpaceNode",
];

foreach ($classes as $class)
{
    try {
        unserialize('O:' . strlen($class) . ':"' . $class . '":0:{}');
    } catch (Exception $e) {
        echo $e->getMessage(), "\n";
    }
}

"#);
    assert_exact(&out, r#"Unserialization of 'DOMXPath' is not allowed
Unserialization of 'DOMDocument' is not allowed, unless unserialization methods are implemented in a subclass
Unserialization of 'DOMNode' is not allowed, unless unserialization methods are implemented in a subclass
Unserialization of 'DOMNameSpaceNode' is not allowed, unless unserialization methods are implemented in a subclass"#);
}

/// php-src `ext/dom/tests/null_text_content_manipulation.phpt`: NULL text content manipulation
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMText::substringData"#]
fn test_null_text_content_manipulation() {
    if skip_without_xml_native("test_null_text_content_manipulation") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$text = new DOMText();
try {
    var_dump($text->substringData(1, 0));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    var_dump($text->insertData(1, ""));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    var_dump($text->deleteData(1, 1));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    var_dump($text->replaceData(1, 1, ""));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
"#);
    assert_exact(&out, r#"Index Size Error
Index Size Error
Index Size Error
Index Size Error"#);
}

/// php-src `ext/dom/tests/parentnode_childnode_too_long_text.phpt`: Passing a too long string to ChildNode or ParentNode methods causes an exception
#[test]
#[ignore = r#"php-src SKIPIF: skip resource-heavy test"#]
fn test_parentnode_childnode_too_long_text() {
    if skip_without_xml_native("test_parentnode_childnode_too_long_text") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php ini_set('memory_limit', '-1');
$dom = new DOMDocument;
$element = $dom->appendChild($dom->createElement('root'));
$str = str_repeat('X', 2**31 + 10);
try {
    $element->append('x', $str);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $element->prepend('x', $str);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $element->after('x', $str);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $element->before('x', $str);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $element->replaceWith('x', $str);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $element->replaceChildren('x', $str);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
var_dump($dom->childNodes->count());
var_dump($element->childNodes->count());
"#);
    assert_exact(&out, r#"DOMElement::append(): Argument #2 must be less than or equal to 2147483647 bytes long
DOMElement::prepend(): Argument #2 must be less than or equal to 2147483647 bytes long
DOMElement::after(): Argument #2 must be less than or equal to 2147483647 bytes long
DOMElement::before(): Argument #2 must be less than or equal to 2147483647 bytes long
DOMElement::replaceWith(): Argument #2 must be less than or equal to 2147483647 bytes long
DOMElement::replaceChildren(): Argument #2 must be less than or equal to 2147483647 bytes long
int(1)
int(0)"#);
}

/// php-src `ext/dom/tests/php_function_edge_cases.phpt`: php:function() edge cases
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXpath"#]
fn test_php_function_edge_cases() {
    if skip_without_xml_native("test_php_function_edge_cases") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$doc->loadHTML('<a href="https://php.net">hello</a>');
$xpath = new DOMXpath($doc);
$xpath->registerNamespace("php", "http://php.net/xpath");
$xpath->registerPHPFunctions();
try {
    $xpath->query("//a[php:function(3)]");
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $xpath->query("//a[php:function()]");
} catch (Throwable $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Handler name must be a string
Function name must be passed as the first argument"#);
}

/// php-src `ext/dom/tests/property_write_errors.phpt`: Test property write errors
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_property_write_errors() {
    if skip_without_xml_native("test_property_write_errors") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require_once("dom_test.inc");

$dom = new DOMDocument();

try {
    $dom->nodeValue = [];
} catch (TypeError $exception) {
    echo $exception->getMessage() . "\n";
}

try {
    $dom->nodeType += 1;
} catch (Error $exception) {
    echo $exception->getMessage() . "\n";
}

try {
    $dom->xmlEncoding = null;
} catch (Error $exception) {
    echo $exception->getMessage() . "\n";
}

$entity = new DOMEntity();

try {
    $entity->actualEncoding = null;
} catch (Error $exception) {
    echo $exception->getMessage() . "\n";
}

try {
    $entity->encoding = null;
} catch (Error $exception) {
    echo $exception->getMessage() . "\n";
}

try {
    $entity->version = null;
} catch (Error $exception) {
    echo $exception->getMessage() . "\n";
}

"#);
    assert_exact(&out, r#"Cannot assign array to property DOMNode::$nodeValue of type ?string
Cannot modify readonly property DOMDocument::$nodeType
Cannot modify readonly property DOMDocument::$xmlEncoding
Cannot modify readonly property DOMEntity::$actualEncoding
Cannot modify readonly property DOMEntity::$encoding
Cannot modify readonly property DOMEntity::$version"#);
}

/// php-src `ext/dom/tests/reconcile_reused_namespace.phpt`: Reconcile a reused namespace from doc->oldNs
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createElementNS"#]
fn test_reconcile_reused_namespace() {
    if skip_without_xml_native("test_reconcile_reused_namespace") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument();
$root = $dom->createElementNS('http://www.w3.org/2000/xhtml', 'html');

$dom->loadXML(<<<XML
<?xml version="1.0"?>
<html
    xmlns="http://www.w3.org/2000/xhtml"
    xmlns:a="http://example.com/A"
    xmlns:b="http://example.com/B"
/>
XML);
$root = $dom->firstElementChild;

echo "Add first\n";
$element = $dom->createElementNS('http://example.com/B', 'p', 'Hello World');
$root->appendChild($element);

echo "Add second\n";
$element = $dom->createElementNS('http://example.com/A', 'p', 'Hello World');
$root->appendChild($element);

echo "Add third\n";
$element = $dom->createElementNS('http://example.com/A', 'p', 'Hello World');
$root->appendChild($element);

var_dump($dom->saveXML());

"#);
    assert_exact(&out, r#"Add first
Add second
Add third
string(201) "<?xml version="1.0"?>
<html xmlns="http://www.w3.org/2000/xhtml" xmlns:a="http://example.com/A" xmlns:b="http://example.com/B"><b:p>Hello World</b:p><a:p>Hello World</a:p><a:p>Hello World</a:p></html>
""#);
}

/// php-src `ext/dom/tests/registerNodeClass_abstract_class.phpt`: registerNodeClass() with an abstract class should fail
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::registerNodeClass"#]
fn test_register_node_class_abstract_class() {
    if skip_without_xml_native("test_register_node_class_abstract_class") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

abstract class Test extends DOMElement {
    abstract function foo();
}

$dom = new DOMDocument;

try {
    $dom->registerNodeClass("DOMElement", "Test");
} catch (ValueError $e) {
    echo "ValueError: ", $e->getMessage(), "\n";
}

$dom->createElement("foo");

"#);
    assert_exact(&out, r#"ValueError: DOMDocument::registerNodeClass(): Argument #2 ($extendedClass) must not be an abstract class"#);
}

/// php-src `ext/dom/tests/register_node_class.phpt`: Test: registerNodeClass()
#[test]
#[ignore = r#"DOM port pending: Unknown class referenced during inheritance flattening: DOMAttr"#]
fn test_register_node_class() {
    if skip_without_xml_native("test_register_node_class") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
class myAttribute extends DOMAttr {
   function testit() { return "HELLO Attribute"; }
}

class myElement extends DOMElement {
   function testit() { return "HELLO Element"; }
}

$doc = new DOMDocument();
$doc->registerNodeClass('DOMAttr', 'myAttribute');
$doc->registerNodeClass('DOMElement', 'myElement');
$doc->appendChild(new DOMElement('root'));
$root = $doc->documentElement;
$root->setAttribute('a', 'a1');
echo get_class($root), "\n";
print $root->testit()."\n";
$attr = $root->getAttributeNode('a');
echo get_class($attr), "\n";
print $attr->testit()."\n";
unset($attr);
$doc->registerNodeClass('DOMAttr', NULL);
$attr = $root->getAttributeNode('a');
echo get_class($attr), "\n";
try {
    print $attr->testit()."\n";
} catch (Error $e) {
    echo $e->getMessage();
}
"#);
    assert_exact(&out, r#"myElement
HELLO Element
myAttribute
HELLO Attribute
DOMAttr
Call to undefined method DOMAttr::testit()"#);
}

/// php-src `ext/dom/tests/replaceChild_attribute_validation.phpt`: replaceChild with attribute children
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createAttribute"#]
fn test_replace_child_attribute_validation() {
    if skip_without_xml_native("test_replace_child_attribute_validation") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$attr = $dom->createAttribute('attr');
$attr->textContent = "test";

try {
    $attr->replaceChild($dom->createProcessingInstruction('pi'), $attr->firstChild);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

$root = $dom->appendChild($dom->createElement('root'));
$root->setAttributeNode($attr);

echo $dom->saveXML();

"#);
    assert_exact(&out, r#"Hierarchy Request Error
<?xml version="1.0"?>
<root attr="test"/>"#);
}

/// php-src `ext/dom/tests/replaceWith_non_viable_next_sibling.phpt`: replaceWith() with a non-viable next sibling
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_replace_with_non_viable_next_sibling() {
    if skip_without_xml_native("test_replace_with_non_viable_next_sibling") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXML(<<<XML
<?xml version="1.0"?>
<container>
    <child>
        <alone/>
    </child>
</container>
XML);

$container = $doc->documentElement;
$child = $container->firstElementChild;
$alone = $child->firstElementChild;

$child->after($alone);
echo $doc->saveXML();
$child->replaceWith($alone);
echo $doc->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<container>
    <child>
        
    </child><alone/>
</container>
<?xml version="1.0"?>
<container>
    <alone/>
</container>"#);
}

/// php-src `ext/dom/tests/setAttributeNodeNS_same_uri_different_prefix.phpt`: setAttributeNodeNS with same URI but different prefix
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createAttributeNS"#]
fn test_set_attribute_node_ns_same_uri_different_prefix() {
    if skip_without_xml_native("test_set_attribute_node_ns_same_uri_different_prefix") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
// Spec doesn't require to remove redundant namespace declarations.
// This means that when appending a new attribute the old namespace declaration is reused, hence the prefix stays "foo".
// Firefox cleans up the old namespace declaration, so there the prefix does change.
// Chrome doesn't clean it up, so there the prefix doesn't change.
// Our behaviour is the same as Chrome's due to implementation difficulties.
$doc = new DOMDocument();
$doc->appendChild($doc->createElement('container'));
$attribute = $doc->createAttributeNS('http://php.net/ns1', 'foo:hello');
$attribute->nodeValue = '1';
var_dump($doc->documentElement->setAttributeNodeNS($attribute)?->nodeValue);
echo $doc->saveXML(), "\n";
$attribute = $doc->createAttributeNS('http://php.net/ns1', 'bar:hello');
$attribute->nodeValue = '2';
var_dump($doc->documentElement->setAttributeNodeNS($attribute)?->nodeValue);
echo $doc->saveXML(), "\n";
$attribute = $doc->createAttributeNS('http://php.net/ns1', 'hello');
$attribute->nodeValue = '3';
var_dump($doc->documentElement->setAttributeNodeNS($attribute)?->nodeValue);
echo $doc->saveXML(), "\n";
"#);
    assert_exact(&out, r#"NULL
<?xml version="1.0"?>
<container xmlns:foo="http://php.net/ns1" foo:hello="1"/>

string(1) "1"
<?xml version="1.0"?>
<container xmlns:foo="http://php.net/ns1" foo:hello="2"/>

string(1) "2"
<?xml version="1.0"?>
<container xmlns:foo="http://php.net/ns1" foo:hello="3"/>"#);
}

/// php-src `ext/dom/tests/specific_namespace_behaviour.phpt`: DOM: specific namespace behaviour for applications with fixed serialization requirements
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::importNode"#]
fn test_specific_namespace_behaviour() {
    if skip_without_xml_native("test_specific_namespace_behaviour") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom1 = new DOMDocument();
$dom1->loadXML(<<<XML
<wsse:Security xmlns:wsse="foo:bar">
    <ds:Signature xmlns:ds="http://www.w3.org/2000/09/xmldsig#">
    </ds:Signature>
</wsse:Security>
XML);
$dom2 = new DOMDocument();
$dom2->loadXML('<xml><child/></xml>');
$wsse = $dom2->importNode($dom1->documentElement, true);
$dom2->firstChild->firstChild->appendChild($wsse);
echo $dom2->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<xml><child><wsse:Security xmlns:wsse="foo:bar" xmlns:ds="http://www.w3.org/2000/09/xmldsig#">
    <ds:Signature xmlns:ds="http://www.w3.org/2000/09/xmldsig#">
    </ds:Signature>
</wsse:Security></child></xml>"#);
}

/// php-src `ext/dom/tests/toString_exceptions.phpt`: Handling of exceptions during __toString
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::encoding"#]
fn test_to_string_exceptions() {
    if skip_without_xml_native("test_to_string_exceptions") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class BadStr {
    public function __toString() {
        throw new Exception("Exception");
    }
}

$badStr = new BadStr;

$doc = new DOMDocument();
$doc->loadXML(
    '<root xmlns:ns="foo"><node attr="foo" /><node>Text</node><ns:node/><?pi foobar?></root>');

try { $doc->encoding = $badStr; } catch (Exception $e) { echo "Exception\n"; }
try { $doc->version = $badStr; } catch (Exception $e) { echo "Exception\n"; }
try { $doc->documentURI = $badStr; } catch (Exception $e) { echo "Exception\n"; }
$root = $doc->childNodes[0];

$node = $root->childNodes[0];
$attrs = $node->attributes;
$attr = $attrs[0];
try { $attr->value = $badStr; } catch (Exception $e) { echo "Exception\n"; }
try { $attr->nodeValue = $badStr; } catch (Exception $e) { echo "Exception\n"; }

$node2 = $root->childNodes[1];
try { $node2->nodeValue = $badStr; } catch (Exception $e) { echo "Exception\n"; }
try { $node2->textContent = $badStr; } catch (Exception $e) { echo "Exception\n"; }
$data = $node2->childNodes[0];
try { $data->data = $badStr; } catch (Exception $e) { echo "Exception\n"; }

$node3 = $root->childNodes[2];
try { $node3->prefix = $badStr; } catch (Exception $e) { echo "Exception\n"; }

$pi = $root->childNodes[3];
try { $pi->data = $badStr; } catch (Exception $e) { echo "Exception\n"; }

echo $doc->saveXML();

"#);
    assert_exact(&out, r#"Exception
Exception
Exception
Exception
Exception
Exception
Exception
Exception
Exception
Exception
<?xml version="1.0"?>
<root xmlns:ns="foo"><node attr="foo"/><node>Text</node><ns:node/><?pi foobar?></root>"#);
}

/// php-src `ext/dom/tests/uaf_doctype_iterator.phpt`: UAF when removing doctype and iterating over the child nodes
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_uaf_doctype_iterator() {
    if skip_without_xml_native("test_uaf_doctype_iterator") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument;
$dom->loadXML(<<<XML
<!DOCTYPE foo [
    <!ENTITY foo1 "bar1">
]>
<foo>&foo1;</foo>
XML);
$ref = $dom->documentElement->firstChild;
$nodes = $ref->childNodes;
$dom->removeChild($dom->doctype);
foreach($nodes as $str) {}
var_dump($nodes);
"#);
    assert_format(&out, r#"object(DOMNodeList)#%d (1) {
  ["length"]=>
  int(0)
}"#);
}

/// php-src `ext/dom/tests/unpack_foreach_behaviour.phpt`: Unpacking vs foreach behaviour
#[test]
#[ignore = r#"DOM port pending: Spread operator requires an array"#]
fn test_unpack_foreach_behaviour() {
    if skip_without_xml_native("test_unpack_foreach_behaviour") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML('<root><a/><b/></root>');

echo "--- By foreach: ---\n";

foreach ($dom->documentElement->getElementsByTagName('*') as $node) {
    var_dump($node->localName);
}

echo "--- By unpacking: ---\n";

$iter = $dom->documentElement->getElementsByTagName('*');
foreach ([...$iter] as $node) {
    var_dump($node->localName);
}

"#);
    assert_exact(&out, r#"--- By foreach: ---
string(1) "a"
string(1) "b"
--- By unpacking: ---
string(1) "a"
string(1) "b""#);
}

/// php-src `ext/dom/tests/unsetting_properties.phpt`: Unsetting properties
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::registerNodeClass"#]
fn test_unsetting_properties() {
    if skip_without_xml_native("test_unsetting_properties") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class MyElement extends DOMElement {
    public int $myProp = 3;
}

$dom = new DOMDocument;
$dom->registerNodeClass('DOMElement', 'MyElement');
$dom->loadXML('<root>foo</root>');
$root = $dom->documentElement;

unset($root->myProp);
try {
    $root->myProp;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
try {
    unset($root->textContent);
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Typed property MyElement::$myProp must not be accessed before initialization
Cannot unset MyElement::$textContent"#);
}

/// php-src `ext/dom/tests/xml_parsing_LIBXML_NO_XXE.phpt`: Test flag LIBXML_NO_XXE
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_xml_parsing_libxml_no_xxe() {
    if skip_without_xml_native("test_xml_parsing_libxml_no_xxe") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = <<< XML
<?xml version='1.0' encoding='utf-8'?>
<!DOCTYPE set [
    <!ENTITY foo '<foo>bar</foo>'>
    <!ENTITY xxe SYSTEM "file:///etc/passwd">
]>
<set>&foo;&xxe;</set>
XML;

$doc = Dom\XMLDocument::createFromString($xml, LIBXML_NOENT | LIBXML_NO_XXE);
echo $doc->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE set [
<!ENTITY foo "<foo>bar</foo>">
<!ENTITY xxe SYSTEM "file:///etc/passwd">
]>
<set><foo>bar</foo></set>"#);
}

/// php-src `ext/dom/tests/xml_parsing_LIBXML_RECOVER.phpt`: XML parsing with LIBXML_RECOVER
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_xml_parsing_libxml_recover() {
    if skip_without_xml_native("test_xml_parsing_libxml_recover") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML('<root><child/>', options: LIBXML_RECOVER);
echo $dom->saveXML();

$dom = Dom\XMLDocument::createFromString('<root><child/>', options: LIBXML_RECOVER);
echo $dom->saveXML(), "\n";

"#);
    assert_format(&out, r#"Warning: DOMDocument::loadXML(): %s
<?xml version="1.0"?>
<root><child/></root>

Warning: Dom\XMLDocument::createFromString(): %s
<?xml version="1.0" encoding="UTF-8"?>
<root><child/></root>"#);
}
