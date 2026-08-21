//! Purpose:
//! Table-driven DOM DTD, XML Schema, and Relax NG validation post-failure regressions.
//!
//! Called from:
//! - `cargo test --test codegen_tests dom_validation_matrix` through Rust's test harness.
//!
//! Key details:
//! - Every failed validation observes both the ordered libxml error state and the still-live document tree.
//! - Fixtures remain small because this suite is designed for focused serial bridge runs.

use crate::support::compile_and_run;

/// Pins DTD, Schema, and Relax NG failures without allowing a failed grammar to mutate the document.
#[test]
fn dom_validation_failure_state_matches_php_oracle_matrix() {
    for (case, source, expected) in [
        (
            "DOM-VALIDATE-SCHEMA-01",
            r#"<?php
libxml_use_internal_errors(true);
$document = new DOMDocument();
$document->loadXML("<root stable=\"yes\"><wrong/></root>");
$schema = "<schema xmlns=\"http://www.w3.org/2001/XMLSchema\"><element name=\"root\"><complexType><sequence><element name=\"child\"/></sequence></complexType></element></schema>";
libxml_clear_errors();
$valid = $document->schemaValidateSource($schema);
$error = libxml_get_last_error();
echo "schema|" . ($valid ? "T" : "F") . "|"
    . $document->documentElement->getAttribute("stable") . "|"
    . $document->documentElement->firstElementChild->nodeName . "|"
    . count(libxml_get_errors()) . "|" . $error->code;
"#,
            "schema|F|yes|wrong|2|1871",
        ),
        (
            "DOM-VALIDATE-DTD-02",
            r#"<?php
libxml_use_internal_errors(true);
$document = new DOMDocument();
$document->loadXML("<!DOCTYPE root [<!ELEMENT root (child)><!ELEMENT child EMPTY>]><root><wrong/></root>");
libxml_clear_errors();
$valid = $document->validate();
$error = libxml_get_last_error();
echo "dtd|" . ($valid ? "T" : "F") . "|"
    . $document->documentElement->firstElementChild->nodeName . "|"
    . count(libxml_get_errors()) . "|" . $error->code;
"#,
            "dtd|F|wrong|2|534",
        ),
        (
            "DOM-VALIDATE-RNG-03",
            r#"<?php
libxml_use_internal_errors(true);
$document = new DOMDocument();
$document->loadXML("<root stable=\"yes\"><wrong/></root>");
$grammar = "<element name=\"root\" xmlns=\"http://relaxng.org/ns/structure/1.0\"><element name=\"child\"><empty/></element></element>";
libxml_clear_errors();
$valid = $document->relaxNGValidateSource($grammar);
$error = libxml_get_last_error();
echo "rng|" . ($valid ? "T" : "F") . "|"
    . $document->documentElement->getAttribute("stable") . "|"
    . $document->documentElement->firstElementChild->nodeName . "|"
    . count(libxml_get_errors()) . "|" . $error->code;
"#,
            "rng|F|yes|wrong|1|38",
        ),
    ] {
        assert_eq!(compile_and_run(source), expected, "{case}");
    }
}
