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
#[test]
fn direct_xpath_result_index_reads_legacy_and_modern_nodes() {
    let legacy = compile_and_run_capture(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root><x>A</x></root>');
$xpath = new DOMXPath($document);
echo ($xpath->query('//x'))[0]->textContent;
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
$document = Dom\XMLDocument::createFromString('<root/>');
$xpath = new Dom\XPath($document);
set_error_handler(function (): bool {
    return true;
});
try {
    $result = ($xpath->query('//*['))[0];
} catch (Error $error) {
    echo get_class($error), ':', $error->getMessage();
}
restore_error_handler();
"#,
    );
    assert!(output.success, "modern invalid direct index failed: {}", output.stderr);
    assert_eq!(output.stdout, "Error:Could not evaluate XPath expression");
    assert_eq!(output.stderr, "");
}
