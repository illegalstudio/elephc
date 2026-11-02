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

/// Closure and arrow parent class-name defaults are legal declarations without a parent.
#[test]
fn test_property_default_sixth_review_parentless_closure_default() {
    for value in ["function(string $name = parent::class): string { return $name; }",
        "fn(string $name = parent::class): string => $name"] {
        check_source(&format!("<?php class Probe {{ public function make(): Closure {{ return {value}; }} }}"))
            .expect("a missing parent is evaluated only when the closure argument is omitted");
    }
}

/// A lazy lexical failure cannot hide forbidden late-static receivers in another array entry.
#[test]
fn test_property_default_seventh_review_closure_late_static_masking() {
    for source in [
        "<?php $value = fn(array $items = [self::class, static::class]): array => $items;",
        "<?php class Probe { public function make(): Closure { return fn(array $items = [parent::A, static::class]): array => $items; } }",
        "<?php trait Factory { public function make(): Closure { return fn(array $items = [parent::A, static::A]): array => $items; } } class Probe { use Factory; }",
    ] {
        let message = if source.contains("static::class") {
            "static::class cannot be used for compile-time class name resolution"
        } else { "\"static::\" is not allowed in compile-time constants" };
        expect_error(source, message);
    }
}

/// A lazy parent constant cannot mask a later hard parent class-name error in method defaults.
#[test]
fn test_property_default_third_review_method_parent_masking() {
    for default in ["[parent::A, parent::class]", "[parent::A, [parent::class]]",
        "[parent::A => parent::class]"] {
        for source in [
            format!("<?php class Probe {{ public function value($names = {default}) {{}} }}"),
            format!("<?php interface Probe {{ public function value($names = {default}); }}"),
            format!("<?php enum Probe {{ case Item; public function value($names = {default}) {{}} }}"),
            format!("<?php namespace Named {{ class Probe {{ public static function value($names = {default}) {{}} }} }}"),
        ] {
            expect_error(&source, "Cannot use \"parent\" when current class scope has no parent");
        }
    }
}

/// Direct instance and static property arrays inspect the entire default before deferring errors.
#[test]
fn test_property_default_third_review_property_parent_masking() {
    for default in ["[parent::A, parent::class]", "[parent::A, [parent::class]]",
        "[parent::A => parent::class]"] {
        for modifier in ["", "static "] {
            let source = format!("<?php class Probe {{ public {modifier}array $names = {default}; }}");
            expect_error(&source, "Cannot use \"parent\" when current class scope has no parent");
        }
    }
}

/// A direct promoted default rejects hard scope errors after an otherwise lazy parent access.
#[test]
fn test_property_default_third_review_promoted_parent_masking() {
    for default in ["[parent::A, parent::class]", "[parent::A, [parent::class]]",
        "[parent::A => parent::class]"] {
        let source = format!("<?php class Probe {{ public function __construct(public array $names = {default}) {{}} }}");
        expect_error(&source, "Cannot use \"parent\" when current class scope has no parent");
    }
}

/// A bound parent keeps mixed constant/name property arrays valid in the frontend.
#[test]
fn test_property_default_third_review_bound_property_scope() {
    check_source("<?php class Base { const A = 1; } class Probe extends Base { public array $names = [parent::A, parent::class]; }")
        .expect("both parent receivers have a declaration scope");
}

/// Ordinary method defaults reject late-static class names and constants on all class-like owners.
#[test]
fn test_property_default_second_review_method_late_static() {
    for (default, message) in [
        ("static::class", "static::class cannot be used for compile-time class name resolution"),
        ("static::A", "\"static::\" is not allowed in compile-time constants"),
        ("[static::class]", "static::class cannot be used for compile-time class name resolution"),
        ("[parent::A, static::class]", "static::class cannot be used for compile-time class name resolution"),
    ] {
        for source in [
            format!("<?php class Probe {{ const A = 1; public function value($name = {default}) {{}} }}"),
            format!("<?php interface Probe {{ const A = 1; public function value($name = {default}); }}"),
            format!("<?php enum Probe {{ case Item; const A = 1; public static function value($name = {default}) {{}} }}"),
            format!("<?php trait Probe {{ const A = 1; public function value($name = {default}) {{}} }}"),
            format!("<?php namespace Named {{ class Probe {{ const A = 1; public static function value($name = {default}) {{}} }} }}"),
        ] {
            expect_error(&source, message);
        }
    }
}

/// Late-static object factories are forbidden in callable defaults at declaration time.
#[test]
fn test_property_default_grok_second_review_new_static() {
    for source in [
        "<?php class Probe { public function value($object = new static) {} }",
        "<?php $closure = function ($object = new static) {};",
        "<?php $arrow = fn($object = new static) => $object;",
        "<?php function value($object = new static) {}",
        "<?php function value($object = [new self, new static]) {}",
    ] {
        expect_error(source, "\"static\" is not allowed in compile-time constants");
    }
}

/// A literal array default is still incompatible with a scalar parameter after normalization.
#[test]
fn test_property_default_grok_third_review_free_function_literal_type() {
    expect_error(
        "<?php function value(int $value = []): int { return $value; }",
        "parameter $value expects int",
    );
}

/// Free functions reject relative class-name literals even though closures defer them.
#[test]
fn test_property_default_grok_third_review_free_function_class_names() {
    for (default, keyword) in [("self::class", "self"), ("parent::class", "parent"),
        ("[new self, self::class]", "self"), ("[new parent, parent::class]", "parent")] {
        expect_error(
            &format!("<?php function value($value = {default}) {{}}"),
            &format!("Cannot use \"{keyword}\" when no class scope is active"),
        );
    }
}

/// Direct interface and enum defaults cannot bind a nonexistent parent class.
#[test]
fn test_property_default_oct9_classlike_method_parent() {
    for source in [
        "<?php interface Probe { public function value($name = parent::class); }",
        "<?php interface ParentContract {} interface Probe extends ParentContract { public function value($name = [parent::class]); }",
        "<?php enum Probe { case Item; public function value($name = parent::class) {} }",
        "<?php namespace Named { enum Probe { case Item; public static function value($name = [parent::class]) {} } }",
    ] {
        expect_error(source, "Cannot use \"parent\" when current class scope has no parent");
    }
}

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
