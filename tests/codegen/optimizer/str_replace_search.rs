//! Purpose:
//! Verifies the `str_replace([...], ...)` array-search rewrite keeps PHP's replacement order.
//!
//! Called from:
//! - The optimizer codegen integration suite.
//!
//! Key details:
//! - The subject depends on `$argc` so the optimizer cannot fold the whole expression.
//! - Both search orders pin that the first array element is applied first at run time.

use super::*;

/// Replacement order for a literal array search matches sequential PHP `str_replace` calls.
#[test]
fn test_str_replace_literal_array_search_keeps_php_order() {
    let out = compile_and_run(
        r#"<?php
$subject = $argc > 0 ? "ab-b" : "";
echo str_replace(["ab", "b"], "x", $subject), "|";
echo str_replace(["b", "ab"], "x", "ab-b"), "|";
"#,
    );
    assert_eq!(out, "x-x|ax-x|");
}
