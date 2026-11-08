//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `modern/html/parser` folder, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/modern/html/parser/Element_innerHTML.phpt`: Test writing Element::$innerHTML on HTML documents
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOM\HTMLDocument"#]
fn test_element_inner_html() {
    if skip_without_xml_native("test_element_inner_html") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = DOM\HTMLDocument::createEmpty();
$el = $dom->createElement('div');
$dom->appendChild($el);
$el->innerHTML = '<p>foo</p>';
echo $dom->saveXML(), "\n";
$el->innerHTML = '</div>';
echo $dom->saveXML(), "\n";
$el->innerHTML = '';
echo $dom->saveXML(), "\n";
$el->innerHTML = '<div></div>&nbsp;<p></p>';
echo $dom->saveXML(), "\n";
$el->innerHTML = "invalid\xffutf-8𐍈𐍈𐍈";
echo $dom->saveXML(), "\n";

// Create a non-interned string that gets the UTF-8 validity flag added
$str = str_repeat("my valid string", random_int(1, 1));
preg_match('/^.*$/u', $str);
$el->innerHTML = $str;
echo $dom->saveXML(), "\n";

$dom = DOM\HTMLDocument::createEmpty();
$el = $dom->createElement('style');
$dom->appendChild($el);
$el->innerHTML = '<p>foo</p>';
echo $dom->saveXML(), "\n";

$dom = DOM\HTMLDocument::createEmpty();
$el = $dom->createElementNS('urn:a', 'style');
$dom->appendChild($el);
$el->innerHTML = '<p>foo</p>';
echo $dom->saveXML(), "\n";

$dom = DOM\HTMLDocument::createEmpty();
$el = $dom->createElement('textarea');
$dom->appendChild($el);
$el->innerHTML = "</textarea>\0-->";
echo $dom->saveXML(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<div xmlns="http://www.w3.org/1999/xhtml"><p>foo</p></div>
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<div xmlns="http://www.w3.org/1999/xhtml"></div>
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<div xmlns="http://www.w3.org/1999/xhtml"></div>
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<div xmlns="http://www.w3.org/1999/xhtml"><div></div> <p></p></div>
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<div xmlns="http://www.w3.org/1999/xhtml">invalid�utf-8𐍈𐍈𐍈</div>
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<div xmlns="http://www.w3.org/1999/xhtml">my valid string</div>
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<style xmlns="http://www.w3.org/1999/xhtml">&lt;p&gt;foo&lt;/p&gt;</style>
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<style xmlns="urn:a"><p xmlns="http://www.w3.org/1999/xhtml">foo</p></style>
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<textarea xmlns="http://www.w3.org/1999/xhtml">&lt;/textarea&gt;�--&gt;</textarea>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/Element_outerHTML.phpt`: Test writing Element::$outerHTML on HTML documents
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_outer_html() {
    if skip_without_xml_native("test_element_outer_html") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString('<p>foo</p>', LIBXML_NOERROR);
$p = $dom->body->firstChild;
$p->outerHTML = '<div></div>&nbsp;<p>'; // intentionally unclosed
echo $dom->saveXML(), "\n";
echo $dom->saveHtml(), "\n";
$div = $dom->body->firstChild;
$div->outerHTML = "invalid\xffutf-8𐍈𐍈𐍈";
echo $dom->saveXML(), "\n";
echo $dom->saveHtml(), "\n";

$dom->body->outerHTML = '<template><p>foo</p></template>';
var_dump($dom->body->querySelector('p')); // Should be NULL because the template contents do not participate in the DOM tree
echo $dom->saveXML(), "\n";
echo $dom->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><div></div> <p></p></body></html>
<html><head></head><body><div></div>&nbsp;<p></p></body></html>
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body>invalid�utf-8𐍈𐍈𐍈 <p></p></body></html>
<html><head></head><body>invalid�utf-8𐍈𐍈𐍈&nbsp;<p></p></body></html>
NULL
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><head><template><p>foo</p></template></head><body></body></html>
<html><head></head><head><template><p>foo</p></template></head><body></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_createFromFile_createFromString_BOM_buffer_edge.phpt`: Dom\HTMLDocument::createFromFile()/createFromString() BOM with a buffer on the edge
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_create_from_file_create_from_string_bom_buffer_edge() {
    if skip_without_xml_native("test_htmldocument_create_from_file_create_from_string_bom_buffer_edge") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$header = "\xEF\xBB\xBF<!doctype html><html><body>";
$trailer = "</body></html>";
$data = $header . str_repeat("a", 4096 - strlen($header) - strlen($trailer)) . $trailer;

$dom = Dom\HTMLDocument::createFromString($header . str_repeat("a", 4096 - strlen($header) - strlen($trailer)) . $trailer);
var_dump($dom->documentElement->textContent);

echo $dom->saveHtml(), "\n";

file_put_contents(__DIR__ . "/BOM_edge.tmp", $data);
$dom = Dom\HTMLDocument::createFromFile(__DIR__ . "/BOM_edge.tmp");
var_dump($dom->documentElement->textContent);

"#);
    assert_exact(&out, r#"string(4052) "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
<!DOCTYPE html><html><head></head><body>aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa</body></html>
string(4052) "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa""#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_createFromString_namespaced_attributes.phpt`: HTMLDocument::createFromString() with namespaced attributes
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_create_from_string_namespaced_attributes() {
    if skip_without_xml_native("test_htmldocument_create_from_string_namespaced_attributes") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE html>
<html>
    <svg width="1" xmlns:xlink='http://www.w3.org/1999/xlink'>
        <use xlink:href='#test' foo="bar"></use>
    </svg>
    <math>
        <mo accent="true"></mo>
    </math>
</html>
HTML);

foreach (['svg', 'use', 'mo'] as $tag) {
    $el = $dom->getElementsByTagName($tag)[0];
    foreach ($el->attributes as $attribute) {
        echo "Attribute: \n";
        var_dump($attribute->name, $attribute->value, $attribute->namespaceURI);
    }
}

"#);
    assert_exact(&out, r##"Attribute: 
string(5) "width"
string(1) "1"
NULL
Attribute: 
string(11) "xmlns:xlink"
string(28) "http://www.w3.org/1999/xlink"
string(29) "http://www.w3.org/2000/xmlns/"
Attribute: 
string(10) "xlink:href"
string(5) "#test"
string(28) "http://www.w3.org/1999/xlink"
Attribute: 
string(3) "foo"
string(3) "bar"
NULL
Attribute: 
string(6) "accent"
string(4) "true"
NULL"##);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromFile_DOM_HTML_NO_DEFAULT_NS.phpt`: Dom\HTMLDocument::createFromFile() with Dom\HTML_NO_DEFAULT_NS
#[test]
#[ignore = r#"DOM port pending: Undefined constant: LIBXML_HTML_NOIMPLIED"#]
fn test_htmldocument_from_file_dom_html_no_default_ns() {
    if skip_without_xml_native("test_htmldocument_from_file_dom_html_no_default_ns") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromFile(__DIR__ . "/paragraph.html", LIBXML_HTML_NOIMPLIED | LIBXML_NOERROR);
$xpath = new Dom\XPath($dom);
$xpath->registerNamespace("x", "http://www.w3.org/1999/xhtml");
var_dump($xpath->query("//p"));
var_dump($xpath->query("//x:p"));

$dom = Dom\HTMLDocument::createFromFile(__DIR__ . "/paragraph.html", LIBXML_HTML_NOIMPLIED | LIBXML_NOERROR | Dom\HTML_NO_DEFAULT_NS);
$xpath = new Dom\XPath($dom);
$xpath->registerNamespace("x", "http://www.w3.org/1999/xhtml");
var_dump($xpath->query("//p"));
var_dump($xpath->query("//x:p"));

"#);
    assert_exact(&out, r#"object(Dom\NodeList)#3 (1) {
  ["length"]=>
  int(0)
}
object(Dom\NodeList)#4 (1) {
  ["length"]=>
  int(1)
}
object(Dom\NodeList)#3 (1) {
  ["length"]=>
  int(1)
}
object(Dom\NodeList)#3 (1) {
  ["length"]=>
  int(0)
}"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromFile_empty_path.phpt`: Dom\HTMLDocument::createFromFile() - empty path
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_file_empty_path() {
    if skip_without_xml_native("test_htmldocument_from_file_empty_path") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromFile("");

"#);
    assert_fatal_format(&out, r#"Fatal error: Uncaught ValueError: Path must not be empty in %s:%d
Stack trace:
#0 %s(%d): Dom\HTMLDocument::createFromFile('')
#1 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromFile_local_existing_file.phpt`: Dom\HTMLDocument::createFromFile() - local existing file
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_file_local_existing_file() {
    if skip_without_xml_native("test_htmldocument_from_file_local_existing_file") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromFile(__DIR__ . "/../../../test.html");
echo $dom->saveHtml(), "\n";

"#);
    assert_format(&out, r#"Warning: Dom\HTMLDocument::createFromFile(): tree error unexpected-token-in-initial-mode in %s on line %d
<html><head>
<title>Hello world</title>
</head>
<body>
This is a not well-formed<br>
html files with undeclared entities&nbsp;


</body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromFile_local_file_does_not_exist.phpt`: Dom\HTMLDocument::createFromFile() - local file that does not exist
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_file_local_file_does_not_exist() {
    if skip_without_xml_native("test_htmldocument_from_file_local_file_does_not_exist") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromFile(__DIR__ . "/../foobar");
echo $dom->saveHtml(), "\n";

"#);
    assert_format(&out, r#"Warning: Dom\HTMLDocument::createFromFile(%s): Failed to open stream: No such file or directory in %s on line %d

Fatal error: Uncaught Exception: Cannot open file '%s' in %s:%d
Stack trace:
#0 %s(%d): Dom\HTMLDocument::createFromFile('%s')
#1 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromFile_nul_terminator_cases_path.phpt`: Dom\HTMLDocument::createFromFile() - NUL terminator cases path
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_file_nul_terminator_cases_path() {
    if skip_without_xml_native("test_htmldocument_from_file_nul_terminator_cases_path") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

try {
    Dom\HTMLDocument::createFromFile("\0");
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
try {
    Dom\HTMLDocument::createFromFile('%00');
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"Dom\HTMLDocument::createFromFile(): Argument #1 ($path) must not contain any null bytes
Dom\HTMLDocument::createFromFile(): Argument #1 ($path) must not contain percent-encoded NUL bytes"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromFile_parser_warning_01.phpt`: Dom\HTMLDocument::createFromFile() - parser warning 01
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_file_parser_warning_01() {
    if skip_without_xml_native("test_htmldocument_from_file_parser_warning_01") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromFile(__DIR__."/parser_warning_01.html", LIBXML_NOERROR);
echo $dom->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<html><head><title>foo</title>
</head><body><datalist id="fruits">
    <option value="Apple">
    </option><option value="Banana">
    </option><option value="Pear">
</option></datalist>
<p>error</p></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromFile_parser_warning_02.phpt`: Dom\HTMLDocument::createFromFile() - parser warning 02
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_file_parser_warning_02() {
    if skip_without_xml_native("test_htmldocument_from_file_parser_warning_02") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromFile(__DIR__."/parser_warning_02.html", LIBXML_NOERROR);
echo $dom->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<!DOCTYPE html><html><head></head><body>
        <p�>foo<p></p>
        <p id="foo" class="bar">
        <!--ELEMENT br EMPTY-->
        <!-- <!-- nested --> --&gt;
        
    
</p></p�></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromFile_parser_warning_03.phpt`: Dom\HTMLDocument::createFromFile() - parser warning 03
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_file_parser_warning_03() {
    if skip_without_xml_native("test_htmldocument_from_file_parser_warning_03") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromFile(__DIR__."/parser_warning_03.html", LIBXML_NOERROR);
echo $dom->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<!DOCTYPE html><html><head></head><body>
        <p>
    
</p></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromFile_parser_warning_libxml_get_last_error.phpt`: Dom\HTMLDocument::createFromFile() - parser warning libxml_get_last_error()
#[test]
#[ignore = r#"DOM port pending: Undefined function: libxml_use_internal_errors"#]
fn test_htmldocument_from_file_parser_warning_libxml_get_last_error() {
    if skip_without_xml_native("test_htmldocument_from_file_parser_warning_libxml_get_last_error") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

libxml_use_internal_errors(true);

$html = '<>x</> <!doctype html>';
$dom = Dom\HTMLDocument::createFromFile(__DIR__."/parser_warning_01.html");

var_dump(libxml_get_last_error());

"#);
    assert_format(&out, r#"object(LibXMLError)#2 (6) {
  ["level"]=>
  int(2)
  ["code"]=>
  int(1)
  ["column"]=>
  int(2)
  ["message"]=>
  string(%d) "tree error unexpected-token-in-initial-mode in %sparser_warning_01.html, line: 1, column: 2-6"
  ["file"]=>
  string(%d) "%sparser_warning_01.html"
  ["line"]=>
  int(1)
}"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromFile_with_failing_stream_wrapper.phpt`: Dom\HTMLDocument::createFromFile() with failing stream wrapper
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_file_with_failing_stream_wrapper() {
    if skip_without_xml_native("test_htmldocument_from_file_with_failing_stream_wrapper") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

class FailingWrapper {
    public $context;
    public $fail = false;

    public function stream_open($path, $mode, $options, &$opened_path) {
        return true;
    }

    public function stream_read($count) {
        if ($this->fail) {
            throw new Error("fail");
        }
        $this->fail = true;
        return str_repeat("X", $count);
    }

    public function stream_eof() {
        return false;
    }

    public function stream_close() {
        return true;
    }
}

stream_wrapper_register("fail", FailingWrapper::class, 0);

Dom\HTMLDocument::createFromFile("fail://x");

"#);
    assert_fatal_format(&out, r#"Fatal error: Uncaught Error: fail in %s:%d
Stack trace:
#0 [internal function]: FailingWrapper->stream_read(8192)
#1 %s(%d): Dom\HTMLDocument::createFromFile('fail://x')
#2 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromFile_with_working_stream_wrapper.phpt`: Dom\HTMLDocument::createFromFile() with working stream wrapper
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_file_with_working_stream_wrapper() {
    if skip_without_xml_native("test_htmldocument_from_file_with_working_stream_wrapper") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

class EchoUriWrapper {
    private int $position = 0;
    private string $data;
    public $context;

    public function stream_open($path, $mode, $options, &$opened_path) {
        $this->data = substr($path, 6);
        return true;
    }

    public function stream_read($count) {
        $ret = substr($this->data, $this->position, $count);
        $this->position += $count;
        return $ret;
    }

    public function stream_eof() {
        return $this->position >= strlen($this->data);
    }

    public function stream_close() {
        return true;
    }
}

stream_wrapper_register("euw", EchoUriWrapper::class, 0);

echo "--- Stream wrapper case ---\n";

$dom = Dom\HTMLDocument::createFromFile("euw://<p>hello</p>");
echo $dom->saveHtml(), "\n";

echo "--- Stream wrapper in two chunks case ---\n";

libxml_use_internal_errors(true);
// To properly test this, keep the 4096 in sync with document.c's input stream buffer size.
$dom = Dom\HTMLDocument::createFromFile("euw://<!doctype html><html>" . str_repeat("\n", 4096-22) . "<></html>");
echo $dom->saveHtml(), "\n";

foreach (libxml_get_errors() as $error) {
    var_dump($error->line, $error->column);
}

"#);
    assert_format(&out, r#"--- Stream wrapper case ---

Warning: Dom\HTMLDocument::createFromFile(): tree error unexpected-token-in-initial-mode in euw://<p>hello</p>, line: 1, column: 2 in %s on line %d
<html><head></head><body><p>hello</p></body></html>
--- Stream wrapper in two chunks case ---
<!DOCTYPE html><html><head></head><body>&lt;&gt;</body></html>
int(4075)
int(2)"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromString_DOM_HTML_NO_DEFAULT_NS.phpt`: Dom\HTMLDocument::createFromString() with Dom\HTML_NO_DEFAULT_NS
#[test]
#[ignore = r#"DOM port pending: Undefined constant: LIBXML_HTML_NOIMPLIED"#]
fn test_htmldocument_from_string_dom_html_no_default_ns() {
    if skip_without_xml_native("test_htmldocument_from_string_dom_html_no_default_ns") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(file_get_contents(__DIR__ . "/paragraph.html"), LIBXML_HTML_NOIMPLIED | LIBXML_NOERROR);
$xpath = new Dom\XPath($dom);
$xpath->registerNamespace("x", "http://www.w3.org/1999/xhtml");
var_dump($xpath->query("//p"));
var_dump($xpath->query("//x:p"));

$dom = Dom\HTMLDocument::createFromString(file_get_contents(__DIR__ . "/paragraph.html"), LIBXML_HTML_NOIMPLIED | LIBXML_NOERROR | Dom\HTML_NO_DEFAULT_NS);
$xpath = new Dom\XPath($dom);
$xpath->registerNamespace("x", "http://www.w3.org/1999/xhtml");
var_dump($xpath->query("//p"));
var_dump($xpath->query("//x:p"));

"#);
    assert_exact(&out, r#"object(Dom\NodeList)#3 (1) {
  ["length"]=>
  int(0)
}
object(Dom\NodeList)#4 (1) {
  ["length"]=>
  int(1)
}
object(Dom\NodeList)#3 (1) {
  ["length"]=>
  int(1)
}
object(Dom\NodeList)#3 (1) {
  ["length"]=>
  int(0)
}"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromString_LIBXML_COMPACT.phpt`: Dom\HTMLDocument::createFromString() with LIBXML_COMPACT
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_string_libxml_compact() {
    if skip_without_xml_native("test_htmldocument_from_string_libxml_compact") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE HTML>
<html>
<head>
</head>
<body>
    <p> x </p>
    <p>foo</p>
    <p>foox</p>
    <p>fooxx</p>
    <p>fooxxx</p>
    <p>fooxxxx</p>
    <p>fooxxxxx</p>
    <p>this does not fit</p>
</body>
</html>
HTML, LIBXML_COMPACT);

$xpath = new Dom\XPath($dom);
foreach ($xpath->query("//*[name()='p']") as $p) {
    echo $p->textContent, "\n";
}

?>
"#);
    assert_exact(&out, r#"x 
foo
foox
fooxx
fooxxx
fooxxxx
fooxxxxx
this does not fit"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromString_LIBXML_HTML_NOIMPLIED_error.phpt`: Dom\HTMLDocument::createFromString() with LIBXML_HTML_NOIMPLIED - tree error should not happen
#[test]
#[ignore = r#"DOM port pending: Undefined constant: LIBXML_HTML_NOIMPLIED"#]
fn test_htmldocument_from_string_libxml_html_noimplied_error() {
    if skip_without_xml_native("test_htmldocument_from_string_libxml_html_noimplied_error") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString("<div>foo</div>", LIBXML_HTML_NOIMPLIED);
echo $dom->saveHtml();

"#);
    assert_exact(&out, r#"<div>foo</div>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromString_LIBXML_HTML_NOIMPLIED_namespace.phpt`: Dom\HTMLDocument::createFromString() with LIBXML_HTML_NOIMPLIED namespace check
#[test]
#[ignore = r#"DOM port pending: Undefined constant: LIBXML_HTML_NOIMPLIED"#]
fn test_htmldocument_from_string_libxml_html_noimplied_namespace() {
    if skip_without_xml_native("test_htmldocument_from_string_libxml_html_noimplied_namespace") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

echo "--- No elements ---\n";

$dom = Dom\HTMLDocument::createFromString("", LIBXML_HTML_NOIMPLIED | LIBXML_NOERROR);
echo $dom->saveXml(), "\n";

echo "--- Single element ---\n";

$dom = Dom\HTMLDocument::createFromString("<p>foo</p>", LIBXML_HTML_NOIMPLIED | LIBXML_NOERROR);
echo $dom->saveXml(), "\n";
var_dump($dom->documentElement->namespaceURI);
var_dump($dom->documentElement->prefix);

echo "--- Multiple elements ---\n";

$dom = Dom\HTMLDocument::createFromString("<p>foo</p><strong>bar</strong>", LIBXML_HTML_NOIMPLIED | LIBXML_NOERROR);
echo $dom->saveXml(), "\n";
var_dump($dom->documentElement->namespaceURI);
var_dump($dom->documentElement->prefix);
var_dump($dom->documentElement->nextSibling->namespaceURI);
var_dump($dom->documentElement->nextSibling->prefix);

"#);
    assert_exact(&out, r#"--- No elements ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>

--- Single element ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p xmlns="http://www.w3.org/1999/xhtml">foo</p>
string(28) "http://www.w3.org/1999/xhtml"
NULL
--- Multiple elements ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p xmlns="http://www.w3.org/1999/xhtml">foo</p><strong xmlns="http://www.w3.org/1999/xhtml">bar</strong>
string(28) "http://www.w3.org/1999/xhtml"
NULL
string(28) "http://www.w3.org/1999/xhtml"
NULL"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromString_empty.phpt`: Dom\HTMLDocument::createFromString() - empty document
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_string_empty() {
    if skip_without_xml_native("test_htmldocument_from_string_empty") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString('');
echo $dom->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<html><head></head><body></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromString_fromFile_LIBXML_HTML_NOIMPLIED.phpt`: Dom\HTMLDocument::createFromString()/createFromFile() with LIBXML_HTML_NOIMPLIED
#[test]
#[ignore = r#"DOM port pending: Undefined constant: LIBXML_HTML_NOIMPLIED"#]
fn test_htmldocument_from_string_from_file_libxml_html_noimplied() {
    if skip_without_xml_native("test_htmldocument_from_string_from_file_libxml_html_noimplied") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php
function test(string $html) {
    echo "Testing: $html\n";
    $dom = Dom\HTMLDocument::createFromString($html, LIBXML_HTML_NOIMPLIED | LIBXML_NOERROR);
    $output = $dom->saveHtml();
    echo $output, "\n";

    // Also test the loadHTMLFile variation. We won't print out the result, just checking the result is the same.
    $temp = fopen(__DIR__."/DOM_HTMLDocument_loadHTML_LIBXML_HTML_NOIMPLIED_input.tmp", "w");
    fwrite($temp, $html);
    fclose($temp);
    $dom = Dom\HTMLDocument::createFromFile(__DIR__."/DOM_HTMLDocument_loadHTML_LIBXML_HTML_NOIMPLIED_input.tmp", LIBXML_HTML_NOIMPLIED | LIBXML_NOERROR);
    var_dump($output === $dom->saveHtml());
}

echo "--- Missing html, head, body ---\n";
test("");
test("foobarbaz");
test("<p>foo</p>");
echo "--- Missing html, head ---\n";
test("<body><p>foo</p></body>");
test("<title>x</title><p>foo</p>");
echo "--- Missing html, body ---\n";
test("<!-- bar --><head><title>x</title><!-- foo --></head><p>foo</p><!-- bar -->");
echo "--- Missing html ---\n";
test("<head><title>x</title></head><body><p>foo</p></body>");
echo "--- Missing head, body ---\n";
test("<html>foobar</html>");
test("<html><!-- foo --><title>a</title><p>foo</p></html><!-- bar -->");
echo "--- Missing head ---\n";
test("<html><!-- foo --><body>hi</body></html>");
echo "--- Missing nothing ---\n";
test("<html><head><title>x</title></head><!-- foo --><body><p>foo</p></body></html>");
echo "--- Malformed document ---\n";
test("<!-- start --><body><head><html>foo</html></head></body><!-- end -->");
"#);
    assert_exact(&out, r#"--- Missing html, head, body ---
Testing: 

bool(true)
Testing: foobarbaz
foobarbaz
bool(true)
Testing: <p>foo</p>
<p>foo</p>
bool(true)
--- Missing html, head ---
Testing: <body><p>foo</p></body>
<body><p>foo</p></body>
bool(true)
Testing: <title>x</title><p>foo</p>
<title>x</title><p>foo</p>
bool(true)
--- Missing html, body ---
Testing: <!-- bar --><head><title>x</title><!-- foo --></head><p>foo</p><!-- bar -->
<!-- bar --><head><title>x</title><!-- foo --></head><p>foo</p><!-- bar -->
bool(true)
--- Missing html ---
Testing: <head><title>x</title></head><body><p>foo</p></body>
<head><title>x</title></head><body><p>foo</p></body>
bool(true)
--- Missing head, body ---
Testing: <html>foobar</html>
<html>foobar</html>
bool(true)
Testing: <html><!-- foo --><title>a</title><p>foo</p></html><!-- bar -->
<html><!-- foo --><title>a</title><p>foo</p></html><!-- bar -->
bool(true)
--- Missing head ---
Testing: <html><!-- foo --><body>hi</body></html>
<html><!-- foo --><body>hi</body></html>
bool(true)
--- Missing nothing ---
Testing: <html><head><title>x</title></head><!-- foo --><body><p>foo</p></body></html>
<html><head><title>x</title></head><!-- foo --><body><p>foo</p></body></html>
bool(true)
--- Malformed document ---
Testing: <!-- start --><body><head><html>foo</html></head></body><!-- end -->
<!-- start --><body>foo</body><!-- end -->
bool(true)"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromString_line_column.phpt`: Dom\HTMLDocument::createFromString() - line and column test
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_string_line_column() {
    if skip_without_xml_native("test_htmldocument_from_string_line_column") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!doctype html>
<html>
    <head>
        <title>foo</title>
    </head>
    <body>
        <div id="mydiv" x="foo">
            <p>
                <strong>This is my paragraph</strong>
                <!-- my comment -->
            </p>
        </div>
    </body>
</html>
HTML);

$xpath = new Dom\XPath($dom);

foreach ($xpath->query("//*") as $element) {
    echo "Element: '", $element->tagName, "', ", $element->getLineNo(), "\n";
}

foreach ($xpath->query("//*[name()='strong']") as $element) {
    echo "Text: '", $element->textContent, "', ", $element->firstChild->getLineNo(), "\n";
}

foreach ($xpath->query("//*[name()='div']") as $element) {
    foreach ($element->attributes as $attribute) {
        echo "Attribute: '", $attribute->nodeName, "', ", $attribute->getLineNo(), "\n";
    }
}

foreach ($xpath->query("//comment()") as $comment) {
    echo "Comment: '", $comment->data, "', ", $comment->getLineNo(), "\n";
}

"#);
    assert_exact(&out, r#"Element: 'HTML', 1
Element: 'HEAD', 2
Element: 'TITLE', 3
Element: 'BODY', 5
Element: 'DIV', 6
Element: 'P', 7
Element: 'STRONG', 8
Text: 'This is my paragraph', 8
Attribute: 'id', 6
Attribute: 'x', 6
Comment: ' my comment ', 9"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromString_normal_no_error.phpt`: Dom\HTMLDocument::createFromString() - normal document, no error
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_string_normal_no_error() {
    if skip_without_xml_native("test_htmldocument_from_string_normal_no_error") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

// The closing p tag breaks libxml2's HTML parser, but doesn't break the HTML5 parser due to the script context parsing rules.
$html = <<<HTML
<!DOCTYPE HTML>
<html>
    <head>
        <meta charset="utf-8">
        <title>foo</title>
    </head>
    <body>
        <script>
        var foo = "</p>";
        </script>
        <p test="<script>">bar <!-- hi --></p>
    </body>
</html>
HTML;
$dom = Dom\HTMLDocument::createFromString($html);
echo $dom->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<!DOCTYPE html><html><head>
        <meta charset="utf-8">
        <title>foo</title>
    </head>
    <body>
        <script>
        var foo = "</p>";
        </script>
        <p test="<script>">bar <!-- hi --></p>
    
</body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromString_old_dtd.phpt`: Dom\HTMLDocument::createFromString(): Old DTD
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_string_old_dtd() {
    if skip_without_xml_native("test_htmldocument_from_string_old_dtd") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd">
<html>
<head>
</head>
<body>
</body>
</html>
HTML);

echo "--- HTML serialization ---\n";
echo $dom->saveHtml(), "\n";
echo "--- XML serialization ---\n";
echo $dom->saveXml();

"#);
    assert_format(&out, r#"Warning: Dom\HTMLDocument::createFromString(): tree error bad-doctype-token-in-initial-mode in Entity, line: 1, column: 3-9 in %s on line %d
--- HTML serialization ---
<!DOCTYPE html><html><head>
</head>
<body>

</body></html>
--- XML serialization ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd">
<html xmlns="http://www.w3.org/1999/xhtml"><head>
</head>
<body>

</body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromString_parser_warning_01.phpt`: Dom\HTMLDocument::createFromString() - parser warning 01
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_string_parser_warning_01() {
    if skip_without_xml_native("test_htmldocument_from_string_parser_warning_01") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$html = file_get_contents(__DIR__."/parser_warning_01.html");
$dom = Dom\HTMLDocument::createFromString($html);
echo $dom->saveHtml(), "\n";

"#);
    assert_format(&out, r#"Warning: Dom\HTMLDocument::createFromString(): tokenizer error missing-end-tag-name in Entity, line: 7, column: 11 in %s on line %d

Warning: Dom\HTMLDocument::createFromString(): tree error unexpected-token-in-initial-mode in Entity, line: 1, column: 2-6 in %s on line %d
<html><head><title>foo</title>
</head><body><datalist id="fruits">
    <option value="Apple">
    </option><option value="Banana">
    </option><option value="Pear">
</option></datalist>
<p>error</p></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromString_parser_warning_02.phpt`: Dom\HTMLDocument::createFromString() - parser warning 02
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_string_parser_warning_02() {
    if skip_without_xml_native("test_htmldocument_from_string_parser_warning_02") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$html = file_get_contents(__DIR__."/parser_warning_02.html");
$dom = Dom\HTMLDocument::createFromString($html);
echo $dom->saveHtml(), "\n";

"#);
    assert_format(&out, r#"Warning: Dom\HTMLDocument::createFromString(): tokenizer error unexpected-null-character in Entity, line: 4, column: 11 in %s on line %d

Warning: Dom\HTMLDocument::createFromString(): tokenizer error missing-whitespace-between-attributes in Entity, line: 5, column: 20 in %s on line %d

Warning: Dom\HTMLDocument::createFromString(): tokenizer error incorrectly-opened-comment in Entity, line: 6, column: 11 in %s on line %d

Warning: Dom\HTMLDocument::createFromString(): tokenizer error nested-comment in Entity, line: 7, column: 18 in %s on line %d

Warning: Dom\HTMLDocument::createFromString(): tree error unexpected-closed-token in Entity, line: 4, column: 18 in %s on line %d

Warning: Dom\HTMLDocument::createFromString(): tree error doctype-token-in-body-mode in Entity, line: 8, column: 11-17 in %s on line %d
<!DOCTYPE html><html><head></head><body>
        <p�>foo<p></p>
        <p id="foo" class="bar">
        <!--ELEMENT br EMPTY-->
        <!-- <!-- nested --> --&gt;
        
    
</p></p�></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromString_parser_warning_03.phpt`: Dom\HTMLDocument::createFromString() - parser warning 03
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_string_parser_warning_03() {
    if skip_without_xml_native("test_htmldocument_from_string_parser_warning_03") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$html = file_get_contents(__DIR__."/parser_warning_03.html");
$dom = Dom\HTMLDocument::createFromString($html, LIBXML_NOERROR);
echo $dom->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<!DOCTYPE html><html><head></head><body>
        <p>
    
</p></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromString_parser_warning_internal_error.phpt`: Dom\HTMLDocument::createFromString() - parser warning via internal error
#[test]
#[ignore = r#"DOM port pending: Undefined function: libxml_use_internal_errors"#]
fn test_htmldocument_from_string_parser_warning_internal_error() {
    if skip_without_xml_native("test_htmldocument_from_string_parser_warning_internal_error") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

libxml_use_internal_errors(true);

$html = '<>x</> <!doctype html>';
$dom = Dom\HTMLDocument::createFromString($html);
foreach (libxml_get_errors() as $error) {
    var_dump($error->message, $error->line, $error->column);
}

var_dump(libxml_get_last_error());

"#);
    assert_exact(&out, r#"string(81) "tokenizer error invalid-first-character-of-tag-name in Entity, line: 1, column: 2"
int(1)
int(2)
string(66) "tokenizer error missing-end-tag-name in Entity, line: 1, column: 6"
int(1)
int(6)
string(75) "tree error unexpected-token-in-initial-mode in Entity, line: 1, column: 1-7"
int(1)
int(1)
string(71) "tree error doctype-token-in-body-mode in Entity, line: 1, column: 10-16"
int(1)
int(10)
object(LibXMLError)#4 (6) {
  ["level"]=>
  int(2)
  ["code"]=>
  int(1)
  ["column"]=>
  int(10)
  ["message"]=>
  string(71) "tree error doctype-token-in-body-mode in Entity, line: 1, column: 10-16"
  ["file"]=>
  string(6) "Entity"
  ["line"]=>
  int(1)
}"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_fromString_without_body.phpt`: Dom\HTMLDocument::createFromString() - document without body
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_from_string_without_body() {
    if skip_without_xml_native("test_htmldocument_from_string_without_body") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString('<!doctype html><p align="\'">foo &apos;</p>');
echo $dom->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<!DOCTYPE html><html><head></head><body><p align="'">foo '</p></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/HTMLDocument_parse_options.phpt`: Dom\HTMLDocument: loading $options check
#[test]
#[ignore = r#"DOM port pending: Expected member name after '::'"#]
fn test_htmldocument_parse_options() {
    if skip_without_xml_native("test_htmldocument_parse_options") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$tested_options = [
    PHP_INT_MAX,
    LIBXML_BIGLINES,
    LIBXML_PARSEHUGE,
    LIBXML_DTDATTR,
    LIBXML_DTDLOAD,
    LIBXML_DTDVALID,
    LIBXML_HTML_NODEFDTD,
    LIBXML_NOBLANKS,
    LIBXML_NOCDATA,
    LIBXML_NOEMPTYTAG,
    LIBXML_NOENT,
    /*LIBXML_NSCLEAN, this collides with HTML_PARSE_NOIMPLIED */
    LIBXML_XINCLUDE,
    LIBXML_SCHEMA_CREATE,
    LIBXML_NONET,
    LIBXML_NOWARNING,
    LIBXML_PEDANTIC,
];

foreach (["createFromString", "createFromFile"] as $method) {
    echo "--- Method $method ---\n";
    foreach ($tested_options as $options) {
        var_dump($options);
        try {
            Dom\HTMLDocument::{$method}("x", $options);
        } catch (ValueError $e) {
            echo $e->getMessage(), "\n";
        }
    }
}

"#);
    assert_format(&out, r#"--- Method createFromString ---
int(%d)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(4194304)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(524288)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(8)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(4)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(16)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(4)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(256)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(16384)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(4)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(2)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(1024)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(1)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(2048)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(64)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(128)
Dom\HTMLDocument::createFromString(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
--- Method createFromFile ---
int(%d)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(4194304)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(524288)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(8)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(4)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(16)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(4)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(256)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(16384)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(4)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(2)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(1024)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(1)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(2048)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(64)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)
int(128)
Dom\HTMLDocument::createFromFile(): Argument #2 ($options) contains invalid flags (allowed flags: LIBXML_NOERROR, LIBXML_COMPACT, LIBXML_HTML_NOIMPLIED, Dom\HTML_NO_DEFAULT_NS)"#);
}

/// php-src `ext/dom/tests/modern/html/parser/gh17485.phpt`: GH-17485 (Self-closing tag on void elements shouldn't be a parse error/warning in \Dom\HTMLDocument)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_gh17485() {
    if skip_without_xml_native("test_gh17485") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php
$Data = "<!DOCTYPE HTML>\n<br />\n<input />";
$Document = \Dom\HTMLDocument::createFromString( $Data );
echo $Document->saveHTML();
"#);
    assert_exact(&out, r#"<!DOCTYPE html><html><head></head><body><br>
<input></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/gh17486.phpt`: GH-17486 (Incorrect error line numbers reported in Dom\HTMLDocument::createFromString)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_gh17486() {
    if skip_without_xml_native("test_gh17486") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php error_reporting(E_ALL);

$repeated = str_repeat('a', 50000);

$html = <<<HTML
<!DOCTYPE html>
<html lang="en">
	<body>
		<svg>
			<path d="{$repeated}" />
		</svg>
		<div>&#x0;</div>
	</body>
</html>
HTML;

\Dom\HTMLDocument::createFromString($html);

file_put_contents(__DIR__ . '/gh17486.tmp', $html);
\Dom\HTMLDocument::createFromFile(__DIR__ . '/gh17486.tmp');

"#);
    assert_format(&out, r#"Warning: Dom\HTMLDocument::createFromString(): tokenizer error null-character-reference in Entity, line: 7, column: 9 in %s on line %d

Warning: Dom\HTMLDocument::createFromFile(): tokenizer error null-character-reference in %s line: 7, column: 9 in %s on line %d"#);
}

/// php-src `ext/dom/tests/modern/html/parser/gh18090.phpt`: GH-18090 (Svg attributes and tag names are being lowercased)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_gh18090() {
    if skip_without_xml_native("test_gh18090") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php
echo \Dom\HTMLDocument::createFromString('<html><body><svg VIEWBOX="1 2 3 4"></svg></html>', LIBXML_NOERROR)->saveHTML(), "\n";

echo \Dom\HTMLDocument::createFromString('<html><body CLASS="no"><svg VIEWBOX="1 2 3 4"><feSpotLight x="10" y="10" z="50" pointsAtX="100" pointsAtY="100" limitingConeAngle="
10" /></svg></html>', LIBXML_NOERROR)->saveHTML(), "\n";

echo \Dom\HTMLDocument::createFromString('<html><body><svg VIEWBOX="1 2 3 4"></svg></html>', LIBXML_NOERROR)->querySelector('svg')->attributes[0]->name, "\n";
"#);
    assert_exact(&out, r#"<html><head></head><body><svg viewBox="1 2 3 4"></svg></body></html>
<html><head></head><body class="no"><svg viewBox="1 2 3 4"><feSpotLight x="10" y="10" z="50" pointsAtX="100" pointsAtY="100" limitingConeAngle="
10"></feSpotLight></svg></body></html>
viewBox"#);
}

/// php-src `ext/dom/tests/modern/html/parser/gh21486.phpt`: GH-21486 (Dom\HTMLDocument parser mangles xml:space and xml:lang attributes)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_gh21486() {
    if skip_without_xml_native("test_gh21486") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php
$d = @\Dom\HTMLDocument::createFromString("<div></div>");
$e = $d->getElementsByTagName("div")[0];
$e->innerHTML = '<svg xml:space="default" xlink:href="about:blank" xmlns:foo="barspace"></svg>';
$svg = $d->querySelector("svg");
echo $e->innerHTML."\n";
echo $svg->attributes[0]->localName." ".var_export($svg->attributes[0]->namespaceURI, true)."\n";
"#);
    assert_exact(&out, r#"<svg xml:space="default" xlink:href="about:blank" xmlns:foo="barspace"></svg>
space 'http://www.w3.org/XML/1998/namespace'"#);
}

/// php-src `ext/dom/tests/modern/html/parser/predefined_namespaces.phpt`: HTMLDocument: Predefined namespaces
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_predefined_namespaces() {
    if skip_without_xml_native("test_predefined_namespaces") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE html>
<html>
<head>
    <title>Test</title>
</head>
<body>
    <svg width="100" height="100" viewBox="0 0 4 2">
        <rect id="rectangle" x="10" y="20" width="90" height="60">
        </rect>
    </svg>
    <div>
        <p>foo</p>
    </div>
    <math>
        <!-- svg should be in the mathml namespace -->
        <mtable id="table"><svg></svg></mtable>
    </math>
</body>
</html>
HTML);

echo "--- Namespaces ---\n";
$xpath = new Dom\XPath($dom);
foreach ($xpath->query("//*[name()='body']//*") as $node) {
    echo $node->nodeName, " ", $node->namespaceURI ?? "(NONE)", "\n";
    foreach ($node->attributes as $attribute) {
        echo "  Attribute: ", $attribute->nodeName, " ", $attribute->namespaceURI ?? "(NONE)", "\n";
    }
}

echo "--- HTML serialization ---\n";
echo $dom->saveHtml(), "\n";
echo "--- XML serialization ---\n";
echo $dom->saveXml();

"#);
    assert_exact(&out, r#"--- Namespaces ---
svg http://www.w3.org/2000/svg
  Attribute: width (NONE)
  Attribute: height (NONE)
  Attribute: viewBox (NONE)
rect http://www.w3.org/2000/svg
  Attribute: id (NONE)
  Attribute: x (NONE)
  Attribute: y (NONE)
  Attribute: width (NONE)
  Attribute: height (NONE)
DIV http://www.w3.org/1999/xhtml
P http://www.w3.org/1999/xhtml
math http://www.w3.org/1998/Math/MathML
mtable http://www.w3.org/1998/Math/MathML
  Attribute: id (NONE)
svg http://www.w3.org/1998/Math/MathML
--- HTML serialization ---
<!DOCTYPE html><html><head>
    <title>Test</title>
</head>
<body>
    <svg width="100" height="100" viewBox="0 0 4 2">
        <rect id="rectangle" x="10" y="20" width="90" height="60">
        </rect>
    </svg>
    <div>
        <p>foo</p>
    </div>
    <math>
        <!-- svg should be in the mathml namespace -->
        <mtable id="table"><svg></svg></mtable>
    </math>

</body></html>
--- XML serialization ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml"><head>
    <title>Test</title>
</head>
<body>
    <svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 4 2">
        <rect id="rectangle" x="10" y="20" width="90" height="60">
        </rect>
    </svg>
    <div>
        <p>foo</p>
    </div>
    <math xmlns="http://www.w3.org/1998/Math/MathML">
        <!-- svg should be in the mathml namespace -->
        <mtable id="table"><svg/></mtable>
    </math>

</body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/parser/user_error_handler.phpt`: Parse HTML document with user error handler and error_reporting(0)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_user_error_handler() {
    if skip_without_xml_native("test_user_error_handler") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php error_reporting(0);

set_error_handler(function ($errno, $errstr, $errfile, $errline) {
    var_dump($errno, $errstr);
}, E_WARNING);

Dom\HTMLDocument::createFromString('<html></html>');

"#);
    assert_exact(&out, r#"int(2)
string(113) "Dom\HTMLDocument::createFromString(): tree error unexpected-token-in-initial-mode in Entity, line: 1, column: 2-5""#);
}

/// php-src `ext/dom/tests/modern/html/parser/xml_style_namespace.phpt`: HTMLDocument: XML-style namespace
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_xml_style_namespace() {
    if skip_without_xml_native("test_xml_style_namespace") {
        return;
    }
    let out = run_case(r#"modern/html/parser"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE html>
<html>
<head>
    <title>Test</title>
</head>
<body xmlns:foo="urn:hi">
    <foo:bar></foo:bar>
</body>
</html>
HTML);

echo "--- Namespaces ---\n";
$xpath = new Dom\XPath($dom);
foreach ($xpath->query("//*[name()='body']//*") as $node) {
    echo $node->nodeName, " ", $node->namespaceURI ?? "(NONE)", "\n";
    echo "prefix: \"", $node->prefix, "\"\n";
    foreach ($node->attributes as $attribute) {
        echo "  Attribute: ", $attribute->nodeName, " ", $attribute->namespaceURI ?? "(NONE)", "\n";
    }
}

echo "--- HTML serialization ---\n";
echo $dom->saveHtml(), "\n";
echo "--- XML serialization ---\n";
echo $dom->saveXml();

"#);
    assert_exact(&out, r#"--- Namespaces ---
FOO:BAR http://www.w3.org/1999/xhtml
prefix: ""
--- HTML serialization ---
<!DOCTYPE html><html><head>
    <title>Test</title>
</head>
<body xmlns:foo="urn:hi">
    <foo:bar></foo:bar>

</body></html>
--- XML serialization ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml"><head>
    <title>Test</title>
</head>
<body xmlns:foo="urn:hi">
    <foo:bar></foo:bar>

</body></html>"#);
}
