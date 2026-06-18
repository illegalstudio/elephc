//! Purpose:
//! End-to-end codegen tests for anonymous classes (`new class { ... }`), which are hoisted to
//! uniquely-named synthetic classes during parsing.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Covers bare bodies, constructor arguments, `extends`, `implements`, multiple distinct
//!   anonymous classes, instantiation inside a loop, and `readonly` anonymous classes.

use super::*;

/// Verifies that a bare anonymous class instantiates and dispatches a method.
#[test]
fn test_anonymous_class_basic() {
    let out = compile_and_run(
        "<?php
        $o = new class {
            public function value(): string { return \"anon\"; }
        };
        echo $o->value();
        ",
    );
    assert_eq!(out, "anon");
}

/// Verifies that an anonymous class receives constructor arguments.
#[test]
fn test_anonymous_class_constructor_args() {
    let out = compile_and_run(
        "<?php
        $o = new class(40, 2) {
            public int $sum;
            public function __construct(int $a, int $b) { $this->sum = $a + $b; }
        };
        echo $o->sum;
        ",
    );
    assert_eq!(out, "42");
}

/// Verifies that an anonymous class can extend an abstract base and satisfy its contract.
#[test]
fn test_anonymous_class_extends_abstract() {
    let out = compile_and_run(
        "<?php
        abstract class Base {
            abstract public function name(): string;
            public function greet(): string { return \"hi \" . $this->name(); }
        }
        $o = new class extends Base {
            public function name(): string { return \"anon\"; }
        };
        echo $o->greet();
        ",
    );
    assert_eq!(out, "hi anon");
}

/// Verifies that an anonymous class can implement an interface and be used through it.
#[test]
fn test_anonymous_class_implements_interface() {
    let out = compile_and_run(
        "<?php
        interface Speaker { public function speak(): string; }
        function make(): Speaker {
            return new class implements Speaker {
                public function speak(): string { return \"hello\"; }
            };
        }
        echo make()->speak();
        ",
    );
    assert_eq!(out, "hello");
}

/// Verifies that two distinct anonymous classes get independent synthetic identities.
#[test]
fn test_two_distinct_anonymous_classes() {
    let out = compile_and_run(
        "<?php
        $a = new class { public function x(): int { return 1; } };
        $b = new class { public function x(): int { return 2; } };
        echo $a->x() + $b->x();
        ",
    );
    assert_eq!(out, "3");
}

/// Verifies that an anonymous class defined inside a loop is hoisted once and instantiated each
/// iteration.
#[test]
fn test_anonymous_class_in_loop() {
    let out = compile_and_run(
        "<?php
        $sum = 0;
        for ($i = 0; $i < 3; $i++) {
            $o = new class { public function val(): int { return 10; } };
            $sum = $sum + $o->val();
        }
        echo $sum;
        ",
    );
    assert_eq!(out, "30");
}

/// Verifies that a `readonly` anonymous class with a promoted constructor property works.
#[test]
fn test_readonly_anonymous_class() {
    let out = compile_and_run(
        "<?php
        $o = new class(\"frozen\") {
            public function __construct(public readonly string $label) {}
        };
        echo $o->label;
        ",
    );
    assert_eq!(out, "frozen");
}

/// Compiles and runs the checked-in `examples/anonymous-classes/main.php` fixture, covering
/// interface-implementing anonymous classes (with and without constructor args) and one that
/// extends an abstract base.
#[test]
fn test_example_anonymous_classes_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/anonymous-classes/main.php"));
    assert_eq!(out, "HELLO!\n>> hello\n246\n");
}
