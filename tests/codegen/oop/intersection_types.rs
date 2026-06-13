//! Purpose:
//! End-to-end codegen tests for intersection type syntax (`A&B`). elephc parses the syntax and
//! types the value as its first listed member.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The value is typed as its first member, so member access resolves against that member.
//! - The by-reference marker (`int &$x`) must remain distinct from an intersection.

use super::*;

/// Verifies that an intersection parameter type accepts an implementor and that the first
/// member's method is callable.
#[test]
fn test_intersection_param_first_member_method() {
    let out = compile_and_run(
        "<?php
        interface Identified { public function id(): int; }
        interface Named { public function label(): string; }
        class Entity implements Identified, Named {
            public function id(): int { return 7; }
            public function label(): string { return \"e\"; }
        }
        function take(Identified&Named $x): int { return $x->id(); }
        echo take(new Entity());
        ",
    );
    assert_eq!(out, "7");
}

/// Verifies that an intersection return type returns an implementor whose first-member method is
/// callable on the result.
#[test]
fn test_intersection_return_type() {
    let out = compile_and_run(
        "<?php
        interface Identified { public function id(): int; }
        interface Named {}
        class Entity implements Identified, Named {
            public function id(): int { return 9; }
        }
        function make(): Identified&Named { return new Entity(); }
        echo make()->id();
        ",
    );
    assert_eq!(out, "9");
}

/// Verifies that a by-reference parameter (`&$x`) is unaffected by intersection parsing.
#[test]
fn test_byref_param_still_mutates() {
    let out = compile_and_run(
        "<?php
        function bump(int &$x): void { $x = $x + 1; }
        $n = 41;
        bump($n);
        echo $n;
        ",
    );
    assert_eq!(out, "42");
}

/// Compiles and runs the checked-in `examples/intersection-types/main.php` fixture.
#[test]
fn test_example_intersection_types_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/intersection-types/main.php"));
    assert_eq!(out, "42\n");
}
