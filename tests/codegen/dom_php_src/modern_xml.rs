//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `modern/xml` folder, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/modern/xml/DTDNamedNodeMap.phpt`: DtdNamedNodeMap
#[test]
#[ignore = r#"DOM port pending: Undefined property: Dom\XMLDocument::doctype"#]
fn test_dtdnamed_node_map() {
    if skip_without_xml_native("test_dtdnamed_node_map") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString(<<<XML
<?xml version="1.0"?>
<!DOCTYPE root [
<!ENTITY test "entity is only for test purposes">
<!ENTITY myimage PUBLIC "-" "mypicture.gif" NDATA GIF>
<!NOTATION GIF SYSTEM "viewgif.exe">
]>
<root/>
XML);

$doctype = $dom->doctype;

var_dump($doctype);

var_dump($doctype->entities["test"]);
var_dump($doctype->entities["myimage"]);
// TODO: isConnected returning false is a bug
var_dump($doctype->notations["GIF"]);

"#);
    assert_format(&out, r#"object(Dom\DocumentType)#2 (20) {
  ["name"]=>
  string(4) "root"
  ["entities"]=>
  string(22) "(object value omitted)"
  ["notations"]=>
  string(22) "(object value omitted)"
  ["publicId"]=>
  string(0) ""
  ["systemId"]=>
  string(0) ""
  ["internalSubset"]=>
  string(105) "<!ENTITY test "entity is only for test purposes">
<!ENTITY myimage PUBLIC "-" "mypicture.gif" NDATA GIF>
"
  ["nodeType"]=>
  int(10)
  ["nodeName"]=>
  string(4) "root"
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
  NULL
  ["lastChild"]=>
  NULL
  ["previousSibling"]=>
  NULL
  ["nextSibling"]=>
  string(22) "(object value omitted)"
  ["nodeValue"]=>
  NULL
  ["textContent"]=>
  NULL
}
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
  string(4) "test"
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
  NULL
  ["lastChild"]=>
  NULL
  ["previousSibling"]=>
  NULL
  ["nextSibling"]=>
  string(22) "(object value omitted)"
  ["nodeValue"]=>
  NULL
  ["textContent"]=>
  NULL
}
object(Dom\Entity)#3 (17) {
  ["publicId"]=>
  string(1) "-"
  ["systemId"]=>
  string(13) "mypicture.gif"
  ["notationName"]=>
  string(3) "GIF"
  ["nodeType"]=>
  int(17)
  ["nodeName"]=>
  string(7) "myimage"
  ["baseURI"]=>
  string(%d) "%smypicture.gif"
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
  NULL
  ["lastChild"]=>
  NULL
  ["previousSibling"]=>
  string(22) "(object value omitted)"
  ["nextSibling"]=>
  NULL
  ["nodeValue"]=>
  NULL
  ["textContent"]=>
  NULL
}
object(Dom\Notation)#4 (13) {
  ["nodeType"]=>
  int(12)
  ["nodeName"]=>
  string(3) "GIF"
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
}"#);
}

/// php-src `ext/dom/tests/modern/xml/Element_innerHTML_prefixed_writing.phpt`: Test setting innerHTML on a prefixed element
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_element_inner_html_prefixed_writing() {
    if skip_without_xml_native("test_element_inner_html_prefixed_writing") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php
$dom = Dom\XMLDocument::createFromString('<x:root xmlns:x="urn:x"/>');
$dom->documentElement->innerHTML = '<child><x:subchild/></child>';
echo $dom->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<x:root xmlns:x="urn:x"><child><x:subchild/></child></x:root>"#);
}

/// php-src `ext/dom/tests/modern/xml/Element_innerHTML_writing.phpt`: Test writing Element::$innerHTML on XML documents
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_element_inner_html_writing() {
    if skip_without_xml_native("test_element_inner_html_writing") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = DOM\XMLDocument::createEmpty();
$el = $dom->createElementNS('urn:a', 'root');
$dom->appendChild($el);
$el->innerHTML = '<p>foo</p><p xmlns="">bar</p>';
echo $dom->saveXML(), "\n";
$el->innerHTML = '';
echo $dom->saveXML(), "\n";
$el->innerHTML = '&amp;';
echo $dom->saveXML(), "\n";
$el->innerHTML = '&lt;foo&gt;';
echo $dom->saveXML(), "\n";

echo "----------------\n";

$dom = DOM\XMLDocument::createFromString('<root/>');
$child = $dom->documentElement->appendChild($dom->createElementNS('urn:a', 'child'));
$child->setAttributeNS('http://www.w3.org/2000/xmlns/', 'xmlns', 'urn:b');
$child->innerHTML = '<default/>';
echo $dom->saveXML(), "\n";
var_dump($child->namespaceURI);
var_dump($child->firstChild->namespaceURI);

echo "----------------\n";

$dom = DOM\XMLDocument::createFromString(<<<XML
<root xmlns="urn:a" xmlns:b="urn:b" xmlns:c="urn:c">
    <b:child a="none" b:b="b" c:c="c"/>
    <c:child a="none" b:b="b" c:c="c"/>
    <?pi ?>
    <!-- comment -->
    <child a="none" b:b="b" c:c="c">
        <![CDATA[ cdata ]]>
    </child>
</root>
XML);
$dom->documentElement->innerHTML = $dom->documentElement->innerHTML;
echo $dom->saveXML(), "\n";

echo "----------------\n";
$dom->documentElement->innerHTML = <<<XML
<child b:b="b" c:c="c">
    <b:child/>
    <c:child/>
</child>
XML;
echo $dom->saveXML(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:a"><p>foo</p><p xmlns="">bar</p></root>
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:a"/>
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:a">&amp;</root>
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:a">&lt;foo&gt;</root>
----------------
<?xml version="1.0" encoding="UTF-8"?>
<root><child xmlns="urn:a"><default/></child></root>
string(5) "urn:a"
string(5) "urn:a"
----------------
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:a" xmlns:b="urn:b" xmlns:c="urn:c">
    <b:child a="none" b:b="b" c:c="c"/>
    <c:child a="none" b:b="b" c:c="c"/>
    <?pi ?>
    <!-- comment -->
    <child xmlns="urn:a" a="none" b:b="b" c:c="c">
        <![CDATA[ cdata ]]>
    </child>
</root>
----------------
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:a" xmlns:b="urn:b" xmlns:c="urn:c"><child b:b="b" c:c="c">
    <b:child/>
    <c:child/>
</child></root>"#);
}

/// php-src `ext/dom/tests/modern/xml/Element_innerHTML_writing_errors.phpt`: Test writing Element::$innerHTML on XML documents - error cases
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createElementNS"#]
fn test_element_inner_html_writing_errors() {
    if skip_without_xml_native("test_element_inner_html_writing_errors") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString(<<<XML
<!DOCTYPE root [
    <!ENTITY foo "content">
]>
<root/>
XML);
$child = $dom->documentElement->appendChild($dom->createElementNS('urn:a', 'child'));
$original = $dom->saveXML();

function test($child, $html) {
    global $dom, $original;
    try {
        $child->innerHTML = $html;
    } catch (DOMException $e) {
        echo $e->getMessage(), "\n";
    }
    var_dump($dom->saveXML() === $original);
}

test($child, '&foo;');
test($child, '</root>');
test($child, '</root><foo/><!--');
test($child, '--></root><!--');
test($child, '<');
test($child, '<!ENTITY foo "content">');

"#);
    assert_exact(&out, r#"XML fragment is not well-formed
bool(true)
XML fragment is not well-formed
bool(true)
XML fragment is not well-formed
bool(true)
XML fragment is not well-formed
bool(true)
XML fragment is not well-formed
bool(true)
XML fragment is not well-formed
bool(true)"#);
}

/// php-src `ext/dom/tests/modern/xml/Element_innerOuterHTML_reading.phpt`: Test reading Element::${inner,outer}HTML on XML documents
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_element_inner_outer_html_reading() {
    if skip_without_xml_native("test_element_inner_outer_html_reading") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = DOM\XMLDocument::createEmpty();

function createContainer() {
    global $dom;
    $element = $dom->createElement("container");
    return $element;
}

$container = createContainer();
$container->append("Hello, world!");
var_dump($container->innerHTML);
var_dump($container->outerHTML);

$container = createContainer();
$container->append($dom->createComment("This is -a- comment"));
var_dump($container->innerHTML);
var_dump($container->outerHTML);

$container = createContainer();
// Note: intentionally typo'd to check whether the string matching against "xml" happens correctly
//       i.e. no bugs with prefix-matching only.
$container->append($dom->createProcessingInstruction("xmll", ""));
var_dump($container->innerHTML);
var_dump($container->outerHTML);

$container = createContainer();
$container->append($dom->createProcessingInstruction("almostmalformed", ">?"));
var_dump($container->innerHTML);
var_dump($container->outerHTML);

$container = createContainer();
$element = $container->appendChild(createContainer());
$element->setAttributeNS('http://www.w3.org/2000/xmlns/', 'xmlns', 'http://example.com/');
var_dump($container->innerHTML);
var_dump($container->outerHTML);

$container = createContainer();
$element = $container->appendChild(createContainer());
$element->setAttributeNS('urn:a', 'name', '');
$element->setAttributeNS('urn:b', 'name', '');
var_dump($container->innerHTML);
var_dump($container->outerHTML);

$dom = DOM\XMLDocument::createFromFile(__DIR__ . '/../../book.xml');
var_dump($dom->documentElement->innerHTML);
var_dump($dom->documentElement->outerHTML);

"#);
    assert_exact(&out, r#"string(13) "Hello, world!"
string(36) "<container>Hello, world!</container>"
string(26) "<!--This is -a- comment-->"
string(49) "<container><!--This is -a- comment--></container>"
string(9) "<?xmll ?>"
string(32) "<container><?xmll ?></container>"
string(22) "<?almostmalformed >??>"
string(45) "<container><?almostmalformed >??></container>"
string(12) "<container/>"
string(35) "<container><container/></container>"
string(72) "<container xmlns:ns1="urn:a" ns1:name="" xmlns:ns2="urn:b" ns2:name=""/>"
string(95) "<container><container xmlns:ns1="urn:a" ns1:name="" xmlns:ns2="urn:b" ns2:name=""/></container>"
string(167) "
 <book>
  <title>The Grapes of Wrath</title>
  <author>John Steinbeck</author>
 </book>
 <book>
  <title>The Pearl</title>
  <author>John Steinbeck</author>
 </book>
"
string(182) "<books>
 <book>
  <title>The Grapes of Wrath</title>
  <author>John Steinbeck</author>
 </book>
 <book>
  <title>The Pearl</title>
  <author>John Steinbeck</author>
 </book>
</books>""#);
}

/// php-src `ext/dom/tests/modern/xml/Element_innerOuterHTML_reading_errors.phpt`: Test reading Element::${inner,outer}HTML on XML documents - error cases
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_element_inner_outer_html_reading_errors() {
    if skip_without_xml_native("test_element_inner_outer_html_reading_errors") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = DOM\XMLDocument::createEmpty();

function createContainer() {
    global $dom;
    $element = $dom->createElement("container");
    return $element;
}

function test($container) {
    try {
        var_dump($container->innerHTML);
    } catch (DOMException $e) {
        echo $e->getMessage(), "\n";
    }
    try {
        var_dump($container->outerHTML);
    } catch (DOMException $e) {
        echo $e->getMessage(), "\n";
    }
}

$container = createContainer();
$container->append("Hello, \x01 world!");
test($container);

$container = createContainer();
$container->append($dom->createComment('Hello -- world'));
test($container);

$container = createContainer();
$container->append($dom->createComment('Hello world-'));
test($container);

$container = createContainer();
$container->append($dom->createComment('Hello world-'));
test($container);

$container = createContainer();
$container->append($dom->createComment("\x01"));
test($container);

$container = createContainer();
$legacy = new DOMDocument;
$container->append($dom->importLegacyNode($legacy->createProcessingInstruction('foo:bar', '?>')));
test($container);

$container = createContainer();
$legacy = new DOMDocument;
$container->append($dom->importLegacyNode($legacy->createProcessingInstruction('foo', '?>')));
test($container);

$container = createContainer();
$legacy = new DOMDocument;
$container->append($dom->importLegacyNode($legacy->createProcessingInstruction('xml', '')));
test($container);

$container = createContainer();
$legacy = new DOMDocument;
$container->append($dom->importLegacyNode($legacy->createProcessingInstruction('foo', "\x01")));
test($container);

$container = createContainer();
$container->append($dom->createElement("with:colon"));
test($container);

$container = createContainer();
$container->append($dom->createElementNS("http://www.w3.org/2000/xmlns/", "xmlns:colon"));
test($container);

$container = createContainer();
$element = $container->appendChild(createContainer());
$element->setAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns:x", "http://www.w3.org/2000/xmlns/");
test($container);

$container = createContainer();
$element = $container->appendChild(createContainer());
$element->setAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns:x", "");
test($container);

$container = createContainer();
$element = $container->appendChild(createContainer());
$element->setAttribute("with:colon", "value");
test($container);

$container = createContainer();
$element = $container->appendChild(createContainer());
$element->setAttribute("xmlns", "value");
test($container);

"#);
    assert_exact(&out, r#"The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed
The resulting XML serialization is not well-formed"#);
}

/// php-src `ext/dom/tests/modern/xml/Element_insertAdjacentHTML.phpt`: Dom\Element::insertAdjacentHTML() with XML nodes
#[test]
#[ignore = r#"DOM port pending: Undefined class constant: Dom\AdjacentPosition::BeforeBegin"#]
fn test_element_insert_adjacent_html() {
    if skip_without_xml_native("test_element_insert_adjacent_html") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

const POSITIONS = [
    Dom\AdjacentPosition::BeforeBegin,
    Dom\AdjacentPosition::AfterBegin,
    Dom\AdjacentPosition::BeforeEnd,
    Dom\AdjacentPosition::AfterEnd,
];

function test(string $xml) {
    echo "=== XML ($xml) ===\n";

    foreach (POSITIONS as $position) {
        echo "--- Position ", $position->name, " ---\n";

        $dom = Dom\XMLDocument::createFromString('<root xmlns:x="urn:x"><div/></root>');
        $div = $dom->documentElement->firstChild;
        $div->append("Sample text");

        $div->insertAdjacentHTML($position, $xml);

        echo $dom->saveXML(), "\n";
        var_dump($div->childNodes->length);
        var_dump($dom->documentElement->childNodes->length);
    }
}

test('<x:x><y xmlns:x="urn:x2"/></x:x><nons/>');
test('<?pi node?><!-- comment -->&amp;');
test('text node');

"#);
    assert_exact(&out, r#"=== XML (<x:x><y xmlns:x="urn:x2"/></x:x><nons/>) ===
--- Position BeforeBegin ---
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:x="urn:x"><x:x><y xmlns:x="urn:x2"/></x:x><nons/><div>Sample text</div></root>
int(1)
int(3)
--- Position AfterBegin ---
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:x="urn:x"><div><x:x><y xmlns:x="urn:x2"/></x:x><nons/>Sample text</div></root>
int(3)
int(1)
--- Position BeforeEnd ---
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:x="urn:x"><div>Sample text<x:x><y xmlns:x="urn:x2"/></x:x><nons/></div></root>
int(3)
int(1)
--- Position AfterEnd ---
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:x="urn:x"><div>Sample text</div><x:x><y xmlns:x="urn:x2"/></x:x><nons/></root>
int(1)
int(3)
=== XML (<?pi node?><!-- comment -->&amp;) ===
--- Position BeforeBegin ---
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:x="urn:x"><?pi node?><!-- comment -->&amp;<div>Sample text</div></root>
int(1)
int(4)
--- Position AfterBegin ---
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:x="urn:x"><div><?pi node?><!-- comment -->&amp;Sample text</div></root>
int(4)
int(1)
--- Position BeforeEnd ---
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:x="urn:x"><div>Sample text<?pi node?><!-- comment -->&amp;</div></root>
int(4)
int(1)
--- Position AfterEnd ---
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:x="urn:x"><div>Sample text</div><?pi node?><!-- comment -->&amp;</root>
int(1)
int(4)
=== XML (text node) ===
--- Position BeforeBegin ---
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:x="urn:x">text node<div>Sample text</div></root>
int(1)
int(2)
--- Position AfterBegin ---
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:x="urn:x"><div>text nodeSample text</div></root>
int(2)
int(1)
--- Position BeforeEnd ---
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:x="urn:x"><div>Sample texttext node</div></root>
int(2)
int(1)
--- Position AfterEnd ---
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns:x="urn:x"><div>Sample text</div>text node</root>
int(1)
int(2)"#);
}

/// php-src `ext/dom/tests/modern/xml/Element_insertAdjacentHTML_errors.phpt`: Dom\Element::insertAdjacentHTML() with XML nodes - errors
#[test]
#[ignore = r#"DOM port pending: Undefined class constant: Dom\AdjacentPosition::AfterBegin"#]
fn test_element_insert_adjacent_html_errors() {
    if skip_without_xml_native("test_element_insert_adjacent_html_errors") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root/>');
try {
    $dom->documentElement->insertAdjacentHTML(Dom\AdjacentPosition::AfterBegin, "<non-well-formed>");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"XML fragment is not well-formed"#);
}

/// php-src `ext/dom/tests/modern/xml/Element_outerHTML_writing.phpt`: Test writing Element::$outerHTML on XML documents
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createDocumentFragment"#]
fn test_element_outer_html_writing() {
    if skip_without_xml_native("test_element_outer_html_writing") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString("<root xmlns='urn:a'><x/></root>");
$dom->documentElement->firstChild->outerHTML = '<x xmlns="urn:a"><p>foo</p><p xmlns="urn:x">bar</p></x>';
echo $dom->saveXML(), "\n";

$dom->documentElement->firstChild->outerHTML = $dom->documentElement->firstChild->outerHTML;
$element = $dom->documentElement->firstChild->firstChild;
echo $dom->saveXML(), "\n";

$dom->documentElement->firstChild->outerHTML = 'tést';
echo $dom->saveXML(), "\n";

var_dump($element->tagName);

$fragment = $dom->createDocumentFragment();
$fragment->appendChild($dom->createElement('p'));
$fragment->firstChild->outerHTML = '<strong>bar</strong>';
echo $dom->saveXML($fragment), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:a"><x xmlns="urn:a"><p>foo</p><p xmlns="urn:x">bar</p></x></root>
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:a"><x xmlns="urn:a"><p>foo</p><p xmlns="urn:x">bar</p></x></root>
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:a">tést</root>
string(1) "p"
<strong xmlns="http://www.w3.org/1999/xhtml">bar</strong>"#);
}

/// php-src `ext/dom/tests/modern/xml/Element_outerHTML_writing_errors.phpt`: Test writing Element::$outerHTML on XML documents - error cases
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_element_outer_html_writing_errors() {
    if skip_without_xml_native("test_element_outer_html_writing_errors") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root/>');
try {
    $dom->documentElement->outerHTML = '<x/>';
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

$dom = Dom\XMLDocument::createFromString('<root><child/></root>');
try {
    $dom->documentElement->firstChild->outerHTML = '<!DOCTYPE html>';
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Invalid Modification Error
XML fragment is not well-formed"#);
}

/// php-src `ext/dom/tests/modern/xml/Node_removeChild_from_comment.phpt`: Removing a child from a comment should result in NOT_FOUND_ERR
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_node_remove_child_from_comment() {
    if skip_without_xml_native("test_node_remove_child_from_comment") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root><!-- comment --><child/></root>');
$comment = $dom->documentElement->firstChild;
$child = $comment->nextSibling;

try {
    $comment->removeChild($child);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Not Found Error"#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_createFromFile_empty_input.phpt`: XMLDocument::createFromFile() - empty input
#[test]
#[ignore = r#"DOM port pending: Undefined property: Dom\XMLDocument::documentURI"#]
fn test_xmldocument_create_from_file_empty_input() {
    if skip_without_xml_native("test_xmldocument_create_from_file_empty_input") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php
$dom = Dom\XMLDocument::createFromFile("php://memory");
var_dump($dom->documentURI);
fclose($memory);
"#);
    assert_format(&out, r#"Warning: Dom\XMLDocument::createFromFile(): Document is empty in php://memory, line: 1 in %s on line %d

Fatal error: Uncaught DOMException: XML fragment is not well-formed in %s:%d
Stack trace:
#0 %s(%d): Dom\XMLDocument::createFromFile('php://memory')
#1 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_createFromFile_override_encoding.phpt`: Dom\XMLDocument::createFromFile() with overrideEncoding
#[test]
#[ignore = r#"DOM port pending: Undefined property: Dom\XMLDocument::charset"#]
fn test_xmldocument_create_from_file_override_encoding() {
    if skip_without_xml_native("test_xmldocument_create_from_file_override_encoding") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

try {
    Dom\XMLDocument::createFromFile(__DIR__ . '/dummy.xml', overrideEncoding: 'nonexistent');
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

// The override encoding matches with the document encoding attribute
$dom = Dom\XMLDocument::createFromFile(__DIR__ . '/dummy.xml', overrideEncoding: 'UTF-8');
var_dump($dom->documentElement->lastChild->textContent);
var_dump($dom->charset);

// The override encoding mismatches with the document encoding attribute
$dom = Dom\XMLDocument::createFromFile(__DIR__ . '/dummy.xml', overrideEncoding: 'Windows-1252');
var_dump($dom->documentElement->lastChild->textContent);
var_dump($dom->charset);

"#);
    assert_exact(&out, r#"Dom\XMLDocument::createFromFile(): Argument #3 ($overrideEncoding) must be a valid document encoding
string(2) "é"
string(5) "UTF-8"
string(4) "Ã©"
string(12) "Windows-1252""#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_createFromString_override_encoding.phpt`: Dom\XMLDocument::createFromString() with overrideEncoding
#[test]
#[ignore = r#"DOM port pending: Static method Dom\XMLDocument::createFromString parameter $source expects string, got string|false"#]
fn test_xmldocument_create_from_string_override_encoding() {
    if skip_without_xml_native("test_xmldocument_create_from_string_override_encoding") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

try {
    Dom\XMLDocument::createFromString(file_get_contents(__DIR__ . '/dummy.xml'), overrideEncoding: 'nonexistent');
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

// The override encoding matches with the document encoding attribute
$dom = Dom\XMLDocument::createFromString(file_get_contents(__DIR__ . '/dummy.xml'), overrideEncoding: 'UTF-8');
var_dump($dom->documentElement->lastChild->textContent);
var_dump($dom->charset);

// The override encoding mismatches with the document encoding attribute
$dom = Dom\XMLDocument::createFromString(file_get_contents(__DIR__ . '/dummy.xml'), overrideEncoding: 'Windows-1252');
var_dump($dom->documentElement->lastChild->textContent);
var_dump($dom->charset);

"#);
    assert_exact(&out, r#"Dom\XMLDocument::createFromString(): Argument #3 ($overrideEncoding) must be a valid document encoding
string(2) "é"
string(5) "UTF-8"
string(4) "Ã©"
string(12) "Windows-1252""#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_debug.phpt`: Dom\XMLDocument debug
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_xmldocument_debug() {
    if skip_without_xml_native("test_xmldocument_debug") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty();
var_dump($dom);

"#);
    assert_exact(&out, r##"object(Dom\XMLDocument)#1 (33) {
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
}"##);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_documentURI.phpt`: Dom\XMLDocument::documentURI
#[test]
#[ignore = r#"DOM port pending: Undefined property: Dom\XMLDocument::documentURI"#]
fn test_xmldocument_document_uri() {
    if skip_without_xml_native("test_xmldocument_document_uri") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromFile(__DIR__ . "/test foo.xml", LIBXML_NOERROR);
var_dump($dom->documentURI);

"#);
    assert_format(&out, r#"string(%d) "file://%stest%sfoo.xml""#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_fromEmptyDocument_01.phpt`: Dom\XMLDocument::createEmpty 01
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_xmldocument_from_empty_document_01() {
    if skip_without_xml_native("test_xmldocument_from_empty_document_01") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

Dom\XMLDocument::createEmpty(encoding: "foo");

"#);
    assert_fatal_format(&out, r#"Fatal error: Uncaught ValueError: Dom\XMLDocument::createEmpty(): Argument #2 ($encoding) is not a valid document encoding in %s:%d
Stack trace:
#0 %s(%d): Dom\XMLDocument::createEmpty('1.0', 'foo')
#1 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_fromEmptyDocument_02.phpt`: Dom\XMLDocument::createEmpty 02
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_xmldocument_from_empty_document_02() {
    if skip_without_xml_native("test_xmldocument_from_empty_document_02") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty("1.1");
var_dump($dom);

"#);
    assert_exact(&out, r##"object(Dom\XMLDocument)#1 (33) {
  ["xmlEncoding"]=>
  string(5) "UTF-8"
  ["xmlStandalone"]=>
  bool(false)
  ["xmlVersion"]=>
  string(3) "1.1"
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
}"##);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_fromEmptyDocument_03.phpt`: Dom\XMLDocument::createEmpty 03
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_xmldocument_from_empty_document_03() {
    if skip_without_xml_native("test_xmldocument_from_empty_document_03") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty("1.0", "ASCII");
$dom->append($dom->createElement("foo"));
echo $dom->saveXml();

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="ASCII"?>
<foo/>"#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_fromFile_01.phpt`: Dom\XMLDocument::createFromFile 01
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_xmldocument_from_file_01() {
    if skip_without_xml_native("test_xmldocument_from_file_01") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

Dom\XMLDocument::createFromString("");

"#);
    assert_fatal_format(&out, r#"Fatal error: Uncaught ValueError: Dom\XMLDocument::createFromString(): Argument #1 ($source) must not be empty in %s:%d
Stack trace:
#0 %s(%d): Dom\XMLDocument::createFromString('')
#1 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_fromFile_02.phpt`: Dom\XMLDocument::createFromFile 02
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_xmldocument_from_file_02() {
    if skip_without_xml_native("test_xmldocument_from_file_02") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

Dom\XMLDocument::createFromFile("\\0");

"#);
    assert_format(&out, r#"Warning: Dom\XMLDocument::createFromFile(): I/O %s : failed to load %s

Fatal error: Uncaught Exception: Cannot open file '\0' in %s:%d
Stack trace:
#0 %s(%d): Dom\XMLDocument::createFromFile('\\0')
#1 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_fromFile_03.phpt`: Dom\XMLDocument::createFromFile 03
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_xmldocument_from_file_03() {
    if skip_without_xml_native("test_xmldocument_from_file_03") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

try {
    Dom\XMLDocument::createFromFile("%00");
} catch (ValueError $e) {
    echo $e->getMessage();
}

"#);
    assert_exact(&out, r#"Dom\XMLDocument::createFromFile(): Argument #1 ($path) must not contain percent-encoded NUL bytes"#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_fromFile_04.phpt`: Dom\XMLDocument::createFromFile 04
#[test]
fn test_xmldocument_from_file_04() {
    if skip_without_xml_native("test_xmldocument_from_file_04") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromFile(__DIR__."/../../book.xml");
echo $dom->saveXml();

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<books>
 <book>
  <title>The Grapes of Wrath</title>
  <author>John Steinbeck</author>
 </book>
 <book>
  <title>The Pearl</title>
  <author>John Steinbeck</author>
 </book>
</books>"#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_fromString_01.phpt`: Dom\XMLDocument::createFromString 01
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_xmldocument_from_string_01() {
    if skip_without_xml_native("test_xmldocument_from_string_01") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

Dom\XMLDocument::createFromString("");

"#);
    assert_fatal_format(&out, r#"Fatal error: Uncaught ValueError: Dom\XMLDocument::createFromString(): Argument #1 ($source) must not be empty in %s:%d
Stack trace:
#0 %s(%d): Dom\XMLDocument::createFromString('')
#1 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_fromString_02.phpt`: Dom\XMLDocument::createFromString 02
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::saveXmlFile"#]
fn test_xmldocument_from_string_02() {
    if skip_without_xml_native("test_xmldocument_from_string_02") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<?xml version="1.0"?><container/>');
var_dump($dom->saveXmlFile(__DIR__ . "/fromString_02.xml"));
echo file_get_contents(__DIR__ . "/fromString_02.xml");

"#);
    assert_exact(&out, r#"int(51)
<?xml version="1.0" encoding="UTF-8"?>
<container/>"#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_fromString_03.phpt`: Dom\XMLDocument::createFromString 03
#[test]
#[ignore = r#"DOM port pending: Undefined constant: LIBXML_XINCLUDE"#]
fn test_xmldocument_from_string_03() {
    if skip_without_xml_native("test_xmldocument_from_string_03") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$flags = [
    LIBXML_RECOVER, LIBXML_NOENT, LIBXML_DTDLOAD, LIBXML_DTDATTR, LIBXML_DTDVALID, LIBXML_NOERROR, LIBXML_NOWARNING, LIBXML_NOBLANKS, LIBXML_XINCLUDE, LIBXML_NSCLEAN, LIBXML_NOCDATA, LIBXML_NONET, LIBXML_PEDANTIC, LIBXML_COMPACT, LIBXML_PARSEHUGE, LIBXML_BIGLINES
];

try {
    Dom\XMLDocument::createFromString('<?xml version="1.0"?><container/>', -1);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

foreach ($flags as $flag) {
    var_dump(Dom\XMLDocument::createFromString('<?xml version="1.0"?><container/>', $flag) instanceof Dom\XMLDocument);
}

"#);
    assert_format(&out, r#"Dom\XMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: %s)
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
bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_fromString_04.phpt`: Dom\XMLDocument::createFromString 04
#[test]
#[ignore = r#"DOM port pending: Undefined property: Dom\XMLDocument::charset"#]
fn test_xmldocument_from_string_04() {
    if skip_without_xml_native("test_xmldocument_from_string_04") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<?xml version="1.0" encoding="Windows-1251"?><container/>');
var_dump($dom->charset);
echo $dom->saveXml();

"#);
    assert_exact(&out, r#"string(12) "Windows-1251"
<?xml version="1.0" encoding="Windows-1251"?>
<container/>"#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_node_ownerDocument_for_XML.phpt`: Dom\XMLDocument getting ownerDocument from a node in an XML document should yield a Dom\XMLDocument
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_xmldocument_node_owner_document_for_xml() {
    if skip_without_xml_native("test_xmldocument_node_owner_document_for_xml") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<?xml version="1.0"?><container/>');

$element = $dom->documentElement;
unset($dom);
var_dump($element->ownerDocument);

"#);
    assert_format(&out, r##"object(Dom\XMLDocument)#1 (33) {
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
  string(%d) "%s"
  ["documentURI"]=>
  string(%d) "%s"
  ["characterSet"]=>
  string(5) "UTF-8"
  ["charset"]=>
  string(5) "UTF-8"
  ["inputEncoding"]=>
  string(5) "UTF-8"
  ["doctype"]=>
  NULL
  ["documentElement"]=>
  string(22) "(object value omitted)"
  ["children"]=>
  string(22) "(object value omitted)"
  ["firstElementChild"]=>
  string(22) "(object value omitted)"
  ["lastElementChild"]=>
  string(22) "(object value omitted)"
  ["childElementCount"]=>
  int(1)
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
  string(%d) "%s"
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
}"##);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_saveXML_node.phpt`: Dom\XMLDocument::saveXml(File) node
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_xmldocument_save_xml_node() {
    if skip_without_xml_native("test_xmldocument_save_xml_node") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createEmpty("1.0", "ASCII");
$root = $dom->appendChild($dom->createElement("root"));
$child1 = $root->appendChild($dom->createElement("child1"));
$child2 = $root->appendChild($dom->createElement("child2"));
echo $dom->saveXml($child1);

"#);
    assert_exact(&out, r#"<child1/>"#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_xpath.phpt`: Dom\XMLDocument interaction with XPath
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\XPath"#]
fn test_xmldocument_xpath() {
    if skip_without_xml_native("test_xmldocument_xpath") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString(<<<XML
<?xml version="1.0"?>
<root>
    <p>hi</p>
    <element xmlns="urn:e" xmlns:a="urn:a">
        <?target data?>
        <!-- comment -->
    </element>
</root>
XML);

$xpath = new Dom\XPath($dom);

echo "--- Get data of the paragraph ---\n";

$result = $xpath->query("//p");
var_dump($result);
var_dump($result->item(0)->textContent);

$result = $xpath->evaluate("//p");
var_dump($result);
var_dump($result->item(0)->textContent);

var_dump($xpath->evaluate("string(//p)"));
var_dump($xpath->evaluate("string-length(//p)"));
var_dump($xpath->evaluate("boolean(//p)"));

echo "--- Get data of special nodes ---\n";

$result = $xpath->query("//*/comment()|//*/processing-instruction()");
foreach ($result as $item) {
    var_dump(get_class($item));
    var_dump($item->textContent);
}

echo "--- Get a namespace node ---\n";

// Namespace nodes don't exist in modern day DOM.
try {
    var_dump($xpath->evaluate("//*/namespace::*"));
} catch (DOMException $e) {
    echo $e->getCode(), ": ", $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"--- Get data of the paragraph ---
object(Dom\NodeList)#4 (1) {
  ["length"]=>
  int(1)
}
string(2) "hi"
object(Dom\NodeList)#5 (1) {
  ["length"]=>
  int(1)
}
string(2) "hi"
string(2) "hi"
float(2)
bool(true)
--- Get data of special nodes ---
string(25) "Dom\ProcessingInstruction"
string(4) "data"
string(11) "Dom\Comment"
string(9) " comment "
--- Get a namespace node ---
9: The namespace axis is not well-defined in the living DOM specification. Use Dom\Element::getInScopeNamespaces() or Dom\Element::getDescendantNamespaces() instead."#);
}

/// php-src `ext/dom/tests/modern/xml/XMLDocument_xpath_errors.phpt`: Dom\XMLDocument interaction with XPath evaluate - errors
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\XPath"#]
fn test_xmldocument_xpath_errors() {
    if skip_without_xml_native("test_xmldocument_xpath_errors") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root/>');

$xpath = new Dom\XPath($dom);

try {
    var_dump($xpath->evaluate('-'));
} catch (Error $e) {
    var_dump($e->getMessage());
}

"#);
    assert_format(&out, r#"Warning: Dom\XPath::evaluate(): Invalid expression in %s on line %d
string(35) "Could not evaluate XPath expression""#);
}

/// php-src `ext/dom/tests/modern/xml/attribute_getNamedItemNS_incorrect_ns_check.phpt`: getNamedItemNS() incorrect namespace check
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_attribute_get_named_item_ns_incorrect_ns_check() {
    if skip_without_xml_native("test_attribute_get_named_item_ns_incorrect_ns_check") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php 

$xml = Dom\XMLDocument::createFromString(<<<EOXML
<?xml version="1.0" encoding="UTF-8"?>
<note
      xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
      xsi:noNamespaceSchemaLocation="note-nonamespace1.xsd http://localhost/note-nonamespace2.xsd"
      xsi:schemaLocation="http://www.happy-helpers1.com note-namespace1.xsd http://www.happy-helpers2.com http://localhost/note-namespace2.xsd">
</note>
EOXML
);
$documentElement = $xml->documentElement;
$attributes = $documentElement->attributes;
$schemaLocation = $attributes->getNamedItemNS('http://www.w3.org/2001/XMLSchema-instance', 'schemaLocation');
var_dump($schemaLocation->textContent);

"#);
    assert_exact(&out, r#"string(116) "http://www.happy-helpers1.com note-namespace1.xsd http://www.happy-helpers2.com http://localhost/note-namespace2.xsd""#);
}

/// php-src `ext/dom/tests/modern/xml/bug48339.phpt`: Bug #48339 (DOMElement::setIdAttribute() throws 'Not Found Error' exception)
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_bug48339() {
    if skip_without_xml_native("test_bug48339") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php
$document = Dom\XMLDocument::createEmpty();
$element = $document->createElement('test');
$element = $document->appendChild($element);
$element->setAttribute("xml:id", 1);
$element->setIdAttribute("xml:id", TRUE);
echo $document->saveXml(), "\n";
"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<test xml:id="1"/>"#);
}

/// php-src `ext/dom/tests/modern/xml/canonicalize_unattached.phpt`: Canonicalize unattached node should fail
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\DOMException"#]
fn test_canonicalize_unattached() {
    if skip_without_xml_native("test_canonicalize_unattached") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$d = \Dom\XMLDocument::createFromString('<root><child/></root>');
$child = $d->documentElement->firstChild;
$child->remove();

try {
    $child->C14N();
} catch (Dom\DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Canonicalization can only happen on nodes attached to a document."#);
}

/// php-src `ext/dom/tests/modern/xml/gh17572.phpt`: GH-17572 (getElementsByTagName returns collections with tagName-based indexing, causing loss of elements when converted to arrays)
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::querySelectorAll"#]
fn test_gh17572() {
    if skip_without_xml_native("test_gh17572") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString(<<<XML
<!DOCTYPE root [
    <!ENTITY a "a">
]>
<root>
    <p/>
    <p xmlns:x="urn:x" x:a="1" b="2" a="3"/>
</root>
XML);

echo "--- querySelectorAll('p') ---\n";

foreach ($dom->querySelectorAll('p') as $k => $v) {
    var_dump($k, $v->nodeName);
}

echo "--- getElementsByTagName('p') ---\n";

foreach ($dom->getElementsByTagName('p') as $k => $v) {
    var_dump($k, $v->nodeName);
}

echo "--- entities ---\n";

foreach ($dom->doctype->entities as $k => $v) {
    var_dump($k, $v->nodeName);
}

echo "--- attributes ---\n";

$attribs = $dom->getElementsByTagName('p')[1]->attributes;
foreach ($attribs as $k => $v) {
    var_dump($k, $v->value);
    var_dump($attribs[$k]->value);
}

"#);
    assert_exact(&out, r#"--- querySelectorAll('p') ---
int(0)
string(1) "p"
int(1)
string(1) "p"
--- getElementsByTagName('p') ---
int(0)
string(1) "p"
int(1)
string(1) "p"
--- entities ---
string(1) "a"
string(1) "a"
--- attributes ---
string(7) "xmlns:x"
string(5) "urn:x"
string(5) "urn:x"
string(3) "x:a"
string(1) "1"
string(1) "1"
string(1) "b"
string(1) "2"
string(1) "2"
string(1) "a"
string(1) "3"
string(1) "3""#);
}

/// php-src `ext/dom/tests/modern/xml/gh18979.phpt`: GH-18979 (DOM\XMLDocument::createComment() triggers undefined behavior with null byte)
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_gh18979() {
    if skip_without_xml_native("test_gh18979") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php
$dom = Dom\XMLDocument::createEmpty();
$container = $dom->createElement("container");
$container->append($dom->createComment("\0"));
var_dump($container->innerHTML);
"#);
    assert_exact(&out, r#"string(7) "<!---->""#);
}

/// php-src `ext/dom/tests/modern/xml/gh20444.phpt`: GH-20444 (Dom\XMLDocument::C14N() seems broken compared to DOMDocument::C14N())
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::C14N"#]
fn test_gh20444() {
    if skip_without_xml_native("test_gh20444") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$xml = <<<EOF
<?xml version="1.0" encoding="UTF-8"?>
<test:root xmlns:test="http://example.com/dummy/ns">
    <test:a xmlns:test="http://example.com/dummy/ns"/>
    <test:b test:kind="123">abc</test:b>
</test:root>
EOF;

$d = \Dom\XMLDocument::createFromString($xml);
var_dump($d->C14N(true));

$xml = <<<EOF
<?xml version="1.0" encoding="UTF-8"?>
<ns1:root xmlns:ns1="http://example.com/dummy/ns">
    <ns1:a/>
    <ns1:b>
        <ns1:c>123</ns1:c>
    </ns1:b>
</ns1:root>
EOF;

$d = \Dom\XMLDocument::createFromString($xml);
var_dump($d->C14N());

"#);
    assert_exact(&out, r#"string(128) "<test:root xmlns:test="http://example.com/dummy/ns">
    <test:a></test:a>
    <test:b test:kind="123">abc</test:b>
</test:root>"
string(134) "<ns1:root xmlns:ns1="http://example.com/dummy/ns">
    <ns1:a></ns1:a>
    <ns1:b>
        <ns1:c>123</ns1:c>
    </ns1:b>
</ns1:root>""#);
}

/// php-src `ext/dom/tests/modern/xml/gh21544.phpt`: GH-21544 (Dom\XMLDocument::C14N() drops namespace declarations on DOM-built documents)
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_gh21544() {
    if skip_without_xml_native("test_gh21544") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$doc = Dom\XMLDocument::createEmpty();
$root = $doc->createElementNS("urn:envelope", "env:Root");
$doc->appendChild($root);
$child = $doc->createElementNS("urn:child", "x:Child");
$root->appendChild($child);

$parsed = Dom\XMLDocument::createFromString(
    '<env:Root xmlns:env="urn:envelope"><x:Child xmlns:x="urn:child"/></env:Root>'
);

echo "DOM-built C14N: " . $doc->C14N() . PHP_EOL;
echo "Parsed C14N:    " . $parsed->C14N() . PHP_EOL;
var_dump($doc->C14N() === $parsed->C14N());

"#);
    assert_exact(&out, r#"DOM-built C14N: <env:Root xmlns:env="urn:envelope"><x:Child xmlns:x="urn:child"></x:Child></env:Root>
Parsed C14N:    <env:Root xmlns:env="urn:envelope"><x:Child xmlns:x="urn:child"></x:Child></env:Root>
bool(true)"#);
}

/// php-src `ext/dom/tests/modern/xml/gh21548.phpt`: GH-21548 (Dom\XMLDocument::C14N() emits duplicate xmlns declarations after setAttributeNS())
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::C14N"#]
fn test_gh21548() {
    if skip_without_xml_native("test_gh21548") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$doc = Dom\XMLDocument::createFromString('<root xmlns="urn:a" attr="val"/>');
$doc->documentElement->setAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns:ns1", "urn:a");

echo $doc->C14N() . PHP_EOL;

"#);
    assert_exact(&out, r#"<root xmlns="urn:a" xmlns:ns1="urn:a" attr="val"></root>"#);
}

/// php-src `ext/dom/tests/modern/xml/return_dom_node_from_xpath.phpt`: Returning a Dom\Node from Dom\XPath callback
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\XPath"#]
fn test_return_dom_node_from_xpath() {
    if skip_without_xml_native("test_return_dom_node_from_xpath") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root/>');
$xpath = new Dom\XPath($dom);
$xpath->registerPhpFunctionNs('urn:x', 'test', fn() => $dom->createElement('foo'));
$xpath->registerNamespace('x', 'urn:x');
$test = $xpath->query('x:test()');
var_dump($test[0]->nodeName);

"#);
    assert_exact(&out, r#"string(3) "foo""#);
}

/// php-src `ext/dom/tests/modern/xml/serialize_LIBXML_NOXMLDECL.phpt`: Serialize modern document with LIBXML_NOXMLDECL
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_serialize_libxml_noxmldecl() {
    if skip_without_xml_native("test_serialize_libxml_noxmldecl") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<?xml version="1.0"?><root/>');
echo $dom->saveXml(options: LIBXML_NOXMLDECL), "\n";

"#);
    assert_exact(&out, r#"<root/>"#);
}

/// php-src `ext/dom/tests/modern/xml/serialize_empty_xmlns.phpt`: XML serializer spec errata: xmlns="" serialization should be allowed
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_serialize_empty_xmlns() {
    if skip_without_xml_native("test_serialize_empty_xmlns") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

// Should be allowed
$dom = Dom\XMLDocument::createFromString('<root><x xmlns=""/></root>');
var_dump($dom->documentElement->innerHTML);

// Should not be allowed
$dom = Dom\XMLDocument::createFromString('<root><x/></root>');
$x = $dom->documentElement->firstChild;
$x->setAttributeNS('http://www.w3.org/2000/xmlns/', 'xmlns:a', '');
try {
    var_dump($dom->documentElement->innerHTML);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"string(13) "<x xmlns=""/>"
The resulting XML serialization is not well-formed"#);
}

/// php-src `ext/dom/tests/modern/xml/simplexml_interop.phpt`: Interoperability with SimpleXML
#[test]
#[ignore = r#"DOM port pending: Undefined function: Dom\import_simplexml"#]
fn test_simplexml_interop() {
    if skip_without_xml_native("test_simplexml_interop") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

$sxe = simplexml_load_string('<container xmlns="urn:a">foo</container>');

$element = Dom\import_simplexml($sxe);
var_dump($element->attributes);
echo $element->ownerDocument->saveXml($element), "\n";

$element->appendChild($element->ownerDocument->createElementNS('urn:a', 'child'));
echo $element->ownerDocument->saveXml($element), "\n";

$sxe->addChild('name', 'value');
echo $element->ownerDocument->saveXml($element), "\n";

// This should fail because it has been imported already above in modern DOM
try {
    dom_import_simplexml($sxe);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}

// Import again and compare
var_dump(Dom\import_simplexml($sxe) === $element);

"#);
    assert_exact(&out, r#"object(Dom\NamedNodeMap)#3 (1) {
  ["length"]=>
  int(1)
}
<container xmlns="urn:a">foo</container>
<container xmlns="urn:a">foo<child/></container>
<container xmlns="urn:a">foo<child/><name>value</name></container>
dom_import_simplexml(): Argument #1 ($node) must not be already imported as a Dom\Node
bool(true)"#);
}

/// php-src `ext/dom/tests/modern/xml/xpath_query_context_in_scope_ns.phpt`: Test Dom\XPath::query() with registering a context node's in-scope namespaces
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::getElementsByTagNameNS"#]
fn test_xpath_query_context_in_scope_ns() {
    if skip_without_xml_native("test_xpath_query_context_in_scope_ns") {
        return;
    }
    let out = run_case(r#"modern/xml"#, &[], r#"<?php

function dump($xpath, $query, $context) {
    echo "--- $query ---\n";
    $nodes = $xpath->query($query, $context);
    foreach ($nodes as $node) {
        echo "nodeName: ";
        var_dump($node->nodeName);
        echo "prefix: ";
        var_dump($node->prefix);
        echo "namespaceURI: ";
        var_dump($node->namespaceURI);
    }
    if (count($nodes) === 0) {
        echo "No nodes found\n";
    }
}

$dom = Dom\XMLDocument::createFromString(<<<XML
<a xmlns="urn:a" xmlns:d="urn:d">
    <b:b xmlns:b="urn:b">
        <c:c1 xmlns:c="urn:c1">
            <b:bar/>
            <c:c2 xmlns:c="urn:c2"/>
            <c:c3 xmlns:c="urn:c3"/>
            <c:c4 xmlns:c="urn:c1"/>
            <d:d/>
            <a/>
        </c:c1>
    </b:b>
</a>
XML);

$c1 = $dom->getElementsByTagNameNS('*', 'c1')->item(0);
$c2 = $dom->getElementsByTagNameNS('*', 'c1')->item(0);

$xpath = new Dom\XPath($dom);

echo "=== Tests with c1 ===\n\n";

dump($xpath, '//b:bar', $c1);
dump($xpath, '//c:c1', $c1);
dump($xpath, '//c:c2', $c1);
dump($xpath, '//c:c3', $c1);
dump($xpath, '//c:c4', $c1);
dump($xpath, '//d:d', $c1);
dump($xpath, '//a', $c1);

echo "\n=== Tests with c2 ===\n\n";

dump($xpath, '//c:c1', $c2);
dump($xpath, '//c:c2', $c2);
dump($xpath, '//d:d', $c2);

"#);
    assert_exact(&out, r#"=== Tests with c1 ===

--- //b:bar ---
nodeName: string(5) "b:bar"
prefix: string(1) "b"
namespaceURI: string(5) "urn:b"
--- //c:c1 ---
nodeName: string(4) "c:c1"
prefix: string(1) "c"
namespaceURI: string(6) "urn:c1"
--- //c:c2 ---
No nodes found
--- //c:c3 ---
No nodes found
--- //c:c4 ---
nodeName: string(4) "c:c4"
prefix: string(1) "c"
namespaceURI: string(6) "urn:c1"
--- //d:d ---
nodeName: string(3) "d:d"
prefix: string(1) "d"
namespaceURI: string(5) "urn:d"
--- //a ---
No nodes found

=== Tests with c2 ===

--- //c:c1 ---
nodeName: string(4) "c:c1"
prefix: string(1) "c"
namespaceURI: string(6) "urn:c1"
--- //c:c2 ---
No nodes found
--- //d:d ---
nodeName: string(3) "d:d"
prefix: string(1) "d"
namespaceURI: string(5) "urn:d""#);
}
