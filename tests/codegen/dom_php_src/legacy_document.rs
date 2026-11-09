//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for `DOMDocument` loading, saving, validation and factory methods, as Elephc codegen tests.
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

/// php-src `ext/dom/tests/DOMDocumentFragment_appendXML_basic_001.phpt`: DOMDocumentFragment::appendXML() with children with properties.
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createDocumentFragment"#]
fn test_domdocument_fragment_append_xml_basic_001() {
    if skip_without_xml_native("test_domdocument_fragment_append_xml_basic_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$document = new DOMDocument;
$root = $document->createElement('root');
$document->appendChild($root);

$fragment = $document->createDocumentFragment();
$fragment->appendXML('<foo id="baz">bar</foo>');
$root->appendChild($fragment);

print $document->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<root><foo id="baz">bar</foo></root>"#);
}

/// php-src `ext/dom/tests/DOMDocumentFragment_appendXML_error_002.phpt`: DOMDocumentFragment::appendXML() with unbound fragment.
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMDocumentFragment"#]
fn test_domdocument_fragment_append_xml_error_002() {
    if skip_without_xml_native("test_domdocument_fragment_append_xml_error_002") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$fragment = new DOMDocumentFragment();
try {
    $fragment->appendXML('<bait>crankbait</bait>');
} catch (DOMException $e) {
    echo $e->getMessage();
}
"#);
    assert_exact(&out, r#"No Modification Allowed Error"#);
}

/// php-src `ext/dom/tests/DOMDocumentFragment_appendXML_error_003.phpt`: DOMDocumentFragment::appendXML() with unbalanced chunks.
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createDocumentFragment"#]
fn test_domdocument_fragment_append_xml_error_003() {
    if skip_without_xml_native("test_domdocument_fragment_append_xml_error_003") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$document = new DOMDocument;
$root = $document->createElement('root');
$document->appendChild($root);

$fragment = $document->createDocumentFragment();
@$fragment->appendXML('<foo>is<bar>great</foo>');
$root->appendChild($fragment);
"#);
    assert_format(&out, r#"Warning: DOMNode::appendChild(): Document Fragment is empty in %s on line %d"#);
}

/// php-src `ext/dom/tests/DOMDocumentFragment_appendXML_hasChildNodes_basic.phpt`: Testing DOMDocumentFragment::appendXML and DOMDocumentFragment::hasChildNodes
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createDocumentFragment"#]
fn test_domdocument_fragment_append_xml_has_child_nodes_basic() {
    if skip_without_xml_native("test_domdocument_fragment_append_xml_has_child_nodes_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();

$fragment = $doc->createDocumentFragment();
if ($fragment->hasChildNodes()) {
  echo "has child nodes\n";
} else {
  echo "has no child nodes\n";
}
$fragment->appendXML('<foo>bar</foo>');
if ($fragment->hasChildNodes()) {
  echo "has child nodes\n";
} else {
  echo "has no child nodes\n";
}
"#);
    assert_exact(&out, r#"has no child nodes
has child nodes"#);
}

/// php-src `ext/dom/tests/DOMDocumentFragment_construct_basic_001.phpt`: DOMDocumentFragment::__construct().
#[test]
fn test_domdocument_fragment_construct_basic_001() {
    if skip_without_xml_native("test_domdocument_fragment_construct_basic_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$fragment = new DOMDocumentFragment();
var_dump(get_class($fragment));
"#);
    assert_exact(&out, r#"string(19) "DOMDocumentFragment""#);
}

/// php-src `ext/dom/tests/DOMDocumentFragment_construct_error_001.phpt`: DOMDocumentFragment::__construct() with too many errors.
#[test]
#[ignore = r#"DOM port pending: Undefined class: DOMDocumentFragment"#]
fn test_domdocument_fragment_construct_error_001() {
    if skip_without_xml_native("test_domdocument_fragment_construct_error_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
try {
    $fragment = new DOMDocumentFragment("root");
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
"#);
    assert_exact(&out, r#"DOMDocumentFragment::__construct() expects exactly 0 arguments, 1 given"#);
}

/// php-src `ext/dom/tests/DOMDocumentType_basic_001.phpt`: DOMDocumentType: basic access to all properties.
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::doctype"#]
fn test_domdocument_type_basic_001() {
    if skip_without_xml_native("test_domdocument_type_basic_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
// Access publicId, systemId, name, internalSubset all with values.
$xml  = '<?xml version="1.0" encoding="UTF-8" ?>';
$xml .= '<!DOCTYPE chapter PUBLIC "-//OASIS//DTD DocBook XML//EN" "docbookx.dtd">';
$xml .= '<chapter>1</chapter>';
$doc = new DOMDocument();
$doc->loadXML($xml);
$doctype = $doc->doctype;
print "publicId: ".$doctype->publicId."\n";
print "systemId: ".$doctype->systemId."\n";
print "name: ".$doctype->name."\n";
print "internalSubset: ".$doctype->internalSubset."\n";


// Access entities and notations with values.
$xml  = '<?xml version="1.0" encoding="UTF-8" ?>';
$xml .= '<!DOCTYPE img [';
$xml .= '  <!ELEMENT img EMPTY>';
$xml .= '  <!ATTLIST img src ENTITY #REQUIRED>';
$xml .= '  <!ENTITY logo SYSTEM "http://www.xmlwriter.net/logo.gif" NDATA gif>';
$xml .= '  <!NOTATION gif PUBLIC "gif viewer">';
$xml .= ']>';
$xml .= '<img src="logo"/>';
$doc = new DOMDocument();
$doc->loadXML($xml);
$doctype = $doc->doctype;
$entities = $doctype->entities;
$entity = $entities->item(0);
print 'entity: '.$entity->nodeName."\n";
$notations = $doctype->notations;
$notation = $notations->item(0);
print 'notation: '.$notation->nodeName."\n";
"#);
    assert_exact(&out, r#"publicId: -//OASIS//DTD DocBook XML//EN
systemId: docbookx.dtd
name: chapter
internalSubset: 
entity: logo
notation: gif"#);
}

/// php-src `ext/dom/tests/DOMDocumentType_entities_error_001.phpt`: DOMDocumentType::entities with invalid state.
#[test]
fn test_domdocument_type_entities_error_001() {
    if skip_without_xml_native("test_domdocument_type_entities_error_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doctype = new DOMDocumentType();
try {
    $doctype->entities;
} catch (DOMException $exception) {
    echo $exception->getMessage() . "\n";
}
"#);
    assert_exact(&out, r#"Invalid State Error"#);
}

/// php-src `ext/dom/tests/DOMDocumentType_internalSubset_error_001.phpt`: DOMDocumentType::internalSubset with invalid state.
#[test]
fn test_domdocument_type_internal_subset_error_001() {
    if skip_without_xml_native("test_domdocument_type_internal_subset_error_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doctype = new DOMDocumentType();
try {
    $doctype->internalSubset;
} catch (DOMException $exception) {
    echo $exception->getMessage() . "\n";
}
"#);
    assert_exact(&out, r#"Invalid State Error"#);
}

/// php-src `ext/dom/tests/DOMDocumentType_name_error_001.phpt`: DOMDocumentType::name with invalid state.
#[test]
fn test_domdocument_type_name_error_001() {
    if skip_without_xml_native("test_domdocument_type_name_error_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doctype = new DOMDocumentType();
try {
    $doctype->name;
} catch (DOMException $exception) {
    echo $exception->getMessage() . "\n";
}
"#);
    assert_exact(&out, r#"Invalid State Error"#);
}

/// php-src `ext/dom/tests/DOMDocumentType_notations_error_001.phpt`: DOMDocumentType::notations with invalid state.
#[test]
fn test_domdocument_type_notations_error_001() {
    if skip_without_xml_native("test_domdocument_type_notations_error_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doctype = new DOMDocumentType();
try {
    $notations = $doctype->notations;
} catch (DOMException $exception) {
    echo $exception->getMessage() . "\n";
}
"#);
    assert_exact(&out, r#"Invalid State Error"#);
}

/// php-src `ext/dom/tests/DOMDocumentType_publicId_basic_001.phpt`: DOMDocumentType::publicId with empty value.
#[test]
fn test_domdocument_type_public_id_basic_001() {
    if skip_without_xml_native("test_domdocument_type_public_id_basic_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml  = '<?xml version="1.0" encoding="UTF-8" ?>';
$xml .= '<!DOCTYPE chapter SYSTEM "http://www.xmlwriter.net/logo.gif">';
$xml .= '<chapter>1</chapter>';
$doc = new DOMDocument();
$doc->loadXML($xml);
$doctype = $doc->doctype;
var_dump($doctype->publicId);
"#);
    assert_exact(&out, r#"string(0) """#);
}

/// php-src `ext/dom/tests/DOMDocumentType_publicId_error_001.phpt`: DOMDocumentType::publicId with invalid state.
#[test]
fn test_domdocument_type_public_id_error_001() {
    if skip_without_xml_native("test_domdocument_type_public_id_error_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doctype = new DOMDocumentType();
try {
    $publicId = $doctype->publicId;
} catch (DOMException $exception) {
    echo $exception->getMessage() . "\n";
}
"#);
    assert_exact(&out, r#"Invalid State Error"#);
}

/// php-src `ext/dom/tests/DOMDocumentType_systemId_basic_001.phpt`: DOMDocumentType::systemId with empty value.
#[test]
fn test_domdocument_type_system_id_basic_001() {
    if skip_without_xml_native("test_domdocument_type_system_id_basic_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml  = '<?xml version="1.0" encoding="UTF-8" ?>';
$xml .= '<!DOCTYPE chapter>';
$xml .= '<chapter>1</chapter>';
$doc = new DOMDocument();
$doc->loadXML($xml);
$doctype = $doc->doctype;
var_dump($doctype->systemId);
"#);
    assert_exact(&out, r#"string(0) """#);
}

/// php-src `ext/dom/tests/DOMDocumentType_systemId_error_001.phpt`: DOMDocumentType::systemId with invalid state.
#[test]
fn test_domdocument_type_system_id_error_001() {
    if skip_without_xml_native("test_domdocument_type_system_id_error_001") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doctype = new DOMDocumentType();
try {
    $systemId = $doctype->systemId;
} catch (DOMException $exception) {
    echo $exception->getMessage() . "\n";
}
"#);
    assert_exact(&out, r#"Invalid State Error"#);
}

/// php-src `ext/dom/tests/DOMDocument_adoptNode.phpt`: Tests DOMDocument::adoptNode()
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::firstChild"#]
fn test_domdocument_adopt_node() {
    if skip_without_xml_native("test_domdocument_adopt_node") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc1 = new DOMDocument();
$doc1->loadXML("<p><b>hi<i attrib=\"1\">x</i></b>world</p>");
$doc2 = new DOMDocument();
$doc2->loadXML("<div/>");

$b_tag_element = $doc1->firstChild->firstChild;
$i_tag_element = $b_tag_element->lastChild;

echo "-- Owner document check before adopting --\n";
var_dump($i_tag_element->ownerDocument === $doc1);
var_dump($i_tag_element->ownerDocument === $doc2);

echo "-- Trying to append child from other document --\n";
try {
    $doc2->firstChild->appendChild($b_tag_element); // Should fail because it's another document
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "-- Adopting --\n";
$adopted = $doc2->adoptNode($b_tag_element);
var_dump($adopted->textContent);
var_dump($doc1->saveXML());
var_dump($doc2->saveXML());

echo "-- Appending the adopted node --\n";

$doc2->firstChild->appendChild($adopted);
var_dump($doc2->saveXML());
var_dump($i_tag_element->ownerDocument === $doc1);
var_dump($i_tag_element->ownerDocument === $doc2);

echo "-- Adopt node to the original document --\n";

$adopted = $doc1->adoptNode($doc1->firstChild->firstChild);
var_dump($adopted->textContent);
var_dump($doc1->saveXML());

echo "-- Adopt a document (strict error on) --\n";

try {
    $doc1->adoptNode($doc1);
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}

echo "-- Adopt a document (strict error off) --\n";

$doc1->strictErrorChecking = false;
try {
    $doc1->adoptNode($doc1);
} catch (\DOMException $e) {
    echo $e->getMessage(), "\n";
}
$doc1->strictErrorChecking = true;

echo "-- Adopt an attribute --\n";

$doc3 = new DOMDocument();
$doc3->loadXML('<p align="center">hi</p>');
$attribute = $doc3->firstChild->attributes->item(0);
var_dump($attribute->parentNode !== NULL);
$adopted = $doc3->adoptNode($attribute);
var_dump($adopted->parentNode === NULL);
echo $doc3->saveXML();

echo "-- Append an attribute from another document --\n";

$doc4 = new DOMDocument();
$doc4->appendChild($doc4->createElement('container'));
$doc4->documentElement->appendChild($doc4->adoptNode($adopted));
echo $doc4->saveXML();

echo "-- Adopt an entity reference --\n";

$doc4 = new DOMDocument();
$doc4->loadXML(<<<'XML'
<?xml version='1.0' encoding='utf-8' ?>
<!DOCTYPE set PUBLIC "-//OASIS//DTD DocBook XML V5.0//EN" "http://www.docbook.org/xml/5.0/dtd/docbook.dtd" [
<!ENTITY my_entity '<p>hi</p>'> ]>
<p/>
XML, LIBXML_NOENT);
$p_tag_element = $doc4->firstChild->nextSibling;
$entity_reference = $doc4->createEntityReference('my_entity');
$p_tag_element->appendChild($entity_reference);
var_dump($doc4->saveXML());
$doc3->adoptNode($entity_reference);
var_dump($doc4->saveXML());
$doc3->firstChild->appendChild($entity_reference);
var_dump($doc3->saveXML());

echo "-- Adopt a node and destroy the new document --\n";

$doc1 = new DOMDocument();
$doc1->appendChild($doc1->createElement('child'));
$doc2 = new DOMDocument();
$doc2->appendChild($doc2->createElement('container'));
$doc2->documentElement->appendChild($child = $doc2->adoptNode($doc1->documentElement));
echo $doc1->saveXML();
echo $doc2->saveXML();
// Try to trigger a use-after-free
unset($doc2);
var_dump($child->nodeName);
unset($doc1);
var_dump($child->nodeName);

?>
"#);
    assert_format(&out, r#"-- Owner document check before adopting --
bool(true)
bool(false)
-- Trying to append child from other document --
Wrong Document Error
-- Adopting --
string(3) "hix"
string(35) "<?xml version="1.0"?>
<p>world</p>
"
string(29) "<?xml version="1.0"?>
<div/>
"
-- Appending the adopted node --
string(62) "<?xml version="1.0"?>
<div><b>hi<i attrib="1">x</i></b></div>
"
bool(false)
bool(true)
-- Adopt node to the original document --
string(5) "world"
string(27) "<?xml version="1.0"?>
<p/>
"
-- Adopt a document (strict error on) --
Not Supported Error
-- Adopt a document (strict error off) --

Warning: DOMDocument::adoptNode(): Not Supported Error in %s on line %d
-- Adopt an attribute --
bool(true)
bool(true)
<?xml version="1.0"?>
<p>hi</p>
-- Append an attribute from another document --
<?xml version="1.0"?>
<container align="center"/>
-- Adopt an entity reference --
string(202) "<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE set PUBLIC "-//OASIS//DTD DocBook XML V5.0//EN" "http://www.docbook.org/xml/5.0/dtd/docbook.dtd" [
<!ENTITY my_entity "<p>hi</p>">
]>
<p>&my_entity;</p>
"
string(188) "<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE set PUBLIC "-//OASIS//DTD DocBook XML V5.0//EN" "http://www.docbook.org/xml/5.0/dtd/docbook.dtd" [
<!ENTITY my_entity "<p>hi</p>">
]>
<p/>
"
string(43) "<?xml version="1.0"?>
<p>hi&my_entity;</p>
"
-- Adopt a node and destroy the new document --
<?xml version="1.0"?>
<?xml version="1.0"?>
<container><child/></container>
string(5) "child"
string(5) "child""#);
}

/// php-src `ext/dom/tests/DOMDocument_adoptNode_attribute_references.phpt`: DOMDocument::adoptNode() with attribute references
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMNode::setAttributeNS"#]
fn test_domdocument_adopt_node_attribute_references() {
    if skip_without_xml_native("test_domdocument_adopt_node_attribute_references") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$dom = new DOMDocument;
$root = $dom->appendChild($dom->createElement('root'));
$root->setAttributeNS("urn:a", "a:root1", "bar");
$root1 = $root->getAttributeNodeNS("urn:a", "root1");
echo $dom->saveXML();

$dom = new DOMDocument;
$dom->appendChild($dom->adoptNode($root));
foreach ($dom->documentElement->attributes as $attr) {
    var_dump($attr->namespaceURI, $attr->prefix, $attr->localName, $attr->nodeValue);
}

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<root xmlns:a="urn:a" a:root1="bar"/>
string(5) "urn:a"
string(1) "a"
string(5) "root1"
string(3) "bar""#);
}

/// php-src `ext/dom/tests/DOMDocument_config_basic.phpt`: Tests DOMDocument::config read
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::config"#]
fn test_domdocument_config_basic() {
    if skip_without_xml_native("test_domdocument_config_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
// create dom document
$dom = new DOMDocument;
echo "DOMDocument created\n";

$test = $dom->config;
echo "Read config:\n";
var_dump( $test );

// note -- will always be null as DOMConfiguration is not implemented in PHP

echo "Done\n";
"#);
    assert_format(&out, r#"DOMDocument created

Deprecated: Property DOMDocument::$config is deprecated in %s on line %d
Read config:
NULL
Done"#);
}

/// php-src `ext/dom/tests/DOMDocument_createAttribute_basic.phpt`: DomDocument::createAttribute() - basic test for DomDocument::createAttribute()
#[test]
fn test_domdocument_create_attribute_basic() {
    if skip_without_xml_native("test_domdocument_create_attribute_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$node = $doc->createElement("para");
$newnode = $doc->appendChild($node);

// A pass case.
$test_attribute = $doc->createAttribute("hahaha");
$node->appendChild($test_attribute);

echo $doc->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<para hahaha=""/>"#);
}

/// php-src `ext/dom/tests/DOMDocument_createAttribute_error.phpt`: Test DOMDocument::createAttribute() for expected exception thrown when wrong parameter passed
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createAttribute"#]
fn test_domdocument_create_attribute_error() {
    if skip_without_xml_native("test_domdocument_create_attribute_error") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument();

try {
    $attr = $dom->createAttribute(0);
}
catch(DOMException $e) {
    $code = $e->getCode();
    if(DOM_INVALID_CHARACTER_ERR === $code) {
        echo "PASS";
    }
    else {
        echo 'Wrong exception code';
    }
}
catch(Exception $e) {
    echo 'Wrong exception thrown';
}

"#);
    assert_exact(&out, r#"PASS"#);
}

/// php-src `ext/dom/tests/DOMDocument_createAttribute_error1.phpt`: DomDocument::createAttribute() - error test for DomDocument::createAttribute()
#[test]
fn test_domdocument_create_attribute_error1() {
    if skip_without_xml_native("test_domdocument_create_attribute_error1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$node = $doc->createElement("para");
$newnode = $doc->appendChild($node);

try {
    $failed_test_attribute = $doc->createAttribute("ha haha");
    $node->appendChild($failed_test_attribute);

    echo $doc->saveXML();
}
catch (DOMException $e) {
    echo 'Test failed!', PHP_EOL;
}

"#);
    assert_exact(&out, r#"Test failed!"#);
}

/// php-src `ext/dom/tests/DOMDocument_createAttribute_variation.phpt`: Test DOMDocument::createAttribute() for expected return value
#[test]
fn test_domdocument_create_attribute_variation() {
    if skip_without_xml_native("test_domdocument_create_attribute_variation") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument();

$attr = $dom->createAttribute('string');
echo get_class($attr);

"#);
    assert_exact(&out, r#"DOMAttr"#);
}

/// php-src `ext/dom/tests/DOMDocument_createEntityReference_basic.phpt`: DOMDocument::createEntityReference() should create a new entity reference node
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createEntityReference"#]
fn test_domdocument_create_entity_reference_basic() {
    if skip_without_xml_native("test_domdocument_create_entity_reference_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument('1.0');
$ref = $dom->createEntityReference('nbsp');
$dom->appendChild($ref);
echo $dom->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
&nbsp;"#);
}

/// php-src `ext/dom/tests/DOMDocument_createEntityReference_error1.phpt`: DomDocument::createEntityReference() - DOM_INVALID_CHARACTER_ERR raised if name contains an invalid character
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createEntityReference"#]
fn test_domdocument_create_entity_reference_error1() {
    if skip_without_xml_native("test_domdocument_create_entity_reference_error1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$objDoc = new DomDocument();

try {
    $objDoc->createEntityReference('!');
} catch (DOMException $e) {
    var_dump($e->getCode() === DOM_INVALID_CHARACTER_ERR);
    echo $e->getMessage();
}
"#);
    assert_exact(&out, r#"bool(true)
Invalid Character Error"#);
}

/// php-src `ext/dom/tests/DOMDocument_createProcessingInstruction_basic.phpt`: DomDocument::createProcessingInstruction() - basic test for DomDocument::createProcessingInstruction()
#[test]
fn test_domdocument_create_processing_instruction_basic() {
    if skip_without_xml_native("test_domdocument_create_processing_instruction_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$node = $doc->createElement("para");
$newnode = $doc->appendChild($node);

$test_proc_inst0 =
    $doc->createProcessingInstruction( "blablabla" );
$node->appendChild($test_proc_inst0);

$test_proc_inst1 =
    $doc->createProcessingInstruction( "blablabla", "datadata" );
$node->appendChild($test_proc_inst1);

echo $doc->saveXML();

"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<para><?blablabla?><?blablabla datadata?></para>"#);
}

/// php-src `ext/dom/tests/DOMDocument_createProcessingInstruction_error.phpt`: DomDocument::createProcessingInstruction() - error test for DomDocument::createProcessingInstruction()
#[test]
fn test_domdocument_create_processing_instruction_error() {
    if skip_without_xml_native("test_domdocument_create_processing_instruction_error") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$node = $doc->createElement("para");
$newnode = $doc->appendChild($node);

try {
    $test_proc_inst =
        $doc->createProcessingInstruction( "bla bla bla" );
    $node->appendChild($test_proc_inst);

    echo $doc->saveXML();
}
catch (DOMException $e)
{
    echo 'Test failed!', PHP_EOL;
}

"#);
    assert_exact(&out, r#"Test failed!"#);
}

/// php-src `ext/dom/tests/DOMDocument_documentURI_basic.phpt`: Tests DOMDocument::documentURI read and write
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::documentURI"#]
fn test_domdocument_document_uri_basic() {
    if skip_without_xml_native("test_domdocument_document_uri_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
// create dom document
$dom = new DOMDocument;
$xml = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE s1 PUBLIC "http://www.ibm.com/example.dtd" "example.dtd">
<s1>foo</s1>';
$dom->loadXML($xml);
if(!$dom) {
  echo "Error while parsing the document\n";
  exit(0);
}
echo "DOMDocument created\n";

$test = $dom->documentURI;
echo "Read initial documentURI:\n";
echo $test."\n";

$dom->documentURI = 'http://dom.example.org/example.xml';
$test = $dom->documentURI;
echo "Set documentURI to a URL, reading again:\n";
var_dump( $test );

echo "Done\n";
"#);
    assert_format(&out, r#"DOMDocument created
Read initial documentURI:
%s
Set documentURI to a URL, reading again:
string(34) "http://dom.example.org/example.xml"
Done"#);
}

/// php-src `ext/dom/tests/DOMDocument_encoding_basic.phpt`: DOMDocument::$encoding - read/write tests (dom_document_encoding_read/dom_document_encoding_write)
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_domdocument_encoding_basic() {
    if skip_without_xml_native("test_domdocument_encoding_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

require_once('dom_test.inc');

$dom = new DOMDocument;
$dom->loadXML($xmlstr);

if( !$dom )
{
    echo "Error while parsing the document\n";
    exit(0);
}

echo "Empty Encoding Read: '{$dom->encoding}'\n";

try {
    $ret = $dom->encoding = 'NYPHP DOMinatrix';
    echo "Adding invalid encoding: $ret\n";
} catch (\ValueError $e) {
    echo $e->getMessage() . \PHP_EOL;
}

$ret = $dom->encoding = 'ISO-8859-1';
echo "Adding ISO-8859-1 encoding: $ret\n";
echo "ISO-8859-1 Encoding Read: {$dom->encoding}\n";

$ret = $dom->encoding = 'UTF-8';
echo "Adding UTF-8 encoding: $ret\n";
echo "UTF-8 Encoding Read: {$dom->encoding}\n";

$ret = $dom->encoding = 'UTF-16';
echo "Adding UTF-16 encoding: $ret\n";
echo "UTF-16 Encoding Read: {$dom->encoding}\n";


"#);
    assert_exact(&out, r#"Empty Encoding Read: ''
Invalid document encoding
Adding ISO-8859-1 encoding: ISO-8859-1
ISO-8859-1 Encoding Read: ISO-8859-1
Adding UTF-8 encoding: UTF-8
UTF-8 Encoding Read: UTF-8
Adding UTF-16 encoding: UTF-16
UTF-16 Encoding Read: UTF-16"#);
}

/// php-src `ext/dom/tests/DOMDocument_getElementsByTagNameNS_match_any_namespace.phpt`: DOMDocument::getElementsByTagNameNS() match any namespace
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::getElementsByTagNameNS"#]
fn test_domdocument_get_elements_by_tag_name_ns_match_any_namespace() {
    if skip_without_xml_native("test_domdocument_get_elements_by_tag_name_ns_match_any_namespace") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

/* Sample document taken from https://www.php.net/manual/en/domdocument.getelementsbytagname.php */
$xml = <<<EOD
<?xml version="1.0" ?>
<chapter xmlns:xi="http://www.w3.org/2001/XInclude">
<title>Books of the other guy..</title>
<para>
 <xi:include href="book.xml">
  <xi:fallback>
   <error>xinclude: book.xml not found</error>
  </xi:fallback>
 </xi:include>
 <include>
  This is another namespace
 </include>
</para>
</chapter>
EOD;
$dom = new DOMDocument;

// load the XML string defined above
$dom->loadXML($xml);

function test($namespace, $local) {
    global $dom;
    $namespace_str = $namespace !== NULL ? "'$namespace'" : "null";
    echo "-- getElementsByTagNameNS($namespace_str, '$local') --\n";
    foreach ($dom->getElementsByTagNameNS($namespace, $local) as $element) {
        echo 'local name: \'', $element->localName, '\', prefix: \'', $element->prefix, "'\n";
    }
}

// Should *also* include objects even without a namespace
test(null, '*');
// Should *also* include objects even without a namespace
test('*', '*');
// Should *only* include objects without a namespace
test('', '*');
// Should *only* include objects with the specified namespace
test('http://www.w3.org/2001/XInclude', '*');
// Should not give any output
test('', 'fallback');
// Should not give any output, because the null namespace is the same as the empty namespace
test(null, 'fallback');
// Should only output the include from the empty namespace
test(null, 'include');

"#);
    assert_exact(&out, r#"-- getElementsByTagNameNS(null, '*') --
local name: 'chapter', prefix: ''
local name: 'title', prefix: ''
local name: 'para', prefix: ''
local name: 'error', prefix: ''
local name: 'include', prefix: ''
-- getElementsByTagNameNS('*', '*') --
local name: 'chapter', prefix: ''
local name: 'title', prefix: ''
local name: 'para', prefix: ''
local name: 'include', prefix: 'xi'
local name: 'fallback', prefix: 'xi'
local name: 'error', prefix: ''
local name: 'include', prefix: ''
-- getElementsByTagNameNS('', '*') --
local name: 'chapter', prefix: ''
local name: 'title', prefix: ''
local name: 'para', prefix: ''
local name: 'error', prefix: ''
local name: 'include', prefix: ''
-- getElementsByTagNameNS('http://www.w3.org/2001/XInclude', '*') --
local name: 'include', prefix: 'xi'
local name: 'fallback', prefix: 'xi'
-- getElementsByTagNameNS('', 'fallback') --
-- getElementsByTagNameNS(null, 'fallback') --
-- getElementsByTagNameNS(null, 'include') --
local name: 'include', prefix: ''"#);
}

/// php-src `ext/dom/tests/DOMDocument_getElementsByTagName_liveness.phpt`: DOMDocument::getElementsByTagName() is live
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domdocument_get_elements_by_tag_name_liveness() {
    if skip_without_xml_native("test_domdocument_get_elements_by_tag_name_liveness") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXML( '<root><e i="1"/><e i="2"/><e i="3"/><e i="4"/><e i="5"/><e i="6"/><e i="7"/><e i="8"/><e i="9"/><e i="10"/><e i="11"/><e i="12"/><e i="13"/><e i="14"/><e i="15"/></root>' );
$root = $doc->documentElement;

$i = 0;

/* Note that the list is live. The explanation for the output is as follows:
   Before the loop we have the following (writing only the attributes):
   1 2 3 4 5 6 7 8 9 10 11 12 13 14 15

   Now the loop starts, the current element is marked with a V. $i == 0:
   V
   1  2  3  4  5  6  7  8  9  10  11  12  13  14  15
   1 gets printed. $i == 0, which is even, so 1 gets removed, which results in:
   V
   2  3  4  5  6  7  8  9  10  11  12  13  14  15
   Note that everything shifted to the left.
   Because the list is live, the current element pointer still refers to the first index, which now corresponds to element with attribute 2.
   Now the foreach body ends, which means we go to the next element, which is now 3 instead of 2.
      V
   2  3  4  5  6  7  8  9  10  11  12  13  14  15
   3 gets printed. $i == 1, which is odd, so nothing happens and we move on to the next element:
         V
   2  3  4  5  6  7  8  9  10  11  12  13  14  15
   4 gets printed. $i == 2, which is even, so 4 gets removed, which results in:
         V
   2  3  5  6  7  8  9  10  11  12  13  14  15
   Note again everything shifted to the left.
   Now the foreach body ends, which means we go to the next element, which is now 6 instead of 5.
            V
   2  3  5  6  7  8  9  10  11  12  13  14  15
   6 gets printed, etc... */
foreach ($doc->getElementsByTagName('e') as $node) {
	print $node->getAttribute('i') . ' ';
	if ($i++ % 2 == 0)
		$root->removeChild($node);
}
print "\n";
"#);
    assert_exact(&out, r#"1 3 4 6 7 9 10 12 13 15"#);
}

/// php-src `ext/dom/tests/DOMDocument_getElementsByTagName_liveness_simplexml.phpt`: DOMDocument::getElementsByTagName() liveness with simplexml_import_dom
#[test]
fn test_domdocument_get_elements_by_tag_name_liveness_simplexml() {
    if skip_without_xml_native("test_domdocument_get_elements_by_tag_name_liveness_simplexml") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;
$doc->loadXML( '<root><e i="1"/><e i="2"/><e i="3"/><e i="4"/><e i="5"/><e i="6"/><e i="7"/><e i="8"/><e i="9"/><e i="10"/></root>' );
$list = $doc->getElementsByTagName('e');
print $list->item(5)->getAttribute('i')."\n";
echo "before import\n";
$s = simplexml_import_dom($doc->documentElement);
echo "after import\n";

unset($s->e[5]);
print $list->item(5)->getAttribute('i')."\n";

unset($s->e[5]);
print $list->item(5)->getAttribute('i')."\n";

"#);
    assert_exact(&out, r#"6
before import
after import
7
8"#);
}

/// php-src `ext/dom/tests/DOMDocument_getElementsByTagName_liveness_tree_walk.phpt`: DOMDocument::getElementsByTagName() liveness tree walk
#[test]
fn test_domdocument_get_elements_by_tag_name_liveness_tree_walk() {
    if skip_without_xml_native("test_domdocument_get_elements_by_tag_name_liveness_tree_walk") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;
$doc->loadXML('<root><container><a><b i="1"/><b i="2"/></a><b i="3"/></container><b i="4"/></root>');

echo "-- On first child, for --\n";
$list = $doc->documentElement->firstChild->getElementsByTagName('b');
var_dump($list->length);
for ($i = 0; $i < $list->length; $i++) {
	echo $i, " ", $list->item($i)->getAttribute('i'), "\n";
}
// Try to access one beyond to check if we don't get excess elements
var_dump($list->item($i));

echo "-- On first child, foreach --\n";
foreach ($list as $item) {
	echo $item->getAttribute('i'), "\n";
}

echo "-- On document, for --\n";
$list = $doc->getElementsByTagName('b');
var_dump($list->length);
for ($i = 0; $i < $list->length; $i++) {
	echo $i, " ", $list->item($i)->getAttribute('i'), "\n";
}
// Try to access one beyond to check if we don't get excess elements
var_dump($list->item($i));

echo "-- On document, foreach --\n";
foreach ($list as $item) {
	echo $item->getAttribute('i'), "\n";
}

echo "-- On document, after caching followed by removing --\n";

$list = $doc->documentElement->firstChild->getElementsByTagName('b');
$list->item(0); // Activate item cache
$list->item(0)->remove();
$list->item(0)->remove();
$list->item(0)->remove();
var_dump($list->length);
var_dump($list->item(0));
foreach ($list as $item) {
    echo "Should not execute\n";
}

echo "-- On document, clean list after removal --\n";
$list = $doc->documentElement->firstChild->getElementsByTagName('b');
var_dump($list->length);
var_dump($list->item(0));
foreach ($list as $item) {
    echo "Should not execute\n";
}

"#);
    assert_exact(&out, r#"-- On first child, for --
int(3)
0 1
1 2
2 3
NULL
-- On first child, foreach --
1
2
3
-- On document, for --
int(4)
0 1
1 2
2 3
3 4
NULL
-- On document, foreach --
1
2
3
4
-- On document, after caching followed by removing --
int(0)
NULL
-- On document, clean list after removal --
int(0)
NULL"#);
}

/// php-src `ext/dom/tests/DOMDocument_getElementsByTagName_liveness_write_properties.phpt`: DOMDocument::getElementsByTagName() liveness affected by writing properties
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::getElementsByTagNameNS"#]
fn test_domdocument_get_elements_by_tag_name_liveness_write_properties() {
    if skip_without_xml_native("test_domdocument_get_elements_by_tag_name_liveness_write_properties") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$xml = '<root xmlns:ns1="foo" xmlns:ns2="bar"><ns1:a/></root>';
$fields = ['nodeValue', 'textContent'];

foreach ($fields as $field) {
    $doc = new DOMDocument;
    $doc->loadXML($xml);
    $list = $doc->getElementsByTagName('a');
    var_dump($list->item(0) === NULL);
    $doc->documentElement->{$field} = 'new_content';
    var_dump($list->item(0) === NULL);
    print $doc->saveXML();
}

// Shouldn't be affected
$doc = new DOMDocument;
$doc->loadXML($xml);
$list = $doc->getElementsByTagNameNS('foo', 'a');
var_dump($list->item(0) === NULL);
$doc->documentElement->firstChild->prefix = 'ns2';
var_dump($list->item(0) === NULL);
print $doc->saveXML();

"#);
    assert_exact(&out, r#"bool(false)
bool(true)
<?xml version="1.0"?>
<root xmlns:ns1="foo" xmlns:ns2="bar">new_content</root>
bool(false)
bool(true)
<?xml version="1.0"?>
<root xmlns:ns1="foo" xmlns:ns2="bar">new_content</root>
bool(false)
bool(false)
<?xml version="1.0"?>
<root xmlns:ns1="foo" xmlns:ns2="bar"><ns2:a xmlns:ns2="foo"/></root>"#);
}

/// php-src `ext/dom/tests/DOMDocument_getElementsByTagName_liveness_xinclude.phpt`: DOMDocument::getElementsByTagName() liveness with DOMDocument::xinclude()
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::xinclude"#]
fn test_domdocument_get_elements_by_tag_name_liveness_xinclude() {
    if skip_without_xml_native("test_domdocument_get_elements_by_tag_name_liveness_xinclude") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

// Adapted from https://www.php.net/manual/en/domdocument.xinclude.php
$xml = <<<EOD
<?xml version="1.0" ?>
<chapter xmlns:xi="http://www.w3.org/2001/XInclude">
 <p>Hello</p>
 <para>
  <xi:include href="book.xml">
   <xi:fallback>
    <p>xinclude: book.xml not found</p>
   </xi:fallback>
  </xi:include>
 </para>
</chapter>
EOD;

$dom = new DOMDocument;
$dom->loadXML($xml);
$elements = $dom->getElementsByTagName('p');
var_dump($elements->item(0)->textContent);
@$dom->xinclude();
var_dump($elements->item(1)->textContent);
echo $dom->saveXML();

"#);
    assert_exact(&out, r#"string(5) "Hello"
string(28) "xinclude: book.xml not found"
<?xml version="1.0"?>
<chapter xmlns:xi="http://www.w3.org/2001/XInclude">
 <p>Hello</p>
 <para>
  
    <p>xinclude: book.xml not found</p>
   
 </para>
</chapter>"#);
}

/// php-src `ext/dom/tests/DOMDocument_implementationRead_basic.phpt`: DOMDocument::DOMImplementation - basic test for DomDocument::DOMImplementation
#[test]
fn test_domdocument_implementation_read_basic() {
    if skip_without_xml_native("test_domdocument_implementation_read_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;
$doc->load(__DIR__."/book.xml");

var_dump($doc->implementation);


"#);
    assert_exact(&out, r#"object(DOMImplementation)#2 (0) {
}"#);
}

/// php-src `ext/dom/tests/DOMDocument_importNode_attribute_prefix_conflict.phpt`: DOMDocument::importNode() with attribute prefix name conflict
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::importNode"#]
fn test_domdocument_import_node_attribute_prefix_conflict() {
    if skip_without_xml_native("test_domdocument_import_node_attribute_prefix_conflict") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

echo "--- Non-default namespace test case without a default namespace in the destination ---\n";

$dom1 = new DOMDocument();
$dom2 = new DOMDocument();
$dom1->loadXML('<?xml version="1.0"?><container xmlns:foo="http://php.net" foo:bar="yes"/>');
$dom2->loadXML('<?xml version="1.0"?><container xmlns:foo="http://php.net/2"/>');
$attribute = $dom1->documentElement->getAttributeNode('foo:bar');
$imported = $dom2->importNode($attribute);
$dom2->documentElement->setAttributeNodeNS($imported);

echo $dom1->saveXML();
echo $dom2->saveXML();

echo "--- Non-default namespace test case with a default namespace in the destination ---\n";

$dom1 = new DOMDocument();
$dom2 = new DOMDocument();
$dom1->loadXML('<?xml version="1.0"?><container xmlns:foo="http://php.net" foo:bar="yes"/>');
$dom2->loadXML('<?xml version="1.0"?><container xmlns="http://php.net" xmlns:foo="http://php.net/2"/>');
$attribute = $dom1->documentElement->getAttributeNode('foo:bar');
$imported = $dom2->importNode($attribute);
var_dump($imported->prefix, $imported->namespaceURI);
$dom2->documentElement->setAttributeNodeNS($imported);
var_dump($imported->prefix, $imported->namespaceURI);

echo $dom1->saveXML();
echo $dom2->saveXML();

echo "--- Default namespace test case ---\n";

// We don't expect the namespace to be imported because default namespaces on the same element don't apply to attributes
// but the attribute should be imported
$dom1 = new DOMDocument();
$dom2 = new DOMDocument();
$dom1->loadXML('<?xml version="1.0"?><container xmlns="http://php.net" bar="yes"/>');
$dom2->loadXML('<?xml version="1.0"?><container xmlns="http://php.net/2"/>');
$attribute = $dom1->documentElement->getAttributeNode('bar');
$imported = $dom2->importNode($attribute);
$dom2->documentElement->setAttributeNodeNS($imported);

echo $dom1->saveXML();
echo $dom2->saveXML();

"#);
    assert_exact(&out, r#"--- Non-default namespace test case without a default namespace in the destination ---
<?xml version="1.0"?>
<container xmlns:foo="http://php.net" foo:bar="yes"/>
<?xml version="1.0"?>
<container xmlns:foo="http://php.net/2" xmlns:default="http://php.net" default:bar="yes"/>
--- Non-default namespace test case with a default namespace in the destination ---
string(7) "default"
string(14) "http://php.net"
string(7) "default"
string(14) "http://php.net"
<?xml version="1.0"?>
<container xmlns:foo="http://php.net" foo:bar="yes"/>
<?xml version="1.0"?>
<container xmlns="http://php.net" xmlns:foo="http://php.net/2" xmlns:default="http://php.net" default:bar="yes"/>
--- Default namespace test case ---
<?xml version="1.0"?>
<container xmlns="http://php.net" bar="yes"/>
<?xml version="1.0"?>
<container xmlns="http://php.net/2" bar="yes"/>"#);
}

/// php-src `ext/dom/tests/DOMDocument_item_cache_invalidation.phpt`: DOMDocument node list item cache invalidation
#[test]
fn test_domdocument_item_cache_invalidation() {
    if skip_without_xml_native("test_domdocument_item_cache_invalidation") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

echo "-- Switch document test --\n";

$doc = new DOMDocument();
$doc->loadHTML('<p>hello</p><p>world</p>');

$elements = $doc->getElementsByTagName('p');
$elements->item(0); // Activate item cache
$doc->loadHTML('<p>A</p><p>B</p><p>C</p>');
var_dump($elements);
var_dump($elements->item(0)->textContent); // First lookup
var_dump($elements->item(2)->textContent); // Uses cache
var_dump($elements->item(1)->textContent); // Does not use cache

echo "-- Remove cached item test --\n";

$doc = new DOMDocument();
$doc->loadHTML('<p>hello</p><p>world</p><p>!</p>');

$elements = $doc->getElementsByTagName('p');
$item = $elements->item(0); // Activate item cache
var_dump($item->textContent);
$item->remove();
// Now element 0 means "world", and 1 means "!"
unset($item);
$item = $elements->item(1);
var_dump($item->textContent);

echo "-- Removal of cached item in loop test --\n";

$doc = new DOMDocument;
$doc->loadXML( '<root><e i="1"/><e i="2"/><e i="3"/><e i="4"/><e i="5"/><e i="6"/><e i="7"/><e i="8"/><e i="9"/><e i="10"/><e i="11"/><e i="12"/><e i="13"/><e i="14"/><e i="15"/></root>' );
$root = $doc->documentElement;

$i = 0;
$elements = $root->getElementsByTagName('e');
for ($i = 0; $i < 11; $i++) {
  $node = $elements->item($i);
  print $node->getAttribute('i') . ' ';
  if ($i++ % 2 == 0)
    $root->removeChild( $node );
}
print "\n";

"#);
    assert_format(&out, r#"-- Switch document test --
object(DOMNodeList)#2 (1) {
  ["length"]=>
  int(3)
}
string(1) "A"
string(1) "C"
string(1) "B"
-- Remove cached item test --
string(5) "hello"
string(1) "!"
-- Removal of cached item in loop test --
1 4 7 10 13 
Fatal error: Uncaught Error: Call to a member function getAttribute() on null in %s:%d
Stack trace:
#0 {main}
  thrown in %s on line %d"#);
}

/// php-src `ext/dom/tests/DOMDocument_json_encode.phpt`: JSON encoding a DOMDocument
#[test]
fn test_domdocument_json_encode() {
    if skip_without_xml_native("test_domdocument_json_encode") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument;
echo json_encode($doc);
"#);
    assert_exact(&out, r#"{}"#);
}

/// php-src `ext/dom/tests/DOMDocument_length_cache_invalidation.phpt`: DOMDocument node list length cache invalidation
#[test]
fn test_domdocument_length_cache_invalidation() {
    if skip_without_xml_native("test_domdocument_length_cache_invalidation") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument();
$doc->loadHTML('<p>hello</p><p>world</p><p>!</p>');

$elements = $doc->getElementsByTagName('p');
$item = $elements->item(0); // Activate item cache
var_dump($elements->length); // Length not cached yet, should still compute
$item->remove();
// Now element 0 means "world", and 1 means "!"
unset($item);
var_dump($elements->length);
$item = $elements->item(1);
var_dump($item->textContent);
$item = $elements->item(1);
var_dump($item->textContent);
$item = $elements->item(0);
var_dump($item->textContent);
$item = $elements->item(1);
var_dump($item->textContent);

"#);
    assert_exact(&out, r#"int(3)
int(2)
string(1) "!"
string(1) "!"
string(5) "world"
string(1) "!""#);
}

/// php-src `ext/dom/tests/DOMDocument_liveness_caching_invalidation.phpt`: DOMDocument liveness caching invalidation by textContent
#[test]
#[ignore = r#"DOM port pending: Constructor 'DOMElement::__construct' expects 0 arguments, got 1"#]
fn test_domdocument_liveness_caching_invalidation() {
    if skip_without_xml_native("test_domdocument_liveness_caching_invalidation") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument;
$doc->loadXML('<root><e id="1"/><e id="2"/><e id="3"/><e id="4"/><e id="5"/></root>');
$root = $doc->documentElement;

$i = 0;

echo "-- Overwrite during iteration --\n";

foreach ($doc->getElementsByTagName('e') as $node) {
    if ($i++ == 2) {
        $root->textContent = 'overwrite';
    }
    var_dump($node->tagName, $node->getAttribute('id'));
}

echo "-- Empty iteration --\n";
foreach ($doc->getElementsByTagName('e') as $node) {
    echo "Should not execute\n";
}

echo "-- After adding an element again --\n";
$root->appendChild(new DOMElement('e'));
foreach ($doc->getElementsByTagName('e') as $node) {
    echo "Should execute once\n";
}
"#);
    assert_exact(&out, r#"-- Overwrite during iteration --
string(1) "e"
string(1) "1"
string(1) "e"
string(1) "2"
string(1) "e"
string(1) "3"
-- Empty iteration --
-- After adding an element again --
Should execute once"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadHTML_basic.phpt`: DOMDocument::loadHTML
#[test]
fn test_domdocument_load_html_basic() {
    if skip_without_xml_native("test_domdocument_load_html_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$doc->loadHTML("<html><body><p>Test<br></p></body></html>");
echo $doc->saveHTML();
"#);
    assert_exact(&out, r#"<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN" "http://www.w3.org/TR/REC-html40/loose.dtd">
<html><body><p>Test<br></p></body></html>"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadHTML_error2.phpt`: DOMDocument::loadHTML() should fail if empty string provided as input
#[test]
fn test_domdocument_load_html_error2() {
    if skip_without_xml_native("test_domdocument_load_html_error2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
try {
    $doc->loadHTML('');
} catch (ValueError $e) {
    echo $e->getMessage() . "\n";
}
"#);
    assert_exact(&out, r#"DOMDocument::loadHTML(): Argument #1 ($source) must not be empty"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadHTMLfile.phpt`: Test DOMDocument::loadHTMLFile
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::loadHTMLFile"#]
fn test_domdocument_load_htmlfile() {
    if skip_without_xml_native("test_domdocument_load_htmlfile") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$result = $doc->loadHTMLFile(__DIR__ . "/test.html");
assert($result === true);
"#);
    assert_exact(&out, r#""#);
}

/// php-src `ext/dom/tests/DOMDocument_loadHTMLfile_error1.phpt`: Test DOMDocument::loadHTMLFile when the file doesn't exist
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::loadHTMLFile"#]
fn test_domdocument_load_htmlfile_error1() {
    if skip_without_xml_native("test_domdocument_load_htmlfile_error1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$result = $doc->loadHTMLFile(__DIR__ . "/ffff/test.html");
assert($result === false);
"#);
    assert_format(&out, r#"%r(PHP ){0,1}%rWarning: DOMDocument::loadHTMLFile(): I/O %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadHTMLfile_error2.phpt`: Test DOMDocument::loadHTMLFile when an empty string is passed
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::loadHTMLFile"#]
fn test_domdocument_load_htmlfile_error2() {
    if skip_without_xml_native("test_domdocument_load_htmlfile_error2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
try {
    $result = $doc->loadHTMLFile("");
} catch (ValueError $e) {
    echo $e->getMessage() . "\n";
}

$doc = new DOMDocument();
try {
    $result = $doc->loadHTMLFile("text.html\0something");
} catch (ValueError $e) {
    echo $e->getMessage() . "\n";
}
"#);
    assert_exact(&out, r#"DOMDocument::loadHTMLFile(): Argument #1 ($filename) must not be empty
DOMDocument::loadHTMLFile(): Argument #1 ($filename) must not contain any null bytes"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadHTMLfile_variation1.phpt`: Test DOMDocument::loadHTMLFile when an empty document is loaded
#[test]
#[ignore = r#"php-src SKIPIF: skip libxml >= 2.14 no longer has this non-standard warning"#]
fn test_domdocument_load_htmlfile_variation1() {
    if skip_without_xml_native("test_domdocument_load_htmlfile_variation1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$result = $doc->loadHTMLFile(__DIR__ . "/empty.html");
assert($result === true);
"#);
    assert_format(&out, r#"%r(PHP ){0,1}%rWarning: DOMDocument::loadHTMLFile(): Document is empty %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadHTMLfile_variation2.phpt`: Test DOMDocument::loadHTMLFile when a not-well formed document is loaded
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::loadHTMLFile"#]
fn test_domdocument_load_htmlfile_variation2() {
    if skip_without_xml_native("test_domdocument_load_htmlfile_variation2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$result = $doc->loadHTMLFile(__DIR__ . "/not_well.html");
assert($result === true);
"#);
    assert_exact(&out, r#""#);
}

/// php-src `ext/dom/tests/DOMDocument_loadXML_basic.phpt`: Test DOMDocument::loadXML() basic behavior
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_xml_basic() {
    if skip_without_xml_native("test_domdocument_load_xml_basic") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/book.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"1"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));
$result = $doc->loadXML(file_get_contents(dirname(__FILE__) . getenv('XML_FILE')),
    $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_exact(&out, r#""#);
}

/// php-src `ext/dom/tests/DOMDocument_loadXML_error1.phpt`: Test DOMDocument::loadXML() detects not-well formed XML
#[test]
#[ignore = r#"php-src SKIPIF: skip libxml2 test variant for version < 2.12"#]
fn test_domdocument_load_xml_error1() {
    if skip_without_xml_native("test_domdocument_load_xml_error1") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));
$result = $doc->loadXML(file_get_contents(dirname(__FILE__) . getenv('XML_FILE')),
    $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load%r(XML){0,1}%r(): Opening and ending tag mismatch: title line 5 and book %s

Warning: DOMDocument::load%r(XML){0,1}%r(): %rexpected '>'|Opening and ending tag mismatch: book line (4|5) and books%r %s

Warning: DOMDocument::load%r(XML){0,1}%r(): %rPremature end of data in tag books|EndTag: '<\/' not found in Entity, line: 13%r %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadXML_error1_gte2_12.phpt`: Test DOMDocument::loadXML() detects not-well formed XML
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_xml_error1_gte2_12() {
    if skip_without_xml_native("test_domdocument_load_xml_error1_gte2_12") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));
$result = $doc->loadXML(file_get_contents(dirname(__FILE__) . getenv('XML_FILE')),
    $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load%r(XML){0,1}%r(): Opening and ending tag mismatch: title line 5 and book %s

Warning: DOMDocument::load%r(XML){0,1}%r(): %rexpected '>'|Opening and ending tag mismatch: book line (4|5) and books%r %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadXML_error2_gte2_11.phpt`: Test DOMDocument::loadXML() detects not-well formed XML
#[test]
#[ignore = r#"php-src SKIPIF: skip libxml2 test variant for version >= 2.11 && <= 2.12"#]
fn test_domdocument_load_xml_error2_gte2_11() {
    if skip_without_xml_native("test_domdocument_load_xml_error2_gte2_11") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed2.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));
$result = $doc->loadXML(file_get_contents(dirname(__FILE__) . getenv('XML_FILE')),
    $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::loadXML(): AttValue: " or ' expected in Entity, line: 4 in %s on line %d

Warning: DOMDocument::loadXML(): internal error: xmlParseStartTag: problem parsing attributes in Entity, line: 4 in %s on line %d

Warning: DOMDocument::loadXML(): Couldn't find end of Start Tag book line 4 in Entity, line: 4 in %s on line %d

Warning: DOMDocument::loadXML(): Opening and ending tag mismatch: books line 3 and book in Entity, line: 7 in %s on line %d

Warning: DOMDocument::loadXML(): Extra content at the end of the document in Entity, line: 8 in %s on line %d"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadXML_error2_gte2_12.phpt`: Test DOMDocument::loadXML() detects not-well formed XML
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_xml_error2_gte2_12() {
    if skip_without_xml_native("test_domdocument_load_xml_error2_gte2_12") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed2.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));
$result = $doc->loadXML(file_get_contents(dirname(__FILE__) . getenv('XML_FILE')),
    $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::loadXML(): AttValue: " or ' expected in Entity, line: 4 in %s on line %d

Warning: DOMDocument::loadXML():%sattributes%s

Warning: DOMDocument::loadXML(): Couldn't find end of Start Tag book line 4 in Entity, line: 4 in %s on line %d

Warning: DOMDocument::loadXML(): Opening and ending tag mismatch: books line 3 and book in Entity, line: 7 in %s on line %d"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadXML_error2_pre2_11.phpt`: Test DOMDocument::loadXML() detects not-well formed XML
#[test]
#[ignore = r#"php-src SKIPIF: skip libxml2 test variant for version < 2.11"#]
fn test_domdocument_load_xml_error2_pre2_11() {
    if skip_without_xml_native("test_domdocument_load_xml_error2_pre2_11") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed2.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));
$result = $doc->loadXML(file_get_contents(dirname(__FILE__) . getenv('XML_FILE')),
    $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load%r(XML){0,1}%r(): AttValue: " or ' expected %s

Warning: DOMDocument::load%r(XML){0,1}%r(): attributes construct error %s

Warning: DOMDocument::load%r(XML){0,1}%r(): Couldn't find end of Start Tag book %s

Warning: DOMDocument::load%r(XML){0,1}%r(): Opening and ending tag mismatch: books %s

Warning: DOMDocument::load%r(XML){0,1}%r(): Extra content at the end of the document %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadXML_error3.phpt`: Test DOMDocument::loadXML() detects not-well formed XML
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_xml_error3() {
    if skip_without_xml_native("test_domdocument_load_xml_error3") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed3.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));
$result = $doc->loadXML(file_get_contents(dirname(__FILE__) . getenv('XML_FILE')),
    $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load%r(XML){0,1}%r(): Opening and ending tag mismatch: boOk line 8 and book %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadXML_error4.phpt`: Test DOMDocument::loadXML() detects not-well formed XML
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_xml_error4() {
    if skip_without_xml_native("test_domdocument_load_xml_error4") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed4.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));
$result = $doc->loadXML(file_get_contents(dirname(__FILE__) . getenv('XML_FILE')),
    $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load%r(XML){0,1}%r(): Unsupported version '3.1' %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadXML_error5.phpt`: Test DOMDocument::loadXML() detects not-well formed XML
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_xml_error5() {
    if skip_without_xml_native("test_domdocument_load_xml_error5") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed5.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));
$result = $doc->loadXML(file_get_contents(dirname(__FILE__) . getenv('XML_FILE')),
    $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load%r(XML){0,1}%r(): Extra content at the end of the document %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadXML_error6.phpt`: Test DOMDocument::loadXML() with empty file path
#[test]
fn test_domdocument_load_xml_error6() {
    if skip_without_xml_native("test_domdocument_load_xml_error6") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
// create dom document
$dom = new DOMDocument();
try {
    $dom->loadXML("");
} catch (ValueError $exception) {
    echo $exception->getMessage() . "\n";
}
"#);
    assert_exact(&out, r#"DOMDocument::loadXML(): Argument #1 ($source) must not be empty"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadXML_variation1.phpt`: Test DOMDocument::loadXML() with LIBXML_DTDLOAD option
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_xml_variation1() {
    if skip_without_xml_native("test_domdocument_load_xml_variation1") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/book_with_dtd2.xml"#), (r#"LOAD_OPTIONS"#, r#"LIBXML_DTDLOAD"#), (r#"EXPECTED_RESULT"#, r#"1"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));
$result = $doc->loadXML(file_get_contents(dirname(__FILE__) . getenv('XML_FILE')),
    $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_exact(&out, r#""#);
}

/// php-src `ext/dom/tests/DOMDocument_loadXML_variation2.phpt`: Test DOMDocument::loadXML() with LIBXML_DTDVALID option
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_xml_variation2() {
    if skip_without_xml_native("test_domdocument_load_xml_variation2") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/wrong_book_with_dtd2.xml"#), (r#"LOAD_OPTIONS"#, r#"LIBXML_DTDVALID"#), (r#"EXPECTED_RESULT"#, r#"1"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));
$result = $doc->loadXML(file_get_contents(dirname(__FILE__) . getenv('XML_FILE')),
    $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load%r(XML){0,1}%r(): Element book content does not follow the DTD, expecting (title , author), got (title author author ) %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_loadXML_variation3.phpt`: Test DOMDocument::loadXML() with LIBXML_DTDVALID and LIBXML_NOERROR options
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_xml_variation3() {
    if skip_without_xml_native("test_domdocument_load_xml_variation3") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/wrong_book_with_dtd.xml"#), (r#"LOAD_OPTIONS"#, r#"LIBXML_DTDVALID | LIBXML_NOERROR"#), (r#"EXPECTED_RESULT"#, r#"1"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));
$result = $doc->loadXML(file_get_contents(dirname(__FILE__) . getenv('XML_FILE')),
    $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_exact(&out, r#""#);
}

/// php-src `ext/dom/tests/DOMDocument_loadXML_variation4.phpt`: Test DOMDocument::loadXML() with LIBXML_DTDATTR, LIBXML_NOCDATA, LIBXML_NOENT, LIBXML_NOBLANKS
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_xml_variation4() {
    if skip_without_xml_native("test_domdocument_load_xml_variation4") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/book_with_dtd2.xml"#), (r#"LOAD_OPTIONS"#, r#"LIBXML_DTDATTR|LIBXML_NOCDATA|LIBXML_NOENT|LIBXML_NOBLANKS"#), (r#"EXPECTED_RESULT"#, r#"1"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));
$result = $doc->loadXML(file_get_contents(dirname(__FILE__) . getenv('XML_FILE')),
    $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);

echo $doc->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE books [
<!ENTITY entitest "entity is only for test purposes">
<!ATTLIST title default CDATA "default title">
<!ELEMENT books (book)*>
<!ELEMENT book (title , author)>
<!ELEMENT title (#PCDATA)>
<!ELEMENT author (#PCDATA)>
]>
<books><book><title default="default title">The Grapes of Wrath</title><author>John Steinbeck</author></book><book><title default="default title">The Pearl</title><author>John Steinbeck</author></book><book><title default="default title">entity is only for test purposes</title><author>data for test</author></book></books>"#);
}

/// php-src `ext/dom/tests/DOMDocument_load_basic.phpt`: Test DOMDocument::load() basic behavior
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_basic() {
    if skip_without_xml_native("test_domdocument_load_basic") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/book.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"1"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));

$result = $doc->load(dirname(__FILE__) . getenv('XML_FILE'), $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_exact(&out, r#""#);
}

/// php-src `ext/dom/tests/DOMDocument_load_error1.phpt`: Test DOMDocument::load() detects not-well formed XML
#[test]
#[ignore = r#"php-src SKIPIF: skip libxml2 test variant for version < 2.12"#]
fn test_domdocument_load_error1() {
    if skip_without_xml_native("test_domdocument_load_error1") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));

$result = $doc->load(dirname(__FILE__) . getenv('XML_FILE'), $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load%r(XML){0,1}%r(): Opening and ending tag mismatch: title line 5 and book %s

Warning: DOMDocument::load%r(XML){0,1}%r(): %rexpected '>'|Opening and ending tag mismatch: book line (4|5) and books%r %s

Warning: DOMDocument::load%r(XML){0,1}%r(): %rPremature end of data in tag books|EndTag: '<\/' not found%r %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_load_error1_gte2_12.phpt`: Test DOMDocument::load() detects not-well formed XML
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_error1_gte2_12() {
    if skip_without_xml_native("test_domdocument_load_error1_gte2_12") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));

$result = $doc->load(dirname(__FILE__) . getenv('XML_FILE'), $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load%r(XML){0,1}%r(): Opening and ending tag mismatch: title line 5 and book %s

Warning: DOMDocument::load%r(XML){0,1}%r(): %rexpected '>'|Opening and ending tag mismatch: book line (4|5) and books%r %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_load_error2_gte2_11.phpt`: Test DOMDocument::load() detects not-well formed
#[test]
#[ignore = r#"php-src SKIPIF: skip libxml2 test variant for version >= 2.11 && <= 2.12"#]
fn test_domdocument_load_error2_gte2_11() {
    if skip_without_xml_native("test_domdocument_load_error2_gte2_11") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed2.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));

$result = $doc->load(dirname(__FILE__) . getenv('XML_FILE'), $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load(): AttValue: " or ' expected in %s on line %d

Warning: DOMDocument::load(): internal error: xmlParseStartTag: problem parsing attributes in %s on line %d

Warning: DOMDocument::load(): Couldn't find end of Start Tag book line 4 in %s on line %d

Warning: DOMDocument::load(): Opening and ending tag mismatch: books line 3 and book in %s on line %d

Warning: DOMDocument::load(): Extra content at the end of the document in %s on line %d"#);
}

/// php-src `ext/dom/tests/DOMDocument_load_error2_gte2_12.phpt`: Test DOMDocument::load() detects not-well formed
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_error2_gte2_12() {
    if skip_without_xml_native("test_domdocument_load_error2_gte2_12") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed2.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));

$result = $doc->load(dirname(__FILE__) . getenv('XML_FILE'), $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load(): AttValue: " or ' expected in %s on line %d

Warning: DOMDocument::load():%sattributes%s

Warning: DOMDocument::load(): Couldn't find end of Start Tag book line 4 in %s on line %d

Warning: DOMDocument::load(): Opening and ending tag mismatch: books line 3 and book in %s on line %d"#);
}

/// php-src `ext/dom/tests/DOMDocument_load_error2_pre2_11.phpt`: Test DOMDocument::load() detects not-well formed XML
#[test]
#[ignore = r#"php-src SKIPIF: skip libxml2 test variant for version < 2.11"#]
fn test_domdocument_load_error2_pre2_11() {
    if skip_without_xml_native("test_domdocument_load_error2_pre2_11") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed2.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));

$result = $doc->load(dirname(__FILE__) . getenv('XML_FILE'), $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load%r(XML){0,1}%r(): AttValue: " or ' expected %s

Warning: DOMDocument::load%r(XML){0,1}%r(): attributes construct error %s

Warning: DOMDocument::load%r(XML){0,1}%r(): Couldn't find end of Start Tag book %s

Warning: DOMDocument::load%r(XML){0,1}%r(): Opening and ending tag mismatch: books %s

Warning: DOMDocument::load%r(XML){0,1}%r(): Extra content at the end of the document %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_load_error3.phpt`: Test DOMDocument::load() detects not-well formed XML
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_error3() {
    if skip_without_xml_native("test_domdocument_load_error3") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed3.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));

$result = $doc->load(dirname(__FILE__) . getenv('XML_FILE'), $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load%r(XML){0,1}%r(): Opening and ending tag mismatch: boOk line 8 and book %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_load_error4.phpt`: Test DOMDocument::load() detects not-well formed XML
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_error4() {
    if skip_without_xml_native("test_domdocument_load_error4") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed4.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));

$result = $doc->load(dirname(__FILE__) . getenv('XML_FILE'), $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load%r(XML){0,1}%r(): Unsupported version '3.1' %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_load_error5.phpt`: Test DOMDocument::load() detects not-well formed XML
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_error5() {
    if skip_without_xml_native("test_domdocument_load_error5") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/not_well_formed5.xml"#), (r#"LOAD_OPTIONS"#, r#"0"#), (r#"EXPECTED_RESULT"#, r#"0"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));

$result = $doc->load(dirname(__FILE__) . getenv('XML_FILE'), $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load%r(XML){0,1}%r(): Extra content at the end of the document %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_load_error6.phpt`: Test DOMDocument::load() with invalid paths
#[test]
#[ignore = r#"DOM port pending: output differs from php-src"#]
fn test_domdocument_load_error6() {
    if skip_without_xml_native("test_domdocument_load_error6") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
// create dom document
$dom = new DOMDocument();
try {
    $dom->load("");
} catch (ValueError $exception) {
    echo $exception->getMessage() . "\n";
}

try {
    $dom->load("/path/with/\0/byte");
} catch (ValueError $exception) {
    echo $exception->getMessage() . "\n";
}

// Path is too long
var_dump($dom->load(str_repeat(" ", PHP_MAXPATHLEN + 1)));
"#);
    assert_exact(&out, r#"DOMDocument::load(): Argument #1 ($filename) must not be empty
DOMDocument::load(): Argument #1 ($filename) must not contain any null bytes
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMDocument_load_variation1.phpt`: Test DOMDocument::load() with LIBXML_DTDLOAD option
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_variation1() {
    if skip_without_xml_native("test_domdocument_load_variation1") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/book_with_dtd.xml"#), (r#"LOAD_OPTIONS"#, r#"LIBXML_DTDLOAD"#), (r#"EXPECTED_RESULT"#, r#"1"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));

$result = $doc->load(dirname(__FILE__) . getenv('XML_FILE'), $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_exact(&out, r#""#);
}

/// php-src `ext/dom/tests/DOMDocument_load_variation2.phpt`: Test DOMDocument::load() with LIBXML_DTDVALID option
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_variation2() {
    if skip_without_xml_native("test_domdocument_load_variation2") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/wrong_book_with_dtd.xml"#), (r#"LOAD_OPTIONS"#, r#"LIBXML_DTDVALID"#), (r#"EXPECTED_RESULT"#, r#"1"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));

$result = $doc->load(dirname(__FILE__) . getenv('XML_FILE'), $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_format(&out, r#"Warning: DOMDocument::load%r(XML){0,1}%r(): Element book content does not follow the DTD, expecting (title , author), got (title author author ) %s"#);
}

/// php-src `ext/dom/tests/DOMDocument_load_variation3.phpt`: Test DOMDocument::load() with LIBXML_DTDVALID and LIBXML_NOERROR options
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_variation3() {
    if skip_without_xml_native("test_domdocument_load_variation3") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/wrong_book_with_dtd.xml"#), (r#"LOAD_OPTIONS"#, r#"LIBXML_DTDVALID | LIBXML_NOERROR"#), (r#"EXPECTED_RESULT"#, r#"1"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));

$result = $doc->load(dirname(__FILE__) . getenv('XML_FILE'), $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);
"#);
    assert_exact(&out, r#""#);
}

/// php-src `ext/dom/tests/DOMDocument_load_variation4.phpt`: Test DOMDocument::load() with LIBXML_DTDATTR, LIBXML_NOCDATA, LIBXML_NOENT, LIBXML_NOBLANKS
#[test]
#[ignore = r#"DOM port pending: array_intersect_key() first argument must be array"#]
fn test_domdocument_load_variation4() {
    if skip_without_xml_native("test_domdocument_load_variation4") {
        return;
    }
    let out = run_case(r#""#, &[(r#"XML_FILE"#, r#"/book_with_dtd.xml"#), (r#"LOAD_OPTIONS"#, r#"LIBXML_DTDATTR|LIBXML_NOCDATA|LIBXML_NOENT|LIBXML_NOBLANKS"#), (r#"EXPECTED_RESULT"#, r#"1"#)], r#"<?php
include(dirname(__FILE__) . '/domdocumentload_utilities.inc');

$doc = new DOMDocument();

$libxml_options = libxml_options_to_int(getenv('LOAD_OPTIONS'));

$result = $doc->load(dirname(__FILE__) . getenv('XML_FILE'), $libxml_options);

$expectedResult = (bool) getenv('EXPECTED_RESULT');
assert($result === $expectedResult);

echo $doc->saveXML();
"#);
    assert_exact(&out, r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE books SYSTEM "books.dtd">
<books><book><title default="default title">The Grapes of Wrath</title><author>John Steinbeck</author></book><book><title default="default title">The Pearl</title><author>John Steinbeck</author></book><book><title default="default title">entity is only for test purposes</title><author>data for test</author></book></books>"#);
}

/// php-src `ext/dom/tests/DOMDocument_preserveWhiteSpace_basic.phpt`: DOMDocument::$preserveWhiteSpace - test ability to read and write property
#[test]
fn test_domdocument_preserve_white_space_basic() {
    if skip_without_xml_native("test_domdocument_preserve_white_space_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;
$doc->load(__DIR__."/book.xml");

var_dump($doc->preserveWhiteSpace);

$doc->preserveWhiteSpace = false;
var_dump($doc->preserveWhiteSpace);

"#);
    assert_exact(&out, r#"bool(true)
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMDocument_preserveWhiteSpace_variations.phpt`: DOMDocument::$preserveWhiteSpace - test ability to read and write property
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::preserveWhiteSpace"#]
fn test_domdocument_preserve_white_space_variations() {
    if skip_without_xml_native("test_domdocument_preserve_white_space_variations") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

echo "Load document with preserveWhiteSpace on\n";
$doc = new DOMDocument;
$doc->load(__DIR__."/book.xml");
echo $doc->saveXML();


echo "\nLoad document with preserveWhiteSpace off\n";
$doc = new DOMDocument;
$doc->preserveWhiteSpace = false;
$doc->load(__DIR__."/book.xml");
echo $doc->saveXML();

"#);
    assert_exact(&out, r#"Load document with preserveWhiteSpace on
<?xml version="1.0"?>
<books>
 <book>
  <title>The Grapes of Wrath</title>
  <author>John Steinbeck</author>
 </book>
 <book>
  <title>The Pearl</title>
  <author>John Steinbeck</author>
 </book>
</books>

Load document with preserveWhiteSpace off
<?xml version="1.0"?>
<books><book><title>The Grapes of Wrath</title><author>John Steinbeck</author></book><book><title>The Pearl</title><author>John Steinbeck</author></book></books>"#);
}

/// php-src `ext/dom/tests/DOMDocument_recover_write.phpt`: DOMDocument::$recover write
#[test]
fn test_domdocument_recover_write() {
    if skip_without_xml_native("test_domdocument_recover_write") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$dom = new DOMDocument;
var_dump($dom->recover);
$dom->recover = true;
var_dump($dom->recover);
echo $dom->saveXML();
"#);
    assert_exact(&out, r#"bool(false)
bool(true)
<?xml version="1.0"?>"#);
}

/// php-src `ext/dom/tests/DOMDocument_relaxNGValidateSource_basic.phpt`: DOMDocument::relaxNGValidateSource()
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::relaxNGValidateSource"#]
fn test_domdocument_relax_ngvalidate_source_basic() {
    if skip_without_xml_native("test_domdocument_relax_ngvalidate_source_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$rng = <<< RNG
<?xml version="1.0" encoding="UTF-8"?>
<grammar ns="" xmlns="http://relaxng.org/ns/structure/1.0"
  datatypeLibrary="http://www.w3.org/2001/XMLSchema-datatypes">
  <start>
    <element name="apple">
      <element name="pear">
        <data type="NCName"/>
      </element>
    </element>
  </start>
</grammar>
RNG;

$good_xml = <<< GOOD_XML
<?xml version="1.0"?>
<apple>
  <pear>Pear</pear>
</apple>
GOOD_XML;

$doc = new DOMDocument();
$doc->loadXML($good_xml);
$result = $doc->relaxNGValidateSource($rng);
var_dump($result);

"#);
    assert_exact(&out, r#"bool(true)"#);
}

/// php-src `ext/dom/tests/DOMDocument_relaxNGValidateSource_error1.phpt`: DOMDocument::relaxNGValidateSource() should fail if document doesn't validate
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::relaxNGValidateSource"#]
fn test_domdocument_relax_ngvalidate_source_error1() {
    if skip_without_xml_native("test_domdocument_relax_ngvalidate_source_error1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$rng = <<< RNG
<?xml version="1.0" encoding="UTF-8"?>
<grammar ns="" xmlns="http://relaxng.org/ns/structure/1.0"
  datatypeLibrary="http://www.w3.org/2001/XMLSchema-datatypes">
  <start>
    <element name="apple">
      <element name="pear">
        <data type="NCName"/>
      </element>
    </element>
  </start>
</grammar>
RNG;

$bad_xml = <<< BAD_XML
<?xml version="1.0"?>
<apple>
  <pear>Pear</pear>
  <pear>Pear</pear>
</apple>
BAD_XML;

$doc = new DOMDocument();
$doc->loadXML($bad_xml);
$result = $doc->relaxNGValidateSource($rng);
var_dump($result);

"#);
    assert_format(&out, r#"Warning: DOMDocument::relaxNGValidateSource(): Did not expect element pear there in %s on line %d
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMDocument_relaxNGValidateSource_error2.phpt`: DOMDocument::relaxNGValidateSource() should fail on invalid RNG schema
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::relaxNGValidateSource"#]
fn test_domdocument_relax_ngvalidate_source_error2() {
    if skip_without_xml_native("test_domdocument_relax_ngvalidate_source_error2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$rng = <<< RNG
<?xml version="1.0" encoding="UTF-8"?>
<grammar ns="" xmlns="http://relaxng.org/ns/structure/1.0"
  datatypeLibrary="http://www.w3.org/2001/XMLSchema-datatypes">
  <start>
    <element name="apple">
    </element>
  </start>
</grammar>
RNG;

$xml = <<< XML
<?xml version="1.0"?>
<apple>
  <pear>Pear</pear>
</apple>
XML;

$doc = new DOMDocument();
$doc->loadXML($xml);
$result = $doc->relaxNGValidateSource($rng);
var_dump($result);

"#);
    assert_format(&out, r#"Warning: DOMDocument::relaxNGValidateSource(): xmlRelaxNGParseElement: element has no content in %s on line %d

Warning: DOMDocument::relaxNGValidateSource(): Invalid RelaxNG in %s on line %d
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMDocument_relaxNGValidate_basic.phpt`: DOMDocument::relaxNGValidate()
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::relaxNGValidate"#]
fn test_domdocument_relax_ngvalidate_basic() {
    if skip_without_xml_native("test_domdocument_relax_ngvalidate_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$rng = __DIR__.'/DOMDocument_relaxNGValidate_basic.rng';
$xml = <<< XML
<?xml version="1.0"?>
<apple>
  <pear>Pear</pear>
</apple>
XML;
$doc = new DOMDocument();
$doc->loadXML($xml);
$result = $doc->relaxNGValidate($rng);
var_dump($result);
"#);
    assert_exact(&out, r#"bool(true)"#);
}

/// php-src `ext/dom/tests/DOMDocument_relaxNGValidate_error1.phpt`: DOMDocument::relaxNGValidate() should fail if document doesn't validate
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::relaxNGValidate"#]
fn test_domdocument_relax_ngvalidate_error1() {
    if skip_without_xml_native("test_domdocument_relax_ngvalidate_error1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$rng = __DIR__.'/DOMDocument_relaxNGValidate_basic.rng';
$xml = <<< XML
<?xml version="1.0"?>
<apple>
  <pear>Pear</pear>
  <pear>Pear</pear>
</apple>
XML;
$doc = new DOMDocument();
$doc->loadXML($xml);
$result = $doc->relaxNGValidate($rng);
var_dump($result);
"#);
    assert_format(&out, r#"Warning: DOMDocument::relaxNGValidate(): Did not expect element pear there in %s on line %d
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMDocument_relaxNGValidate_error2.phpt`: DOMDocument::relaxNGValidate() should fail on invalid RelaxNG file source
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::relaxNGValidate"#]
fn test_domdocument_relax_ngvalidate_error2() {
    if skip_without_xml_native("test_domdocument_relax_ngvalidate_error2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$rng = __DIR__.'/foo.rng';
$xml = <<< XML
<?xml version="1.0"?>
<apple>
  <pear>Pear</pear>
  <pear>Pear</pear>
</apple>
XML;
$doc = new DOMDocument();
$doc->loadXML($xml);
$result = $doc->relaxNGValidate($rng);
var_dump($result);
"#);
    assert_format(&out, r#"Warning: DOMDocument::relaxNGValidate(): I/O %s : failed to load %s

Warning: DOMDocument::relaxNGValidate(): xmlRelaxNGParse: could not load %s/foo.rng in %s on line %d

Warning: DOMDocument::relaxNGValidate(): Invalid RelaxNG in %s on line %d
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMDocument_resolveExternals_basic.phpt`: Tests DOMDocument::resoleExternals get and set
#[test]
fn test_domdocument_resolve_externals_basic() {
    if skip_without_xml_native("test_domdocument_resolve_externals_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
// create dom document
$dom = new DOMDocument;
$xml = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN"
 "http://www.w3.org/TR/html4/strict.dtd">
<h1>&quot;Foo&quot;</h1>';
$dom->loadXML($xml);
if(!$dom) {
  echo "Error while parsing the document\n";
  exit(0);
}
echo "DOMDocument with external entities created\n";

$test = $dom->resolveExternals;
echo "Read initial resolveExternals:\n";
var_dump( $test );

$dom->resolveExternals = TRUE;
$test = $dom->resolveExternals;
echo "Set resolveExternals to TRUE, reading again:\n";
var_dump( $test );

/**
 * Don't bother testing the resolveExternals functionality here, it throws warnings on html dtd
 *
echo "Reloading xml with resolveExternals turned on\n";
$dom->loadXML($xml);
$test = $dom->saveXML();
var_dump( $test );
 */

echo "Done";
"#);
    assert_exact(&out, r#"DOMDocument with external entities created
Read initial resolveExternals:
bool(false)
Set resolveExternals to TRUE, reading again:
bool(true)
Done"#);
}

/// php-src `ext/dom/tests/DOMDocument_saveHTMLFile_basic.phpt`: DOMDocument::saveHTMLFile() should dump the internal document into a file using HTML formatting
#[test]
#[ignore = r#"php-src SKIPIF: skip see https://gitlab.gnome.org/GNOME/libxml2/-/issues/756"#]
fn test_domdocument_save_htmlfile_basic() {
    if skip_without_xml_native("test_domdocument_save_htmlfile_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$filename = __DIR__."/DOMDocument_saveHTMLFile_basic.html";
$doc = new DOMDocument('1.0');
$root = $doc->createElement('html');
$root = $doc->appendChild($root);
$head = $doc->createElement('head');
$head = $root->appendChild($head);
$title = $doc->createElement('title');
$title = $head->appendChild($title);
$text = $doc->createTextNode('This is the title');
$text = $title->appendChild($text);
$bytes = $doc->saveHTMLFile($filename);
var_dump($bytes);
echo file_get_contents($filename);
unlink($filename);
"#);
    assert_exact(&out, r#"int(126)
<html><head><meta http-equiv="Content-Type" content="text/html; charset=UTF-8"><title>This is the title</title></head></html>"#);
}

/// php-src `ext/dom/tests/DOMDocument_saveHTMLFile_error2.phpt`: DOMDocument::saveHTMLFile() should fail if called statically
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::saveHTMLFile"#]
fn test_domdocument_save_htmlfile_error2() {
    if skip_without_xml_native("test_domdocument_save_htmlfile_error2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
try {
    DOMDocument::saveHTMLFile();
} catch (Error $e) {
    echo $e->getMessage();
}
"#);
    assert_exact(&out, r#"Non-static method DOMDocument::saveHTMLFile() cannot be called statically"#);
}

/// php-src `ext/dom/tests/DOMDocument_saveHTMLFile_formatOutput.phpt`: DOMDocument::saveHTMLFile() should format output on demand
#[test]
#[ignore = r#"php-src SKIPIF: skip see https://gitlab.gnome.org/GNOME/libxml2/-/issues/756"#]
fn test_domdocument_save_htmlfile_format_output() {
    if skip_without_xml_native("test_domdocument_save_htmlfile_format_output") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$filename = __DIR__."/DOMDocument_saveHTMLFile_formatOutput.html";
$doc = new DOMDocument('1.0');
$doc->formatOutput = true;
$root = $doc->createElement('html');
$root = $doc->appendChild($root);
$head = $doc->createElement('head');
$head = $root->appendChild($head);
$title = $doc->createElement('title');
$title = $head->appendChild($title);
$text = $doc->createTextNode('This is the title');
$text = $title->appendChild($text);
$bytes = $doc->saveHTMLFile($filename);
var_dump($bytes);
echo file_get_contents($filename);
unlink($filename);
"#);
    assert_exact(&out, r#"int(129)
<html><head>
<meta http-equiv="Content-Type" content="text/html; charset=UTF-8">
<title>This is the title</title>
</head></html>"#);
}

/// php-src `ext/dom/tests/DOMDocument_saveHTMLFile_formatOutput_gte_2_13.phpt`: DOMDocument::saveHTMLFile() should format output on demand
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::formatOutput"#]
fn test_domdocument_save_htmlfile_format_output_gte_2_13() {
    if skip_without_xml_native("test_domdocument_save_htmlfile_format_output_gte_2_13") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$filename = __DIR__."/DOMDocument_saveHTMLFile_formatOutput_gte_2_13.html";
$doc = new DOMDocument('1.0');
$doc->formatOutput = true;
$root = $doc->createElement('html');
$root = $doc->appendChild($root);
$head = $doc->createElement('head');
$head = $root->appendChild($head);
$title = $doc->createElement('title');
$title = $head->appendChild($title);
$text = $doc->createTextNode('This is the title');
$text = $title->appendChild($text);
$bytes = $doc->saveHTMLFile($filename);
var_dump($bytes);
echo file_get_contents($filename);
unlink($filename);
"#);
    assert_exact(&out, r#"int(59)
<html><head><title>This is the title</title></head></html>"#);
}

/// php-src `ext/dom/tests/DOMDocument_saveHTMLFile_invalid_filename.phpt`: DOMDocument::saveHTMLFile() should fail with invalid filename
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createTextNode"#]
fn test_domdocument_save_htmlfile_invalid_filename() {
    if skip_without_xml_native("test_domdocument_save_htmlfile_invalid_filename") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$filename = '';
$doc = new DOMDocument('1.0');
$root = $doc->createElement('html');
$root = $doc->appendChild($root);
$head = $doc->createElement('head');
$head = $root->appendChild($head);
$title = $doc->createElement('title');
$title = $head->appendChild($title);
$text = $doc->createTextNode('This is the title');
$text = $title->appendChild($text);
try {
    $doc->saveHTMLFile($filename);
} catch (ValueError $exception) {
    echo $exception->getMessage() . "\n";
}
"#);
    assert_exact(&out, r#"DOMDocument::saveHTMLFile(): Argument #1 ($filename) must not be empty"#);
}

/// php-src `ext/dom/tests/DOMDocument_saveHTML_basic.phpt`: DOMDocument::saveHTML() should dump the internal document into a string using HTML formatting
#[test]
fn test_domdocument_save_html_basic() {
    if skip_without_xml_native("test_domdocument_save_html_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument('1.0');
$root = $doc->createElement('html');
$root = $doc->appendChild($root);
$head = $doc->createElement('head');
$head = $root->appendChild($head);
$title = $doc->createElement('title');
$title = $head->appendChild($title);
$text = $doc->createTextNode('This is the title');
$text = $title->appendChild($text);
echo $doc->saveHTML();
"#);
    assert_exact(&out, r#"<html><head><title>This is the title</title></head></html>"#);
}

/// php-src `ext/dom/tests/DOMDocument_saveHTML_basic_gte_2_13.phpt`: DOMDocument::saveHTMLFile() should dump the internal document into a file using HTML formatting
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::createTextNode"#]
fn test_domdocument_save_html_basic_gte_2_13() {
    if skip_without_xml_native("test_domdocument_save_html_basic_gte_2_13") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$filename = __DIR__."/DOMDocument_saveHTMLFile_basic_gte_2_13.html";
$doc = new DOMDocument('1.0');
$root = $doc->createElement('html');
$root = $doc->appendChild($root);
$head = $doc->createElement('head');
$head = $root->appendChild($head);
$title = $doc->createElement('title');
$title = $head->appendChild($title);
$text = $doc->createTextNode('This is the title');
$text = $title->appendChild($text);
$bytes = $doc->saveHTMLFile($filename);
var_dump($bytes);
echo file_get_contents($filename);
unlink($filename);
"#);
    assert_exact(&out, r#"int(59)
<html><head><title>This is the title</title></head></html>"#);
}

/// php-src `ext/dom/tests/DOMDocument_saveHTML_error2.phpt`: DOMDocument::saveHTML() should fail if called statically
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::saveHTML"#]
fn test_domdocument_save_html_error2() {
    if skip_without_xml_native("test_domdocument_save_html_error2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
try {
    DOMDocument::saveHTML(true);
} catch (Error $e) {
    echo $e->getMessage();
}
"#);
    assert_exact(&out, r#"Non-static method DOMDocument::saveHTML() cannot be called statically"#);
}

/// php-src `ext/dom/tests/DOMDocument_saveHTML_variant1.phpt`: DOMDocument::saveHTML() optional parameters
#[test]
fn test_domdocument_save_html_variant1() {
    if skip_without_xml_native("test_domdocument_save_html_variant1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument('1.0');
$root = $doc->createElement('html');
$root = $doc->appendChild($root);
$head = $doc->createElement('head');
$head = $root->appendChild($head);
$title = $doc->createElement('title');
$title = $head->appendChild($title);
$text = $doc->createTextNode('This is the title');
$text = $title->appendChild($text);
echo $doc->saveHTML(NULL), "\n";
echo $doc->saveHTML($title), "\n";
"#);
    assert_exact(&out, r#"<html><head><title>This is the title</title></head></html>

<title>This is the title</title>"#);
}

/// php-src `ext/dom/tests/DOMDocument_saveHTML_variant2.phpt`: DOMDocument::saveHTML() vs DOMDocument::saveXML()
#[test]
fn test_domdocument_save_html_variant2() {
    if skip_without_xml_native("test_domdocument_save_html_variant2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$d = new DOMDocument();
$str = <<<EOD
<html>
<head>
</head>
<body>
<p>Hi.<br/>there</p>
</body>
</html>
EOD;
$d->loadHTML($str);
$e = $d->getElementsByTagName("p");
$e = $e->item(0);
echo $d->saveXml($e),"\n";
echo $d->saveHtml($e),"\n";
"#);
    assert_exact(&out, r#"<p>Hi.<br/>there</p>
<p>Hi.<br>there</p>"#);
}

/// php-src `ext/dom/tests/DOMDocument_saveXML_XML_SAVE_NO_DECL.phpt`: DOMDocument::saveXML(): XML_SAVE_NO_DECL
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::encoding"#]
fn test_domdocument_save_xml_xml_save_no_decl() {
    if skip_without_xml_native("test_domdocument_save_xml_xml_save_no_decl") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument();
$doc->loadXML('<root>é</root>');

echo $doc->saveXML(options: 0);
echo $doc->saveXML(options: LIBXML_NOXMLDECL);
// Explicit encoding test, to ensure no encoding declaration.
$doc->encoding = "UTF-8";
echo $doc->saveXML(options: LIBXML_NOXMLDECL);

// Edge case
$doc = new DOMDocument();
var_dump($doc->saveXML(options: LIBXML_NOXMLDECL));
"#);
    assert_exact(&out, r#"<?xml version="1.0"?>
<root>&#xE9;</root>
<root>&#xE9;</root>
<root>é</root>
string(0) """#);
}

/// php-src `ext/dom/tests/DOMDocument_save_basic.phpt`: DOMDocument::save Test basic function of save method
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::formatOutput"#]
fn test_domdocument_save_basic() {
    if skip_without_xml_native("test_domdocument_save_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$doc = new DOMDocument('1.0');
$doc->formatOutput = true;

$root = $doc->createElement('book');

$root = $doc->appendChild($root);

$title = $doc->createElement('title');
$title = $root->appendChild($title);

$text = $doc->createTextNode('This is the title');
$text = $title->appendChild($text);

$temp_filename = __DIR__."/DomDocument_save_basic.tmp";

echo 'Wrote: ' . $doc->save($temp_filename) . ' bytes'; // Wrote: 72 bytes;"#);
    assert_exact(&out, r#"Wrote: 72 bytes"#);
}

/// php-src `ext/dom/tests/DOMDocument_savexml_basic.phpt`: DOM Document : save and saveXML
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::save"#]
fn test_domdocument_savexml_basic() {
    if skip_without_xml_native("test_domdocument_savexml_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$xml = <<< EOXML
<?xml version="1.0" encoding="utf-8"?>
<courses>
    <!-- Hello World! -->
    <aNode>
        <childNode>
            <childlessNode />
        </childNode>
    </aNode>
</courses>
EOXML;

$dom = new DOMDocument();
$dom->loadXML($xml);
$root = $dom->documentElement;
$directory = __DIR__;

$filename = $directory."/tmp_dom_savexml".time();
var_dump($dom->save($filename));
$result = file_get_contents($filename);
var_dump($result == $dom->saveXML());

unlink($filename);
"#);
    assert_exact(&out, r#"int(181)
bool(true)"#);
}

/// php-src `ext/dom/tests/DOMDocument_schemaValidateSource_addAttrs.phpt`: DomDocument::schemaValidateSource() - Add missing attribute default values from schema
#[test]
#[ignore = r#"DOM port pending: Undefined constant: LIBXML_SCHEMA_CREATE"#]
fn test_domdocument_schema_validate_source_add_attrs() {
    if skip_without_xml_native("test_domdocument_schema_validate_source_add_attrs") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->load(__DIR__."/book-attr.xml");

$xsd = file_get_contents(__DIR__."/book.xsd");

$doc->schemaValidateSource($xsd, LIBXML_SCHEMA_CREATE);

foreach ($doc->getElementsByTagName('book') as $book) {
    var_dump($book->getAttribute('is-hardback'));
}

"#);
    assert_exact(&out, r#"string(5) "false"
string(4) "true""#);
}

/// php-src `ext/dom/tests/DOMDocument_schemaValidateSource_basic.phpt`: DomDocument::schemaValidateSource() - basic
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::schemaValidateSource"#]
fn test_domdocument_schema_validate_source_basic() {
    if skip_without_xml_native("test_domdocument_schema_validate_source_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->load(__DIR__."/book.xml");

$xsd = file_get_contents(__DIR__."/book.xsd");

$result = $doc->schemaValidateSource($xsd);
var_dump($result);

"#);
    assert_exact(&out, r#"bool(true)"#);
}

/// php-src `ext/dom/tests/DOMDocument_schemaValidateSource_error1.phpt`: DomDocument::schemaValidateSource() - string that is not a schema
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::schemaValidateSource"#]
fn test_domdocument_schema_validate_source_error1() {
    if skip_without_xml_native("test_domdocument_schema_validate_source_error1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->load(__DIR__."/book.xml");

$result = $doc->schemaValidateSource('string that is not a schema');
var_dump($result);

"#);
    assert_format(&out, r#"Warning: DOMDocument::schemaValidateSource(): Entity: line 1: parser error : Start tag expected, '<' not found in %s on line %d

Warning: DOMDocument::schemaValidateSource(): string that is not a schema in %s on line %d

Warning: DOMDocument::schemaValidateSource(): ^ in %s on line %d

Warning: DOMDocument::schemaValidateSource(): Failed to parse the XML resource 'in_memory_buffer'. in %s on line %d

Warning: DOMDocument::schemaValidateSource(): Invalid Schema in %s on line %d
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMDocument_schemaValidateSource_error2.phpt`: DomDocument::schemaValidateSource() - non-conforming schema
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::schemaValidateSource"#]
fn test_domdocument_schema_validate_source_error2() {
    if skip_without_xml_native("test_domdocument_schema_validate_source_error2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->load(__DIR__."/book.xml");

$xsd = file_get_contents(__DIR__."/book-non-conforming-schema.xsd");

$result = $doc->schemaValidateSource($xsd);
var_dump($result);

"#);
    assert_format(&out, r#"Warning: DOMDocument::schemaValidateSource(): Element 'books': No matching global declaration available for the validation root. in %s on line %d
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMDocument_schemaValidateSource_error3.phpt`: DomDocument::schemaValidateSource() - empty string for schema string
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::schemaValidateSource"#]
fn test_domdocument_schema_validate_source_error3() {
    if skip_without_xml_native("test_domdocument_schema_validate_source_error3") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->load(__DIR__."/book.xml");

try {
    $doc->schemaValidateSource('');
} catch (ValueError $e) {
    echo $e->getMessage() . "\n";
}

"#);
    assert_exact(&out, r#"DOMDocument::schemaValidateSource(): Argument #1 ($source) must not be empty"#);
}

/// php-src `ext/dom/tests/DOMDocument_schemaValidateSource_missingAttrs.phpt`: DomDocument::schemaValidateSource() - Don't add missing attribute default values from schema
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::schemaValidateSource"#]
fn test_domdocument_schema_validate_source_missing_attrs() {
    if skip_without_xml_native("test_domdocument_schema_validate_source_missing_attrs") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->load(__DIR__."/book-attr.xml");

$xsd = file_get_contents(__DIR__."/book.xsd");

$doc->schemaValidateSource($xsd);

foreach ($doc->getElementsByTagName('book') as $book) {
    var_dump($book->getAttribute('is-hardback'));
}

"#);
    assert_exact(&out, r#"string(0) ""
string(4) "true""#);
}

/// php-src `ext/dom/tests/DOMDocument_schemaValidate_addAttrs.phpt`: DomDocument::schemaValidate() - Add missing attribute default values from schema
#[test]
#[ignore = r#"DOM port pending: Undefined constant: LIBXML_SCHEMA_CREATE"#]
fn test_domdocument_schema_validate_add_attrs() {
    if skip_without_xml_native("test_domdocument_schema_validate_add_attrs") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->load(__DIR__."/book-attr.xml");

$doc->schemaValidate(__DIR__."/book.xsd", LIBXML_SCHEMA_CREATE);

foreach ($doc->getElementsByTagName('book') as $book) {
    var_dump($book->getAttribute('is-hardback'));
}

"#);
    assert_exact(&out, r#"string(5) "false"
string(4) "true""#);
}

/// php-src `ext/dom/tests/DOMDocument_schemaValidate_basic.phpt`: DomDocument::schemaValidate() - basic
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::schemaValidate"#]
fn test_domdocument_schema_validate_basic() {
    if skip_without_xml_native("test_domdocument_schema_validate_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->load(__DIR__."/book.xml");

$result = $doc->schemaValidate(__DIR__."/book.xsd");
var_dump($result);

"#);
    assert_exact(&out, r#"bool(true)"#);
}

/// php-src `ext/dom/tests/DOMDocument_schemaValidate_error1.phpt`: DomDocument::schemaValidate() - file that is not a schema
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::schemaValidate"#]
fn test_domdocument_schema_validate_error1() {
    if skip_without_xml_native("test_domdocument_schema_validate_error1") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->load(__DIR__."/book.xml");

$result = $doc->schemaValidate(__DIR__."/book-not-a-schema.xsd");
var_dump($result);

"#);
    assert_format(&out, r#"Warning: DOMDocument::schemaValidate(): %sbook-not-a-schema.xsd:1: parser error : Start tag expected, '<' not found in %s on line %d

Warning: DOMDocument::schemaValidate(): Let's see what happens upon parsing a file that doesn't contain a schema. in %s on line %d

Warning: DOMDocument::schemaValidate(): ^ in %s on line %d

Warning: DOMDocument::schemaValidate(): Failed to parse the XML resource '%sbook-not-a-schema.xsd'. in %s on line %d

Warning: DOMDocument::schemaValidate(): Invalid Schema in %s on line %d
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMDocument_schemaValidate_error2.phpt`: DomDocument::schemaValidate() - non-conforming schema file
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::schemaValidate"#]
fn test_domdocument_schema_validate_error2() {
    if skip_without_xml_native("test_domdocument_schema_validate_error2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->load(__DIR__."/book.xml");

$result = $doc->schemaValidate(__DIR__."/book-non-conforming-schema.xsd");
var_dump($result);

"#);
    assert_format(&out, r#"Warning: DOMDocument::schemaValidate(): Element 'books': No matching global declaration available for the validation root. in %s on line %d
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMDocument_schemaValidate_error3.phpt`: DomDocument::schemaValidate() - empty string for schema file name
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::schemaValidate"#]
fn test_domdocument_schema_validate_error3() {
    if skip_without_xml_native("test_domdocument_schema_validate_error3") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->load(__DIR__."/book.xml");

try {
    $doc->schemaValidate('');
} catch (ValueError $e) {
    echo $e->getMessage() . "\n";
}

"#);
    assert_exact(&out, r#"DOMDocument::schemaValidate(): Argument #1 ($filename) must not be empty"#);
}

/// php-src `ext/dom/tests/DOMDocument_schemaValidate_error5.phpt`: DomDocument::schemaValidate() - non-existent schema file
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::schemaValidate"#]
fn test_domdocument_schema_validate_error5() {
    if skip_without_xml_native("test_domdocument_schema_validate_error5") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->load(__DIR__."/book.xml");

$result = $doc->schemaValidate(__DIR__."/non-existent-file");
var_dump($result);

"#);
    assert_format(&out, r#"Warning: DOMDocument::schemaValidate(): I/O %s : failed to load %s

Warning: DOMDocument::schemaValidate(): Failed to locate the main schema resource at '%snon-existent-file'. in %s on line %d

Warning: DOMDocument::schemaValidate(): Invalid Schema in %s on line %d
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMDocument_schemaValidate_error6.phpt`: DomDocument::schemaValidate() - invalid path to schema
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::schemaValidate"#]
fn test_domdocument_schema_validate_error6() {
    if skip_without_xml_native("test_domdocument_schema_validate_error6") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->load(__DIR__."/book.xml");

try {
    $doc->schemaValidate("/path/with/\0/byte");
} catch (ValueError $e) {
    echo $e->getMessage() . "\n";
}

var_dump($doc->schemaValidate(str_repeat(" ", PHP_MAXPATHLEN + 1)));

"#);
    assert_format(&out, r#"DOMDocument::schemaValidate(): Argument #1 ($filename) must not contain any null bytes

Warning: DOMDocument::schemaValidate(): Invalid Schema file source in %s on line %d
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMDocument_schemaValidate_missingAttrs.phpt`: DomDocument::schemaValidate() - Don't add missing attribute default values from schema
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::schemaValidate"#]
fn test_domdocument_schema_validate_missing_attrs() {
    if skip_without_xml_native("test_domdocument_schema_validate_missing_attrs") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;

$doc->load(__DIR__."/book-attr.xml");

$doc->schemaValidate(__DIR__."/book.xsd");

foreach ($doc->getElementsByTagName('book') as $book) {
    var_dump($book->getAttribute('is-hardback'));
}

"#);
    assert_exact(&out, r#"string(0) ""
string(4) "true""#);
}

/// php-src `ext/dom/tests/DOMDocument_standalone_basic.phpt`: Tests DOMDocument::standalone get, set, and functionality
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::standalone"#]
fn test_domdocument_standalone_basic() {
    if skip_without_xml_native("test_domdocument_standalone_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
// create dom document
$dom = new DOMDocument;
$xml = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!DOCTYPE s1 PUBLIC "http://www.ibm.com/example.dtd" "example.dtd">
<s1>foo</s1>';
$dom->loadXML($xml);
if(!$dom) {
  echo "Error while parsing the document\n";
  exit(0);
}
echo "Standalone DOMDocument created\n";

$test = $dom->standalone;
echo "Read initial standalone:\n";
var_dump( $test );

$dom->standalone = FALSE;
$test = $dom->standalone;
echo "Set standalone to FALSE, reading again:\n";
var_dump( $test );

$test = $dom->saveXML();
echo "Document is no longer standalone\n";
var_dump( $test );

echo "Done";
"#);
    assert_exact(&out, r#"Standalone DOMDocument created
Read initial standalone:
bool(true)
Set standalone to FALSE, reading again:
bool(false)
Document is no longer standalone
string(136) "<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<!DOCTYPE s1 PUBLIC "http://www.ibm.com/example.dtd" "example.dtd">
<s1>foo</s1>
"
Done"#);
}

/// php-src `ext/dom/tests/DOMDocument_strictErrorChecking_basic.phpt`: DomDocument::$strictErrorChecking - basic test
#[test]
fn test_domdocument_strict_error_checking_basic() {
    if skip_without_xml_native("test_domdocument_strict_error_checking_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

$doc = new DOMDocument;
$doc->load(__DIR__."/book.xml");

var_dump($doc->strictErrorChecking);

$doc->strictErrorChecking = false;
var_dump($doc->strictErrorChecking);

"#);
    assert_exact(&out, r#"bool(true)
bool(false)"#);
}

/// php-src `ext/dom/tests/DOMDocument_strictErrorChecking_variation.phpt`: DomDocument::$strictErrorChecking - ensure turning off actually works
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::strictErrorChecking"#]
fn test_domdocument_strict_error_checking_variation() {
    if skip_without_xml_native("test_domdocument_strict_error_checking_variation") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

echo "Load document\n";
$doc = new DOMDocument;
$doc->load(__DIR__."/book.xml");

echo "See if strictErrorChecking is on\n";
var_dump($doc->strictErrorChecking);

echo "Should throw DOMException when strictErrorChecking is on\n";
try {
    $attr = $doc->createAttribute(0);
} catch (DOMException $e) {
    echo "GOOD. DOMException thrown\n";
    echo $e->getMessage() ."\n";
} catch (Exception $e) {
    echo "OOPS. Other exception thrown\n";
}


echo "Turn strictErrorChecking off\n";
$doc->strictErrorChecking = false;

echo "See if strictErrorChecking is off\n";
var_dump($doc->strictErrorChecking);

echo "Should raise PHP error because strictErrorChecking is off\n";
try {
    $attr = $doc->createAttribute(0);
} catch (DOMException $e) {
    echo "OOPS. DOMException thrown\n";
    echo $e->getMessage() ."\n";
} catch (Exception $e) {
    echo "OOPS. Other exception thrown\n";
}

"#);
    assert_format(&out, r#"Load document
See if strictErrorChecking is on
bool(true)
Should throw DOMException when strictErrorChecking is on
GOOD. DOMException thrown
Invalid Character Error
Turn strictErrorChecking off
See if strictErrorChecking is off
bool(false)
Should raise PHP error because strictErrorChecking is off

Warning: DOMDocument::createAttribute(): Invalid Character Error in %s on line %d"#);
}

/// php-src `ext/dom/tests/DOMDocument_validate_basic.phpt`: DOMDocument::validate() should validate an internal DTD declaration
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::validate"#]
fn test_domdocument_validate_basic() {
    if skip_without_xml_native("test_domdocument_validate_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
$xml = "<?xml version=\"1.0\"?>
<!DOCTYPE note [
<!ELEMENT note (to,from,heading,body)>
<!ELEMENT to (#PCDATA)>
<!ELEMENT from (#PCDATA)>
<!ELEMENT heading (#PCDATA)>
<!ELEMENT body (#PCDATA)>
]>
<note>
<to>Tove</to>
<from>Jani</from>
<heading>Reminder</heading>
<body>Don't forget me this weekend</body>
</note>";
$dom = new DOMDocument('1.0');
$dom->loadXML($xml);
var_dump($dom->validate());
"#);
    assert_exact(&out, r#"bool(true)"#);
}

/// php-src `ext/dom/tests/DOMDocument_validate_error2.phpt`: DOMDocument::validate() should fail if called statically
#[test]
#[ignore = r#"DOM port pending: Undefined method: DOMDocument::validate"#]
fn test_domdocument_validate_error2() {
    if skip_without_xml_native("test_domdocument_validate_error2") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
try {
    DOMDocument::validate();
} catch (Error $e) {
    echo $e->getMessage();
}
"#);
    assert_exact(&out, r#"Non-static method DOMDocument::validate() cannot be called statically"#);
}

/// php-src `ext/dom/tests/DOMDocument_validate_external_dtd.phpt`: DOMDocument::validate() should validate an external DTD declaration
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::substituteEntities"#]
fn test_domdocument_validate_external_dtd() {
    if skip_without_xml_native("test_domdocument_validate_external_dtd") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
// reusing existing xml: http://cvs.php.net/viewvc.cgi/php-src/ext/dom/tests/dom.xml?view=co&content-type=text%2Fplain
// reusing existing dtd: http://cvs.php.net/viewvc.cgi/php-src/ext/dom/tests/dom.ent?view=co&content-type=text%2Fplain
$dom = new DOMDocument('1.0');
$dom->substituteEntities = true;
$dom->load(__DIR__.'/dom.xml');
var_dump($dom->validate());
"#);
    assert_exact(&out, r#"bool(true)"#);
}

/// php-src `ext/dom/tests/DOMDocument_validate_on_parse_basic.phpt`: DOMDocument::$validateOnParse - read/write tests (dom_document_validate_on_parse_read/dom_document_validate_on_parse_write)
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_domdocument_validate_on_parse_basic() {
    if skip_without_xml_native("test_domdocument_validate_on_parse_basic") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

require_once('dom_test.inc');

$dom = new DOMDocument;
$dom->loadXML($xmlstr);

if( !$dom )
{
    echo "Error while parsing the document\n";
    exit(0);
}

echo "Checking documented default value: ";
var_dump($dom->validateOnParse);

$dom->validateOnParse = TRUE;
echo "Setting validateOnParse to TRUE: ";
var_dump($dom->validateOnParse);

$dom->validateOnParse = FALSE;
echo "Setting validateOnParse to FALSE: ";
var_dump($dom->validateOnParse);

"#);
    assert_exact(&out, r#"Checking documented default value: bool(false)
Setting validateOnParse to TRUE: bool(true)
Setting validateOnParse to FALSE: bool(false)"#);
}

/// php-src `ext/dom/tests/DOMDocument_validate_on_parse_variation.phpt`: DOMDocument::$validateOnParse - effectual determination (dom_document_validate_on_parse_read/dom_document_validate_on_parse_write)
#[test]
#[ignore = r#"DOM port pending: Property access requires an object or typed pointer"#]
fn test_domdocument_validate_on_parse_variation() {
    if skip_without_xml_native("test_domdocument_validate_on_parse_variation") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php

require_once('dom_test.inc');

chdir(__DIR__);
$XMLStringGood = file_get_contents(__DIR__.'/note.xml');

$dom = new DOMDocument;
$dom->resolveExternals = TRUE;

$dom->validateOnParse = FALSE;
echo "validateOnParse set to FALSE: \n";
$dom->loadXML($XMLStringGood);
echo "No Error Report Above\n";

$BogusElement = $dom->createElement('NYPHP','DOMinatrix');
$Body = $dom->getElementsByTagName('from')->item(0);
$Body->appendChild($BogusElement);
$XMLStringBad = $dom->saveXML();

echo "validateOnParse set to TRUE: \n";
$dom->validateOnParse = TRUE;
$dom->loadXML($XMLStringBad);
echo "Error Report Above\n";

"#);
    assert_format(&out, r#"validateOnParse set to FALSE: 
No Error Report Above
validateOnParse set to TRUE: 

Warning: DOMDocument::loadXML(): No declaration for element NYPHP in Entity, line: %d in %s on line %d

Warning: DOMDocument::loadXML(): Element from was declared #PCDATA but contains non text nodes in Entity, line: %d in %s on line %d
Error Report Above"#);
}

/// php-src `ext/dom/tests/DOMDocument_version_write.phpt`: DOMDocument::$version write
#[test]
#[ignore = r#"DOM port pending: Undefined property: DOMDocument::version"#]
fn test_domdocument_version_write() {
    if skip_without_xml_native("test_domdocument_version_write") {
        return;
    }
    let out = run_case(r#""#, &[], r#"<?php
class MyThrowingStringable {
    public function __toString(): string {
        throw new Exception("An exception was thrown");
    }
}

$dom = new DOMDocument;
var_dump($dom->version);
$dom->version = "foobar";
var_dump($dom->version);
echo $dom->saveXML();

try {
    $dom->version = new MyThrowingStringable;
} catch (Exception $e) {
    echo $e->getMessage(), "\n";
}
var_dump($dom->version);
echo $dom->saveXML();
"#);
    assert_exact(&out, r#"string(3) "1.0"
string(6) "foobar"
<?xml version="foobar"?>
An exception was thrown
string(6) "foobar"
<?xml version="foobar"?>"#);
}
