//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of static class features, including class class named, class class namespaced, and class class self inside method.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

// --- ::class magic constant ---

#[test]
fn test_class_class_named() {
    let out = compile_and_run(
        "<?php class C { public int $x = 0; } echo C::class;",
    );
    assert_eq!(out, "C");
}

#[test]
fn test_class_class_namespaced() {
    let out = compile_and_run(
        "<?php namespace App; class C { public int $x = 0; } echo C::class;",
    );
    assert_eq!(out, "App\\C");
}

#[test]
fn test_class_class_self_inside_method() {
    let out = compile_and_run(
        "<?php\nclass C {\n    public static function name() { return self::class; }\n}\necho C::name();\n",
    );
    assert_eq!(out, "C");
}

#[test]
fn test_class_class_parent_inside_child() {
    let out = compile_and_run(
        "<?php\nclass Base { public int $x = 0; }\nclass Child extends Base {\n    public static function parent_name() { return parent::class; }\n}\necho Child::parent_name();\n",
    );
    assert_eq!(out, "Base");
}

#[test]
fn test_class_class_static_uses_late_static_binding() {
    let out = compile_and_run(
        "<?php\nclass Base {\n    public static function name() { return static::class; }\n}\nclass Child extends Base {}\necho Child::name();\n",
    );
    assert_eq!(out, "Child");
}

#[test]
fn test_class_class_concat_in_message() {
    let out = compile_and_run(
        "<?php class Logger { public int $x = 0; } echo \"From: \" . Logger::class;",
    );
    assert_eq!(out, "From: Logger");
}

// --- new self() / new static() / new parent() ---

#[test]
fn test_new_self_returns_instance_of_lexical_class() {
    let out = compile_and_run(
        "<?php\nclass Box {\n    public string $label = \"hello\";\n    public static function make(): Box { return new self(); }\n}\n$b = Box::make();\necho $b->label;\n",
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_new_static_returns_instance_of_called_class() {
    let out = compile_and_run(
        "<?php\nclass Base {\n    public static function make(): Base { return new static(); }\n    public function name(): string { return self::class; }\n}\nclass Child extends Base {\n    public function name(): string { return self::class; }\n}\n$b = Child::make();\necho $b->name();\n",
    );
    assert_eq!(out, "Child");
}

#[test]
fn test_new_parent_returns_instance_of_parent_class() {
    let out = compile_and_run(
        "<?php\nclass Base {\n    public string $tag = \"base\";\n}\nclass Child extends Base {\n    public static function makeBase(): Base { return new parent(); }\n}\n$b = Child::makeBase();\necho $b->tag;\n",
    );
    assert_eq!(out, "base");
}

#[test]
fn test_new_self_with_constructor_args() {
    let out = compile_and_run(
        "<?php\nclass Greeter {\n    public string $name;\n    public function __construct(string $n) { $this->name = $n; }\n    public static function make(string $n): Greeter { return new self($n); }\n}\n$g = Greeter::make(\"Alice\");\necho $g->name;\n",
    );
    assert_eq!(out, "Alice");
}

// --- Static closures ---

#[test]
fn test_static_closure_runs() {
    let out = compile_and_run(
        "<?php $f = static function($a, $b) { return $a + $b; }; echo $f(3, 4);",
    );
    assert_eq!(out, "7");
}

#[test]
fn test_static_arrow_function_runs() {
    let out = compile_and_run("<?php $g = static fn($x) => $x * 2; echo $g(5);");
    assert_eq!(out, "10");
}
