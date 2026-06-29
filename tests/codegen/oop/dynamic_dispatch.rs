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

/// Regression: a function with *no* return type annotation whose body returns a
/// method call on a `mixed` receiver must infer a `mixed` return type, not `int`.
/// Previously the method-call-on-`mixed` inference fell back to `int`, so the
/// inferred function return type coerced the boxed string result to `0`.
#[test]
fn test_inferred_return_of_mixed_receiver_method_keeps_string() {
    let out = compile_and_run(
        "<?php
        class Foo { public function hi(): string { return \"hi\"; } }
        function call_it(mixed $x) { return $x->hi(); }
        echo \"[\", call_it(new Foo()), \"]\";
        ",
    );
    assert_eq!(out, "[hi]");
}

/// Regression: the same inferred-return path must still carry integer results
/// correctly (the `mixed` widening must not break the scalar case).
#[test]
fn test_inferred_return_of_mixed_receiver_method_keeps_int() {
    let out = compile_and_run(
        "<?php
        class Counter { public function n(): int { return 7; } }
        function get_n(mixed $x) { return $x->n(); }
        echo get_n(new Counter()) + 1;
        ",
    );
    assert_eq!(out, "8");
}

/// Regression: an inferred `mixed` return flowing into string concatenation must
/// materialize the boxed string, not a coerced scalar.
#[test]
fn test_inferred_mixed_receiver_method_in_concat() {
    let out = compile_and_run(
        "<?php
        class Foo { public function name(): string { return \"world\"; } }
        function call_it(mixed $x) { return $x->name(); }
        echo \"hello \" . call_it(new Foo());
        ",
    );
    assert_eq!(out, "hello world");
}
