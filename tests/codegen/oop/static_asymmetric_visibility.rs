//! Purpose:
//! Verifies PHP 8.5 asymmetric write visibility on static properties.
//!
//! Called from:
//! - `cargo test --test codegen_tests static_asymmetric_visibility`.
//!
//! Key details:
//! - `private(set)` is writable from the declaring class only and exposed to reflection
//!   as `IS_PRIVATE_SET`; `protected(set)` remains writable from child classes.

use crate::support::*;

/// Writes private(set) and protected(set) static properties from their allowed scopes.
#[test]
fn test_static_asymmetric_visibility_writes() {
    let out = compile_and_run(
        r#"<?php
class A {
    public private(set) static int $x = 0;
    public protected(set) static int $y = 1;

    public static function set(int $value): void {
        self::$x = $value;
        self::$y = $value;
    }
}

class B extends A {
    public static function setY(int $value): void {
        static::$y = $value;
    }
}

A::set(5);
B::setY(7);
echo A::$x, ':', A::$y;
"#,
    );
    assert_eq!(out, "5:7");
}

/// Exposes the static set visibility through the ReflectionProperty surface.
#[test]
fn test_static_asymmetric_visibility_reflection() {
    let out = compile_and_run(
        r#"<?php
class A {
    public private(set) static int $x = 0;
}

$p = new ReflectionProperty('A', 'x');
echo ($p->getModifiers() & ReflectionProperty::IS_PRIVATE_SET) !== 0 ? 'ps' : '-';
echo ':', $p->isPrivateSet() ? 'ps' : '-';
echo ':', $p->isWritable('A') ? 'w' : '-';
echo ':', $p->isWritable(null) ? 'w' : '-';
"#,
    );
    assert_eq!(out, "ps:ps:w:-");
}
