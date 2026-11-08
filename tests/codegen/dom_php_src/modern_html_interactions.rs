//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `modern/html/interactions` folder, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/modern/html/interactions/Document_body_getter.phpt`: Test Dom\Document::$body getter
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_document_body_getter() {
    if skip_without_xml_native("test_document_body_getter") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

echo "--- From parsing ---\n";

$dom = Dom\HTMLDocument::createFromString("<p>foo</p>", LIBXML_NOERROR);
var_dump($dom->body?->nodeName);

echo "--- After body removal ---\n";

$dom->body->remove();
var_dump($dom->body?->nodeName);

echo "--- body in no namespace ---\n";

$tmp = $dom->documentElement->appendChild($dom->createElementNS("", "body"));
var_dump($dom->body?->nodeName);
$tmp->remove();

echo "--- frameset in no namespace ---\n";

$tmp = $dom->documentElement->appendChild($dom->createElementNS("", "frameset"));
var_dump($dom->body?->nodeName);
$tmp->remove();

echo "--- body in right namespace ---\n";

$tmp = $dom->documentElement->appendChild($dom->createElementNS("http://www.w3.org/1999/xhtml", "body"));
var_dump($dom->body?->nodeName);
$tmp->remove();

echo "--- frameset in right namespace ---\n";

$tmp = $dom->documentElement->appendChild($dom->createElementNS("http://www.w3.org/1999/xhtml", "frameset"));
var_dump($dom->body?->nodeName);
$tmp->remove();

echo "--- prefixed body in right namespace ---\n";

$tmp = $dom->documentElement->appendChild($dom->createElementNS("http://www.w3.org/1999/xhtml", "prefix:body"));
var_dump($dom->body?->nodeName);
$tmp->remove();

echo "--- prefixed frameset in right namespace ---\n";

$tmp = $dom->documentElement->appendChild($dom->createElementNS("http://www.w3.org/1999/xhtml", "prefix:frameset"));
var_dump($dom->body?->nodeName);
$tmp->remove();

echo "--- multiple body-like elements in right namespace ---\n";

$tmp1 = $dom->documentElement->appendChild($dom->createElementNS("http://www.w3.org/1999/xhtml", "prefix1:body"));
var_dump($dom->body?->nodeName);
$tmp2 = $dom->documentElement->appendChild($dom->createElementNS("http://www.w3.org/1999/xhtml", "prefix2:frameset"));
var_dump($dom->body?->nodeName);
$tmp1->remove();
var_dump($dom->body?->nodeName);
$tmp2->remove();
var_dump($dom->body?->nodeName);

echo "--- html element in no namespace ---\n";

$dom = Dom\XMLDocument::createFromString(<<<XML
<html xmlns="">
  <body/>
</html>
XML);
var_dump($dom->body);

"#);
    assert_exact(&out, r#"--- From parsing ---
string(4) "BODY"
--- After body removal ---
NULL
--- body in no namespace ---
NULL
--- frameset in no namespace ---
NULL
--- body in right namespace ---
string(4) "BODY"
--- frameset in right namespace ---
string(8) "FRAMESET"
--- prefixed body in right namespace ---
string(11) "PREFIX:BODY"
--- prefixed frameset in right namespace ---
string(15) "PREFIX:FRAMESET"
--- multiple body-like elements in right namespace ---
string(12) "PREFIX1:BODY"
string(12) "PREFIX1:BODY"
string(16) "PREFIX2:FRAMESET"
NULL
--- html element in no namespace ---
NULL"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/Document_body_setter.phpt`: Test DOM\Document::$body setter
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOM\HTMLDocument"#]
fn test_document_body_setter() {
    if skip_without_xml_native("test_document_body_setter") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

echo "--- Replace body with itself ---\n";
$dom = DOM\HTMLDocument::createFromString('<p>foo</p>', LIBXML_NOERROR);
$dom->body = $dom->body;
var_dump($dom->body?->nodeName);

echo "--- Add body when there is no body yet ---\n";
$dom = DOM\HTMLDocument::createFromString('<p>foo</p>', LIBXML_NOERROR);
$dom->body->remove();
$dom->body = $dom->createElementNS("http://www.w3.org/1999/xhtml", "prefix:body");
var_dump($dom->body?->nodeName);

echo "--- Replace old body with new body ---\n";
$dom = DOM\HTMLDocument::createFromString('<p>foo</p>', LIBXML_NOERROR);
$dom->body = $dom->createElementNS("http://www.w3.org/1999/xhtml", "prefix:body");
var_dump($dom->body?->nodeName);

echo "--- Replace old body with new body, while still having a reference to the old body ---\n";
$dom = DOM\HTMLDocument::createFromString('<p>foo</p>', LIBXML_NOERROR);
$old = $dom->body;
$dom->body = $dom->createElementNS("http://www.w3.org/1999/xhtml", "prefix:body");
var_dump($dom->body?->nodeName);
var_dump($old->nodeName);

echo "--- Special note from the DOM spec ---\n";
$dom = DOM\XMLDocument::createFromString('<svg xmlns="http://www.w3.org/2000/svg"/>');
$dom->body = $dom->createElementNS("http://www.w3.org/1999/xhtml", "body");
var_dump($dom->body?->nodeName);

"#);
    assert_exact(&out, r#"--- Replace body with itself ---
string(4) "BODY"
--- Add body when there is no body yet ---
string(11) "PREFIX:BODY"
--- Replace old body with new body ---
string(11) "PREFIX:BODY"
--- Replace old body with new body, while still having a reference to the old body ---
string(11) "PREFIX:BODY"
string(4) "BODY"
--- Special note from the DOM spec ---
NULL"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/Document_body_setter_errors.phpt`: Test DOM\Document::$body setter errors
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOM\HTMLDocument"#]
fn test_document_body_setter_errors() {
    if skip_without_xml_native("test_document_body_setter_errors") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

function testNormalReplace($cb)
{
    $dom = DOM\HTMLDocument::createFromString('<p>foo</p>', LIBXML_NOERROR);
    var_dump($dom->body?->nodeName);
    try {
        $dom->body = $cb($dom);
    } catch (Throwable $e) {
        echo $e->getMessage(), "\n";
    }
    var_dump($dom->body?->nodeName);
}

echo "--- Set body to NULL ---\n";
testNormalReplace(fn ($dom) => NULL);

echo "--- Wrong element tag in right namespace ---\n";
testNormalReplace(fn ($dom) => $dom->createElementNS("http://www.w3.org/1999/xhtml", "foo"));

echo "--- Right element tag in wrong namespace ---\n";
testNormalReplace(fn ($dom) => $dom->createElementNS("urn:a", "body"));

echo "--- Right element tag in no namespace ---\n";
testNormalReplace(fn ($dom) => $dom->createElementNS("", "frameset"));

echo "--- Set body without document element ---\n";
$dom = DOM\XMLDocument::createEmpty();
try {
    $dom->body = $dom->createElementNS("http://www.w3.org/1999/xhtml", "body");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
var_dump($dom->body?->nodeName);

"#);
    assert_exact(&out, r#"--- Set body to NULL ---
string(4) "BODY"
The new body must either be a body or a frameset tag
string(4) "BODY"
--- Wrong element tag in right namespace ---
string(4) "BODY"
The new body must either be a body or a frameset tag
string(4) "BODY"
--- Right element tag in wrong namespace ---
string(4) "BODY"
Cannot assign Dom\Element to property Dom\Document::$body of type ?Dom\HTMLElement
string(4) "BODY"
--- Right element tag in no namespace ---
string(4) "BODY"
Cannot assign Dom\Element to property Dom\Document::$body of type ?Dom\HTMLElement
string(4) "BODY"
--- Set body without document element ---
A body can only be set if there is a document element
NULL"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/Document_head.phpt`: Test Dom\Document::$head
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_document_head() {
    if skip_without_xml_native("test_document_head") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

echo "--- From parsing ---\n";

$dom = Dom\HTMLDocument::createFromString("<p>foo</p>", LIBXML_NOERROR);
var_dump($dom->head?->nodeName);

echo "--- After head removal ---\n";

$dom->head->remove();
var_dump($dom->head?->nodeName);

echo "--- head in no namespace ---\n";

$tmp = $dom->documentElement->appendChild($dom->createElementNS("", "head"));
var_dump($dom->head?->nodeName);
$tmp->remove();

echo "--- head in right namespace ---\n";

$tmp = $dom->documentElement->appendChild($dom->createElementNS("http://www.w3.org/1999/xhtml", "head"));
var_dump($dom->head?->nodeName);
$tmp->remove();

echo "--- prefixed head in right namespace ---\n";

$tmp = $dom->documentElement->appendChild($dom->createElementNS("http://www.w3.org/1999/xhtml", "prefix:head"));
var_dump($dom->head?->nodeName);
$tmp->remove();

echo "--- multiple head elements in right namespace ---\n";

$tmp1 = $dom->documentElement->appendChild($dom->createElementNS("http://www.w3.org/1999/xhtml", "prefix1:head"));
var_dump($dom->head?->nodeName);
$tmp2 = $dom->documentElement->appendChild($dom->createElementNS("http://www.w3.org/1999/xhtml", "prefix2:head"));
var_dump($dom->head?->nodeName);
$tmp1->remove();
var_dump($dom->head?->nodeName);
$tmp2->remove();
var_dump($dom->head?->nodeName);

echo "--- html element in no namespace ---\n";

$dom = Dom\XMLDocument::createFromString(<<<XML
<html xmlns="">
  <head/>
</html>
XML);
var_dump($dom->head);

"#);
    assert_exact(&out, r#"--- From parsing ---
string(4) "HEAD"
--- After head removal ---
NULL
--- head in no namespace ---
NULL
--- head in right namespace ---
string(4) "HEAD"
--- prefixed head in right namespace ---
string(11) "PREFIX:HEAD"
--- multiple head elements in right namespace ---
string(12) "PREFIX1:HEAD"
string(12) "PREFIX1:HEAD"
string(12) "PREFIX2:HEAD"
NULL
--- html element in no namespace ---
NULL"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/Dom_Element_insertAdjacentHTML.phpt`: Dom\Element::insertAdjacentHTML() with HTML nodes
#[test]
#[ignore = r#"DOM port pending: Undefined class constant: Dom\AdjacentPosition::BeforeBegin"#]
fn test_dom_element_insert_adjacent_html() {
    if skip_without_xml_native("test_dom_element_insert_adjacent_html") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

const POSITIONS = [
    Dom\AdjacentPosition::BeforeBegin,
    Dom\AdjacentPosition::AfterBegin,
    Dom\AdjacentPosition::BeforeEnd,
    Dom\AdjacentPosition::AfterEnd,
];

function test(string $html) {
    echo "=== HTML ($html) ===\n";

    foreach (POSITIONS as $position) {
        echo "--- Position ", $position->name, " ---\n";

        $dom = Dom\HTMLDocument::createFromString("<div></div>", LIBXML_NOERROR);
        $div = $dom->body->firstChild;
        $div->append("Sample text");

        $div->insertAdjacentHTML($position, $html);

        echo $dom->saveXML(), "\n";
        echo $dom->saveHTML(), "\n";
        var_dump($div->childNodes->length);
        var_dump($dom->body->childNodes->length);
    }
}

test("<p>foo</p><p>bar</p>");
test("text");
test("");

"#);
    assert_exact(&out, r#"=== HTML (<p>foo</p><p>bar</p>) ===
--- Position BeforeBegin ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><p>foo</p><p>bar</p><div>Sample text</div></body></html>
<html><head></head><body><p>foo</p><p>bar</p><div>Sample text</div></body></html>
int(1)
int(3)
--- Position AfterBegin ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><div><p>foo</p><p>bar</p>Sample text</div></body></html>
<html><head></head><body><div><p>foo</p><p>bar</p>Sample text</div></body></html>
int(3)
int(1)
--- Position BeforeEnd ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><div>Sample text<p>foo</p><p>bar</p></div></body></html>
<html><head></head><body><div>Sample text<p>foo</p><p>bar</p></div></body></html>
int(3)
int(1)
--- Position AfterEnd ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><div>Sample text</div><p>foo</p><p>bar</p></body></html>
<html><head></head><body><div>Sample text</div><p>foo</p><p>bar</p></body></html>
int(1)
int(3)
=== HTML (text) ===
--- Position BeforeBegin ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body>text<div>Sample text</div></body></html>
<html><head></head><body>text<div>Sample text</div></body></html>
int(1)
int(2)
--- Position AfterBegin ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><div>textSample text</div></body></html>
<html><head></head><body><div>textSample text</div></body></html>
int(2)
int(1)
--- Position BeforeEnd ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><div>Sample texttext</div></body></html>
<html><head></head><body><div>Sample texttext</div></body></html>
int(2)
int(1)
--- Position AfterEnd ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><div>Sample text</div>text</body></html>
<html><head></head><body><div>Sample text</div>text</body></html>
int(1)
int(2)
=== HTML () ===
--- Position BeforeBegin ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><div>Sample text</div></body></html>
<html><head></head><body><div>Sample text</div></body></html>
int(1)
int(1)
--- Position AfterBegin ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><div>Sample text</div></body></html>
<html><head></head><body><div>Sample text</div></body></html>
int(1)
int(1)
--- Position BeforeEnd ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><div>Sample text</div></body></html>
<html><head></head><body><div>Sample text</div></body></html>
int(1)
int(1)
--- Position AfterEnd ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><div>Sample text</div></body></html>
<html><head></head><body><div>Sample text</div></body></html>
int(1)
int(1)"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/Dom_Element_insertAdjacentHTML_edge_case.phpt`: Dom\Element::insertAdjacentHTML() with HTML nodes - edge case
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_dom_element_insert_adjacent_html_edge_case() {
    if skip_without_xml_native("test_dom_element_insert_adjacent_html_edge_case") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString("", LIBXML_NOERROR);

$fragment = $dom->createDocumentFragment();
$node = $fragment->appendChild($dom->createElement("node"));

$node->insertAdjacentHTML(Dom\AdjacentPosition::BeforeBegin, "<p>foo</p>");

echo $dom->saveHtml($fragment), "\n";

$dom->firstChild->insertAdjacentHTML(Dom\AdjacentPosition::AfterBegin, $node->outerHTML);

echo $dom->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<p>foo</p><node></node>
<html><node></node><head></head><body></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/Dom_Element_insertAdjacentHTML_errors.phpt`: Dom\Element::insertAdjacentHTML() with HTML nodes - error conditions
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_dom_element_insert_adjacent_html_errors() {
    if skip_without_xml_native("test_dom_element_insert_adjacent_html_errors") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$element = $dom->createElement('root');

echo "--- BeforeBegin no parent ---\n";

try {
    $element->insertAdjacentHTML(Dom\AdjacentPosition::BeforeBegin, "test");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- AfterEnd no parent ---\n";

try {
    $element->insertAdjacentHTML(Dom\AdjacentPosition::AfterEnd, "test");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

$dom->appendChild($element);

echo "--- BeforeBegin document parent ---\n";

try {
    $element->insertAdjacentHTML(Dom\AdjacentPosition::BeforeBegin, "test");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "--- AfterEnd document parent ---\n";

try {
    $element->insertAdjacentHTML(Dom\AdjacentPosition::AfterEnd, "test");
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"--- BeforeBegin no parent ---
No Modification Allowed Error
--- AfterEnd no parent ---
No Modification Allowed Error
--- BeforeBegin document parent ---
No Modification Allowed Error
--- AfterEnd document parent ---
No Modification Allowed Error"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/HTMLCollection_dimension_errors.phpt`: HTMLCollection::namedItem() and dimension handling for named accesses
#[test]
#[ignore = r#"DOM port pending: Invalid assignment target"#]
fn test_htmlcollection_dimension_errors() {
    if skip_without_xml_native("test_htmlcollection_dimension_errors") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString('<root/>');

try {
    $dom->getElementsByTagName('root')[][1] = 1;
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

try {
    $dom->getElementsByTagName('root')[true];
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

try {
    isset($dom->getElementsByTagName('root')[true]);
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Cannot append to Dom\HTMLCollection
Cannot access offset of type bool on Dom\HTMLCollection
Cannot access offset of type bool in isset or empty"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/HTMLCollection_named_reads.phpt`: HTMLCollection::namedItem() and dimension handling for named accesses
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMNodeList::namedItem"#]
fn test_htmlcollection_named_reads() {
    if skip_without_xml_native("test_htmlcollection_named_reads") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

$xml = <<<XML
<!DOCTYPE root [
    <!ENTITY ent "test">
]>
<root>
    <node name="foo">1</node>
    <x id="foo">2</x>
    <x id="foo&ent;">2 with entity</x>
    <node test:id="foo" xmlns:test="http://example.com">3</node>
    <node id="wrong">4</node>
    <node id="foo">5</node>
    <node name="bar">without html ns</node>
    <node name="bar" xmlns="http://www.w3.org/1999/xhtml">with html ns</node>
</root>
XML;

$dom = Dom\XMLDocument::createFromString($xml);

function test($obj, $name) {
    echo "--- Query \"$name\" ---\n";
    var_dump($obj->namedItem($name)?->textContent);
    var_dump($obj[$name]?->textContent);
    var_dump(isset($obj[$name]));

    // Search to check for dimension access consistency
    $node = $obj[$name];
    if ($node) {
        $found = false;
        for ($i = 0; $i < $obj->length && !$found; $i++) {
            $found = $obj[$i] === $node;
        }
        if (!$found) {
            throw new Error('inconsistency in dimension access');
        }
    }
}

test($dom->getElementsByTagName('node'), 'foo');
test($dom->getElementsByTagName('node'), '');
test($dom->getElementsByTagName('node'), 'does not exist');
test($dom->getElementsByTagName('node'), 'wrong');
test($dom->getElementsByTagName('node'), 'bar');
test($dom->getElementsByTagName('x'), 'foo');
test($dom->getElementsByTagName('x'), 'footest');

"#);
    assert_exact(&out, r#"--- Query "foo" ---
string(1) "5"
string(1) "5"
bool(true)
--- Query "" ---
NULL
NULL
bool(false)
--- Query "does not exist" ---
NULL
NULL
bool(false)
--- Query "wrong" ---
string(1) "4"
string(1) "4"
bool(true)
--- Query "bar" ---
string(12) "with html ns"
string(12) "with html ns"
bool(true)
--- Query "foo" ---
string(1) "2"
string(1) "2"
bool(true)
--- Query "footest" ---
string(13) "2 with entity"
string(13) "2 with entity"
bool(true)"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/HTMLDocument_clone.phpt`: Cloning a Dom\HTMLDocument
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_clone() {
    if skip_without_xml_native("test_htmldocument_clone") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString("<p>foo</p>");

$dom2 = clone $dom;
var_dump($dom2->firstChild->tagName);
var_dump($dom2->firstChild->textContent);

$element = $dom2->firstChild;
unset($dom2);
var_dump(get_class($element->ownerDocument));

"#);
    assert_format(&out, r#"Warning: Dom\HTMLDocument::createFromString(): tree error unexpected-token-in-initial-mode in Entity, line: 1, column: 2 in %s on line %d
string(4) "HTML"
string(3) "foo"
string(16) "Dom\HTMLDocument""#);
}

/// php-src `ext/dom/tests/modern/html/interactions/HTMLDocument_documentURI.phpt`: Dom\HTMLDocument::documentURI
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_document_uri() {
    if skip_without_xml_native("test_htmldocument_document_uri") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromFile(__DIR__ . "/test foo.html", LIBXML_NOERROR);
var_dump($dom->documentURI);

$memory = fopen("php://memory", "w+");
fwrite($memory, "foobar");
rewind($memory);
$dom = Dom\HTMLDocument::createFromFile("php://memory");
var_dump($dom->documentURI);
fclose($memory);

class DummyWrapper {
    public $context;

    public function stream_open($path, $mode, $options, &$opened_path) {
        return true;
    }

    public function stream_read($count) {
        return "";
    }

    public function stream_eof() {
        return true;
    }

    public function stream_close() {
        return true;
    }
}

stream_wrapper_register("dummy", DummyWrapper::class);

$dom = Dom\HTMLDocument::createFromFile("dummy://foo/ bar");
var_dump($dom->documentURI);

"#);
    assert_format(&out, r#"string(%d) "file://%stest%sfoo.html"
string(12) "php://memory"
string(16) "dummy://foo/ bar""#);
}

/// php-src `ext/dom/tests/modern/html/interactions/HTMLDocument_getElementsByTagName.phpt`: Test Dom\HTMLDocument::getElementsByTagName(NS)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_get_elements_by_tag_name() {
    if skip_without_xml_native("test_htmldocument_get_elements_by_tag_name") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE html>
<html>
<head>
    <title>Test</title>
</head>
<body>
    <h1>Test</h1>
    <p>Test</p>
    <svg width="100" height="100">
        <circle cx="0" cy="0" r="10"/>
    </svg>
    <math>
        <mtable id="table"></mtable>
    </math>
</body>
</html>
HTML);

echo "--- getElementsByTagName ---\n";

var_dump($dom->getElementsByTagName("p")[0]?->nodeName);
var_dump($dom->getElementsByTagName("math")[0]?->nodeName);
var_dump($dom->getElementsByTagName("mtable")[0]?->nodeName);
var_dump($dom->getElementsByTagName("svg")[0]?->nodeName);
var_dump($dom->getElementsByTagName("circle")[0]?->nodeName);

echo "--- getElementsByTagNameNS (*) ---\n";

var_dump($dom->getElementsByTagNameNS("*", "p")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("*", "math")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("*", "mtable")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("*", "svg")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("*", "circle")[0]?->nodeName);

echo "--- getElementsByTagNameNS (xhtml) ---\n";

var_dump($dom->getElementsByTagNameNS("http://www.w3.org/1999/xhtml", "p")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("http://www.w3.org/1999/xhtml", "math")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("http://www.w3.org/1999/xhtml", "mtable")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("http://www.w3.org/1999/xhtml", "svg")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("http://www.w3.org/1999/xhtml", "circle")[0]?->nodeName);

echo "--- getElementsByTagNameNS (svg) ---\n";

var_dump($dom->getElementsByTagNameNS("http://www.w3.org/2000/svg", "p")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("http://www.w3.org/2000/svg", "math")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("http://www.w3.org/2000/svg", "mtable")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("http://www.w3.org/2000/svg", "svg")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("http://www.w3.org/2000/svg", "circle")[0]?->nodeName);

echo "--- getElementsByTagNameNS (math) ---\n";

var_dump($dom->getElementsByTagNameNS("http://www.w3.org/1998/Math/MathML", "p")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("http://www.w3.org/1998/Math/MathML", "math")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("http://www.w3.org/1998/Math/MathML", "mtable")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("http://www.w3.org/1998/Math/MathML", "svg")[0]?->nodeName);
var_dump($dom->getElementsByTagNameNS("http://www.w3.org/1998/Math/MathML", "circle")[0]?->nodeName);

"#);
    assert_exact(&out, r#"--- getElementsByTagName ---
string(1) "P"
string(4) "math"
string(6) "mtable"
string(3) "svg"
string(6) "circle"
--- getElementsByTagNameNS (*) ---
string(1) "P"
string(4) "math"
string(6) "mtable"
string(3) "svg"
string(6) "circle"
--- getElementsByTagNameNS (xhtml) ---
string(1) "P"
NULL
NULL
NULL
NULL
--- getElementsByTagNameNS (svg) ---
NULL
NULL
NULL
string(3) "svg"
string(6) "circle"
--- getElementsByTagNameNS (math) ---
NULL
string(4) "math"
string(6) "mtable"
NULL
NULL"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/HTMLDocument_registerNodeClass_01.phpt`: Dom\HTMLDocument::registerNodeClass 01
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::registerNodeClass"#]
fn test_htmldocument_register_node_class_01() {
    if skip_without_xml_native("test_htmldocument_register_node_class_01") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

$dom = new DOMDocument();
$dom->registerNodeClass("Dom\\HTMLDocument", "DOMDocument");

"#);
    assert_fatal_format(&out, r#"Fatal error: Uncaught TypeError: DOMDocument::registerNodeClass(): Argument #1 ($baseClass) must be a class name derived from DOMNode, Dom\HTMLDocument given in %s:%d
Stack trace:
#0 %s(%d): DOMDocument->registerNodeClass('Dom\\HTMLDocumen...', 'DOMDocument')
#1 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/HTMLDocument_registerNodeClass_02.phpt`: Dom\HTMLDocument::registerNodeClass 02
#[test]
#[ignore = r#"DOM port pending: Unknown class referenced during inheritance flattening: Dom\Document"#]
fn test_htmldocument_register_node_class_02() {
    if skip_without_xml_native("test_htmldocument_register_node_class_02") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

class Custom extends Dom\Document {
    public function foo() {
    }
}

$dom = Dom\HTMLDocument::createEmpty();
try {
    $dom->registerNodeClass("Dom\\Document", "Custom");
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

$element = $dom->appendChild($dom->createElement("foo"));
unset($dom);

var_dump(get_class($element->ownerDocument));

// Should fail
$element->ownerDocument->foo();

"#);
    assert_format(&out, r#"Dom\Document::registerNodeClass(): Argument #1 ($baseClass) must not be an abstract class
string(16) "Dom\HTMLDocument"

Fatal error: Uncaught Error: Call to undefined method Dom\HTMLDocument::foo() in %s:%d
Stack trace:
#0 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/HTMLDocument_registerNodeClass_03.phpt`: Dom\HTMLDocument::registerNodeClass 03
#[test]
#[ignore = r#"DOM port pending: Unknown class referenced during inheritance flattening: Dom\HTMLElement"#]
fn test_htmldocument_register_node_class_03() {
    if skip_without_xml_native("test_htmldocument_register_node_class_03") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

class Custom extends Dom\HTMLElement {
    public int $test = 1;

    public function reverseTagName(): string {
        var_dump($this->test);
        return strrev($this->tagName);
    }
}

$dom = Dom\HTMLDocument::createFromString("<div>foo</div>", LIBXML_NOERROR);
$dom->registerNodeClass("Dom\\HTMLElement", "Custom");

var_dump($dom->getElementsByTagName('div')[0]->reverseTagName());

"#);
    assert_exact(&out, r#"int(1)
string(3) "VID""#);
}

/// php-src `ext/dom/tests/modern/html/interactions/HTMLDocument_saveHTMLFile_empty_path.phpt`: Dom\HTMLDocument::saveHtmlFile() empty path
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_save_htmlfile_empty_path() {
    if skip_without_xml_native("test_htmldocument_save_htmlfile_empty_path") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$root = $dom->appendChild($dom->createElement("root"));
$dom->saveHtmlFile("");

"#);
    assert_fatal_format(&out, r#"Fatal error: Uncaught ValueError: Dom\HTMLDocument::saveHtmlFile(): Argument #1 ($filename) must not be empty in %s:%d
Stack trace:
#0 %s(%d): Dom\HTMLDocument->saveHtmlFile('')
#1 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/HTMLDocument_saveHTML_wrong_document.phpt`: Dom\HTMLDocument::saveHtml() wrong document
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_save_html_wrong_document() {
    if skip_without_xml_native("test_htmldocument_save_html_wrong_document") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$dom->saveHtml(Dom\HTMLDocument::createEmpty());

"#);
    assert_fatal_format(&out, r#"Fatal error: Uncaught DOMException: Wrong Document Error in %s:%d
Stack trace:
#0 %s(%d): Dom\HTMLDocument->saveHtml(Object(Dom\HTMLDocument))
#1 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/HTMLDocument_should_retain_properties_and_owner_01.phpt`: Dom\HTMLDocument should retain properties and ownerDocument relation 01
#[test]
#[ignore = r#"DOM port pending: Unknown class referenced during inheritance flattening: Dom\HTMLElement"#]
fn test_htmldocument_should_retain_properties_and_owner_01() {
    if skip_without_xml_native("test_htmldocument_should_retain_properties_and_owner_01") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

class MyElement extends Dom\HTMLElement {}

$dom = Dom\HTMLDocument::createFromString("<p>foo</p>", LIBXML_NOERROR);
$dom->registerNodeClass("Dom\\HTMLElement", "MyElement");

// Destroy reference to the DOM
$child = $dom->documentElement;
unset($dom);

// Regain reference using the ownerDocument property
// Should be a Dom\HTMLDocument
$dom = $child->ownerDocument;
var_dump($dom);
// Test if property is preserved (any random doc_props property will do)
var_dump(get_class($dom->getElementsByTagName("p")->item(0)));

"#);
    assert_exact(&out, r##"object(Dom\HTMLDocument)#1 (29) {
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
  string(22) "(object value omitted)"
  ["head"]=>
  string(22) "(object value omitted)"
  ["title"]=>
  string(0) ""
  ["nodeType"]=>
  int(13)
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
}
string(9) "MyElement""##);
}

/// php-src `ext/dom/tests/modern/html/interactions/HTMLDocument_should_retain_properties_and_owner_02.phpt`: Dom\HTMLDocument should retain properties and ownerDocument relation 02
#[test]
#[ignore = r#"DOM port pending: Unknown class referenced during inheritance flattening: Dom\HTMLElement"#]
fn test_htmldocument_should_retain_properties_and_owner_02() {
    if skip_without_xml_native("test_htmldocument_should_retain_properties_and_owner_02") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

class MyElement extends Dom\HTMLElement {}

$dom = Dom\HTMLDocument::createFromString("<p>foo</p>", LIBXML_NOERROR);
$dom->registerNodeClass("Dom\\HTMLElement", "MyElement");
$child = $dom->documentElement->appendChild($dom->createElement('html'));

// Destroy reference to the DOM
unset($dom);

// Regain reference using the ownerDocument property
// Should be a Dom\HTMLDocument
$dom = $child->ownerDocument;
var_dump($dom);
// Test if property is preserved (any random doc_props property will do)
var_dump(get_class($dom->getElementsByTagName("p")->item(0)));

"#);
    assert_exact(&out, r##"object(Dom\HTMLDocument)#1 (29) {
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
  string(22) "(object value omitted)"
  ["head"]=>
  string(22) "(object value omitted)"
  ["title"]=>
  string(0) ""
  ["nodeType"]=>
  int(13)
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
}
string(9) "MyElement""##);
}

/// php-src `ext/dom/tests/modern/html/interactions/getLineNo_65536.phpt`: getLineNo() returns the line number of the node >= 65536
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_get_line_no_65536() {
    if skip_without_xml_native("test_get_line_no_65536") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

$html = str_repeat("\n", 65536) . <<<EOF
<!doctype html>
<body>
    <p>hello</p>
</body>
EOF;

$dom = Dom\HTMLDocument::createFromString($html);
var_dump($dom->documentElement->firstChild->nextSibling->firstChild->nextSibling->getLineNo());

"#);
    assert_exact(&out, r#"int(65538)"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/gh20281.phpt`: GH-20281 (\Dom\Document::getElementById() is inconsistent after nodes are removed)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_gh20281() {
    if skip_without_xml_native("test_gh20281") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php
$d = \Dom\HTMLDocument::createFromString('<p id="a">b</p><p id="a">c</p>', LIBXML_NOERROR);
$p = $d->getElementById('a');
$p->remove();
echo $d->getElementById('a')->textContent, "\n";
"#);
    assert_exact(&out, r#"c"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/noscript.phpt`: noscript behaviour
#[test]
#[ignore = r#"DOM port pending: Undefined constant: Dom\HTML_NO_DEFAULT_NS"#]
fn test_noscript() {
    if skip_without_xml_native("test_noscript") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

echo "--- Parsing ---\n";

$dom = Dom\HTMLDocument::createFromString("<!doctype html><html><body><noscript><p>hi</p></noscript></body></html>", Dom\HTML_NO_DEFAULT_NS);
var_dump($dom->documentElement->textContent);
var_dump($dom->documentElement->namespaceURI);
echo $dom->saveHtml(), "\n";
echo $dom->saveXml(), "\n";

echo "--- Modifying the text content: tag ---\n";

$xpath = new Dom\XPath($dom);
$noscript = $xpath->query("//noscript")[0];
$noscript->textContent = "<p>bye</p>";
echo $dom->saveHtml(), "\n";
echo $dom->saveXml(), "\n";

echo "--- Modifying the text content: trick ---\n";

$noscript->textContent = "<!-- </noscript> -->";
echo $dom->saveHtml(), "\n";
echo $dom->saveXml(), "\n";

"#);
    assert_exact(&out, r#"--- Parsing ---
string(2) "hi"
NULL
<!DOCTYPE html><html><head></head><body><noscript><p>hi</p></noscript></body></html>
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE html>
<html><head/><body><noscript><p>hi</p></noscript></body></html>
--- Modifying the text content: tag ---
<!DOCTYPE html><html><head></head><body><noscript>&lt;p&gt;bye&lt;/p&gt;</noscript></body></html>
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE html>
<html><head/><body><noscript>&lt;p&gt;bye&lt;/p&gt;</noscript></body></html>
--- Modifying the text content: trick ---
<!DOCTYPE html><html><head></head><body><noscript>&lt;!-- &lt;/noscript&gt; --&gt;</noscript></body></html>
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE html>
<html><head/><body><noscript>&lt;!-- &lt;/noscript&gt; --&gt;</noscript></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/interactions/without_constructor.phpt`: Tests without running the constructor
#[test]
#[ignore = r#"DOM port pending: ReflectionClass::__construct() requires a string literal class name (dynamic lookup is not yet supported)"#]
fn test_without_constructor() {
    if skip_without_xml_native("test_without_constructor") {
        return;
    }
    let out = run_case(r#"modern/html/interactions"#, &[], r#"<?php

foreach (['Dom\HTMLDocument', 'Dom\XMLDocument'] as $class) {
    try {
        $rc = new ReflectionClass($class);
        $rc->newInstanceWithoutConstructor();
    } catch (ReflectionException $e) {
        echo $e->getMessage(), "\n";
    }
}

"#);
    assert_exact(&out, r#"Class Dom\HTMLDocument is an internal class marked as final that cannot be instantiated without invoking its constructor
Class Dom\XMLDocument is an internal class marked as final that cannot be instantiated without invoking its constructor"#);
}
