//! Purpose:
//! Checker boundaries of a declared `array<K, V>` local written under a key inside a loop.
//!
//! Called from:
//! - The error integration test harness.
//!
//! Key details:
//! - The loop-storage fixed point runs before the body's guards are known. It used to widen a
//!   declared map to boxed values for any keyed write it could only type `mixed`, which both
//!   bypassed the declaration and boxed a map whose every write was guarded.
//! - A global or by-reference captured source is refused by name: its boxed cell can be
//!   rewritten behind the write, so its checked type is not the stored value.
//! - Runtime behavior is covered by `tests/codegen/arrays/element_value_views.rs`.

use super::{expect_error, expect_no_error};

/// A guard the loop-storage pass cannot see still proves the declared value type at the write.
#[test]
fn test_loop_guarded_keyed_write_keeps_declared_map_contract() {
    expect_no_error(
        "<?php final class Settings { private array<string, bool> $values = []; \
         public function load(mixed $data): void { array<string, bool> $values = []; \
         foreach ($data as $name => $value) { \
         if (!is_string($name) || !is_bool($value)) { throw new UnexpectedValueException('bad'); } \
         $values[$name] = $value; } \
         $this->values = $values; } }",
    );
}

/// An unguarded `mixed` value is refused in a loop exactly as it is in straight-line code.
#[test]
fn test_error_loop_unguarded_keyed_write_into_declared_map() {
    expect_error(
        "<?php function f(mixed $data): void { array<string, bool> $values = []; \
         foreach ($data as $name => $value) { $values[$name] = $value; } }",
        "cannot store mixed into $values declared as array<string, bool>",
    );
}

/// A loop-carried value of another scalar type is refused instead of boxing the declared map.
#[test]
fn test_error_loop_keyed_write_of_other_scalar_into_declared_map() {
    expect_error(
        "<?php function f(string $name): void { array<string, int> $counts = ['seed' => 1]; \
         $i = 0; while ($i < 2) { $counts['k' . $i] = $name; $i++; } }",
        "cannot store string into $counts declared as array<string, int>",
    );
}

/// A declared `mixed` value type imposes no contract, in a loop or outside one.
#[test]
fn test_loop_keyed_write_into_declared_mixed_map_stays_writable() {
    expect_no_error(
        "<?php function f(mixed $data): void { array<string, mixed> $values = []; \
         foreach ($data as $name => $value) { $values[$name] = $value; } }",
    );
}

/// An append keeps its widening rule: only keyed writes are held to the declared value type.
#[test]
fn test_loop_append_into_declared_map_still_widens() {
    expect_no_error(
        "<?php function f(mixed $data): void { array<string, int> $values = ['seed' => 1]; \
         foreach ($data as $value) { $values[] = $value; } }",
    );
}

/// A global is a boxed cell other code can replace, so its checked type proves nothing at the
/// write: the declared map refuses it and names the variable and the way out.
#[test]
fn test_error_keyed_write_of_global_into_declared_map() {
    expect_error(
        "<?php $limit = 5; function f(): void { global $limit; \
         array<string, int> $limits = ['seed' => 1]; $limits['max'] = $limit; }",
        "cannot store $limit into $limits declared as array<string, int>: a global or \
         by-reference captured variable is a boxed cell other code can replace",
    );
}

/// The cast the diagnostic asks for produces the declared value and is accepted.
#[test]
fn test_keyed_write_of_cast_global_into_declared_map() {
    expect_no_error(
        "<?php $limit = 5; function f(): void { global $limit; \
         array<string, int> $limits = ['seed' => 1]; $limits['max'] = (int) $limit; }",
    );
}
