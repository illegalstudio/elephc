//! Purpose:
//! Table-driven construction and write-diagnostic coverage for the PHP 8.5 DOM surface.
//!
//! Called from:
//! - `cargo test --test error_tests dom_surface` through the Rust test harness.
//!
//! Key details:
//! - Case IDs track the generated construction-state matrix described by the locked DOM spec.
//! - Existing private-constructor cases DOM-CONSTRUCT-PRIVATE-01..03 stay in `dom.rs`; runtime-only
//!   lifecycle and dynamic-property cases intentionally stay in `codegen::dom`.

/// One compiler-visible row in the locked DOM construction-state diagnostic matrix.
struct DiagnosticCase {
    id: &'static str,
    source: &'static str,
    expected: Option<&'static str>,
}

/// Verifies construction-state, callable-signature, and native-property diagnostics as one matrix.
///
/// The native engine owns serialization, unserialization, and dynamic-property deprecations at
/// runtime. Those rows are deliberately not duplicated here: see DOM-SURFACE-RUNTIME-01 through
/// DOM-SURFACE-RUNTIME-03 in `tests/codegen/dom.rs`.
#[test]
fn dom_construction_and_native_write_diagnostic_matrix() {
    let cases = [
        DiagnosticCase {
            id: "DOM-CONSTRUCT-ABSTRACT-01",
            source: "<?php new Dom\\Document();",
            expected: Some("Cannot instantiate abstract class Dom\\Document"),
        },
        DiagnosticCase {
            id: "DOM-CONSTRUCT-PRIVATE-04",
            source: "<?php new Dom\\Element();",
            expected: Some("Call to private Dom\\Node::__construct() from global scope"),
        },
        DiagnosticCase {
            id: "DOM-CONSTRUCT-ENGINE-ONLY-05",
            source: "<?php new DOMNodeList();",
            expected: None,
        },
        DiagnosticCase {
            id: "DOM-CONSTRUCT-ENGINE-ONLY-06",
            source: "<?php new DOMNameSpaceNode();",
            expected: None,
        },
        DiagnosticCase {
            id: "DOM-CONSTRUCT-FINAL-01",
            source: "<?php class InvalidXmlDocument extends Dom\\XMLDocument {}",
            expected: Some("cannot extend final class Dom\\XMLDocument"),
        },
        DiagnosticCase {
            id: "DOM-CONSTRUCT-FINAL-02",
            source: "<?php class InvalidHtmlDocument extends Dom\\HTMLDocument {}",
            expected: Some("cannot extend final class Dom\\HTMLDocument"),
        },
        DiagnosticCase {
            id: "DOM-CONSTRUCT-FINAL-03",
            source: "<?php class InvalidXPath extends Dom\\XPath {}",
            expected: Some("cannot extend final class Dom\\XPath"),
        },
        DiagnosticCase {
            id: "DOM-SIGNATURE-ARITY-01",
            source: "<?php new DOMDocument('1.0', 'UTF-8', 'extra');",
            expected: Some("DOMDocument::__construct() expects at most 2 arguments, 3 given"),
        },
        DiagnosticCase {
            id: "DOM-SIGNATURE-NAMED-01",
            source: "<?php new DOMDocument(encoding: 'UTF-8', unsupported: 'x');",
            expected: Some("Unknown named parameter $unsupported"),
        },
        DiagnosticCase {
            id: "DOM-PROPERTY-READONLY-WRITE-01",
            source: "<?php function mutate(Dom\\Node $node): void { $node->nodeName = 'changed'; }",
            expected: Some("Cannot write to read-only property Dom\\Node::$nodeName"),
        },
        DiagnosticCase {
            id: "DOM-PROPERTY-READONLY-UNSET-01",
            source: "<?php function mutate(DOMNode $node): void { unset($node->nodeName); }",
            expected: Some("Cannot unset read-only property DOMNode::$nodeName"),
        },
        DiagnosticCase {
            id: "DOM-COLLECTION-WRITE-01",
            source: "<?php function mutate(DOMNodeList $nodes): void { $nodes[0] = null; }",
            expected: Some("Cannot write to DOMNodeList offset"),
        },
        DiagnosticCase {
            id: "DOM-XPATH-DIRECT-DIMENSION-LEGACY-01",
            source: "<?php $document = new DOMDocument(); $xpath = new DOMXPath($document); $node = ($xpath->query('//x'))[0];",
            expected: None,
        },
        DiagnosticCase {
            id: "DOM-XPATH-DIRECT-DIMENSION-MODERN-01",
            source: "<?php $document = Dom\\XMLDocument::createFromString('<root/>'); $xpath = new Dom\\XPath($document); $node = ($xpath->query('//x'))[0];",
            expected: None,
        },
    ];

    for case in cases {
        match (case.expected, super::check_source(case.source)) {
            (None, Ok(())) => {}
            (None, Err(actual)) => panic!("{}: expected accepted engine-created shell, got {actual:?}", case.id),
            (Some(expected), Ok(())) => panic!("{}: expected diagnostic containing {expected:?}", case.id),
            (Some(expected), Err(actual)) => assert!(
                actual.contains(expected),
                "{}: expected {expected:?}, got {actual:?}",
                case.id,
            ),
        }
    }
}

// DOM-SURFACE-RUNTIME-01: `dom_node_serialization_hooks_match_php_concrete_class_errors`
// proves serialize/unserialize denial and its exact runtime Exception messages.
// DOM-SURFACE-RUNTIME-02: clone/import/adopt dynamic-property semantics live in
// `tests/codegen/dom.rs` because the property bag is observable only after native execution.
// DOM-SURFACE-RUNTIME-03: PHP 8.5 dynamic-property deprecation is emitted by the runtime bridge;
// it cannot truthfully be asserted as a frontend diagnostic.
