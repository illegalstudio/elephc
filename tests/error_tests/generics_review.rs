//! Purpose:
//! Diagnostics for unsupported true-only properties and invalid void declarations.
//!
//! Called from:
//! - The error integration test harness.
//!
//! Key details:
//! - Reject true-only storage explicitly instead of widening it to bool.
//! - Preserve the prohibition of void properties while permitting null declarations.

use super::{expect_error, expect_no_error};

/// Ordinary, nullable, union, and promoted true property types must never become bool slots.
#[test]
fn test_generics_review_rejects_true_property_declarations() {
    for source in [
        "<?php class C { public true $v; }",
        "<?php class C { public ?true $v; }",
        "<?php class C { public true|int $v; }",
        "<?php class C { public function __construct(public true $v) {} }",
    ] {
        expect_error(source, "Literal true property types are not supported");
    }
}

/// Null-only properties are distinct from void return annotations.
#[test]
fn test_generics_review_null_property_does_not_enable_void_properties() {
    expect_no_error("<?php class C { public null $v; } $c = new C(); $c->v = null;");
    expect_error("<?php class C { public void $v; }", "cannot use type void");
}


/// The promoted null exception must not admit void constructor parameters.
#[test]
fn test_generics_review_promoted_void_parameter_is_rejected() {
    expect_error("<?php class C { public function __construct(public void $v) {} }", "cannot use type void");
}
