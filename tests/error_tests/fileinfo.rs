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
        "<?php finfo_file(new finfo());",
        "<?php finfo_set_flags(new finfo());",
        "<?php finfo_close();",
        "<?php mime_content_type();",
    ] {
        let error = check_source(source).expect_err("fileinfo must validate arity/types");
        assert!(!error.contains("Undefined function") && !error.contains("Undefined class"), "{error}");
    }
}

/// A strict file compares fileinfo's declared parameters, not the prelude's widened ABI slots.
#[test]
fn test_fileinfo_strict_types_reject_weak_scalars() {
    for source in [
        "<?php declare(strict_types=1); finfo_open(\"1\");",
        "<?php declare(strict_types=1); finfo_open(FILEINFO_NONE, 42);",
        "<?php declare(strict_types=1); new finfo(\"1\");",
    ] {
        let error = check_source(source).expect_err("strict fileinfo must reject a weak scalar");
        assert!(error.contains("declare(strict_types=1)"), "{error}");
    }
}

/// Strict files reject a Stringable object for the `?string` database parameter: PHP performs
/// no object-to-string conversion under `strict_types`.
#[test]
fn test_fileinfo_strict_types_reject_stringable_database() {
    let error = check_source(
        "<?php declare(strict_types=1); class S { public function __toString(): string { return 'x'; } } finfo_open(0, new S());",
    )
    .expect_err("strict fileinfo must reject a Stringable object");
    assert!(error.contains("declare(strict_types=1)"), "{error}");
}

/// Strict files reject a weak string for the widened `finfo_set_flags()` and method slots.
#[test]
fn test_fileinfo_strict_types_reject_set_flags_string() {
    for source in [
        "<?php declare(strict_types=1); $f = new finfo(); finfo_set_flags($f, \"1\");",
        "<?php declare(strict_types=1); $f = new finfo(); $f->set_flags(\"1\");",
    ] {
        let error = check_source(source).expect_err("strict set_flags must reject a string");
        assert!(error.contains("declare(strict_types=1)"), "{error}");
    }
}
