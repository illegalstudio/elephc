//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `manually/call/constructor` folder, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/manually_call_constructor/attribute.phpt`: Manually call __construct() - attribute variation
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMAttr"#]
fn test_attribute() {
    if skip_without_xml_native("test_attribute") {
        return;
    }
    let out = run_case(r#"manually_call_constructor"#, &[], r#"<?php

$attr = new DOMAttr("attribute", "my value");
var_dump($attr->nodeName, $attr->nodeValue);
$attr->__construct("newattribute", "my new value");
var_dump($attr->nodeName, $attr->nodeValue);

$doc = new DOMDocument();
$doc->loadXML(<<<XML
<?xml version="1.0"?>
<container/>
XML);
$doc->documentElement->setAttributeNode($attr);
echo $doc->saveXML();

$attr->__construct("newnewattribute", "my even newer value");
$doc->documentElement->setAttributeNode($attr);
echo $doc->saveXML();

"#);
    assert_exact(&out, r#"string(9) "attribute"
string(8) "my value"
string(12) "newattribute"
string(12) "my new value"
<?xml version="1.0"?>
<container newattribute="my new value"/>
<?xml version="1.0"?>
<container newattribute="my new value" newnewattribute="my even newer value"/>"#);
}

/// php-src `ext/dom/tests/manually_call_constructor/cdatasection.phpt`: Manually call __construct() - CDATA section variation
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMCdataSection"#]
fn test_cdatasection() {
    if skip_without_xml_native("test_cdatasection") {
        return;
    }
    let out = run_case(r#"manually_call_constructor"#, &[], r#"<?php

$cdata = new DOMCdataSection("my value");
var_dump($cdata->nodeValue);
$cdata->__construct("my new value");
var_dump($cdata->nodeValue);

$doc = new DOMDocument();
$doc->loadXML(<<<XML
<?xml version="1.0"?>
<container/>
XML);
$doc->documentElement->appendChild($cdata);
echo $doc->saveXML();

$cdata->__construct("my even newer value");
$doc->documentElement->appendChild($cdata);
echo $doc->saveXML();

"#);
    assert_exact(&out, r#"string(8) "my value"
string(12) "my new value"
<?xml version="1.0"?>
<container><![CDATA[my new value]]></container>
<?xml version="1.0"?>
<container><![CDATA[my new value]]><![CDATA[my even newer value]]></container>"#);
}

/// php-src `ext/dom/tests/manually_call_constructor/comment.phpt`: Manually call __construct() - comment variation
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMComment::__construct' expects 0 arguments, got 1"#]
fn test_comment() {
    if skip_without_xml_native("test_comment") {
        return;
    }
    let out = run_case(r#"manually_call_constructor"#, &[], r#"<?php

$comment = new DOMComment("my value");
var_dump($comment->nodeName, $comment->nodeValue);
$comment->__construct("my new value");
var_dump($comment->nodeName, $comment->nodeValue);

$doc = new DOMDocument();
$doc->loadXML(<<<XML
<?xml version="1.0"?>
<container/>
XML);
$doc->documentElement->appendChild($comment);
echo $doc->saveXML();

$comment->__construct("my even newer value");
$doc->documentElement->appendChild($comment);
echo $doc->saveXML();

"#);
    assert_exact(&out, r##"string(8) "#comment"
string(8) "my value"
string(8) "#comment"
string(12) "my new value"
<?xml version="1.0"?>
<container><!--my new value--></container>
<?xml version="1.0"?>
<container><!--my new value--><!--my even newer value--></container>"##);
}

/// php-src `ext/dom/tests/manually_call_constructor/document.phpt`: Manually call __construct() - document variation
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_document() {
    if skip_without_xml_native("test_document") {
        return;
    }
    let out = run_case(r#"manually_call_constructor"#, &[], r#"<?php

$doc = new DOMDocument();
$doc->loadXML('<?xml version="1.0"?><foo/>');
$doc->__construct("1.1", "UTF-8");
echo $doc->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.1" encoding="UTF-8"?>"#);
}

/// php-src `ext/dom/tests/manually_call_constructor/documentfragment.phpt`: Manually call __construct() - document fragment variation
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMDocumentFragment"#]
fn test_documentfragment() {
    if skip_without_xml_native("test_documentfragment") {
        return;
    }
    let out = run_case(r#"manually_call_constructor"#, &[], r#"<?php

$fragment = new DOMDocumentFragment();
var_dump($fragment->textContent);
$fragment->__construct();
var_dump($fragment->textContent);

$doc = new DOMDocument();
$doc->loadXML(<<<XML
<?xml version="1.0"?>
<container/>
XML);
@$doc->documentElement->appendChild($fragment);
echo $doc->saveXML();

$fragment->__construct();
@$doc->documentElement->appendChild($fragment);
echo $doc->saveXML();

"#);
    assert_exact(&out, r#"string(0) ""
string(0) ""
<?xml version="1.0"?>
<container/>
<?xml version="1.0"?>
<container/>"#);
}

/// php-src `ext/dom/tests/manually_call_constructor/element.phpt`: Manually call __construct() - element variation
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMElement::__construct' expects 0 arguments, got 2"#]
fn test_element() {
    if skip_without_xml_native("test_element") {
        return;
    }
    let out = run_case(r#"manually_call_constructor"#, &[], r#"<?php

$element = new DOMElement('foo', 'my value');
var_dump($element->nodeName, $element->textContent);
$element->__construct('foo2', 'my new value');
var_dump($element->nodeName, $element->textContent);

$doc = new DOMDocument();
$doc->loadXML(<<<XML
<?xml version="1.0"?>
<container/>
XML);
$doc->documentElement->appendChild($element);
echo $doc->saveXML();

$element->__construct('foo3', 'my new new value');
$doc->documentElement->appendChild($element);
echo $doc->saveXML();

"#);
    assert_exact(&out, r#"string(3) "foo"
string(8) "my value"
string(4) "foo2"
string(12) "my new value"
<?xml version="1.0"?>
<container><foo2>my new value</foo2></container>
<?xml version="1.0"?>
<container><foo2>my new value</foo2><foo3>my new new value</foo3></container>"#);
}

/// php-src `ext/dom/tests/manually_call_constructor/entityreference.phpt`: Manually call __construct() - entity reference variation
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMEntityReference"#]
fn test_entityreference() {
    if skip_without_xml_native("test_entityreference") {
        return;
    }
    let out = run_case(r#"manually_call_constructor"#, &[], r#"<?php

$entityRef = new DOMEntityReference('foo');
var_dump($entityRef->nodeName, $entityRef->textContent);
$entityRef->__construct('foo2');
var_dump($entityRef->nodeName, $entityRef->textContent);

$doc = new DOMDocument();
$doc->loadXML(<<<XML
<?xml version="1.0"?>
<container/>
XML);
$doc->documentElement->appendChild($entityRef);
echo $doc->saveXML();

$entityRef->__construct('foo3');
$doc->documentElement->appendChild($entityRef);
echo $doc->saveXML();

"#);
    assert_exact(&out, r#"string(3) "foo"
string(0) ""
string(4) "foo2"
string(0) ""
<?xml version="1.0"?>
<container>&foo2;</container>
<?xml version="1.0"?>
<container>&foo2;&foo3;</container>"#);
}

/// php-src `ext/dom/tests/manually_call_constructor/processinginstruction.phpt`: Manually call __construct() - processing instruction variation
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_processinginstruction() {
    if skip_without_xml_native("test_processinginstruction") {
        return;
    }
    let out = run_case(r#"manually_call_constructor"#, &[], r#"<?php

$pi = new DOMProcessingInstruction('name1', 'value1');
var_dump($pi->target, $pi->data);
$pi->__construct('name2', 'value2');
var_dump($pi->target, $pi->data);

$doc = new DOMDocument();
$doc->loadXML(<<<XML
<?xml version="1.0"?>
<container/>
XML);
$doc->documentElement->appendChild($pi);
echo $doc->saveXML();

$pi->__construct('name3', 'value3');
$doc->documentElement->appendChild($pi);
echo $doc->saveXML();

"#);
    assert_exact(&out, r#"string(5) "name1"
string(6) "value1"
string(5) "name2"
string(6) "value2"
<?xml version="1.0"?>
<container><?name2 value2?></container>
<?xml version="1.0"?>
<container><?name2 value2?><?name3 value3?></container>"#);
}

/// php-src `ext/dom/tests/manually_call_constructor/text.phpt`: Manually call __construct() - text variation
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMText::__construct' expects 0 arguments, got 1"#]
fn test_text() {
    if skip_without_xml_native("test_text") {
        return;
    }
    let out = run_case(r#"manually_call_constructor"#, &[], r#"<?php

$text = new DOMText('my value');
var_dump($text->textContent);
$text->__construct('my new value');
var_dump($text->textContent);

$doc = new DOMDocument();
$doc->loadXML(<<<XML
<?xml version="1.0"?>
<container/>
XML);
$doc->documentElement->appendChild($text);
echo $doc->saveXML();

$text->__construct("\nmy new new value");
$doc->documentElement->appendChild($text);
echo $doc->saveXML();

"#);
    assert_exact(&out, r#"string(8) "my value"
string(12) "my new value"
<?xml version="1.0"?>
<container>my new value</container>
<?xml version="1.0"?>
<container>my new value
my new new value</container>"#);
}
