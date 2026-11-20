//! Purpose:
//! Checks ext/filter arity and signature diagnostics before linking.
//!
//! Called from:
//! - The compiler error integration suite.
//!
//! Key details:
//! - The synthetic filter prelude is injected by the error harness for these calls.

use super::*;

/// Invalid filter calls fail at their PHP signatures instead of as undefined functions.
#[test]
fn test_filter_signature_errors() {
    for (source, expected) in [
        ("<?php filter_var();", "expects 1 to 3 arguments"),
        (
            "<?php filter_input_array();",
            "expects 1 to 3 arguments",
        ),
        (
            "<?php filter_input(INPUT_GET);",
            "expects 2 to 4 arguments",
        ),
        (
            "<?php filter_has_var(INPUT_GET);",
            "expects 2 arguments",
        ),
    ] {
        let error = check_source(source).expect_err("filter must validate arity");
        assert!(!error.contains("Undefined function"), "{source}: {error}");
        assert!(error.contains(expected), "{source}: {error}");
    }
}
