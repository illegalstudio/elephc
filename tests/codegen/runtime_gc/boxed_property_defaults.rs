//! Purpose:
//! Regression tests for issue #688: an array literal defaulting a `mixed` or union-typed
//! property allocates its container and then boxes it into a Mixed cell, and the box takes
//! its OWN reference — so the object-construction path must use the owned boxer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`.
//! - Objects are constructed in a LOOP on purpose: the plain boxer retains without
//!   releasing and leaks exactly one block per object, which a single construction hides.
//! - Both literal spellings are covered together — keyed (`["k" => 1]`) and positional
//!   (`[1, 2]`) reach different emitters — so a future change cannot fix one and lose the
//!   other's ownership.

use crate::support::compile_and_run_with_heap_debug;

/// Verifies the boxed array-literal defaults release the container they allocate.
///
/// Each object allocates the literal and boxes it into a Mixed cell, and the box takes its own
/// reference — so the OWNED boxer is required. The plain one retains without releasing and leaks
/// one block per object, which a single-iteration total hides.
#[test]
fn test_array_literal_defaults_on_union_properties_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class A { public ?array $x = ["k" => 1, "j" => "s"]; }
class B { public mixed $y = ["a" => 1]; }
class C { public ?array $z = [1, 2]; }
for ($i = 0; $i < 32; $i++) {
    $a = new A();
    $b = new B();
    $c = new C();
}
echo count($a->x), count($b->y), count($c->z), "\n";
"#,
    );
    assert_eq!(out.stdout, "212\n", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "boxed array-literal property default leaked: {}",
        out.stderr
    );
}
