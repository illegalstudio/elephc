//! Purpose:
//! Oracle-pinned legacy DOM constructor, mutation, namespace, and DTD regression matrices.
//!
//! Called from:
//! - `cargo test --test codegen_tests dom_legacy_matrix` through Rust's test harness.
//!
//! Key details:
//! - Cases are derived from the frozen PHP 8.5.8 oracle and intentionally complement `dom.rs`.
//! - Mutation cases pin returned-wrapper identity and the fragment/foreign-document state machine.

use crate::support::{compile_and_run_capture, compile_and_run_with_heap_debug};

/// One PHP 8.5.8 legacy DOM oracle fixture with a stable identifier for failure reports.
struct LegacyDomCase {
    id: &'static str,
    source: &'static str,
    stdout: &'static str,
}

/// Compiles each legacy DOM fixture and compares its complete observable result with PHP 8.5.8.
fn assert_legacy_dom_cases(cases: &[LegacyDomCase]) {
    for case in cases {
        let output = compile_and_run_capture(case.source);
        assert!(
            output.success,
            "{} failed: stdout={:?} stderr={}",
            case.id,
            output.stdout,
            output.stderr,
        );
        assert_eq!(output.stdout, case.stdout, "{} stdout", case.id);
        assert_eq!(output.stderr, "", "{} stderr", case.id);
    }
}

/// Pins legacy implementation factories and the distinct mutation return/exception states.
///
/// `dom.rs` already covers individual factory and mutation APIs; this matrix keeps the
/// receiver/new/reference/self/fragment/foreign-document interaction in one PHP oracle trace.
#[test]
fn legacy_dom_factory_and_mutation_matrix_matches_php_8_5_8() {
    assert_legacy_dom_cases(&[
        LegacyDomCase {
            id: "legacy_implementation_factories_keep_wrapper_identity_rules",
            source: r#"<?php
$implementation = new DOMImplementation();
$doctype = $implementation->createDocumentType("r:root", "-//R", "urn:r");
$document = $implementation->createDocument("urn:r", "r:root", $doctype);
echo get_class($implementation), ":", get_class($document), ":";
echo $document->documentElement->nodeName, ":", $document->doctype->name, ":";
echo $document->doctype->publicId, ":", $document->doctype->systemId, ":";
echo ($document->implementation === $implementation ? "I" : "X"), "|";
echo $document->saveXML();
"#,
            stdout: concat!(
                "DOMImplementation:DOMDocument:r:root:r:root:-//R:urn:r:X|",
                "<?xml version=\"1.0\"?>\n",
                "<!DOCTYPE r:root PUBLIC \"-//R\" \"urn:r\">\n",
                "<r:root xmlns:r=\"urn:r\"/>\n",
            ),
        },
        LegacyDomCase {
            id: "legacy_mutation_receiver_new_reference_fragment_foreign_and_self",
            source: r#"<?php
$document = new DOMDocument();
$root = $document->appendChild($document->createElement("root"));
$old = $root->appendChild($document->createElement("old"));
$new = $document->createElement("new");
$received = $root->replaceChild($new, $old);
echo ($received === $old ? "R" : "r"), ($new->parentNode === $root ? "N" : "n"), ":";
echo $root->firstChild->nodeName, "|";
$reference = $document->createElement("ref");
$root->appendChild($reference);
$middle = $document->createElement("middle");
$inserted = $root->insertBefore($middle, $reference);
echo ($inserted === $middle ? "I" : "i"), ":", $root->childNodes->length, ":";
echo $root->lastChild->nodeName, "|";
$fragment = $document->createDocumentFragment();
$fragment->appendChild($document->createElement("a"));
$fragment->appendChild($document->createElement("b"));
$fragment_result = $root->appendChild($fragment);
echo get_class($fragment_result), ":", $fragment->childNodes->length, ":";
echo $root->childNodes->length, ":", $root->lastChild->nodeName, "|";
$other = new DOMDocument();
try {
    $root->appendChild($other->createElement("foreign"));
} catch (DOMException $error) {
    echo $error->code, ":", $error->getMessage(), "|";
}
try {
    $root->appendChild($root);
} catch (DOMException $error) {
    echo $error->code, ":", $error->getMessage();
}
"#,
            stdout: "RN:new|I:3:ref|DOMElement:0:5:b|4:Wrong Document Error|3:Hierarchy Request Error",
        },
    ]);
}

/// Pins legacy normalization, attribute namespaces, parsed DTD entities, and serialization.
#[test]
fn legacy_dom_normalize_namespace_and_dtd_matrix_matches_php_8_5_8() {
    assert_legacy_dom_cases(&[LegacyDomCase {
        id: "legacy_normalize_namespaces_dtd_entity_notation_and_xml_serialization",
        source: r#"<?php
$document = new DOMDocument("1.0", "UTF-8");
$root = $document->createElementNS("urn:root", "r:root");
$document->appendChild($root);
$root->setAttributeNS("urn:attr", "a:id", "one");
$root->appendChild($document->createTextNode("a"));
$root->appendChild($document->createTextNode(""));
$root->appendChild($document->createTextNode("b"));
$root->normalize();
echo $root->getAttributeNS("urn:attr", "id"), ":", $root->textContent, ":";
echo $root->childNodes->length, ":", $root->lookupPrefix("urn:attr"), ":";
echo $document->saveXML($root), "|";
$dtd = new DOMDocument();
$dtd->loadXML('<!DOCTYPE root [<!ENTITY entity "E"><!NOTATION note SYSTEM "urn:note">]><root>&entity;</root>');
$type = $dtd->doctype;
$entity = $type->entities->getNamedItem("entity");
$notation = $type->notations->getNamedItem("note");
echo $type->name, ":", $entity->nodeName, ":", $entity->nodeValue, ":";
echo $notation->nodeName, ":", $notation->systemId, ":";
echo $dtd->saveXML($dtd->documentElement);
"#,
        stdout: concat!(
            "one:ab:1:a:<r:root xmlns:r=\"urn:root\" xmlns:a=\"urn:attr\" a:id=\"one\">",
            "ab</r:root>|root:entity::note:urn:note:<root>&entity;</root>",
        ),
    }]);
}

/// Ensures legacy mutation wrappers can be repeatedly released after fragment transfer.
#[test]
fn legacy_dom_fragment_mutation_identity_is_heap_clean() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
for ($index = 0; $index < 2; $index++) {
    $document = new DOMDocument();
    $root = $document->appendChild($document->createElement("root"));
    $fragment = $document->createDocumentFragment();
    $fragment->appendChild($document->createElement("child"));
    $returned = $root->appendChild($fragment);
    echo $returned->nodeName;
    unset($returned, $fragment, $root, $document);
}
echo "\n";
"#,
    );
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(output.stdout, "childchild\n");
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "legacy fragment transfer leaked: {}",
        output.stderr,
    );
}
