//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `modern/css/selectors` folder, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/modern/css_selectors/attribute.phpt`: CSS Selectors - Attribute
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_attribute() {
    if skip_without_xml_native("test_attribute") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container>
    <a title="http://example.com" lang="en-us"/>
    <a title="http://example.be"/>
    <a title="ftp://example.be" lang="nl-be" tokens="abc def ghi"/>
    <a title="ftp://example.nl" lang="nl-be"/>
</container>
XML);

echo "=== Case sensitive ===\n";

test_helper($dom, 'a[title]');
test_helper($dom, 'a[title="http://example.com"]');
test_helper($dom, 'a[title="http://example."]');
test_helper($dom, 'a[title*="example"]');
test_helper($dom, 'a[title*=""]');
test_helper($dom, 'a[title^="HTTP"]');
test_helper($dom, 'a[title^="http"]');
test_helper($dom, 'a[title^="http"][title$=".be"]');
test_helper($dom, 'a[title$=".com"]');
test_helper($dom, 'a[title$=".foo"]');
test_helper($dom, 'a[lang|="nl"]');
test_helper($dom, 'a[lang|="nl-be"]');
test_helper($dom, 'a[tokens~="def"]');
test_helper($dom, 'a[tokens~="de"]');
test_helper($dom, 'a[tokens~="def ghi"]');

echo "=== Case insensitive ===\n";

test_helper($dom, 'a[title]');
test_helper($dom, 'a[title="http://example.COM" i]');
test_helper($dom, 'a[title="http://EXAMPLE." i]');
test_helper($dom, 'a[title*="ExAmPlE" i]');
test_helper($dom, 'a[title^="HTTP" i]');
test_helper($dom, 'a[title^="HTTP" i][title$=".be"]');
test_helper($dom, 'a[title$=".COM" i]');
test_helper($dom, 'a[lang|="NL" i]');
test_helper($dom, 'a[lang|="NL-BE" i]');
test_helper($dom, 'a[tokens~="DE" i]');
test_helper($dom, 'a[tokens~="DEF" i]');
test_helper($dom, 'a[tokens~="DEF GHI" i]');

"#);
    assert_exact(&out, r#"=== Case sensitive ===
--- Selector: a[title] ---
<a title="http://example.com" lang="en-us"/>
<a title="http://example.be"/>
<a title="ftp://example.be" lang="nl-be" tokens="abc def ghi"/>
<a title="ftp://example.nl" lang="nl-be"/>
--- Selector: a[title="http://example.com"] ---
<a title="http://example.com" lang="en-us"/>
--- Selector: a[title="http://example."] ---
--- Selector: a[title*="example"] ---
<a title="http://example.com" lang="en-us"/>
<a title="http://example.be"/>
<a title="ftp://example.be" lang="nl-be" tokens="abc def ghi"/>
<a title="ftp://example.nl" lang="nl-be"/>
--- Selector: a[title*=""] ---
--- Selector: a[title^="HTTP"] ---
--- Selector: a[title^="http"] ---
<a title="http://example.com" lang="en-us"/>
<a title="http://example.be"/>
--- Selector: a[title^="http"][title$=".be"] ---
<a title="http://example.be"/>
--- Selector: a[title$=".com"] ---
<a title="http://example.com" lang="en-us"/>
--- Selector: a[title$=".foo"] ---
--- Selector: a[lang|="nl"] ---
<a title="ftp://example.be" lang="nl-be" tokens="abc def ghi"/>
<a title="ftp://example.nl" lang="nl-be"/>
--- Selector: a[lang|="nl-be"] ---
<a title="ftp://example.be" lang="nl-be" tokens="abc def ghi"/>
<a title="ftp://example.nl" lang="nl-be"/>
--- Selector: a[tokens~="def"] ---
<a title="ftp://example.be" lang="nl-be" tokens="abc def ghi"/>
--- Selector: a[tokens~="de"] ---
--- Selector: a[tokens~="def ghi"] ---
=== Case insensitive ===
--- Selector: a[title] ---
<a title="http://example.com" lang="en-us"/>
<a title="http://example.be"/>
<a title="ftp://example.be" lang="nl-be" tokens="abc def ghi"/>
<a title="ftp://example.nl" lang="nl-be"/>
--- Selector: a[title="http://example.COM" i] ---
<a title="http://example.com" lang="en-us"/>
--- Selector: a[title="http://EXAMPLE." i] ---
--- Selector: a[title*="ExAmPlE" i] ---
<a title="http://example.com" lang="en-us"/>
<a title="http://example.be"/>
<a title="ftp://example.be" lang="nl-be" tokens="abc def ghi"/>
<a title="ftp://example.nl" lang="nl-be"/>
--- Selector: a[title^="HTTP" i] ---
<a title="http://example.com" lang="en-us"/>
<a title="http://example.be"/>
--- Selector: a[title^="HTTP" i][title$=".be"] ---
<a title="http://example.be"/>
--- Selector: a[title$=".COM" i] ---
<a title="http://example.com" lang="en-us"/>
--- Selector: a[lang|="NL" i] ---
<a title="ftp://example.be" lang="nl-be" tokens="abc def ghi"/>
<a title="ftp://example.nl" lang="nl-be"/>
--- Selector: a[lang|="NL-BE" i] ---
<a title="ftp://example.be" lang="nl-be" tokens="abc def ghi"/>
<a title="ftp://example.nl" lang="nl-be"/>
--- Selector: a[tokens~="DE" i] ---
--- Selector: a[tokens~="DEF" i] ---
<a title="ftp://example.be" lang="nl-be" tokens="abc def ghi"/>
--- Selector: a[tokens~="DEF GHI" i] ---"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/closest.phpt`: Test DOM\Element::closest() method: legit cases
#[test]
#[ignore = r#"DOM port pending: Nullsafe method call requires an object or null"#]
fn test_closest() {
    if skip_without_xml_native("test_closest") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

$xml = <<<XML
<root>
  <a/>
  <div class="foo" xml:id="div1">
    <div xml:id="div2">
      <div class="bar" xml:id="div3"/>
    </div>
  </div>
</root>
XML;

$dom = DOM\XMLDocument::createFromString($xml);

function test($el, $selector) {
  echo "--- Selector: $selector ---\n";
  var_dump($el->closest($selector)?->getAttribute('xml:id'));
}

test($dom->getElementById('div3'), 'div');
test($dom->getElementById('div3'), '[class="foo"]');
test($dom->getElementById('div3'), ':not(root)');
test($dom->getElementById('div3'), ':not(div)');
test($dom->getElementById('div3'), 'a');
test($dom->getElementById('div3'), 'root :not(div[class])');
test($dom->getElementById('div3'), 'root > :not(div[class])');

"#);
    assert_exact(&out, r#"--- Selector: div ---
string(4) "div3"
--- Selector: [class="foo"] ---
string(4) "div1"
--- Selector: :not(root) ---
string(4) "div3"
--- Selector: :not(div) ---
NULL
--- Selector: a ---
NULL
--- Selector: root :not(div[class]) ---
string(4) "div2"
--- Selector: root > :not(div[class]) ---
NULL"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/closest_invalid_selector.phpt`: Test DOM\Element::closest() method: invalid selector
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_closest_invalid_selector() {
    if skip_without_xml_native("test_closest_invalid_selector") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString("<root/>");

try {
  var_dump($dom->documentElement->closest('@invalid'));
} catch (DOMException $e) {
  echo $e->getMessage();
}

"#);
    assert_exact(&out, r#"Invalid selector (Selectors. Unexpected token: @invalid)"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/combinators.phpt`: CSS Selectors - Combinators
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_combinators() {
    if skip_without_xml_native("test_combinators") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE html>
<html>
    <body>
        <p>First p</p>
        <p>Second p</p>
        <img src="1.png">
        <div class="">
            <p>Third p</p>
            <img src="2.png">
            <img src="3.png">
            <div class="foo bar baz">
                <p>Fourth p</p>
            </div>
        </div>
        <article title="foo" class="bar">
            <p class="bar">Fifth p</p>
        </article>
        <table border="1">
            <colgroup>
                <col class="selected">
            </colgroup>
            <tbody>
                <tr>
                    <td>A</td>
                    <td>B</td>
                    <td>C</td>
                </tr>
            </tbody>
        </table>
    </body>
</html>
HTML);

test_helper($dom, 'nonsense');
test_helper($dom, 'p');
test_helper($dom, 'p, img');
test_helper($dom, 'body p');
test_helper($dom, 'body div p');
test_helper($dom, 'div > *');
test_helper($dom, 'div > p');
test_helper($dom, 'div > p + img');
test_helper($dom, 'div > p ~ img');
test_helper($dom, 'body > img');
test_helper($dom, 'div.bar.baz > p');
test_helper($dom, 'article[title].bar p');

try {
    test_helper($dom, 'col.selected||td');
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"--- Selector: nonsense ---
--- Selector: p ---
<p xmlns="http://www.w3.org/1999/xhtml">First p</p>
<p xmlns="http://www.w3.org/1999/xhtml">Second p</p>
<p xmlns="http://www.w3.org/1999/xhtml">Third p</p>
<p xmlns="http://www.w3.org/1999/xhtml">Fourth p</p>
<p xmlns="http://www.w3.org/1999/xhtml" class="bar">Fifth p</p>
--- Selector: p, img ---
<p xmlns="http://www.w3.org/1999/xhtml">First p</p>
<p xmlns="http://www.w3.org/1999/xhtml">Second p</p>
<img xmlns="http://www.w3.org/1999/xhtml" src="1.png" />
<p xmlns="http://www.w3.org/1999/xhtml">Third p</p>
<img xmlns="http://www.w3.org/1999/xhtml" src="2.png" />
<img xmlns="http://www.w3.org/1999/xhtml" src="3.png" />
<p xmlns="http://www.w3.org/1999/xhtml">Fourth p</p>
<p xmlns="http://www.w3.org/1999/xhtml" class="bar">Fifth p</p>
--- Selector: body p ---
<p xmlns="http://www.w3.org/1999/xhtml">First p</p>
<p xmlns="http://www.w3.org/1999/xhtml">Second p</p>
<p xmlns="http://www.w3.org/1999/xhtml">Third p</p>
<p xmlns="http://www.w3.org/1999/xhtml">Fourth p</p>
<p xmlns="http://www.w3.org/1999/xhtml" class="bar">Fifth p</p>
--- Selector: body div p ---
<p xmlns="http://www.w3.org/1999/xhtml">Third p</p>
<p xmlns="http://www.w3.org/1999/xhtml">Fourth p</p>
--- Selector: div > * ---
<p xmlns="http://www.w3.org/1999/xhtml">Third p</p>
<img xmlns="http://www.w3.org/1999/xhtml" src="2.png" />
<img xmlns="http://www.w3.org/1999/xhtml" src="3.png" />
<div xmlns="http://www.w3.org/1999/xhtml" class="foo bar baz">
                <p>Fourth p</p>
            </div>
<p xmlns="http://www.w3.org/1999/xhtml">Fourth p</p>
--- Selector: div > p ---
<p xmlns="http://www.w3.org/1999/xhtml">Third p</p>
<p xmlns="http://www.w3.org/1999/xhtml">Fourth p</p>
--- Selector: div > p + img ---
<img xmlns="http://www.w3.org/1999/xhtml" src="2.png" />
--- Selector: div > p ~ img ---
<img xmlns="http://www.w3.org/1999/xhtml" src="2.png" />
<img xmlns="http://www.w3.org/1999/xhtml" src="3.png" />
--- Selector: body > img ---
<img xmlns="http://www.w3.org/1999/xhtml" src="1.png" />
--- Selector: div.bar.baz > p ---
<p xmlns="http://www.w3.org/1999/xhtml">Fourth p</p>
--- Selector: article[title].bar p ---
<p xmlns="http://www.w3.org/1999/xhtml" class="bar">Fifth p</p>
--- Selector: col.selected||td ---
Dom\Document::querySelectorAll(): Argument #1 ($selectors) contains an unsupported selector"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/entities.phpt`: CSS Selectors - Handling entities
#[test]
#[ignore = r#"DOM port pending: Undefined method: Dom\XMLDocument::querySelector"#]
fn test_entities() {
    if skip_without_xml_native("test_entities") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

$dom = Dom\XMLDocument::createFromString(<<<XML
<!DOCTYPE root [
    <!ENTITY ent "box">
]>
<root>
    <input type="check&ent;" checked="" class="a&ent;a" id="a&ent;a" xmlns="http://www.w3.org/1999/xhtml"/>
</root>
XML);
var_dump($dom->querySelector('input:checked')->nodeName);
var_dump($dom->querySelector('input[type$="ox"]')->nodeName);
var_dump($dom->querySelector('input.aboxa')->nodeName);
var_dump($dom->querySelector('input#aboxa')->nodeName);

"#);
    assert_exact(&out, r#"string(5) "input"
string(5) "input"
string(5) "input"
string(5) "input""#);
}

/// php-src `ext/dom/tests/modern/css_selectors/gh17802.phpt`: GH-17802 (\Dom\HTMLDocument querySelector attribute name is case sensitive in HTML)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_gh17802() {
    if skip_without_xml_native("test_gh17802") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

$text = <<<TEXT
<html>
<head>
<meta charset="Windows-1252">
</head>
<body>
</body>
</html>
TEXT;

$dom = \Dom\HTMLDocument::createFromString($text, options: LIBXML_NOERROR);
$meta2 = $dom->head->appendChild($dom->createElementNS('urn:x', 'meta'));
$meta2->setAttribute('charset', 'x');
echo $dom->saveHtml(), "\n";

echo "--- charseT ---\n";

foreach ($dom->querySelectorAll('meta[charseT]') as $entry) {
    var_dump($dom->saveHtml($entry));
}

echo "--- charset ---\n";

foreach ($dom->querySelectorAll('meta[charset]') as $entry) {
    var_dump($dom->saveHtml($entry));
}

echo "--- charseT and lowercase value ---\n";

foreach ($dom->querySelectorAll('meta[charseT="windows-1252"]') as $entry) {
    var_dump($dom->saveHtml($entry));
}

echo "--- charset and lowercase value ---\n";

foreach ($dom->querySelectorAll('meta[charset="windows-1252"]') as $entry) {
    var_dump($dom->saveHtml($entry));
}

"#);
    assert_exact(&out, r#"<html><head>
<meta charset="Windows-1252">
<meta charset="x"></meta></head>
<body>

</body></html>
--- charseT ---
string(29) "<meta charset="Windows-1252">"
--- charset ---
string(29) "<meta charset="Windows-1252">"
string(25) "<meta charset="x"></meta>"
--- charseT and lowercase value ---
string(29) "<meta charset="Windows-1252">"
--- charset and lowercase value ---
string(29) "<meta charset="Windows-1252">""#);
}

/// php-src `ext/dom/tests/modern/css_selectors/gh18877.phpt`: GH-18877 (\Dom\HTMLDocument querySelectorAll selecting only the first when using ~ and :has)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_gh18877() {
    if skip_without_xml_native("test_gh18877") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

$text = <<<TEXT
<html>
<head>
</head>
<body>
    <div><div></div></div>
    <span>1</span>
    <span>2</span>
    <span>3</span>
</body>
</html>
TEXT;

$dom = \Dom\HTMLDocument::createFromString($text, options: LIBXML_NOERROR);
foreach ($dom->querySelectorAll('div:has(div) ~ *') as $node) {
    var_dump($node->textContent);
}

"#);
    assert_exact(&out, r#"string(1) "1"
string(1) "2"
string(1) "3""#);
}

/// php-src `ext/dom/tests/modern/css_selectors/gh20395.phpt`: GH-20395 (\Dom\ParentNode::querySelector and \Dom\ParentNode::querySelectorAll requires elements in $selectors to be lowercase)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HtmlDocument"#]
fn test_gh20395() {
    if skip_without_xml_native("test_gh20395") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

$html   = '<!doctype html><html><head></head><body></body></html>';
$dom    = Dom\HtmlDocument::createFromString($html);
var_dump(is_null($dom->querySelector('html')));
var_dump(is_null($dom->querySelector('Html')));
var_dump(is_null($dom->querySelector('HTML')));

$dom->body->appendChild($dom->createElement('div'));
$dom->body->appendChild($dom->createElementNS('http://www.w3.org/1999/xhtml', 'Div'));

foreach ($dom->querySelectorAll('div') as $div) {
	var_dump($div->localName);
}

foreach ($dom->querySelectorAll('Div') as $div) {
	var_dump($div->localName);
}

"#);
    assert_exact(&out, r#"bool(false)
bool(false)
bool(false)
string(3) "div"
string(3) "div""#);
}

/// php-src `ext/dom/tests/modern/css_selectors/id.phpt`: CSS Selectors - ID
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_id() {
    if skip_without_xml_native("test_id") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container>
    <test id="test1"/>
    <test/>
    <test id="test2"/>
    <test id="test3"/>
    <test xmlns:x="urn:x" x:id="test4"/>
</container>
XML);

test_helper($dom, '#test');
test_helper($dom, '#test1');
test_helper($dom, '#test2');
test_helper($dom, '#test3');
test_helper($dom, '#test4');

"#);
    assert_exact(&out, r#"--- Selector: #test ---
--- Selector: #test1 ---
<test id="test1"/>
--- Selector: #test2 ---
<test id="test2"/>
--- Selector: #test3 ---
<test id="test3"/>
--- Selector: #test4 ---"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/lexbor274.phpt`: Lexbor #274
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_lexbor274() {
    if skip_without_xml_native("test_lexbor274") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

$dom = Dom\HTMLDocument::createFromString('<html><body><div></div>\n</body></html>', LIBXML_NOERROR);
var_dump($dom->querySelector('a:has(+ b)'));

"#);
    assert_exact(&out, r#"NULL"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/matches_invalid_selector.phpt`: Test DOM\Element::matches() method: invalid selector
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_matches_invalid_selector() {
    if skip_without_xml_native("test_matches_invalid_selector") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

$dom = DOM\XMLDocument::createFromString("<root/>");

try {
  var_dump($dom->documentElement->matches('@invalid'));
} catch (DOMException $e) {
  echo $e->getMessage();
}

"#);
    assert_exact(&out, r#"Invalid selector (Selectors. Unexpected token: @invalid)"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/namespaces.phpt`: CSS Selectors - Namespaces
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_namespaces() {
    if skip_without_xml_native("test_namespaces") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<root>
    <container align="left"/>
    <only>
        <a xmlns="urn:a"/>
        <a xmlns="urn:a"/>
        <a xmlns="urn:b"/>
        <a xmlns=""/>
        <a/>
    </only>
</root>
XML);

$container = $dom->documentElement->firstElementChild;
$container->setAttribute("foo:bar", "baz");
$container->setAttributeNS("urn:a", "a:bar", "baz");

test_helper($dom, 'container[align]');
test_helper($dom, 'container[foo\\:bar]');
test_helper($dom, 'container[a\\:bar]');
test_helper($dom, 'container[bar]');

test_helper($dom, 'a:first-of-type');
test_helper($dom, 'a:last-of-type');

test_failure($dom, 'container[* | bar]');

"#);
    assert_exact(&out, r#"--- Selector: container[align] ---
<container align="left" foo:bar="baz" xmlns:a="urn:a" a:bar="baz"/>
--- Selector: container[foo\:bar] ---
<container align="left" foo:bar="baz" xmlns:a="urn:a" a:bar="baz"/>
--- Selector: container[a\:bar] ---
--- Selector: container[bar] ---
--- Selector: a:first-of-type ---
<a xmlns="urn:a"/>
<a xmlns="urn:b"/>
<a xmlns=""/>
--- Selector: a:last-of-type ---
<a xmlns="urn:a"/>
<a xmlns="urn:b"/>
<a/>
--- Selector: container[* | bar] ---
Code 12 Invalid selector (Selectors. Unexpected token: *)"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_blank.phpt`: CSS Selectors - Pseudo classes: blank
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_blank() {
    if skip_without_xml_native("test_pseudo_classes_blank") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container/>
XML);

try {
    test_helper($dom, ':blank');
} catch (DOMException $e) {
    echo $e->getMessage(), "\n";
}

"#);
    assert_exact(&out, r#"--- Selector: :blank ---
:blank selector is not implemented because CSSWG has not yet decided its semantics (https://github.com/w3c/csswg-drafts/issues/1967)"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_checked.phpt`: CSS Selectors - Pseudo classes: checked
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_checked() {
    if skip_without_xml_native("test_pseudo_classes_checked") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<html xmlns="http://www.w3.org/1999/xhtml">
    <input type="checkbox" checked="checked" />
    <input type="radio" checked="checked" />
    <option selected="" />
    <option xmlns="" selected="" />
    <input/>
</html>
XML);

test_helper($dom, ':checked');

"#);
    assert_exact(&out, r#"--- Selector: :checked ---
<input xmlns="http://www.w3.org/1999/xhtml" type="checkbox" checked="checked" />
<input xmlns="http://www.w3.org/1999/xhtml" type="radio" checked="checked" />
<option xmlns="http://www.w3.org/1999/xhtml" selected=""></option>"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_current.phpt`: CSS Selectors - Pseudo classes: current
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_current() {
    if skip_without_xml_native("test_pseudo_classes_current") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<html xmlns="http://www.w3.org/1999/xhtml">
    <div/>
</html>
XML);

test_helper($dom, ':current(div)');

"#);
    assert_exact(&out, r#"--- Selector: :current(div) ---
<div xmlns="http://www.w3.org/1999/xhtml"></div>"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_dir.phpt`: CSS Selectors - Pseudo classes: dir
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_dir() {
    if skip_without_xml_native("test_pseudo_classes_dir") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container dir="rtl">
    <p>1</p>
    <html xmlns="http://www.w3.org/1999/xhtml" dir="ltr">
        <p>2</p>
    </html>
</container>
XML);

test_failure($dom, ':dir(rtl)', true);
test_failure($dom, ':dir(ltr)', true);

"#);
    assert_exact(&out, r#"--- Selector: :dir(rtl) ---
Code 12 Invalid selector (Selectors. Not supported: dir)
--- Selector: :dir(ltr) ---
Code 12 Invalid selector (Selectors. Not supported: dir)"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_disabled_enabled.phpt`: CSS Selectors - Pseudo classes: disabled/enabled
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_disabled_enabled() {
    if skip_without_xml_native("test_pseudo_classes_disabled_enabled") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<html xmlns="http://www.w3.org/1999/xhtml">
    <button/>
    <button xmlns="" disabled="disabled"/>
    <button disabled="disabled"/>
    <input disabled="disabled"/>
    <select disabled="disabled"/>
    <textarea disabled="disabled"/>
    <optgroup disabled="disabled"/>

    <!-- fieldset rules are complicated -->
    <fieldset disabled="disabled"/>
    <fieldset disabled="disabled">
        <fieldset id="1"/>
    </fieldset>
    <fieldset disabled="disabled">
        <!-- foo -->
        <legend>
            <div>
                <fieldset id="2"/>
            </div>
        </legend>
        <div>
            <fieldset id="3"/>
        </div>
    </fieldset>
</html>
XML);

test_helper($dom, '*:disabled');
test_helper($dom, '*:enabled');

"#);
    assert_exact(&out, r#"--- Selector: *:disabled ---
<button xmlns="http://www.w3.org/1999/xhtml" disabled="disabled"></button>
<input xmlns="http://www.w3.org/1999/xhtml" disabled="disabled" />
<select xmlns="http://www.w3.org/1999/xhtml" disabled="disabled"></select>
<textarea xmlns="http://www.w3.org/1999/xhtml" disabled="disabled"></textarea>
<optgroup xmlns="http://www.w3.org/1999/xhtml" disabled="disabled"></optgroup>
<fieldset xmlns="http://www.w3.org/1999/xhtml" disabled="disabled"></fieldset>
<fieldset xmlns="http://www.w3.org/1999/xhtml" disabled="disabled">
        <fieldset id="1"></fieldset>
    </fieldset>
<fieldset xmlns="http://www.w3.org/1999/xhtml" disabled="disabled">
        <!-- foo -->
        <legend>
            <div>
                <fieldset id="2"></fieldset>
            </div>
        </legend>
        <div>
            <fieldset id="3"></fieldset>
        </div>
    </fieldset>
<fieldset xmlns="http://www.w3.org/1999/xhtml" id="3"></fieldset>
--- Selector: *:enabled ---
<html xmlns="http://www.w3.org/1999/xhtml">
    <button></button>
    <button xmlns="" disabled="disabled"/>
    <button disabled="disabled"></button>
    <input disabled="disabled" />
    <select disabled="disabled"></select>
    <textarea disabled="disabled"></textarea>
    <optgroup disabled="disabled"></optgroup>

    <!-- fieldset rules are complicated -->
    <fieldset disabled="disabled"></fieldset>
    <fieldset disabled="disabled">
        <fieldset id="1"></fieldset>
    </fieldset>
    <fieldset disabled="disabled">
        <!-- foo -->
        <legend>
            <div>
                <fieldset id="2"></fieldset>
            </div>
        </legend>
        <div>
            <fieldset id="3"></fieldset>
        </div>
    </fieldset>
</html>
<button xmlns="http://www.w3.org/1999/xhtml"></button>
<button xmlns="" disabled="disabled"/>
<fieldset xmlns="http://www.w3.org/1999/xhtml" id="1"></fieldset>
<legend xmlns="http://www.w3.org/1999/xhtml">
            <div>
                <fieldset id="2"></fieldset>
            </div>
        </legend>
<div xmlns="http://www.w3.org/1999/xhtml">
                <fieldset id="2"></fieldset>
            </div>
<fieldset xmlns="http://www.w3.org/1999/xhtml" id="2"></fieldset>
<div xmlns="http://www.w3.org/1999/xhtml">
            <fieldset id="3"></fieldset>
        </div>"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_empty.phpt`: CSS Selectors - Pseudo classes: empty
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_empty() {
    if skip_without_xml_native("test_pseudo_classes_empty") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container>
    <div class="empty">
        <p>Element with no content:</p>
        <div></div>

        <p>Element with comment:</p>
        <div><!-- Simple Comment --></div>

        <p>Element with PI:</p>
        <div><?foo?></div>

        <p>Element with CDATA:</p>
        <div><![CDATA[foo]]></div>

        <p>Element with nested empty element:</p>
        <div><p></p></div>
    </div>
</container>
XML);

test_helper($dom, '.empty > div:empty');

"#);
    assert_exact(&out, r#"--- Selector: .empty > div:empty ---
<div/>
<div><!-- Simple Comment --></div>
<div><?foo ?></div>"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_first_last_child.phpt`: CSS Selectors - Pseudo classes: first/last child
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_first_last_child() {
    if skip_without_xml_native("test_pseudo_classes_first_last_child") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container>
    <?foo?>
    <!--bar-->
    <first />
    &amp;
    <last/>
    <![CDATA[skip me]]>
</container>
XML);

test_helper($dom, 'container > *:first-child');
test_helper($dom, 'container > *:last-child');

"#);
    assert_exact(&out, r#"--- Selector: container > *:first-child ---
<first/>
--- Selector: container > *:last-child ---
<last/>"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_has.phpt`: CSS Selectors - Pseudo classes: has
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_has() {
    if skip_without_xml_native("test_pseudo_classes_has") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container>
    <div>
        <p class="foo"/>
    </div>
    <div>
        <p/>
    </div>
</container>
XML);

test_helper($dom, 'div:has(p.foo)');
test_helper($dom, 'div:has(:not(p.foo))');

"#);
    assert_exact(&out, r#"--- Selector: div:has(p.foo) ---
<div>
        <p class="foo"/>
    </div>
--- Selector: div:has(:not(p.foo)) ---
<div>
        <p/>
    </div>"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_is_where.phpt`: CSS Selectors - Pseudo classes: is/where
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_is_where() {
    if skip_without_xml_native("test_pseudo_classes_is_where") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container>
    <article>
        <p>1</p>
    </article>
    <main>
        <p>2</p>
    </main>
    <div>
        <p>3</p>
    </div>
</container>
XML);

test_helper($dom, ':is(article, main) p');
test_helper($dom, ':where(article, main) p');

"#);
    assert_exact(&out, r#"--- Selector: :is(article, main) p ---
<p>1</p>
<p>2</p>
--- Selector: :where(article, main) p ---
<p>1</p>
<p>2</p>"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_lang.phpt`: CSS Selectors - Pseudo classes: lang
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_lang() {
    if skip_without_xml_native("test_pseudo_classes_lang") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container lang="en">
    <p>1</p>
    <html xmlns="http://www.w3.org/1999/xhtml" lang="nl">
        <p>2</p>
    </html>
</container>
XML);

test_failure($dom, ':lang(en)', true);
test_failure($dom, ':lang(nl)', true);

"#);
    assert_exact(&out, r#"--- Selector: :lang(en) ---
Code 12 Invalid selector (Selectors. Not supported: lang)
--- Selector: :lang(nl) ---
Code 12 Invalid selector (Selectors. Not supported: lang)"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_links.phpt`: CSS Selectors - Pseudo classes: links
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_links() {
    if skip_without_xml_native("test_pseudo_classes_links") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container>
    <a href="http://example.com">Link</a>
    <a xmlns="http://www.w3.org/1999/xhtml" href="http://example.com">Link</a>
    <A xmlns="http://www.w3.org/1999/xhtml" href="http://example.com">Not actually a link</A>
    <area xmlns="http://www.w3.org/1999/xhtml" href="http://example.com">Link</area>
</container>
XML);

test_helper($dom, ':any-link');
test_helper($dom, ':link');
test_helper($dom, 'a:not(:any-link)');
test_helper($dom, ':not(:any-link)');

"#);
    assert_exact(&out, r#"--- Selector: :any-link ---
<a xmlns="http://www.w3.org/1999/xhtml" href="http://example.com">Link</a>
<area xmlns="http://www.w3.org/1999/xhtml" href="http://example.com">Link</area>
--- Selector: :link ---
<a xmlns="http://www.w3.org/1999/xhtml" href="http://example.com">Link</a>
<area xmlns="http://www.w3.org/1999/xhtml" href="http://example.com">Link</area>
--- Selector: a:not(:any-link) ---
<a href="http://example.com">Link</a>
--- Selector: :not(:any-link) ---
<container>
    <a href="http://example.com">Link</a>
    <a xmlns="http://www.w3.org/1999/xhtml" href="http://example.com">Link</a>
    <A xmlns="http://www.w3.org/1999/xhtml" href="http://example.com">Not actually a link</A>
    <area xmlns="http://www.w3.org/1999/xhtml" href="http://example.com">Link</area>
</container>
<a href="http://example.com">Link</a>
<A xmlns="http://www.w3.org/1999/xhtml" href="http://example.com">Not actually a link</A>"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_no_op.phpt`: CSS Selectors - Pseudo classes: no-ops
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_no_op() {
    if skip_without_xml_native("test_pseudo_classes_no_op") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r##"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<html xmlns="http://www.w3.org/1999/xhtml">
    <div/>
    <a href="#foo"/>
</html>
XML);

// These will always fail or return an empty list because they are display-specific or require user interaction
test_failure($dom, ':default');
test_failure($dom, ':focus');
test_failure($dom, ':focus-visible');
test_failure($dom, ':focus-within');
test_failure($dom, ':fullscreen');
test_failure($dom, ':hover');
test_failure($dom, ':in-range');
test_failure($dom, ':indeterminate');
test_failure($dom, ':invalid');
test_failure($dom, ':out-of-range');
test_failure($dom, ':past');
test_failure($dom, ':future');
test_failure($dom, ':scope');
test_failure($dom, ':target');
test_failure($dom, ':target-within');
test_failure($dom, ':user-invalid');
test_failure($dom, ':valid');
test_failure($dom, ':visited');
test_failure($dom, ':warning');
test_failure($dom, ':local-link');
test_failure($dom, ':active');

"##);
    assert_exact(&out, r#"--- Selector: :default ---
Code 12 Invalid selector (Selectors. Not supported: default)
--- Selector: :focus ---
int(0)
--- Selector: :focus-visible ---
Code 12 Invalid selector (Selectors. Not supported: focus-visible)
--- Selector: :focus-within ---
Code 12 Invalid selector (Selectors. Not supported: focus-within)
--- Selector: :fullscreen ---
Code 12 Invalid selector (Selectors. Not supported: fullscreen)
--- Selector: :hover ---
int(0)
--- Selector: :in-range ---
Code 12 Invalid selector (Selectors. Not supported: in-range)
--- Selector: :indeterminate ---
Code 12 Invalid selector (Selectors. Not supported: indeterminate)
--- Selector: :invalid ---
Code 12 Invalid selector (Selectors. Not supported: invalid)
--- Selector: :out-of-range ---
Code 12 Invalid selector (Selectors. Not supported: out-of-range)
--- Selector: :past ---
Code 12 Invalid selector (Selectors. Not supported: past)
--- Selector: :future ---
Code 12 Invalid selector (Selectors. Not supported: future)
--- Selector: :scope ---
Code 12 Invalid selector (Selectors. Not supported: scope)
--- Selector: :target ---
Code 12 Invalid selector (Selectors. Not supported: target)
--- Selector: :target-within ---
Code 12 Invalid selector (Selectors. Not supported: target-within)
--- Selector: :user-invalid ---
Code 12 Invalid selector (Selectors. Not supported: user-invalid)
--- Selector: :valid ---
Code 12 Invalid selector (Selectors. Not supported: valid)
--- Selector: :visited ---
Code 12 Invalid selector (Selectors. Not supported: visited)
--- Selector: :warning ---
Code 12 Invalid selector (Selectors. Not supported: warning)
--- Selector: :local-link ---
Code 12 Invalid selector (Selectors. Not supported: local-link)
--- Selector: :active ---
int(0)"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_nth_child.phpt`: CSS Selectors - Pseudo classes: nth-child
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_nth_child() {
    if skip_without_xml_native("test_pseudo_classes_nth_child") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container>
    <h2>1</h2>
    <h2>2</h2>
    <h2>3</h2>
    <h2>4</h2>
    <h2>5</h2>
</container>
XML);

test_helper($dom, 'h2:nth-of-type(n+2):nth-last-of-type(n+2)');
test_helper($dom, 'h2:not(:first-of-type):not(:last-of-type)'); // Equivalent to the above
test_helper($dom, 'h2:nth-child(2)');
test_helper($dom, 'h2:nth-last-child(2)');
test_helper($dom, 'h2:nth-child(2n + 1)');
test_helper($dom, 'h2:nth-last-child(2n + 1)');
test_helper($dom, 'h2:nth-child(3n - 2)');
test_helper($dom, 'h2:nth-last-child(3n - 2)');

"#);
    assert_exact(&out, r#"--- Selector: h2:nth-of-type(n+2):nth-last-of-type(n+2) ---
<h2>2</h2>
<h2>3</h2>
<h2>4</h2>
--- Selector: h2:not(:first-of-type):not(:last-of-type) ---
<h2>2</h2>
<h2>3</h2>
<h2>4</h2>
--- Selector: h2:nth-child(2) ---
<h2>2</h2>
--- Selector: h2:nth-last-child(2) ---
<h2>4</h2>
--- Selector: h2:nth-child(2n + 1) ---
<h2>1</h2>
<h2>3</h2>
<h2>5</h2>
--- Selector: h2:nth-last-child(2n + 1) ---
<h2>1</h2>
<h2>3</h2>
<h2>5</h2>
--- Selector: h2:nth-child(3n - 2) ---
<h2>1</h2>
<h2>4</h2>
--- Selector: h2:nth-last-child(3n - 2) ---
<h2>2</h2>
<h2>5</h2>"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_nth_child_of.phpt`: CSS Selectors - Pseudo classes: nth-child of
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_nth_child_of() {
    if skip_without_xml_native("test_pseudo_classes_nth_child_of") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container>
    <h2 class="hi">1</h2>
    <h2>2</h2>
    <h2 class="hi">3</h2>
    <h2 class="hi">4</h2>
    <h2>5</h2>
    <h2 class="hi">6</h2>
</container>
XML);

test_helper($dom, 'h2:nth-child(even of .hi)');
test_helper($dom, 'h2.hi:nth-child(even)');
test_helper($dom, 'h2:nth-child(odd of .hi)');
test_helper($dom, 'h2.hi:nth-child(odd)');

test_helper($dom, 'h2:nth-last-child(even of .hi)');
test_helper($dom, 'h2.hi:nth-last-child(even)');
test_helper($dom, 'h2:nth-last-child(odd of .hi)');
test_helper($dom, 'h2.hi:nth-last-child(odd)');

"#);
    assert_exact(&out, r#"--- Selector: h2:nth-child(even of .hi) ---
<h2 class="hi">3</h2>
<h2 class="hi">6</h2>
--- Selector: h2.hi:nth-child(even) ---
<h2 class="hi">4</h2>
<h2 class="hi">6</h2>
--- Selector: h2:nth-child(odd of .hi) ---
<h2 class="hi">1</h2>
<h2 class="hi">4</h2>
--- Selector: h2.hi:nth-child(odd) ---
<h2 class="hi">1</h2>
<h2 class="hi">3</h2>
--- Selector: h2:nth-last-child(even of .hi) ---
<h2 class="hi">1</h2>
<h2 class="hi">4</h2>
--- Selector: h2.hi:nth-last-child(even) ---
<h2 class="hi">1</h2>
<h2 class="hi">3</h2>
--- Selector: h2:nth-last-child(odd of .hi) ---
<h2 class="hi">3</h2>
<h2 class="hi">6</h2>
--- Selector: h2.hi:nth-last-child(odd) ---
<h2 class="hi">4</h2>
<h2 class="hi">6</h2>"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_nth_col.phpt`: CSS Selectors - Pseudo classes: nth-(last-)col
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_nth_col() {
    if skip_without_xml_native("test_pseudo_classes_nth_col") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE html>
<html>
    <body>
        <table>
            <tr>
                <th>Col 1</th>
                <th>Col 2</th>
            </tr>
            <tr>
                <td>1</td>
                <td>2</td>
            </tr>
        </table>
    </body>
</html>
HTML);

test_failure($dom, ':nth-col(1)', true);
test_failure($dom, ':nth-last-col(1)', true);

"#);
    assert_exact(&out, r#"--- Selector: :nth-col(1) ---
Code 12 Invalid selector (Selectors. Not supported: nth-col)
--- Selector: :nth-last-col(1) ---
Code 12 Invalid selector (Selectors. Not supported: nth-last-col)"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_only_child.phpt`: CSS Selectors - Pseudo classes: only-child
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_only_child() {
    if skip_without_xml_native("test_pseudo_classes_only_child") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container>
    <div class="only-child1">
        <p>Lonely</p>
    </div>
    <div class="only-child2">
        <p>With 2</p>
        <p>With 2</p>
    </div>
</container>
XML);

test_helper($dom, '.only-child1 p:only-child');
test_helper($dom, '.only-child2 p:only-child');

"#);
    assert_exact(&out, r#"--- Selector: .only-child1 p:only-child ---
<p>Lonely</p>
--- Selector: .only-child2 p:only-child ---"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_only_of_type.phpt`: CSS Selectors - Pseudo classes: only-of-type
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_only_of_type() {
    if skip_without_xml_native("test_pseudo_classes_only_of_type") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container>
    <div class="only-of-type1">
        <p>Alone</p>
    </div>
    <div class="only-of-type2">
        <p>With 2</p>
        <p>With 2</p>
    </div>
    <div class="only-of-type3">
        <p>With 2</p>
        <div/>
        <p>With 2</p>
    </div>
</container>
XML);

test_helper($dom, '.only-of-type1 p:only-of-type');
test_helper($dom, '.only-of-type2 p:only-of-type');
test_helper($dom, '.only-of-type3 p:only-of-type');

"#);
    assert_exact(&out, r#"--- Selector: .only-of-type1 p:only-of-type ---
<p>Alone</p>
--- Selector: .only-of-type2 p:only-of-type ---
--- Selector: .only-of-type3 p:only-of-type ---"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_optional_required.phpt`: CSS Selectors - Pseudo classes: optional/required
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_optional_required() {
    if skip_without_xml_native("test_pseudo_classes_optional_required") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<html xmlns="http://www.w3.org/1999/xhtml">
    <input type="checkbox" required="required" />
    <select required="required" />
    <textarea required="" />
    <input type="checkbox" />
    <select />
    <textarea />
    <input xmlns=""/>
    <input xmlns="" required="" />
</html>
XML);

test_helper($dom, ':required');
test_helper($dom, ':optional');

"#);
    assert_exact(&out, r#"--- Selector: :required ---
<input xmlns="http://www.w3.org/1999/xhtml" type="checkbox" required="required" />
<select xmlns="http://www.w3.org/1999/xhtml" required="required"></select>
<textarea xmlns="http://www.w3.org/1999/xhtml" required=""></textarea>
--- Selector: :optional ---
<input xmlns="http://www.w3.org/1999/xhtml" type="checkbox" />
<select xmlns="http://www.w3.org/1999/xhtml"></select>
<textarea xmlns="http://www.w3.org/1999/xhtml"></textarea>"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_placeholder_shown.phpt`: CSS Selectors - Pseudo classes: placeholder-shown
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_placeholder_shown() {
    if skip_without_xml_native("test_pseudo_classes_placeholder_shown") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<html xmlns="http://www.w3.org/1999/xhtml">
    <input type="text" placeholder="" />
    <textarea placeholder="" />
    <input xmlns="" type="text" placeholder="" />
    <textarea xmlns="" placeholder="" />
    <input type="text" />
    <textarea />
</html>
XML);

test_helper($dom, ':placeholder-shown');

"#);
    assert_exact(&out, r#"--- Selector: :placeholder-shown ---
<input xmlns="http://www.w3.org/1999/xhtml" type="text" placeholder="" />
<textarea xmlns="http://www.w3.org/1999/xhtml" placeholder=""></textarea>"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_read_write_read_only.phpt`: CSS Selectors - Pseudo classes: read-write/read-only
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_read_write_read_only() {
    if skip_without_xml_native("test_pseudo_classes_read_write_read_only") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<html xmlns="http://www.w3.org/1999/xhtml">
    <input type="text" readonly="" />
    <textarea readonly="" />
    <input type="text" disabled="" />
    <textarea disabled="" />
    <input type="text" xmlns="" />
    <textarea xmlns="" />
    <input type="text" />
    <textarea />
    <p contenteditable="" />
    <p contenteditable="false" />
</html>
XML);

test_helper($dom, ':read-write');
test_helper($dom, ':read-only');

"#);
    assert_exact(&out, r#"--- Selector: :read-write ---
<input xmlns="http://www.w3.org/1999/xhtml" type="text" />
<textarea xmlns="http://www.w3.org/1999/xhtml"></textarea>
<p xmlns="http://www.w3.org/1999/xhtml" contenteditable=""></p>
--- Selector: :read-only ---
<html xmlns="http://www.w3.org/1999/xhtml">
    <input type="text" readonly="" />
    <textarea readonly=""></textarea>
    <input type="text" disabled="" />
    <textarea disabled=""></textarea>
    <input xmlns="" type="text"/>
    <textarea xmlns=""/>
    <input type="text" />
    <textarea></textarea>
    <p contenteditable=""></p>
    <p contenteditable="false"></p>
</html>
<input xmlns="http://www.w3.org/1999/xhtml" type="text" readonly="" />
<textarea xmlns="http://www.w3.org/1999/xhtml" readonly=""></textarea>
<input xmlns="http://www.w3.org/1999/xhtml" type="text" disabled="" />
<textarea xmlns="http://www.w3.org/1999/xhtml" disabled=""></textarea>
<input xmlns="" type="text"/>
<textarea xmlns=""/>
<p xmlns="http://www.w3.org/1999/xhtml" contenteditable="false"></p>"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_classes_root.phpt`: CSS Selectors - Pseudo classes: root
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_classes_root() {
    if skip_without_xml_native("test_pseudo_classes_root") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container/>
XML);

test_helper($dom, ':root', true);
$fragment = $dom->createDocumentFragment();
$fragment->appendXML('<div><p></p></div>');
test_helper($fragment, ':root', true);
test_helper($dom->createElement("foo"), ':root', true);

test_helper($dom->createElement("not-a-root"), ":root", true);

"#);
    assert_exact(&out, r#"--- Selector: :root ---
container
--- Selector: :root ---
div
--- Selector: :root ---
--- Selector: :root ---"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/pseudo_elements.phpt`: CSS Selectors - Pseudo elements
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_pseudo_elements() {
    if skip_without_xml_native("test_pseudo_elements") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

$dom = DOM\XMLDocument::createFromString(<<<XML
<container>
    <a/>
</container>
XML);

// Pseudo-elements can't work in a static DOM
test_failure($dom, 'a::after');
test_failure($dom, 'a::before');
test_failure($dom, 'a::backdrop');
test_failure($dom, 'a::first-letter');
test_failure($dom, 'a::first-line');
test_failure($dom, 'a::grammar-error');
test_failure($dom, 'a::inactive-selection');
test_failure($dom, 'a::marker');
test_failure($dom, 'a::placeholder');
test_failure($dom, 'a::selection');
test_failure($dom, 'a::spelling-error');
test_failure($dom, 'a::target-text');

"#);
    assert_exact(&out, r#"--- Selector: a::after ---
Code 12 Invalid selector (Selectors. Not supported: after)
--- Selector: a::before ---
Code 12 Invalid selector (Selectors. Not supported: before)
--- Selector: a::backdrop ---
Code 12 Invalid selector (Selectors. Not supported: backdrop)
--- Selector: a::first-letter ---
Code 12 Invalid selector (Selectors. Not supported: first-letter)
--- Selector: a::first-line ---
Code 12 Invalid selector (Selectors. Not supported: first-line)
--- Selector: a::grammar-error ---
Code 12 Invalid selector (Selectors. Not supported: grammar-error)
--- Selector: a::inactive-selection ---
Code 12 Invalid selector (Selectors. Not supported: inactive-selection)
--- Selector: a::marker ---
Code 12 Invalid selector (Selectors. Not supported: marker)
--- Selector: a::placeholder ---
Code 12 Invalid selector (Selectors. Not supported: placeholder)
--- Selector: a::selection ---
Code 12 Invalid selector (Selectors. Not supported: selection)
--- Selector: a::spelling-error ---
Code 12 Invalid selector (Selectors. Not supported: spelling-error)
--- Selector: a::target-text ---
Code 12 Invalid selector (Selectors. Not supported: target-text)"#);
}

/// php-src `ext/dom/tests/modern/css_selectors/quirks_mode.phpt`: CSS Selectors - Quirks mode test
#[test]
#[ignore = r#"DOM port pending: Unknown type: DOM\ParentNode"#]
fn test_quirks_mode() {
    if skip_without_xml_native("test_quirks_mode") {
        return;
    }
    let out = run_case(r#"modern/css_selectors"#, &[], r#"<?php

require __DIR__ . '/test_utils.inc';

echo "\n=== Document in quirks mode ===\n\n";

$dom = DOM\HTMLDocument::createFromString(<<<HTML
<html>
    <div class="HElLoWorLD"/>
    <div id="hI"/>
</html>
HTML, LIBXML_NOERROR);

test_helper($dom, 'div.helloworld');
test_helper($dom, 'div.HElLoWorLD');
test_helper($dom, '#hi');
test_helper($dom, '#hI');

echo "\n=== Document not in quirks mode ===\n\n";

$dom = DOM\HTMLDocument::createFromString(<<<HTML
<!DOCTYPE html>
<html>
    <div class="HElLoWorLD"/>
    <div id="hI"/>
</html>
HTML, LIBXML_NOERROR);

test_helper($dom, 'div.helloworld');
test_helper($dom, 'div.HElLoWorLD');
test_helper($dom, '#hi');
test_helper($dom, '#hI');

?>
"#);
    assert_exact(&out, r#"=== Document in quirks mode ===

--- Selector: div.helloworld ---
<div xmlns="http://www.w3.org/1999/xhtml" class="HElLoWorLD">
    <div id="hI">
</div></div>
--- Selector: div.HElLoWorLD ---
<div xmlns="http://www.w3.org/1999/xhtml" class="HElLoWorLD">
    <div id="hI">
</div></div>
--- Selector: #hi ---
<div xmlns="http://www.w3.org/1999/xhtml" id="hI">
</div>
--- Selector: #hI ---
<div xmlns="http://www.w3.org/1999/xhtml" id="hI">
</div>

=== Document not in quirks mode ===

--- Selector: div.helloworld ---
--- Selector: div.HElLoWorLD ---
<div xmlns="http://www.w3.org/1999/xhtml" class="HElLoWorLD">
    <div id="hI">
</div></div>
--- Selector: #hi ---
--- Selector: #hI ---
<div xmlns="http://www.w3.org/1999/xhtml" id="hI">
</div>"#);
}
