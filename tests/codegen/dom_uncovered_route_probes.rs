//! Purpose:
//! Targeted regression probes for public DOM routes not covered by the existing matrices.
//!
//! Called from:
//! - `cargo test --test codegen_tests dom_uncovered_route_probes` through Rust's test harness.
//!
//! Key details:
//! - Each expected result is pinned against the PHP 8.5.8/libxml2 2.15.3 oracle.

use crate::support::compile_and_run;

/// Verifies shallow and deep modern imports of legacy nodes preserve the requested depth.
#[test]
fn document_import_legacy_node_preserves_requested_depth() {
    let out = compile_and_run(
        r#"<?php
$legacy = new DOMDocument();
$legacy->loadXML('<legacy><child/></legacy>');
$modern = Dom\XMLDocument::createEmpty();
$shallow = $modern->importLegacyNode($legacy->documentElement);
$deep = $modern->importLegacyNode($legacy->documentElement, true);
echo get_class($shallow), "|", $shallow->nodeName, "|";
echo $shallow === $deep ? "same" : "different";
echo "|", $shallow->childNodes->length, ":", $deep->childNodes->length;
"#,
    );
    assert_eq!(out, "Dom\\Element|legacy|different|0:1");
}

/// Verifies Relax NG source validation reports boolean validity and buffers failures.
#[test]
fn document_relaxng_validate_source_reports_boolean_validity() {
    let out = compile_and_run(
        r#"<?php
libxml_use_internal_errors(true);
$valid = Dom\XMLDocument::createFromString('<root/>');
$invalid = Dom\XMLDocument::createFromString('<other/>');
$grammar = '<element xmlns="http://relaxng.org/ns/structure/1.0" name="root"><empty/></element>';
echo $valid->relaxNgValidateSource($grammar) ? "T" : "F";
echo $invalid->relaxNgValidateSource($grammar) ? "T" : "F";
echo count(libxml_get_errors());
libxml_clear_errors();
libxml_use_internal_errors(false);
"#,
    );
    assert_eq!(out, "TF1");
}

/// Verifies DOM feature and version support probes for known PHP-supported pairs.
#[test]
fn dom_node_is_supported_matches_known_feature_version_pairs() {
    let out = compile_and_run(
        r#"<?php
$document = new DOMDocument();
$node = $document->createElement('root');
echo $node->isSupported('Core', '3.0') ? "T" : "F";
echo $node->isSupported('', '') ? "T" : "F";
echo $node->isSupported('XML', '1.0') ? "T" : "F";
"#,
    );
    assert_eq!(out, "FFT");
}
