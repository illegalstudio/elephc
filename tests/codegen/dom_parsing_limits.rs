//! Purpose:
//! Table-driven DOM parsing, encoding, parser-option, and post-failure-state regressions.
//!
//! Called from:
//! - `cargo test --test codegen_tests dom_parsing_limits` through Rust's test harness.
//!
//! Key details:
//! - Each compact oracle case is deliberately independent so the costly bridge suite can run one filter at a time.
//! - Expectations pin PHP 8.5.8 with libxml2 2.15.3, including retained documents after parse failures.

use crate::support::compile_and_run;

/// Pins BOM, NUL, malformed UTF-8, recovery, PARSEHUGE, and NO_XXE parser contracts.
#[test]
fn dom_parsing_limits_match_php_oracle_matrix() {
    for (case, source, expected) in [
        (
            "DOM-PARSE-ENCODING-01",
            r#"<?php
libxml_use_internal_errors(true);
$document = new DOMDocument();
echo "bom|" . ($document->loadXML("\xEF\xBB\xBF<?xml version=\"1.0\"?><root/>") ? "T" : "F")
    . "|" . $document->documentElement->nodeName . "\n";

$document->loadXML("<before/>");
libxml_clear_errors();
$nul = $document->loadXML("<after/>\0");
$nulError = libxml_get_last_error();
echo "nul|" . ($nul ? "T" : "F") . "|" . $document->documentElement->nodeName
    . "|" . $nulError->code . "\n";

$invalid = new DOMDocument();
libxml_clear_errors();
$loaded = $invalid->loadXML("<root>\xC3\x28</root>");
$invalidError = libxml_get_last_error();
echo "utf8|" . ($loaded ? "T" : "F") . "|"
    . ($invalid->documentElement === null ? "N" : "X") . "|" . $invalidError->code;
"#,
            "bom|T|root\nnul|F|before|5\nutf8|F|N|81",
        ),
        (
            "DOM-PARSE-OPTIONS-02",
            r#"<?php
class NoXxeLoader {
    public mixed $context;
    public static int $calls = 0;

    public function __invoke($public, $system, $context): mixed {
        self::$calls++;
        return null;
    }
}

$loader = new NoXxeLoader();
libxml_set_external_entity_loader($loader);
libxml_use_internal_errors(true);
libxml_clear_errors();
$document = new DOMDocument();
$huge = $document->loadXML("<root/>", LIBXML_PARSEHUGE);
$blocked = $document->loadXML(
    "<!DOCTYPE root SYSTEM \"memory://blocked.dtd\"><root/>",
    LIBXML_DTDLOAD | LIBXML_NO_XXE,
);
echo "flags|" . ($huge ? "T" : "F") . "|" . ($blocked ? "T" : "F")
    . "|" . NoXxeLoader::$calls . "|" . count(libxml_get_errors());
libxml_set_external_entity_loader(null);
"#,
            "flags|T|T|0|0",
        ),
        (
            "DOM-PARSE-RECOVERY-03",
            r#"<?php
libxml_use_internal_errors(true);
$strict = new DOMDocument();
libxml_clear_errors();
$strictResult = $strict->loadXML("<root><child></root>");
$strictError = libxml_get_last_error();

$recover = new DOMDocument();
libxml_clear_errors();
$recoverResult = $recover->loadXML("<root><child></root>", LIBXML_RECOVER);
$recoverError = libxml_get_last_error();
echo "strict|" . ($strictResult ? "T" : "F") . "|"
    . ($strict->documentElement === null ? "N" : "X") . "|"
    . $strictError->level . "/" . $strictError->code . "\n";
echo "recover|" . ($recoverResult ? "T" : "F") . "|"
    . $recover->documentElement->nodeName . "|"
    . $recoverError->level . "/" . $recoverError->code;
"#,
            "strict|F|N|3/76\nrecover|T|root|3/76",
        ),
    ] {
        assert_eq!(compile_and_run(source), expected, "{case}");
    }
}
