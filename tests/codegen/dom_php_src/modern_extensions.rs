//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `modern/extensions` folder, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/modern/extensions/Element_getDescendantNamespaces.phpt`: DOM\Element::getDescendantNamespaces()
#[test]
#[ignore = r#"DOM port pending: Cannot index non-array"#]
fn test_element_get_descendant_namespaces() {
    if skip_without_xml_native("test_element_get_descendant_namespaces") {
        return;
    }
    let out = run_case(r#"modern/extensions"#, &[], r#"<?php

function dump($dom, $name) {
    echo "\n=== $name ===\n";
    $list = $dom->getElementsByTagName($name)[0]->getDescendantNamespaces();
    foreach ($list as $entry) {
        echo "prefix: ";
        var_dump($entry->prefix);
        echo "namespaceURI: ";
        var_dump($entry->namespaceURI);
        echo "element->nodeName: ";
        var_dump($entry->element->nodeName);
        echo "---\n";
    }
}

$dom = DOM\XMLDocument::createFromString(<<<XML
<root xmlns="urn:a">
    <child xmlns="">
        <c:child xmlns:c="urn:c"/>
    </child>
    <b:sibling xmlns:b="urn:b" xmlns:d="urn:d" d:foo="bar">
        <d:child xmlns:d="urn:d2"/>
    </b:sibling>
</root>
XML);

dump($dom, 'c:child');
dump($dom, 'child');
dump($dom, 'b:sibling');
dump($dom, 'd:child');
dump($dom, 'root');

"#);
    assert_exact(&out, r#"=== c:child ===
prefix: string(1) "c"
namespaceURI: string(5) "urn:c"
element->nodeName: string(7) "c:child"
---

=== child ===
prefix: string(1) "c"
namespaceURI: string(5) "urn:c"
element->nodeName: string(7) "c:child"
---

=== b:sibling ===
prefix: NULL
namespaceURI: string(5) "urn:a"
element->nodeName: string(9) "b:sibling"
---
prefix: string(1) "b"
namespaceURI: string(5) "urn:b"
element->nodeName: string(9) "b:sibling"
---
prefix: string(1) "d"
namespaceURI: string(5) "urn:d"
element->nodeName: string(9) "b:sibling"
---
prefix: NULL
namespaceURI: string(5) "urn:a"
element->nodeName: string(7) "d:child"
---
prefix: string(1) "b"
namespaceURI: string(5) "urn:b"
element->nodeName: string(7) "d:child"
---
prefix: string(1) "d"
namespaceURI: string(6) "urn:d2"
element->nodeName: string(7) "d:child"
---

=== d:child ===
prefix: NULL
namespaceURI: string(5) "urn:a"
element->nodeName: string(7) "d:child"
---
prefix: string(1) "b"
namespaceURI: string(5) "urn:b"
element->nodeName: string(7) "d:child"
---
prefix: string(1) "d"
namespaceURI: string(6) "urn:d2"
element->nodeName: string(7) "d:child"
---

=== root ===
prefix: NULL
namespaceURI: string(5) "urn:a"
element->nodeName: string(4) "root"
---
prefix: string(1) "c"
namespaceURI: string(5) "urn:c"
element->nodeName: string(7) "c:child"
---
prefix: NULL
namespaceURI: string(5) "urn:a"
element->nodeName: string(9) "b:sibling"
---
prefix: string(1) "b"
namespaceURI: string(5) "urn:b"
element->nodeName: string(9) "b:sibling"
---
prefix: string(1) "d"
namespaceURI: string(5) "urn:d"
element->nodeName: string(9) "b:sibling"
---
prefix: NULL
namespaceURI: string(5) "urn:a"
element->nodeName: string(7) "d:child"
---
prefix: string(1) "b"
namespaceURI: string(5) "urn:b"
element->nodeName: string(7) "d:child"
---
prefix: string(1) "d"
namespaceURI: string(6) "urn:d2"
element->nodeName: string(7) "d:child"
---"#);
}

/// php-src `ext/dom/tests/modern/extensions/Element_getInScopeNamespaces.phpt`: Dom\Element::getInScopeNamespaces()
#[test]
#[ignore = r#"DOM port pending: Cannot index non-array"#]
fn test_element_get_in_scope_namespaces() {
    if skip_without_xml_native("test_element_get_in_scope_namespaces") {
        return;
    }
    let out = run_case(r#"modern/extensions"#, &[], r#"<?php

function dump($dom, $name) {
    echo "\n=== $name ===\n";
    $list = $dom->getElementsByTagName($name)[0]->getInScopeNamespaces();
    foreach ($list as $entry) {
        echo "prefix: ";
        var_dump($entry->prefix);
        echo "namespaceURI: ";
        var_dump($entry->namespaceURI);
        echo "element->nodeName: ";
        var_dump($entry->element->nodeName);
        echo "---\n";
    }
}

$dom = Dom\XMLDocument::createFromString(<<<XML
<root xmlns="urn:a">
    <child xmlns="">
        <c:child xmlns:c="urn:c"/>
    </child>
    <b:sibling xmlns:b="urn:b" xmlns:d="urn:d" d:foo="bar">
        <d:child xmlns:d="urn:d2"/>
    </b:sibling>
</root>
XML);

dump($dom, 'c:child');
dump($dom, 'child');
dump($dom, 'b:sibling');
dump($dom, 'd:child');
dump($dom, 'root');

"#);
    assert_exact(&out, r#"=== c:child ===
prefix: string(1) "c"
namespaceURI: string(5) "urn:c"
element->nodeName: string(7) "c:child"
---

=== child ===

=== b:sibling ===
prefix: NULL
namespaceURI: string(5) "urn:a"
element->nodeName: string(9) "b:sibling"
---
prefix: string(1) "b"
namespaceURI: string(5) "urn:b"
element->nodeName: string(9) "b:sibling"
---
prefix: string(1) "d"
namespaceURI: string(5) "urn:d"
element->nodeName: string(9) "b:sibling"
---

=== d:child ===
prefix: NULL
namespaceURI: string(5) "urn:a"
element->nodeName: string(7) "d:child"
---
prefix: string(1) "b"
namespaceURI: string(5) "urn:b"
element->nodeName: string(7) "d:child"
---
prefix: string(1) "d"
namespaceURI: string(6) "urn:d2"
element->nodeName: string(7) "d:child"
---

=== root ===
prefix: NULL
namespaceURI: string(5) "urn:a"
element->nodeName: string(4) "root"
---"#);
}

/// php-src `ext/dom/tests/modern/extensions/Element_renaming_html_ns_01.phpt`: Element renaming interaction with the HTML namespace 01
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_element_renaming_html_ns_01() {
    if skip_without_xml_native("test_element_renaming_html_ns_01") {
        return;
    }
    let out = run_case(r#"modern/extensions"#, &[], r#"<?php

$dom = DOM\XMLDocument::createEmpty();
$el = $dom->createElementNS("http://www.w3.org/1999/xhtml", "foo:bar");
$el->rename("http://www.w3.org/1999/xhtml", "foo:baz");
var_dump($el->nodeName, $el->namespaceURI, $el->prefix);

// Very subtly *not* the HTML namespace!
$el = $dom->createElementNS("http://www.w3.org/1999/xhtml/", "foo:bar");
$el->rename("urn:x", "foo:baz");
var_dump($el->nodeName, $el->namespaceURI, $el->prefix);

"#);
    assert_exact(&out, r#"string(7) "foo:baz"
string(28) "http://www.w3.org/1999/xhtml"
string(3) "foo"
string(7) "foo:baz"
string(5) "urn:x"
string(3) "foo""#);
}

/// php-src `ext/dom/tests/modern/extensions/Element_renaming_html_ns_02.phpt`: Element renaming interaction with the HTML namespace 02
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_element_renaming_html_ns_02() {
    if skip_without_xml_native("test_element_renaming_html_ns_02") {
        return;
    }
    let out = run_case(r#"modern/extensions"#, &[], r#"<?php

$dom = DOM\XMLDocument::createEmpty();
$el = $dom->createElementNS("http://www.w3.org/1999/xhtml", "foo:bar");
try {
    $el->rename("urn:a", "foo:baz");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
$el = $dom->createElementNS("urn:a", "foo:bar");
try {
    $el->rename("http://www.w3.org/1999/xhtml", "foo:baz");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"It is not possible to move an element out of the HTML namespace because the HTML namespace is tied to the HTMLElement class
It is not possible to move an element into the HTML namespace because the HTML namespace is tied to the HTMLElement class"#);
}

/// php-src `ext/dom/tests/modern/extensions/Element_substitutedNodeValue.phpt`: Element::$substitutedNodeValue
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_element_substituted_node_value() {
    if skip_without_xml_native("test_element_substituted_node_value") {
        return;
    }
    let out = run_case(r#"modern/extensions"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root/>');

$dom->documentElement->substitutedNodeValue = "&#x31;";
var_dump($dom->documentElement->substitutedNodeValue);
var_dump($dom->documentElement->nodeValue); // Should always be NULL for elements
echo $dom->saveXML(), "\n";

$dom->documentElement->substitutedNodeValue = "&lt;&gt;";
var_dump($dom->documentElement->substitutedNodeValue);
var_dump($dom->documentElement->nodeValue); // Should always be NULL for elements
echo $dom->saveXML(), "\n";

$dom->documentElement->substitutedNodeValue = "";
var_dump($dom->documentElement->substitutedNodeValue);
var_dump($dom->documentElement->nodeValue); // Should always be NULL for elements
echo $dom->saveXML(), "\n";

"#);
    assert_format(&out, r#"string(1) "1"
NULL
<?xml version="1.0" encoding="UTF-8"?>
<root>1</root>
string(2) "<>"
NULL
<?xml version="1.0" encoding="UTF-8"?>
<root>&lt;&gt;</root>
string(0) ""
NULL
<?xml version="1.0" encoding="UTF-8"?>
%r(<root><\/root>|<root\/>)%r"#);
}

/// php-src `ext/dom/tests/modern/extensions/attribute_renaming_conflict.phpt`: Renaming an attribute node to a name that already exists
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_attribute_renaming_conflict() {
    if skip_without_xml_native("test_attribute_renaming_conflict") {
        return;
    }
    let out = run_case(r#"modern/extensions"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString(<<<XML
<!DOCTYPE root [
    <!ELEMENT implied-attribute ANY>
    <!ATTLIST implied-attribute hello CDATA #FIXED "world">
]>
<root a="b" c="d" xmlns:ns1="urn:a" ns1:foo="bar">
    <implied-attribute my-attr="x"/>
    <implied-attribute my-attr="x"/>
</root>
XML, LIBXML_DTDATTR);

$root = $dom->documentElement;
try {
    $root->attributes[0]->rename(NULL, 'c');
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    $root->attributes[0]->rename(NULL, 'c');
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    $root->attributes[1]->rename(NULL, 'a');
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    $root->attributes[1]->rename('urn:a', 'foo');
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    $root->attributes[3]->rename('', 'a');
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    $root->firstElementChild->attributes[0]->rename(NULL, 'hello');
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    $root->firstElementChild->attributes[1]->rename(NULL, 'my-attr');
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

// This is here to validate that nothing actually changed
echo $dom->saveXML();

?>
"#);
    assert_exact(&out, r#"An attribute with the given name in the given namespace already exists
An attribute with the given name in the given namespace already exists
An attribute with the given name in the given namespace already exists
An attribute with the given name in the given namespace already exists
An attribute with the given name in the given namespace already exists
An attribute with the given name in the given namespace already exists
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE root [
<!ELEMENT implied-attribute ANY>
<!ATTLIST implied-attribute hello CDATA #FIXED "world">
]>
<root xmlns:ns1="urn:a" a="b" c="d" ns1:foo="bar">
    <implied-attribute my-attr="x" hello="world"/>
    <implied-attribute my-attr="x" hello="world"/>
</root>"#);
}

/// php-src `ext/dom/tests/modern/extensions/node_renaming.phpt`: Node renaming
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_node_renaming() {
    if skip_without_xml_native("test_node_renaming") {
        return;
    }
    let out = run_case(r#"modern/extensions"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString(<<<XML
<root xmlns:a="urn:a">
    <a:child attrib="value"/>
</root>
XML);

function test($target, $namespaceURI, $qualifiedName) {
    $namespaceURIPretty = json_encode($namespaceURI);
    $qualifiedNamePretty = json_encode($qualifiedName);
    echo "--- rename to $namespaceURIPretty $qualifiedNamePretty ---\n";
    $target->rename($namespaceURI, $qualifiedName);
    echo $target->ownerDocument->saveXML(), "\n";
    var_dump($target->namespaceURI, $target->prefix);
}

echo "=== Element test ===\n";

test($dom->documentElement, "urn:x", "x:foo");
test($dom->documentElement, "urn:x", "a:foo");
test($dom->documentElement, "", "foo");
test($dom->documentElement, null, "bar");

echo "=== Attribute test ===\n";

$attribute = $dom->documentElement->firstElementChild->attributes[0];

test($attribute, "urn:x", "x:foo");
test($attribute, "urn:x", "a:foo");
test($attribute, "", "foo");
test($attribute, null, "bar");

"#);
    assert_exact(&out, r#"=== Element test ===
--- rename to "urn:x" "x:foo" ---
<?xml version="1.0" encoding="UTF-8"?>
<x:foo xmlns:x="urn:x" xmlns:a="urn:a">
    <a:child attrib="value"/>
</x:foo>
string(5) "urn:x"
string(1) "x"
--- rename to "urn:x" "a:foo" ---
<?xml version="1.0" encoding="UTF-8"?>
<ns1:foo xmlns:ns1="urn:x" xmlns:a="urn:a">
    <a:child attrib="value"/>
</ns1:foo>
string(5) "urn:x"
string(1) "a"
--- rename to "" "foo" ---
<?xml version="1.0" encoding="UTF-8"?>
<foo xmlns:a="urn:a">
    <a:child attrib="value"/>
</foo>
NULL
NULL
--- rename to null "bar" ---
<?xml version="1.0" encoding="UTF-8"?>
<bar xmlns:a="urn:a">
    <a:child attrib="value"/>
</bar>
NULL
NULL
=== Attribute test ===
--- rename to "urn:x" "x:foo" ---
<?xml version="1.0" encoding="UTF-8"?>
<bar xmlns:a="urn:a">
    <a:child xmlns:x="urn:x" x:foo="value"/>
</bar>
string(5) "urn:x"
string(1) "x"
--- rename to "urn:x" "a:foo" ---
<?xml version="1.0" encoding="UTF-8"?>
<bar xmlns:a="urn:a">
    <a:child xmlns:a="urn:x" a:foo="value"/>
</bar>
string(5) "urn:x"
string(1) "a"
--- rename to "" "foo" ---
<?xml version="1.0" encoding="UTF-8"?>
<bar xmlns:a="urn:a">
    <a:child foo="value"/>
</bar>
NULL
NULL
--- rename to null "bar" ---
<?xml version="1.0" encoding="UTF-8"?>
<bar xmlns:a="urn:a">
    <a:child bar="value"/>
</bar>
NULL
NULL"#);
}

/// php-src `ext/dom/tests/modern/extensions/node_renaming_validation_errors.phpt`: Node renaming validation errors
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_node_renaming_validation_errors() {
    if skip_without_xml_native("test_node_renaming_validation_errors") {
        return;
    }
    let out = run_case(r#"modern/extensions"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root/>');

try {
    $dom->documentElement->rename('', 'a:b');
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

try {
    $dom->documentElement->rename('urn:a', 'a:b:c');
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo $dom->saveXML();

"#);
    assert_exact(&out, r#"Namespace Error
Invalid Character Error
<?xml version="1.0" encoding="UTF-8"?>
<root/>"#);
}
