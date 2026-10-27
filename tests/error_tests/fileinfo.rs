//! Purpose:
//! Checks fileinfo arity and class/string argument diagnostics before linking.
//!
//! Called from:
//! - The compiler error integration suite.
//!
//! Key details:
//! - Requires no native artifacts or executable fixtures.

use super::*;

/// Invalid calls must fail at their PHP signatures rather than from missing declarations.
#[test]
fn test_fileinfo_signature_errors() {
    for source in [
        "<?php finfo_buffer();",
        "<?php finfo_buffer(1, 'hello');",
        "<?php finfo_file(new finfo());",
        "<?php finfo_set_flags(new finfo());",
        "<?php finfo_close();",
        "<?php mime_content_type();",
        "<?php new finfo(0, null, 1);",
    ] {
        let error = check_source(source).expect_err("fileinfo must validate arity/types");
        assert!(!error.contains("Undefined function") && !error.contains("Undefined class"), "{error}");
    }
}
