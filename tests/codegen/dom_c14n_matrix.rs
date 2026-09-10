//! Purpose:
//! Table-driven DOM canonicalization filters, file output, and failure regressions.
//!
//! Called from:
//! - `cargo test --test codegen_tests dom_c14n_matrix` through Rust's test harness.
//!
//! Key details:
//! - XPath filtering and output-file cases keep canonical byte expectations separate from diagnostic expectations.
//! - Relative files live inside the existing per-test temporary project directory.

use crate::support::compile_and_run;

/// Pins XPath namespace filters, empty node sets, file bytes, and output-open warnings for C14N.
#[test]
fn dom_c14n_matches_php_oracle_matrix() {
    for (case, source, expected) in [
        (
            "DOM-C14N-FILTER-01",
            r#"<?php
$document = new DOMDocument();
$document->loadXML("<root xmlns:p=\"urn:p\"><p:item/><other/></root>");
$empty = $document->C14N(false, false, ["query" => "//none"]);
$prefix = $document->C14N(false, false, [
    "query" => "//p:item",
    "namespaces" => ["p" => "urn:p"],
]);
echo "empty|" . ($empty === false ? "F" : $empty) . "\n";
echo "prefix|" . $prefix;
"#,
            "empty|\nprefix|<p:item></p:item>",
        ),
        (
            "DOM-C14N-FILE-02",
            r#"<?php
$document = new DOMDocument();
$document->loadXML("<root xmlns:p=\"urn:p\"><p:item/><other/></root>");
$bytes = $document->C14NFile("dom-c14n-matrix.xml");
echo "file|" . $bytes . "|" . file_get_contents("dom-c14n-matrix.xml");
unlink("dom-c14n-matrix.xml");
"#,
            "file|61|<root xmlns:p=\"urn:p\"><p:item></p:item><other></other></root>",
        ),
        (
            "DOM-C14N-ERROR-03",
            r#"<?php
set_error_handler(function ($severity, $message) {
    echo "warning|" . $severity . "\n";
    return true;
});
$document = new DOMDocument();
$document->loadXML("<root/>");
$written = $document->C14NFile("missing-directory/output.xml");
echo "result|" . ($written === false ? "F" : "T");
restore_error_handler();
"#,
            "warning|2\nresult|F",
        ),
    ] {
        assert_eq!(compile_and_run(source), expected, "{case}");
    }
}
