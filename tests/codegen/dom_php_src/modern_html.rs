//! Purpose:
//! PHP 8.5.8 `ext/dom` php-src tests for php-src's `modern/html` folder, as Elephc codegen tests.
//!
//! Called from:
//! - `cargo test` through `tests/codegen/dom_php_src/mod.rs`.
//!
//! Key details:
//! - Each test keeps the php-src program and expected output verbatim; its doc
//!   comment names the PHPT it was migrated from.
//! - A test Elephc does not pass yet is `#[ignore]`d with the missing behaviour, and
//!   php-src's own environment skips keep their `SKIPIF` message.

use super::*;

/// php-src `ext/dom/tests/modern/html/gh21688.phpt`: GH-21688 (SEGV in C14N on empty HTMLDocument)
#[test]
#[ignore = r#"DOM port pending: Undefined class: Dom\HTMLDocument"#]
fn test_gh21688() {
    if skip_without_xml_native("test_gh21688") {
        return;
    }
    let out = run_case(r#"modern/html"#, &[], r#"<?php
$dom = Dom\HTMLDocument::createEmpty();
var_dump($dom->C14N());
"#);
    assert_exact(&out, r#"string(0) """#);
}
