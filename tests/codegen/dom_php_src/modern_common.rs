//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `modern/common` folder, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/modern/common/Document_title_getter.phpt`: Dom\Document::$title getter
#[test]
#[ignore = r#"DOM port pending: Undefined property: Dom\XMLDocument::title"#]
fn test_document_title_getter() {
    if skip_without_xml_native("test_document_title_getter") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

echo "=== HTML namespaced root ===\n";

$dom = Dom\XMLDocument::createFromString("<root xmlns=\"http://www.w3.org/1999/xhtml\"><title>A normal title without collapsable or strippable whitespace</title></root>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<root xmlns=\"http://www.w3.org/1999/xhtml\"><title>  only ws at front</title></root>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<root xmlns=\"http://www.w3.org/1999/xhtml\"><title>only ws at back  </title></root>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<root xmlns=\"http://www.w3.org/1999/xhtml\"><ignoreme/><div><title>first</title></div><title>second</title></root>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<title xmlns=\"http://www.w3.org/1999/xhtml\">title</title>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<root xmlns=\"http://www.w3.org/1999/xhtml\"><title>  abc def   ghi  </title></root>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<root xmlns=\"http://www.w3.org/1999/xhtml\"><title></title></root>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<root xmlns=\"http://www.w3.org/1999/xhtml\"></root>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<root xmlns=\"http://www.w3.org/1999/xhtml\"><title>  \t\r\n  </title></root>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<root xmlns=\"http://www.w3.org/1999/xhtml\"><title>  \tx<?y y?><![CDATA[z]]>\n  </title></root>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<root xmlns=\"http://www.w3.org/1999/xhtml\"><title><div><!-- comment -->x</div>y<p>z</p>w</title></root>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<root xmlns=\"http://www.w3.org/1999/xhtml\"><title>title\nhere</title></root>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<root xmlns=\"http://www.w3.org/1999/xhtml\"><title/></root>");
$dom->getElementsByTagName('title')[0]->appendChild($dom->importLegacyNode(new DOMText));
var_dump($dom->title);

echo "=== SVG namespaced root ===\n";

$dom = Dom\XMLDocument::createFromString("<root xmlns=\"http://www.w3.org/1999/xhtml\"><title>title</title></root>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<svg xmlns=\"http://www.w3.org/1999/xhtml\"><title xmlns=\"http://www.w3.org/1999/xhtml\">title</title></svg>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<svg xmlns=\"http://www.w3.org/1999/xhtml\"><title xmlns=\"http://www.w3.org/1999/xhtml\">title</title><foo/><title>hi</title></svg>");
var_dump($dom->title);

$dom = Dom\XMLDocument::createFromString("<svg xmlns=\"http://www.w3.org/1999/xhtml\"/>");
var_dump($dom->title);

"#);
    assert_exact(&out, r#"=== HTML namespaced root ===
string(59) "A normal title without collapsable or strippable whitespace"
string(16) "only ws at front"
string(15) "only ws at back"
string(5) "first"
string(5) "title"
string(11) "abc def ghi"
string(0) ""
string(0) ""
string(0) ""
string(2) "xz"
string(2) "yw"
string(10) "title here"
string(0) ""
=== SVG namespaced root ===
string(5) "title"
string(5) "title"
string(5) "title"
string(0) """#);
}

/// php-src `ext/dom/tests/modern/common/Document_title_setter.phpt`: Dom\Document::$title setter
#[test]
#[ignore = r#"DOM port pending: Undefined property: Dom\XMLDocument::title"#]
fn test_document_title_setter() {
    if skip_without_xml_native("test_document_title_setter") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

echo "\n=== SVG namespaced test ===\n\n";

$dom = Dom\XMLDocument::createFromString('<root xmlns="http://www.w3.org/2000/svg"/>');
$dom->title = "hello &amp; world";
echo $dom->saveXML(), "\n";

$dom = Dom\XMLDocument::createFromString('<svg xmlns="http://www.w3.org/2000/svg"/>');
$dom->title = "hello &amp; world";
echo $dom->saveXML(), "\n";
$dom->title = "";
echo $dom->saveXML(), "\n";
$dom->title = "test";
echo $dom->saveXML(), "\n";

$dom = Dom\XMLDocument::createFromString('<svg:svg xmlns:svg="http://www.w3.org/2000/svg"/>');
$dom->title = "test";
echo $dom->saveXML(), "\n";
var_dump($dom->documentElement->firstElementChild->prefix, $dom->documentElement->firstElementChild->namespaceURI);

$dom = Dom\XMLDocument::createFromString('<svg xmlns="http://www.w3.org/2000/svg">first node<div/></svg>');
$dom->title = "test";
echo $dom->saveXML(), "\n";
$dom->documentElement->firstElementChild->remove();
$dom->title = "test2";
echo $dom->saveXML(), "\n";

echo "\n=== HTML namespaced test ===\n\n";

$dom = Dom\XMLDocument::createFromString('<root xmlns="http://www.w3.org/1999/xhtml"/>');
$dom->title = "test";
echo $dom->saveXML(), "\n";

$dom = Dom\XMLDocument::createFromString('<html xmlns="http://www.w3.org/1999/xhtml"/>');
$dom->title = "test";
echo $dom->saveXML(), "\n";

$dom = Dom\XMLDocument::createFromString('<html xmlns="http://www.w3.org/1999/xhtml"><foo/><head/></html>');
$dom->title = "test";
echo $dom->saveXML(), "\n";

$dom = Dom\XMLDocument::createFromString('<html xmlns="http://www.w3.org/1999/xhtml"><head><?ignore me?></head></html>');
$dom->title = "test";
echo $dom->saveXML(), "\n";

$dom = Dom\XMLDocument::createFromString('<html xmlns="http://www.w3.org/1999/xhtml"><head><?ignore me?><title>foo<div/></title></head></html>');
$dom->title = "test";
echo $dom->saveXML(), "\n";

echo "\n=== neither namespaced test ===\n\n";

$dom = Dom\XMLDocument::createEmpty();
$dom->title = "";
echo $dom->saveXML(), "\n";

$dom = Dom\XMLDocument::createFromString('<root/>');
$dom->title = "test";
echo $dom->saveXML(), "\n";

"#);
    assert_exact(&out, r#"=== SVG namespaced test ===

<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="http://www.w3.org/2000/svg"/>
<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg"><title>hello &amp;amp; world</title></svg>
<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg"><title></title></svg>
<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg"><title>test</title></svg>
<?xml version="1.0" encoding="UTF-8"?>
<svg:svg xmlns:svg="http://www.w3.org/2000/svg"><svg:title>test</svg:title></svg:svg>
NULL
string(26) "http://www.w3.org/2000/svg"
<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg"><title>test</title>first node<div/></svg>
<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg"><title>test2</title>first node<div/></svg>

=== HTML namespaced test ===

<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="http://www.w3.org/1999/xhtml"></root>
<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"></html>
<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><foo></foo><head><title>test</title></head></html>
<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><?ignore me?><title>test</title></head></html>
<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><?ignore me?><title>test</title></head></html>

=== neither namespaced test ===

<?xml version="1.0" encoding="UTF-8"?>

<?xml version="1.0" encoding="UTF-8"?>
<root/>"#);
}

/// php-src `ext/dom/tests/modern/common/Element_getElementsByClassName_empty.phpt`: Dom\Element::getElementsByClassName() empty class names
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_get_elements_by_class_name_empty() {
    if skip_without_xml_native("test_element_get_elements_by_class_name_empty") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<div class="  foo bar ">
    <p id="child"></p>
</div>
HTML, LIBXML_NOERROR);

$collection = $dom->documentElement->getElementsByClassName("");
var_dump($collection->count());

foreach ($collection as $node) {
    throw new Error("unreachable");
}

var_dump($dom->getElementsByClassName(" ")->count());
var_dump($dom->getElementsByClassName("\t")->count());
var_dump($dom->getElementsByClassName("\t\n\f\v")->count());
var_dump($dom->getElementsByClassName("\t\n\f\v")->namedItem("child"));

?>
"#);
    assert_exact(&out, r#"int(0)
int(0)
int(0)
int(0)
NULL"#);
}

/// php-src `ext/dom/tests/modern/common/Element_getElementsByClassName_non_quirks.phpt`: Dom\Element::getElementsByClassName() non quirks mode
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_get_elements_by_class_name_non_quirks() {
    if skip_without_xml_native("test_element_get_elements_by_class_name_non_quirks") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE html>
<div id="container">
    <p class="Bar">1</p>
    <p class="bar">2</p>
    <p class="Bar Foo">3</p>
    <p class="Bar foo">4</p>
    <p class="foo bar">5</p>
    <p class="foo bar" name="here">6</p>
</div>
<div>
    <p class="Bar">7</p>
    <p class="bar">8</p>
    <p class="Bar Foo">9</p>
    <p class="Bar foo">10</p>
    <p class="foo bar">11</p>
</div>
HTML);

$collection = $dom->getElementsByClassName("foo \n bar");

echo "There are {$collection->length} items in the document in total that have both \"foo\" and \"bar\"\n";

$collection = $dom->getElementById('container')->getElementsByClassName("foo \n bar");

echo "There are {$collection->length} items in #container in total that have both \"foo\" and \"bar\"\n";

foreach ($collection as $key => $node) {
    echo "--- Key $key ---\n";
    var_dump($node->tagName, $node->textContent);
    var_dump($node === $collection->item($key));
}

var_dump($collection->namedItem("here")->textContent);

"#);
    assert_exact(&out, r#"There are 3 items in the document in total that have both "foo" and "bar"
There are 2 items in #container in total that have both "foo" and "bar"
--- Key 0 ---
string(1) "P"
string(1) "5"
bool(true)
--- Key 1 ---
string(1) "P"
string(1) "6"
bool(true)
string(1) "6""#);
}

/// php-src `ext/dom/tests/modern/common/Element_getElementsByClassName_quirks.phpt`: Dom\Element::getElementsByClassName() quirks mode
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_get_elements_by_class_name_quirks() {
    if skip_without_xml_native("test_element_get_elements_by_class_name_quirks") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<div class="  foo bar ">
    <main class="bar">
        <p name="here">1</p>
        <p class="bar" name="here">2</p>
        <p name="here">3</p>
        <p class="bAR" name="here">4</p>
    </main>
    <b class="foo bars"></b>
</div>
HTML, LIBXML_NOERROR);
$collection = $dom->documentElement->getElementsByClassName("Bar");

echo "There are {$dom->getElementsByClassName("foo \n bar")->count()} items in the document in total that have both \"foo\" and \"bar\"\n";

echo "There are {$collection->count()} \"Bar\" items\n";

foreach ($collection as $key => $node) {
    echo "--- Key $key ---\n";
    var_dump($node->tagName, $node->textContent);
    var_dump($node === $collection->item($key));
}

echo "--- named item \"here\" ---\n";

var_dump($collection->namedItem("here")->textContent);

?>
"#);
    assert_exact(&out, r#"There are 1 items in the document in total that have both "foo" and "bar"
There are 4 "Bar" items
--- Key 0 ---
string(3) "DIV"
string(56) "
    
        1
        2
        3
        4
    
    
"
bool(true)
--- Key 1 ---
string(4) "MAIN"
string(45) "
        1
        2
        3
        4
    "
bool(true)
--- Key 2 ---
string(1) "P"
string(1) "2"
bool(true)
--- Key 3 ---
string(1) "P"
string(1) "4"
bool(true)
--- named item "here" ---
string(1) "2""#);
}

/// php-src `ext/dom/tests/modern/common/Element_insertAdjacentElement.phpt`: Element::insertAdjacentElement()
#[test]
#[ignore = r#"DOM port pending: Undefined class constant: Dom\AdjacentPosition::BeforeBegin"#]
fn test_element_insert_adjacent_element() {
    if skip_without_xml_native("test_element_insert_adjacent_element") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<?xml version="1.0"?><container><p>foo</p></container>');
$container = $dom->documentElement;
$p = $container->firstElementChild;

var_dump($p->insertAdjacentElement(Dom\AdjacentPosition::BeforeBegin, $dom->createElement('A'))->tagName);
echo $dom->saveXML(), "\n";

var_dump($p->insertAdjacentElement(Dom\AdjacentPosition::AfterBegin, $dom->createElement('B'))->tagName);
echo $dom->saveXML(), "\n";

var_dump($p->insertAdjacentElement(Dom\AdjacentPosition::BeforeEnd, $dom->createElement('C'))->tagName);
echo $dom->saveXML(), "\n";

var_dump($p->insertAdjacentElement(Dom\AdjacentPosition::AfterEnd, $dom->createElement('D'))->tagName);
echo $dom->saveXML(), "\n";

"#);
    assert_exact(&out, r#"string(1) "A"
<?xml version="1.0" encoding="UTF-8"?>
<container><A/><p>foo</p></container>
string(1) "B"
<?xml version="1.0" encoding="UTF-8"?>
<container><A/><p><B/>foo</p></container>
string(1) "C"
<?xml version="1.0" encoding="UTF-8"?>
<container><A/><p><B/>foo<C/></p></container>
string(1) "D"
<?xml version="1.0" encoding="UTF-8"?>
<container><A/><p><B/>foo<C/></p><D/></container>"#);
}

/// php-src `ext/dom/tests/modern/common/ParentNode_children.phpt`: ParentNode::$children
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_parent_node_children() {
    if skip_without_xml_native("test_parent_node_children") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString(<<<XML
<root>
    <a/>
    <b/>
    <c id="foo"><c1/><!-- comment --><c2/></c>
    <?garbage ?>
    <e/>
</root>
XML);
$children = $dom->documentElement->children;
var_dump($children === $dom->documentElement->children); // Tests caching behaviour
var_dump($children !== (clone $dom->documentElement)->children); // Tests caching behaviour does not persist across clones
var_dump(count($children));
var_dump($children->length);

foreach ($children as $key => $child) {
    var_dump($key, $child->nodeName);
}

foreach ($children->namedItem('foo')->children as $key => $child) {
    var_dump($key, $child->nodeName);
}

"#);
    assert_exact(&out, r#"bool(true)
bool(true)
int(4)
int(4)
int(0)
string(1) "a"
int(1)
string(1) "b"
int(2)
string(1) "c"
int(3)
string(1) "e"
int(0)
string(2) "c1"
int(1)
string(2) "c2""#);
}

/// php-src `ext/dom/tests/modern/common/attr_named_node_map_cache.phpt`: Attribute named node map cache
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_attr_named_node_map_cache() {
    if skip_without_xml_native("test_attr_named_node_map_cache") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root a="1" b="2" c="3"/>');
$attrs = $dom->documentElement->attributes;
var_dump($attrs[1]->nodeName);
$dom->documentElement->removeAttribute('b');
var_dump($attrs[1]->nodeName);

"#);
    assert_exact(&out, r#"string(1) "b"
string(1) "c""#);
}

/// php-src `ext/dom/tests/modern/common/gh21077.phpt`: GH-21077 (Accessing Dom\Node::baseURI can throw TypeError)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\Implementation"#]
fn test_gh21077() {
    if skip_without_xml_native("test_gh21077") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$implementation = new Dom\Implementation();
$node = $implementation->createDocumentType('html', 'publicId', 'systemId');

var_dump($node->baseURI);

$dom = Dom\XMLDocument::createEmpty();
$dom->append($node = $dom->importNode($node));

var_dump($dom->saveXML());

var_dump($node->baseURI);

"#);
    assert_exact(&out, r#"string(11) "about:blank"
string(84) "<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "publicId" "systemId">
"
string(11) "about:blank""#);
}

/// php-src `ext/dom/tests/modern/common/gh21097.phpt`: GH-21097 (Accessing Dom\Node properties can can throw TypeError(s))
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\Implementation"#]
fn test_gh21097() {
    if skip_without_xml_native("test_gh21097") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$implementation = new \Dom\Implementation();
$node = $implementation->createDocumentType('html', 'publicId', 'systemId');

$r = new \ReflectionClass($node);
foreach ($r->getProperties(\ReflectionProperty::IS_PUBLIC) as $p) {
    echo $p->getName(), ": ";
    var_dump($node->{$p->getName()});
}

"#);
    assert_exact(&out, r#"nodeType: int(10)
nodeName: string(4) "html"
baseURI: string(11) "about:blank"
isConnected: bool(false)
ownerDocument: NULL
parentNode: NULL
parentElement: NULL
childNodes: object(Dom\NodeList)#24 (1) {
  ["length"]=>
  int(0)
}
firstChild: NULL
lastChild: NULL
previousSibling: NULL
nextSibling: NULL
nodeValue: NULL
textContent: string(0) ""
name: string(4) "html"
entities: object(Dom\DtdNamedNodeMap)#24 (1) {
  ["length"]=>
  int(0)
}
notations: object(Dom\DtdNamedNodeMap)#24 (1) {
  ["length"]=>
  int(0)
}
publicId: string(8) "publicId"
systemId: string(8) "systemId"
internalSubset: NULL"#);
}

/// php-src `ext/dom/tests/modern/common/innerHTML_cache_invalidation.phpt`: $innerHTML cache invalidation
#[test]
#[ignore = r#"DOM port pending: Cannot index non-array"#]
fn test_inner_html_cache_invalidation() {
    if skip_without_xml_native("test_inner_html_cache_invalidation") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root><a/><a/><a/></root>');
$els = $dom->getElementsByTagName('a');
var_dump($els[0]->tagName);

$dom->documentElement->innerHTML = '<b/>';

echo $dom->saveXML(), "\n";
var_dump($els);
var_dump($els[0]?->tagName);

"#);
    assert_exact(&out, r#"string(1) "a"
<?xml version="1.0" encoding="UTF-8"?>
<root><b/></root>
object(Dom\HTMLCollection)#2 (1) {
  ["length"]=>
  int(0)
}
NULL"#);
}

/// php-src `ext/dom/tests/modern/common/namespace_sxe_interaction.phpt`: Serialization interaction between simplexml and dom for namespaces
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createElementNS"#]
fn test_namespace_sxe_interaction() {
    if skip_without_xml_native("test_namespace_sxe_interaction") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root/>');
$sxe = simplexml_import_dom($dom);

$sxe->addAttribute('a:attr', 'value', 'urn:a');
$sxe->addChild('b:child', 'value', 'urn:b');
$sxe->addChild('foo', 'value2');
$dom->documentElement->firstElementChild->appendChild($dom->createElementNS('urn:c', 'c:child'));

echo "namespace c: ";
var_dump($dom->documentElement->firstElementChild->firstElementChild->lookupNamespaceURI('c'));
echo "namespace b: ";
var_dump($dom->documentElement->firstElementChild->firstElementChild->lookupNamespaceURI('b'));
echo "namespace a: ";
var_dump($dom->documentElement->firstElementChild->firstElementChild->lookupNamespaceURI('a'));

echo "=== serialize SimpleXML ===\n";

echo $sxe->saveXML(), "\n";
echo $sxe->foo->saveXML(), "\n";
$sxe->asXML(__DIR__ . "/namespace_sxe_interaction1.xml");
$sxe->foo->asXML(__DIR__ . "/namespace_sxe_interaction2.xml");
echo file_get_contents(__DIR__ . "/namespace_sxe_interaction1.xml"), "\n";
echo file_get_contents(__DIR__ . "/namespace_sxe_interaction2.xml"), "\n";

echo "=== serialize DOM ===\n";

echo $dom->saveXML(), "\n\n";

echo "=== serialize imported DOM ===\n";

// Importing should yield the exact same document
$new_dom = Dom\XMLDocument::createEmpty();
$new_dom->append($new_dom->importNode($dom->documentElement, true));
echo $new_dom->saveXML(), "\n";

"#);
    assert_exact(&out, r#"namespace c: string(5) "urn:c"
namespace b: string(5) "urn:b"
namespace a: NULL
=== serialize SimpleXML ===
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:a="urn:a" a:attr="value"><b:child xmlns:b="urn:b">value<c:child xmlns:c="urn:c"/></b:child><foo>value2</foo></root>
<foo>value2</foo>
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:a="urn:a" a:attr="value"><b:child xmlns:b="urn:b">value<c:child xmlns:c="urn:c"/></b:child><foo>value2</foo></root>
<foo>value2</foo>
=== serialize DOM ===
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:a="urn:a" a:attr="value"><b:child xmlns:b="urn:b">value<c:child xmlns:c="urn:c"/></b:child><foo>value2</foo></root>

=== serialize imported DOM ===
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:a="urn:a" a:attr="value"><b:child xmlns:b="urn:b">value<c:child xmlns:c="urn:c"/></b:child><foo>value2</foo></root>"#);
}

/// php-src `ext/dom/tests/modern/common/serialize_entity_reference_in_attribute.phpt`: Serialize entity reference within attribute
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_serialize_entity_reference_in_attribute() {
    if skip_without_xml_native("test_serialize_entity_reference_in_attribute") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$xml = Dom\XMLDocument::createFromString(<<<XML
<!DOCTYPE root [
    <!ENTITY foo "foo">
]>
<root><el x="&foo;bar&foo;"/></root>
XML);

$el = $xml->documentElement->firstChild;
echo $xml->saveXml(), "\n";

$html = Dom\HTMLDocument::createEmpty();
$html->append($html->importNode($el, true));
echo $html->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE root [
<!ENTITY foo "foo">
]>
<root><el x="&foo;bar&foo;"/></root>
<el x="&foo;bar&foo;"></el>"#);
}

/// php-src `ext/dom/tests/modern/common/template_cloning.phpt`: Template cloning
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_template_cloning() {
    if skip_without_xml_native("test_template_cloning") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php
$dom = Dom\HTMLDocument::createFromString('<template>x</template>', LIBXML_NOERROR);
$a = $dom->head->firstChild->cloneNode(false);
echo $dom->saveXML($a), "\n";
echo $dom->saveHTML($a), "\n";
"#);
    assert_exact(&out, r#"<template xmlns="http://www.w3.org/1999/xhtml"></template>
<template></template>"#);
}

/// php-src `ext/dom/tests/modern/common/template_indirect_removal.phpt`: template content indirect removal
#[test]
#[ignore = r#"php-src SKIPIF: skip only for debug build"#]
fn test_template_indirect_removal() {
    if skip_without_xml_native("test_template_indirect_removal") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php
$dom = Dom\HTMLDocument::createFromString('<template>foo<template>nested</template></template>', LIBXML_NOERROR);
$head = $dom->head;
var_dump($dom->debugGetTemplateCount());
$head->remove();
var_dump($dom->debugGetTemplateCount());
unset($head);
var_dump($dom->debugGetTemplateCount());
"#);
    assert_exact(&out, r#"int(2)
int(2)
int(0)"#);
}

/// php-src `ext/dom/tests/modern/common/template_manual.phpt`: <template> element manual creation
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_template_manual() {
    if skip_without_xml_native("test_template_manual") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

echo "=== After creation ===\n";

$dom = Dom\HTMLDocument::createEmpty();
$template = $dom->appendChild($dom->createElement("template"));
var_dump($template->innerHTML);
echo $dom->saveXML(), "\n";
echo $dom->saveHTML(), "\n";

echo "=== After setting content ===\n";

$template->innerHTML = "<p>hello</template></p>";
var_dump($template->innerHTML);
var_dump($template->firstChild);
echo $dom->saveXML(), "\n";
echo $dom->saveHTML(), "\n";

"#);
    assert_exact(&out, r#"=== After creation ===
string(0) ""
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<template xmlns="http://www.w3.org/1999/xhtml"></template>
<template></template>
=== After setting content ===
string(12) "<p>hello</p>"
NULL
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<template xmlns="http://www.w3.org/1999/xhtml"><p>hello</p></template>
<template><p>hello</p></template>"#);
}

/// php-src `ext/dom/tests/modern/common/template_nested.phpt`: <template> element nesting
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_template_nested() {
    if skip_without_xml_native("test_template_nested") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$html = <<<HTML
<!DOCTYPE html>
<html>
    <body>
        <template>foo<template>bar</template></template>
    </body>
</html>
HTML;
$dom = Dom\HTMLDocument::createFromString($html);
$template = $dom->body->firstElementChild;
var_dump($template->innerHTML);
echo $dom->saveXML();

"#);
    assert_exact(&out, r#"string(27) "foo<template>bar</template>"
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body>
        <template>foo<template>bar</template></template>
    
</body></html>"#);
}

/// php-src `ext/dom/tests/modern/common/template_no_default_ns.phpt`: <template> element no default namespace
#[test]
#[ignore = r#"DOM port pending: Undefined constant: Dom\HTML_NO_DEFAULT_NS"#]
fn test_template_no_default_ns() {
    if skip_without_xml_native("test_template_no_default_ns") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$html = <<<HTML
<!DOCTYPE html>
<html>
    <body>
        <template>a<div>foo</div>b</template>
    </body>
</html>
HTML;
$dom = Dom\HTMLDocument::createFromString($html, Dom\HTML_NO_DEFAULT_NS);
$template = $dom->getElementsByTagName('template')[0];
var_dump($template->innerHTML);
var_dump($template->firstElementChild->tagName);

"#);
    assert_exact(&out, r#"string(16) "a<div>foo</div>b"
string(3) "div""#);
}

/// php-src `ext/dom/tests/modern/common/template_participation.phpt`: <template> element contents do not participate in DOM
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_template_participation() {
    if skip_without_xml_native("test_template_participation") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$html = <<<HTML
<!DOCTYPE html>
<html>
    <body>
        <template>a<div>foo</div>b</template>
    </body>
</html>
HTML;
$dom = Dom\HTMLDocument::createFromString($html);
$template = $dom->body->firstElementChild;

echo "=== Manipulation ===\n";

echo "First child of template: ";
var_dump($template->firstChild?->nodeName);
$template->append($dom->createElement('invisible'));

echo "First child of template after appending: ";
var_dump($template->firstChild->nodeName);
$template->innerHTML = $template->innerHTML;
echo "Inner HTML after idempotent modification: ";
var_dump($template->innerHTML);
echo "Selector should not find div element in shadow DOM: ";
var_dump($template->querySelector('div'));

echo "XPath should not find div element in shadow DOM:\n";
$xpath = new Dom\XPath($dom);
var_dump($xpath->query('//div'));

echo "=== HTML serialization ===\n";
echo $dom->saveHTML(), "\n";
echo "=== HTML serialization of <template> ===\n";
echo $dom->saveHTML($template), "\n";
echo "=== XML serialization ===\n";
echo $dom->saveXML(), "\n";
echo "=== XML serialization of <template> ===\n";
echo $dom->saveXML($template), "\n";

// Should not crash
$template->remove();
unset($template);

echo "=== Creating a new template should not leak the old contents ===\n";
$template = $dom->createElement('template');
var_dump($template->innerHTML);

"#);
    assert_exact(&out, r#"=== Manipulation ===
First child of template: NULL
First child of template after appending: string(9) "INVISIBLE"
Inner HTML after idempotent modification: string(16) "a<div>foo</div>b"
Selector should not find div element in shadow DOM: NULL
XPath should not find div element in shadow DOM:
object(Dom\NodeList)#4 (1) {
  ["length"]=>
  int(0)
}
=== HTML serialization ===
<!DOCTYPE html><html><head></head><body>
        <template>a<div>foo</div>b</template>
    
</body></html>
=== HTML serialization of <template> ===
<template>a<div>foo</div>b</template>
=== XML serialization ===
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body>
        <template>a<div>foo</div>b</template>
    
</body></html>
=== XML serialization of <template> ===
<template xmlns="http://www.w3.org/1999/xhtml">a<div>foo</div>b</template>
=== Creating a new template should not leak the old contents ===
string(0) """#);
}

/// php-src `ext/dom/tests/modern/common/template_rename.phpt`: <template> element renaming
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_template_rename() {
    if skip_without_xml_native("test_template_rename") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$html = <<<HTML
<!DOCTYPE html>
<html>
    <body>
        <template>a<div>foo</div>b</template>
    </body>
</html>
HTML;
$dom = Dom\HTMLDocument::createFromString($html);
$template = $dom->body->firstElementChild;
var_dump($template->innerHTML);

try {
    $template->rename($template->namespaceURI, 'screwthis');
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

// These shouldn't be changed!
var_dump($template->nodeName);
var_dump($template->innerHTML);

"#);
    assert_exact(&out, r#"string(16) "a<div>foo</div>b"
It is not possible to rename the template element because it hosts a document fragment
string(8) "TEMPLATE"
string(16) "a<div>foo</div>b""#);
}

/// php-src `ext/dom/tests/modern/common/template_simplexml.phpt`: SimpleXML and template content
#[test]
#[ignore = r#"php-src SKIPIF: skip only for debug build"#]
fn test_template_simplexml() {
    if skip_without_xml_native("test_template_simplexml") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php
$dom = Dom\HTMLDocument::createFromString('<template>foo<template>nested</template></template>', LIBXML_NOERROR);
$head = $dom->head;
$head_sxe = simplexml_import_dom($head);
var_dump($head_sxe);
var_dump($dom->debugGetTemplateCount());
unset($head_sxe->template);
var_dump($head_sxe);
var_dump($dom->debugGetTemplateCount());
"#);
    assert_format(&out, r#"object(SimpleXMLElement)#%d (1) {
  ["template"]=>
  object(SimpleXMLElement)#%d (0) {
  }
}
int(2)
object(SimpleXMLElement)#%d (0) {
}
int(0)"#);
}

/// php-src `ext/dom/tests/modern/common/unpacking_foreach.phpt`: unpacking vs foreach in new DOM
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_unpacking_foreach() {
    if skip_without_xml_native("test_unpacking_foreach") {
        return;
    }
    let out = run_case(r#"modern/common"#, &[], r#"<?php

$html = Dom\HTMLDocument::createFromString
(
    source: "<h1>Hi</h1><p>hi</p>",
    options: LIBXML_NOERROR,
);

foreach ($html->body->childNodes as $node) {
    echo $node->localName, "\n";
}

echo "---\n";

foreach ([...$html->body->childNodes] as $node) {
    echo $node->localName, "\n";
}

"#);
    assert_exact(&out, r#"h1
p
---
h1
p"#);
}
