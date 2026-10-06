//! Purpose:
//! Regression diagnostics for lexical receivers in stored and promoted property defaults.
//!
//! Called from:
//! - `tests/error_tests.rs` through the frontend diagnostic harness.
//!
//! Key details:
//! - Trait defaults are checked even without consumers, while parent binding is deferred.
//! - PHP distinguishes late-static class names from other late-static constants.

use super::*;

/// An unused trait may retain parent receivers until a consuming class provides its scope.
#[test]
fn test_property_default_scope_unused_trait_accepts_relative_receivers() {
    check_source("<?php trait T { public string $parentName = parent::class; public string $selfName = self::class; }")
        .expect("trait declaration alone does not bind a parent class");
}

/// Constructor promotion must reject late-static class names before backend lowering.
#[test]
fn test_property_default_scope_promoted_late_static() {
    for source in [
        "<?php class Probe { public function __construct(public string $name = static::class) {} }",
        "<?php class Probe { public function __construct(public array $names = [static::class]) {} }",
    ] {
        expect_error(source, "static::class cannot be used for compile-time class name resolution");
    }
}

/// A trait declaration is validated even when no class uses its properties or constructor.
#[test]
fn test_property_default_scope_unused_trait_late_static() {
    for source in [
        "<?php trait T { public string $name = static::class; } echo 'ok';",
        "<?php trait T { public array $names = [static::class]; } echo 'ok';",
        "<?php trait T { public function __construct(public string $name = static::class) {} }",
    ] {
        expect_error(source, "static::class cannot be used for compile-time class name resolution");
    }
}

/// Late-static constants use PHP's compile-time constant diagnostic in every property form.
#[test]
fn test_property_default_scope_late_static_constant_diagnostic() {
    for source in [
        "<?php class Probe { const A = 1; public int $value = static::A; }",
        "<?php class Probe { const A = 1; public static int $value = static::A; }",
        "<?php class Probe { const A = 1; public function __construct(public int $value = static::A) {} }",
        "<?php trait T { public int $value = static::A; }",
        "<?php class Probe { const A = 1; const B = static::A; }",
    ] {
        expect_error(source, "\"static::\" is not allowed in compile-time constants");
    }
}

/// A directly declared class property with no parent is a compile error with PHP's wording.
#[test]
fn test_property_default_scope_parent_without_parent() {
    for source in [
        "<?php class Probe { public string $name = parent::class; }",
        "<?php class Probe { public array $names = [parent::class]; }",
        "<?php class Probe { public function __construct(public string $name = parent::class) {} }",
        "<?php class Probe { public function value($name = parent::class) {} }",
        "<?php class Probe { public static function value(array $names = [parent::class]) {} }",
    ] {
        expect_error(source, "Cannot use \"parent\" when current class scope has no parent");
    }
}

/// Trait imports retain unresolved parent defaults without skipping declaration-time validation.
#[test]
fn test_property_default_scope_trait_consumer_accepts_unbound_parent() {
    for source in [
        "<?php trait T { public string $name = parent::class; } class Consumer { use T; }",
        "<?php trait T { public static string $name = parent::class; } class Consumer { use T; }",
        "<?php trait T { public function __construct(public string $name = parent::class) {} } class Consumer { use T; }",
    ] {
        check_source(source).expect("a trait import does not evaluate its unbound parent default");
    }
}
