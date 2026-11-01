//! Purpose:
//! End-to-end coverage for PHP's coercive parameter and return binding on declared user-defined
//! boundaries: scalars widening into `string`/`bool` parameters, `Stringable` objects selecting
//! string declarations, and compile-time-constant numeric arguments binding to `int`/`float`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value is verbatim `LC_ALL=C php` 8.4.20 stdout.
//! - `$argc` keeps an argument runtime-valued so the binding is exercised on a real value
//!   rather than being decided by AST constant folding.
//! - The bindings elephc deliberately refuses (lossy or non-numeric conversions, which PHP
//!   signals with a runtime `Deprecated:` notice or `TypeError`) are pinned in
//!   `tests/error_tests/type_system.rs`.

use crate::support::*;

/// Verifies a parameter union naming TWO object classes still admits an argument whose own class
/// relates to exactly one of them, and rejects at run time the way PHP does. A multi-class union
/// gives the runtime nominal guard no single class to test, so the boundary was refused outright —
/// but the argument narrows it: PHP's single inheritance means a class unrelated to the argument's
/// own can never be what arrives. Symfony's
/// `ViewEvent::__construct(ControllerArgumentsMetadata|ControllerArgumentsEvent|null)` is called
/// with a `?ControllerMetadata`, and only the first descends from it.
/// PHP outputs "t|none|TypeError".
#[test]
fn test_two_class_union_parameter_narrows_to_one_runtime_guard() {
    let out = compile_and_run(
        r#"<?php
class Meta { public function __construct(public string $tag) {} }
class ArgsMeta extends Meta {
    public function __construct(string $tag, public int $count) { parent::__construct($tag); }
}
class ArgsEvent { public function __construct(public string $name) {} }

class ViewEvent {
    public function __construct(public ArgsMeta|ArgsEvent|null $controllerMetadata = null) {}
}

function build(?Meta $meta): string {
    $event = new ViewEvent($meta);
    $stored = $event->controllerMetadata;

    return $stored instanceof Meta ? $stored->tag : 'none';
}

echo build(new ArgsMeta('t', 2)), '|', build(null), '|';
try {
    echo build(new Meta('plain'));
} catch (\TypeError $e) {
    echo 'TypeError';
}
"#,
    );
    assert_eq!(out, "t|none|TypeError");
}

/// Verifies a gradual union argument reaching a declared `?string` parameter through an
/// INTERFACE-dispatched call. PHP checks such a value at the call, not at compile time, and the
/// coercive path already deferred it; the interface path consulted only two of the four runtime
/// guards and refused it. Symfony's `Kernel::initializeContainer()` passes
/// `$this->container->getParameter('kernel.build_dir')` — declared
/// `array|bool|string|int|float|UnitEnum|null` — into `WarmableInterface::warmUp()`'s
/// `?string $buildDir`. PHP outputs "/cache//build".
#[test]
fn test_gradual_union_argument_binds_a_nullable_string_through_an_interface() {
    let out = compile_and_run(
        r#"<?php
interface Warmable {
    public function warmUp(string $cacheDir, ?string $buildDir = null): string;
}

class Params {
    public function __construct(private array $values) {}

    public function getParameter(string $name): array|bool|string|int|float|null {
        return $this->values[$name] ?? null;
    }
}

class Kernel implements Warmable {
    public function warmUp(string $cacheDir, ?string $buildDir = null): string {
        return $cacheDir . '/' . ($buildDir ?? 'none');
    }
}

function boot(Warmable $warmer, Params $container): string {
    $buildDir = $container->getParameter('kernel.build_dir');
    $cacheDir = $container->getParameter('kernel.cache_dir');

    return $warmer->warmUp($cacheDir, $buildDir);
}

$container = new Params(['kernel.cache_dir' => '/cache', 'kernel.build_dir' => '/build']);
echo boot(new Kernel(), $container);
"#,
    );
    assert_eq!(out, "/cache//build");
}

/// Verifies the parameter-typing audit repro: a float and a numeric string binding to `int`,
/// and an int binding to `string`.
#[test]
fn test_coercive_binding_audit_repro() {
    let out = compile_and_run(
        r#"<?php
        function takesInt(int $i) { return $i; }
        function takesString(string $s) { return $s; }
        echo takesInt(5.0), " ", takesInt("42"), " ", takesString(42);
        "#,
    );
    assert_eq!(out, "5 42 42");
}

/// Verifies every scalar binds to a `string` parameter exactly as PHP's `(string)` cast does,
/// including `false` becoming the empty string and `1.0` losing its fractional part.
#[test]
fn test_scalars_bind_to_string_parameter() {
    let out = compile_and_run(
        r#"<?php
        function fmt(string $s) { return "[" . $s . "]"; }
        echo fmt(42), fmt(4.5), fmt(true), fmt(false), fmt(1.0);
        "#,
    );
    assert_eq!(out, "[42][4.5][1][][1]");
}

/// Verifies every scalar binds to a `bool` parameter using PHP's truthiness, including the
/// `"0"` and `0.0` falsy cases.
#[test]
fn test_scalars_bind_to_bool_parameter() {
    let out = compile_and_run(
        r#"<?php
        function flag(bool $b) { return $b ? "T" : "F"; }
        echo flag(1), flag(0), flag("a"), flag(""), flag("0"), flag(0.0), flag(-0.5);
        "#,
    );
    assert_eq!(out, "TFTFFFT");
}

/// Verifies the `string` binding also fires for runtime values, not only literals. `$argc`
/// keeps each argument opaque to constant folding.
#[test]
fn test_runtime_scalars_bind_to_string_parameter() {
    let out = compile_and_run(
        r#"<?php
        function fmt(string $s) { return "[" . $s . "]"; }
        $n = 40 + $argc;
        $f = 4.5 + $argc;
        $b = $argc > 0;
        echo fmt($n), fmt($f), fmt($b);
        "#,
    );
    assert_eq!(out, "[41][5.5][1]");
}

/// Verifies numeric-string constants bind to `int` and `float` parameters, covering PHP's
/// surrounding-whitespace allowance, exponent spelling, and a negative value.
#[test]
fn test_numeric_string_constants_bind_to_numeric_parameters() {
    let out = compile_and_run(
        r#"<?php
        function takesInt(int $i) { return $i; }
        function takesFloat(float $f) { return $f; }
        echo takesInt(" 42 "), " ", takesFloat("4.5"), " ", takesFloat("1e3"), " ", takesInt("-7");
        "#,
    );
    assert_eq!(out, "42 4.5 1000 -7");
}

/// Verifies the binding covers declared parameters on constructors, instance methods, and
/// static methods, not just plain functions.
#[test]
fn test_coercive_binding_applies_to_methods_and_constructors() {
    let out = compile_and_run(
        r#"<?php
        class Box {
            public function __construct(public string $label) {}
            public function tag(string $s): string { return $this->label . ":" . $s; }
            public static function of(int $n): string { return "n=" . $n; }
        }
        echo (new Box(1.5))->tag(42), " ", Box::of(7.0);
        "#,
    );
    assert_eq!(out, "1.5:42 n=7");
}

/// Verifies the binding fires for named arguments, which reach EIR through the reordered
/// named-argument path rather than the positional one.
#[test]
fn test_coercive_binding_applies_to_named_arguments() {
    let out = compile_and_run(
        r#"<?php
        function pair(string $a, string $b) { return $a . "|" . $b; }
        echo pair(b: 7, a: 4.5);
        "#,
    );
    assert_eq!(out, "4.5|7");
}

/// Verifies a weak call converts a `Stringable` object into both a direct `string` parameter
/// and the `string` member of a union, including reordered and fresh owning arguments.
#[test]
fn test_stringable_objects_bind_to_string_parameter_members() {
    let out = compile_and_run(
        r#"<?php
        class Label {
            public function __construct(public string $text) {}
            public function __toString(): string { return $this->text; }
        }
        function direct(string $value): string { return $value; }
        function either(string|iterable $value): string {
            return is_string($value) ? $value : "iterable";
        }
        function many(string ...$values): string { return implode(",", $values); }
        $label = new Label("local");
        echo direct($label), "|", either($label), "|", either(value: new Label("fresh")),
            "|", many($label, new Label("tail"));
        "#,
    );
    assert_eq!(out, "local|local|fresh|local,tail");
}

/// Verifies coercive methods convert a returned `Stringable` object at a declared `string`
/// boundary, including the owning temporary returned by the method call chain.
#[test]
fn test_stringable_object_binds_to_coercive_string_return() {
    let out = compile_and_run(
        r#"<?php
class ReturnLabel {
    public function __construct(public string $text) {}
    public function __toString(): string { return $this->text; }
}
class ReturnFormatter {
    public static function format(string $text): string {
        return new ReturnLabel($text);
    }
}
echo ReturnFormatter::format("ready");
"#,
    );
    assert_eq!(out, "ready");
}

/// Verifies union identity wins over weak coercion: an object satisfying `iterable` remains an
/// iterable for `string|iterable`, even when that same object is also `Stringable`.
#[test]
fn test_stringable_iterable_object_keeps_iterable_union_member() {
    let out = compile_and_run(
        r#"<?php
        class Labels implements Stringable, IteratorAggregate {
            public function __toString(): string { return "string"; }
            public function getIterator(): Traversable { yield "item"; }
        }
        function either(string|iterable $value): string {
            return is_string($value) ? $value : "iterable";
        }
        echo either(new Labels());
        "#,
    );
    assert_eq!(out, "iterable");
}
