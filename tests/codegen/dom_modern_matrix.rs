//! Purpose:
//! Oracle-pinned living-DOM XML and HTML document behavior matrices.
//!
//! Called from:
//! - `cargo test --test codegen_tests dom_modern_matrix` through Rust's test harness.
//!
//! Key details:
//! - Cases isolate modern wrapper classes, fragments, token lists, collections, and HTML properties.
//! - Expected output was captured only from the frozen PHP 8.5.8 oracle.

use crate::support::{compile_and_run_capture, compile_and_run_with_heap_debug};

/// One living-DOM PHP oracle fixture with an identifier suitable for targeted reruns.
struct ModernDomCase {
    id: &'static str,
    source: &'static str,
    stdout: &'static str,
}

/// Executes each modern DOM fixture and checks the entire observable PHP result.
fn assert_modern_dom_cases(cases: &[ModernDomCase]) {
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

/// Pins modern XML document/implementation factories and fragment transfer identity.
///
/// This complements the direct factory checks in `dom.rs` by retaining the complete
/// `createEmpty` → implementation → doctype → fragment state transition in one case.
#[test]
fn modern_xml_document_implementation_and_fragment_matrix_matches_php_8_5_8() {
    assert_modern_dom_cases(&[ModernDomCase {
        id: "modern_xml_empty_document_implementation_doctype_and_fragment",
        source: r#"<?php
$empty = Dom\XMLDocument::createEmpty("1.1", "ISO-8859-1");
$implementation = $empty->implementation;
$doctype = $implementation->createDocumentType("r:root", "-//PUBLIC", "urn:system");
$made = $implementation->createDocument("urn:r", "r:root", $doctype);
echo get_class($empty), ":", $empty->xmlVersion, ":", $empty->xmlEncoding, "|";
echo get_class($implementation), ":", get_class($made), ":";
echo $made->documentElement->namespaceURI, ":", $made->doctype->publicId, ":";
echo $made->doctype->systemId, "|";
$fragment = $made->createDocumentFragment();
$fragment->append("left", $made->createElementNS("urn:r", "r:child"), "right");
$last = $made->documentElement->appendChild($fragment);
echo get_class($last), ":", $made->documentElement->textContent, ":";
echo $made->documentElement->childNodes->length, ":", $fragment->childNodes->length;
"#,
        stdout: "Dom\\XMLDocument:1.1:ISO-8859-1|Dom\\Implementation:Dom\\XMLDocument:urn:r:-//PUBLIC:urn:system|Dom\\DocumentFragment:leftright:3:0",
    }]);
}

/// Pins HTML document element properties, class token mutation, live collections, and serialization.
#[test]
fn modern_html_document_token_list_collection_and_property_matrix_matches_php_8_5_8() {
    assert_modern_dom_cases(&[ModernDomCase {
        id: "modern_html_head_body_title_class_tokens_fragment_and_collection",
        source: r#"<?php
$html = Dom\HTMLDocument::createFromString(
    '<!doctype html><html><head><title>first</title></head><body><main id="app" class="one two"></main></body></html>'
);
$main = $html->querySelector("main");
echo get_class($html->head), ":", get_class($html->body), ":", $html->title, ":";
echo $main->id, ":", $main->className, "|";
$tokens = $main->classList;
$tokens->replace("one", "ready");
$tokens->toggle("two", false);
$tokens->add("ready", "three");
echo $tokens->value, ":", $tokens->length, ":", ($tokens->contains("three") ? "T" : "F"), "|";
$spans = $html->getElementsByTagName("span");
$fragment = $html->createDocumentFragment();
$fragment->append("a", $html->createElement("span"));
$returned = $main->appendChild($fragment);
echo get_class($returned), ":", $spans->length, ":", $main->textContent, ":";
echo $main->lastElementChild->nodeName, "|";
$html->title = "second";
echo $html->title, ":", $html->saveHtml();
"#,
        stdout: concat!(
            "Dom\\HTMLElement:Dom\\HTMLElement:first:app:one two|ready three:2:T|",
            "Dom\\DocumentFragment:1:a:SPAN|second:",
            "<!DOCTYPE html><html><head><title>second</title></head><body>",
            "<main id=\"app\" class=\"ready three\">a<span></span></main>",
            "</body></html>",
        ),
    }]);
}

/// Ensures modern fragment transfer preserves node identities and releases wrapper allocations.
#[test]
fn modern_dom_fragment_transfer_identity_is_heap_clean() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
$document = Dom\XMLDocument::createFromString("<root/>");
$root = $document->documentElement;
$fragment = $document->createDocumentFragment();
$child = $document->createElement("child");
$fragment->append("x", $child);
$returned = $root->appendChild($fragment);
echo get_class($returned), ":", ($returned === $fragment ? "I" : "X"), ":";
echo $fragment->childNodes->length, ":", ($root->lastElementChild === $child ? "I" : "X"), ":";
echo $root->textContent, "\n";
unset($returned, $child, $fragment, $root, $document);
"#,
    );
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(output.stdout, "Dom\\DocumentFragment:I:0:I:x\n");
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "modern fragment transfer leaked: {}",
        output.stderr,
    );
}
