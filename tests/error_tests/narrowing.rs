//! Purpose:
//! Regression tests for sound flow-sensitive narrowing diagnostics.
//!
//! Called from:
//! - `cargo test --test error_tests` through Rust's test harness.
//!
//! Key details:
//! - Negative fixtures ensure literal-false and property facts are not retained beyond mutations,
//!   receiver rebindings, or user-code property getters.

use super::*;

/// Verifies the literal `false` parameter type rejects `true` rather than widening to bool.
#[test]
fn test_literal_false_parameter_rejects_true() {
    expect_error(
        "<?php function onlyFalse(false $value): void {} onlyFalse(true);",
        "expects False, got Bool",
    );
}

/// Verifies a remaining bool member is accepted for PHP's runtime int coercion.
#[test]
fn test_strict_false_guard_uses_runtime_return_coercion() {
    expect_no_error(
        "<?php function requireInt(int|bool $value): int { if ($value === false) { throw new Exception('false'); } return $value; }",
    );
}

/// Verifies a direct property write leaves validation to the runtime return boundary.
#[test]
fn test_property_write_runtime_boundary_is_accepted() {
    expect_no_error(
        "<?php class W {} class Box { public function __construct(public ?W $value) {} } function read(Box $box): W { if (!$box->value instanceof W) { throw new Exception('missing'); } $box->value = null; return $box->value; }",
    );
}

/// Verifies a rebound receiver is checked at the runtime return boundary.
#[test]
fn test_property_receiver_rebinding_runtime_boundary_is_accepted() {
    expect_no_error(
        "<?php class W {} class Box { public function __construct(public ?W $value) {} } function read(Box $box, Box $replacement): W { if (!$box->value instanceof W) { throw new Exception('missing'); } $box = $replacement; return $box->value; }",
    );
}

/// Verifies a hooked property's second read is accepted for runtime return validation.
#[test]
fn test_property_get_hook_runtime_boundary_is_accepted() {
    expect_no_error(
        "<?php class W {} class Box { private ?W $stored; public function __construct(?W $stored) { $this->stored = $stored; } public ?W $value { get { $result = $this->stored; $this->stored = null; return $result; } } } function read(Box $box): W { if (!$box->value instanceof W) { throw new Exception('missing'); } return $box->value; }",
    );
}

/// Verifies a magic property's second read is accepted for runtime return validation.
#[test]
fn test_magic_get_property_runtime_boundary_is_accepted() {
    expect_no_error(
        "<?php class W {} class Box { private ?W $stored; public function __construct(?W $stored) { $this->stored = $stored; } public function __get(string $name): ?W { $result = $this->stored; $this->stored = null; return $result; } } function read(Box $box): W { if (!$box->value instanceof W) { throw new Exception('missing'); } return $box->value; }",
    );
}

/// Verifies the post-guard narrowing is NOT kept when a nested branch inside the null guard can
/// fall through: the inner `if` has no `else`, so reaching the code after the guard does not imply
/// the guard was false and `?array` must stay a union (issue #590 negative case).
#[test]
fn test_no_narrow_when_nested_branch_falls_through() {
    expect_error(
        "<?php function consume(?array $entry, bool $flag): void { if ($entry === null) { if ($flag) { return; } } [$key, $value] = $entry; }",
        "List unpacking requires an array",
    );
}

/// Verifies the narrowing is NOT kept when a nested `switch` in the null guard has no `default`, so
/// a subject matching no case falls through to the code after the guard.
#[test]
fn test_no_narrow_when_switch_has_no_default() {
    expect_error(
        "<?php function consume(?array $entry, int $mode): void { if ($entry === null) { switch ($mode) { case 1: return; } } [$key, $value] = $entry; }",
        "List unpacking requires an array",
    );
}

/// Verifies a nested diverging call in only one arm does not make the enclosing `if` terminal.
#[test]
fn test_no_narrow_when_nested_exit_branch_falls_through() {
    expect_error(
        "<?php function consume(?array $entry, bool $flag): void { if ($entry === null) { if ($flag) { exit(1); } } [$key, $value] = $entry; }",
        "List unpacking requires an array",
    );
}

/// Verifies a literal-true loop that can break may still fall through to the code after the guard.
#[test]
fn test_no_narrow_when_literal_true_loop_can_break() {
    expect_error(
        "<?php function consume(?array $entry, bool $flag): void { if ($entry === null) { while (true) { if ($flag) { break; } } } [$key, $value] = $entry; }",
        "List unpacking requires an array",
    );
}

/// Verifies a static-property narrowing does not survive an intervening call that could reassign
/// it: PHP raises a `TypeError` at the runtime return boundary.
#[test]
fn test_static_property_after_intervening_call_is_runtime_checked() {
    expect_no_error(
        r#"<?php
class S {
    private static ?S $inst = null;
    private static function wipe(): void { self::$inst = null; }
    public static function get(): S {
        if (self::$inst === null) { self::$inst = new S(); }
        self::wipe();
        return self::$inst;
    }
}
"#,
    );
}

/// Verifies return-type validation is flow-sensitive: a `return` placed BEFORE the guard that
/// establishes the narrowing must not borrow that later fact.
#[test]
fn test_earlier_nullable_property_return_is_runtime_checked() {
    expect_no_error(
        r#"<?php
class A {
    public ?A $p = null;
    public function f(bool $c): A {
        if ($c) { return $this->p; }
        if ($this->p === null) { throw new Exception("x"); }
        return $this->p;
    }
}
"#,
    );
}

/// Verifies a nullable static property reaches the non-null runtime return boundary.
#[test]
fn test_unguarded_nullable_static_property_return_is_runtime_checked() {
    expect_no_error(
        r#"<?php
class S {
    private static ?S $inst = null;
    public static function get(): S { return self::$inst; }
}
"#,
    );
}

/// Verifies `static::$p` is not narrowed: late static binding can select a subclass that
/// redeclares the property, so the guarded fact does not describe the storage a later read hits.
#[test]
fn test_late_static_bound_property_is_runtime_checked() {
    expect_no_error(
        r#"<?php
class S {
    protected static ?S $inst = null;
    public static function get(): S {
        if (static::$inst === null) { static::$inst = new S(); }
        return static::$inst;
    }
}
"#,
    );
}
