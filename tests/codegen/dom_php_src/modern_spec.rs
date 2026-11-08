//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `modern/spec` folder, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/modern/spec/Attr_name.phpt`: DOMAttr::$name
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_attr_name() {
    if skip_without_xml_native("test_attr_name") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$foo = $dom->appendChild($dom->createElement("foo"));
$foo->setAttributeNS("http://example.com", "foo:bar", "baz");
var_dump($foo->attributes[0]->name);

"#);
    assert_exact(&out, r#"string(7) "foo:bar""#);
}

/// php-src `ext/dom/tests/modern/spec/CharacterData_deleteData_negative_in_bounds_length.phpt`: deleteData() negative in bounds length
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_character_data_delete_data_negative_in_bounds_length() {
    if skip_without_xml_native("test_character_data_delete_data_negative_in_bounds_length") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

echo "--- Modern behaviour ---\n";

$dom = Dom\HTMLDocument::createEmpty();
$comment = $dom->createComment("foobarbaz");
$comment->deleteData(3, -1);
echo $dom->saveHtml($comment), "\n";

echo "--- Legacy behaviour ---\n";

$dom = new DOMDocument;
$comment = $dom->createComment("foobarbaz");
try {
    $comment->deleteData(3, -1);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveHtml($comment), "\n";

"#);
    assert_exact(&out, r#"--- Modern behaviour ---
<!--foo-->
--- Legacy behaviour ---
Index Size Error
<!--foobarbaz-->"#);
}

/// php-src `ext/dom/tests/modern/spec/CharacterData_insertData_edge_cases.phpt`: insertData() edge cases
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_character_data_insert_data_edge_cases() {
    if skip_without_xml_native("test_character_data_insert_data_edge_cases") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$comment = $dom->createComment("foobarbaz");
try {
    var_dump($comment->insertData(100, "data"));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    var_dump($comment->insertData(2**31+1, "data"));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveHtml($comment), "\n";

"#);
    assert_exact(&out, r#"Index Size Error
Index Size Error
<!--foobarbaz-->"#);
}

/// php-src `ext/dom/tests/modern/spec/CharacterData_insertData_negative_offset.phpt`: insertData() negative offset
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_character_data_insert_data_negative_offset() {
    if skip_without_xml_native("test_character_data_insert_data_negative_offset") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$comment = $dom->createComment("foobarbaz");
try {
    $comment->insertData(-1, "A");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveHtml($comment), "\n";
$comment->insertData(1, "A");
echo $dom->saveHtml($comment), "\n";

"#);
    assert_exact(&out, r#"Index Size Error
<!--foobarbaz-->
<!--fAoobarbaz-->"#);
}

/// php-src `ext/dom/tests/modern/spec/CharacterData_insertData_negative_offset_mod32.phpt`: insertData() negative offset (mod 32)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_character_data_insert_data_negative_offset_mod32() {
    if skip_without_xml_native("test_character_data_insert_data_negative_offset_mod32") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

echo "--- Modern behaviour ---\n";

$dom = Dom\HTMLDocument::createEmpty();
$comment = $dom->createComment("foobarbaz");
try {
    $comment->insertData(-1, "A");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveHtml($comment), "\n";
$comment->insertData(-(2**32 - 1), "A");
echo $dom->saveHtml($comment), "\n";

echo "--- Legacy behaviour ---\n";

$dom = new DOMDocument;
$comment = $dom->createComment("foobarbaz");
try {
    $comment->insertData(-1, "A");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveHtml($comment), "\n";
try {
    $comment->insertData(-(2**32 - 1), "A");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveHtml($comment), "\n";

"#);
    assert_exact(&out, r#"--- Modern behaviour ---
Index Size Error
<!--foobarbaz-->
<!--fAoobarbaz-->
--- Legacy behaviour ---
Index Size Error
<!--foobarbaz-->
Index Size Error
<!--foobarbaz-->"#);
}

/// php-src `ext/dom/tests/modern/spec/CharacterData_replaceData_negative_count.phpt`: replaceData() negative count
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_character_data_replace_data_negative_count() {
    if skip_without_xml_native("test_character_data_replace_data_negative_count") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$comment = $dom->createComment("foobarbaz");
$comment->replaceData(0, -1, "A");
echo $dom->saveHtml($comment), "\n";
$comment = $dom->createComment("foobarbaz");
$comment->replaceData(2, -2, "A");
echo $dom->saveHtml($comment), "\n";

"#);
    assert_exact(&out, r#"<!--A-->
<!--foA-->"#);
}

/// php-src `ext/dom/tests/modern/spec/CharacterData_replaceData_negative_count_mod32.phpt`: replaceData() negative count (mod 32)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_character_data_replace_data_negative_count_mod32() {
    if skip_without_xml_native("test_character_data_replace_data_negative_count_mod32") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

echo "--- Modern behaviour ---\n";

$dom = Dom\HTMLDocument::createEmpty();
$comment = $dom->createComment("foobarbaz");
$comment->replaceData(0, -1, "A");
echo $dom->saveHtml($comment), "\n";
$comment = $dom->createComment("foobarbaz");
$comment->replaceData(2, -(2**32 - 2), "A");
echo $dom->saveHtml($comment), "\n";

echo "--- Legacy behaviour ---\n";

$dom = new DOMDocument;
$comment = $dom->createComment("foobarbaz");
try {
    $comment->replaceData(0, -1, "A");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveHtml($comment), "\n";
try {
    $comment->replaceData(2, -(2**32 - 2), "A");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveHtml($comment), "\n";

"#);
    assert_exact(&out, r#"--- Modern behaviour ---
<!--A-->
<!--foAarbaz-->
--- Legacy behaviour ---
Index Size Error
<!--foobarbaz-->
Index Size Error
<!--foobarbaz-->"#);
}

/// php-src `ext/dom/tests/modern/spec/CharacterData_substringData_edge_cases.phpt`: substringData() edge cases
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_character_data_substring_data_edge_cases() {
    if skip_without_xml_native("test_character_data_substring_data_edge_cases") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$comment = $dom->createComment("foobarbaz");
try {
    var_dump($comment->substringData(0, 2**31+1));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    var_dump($comment->substringData(2**31+1, 0));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    var_dump($comment->substringData(100, 0));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveHtml($comment), "\n";

"#);
    assert_exact(&out, r#"Index Size Error
Index Size Error
Index Size Error
<!--foobarbaz-->"#);
}

/// php-src `ext/dom/tests/modern/spec/CharacterData_substringData_negative_arguments.phpt`: substringData() negative arguments
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_character_data_substring_data_negative_arguments() {
    if skip_without_xml_native("test_character_data_substring_data_negative_arguments") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$comment = $dom->createComment("foobarbaz");
var_dump($comment->substringData(0, -1));
echo $dom->saveHtml($comment), "\n";
var_dump($comment->substringData(2, -2));
echo $dom->saveHtml($comment), "\n";
try {
    var_dump($comment->substringData(-2, 2));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveHtml($comment), "\n";

"#);
    assert_exact(&out, r#"string(9) "foobarbaz"
<!--foobarbaz-->
string(7) "obarbaz"
<!--foobarbaz-->
Index Size Error
<!--foobarbaz-->"#);
}

/// php-src `ext/dom/tests/modern/spec/CharacterData_substringData_negative_arguments_mod32.phpt`: substringData() negative arguments (mod 32)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_character_data_substring_data_negative_arguments_mod32() {
    if skip_without_xml_native("test_character_data_substring_data_negative_arguments_mod32") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

echo "--- Modern behaviour ---\n";

$dom = Dom\HTMLDocument::createEmpty();
$comment = $dom->createComment("foobarbaz");
var_dump($comment->substringData(0, -1));
echo $dom->saveHtml($comment), "\n";
var_dump($comment->substringData(2, -(2**32 - 2)));
echo $dom->saveHtml($comment), "\n";
var_dump($comment->substringData(-(2**32 - 2), 2));
echo $dom->saveHtml($comment), "\n";

echo "--- Legacy behaviour ---\n";

$dom = new DOMDocument;
$comment = $dom->createComment("foobarbaz");
try {
    var_dump($comment->substringData(0, -1));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveHtml($comment), "\n";
try {
    var_dump($comment->substringData(2, -(2**32 - 2)));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveHtml($comment), "\n";
try {
    var_dump($comment->substringData(-(2**32 - 2), 2));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveHtml($comment), "\n";

"#);
    assert_exact(&out, r#"--- Modern behaviour ---
string(9) "foobarbaz"
<!--foobarbaz-->
string(2) "ob"
<!--foobarbaz-->
string(2) "ob"
<!--foobarbaz-->
--- Legacy behaviour ---
Index Size Error
<!--foobarbaz-->
Index Size Error
<!--foobarbaz-->
Index Size Error
<!--foobarbaz-->"#);
}

/// php-src `ext/dom/tests/modern/spec/Document_createAttribute.phpt`: Document::createAttribute()
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\Element"#]
fn test_document_create_attribute() {
    if skip_without_xml_native("test_document_create_attribute") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php
require __DIR__ . "/dump_attr.inc";

echo "--- HTML document ---\n";

$dom = Dom\HTMLDocument::createEmpty();
$attr = $dom->createAttribute('foo');
dumpAttr($attr);
$attr = $dom->createAttribute('FoOo');
dumpAttr($attr);

echo "--- XML document ---\n";

$dom = Dom\XMLDocument::createEmpty();
$attr = $dom->createAttribute('foo');
dumpAttr($attr);
$attr = $dom->createAttribute('FoOo');
dumpAttr($attr);
"#);
    assert_exact(&out, r#"--- HTML document ---
Attr: foo
NULL
string(3) "foo"
NULL
Attr: fooo
NULL
string(4) "fooo"
NULL
--- XML document ---
Attr: foo
NULL
string(3) "foo"
NULL
Attr: FoOo
NULL
string(4) "FoOo"
NULL"#);
}

/// php-src `ext/dom/tests/modern/spec/Document_createAttributeNS.phpt`: Document::createAttributeNS()
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\Element"#]
fn test_document_create_attribute_ns() {
    if skip_without_xml_native("test_document_create_attribute_ns") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

require __DIR__ . '/dump_attr.inc';

function testErrorCase($dom, $ns, $qname) {
    try {
        $dom->createAttributeNS($ns, $qname);
    } catch (DOMException $e) {
        $ns_readable = is_null($ns) ? 'null' : "\"$ns\"";
        echo "($ns_readable, \"$qname\"): {$e->getMessage()}\n";
    }
}

$dom = Dom\HTMLDocument::createEmpty();
$attrs = [];
$attrs[] = $dom->createAttributeNS('http://www.w3.org/2000/xmlns/', 'xmlns:foo');
$attrs[] = $dom->createAttributeNS('http://www.w3.org/2000/xmlns/', 'xmlns:bar');
$attrs[] = $dom->createAttributeNS('http://www.w3.org/2000/xmlns/', 'xmlns');
$attrs[] = $dom->createAttributeNS('http://www.w3.org/XML/1998/namespace', 'xml:foo');
$attrs[] = $dom->createAttributeNS('urn:a', 'foo:bar');
$attrs[] = $dom->createAttributeNS('urn:a', 'bar:bar');
$attrs[] = $dom->createAttributeNS('http://www.w3.org/2000/xmlns', 'foo:bar');

echo "--- Error cases ---\n";

testErrorCase($dom, '', 'bar:bar');
testErrorCase($dom, null, 'bar:bar');
testErrorCase($dom, 'urn:a', '@');
testErrorCase($dom, 'urn:a', 'foo:bar:baz');
testErrorCase($dom, 'http://www.w3.org/2000/xmlns', 'xmlns');
testErrorCase($dom, 'http://www.w3.org/2000/xmlns', 'xmlns:bar');
testErrorCase($dom, 'http://www.w3.org/2000/xmlns', 'xml:foo');

echo "\n--- Normal cases ---\n";

// Test after creating to make sure they cannot corrupt each other
foreach ($attrs as $attr) {
    dumpAttr($attr);
}

// Test in document
$root = $dom->appendChild($dom->createElement('foo'));
foreach ($attrs as $attr) {
    $root->setAttributeNodeNS($attr);
}

echo $dom->saveHtml(), "\n";

echo "\n--- NULL prefix cases ---\n";

// Test multiple "null prefixes" after having a root
$attrs = [];
$attrs[] = $dom->createAttributeNS(null, 'baz1');
$attrs[] = $dom->createAttributeNS(null, 'baz2');
$attrs[] = $dom->createAttributeNS('', 'baz1');
$attrs[] = $dom->createAttributeNS('', 'baz2');
foreach ($attrs as $attr) {
    dumpAttr($attr);
    $root->setAttributeNodeNS($attr);
}

echo $dom->saveHtml(), "\n";
"#);
    assert_exact(&out, r#"--- Error cases ---
("", "bar:bar"): Namespace Error
(null, "bar:bar"): Namespace Error
("urn:a", "@"): Invalid Character Error
("urn:a", "foo:bar:baz"): Invalid Character Error
("http://www.w3.org/2000/xmlns", "xmlns"): Namespace Error
("http://www.w3.org/2000/xmlns", "xmlns:bar"): Namespace Error
("http://www.w3.org/2000/xmlns", "xml:foo"): Namespace Error

--- Normal cases ---
Attr: xmlns:foo
string(5) "xmlns"
string(9) "xmlns:foo"
string(29) "http://www.w3.org/2000/xmlns/"
Attr: xmlns:bar
string(5) "xmlns"
string(9) "xmlns:bar"
string(29) "http://www.w3.org/2000/xmlns/"
Attr: xmlns
NULL
string(5) "xmlns"
string(29) "http://www.w3.org/2000/xmlns/"
Attr: xml:foo
string(3) "xml"
string(7) "xml:foo"
string(36) "http://www.w3.org/XML/1998/namespace"
Attr: foo:bar
string(3) "foo"
string(7) "foo:bar"
string(5) "urn:a"
Attr: bar:bar
string(3) "bar"
string(7) "bar:bar"
string(5) "urn:a"
Attr: foo:bar
string(3) "foo"
string(7) "foo:bar"
string(28) "http://www.w3.org/2000/xmlns"
<foo xmlns:foo="" xmlns:bar="" xmlns="" xml:foo="" bar:bar="" foo:bar=""></foo>

--- NULL prefix cases ---
Attr: baz1
NULL
string(4) "baz1"
NULL
Attr: baz2
NULL
string(4) "baz2"
NULL
Attr: baz1
NULL
string(4) "baz1"
NULL
Attr: baz2
NULL
string(4) "baz2"
NULL
<foo xmlns:foo="" xmlns:bar="" xmlns="" xml:foo="" bar:bar="" foo:bar="" baz1="" baz2=""></foo>"#);
}

/// php-src `ext/dom/tests/modern/spec/Document_createElement.phpt`: Document::createElement()
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\Element"#]
fn test_document_create_element() {
    if skip_without_xml_native("test_document_create_element") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

require __DIR__ . "/element_dump.inc";

echo "--- Into rootless document ---\n";

$dom = Dom\HTMLDocument::createEmpty();
$element = $dom->createElement("HTML");
$element->textContent = "&hello";
dumpElement($element);

$element = $dom->createElement("HEad");
dumpElement($element);

echo "--- Into document with HTML root ---\n";

$dom = Dom\HTMLDocument::createEmpty();
$element = $dom->createElement("HTML");
$element->textContent = "&hello";
$dom->appendChild($element);
$element = $dom->createElement("HEad");
dumpElement($element);

echo "--- Into document with non-HTML root ---\n";

$dom = Dom\HTMLDocument::createEmpty();
$element = $dom->createElementNS("urn:a", "container");
$dom->appendChild($element);
$element = $dom->createElement("HEad");
dumpElement($element);

"#);
    assert_exact(&out, r#"--- Into rootless document ---
tagName: string(4) "HTML"
nodeName: string(4) "HTML"
textContent: string(6) "&hello"
prefix: NULL
namespaceURI: string(28) "http://www.w3.org/1999/xhtml"
<html>&amp;hello</html>

tagName: string(4) "HEAD"
nodeName: string(4) "HEAD"
textContent: string(0) ""
prefix: NULL
namespaceURI: string(28) "http://www.w3.org/1999/xhtml"
<head></head>

--- Into document with HTML root ---
tagName: string(4) "HEAD"
nodeName: string(4) "HEAD"
textContent: string(0) ""
prefix: NULL
namespaceURI: string(28) "http://www.w3.org/1999/xhtml"
<head></head>

--- Into document with non-HTML root ---
tagName: string(4) "HEAD"
nodeName: string(4) "HEAD"
textContent: string(0) ""
prefix: NULL
namespaceURI: string(28) "http://www.w3.org/1999/xhtml"
<head></head>"#);
}

/// php-src `ext/dom/tests/modern/spec/Document_createElementNS.phpt`: Document::createElementNS()
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\Element"#]
fn test_document_create_element_ns() {
    if skip_without_xml_native("test_document_create_element_ns") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

require __DIR__ . "/element_dump.inc";
require __DIR__ . "/create_element_util.inc";

echo "--- Normal cases ---\n";

$dom = Dom\HTMLDocument::createEmpty();
$elements = [];

$elements[] = createElementNS($dom, NULL, "qname", "&hello");
$elements[] = createElementNS($dom, "urn:foo", "foo:bar", "&hello");
$elements[] = createElementNS($dom, "urn:foo", "bar:BAR", "&hello");
$elements[] = createElementNS($dom, "urn:foo", "bar:BAR", "&hello");
$elements[] = createElementNS($dom, "http://www.w3.org/2000/xmlns/", "xmlns", "&hello");
$elements[] = createElementNS($dom, "http://www.w3.org/XML/1998/namespace", "xml", "&hello");

foreach ($elements as $element) {
    dumpElement($element);
}

echo "--- Error cases ---\n";

function testError($dom, $namespaceURI, $qualifiedName)
{
    $ns_readable = is_null($namespaceURI) ? "NULL" : "\"$namespaceURI\"";
    echo "($ns_readable, \"$qualifiedName\"): ";
    try {
        $dom->createElementNS($namespaceURI, $qualifiedName);
    } catch (DOMException $e) {
        echo $e->getMessage(), "\n";
    }
}

testError($dom, NULL, "prefix:name");
testError($dom, "", "prefix:name");
testError($dom, "urn:foo", "@");
testError($dom, "http://www.w3.org/2000/xmlns/", "svg");
testError($dom, "urn:foo", "xml:xml");

"#);
    assert_exact(&out, r#"--- Normal cases ---
tagName: string(5) "qname"
nodeName: string(5) "qname"
textContent: string(6) "&hello"
prefix: NULL
namespaceURI: NULL
<qname>&amp;hello</qname>

tagName: string(7) "foo:bar"
nodeName: string(7) "foo:bar"
textContent: string(6) "&hello"
prefix: string(3) "foo"
namespaceURI: string(7) "urn:foo"
<foo:bar>&amp;hello</foo:bar>

tagName: string(7) "bar:BAR"
nodeName: string(7) "bar:BAR"
textContent: string(6) "&hello"
prefix: string(3) "bar"
namespaceURI: string(7) "urn:foo"
<bar:BAR>&amp;hello</bar:BAR>

tagName: string(7) "bar:BAR"
nodeName: string(7) "bar:BAR"
textContent: string(6) "&hello"
prefix: string(3) "bar"
namespaceURI: string(7) "urn:foo"
<bar:BAR>&amp;hello</bar:BAR>

tagName: string(5) "xmlns"
nodeName: string(5) "xmlns"
textContent: string(6) "&hello"
prefix: NULL
namespaceURI: string(29) "http://www.w3.org/2000/xmlns/"
<xmlns>&amp;hello</xmlns>

tagName: string(3) "xml"
nodeName: string(3) "xml"
textContent: string(6) "&hello"
prefix: NULL
namespaceURI: string(36) "http://www.w3.org/XML/1998/namespace"
<xml>&amp;hello</xml>

--- Error cases ---
(NULL, "prefix:name"): Namespace Error
("", "prefix:name"): Namespace Error
("urn:foo", "@"): Invalid Character Error
("http://www.w3.org/2000/xmlns/", "svg"): Namespace Error
("urn:foo", "xml:xml"): Namespace Error"#);
}

/// php-src `ext/dom/tests/modern/spec/Document_createElement_edge_cases.phpt`: Document::createElement() edge cases
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_document_create_element_edge_cases() {
    if skip_without_xml_native("test_document_create_element_edge_cases") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
try {
    $dom->createElement('');
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    $dom->createElement('$');
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Invalid Character Error
Invalid Character Error"#);
}

/// php-src `ext/dom/tests/modern/spec/Document_getElementsByTagNameNS.phpt`: Document::getElementsByTagNameNS
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_document_get_elements_by_tag_name_ns() {
    if skip_without_xml_native("test_document_get_elements_by_tag_name_ns") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

require __DIR__ . '/create_element_util.inc';

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE html>
<html>
<head>
    <title>Test</title>
</head>
<body>
    <p>Hello World</p>
</body>
</html>
HTML);

$body = $dom->getElementsByTagName('body')->item(0);
$body->appendChild(createElementNS($dom, NULL, "p", "content 1"));
$body->appendChild(createElementNS($dom, "", "p", "content 2"));
$body->appendChild(createElementNS($dom, "http://www.w3.org/2000/svg", "svg:svg", "content 3"));

function dump($dom, $uri, $local) {
    $list = $dom->getElementsByTagNameNS($uri, $local);

    $uri_readable = is_null($uri) ? "NULL" : "\"$uri\"";
    echo "--- ($uri_readable, \"$local\"): {$list->length} ---\n";

    var_dump($list->length);
    foreach ($list as $item) {
        var_dump($item->textContent);
    }
}

dump($dom, 'http://www.w3.org/1999/xhtml', 'p');
dump($dom, '*', 'p');
dump($dom, '*', '*');
dump($dom, '', '*');
dump($dom, NULL, '*');
dump($dom, '*', 'svg');
dump($dom, '*', 'svg:svg');

"#);
    assert_exact(&out, r#"--- ("http://www.w3.org/1999/xhtml", "p"): 1 ---
int(1)
string(11) "Hello World"
--- ("*", "p"): 3 ---
int(3)
string(11) "Hello World"
string(9) "content 1"
string(9) "content 2"
--- ("*", "*"): 8 ---
int(8)
string(56) "
    Test


    Hello World

content 1content 2content 3"
string(10) "
    Test
"
string(4) "Test"
string(45) "
    Hello World

content 1content 2content 3"
string(11) "Hello World"
string(9) "content 1"
string(9) "content 2"
string(9) "content 3"
--- ("", "*"): 2 ---
int(2)
string(9) "content 1"
string(9) "content 2"
--- (NULL, "*"): 2 ---
int(2)
string(9) "content 1"
string(9) "content 2"
--- ("*", "svg"): 1 ---
int(1)
string(9) "content 3"
--- ("*", "svg:svg"): 0 ---
int(0)"#);
}

/// php-src `ext/dom/tests/modern/spec/Document_implementation_createDocument.phpt`: Document::$implementation createDocument
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_document_implementation_create_document() {
    if skip_without_xml_native("test_document_implementation_create_document") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();

echo "--- (null, \"\") ---\n";

var_dump($dom->implementation->createDocument(null, ""));

echo "--- (null, \"qname\") ---\n";

echo $dom->implementation->createDocument(null, "qname")->saveXml(), "\n";

echo "--- (\"\", \"qname\") ---\n";

echo $dom->implementation->createDocument("", "qname")->saveXml(), "\n";

echo "--- (\"urn:a\", \"qname\") ---\n";

echo $dom->implementation->createDocument("urn:a", "qname")->saveXml(), "\n";

echo "--- With doctype ---\n";

$dtd = $dom->implementation->createDocumentType("mydoc", "", "");
echo $dom->implementation->createDocument(null, "", $dtd)->saveXml(), "\n";

echo "--- With auto-adopting doctype ---\n";

$dom2 = Dom\XMLDocument::createEmpty();
$dtd = $dom2->implementation->createDocumentType("mydoc", "", "");
echo $dom->implementation->createDocument(null, "", $dtd)->saveXml(), "\n";

"#);
    assert_exact(&out, r##"--- (null, "") ---
object(Dom\XMLDocument)#3 (33) {
  ["xmlEncoding"]=>
  string(5) "UTF-8"
  ["xmlStandalone"]=>
  bool(false)
  ["xmlVersion"]=>
  string(3) "1.0"
  ["formatOutput"]=>
  bool(false)
  ["implementation"]=>
  string(22) "(object value omitted)"
  ["URL"]=>
  string(11) "about:blank"
  ["documentURI"]=>
  string(11) "about:blank"
  ["characterSet"]=>
  string(5) "UTF-8"
  ["charset"]=>
  string(5) "UTF-8"
  ["inputEncoding"]=>
  string(5) "UTF-8"
  ["doctype"]=>
  NULL
  ["documentElement"]=>
  NULL
  ["children"]=>
  string(22) "(object value omitted)"
  ["firstElementChild"]=>
  NULL
  ["lastElementChild"]=>
  NULL
  ["childElementCount"]=>
  int(0)
  ["body"]=>
  NULL
  ["head"]=>
  NULL
  ["title"]=>
  string(0) ""
  ["nodeType"]=>
  int(9)
  ["nodeName"]=>
  string(9) "#document"
  ["baseURI"]=>
  string(11) "about:blank"
  ["isConnected"]=>
  bool(true)
  ["ownerDocument"]=>
  NULL
  ["parentNode"]=>
  NULL
  ["parentElement"]=>
  NULL
  ["childNodes"]=>
  string(22) "(object value omitted)"
  ["firstChild"]=>
  NULL
  ["lastChild"]=>
  NULL
  ["previousSibling"]=>
  NULL
  ["nextSibling"]=>
  NULL
  ["nodeValue"]=>
  NULL
  ["textContent"]=>
  NULL
}
--- (null, "qname") ---
<?xml version="1.0" encoding="UTF-8"?>
<qname/>
--- ("", "qname") ---
<?xml version="1.0" encoding="UTF-8"?>
<qname/>
--- ("urn:a", "qname") ---
<?xml version="1.0" encoding="UTF-8"?>
<qname xmlns="urn:a"/>
--- With doctype ---
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE mydoc>

--- With auto-adopting doctype ---
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE mydoc>"##);
}

/// php-src `ext/dom/tests/modern/spec/Document_implementation_createDocumentType.phpt`: Document::$implementation createDocumentType
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_document_implementation_create_document_type() {
    if skip_without_xml_native("test_document_implementation_create_document_type") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$test_matrix = [
    ['uri' => 'qname', 'public' => 'public', 'system' => 'system'],
    ['uri' => 'qname', 'public' => 'public', 'system' => ''],
    ['uri' => 'qname', 'public' => '', 'system' => 'system'],
    ['uri' => 'qname', 'public' => '', 'system' => ''],
];

foreach ($test_matrix as $test_item) {
    $dom = Dom\XMLDocument::createEmpty();
    $dtd = $dom->implementation->createDocumentType($test_item['uri'], $test_item['public'], $test_item['system']);

    var_dump($dtd);

    $dom->append($dom->importNode($dtd));

    echo $dom->saveXml(), "\n";
}

"#);
    assert_exact(&out, r#"object(Dom\DocumentType)#3 (19) {
  ["name"]=>
  string(5) "qname"
  ["entities"]=>
  string(22) "(object value omitted)"
  ["notations"]=>
  string(22) "(object value omitted)"
  ["publicId"]=>
  string(6) "public"
  ["systemId"]=>
  string(6) "system"
  ["internalSubset"]=>
  NULL
  ["nodeType"]=>
  int(10)
  ["nodeName"]=>
  string(5) "qname"
  ["baseURI"]=>
  string(11) "about:blank"
  ["isConnected"]=>
  bool(false)
  ["parentNode"]=>
  NULL
  ["parentElement"]=>
  NULL
  ["childNodes"]=>
  string(22) "(object value omitted)"
  ["firstChild"]=>
  NULL
  ["lastChild"]=>
  NULL
  ["previousSibling"]=>
  NULL
  ["nextSibling"]=>
  NULL
  ["nodeValue"]=>
  NULL
  ["textContent"]=>
  string(0) ""
}
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE qname PUBLIC "public" "system">

object(Dom\DocumentType)#2 (19) {
  ["name"]=>
  string(5) "qname"
  ["entities"]=>
  string(22) "(object value omitted)"
  ["notations"]=>
  string(22) "(object value omitted)"
  ["publicId"]=>
  string(6) "public"
  ["systemId"]=>
  string(0) ""
  ["internalSubset"]=>
  NULL
  ["nodeType"]=>
  int(10)
  ["nodeName"]=>
  string(5) "qname"
  ["baseURI"]=>
  string(11) "about:blank"
  ["isConnected"]=>
  bool(false)
  ["parentNode"]=>
  NULL
  ["parentElement"]=>
  NULL
  ["childNodes"]=>
  string(22) "(object value omitted)"
  ["firstChild"]=>
  NULL
  ["lastChild"]=>
  NULL
  ["previousSibling"]=>
  NULL
  ["nextSibling"]=>
  NULL
  ["nodeValue"]=>
  NULL
  ["textContent"]=>
  string(0) ""
}
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE qname PUBLIC "public" "">

object(Dom\DocumentType)#1 (19) {
  ["name"]=>
  string(5) "qname"
  ["entities"]=>
  string(22) "(object value omitted)"
  ["notations"]=>
  string(22) "(object value omitted)"
  ["publicId"]=>
  string(0) ""
  ["systemId"]=>
  string(6) "system"
  ["internalSubset"]=>
  NULL
  ["nodeType"]=>
  int(10)
  ["nodeName"]=>
  string(5) "qname"
  ["baseURI"]=>
  string(11) "about:blank"
  ["isConnected"]=>
  bool(false)
  ["parentNode"]=>
  NULL
  ["parentElement"]=>
  NULL
  ["childNodes"]=>
  string(22) "(object value omitted)"
  ["firstChild"]=>
  NULL
  ["lastChild"]=>
  NULL
  ["previousSibling"]=>
  NULL
  ["nextSibling"]=>
  NULL
  ["nodeValue"]=>
  NULL
  ["textContent"]=>
  string(0) ""
}
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE qname SYSTEM "system">

object(Dom\DocumentType)#4 (19) {
  ["name"]=>
  string(5) "qname"
  ["entities"]=>
  string(22) "(object value omitted)"
  ["notations"]=>
  string(22) "(object value omitted)"
  ["publicId"]=>
  string(0) ""
  ["systemId"]=>
  string(0) ""
  ["internalSubset"]=>
  NULL
  ["nodeType"]=>
  int(10)
  ["nodeName"]=>
  string(5) "qname"
  ["baseURI"]=>
  string(11) "about:blank"
  ["isConnected"]=>
  bool(false)
  ["parentNode"]=>
  NULL
  ["parentElement"]=>
  NULL
  ["childNodes"]=>
  string(22) "(object value omitted)"
  ["firstChild"]=>
  NULL
  ["lastChild"]=>
  NULL
  ["previousSibling"]=>
  NULL
  ["nextSibling"]=>
  NULL
  ["nodeValue"]=>
  NULL
  ["textContent"]=>
  string(0) ""
}
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE qname>"#);
}

/// php-src `ext/dom/tests/modern/spec/Document_implementation_createDocumentType_errors.phpt`: Document::$implementation createDocumentType errors
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_document_implementation_create_document_type_errors() {
    if skip_without_xml_native("test_document_implementation_create_document_type_errors") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();
try {
    $dom->implementation->createDocumentType("invalid name", "public", "system");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    $dom->implementation->createDocumentType("", "public", "system");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    $dom->implementation->createDocumentType("@", "", "");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Namespace Error
Namespace Error
Namespace Error"#);
}

/// php-src `ext/dom/tests/modern/spec/Document_implementation_createDocument_errors.phpt`: Document::$implementation createDocument errors
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_document_implementation_create_document_errors() {
    if skip_without_xml_native("test_document_implementation_create_document_errors") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();

try {
    $dom->implementation->createDocument("urn:a", "@");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

try {
    $dom->implementation->createDocument("", "foo:bar");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Invalid Character Error
Namespace Error"#);
}

/// php-src `ext/dom/tests/modern/spec/Document_implementation_createHTMLDocument.phpt`: Document::$implementation createHTMLDocument
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_document_implementation_create_htmldocument() {
    if skip_without_xml_native("test_document_implementation_create_htmldocument") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();

echo "---\n";

echo $dom->implementation->createHTMLDocument()->saveXml(), "\n";

echo "---\n";

echo $dom->implementation->createHTMLDocument("")->saveXml(), "\n";

echo "---\n";

echo $dom->implementation->createHTMLDocument("my <title>")->saveXml(), "\n";

"#);
    assert_exact(&out, r#"---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body></body></html>
---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title></title></head><body></body></html>
---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>my &lt;title&gt;</title></head><body></body></html>"#);
}

/// php-src `ext/dom/tests/modern/spec/Document_implementation_same_object.phpt`: Document::$implementation should return the same object
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_document_implementation_same_object() {
    if skip_without_xml_native("test_document_implementation_same_object") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();
var_dump($dom->implementation === $dom->implementation);
$implementation = $dom->implementation;
var_dump($dom->implementation === $implementation);

$dom2 = Dom\XMLDocument::createEmpty();
var_dump($dom2->implementation === $dom->implementation);

var_dump($implementation);

try {
    clone $implementation;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"bool(true)
bool(true)
bool(false)
object(Dom\Implementation)#2 (0) {
}
Trying to clone an uncloneable object of class Dom\Implementation"#);
}

/// php-src `ext/dom/tests/modern/spec/Document_importLegacyNode.phpt`: Document::importLegacyNode
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_document_import_legacy_node() {
    if skip_without_xml_native("test_document_import_legacy_node") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$old = new DOMDocument;
$old->loadXML(<<<XML
<root>
    <child xmlns="urn:a" a="b"/>
    <child xmlns="urn:b" xmlns:c="urn:c" c:c="d"/>
    <?pi?>
    <!-- comment -->
    <![CDATA[foo]]>
</root>
XML);

$new = Dom\XMLDocument::createEmpty();
$new->append($new->importLegacyNode($old->documentElement, true));

unset($old);

foreach ($new->getElementsByTagName('child') as $child) {
    var_dump($child->attributes);
    foreach ($child->attributes as $attr) {
        echo "name: ";
        var_dump($attr->name);
        echo "prefix: ";
        var_dump($attr->prefix);
        echo "namespaceURI: ";
        var_dump($attr->namespaceURI);
    }
}

echo $new->saveXml(), "\n";

"#);
    assert_format(&out, r#"object(Dom\NamedNodeMap)#%d (1) {
  ["length"]=>
  int(2)
}
name: string(5) "xmlns"
prefix: NULL
namespaceURI: string(29) "http://www.w3.org/2000/xmlns/"
name: string(1) "a"
prefix: NULL
namespaceURI: NULL
object(Dom\NamedNodeMap)#%d (1) {
  ["length"]=>
  int(3)
}
name: string(5) "xmlns"
prefix: NULL
namespaceURI: string(29) "http://www.w3.org/2000/xmlns/"
name: string(7) "xmlns:c"
prefix: string(5) "xmlns"
namespaceURI: string(29) "http://www.w3.org/2000/xmlns/"
name: string(3) "c:c"
prefix: string(1) "c"
namespaceURI: string(5) "urn:c"
<?xml version="1.0" encoding="UTF-8"?>
<root>
    <child xmlns="urn:a" a="b"/>
    <child xmlns="urn:b" xmlns:c="urn:c" c:c="d"/>
    <?pi ?>
    <!-- comment -->
    <![CDATA[foo]]>
</root>"#);
}

/// php-src `ext/dom/tests/modern/spec/Document_importNode_not_supported.phpt`: Document::importNode() with unsupported node type
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::importNode"#]
fn test_document_import_node_not_supported() {
    if skip_without_xml_native("test_document_import_node_not_supported") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

// For legacy documents this should warn
$dom = new DOMDocument();
var_dump($dom->importNode($dom));

// For modern documents this should throw
$dom = Dom\HTMLDocument::createEmpty();
try {
    $dom->importNode($dom);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_format(&out, r#"Warning: DOMDocument::importNode(): Cannot import: Node Type Not Supported in %s on line %d
bool(false)
Not Supported Error"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_getAttribute.phpt`: Element::getAttribute()
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_get_attribute() {
    if skip_without_xml_native("test_element_get_attribute") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

echo "--- Get after parsing ---\n";

$dom = Dom\HTMLDocument::createFromString('<!DOCTYPE html><html><body align="foo" foo:bar="baz"></body></html>');
$body = $dom->getElementsByTagName("body")[0];
var_dump($body->getAttribute("align"));
var_dump($body->getAttribute("foo:bar"));

echo "--- Get after creating without namespace ---\n";

$body->setAttributeNode($attr = $dom->createAttribute("prefix:local"));
$attr->value = "A";
var_dump($body->getAttribute("prefix:local"));

echo "--- Get after creating with namespace ---\n";

$body->setAttributeNode($attr = $dom->createAttributeNS("urn:a", "prefix:local2"));
$attr->value = "B";
var_dump($body->getAttribute("prefix:local2"));
var_dump($body->getAttribute("Prefix:LOCAL2"));

echo "--- Get after creating with namespace case sensitive ---\n";

$element = $dom->createElementNS("urn:a", "a:element");
$attr = $dom->createAttributeNS("urn:a", "Prefix:local2");
$element->setAttributeNode($attr);
$attr->value = "C";
var_dump($element->getAttribute("Prefix:local2"));
var_dump($element->getAttribute("Prefix:LOCAL2"));
var_dump($element->getAttribute("prefix:local2"));

"#);
    assert_exact(&out, r#"--- Get after parsing ---
string(3) "foo"
string(3) "baz"
--- Get after creating without namespace ---
string(1) "A"
--- Get after creating with namespace ---
string(1) "B"
string(1) "B"
--- Get after creating with namespace case sensitive ---
string(1) "C"
NULL
NULL"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_getAttributeNS.phpt`: Element::getAttributeNS()
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_get_attribute_ns() {
    if skip_without_xml_native("test_element_get_attribute_ns") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString('<!DOCTYPE html><html><body align="foo" foo:bar="baz"></body></html>');
$body = $dom->getElementsByTagName("body")[0];

echo "--- After parsing, i.e. without namespace ---\n";

// Every pair of 2 calls should return the same result
var_dump($body->getAttributeNS(NULL, "align"));
var_dump($body->getAttributeNS("", "align"));
var_dump($body->getAttributeNS(NULL, "ALIGN"));
var_dump($body->getAttributeNS("", "ALIGN"));
var_dump($body->getAttributeNS(NULL, "foo:bar"));
var_dump($body->getAttributeNS("", "foo:bar"));
var_dump($body->getAttributeNS(NULL, "FOO:BAR"));
var_dump($body->getAttributeNS("", "FOO:BAR"));

echo "--- Special legacy case ---\n";

var_dump($dom->documentElement->getAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns"));

echo "--- Get after creating without namespace ---\n";

$body->setAttributeNode($attr = $dom->createAttribute("prefix:local"));
$attr->value = "A";
var_dump($body->getAttributeNS(NULL, "prefix:local"));
var_dump($body->getAttributeNS(NULL, "prefix:LOCAL"));

echo "--- Get after creating with namespace ---\n";

$body->setAttributeNode($attr = $dom->createAttributeNS("urn:a", "prefix:local2"));
$attr->value = "B";
var_dump($body->getAttributeNS("urn:a", "local2"));
var_dump($body->getAttributeNS("urn:a", "LOCAL2"));

"#);
    assert_exact(&out, r#"--- After parsing, i.e. without namespace ---
string(3) "foo"
string(3) "foo"
NULL
NULL
string(3) "baz"
string(3) "baz"
NULL
NULL
--- Special legacy case ---
NULL
--- Get after creating without namespace ---
string(1) "A"
NULL
--- Get after creating with namespace ---
string(1) "B"
NULL"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_getAttributeNode.phpt`: Element::getAttributeNode()
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_get_attribute_node() {
    if skip_without_xml_native("test_element_get_attribute_node") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

echo "--- Get after parsing ---\n";

$dom = Dom\HTMLDocument::createFromString('<!DOCTYPE html><html><body align="foo" foo:bar="baz"></body></html>');
$body = $dom->getElementsByTagName("body")[0];
var_dump($body->getAttributeNode("align")->textContent);
var_dump($body->getAttributeNode("foo:bar")->textContent);

echo "--- Get after creating without namespace ---\n";

$body->setAttributeNode($attr = $dom->createAttribute("prefix:local"));
$attr->value = "A";
var_dump($body->getAttributeNode("prefix:local")->textContent);

echo "--- Get after creating with namespace ---\n";

$body->setAttributeNode($attr = $dom->createAttributeNS("urn:a", "prefix:local2"));
$attr->value = "B";
var_dump($body->getAttributeNode("prefix:local2")->textContent);
var_dump($body->getAttributeNode("Prefix:LOCAL2")->textContent);

echo "--- Get after creating with namespace case sensitive ---\n";

$element = $dom->createElementNS("urn:a", "a:element");
$attr = $dom->createAttributeNS("urn:a", "Prefix:local2");
$element->setAttributeNode($attr);
$attr->value = "C";
var_dump($element->getAttributeNode("Prefix:local2")->textContent);
var_dump($element->getAttributeNode("Prefix:LOCAL2"));
var_dump($element->getAttributeNode("prefix:local2"));

"#);
    assert_exact(&out, r#"--- Get after parsing ---
string(3) "foo"
string(3) "baz"
--- Get after creating without namespace ---
string(1) "A"
--- Get after creating with namespace ---
string(1) "B"
string(1) "B"
--- Get after creating with namespace case sensitive ---
string(1) "C"
NULL
NULL"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_getAttributeNodeNS.phpt`: Element::getAttributeNodeNS()
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_get_attribute_node_ns() {
    if skip_without_xml_native("test_element_get_attribute_node_ns") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

echo "--- Get after parsing ---\n";

$dom = Dom\HTMLDocument::createFromString('<!DOCTYPE html><html><body align="foo" foo:bar="baz"></body></html>');
$body = $dom->getElementsByTagName("body")[0];

echo "--- After parsing, i.e. without namespace ---\n";

// Every pair of 2 calls should return the same result
var_dump($body->getAttributeNodeNS(NULL, "align")->textContent);
var_dump($body->getAttributeNodeNS("", "align")->textContent);
var_dump($body->getAttributeNodeNS(NULL, "ALIGN"));
var_dump($body->getAttributeNodeNS("", "ALIGN"));
var_dump($body->getAttributeNodeNS(NULL, "foo:bar")->textContent);
var_dump($body->getAttributeNodeNS("", "foo:bar")->textContent);
var_dump($body->getAttributeNodeNS(NULL, "FOO:BAR"));
var_dump($body->getAttributeNodeNS("", "FOO:BAR"));

echo "--- Special legacy case ---\n";

var_dump($dom->documentElement->getAttributeNodeNS("http://www.w3.org/2000/xmlns/", "xmlns"));

echo "--- Get after creating without namespace ---\n";

$body->setAttributeNode($attr = $dom->createAttribute("prefix:local"));
$attr->value = "A";
var_dump($body->getAttributeNodeNS(NULL, "prefix:local")->textContent);
var_dump($body->getAttributeNodeNS(NULL, "prefix:LOCAL"));

echo "--- Get after creating with namespace ---\n";

$body->setAttributeNode($attr = $dom->createAttributeNS("urn:a", "prefix:local2"));
$attr->value = "B";
var_dump($body->getAttributeNodeNS("urn:a", "local2")->textContent);
var_dump($body->getAttributeNodeNS("urn:a", "LOCAL2"));

"#);
    assert_exact(&out, r#"--- Get after parsing ---
--- After parsing, i.e. without namespace ---
string(3) "foo"
string(3) "foo"
NULL
NULL
string(3) "baz"
string(3) "baz"
NULL
NULL
--- Special legacy case ---
NULL
--- Get after creating without namespace ---
string(1) "A"
NULL
--- Get after creating with namespace ---
string(1) "B"
NULL"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_hasAttribute.phpt`: Element::hasAttribute()
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_has_attribute() {
    if skip_without_xml_native("test_element_has_attribute") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

echo "--- Get after parsing ---\n";

$dom = Dom\HTMLDocument::createFromString('<!DOCTYPE html><html><body align="foo" foo:bar="baz"></body></html>');
$body = $dom->getElementsByTagName("body")[0];
var_dump($body->hasAttribute("align"));
var_dump($body->hasAttribute("foo:bar"));

echo "--- Get after creating without namespace ---\n";

$body->setAttributeNode($attr = $dom->createAttribute("prefix:local"));
$attr->value = "A";
var_dump($body->hasAttribute("prefix:local"));

echo "--- Get after creating with namespace ---\n";

$body->setAttributeNode($attr = $dom->createAttributeNS("urn:a", "prefix:local2"));
$attr->value = "B";
var_dump($body->hasAttribute("prefix:local2"));
var_dump($body->hasAttribute("Prefix:LOCAL2"));

echo "--- Get after creating with namespace case sensitive ---\n";

$element = $dom->createElementNS("urn:a", "a:element");
$attr = $dom->createAttributeNS("urn:a", "Prefix:local2");
$element->setAttributeNode($attr);
$attr->value = "C";
var_dump($element->hasAttribute("Prefix:local2"));
var_dump($element->hasAttribute("Prefix:LOCAL2"));
var_dump($element->hasAttribute("prefix:local2"));

"#);
    assert_exact(&out, r#"--- Get after parsing ---
bool(true)
bool(true)
--- Get after creating without namespace ---
bool(true)
--- Get after creating with namespace ---
bool(true)
bool(true)
--- Get after creating with namespace case sensitive ---
bool(true)
bool(false)
bool(false)"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_hasAttributeNS.phpt`: Element::hasAttributeNS()
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_has_attribute_ns() {
    if skip_without_xml_native("test_element_has_attribute_ns") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString('<!DOCTYPE html><html><body align="foo" foo:bar="baz"></body></html>');
$body = $dom->getElementsByTagName("body")[0];

echo "--- After parsing, i.e. without namespace ---\n";

// Every pair of 2 calls should return the same result
var_dump($body->hasAttributeNS(NULL, "align"));
var_dump($body->hasAttributeNS("", "align"));
var_dump($body->hasAttributeNS(NULL, "ALIGN"));
var_dump($body->hasAttributeNS("", "ALIGN"));
var_dump($body->hasAttributeNS(NULL, "foo:bar"));
var_dump($body->hasAttributeNS("", "foo:bar"));
var_dump($body->hasAttributeNS(NULL, "FOO:BAR"));
var_dump($body->hasAttributeNS("", "FOO:BAR"));

echo "--- Special legacy case ---\n";

var_dump($dom->documentElement->hasAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns"));

echo "--- Get after creating without namespace ---\n";

$body->setAttributeNode($attr = $dom->createAttribute("prefix:local"));
$attr->value = "A";
var_dump($body->hasAttributeNS(NULL, "prefix:local"));
var_dump($body->hasAttributeNS(NULL, "prefix:LOCAL"));

echo "--- Get after creating with namespace ---\n";

$body->setAttributeNode($attr = $dom->createAttributeNS("urn:a", "prefix:local2"));
$attr->value = "B";
var_dump($body->hasAttributeNS("urn:a", "local2"));
var_dump($body->hasAttributeNS("urn:a", "LOCAL2"));

"#);
    assert_exact(&out, r#"--- After parsing, i.e. without namespace ---
bool(true)
bool(true)
bool(false)
bool(false)
bool(true)
bool(true)
bool(false)
bool(false)
--- Special legacy case ---
bool(false)
--- Get after creating without namespace ---
bool(true)
bool(false)
--- Get after creating with namespace ---
bool(true)
bool(false)"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_insertAdjacentText.phpt`: Element::insertAdjacentText() edge cases
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_insert_adjacent_text() {
    if skip_without_xml_native("test_element_insert_adjacent_text") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$foo = $dom->appendChild($dom->createElement("foo"));
try {
    $foo->insertAdjacentText(Dom\AdjacentPosition::BeforeBegin, "bar");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

$foo->insertAdjacentText(Dom\AdjacentPosition::AfterBegin, "bar");
$foo->insertAdjacentText(Dom\AdjacentPosition::BeforeEnd, "baz");

echo $dom->saveHtml(), "\n";

var_dump($foo->firstChild->textContent);
var_dump($foo->firstChild->nextSibling->textContent);

"#);
    assert_exact(&out, r#"Cannot insert text as a child of a document
<foo>barbaz</foo>
string(3) "bar"
string(3) "baz""#);
}

/// php-src `ext/dom/tests/modern/spec/Element_prefix_readonly.phpt`: Ensure that Dom\Node::$prefix is read-only
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_prefix_readonly() {
    if skip_without_xml_native("test_element_prefix_readonly") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php
$dom = Dom\HTMLDocument::createEmpty();
$div = $dom->createElement('div');
try {
    $div->prefix = "foo";
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveXml();
"#);
    assert_exact(&out, r#"Cannot modify readonly property Dom\HTMLElement::$prefix
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_removeAttribute.phpt`: Element::removeAttribute()
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_remove_attribute() {
    if skip_without_xml_native("test_element_remove_attribute") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

echo "--- Remove after parsing ---\n";

$dom = Dom\HTMLDocument::createFromString('<!DOCTYPE html><html><body align="foo" foo:bar="baz"></body></html>');
$body = $dom->getElementsByTagName("body")[0];
var_dump($body->removeAttribute("align"));
var_dump($body->removeAttribute("foo:bar"));
echo $dom->saveHtml($body), "\n";

echo "--- Remove after creating without namespace ---\n";

$body->setAttributeNode($attr = $dom->createAttribute("prefix:local"));
$attr->value = "A";
var_dump($body->removeAttribute("prefix:local"));
echo $dom->saveHtml($body), "\n";

echo "--- Remove after creating with namespace ---\n";

$body->setAttributeNode($attr = $dom->createAttributeNS("urn:a", "prefix:local2"));
$attr->value = "B";
var_dump($body->removeAttribute("prefix:local2"));
echo $dom->saveHtml($body), "\n";
$body->setAttributeNode($attr = $dom->createAttributeNS("urn:a", "prefix:local2"));
$attr->value = "B";
var_dump($body->removeAttribute("Prefix:LOCAL2"));
echo $dom->saveHtml($body), "\n";

echo "--- Remove after creating with namespace case sensitive ---\n";

$element = $dom->createElementNS("urn:a", "a:element");
$attr = $dom->createAttributeNS("urn:a", "Prefix:local2");
$element->setAttributeNode($attr);
echo $dom->saveHtml($element), "\n";
$attr->value = "C";
var_dump($element->removeAttribute("Prefix:local2"));
var_dump($element->removeAttribute("Prefix:LOCAL2"));
var_dump($element->removeAttribute("prefix:local2"));
echo $dom->saveHtml($element), "\n";

"#);
    assert_exact(&out, r#"--- Remove after parsing ---
NULL
NULL
<body></body>
--- Remove after creating without namespace ---
NULL
<body></body>
--- Remove after creating with namespace ---
NULL
<body></body>
NULL
<body></body>
--- Remove after creating with namespace case sensitive ---
<a:element Prefix:local2=""></a:element>
NULL
NULL
NULL
<a:element></a:element>"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_removeAttributeNS.phpt`: Element::removeAttributeNS()
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_remove_attribute_ns() {
    if skip_without_xml_native("test_element_remove_attribute_ns") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

function createFreshBody() {
    $dom = Dom\HTMLDocument::createFromString('<!DOCTYPE html><html><body align="foo" foo:bar="baz"></body></html>');
    $body = $dom->getElementsByTagName("body")[0];
    return $body;
}

echo "--- After parsing, i.e. without namespace ---\n";

$body = createFreshBody();
$body->removeAttributeNS(NULL, "align");
echo $body->ownerDocument->saveHtml($body), "\n";

$body = createFreshBody();
$body->removeAttributeNS("", "align");
echo $body->ownerDocument->saveHtml($body), "\n";

$body = createFreshBody();
$body->removeAttributeNS(NULL, "ALIGN");
echo $body->ownerDocument->saveHtml($body), "\n";

$body = createFreshBody();
$body->removeAttributeNS("", "ALIGN");
echo $body->ownerDocument->saveHtml($body), "\n";

$body = createFreshBody();
$body->removeAttributeNS(NULL, "foo:bar");
echo $body->ownerDocument->saveHtml($body), "\n";

$body = createFreshBody();
$body->removeAttributeNS("", "foo:bar");
echo $body->ownerDocument->saveHtml($body), "\n";

$body = createFreshBody();
$body->removeAttributeNS(NULL, "FOO:BAR");
echo $body->ownerDocument->saveHtml($body), "\n";

$body = createFreshBody();
$body->removeAttributeNS("", "FOO:BAR");
echo $body->ownerDocument->saveHtml($body), "\n";

echo "--- Special legacy case ---\n";

$body = createFreshBody();
$body->ownerDocument->documentElement->removeAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns");

echo "--- Remove after creating without namespace ---\n";

$body = createFreshBody();
$body->setAttributeNode($attr = $body->ownerDocument->createAttribute("prefix:local"));
$attr->value = "A";
echo $body->ownerDocument->saveHtml($body), "\n";
$body->removeAttributeNS(NULL, "prefix:LOCAL");
echo $body->ownerDocument->saveHtml($body), "\n";
$body->removeAttributeNS(NULL, "prefix:local");
echo $body->ownerDocument->saveHtml($body), "\n";

echo "--- Remove after creating with namespace ---\n";

$body = createFreshBody();
$body->setAttributeNode($attr = $body->ownerDocument->createAttributeNS("urn:a", "prefix:local2"));
$attr->value = "B";
echo $body->ownerDocument->saveHtml($body), "\n";
$body->removeAttributeNS("urn:a", "LOCAL2");
echo $body->ownerDocument->saveHtml($body), "\n";
$body->removeAttributeNS("urn:a", "local2");
echo $body->ownerDocument->saveHtml($body), "\n";

"#);
    assert_exact(&out, r#"--- After parsing, i.e. without namespace ---
<body foo:bar="baz"></body>
<body foo:bar="baz"></body>
<body align="foo" foo:bar="baz"></body>
<body align="foo" foo:bar="baz"></body>
<body align="foo"></body>
<body align="foo"></body>
<body align="foo" foo:bar="baz"></body>
<body align="foo" foo:bar="baz"></body>
--- Special legacy case ---
--- Remove after creating without namespace ---
<body align="foo" foo:bar="baz" prefix:local="A"></body>
<body align="foo" foo:bar="baz" prefix:local="A"></body>
<body align="foo" foo:bar="baz"></body>
--- Remove after creating with namespace ---
<body align="foo" foo:bar="baz" prefix:local2="B"></body>
<body align="foo" foo:bar="baz" prefix:local2="B"></body>
<body align="foo" foo:bar="baz"></body>"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_removeAttribute_edge_cases.phpt`: Element::removeAttribute() edge cases
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createAttribute"#]
fn test_element_remove_attribute_edge_cases() {
    if skip_without_xml_native("test_element_remove_attribute_edge_cases") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root a="b"/>');
try {
    $dom->documentElement->removeAttributeNode($dom->createAttribute('test'));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Not Found Error"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_setAttributeNS.phpt`: Element::setAttributeNS()
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\Attr"#]
fn test_element_set_attribute_ns() {
    if skip_without_xml_native("test_element_set_attribute_ns") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

require __DIR__ . "/dump_attr.inc";

$dom = Dom\HTMLDocument::createEmpty();
$container = $dom->appendChild($dom->createElement("container"));

echo "--- xmlns attribute ---\n";

$container->setAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns:foo", "1");
echo $dom->saveHtml($container), "\n";
dumpAttrs($container);

echo "--- name validation ---\n";

try {
    $container->setAttributeNS("urn:a", "a:b:c", "");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- ns attributes with same namespace but different prefix ---\n";

$dom = Dom\HTMLDocument::createEmpty();
$container = $dom->appendChild($dom->createElement("container"));

$container->setAttributeNS("urn:a", "x:foo", "1");
$container->setAttributeNS("urn:a", "y:foo", "2");
echo $dom->saveHtml($container), "\n";
dumpAttrs($container);

echo "--- ns attributes with different namespace but same prefix ---\n";

$dom = Dom\HTMLDocument::createEmpty();
$container = $dom->appendChild($dom->createElement("container"));

$container->setAttributeNS("urn:a", "x:foo", "1");
$container->setAttributeNS("urn:b", "x:foo", "2");
echo $dom->saveHtml($container), "\n";
dumpAttrs($container);

"#);
    assert_exact(&out, r#"--- xmlns attribute ---
<container xmlns:foo="1"></container>
Attr: xmlns:foo
string(5) "xmlns"
string(9) "xmlns:foo"
string(29) "http://www.w3.org/2000/xmlns/"
--- name validation ---
Invalid Character Error
--- ns attributes with same namespace but different prefix ---
<container y:foo="2"></container>
Attr: y:foo
string(1) "y"
string(5) "y:foo"
string(5) "urn:a"
--- ns attributes with different namespace but same prefix ---
<container x:foo="1" x:foo="2"></container>
Attr: x:foo
string(1) "x"
string(5) "x:foo"
string(5) "urn:a"
Attr: x:foo
string(1) "x"
string(5) "x:foo"
string(5) "urn:b""#);
}

/// php-src `ext/dom/tests/modern/spec/Element_setAttributeNode.phpt`: Dom\Element::setAttributeNode(NS) in the same document
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_set_attribute_node() {
    if skip_without_xml_native("test_element_set_attribute_node") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom1 = Dom\HTMLDocument::createEmpty();
$container = $dom1->appendChild($dom1->createElement("container"));

echo "--- Without namespace ---\n";

$attribute1 = $dom1->createAttribute("my-attribute");
$attribute1->value = "1";
$container->setAttributeNode($attribute1);
var_dump($container->setAttributeNode($attribute1) === null);
$attribute2 = $dom1->createAttribute("my-attribute");
$attribute2->value = "2";
var_dump($container->setAttributeNode($attribute2) === $attribute1);
$attribute3 = $dom1->createAttributeNS("", "my-ATTRIBUTE");
$attribute3->value = "3";
var_dump($container->setAttributeNode($attribute3) === null);

echo "--- With namespace ---\n";

$attribute4 = $dom1->createAttributeNS("urn:a", "my-attribute");
$attribute4->value = "4";
$container->setAttributeNode($attribute4);
var_dump($container->setAttributeNode($attribute4) === null);
$attribute5 = $dom1->createAttributeNS("urn:b", "my-attribute");
$attribute5->value = "5";
var_dump($container->setAttributeNodeNS($attribute5) === null);

echo "--- Resulting document ---\n";

echo $dom1->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"--- Without namespace ---
bool(true)
bool(true)
bool(true)
--- With namespace ---
bool(true)
bool(true)
--- Resulting document ---
<container my-attribute="2" my-ATTRIBUTE="3" my-attribute="4" my-attribute="5"></container>"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_setAttributeNode_adoption.phpt`: Dom\Element::setAttributeNode(NS) adopting from another document
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_set_attribute_node_adoption() {
    if skip_without_xml_native("test_element_set_attribute_node_adoption") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom1 = Dom\HTMLDocument::createEmpty();
$container = $dom1->appendChild($dom1->createElement("container"));

echo "--- Without namespace ---\n";

$dom2 = Dom\HTMLDocument::createEmpty();
$attr = $dom2->createAttribute("my-attribute");
$attr->value = "1";

$container->setAttributeNode($attr);
var_dump($attr->ownerDocument === $dom1);

// Should not cause problems
unset($dom2);
unset($attr);

echo "--- With namespace ---\n";

$dom2 = Dom\HTMLDocument::createEmpty();
$attr2 = $dom2->createAttributeNS("urn:a", "a:my-attribute");
$attr2->value = "2";
$dom1->documentElement->setAttributeNode($attr2);

$dom2 = Dom\HTMLDocument::createEmpty();
$attr3 = $dom2->createAttributeNS("urn:b", "a:my-attribute");
$attr3->value = "3";
$dom1->documentElement->setAttributeNode($attr3);

var_dump($attr2->prefix, $attr2->namespaceURI, $attr2->localName);
var_dump($attr3->prefix, $attr3->namespaceURI, $attr3->localName);

echo "--- Resulting document ---\n";

echo $dom1->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"--- Without namespace ---
bool(true)
--- With namespace ---
string(1) "a"
string(5) "urn:a"
string(12) "my-attribute"
string(1) "a"
string(5) "urn:b"
string(12) "my-attribute"
--- Resulting document ---
<container my-attribute="1" a:my-attribute="2" a:my-attribute="3"></container>"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_setAttributeNode_inuse.phpt`: Dom\Element::setAttributeNode(NS) inuse error
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_set_attribute_node_inuse() {
    if skip_without_xml_native("test_element_set_attribute_node_inuse") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom1 = Dom\HTMLDocument::createEmpty();
$container = $dom1->appendChild($dom1->createElement("container"));
$attr1 = $dom1->createAttribute("my-attribute");
$attr1->value = "1";
$container->setAttributeNode($attr1);
$element = $container->appendChild($dom1->createElement("element"));
try {
    $element->setAttributeNode($attr1);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom1->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"Inuse Attribute Error
<container my-attribute="1"><element></element></container>"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_setAttribute_with_namespace.phpt`: Element::setAttribute() with namespace
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\Element"#]
fn test_element_set_attribute_with_namespace() {
    if skip_without_xml_native("test_element_set_attribute_with_namespace") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

require __DIR__ . "/dump_attr.inc";

$dom = Dom\HTMLDocument::createEmpty();
$container = $dom->appendChild($dom->createElementNS("urn:a", "container"));

$attrs = [];

function setAttribute($container, string $name, string $value): Dom\Attr {
    $container->setAttribute($name, $value);
    return $container->getAttributeNode($name);
}

$attrs[] = setAttribute($container, "foo:bar", "&hello1");
echo $dom->saveHtml($container), "\n";
$attrs[] = setAttribute($container, "foo:bar", "&hello2");
echo $dom->saveHtml($container), "\n";
$attrs[] = setAttribute($container, "bar", "&hello3");
echo $dom->saveHtml($container), "\n";
$attrs[] = setAttribute($container, "xmlns", "&hello4");
echo $dom->saveHtml($container), "\n";
$attrs[] = setAttribute($container, "XMLns", "&hello5");
echo $dom->saveHtml($container), "\n";
$attrs[] = setAttribute($container, "BAR", "&hello6");
echo $dom->saveHtml($container), "\n";

$container->setAttributeNode($dom->createAttributeNS("urn:b", "in:ns"));
$attrs[] = setAttribute($container, "in:ns", "&hello7");
echo $dom->saveHtml($container), "\n";

// Dump at the end to check whether they influenced each other
foreach ($attrs as $attr) {
    dumpAttr($attr);
}

"#);
    assert_exact(&out, r#"<container foo:bar="&amp;hello1"></container>
<container foo:bar="&amp;hello2"></container>
<container foo:bar="&amp;hello2" bar="&amp;hello3"></container>
<container foo:bar="&amp;hello2" bar="&amp;hello3" xmlns="&amp;hello4"></container>
<container foo:bar="&amp;hello2" bar="&amp;hello3" xmlns="&amp;hello4" XMLns="&amp;hello5"></container>
<container foo:bar="&amp;hello2" bar="&amp;hello3" xmlns="&amp;hello4" XMLns="&amp;hello5" BAR="&amp;hello6"></container>
<container foo:bar="&amp;hello2" bar="&amp;hello3" xmlns="&amp;hello4" XMLns="&amp;hello5" BAR="&amp;hello6" in:ns="&amp;hello7"></container>
Attr: foo:bar
NULL
string(7) "foo:bar"
NULL
Attr: foo:bar
NULL
string(7) "foo:bar"
NULL
Attr: bar
NULL
string(3) "bar"
NULL
Attr: xmlns
NULL
string(5) "xmlns"
NULL
Attr: XMLns
NULL
string(5) "XMLns"
NULL
Attr: BAR
NULL
string(3) "BAR"
NULL
Attr: in:ns
string(2) "in"
string(5) "in:ns"
string(5) "urn:b""#);
}

/// php-src `ext/dom/tests/modern/spec/Element_setAttribute_without_namespace.phpt`: Element::setAttribute() without namespace
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\Element"#]
fn test_element_set_attribute_without_namespace() {
    if skip_without_xml_native("test_element_set_attribute_without_namespace") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

require __DIR__ . "/dump_attr.inc";

$dom = Dom\HTMLDocument::createEmpty();
$container = $dom->appendChild($dom->createElement("container"));

$attrs = [];

function setAttribute($container, string $name, string $value): Dom\Attr {
    $container->setAttribute($name, $value);
    return $container->getAttributeNode($name);
}

$attrs[] = setAttribute($container, "foo:bar", "&hello1");
echo $dom->saveHtml($container), "\n";
$attrs[] = setAttribute($container, "foo:bar", "&hello2");
echo $dom->saveHtml($container), "\n";
$attrs[] = setAttribute($container, "bar", "&hello3");
echo $dom->saveHtml($container), "\n";
$attrs[] = setAttribute($container, "xmlns", "&hello4");
echo $dom->saveHtml($container), "\n";
$attrs[] = setAttribute($container, "XMLns", "&hello5");
echo $dom->saveHtml($container), "\n";
$attrs[] = setAttribute($container, "BAR", "&hello6");
echo $dom->saveHtml($container), "\n";

// Dump at the end to check whether they influenced each other
foreach ($attrs as $attr) {
    dumpAttr($attr);
}

"#);
    assert_exact(&out, r#"<container foo:bar="&amp;hello1"></container>
<container foo:bar="&amp;hello2"></container>
<container foo:bar="&amp;hello2" bar="&amp;hello3"></container>
<container foo:bar="&amp;hello2" bar="&amp;hello3" xmlns="&amp;hello4"></container>
<container foo:bar="&amp;hello2" bar="&amp;hello3" xmlns="&amp;hello5"></container>
<container foo:bar="&amp;hello2" bar="&amp;hello6" xmlns="&amp;hello5"></container>
Attr: foo:bar
NULL
string(7) "foo:bar"
NULL
Attr: foo:bar
NULL
string(7) "foo:bar"
NULL
Attr: bar
NULL
string(3) "bar"
NULL
Attr: xmlns
NULL
string(5) "xmlns"
NULL
Attr: xmlns
NULL
string(5) "xmlns"
NULL
Attr: bar
NULL
string(3) "bar"
NULL"#);
}

/// php-src `ext/dom/tests/modern/spec/Element_tagName.phpt`: Dom\Element::$tagName HTML-uppercased qualified name
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_tag_name() {
    if skip_without_xml_native("test_element_tag_name") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

echo "--- For HTML document ---\n";

$dom = Dom\HTMLDocument::createEmpty();
var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml', 'html')->tagName);
var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml/', 'html')->tagName);
var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml', 'foo:html')->tagName);
var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml/', 'foo:html')->tagName);
var_dump($dom->createElementNS('', 'html')->tagName);
var_dump($dom->createElementNS(null, 'html')->tagName);

echo "--- For XML document ---\n";

$dom = Dom\XMLDocument::createEmpty();
var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml', 'html')->tagName);
var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml/', 'html')->tagName);
var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml', 'foo:html')->tagName);
var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml/', 'foo:html')->tagName);
var_dump($dom->createElementNS('', 'html')->tagName);
var_dump($dom->createElementNS(null, 'html')->tagName);
"#);
    assert_exact(&out, r#"--- For HTML document ---
string(4) "HTML"
string(4) "html"
string(8) "FOO:HTML"
string(8) "foo:html"
string(4) "html"
string(4) "html"
--- For XML document ---
string(4) "html"
string(4) "html"
string(8) "foo:html"
string(8) "foo:html"
string(4) "html"
string(4) "html""#);
}

/// php-src `ext/dom/tests/modern/spec/Element_toggleAttribute.phpt`: Element::toggleAttribute() xmlns spec compliance
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_toggle_attribute() {
    if skip_without_xml_native("test_element_toggle_attribute") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$container = $dom->appendChild($dom->createElement('container'));

$container->toggleAttribute('hidden');
$container->toggleAttribute('xmlns:foo');
$container->toggleAttribute('xmlns:bar');
$container->toggleAttribute('xmlns');

echo $dom->saveHtml(), "\n";

$container->toggleAttribute('hidden');
$container->toggleAttribute('xmlns:foo');
$container->toggleAttribute('xmlns:bar');
$container->toggleAttribute('xmlns');

echo $dom->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<container hidden="" xmlns:foo="" xmlns:bar="" xmlns=""></container>
<container></container>"#);
}

/// php-src `ext/dom/tests/modern/spec/HTMLDocument_createCDATASection.phpt`: Dom\HTMLDocument::createCDATASection()
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_create_cdatasection() {
    if skip_without_xml_native("test_htmldocument_create_cdatasection") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php
$dom = Dom\HTMLDocument::createEmpty();
try {
    $dom->createCDATASection("foo");
} catch (DOMException $e) {
    var_dump($e->getCode());
    echo $e->getMessage(), "\n";
}
try {
    $dom->createCDATASection("]]>");
} catch (DOMException $e) {
    var_dump($e->getCode());
    echo $e->getMessage(), "\n";
}

$dom = Dom\XMLDocument::createEmpty();
try {
    $dom->createCDATASection("]]>");
} catch (DOMException $e) {
    var_dump($e->getCode());
    echo $e->getMessage(), "\n";
}
$dom->createCDATASection("]>");
"#);
    assert_exact(&out, r#"int(9)
This operation is not supported for HTML documents
int(9)
This operation is not supported for HTML documents
int(5)
Invalid character sequence "]]>" in CDATA section"#);
}

/// php-src `ext/dom/tests/modern/spec/HTMLDocument_createProcessingInstruction.phpt`: Dom\HTMLDocument::createProcessingInstruction()
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_create_processing_instruction() {
    if skip_without_xml_native("test_htmldocument_create_processing_instruction") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php
$dom = Dom\HTMLDocument::createEmpty();
try {
    $dom->createProcessingInstruction("?>", "");
} catch (DOMException $e) {
    var_dump($e->getCode());
    echo $e->getMessage(), "\n";
}
try {
    $dom->createProcessingInstruction("?>", "?>");
} catch (DOMException $e) {
    var_dump($e->getCode());
    echo $e->getMessage(), "\n";
}
try {
    $dom->createProcessingInstruction("target", "?>");
} catch (DOMException $e) {
    var_dump($e->getCode());
    echo $e->getMessage(), "\n";
}
$dom->appendChild($dom->createProcessingInstruction("foo", ""));
$dom->appendChild($dom->createProcessingInstruction("foo", "bar"));
echo $dom->saveHtml();
"#);
    assert_exact(&out, r#"int(5)
Invalid Character Error
int(5)
Invalid Character Error
int(5)
Invalid character sequence "?>" in processing instruction
<?foo ><?foo bar>"#);
}

/// php-src `ext/dom/tests/modern/spec/HTMLDocument_getElementsByTagName.phpt`: HTMLDocument::getElementsByTagName
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_get_elements_by_tag_name() {
    if skip_without_xml_native("test_htmldocument_get_elements_by_tag_name") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

require __DIR__ . "/node_list_dump.inc";
require __DIR__ . "/create_element_util.inc";

$dom = Dom\HTMLDocument::createEmpty();
$container = $dom->appendChild(createElement($dom, "container"));
$container->appendChild(createElement($dom, "HTML", "1"));
$container->appendChild(createElementNS($dom, "http://www.w3.org/1999/xhtml", "html", "2"));
$container->appendChild(createElementNS($dom, NULL, "html", "3"));
$container->appendChild(createElementNS($dom, NULL, "HTML", "4"));
$container->appendChild(createElementNS($dom, "urn:foo", "htML", "5"));
$container->appendChild(createElement($dom, "foo:HTML", "6"));
$container->appendChild(createElementNS($dom, "urn:a", "foo:HTML", "7")); // Should never match in this test file
$container->appendChild(createElementNS($dom, "http://www.w3.org/1999/xhtml", "bar:HTML", "8"));
$container->appendChild(createElementNS($dom, "http://www.w3.org/1999/xhtml", "bar:html", "9"));

dumpNodeList($dom->getElementsByTagName("HTml"));
dumpNodeList($dom->getElementsByTagName("htML"));
dumpNodeList($dom->getElementsByTagName("html"));
dumpNodeList($dom->getElementsByTagName("HTML"));
// Matches 6 instead of 7 because the input to this function is lowercased
// _and_ createElement also lowercases.
// This is in contrast to createElementNS that won't lowercase.
dumpNodeList($dom->getElementsByTagName("foo:html"));
// This will match both 6 and 7. 7 is now matched because the casing is right.
dumpNodeList($dom->getElementsByTagName("foo:HTML"));
// Both of the following calls will match 9 because 8 isn't lowercased due to the use of createElementNS.
dumpNodeList($dom->getElementsByTagName("bar:HTML"));
dumpNodeList($dom->getElementsByTagName("bar:html"));

"#);
    assert_exact(&out, r#"Node list length: int(2)
	HTML 1
	HTML 2
---
Node list length: int(3)
	HTML 1
	HTML 2
	htML 5
---
Node list length: int(3)
	HTML 1
	HTML 2
	html 3
---
Node list length: int(3)
	HTML 1
	HTML 2
	HTML 4
---
Node list length: int(1)
	FOO:HTML 6
---
Node list length: int(2)
	FOO:HTML 6
	foo:HTML 7
---
Node list length: int(1)
	BAR:HTML 9
---
Node list length: int(1)
	BAR:HTML 9
---"#);
}

/// php-src `ext/dom/tests/modern/spec/HTMLDocument_importNode_01.phpt`: HTMLDocument::importNode() with namespace should not shift namespaces
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_import_node_01() {
    if skip_without_xml_native("test_htmldocument_import_node_01") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString(<<<XML
<?xml version="1.0"?>
<html>
    <body xmlns="http://www.w3.org/1999/xhtml">
        <h1>hello world.</h1>
        <p>test</p>
        <br/>
        <p>test 2</p>
        <default:p xmlns:default="http://www.w3.org/1999/xhtml" class="foo" id="import">namespace prefixed</default:p>
    </body>
</html>
XML);

// Note the HTMLDocument class!
$dom2 = Dom\HTMLDocument::createEmpty();
$imported = $dom2->importNode($dom->documentElement, true);
$dom2->appendChild($imported);

$body = $dom2->getElementsByTagName("body")[0];
$default_p = $body->lastElementChild;
var_dump($default_p->prefix);
var_dump($default_p->namespaceURI);

echo $dom2->saveXml();

"#);
    assert_exact(&out, r#"string(7) "default"
string(28) "http://www.w3.org/1999/xhtml"
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html>
    <body xmlns="http://www.w3.org/1999/xhtml">
        <h1>hello world.</h1>
        <p>test</p>
        <br />
        <p>test 2</p>
        <p xmlns:default="http://www.w3.org/1999/xhtml" class="foo" id="import">namespace prefixed</p>
    </body>
</html>"#);
}

/// php-src `ext/dom/tests/modern/spec/NamedNodeMap_dimensions.phpt`: NamedNodeMap dimensions
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_named_node_map_dimensions() {
    if skip_without_xml_native("test_named_node_map_dimensions") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root a="1" b="2" c="3"></root>');
$attributes = $dom->documentElement->attributes;

$test_values = [-1, 0, 1, 2, 3, 1.0, 1.1, true, null, "0", "", "1", "8", "a", "b", "c", "d"];

foreach ($test_values as $value) {
    echo "--- ", json_encode($value), " ---\n";
    try {
        var_dump($attributes[$value] ? $attributes[$value]->nodeName : "N/A", isset($attributes[$value]), empty($attributes[$value]));
    } catch (Error $e) {
        echo $e->getMessage(), "\n";
    }
}

"#);
    assert_format(&out, r#"--- -1 ---
string(3) "N/A"
bool(false)
bool(true)
--- 0 ---
string(1) "a"
bool(true)
bool(false)
--- 1 ---
string(1) "b"
bool(true)
bool(false)
--- 2 ---
string(1) "c"
bool(true)
bool(false)
--- 3 ---
string(3) "N/A"
bool(false)
bool(true)
--- 1 ---
string(1) "b"
bool(true)
bool(false)
--- 1.1 ---

Deprecated: Implicit conversion from float 1.1 to int loses precision in %s on line %d

Deprecated: Implicit conversion from float 1.1 to int loses precision in %s on line %d

Deprecated: Implicit conversion from float 1.1 to int loses precision in %s on line %d

Deprecated: Implicit conversion from float 1.1 to int loses precision in %s on line %d
string(1) "b"
bool(true)
bool(false)
--- true ---
Cannot access offset of type bool on Dom\NamedNodeMap
--- null ---
Cannot access offset of type null on Dom\NamedNodeMap
--- "0" ---
string(1) "a"
bool(true)
bool(false)
--- "" ---
string(3) "N/A"
bool(false)
bool(true)
--- "1" ---
string(1) "b"
bool(true)
bool(false)
--- "8" ---
string(3) "N/A"
bool(false)
bool(true)
--- "a" ---
string(1) "a"
bool(true)
bool(false)
--- "b" ---
string(1) "b"
bool(true)
bool(false)
--- "c" ---
string(1) "c"
bool(true)
bool(false)
--- "d" ---
string(3) "N/A"
bool(false)
bool(true)"#);
}

/// php-src `ext/dom/tests/modern/spec/NamedNodeMap_dimensions_errors.phpt`: NamedNodeMap dimensions errors
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_named_node_map_dimensions_errors() {
    if skip_without_xml_native("test_named_node_map_dimensions_errors") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root a="1" b="2" c="3"></root>');
$attributes = $dom->documentElement->attributes;

try {
    $attributes[][0] = 1;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Cannot append to Dom\NamedNodeMap"#);
}

/// php-src `ext/dom/tests/modern/spec/NamedNodeMap_getNamedItem.phpt`: DOMNamedNodeMap::getNamedItem()
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_named_node_map_get_named_item() {
    if skip_without_xml_native("test_named_node_map_get_named_item") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE html>
<html>
<head>
    <title>Test</title>
</head>
<body align="foo" foo:bar="baz">
</body>
</html>
HTML);

echo "--- On HTML document ---\n";

$body = $dom->getElementsByTagName('body')->item(0);
$body->setAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns:foo", "http://example.com/foo");
$attributes = $body->attributes;

var_dump($attributes->getNamedItem("FOO:BAR")->value);
var_dump($attributes->getNamedItem("foo:BAR")->value);
var_dump($attributes->getNamedItem("foo:bar")->value);
var_dump($attributes->getNamedItem("XmLnS:foo")->value);
var_dump($attributes->getNamedItem("xmlns:foo")->value);

var_dump($attributes["FOO:BAR"]->value);
var_dump($attributes["foo:BAR"]->value);
var_dump($attributes["foo:bar"]->value);
var_dump($attributes["XmLnS:foo"]->value);
var_dump($attributes["xmlns:foo"]->value);

echo "--- On XML document ---\n";

$dom = Dom\XMLDocument::createEmpty();
$dom->appendChild($dom->importNode($body, true));

$body = $dom->getElementsByTagName('body')->item(0);
$attributes = $body->attributes;

var_dump($attributes->getNamedItem("FOO:BAR"));
var_dump($attributes->getNamedItem("foo:BAR"));
var_dump($attributes->getNamedItem("foo:bar")->value);
var_dump($attributes->getNamedItem("XmLnS:foo"));
var_dump($attributes->getNamedItem("xmlns:foo")->value);

var_dump($attributes["FOO:BAR"]);
var_dump($attributes["foo:BAR"]);
var_dump($attributes["foo:bar"]->value);
var_dump($attributes["XmLnS:foo"]);
var_dump($attributes["xmlns:foo"]->value);

"#);
    assert_exact(&out, r#"--- On HTML document ---
string(3) "baz"
string(3) "baz"
string(3) "baz"
string(22) "http://example.com/foo"
string(22) "http://example.com/foo"
string(3) "baz"
string(3) "baz"
string(3) "baz"
string(22) "http://example.com/foo"
string(22) "http://example.com/foo"
--- On XML document ---
NULL
NULL
string(3) "baz"
NULL
string(22) "http://example.com/foo"
NULL
NULL
string(3) "baz"
NULL
string(22) "http://example.com/foo""#);
}

/// php-src `ext/dom/tests/modern/spec/NodeList_dimensions.phpt`: NodeList dimensions
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_node_list_dimensions() {
    if skip_without_xml_native("test_node_list_dimensions") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root><a/><b/><c/></root>');
$children = $dom->documentElement->childNodes;

$test_values = [-1, 0, 1, 2, 3, 1.0, 1.1, true, null, "0", "1", "", "foo"];

foreach ($test_values as $value) {
    echo "--- ", json_encode($value), " ---\n";
    try {
        var_dump($children[$value] ? $children[$value]->nodeName : "N/A", isset($children[$value]), empty($children[$value]));
    } catch (Error $e) {
        echo $e->getMessage(), "\n";
    }
}

"#);
    assert_format(&out, r#"--- -1 ---
string(3) "N/A"
bool(false)
bool(true)
--- 0 ---
string(1) "a"
bool(true)
bool(false)
--- 1 ---
string(1) "b"
bool(true)
bool(false)
--- 2 ---
string(1) "c"
bool(true)
bool(false)
--- 3 ---
string(3) "N/A"
bool(false)
bool(true)
--- 1 ---
string(1) "b"
bool(true)
bool(false)
--- 1.1 ---

Deprecated: Implicit conversion from float 1.1 to int loses precision in %s on line %d

Deprecated: Implicit conversion from float 1.1 to int loses precision in %s on line %d

Deprecated: Implicit conversion from float 1.1 to int loses precision in %s on line %d

Deprecated: Implicit conversion from float 1.1 to int loses precision in %s on line %d
string(1) "b"
bool(true)
bool(false)
--- true ---
Cannot access offset of type bool on Dom\NodeList
--- null ---
Cannot access offset of type null on Dom\NodeList
--- "0" ---
string(1) "a"
bool(true)
bool(false)
--- "1" ---
string(1) "b"
bool(true)
bool(false)
--- "" ---
Cannot access offset of type string on Dom\NodeList
--- "foo" ---
Cannot access offset of type string on Dom\NodeList"#);
}

/// php-src `ext/dom/tests/modern/spec/NodeList_dimensions_errors.phpt`: NodeList dimensions errors
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_node_list_dimensions_errors() {
    if skip_without_xml_native("test_node_list_dimensions_errors") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root><a/><b/><c/></root>');
$children = $dom->documentElement->childNodes;

try {
    $children[][0] = 1;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Cannot append to Dom\NodeList"#);
}

/// php-src `ext/dom/tests/modern/spec/Node_appendChild_attribute.phpt`: Dom\Node::appendChild() with attribute should fail
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_node_append_child_attribute() {
    if skip_without_xml_native("test_node_append_child_attribute") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();

$element = $dom->createElement('foo');
$attr = $dom->createAttribute('bar');
$attr->value = "hello";
try {
    $element->appendChild($attr);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

var_dump($attr->value);

"#);
    assert_exact(&out, r#"Hierarchy Request Error
string(5) "hello""#);
}

/// php-src `ext/dom/tests/modern/spec/Node_appendChild_fragment_multiple_elements.phpt`: Dom\Node::appendChild() with fragment of multiple elements
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_node_append_child_fragment_multiple_elements() {
    if skip_without_xml_native("test_node_append_child_fragment_multiple_elements") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();

$fragment = $dom->createDocumentFragment();
$fragment->appendChild($dom->createElement('foo'));
$fragment->appendChild($dom->createElement('bar'));

try {
    $dom->appendChild($fragment);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo $dom->saveXml($fragment), "\n";

"#);
    assert_exact(&out, r#"Cannot have more than one element child in a document
<foo/><bar/>"#);
}

/// php-src `ext/dom/tests/modern/spec/Node_appendChild_invalid_parent.phpt`: Dom\Node::appendChild() invalid parent
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_node_append_child_invalid_parent() {
    if skip_without_xml_native("test_node_append_child_invalid_parent") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();

$text = $dom->createTextNode('Hello World');
$cdata = $dom->createCDATASection('Hello World');
$pi = $dom->createProcessingInstruction('Hello', '');
$attr = $dom->createAttribute('Hello');

try {
    $text->appendChild($dom->createElement('br'));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

try {
    $cdata->appendChild($dom->createElement('br'));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

try {
    $pi->appendChild($dom->createElement('br'));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

try {
    $attr->appendChild($dom->createElement('br'));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Hierarchy Request Error
Hierarchy Request Error
Hierarchy Request Error
Hierarchy Request Error"#);
}

/// php-src `ext/dom/tests/modern/spec/Node_baseURI.phpt`: Node::$baseURI
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_node_base_uri() {
    if skip_without_xml_native("test_node_base_uri") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$html = $dom->appendChild($dom->createElement('html'));
var_dump($html->baseURI);

$dom = Dom\HTMLDocument::createFromFile(__DIR__ . "/../../empty.html", LIBXML_NOERROR);
var_dump($dom->documentElement->baseURI);

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE html>
<html>
    <head>
        <base href="http://example.com/">
    </head>
</html>
HTML);
var_dump($dom->documentElement->baseURI);

"#);
    assert_format(&out, r#"string(11) "about:blank"
string(%d) "file://%sempty.html"
string(19) "http://example.com/""#);
}

/// php-src `ext/dom/tests/modern/spec/Node_cloneNode_copy_document_properties.phpt`: Dom\Node::cloneNode() should copy the document properties
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_node_clone_node_copy_document_properties() {
    if skip_without_xml_native("test_node_clone_node_copy_document_properties") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$dom = $dom->cloneNode();

$dom->appendChild($dom->createElement("foo"));

try {
    $dom->prepend($dom->createElement("bar"));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Cannot have more than one element child in a document"#);
}

/// php-src `ext/dom/tests/modern/spec/Node_getRootNode.phpt`: Dom\Node::getRootNode()
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMElement::getRootNode"#]
fn test_node_get_root_node() {
    if skip_without_xml_native("test_node_get_root_node") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

// Same as DOMNode_getRootNode.phpt but for modern DOM

$dom = Dom\XMLDocument::createFromString('<?xml version="1.0"?><html><body/></html>');

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

/// php-src `ext/dom/tests/modern/spec/Node_insertBefore_01.phpt`: Dom\Node::insertBefore() with DocumentFragment and dtd
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_node_insert_before_01() {
    if skip_without_xml_native("test_node_insert_before_01") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString('<!DOCTYPE html><html><head></head><body></body></html>');
$dom->documentElement->remove();

$fragment = $dom->createDocumentFragment();
$fragment->appendChild($dom->createElement('a'));

try {
    $dom->insertBefore($fragment, $dom->doctype);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo $dom->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"Document types must be the first child in a document
<!DOCTYPE html>"#);
}

/// php-src `ext/dom/tests/modern/spec/Node_insertBefore_02.phpt`: Dom\Node::insertBefore() with DocumentFragment and a document element
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_node_insert_before_02() {
    if skip_without_xml_native("test_node_insert_before_02") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString('<!DOCTYPE html><html><head></head><body></body></html>');

$fragment = $dom->createDocumentFragment();
$fragment->appendChild($dom->createElement('a'));

try {
    $dom->insertBefore($fragment, $dom->documentElement);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo $dom->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"Cannot have more than one element child in a document
<!DOCTYPE html><html><head></head><body></body></html>"#);
}

/// php-src `ext/dom/tests/modern/spec/Node_isDefaultNamespace.phpt`: Dom\Node::isDefaultNamespace()
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_node_is_default_namespace() {
    if skip_without_xml_native("test_node_is_default_namespace") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE html>
<html>
    <body>
        <svg xmlns="http://www.w3.org/2000/svg" height="1"></svg>
        <math></math>
    </body>
</html>
HTML);

function dump($node) {
    echo "svg NS: ";
    var_dump($node->isDefaultNamespace('http://www.w3.org/2000/svg'));
    echo "xhtml NS: ";
    var_dump($node->isDefaultNamespace('http://www.w3.org/1999/xhtml'));
    echo "mathml NS: ";
    var_dump($node->isDefaultNamespace('http://www.w3.org/1998/Math/MathML'));
    echo "empty NS: ";
    var_dump($node->isDefaultNamespace(''));
}

echo "--- Document ---\n";
dump($dom);
echo "--- svg element ---\n";
dump($dom->getElementsByTagName('svg')[0]);
echo "--- math element ---\n";
dump($dom->getElementsByTagName('math')[0]);
echo "--- svg attribute ---\n";
dump($dom->getElementsByTagName('svg')[0]->attributes[0]);
echo "--- empty fragment ---\n";
dump($dom->createDocumentFragment());
echo "--- doctype ---\n";
dump($dom->doctype);
echo "--- detached attribute ---\n";
dump($dom->createAttribute('foo'));
echo "--- custom namespace with a prefix ---\n";
var_dump($dom->createElementNS('urn:a', 'q:name')->isDefaultNamespace('urn:a'));
echo "--- custom namespace with a prefix and with attributes ---\n";
$element = $dom->createElementNS('urn:a', 'q:name');
$element->setAttributeNS("urn:x", "x:foo", "");
$element->setAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns:q", "urn:test");
$element->setAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns", "urn:test2");
var_dump($element->isDefaultNamespace('urn:a'));
var_dump($element->isDefaultNamespace('urn:test'));
var_dump($element->isDefaultNamespace('urn:test2'));
echo "--- custom namespace without a prefix ---\n";
var_dump($dom->createElementNS('urn:a', 'name')->isDefaultNamespace('urn:a'));

"#);
    assert_exact(&out, r#"--- Document ---
svg NS: bool(false)
xhtml NS: bool(true)
mathml NS: bool(false)
empty NS: bool(false)
--- svg element ---
svg NS: bool(true)
xhtml NS: bool(false)
mathml NS: bool(false)
empty NS: bool(false)
--- math element ---
svg NS: bool(false)
xhtml NS: bool(false)
mathml NS: bool(true)
empty NS: bool(false)
--- svg attribute ---
svg NS: bool(true)
xhtml NS: bool(false)
mathml NS: bool(false)
empty NS: bool(false)
--- empty fragment ---
svg NS: bool(false)
xhtml NS: bool(false)
mathml NS: bool(false)
empty NS: bool(true)
--- doctype ---
svg NS: bool(false)
xhtml NS: bool(false)
mathml NS: bool(false)
empty NS: bool(true)
--- detached attribute ---
svg NS: bool(false)
xhtml NS: bool(false)
mathml NS: bool(false)
empty NS: bool(true)
--- custom namespace with a prefix ---
bool(false)
--- custom namespace with a prefix and with attributes ---
bool(false)
bool(false)
bool(true)
--- custom namespace without a prefix ---
bool(true)"#);
}

/// php-src `ext/dom/tests/modern/spec/Node_isEqualNode_01.phpt`: Dom\Node::isEqualNode edge cases 01
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_node_is_equal_node_01() {
    if skip_without_xml_native("test_node_is_equal_node_01") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$dom->append($dom->createElement("container"));
$e1 = $dom->documentElement->appendChild($dom->createElementNS("urn:example1", "example:foo"));
$e1->after("\n");
$e2 = $dom->documentElement->appendChild($dom->createElementNS("urn:example2", "example:foo"));

$e2->setAttributeNS("urn:example2", "example2:bar", "baz1");
$e2->setAttributeNS("urn:example1", "example2:bar", "baz2");

$dom2 = Dom\HTMLDocument::createFromString("<!DOCTYPE HTML>" . $dom->saveHtml());

// Shouldn't match because the attributes can't roundtrip.
var_dump($dom->saveHtml($dom->getElementsByTagName("container")[0]));
var_dump($dom2->saveHtml($dom2->getElementsByTagName("container")[0]));
var_dump($dom->getElementsByTagName("container")[0]->isEqualNode($dom2->getElementsByTagName("container")[0]));

"#);
    assert_exact(&out, r#"string(118) "<container><example:foo></example:foo>
<example:foo example2:bar="baz1" example2:bar="baz2"></example:foo></container>"
string(98) "<container><example:foo></example:foo>
<example:foo example2:bar="baz1"></example:foo></container>"
bool(false)"#);
}

/// php-src `ext/dom/tests/modern/spec/Node_isEqualNode_02.phpt`: Dom\Node::isEqualNode edge cases 02
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_node_is_equal_node_02() {
    if skip_without_xml_native("test_node_is_equal_node_02") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$dom->append($dom->createElement("container"));
$e1 = $dom->documentElement->appendChild($dom->createElement("foo1"));
$e1->after("\n");
$e2 = $dom->documentElement->appendChild($dom->createElement("foo2"));

$e2->setAttribute("bar1", "baz");
$e2->setAttribute("bar2", "baz");
// Internal namespace node creation
$e2->setAttributeNodeNS($child = $dom->createAttributeNS("http://example.com", "foo:bar"));
$e2->removeAttributeNode($child);

$dom2 = Dom\HTMLDocument::createFromString("<!DOCTYPE HTML>" . $dom->saveHtml());

var_dump($dom->saveHtml($dom->getElementsByTagName("container")[0]));
var_dump($dom2->saveHtml($dom2->getElementsByTagName("container")[0]));
var_dump($dom->getElementsByTagName("container")[0]->isEqualNode($dom2->getElementsByTagName("container")[0]));

"#);
    assert_exact(&out, r#"string(72) "<container><foo1></foo1>
<foo2 bar1="baz" bar2="baz"></foo2></container>"
string(72) "<container><foo1></foo1>
<foo2 bar1="baz" bar2="baz"></foo2></container>"
bool(true)"#);
}

/// php-src `ext/dom/tests/modern/spec/Node_lookupNamespaceURI.phpt`: Dom\Node::lookupNamespaceURI()
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_node_lookup_namespace_uri() {
    if skip_without_xml_native("test_node_lookup_namespace_uri") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE html>
<html>
    <body>
        <svg xmlns="http://www.w3.org/2000/svg" height="1"></svg>
        <math></math>
    </body>
</html>
HTML);

$body = $dom->getElementsByTagName("body")[0];
$namespaceless = $body->appendChild($dom->createElementNS(NULL, "foo"));
$prefixed = $body->appendChild($dom->createElementNS("urn:a", "a:a"));

echo "--- Hardcoded prefixes ---\n";
var_dump($dom->lookupNamespaceURI("xml"));
var_dump($dom->lookupNamespaceURI("xmlns"));

echo "--- Default prefix ---\n";
var_dump($dom->lookupNamespaceURI(""));
var_dump($dom->lookupNamespaceURI(NULL));

echo "--- NULL namespace should propagate up ---\n";
var_dump($namespaceless->lookupNamespaceURI(""));
var_dump($namespaceless->lookupNamespaceURI(NULL));
var_dump($namespaceless->lookupNamespaceURI("a"));

echo "--- Prefixed element ---\n";
var_dump($prefixed->lookupNamespaceURI(""));
var_dump($prefixed->lookupNamespaceURI(NULL));
var_dump($prefixed->lookupNamespaceURI("a"));

echo "--- Prefixed element custom xmlns attribute should not change ns ---\n";
$prefixed->setAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns:a", "urn:another");
var_dump($prefixed->lookupNamespaceURI(""));
var_dump($prefixed->lookupNamespaceURI(NULL));
var_dump($prefixed->lookupNamespaceURI("a"));

echo "--- xmlns attribute defines new namespace ---\n";
$body->setAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns:a", "urn:another");
var_dump($body->lookupNamespaceURI(""));
var_dump($body->lookupNamespaceURI(NULL));
var_dump($body->lookupNamespaceURI("a"));

echo "--- empty xmlns attribute defines no new namespace ---\n";
$body->setAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns:a", "");
var_dump($body->lookupNamespaceURI(""));
var_dump($body->lookupNamespaceURI(NULL));
var_dump($body->lookupNamespaceURI("a"));

echo "--- lookup in empty document ---\n";
$dom = Dom\HTMLDocument::createEmpty();
var_dump($dom->lookupNamespaceURI(""));
var_dump($dom->lookupNamespaceURI(NULL));
var_dump($dom->lookupNamespaceURI("a"));

"#);
    assert_exact(&out, r#"--- Hardcoded prefixes ---
string(36) "http://www.w3.org/XML/1998/namespace"
string(29) "http://www.w3.org/2000/xmlns/"
--- Default prefix ---
string(28) "http://www.w3.org/1999/xhtml"
string(28) "http://www.w3.org/1999/xhtml"
--- NULL namespace should propagate up ---
string(28) "http://www.w3.org/1999/xhtml"
string(28) "http://www.w3.org/1999/xhtml"
NULL
--- Prefixed element ---
string(28) "http://www.w3.org/1999/xhtml"
string(28) "http://www.w3.org/1999/xhtml"
string(5) "urn:a"
--- Prefixed element custom xmlns attribute should not change ns ---
string(28) "http://www.w3.org/1999/xhtml"
string(28) "http://www.w3.org/1999/xhtml"
string(5) "urn:a"
--- xmlns attribute defines new namespace ---
string(28) "http://www.w3.org/1999/xhtml"
string(28) "http://www.w3.org/1999/xhtml"
string(11) "urn:another"
--- empty xmlns attribute defines no new namespace ---
string(28) "http://www.w3.org/1999/xhtml"
string(28) "http://www.w3.org/1999/xhtml"
NULL
--- lookup in empty document ---
NULL
NULL
NULL"#);
}

/// php-src `ext/dom/tests/modern/spec/Node_lookupPrefix.phpt`: Dom\Node::lookupPrefix()
#[test]
#[ignore = r#"DOM port pending: Cannot index non-array"#]
fn test_node_lookup_prefix() {
    if skip_without_xml_native("test_node_lookup_prefix") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString(<<<XML
<?xml version="1.0"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:x="test">
    <body>
        <svg:svg xmlns:svg="http://www.w3.org/2000/svg" height="1"/>
        <p xmlns:y="test">
            <x/>
        </p>
    </body>
</html>
XML);

$body = $dom->getElementsByTagName("body")[0];
$body->setAttribute("xmlns:a", "urn:a");

echo "--- NULL case because invalid node type ---\n";

var_dump($dom->doctype->lookupPrefix(""));

echo "--- NULL case because xmlns attribute not in xmlns namespace ---\n";

var_dump($body->lookupPrefix("urn:a"));

echo "--- svg case ---\n";

$svg = $dom->getElementsByTagNameNS("*", "svg")[0];

var_dump($svg->lookupPrefix(""));
var_dump($svg->lookupPrefix("http://www.w3.org/2000/svg"));
var_dump($svg->lookupPrefix("1"));

echo "--- search for \"test\" ---\n";

foreach (['x', 'p', 'html'] as $name) {
    $x = $dom->getElementsByTagNameNS("*", $name)[0];
    var_dump($x->lookupPrefix(""));
    var_dump($x->lookupPrefix("test"));
}

"#);
    assert_exact(&out, r#"--- NULL case because invalid node type ---
NULL
--- NULL case because xmlns attribute not in xmlns namespace ---
NULL
--- svg case ---
NULL
string(3) "svg"
NULL
--- search for "test" ---
NULL
string(1) "y"
NULL
string(1) "y"
NULL
string(1) "x""#);
}

/// php-src `ext/dom/tests/modern/spec/Node_nodeName.phpt`: Dom\Node::$nodeName
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_node_node_name() {
    if skip_without_xml_native("test_node_node_name") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

echo "--- For HTML document ---\n";

$dom = Dom\HTMLDocument::createEmpty();

$dom->appendChild($dom->createElement('container'));
var_dump($dom->createAttributeNS('http://www.w3.org/1999/xhtml', 'html')->nodeName);
var_dump($dom->createAttributeNS('http://www.w3.org/1999/xhtml', 'foo:html')->nodeName);

echo "\n";

var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml', 'html')->nodeName);
var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml/', 'html')->nodeName);
var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml', 'foo:html')->nodeName);
var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml/', 'foo:html')->nodeName);
var_dump($dom->createElementNS('', 'html')->nodeName);
var_dump($dom->createElementNS(null, 'html')->nodeName);

echo "--- For XML document ---\n";

$dom = Dom\XMLDocument::createEmpty();

$dom->appendChild($dom->createElement('container'));
var_dump($dom->createAttributeNS('http://www.w3.org/1999/xhtml', 'html')->nodeName);
var_dump($dom->createAttributeNS('http://www.w3.org/1999/xhtml', 'foo:html')->nodeName);

echo "\n";

var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml', 'html')->nodeName);
var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml/', 'html')->nodeName);
var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml', 'foo:html')->nodeName);
var_dump($dom->createElementNS('http://www.w3.org/1999/xhtml/', 'foo:html')->nodeName);
var_dump($dom->createElementNS('', 'html')->nodeName);
var_dump($dom->createElementNS(null, 'html')->nodeName);
"#);
    assert_exact(&out, r#"--- For HTML document ---
string(4) "html"
string(8) "foo:html"

string(4) "HTML"
string(4) "html"
string(8) "FOO:HTML"
string(8) "foo:html"
string(4) "html"
string(4) "html"
--- For XML document ---
string(4) "html"
string(8) "foo:html"

string(4) "html"
string(4) "html"
string(8) "foo:html"
string(8) "foo:html"
string(4) "html"
string(4) "html""#);
}

/// php-src `ext/dom/tests/modern/spec/Node_nodeValue_element.phpt`: Dom\Node::$nodeValue on an element
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_node_node_value_element() {
    if skip_without_xml_native("test_node_node_value_element") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$foo = $dom->createElement("foo");
$foo->textContent = "bar";
var_dump($foo->nodeValue);

"#);
    assert_exact(&out, r#"NULL"#);
}

/// php-src `ext/dom/tests/modern/spec/Node_normalize.phpt`: Dom\Node::normalize()
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_node_normalize() {
    if skip_without_xml_native("test_node_normalize") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

// This is a copy of the non-spec test "DOMNode_normalize_basic.php".

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

$doc = Dom\XMLDocument::createEmpty();

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

/// php-src `ext/dom/tests/modern/spec/Node_normalize_edge_case.phpt`: Dom\Node::normalize() edge case
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_node_normalize_edge_case() {
    if skip_without_xml_native("test_node_normalize_edge_case") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$doc = Dom\XMLDocument::createEmpty();
$container = $doc->appendChild($doc->createElement("container"));

$container->appendChild($doc->createTextNode(""));
$middle = $container->appendChild($doc->createTextNode("foo"));
$container->appendChild($doc->createTextNode(""));

$doc->normalize();

echo $doc->saveXml(), "\n";

var_dump($middle->textContent);

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<container>foo</container>
string(3) "foo""#);
}

/// php-src `ext/dom/tests/modern/spec/Node_replaceChild_edge_cases.phpt`: Dom\Node::replaceChild() edge cases
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_node_replace_child_edge_cases() {
    if skip_without_xml_native("test_node_replace_child_edge_cases") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString("<!DOCTYPE html><html></html>");
$dom->documentElement->remove();
$parent = $dom->createElement("parent");
$child = $dom->createElement("child");
$parent->appendChild($child);
$dom->appendChild($parent);

echo "--- Wrong parent node type ---\n";

$comment = $dom->createComment('This is a comment');
try {
    $comment->replaceChild($comment, $dom->createElement("old-child"));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Node is an inclusive ancestor of parent ---\n";

try {
    $parent->replaceChild($parent, $child);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

try {
    $parent->replaceChild($dom, $child);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Child's parent is not parent ---\n";

try {
    $parent->replaceChild($dom->createElement("new-child"), $dom->createElement("old-child"));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Invalid child to replace with ---\n";

try {
    $entityReference = $dom->importNode(Dom\XMLDocument::createEmpty()->createEntityReference("foo"));
    $parent->replaceChild($entityReference, $child);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Replace element with text in document root ---\n";

try {
    $dom->replaceChild($dom->createTextNode("text"), $parent);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Replace child element with doctype inside element ---\n";

try {
    $parent->replaceChild($dom->doctype, $child);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Replace element with fragment containing multiple elements ---\n";

$fragment = $dom->createDocumentFragment();
$fragment->appendChild($dom->createElement("new-child1"));
$fragment->appendChild($dom->createElement("new-child2"));

try {
    $dom->replaceChild($fragment, $parent);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Replace comment in document causing more than two elements ---\n";

$comment = $dom->appendChild($dom->createComment("comment"));
try {
    $dom->replaceChild($dom->createElement("new-child"), $comment);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Replace dtd with element ---\n";

try {
    $dom->replaceChild($dom->createElement("new-child"), $dom->doctype);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Replace element with another dtd ---\n";

try {
    $dom->replaceChild($dom->doctype, $parent);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- Replace parent with itself ---\n";

$dom->replaceChild($parent, $parent);
echo $dom->saveHtml(), "\n";

echo "--- Replace parent with single-child fragment ---\n";

$fragment = $dom->createDocumentFragment();
$fragment->appendChild($dom->createElement("new-child"));
$dom->replaceChild($fragment, $parent);
echo $dom->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"--- Wrong parent node type ---
Hierarchy Request Error
--- Node is an inclusive ancestor of parent ---
Hierarchy Request Error
Hierarchy Request Error
--- Child's parent is not parent ---
Not Found Error
--- Invalid child to replace with ---
Hierarchy Request Error
--- Replace element with text in document root ---
Cannot insert text as a child of a document
--- Replace child element with doctype inside element ---
Cannot insert a document type into anything other than a document
--- Replace element with fragment containing multiple elements ---
Cannot have more than one element child in a document
--- Replace comment in document causing more than two elements ---
Cannot have more than one element child in a document
--- Replace dtd with element ---
Cannot have more than one element child in a document
--- Replace element with another dtd ---
Document types must be the first child in a document
--- Replace parent with itself ---
<!DOCTYPE html><parent><child></child></parent><!--comment-->
--- Replace parent with single-child fragment ---
<!DOCTYPE html><new-child></new-child><!--comment-->"#);
}

/// php-src `ext/dom/tests/modern/spec/ParentNode_append_exception_consistency.phpt`: Consistency of parent node after exception in ParentNode::append()
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_parent_node_append_exception_consistency() {
    if skip_without_xml_native("test_parent_node_append_exception_consistency") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php
$dom = Dom\XMLDocument::createEmpty();
$fragment = $dom->createDocumentFragment();
$fragment->append($element = $dom->createElement("foo"));
$fragment->append($dom->createElement("bar"));
try {
    $dom->append($fragment);
} catch (DOMException $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}

var_dump($element->parentNode);
"#);
    assert_exact(&out, r##"Exception: Cannot have more than one element child in a document
object(Dom\DocumentFragment)#2 (18) {
  ["children"]=>
  string(22) "(object value omitted)"
  ["firstElementChild"]=>
  string(22) "(object value omitted)"
  ["lastElementChild"]=>
  string(22) "(object value omitted)"
  ["childElementCount"]=>
  int(2)
  ["nodeType"]=>
  int(11)
  ["nodeName"]=>
  string(18) "#document-fragment"
  ["baseURI"]=>
  string(11) "about:blank"
  ["isConnected"]=>
  bool(false)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
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
  ["nodeValue"]=>
  NULL
  ["textContent"]=>
  string(0) ""
}"##);
}

/// php-src `ext/dom/tests/modern/spec/ParentNode_edge_case.phpt`: HierarchyRequestError on appending document to element
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_parent_node_edge_case() {
    if skip_without_xml_native("test_parent_node_edge_case") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$container = $dom->createElement('container');
try {
    $container->append($dom);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Hierarchy Request Error"#);
}

/// php-src `ext/dom/tests/modern/spec/ParentNode_hierarchy_text_nodes.phpt`: ParentNode hierarchy exceptions with temporary and non-temporary text nodes
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_parent_node_hierarchy_text_nodes() {
    if skip_without_xml_native("test_parent_node_hierarchy_text_nodes") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();

try {
    $dom->append("bar");
} catch (DOMException $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}

try {
    $dom->append($dom->createTextNode("bar"));
} catch (DOMException $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}

$text = $dom->createTextNode("bar");
try {
    $dom->append($text);
} catch (DOMException $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}

var_dump($text->parentNode);
var_dump($text->textContent);

$element = $dom->createElement("container");
$text = $element->appendChild($dom->createTextNode("text"));
try {
    $dom->append($text);
} catch (DOMException $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}

"#);
    assert_exact(&out, r#"Exception: Cannot insert text as a child of a document
Exception: Cannot insert text as a child of a document
Exception: Cannot insert text as a child of a document
NULL
string(3) "bar"
Exception: Cannot insert text as a child of a document"#);
}

/// php-src `ext/dom/tests/modern/spec/ProcessingInstruction_data_manipulation.phpt`: Test processing instruction character data manipulation
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_processing_instruction_data_manipulation() {
    if skip_without_xml_native("test_processing_instruction_data_manipulation") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root><?pi value?></root>');

$pi = $dom->documentElement->firstChild;

echo $pi->substringData(0, 3), "\n";
$pi->appendData('foobar');
echo $pi->textContent, "\n";
$pi->insertData(6, 'oooooo');
echo $pi->textContent, "\n";
$pi->deleteData(0, strlen('value'));
echo $pi->textContent, "\n";
$pi->replaceData(1, 8, 'oo');
echo $pi->textContent, "\n";

"#);
    assert_exact(&out, r#"val
valuefoobar
valuefoooooooobar
foooooooobar
foobar"#);
}

/// php-src `ext/dom/tests/modern/spec/Text_splitText_edge_case.phpt`: Dom\Text:splitText() edge case
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_text_split_text_edge_case() {
    if skip_without_xml_native("test_text_split_text_edge_case") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php
$dom = Dom\HTMLDocument::createEmpty();
$text = $dom->createTextNode('Hello World!');
var_dump($text->splitText(5)->wholeText);
try {
    var_dump($text->splitText(-1));
} catch (ValueError $e) {
  echo $e->getMessage(), "\n";
}
try {
    var_dump($text->splitText(200));
} catch (DOMException $e) {
  echo $e->getMessage(), "\n";
}
"#);
    assert_exact(&out, r#"string(7) " World!"
Dom\Text::splitText(): Argument #1 ($offset) must be greater than or equal to 0
Index Size Error"#);
}

/// php-src `ext/dom/tests/modern/spec/XMLDocument_getElementsByTagName.phpt`: XMLDocument::getElementsByTagName
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_xmldocument_get_elements_by_tag_name() {
    if skip_without_xml_native("test_xmldocument_get_elements_by_tag_name") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

require __DIR__ . "/node_list_dump.inc";
require __DIR__ . "/create_element_util.inc";

$dom = Dom\XMLDocument::createEmpty();
$container = $dom->appendChild(createElement($dom, "container"));
$container->appendChild(createElement($dom, "HTML", "1"));
$container->appendChild(createElementNS($dom, "http://www.w3.org/1999/xhtml", "html", "2"));
$container->appendChild(createElementNS($dom, NULL, "html", "3"));
$container->appendChild(createElementNS($dom, NULL, "HTML", "4"));
$container->appendChild(createElementNS($dom, "urn:foo", "htML", "5"));
$container->appendChild(createElement($dom, "foo:HTML", "6"));
$container->appendChild(createElementNS($dom, "urn:a", "foo:HTML", "7")); // Should never match in this test file
$container->appendChild(createElementNS($dom, "http://www.w3.org/1999/xhtml", "bar:HTML", "8"));
$container->appendChild(createElementNS($dom, "http://www.w3.org/1999/xhtml", "bar:html", "9"));

dumpNodeList($dom->getElementsByTagName("HTml"));
dumpNodeList($dom->getElementsByTagName("htML"));
dumpNodeList($dom->getElementsByTagName("html"));
dumpNodeList($dom->getElementsByTagName("HTML"));
dumpNodeList($dom->getElementsByTagName("foo:html"));
dumpNodeList($dom->getElementsByTagName("foo:HTML"));
dumpNodeList($dom->getElementsByTagName("bar:HTML"));
dumpNodeList($dom->getElementsByTagName("bar:html"));

"#);
    assert_exact(&out, r#"Node list length: int(0)
---
Node list length: int(1)
	htML 5
---
Node list length: int(2)
	html 2
	html 3
---
Node list length: int(2)
	HTML 1
	HTML 4
---
Node list length: int(0)
---
Node list length: int(2)
	foo:HTML 6
	foo:HTML 7
---
Node list length: int(1)
	bar:HTML 8
---
Node list length: int(1)
	bar:html 9
---"#);
}

/// php-src `ext/dom/tests/modern/spec/XMLDocument_version.phpt`: XMLDocument::$version
#[test]
#[ignore = r#"DOM port pending: Undefined property: Dom\XMLDocument::xmlVersion"#]
fn test_xmldocument_version() {
    if skip_without_xml_native("test_xmldocument_version") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root><child/></root>');
var_dump($dom->xmlVersion);

foreach (['0.1', '1.0', '1.1', '', 'foo'] as $version) {
    try {
        $dom->xmlVersion = $version;
    } catch (ValueError $e) {
        echo $e->getMessage(), "\n";
    }
    var_dump($dom->xmlVersion);
}

"#);
    assert_exact(&out, r#"string(3) "1.0"
Invalid XML version
string(3) "1.0"
string(3) "1.0"
string(3) "1.1"
Invalid XML version
string(3) "1.1"
Invalid XML version
string(3) "1.1""#);
}

/// php-src `ext/dom/tests/modern/spec/appendChild_dtd_legacy.phpt`: appendChild() with DocumentType
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_append_child_dtd_legacy() {
    if skip_without_xml_native("test_append_child_dtd_legacy") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$original = new DOMDocument();
$original->loadXML(<<<XML
<?xml version="1.0"?>
<!DOCTYPE doc [
    <!ENTITY foo "bar">
]>
<doc/>
XML);
$doctype = $original->doctype->cloneNode();
foreach ($doctype->entities as $entity) {
    echo "Found entity: ", $entity->nodeName, "\n";
}

$other = new DOMDocument();
$doctype = $other->importNode($original->doctype);
$other->appendChild($doctype);
$other->appendChild($doctype);
try {
    $other->appendChild($other->implementation->createDocumentType('doc', '', ''));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $other->saveXml();

"#);
    assert_exact(&out, r#"Found entity: foo
A document may only contain one document type
<?xml version="1.0"?>
<!DOCTYPE doc [
<!ENTITY foo "bar">
]>"#);
}

/// php-src `ext/dom/tests/modern/spec/append_text_nodes_invalid_hierarchy.phpt`: Document::append() with text nodes in an invalid hierarchy
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_append_text_nodes_invalid_hierarchy() {
    if skip_without_xml_native("test_append_text_nodes_invalid_hierarchy") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();

try {
    $dom->append("foo", "bar", "baz", $dom);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

try {
    $dom->append("foo", "bar", "baz");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

var_dump($dom->saveHtml());

"#);
    assert_exact(&out, r#"Hierarchy Request Error
Cannot insert text as a child of a document
string(0) """#);
}

/// php-src `ext/dom/tests/modern/spec/attribute_entity_expansion.phpt`: Attribute entity expansion
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_attribute_entity_expansion() {
    if skip_without_xml_native("test_attribute_entity_expansion") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php
$doc = Dom\HTMLDocument::createEmpty();
$elt = $doc->createElement('elt');
$doc->appendChild($elt);
$elt->setAttribute('a','&');
print $doc->saveHtml($elt) . "\n";

$attr = $elt->getAttributeNode('a');
$attr->value = '&amp;';
print "$attr->value\n";
print $doc->saveHtml($elt) . "\n";

$attr->removeChild($attr->firstChild);
print $doc->saveHtml($elt) . "\n";

$attr->nodeValue = '&';
print "$attr->nodeValue\n";
print $doc->saveHtml($elt) . "\n";

$attr->nodeValue = '&amp;';
print "$attr->nodeValue\n";
print $doc->saveHtml($elt) . "\n";

$elt->removeAttributeNode($attr);
$elt->setAttributeNS('http://www.w3.org/2000/svg', 'svg:id','&amp;');
print $doc->saveHtml($elt) . "\n";

$attr = $elt->getAttributeNodeNS('http://www.w3.org/2000/svg', 'id');
$attr->value = '&lt;&amp;';
print "$attr->value\n";
print $doc->saveHtml($elt) . "\n";
"#);
    assert_exact(&out, r#"<elt a="&amp;"></elt>
&amp;
<elt a="&amp;amp;"></elt>
<elt a=""></elt>
&
<elt a="&amp;"></elt>
&amp;
<elt a="&amp;amp;"></elt>
<elt svg:id="&amp;amp;"></elt>
&lt;&amp;
<elt svg:id="&amp;lt;&amp;amp;"></elt>"#);
}

/// php-src `ext/dom/tests/modern/spec/attribute_entity_expansion_legacy.phpt`: Attribute entity expansion in a legacy document
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMElement::getAttributeNode"#]
fn test_attribute_entity_expansion_legacy() {
    if skip_without_xml_native("test_attribute_entity_expansion_legacy") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php
$doc = new DOMDocument;
$elt = $doc->createElement('elt');
$doc->appendChild($elt);
$elt->setAttribute('a','&');
print $doc->saveXml($elt) . "\n";

$attr = $elt->getAttributeNode('a');
$attr->value = '&amp;';
print "$attr->value\n";
print $doc->saveXml($elt) . "\n";

$attr->removeChild($attr->firstChild);
print $doc->saveXml($elt) . "\n";

// Note: since libxml2 commit aca16fb3d45e0b2c45364ffc1cea8eb4abaca87d this no longer explicitly warns. This seems intentional.
@$attr->nodeValue = '&';
print "$attr->nodeValue\n";
print $doc->saveXml($elt) . "\n";

$attr->nodeValue = '&amp;';
print "$attr->nodeValue\n";
print $doc->saveXml($elt) . "\n";

$elt->removeAttributeNode($attr);
$elt->setAttributeNS('http://www.w3.org/2000/svg', 'svg:id','&amp;');
print $doc->saveXml($elt) . "\n";

$attr = $elt->getAttributeNodeNS('http://www.w3.org/2000/svg', 'id');
$attr->value = '&lt;&amp;';
print "$attr->value\n";
print $doc->saveXml($elt) . "\n";
"#);
    assert_format(&out, r#"<elt a="&amp;"/>
&
<elt a="&amp;"/>
<elt a=""/>

<elt a=""/>
&
<elt a="&amp;"/>
<elt xmlns:svg="http://www.w3.org/2000/svg" svg:id="&amp;amp;"/>
<&
<elt xmlns:svg="http://www.w3.org/2000/svg" svg:id="&lt;&amp;"/>"#);
}

/// php-src `ext/dom/tests/modern/spec/bug47530.phpt`: Bug #47530 (Importing objects into document fragments creates bogus "default" namespace)
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createDocumentFragment"#]
fn test_bug47530() {
    if skip_without_xml_native("test_bug47530") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

function test_document_fragment_with_import() {
    $doc = Dom\XMLDocument::createFromString('<html xmlns="https://php.net/something" xmlns:ns="https://php.net/whatever"><element ns:foo="https://php.net/bar"/></html>');
    $root = $doc->documentElement;
    $frag = $doc->createDocumentFragment();
    $frag->appendChild($doc->importNode($root->firstChild));
    $root->appendChild($frag);
    echo $doc->saveXml(), "\n";
}

function test_document_fragment_without_import() {
    $doc = Dom\XMLDocument::createFromString('<html xmlns=""><element xmlns:foo="https://php.net/bar"/></html>');
    $frag = $doc->createDocumentFragment();
    $frag->appendChild($doc->createElementNS('https://php.net/bar', 'bar'));
    $frag->appendChild($doc->createElementNS('', 'bar'));
    $element = $doc->documentElement->firstChild;
    $element->appendChild($frag);
    unset($frag); // Free fragment, should not break getting the namespaceURI below
    echo $doc->saveXml(), "\n";
    unset($doc);
    var_dump($element->firstChild->tagName);
    var_dump($element->firstChild->namespaceURI);
    var_dump($element->firstChild->nextSibling->tagName);
    var_dump($element->firstChild->nextSibling->namespaceURI);
}

function test_document_import() {
    $xml = <<<XML
<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
<div xmlns="http://www.w3.org/1999/xhtml">
    <p>Test-Text</p>
</div>
</feed>
XML;

    $dom = Dom\XMLDocument::createFromString($xml);

    $dom2 = Dom\XMLDocument::createEmpty();
    $importedNode = $dom2->importNode($dom->documentElement, true);
    $dom2->appendChild($importedNode);

    echo $dom2->saveXml(), "\n";
}

function test_partial_document_import() {
    $xml = <<<XML
<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/1999/xhtml" xmlns:test="https://php.net/test" xmlns:example="https://php.net/example">
<div>
    <p>Test-Text</p>
    <example:p>More test text</example:p>
    <test:p>Even more test text</test:p>
</div>
</feed>
XML;

    $dom = Dom\XMLDocument::createFromString($xml);

    $dom2 = Dom\XMLDocument::createFromString('<?xml version="1.0"?><container xmlns:test="https://php.net/test" xmlns="https://php.net/example"/>');
    $importedNode = $dom2->importNode($dom->documentElement, true);
    $dom2->documentElement->appendChild($importedNode);

    // Freeing the original document shouldn't break the other document
    unset($importedNode);
    unset($dom);

    echo $dom2->saveXml(), "\n";
}

function test_document_import_with_attributes() {
    $dom = Dom\XMLDocument::createFromString('<?xml version="1.0"?><div xmlns="https://php.net/default" xmlns:example="https://php.net/example"><p example:test="test"/><i/></div>');
    $dom2 = Dom\XMLDocument::createFromString('<?xml version="1.0"?><div xmlns:example="https://php.net/somethingelse"/>');
    $dom2->documentElement->appendChild($dom2->importNode($dom->documentElement->firstChild));
    echo $dom2->saveXml(), "\n";

    $dom2->documentElement->firstChild->appendChild($dom2->importNode($dom->documentElement->firstChild->nextSibling));
    echo $dom2->saveXml(), "\n";
}

function test_appendChild_with_shadowing() {
    $dom = Dom\XMLDocument::createFromString('<?xml version="1.0"?><container xmlns:default="http://php.net/default"><a xmlns:foo="http://php.net/bar"/><b xmlns:foo="http://php.net/foo"><default:test foo:bar=""/><foo:test2/></b></container>');

    $a = $dom->documentElement->firstElementChild;
    $b = $a->nextSibling;
    $b->remove();
    $a->appendChild($b);

    echo $dom->saveXml(), "\n";
}

echo "-- Test document fragment with import --\n";
test_document_fragment_with_import();
echo "-- Test document fragment without import --\n";
test_document_fragment_without_import();
echo "-- Test document import --\n";
test_document_import();
echo "-- Test partial document import --\n";
test_partial_document_import();
echo "-- Test document import with attributes --\n";
test_document_import_with_attributes();
echo "-- Test appendChild with shadowing --\n";
test_appendChild_with_shadowing();

"#);
    assert_exact(&out, r#"-- Test document fragment with import --
<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="https://php.net/something" xmlns:ns="https://php.net/whatever"><element ns:foo="https://php.net/bar"/></html>
-- Test document fragment without import --
<?xml version="1.0" encoding="UTF-8"?>
<html xmlns=""><element xmlns:foo="https://php.net/bar"><foo:bar/><bar/></element></html>
string(3) "bar"
string(19) "https://php.net/bar"
string(3) "bar"
NULL
-- Test document import --
<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
<div xmlns="http://www.w3.org/1999/xhtml">
    <p>Test-Text</p>
</div>
</feed>
-- Test partial document import --
<?xml version="1.0" encoding="UTF-8"?>
<container xmlns:test="https://php.net/test" xmlns="https://php.net/example"><feed xmlns="http://www.w3.org/1999/xhtml" xmlns:example="https://php.net/example">
<div>
    <p>Test-Text</p>
    <example:p>More test text</example:p>
    <test:p>Even more test text</test:p>
</div>
</feed></container>
-- Test document import with attributes --
<?xml version="1.0" encoding="UTF-8"?>
<div xmlns:example="https://php.net/somethingelse"><p xmlns="https://php.net/default" xmlns:example="https://php.net/example" example:test="test"/></div>
<?xml version="1.0" encoding="UTF-8"?>
<div xmlns:example="https://php.net/somethingelse"><p xmlns="https://php.net/default" xmlns:example="https://php.net/example" example:test="test"><i/></p></div>
-- Test appendChild with shadowing --
<?xml version="1.0" encoding="UTF-8"?>
<container xmlns:default="http://php.net/default"><a xmlns:foo="http://php.net/bar"><b xmlns:foo="http://php.net/foo"><default:test foo:bar=""/><foo:test2/></b></a></container>"#);
}

/// php-src `ext/dom/tests/modern/spec/bug47847.phpt`: Bug #47847 (importNode loses the namespace of an XML element)
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_bug47847() {
    if skip_without_xml_native("test_bug47847") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php
$fromdom = Dom\XMLDocument::createFromString(<<<XML
<?xml version="1.0"?>
<ns:container xmlns:ns="http://php.net">
<ns:inner xmlns="http://php.net">
<ns:WATCH-MY-NAMESPACE xmlns=""/>
</ns:inner>
</ns:container>
XML);

$aDOM = Dom\XMLDocument::createEmpty();
$imported = $aDOM->importNode($fromdom->documentElement->firstElementChild, true);
$aDOM->appendChild($imported);

echo $aDOM->saveXml(), "\n";

var_dump($aDOM->documentElement->firstElementChild->prefix);
"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<ns:inner xmlns:ns="http://php.net" xmlns="http://php.net">
<WATCH-MY-NAMESPACE/>
</ns:inner>
string(2) "ns""#);
}

/// php-src `ext/dom/tests/modern/spec/bug55294.phpt`: Bug #55294 (DOMDocument::importNode shifts namespaces when "default" namespace exists)
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_bug55294() {
    if skip_without_xml_native("test_bug55294") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$aDOM = Dom\XMLDocument::createFromString(<<<EOXML
<A xmlns="http://example.com/A">
<B>
<C xmlns="http://example.com/C" xmlns:default="http://example.com/Z" />
</B>
</A>
EOXML
);

$bDOM = Dom\XMLDocument::createEmpty();
$node = $bDOM->importNode($aDOM->getElementsByTagNameNS('http://example.com/A', 'B')->item(0), true);
$bDOM->appendChild($node);

echo $bDOM->saveXml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<B xmlns="http://example.com/A">
<C xmlns="http://example.com/C" xmlns:default="http://example.com/Z"/>
</B>"#);
}

/// php-src `ext/dom/tests/modern/spec/bug81468.phpt`: Bug #81468 (Inconsistent default namespace inheritance)
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_bug81468() {
    if skip_without_xml_native("test_bug81468") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

// Note: the original expectation from https://bugs.php.net/bug.php?id=81468 isn't *entirely* right

$dom = Dom\XMLDocument::createEmpty();
$dom
  ->appendChild($dom->createElementNS('some:namespace', 'foo'))
  ->appendChild($dom->createElement('bar'));
echo ($xml = $dom->saveXml()), "\n";

$xpath = new Dom\XPath($dom);
$xpath->registerNamespace('n', 'some:namespace');
echo "/n:foo/bar -> ", count($xpath->query('/n:foo/bar')), "\n";
echo "/n:foo/n:bar -> ", count($xpath->query('/n:foo/n:bar')), "\n";

//

$dom = Dom\XMLDocument::createFromString($xml);
echo ($xml = $dom->saveXml()), "\n";

$xpath = new Dom\XPath($dom);
$xpath->registerNamespace('n', 'some:namespace');
echo "/n:foo/bar -> ", count($xpath->query('/n:foo/bar')), "\n";
echo "/n:foo/n:bar -> ", count($xpath->query('/n:foo/n:bar')), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<foo xmlns="some:namespace"><bar xmlns=""/></foo>
/n:foo/bar -> 1
/n:foo/n:bar -> 0
<?xml version="1.0" encoding="UTF-8"?>
<foo xmlns="some:namespace"><bar xmlns=""/></foo>
/n:foo/bar -> 1
/n:foo/n:bar -> 0"#);
}

/// php-src `ext/dom/tests/modern/spec/clone_conflicting_namespace_prefix.phpt`: Test clone with conflicting namespace prefix
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_clone_conflicting_namespace_prefix() {
    if skip_without_xml_native("test_clone_conflicting_namespace_prefix") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString(<<<XML
<?xml version="1.0"?>
<root>
    <a:root1 xmlns:a="urn:a">
        <a:root2 xmlns:a="urn:b">
            <a:child xmlns:b="urn:b">
                <b:child1 xmlns:a="urn:x">bar</b:child1>
            </a:child>
        </a:root2>
    </a:root1>
</root>
XML);

$dom->documentElement->firstElementChild->setAttributeNS("urn:y", "a:foo", "bar");

echo $dom->saveXml(), "\n";

$clone = clone $dom->documentElement->firstElementChild;
var_dump($clone->nodeName, $clone->namespaceURI);

foreach ($clone->attributes as $attr) {
    var_dump($attr->name, $attr->namespaceURI);
}

echo $dom->saveXml($clone), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root>
    <a:root1 xmlns:a="urn:a" xmlns:ns1="urn:y" ns1:foo="bar">
        <a:root2 xmlns:a="urn:b">
            <a:child xmlns:b="urn:b">
                <b:child1 xmlns:a="urn:x">bar</b:child1>
            </a:child>
        </a:root2>
    </a:root1>
</root>
string(7) "a:root1"
string(5) "urn:a"
string(7) "xmlns:a"
string(29) "http://www.w3.org/2000/xmlns/"
string(5) "a:foo"
string(5) "urn:y"
<a:root1 xmlns:a="urn:a" xmlns:ns1="urn:y" ns1:foo="bar">
        <a:root2 xmlns:a="urn:b">
            <a:child xmlns:b="urn:b">
                <b:child1 xmlns:a="urn:x">bar</b:child1>
            </a:child>
        </a:root2>
    </a:root1>"#);
}

/// php-src `ext/dom/tests/modern/spec/clone_document.phpt`: Cloning a whole document
#[test]
#[ignore = r#"DOM port pending: Cannot index non-array"#]
fn test_clone_document() {
    if skip_without_xml_native("test_clone_document") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString(<<<XML
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE root [
    <!ELEMENT child (#PCDATA)>
    <!ELEMENT child2 (#PCDATA)>
    <!ENTITY foo "bar">
]>
<root xmlns:a="urn:a" a:foo="bar">
    afoob
    <child>
        <![CDATA[c]]>
    </child>
    <child2>&foo;</child2>
</root>
XML);

echo "---\n";

var_dump($dom->getElementsByTagName("child2")[0]->firstChild->nodeName);

echo "---\n";

$clone = clone $dom;
echo $clone->saveXml(), "\n";

var_dump($clone->getElementsByTagName("child2")[0]->firstChild->nodeName);

echo "---\n";

$clone = $dom->cloneNode(false);
echo $clone->saveXml(), "\n";

echo "---\n";

$clone = $dom->documentElement->cloneNode(false);
echo $clone->ownerDocument->saveXml($clone), "\n";

"#);
    assert_exact(&out, r#"---
string(3) "foo"
---
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE root [
<!ELEMENT child (#PCDATA)>
<!ELEMENT child2 (#PCDATA)>
<!ENTITY foo "bar">
]>
<root xmlns:a="urn:a" a:foo="bar">
    afoob
    <child>
        <![CDATA[c]]>
    </child>
    <child2>&foo;</child2>
</root>
string(3) "foo"
---
<?xml version="1.0" encoding="UTF-8"?>

---
<root xmlns:a="urn:a" a:foo="bar"/>"#);
}

/// php-src `ext/dom/tests/modern/spec/clone_dtd_node.phpt`: Cloning a dtd node
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_clone_dtd_node() {
    if skip_without_xml_native("test_clone_dtd_node") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString('<!DOCTYPE html><html></html>');

$dt1 = clone $dom->doctype;
var_dump($dt1 === $dom->doctype);
var_dump($dt1->name);

echo "\n";

$dt2 = $dom->doctype->cloneNode();
var_dump($dt2 === $dt1);
var_dump($dt2 === $dom->doctype);
var_dump($dt2->name);

"#);
    assert_exact(&out, r#"bool(false)
string(4) "html"

bool(false)
bool(false)
string(4) "html""#);
}

/// php-src `ext/dom/tests/modern/spec/clone_entity_reference.phpt`: Cloning an entity reference without also cloning the document
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_clone_entity_reference() {
    if skip_without_xml_native("test_clone_entity_reference") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString(<<<XML
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE root [
    <!ENTITY foo "bar">
]>
<root>&foo;</root>
XML);

$clone = $dom->documentElement->cloneNode(true);
echo $dom->saveXml($clone), "\n";

var_dump($clone->firstChild->firstChild);

"#);
    assert_format(&out, r#"<root>&foo;</root>
object(Dom\Entity)#4 (17) {
  ["publicId"]=>
  NULL
  ["systemId"]=>
  NULL
  ["notationName"]=>
  NULL
  ["nodeType"]=>
  int(17)
  ["nodeName"]=>
  string(3) "foo"
  ["baseURI"]=>
  string(%d) "%s"
  ["isConnected"]=>
  bool(true)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["parentNode"]=>
  string(22) "(object value omitted)"
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
  ["nodeValue"]=>
  NULL
  ["textContent"]=>
  NULL
}"#);
}

/// php-src `ext/dom/tests/modern/spec/clone_text_nodes.phpt`: Cloning text nodes should not merge adjacent text nodes
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_clone_text_nodes() {
    if skip_without_xml_native("test_clone_text_nodes") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();
$root = $dom->appendChild($dom->createElementNS("urn:a", "a:root"));
$root->setAttribute("foo", "bar");

$root->appendChild($dom->createTextNode("a"));
$root->appendChild($dom->createTextNode("foo"));
$root->appendChild($dom->createTextNode("b"));
$child = $root->appendChild($dom->createElement("child"));
$child->appendChild($dom->createTextNode("c"));
$child2 = $root->appendChild($dom->createElement("child2"));
$child2->appendChild($dom->createTextNode("d"));

echo $dom->saveXml(), "\n";

$clone = clone $root;
var_dump($clone->firstChild->textContent);
var_dump($clone->firstChild->nextSibling->textContent);
var_dump($clone->firstChild->nextSibling->nextSibling->textContent);
echo $dom->saveXml($clone), "\n";

$clone = $child2->cloneNode(true);
echo $dom->saveXml($clone), "\n";

$clone = $child2->cloneNode(false);
echo $dom->saveXml($clone), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<a:root xmlns:a="urn:a" foo="bar">afoob<child>c</child><child2>d</child2></a:root>
string(1) "a"
string(3) "foo"
string(1) "b"
<a:root xmlns:a="urn:a" foo="bar">afoob<child>c</child><child2>d</child2></a:root>
<child2>d</child2>
<child2/>"#);
}

/// php-src `ext/dom/tests/modern/spec/default_namespace_move.phpt`: Default namespace move
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_default_namespace_move() {
    if skip_without_xml_native("test_default_namespace_move") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();
$root = $dom->appendChild($dom->createElementNS('urn:a', 'foo:bar'));
$child = $root->appendChild($dom->createElementNS('urn:a', 'foo:baz'));
$child->setAttributeNS("urn:b", "foo:attr", "value");
$root->remove();
$dom->appendChild($child);
echo $dom->saveXml(), "\n";

$dom2 = Dom\XMLDocument::createEmpty();
$child = $dom2->importNode($child, true);
$dom2->appendChild($child);
echo $dom2->saveXml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<foo:baz xmlns:foo="urn:a" xmlns:ns1="urn:b" ns1:attr="value"/>
<?xml version="1.0" encoding="UTF-8"?>
<foo:baz xmlns:foo="urn:a" xmlns:ns1="urn:b" ns1:attr="value"/>"#);
}

/// php-src `ext/dom/tests/modern/spec/documentURI_URL.phpt`: Document URI/URL
#[test]
#[ignore = r#"DOM port pending: Undefined property: Dom\XMLDocument::documentURI"#]
fn test_document_uri_url() {
    if skip_without_xml_native("test_document_uri_url") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root><child/></root>');
var_dump($dom->documentURI);
var_dump($dom->URL);

try {
    $dom->URL = NULL;
    var_dump($dom->documentURI);
    var_dump($dom->URL);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}

$dom->URL = "";
var_dump($dom->documentURI);
var_dump($dom->URL);

$dom->URL = "http://example.com/";
var_dump($dom->documentURI);
var_dump($dom->URL);

"#);
    assert_format(&out, r#"string(%d) "%s"
string(%d) "%s"
Cannot assign null to property Dom\Document::$URL of type string
string(0) ""
string(0) ""
string(%d) "%s"
string(%d) "%s""#);
}

/// php-src `ext/dom/tests/modern/spec/dom_parsing_gh44.phpt`: DOM-Parsing GH-44 (It's possible for 'generate a prefix' algorithm to generate a prefix conflicting with an existing one)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_dom_parsing_gh44() {
    if skip_without_xml_native("test_dom_parsing_gh44") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$root = Dom\XMLDocument::createFromString('<root xmlns:ns2="uri2"><child xmlns:ns1="uri1" xmlns:a0="uri1" xmlns:NS1="uri1"/></root>')->documentElement;
$root->firstChild->setAttributeNS('uri3', 'attr1', 'value1');
echo $root->ownerDocument->saveXml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:ns2="uri2"><child xmlns:ns1="uri1" xmlns:a0="uri1" xmlns:NS1="uri1" xmlns:ns2="uri3" ns2:attr1="value1"/></root>"#);
}

/// php-src `ext/dom/tests/modern/spec/dom_parsing_gh45.phpt`: DOM-Parsing GH-45 (It's possible that 'retrieve a preferred prefix string' returns a wrong prefix for the specified namespace)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_dom_parsing_gh45() {
    if skip_without_xml_native("test_dom_parsing_gh45") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$root = Dom\XMLDocument::createFromString('<el1 xmlns:p="u1" xmlns:q="u1"><el2 xmlns:q="u2"/></el1>')->documentElement;
$root->firstChild->setAttributeNS('u1', 'name', 'v');
$attr = $root->firstChild->getAttributeNodeNS("u1", "name");
var_dump($attr->prefix, $attr->namespaceURI);
echo $root->ownerDocument->saveXml($root), "\n";

"#);
    assert_exact(&out, r#"NULL
string(2) "u1"
<el1 xmlns:p="u1" xmlns:q="u1"><el2 xmlns:q="u2" p:name="v"/></el1>"#);
}

/// php-src `ext/dom/tests/modern/spec/dom_parsing_gh47.phpt`: DOM-Parsing GH-47 (XML null namespaces need to be preserved)
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::getElementsByTagNameNS"#]
fn test_dom_parsing_gh47() {
    if skip_without_xml_native("test_dom_parsing_gh47") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$document = Dom\XMLDocument::createFromString(<<<XML
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head>
    <people xmlns="">
      <person>
        <lastname>Smith</lastname>
        <firstname>Joe</firstname>
      </person>
      <person>
        <lastname>Jones</lastname>
        <firstname>John</firstname>
      </person>
    </people>
  </head>
</html>
XML);

echo $document->saveXml(), "\n";

$people = $document->getElementsByTagNameNS(null, 'people')->item(0);
echo $document->saveXml($people), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head>
    <people xmlns="">
      <person>
        <lastname>Smith</lastname>
        <firstname>Joe</firstname>
      </person>
      <person>
        <lastname>Jones</lastname>
        <firstname>John</firstname>
      </person>
    </people>
  </head>
</html>
<people xmlns="">
      <person>
        <lastname>Smith</lastname>
        <firstname>Joe</firstname>
      </person>
      <person>
        <lastname>Jones</lastname>
        <firstname>John</firstname>
      </person>
    </people>"#);
}

/// php-src `ext/dom/tests/modern/spec/dom_parsing_gh47_bis.phpt`: DOM-Parsing GH-47 bis (XML null namespaces need to be preserved)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_dom_parsing_gh47_bis() {
    if skip_without_xml_native("test_dom_parsing_gh47_bis") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

// Adapted from https://github.com/web-platform-tests/wpt/blob/master/domparsing/XMLSerializer-serializeToString.html

$dom = Dom\XMLDocument::createFromString('<root xmlns=""><child xmlns=""/></root>');
echo $dom->saveXml(), "\n";

$dom = Dom\XMLDocument::createFromString('<root xmlns="urn:u1"><child xmlns="urn:u1"/></root>');
echo $dom->saveXml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root xmlns=""><child xmlns=""/></root>
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:u1"><child xmlns="urn:u1"/></root>"#);
}

/// php-src `ext/dom/tests/modern/spec/gh11404_1.phpt`: GH-11404 (DOMDocument::savexml and friends omit xmlns="" declaration for null namespace, creating incorrect xml representation of the DOM)
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createElementNS"#]
fn test_gh11404_1() {
    if skip_without_xml_native("test_gh11404_1") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom1 = Dom\XMLDocument::createFromString('<?xml version="1.0"?><with xmlns="some:ns" />');

$nodeA = $dom1->createElement('none');
$nodeB = $dom1->createElementNS(null, 'none');

$dom1->documentElement->appendChild($nodeA);
$dom1->documentElement->appendChild($nodeB);

echo $dom1->saveXml(), "\n";

var_dump($nodeA->namespaceURI, $nodeB->namespaceURI);

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<with xmlns="some:ns"><none xmlns=""/><none xmlns=""/></with>
NULL
NULL"#);
}

/// php-src `ext/dom/tests/modern/spec/gh11404_2.phpt`: GH-11404 (DOMDocument::savexml and friends omit xmlns="" declaration for null namespace, creating incorrect xml representation of the DOM)
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::importNode"#]
fn test_gh11404_2() {
    if skip_without_xml_native("test_gh11404_2") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom1 = Dom\XMLDocument::createFromString('<?xml version="1.0" ?><with xmlns="some:ns" />');

$dom2 = Dom\XMLDocument::createFromString('<?xml version="1.0" ?><none />');

$dom1->documentElement->append(
    $dom1->importNode(
        $dom2->documentElement
    )
);

echo $dom1->saveXml();
"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<with xmlns="some:ns"><none xmlns=""/></with>"#);
}

/// php-src `ext/dom/tests/modern/spec/import_entity_reference.phpt`: Importing an entity reference without also importing the document
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_import_entity_reference() {
    if skip_without_xml_native("test_import_entity_reference") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString(<<<XML
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE root [
    <!ENTITY foo "bar">
]>
<root>&foo;</root>
XML);

$importer = Dom\XMLDocument::createEmpty();
$imported = $importer->importNode($dom->documentElement, true);
echo $importer->saveXml($imported), "\n";

var_dump($imported->firstChild);

"#);
    assert_exact(&out, r#"<root>&foo;</root>
object(Dom\EntityReference)#3 (14) {
  ["nodeType"]=>
  int(5)
  ["nodeName"]=>
  string(3) "foo"
  ["baseURI"]=>
  string(11) "about:blank"
  ["isConnected"]=>
  bool(false)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["parentNode"]=>
  string(22) "(object value omitted)"
  ["parentElement"]=>
  string(22) "(object value omitted)"
  ["childNodes"]=>
  string(22) "(object value omitted)"
  ["firstChild"]=>
  NULL
  ["lastChild"]=>
  NULL
  ["previousSibling"]=>
  NULL
  ["nextSibling"]=>
  NULL
  ["nodeValue"]=>
  NULL
  ["textContent"]=>
  NULL
}"#);
}

/// php-src `ext/dom/tests/modern/spec/import_of_lone_attribute.phpt`: Import of lone attribute
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_import_of_lone_attribute() {
    if skip_without_xml_native("test_import_of_lone_attribute") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();
$root = $dom->appendChild($dom->createElement('root'));

$root->setAttributeNS("urn:a", "a:foo", "bar");
$attr = $root->getAttributeNodeNS("urn:a", "foo");

$importer = Dom\XMLDocument::createEmpty();
$attr = $importer->importNode($attr, true);
echo $importer->saveXml($attr), "\n";

unset($dom);
unset($root);

var_dump($attr->prefix, $attr->namespaceURI);

"#);
    assert_exact(&out, r#"a:foo="bar"
string(1) "a"
string(5) "urn:a""#);
}

/// php-src `ext/dom/tests/modern/spec/import_of_namepace_conflicting_attributes.phpt`: Import of namespace-conflicting attributes
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_import_of_namepace_conflicting_attributes() {
    if skip_without_xml_native("test_import_of_namepace_conflicting_attributes") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();
$root = $dom->appendChild($dom->createElement('root'));

$root->setAttributeNS("urn:a", "a:root1", "bar");
$root1 = $root->getAttributeNodeNS("urn:a", "root1");
$root->setAttributeNS("urn:b", "a:root2", "bar");
$root2 = $root->getAttributeNodeNS("urn:b", "root2");

$child = $root->appendChild($dom->createElement("child"));
$child->setAttributeNS("urn:x", "a:child1", "bar");
$child1 = $child->getAttributeNodeNS("urn:x", "child1");

echo $dom->saveXml(), "\n";

var_dump($root1->prefix, $root1->namespaceURI);
var_dump($root2->prefix, $root2->namespaceURI);

$child->removeAttribute("a:child1");
$root->setAttributeNodeNS($child1);

$importer = Dom\XMLDocument::createEmpty();
$imported = $importer->importNode($root, true);
echo $importer->saveXml($imported), "\n";

var_dump($imported->getAttributeNodeNS("urn:a", "root1")->prefix);
var_dump($imported->getAttributeNodeNS("urn:a", "root1")->namespaceURI);
var_dump($imported->getAttributeNodeNS("urn:b", "root2")->prefix);
var_dump($imported->getAttributeNodeNS("urn:b", "root2")->namespaceURI);
var_dump($imported->getAttributeNodeNS("urn:x", "child1")->prefix);
var_dump($imported->getAttributeNodeNS("urn:x", "child1")->namespaceURI);

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:a="urn:a" a:root1="bar" xmlns:ns1="urn:b" ns1:root2="bar"><child xmlns:a="urn:x" a:child1="bar"/></root>
string(1) "a"
string(5) "urn:a"
string(1) "a"
string(5) "urn:b"
<root xmlns:a="urn:a" a:root1="bar" xmlns:ns1="urn:b" ns1:root2="bar" xmlns:ns2="urn:x" ns2:child1="bar"><child/></root>
string(1) "a"
string(5) "urn:a"
string(1) "a"
string(5) "urn:b"
string(1) "a"
string(5) "urn:x""#);
}

/// php-src `ext/dom/tests/modern/spec/pre_insertion_validation.phpt`: ParentNode/ChildNode pre-insertion validation
#[test]
#[ignore = r#"DOM port pending: Undefined property: Dom\XMLDocument::doctype"#]
fn test_pre_insertion_validation() {
    if skip_without_xml_native("test_pre_insertion_validation") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php
$dom = Dom\XMLDocument::createFromString("<!DOCTYPE root><root/>");
$doctype = $dom->doctype;
$dom->removeChild($doctype);

echo "--- Trying to insert text node into the document ---\n";

try {
    $dom->append("foo", "bar", "baz");
} catch (DOMException $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}
try {
    $dom->append($dom->createTextNode("text node"));
} catch (DOMException $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}
try {
    $dom->append($dom->createCDATASection("text node"));
} catch (DOMException $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}

echo "--- Trying to insert doctype into not a document ---\n";

$element = $dom->createElement("foo");
try {
    $element->append($doctype);
} catch (DOMException $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}

echo "--- Trying to insert doctype at the wrong place in a document ---\n";

try {
    $dom->append($doctype);
} catch (DOMException $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}

echo "--- Prepend doctype in a document should work ---\n";

$dom->prepend($doctype);

echo "--- Trying to create multiple document roots ---\n";

try {
    $dom->append($dom->createElement("foo"));
} catch (DOMException $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}

echo "--- Trying to insert an element before a document type ---\n";

$dom->documentElement->remove();
try {
    $dom->prepend($element);
} catch (DOMException $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}

echo "--- Document output ---\n";

echo $dom->saveXml(), "\n";

echo "--- Document fragment edge cases with multiple elements ---\n";

$dom = Dom\XMLDocument::createEmpty();
$fragment = $dom->createDocumentFragment();
$fragment->append($dom->createElement("foo"));
$fragment->append($dom->createElement("bar"));
try {
    $dom->append($fragment);
} catch (DOMException $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}

echo "--- Document fragment edge cases with text ---\n";

$dom = Dom\XMLDocument::createEmpty();
$fragment = $dom->createDocumentFragment();
$fragment->append("foo");
$fragment->append($dom->createCDATASection("bar"));
try {
    $dom->append($fragment);
} catch (DOMException $e) {
    echo "Exception: " . $e->getMessage() . "\n";
}

"#);
    assert_exact(&out, r#"--- Trying to insert text node into the document ---
Exception: Cannot insert text as a child of a document
Exception: Cannot insert text as a child of a document
Exception: Cannot insert text as a child of a document
--- Trying to insert doctype into not a document ---
Exception: Cannot insert a document type into anything other than a document
--- Trying to insert doctype at the wrong place in a document ---
Exception: Document types must be the first child in a document
--- Prepend doctype in a document should work ---
--- Trying to create multiple document roots ---
Exception: Cannot have more than one element child in a document
--- Trying to insert an element before a document type ---
Exception: Document types must be the first child in a document
--- Document output ---
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE root>

--- Document fragment edge cases with multiple elements ---
Exception: Cannot have more than one element child in a document
--- Document fragment edge cases with text ---
Exception: Cannot insert text as a child of a document"#);
}

/// php-src `ext/dom/tests/modern/spec/reconciliation_of_attributes.phpt`: Reconciliation of attributes
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_reconciliation_of_attributes() {
    if skip_without_xml_native("test_reconciliation_of_attributes") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();
$root = $dom->appendChild($dom->createElement('root'));

$root->setAttributeNS("urn:a", "a:root1", "bar");
$root1 = $root->getAttributeNodeNS("urn:a", "root1");
$root->setAttributeNS("urn:b", "a:root2", "bar");
$root2 = $root->getAttributeNodeNS("urn:b", "root2");
$root->setAttributeNS("urn:a", "a:root3", "bar");
$root3 = $root->getAttributeNodeNS("urn:a", "root3");

$child = $root->appendChild($dom->createElement("child"));
$child->setAttributeNS("urn:x", "a:child1", "bar");
$child1 = $child->getAttributeNodeNS("urn:x", "child1");
$child->setAttributeNS("urn:a", "a:child2", "bar");
$child2 = $child->getAttributeNodeNS("urn:a", "child2");

echo $dom->saveXml(), "\n";

var_dump($root1->prefix, $root1->namespaceURI);
var_dump($root2->prefix, $root2->namespaceURI);
var_dump($root3->prefix, $root3->namespaceURI);

$child->removeAttribute("a:child1");
$root->setAttributeNodeNS($child1);

var_dump($child1->prefix, $child1->namespaceURI);
var_dump($child2->prefix, $child2->namespaceURI);

echo $dom->saveXml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:a="urn:a" a:root1="bar" xmlns:ns1="urn:b" ns1:root2="bar" a:root3="bar"><child xmlns:a="urn:x" a:child1="bar" xmlns:ns2="urn:a" ns2:child2="bar"/></root>
string(1) "a"
string(5) "urn:a"
string(1) "a"
string(5) "urn:b"
string(1) "a"
string(5) "urn:a"
string(1) "a"
string(5) "urn:x"
string(1) "a"
string(5) "urn:a"
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:a="urn:a" a:root1="bar" xmlns:ns1="urn:b" ns1:root2="bar" a:root3="bar" xmlns:ns2="urn:x" ns2:child1="bar"><child a:child2="bar"/></root>"#);
}

/// php-src `ext/dom/tests/modern/spec/reconciliation_of_elements.phpt`: Reconciliation of elements
#[test]
#[ignore = r#"DOM port pending: Cannot index non-array"#]
fn test_reconciliation_of_elements() {
    if skip_without_xml_native("test_reconciliation_of_elements") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString(<<<XML
<?xml version="1.0"?>
<root>
    <a:root1 xmlns:a="urn:a">
        <a:root2 xmlns:a="urn:b">
            <a:child xmlns:b="urn:b">
                <b:child1 xmlns:a="urn:x">bar</b:child1>
            </a:child>
        </a:root2>
    </a:root1>
</root>
XML);

$root = $dom->documentElement;
$a_child = $dom->getElementsByTagName("a:child")[0];

$root->append($a_child);

echo $dom->saveXml(), "\n";

echo "---\n";

// Trigger reconciliation via detach logic in ext/libxml
$root->remove();
unset($root);

echo $dom->saveXml($a_child), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root>
    <a:root1 xmlns:a="urn:a">
        <a:root2 xmlns:a="urn:b">
            
        </a:root2>
    </a:root1>
<b:child xmlns:b="urn:b">
                <b:child1 xmlns:a="urn:x">bar</b:child1>
            </b:child></root>
---
<b:child xmlns:b="urn:b">
                <b:child1 xmlns:a="urn:x">bar</b:child1>
            </b:child>"#);
}

/// php-src `ext/dom/tests/modern/spec/serialize_default_and_prefixed_xmlns.phpt`: Serialize default and prefixed xmlns on elements
#[test]
fn test_serialize_default_and_prefixed_xmlns() {
    if skip_without_xml_native("test_serialize_default_and_prefixed_xmlns") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString(<<<XML
<?xml version="1.0" encoding="UTF-8"?>
<root>
    <a:child xmlns:a="urn:a" xmlns="urn:default">
        <default/>
    </a:child>
</root>
XML);

echo $dom->saveXml();

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root>
    <a:child xmlns:a="urn:a" xmlns="urn:default">
        <default/>
    </a:child>
</root>"#);
}

/// php-src `ext/dom/tests/modern/spec/serialize_element_unqualified.phpt`: Serialize element choose nearest prefix if unqualified name
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_serialize_element_unqualified() {
    if skip_without_xml_native("test_serialize_element_unqualified") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root xmlns:p1="u1"><child xmlns:p2="u1"><p1:child2/></child></root>');
echo $dom->saveXml(), "\n";

$dom = Dom\XMLDocument::createFromString('<root xmlns:p1="u1"><child xmlns:p2="u1"></child></root>');
$root = $dom->documentElement;
$child2 = $root->ownerDocument->createElementNS('u1', 'child2');
$root->firstChild->appendChild($child2);
echo $dom->saveXml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:p1="u1"><child xmlns:p2="u1"><p1:child2/></child></root>
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:p1="u1"><child xmlns:p2="u1"><p2:child2/></child></root>"#);
}

/// php-src `ext/dom/tests/modern/spec/serialize_non_default_empty_xmlns.phpt`: Serialize non-default but empty xmlns
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_serialize_non_default_empty_xmlns() {
    if skip_without_xml_native("test_serialize_non_default_empty_xmlns") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<test xmlns="urn:a"><a:child xmlns:a=" "/></test>');

echo $dom->saveXml(), "\n";

"#);
    assert_format(&out, r#"Warning: Dom\XMLDocument::createFromString(): xmlns:a: ' ' is not a valid URI in Entity, line: 1 in %s on line %d
<?xml version="1.0" encoding="UTF-8"?>
<test xmlns="urn:a"><a:child xmlns:a=" "/></test>"#);
}

/// php-src `ext/dom/tests/modern/spec/serialize_xml_attribute.phpt`: Serialize XML attribute
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_serialize_xml_attribute() {
    if skip_without_xml_native("test_serialize_xml_attribute") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString("<root/>");
$root = $dom->documentElement;
$root->setAttribute("foo", "<>\"& \t\n\rfoo   ");

echo $dom->saveXml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root foo="&lt;&gt;&quot;&amp; &#9;&#10;&#13;foo   "/>"#);
}

/// php-src `ext/dom/tests/modern/spec/serialize_xml_namespace_on_elements.phpt`: Serialize XML namespace on elements
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_serialize_xml_namespace_on_elements() {
    if skip_without_xml_native("test_serialize_xml_namespace_on_elements") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();
$root = $dom->appendChild($dom->createElementNS("http://www.w3.org/XML/1998/namespace", "xml:test"));
$root->appendChild($dom->createElementNS("http://www.w3.org/XML/1998/namespace", "xml:child"));

echo $dom->saveXml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<xml:test><xml:child/></xml:test>"#);
}

/// php-src `ext/dom/tests/modern/spec/serialize_xml_ns_decl.phpt`: Document::saveXml() with XML namespace declaration
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_serialize_xml_ns_decl() {
    if skip_without_xml_native("test_serialize_xml_ns_decl") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root/>');
$root = $dom->documentElement;
$root->setAttributeNS('http://www.w3.org/2000/xmlns/', 'xmlns:xml', 'http://www.w3.org/XML/1998/namespace');
echo $dom->saveXml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root/>"#);
}

/// php-src `ext/dom/tests/modern/spec/textContent_edge_cases.phpt`: textContent edge cases
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_text_content_edge_cases() {
    if skip_without_xml_native("test_text_content_edge_cases") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<container>text<?pi value?></container>');

echo "document text content: ";
var_dump($dom->textContent);
try {
    $dom->textContent = "foo";
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

$container = $dom->documentElement;

$text = $container->firstChild;
$pi = $text->nextSibling;

echo "text node text content: ";
var_dump($text->textContent);
echo "pi node text content: ";
var_dump($pi->textContent);

$text->textContent = NULL;
$pi->textContent = NULL;

echo "text node text content: ";
var_dump($text->textContent);
echo "pi node text content: ";
var_dump($pi->textContent);

$container->textContent = NULL;
echo $dom->saveXml(), "\n";

"#);
    assert_exact(&out, r#"document text content: NULL
Cannot modify readonly property Dom\XMLDocument::$textContent
text node text content: string(4) "text"
pi node text content: string(5) "value"
text node text content: string(0) ""
pi node text content: string(0) ""
<?xml version="1.0" encoding="UTF-8"?>
<container></container>"#);
}

/// php-src `ext/dom/tests/modern/spec/text_merging.phpt`: Appending should not merge text nodes
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_text_merging() {
    if skip_without_xml_native("test_text_merging") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();
$element = $dom->createElement("div");
$span = $dom->createElement("span");
$span->textContent = "qux";
$element->append("foo", "bar", "baz", $span, $dom->createCDATASection("a"), $dom->createCDATASection("b"));
$dom->append($element);
echo $dom->saveXml(), "\n";

var_dump($element->firstChild->textContent);
var_dump($element->firstChild->nextSibling->textContent);
var_dump($element->firstChild->nextSibling->nextSibling->textContent);

$dom->normalize();
echo $dom->saveXml(), "\n";

var_dump($element->firstChild->textContent);
var_dump($element->firstChild->nextSibling->textContent);

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<div>foobarbaz<span>qux</span><![CDATA[a]]><![CDATA[b]]></div>
string(3) "foo"
string(3) "bar"
string(3) "baz"
<?xml version="1.0" encoding="UTF-8"?>
<div>foobarbaz<span>qux</span><![CDATA[a]]><![CDATA[b]]></div>
string(9) "foobarbaz"
string(3) "qux""#);
}

/// php-src `ext/dom/tests/modern/spec/xml_serialize_LIBXML_NOEMPTYTAG.phpt`: XML Serialization with LIBXML_NOEMPTYTAG
#[test]
#[ignore = r#"DOM port pending: Undefined constant: LIBXML_NOEMPTYTAG"#]
fn test_xml_serialize_libxml_noemptytag() {
    if skip_without_xml_native("test_xml_serialize_libxml_noemptytag") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$doc = Dom\XMLDocument::createFromString('<root><child/><br xmlns="http://www.w3.org/1999/xhtml"/></root>');
echo $doc->saveXml($doc, LIBXML_NOEMPTYTAG), "\n";

$doc->formatOutput = true;
echo $doc->saveXml($doc, LIBXML_NOEMPTYTAG), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root><child></child><br xmlns="http://www.w3.org/1999/xhtml"></br></root>
<?xml version="1.0" encoding="UTF-8"?>
<root>
  <child></child>
  <br xmlns="http://www.w3.org/1999/xhtml"></br>
</root>"#);
}

/// php-src `ext/dom/tests/modern/spec/xml_serialize_formatting.phpt`: XML Serialization formatting
#[test]
#[ignore = r#"DOM port pending: Undefined property: Dom\XMLDocument::formatOutput"#]
fn test_xml_serialize_formatting() {
    if skip_without_xml_native("test_xml_serialize_formatting") {
        return;
    }
    let out = run_case(r#"modern/spec"#, &[], r#"<?php

$tests = [
    '<root></root>',
    '<root> </root>',
    '<!DOCTYPE html><html><head/></html>',
    '<root><child>text</child><!-- comment --><foo>&amp;</foo></root>',
    '<root><child>text</child><![CDATA[x]]><foo><bar/></foo></root>',
    '<root><a/><foo><bar><?foo <x><y/></x> ?><baz/></bar></foo></root>',
    <<<XML
    <root>
      <child>
        <child/>
      </child>
    </root>
    XML,
];

foreach ($tests as $test) {
    echo "---\n";
    $dom = Dom\XMLDocument::createFromString($test);
    $dom->formatOutput = true;
    echo $dom->saveXml(), "\n";
}

"#);
    assert_exact(&out, r#"---
<?xml version="1.0" encoding="UTF-8"?>
<root/>
---
<?xml version="1.0" encoding="UTF-8"?>
<root> </root>
---
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html>
  <head/>
</html>
---
<?xml version="1.0" encoding="UTF-8"?>
<root>
  <child>text</child>
  <!-- comment -->
  <foo>&amp;</foo>
</root>
---
<?xml version="1.0" encoding="UTF-8"?>
<root><child>text</child><![CDATA[x]]><foo><bar/></foo></root>
---
<?xml version="1.0" encoding="UTF-8"?>
<root>
  <a/>
  <foo>
    <bar>
      <?foo <x><y/></x> ?>
      <baz/>
    </bar>
  </foo>
</root>
---
<?xml version="1.0" encoding="UTF-8"?>
<root>
  <child>
    <child/>
  </child>
</root>"#);
}
