//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `create/attribute/ns/prefix/conflicts` folder, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/createAttributeNS_prefix_conflicts/setAttributeNS_with_prefix.phpt`: DOMDocument::createAttributeNS() with prefix name conflict - setAttributeNodeNS variation, with prefix
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createAttributeNS"#]
fn test_set_attribute_ns_with_prefix() {
    if skip_without_xml_native("test_set_attribute_ns_with_prefix") {
        return;
    }
    let out = run_case(r#"createAttributeNS_prefix_conflicts"#, &[], r#"<?php

$doc = new DOMDocument();
$doc->appendChild($doc->createElement('container'));

var_dump($doc->documentElement->setAttributeNodeNS($doc->createAttributeNS('http://php.net/ns1', 'foo:hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";
var_dump($doc->documentElement->setAttributeNodeNS($doc->createAttributeNS('http://php.net/ns2', 'foo:hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";
var_dump($doc->documentElement->setAttributeNodeNS($doc->createAttributeNS('http://php.net/ns3', 'foo:hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";
var_dump($doc->documentElement->setAttributeNodeNS($doc->createAttributeNS('http://php.net/ns4', 'foo:hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";

"#);
    assert_exact(&out, r#"NULL
<?xml version="1.0"?>
<container xmlns:foo="http://php.net/ns1" foo:hello=""/>

NULL
<?xml version="1.0"?>
<container xmlns:foo="http://php.net/ns1" xmlns:default="http://php.net/ns2" foo:hello="" default:hello=""/>

NULL
<?xml version="1.0"?>
<container xmlns:foo="http://php.net/ns1" xmlns:default="http://php.net/ns2" xmlns:default1="http://php.net/ns3" foo:hello="" default:hello="" default1:hello=""/>

NULL
<?xml version="1.0"?>
<container xmlns:foo="http://php.net/ns1" xmlns:default="http://php.net/ns2" xmlns:default1="http://php.net/ns3" xmlns:default2="http://php.net/ns4" foo:hello="" default:hello="" default1:hello="" default2:hello=""/>"#);
}

/// php-src `ext/dom/tests/createAttributeNS_prefix_conflicts/setAttributeNS_without_prefix.phpt`: DOMDocument::createAttributeNS() with prefix name conflict - setAttributeNodeNS variation, without prefix
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createAttributeNS"#]
fn test_set_attribute_ns_without_prefix() {
    if skip_without_xml_native("test_set_attribute_ns_without_prefix") {
        return;
    }
    let out = run_case(r#"createAttributeNS_prefix_conflicts"#, &[], r#"<?php

$doc = new DOMDocument();
$doc->appendChild($doc->createElement('container'));

var_dump($doc->documentElement->setAttributeNodeNS($doc->createAttributeNS('http://php.net/ns1', 'hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";
var_dump($doc->documentElement->setAttributeNodeNS($doc->createAttributeNS('http://php.net/ns2', 'hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";
var_dump($doc->documentElement->setAttributeNodeNS($doc->createAttributeNS('http://php.net/ns3', 'hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";
var_dump($doc->documentElement->setAttributeNodeNS($doc->createAttributeNS('http://php.net/ns4', 'hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";

"#);
    assert_exact(&out, r#"NULL
<?xml version="1.0"?>
<container xmlns:default="http://php.net/ns1" default:hello=""/>

NULL
<?xml version="1.0"?>
<container xmlns:default="http://php.net/ns1" xmlns:default1="http://php.net/ns2" default:hello="" default1:hello=""/>

NULL
<?xml version="1.0"?>
<container xmlns:default="http://php.net/ns1" xmlns:default1="http://php.net/ns2" xmlns:default2="http://php.net/ns3" default:hello="" default1:hello="" default2:hello=""/>

NULL
<?xml version="1.0"?>
<container xmlns:default="http://php.net/ns1" xmlns:default1="http://php.net/ns2" xmlns:default2="http://php.net/ns3" xmlns:default3="http://php.net/ns4" default:hello="" default1:hello="" default2:hello="" default3:hello=""/>"#);
}

/// php-src `ext/dom/tests/createAttributeNS_prefix_conflicts/setAttribute_mixed_prefix.phpt`: DOMDocument::createAttributeNS() with prefix name conflict - setAttributeNode variation (DOM Level 3), mixed
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createAttributeNS"#]
fn test_set_attribute_mixed_prefix() {
    if skip_without_xml_native("test_set_attribute_mixed_prefix") {
        return;
    }
    let out = run_case(r#"createAttributeNS_prefix_conflicts"#, &[], r#"<?php

$doc = new DOMDocument();
$doc->appendChild($doc->createElement('container'));

var_dump($doc->documentElement->setAttributeNode($doc->createAttributeNS('http://php.net/ns1', 'foo:hello'))?->namespaceURI);
var_dump($doc->documentElement->setAttributeNode($doc->createAttributeNS('http://php.net/ns1', 'hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";

"#);
    assert_exact(&out, r#"NULL
string(18) "http://php.net/ns1"
<?xml version="1.0"?>
<container xmlns:foo="http://php.net/ns1" foo:hello=""/>"#);
}

/// php-src `ext/dom/tests/createAttributeNS_prefix_conflicts/setAttribute_with_prefix.phpt`: DOMDocument::createAttributeNS() with prefix name conflict - setAttributeNode variation (DOM Level 3), with prefix
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createAttributeNS"#]
fn test_set_attribute_with_prefix() {
    if skip_without_xml_native("test_set_attribute_with_prefix") {
        return;
    }
    let out = run_case(r#"createAttributeNS_prefix_conflicts"#, &[], r#"<?php

$doc = new DOMDocument();
$doc->appendChild($doc->createElement('container'));

var_dump($doc->documentElement->setAttributeNode($doc->createAttributeNS('http://php.net/ns1', 'foo:hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";
var_dump($doc->documentElement->setAttributeNode($doc->createAttributeNS('http://php.net/ns2', 'foo:hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";
var_dump($doc->documentElement->setAttributeNode($doc->createAttributeNS('http://php.net/ns3', 'foo:hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";
var_dump($doc->documentElement->setAttributeNode($doc->createAttributeNS('http://php.net/ns4', 'foo:hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";

"#);
    assert_exact(&out, r#"NULL
<?xml version="1.0"?>
<container xmlns:foo="http://php.net/ns1" foo:hello=""/>

string(18) "http://php.net/ns1"
<?xml version="1.0"?>
<container xmlns:foo="http://php.net/ns1" xmlns:default="http://php.net/ns2" default:hello=""/>

string(18) "http://php.net/ns2"
<?xml version="1.0"?>
<container xmlns:foo="http://php.net/ns1" xmlns:default="http://php.net/ns2" xmlns:default1="http://php.net/ns3" default1:hello=""/>

string(18) "http://php.net/ns3"
<?xml version="1.0"?>
<container xmlns:foo="http://php.net/ns1" xmlns:default="http://php.net/ns2" xmlns:default1="http://php.net/ns3" xmlns:default2="http://php.net/ns4" default2:hello=""/>"#);
}

/// php-src `ext/dom/tests/createAttributeNS_prefix_conflicts/setAttribute_without_prefix.phpt`: DOMDocument::createAttributeNS() with prefix name conflict - setAttributeNode variation (DOM Level 3), without prefix
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createAttributeNS"#]
fn test_set_attribute_without_prefix() {
    if skip_without_xml_native("test_set_attribute_without_prefix") {
        return;
    }
    let out = run_case(r#"createAttributeNS_prefix_conflicts"#, &[], r#"<?php

$doc = new DOMDocument();
$doc->appendChild($doc->createElement('container'));

var_dump($doc->documentElement->setAttributeNode($doc->createAttributeNS('http://php.net/ns1', 'hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";
var_dump($doc->documentElement->setAttributeNode($doc->createAttributeNS('http://php.net/ns2', 'hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";
var_dump($doc->documentElement->setAttributeNode($doc->createAttributeNS('http://php.net/ns3', 'hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";
var_dump($doc->documentElement->setAttributeNode($doc->createAttributeNS('http://php.net/ns4', 'hello'))?->namespaceURI);
echo $doc->saveXML(), "\n";

"#);
    assert_exact(&out, r#"NULL
<?xml version="1.0"?>
<container xmlns:default="http://php.net/ns1" default:hello=""/>

string(18) "http://php.net/ns1"
<?xml version="1.0"?>
<container xmlns:default="http://php.net/ns1" xmlns:default1="http://php.net/ns2" default1:hello=""/>

string(18) "http://php.net/ns2"
<?xml version="1.0"?>
<container xmlns:default="http://php.net/ns1" xmlns:default1="http://php.net/ns2" xmlns:default2="http://php.net/ns3" default2:hello=""/>

string(18) "http://php.net/ns3"
<?xml version="1.0"?>
<container xmlns:default="http://php.net/ns1" xmlns:default1="http://php.net/ns2" xmlns:default2="http://php.net/ns3" xmlns:default3="http://php.net/ns4" default3:hello=""/>"#);
}
