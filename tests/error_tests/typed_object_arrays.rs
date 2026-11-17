//! Purpose:
//! Checker boundaries of the nominal object upcast that declared `array<Base>` builders keep.
//!
//! Called from:
//! - The error integration test harness.
//!
//! Key details:
//! - A subclass instance keeps the declared object element storage in loops too; runtime
//!   behavior is covered by `tests/codegen/arrays/object_element_upcasts.rs`.
//! - An unrelated class is not an upcast: an offset write is rejected at the write, and a loop
//!   append still boxes the builder, so the typed property store reports the layout mismatch.

use super::{expect_error, expect_no_error};

/// Prefixes `body` with an abstract `Actor`, its `Player` subclass and an unrelated `Stranger`.
fn actors(body: &str) -> String {
    format!(
        "<?php abstract class Actor {{ abstract public function value(): int; }} \
         final class Player extends Actor {{ public function value(): int {{ return 42; }} }} \
         final class Stranger {{ public function value(): int {{ return 1; }} }} {body}"
    )
}

/// The polymorphic lifetime loop type-checks: appending a declared `Actor` local holding a
/// `Player` must not make the loop box the `array<Actor>` builder before its header, which
/// rejected the property store with `expects array<Actor>, got array<mixed>`.
#[test]
fn test_loop_subclass_append_keeps_declared_object_array_contract() {
    expect_no_error(&actors(
        "final class L { private array<Actor> $items = []; \
         public function fill(): void { array<Actor> $items = []; \
         for ($i = 0; $i < 2; $i++) { Actor $p = new Player(); $items[] = $p; } \
         $this->items = $items; } }",
    ));
}

/// An unrelated class is not an upcast: its loop append still boxes the builder, so the typed
/// property store keeps reporting the element layout mismatch.
#[test]
fn test_error_loop_unrelated_append_still_boxes_declared_object_array() {
    expect_error(
        &actors(
            "final class L { private array<Actor> $items = []; \
             public function fill(): void { array<Actor> $items = []; \
             for ($i = 0; $i < 2; $i++) { $items[] = new Stranger(); } \
             $this->items = $items; } }",
        ),
        "Property L::$items expects array<Actor>, got array<mixed>",
    );
}

/// An offset write of an unrelated class is rejected against the declared element type.
#[test]
fn test_error_unrelated_offset_write_rejects_declared_object_array() {
    expect_error(
        &actors("array<Actor> $items = [new Player()]; $items[0] = new Stranger();"),
        "cannot store Stranger into $items declared as array<Actor>",
    );
}
