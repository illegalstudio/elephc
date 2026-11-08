//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `compare/document/position` folder, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/compareDocumentPosition/attribute_child_order.phpt`: compareDocumentPosition: attribute vs child order
#[test]
#[ignore = r#"DOM port pending: Undefined class constant: DOMNode::DOCUMENT_POSITION_PRECEDING"#]
fn test_attribute_child_order() {
    if skip_without_xml_native("test_attribute_child_order") {
        return;
    }
    let out = run_case(r#"compareDocumentPosition"#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<container>
<before/>
<outer align="center">
    <p>foo</p>
    <div>
        <div>
            <p>bar</p>
        </div>
    </div>
</outer>
</container>
XML);

$before = $dom->documentElement->firstElementChild;
$outer = $before->nextElementSibling;
$foo = $outer->firstElementChild;
$bar = $foo->nextElementSibling->firstElementChild->firstElementChild;

// See note about attributes vs children positions: attributes precede children

echo "--- outer attribute vs before ---\n";
var_dump($outer->attributes[0]->compareDocumentPosition($before) === DOMNode::DOCUMENT_POSITION_PRECEDING);
var_dump($before->compareDocumentPosition($outer->attributes[0]) === DOMNode::DOCUMENT_POSITION_FOLLOWING);

echo "--- outer attribute vs foo ---\n";
var_dump($outer->attributes[0]->compareDocumentPosition($foo) === DOMNode::DOCUMENT_POSITION_FOLLOWING);
var_dump($foo->compareDocumentPosition($outer->attributes[0]) === DOMNode::DOCUMENT_POSITION_PRECEDING);

echo "--- outer attribute vs bar ---\n";
var_dump($outer->attributes[0]->compareDocumentPosition($bar) === DOMNode::DOCUMENT_POSITION_FOLLOWING);
var_dump($bar->compareDocumentPosition($outer->attributes[0]) === DOMNode::DOCUMENT_POSITION_PRECEDING);

"#);
    assert_exact(&out, r#"--- outer attribute vs before ---
bool(true)
bool(true)
--- outer attribute vs foo ---
bool(true)
bool(true)
--- outer attribute vs bar ---
bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/compareDocumentPosition/attribute_order_different_element.phpt`: compareDocumentPosition: attribute order for different element
#[test]
#[ignore = r#"DOM port pending: Undefined class constant: DOMNode::DOCUMENT_POSITION_FOLLOWING"#]
fn test_attribute_order_different_element() {
    if skip_without_xml_native("test_attribute_order_different_element") {
        return;
    }
    let out = run_case(r#"compareDocumentPosition"#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<outer>
    <inner a="b" c="d" e="f" />
    <inner a="b" x="y" />
</outer>
XML);

$attrs1 = $dom->documentElement->firstElementChild->attributes;
$attrs2 = $dom->documentElement->firstElementChild->nextElementSibling->attributes;

foreach ($attrs1 as $attr1) {
    foreach ($attrs2 as $attr2) {
        echo "--- Compare 1->2 {$attr1->name} and {$attr2->name} ---\n";
        var_dump($attr1->compareDocumentPosition($attr2) === DOMNode::DOCUMENT_POSITION_FOLLOWING);
        echo "--- Compare 2->1 {$attr1->name} and {$attr2->name} ---\n";
        var_dump($attr2->compareDocumentPosition($attr1) === DOMNode::DOCUMENT_POSITION_PRECEDING);
    }
}

"#);
    assert_exact(&out, r#"--- Compare 1->2 a and a ---
bool(true)
--- Compare 2->1 a and a ---
bool(true)
--- Compare 1->2 a and x ---
bool(true)
--- Compare 2->1 a and x ---
bool(true)
--- Compare 1->2 c and a ---
bool(true)
--- Compare 2->1 c and a ---
bool(true)
--- Compare 1->2 c and x ---
bool(true)
--- Compare 2->1 c and x ---
bool(true)
--- Compare 1->2 e and a ---
bool(true)
--- Compare 2->1 e and a ---
bool(true)
--- Compare 1->2 e and x ---
bool(true)
--- Compare 2->1 e and x ---
bool(true)"#);
}

/// php-src `ext/dom/tests/compareDocumentPosition/attribute_order_same_element.phpt`: compareDocumentPosition: attribute order for same element
#[test]
#[ignore = r#"DOM port pending: Undefined class constant: DOMNode::DOCUMENT_POSITION_FOLLOWING"#]
fn test_attribute_order_same_element() {
    if skip_without_xml_native("test_attribute_order_same_element") {
        return;
    }
    let out = run_case(r#"compareDocumentPosition"#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<container a="b" c="d" e="f" />
XML);

$attrs = $dom->documentElement->attributes;
for ($i = 0; $i <= 2; $i++) {
    for ($j = $i + 1; $j <= $i + 2; $j++) {
        echo "--- Compare $i and ", ($j % 3), " ---\n";
        if ($i < ($j % 3)) {
            $expected = DOMNode::DOCUMENT_POSITION_FOLLOWING | DOMNode::DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
        } else {
            $expected = DOMNode::DOCUMENT_POSITION_PRECEDING | DOMNode::DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
        }
        var_dump($attrs[$i]->compareDocumentPosition($attrs[$j % 3]) === $expected);
    }
}

"#);
    assert_exact(&out, r#"--- Compare 0 and 1 ---
bool(true)
--- Compare 0 and 2 ---
bool(true)
--- Compare 1 and 2 ---
bool(true)
--- Compare 1 and 0 ---
bool(true)
--- Compare 2 and 0 ---
bool(true)
--- Compare 2 and 1 ---
bool(true)"#);
}

/// php-src `ext/dom/tests/compareDocumentPosition/contains_attribute_direct_descendent.phpt`: compareDocumentPosition: contains attribute as a direct descendent
#[test]
#[ignore = r#"DOM port pending: Undefined class constant: DOMNode::DOCUMENT_POSITION_FOLLOWING"#]
fn test_contains_attribute_direct_descendent() {
    if skip_without_xml_native("test_contains_attribute_direct_descendent") {
        return;
    }
    let out = run_case(r#"compareDocumentPosition"#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<container align="center"/>
XML);

$container = $dom->documentElement;
$attribute = $container->attributes[0];

var_dump($container->compareDocumentPosition($attribute) === (DOMNode::DOCUMENT_POSITION_FOLLOWING | DOMNode::DOCUMENT_POSITION_CONTAINED_BY));
var_dump($attribute->compareDocumentPosition($container) === (DOMNode::DOCUMENT_POSITION_PRECEDING | DOMNode::DOCUMENT_POSITION_CONTAINS));

"#);
    assert_exact(&out, r#"bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/compareDocumentPosition/contains_attribute_freestanding.phpt`: compareDocumentPosition: contains attribute for a freestanding element
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMElement::attributes"#]
fn test_contains_attribute_freestanding() {
    if skip_without_xml_native("test_contains_attribute_freestanding") {
        return;
    }
    let out = run_case(r#"compareDocumentPosition"#, &[], r#"<?php

$dom = new DOMDocument();
$foo = $dom->createElement("foo");
$foo->setAttribute("test", "value");
$attribute = $foo->attributes[0];

echo $dom->saveXML($foo), "\n";

var_dump($foo->compareDocumentPosition($attribute) === (DOMNode::DOCUMENT_POSITION_FOLLOWING | DOMNode::DOCUMENT_POSITION_CONTAINED_BY));
var_dump($attribute->compareDocumentPosition($foo) === (DOMNode::DOCUMENT_POSITION_PRECEDING | DOMNode::DOCUMENT_POSITION_CONTAINS));

"#);
    assert_exact(&out, r#"<foo test="value"/>
bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/compareDocumentPosition/contains_attribute_longer_descendent.phpt`: compareDocumentPosition: contains attribute as a descendent in a longer path
#[test]
#[ignore = r#"DOM port pending: Undefined class constant: DOMNode::DOCUMENT_POSITION_FOLLOWING"#]
fn test_contains_attribute_longer_descendent() {
    if skip_without_xml_native("test_contains_attribute_longer_descendent") {
        return;
    }
    let out = run_case(r#"compareDocumentPosition"#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<container>
    <div>
        <p align="center"/>
    </div>
</container>
XML);

$container = $dom->documentElement;
$p = $container->firstElementChild->firstElementChild;
$attribute = $p->attributes[0];

var_dump($container->compareDocumentPosition($attribute) === (DOMNode::DOCUMENT_POSITION_FOLLOWING | DOMNode::DOCUMENT_POSITION_CONTAINED_BY));
var_dump($attribute->compareDocumentPosition($container) === (DOMNode::DOCUMENT_POSITION_PRECEDING | DOMNode::DOCUMENT_POSITION_CONTAINS));

"#);
    assert_exact(&out, r#"bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/compareDocumentPosition/contains_element_direct_descendent.phpt`: compareDocumentPosition: contains nodes as a direct descendent
#[test]
#[ignore = r#"DOM port pending: Undefined class constant: DOMNode::DOCUMENT_POSITION_FOLLOWING"#]
fn test_contains_element_direct_descendent() {
    if skip_without_xml_native("test_contains_element_direct_descendent") {
        return;
    }
    let out = run_case(r#"compareDocumentPosition"#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<container>
<div/>
</container>
XML);

$container = $dom->documentElement;
$div = $container->firstChild;

var_dump($container->compareDocumentPosition($div) === (DOMNode::DOCUMENT_POSITION_FOLLOWING | DOMNode::DOCUMENT_POSITION_CONTAINED_BY));
var_dump($div->compareDocumentPosition($container) === (DOMNode::DOCUMENT_POSITION_PRECEDING | DOMNode::DOCUMENT_POSITION_CONTAINS));

"#);
    assert_exact(&out, r#"bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/compareDocumentPosition/contains_element_longer_descendent.phpt`: compareDocumentPosition: contains nodes as a descendent in a longer path
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_contains_element_longer_descendent() {
    if skip_without_xml_native("test_contains_element_longer_descendent") {
        return;
    }
    let out = run_case(r#"compareDocumentPosition"#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<container>
    <a>
        <b/>
        <c>
            <d/>
        </c>
    </a>
</container>
XML);

$xpath = new DOMXPath($dom);
$container = $xpath->query("//container")->item(0);

foreach (["a", "b", "c", "d"] as $tag) {
    echo "--- Test $tag ---\n";
    $element = $xpath->query("//$tag")->item(0);
    var_dump($container->compareDocumentPosition($element) === (DOMNode::DOCUMENT_POSITION_FOLLOWING | DOMNode::DOCUMENT_POSITION_CONTAINED_BY));
    var_dump($element->compareDocumentPosition($container) === (DOMNode::DOCUMENT_POSITION_PRECEDING | DOMNode::DOCUMENT_POSITION_CONTAINS));    
}

"#);
    assert_exact(&out, r#"--- Test a ---
bool(true)
bool(true)
--- Test b ---
bool(true)
bool(true)
--- Test c ---
bool(true)
bool(true)
--- Test d ---
bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/compareDocumentPosition/disconnected.phpt`: compareDocumentPosition: disconnected
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::compareDocumentPosition"#]
fn test_disconnected() {
    if skip_without_xml_native("test_disconnected") {
        return;
    }
    let out = run_case(r#"compareDocumentPosition"#, &[], r#"<?php

function check($element1, $element2) {
    echo "Disconnect and implementation flag (1->2): ";
    var_dump(($element1->compareDocumentPosition($element2) & (DOMNode::DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC | DOMNode::DOCUMENT_POSITION_DISCONNECTED)) === (DOMNode::DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC | DOMNode::DOCUMENT_POSITION_DISCONNECTED));
    echo "Disconnect and implementation flag (2->1): ";
    var_dump(($element2->compareDocumentPosition($element1) & (DOMNode::DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC | DOMNode::DOCUMENT_POSITION_DISCONNECTED)) === (DOMNode::DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC | DOMNode::DOCUMENT_POSITION_DISCONNECTED));
    
    // Must be the opposite result
    echo "Opposite result: ";
    if ($element1->compareDocumentPosition($element2) & DOMNode::DOCUMENT_POSITION_FOLLOWING) {
        var_dump(($element2->compareDocumentPosition($element1) & DOMNode::DOCUMENT_POSITION_PRECEDING) === DOMNode::DOCUMENT_POSITION_PRECEDING);
    } else {
        var_dump(($element2->compareDocumentPosition($element1) & DOMNode::DOCUMENT_POSITION_FOLLOWING) === DOMNode::DOCUMENT_POSITION_FOLLOWING);
    }
}

$dom1 = new DOMDocument();
$dom1->loadXML('<?xml version="1.0"?><container/>');
$dom2 = new DOMDocument();
$dom2->loadXML('<?xml version="1.0"?><container/>');

echo "--- Two documents ---\n";
check($dom1, $dom2);
echo "--- Two document roots ---\n";
check($dom1->documentElement, $dom2->documentElement);
echo "--- Fragment ---\n";
$fragment = $dom1->createDocumentFragment();
$foo = $fragment->appendChild(new DOMText("foo"));
check($foo, $dom1);
echo "--- Unattached element ---\n";
check(new DOMElement("foo"), new DOMElement("bar"));
echo "--- Unattached attribute ---\n";
check(new DOMAttr("foo"), new DOMAttr("bar"));
echo "--- Unattached attribute & element ---\n";
check(new DOMAttr("foo"), new DOMElement("bar"));

"#);
    assert_exact(&out, r#"--- Two documents ---
Disconnect and implementation flag (1->2): bool(true)
Disconnect and implementation flag (2->1): bool(true)
Opposite result: bool(true)
--- Two document roots ---
Disconnect and implementation flag (1->2): bool(true)
Disconnect and implementation flag (2->1): bool(true)
Opposite result: bool(true)
--- Fragment ---
Disconnect and implementation flag (1->2): bool(true)
Disconnect and implementation flag (2->1): bool(true)
Opposite result: bool(true)
--- Unattached element ---
Disconnect and implementation flag (1->2): bool(true)
Disconnect and implementation flag (2->1): bool(true)
Opposite result: bool(true)
--- Unattached attribute ---
Disconnect and implementation flag (1->2): bool(true)
Disconnect and implementation flag (2->1): bool(true)
Opposite result: bool(true)
--- Unattached attribute & element ---
Disconnect and implementation flag (1->2): bool(true)
Disconnect and implementation flag (2->1): bool(true)
Opposite result: bool(true)"#);
}

/// php-src `ext/dom/tests/compareDocumentPosition/element_order_different_depth.phpt`: compareDocumentPosition: element order at a different tree depth
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_element_order_different_depth() {
    if skip_without_xml_native("test_element_order_different_depth") {
        return;
    }
    let out = run_case(r#"compareDocumentPosition"#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<container>
    <div>
        <p>foo</p>
    </div>
    <div>
        <div>
            <p>bar</p>
        </div>
    </div>
</container>
XML);

$xpath = new DOMXPath($dom);
$query = $xpath->query("//p");
$foo = $query->item(0);
$bar = $query->item(1);

for ($i = 0; $i < 2; $i++) {
    echo "--- Check on depth $i ---\n";
    var_dump($foo->compareDocumentPosition($bar) === DOMNode::DOCUMENT_POSITION_FOLLOWING);
    var_dump($bar->compareDocumentPosition($foo) === DOMNode::DOCUMENT_POSITION_PRECEDING);
    $foo = $foo->parentElement;
    $bar = $bar->parentElement;
}

echo "--- One contains the other at depth 2 ---\n";
var_dump(boolval($foo->compareDocumentPosition($bar) & DOMNode::DOCUMENT_POSITION_CONTAINED_BY));
var_dump(boolval($bar->compareDocumentPosition($foo) & DOMNode::DOCUMENT_POSITION_CONTAINS));

"#);
    assert_exact(&out, r#"--- Check on depth 0 ---
bool(true)
bool(true)
--- Check on depth 1 ---
bool(true)
bool(true)
--- One contains the other at depth 2 ---
bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/compareDocumentPosition/element_order_direct_root_children.phpt`: compareDocumentPosition: direct root children
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMNode::compareDocumentPosition"#]
fn test_element_order_direct_root_children() {
    if skip_without_xml_native("test_element_order_direct_root_children") {
        return;
    }
    let out = run_case(r#"compareDocumentPosition"#, &[], r#"<?php

$dom = new DOMDocument();
$foo = $dom->appendChild($dom->createElement("foo"));
$bar = $dom->appendChild($dom->createElement("bar"));

var_dump($foo->compareDocumentPosition($bar) === DOMNode::DOCUMENT_POSITION_FOLLOWING);
var_dump($bar->compareDocumentPosition($foo) === DOMNode::DOCUMENT_POSITION_PRECEDING);

"#);
    assert_exact(&out, r#"bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/compareDocumentPosition/element_order_same_depth.phpt`: compareDocumentPosition: element order at the same tree depth
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_element_order_same_depth() {
    if skip_without_xml_native("test_element_order_same_depth") {
        return;
    }
    let out = run_case(r#"compareDocumentPosition"#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<container>
    <div>
        <div>
            <strong/>
            <p>foo</p>
        </div>
    </div>
    <div>
        <div>
            <p>bar</p>
            <strong/>
        </div>
    </div>
</container>
XML);

$xpath = new DOMXPath($dom);
$query = $xpath->query("//p");
$foo = $query->item(0);
$bar = $query->item(1);

echo "--- strong & foo ---\n";
var_dump($foo->previousElementSibling->compareDocumentPosition($foo) === DOMNode::DOCUMENT_POSITION_FOLLOWING);
var_dump($foo->compareDocumentPosition($foo->previousElementSibling) === DOMNode::DOCUMENT_POSITION_PRECEDING);

echo "--- strong & bar ---\n";
var_dump($bar->nextElementSibling->compareDocumentPosition($bar) === DOMNode::DOCUMENT_POSITION_PRECEDING);
var_dump($bar->compareDocumentPosition($bar->nextElementSibling) === DOMNode::DOCUMENT_POSITION_FOLLOWING);

echo "--- strong & strong ---\n";
var_dump($foo->previousElementSibling->compareDocumentPosition($bar->nextElementSibling) === DOMNode::DOCUMENT_POSITION_FOLLOWING);
var_dump($bar->nextElementSibling->compareDocumentPosition($foo->previousElementSibling) === DOMNode::DOCUMENT_POSITION_PRECEDING);

for ($i = 0; $i < 3; $i++) {
    echo "--- Check on depth $i ---\n";
    var_dump($foo->compareDocumentPosition($bar) === DOMNode::DOCUMENT_POSITION_FOLLOWING);
    var_dump($bar->compareDocumentPosition($foo) === DOMNode::DOCUMENT_POSITION_PRECEDING);
    $foo = $foo->parentElement;
    $bar = $bar->parentElement;
}

"#);
    assert_exact(&out, r#"--- strong & foo ---
bool(true)
bool(true)
--- strong & bar ---
bool(true)
bool(true)
--- strong & strong ---
bool(true)
bool(true)
--- Check on depth 0 ---
bool(true)
bool(true)
--- Check on depth 1 ---
bool(true)
bool(true)
--- Check on depth 2 ---
bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/compareDocumentPosition/entity.phpt`: compareDocumentPosition: entity ordering
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_entity() {
    if skip_without_xml_native("test_entity") {
        return;
    }
    let out = run_case(r#"compareDocumentPosition"#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<!DOCTYPE root [
<!ENTITY e1 "e1">
<!ENTITY e2 "e2">
]>
<container>
    <child>&e1;</child>
</container>
XML, LIBXML_NOENT);

$entities = iterator_to_array($dom->doctype->entities);
usort($entities, fn ($a, $b) => strcmp($a->nodeName, $b->nodeName));

echo "--- Compare entities ---\n";

var_dump($entities[0]->compareDocumentPosition($entities[1]) === DOMNode::DOCUMENT_POSITION_FOLLOWING);
var_dump($entities[1]->compareDocumentPosition($entities[0]) === DOMNode::DOCUMENT_POSITION_PRECEDING);

$xpath = new DOMXPath($dom);
$child = $xpath->query('//child')->item(0);

echo "--- Compare entities against first child ---\n";

var_dump($entities[0]->compareDocumentPosition($child->firstChild) === DOMNode::DOCUMENT_POSITION_FOLLOWING);
var_dump($entities[1]->compareDocumentPosition($child->firstChild) === DOMNode::DOCUMENT_POSITION_FOLLOWING);

echo "--- Compare first child against entities ---\n";

var_dump($child->firstChild->compareDocumentPosition($entities[0]) === DOMNode::DOCUMENT_POSITION_PRECEDING);
var_dump($child->firstChild->compareDocumentPosition($entities[1]) === DOMNode::DOCUMENT_POSITION_PRECEDING);

?>
"#);
    assert_exact(&out, r#"--- Compare entities ---
bool(true)
bool(true)
--- Compare entities against first child ---
bool(true)
bool(true)
--- Compare first child against entities ---
bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/compareDocumentPosition/equal.phpt`: compareDocumentPosition: equal nodes
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_equal() {
    if skip_without_xml_native("test_equal") {
        return;
    }
    let out = run_case(r#"compareDocumentPosition"#, &[], r#"<?php

$dom = new DOMDocument();
$dom->loadXML(<<<XML
<?xml version="1.0"?>
<container>
<p>foo</p>
<p>foo</p>
</container>
XML);

$xpath = new DOMXPath($dom);
$nodes = $xpath->query('//p');
var_dump($nodes->item(0)->compareDocumentPosition($nodes->item(0)) === 0);
var_dump($nodes->item(1)->compareDocumentPosition($nodes->item(1)) === 0);
var_dump($nodes->item(0)->compareDocumentPosition($nodes->item(1)) === 0);
var_dump($nodes->item(1)->compareDocumentPosition($nodes->item(0)) === 0);

"#);
    assert_exact(&out, r#"bool(true)
bool(true)
bool(false)
bool(false)"#);
}
