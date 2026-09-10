//! Purpose:
//! End-to-end regressions for legacy and modern document-fragment XML insertion.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::dom_fragment` through Rust's test harness.
//!
//! Key details:
//! - Expected parser diagnostics follow the pinned libxml2 2.15.3 bridge engine.
//! - Tests cover binding, namespace, malformed-input, suppression, and ownership behavior.

use crate::support::{
    compile_and_run, compile_and_run_capture, compile_and_run_with_heap_debug,
};

/// Verifies legacy, modern XML, and modern HTML fragments use balanced XML chunks.
#[test]
fn document_fragment_append_xml_round_trips_all_php_families() {
    let out = compile_and_run(
        r#"<?php
$legacy = new DOMDocument();
$legacyFragment = $legacy->createDocumentFragment();
echo $legacyFragment->appendXML('<foo id="baz">bar</foo><tail/>') ? "T" : "F";
echo $legacyFragment->hasChildNodes() ? "H" : "N";
echo "|" . $legacy->saveXML($legacyFragment);

$namespaced = new DOMDocument();
$namespaced->loadXML('<root xmlns="urn:root"/>');
$plain = $namespaced->createDocumentFragment();
$plain->appendXML('<child/>');
echo "|" . ($plain->firstElementChild->namespaceURI === null ? "N" : "X");

$modern = Dom\XMLDocument::createEmpty();
$modernFragment = $modern->createDocumentFragment();
echo "|" . ($modernFragment->appendXml('<modern/>text&amp;') ? "T" : "F");
echo "|" . $modern->saveXml($modernFragment);

libxml_use_internal_errors(true);
$html = Dom\HTMLDocument::createEmpty();
$htmlFragment = $html->createDocumentFragment();
echo "|" . ($htmlFragment->appendXml('<foo>bar</foo><br>tail') ? "T" : "F");
$errors = libxml_get_errors();
echo ":" . count($errors) . ":" . $errors[0]->code;
"#,
    );
    assert_eq!(
        out,
        "TH|<foo id=\"baz\">bar</foo><tail/>|N|T|<modern/>text&amp;|F:1:77"
    );
}

/// Verifies unbound, malformed, empty, and suppressed fragment calls match php-src.
#[test]
fn document_fragment_append_xml_failures_and_diagnostics_match_php() {
    let out = compile_and_run_capture(
        r#"<?php
$unbound = new DOMDocumentFragment();
try {
    $unbound->appendXML('<x/>');
} catch (DOMException $error) {
    echo get_class($error) . ":" . $error->getCode() . ":" . $error->getMessage();
}

$document = new DOMDocument();
$fragment = $document->createDocumentFragment();
echo "|" . ($fragment->appendXML('<foo>is<bar>great</foo>') ? "T" : "F");
echo ":" . $fragment->childNodes->length;

libxml_use_internal_errors(true);
$internal = $document->createDocumentFragment();
echo "|" . ($internal->appendXML('<foo>is<bar>great</foo>') ? "T" : "F");
$errors = libxml_get_errors();
echo ":" . count($errors);
echo ":" . $errors[0]->level;
echo ":" . $errors[0]->code;
echo ":" . $errors[0]->line;
echo ":" . $errors[0]->column;
echo ":" . trim($errors[0]->message);
libxml_clear_errors();
echo "|" . ($internal->appendXML('') ? "T" : "F");
echo ":" . count(libxml_get_errors());
libxml_use_internal_errors(false);
echo "|" . (@$internal->appendXML('<broken>') ? "T" : "F");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "DOMException:7:No Modification Allowed Error|F:0|F:1:3:76:1:24:Opening and ending tag mismatch: bar line 1 and foo|T:0|F"
    );
    assert_eq!(
        out.stderr,
        "Warning: DOMDocumentFragment::appendXML(): Opening and ending tag mismatch: bar line 1 and foo in Entity, line: 1\n"
    );
}

/// Verifies fragment parsing and wrapper teardown leave the runtime heap balanced.
#[test]
fn document_fragment_append_xml_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$legacy = new DOMDocument();
$fragment = $legacy->createDocumentFragment();
$fragment->appendXML('<one/><two>text</two>');
$legacy->saveXML($fragment);

$modern = Dom\XMLDocument::createEmpty();
$modernFragment = $modern->createDocumentFragment();
$modernFragment->appendXml('<three/>tail');
$modern->saveXml($modernFragment);
echo "clean";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout,
        out.stderr
    );
    assert_eq!(out.stdout, "clean");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}
