//! Purpose:
//! Table-driven DOM/libxml diagnostic ordering, recovery, and exception regressions.
//!
//! Called from:
//! - `cargo test --test codegen_tests dom_diagnostics_matrix` through Rust's test harness.
//!
//! Key details:
//! - Cases consume diagnostics in PHP-visible order rather than reducing parse failures to booleans.
//! - The compact fixtures are intended for serial filtered execution while the DOM bridge is memory-sensitive.

use crate::support::compile_and_run;

/// Pins ordered libxml buffers, warning delivery, recoverable errors, and empty-source exceptions.
#[test]
fn dom_diagnostics_follow_php_oracle_matrix() {
    for (case, source, expected) in [
        (
            "DOM-DIAG-BUFFER-01",
            r#"<?php
libxml_use_internal_errors(true);
libxml_clear_errors();
$document = new DOMDocument();
$document->loadXML("<a>");
$document->loadXML("<b><c></b>");
echo "queue|";
foreach (libxml_get_errors() as $error) {
    echo $error->level . "/" . $error->code . "/" . $error->line . ";";
}
libxml_clear_errors();
echo "|clear|" . count(libxml_get_errors()) . "|"
    . (libxml_get_last_error() === false ? "N" : "E");
"#,
            "queue|3/77/1;3/76/1;|clear|0|N",
        ),
        (
            "DOM-DIAG-RECOVERY-02",
            r#"<?php
libxml_use_internal_errors(true);
$strict = new DOMDocument();
libxml_clear_errors();
$strictResult = $strict->loadXML("<root><child></root>");
$strictErrors = libxml_get_errors();

$recover = new DOMDocument();
libxml_clear_errors();
$recoverResult = $recover->loadXML("<root><child></root>", LIBXML_RECOVER);
$recoverErrors = libxml_get_errors();
echo "internal|" . ($strictResult ? "T" : "F") . "|" . count($strictErrors)
    . "|" . ($recoverResult ? "T" : "F") . "|" . $recover->documentElement->nodeName
    . "|" . count($recoverErrors) . "\n";

libxml_use_internal_errors(false);
set_error_handler(function ($severity, $message) {
    echo "warning|" . $severity . "\n";
    return true;
});
$warning = new DOMDocument();
echo "result|" . ($warning->loadXML("<root><child></root>") ? "T" : "F") . "|"
    . (libxml_get_last_error() === false ? "N" : "E");
restore_error_handler();
"#,
            "internal|F|1|T|root|1\nwarning|2\nresult|F|E",
        ),
        (
            "DOM-DIAG-ARGUMENT-03",
            r#"<?php
$document = new DOMDocument();
try {
    $document->loadXML("");
} catch (Throwable $error) {
    echo get_class($error) . "|" . $error->getMessage();
}
"#,
            "ValueError|DOMDocument::loadXML(): Argument #1 ($source) must not be empty",
        ),
    ] {
        assert_eq!(compile_and_run(source), expected, "{case}");
    }
}
