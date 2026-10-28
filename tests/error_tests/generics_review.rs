//! Purpose:
//! Diagnostics for generic specialization limits and invalid declaration types.
//!
//! Called from:
//! - The error integration test harness.
//!
//! Key details:
//! - Reject true-only storage explicitly instead of widening it to bool.
//! - Preserve the prohibition of void properties while permitting null declarations.
//! - Specialization-limit errors identify the template instead of trailing executable code.

use super::{check_source_full, expect_error, expect_no_error};

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

/// A runaway class specialization reports its template even when executable code follows it.
#[test]
fn test_generic_class_runaway_reports_template_span() {
    let error = check_source_full(
        r#"<?php
class Node<T> {
    public function grow(): Node<array<T>> { return new Node<array<T>>(); }
}
$n = new Node<int>();
echo "done";
"#,
    )
    .err()
    .expect("recursive class specialization must exceed the round limit");
    assert!(error.message.contains("Generic class instantiation did not settle"), "{}", error.message);
    assert_eq!(error.span.line, 2);
    assert_eq!(error.span.col, 1);
}
