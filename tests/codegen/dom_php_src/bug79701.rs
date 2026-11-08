//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `bug79701` folder, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/bug79701/id_property.phpt`: Bug #79701 (getElementById does not correctly work with duplicate definitions) - id property variation
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::getElementById"#]
fn test_id_property() {
    if skip_without_xml_native("test_id_property") {
        return;
    }
    let out = run_case(r#"bug79701"#, &[], r#"<?php
$dom = Dom\XMLDocument::createFromString(<<<XML
<root>
    <test1/>
    <test2/>
</root>
XML);

$test1 = $dom->documentElement->firstElementChild;
$test2 = $test1->nextElementSibling;

echo "--- After parsing ---\n";
var_dump($dom->getElementById("x")?->nodeName);

echo "--- After setting test2 ---\n";
$test2->id = "x";
var_dump($dom->getElementById("x")?->nodeName);
echo "--- After setting test1 ---\n";
$test1->id = "x";
var_dump($dom->getElementById("x")?->nodeName);
echo "--- After resetting test1 ---\n";
$test1->id = "y";
var_dump($dom->getElementById("x")?->nodeName);
echo "--- After resetting test2 ---\n";
$test2->id = "y";
var_dump($dom->getElementById("x")?->nodeName);
echo "--- After resetting test1 ---\n";
$test1->id = "x";
var_dump($dom->getElementById("x")?->nodeName);
echo "--- After calling setIdAttribute with false on test1 ---\n";
$test1->setIdAttribute("id", false);
var_dump($dom->getElementById("x")?->nodeName);
"#);
    assert_exact(&out, r#"--- After parsing ---
NULL
--- After setting test2 ---
string(5) "test2"
--- After setting test1 ---
string(5) "test1"
--- After resetting test1 ---
string(5) "test2"
--- After resetting test2 ---
NULL
--- After resetting test1 ---
string(5) "test1"
--- After calling setIdAttribute with false on test1 ---
NULL"#);
}

/// php-src `ext/dom/tests/bug79701/node.phpt`: Bug #79701 (getElementById does not correctly work with duplicate definitions) - attribute node variation
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_node() {
    if skip_without_xml_native("test_node") {
        return;
    }
    let out = run_case(r#"bug79701"#, &[], r#"<?php
$dom = Dom\HTMLDocument::createEmpty();
$element = $dom->createElement('foo');
$dom->append($element);
$attr = $dom->createAttribute('id');
$attr->value = 'test';
var_dump($dom->getElementById('test')?->nodeName);
$element->setAttributeNode($attr);
var_dump($dom->getElementById('test')?->nodeName);
"#);
    assert_exact(&out, r#"NULL
string(3) "FOO""#);
}

/// php-src `ext/dom/tests/bug79701/prepend.phpt`: Bug #79701 (getElementById does not correctly work with duplicate definitions) - prepending variation
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMElement::setIdAttribute"#]
fn test_prepend() {
    if skip_without_xml_native("test_prepend") {
        return;
    }
    let out = run_case(r#"bug79701"#, &[], r#"<?php
$dom = new DOMDocument();
$root = $dom->createElement('html');
$dom->appendChild($root);

$el1 = $dom->createElement('p1');
$el1->setAttribute('id', 'foo');
$el1->setIdAttribute('id', true);

$root->appendChild($el1);

$el2 = $dom->createElement('p2');
$el2->setAttribute('id', 'foo');
$el2->setIdAttribute('id', true);

$root->appendChild($el2);
unset($el1, $el2);

$root->removeChild($dom->getElementById('foo'));

var_dump($dom->getElementById('foo')?->nodeName);
"#);
    assert_exact(&out, r#"string(2) "p2""#);
}

/// php-src `ext/dom/tests/bug79701/remove_attribute.phpt`: Bug #79701 (getElementById does not correctly work with duplicate definitions) - remove attribute variation
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::getElementById"#]
fn test_remove_attribute() {
    if skip_without_xml_native("test_remove_attribute") {
        return;
    }
    let out = run_case(r#"bug79701"#, &[], r#"<?php
$dom = Dom\XMLDocument::createFromString(<<<XML
<root>
    <test1 xml:id="x"/>
    <test2 xml:id="x"/>
</root>
XML);

var_dump($dom->getElementById('x')?->nodeName);
$dom->getElementById('x')->removeAttribute('xml:id');
var_dump($dom->getElementById('x')?->nodeName);
"#);
    assert_format(&out, r#"Warning: Dom\XMLDocument::createFromString(): ID x already defined in Entity, line: 3 in %s on line %d
string(5) "test1"
NULL"#);
}

/// php-src `ext/dom/tests/bug79701/set_attr_value.phpt`: Bug #79701 (getElementById does not correctly work with duplicate definitions) - nodeValue / value variation
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::getElementById"#]
fn test_set_attr_value() {
    if skip_without_xml_native("test_set_attr_value") {
        return;
    }
    let out = run_case(r#"bug79701"#, &[], r#"<?php
foreach (["value", "nodeValue"] as $property) {
    echo "--- Testing property \$$property ---\n";
    $dom = Dom\XMLDocument::createFromString(<<<XML
    <root>
        <test1 xml:id="x"/>
        <test2 xml:id="y"/>
    </root>
    XML);

    $test1 = $dom->getElementById('x');
    $test2 = $dom->getElementById('y');

    var_dump($test1?->nodeName);
    var_dump($test2?->nodeName);

    $test1->getAttributeNode('xml:id')->$property = "y";
    var_dump($dom->getElementById('x')?->nodeName);
    var_dump($dom->getElementById('y')?->nodeName);

    $test2->getAttributeNode('xml:id')->$property = "x";
    var_dump($dom->getElementById('x')?->nodeName);
    var_dump($dom->getElementById('y')?->nodeName);
}
"#);
    assert_exact(&out, r#"--- Testing property $value ---
string(5) "test1"
string(5) "test2"
NULL
string(5) "test1"
string(5) "test2"
string(5) "test1"
--- Testing property $nodeValue ---
string(5) "test1"
string(5) "test2"
NULL
string(5) "test1"
string(5) "test2"
string(5) "test1""#);
}

/// php-src `ext/dom/tests/bug79701/set_attribute_html.phpt`: Bug #79701 (getElementById does not correctly work with duplicate definitions) - set attribute in html variation
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_set_attribute_html() {
    if skip_without_xml_native("test_set_attribute_html") {
        return;
    }
    let out = run_case(r#"bug79701"#, &[], r#"<?php
foreach (["xml:id", "ID"] as $name) {
    $dom = Dom\HTMLDocument::createFromString(<<<HTML
    <p>
        <em id="x">1</em>
        <strong id="y">2</strong>
    </p>
    HTML, LIBXML_NOERROR);

    echo "\n=== Attribute name $name ===\n\n";

    $test1 = $dom->getElementById('x');
    $test2 = $dom->getElementById('y');

    echo "--- After resetting em's id ---\n";

    $test1->setAttribute($name, 'y');
    var_dump($dom->getElementById('x')?->nodeName);
    var_dump($dom->getElementById('y')?->nodeName);

    echo "--- After resetting strong's id ---\n";

    $test2->setAttribute($name, 'x');
    var_dump($dom->getElementById('x')?->nodeName);
    var_dump($dom->getElementById('y')?->nodeName);

    echo "--- After resetting em's id ---\n";

    $test1->setAttribute($name, 'z');
    var_dump($dom->getElementById('x')?->nodeName);
    var_dump($dom->getElementById('y')?->nodeName);

    echo "--- After resetting strong's id ---\n";

    $test2->setAttribute($name, 'z');
    var_dump($dom->getElementById('x')?->nodeName);
    var_dump($dom->getElementById('y')?->nodeName);

    echo "--- Get id z ---\n";

    var_dump($dom->getElementById('z')?->nodeName);
}
?>
"#);
    assert_exact(&out, r#"=== Attribute name xml:id ===

--- After resetting em's id ---
string(2) "EM"
string(6) "STRONG"
--- After resetting strong's id ---
string(2) "EM"
string(6) "STRONG"
--- After resetting em's id ---
string(2) "EM"
string(6) "STRONG"
--- After resetting strong's id ---
string(2) "EM"
string(6) "STRONG"
--- Get id z ---
NULL

=== Attribute name ID ===

--- After resetting em's id ---
NULL
string(2) "EM"
--- After resetting strong's id ---
string(6) "STRONG"
string(2) "EM"
--- After resetting em's id ---
string(6) "STRONG"
NULL
--- After resetting strong's id ---
NULL
NULL
--- Get id z ---
string(2) "EM""#);
}

/// php-src `ext/dom/tests/bug79701/set_attribute_ns_html.phpt`: Bug #79701 (getElementById does not correctly work with duplicate definitions) - set attribute ns in html variation
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_set_attribute_ns_html() {
    if skip_without_xml_native("test_set_attribute_ns_html") {
        return;
    }
    let out = run_case(r#"bug79701"#, &[], r#"<?php
function test($namespace) {
    $dom = Dom\HTMLDocument::createFromString(<<<HTML
    <p>
        <em id="x">1</em>
        <strong id="y">2</strong>
    </p>
    HTML, LIBXML_NOERROR);

    $test1 = $dom->getElementById('x');
    $test2 = $dom->getElementById('y');

    echo "--- After resetting em's id ---\n";

    $test1->setAttributeNS($namespace, "id", 'y');
    var_dump($dom->getElementById('x')?->nodeName);
    var_dump($dom->getElementById('y')?->nodeName);

    echo "--- After resetting strong's id ---\n";

    $test2->setAttributeNS($namespace, "id", 'x');
    var_dump($dom->getElementById('x')?->nodeName);
    var_dump($dom->getElementById('y')?->nodeName);

    echo "--- After resetting em's id ---\n";

    $test1->setAttributeNS($namespace, "id", 'z');
    var_dump($dom->getElementById('x')?->nodeName);
    var_dump($dom->getElementById('y')?->nodeName);

    echo "--- After resetting strong's id ---\n";

    $test2->setAttributeNS($namespace, "id", 'z');
    var_dump($dom->getElementById('x')?->nodeName);
    var_dump($dom->getElementById('y')?->nodeName);

    echo "--- Get id z ---\n";

    var_dump($dom->getElementById('z')?->nodeName);
}

echo "=== Test empty namespace ===\n\n";
test("");
echo "\n=== Test \"urn:x\" namespace ===\n\n";
test("urn:x");
?>
"#);
    assert_exact(&out, r#"=== Test empty namespace ===

--- After resetting em's id ---
NULL
string(2) "EM"
--- After resetting strong's id ---
string(6) "STRONG"
string(2) "EM"
--- After resetting em's id ---
string(6) "STRONG"
NULL
--- After resetting strong's id ---
NULL
NULL
--- Get id z ---
string(2) "EM"

=== Test "urn:x" namespace ===

--- After resetting em's id ---
string(2) "EM"
string(6) "STRONG"
--- After resetting strong's id ---
string(2) "EM"
string(6) "STRONG"
--- After resetting em's id ---
string(2) "EM"
string(6) "STRONG"
--- After resetting strong's id ---
string(2) "EM"
string(6) "STRONG"
--- Get id z ---
NULL"#);
}

/// php-src `ext/dom/tests/bug79701/set_attribute_xml.phpt`: Bug #79701 (getElementById does not correctly work with duplicate definitions) - set attribute in xml variation
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::getElementById"#]
fn test_set_attribute_xml() {
    if skip_without_xml_native("test_set_attribute_xml") {
        return;
    }
    let out = run_case(r#"bug79701"#, &[], r#"<?php
function test($dom, $fn) {
    $test1 = $dom->getElementById('x');
    $test2 = $dom->getElementById('y');

    echo "--- After resetting test1's id ---\n";

    $fn($test1, 'xml:id', 'y');
    var_dump($dom->getElementById('x')?->nodeName);
    var_dump($dom->getElementById('y')?->nodeName);

    echo "--- After resetting test2's id ---\n";

    $fn($test2, 'xml:id', 'x');
    var_dump($dom->getElementById('x')?->nodeName);
    var_dump($dom->getElementById('y')?->nodeName);

    echo "--- After resetting test1's id ---\n";

    $fn($test1, 'xml:id', 'z');
    var_dump($dom->getElementById('x')?->nodeName);
    var_dump($dom->getElementById('y')?->nodeName);

    echo "--- After resetting test2's id ---\n";

    $fn($test2, 'xml:id', 'z');
    var_dump($dom->getElementById('x')?->nodeName);
    var_dump($dom->getElementById('y')?->nodeName);

    echo "--- Get id z ---\n";

    var_dump($dom->getElementById('z')?->nodeName);
}

function getNamespace($name) {
    if (str_contains($name, 'xml:')) {
        return 'http://www.w3.org/XML/1998/namespace';
    }
    return '';
}

$common_xml = <<<XML
<root>
  <test1 xml:id="x"/>
  <test2 xml:id="y"/>
</root>
XML;

echo "\n=== DOMDocument: setAttribute ===\n\n";

$dom = new DOMDocument;
$dom->loadXML($common_xml);
test($dom, fn ($element, $name, $value) => $element->setAttribute($name, $value));

echo "\n=== DOMDocument: setAttributeNS ===\n\n";

$dom = new DOMDocument;
$dom->loadXML($common_xml);
test($dom, fn ($element, $name, $value) => $element->setAttributeNS(getNamespace($name), $name, $value));

echo "\n=== Dom\\XMLDocument: setAttribute ===\n\n";

$dom = Dom\XMLDocument::createFromString($common_xml);
test($dom, fn ($element, $name, $value) => $element->setAttribute($name, $value));

echo "\n=== Dom\\XMLDocument: setAttributeNS ===\n\n";

$dom = Dom\XMLDocument::createFromString($common_xml);
test($dom, fn ($element, $name, $value) => $element->setAttributeNS(getNamespace($name), $name, $value));
"#);
    assert_exact(&out, r#"=== DOMDocument: setAttribute ===

--- After resetting test1's id ---
NULL
string(5) "test1"
--- After resetting test2's id ---
string(5) "test2"
string(5) "test1"
--- After resetting test1's id ---
string(5) "test2"
NULL
--- After resetting test2's id ---
NULL
NULL
--- Get id z ---
string(5) "test1"

=== DOMDocument: setAttributeNS ===

--- After resetting test1's id ---
NULL
string(5) "test1"
--- After resetting test2's id ---
string(5) "test2"
string(5) "test1"
--- After resetting test1's id ---
string(5) "test2"
NULL
--- After resetting test2's id ---
NULL
NULL
--- Get id z ---
string(5) "test1"

=== Dom\XMLDocument: setAttribute ===

--- After resetting test1's id ---
NULL
string(5) "test1"
--- After resetting test2's id ---
string(5) "test2"
string(5) "test1"
--- After resetting test1's id ---
string(5) "test2"
NULL
--- After resetting test2's id ---
NULL
NULL
--- Get id z ---
string(5) "test1"

=== Dom\XMLDocument: setAttributeNS ===

--- After resetting test1's id ---
NULL
string(5) "test1"
--- After resetting test2's id ---
string(5) "test2"
string(5) "test1"
--- After resetting test1's id ---
string(5) "test2"
NULL
--- After resetting test2's id ---
NULL
NULL
--- Get id z ---
string(5) "test1""#);
}

/// php-src `ext/dom/tests/bug79701/swap.phpt`: Bug #79701 (getElementById does not correctly work with duplicate definitions) - swapping variation
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::getElementById"#]
fn test_swap() {
    if skip_without_xml_native("test_swap") {
        return;
    }
    let out = run_case(r#"bug79701"#, &[], r#"<?php
$dom = new DOMDocument;
$dom->loadXML(<<<XML
<root>
    <test1 xml:id="x"/>
    <test2 attr="x"/>
</root>
XML);

$test1 = $dom->getElementById('x');

echo "--- After parsing ---\n";
var_dump($dom->getElementById('x')?->nodeName);
echo "--- After adding a new id attribute ---\n";
$dom->getElementsByTagName('test2')[0]->setIdAttribute('attr', true);
var_dump($dom->getElementById('x')?->nodeName);
echo "--- After removing the first id ---\n";
$dom->getElementById('x')->remove();
var_dump($dom->getElementById('x')?->nodeName);
echo "--- After removing the second id ---\n";
$dom->getElementById('x')->remove();
var_dump($dom->getElementById('x')?->nodeName);
echo "--- After re-adding the first id ---\n";
$dom->documentElement->appendChild($test1);
var_dump($dom->getElementById('x')?->nodeName);
echo "--- After changing the first id ---\n";
$test1->setAttribute('xml:id', 'y');
var_dump($dom->getElementById('x')?->nodeName);
"#);
    assert_exact(&out, r#"--- After parsing ---
string(5) "test1"
--- After adding a new id attribute ---
string(5) "test1"
--- After removing the first id ---
string(5) "test2"
--- After removing the second id ---
NULL
--- After re-adding the first id ---
string(5) "test1"
--- After changing the first id ---
NULL"#);
}

/// php-src `ext/dom/tests/bug79701/toggle.phpt`: Bug #79701 (getElementById does not correctly work with duplicate definitions) - toggle variation
#[test]
#[ignore = r#"DOM port pending: Undefined constant: LIBXML_HTML_NOIMPLIED"#]
fn test_toggle() {
    if skip_without_xml_native("test_toggle") {
        return;
    }
    let out = run_case(r#"bug79701"#, &[], r#"<?php
$dom = Dom\HTMLDocument::createFromString('<p id="test">foo</p>', LIBXML_NOERROR | LIBXML_HTML_NOIMPLIED);
var_dump($dom->getElementById('test')?->nodeName);
$dom->documentElement->toggleAttribute('id');
var_dump($dom->getElementById('test')?->nodeName);
$dom->documentElement->toggleAttribute('id');
var_dump($dom->getElementById('test')?->nodeName);
"#);
    assert_exact(&out, r#"string(1) "P"
NULL
NULL"#);
}

/// php-src `ext/dom/tests/bug79701/unconnected.phpt`: Bug #79701 (getElementById does not correctly work with duplicate definitions) - unconnected variation
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_unconnected() {
    if skip_without_xml_native("test_unconnected") {
        return;
    }
    let out = run_case(r#"bug79701"#, &[], r#"<?php
$dom = Dom\HTMLDocument::createEmpty();
$element = $dom->createElement('foo');
$element->setAttribute('id', 'test');
var_dump($dom->getElementById('test')?->nodeName);
$dom->append($element);
var_dump($dom->getElementById('test')?->nodeName);
"#);
    assert_exact(&out, r#"NULL
string(3) "FOO""#);
}
