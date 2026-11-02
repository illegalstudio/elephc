//! Purpose:
//! End-to-end coverage for `filter_var()` in dynamically evaluated source.
//!
//! Called from:
//! - `tests/codegen/mod.rs` through the codegen integration suite.

use crate::support::*;

/// Eval supports the scalar validation filters and exposes the shared function
/// contract to dynamic lookup.
#[test]
fn test_eval_filter_var_scalar_validation_filters() {
    let out = compile_and_run(
        r#"<?php
eval('$bool = filter_var("yes", FILTER_VALIDATE_BOOL);
$int = filter_var("42", FILTER_VALIDATE_INT);
$float = filter_var("1.5e2", FILTER_VALIDATE_FLOAT);
echo ($bool ? "Y" : "?") . ":" . $int . ":" . $float . ":" . (function_exists("filter_var") ? "F" : "?");');
"#,
    );
    assert_eq!(out, "Y:42:150:F");
}
