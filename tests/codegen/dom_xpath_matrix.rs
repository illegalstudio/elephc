//! Purpose:
//! Oracle-pinned legacy and living-DOM XPath behavior matrices.
//!
//! Called from:
//! - `cargo test --test codegen_tests dom_xpath_matrix` through Rust's test harness.
//!
//! Key details:
//! - Cases pin namespaces, scalar/node-set semantics, callback reentrancy, exceptions, clone families, and contexts.
//! - The ignored legacy namespace-axis case is the explicit TDD marker for the unsupported result wrapper path.

use crate::support::{compile_and_run_capture, compile_and_run_with_heap_debug};

/// One XPath oracle fixture, including diagnostics where PHP exposes them on stderr.
struct XPathCase {
    id: &'static str,
    source: &'static str,
    stdout: &'static str,
    stderr: &'static str,
}

/// Runs every XPath fixture and checks both output streams against the frozen PHP oracle.
fn assert_xpath_cases(cases: &[XPathCase]) {
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
        assert_eq!(output.stderr, case.stderr, "{} stderr", case.id);
    }
}

/// Pins modern XPath node-set union ordering/deduplication, scalar evaluation, and namespace registration modes.
#[test]
fn modern_xpath_nodes_scalars_namespaces_and_context_matrix_matches_php_8_5_8() {
    assert_xpath_cases(&[XPathCase {
        id: "modern_xpath_union_dedup_scalar_context_and_register_node_namespaces",
        source: r#"<?php
$document = Dom\XMLDocument::createFromString(
    '<root><scope xmlns:p="urn:p"><p:item rank="2">A</p:item><p:item rank="3">B</p:item></scope></root>'
);
$scope = $document->documentElement->firstChild;
$xpath = new Dom\XPath($document);
$items = $xpath->query(".//p:item | .//p:item", $scope);
echo get_class($items), ":", $items->length, ":", $items->item(0)->textContent, ":";
echo $items->item(1)->textContent, ":", $xpath->evaluate("sum(.//p:item/@rank)", $scope), "|";
$xpath->registerNodeNamespaces = false;
try {
    $xpath->query(".//p:item", $scope);
} catch (Error $error) {
    echo get_class($error), ":", $error->getMessage(), "|";
}
echo $xpath->query(".//p:item", $scope, true)->length, "|";
$xpath->registerNamespace("p", "urn:p");
echo $xpath->query(".//p:item", $scope, false)->length;
"#,
        stdout: "Dom\\NodeList:2:A:B:5|Error:Could not evaluate XPath expression|2|2",
        stderr: "Warning: Dom\\XPath::query(): Undefined namespace prefix: p\n",
    }]);
}

/// Pins custom XPath namespace callbacks through nested evaluation and exact thrown-object transport.
#[test]
fn modern_xpath_callback_reentrancy_and_exception_identity_match_php_8_5_8() {
    assert_xpath_cases(&[XPathCase {
        id: "modern_xpath_callback_reentrancy_and_exception_identity",
        source: r#"<?php
$document = Dom\XMLDocument::createFromString("<root><item>A</item><item>B</item></root>");
$xpath = new Dom\XPath($document);
$xpath->registerNamespace("cb", "urn:callback");
$xpath->registerPhpFunctionNS(
    "urn:callback",
    "inspect",
    function (array $nodes) use ($xpath): string {
        echo count($nodes), ":", $xpath->evaluate("count(//item)"), "|";
        return $nodes[1]->textContent;
    }
);
echo $xpath->evaluate("cb:inspect(//item)"), "|";
$expected = new Exception("callback");
$xpath->registerPhpFunctionNS(
    "urn:callback",
    "fail",
    function () use ($expected): string {
        throw $expected;
    }
);
try {
    $xpath->evaluate("cb:fail()");
} catch (Throwable $actual) {
    echo get_class($actual), ":", ($actual === $expected ? "I" : "X"), ":", $actual->getMessage();
}
"#,
        stdout: "2:2|B|Exception:I:callback",
        stderr: "",
    }]);
}

/// Pins the PHP 8.5.8 cloneability and cross-document context contracts of both XPath families.
#[test]
fn xpath_clone_family_and_context_errors_match_php_8_5_8() {
    assert_xpath_cases(&[XPathCase {
        id: "legacy_and_modern_xpath_are_uncloneable_and_reject_foreign_contexts",
        source: r#"<?php
$legacy_document = new DOMDocument();
$legacy_document->loadXML("<root/>");
$legacy = new DOMXPath($legacy_document);
$modern_document = Dom\XMLDocument::createFromString("<root/>");
$modern = new Dom\XPath($modern_document);
try {
    $legacy_copy = clone $legacy;
} catch (Error $error) {
    echo "legacy", ":", get_class($error), ":", $error->getMessage(), "|";
}
try {
    $modern_copy = clone $modern;
} catch (Error $error) {
    echo "modern", ":", get_class($error), ":", $error->getMessage(), "|";
}
$other = Dom\XMLDocument::createFromString("<other/>");
try {
    $modern->query(".", $other->documentElement);
} catch (Error $error) {
    echo get_class($error), ":", $error->getMessage();
}
"#,
        stdout: concat!(
            "legacy:Error:Trying to clone an uncloneable object of class DOMXPath|",
            "modern:Error:Trying to clone an uncloneable object of class Dom\\XPath|",
            "Error:Node from wrong document",
        ),
        stderr: "",
    }]);
}

/// Documents the currently unsupported legacy namespace-node result materialization from PHP 8.5.8.
///
/// This test is intentionally red and ignored until legacy XPath namespace-axis results are implemented.
#[test]
#[ignore = "TDD red: Legacy XPath namespace-node results are not implemented"]
fn legacy_xpath_namespace_node_results_match_php_8_5_8() {
    assert_xpath_cases(&[XPathCase {
        id: "legacy_xpath_namespace_axis_returns_namespace_node_wrappers",
        source: r#"<?php
$document = new DOMDocument();
$document->loadXML('<root xmlns="urn:default" xmlns:p="urn:p"><child/></root>');
$xpath = new DOMXPath($document);
$nodes = $xpath->query("//namespace::*");
echo get_class($nodes), ":", $nodes->length, "|";
for ($index = 0; $index < $nodes->length; $index++) {
    $node = $nodes->item($index);
    echo get_class($node), ":", $node->nodeName, ":", $node->nodeValue, ":";
    echo ($node->ownerDocument === $document ? "I" : "X"), "|";
}
"#,
        stdout: concat!(
            "DOMNodeList:6|",
            "DOMNameSpaceNode:xmlns:xml:http://www.w3.org/XML/1998/namespace:I|",
            "DOMNameSpaceNode:xmlns:p:urn:p:I|",
            "DOMNameSpaceNode:xmlns:urn:default:I|",
            "DOMNameSpaceNode:xmlns:xml:http://www.w3.org/XML/1998/namespace:I|",
            "DOMNameSpaceNode:xmlns:p:urn:p:I|",
            "DOMNameSpaceNode:xmlns:urn:default:I|",
        ),
        stderr: "",
    }]);
}

/// Ensures a namespaced modern XPath node list can be released after context-scoped evaluation.
#[test]
fn modern_xpath_namespace_context_node_list_is_heap_clean() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
$document = Dom\XMLDocument::createFromString(
    '<root><scope xmlns:p="urn:p"><p:item>A</p:item><p:item>B</p:item></scope></root>'
);
$scope = $document->documentElement->firstChild;
$xpath = new Dom\XPath($document);
$items = $xpath->query(".//p:item", $scope);
echo $items->length, ":", $items->item(0)->textContent, ":", $items->item(1)->textContent, "\n";
unset($items, $xpath, $scope, $document);
"#,
    );
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(output.stdout, "2:A:B\n");
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "XPath context node list leaked: {}",
        output.stderr,
    );
}
