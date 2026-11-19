//! Purpose:
//! Rejects writes to static properties from outside their asymmetric set visibility.
//!
//! Called from:
//! - `cargo test --test error_tests static_asymmetric_visibility`.
//!
//! Key details:
//! - PHP 8.5 permits `private(set)`/`protected(set)` on static properties; the checker
//!   must reject a global or unrelated-class write before codegen.

use super::*;

/// A global write to a `private(set)` static property is rejected.
#[test]
fn test_static_private_set_write_from_global_scope() {
    expect_error(
        r#"<?php
class A { public private(set) static int $x = 0; }
A::$x = 1;
"#,
        "Cannot access private(set) static property: A::x",
    );
}

/// An unrelated class cannot write a `protected(set)` static property.
#[test]
fn test_static_protected_set_write_from_unrelated_class() {
    expect_error(
        r#"<?php
class A { public protected(set) static int $x = 0; }
class B { public static function write(): void { A::$x = 1; } }
"#,
        "Cannot access protected(set) static property: A::x",
    );
}
