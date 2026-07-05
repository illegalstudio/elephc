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

/// Verifies `ClassName::class` resolves to the unqualified class name `C`.
#[test]
fn test_class_class_named() {
    let out = compile_and_run(
        "<?php class C { public int $x = 0; } echo C::class;",
    );
    assert_eq!(out, "C");
}

/// Verifies `ClassName::class` inside a namespace resolves to the fully-qualified name `App\C`.
#[test]
fn test_class_class_namespaced() {
    let out = compile_and_run(
        "<?php namespace App; class C { public int $x = 0; } echo C::class;",
    );
    assert_eq!(out, "App\\C");
}

/// Verifies `self::class` inside a method resolves to the lexical (defining) class `C`, not the runtime-called subclass.
#[test]
fn test_class_class_self_inside_method() {
    let out = compile_and_run(
        "<?php\nclass C {\n    public static function name() { return self::class; }\n}\necho C::name();\n",
    );
    assert_eq!(out, "C");
}

/// Verifies `parent::class` inside a child class resolves to the parent class `Base`.
#[test]
fn test_class_class_parent_inside_child() {
    let out = compile_and_run(
        "<?php\nclass Base { public int $x = 0; }\nclass Child extends Base {\n    public static function parent_name() { return parent::class; }\n}\necho Child::parent_name();\n",
    );
    assert_eq!(out, "Base");
}

/// Verifies `static::class` uses late static binding — resolves to the runtime class `Child` even when called from base method.
#[test]
fn test_class_class_static_uses_late_static_binding() {
    let out = compile_and_run(
        "<?php\nclass Base {\n    public static function name() { return static::class; }\n}\nclass Child extends Base {}\necho Child::name();\n",
    );
    assert_eq!(out, "Child");
}

/// Verifies `::class` can be used in string concatenation expressions.
#[test]
fn test_class_class_concat_in_message() {
    let out = compile_and_run(
        "<?php class Logger { public int $x = 0; } echo \"From: \" . Logger::class;",
    );
    assert_eq!(out, "From: Logger");
}

// --- new self() / new static() / new parent() ---

/// Verifies `new self()` inside a static method returns an instance of the lexical (defining) class `Box` and that fields are accessible.
#[test]
fn test_new_self_returns_instance_of_lexical_class() {
    let out = compile_and_run(
        "<?php\nclass Box {\n    public string $label = \"hello\";\n    public static function make(): Box { return new self(); }\n}\n$b = Box::make();\necho $b->label;\n",
    );
    assert_eq!(out, "hello");
}

/// Verifies `new static()` uses late static binding — returns an instance of the runtime-called class `Child` when called via `Child::make()`.
#[test]
fn test_new_static_returns_instance_of_called_class() {
    let out = compile_and_run(
        "<?php\nclass Base {\n    public static function make(): Base { return new static(); }\n    public function name(): string { return self::class; }\n}\nclass Child extends Base {\n    public function name(): string { return self::class; }\n}\n$b = Child::make();\necho $b->name();\n",
    );
    assert_eq!(out, "Child");
}

/// Verifies `new parent()` inside a child class returns an instance of the parent class `Base`.
#[test]
fn test_new_parent_returns_instance_of_parent_class() {
    let out = compile_and_run(
        "<?php\nclass Base {\n    public string $tag = \"base\";\n}\nclass Child extends Base {\n    public static function makeBase(): Base { return new parent(); }\n}\n$b = Child::makeBase();\necho $b->tag;\n",
    );
    assert_eq!(out, "base");
}

/// Verifies `new self`, `new static`, and `new parent` work without constructor parentheses.
#[test]
fn test_new_relative_receivers_without_constructor_parentheses() {
    let out = compile_and_run(
        "<?php\nclass Base {\n    public function who(): string { return \"base\"; }\n    public static function makeStatic() { return new static; }\n}\nclass Child extends Base {\n    public function who(): string { return \"child\"; }\n    public static function makeSelf() { return new self; }\n    public static function makeParent() { return new parent; }\n}\necho Child::makeSelf()->who(), \"|\", Child::makeStatic()->who(), \"|\", Child::makeParent()->who();\n",
    );
    assert_eq!(out, "child|child|base");
}

/// Verifies `new self()` passes constructor arguments correctly.
#[test]
fn test_new_self_with_constructor_args() {
    let out = compile_and_run(
        "<?php\nclass Greeter {\n    public string $name;\n    public function __construct(string $n) { $this->name = $n; }\n    public static function make(string $n): Greeter { return new self($n); }\n}\n$g = Greeter::make(\"Alice\");\necho $g->name;\n",
    );
    assert_eq!(out, "Alice");
}

// --- Static closures ---

/// Verifies static anonymous functions (closures) can be created and invoked with positional arguments.
#[test]
fn test_static_closure_runs() {
    let out = compile_and_run(
        "<?php $f = static function($a, $b) { return $a + $b; }; echo $f(3, 4);",
    );
    assert_eq!(out, "7");
}

/// Verifies static arrow functions (fn) can be created and invoked.
#[test]
fn test_static_arrow_function_runs() {
    let out = compile_and_run("<?php $g = static fn($x) => $x * 2; echo $g(5);");
    assert_eq!(out, "10");
}
