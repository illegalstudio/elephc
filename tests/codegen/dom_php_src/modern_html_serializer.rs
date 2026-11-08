//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `modern/html/serializer` folder, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/modern/html/serializer/Element_innerHTML.phpt`: Test reading Element::$innerHTML on HTML documents
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOM\HTMLDocument"#]
fn test_element_inner_html() {
    if skip_without_xml_native("test_element_inner_html") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = DOM\HTMLDocument::createFromString('<!DOCTYPE html><html><head><title>Test</title></head><body><div></div><p>Hello, World!</p></body></html>');
var_dump($dom->getElementsByTagName('body')[0]->innerHTML);
var_dump($dom->getElementsByTagName('head')[0]->innerHTML);
var_dump($dom->getElementsByTagName('html')[0]->innerHTML);
var_dump($dom->getElementsByTagName('div')[0]->innerHTML);
var_dump($dom->getElementsByTagName('p')[0]->innerHTML);

"#);
    assert_exact(&out, r#"string(31) "<div></div><p>Hello, World!</p>"
string(19) "<title>Test</title>"
string(76) "<head><title>Test</title></head><body><div></div><p>Hello, World!</p></body>"
string(0) ""
string(13) "Hello, World!""#);
}

/// php-src `ext/dom/tests/modern/html/serializer/Element_outerHTML.phpt`: Test reading Element::$outerHTML on HTML documents
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_outer_html() {
    if skip_without_xml_native("test_element_outer_html") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString('<p>foo</p>', LIBXML_NOERROR);

$p = $dom->body->firstChild;
var_dump($p->outerHTML);

$root = $dom->documentElement;
var_dump($root->outerHTML);

$unattached_element = $dom->createElement('unattached');
var_dump($unattached_element->outerHTML);

$template = $dom->createElement('template');
$template->innerHTML = '<p>foo</p>';
var_dump($template->outerHTML);

"#);
    assert_exact(&out, r#"string(10) "<p>foo</p>"
string(49) "<html><head></head><body><p>foo</p></body></html>"
string(25) "<unattached></unattached>"
string(31) "<template><p>foo</p></template>""#);
}

/// php-src `ext/dom/tests/modern/html/serializer/Element_outerHTML_invalid_tree.phpt`: Test reading Element::$outerHTML on HTML documents - invalid tree variation
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_element_outer_html_invalid_tree() {
    if skip_without_xml_native("test_element_outer_html_invalid_tree") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r##"<?php

$dom = Dom\HTMLDocument::createFromString('<a href="#one"><p>Link</p></a>', LIBXML_NOERROR);
$p = $dom->body->querySelector('p');
$p->outerHTML = '<a href="#two">Another Link</a>';
echo $dom->saveHTML();

"##);
    assert_exact(&out, r##"<html><head></head><body><a href="#one"><a href="#two">Another Link</a></a></body></html>"##);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_entity_reference.phpt`: Serialize entity reference
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createEntityReference"#]
fn test_htmldocument_entity_reference() {
    if skip_without_xml_native("test_htmldocument_entity_reference") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom1 = new DOMDocument();
$root = $dom1->appendChild($dom1->createElement('root'));
$root->appendChild($dom1->createEntityReference('nbsp'));

$dom2 = Dom\HTMLDocument::createEmpty();
$dom2->appendChild($dom2->importLegacyNode($root, true));

echo $dom2->saveXml(), "\n";
echo $dom2->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<root>&nbsp;</root>
<root>&nbsp;</root>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_escape_attribute.phpt`: Dom\HTMLDocument serialization escape attribute
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_escape_attribute() {
    if skip_without_xml_native("test_htmldocument_escape_attribute") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString("<p></p>", LIBXML_NOERROR);
$p = $dom->documentElement->firstChild->nextSibling->firstChild;
$p->setAttribute("foo", "<bar>\"'&");
echo $dom->saveHtml();

"#);
    assert_exact(&out, r#"<html><head></head><body><p foo="<bar>&quot;'&amp;"></p></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_escape_nbsp.phpt`: Dom\HTMLDocument serialization escape nbsp
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_escape_nbsp() {
    if skip_without_xml_native("test_htmldocument_escape_nbsp") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString("<p>these must transform: \xc2\xa0\xc2\xa0 but these not: \xa0|\xc2...</p><br>", LIBXML_NOERROR);
echo $dom->saveHtml();

"#);
    assert_exact(&out, r#"<html><head></head><body><p>these must transform: &nbsp;&nbsp; but these not: �|�...</p><br></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_attribute_ns.phpt`: Dom\HTMLDocument serialization of an attribute in a namespace
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_serialize_attribute_ns() {
    if skip_without_xml_native("test_htmldocument_serialize_attribute_ns") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$root = $dom->appendChild($dom->createElement("root"));
$root->setAttributeNodeNS($dom->createAttributeNS("http://php.net", "x:foo"));
$root->setAttributeNodeNS($dom->createAttributeNS("http://www.w3.org/XML/1998/namespace", "y:id"));
$root->setAttributeNodeNS($dom->createAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns"));
$root->setAttributeNodeNS($dom->createAttributeNS("http://www.w3.org/2000/xmlns/", "xmlns:f"));
$root->setAttributeNodeNS($dom->createAttributeNS("http://www.w3.org/1999/xlink", "z:f"));

// Note: XML declarations are not emitted in HTML5
echo $dom->saveHtml();

"#);
    assert_exact(&out, r#"<root x:foo="" xml:id="" xmlns="" xmlns:f="" xlink:f=""></root>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_cdata.phpt`: Dom\HTMLDocument serialization of CData
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_htmldocument_serialize_cdata() {
    if skip_without_xml_native("test_htmldocument_serialize_cdata") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

// Note: can't create CData in an HTML document.
$dom = Dom\XMLDocument::createEmpty();
$cdata = $dom->createCDATASection("foobaré\"<>-&");

$dom = Dom\HTMLDocument::createEmpty();
$container = $dom->appendChild($dom->createElement("container"));
$container->appendChild($dom->importNode($cdata));
echo $dom->saveHtml();

"#);
    assert_exact(&out, r#"<container>foobaré"&lt;&gt;-&amp;</container>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_comment.phpt`: Dom\HTMLDocument serialization of comment
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_serialize_comment() {
    if skip_without_xml_native("test_htmldocument_serialize_comment") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$dom->appendChild($dom->createComment("foobaré\"<>-&"));
echo $dom->saveHtml();

"#);
    assert_exact(&out, r#"<!--foobaré"<>-&-->"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_doctype.phpt`: Dom\HTMLDocument serialization of document type
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_serialize_doctype() {
    if skip_without_xml_native("test_htmldocument_serialize_doctype") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE HTML1234 PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd">
<html>
<head>
</head>
<body>
</body>
</html>
HTML, LIBXML_NOERROR);

echo "--- XML encoding ---\n";
echo $dom->saveXml(), "\n";
echo "--- HTML encoding ---\n";
// We don't expect to see the public ID and the system ID because the serialization algorithm doesn't serialize those
echo $dom->saveHtml(), "\n";

?>
"#);
    assert_exact(&out, r#"--- XML encoding ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE html1234 PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd">
<html xmlns="http://www.w3.org/1999/xhtml"><head>
</head>
<body>

</body></html>
--- HTML encoding ---
<!DOCTYPE html1234><html><head>
</head>
<body>

</body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_element_ns.phpt`: Dom\HTMLDocument serialization of element in a namespace
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_serialize_element_ns() {
    if skip_without_xml_native("test_htmldocument_serialize_element_ns") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$root = $dom->appendChild($dom->createElement("root"));

$root->append("\n");
$root->append($dom->createElementNS("http://php.net", "noprefix"), "\n");
$root->append($dom->createElementNS("http://php.net", "with:prefix"), "\n");
$root->append($dom->createElementNS("http://www.w3.org/1999/xhtml", "xhtml:br"), "\n");
$root->append($dom->createElementNS("http://www.w3.org/2000/svg", "s:svg"), "\n");
$root->append($dom->createElementNS("http://www.w3.org/1998/Math/MathML", "m:math"), "\n");

// Note: XML declarations are not emitted in HTML5
echo $dom->saveHtml();

"#);
    assert_exact(&out, r#"<root>
<noprefix></noprefix>
<with:prefix></with:prefix>
<br>
<svg></svg>
<math></math>
</root>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_failing_stream.phpt`: Dom\HTMLDocument serialization with a failing stream
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_serialize_failing_stream() {
    if skip_without_xml_native("test_htmldocument_serialize_failing_stream") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

class FailingWrapper {
    public $context;
    public $fail = false;

    public function stream_open($path, $mode, $options, &$opened_path) {
        return true;
    }

    public function stream_write(string $data): int {
        if ($this->fail) {
            throw new Error("fail");
        }
        $this->fail = true;
        var_dump($data);
        return strlen($data);
    }

    public function stream_eof() {
        return false;
    }

    public function stream_close() {
        return true;
    }
}

stream_wrapper_register("failing", "FailingWrapper");

$dom = Dom\HTMLDocument::createEmpty();
$root = $dom->appendChild($dom->createElement("root"));
$dom->saveHtmlFile("failing://foo");

"#);
    assert_format(&out, r#"string(1) "<"

Fatal error: Uncaught Error: fail in %s:%d
Stack trace:
#0 [internal function]: FailingWrapper->stream_write('root')
#1 %s(%d): Dom\HTMLDocument->saveHtmlFile('failing://foo')
#2 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_fragment.phpt`: Dom\HTMLDocument serialization of document fragment
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_serialize_fragment() {
    if skip_without_xml_native("test_htmldocument_serialize_fragment") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$fragment = $dom->createDocumentFragment();
$fragment->appendChild($dom->createElement("foo"));
$bar = $fragment->appendChild($dom->createElement("bar"));
$fragment->appendChild($dom->createElement("baz"));
$bar->appendChild($dom->createElement("inner"));
echo $dom->saveHtml($fragment);

"#);
    assert_exact(&out, r#"<foo></foo><bar><inner></inner></bar><baz></baz>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_full_document.phpt`: Dom\HTMLDocument serialization of full document
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_serialize_full_document() {
    if skip_without_xml_native("test_htmldocument_serialize_full_document") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE HTML>
<html lang="en">
    <head>
        <title>This is my épic title!</title>
        <meta charset="utf-8">
        <script>
        var foo="</p>";
        </script>
        <style>
        foo > bar {}
        </style>
    </head>
    <body>
        <main xmlns="some:ns" foo:bar="x" normal="x">
            <img alt="foo" src="/">
            <br>
            <test:test></test:test>
        </main>
    </body>
</html>
HTML);
echo $dom->saveHtml();

"#);
    assert_exact(&out, r#"<!DOCTYPE html><html lang="en"><head>
        <title>This is my épic title!</title>
        <meta charset="utf-8">
        <script>
        var foo="</p>";
        </script>
        <style>
        foo > bar {}
        </style>
    </head>
    <body>
        <main xmlns="some:ns" foo:bar="x" normal="x">
            <img alt="foo" src="/">
            <br>
            <test:test></test:test>
        </main>
    
</body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_ns_imported_01.phpt`: Dom\HTMLDocument serialization with an imported namespace node 01
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createElementNS"#]
fn test_htmldocument_serialize_ns_imported_01() {
    if skip_without_xml_native("test_htmldocument_serialize_ns_imported_01") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$xml = Dom\XMLDocument::createFromString('<?xml version="1.0"?><container xmlns="some:ns" xmlns:bar="another:ns"/>');
$xml->documentElement->setAttributeNS("http://foo/", "foo:bar", "value");
$xml->documentElement->appendChild($xml->createElementNS('some:ns2', 'child'));
echo $xml->saveXml(), "\n";

echo "--- After import into HTML ---\n";

$html = Dom\HTMLDocument::createFromString('<p>foo</p>', LIBXML_NOERROR);

$p = $html->documentElement->firstChild->nextSibling->firstChild;
$p->appendChild($html->importNode($xml->documentElement, true));

echo $html->saveXml(), "\n";
echo $html->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<container xmlns="some:ns" xmlns:bar="another:ns" xmlns:foo="http://foo/" foo:bar="value"><child xmlns="some:ns2"/></container>
--- After import into HTML ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><p>foo<container xmlns="some:ns" xmlns:bar="another:ns" xmlns:foo="http://foo/" foo:bar="value"><child xmlns="some:ns2"/></container></p></body></html>
<html><head></head><body><p>foo<container xmlns="some:ns" xmlns:bar="another:ns" foo:bar="value"><child></child></container></p></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_ns_imported_02.phpt`: Dom\HTMLDocument serialization with an imported namespace node 02
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createElementNS"#]
fn test_htmldocument_serialize_ns_imported_02() {
    if skip_without_xml_native("test_htmldocument_serialize_ns_imported_02") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$xml = Dom\XMLDocument::createFromFile(__DIR__.'/sample.xml');
$xml->documentElement->appendChild($xml->createElementNS('some:ns2', 'child'));
echo $xml->saveXml(), "\n";

echo "--- After import into HTML ---\n";

$html = Dom\HTMLDocument::createFromString('<p>foo</p>', LIBXML_NOERROR);

$p = $html->documentElement->firstChild->nextSibling->firstChild;
$p->appendChild($html->importNode($xml->documentElement, true));

echo $html->saveXml(), "\n";
echo $html->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<container xmlns="some:ns" xmlns:bar="another:ns">
    <x>
        <subcontainer>
            <test xmlns="x:y"/>
            <child2/>
        </subcontainer>
        <subcontainer2>
            <foo xmlns="some:ns"/>
        </subcontainer2>
    </x>
<child xmlns="some:ns2"/></container>
--- After import into HTML ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><p>foo<container xmlns="some:ns" xmlns:bar="another:ns">
    <x>
        <subcontainer>
            <test xmlns="x:y"/>
            <child2/>
        </subcontainer>
        <subcontainer2>
            <foo xmlns="some:ns"/>
        </subcontainer2>
    </x>
<child xmlns="some:ns2"/></container></p></body></html>
<html><head></head><body><p>foo<container xmlns="some:ns" xmlns:bar="another:ns">
    <x>
        <subcontainer>
            <test xmlns="x:y"></test>
            <child2></child2>
        </subcontainer>
        <subcontainer2>
            <foo xmlns="some:ns"></foo>
        </subcontainer2>
    </x>
<child></child></container></p></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_ns_imported_03.phpt`: Dom\HTMLDocument serialization with an imported namespace node 03
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createElementNS"#]
fn test_htmldocument_serialize_ns_imported_03() {
    if skip_without_xml_native("test_htmldocument_serialize_ns_imported_03") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$xml = Dom\XMLDocument::createFromFile(__DIR__.'/sample.xml');
$xml->documentElement->appendChild($xml->createElementNS('some:ns2', 'child'));
echo $xml->saveXml(), "\n";

echo "--- After import into HTML ---\n";

$html = Dom\HTMLDocument::createFromString('<p>foo</p>', LIBXML_NOERROR);

$p = $html->documentElement->firstChild->nextSibling->firstChild;
$p->appendChild($html->importNode($xml->documentElement, false));

echo $html->saveXml(), "\n";
echo $html->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<container xmlns="some:ns" xmlns:bar="another:ns">
    <x>
        <subcontainer>
            <test xmlns="x:y"/>
            <child2/>
        </subcontainer>
        <subcontainer2>
            <foo xmlns="some:ns"/>
        </subcontainer2>
    </x>
<child xmlns="some:ns2"/></container>
--- After import into HTML ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><p>foo<container xmlns="some:ns" xmlns:bar="another:ns"/></p></body></html>
<html><head></head><body><p>foo<container xmlns="some:ns" xmlns:bar="another:ns"></container></p></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_ns_imported_04.phpt`: Dom\HTMLDocument serialization with an imported namespace node 04
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createElementNS"#]
fn test_htmldocument_serialize_ns_imported_04() {
    if skip_without_xml_native("test_htmldocument_serialize_ns_imported_04") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$xml = Dom\XMLDocument::createFromFile(__DIR__.'/sample.xml');
$xml->documentElement->appendChild($xml->createElementNS('some:ns2', 'child'));
echo $xml->saveXml(), "\n";

echo "--- After import into HTML ---\n";

$html = Dom\HTMLDocument::createFromString('<p>foo</p>', LIBXML_NOERROR);

$p = $html->documentElement->firstChild->nextSibling->firstChild;
$p->appendChild($html->importNode($xml->documentElement, false));

echo $html->saveXml(), "\n";
echo $html->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<container xmlns="some:ns" xmlns:bar="another:ns">
    <x>
        <subcontainer>
            <test xmlns="x:y"/>
            <child2/>
        </subcontainer>
        <subcontainer2>
            <foo xmlns="some:ns"/>
        </subcontainer2>
    </x>
<child xmlns="some:ns2"/></container>
--- After import into HTML ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><p>foo<container xmlns="some:ns" xmlns:bar="another:ns"/></p></body></html>
<html><head></head><body><p>foo<container xmlns="some:ns" xmlns:bar="another:ns"></container></p></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_ns_imported_05.phpt`: Dom\HTMLDocument serialization with an imported namespace node 05
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createElementNS"#]
fn test_htmldocument_serialize_ns_imported_05() {
    if skip_without_xml_native("test_htmldocument_serialize_ns_imported_05") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$xml = Dom\XMLDocument::createFromFile(__DIR__.'/sample.xml');
$xml->documentElement->appendChild($xml->createElementNS('some:ns2', 'child'));
echo $xml->saveXml(), "\n";

echo "--- After adoption into HTML ---\n";

$html = Dom\HTMLDocument::createFromString('<p>foo</p>', LIBXML_NOERROR);

$p = $html->documentElement->firstChild->nextSibling->firstChild;
$p->appendChild($html->adoptNode($xml->documentElement));

echo $html->saveXml(), "\n";
echo $html->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<container xmlns="some:ns" xmlns:bar="another:ns">
    <x>
        <subcontainer>
            <test xmlns="x:y"/>
            <child2/>
        </subcontainer>
        <subcontainer2>
            <foo xmlns="some:ns"/>
        </subcontainer2>
    </x>
<child xmlns="some:ns2"/></container>
--- After adoption into HTML ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><p>foo<container xmlns="some:ns" xmlns:bar="another:ns">
    <x>
        <subcontainer>
            <test xmlns="x:y"/>
            <child2/>
        </subcontainer>
        <subcontainer2>
            <foo xmlns="some:ns"/>
        </subcontainer2>
    </x>
<child xmlns="some:ns2"/></container></p></body></html>
<html><head></head><body><p>foo<container xmlns="some:ns" xmlns:bar="another:ns">
    <x>
        <subcontainer>
            <test xmlns="x:y"></test>
            <child2></child2>
        </subcontainer>
        <subcontainer2>
            <foo xmlns="some:ns"></foo>
        </subcontainer2>
    </x>
<child></child></container></p></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_ns_imported_06.phpt`: Dom\HTMLDocument serialization with an imported namespace node 06
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createElementNS"#]
fn test_htmldocument_serialize_ns_imported_06() {
    if skip_without_xml_native("test_htmldocument_serialize_ns_imported_06") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$xml = Dom\XMLDocument::createFromFile(__DIR__.'/sample.xml');
$xml->documentElement->firstElementChild->appendChild($xml->createElementNS('some:ns2', 'child'));
echo $xml->saveXml(), "\n";

echo "--- After clone + import into HTML ---\n";

$html = Dom\HTMLDocument::createFromString('<p>foo</p>', LIBXML_NOERROR);

$p = $html->documentElement->firstElementChild->nextElementSibling->firstElementChild;
$p->appendChild($html->adoptNode($xml->documentElement->firstElementChild->cloneNode(true)));

echo $html->saveXml(), "\n";
echo $html->saveHtml(), "\n";

"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<container xmlns="some:ns" xmlns:bar="another:ns">
    <x>
        <subcontainer>
            <test xmlns="x:y"/>
            <child2/>
        </subcontainer>
        <subcontainer2>
            <foo xmlns="some:ns"/>
        </subcontainer2>
    <child xmlns="some:ns2"/></x>
</container>
--- After clone + import into HTML ---
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head></head><body><p>foo<x xmlns="some:ns">
        <subcontainer>
            <test xmlns="x:y"/>
            <child2/>
        </subcontainer>
        <subcontainer2>
            <foo xmlns="some:ns"/>
        </subcontainer2>
    <child xmlns="some:ns2"/></x></p></body></html>
<html><head></head><body><p>foo<x>
        <subcontainer>
            <test xmlns="x:y"></test>
            <child2></child2>
        </subcontainer>
        <subcontainer2>
            <foo xmlns="some:ns"></foo>
        </subcontainer2>
    <child></child></x></p></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_processing_instruction.phpt`: Dom\HTMLDocument serialization of processing instruction
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_serialize_processing_instruction() {
    if skip_without_xml_native("test_htmldocument_serialize_processing_instruction") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
// Note: you can put > in a processing instruction element but that breaks (as expected)
$dom->appendChild($dom->createProcessingInstruction("target", "foobaré\"&<\xc2\xa0"));
echo $dom->saveHtml();

"#);
    assert_exact(&out, r#"<?target foobaré"&< >"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_roots.phpt`: Dom\HTMLDocument serialization of different roots
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::createEmpty"#]
fn test_htmldocument_serialize_roots() {
    if skip_without_xml_native("test_htmldocument_serialize_roots") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

// Note: can't create CData in an HTML document.
$dom = Dom\XMLDocument::createEmpty();
$cdata = $dom->createCDATASection("cdata");

$dom = Dom\HTMLDocument::createEmpty();
$container = $dom->appendChild($dom->createElement("container"));
$comment = $container->appendChild($dom->createComment("comment"));
$cdata = $container->appendChild($dom->importNode($cdata));
$emptyElement = $container->appendChild($dom->createElement("empty"));
$text = $container->appendChild($dom->createTextNode("text"));
$pi = $container->appendChild($dom->createProcessingInstruction("target", "data"));
$fragment = $dom->createDocumentFragment();

var_dump($dom->saveHtml($comment));
var_dump($dom->saveHtml($cdata));
var_dump($dom->saveHtml($emptyElement));
var_dump($dom->saveHtml($text));
var_dump($dom->saveHtml($pi));
var_dump($dom->saveHtml($fragment));

"#);
    assert_exact(&out, r#"string(14) "<!--comment-->"
string(5) "cdata"
string(15) "<empty></empty>"
string(4) "text"
string(14) "<?target data>"
string(0) """#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_text_01.phpt`: Dom\HTMLDocument serialization escape text 01
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_serialize_text_01() {
    if skip_without_xml_native("test_htmldocument_serialize_text_01") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString("<p></p>", LIBXML_NOERROR);
$p = $dom->documentElement->firstChild->nextSibling->firstChild;
$p->textContent = "this is <some> &text! \"\"";
echo $dom->saveHtml();

"#);
    assert_exact(&out, r#"<html><head></head><body><p>this is &lt;some&gt; &amp;text! ""</p></body></html>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_text_02.phpt`: Dom\HTMLDocument serialization escape text 02 - special tags in html namespace
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_serialize_text_02() {
    if skip_without_xml_native("test_htmldocument_serialize_text_02") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$body = $dom->appendChild($dom->createElement("body"));
foreach (["style", "script", "xmp", "iframe", "noembed", "noframes", "plaintext", "noscript"] as $tag) {
    $tag = $body->appendChild($dom->createElement($tag));
    $tag->textContent = "&\"<>\xc2\xa0 foobar";
    $body->append("\n");
}
echo $dom->saveHtml();

"#);
    assert_exact(&out, r#"<body><style>&"<>  foobar</style>
<script>&"<>  foobar</script>
<xmp>&"<>  foobar</xmp>
<iframe>&"<>  foobar</iframe>
<noembed>&"<>  foobar</noembed>
<noframes>&"<>  foobar</noframes>
<plaintext>&"<>  foobar</plaintext>
<noscript>&amp;"&lt;&gt;&nbsp; foobar</noscript>
</body>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_text_03.phpt`: Dom\HTMLDocument serialization escape text 03 - special tags in namespace should encode content
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_serialize_text_03() {
    if skip_without_xml_native("test_htmldocument_serialize_text_03") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$body = $dom->appendChild($dom->createElement("body"));
foreach (["style", "script", "xmp", "iframe", "noembed", "noframes", "plaintext", "noscript"] as $tag) {
    $tag = $body->appendChild($dom->createElementNS("some:ns", $tag));
    $tag->textContent = "&\"<>\xc2\xa0 foobar";
    $body->append("\n");
}
echo $dom->saveHtml();

"#);
    assert_exact(&out, r#"<body><style>&amp;"&lt;&gt;&nbsp; foobar</style>
<script>&amp;"&lt;&gt;&nbsp; foobar</script>
<xmp>&amp;"&lt;&gt;&nbsp; foobar</xmp>
<iframe>&amp;"&lt;&gt;&nbsp; foobar</iframe>
<noembed>&amp;"&lt;&gt;&nbsp; foobar</noembed>
<noframes>&amp;"&lt;&gt;&nbsp; foobar</noframes>
<plaintext>&amp;"&lt;&gt;&nbsp; foobar</plaintext>
<noscript>&amp;"&lt;&gt;&nbsp; foobar</noscript>
</body>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/HTMLDocument_serialize_void_elements.phpt`: Dom\HTMLDocument serialization of void elements
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_htmldocument_serialize_void_elements() {
    if skip_without_xml_native("test_htmldocument_serialize_void_elements") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createEmpty();
$container = $dom->createElement("container");
$dom->append($container);

$tags = [
    "area",
    "base",
    "br",
    "col",
    "embed",
    "hr",
    "img",
    "input",
    "link",
    "meta",
    "source",
    "track",
    "wbr",
    "basefont",
    "bgsound",
    "frame",
    "keygen",
    "param",
];

foreach ($tags as $tag) {
    $element = $container->appendChild($dom->createElement($tag));
    $element->appendChild($dom->createElement("inner"));
    $element->after("\n");
    echo "$tag: ";
    var_dump($dom->saveHtml($element));

    $element = $container->appendChild($dom->createElementNS("http://php.net/foo", "x:$tag"));
    $element->appendChild($dom->createElement("inner"));
    $element->after("\n");
}

echo $dom->saveHtml();

"#);
    assert_exact(&out, r#"area: string(6) "<area>"
base: string(6) "<base>"
br: string(4) "<br>"
col: string(5) "<col>"
embed: string(7) "<embed>"
hr: string(4) "<hr>"
img: string(5) "<img>"
input: string(7) "<input>"
link: string(6) "<link>"
meta: string(6) "<meta>"
source: string(8) "<source>"
track: string(7) "<track>"
wbr: string(5) "<wbr>"
basefont: string(10) "<basefont>"
bgsound: string(9) "<bgsound>"
frame: string(7) "<frame>"
keygen: string(8) "<keygen>"
param: string(7) "<param>"
<container><area>
<x:area><inner></inner></x:area>
<base>
<x:base><inner></inner></x:base>
<br>
<x:br><inner></inner></x:br>
<col>
<x:col><inner></inner></x:col>
<embed>
<x:embed><inner></inner></x:embed>
<hr>
<x:hr><inner></inner></x:hr>
<img>
<x:img><inner></inner></x:img>
<input>
<x:input><inner></inner></x:input>
<link>
<x:link><inner></inner></x:link>
<meta>
<x:meta><inner></inner></x:meta>
<source>
<x:source><inner></inner></x:source>
<track>
<x:track><inner></inner></x:track>
<wbr>
<x:wbr><inner></inner></x:wbr>
<basefont>
<x:basefont><inner></inner></x:basefont>
<bgsound>
<x:bgsound><inner></inner></x:bgsound>
<frame>
<x:frame><inner></inner></x:frame>
<keygen>
<x:keygen><inner></inner></x:keygen>
<param>
<x:param><inner></inner></x:param>
</container>"#);
}

/// php-src `ext/dom/tests/modern/html/serializer/legacy_null_content.phpt`: Serialize legacy nodes with NULL content
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_legacy_null_content() {
    if skip_without_xml_native("test_legacy_null_content") {
        return;
    }
    let out = run_case(r#"modern/html/serializer"#, &[], r#"<?php
$dom = Dom\HTMLDocument::createEmpty();
$root = $dom->appendChild($dom->createElement('html'));

$root->appendChild($dom->importLegacyNode(new DOMText));
$root->appendChild($dom->importLegacyNode(new DOMComment));
$root->appendChild($dom->importLegacyNode(new DOMProcessingInstruction('target')));
$root->appendChild($dom->importLegacyNode(new DOMCdataSection('')));

echo $dom->saveHTML(), "\n";
echo $dom->documentElement->innerHTML, "\n";
"#);
    assert_exact(&out, r#"<html><!----><?target ></html>
<!----><?target >"#);
}
