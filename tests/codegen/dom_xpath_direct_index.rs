//! Purpose:
//! Regression tests for direct XPath-result dimension reads.
//!
//! Called from:
//! - `cargo test --test codegen_tests dom_xpath_direct_index` through Rust's test harness.
//!
//! Key details:
//! - Legacy `DOMXPath::query()` carries a `DOMNodeList|false` result and must evaluate its
//!   dimension once, using `item()` only on the object arm.
//! - Modern `Dom\XPath` keeps the same direct-index syntax while invalid expressions surface
//!   PHP's catchable `Error` contract.
//! - Expected output is byte-for-byte pinned to PHP 8.5.8 (oracle binary SHA-256
//!   `6253fe2a...`, libxml2 2.15.3).

use crate::support::compile_and_run_capture;

/// Verifies direct indexing of successful legacy and modern XPath queries.
///
/// Legacy element `nodeValue` retains its historical text convenience, while
/// spec-following modern elements expose `null` there and their descendant text
/// through `textContent`. Keep the properties separate so the direct-index
/// regression checks each API family's actual PHP contract.
#[test]
fn direct_xpath_result_index_reads_legacy_and_modern_nodes() {
    let legacy = compile_and_run_capture(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root><x>A</x></root>');
$xpath = new DOMXPath($document);
echo ($xpath->query('//x'))[0]->nodeValue;
"#,
    );
    assert!(legacy.success, "legacy direct index failed: {}", legacy.stderr);
    assert_eq!(legacy.stdout, "A");
    assert_eq!(legacy.stderr, "");

    let modern = compile_and_run_capture(
        r#"<?php
$document = Dom\XMLDocument::createFromString('<root><x>B</x></root>');
$xpath = new Dom\XPath($document);
echo ($xpath->query('//x'))[0]->textContent;
"#,
    );
    assert!(modern.success, "modern direct index failed: {}", modern.stderr);
    assert_eq!(modern.stdout, "B");
    assert_eq!(modern.stderr, "");
}

/// Verifies direct modern XPath indexing preserves XML and namespace-free HTML element values.
///
/// PHP 8.5.8's modern DOM follows the DOM Standard: element `nodeValue` is
/// `null`, whereas `textContent` contains descendant text. An unprefixed
/// XPath name test selects HTML elements only when `Dom\HTML_NO_DEFAULT_NS`
/// opts out of PHP's default XHTML namespace conversion.
#[test]
fn direct_modern_xpath_index_preserves_spec_node_value_and_text_content() {
    let xml = compile_and_run_capture(
        r#"<?php
$document = Dom\XMLDocument::createFromString('<root><x>B</x></root>');
$xpath = new Dom\XPath($document);
$node = ($xpath->query('//x'))[0];
echo ($node->nodeValue === null ? 'N' : 'V'), ':', $node->textContent;
"#,
    );
    assert!(xml.success, "modern XML direct index failed: {}", xml.stderr);
    assert_eq!(xml.stdout, "N:B");
    assert_eq!(xml.stderr, "");

    let html = compile_and_run_capture(
        r#"<?php
$document = Dom\HTMLDocument::createFromString(
    '<!doctype html><html><body><p>C</p></body></html>',
    Dom\HTML_NO_DEFAULT_NS,
);
$xpath = new Dom\XPath($document);
$node = ($xpath->query('//p'))[0];
echo ($node->nodeValue === null ? 'N' : 'V'), ':', $node->textContent;
"#,
    );
    assert!(html.success, "modern HTML direct index failed: {}", html.stderr);
    assert_eq!(html.stdout, "N:C");
    assert_eq!(html.stderr, "");
}

/// Verifies modern HTML XPath retains PHP's XHTML default-namespace behavior.
///
/// Default `Dom\HTMLDocument` conversion puts HTML elements in the XHTML
/// namespace, so libxml's unprefixed XPath `//p` returns an empty `Dom\NodeList`.
/// The explicit no-default-namespace mode keeps the same expression usable for
/// direct indexing and retains the selected element's DOM-standard text surface.
#[test]
fn direct_modern_html_xpath_honors_default_namespace_selection() {
    let default_namespace = compile_and_run_capture(
        r#"<?php
$document = Dom\HTMLDocument::createFromString(
    '<!doctype html><html><body><p>C</p></body></html>',
);
$xpath = new Dom\XPath($document);
$nodes = $xpath->query('//p');
$node = $nodes->item(0);
echo $nodes->length, ':', ($node === null ? 'N' : 'V'), ':';
echo $document->documentElement->namespaceURI;
"#,
    );
    assert!(
        default_namespace.success,
        "default-namespace modern HTML XPath failed: {}",
        default_namespace.stderr
    );
    assert_eq!(
        default_namespace.stdout,
        "0:N:http://www.w3.org/1999/xhtml"
    );
    assert_eq!(default_namespace.stderr, "");

    let no_default_namespace = compile_and_run_capture(
        r#"<?php
$document = Dom\HTMLDocument::createFromString(
    '<!doctype html><html><body><p>C</p></body></html>',
    Dom\HTML_NO_DEFAULT_NS,
);
$xpath = new Dom\XPath($document);
$node = ($xpath->query('//p'))[0];
echo ($node->nodeValue === null ? 'N' : 'V'), ':', $node->textContent;
"#,
    );
    assert!(
        no_default_namespace.success,
        "namespace-free modern HTML XPath failed: {}",
        no_default_namespace.stderr
    );
    assert_eq!(no_default_namespace.stdout, "N:C");
    assert_eq!(no_default_namespace.stderr, "");
}

/// Verifies query materializes its eager members through the matching DOM family table.
///
/// `Dom\XPath::query()` returns modern member kind `201` for an XML element, while the
/// legacy API returns kind `101`.  Keep this focused outer-wrapper regression separate from
/// direct indexing so an ABI containment failure identifies family selection immediately.
#[test]
fn xpath_query_materializes_legacy_and_modern_node_list_families() {
    let legacy = compile_and_run_capture(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root><x>A</x></root>');
$xpath = new DOMXPath($document);
$nodes = $xpath->query('//x');
echo get_class($nodes), ':', $nodes->length;
"#,
    );
    assert!(legacy.success, "legacy XPath node-list failed: {}", legacy.stderr);
    assert_eq!(legacy.stdout, "DOMNodeList:1");
    assert_eq!(legacy.stderr, "");

    let modern = compile_and_run_capture(
        r#"<?php
$document = Dom\XMLDocument::createFromString('<root><x>B</x></root>');
$xpath = new Dom\XPath($document);
$nodes = $xpath->query('//x');
echo get_class($nodes), ':', $nodes->length;
"#,
    );
    assert!(modern.success, "modern XPath node-list failed: {}", modern.stderr);
    assert_eq!(modern.stdout, "Dom\\NodeList:1");
    assert_eq!(modern.stderr, "");

}

/// Verifies a namespace declaration result exposes PHP's shared node metadata.
#[test]
fn namespace_declaration_attribute_uses_node_value_without_text_content() {
    let output = compile_and_run_capture(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root xmlns:p="urn:p"/>');
$node = $document->documentElement->getAttributeNodeNS(
    'http://www.w3.org/2000/xmlns/',
    'p',
);
if ($node === null) {
    exit(2);
}
echo get_class($node), ':', $node->nodeName, ':', $node->nodeValue;
"#,
    );
    assert!(output.success, "namespace declaration read failed: {}", output.stderr);
    assert_eq!(output.stdout, "DOMNameSpaceNode:xmlns:p:urn:p");
    assert_eq!(output.stderr, "");
}

/// Verifies the legacy `false` query result takes PHP's warning/error offset path exactly once.
#[test]
fn direct_legacy_xpath_false_result_keeps_php_offset_diagnostics() {
    let output = compile_and_run_capture(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root/>');
$xpath = new DOMXPath($document);
$warnings = 0;
set_error_handler(function () use (&$warnings): bool {
    $warnings++;
    return true;
});
$result = ($xpath->query('//*['))[0];
restore_error_handler();
echo $warnings, ':', ($result === null ? 'N' : 'V');
"#,
    );
    assert!(output.success, "legacy false-result direct index failed: {}", output.stderr);
    assert_eq!(output.stdout, "2:N");
    assert_eq!(output.stderr, "");
}

/// Verifies modern XPath keeps its invalid-expression `Error` contract around direct indexing.
#[test]
fn direct_modern_xpath_invalid_query_throws_php_error() {
    let output = compile_and_run_capture(
        r#"<?php
$document = Dom\HTMLDocument::createFromString(
    '<!doctype html><html><body><p>C</p></body></html>',
    Dom\HTML_NO_DEFAULT_NS,
);
$xpath = new Dom\XPath($document);
try {
    $result = ($xpath->query('//*['))[0];
} catch (Error $error) {
    echo get_class($error), ':', $error->getMessage();
}
"#,
    );
    assert!(output.success, "modern invalid direct index failed: {}", output.stderr);
    assert_eq!(output.stdout, "Error:Could not evaluate XPath expression");
    assert_eq!(output.stderr, "Warning: Dom\\XPath::query(): Invalid expression\n");
}
