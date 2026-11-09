//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for regressions filed as GitHub issues and security advisories, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/gh10234.phpt`: GH-10234 (Setting DOMAttr::textContent results in an empty attribute value.)
#[test]
fn test_gh10234() {
    if skip_without_xml_native("test_gh10234") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$document = new DOMDocument();
$document->loadXML('<element attribute="value"/>');
$attribute = $document->documentElement->getAttributeNode('attribute');

echo "-- Attribute tests --\n";

var_dump($document->saveHTML());
var_dump($attribute->textContent);

$attribute->textContent = 'new value';
var_dump($attribute->textContent);
var_dump($document->saveHTML());

$attribute->textContent = 'hello & world';
var_dump($attribute->textContent);
var_dump($document->saveHTML());

$attribute->textContent = '<b>hi</b>';
var_dump($attribute->textContent);
var_dump($document->saveHTML());

$attribute->textContent = 'quote "test"';
var_dump($attribute->textContent);
var_dump($document->saveHTML());

$attribute->textContent = "quote 'test'";
var_dump($attribute->textContent);
var_dump($document->saveHTML());

$attribute->textContent = "quote '\"test\"'";
var_dump($attribute->textContent);
var_dump($document->saveHTML());

echo "-- Document element tests --\n";

$document->documentElement->textContent = 'hello & world';
var_dump($document->documentElement->textContent);
var_dump($document->saveHTML());

$document->documentElement->textContent = '<b>hi</b>';
var_dump($document->documentElement->textContent);
var_dump($document->saveHTML());

$document->documentElement->textContent = 'quote "test"';
var_dump($document->documentElement->textContent);
var_dump($document->saveHTML());

$document->documentElement->textContent = "quote 'test'";
var_dump($document->documentElement->textContent);
var_dump($document->saveHTML());
"#);
    assert_format(&out, r#"-- Attribute tests --
string(38) "<element attribute="value"></element>
"
string(5) "value"
string(9) "new value"
string(42) "<element attribute="new value"></element>
"
string(13) "hello & world"
string(50) "<element attribute="hello &amp; world"></element>
"
string(9) "<b>hi</b>"
string(%d) "<element attribute=%r("&lt;b&gt;hi&lt;\/b&gt;"|"<b>hi<\/b>")%r></element>
"
string(12) "quote "test""
string(%d) "<element attribute=%r('quote "test"'|"quote &quot;test&quot;")%r></element>
"
string(12) "quote 'test'"
string(45) "<element attribute="quote 'test'"></element>
"
string(14) "quote '"test"'"
string(57) "<element attribute="quote '&quot;test&quot;'"></element>
"
-- Document element tests --
string(13) "hello & world"
string(74) "<element attribute="quote '&quot;test&quot;'">hello &amp; world</element>
"
string(9) "<b>hi</b>"
string(78) "<element attribute="quote '&quot;test&quot;'">&lt;b&gt;hi&lt;/b&gt;</element>
"
string(12) "quote "test""
string(69) "<element attribute="quote '&quot;test&quot;'">quote "test"</element>
"
string(12) "quote 'test'"
string(69) "<element attribute="quote '&quot;test&quot;'">quote 'test'</element>
""#);
}

/// php-src `ext/dom/tests/gh11288.phpt`: GH-11288 (Error: Couldn't fetch DOMElement introduced in 8.2.6, 8.1.19)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createDocumentFragment"#]
fn test_gh11288() {
    if skip_without_xml_native("test_gh11288") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$html = <<<HTML
<!DOCTYPE HTML>
<html>
<span class="unwrap_me"><i><span class="foo">Lorem</span></i><span class="foo">ipsum</span></span>
</html>
HTML;

$dom = new DOMDocument();
$dom->loadHTML($html);

$spans = iterator_to_array($dom->getElementsByTagName('span')->getIterator());
foreach ($spans as $span) {
    if ('unwrap_me' === $span->getAttribute('class')) {
        $fragment = $dom->createDocumentFragment();
        $fragment->append(...$span->childNodes);
        $span->parentNode?->replaceChild($fragment, $span);
    }
}

var_dump(str_replace("\n", "", $dom->saveHTML()));

$html = <<<HTML
<!DOCTYPE HTML>
<html>
<span class="unwrap_me"><i><span class="foo">Lorem</span></i><span class="foo">ipsum</span></span>
</html>
HTML;

$dom = new DOMDocument();
$dom->loadHTML($html);

$spans = iterator_to_array($dom->getElementsByTagName('span')->getIterator());
foreach ($spans as $span) {
    if ('unwrap_me' === $span->getAttribute('class')) {
        $span->replaceWith(...$span->childNodes);
    }
}

var_dump(str_replace("\n", "", $dom->saveHTML()));

$html = <<<HTML
<!DOCTYPE HTML>
<html>
<span class="unwrap_me"><i><span class="foo">Lorem</span></i><span class="foo">ipsum</span></span>
</html>
HTML;

$dom = new DOMDocument();
$dom->loadHTML($html);

$spans = iterator_to_array($dom->getElementsByTagName('span')->getIterator());
foreach ($spans as $span) {
    if ('unwrap_me' === $span->getAttribute('class')) {
        $span->replaceWith('abc');
    }
}

var_dump(str_replace("\n", "", $dom->saveHTML()));
"#);
    assert_exact(&out, r#"string(108) "<!DOCTYPE HTML><html><body><i><span class="foo">Lorem</span></i><span class="foo">ipsum</span></body></html>"
string(108) "<!DOCTYPE HTML><html><body><i><span class="foo">Lorem</span></i><span class="foo">ipsum</span></body></html>"
string(44) "<!DOCTYPE HTML><html><body>abc</body></html>""#);
}

/// php-src `ext/dom/tests/gh11289.phpt`: GH-11289 (DOMException: Not Found Error introduced in 8.2.6, 8.1.19)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createDocumentFragment"#]
fn test_gh11289() {
    if skip_without_xml_native("test_gh11289") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$html = <<<HTML
<!DOCTYPE HTML>
<html>
<body>
    <div></div>
</body>
</html>
HTML;

$dom = new DOMDocument();
$dom->loadHTML($html);

$divs = iterator_to_array($dom->getElementsByTagName('div')->getIterator());
foreach ($divs as $div) {
    $fragment = $dom->createDocumentFragment();
    $fragment->appendXML('<p>Hi!</p>');
    $div->replaceWith(...$fragment->childNodes);
}

var_dump(str_replace("\n", "", $dom->saveHTML()));
"#);
    assert_exact(&out, r#"string(55) "<!DOCTYPE HTML><html><body>    <p>Hi!</p></body></html>""#);
}

/// php-src `ext/dom/tests/gh11290.phpt`: GH-11290 (DOMElement::replaceWith causes crash)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::saveHTML"#]
fn test_gh11290() {
    if skip_without_xml_native("test_gh11290") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$html = <<<HTML
<!DOCTYPE HTML>
<html>
<body>
    <p><span class="unwrap_me">Lorem</span><span class="unwrap_me">ipsum</span><span class="unwrap_me">dolor</span></p>
</body>
</html>
HTML;

$dom = new DOMDocument();
$dom->loadHTML($html);

$spans = iterator_to_array($dom->getElementsByTagName('span')->getIterator());
foreach ($spans as $span) {
    if ('unwrap_me' === $span->getAttribute('class')) {
        $span->replaceWith(...$span->childNodes);
    }
}

var_dump(str_replace("\n", "", $dom->saveHTML()));
"#);
    assert_exact(&out, r#"string(67) "<!DOCTYPE HTML><html><body>    <p>Loremipsumdolor</p></body></html>""#);
}

/// php-src `ext/dom/tests/gh11347.phpt`: GH-11347 (Memory leak when calling a static method inside an xpath query)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXpath"#]
fn test_gh11347() {
    if skip_without_xml_native("test_gh11347") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class MyClass
{
    public static function dump(string $var) {
        var_dump($var);
    }
}

$doc = new DOMDocument();
$doc->loadHTML('<a href="https://php.net">hello</a>');
$xpath = new DOMXpath($doc);
$xpath->registerNamespace("php", "http://php.net/xpath");
$xpath->registerPHPFunctions();
$xpath->query("//a[php:function('MyClass::dump', string(@href))]");

echo 'Done
';"#);
    assert_exact(&out, r#"string(15) "https://php.net"
Done"#);
}

/// php-src `ext/dom/tests/gh11500.phpt`: GH-11500 (Namespace reuse in createElementNS() generates wrong output)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createElementNS"#]
fn test_gh11500() {
    if skip_without_xml_native("test_gh11500") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
function api_test_depth2($root_ns) {
    $dom = new DOMDocument();
    $root = $dom->createElementNS($root_ns, 'root');
    $dom->appendChild($root);

    $a1 = $dom->createElementNS('http://example.com', 'a1');
    $b1 = $a1->appendChild($dom->createElementNS('http://example.com', 'b1'));
    $root->appendChild($a1);

    $a2 = $dom->createElementNS('http://example.com', 'a2');
    $b2 = $a2->appendChild($dom->createElementNS('http://example.com', 'b2'));
    $root->appendChild($a2);

    echo $dom->saveXML();

    var_dump($root->namespaceURI);
    var_dump($a1->namespaceURI);
    var_dump($b1->namespaceURI);
    var_dump($a2->namespaceURI);
    var_dump($b2->namespaceURI);
}

function api_test_depth3($root_ns, $swapped) {
    $dom = new DOMDocument();
    $root = $dom->createElementNS($root_ns, 'root');
    $dom->appendChild($root);

    $a1 = $dom->createElementNS('http://example.com', 'a1');
    $b1 = $a1->appendChild($dom->createElementNS('http://example.com', 'b1'));
    $c1 = $b1->appendChild($dom->createElementNS('http://example.com', 'c1'));
    $root->appendChild($a1);

    $a2 = $dom->createElementNS('http://example.com', 'a2');
    if ($swapped) {
        $b2 = $dom->createElementNS('http://example.com', 'b2');
        $c2 = $b2->appendChild($dom->createElementNS('http://example.com', 'c2'));
        $a2->appendChild($b2);
    } else {
        $b2 = $a2->appendChild($dom->createElementNS('http://example.com', 'b2'));
        $c2 = $b2->appendChild($dom->createElementNS('http://example.com', 'c2'));
    }
    $root->appendChild($a2);

    echo $dom->saveXML();

    var_dump($root->namespaceURI);
    var_dump($a1->namespaceURI);
    var_dump($b1->namespaceURI);
    var_dump($c1->namespaceURI);
    var_dump($a2->namespaceURI);
    var_dump($b2->namespaceURI);
    var_dump($c2->namespaceURI);
}

echo "-- Constructed from API (depth 2, mismatched root variation) --\n";
api_test_depth2('http://example2.com');

echo "-- Constructed from API (depth 2, matching root variation) --\n";
api_test_depth2('http://example.com');

echo "-- Constructed from API (depth 3, mismatched root variation, non-swapped) --\n";
api_test_depth3('http://example2.com', false);

echo "-- Constructed from API (depth 3, matching root variation, non-swapped) --\n";
api_test_depth3('http://example.com', false);

echo "-- Constructed from API (depth 3, mismatched root variation, swapped) --\n";
api_test_depth3('http://example2.com', true);

echo "-- Constructed from API (depth 3, matching root variation, swapped) --\n";
api_test_depth3('http://example.com', true);

echo "-- Constructed depth 2 from string --\n";
$xml = '<?xml version="1.0"?><root xmlns="http://example2.com"><a1 xmlns="http://example.com"><b1/></a1><a2 xmlns="http://example.com"><b2/></a2></root>';
$dom = new DOMDocument;
$dom->loadXML($xml);
echo $dom->saveXML(), "\n";

var_dump($dom->documentElement->namespaceURI); // root
var_dump($dom->documentElement->firstChild->namespaceURI); // a1
var_dump($dom->documentElement->firstChild->firstChild->namespaceURI); // b1
var_dump($dom->documentElement->firstChild->nextSibling->namespaceURI); // a2
var_dump($dom->documentElement->firstChild->nextSibling->firstChild->namespaceURI); // b2;"#);
    assert_exact(&out, r#"-- Constructed from API (depth 2, mismatched root variation) --
<?xml version="1.0"?>
<root xmlns="http://example2.com"><a1 xmlns="http://example.com"><b1/></a1><a2 xmlns="http://example.com"><b2/></a2></root>
string(19) "http://example2.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
-- Constructed from API (depth 2, matching root variation) --
<?xml version="1.0"?>
<root xmlns="http://example.com"><a1><b1/></a1><a2><b2/></a2></root>
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
-- Constructed from API (depth 3, mismatched root variation, non-swapped) --
<?xml version="1.0"?>
<root xmlns="http://example2.com"><a1 xmlns="http://example.com"><b1><c1/></b1></a1><a2 xmlns="http://example.com"><b2><c2/></b2></a2></root>
string(19) "http://example2.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
-- Constructed from API (depth 3, matching root variation, non-swapped) --
<?xml version="1.0"?>
<root xmlns="http://example.com"><a1><b1><c1/></b1></a1><a2><b2><c2/></b2></a2></root>
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
-- Constructed from API (depth 3, mismatched root variation, swapped) --
<?xml version="1.0"?>
<root xmlns="http://example2.com"><a1 xmlns="http://example.com"><b1><c1/></b1></a1><a2 xmlns="http://example.com"><b2><c2/></b2></a2></root>
string(19) "http://example2.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
-- Constructed from API (depth 3, matching root variation, swapped) --
<?xml version="1.0"?>
<root xmlns="http://example.com"><a1><b1><c1/></b1></a1><a2><b2><c2/></b2></a2></root>
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
-- Constructed depth 2 from string --
<?xml version="1.0"?>
<root xmlns="http://example2.com"><a1 xmlns="http://example.com"><b1/></a1><a2 xmlns="http://example.com"><b2/></a2></root>

string(19) "http://example2.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com"
string(18) "http://example.com""#);
}

/// php-src `ext/dom/tests/gh11625.phpt`: GH-11625 (DOMElement::replaceWith() doesn't replace node with DOMDocumentFragment but just deletes node or causes wrapping <></> depending on libxml2 version)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::saveHTML"#]
fn test_gh11625() {
    if skip_without_xml_native("test_gh11625") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

function test($mutator) {
    $html = <<<XML
    <body>
        <div></div><div></div>
    </body>
    XML;

    $dom = new DOMDocument();
    $dom->loadXML($html);

    $divs = iterator_to_array($dom->getElementsByTagName('div')->getIterator());
    $i = 0;
    foreach ($divs as $div) {
        $mutator($dom, $div, $i);
        echo $dom->saveHTML();
        $i++;
    }
}

echo "--- Single replacement ---\n";

test(function($dom, $div, $i) {
    $fragment = $dom->createDocumentFragment();
    $fragment->appendXML("<p>Hi $i!</p>");
    $div->replaceWith($fragment);
});

echo "--- Multiple replacement ---\n";

test(function($dom, $div, $i) {
    $fragment = $dom->createDocumentFragment();
    $fragment->appendXML("<p>Hi $i!</p>");
    $div->replaceWith($fragment, $dom->createElement('x'), "hello");
});

echo "--- Empty fragment replacement ---\n";

test(function($dom, $div, $i) {
    $fragment = $dom->createDocumentFragment();
    $div->replaceWith($fragment);
});

"#);
    assert_exact(&out, r#"--- Single replacement ---
<body>
    <p>Hi 0!</p><div></div>
</body>
<body>
    <p>Hi 0!</p><p>Hi 1!</p>
</body>
--- Multiple replacement ---
<body>
    <p>Hi 0!</p><x></x>hello<div></div>
</body>
<body>
    <p>Hi 0!</p><x></x>hello<p>Hi 1!</p><x></x>hello
</body>
--- Empty fragment replacement ---
<body>
    <div></div>
</body>
<body>
    
</body>"#);
}

/// php-src `ext/dom/tests/gh11682.phpt`: Fix GH-11682 (Consecutive calls to DOMNode::before and DOMNode::remove unlink wrong nodes)
#[test]
fn test_gh11682() {
    if skip_without_xml_native("test_gh11682") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML('<?xml version="1.0" ?><root><node /></root>');

$list = [
    $dom->createElement('foo1'),
    $dom->createElement('foo2'),
    $dom->createElement('foo3')
];

$node = $dom->getElementsByTagName('node')->item(0);

$node->before(...$list);
$node->remove();

echo $dom->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<root><foo1/><foo2/><foo3/></root>"#);
}

/// php-src `ext/dom/tests/gh11791.phpt`: GH-11791 (Wrong default value of DOMDocument.xmlStandalone)
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::xmlStandalone"#]
fn test_gh11791() {
    if skip_without_xml_native("test_gh11791") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$doc->loadXML('<root/>');
var_dump($doc->xmlStandalone);
$doc->xmlStandalone = true;
var_dump($doc->xmlStandalone);

$doc = new DOMDocument();
$doc->loadXML('<?xml version="1.0"?><root/>');
var_dump($doc->xmlStandalone);
$doc->xmlStandalone = true;
var_dump($doc->xmlStandalone);

$doc = new DOMDocument();
$doc->loadXML('<?xml version="1.0" standalone="no"?><root/>');
var_dump($doc->xmlStandalone);
$doc->xmlStandalone = true;
var_dump($doc->xmlStandalone);

$doc = new DOMDocument();
$doc->loadXML('<?xml version="1.0" standalone="yes"?><root/>');
var_dump($doc->xmlStandalone);
$doc->xmlStandalone = false;
var_dump($doc->xmlStandalone);
"#);
    assert_exact(&out, r#"bool(false)
bool(true)
bool(false)
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)"#);
}

/// php-src `ext/dom/tests/gh11906.phpt`: GH-11906 (prepend without children after creating fragment results in segfault)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMElement::prepend"#]
fn test_gh11906() {
    if skip_without_xml_native("test_gh11906") {
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
$test->append($child);
echo "--- document output ---\n";
echo $doc->saveXML();
echo "--- \$test output ---\n";
echo $doc->saveXML($test), "\n";
$test->prepend($child);
echo "--- document output ---\n";
echo $doc->saveXML();
echo "--- \$test output ---\n";
echo $doc->saveXML($test), "\n";
$test->append($child);
"#);
    assert_exact(&out, r#"--- document output ---
<?xml version="1.0"?>
<container>
    
</container>
--- $test output ---
<foo><child/></foo>
--- document output ---
<?xml version="1.0"?>
<container>
    
</container>
--- $test output ---
<foo><child/></foo>"#);
}

/// php-src `ext/dom/tests/gh12002.phpt`: GH-12002 (DOMDocument::encoding memory leak with invalid encoding)
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::encoding"#]
fn test_gh12002() {
    if skip_without_xml_native("test_gh12002") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

function make_nonconst(string $x) {
    // Defeat SCCP, even with inlining
    return str_repeat($x, random_int(1, 1));
}

$dom = new DOMDocument();
$dom->encoding = make_nonconst('utf-8');
var_dump($dom->encoding);
try {
    $dom->encoding = make_nonconst('foobar');
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
var_dump($dom->encoding);
$dom->encoding = make_nonconst('utf-16le');
var_dump($dom->encoding);
try {
    $dom->encoding = NULL;
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
var_dump($dom->encoding);

"#);
    assert_exact(&out, r#"string(5) "utf-8"
Invalid document encoding
string(5) "utf-8"
string(8) "utf-16le"
Invalid document encoding
string(8) "utf-16le""#);
}

/// php-src `ext/dom/tests/gh12455.phpt`: GH-12455 (Namespace prefixes reused incorrectly depending on libxml2 version)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createElementNS"#]
fn test_gh12455() {
    if skip_without_xml_native("test_gh12455") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$element = $doc->createElementNS('http://test', 'a:x');
$doc->appendChild($element);
$element1 = $doc->createElementNS('http://test', 'b:y');
$element->appendChild($element1);
$element1->appendChild($doc->createElementNS('http://test', 'b:z'));
echo $doc->saveXml();

$xpath = new DOMXPath($doc);
$xpath->registerNodeNamespaces = true;
$xpath->registerNamespace('b', 'http://test');

$elements = $xpath->query('//b:z');
foreach ($elements as $e) {
    var_dump($e->nodeName);
}

$elements = $xpath->query('//*[name()="b:z"]');
echo $elements->length;

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<a:x xmlns:a="http://test"><b:y xmlns:b="http://test"><b:z/></b:y></a:x>
string(3) "b:z"
1"#);
}

/// php-src `ext/dom/tests/gh12616_1.phpt`: GH-12616 (DOM: Removing XMLNS namespace node results in invalid default: prefix)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::importNode"#]
fn test_gh12616_1() {
    if skip_without_xml_native("test_gh12616_1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$doc->loadXML(
    <<<XML
    <container xmlns="http://symfony.com/schema/dic/services">
      CHILDREN
    </container>
    XML
);

$doc->documentElement->removeAttributeNS('http://symfony.com/schema/dic/services', '');
echo $doc->saveXML();

$new = new DOMDocument();
$new->append(
    $new->importNode($doc->documentElement, true)
);

echo $new->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<container>
  CHILDREN
</container>
<?xml version="1.0"?>
<container>
  CHILDREN
</container>"#);
}

/// php-src `ext/dom/tests/gh12616_2.phpt`: GH-12616 (DOM: Removing XMLNS namespace node results in invalid default: prefix)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_gh12616_2() {
    if skip_without_xml_native("test_gh12616_2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$doc->loadXML(
    <<<XML
    <container xmlns:test="urn:test" xmlns:symfony="http://symfony.com/schema/dic/services">
        <symfony:services>
            <test:service id="hello" />
        </symfony:services>
    </container>
    XML
);

$doc->documentElement->removeAttributeNS('http://symfony.com/schema/dic/services', 'symfony');
$xpath = new DOMXPath($doc);
$xpath->registerNamespace('test', 'urn:test');

echo $doc->saveXML();

$result = $xpath->query('//container/services/test:service[@id="hello"]');
var_dump($result);

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<container xmlns:test="urn:test">
    <services>
        <test:service id="hello"/>
    </services>
</container>
object(DOMNodeList)#4 (1) {
  ["length"]=>
  int(1)
}"#);
}

/// php-src `ext/dom/tests/gh12616_3.phpt`: GH-12616 (DOM: Removing XMLNS namespace node results in invalid default: prefix)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_gh12616_3() {
    if skip_without_xml_native("test_gh12616_3") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$doc->loadXML(
    <<<XML
    <container>
      <child1 xmlns:x="http://symfony.com/schema/dic/services">
        <x:foo x:bar=""/>
        <x:foo x:bar=""/>
      </child1>
      <child2 xmlns:x="http://symfony.com/schema/dic/services">
        <x:foo x:bar=""/>
        <x:foo x:bar=""/>
      </child2>
    </container>
    XML
);

$doc->documentElement->firstElementChild->removeAttributeNS('http://symfony.com/schema/dic/services', 'x');
echo $doc->saveXML();

$xpath = new DOMXPath($doc);

echo "--- Namespaces of child1 ---\n";

foreach ($xpath->query("/container/child1/namespace::*") as $ns) {
    var_dump($ns);
}

echo "--- Namespaces of child1/foo (both nodes) ---\n";

foreach ($xpath->query("/container/child1/foo/namespace::*") as $ns) {
    var_dump($ns);
}

echo "--- Namespaces of child2 ---\n";

foreach ($xpath->query("/container/child2/namespace::*") as $ns) {
    var_dump($ns);
}

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<container>
  <child1>
    <foo bar=""/>
    <foo bar=""/>
  </child1>
  <child2 xmlns:x="http://symfony.com/schema/dic/services">
    <x:foo x:bar=""/>
    <x:foo x:bar=""/>
  </child2>
</container>
--- Namespaces of child1 ---
object(DOMNameSpaceNode)#4 (10) {
  ["nodeName"]=>
  string(9) "xmlns:xml"
  ["nodeValue"]=>
  string(36) "http://www.w3.org/XML/1998/namespace"
  ["nodeType"]=>
  int(18)
  ["prefix"]=>
  string(3) "xml"
  ["localName"]=>
  string(3) "xml"
  ["namespaceURI"]=>
  string(36) "http://www.w3.org/XML/1998/namespace"
  ["isConnected"]=>
  bool(true)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["parentNode"]=>
  string(22) "(object value omitted)"
  ["parentElement"]=>
  string(22) "(object value omitted)"
}
--- Namespaces of child1/foo (both nodes) ---
object(DOMNameSpaceNode)#5 (10) {
  ["nodeName"]=>
  string(9) "xmlns:xml"
  ["nodeValue"]=>
  string(36) "http://www.w3.org/XML/1998/namespace"
  ["nodeType"]=>
  int(18)
  ["prefix"]=>
  string(3) "xml"
  ["localName"]=>
  string(3) "xml"
  ["namespaceURI"]=>
  string(36) "http://www.w3.org/XML/1998/namespace"
  ["isConnected"]=>
  bool(true)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["parentNode"]=>
  string(22) "(object value omitted)"
  ["parentElement"]=>
  string(22) "(object value omitted)"
}
object(DOMNameSpaceNode)#8 (10) {
  ["nodeName"]=>
  string(9) "xmlns:xml"
  ["nodeValue"]=>
  string(36) "http://www.w3.org/XML/1998/namespace"
  ["nodeType"]=>
  int(18)
  ["prefix"]=>
  string(3) "xml"
  ["localName"]=>
  string(3) "xml"
  ["namespaceURI"]=>
  string(36) "http://www.w3.org/XML/1998/namespace"
  ["isConnected"]=>
  bool(true)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["parentNode"]=>
  string(22) "(object value omitted)"
  ["parentElement"]=>
  string(22) "(object value omitted)"
}
--- Namespaces of child2 ---
object(DOMNameSpaceNode)#9 (10) {
  ["nodeName"]=>
  string(9) "xmlns:xml"
  ["nodeValue"]=>
  string(36) "http://www.w3.org/XML/1998/namespace"
  ["nodeType"]=>
  int(18)
  ["prefix"]=>
  string(3) "xml"
  ["localName"]=>
  string(3) "xml"
  ["namespaceURI"]=>
  string(36) "http://www.w3.org/XML/1998/namespace"
  ["isConnected"]=>
  bool(true)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["parentNode"]=>
  string(22) "(object value omitted)"
  ["parentElement"]=>
  string(22) "(object value omitted)"
}
object(DOMNameSpaceNode)#5 (10) {
  ["nodeName"]=>
  string(7) "xmlns:x"
  ["nodeValue"]=>
  string(38) "http://symfony.com/schema/dic/services"
  ["nodeType"]=>
  int(18)
  ["prefix"]=>
  string(1) "x"
  ["localName"]=>
  string(1) "x"
  ["namespaceURI"]=>
  string(38) "http://symfony.com/schema/dic/services"
  ["isConnected"]=>
  bool(true)
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["parentNode"]=>
  string(22) "(object value omitted)"
  ["parentElement"]=>
  string(22) "(object value omitted)"
}"#);
}

/// php-src `ext/dom/tests/gh12870_a.phpt`: GH-12870 (Creating an xmlns attribute results in a DOMException) - xmlns variations
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createAttributeNS"#]
fn test_gh12870_a() {
    if skip_without_xml_native("test_gh12870_a") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

require __DIR__ . '/gh12870.inc';

echo "=== NORMAL CASES ===\n";

test('http://www.w3.org/2000/xmlns/qx', 'foo:xmlns');
test('http://www.w3.org/2000/xmlns/', 'xmlns');
test('http://www.w3.org/2000/xmlns/', 'xmlns:xmlns');

echo "=== ERROR CASES ===\n";

test('http://www.w3.org/2000/xmlns/', 'bar:xmlns');
test('http://www.w3.org/2000/xmlns/a', 'xmlns');
test('http://www.w3.org/2000/xmlns/a', 'xmlns:bar');
test(null, 'xmlns:bar');
test('', 'xmlns');
test('http://www.w3.org/2000/xmlns/', '');

"#);
    assert_exact(&out, r#"=== NORMAL CASES ===
--- Testing "http://www.w3.org/2000/xmlns/qx", "foo:xmlns" ---
Attr prefix: string(3) "foo"
Attr namespaceURI: string(31) "http://www.w3.org/2000/xmlns/qx"
Attr value: string(0) ""
root namespaceURI: NULL
Equality check: bool(true)
<?xml version="1.0"?>
<root xmlns:foo="http://www.w3.org/2000/xmlns/qx" foo:xmlns=""/>

--- Testing "http://www.w3.org/2000/xmlns/", "xmlns" ---
Attr prefix: string(0) ""
Attr namespaceURI: string(29) "http://www.w3.org/2000/xmlns/"
Attr value: string(0) ""
root namespaceURI: NULL
Equality check: bool(true)
<?xml version="1.0"?>
<root xmlns=""/>

--- Testing "http://www.w3.org/2000/xmlns/", "xmlns:xmlns" ---
Attr prefix: string(5) "xmlns"
Attr namespaceURI: string(29) "http://www.w3.org/2000/xmlns/"
Attr value: string(0) ""
root namespaceURI: NULL
Equality check: bool(true)
<?xml version="1.0"?>
<root xmlns:xmlns="http://www.w3.org/2000/xmlns/" xmlns:xmlns=""/>

=== ERROR CASES ===
--- Testing "http://www.w3.org/2000/xmlns/", "bar:xmlns" ---
Exception: Namespace Error

--- Testing "http://www.w3.org/2000/xmlns/a", "xmlns" ---
Exception: Namespace Error

--- Testing "http://www.w3.org/2000/xmlns/a", "xmlns:bar" ---
Exception: Namespace Error

--- Testing NULL, "xmlns:bar" ---
Exception: Namespace Error

--- Testing "", "xmlns" ---
Exception: Namespace Error

--- Testing "http://www.w3.org/2000/xmlns/", "" ---
Exception: Namespace Error"#);
}

/// php-src `ext/dom/tests/gh12870_b.phpt`: GH-12870 (Creating an xmlns attribute results in a DOMException) - xml variations
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createAttributeNS"#]
fn test_gh12870_b() {
    if skip_without_xml_native("test_gh12870_b") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

require __DIR__ . '/gh12870.inc';

echo "=== NORMAL CASES ===\n";

test('http://www.w3.org/XML/1998/namespaceqx', 'foo:xml');
test('http://www.w3.org/XML/1998/namespace', 'xml');
test('http://www.w3.org/XML/1998/namespace', 'bar:xml');
test('', 'xml');
test('http://www.w3.org/XML/1998/namespacea', 'xml');

echo "=== ERROR CASES ===\n";

test('http://www.w3.org/XML/1998/namespace', 'xmlns:xml');
test('http://www.w3.org/XML/1998/namespace', '');
test('http://www.w3.org/XML/1998/namespacea', 'xml:foo');
test(NULL, 'xml:foo');

"#);
    assert_exact(&out, r#"=== NORMAL CASES ===
--- Testing "http://www.w3.org/XML/1998/namespaceqx", "foo:xml" ---
Attr prefix: string(3) "foo"
Attr namespaceURI: string(38) "http://www.w3.org/XML/1998/namespaceqx"
Attr value: string(0) ""
root namespaceURI: NULL
Equality check: bool(true)
<?xml version="1.0"?>
<root xmlns:foo="http://www.w3.org/XML/1998/namespaceqx" foo:xml=""/>

--- Testing "http://www.w3.org/XML/1998/namespace", "xml" ---
Attr prefix: string(3) "xml"
Attr namespaceURI: string(36) "http://www.w3.org/XML/1998/namespace"
Attr value: string(0) ""
root namespaceURI: NULL
Equality check: bool(true)
<?xml version="1.0"?>
<root xml:xml=""/>

--- Testing "http://www.w3.org/XML/1998/namespace", "bar:xml" ---
Attr prefix: string(3) "xml"
Attr namespaceURI: string(36) "http://www.w3.org/XML/1998/namespace"
Attr value: string(0) ""
root namespaceURI: NULL
Equality check: bool(true)
<?xml version="1.0"?>
<root xml:xml=""/>

--- Testing "", "xml" ---
Attr prefix: string(0) ""
Attr namespaceURI: NULL
Attr value: string(0) ""
root namespaceURI: NULL
Equality check: bool(false)
<?xml version="1.0"?>
<root xml=""/>

--- Testing "http://www.w3.org/XML/1998/namespacea", "xml" ---
Attr prefix: string(7) "default"
Attr namespaceURI: string(37) "http://www.w3.org/XML/1998/namespacea"
Attr value: string(0) ""
root namespaceURI: NULL
Equality check: bool(true)
<?xml version="1.0"?>
<root xmlns:default="http://www.w3.org/XML/1998/namespacea" default:xml=""/>

=== ERROR CASES ===
--- Testing "http://www.w3.org/XML/1998/namespace", "xmlns:xml" ---
Exception: Namespace Error

--- Testing "http://www.w3.org/XML/1998/namespace", "" ---
Exception: Namespace Error

--- Testing "http://www.w3.org/XML/1998/namespacea", "xml:foo" ---
Exception: Namespace Error

--- Testing NULL, "xml:foo" ---
Exception: Namespace Error"#);
}

/// php-src `ext/dom/tests/gh13012.phpt`: GH-13012 (DOMNode::isEqualNode() is incorrect when attribute order is different)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_gh13012() {
    if skip_without_xml_native("test_gh13012") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML("<root><x a='a' b='b'/><x b='b' a='a'/><x b='b' a='a' c='c'/></root>");

foreach ($dom->getElementsByTagName('x') as $x1) {
    foreach ($dom->getElementsByTagName('x') as $x2) {
        echo "Comparing ", $dom->saveXML($x1), " with ", $dom->saveXML($x2), "\n";
        var_dump($x1->isEqualNode($x2));
    }
}

"#);
    assert_exact(&out, r#"Comparing <x a="a" b="b"/> with <x a="a" b="b"/>
bool(true)
Comparing <x a="a" b="b"/> with <x b="b" a="a"/>
bool(true)
Comparing <x a="a" b="b"/> with <x b="b" a="a" c="c"/>
bool(false)
Comparing <x b="b" a="a"/> with <x a="a" b="b"/>
bool(true)
Comparing <x b="b" a="a"/> with <x b="b" a="a"/>
bool(true)
Comparing <x b="b" a="a"/> with <x b="b" a="a" c="c"/>
bool(false)
Comparing <x b="b" a="a" c="c"/> with <x a="a" b="b"/>
bool(false)
Comparing <x b="b" a="a" c="c"/> with <x b="b" a="a"/>
bool(false)
Comparing <x b="b" a="a" c="c"/> with <x b="b" a="a" c="c"/>
bool(true)"#);
}

/// php-src `ext/dom/tests/gh13012_ns.phpt`: GH-13012 (DOMNode::isEqualNode() is incorrect when attribute order is different - ns variation)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_gh13012_ns() {
    if skip_without_xml_native("test_gh13012_ns") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$dom->loadXML("<root><x/><x xmlns:a=\"urn:a\" xmlns:b=\"urn:b\"/><x xmlns:b=\"urn:b\" xmlns:a=\"urn:a\"/></root>");

foreach ($dom->getElementsByTagName('x') as $x1) {
    foreach ($dom->getElementsByTagName('x') as $x2) {
        echo "Comparing ", $dom->saveXML($x1), " with ", $dom->saveXML($x2), "\n";
        var_dump($x1->isEqualNode($x2));
    }
}

"#);
    assert_exact(&out, r#"Comparing <x/> with <x/>
bool(true)
Comparing <x/> with <x xmlns:a="urn:a" xmlns:b="urn:b"/>
bool(false)
Comparing <x/> with <x xmlns:b="urn:b" xmlns:a="urn:a"/>
bool(false)
Comparing <x xmlns:a="urn:a" xmlns:b="urn:b"/> with <x/>
bool(false)
Comparing <x xmlns:a="urn:a" xmlns:b="urn:b"/> with <x xmlns:a="urn:a" xmlns:b="urn:b"/>
bool(true)
Comparing <x xmlns:a="urn:a" xmlns:b="urn:b"/> with <x xmlns:b="urn:b" xmlns:a="urn:a"/>
bool(true)
Comparing <x xmlns:b="urn:b" xmlns:a="urn:a"/> with <x/>
bool(false)
Comparing <x xmlns:b="urn:b" xmlns:a="urn:a"/> with <x xmlns:a="urn:a" xmlns:b="urn:b"/>
bool(true)
Comparing <x xmlns:b="urn:b" xmlns:a="urn:a"/> with <x xmlns:b="urn:b" xmlns:a="urn:a"/>
bool(true)"#);
}

/// php-src `ext/dom/tests/gh13863.phpt`: GH-13863 (Removal during NodeList iteration breaks loop)
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_gh13863() {
    if skip_without_xml_native("test_gh13863") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

enum DomType {
    case Legacy;
    case Modern;
}

function createTestDoc(DomType $type) {
    $xml = <<<EOXML
    <x>
        <item1 />
        <item2 />
        <item3 />
        <item4 />
        <item5 />
    </x>
    EOXML;

    if ($type === DomType::Legacy) {
        $doc = new DOMDocument();
        $doc->preserveWhiteSpace = false;
        $doc->loadXML($xml);
    } else {
        $doc = Dom\XMLDocument::createFromString($xml, LIBXML_NOBLANKS);
    }

    return $doc;
}

foreach ([DomType::Legacy, DomType::Modern] as $case) {
    $name = match ($case) {
        DomType::Legacy => 'Legacy',
        DomType::Modern => 'Modern',
    };

    echo "--- $name test remove index 2 at index 2 ---\n";

    $doc = createTestDoc($case);

    foreach ($doc->documentElement->childNodes as $i => $node) {
        var_dump($doc->documentElement->childNodes->item($i) === $node);
        var_dump($i, $node->localName);
        if ($i === 2) {
            $node->remove();
        }
    }

    echo $doc->saveXML(), "\n";

    echo "--- $name test remove index 1 at index 2 ---\n";

    $doc = createTestDoc($case);

    foreach ($doc->documentElement->childNodes as $i => $node) {
        var_dump($doc->documentElement->childNodes->item($i) === $node);
        var_dump($i, $node->localName);
        if ($i === 2) {
            $doc->documentElement->childNodes[1]->remove();
        }
    }

    echo $doc->saveXML(), "\n";

    echo "--- $name test remove all ---\n";

    $doc = createTestDoc($case);

    foreach ($doc->documentElement->childNodes as $i => $node) {
        var_dump($doc->documentElement->childNodes->item($i) === $node);
        var_dump($i, $node->localName);
        $node->remove();
    }

    echo $doc->saveXML(), "\n";
}

"#);
    assert_exact(&out, r#"--- Legacy test remove index 2 at index 2 ---
bool(true)
int(0)
string(5) "item1"
bool(true)
int(1)
string(5) "item2"
bool(true)
int(2)
string(5) "item3"
<?xml version="1.0"?>
<x><item1/><item2/><item4/><item5/></x>

--- Legacy test remove index 1 at index 2 ---
bool(true)
int(0)
string(5) "item1"
bool(true)
int(1)
string(5) "item2"
bool(true)
int(2)
string(5) "item3"
bool(false)
int(3)
string(5) "item4"
bool(false)
int(4)
string(5) "item5"
<?xml version="1.0"?>
<x><item1/><item3/><item4/><item5/></x>

--- Legacy test remove all ---
bool(true)
int(0)
string(5) "item1"
<?xml version="1.0"?>
<x><item2/><item3/><item4/><item5/></x>

--- Modern test remove index 2 at index 2 ---
bool(true)
int(0)
string(5) "item1"
bool(true)
int(1)
string(5) "item2"
bool(true)
int(2)
string(5) "item3"
bool(true)
int(3)
string(5) "item5"
<?xml version="1.0" encoding="UTF-8"?>
<x><item1/><item2/><item4/><item5/></x>
--- Modern test remove index 1 at index 2 ---
bool(true)
int(0)
string(5) "item1"
bool(true)
int(1)
string(5) "item2"
bool(true)
int(2)
string(5) "item3"
bool(true)
int(3)
string(5) "item5"
<?xml version="1.0" encoding="UTF-8"?>
<x><item1/><item3/><item4/><item5/></x>
--- Modern test remove all ---
bool(true)
int(0)
string(5) "item1"
bool(true)
int(1)
string(5) "item3"
bool(true)
int(2)
string(5) "item5"
<?xml version="1.0" encoding="UTF-8"?>
<x><item2/><item4/></x>"#);
}

/// php-src `ext/dom/tests/gh13960.phpt`: GH-13960 (NULL bytes in XPath query)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_gh13960() {
    if skip_without_xml_native("test_gh13960") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$domd = new DOMDocument();
@$domd->loadHTML("<foo>tes\x00t</foo>");
$xp = new DOMXPath($domd);
try {
    $xp->query("//foo[contains(text(), " . $xp->quote("tes\x00t") . ")]");
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
"#);
    assert_exact(&out, r#"DOMXPath::quote(): Argument #1 ($str) must not contain any null bytes"#);
}

/// php-src `ext/dom/tests/gh14343.phpt`: GH-14343 (Memory leak in xml and dom)
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::firstChild"#]
fn test_gh14343() {
    if skip_without_xml_native("test_gh14343") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$aDOM = new DOMDocument();
$fromdom = new DOMDocument();
$fromdom->loadXML('<data xmlns:ai="http://test.org" ai:attr="namespaced" />');
$attr= $fromdom->firstChild->attributes->item(0);
$att = $aDOM->importNode($attr);
echo $aDOM->saveXML($att);
"#);
    assert_exact(&out, r#"ai:attr="namespaced""#);
}

/// php-src `ext/dom/tests/gh14652.phpt`: GH-14652 segfault on object cloning
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMAttr"#]
fn test_gh14652() {
    if skip_without_xml_native("test_gh14652") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$attr = new DOMAttr('category', 'books');
$clone = clone $attr;
$attr->value = "hello";

var_dump($attr->value);
var_dump($clone->value);
"#);
    assert_exact(&out, r#"string(5) "hello"
string(5) "books""#);
}

/// php-src `ext/dom/tests/gh14698.phpt`: GH-14698 crash on DOM node dereference
#[test]
fn test_gh14698() {
    if skip_without_xml_native("test_gh14698") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
  $dom = new DOMDocument;
  $dom->loadHTML('<span title="y">x</span><span title="z">x</span>');
  $html = simplexml_import_dom($dom);
  foreach ($html->body->span as $obj) {
  }
  $script1_dataflow = $html;
  $array = ['foo'];
  foreach ($array as $key => &$value) {
    unset($script1_dataflow[$key]);
  }
  echo "DONE";
"#);
    assert_format(&out, r#"DONE"#);
}

/// php-src `ext/dom/tests/gh14702.phpt`: GH-14702 (DOMDocument::xinclude() crash)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createElementNS"#]
fn test_gh14702() {
    if skip_without_xml_native("test_gh14702") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$doc->loadXML(<<<XML
<?xml version="1.0"?>
<root>
  <child/>
  <include href="foo" xmlns="http://www.w3.org/2001/XInclude">
    <fallback/>
  </include>
  <keep/>
</root>
XML);
$xi = $doc->createElementNS('http://www.w3.org/2001/XInclude', 'xi:include');
$xi->setAttribute('href', 'nonexistent');

$fallback = $doc->createElementNS('http://www.w3.org/2001/XInclude', 'xi:fallback');
$xi->appendChild($fallback);
$child1 = $fallback->appendChild($doc->createElement('fallback-child1'));
$child2 = $fallback->appendChild($doc->createElement('fallback-child2'));

$xpath = new DOMXPath($doc);
$toReplace = $xpath->query('//child')->item(0);
$toReplace->parentNode->replaceChild($xi, $toReplace);

$keep = $doc->documentElement->lastElementChild;

var_dump(@$doc->xinclude());
echo $doc->saveXML();

var_dump($keep->nodeName);

$keep->textContent = 'still works';
echo $doc->saveXML();
"#);
    assert_exact(&out, r#"int(2)
<?xml version="1.0"?>
<root>
  <fallback-child1/><fallback-child2/>
  
  <keep/>
</root>
string(4) "keep"
<?xml version="1.0"?>
<root>
  <fallback-child1/><fallback-child2/>
  
  <keep>still works</keep>
</root>"#);
}

/// php-src `ext/dom/tests/gh15137.phpt`: GH-15137: Unexpected null pointer in Zend/zend_smart_str.h
#[test]
fn test_gh15137() {
    if skip_without_xml_native("test_gh15137") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
var_dump(new DOMText()->wholeText);
"#);
    assert_exact(&out, r#"string(0) """#);
}

/// php-src `ext/dom/tests/gh15192.phpt`: GH-15192 (Segmentation fault in dom extension (html5_serializer))
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_gh15192() {
    if skip_without_xml_native("test_gh15192") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = @Dom\HTMLDocument::createFromString("<p>foo</p>");
$dom2 = clone $dom;
$element = $dom2->firstChild;
$dom = new DomDocument();
var_dump($element);
"#);
    assert_exact(&out, r#"object(Dom\HTMLElement)#3 (31) {
  ["namespaceURI"]=>
  string(28) "http://www.w3.org/1999/xhtml"
  ["prefix"]=>
  NULL
  ["localName"]=>
  string(4) "html"
  ["tagName"]=>
  string(4) "HTML"
  ["id"]=>
  string(0) ""
  ["className"]=>
  string(0) ""
  ["classList"]=>
  string(22) "(object value omitted)"
  ["attributes"]=>
  string(22) "(object value omitted)"
  ["children"]=>
  string(22) "(object value omitted)"
  ["firstElementChild"]=>
  string(22) "(object value omitted)"
  ["lastElementChild"]=>
  string(22) "(object value omitted)"
  ["childElementCount"]=>
  int(2)
  ["previousElementSibling"]=>
  NULL
  ["nextElementSibling"]=>
  NULL
  ["innerHTML"]=>
  string(36) "<head></head><body><p>foo</p></body>"
  ["outerHTML"]=>
  string(49) "<html><head></head><body><p>foo</p></body></html>"
  ["substitutedNodeValue"]=>
  string(3) "foo"
  ["nodeType"]=>
  int(1)
  ["nodeName"]=>
  string(4) "HTML"
  ["baseURI"]=>
  string(11) "about:blank"
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
  string(3) "foo"
}"#);
}

/// php-src `ext/dom/tests/gh15551.phpt`: GH-15551 (Segmentation fault (access null pointer) in ext/dom/xml_common.h)
#[test]
fn test_gh15551() {
    if skip_without_xml_native("test_gh15551") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$fragment = new DOMDocumentFragment();
$nodes = $fragment->childNodes;
$iter = $nodes->getIterator();
$iter->next();
var_dump($iter->valid());
"#);
    assert_exact(&out, r#"bool(false)"#);
}

/// php-src `ext/dom/tests/gh15570.phpt`: GH-15570 (Segmentation fault (access null pointer) in ext/dom/html5_serializer.c)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_gh15570() {
    if skip_without_xml_native("test_gh15570") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$html = <<<HTML
<head>
</html>
HTML;
$dom = Dom\HTMLDocument::createFromString($html, LIBXML_NOERROR);
$a = $dom->head->firstChild->cloneNode(false);
var_dump($dom->saveHTML($a));
"#);
    assert_exact(&out, r#"string(1) "
""#);
}

/// php-src `ext/dom/tests/gh15654.phpt`: GH-15654 (Signed integer overflow in ext/dom/nodelist.c)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_gh15654() {
    if skip_without_xml_native("test_gh15654") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
define("MAX_64Bit", 9223372036854775807);
define("MAX_32Bit", 2147483647);
define("MIN_64Bit", -9223372036854775807 - 1);
define("MIN_32Bit", -2147483647 - 1);
$longVals = array(
    0, MAX_64Bit, MIN_64Bit, MAX_32Bit, MIN_32Bit, MAX_64Bit - MAX_32Bit, MIN_64Bit - MIN_32Bit,
);
$dom = new DOMDocument;
$dom->loadXML('<root><a/><b/><c/></root>');
$children = $dom->documentElement->childNodes;
foreach ($longVals as $value) {
    var_dump($children[$value]?->nodeName);
}
"#);
    assert_exact(&out, r#"string(1) "a"
NULL
NULL
NULL
NULL
NULL
NULL"#);
}

/// php-src `ext/dom/tests/gh15670.phpt`: GH-15670 (Polymorphic cache slot issue in DOM)
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createElementNS"#]
fn test_gh15670() {
    if skip_without_xml_native("test_gh15670") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$doc->loadHTML('<p id=x>foo</p>');
$dom = DOM\XMLDocument::createFromString('<root/>');
$child = $dom->documentElement->appendChild($dom->createElementNS('urn:a', 'child'));
function test($child, $html) {
    try {
        $child->innerHTML = $html;
    } catch (DOMException $e) {
    }
}
test($child, '--></root><!--');
test($doc, '<');
echo $doc->saveXML(), "\n";
echo $dom->saveXML(), "\n";
"#);
    assert_format(&out, r#"Deprecated: Creation of dynamic property DOMDocument::$innerHTML is deprecated in %s on line %d
<?xml version="1.0" standalone="yes"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<html><body><p id="x">foo</p></body></html>

<?xml version="1.0" encoding="UTF-8"?>
<root><child xmlns="urn:a"/></root>"#);
}

/// php-src `ext/dom/tests/gh15910.phpt`: GH-15910 (Assertion failure in ext/dom/element.c)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_gh15910() {
    if skip_without_xml_native("test_gh15910") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$doc->appendChild($doc->createElement('container'));
try {
    $doc->documentElement->setAttributeNodeNS($doc);
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
"#);
    assert_exact(&out, r#"DOMElement::setAttributeNodeNS(): Argument #1 ($attr) must be of type DOMAttr, DOMDocument given"#);
}

/// php-src `ext/dom/tests/gh16039.phpt`: GH-16039 (Segmentation fault (access null pointer) in ext/dom/parentnode/tree.c)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMEntity"#]
fn test_gh16039() {
    if skip_without_xml_native("test_gh16039") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$element = $dom->appendChild($dom->createElement('root'));
try {
    $element->prepend('x', new DOMEntity);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveXML();
$dom->strictErrorChecking = false; // Should not have influence
try {
    $element->prepend('x', new DOMEntity);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $dom->saveXML();

"#);
    assert_exact(&out, r#"Invalid State Error
<?xml version="1.0"?>
<root/>
Invalid State Error
<?xml version="1.0"?>
<root/>"#);
}

/// php-src `ext/dom/tests/gh16149.phpt`: GH-16149 (Null pointer dereference in DOMElement->getAttributeNames())
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMElement::__construct' expects 0 arguments, got 3"#]
fn test_gh16149() {
    if skip_without_xml_native("test_gh16149") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$element = new DOMElement("b", null, "a");
var_dump($element->getAttributeNames());
"#);
    assert_exact(&out, r#"array(1) {
  [0]=>
  string(5) "xmlns"
}"#);
}

/// php-src `ext/dom/tests/gh16150.phpt`: GH-16150 (Use after free in php_dom.c)
#[test]
fn test_gh16150() {
    if skip_without_xml_native("test_gh16150") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

function test($fname) {
    $e1 = new DOMElement("E1");
    $e2 = new DOMElement("E2");
    $e3 = new DOMElement("E3");
    $doc = new DOMDocument(); // Must be placed here so it is destroyed first
    $doc->{$fname}($e3);
    $e2->append($e1);
    $e3->{$fname}($e2);
    echo $doc->saveXML();
}

test('appendChild');
test('insertBefore');

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<E3><E2><E1/></E2></E3>
<?xml version="1.0"?>
<E3><E2><E1/></E2></E3>"#);
}

/// php-src `ext/dom/tests/gh16151.phpt`: GH-16151 (Assertion failure in ext/dom/parentnode/tree.c)
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMElement::__construct' expects 0 arguments, got 3"#]
fn test_gh16151() {
    if skip_without_xml_native("test_gh16151") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$element = new DOMElement("N", "W", "y");
$attr = new DOMAttr("c" , "n");
$doc = new DOMDocument();
$doc->appendChild($element);
$element->setAttributeNodeNS($attr);

try {
    $attr->insertBefore(new DOMComment("h"));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
try {
    $attr->appendChild(new DOMComment("h"));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

$attr->insertBefore($doc->createEntityReference('amp'));
$attr->appendChild($doc->createEntityReference('amp'));

echo $doc->saveXML();

"#);
    assert_exact(&out, r#"Hierarchy Request Error
Hierarchy Request Error
<?xml version="1.0"?>
<N xmlns="y" c="n&amp;&amp;">W</N>"#);
}

/// php-src `ext/dom/tests/gh16152.phpt`: GH-16152 (Memory leak in DOMProcessingInstruction/DOMDocument)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMDocumentFragment"#]
fn test_gh16152() {
    if skip_without_xml_native("test_gh16152") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

function test($fname) {
    $doc = new DOMDocument();
    $instr = new DOMProcessingInstruction("tr", "r");
    $frag = new DOMDocumentFragment();
    $frag2 = new DOMDocumentFragment();
    $frag2->append($instr);
    $frag->append($frag2);
    $doc->{$fname}($frag);
    echo $doc->saveXML();
}

test('insertBefore');
test('appendChild');

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<?tr r?>
<?xml version="1.0"?>
<?tr r?>"#);
}

/// php-src `ext/dom/tests/gh16190.phpt`: GH-16190 (Using reflection to call Dom\Node::__construct causes assertion failure)
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_gh16190() {
    if skip_without_xml_native("test_gh16190") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = Dom\XMLDocument::createEmpty();
$rm = new ReflectionMethod($doc, '__construct');
try {
    $rm->invoke($doc);
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Cannot directly construct Dom\XMLDocument, use document methods instead"#);
}

/// php-src `ext/dom/tests/gh16316.phpt`: GH-16316 (DOMXPath breaks when not initialized properly)
#[test]
#[ignore = r#"DOM port pending: Unknown class referenced during inheritance flattening: DOMXPath"#]
fn test_gh16316() {
    if skip_without_xml_native("test_gh16316") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class Demo extends DOMXPath {
    public function __construct() {}
}

$demo = new Demo;
try {
    var_dump($demo);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

try {
    var_dump($demo->document);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"object(Demo)#1 (1) {
  ["registerNodeNamespaces"]=>
  bool(true)
}
Invalid State Error
Invalid State Error"#);
}

/// php-src `ext/dom/tests/gh16317.phpt`: GH-16317 (DOM classes do not allow __debugInfo() overrides to work)
#[test]
fn test_gh16317() {
    if skip_without_xml_native("test_gh16317") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

class Demo extends DOMNode {
    public function __construct() {}
    public function __debugInfo(): array {
        return ['x' => 'y'];
    }
}

var_dump(new Demo());

"#);
    assert_exact(&out, r#"object(Demo)#1 (1) {
  ["x"]=>
  string(1) "y"
}"#);
}

/// php-src `ext/dom/tests/gh16336_1.phpt`: GH-16336 (Attribute intern document mismanagement)
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMElement::__construct' expects 0 arguments, got 1"#]
fn test_gh16336_1() {
    if skip_without_xml_native("test_gh16336_1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$elem = new DOMElement("g");
$attr = new DOMAttr("iF", "j");

// First append, then attribute
$doc->appendChild($elem);
$elem->setAttributeNode($attr);
echo $attr->firstChild->textContent;

"#);
    assert_exact(&out, r#"j"#);
}

/// php-src `ext/dom/tests/gh16336_2.phpt`: GH-16336 (Attribute intern document mismanagement)
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMElement::__construct' expects 0 arguments, got 1"#]
fn test_gh16336_2() {
    if skip_without_xml_native("test_gh16336_2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$elem = new DOMElement("g");
$attr = new DOMAttr("iF", "j");

// First attribute, then append
$elem->setAttributeNode($attr);
$doc->appendChild($elem);
echo $attr->firstChild->textContent;

"#);
    assert_exact(&out, r#"j"#);
}

/// php-src `ext/dom/tests/gh16338.phpt`: GH-16338 (Null-dereference in ext/dom/node.c)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMEntityReference"#]
fn test_gh16338() {
    if skip_without_xml_native("test_gh16338") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$ref = new DOMEntityReference("G");
$com = new DOMComment();
$doc = new DOMDocument();
$elem = new DOMElement("Rj", "o");
$com2 = new DOMComment();
$elem2 = new DOMElement("kx", null, "r");

$elem2->prepend($com);
$com->before("Z");
$com->before($com2);
$com2->after($elem);
$doc->insertBefore($elem2);
$elem->insertBefore($ref);

echo $doc->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<kx xmlns="r">Z<Rj>o&G;</Rj></kx>"#);
}

/// php-src `ext/dom/tests/gh16356.phpt`: GH-16356 (Segmentation fault with $outerHTML and next node)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_gh16356() {
    if skip_without_xml_native("test_gh16356") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$dom->append($dom->createElement("container"));
$e1 = $dom->documentElement->appendChild($dom->createElementNS("urn:example1", "example:foo"));
$e2 = $dom->documentElement->appendChild($dom->createElementNS("urn:example2", "example:foo"));
var_dump($e1, $e2);

"#);
    assert_exact(&out, r#"object(Dom\Element)#3 (31) {
  ["namespaceURI"]=>
  string(12) "urn:example1"
  ["prefix"]=>
  string(7) "example"
  ["localName"]=>
  string(3) "foo"
  ["tagName"]=>
  string(11) "example:foo"
  ["id"]=>
  string(0) ""
  ["className"]=>
  string(0) ""
  ["classList"]=>
  string(22) "(object value omitted)"
  ["attributes"]=>
  string(22) "(object value omitted)"
  ["children"]=>
  string(22) "(object value omitted)"
  ["firstElementChild"]=>
  NULL
  ["lastElementChild"]=>
  NULL
  ["childElementCount"]=>
  int(0)
  ["previousElementSibling"]=>
  NULL
  ["nextElementSibling"]=>
  string(22) "(object value omitted)"
  ["innerHTML"]=>
  string(0) ""
  ["outerHTML"]=>
  string(27) "<example:foo></example:foo>"
  ["substitutedNodeValue"]=>
  string(0) ""
  ["nodeType"]=>
  int(1)
  ["nodeName"]=>
  string(11) "example:foo"
  ["baseURI"]=>
  string(11) "about:blank"
  ["isConnected"]=>
  bool(true)
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
  string(22) "(object value omitted)"
  ["nodeValue"]=>
  NULL
  ["textContent"]=>
  string(0) ""
}
object(Dom\Element)#4 (31) {
  ["namespaceURI"]=>
  string(12) "urn:example2"
  ["prefix"]=>
  string(7) "example"
  ["localName"]=>
  string(3) "foo"
  ["tagName"]=>
  string(11) "example:foo"
  ["id"]=>
  string(0) ""
  ["className"]=>
  string(0) ""
  ["classList"]=>
  string(22) "(object value omitted)"
  ["attributes"]=>
  string(22) "(object value omitted)"
  ["children"]=>
  string(22) "(object value omitted)"
  ["firstElementChild"]=>
  NULL
  ["lastElementChild"]=>
  NULL
  ["childElementCount"]=>
  int(0)
  ["previousElementSibling"]=>
  string(22) "(object value omitted)"
  ["nextElementSibling"]=>
  NULL
  ["innerHTML"]=>
  string(0) ""
  ["outerHTML"]=>
  string(27) "<example:foo></example:foo>"
  ["substitutedNodeValue"]=>
  string(0) ""
  ["nodeType"]=>
  int(1)
  ["nodeName"]=>
  string(11) "example:foo"
  ["baseURI"]=>
  string(11) "about:blank"
  ["isConnected"]=>
  bool(true)
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
  string(22) "(object value omitted)"
  ["nextSibling"]=>
  NULL
  ["nodeValue"]=>
  NULL
  ["textContent"]=>
  string(0) ""
}"#);
}

/// php-src `ext/dom/tests/gh16465.phpt`: GH-16465 (Heap buffer overflow in DOMNode->getElementByTagName)
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMElement::__construct' expects 0 arguments, got 1"#]
fn test_gh16465() {
    if skip_without_xml_native("test_gh16465") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$v10 = new DOMElement("a");
try {
    $v10->getElementsByTagName("text\0something");
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $v10->getElementsByTagNameNS("", "text\0something");
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    $v10->getElementsByTagNameNS("text\0something", "");
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"DOMElement::getElementsByTagName(): Argument #1 ($qualifiedName) must not contain any null bytes
DOMElement::getElementsByTagNameNS(): Argument #2 ($localName) must not contain any null bytes
DOMElement::getElementsByTagNameNS(): Argument #1 ($namespace) must not contain any null bytes"#);
}

/// php-src `ext/dom/tests/gh16473.phpt`: GH-16473 (dom_import_simplexml stub is wrong)
#[test]
#[ignore = r#"DOM port pending: Undefined function: dom_import_simplexml"#]
fn test_gh16473() {
    if skip_without_xml_native("test_gh16473") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$root = simplexml_load_string('<root xmlns:x="urn:x" x:attr="foo"/>');
$attr = $root->attributes('urn:x');
var_dump(dom_import_simplexml($attr)->textContent);
"#);
    assert_exact(&out, r#"string(3) "foo""#);
}

/// php-src `ext/dom/tests/gh16533.phpt`: GH-16533 (Segfault when adding attribute to parent that is not an element)
#[test]
fn test_gh16533() {
    if skip_without_xml_native("test_gh16533") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
try {
    $doc->appendChild($doc->createAttribute('foo'));
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo $doc->saveXML();

"#);
    assert_exact(&out, r#"Hierarchy Request Error
<?xml version="1.0"?>"#);
}

/// php-src `ext/dom/tests/gh16535.phpt`: GH-16535 (UAF when using document as a child)
#[test]
fn test_gh16535() {
    if skip_without_xml_native("test_gh16535") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$v2 = new DOMDocument("t");

$v2->loadHTML("t");
$v4 = $v2->createElement('foo');
try {
    $v4->appendChild($v2);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
$v2->loadHTML("<p>oU</p>");
echo $v2->saveXML();

"#);
    assert_exact(&out, r#"Hierarchy Request Error
<?xml version="1.0" standalone="yes"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<html><body><p>oU</p></body></html>"#);
}

/// php-src `ext/dom/tests/gh16593.phpt`: GH-16593 (Assertion failure in DOM->replaceChild)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createAttribute"#]
fn test_gh16593() {
    if skip_without_xml_native("test_gh16593") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;
$root = $doc->appendChild($doc->createElement('root'));
$child = $root->appendChild($doc->createElement('child'));
try {
    $root->replaceChild($doc->createAttribute('foo'), $child);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
echo $doc->saveXML();

"#);
    assert_exact(&out, r#"Hierarchy Request Error
<?xml version="1.0"?>
<root><child/></root>"#);
}

/// php-src `ext/dom/tests/gh16594.phpt`: GH-16594 (Assertion failure in DOM -> before)
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMText::__construct' expects 0 arguments, got 1"#]
fn test_gh16594() {
    if skip_without_xml_native("test_gh16594") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$v1 = new DOMText("wr");
$v2 = new DOMDocument();
$v6 = new DOMComment("aw");
$v7 = new DOMAttr("r", "iL");

$v9 = $v2->createElement("test");
$v9->setAttributeNodeNS($v7);
$v7->appendChild($v1);

try {
    $v1->before($v6);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Hierarchy Request Error"#);
}

/// php-src `ext/dom/tests/gh16595.phpt`: GH-16595 (Another UAF in DOM -> cloneNode)
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMElement::__construct' expects 0 arguments, got 1"#]
fn test_gh16595() {
    if skip_without_xml_native("test_gh16595") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$v0 = new DOMElement ( "jg" );
$v1 = new DOMDocument ( "Zb" );
$v2 = new DOMElement ( "IU" );
$v7 = new DOMElement ( "L" , null , "df" );
$v9 = new DOMDocument (  );

try { $v1 -> insertBefore ( $v0 , $v9 ); } catch (\Throwable) { }
$v0 -> replaceChildren ( $v7 );
$v7 -> before ( $v2 );
$v1 -> insertBefore ( $v0 );
$v2 -> cloneNode (  );
echo $v1->saveXML();
echo $v9->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="Zb"?>
<jg xmlns:default="df"><IU/><default:L xmlns="df"/></jg>
<?xml version="1.0"?>"#);
}

/// php-src `ext/dom/tests/gh16777_1.phpt`: GH-16777 (Calling the constructor again on a DOM object after it is in a document causes UAF)
#[test]
fn test_gh16777_1() {
    if skip_without_xml_native("test_gh16777_1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$text = new DOMText('my value');
$doc = new DOMDocument();
$doc->appendChild($text);
$text->__construct('my new value');
$doc->appendChild($text);
echo $doc->saveXML();
$dom2 = new DOMDocument();
try {
    $dom2->appendChild($text);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
my value
my new value
Wrong Document Error"#);
}

/// php-src `ext/dom/tests/gh16777_2.phpt`: GH-16777 (Calling the constructor again on a DOM object after it is in a document causes UAF)
#[test]
fn test_gh16777_2() {
    if skip_without_xml_native("test_gh16777_2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$el = new DOMElement('name');
$el->append($child = new DOMElement('child'));
$doc = new DOMDocument();
$doc->appendChild($el);
$el->__construct('newname');
$doc->appendChild($el);
echo $doc->saveXML();
$dom2 = new DOMDocument();
try {
    $dom2->appendChild($el);
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}
var_dump($child->ownerDocument === $doc);
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<name><child/></name>
<newname/>
Wrong Document Error
bool(true)"#);
}

/// php-src `ext/dom/tests/gh16906.phpt`: GH-16906 (Reloading document can cause UAF in iterator)
#[test]
fn test_gh16906() {
    if skip_without_xml_native("test_gh16906") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXML('<?xml version="1.0"?><span><strong id="1"/><strong id="2"/></span>');
$list = $doc->getElementsByTagName('strong');
$doc->load(__DIR__."/book.xml");
var_dump($list);
"#);
    assert_exact(&out, r#"object(DOMNodeList)#2 (1) {
  ["length"]=>
  int(0)
}"#);
}

/// php-src `ext/dom/tests/gh17145.phpt`: GH-17145 (DOM memory leak)
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMElement::__construct' expects 0 arguments, got 3"#]
fn test_gh17145() {
    if skip_without_xml_native("test_gh17145") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$element = new DOMElement("N", "W", "y");
$attr = new DOMAttr("c" , "n");
$doc = new DOMDocument();
$doc->appendChild($element);
$element->setAttributeNodeNS($attr);
$attr->appendChild($doc->createEntityReference('amp'));
echo $attr->value;
"#);
    assert_exact(&out, r#"n&"#);
}

/// php-src `ext/dom/tests/gh17201.phpt`: GH-17201 (Dom\TokenList issues with interned string replace)
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_gh17201() {
    if skip_without_xml_native("test_gh17201") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = DOM\XMLDocument::createFromString('<root class="AA B C"/>');
$element = $dom->documentElement;
$list = $element->classList;
$list->replace('AA', 'AB'); // Use interned string
foreach ($list as $entry) {
    var_dump($entry);
}
"#);
    assert_exact(&out, r#"string(2) "AB"
string(1) "B"
string(1) "C""#);
}

/// php-src `ext/dom/tests/gh17223.phpt`: GH-17223 (Memory leak in libxml encoding handling)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::save"#]
fn test_gh17223() {
    if skip_without_xml_native("test_gh17223") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument("1.0", "Shift-JIS");
@$doc->save("%00");
echo "Done\n";
"#);
    assert_exact(&out, r#"Done"#);
}

/// php-src `ext/dom/tests/gh17224.phpt`: GH-17224 (UAF in importNode)
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::firstChild"#]
fn test_gh17224() {
    if skip_without_xml_native("test_gh17224") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$aDOM = new DOMDocument();
$fromdom = new DOMDocument();
$fromdom->loadXML('<data xmlns:ai="http://test.org" ai:attr="namespaced" />');
$attr = $fromdom->firstChild->attributes->item(0);
$att = $aDOM->importNode($attr);
$doc = new DOMDocument;
$fromdom->load(__DIR__."/book.xml");
unset($attr);
var_dump($att);
"#);
    assert_format(&out, r#"object(DOMAttr)#%d (%d) {
  ["name"]=>
  string(4) "attr"
  ["specified"]=>
  bool(true)
  ["value"]=>
  string(10) "namespaced"
  ["ownerElement"]=>
  NULL
  ["schemaTypeInfo"]=>
  NULL
  ["nodeName"]=>
  string(7) "ai:attr"
  ["nodeValue"]=>
  string(10) "namespaced"
  ["nodeType"]=>
  int(2)
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
  ["ownerDocument"]=>
  string(22) "(object value omitted)"
  ["namespaceURI"]=>
  string(15) "http://test.org"
  ["prefix"]=>
  string(2) "ai"
  ["localName"]=>
  string(4) "attr"
  ["baseURI"]=>
  NULL
  ["textContent"]=>
  string(10) "namespaced"
}"#);
}

/// php-src `ext/dom/tests/gh17397.phpt`: GH-17397 (Assertion failure ext/dom/php_dom.c)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_gh17397() {
    if skip_without_xml_native("test_gh17397") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = Dom\HTMLDocument::createFromString('<!DOCTYPE html><html></html>');
var_dump($dom->doctype->prefix);
echo $dom->saveXml();
"#);
    assert_format(&out, r#"Warning: Undefined property: Dom\DocumentType::$prefix in %s on line %d
NULL
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body></body></html>"#);
}

/// php-src `ext/dom/tests/gh17500.phpt`: GH-17500 (Segfault with requesting nodeName on nameless doctype)
#[test]
fn test_gh17500() {
    if skip_without_xml_native("test_gh17500") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$doc->loadHTML("<!DOCTYPE>", LIBXML_NOERROR);
var_dump($doc->doctype->nodeName);

"#);
    assert_exact(&out, r#"string(0) """#);
}

/// php-src `ext/dom/tests/gh17847.phpt`: GH-17847 (xinclude destroys live node)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_gh17847() {
    if skip_without_xml_native("test_gh17847") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$doc->loadXML(<<<XML
<root>
    <xi:include href="thisisnonexistent" xmlns:xi="http://www.w3.org/2001/XInclude">
        <xi:fallback>fallback<p>garbage</p></xi:fallback>
        <p>garbage</p>
    </xi:include>
    <xi:test xmlns:xi="http://www.w3.org/2001/XInclude">
      <xi:include href="thisisnonexistent">
        <p attr="foo" attr2="bar">garbage</p>
      </xi:include>
    </xi:test>
</root>
XML);

$xpath = new DOMXPath($doc);

$garbage = [];
foreach ($xpath->query('//p') as $entry) {
    $garbage[] = $entry;
    foreach ($entry->attributes as $attr) {
        $garbage[] = $attr;
        foreach ($attr->childNodes as $child) {
            $garbage[] = $child;
        }
    }
}

@$doc->xinclude();

foreach ($garbage as $node) {
    try {
        var_dump($node->localName);
    } catch (DOMException $e) {
        echo $e->getMessage(), "\n";
    }
}
"#);
    assert_exact(&out, r#"Invalid State Error
Invalid State Error
Invalid State Error
Invalid State Error
Invalid State Error
Invalid State Error
Invalid State Error"#);
}

/// php-src `ext/dom/tests/gh17991.phpt`: GH-17991 (Assertion failure dom_attr_value_write)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMAttr"#]
fn test_gh17991() {
    if skip_without_xml_native("test_gh17991") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$attr = new DOMAttr("r", "iL");
class Box {
    public ?Test $value;
}
class Test {
}
function test($box) {
    var_dump($box->value = new Test);
}
$box = new Box();
test($box);
test($attr);
"#);
    assert_format(&out, r#"object(Test)#%d (0) {
}

Fatal error: Uncaught TypeError: Cannot assign Test to property DOMAttr::$value of type string in %s:%d
Stack trace:
#0 %s(%d): test(Object(DOMAttr))
#1 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/gh18304.phpt`: GH-18304 (Changing the properties of a DateInterval through dynamic properties triggers a SegFault)
#[test]
fn test_gh18304() {
    if skip_without_xml_native("test_gh18304") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$text = new \DOMText();
$field = 'textContent';
$text->$field .= 'hello';
var_dump($text->$field);
"#);
    assert_exact(&out, r#"string(5) "hello""#);
}

/// php-src `ext/dom/tests/gh19612.phpt`: GH-19612 (Mitigate libxml2 tree dictionary bug)
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::adoptNode"#]
fn test_gh19612() {
    if skip_without_xml_native("test_gh19612") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = new DOMDocument;
$xml->loadXML(<<<XML
<!DOCTYPE root [
<!ENTITY foo "foo">
]>
<root><el x="&foo;"/></root>
XML);
$html = new DOMDocument;
$html->loadHTML('<p>foo</p>', LIBXML_NOERROR);
$p = $html->documentElement->firstChild->firstChild;
$p->appendChild($html->adoptNode($xml->documentElement->firstElementChild->cloneNode(true)));

echo $html->saveXML();
echo $xml->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0" standalone="yes"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<html><body><p>foo<el x="&foo;"/></p></body></html>
<?xml version="1.0"?>
<!DOCTYPE root [
<!ENTITY foo "foo">
]>
<root><el x="&foo;"/></root>"#);
}

/// php-src `ext/dom/tests/gh20722.phpt`: GH-20722 (Null pointer dereference in DOM namespace node cloning via clone on malformed objects)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMNameSpaceNode"#]
fn test_gh20722() {
    if skip_without_xml_native("test_gh20722") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

clone new DOMNameSpaceNode();
echo "Done";

"#);
    assert_exact(&out, r#"Done"#);
}

/// php-src `ext/dom/tests/gh22077.phpt`: GH-22077 (UAF in custom XPath function)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_gh22077() {
    if skip_without_xml_native("test_gh22077") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$document = new DOMDocument;
$xpath = new DOMXPath($document);
$xpath->registerNamespace("my", "my.ns");
$xpath->registerPHPFunctionNS('my.ns', 'include', function(): DOMElement {
    $includedDocument = new DOMDocument;
    $includedDocument->loadXML('<root><uaf/><node/><uaf/></root>');
    return $includedDocument->documentElement;
});
$nodeset = $xpath->query('my:include()/uaf');
$node = $nodeset->item(0);
var_dump($nodeset->length);
var_dump($node->ownerDocument->saveXML($node));
"#);
    assert_exact(&out, r#"int(2)
string(6) "<uaf/>""#);
}

/// php-src `ext/dom/tests/gh22219.phpt`: GH-22219 (Dom\XMLDocument::schemaValidate fails to resolve xs:QName value from an in-scope prefix)
#[test]
#[ignore = r#"DOM port pending: Undefined function: libxml_use_internal_errors"#]
fn test_gh22219() {
    if skip_without_xml_native("test_gh22219") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$xml = <<<XML
<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:test" xmlns:ref="urn:other">
  <item target="ref:something"/>
</root>
XML;

// The 'ref' prefix is declared on <root> but only used inside the xs:QName
// attribute value, never as an element or attribute namespace.
$xsd = <<<XSD
<?xml version="1.0" encoding="UTF-8"?>
<schema xmlns="http://www.w3.org/2001/XMLSchema"
        targetNamespace="urn:test" elementFormDefault="qualified">
  <element name="root">
    <complexType>
      <sequence>
        <element name="item">
          <complexType>
            <attribute name="target" type="QName"/>
          </complexType>
        </element>
      </sequence>
    </complexType>
  </element>
</schema>
XSD;

libxml_use_internal_errors(true);

$modern = Dom\XMLDocument::createFromString($xml, LIBXML_NSCLEAN);
var_dump($modern->schemaValidateSource($xsd));

$legacy = new DOMDocument();
$legacy->loadXML($xml, LIBXML_NSCLEAN);
var_dump($legacy->schemaValidateSource($xsd));

foreach (libxml_get_errors() as $error) {
    echo trim($error->message), PHP_EOL;
}
"#);
    assert_exact(&out, r#"bool(true)
bool(true)"#);
}

/// php-src `ext/dom/tests/gh8996.phpt`: GH-8996: DOMNode serialization on PHP ^8.1
#[test]
#[ignore = r#"DOM port pending: Method SerializableDomDocumentSleepWakeup::loadXML parameter $source expects string, got null|string"#]
fn test_gh8996() {
    if skip_without_xml_native("test_gh8996") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

echo "=== __sleep and __wakeup ===\n";

class SerializableDomDocumentSleepWakeup extends DOMDocument
{
    private $xmlData;

    public function __sleep(): array
    {
        $this->xmlData = $this->saveXML();
        return ['xmlData'];
    }

    public function __wakeup(): void
    {
        $this->loadXML($this->xmlData);
    }
}

$dom = new SerializableDomDocumentSleepWakeup('1.0', 'UTF-8');
$dom->loadXML('<tag>value</tag>');

$serialized = serialize($dom);
var_dump($serialized);
$unserialized = unserialize($serialized);

echo "Serialized:\n-----------\n$serialized\n-----------\nRestored:\n-----------\n{$unserialized->saveXml()}";

echo "=== __serialize and __unserialize ===\n";

class SerializableDomDocument__Serialize__Unserialize extends DOMDocument
{
    public function __serialize(): array
    {
        return ['xmlData' => $this->saveXML()];
    }

    public function __unserialize(array $data): void
    {
        $this->loadXML($data['xmlData']);
    }
}

$dom = new SerializableDomDocument__Serialize__Unserialize('1.0', 'UTF-8');
$dom->loadXML('<tag>value</tag>');

$serialized = serialize($dom);
$unserialized = unserialize($serialized);

echo "Serialized:\n-----------\n$serialized\n-----------\nRestored:\n-----------\n{$unserialized->saveXml()}";

echo "=== serialize and unserialize ===\n";

class SerializableDomDocumentSerializeUnserialize extends DOMDocument implements Serializable
{
    public function serialize(): ?string
    {
        return $this->saveXML();
    }

    public function unserialize(string $data): void
    {
        $this->loadXML($data);
    }
}

$dom = new SerializableDomDocumentSerializeUnserialize('1.0', 'UTF-8');
$dom->loadXML('<tag>value</tag>');

$serialized = serialize($dom);
$unserialized = unserialize($serialized);

echo "Serialized:\n-----------\n$serialized\n-----------\nRestored:\n-----------\n{$unserialized->saveXml()}";

"#);
    assert_format(&out, r#"=== __sleep and __wakeup ===
string(144) "O:34:"SerializableDomDocumentSleepWakeup":1:{s:43:"%0SerializableDomDocumentSleepWakeup%0xmlData";s:39:"<?xml version="1.0"?>
<tag>value</tag>
";}"
Serialized:
-----------
O:34:"SerializableDomDocumentSleepWakeup":1:{s:43:"%0SerializableDomDocumentSleepWakeup%0xmlData";s:39:"<?xml version="1.0"?>
<tag>value</tag>
";}
-----------
Restored:
-----------
<?xml version="1.0"?>
<tag>value</tag>
=== __serialize and __unserialize ===
Serialized:
-----------
O:47:"SerializableDomDocument__Serialize__Unserialize":1:{s:7:"xmlData";s:39:"<?xml version="1.0"?>
<tag>value</tag>
";}
-----------
Restored:
-----------
<?xml version="1.0"?>
<tag>value</tag>
=== serialize and unserialize ===

Deprecated: SerializableDomDocumentSerializeUnserialize implements the Serializable interface, which is deprecated. Implement __serialize() and __unserialize() instead (or in addition, if support for old PHP versions is necessary) in %s on line %d
Serialized:
-----------
C:43:"SerializableDomDocumentSerializeUnserialize":39:{<?xml version="1.0"?>
<tag>value</tag>
}
-----------
Restored:
-----------
<?xml version="1.0"?>
<tag>value</tag>"#);
}

/// php-src `ext/dom/tests/gh9142.phpt`: GH-9142 (DOMChildNode replaceWith() double-free error when replacing elements not separated by any whitespace)
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMXPath"#]
fn test_gh9142() {
    if skip_without_xml_native("test_gh9142") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$document = '<var>One</var><var>Two</var>';

($dom = new DOMDocument('1.0', 'UTF-8'))->loadHTML($document);

foreach ((new DOMXPath($dom))->query('//var') as $var) {
    $var->replaceWith($dom->createElement('p', $var->nodeValue));
}

var_dump($dom->saveHTML());

"#);
    assert_exact(&out, r#"string(154) "<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<html><body><p>One</p><p>Two</p></body></html>
""#);
}

/// php-src `ext/dom/tests/ghsa-p3x9-6h7p-cgfc_001.phpt`: GHSA-p3x9-6h7p-cgfc: libxml streams use wrong `content-type` header when requesting a redirected resource (Basic)
#[test]
#[ignore = r#"needs php-src's ext/standard/tests/http/server.inc test HTTP server"#]
fn test_ghsa_p3x9_6h7p_cgfc_001() {
    if skip_without_xml_native("test_ghsa_p3x9_6h7p_cgfc_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require "./ext/standard/tests/http/server.inc";

function genResponses($server) {
    $uri = 'http://' . stream_socket_get_name($server, false);
    yield "data://text/plain,HTTP/1.1 302 Moved Temporarily\r\nLocation: $uri/document.xml\r\nContent-Type: text/html;charset=utf-16\r\n\r\n";
    $xml = <<<'EOT'
        <!doctype html>
        <html>
            <head>
                <title>GHSA-p3x9-6h7p-cgfc</title>

                <meta charset="utf-8" />
                <meta http-equiv="Content-type" content="text/html; charset=utf-8" />
            </head>

            <body>
                <h1>GHSA-p3x9-6h7p-cgfc</h1>
            </body>
        </html>
        EOT;
    // Intentionally using non-standard casing for content-type to verify it is matched not case sensitively.
    yield "data://text/plain,HTTP/1.1 200 OK\r\nconteNt-tyPe: text/html; charset=utf-8\r\n\r\n{$xml}";
}

['pid' => $pid, 'uri' => $uri] = http_server('genResponses', $output);
$document = new \DOMDocument();
$document->loadHTMLFile($uri);

$h1 = $document->getElementsByTagName('h1');
var_dump($h1->length);
var_dump($document->saveHTML());
http_server_kill($pid);
"#);
    assert_exact(&out, r#"int(1)
string(266) "<!DOCTYPE html>
<html>
    <head>
        <title>GHSA-p3x9-6h7p-cgfc</title>

        <meta charset="utf-8">
        <meta http-equiv="Content-type" content="text/html; charset=utf-8">
    </head>

    <body>
        <h1>GHSA-p3x9-6h7p-cgfc</h1>
    </body>
</html>
""#);
}

/// php-src `ext/dom/tests/ghsa-p3x9-6h7p-cgfc_002.phpt`: GHSA-p3x9-6h7p-cgfc: libxml streams use wrong `content-type` header when requesting a redirected resource (Missing content-type)
#[test]
#[ignore = r#"needs php-src's ext/standard/tests/http/server.inc test HTTP server"#]
fn test_ghsa_p3x9_6h7p_cgfc_002() {
    if skip_without_xml_native("test_ghsa_p3x9_6h7p_cgfc_002") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require "./ext/standard/tests/http/server.inc";

function genResponses($server) {
    $uri = 'http://' . stream_socket_get_name($server, false);
    yield "data://text/plain,HTTP/1.1 302 Moved Temporarily\r\nLocation: $uri/document.xml\r\nContent-Type: text/html;charset=utf-16\r\n\r\n";
    $xml = <<<'EOT'
        <!doctype html>
        <html>
            <head>
                <title>GHSA-p3x9-6h7p-cgfc</title>

                <meta charset="utf-8" />
                <meta http-equiv="Content-type" content="text/html; charset=utf-8" />
            </head>

            <body>
                <h1>GHSA-p3x9-6h7p-cgfc</h1>
            </body>
        </html>
        EOT;
    // Missing content-type in actual response.
    yield "data://text/plain,HTTP/1.1 200 OK\r\n\r\n{$xml}";
}

['pid' => $pid, 'uri' => $uri] = http_server('genResponses', $output);
$document = new \DOMDocument();
$document->loadHTMLFile($uri);

$h1 = $document->getElementsByTagName('h1');
var_dump($h1->length);
var_dump($document->saveHTML());
http_server_kill($pid);
"#);
    assert_exact(&out, r#"int(1)
string(266) "<!DOCTYPE html>
<html>
    <head>
        <title>GHSA-p3x9-6h7p-cgfc</title>

        <meta charset="utf-8">
        <meta http-equiv="Content-type" content="text/html; charset=utf-8">
    </head>

    <body>
        <h1>GHSA-p3x9-6h7p-cgfc</h1>
    </body>
</html>
""#);
}

/// php-src `ext/dom/tests/ghsa-p3x9-6h7p-cgfc_003.phpt`: GHSA-p3x9-6h7p-cgfc: libxml streams use wrong `content-type` header when requesting a redirected resource (Reason with colon)
#[test]
#[ignore = r#"needs php-src's ext/standard/tests/http/server.inc test HTTP server"#]
fn test_ghsa_p3x9_6h7p_cgfc_003() {
    if skip_without_xml_native("test_ghsa_p3x9_6h7p_cgfc_003") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
require "./ext/standard/tests/http/server.inc";

function genResponses($server) {
    $uri = 'http://' . stream_socket_get_name($server, false);
    yield "data://text/plain,HTTP/1.1 302 Moved Temporarily\r\nLocation: $uri/document.xml\r\nContent-Type: text/html;charset=utf-16\r\n\r\n";
    $xml = <<<'EOT'
        <!doctype html>
        <html>
            <head>
                <title>GHSA-p3x9-6h7p-cgfc</title>

                <meta charset="utf-8" />
                <meta http-equiv="Content-type" content="text/html; charset=utf-8" />
            </head>

            <body>
                <h1>GHSA-p3x9-6h7p-cgfc</h1>
            </body>
        </html>
        EOT;
    // Missing content-type in actual response.
    yield "data://text/plain,HTTP/1.1 200 OK: This is fine\r\n\r\n{$xml}";
}

['pid' => $pid, 'uri' => $uri] = http_server('genResponses', $output);
$document = new \DOMDocument();
$document->loadHTMLFile($uri);

$h1 = $document->getElementsByTagName('h1');
var_dump($h1->length);
var_dump($document->saveHTML());
http_server_kill($pid);
"#);
    assert_exact(&out, r#"int(1)
string(266) "<!DOCTYPE html>
<html>
    <head>
        <title>GHSA-p3x9-6h7p-cgfc</title>

        <meta charset="utf-8">
        <meta http-equiv="Content-type" content="text/html; charset=utf-8">
    </head>

    <body>
        <h1>GHSA-p3x9-6h7p-cgfc</h1>
    </body>
</html>
""#);
}
