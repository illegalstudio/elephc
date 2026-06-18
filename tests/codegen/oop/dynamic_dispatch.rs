//! Purpose:
//! End-to-end codegen tests for dynamic method and static calls (`$obj->$method()`,
//! `$cls::method()`), which desugar to `call_user_func([$recv, $name], ...args)`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Covers instance and static receivers, dynamic and literal member names, and arguments.
//! - Regular method/static calls and dynamic property access must remain unaffected.

use super::*;

/// Verifies that `$obj->$method()` dispatches to the named instance method.
#[test]
fn test_dynamic_instance_method_call() {
    let out = compile_and_run(
        "<?php
        class Greeter {
            public function hello(): string { return \"hello\"; }
        }
        $g = new Greeter();
        $m = \"hello\";
        echo $g->$m();
        ",
    );
    assert_eq!(out, "hello");
}

/// Verifies that a dynamic instance method call forwards arguments.
#[test]
fn test_dynamic_instance_method_call_with_args() {
    let out = compile_and_run(
        "<?php
        class Calc {
            public function add(int $a, int $b): int { return $a + $b; }
        }
        $c = new Calc();
        $op = \"add\";
        echo $c->$op(15, 27);
        ",
    );
    assert_eq!(out, "42");
}

/// Verifies that the brace form `$obj->{$expr}()` also dispatches dynamically.
#[test]
fn test_dynamic_instance_method_brace_form() {
    let out = compile_and_run(
        "<?php
        class C { public function run(): string { return \"r\"; } }
        $c = new C();
        $names = [\"run\"];
        echo $c->{$names[0]}();
        ",
    );
    assert_eq!(out, "r");
}

/// Verifies that `$cls::method()` dispatches to a static method on the named class.
#[test]
fn test_dynamic_static_call_literal_method() {
    let out = compile_and_run(
        "<?php
        class Factory {
            public static function build(): string { return \"built\"; }
        }
        $cls = \"Factory\";
        echo $cls::build();
        ",
    );
    assert_eq!(out, "built");
}

/// Verifies that `$cls::$method(args)` (both dynamic) dispatches a static method with arguments.
#[test]
fn test_dynamic_static_call_dynamic_method_with_args() {
    let out = compile_and_run(
        "<?php
        class Math {
            public static function triple(int $n): int { return $n * 3; }
        }
        $cls = \"Math\";
        $m = \"triple\";
        echo $cls::$m(14);
        ",
    );
    assert_eq!(out, "42");
}

/// Verifies that ordinary (non-dynamic) method and static calls are unaffected.
#[test]
fn test_static_and_instance_calls_unaffected() {
    let out = compile_and_run(
        "<?php
        class C {
            const TAG = \"T\";
            public function inst(): string { return \"i\"; }
            public static function stat(): string { return \"s\"; }
        }
        echo (new C())->inst() . C::stat() . C::TAG;
        ",
    );
    assert_eq!(out, "isT");
}

/// Compiles and runs the checked-in `examples/dynamic-dispatch/main.php` fixture, covering
/// dynamic instance method dispatch by name and a dynamic static call.
#[test]
fn test_example_dynamic_dispatch_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/dynamic-dispatch/main.php"));
    assert_eq!(out, "Hello, world\nLOUD!\ncommands: greet, shout\n");
}

/// Verifies a dynamic static method call on a literal class name (`C::$method()`), which has a
/// dynamic method name but a statically-named class. Regression test — this form previously
/// failed to parse with "Expected ';'".
#[test]
fn test_dynamic_static_call_on_literal_class() {
    let out = compile_and_run(
        "<?php
        class C { public static function greet(): string { return \"hi\"; } }
        $m = \"greet\";
        echo C::$m();
        ",
    );
    assert_eq!(out, "hi");
}

/// Verifies that a dynamic static call on a literal class name forwards arguments correctly.
#[test]
fn test_dynamic_static_call_on_literal_class_with_args() {
    let out = compile_and_run(
        "<?php
        class Calc { public static function add(int $a, int $b): int { return $a + $b; } }
        $op = \"add\";
        echo Calc::$op(4, 5);
        ",
    );
    assert_eq!(out, "9");
}
