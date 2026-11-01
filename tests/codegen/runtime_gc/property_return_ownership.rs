//! Purpose:
//! Regression tests for the ownership of a property container returned by value.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - A property's array is owned by the OBJECT. A function returning `$this->items` must hand the
//!   caller its own reference, so the caller's release cannot free the object's storage. Symfony's
//!   `Definition::getErrors()` returned its `errors` array this way; the frame epilogue released the
//!   object's only reference and the next call read reused memory (a `ServiceLocator` class string).
//!   The heap-debug run pins both halves: the second read still sees the array and the extra
//!   reference is balanced (no leak).

use crate::support::*;

/// Verifies a returned property array survives the caller releasing it, without leaking.
#[test]
fn test_returned_property_array_keeps_the_object_storage_alive() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
final class Bag {
    private array $items = [];
    public function add(string $value): static { $this->items[] = $value; return $this; }
    public function items(): array { return $this->items; }
}
$bag = new Bag();
$bag->add('a')->add('b');
$first = $bag->items();
unset($first);
for ($i = 0; $i < 2000; $i++) { $junk = 'Symfony\\Component\\DependencyInjection\\ServiceLocator' . $i; }
$second = $bag->items();
echo json_encode($second), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "[\"a\",\"b\"]\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}
