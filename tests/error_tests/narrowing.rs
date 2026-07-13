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

/// Verifies the fallthrough after `$value === false` does not remove a full bool member because
/// `true` remains possible.
#[test]
fn test_strict_false_guard_keeps_full_bool_member() {
    expect_error(
        "<?php function requireInt(int|bool $value): int { if ($value === false) { throw new Exception('false'); } return $value; }",
        "got Union([Int, Bool])",
    );
}

/// Verifies a direct property write clears a prior property narrowing before a later return.
#[test]
fn test_property_write_invalidates_narrowing() {
    expect_error(
        "<?php class W {} class Box { public function __construct(public ?W $value) {} } function read(Box $box): W { if (!$box->value instanceof W) { throw new Exception('missing'); } $box->value = null; return $box->value; }",
        "return type expects Object(\"W\"), got Union",
    );
}

/// Verifies rebinding the local receiver clears property facts tied to the old object.
#[test]
fn test_property_receiver_rebinding_invalidates_narrowing() {
    expect_error(
        "<?php class W {} class Box { public function __construct(public ?W $value) {} } function read(Box $box, Box $replacement): W { if (!$box->value instanceof W) { throw new Exception('missing'); } $box = $replacement; return $box->value; }",
        "return type expects Object(\"W\"), got Union",
    );
}

/// Verifies a hooked property is never treated as a stable flow binding across two reads.
#[test]
fn test_property_get_hook_is_not_persistently_narrowed() {
    expect_error(
        "<?php class W {} class Box { private ?W $stored; public function __construct(?W $stored) { $this->stored = $stored; } public ?W $value { get { $result = $this->stored; $this->stored = null; return $result; } } } function read(Box $box): W { if (!$box->value instanceof W) { throw new Exception('missing'); } return $box->value; }",
        "return type expects Object(\"W\"), got Union",
    );
}

/// Verifies an undeclared property served by `__get` is not treated as a stable flow binding.
#[test]
fn test_magic_get_property_is_not_persistently_narrowed() {
    expect_error(
        "<?php class W {} class Box { private ?W $stored; public function __construct(?W $stored) { $this->stored = $stored; } public function __get(string $name): ?W { $result = $this->stored; $this->stored = null; return $result; } } function read(Box $box): W { if (!$box->value instanceof W) { throw new Exception('missing'); } return $box->value; }",
        "return type expects Object(\"W\"), got Union",
    );
}
