//! Purpose:
//! End-to-end codegen tests for PHP attribute syntax and the compile-time or
//! runtime behavior of supported built-in attributes.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Most user-defined attributes should not change output.
//! - Built-in attributes must respect PHP class-name resolution.

use super::*;

/// Verifies that arbitrary user-defined attributes on classes, methods,
/// and properties do not change the compiled output or observable runtime
/// behavior. The class under test has multiple attributes and a method
/// that increments a field; the expected output is identical to the
/// attribute-free version.
#[test]
fn test_attributes_do_not_alter_runtime_behavior() {
    let out = compile_and_run(
        r#"<?php
#[Foo]
#[Bar(1, "two")]
class Counter {
    #[Slot]
    public int $n = 0;

    #[Mutator]
    public function inc(): void {
        $this->n = $this->n + 1;
    }
}

$c = new Counter();
$c->inc();
$c->inc();
$c->inc();
echo $c->n;
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies that the `#[\Memoized]` attribute on a named function does not
/// prevent compilation and does not change the function's output.
#[test]
fn test_attribute_on_function_decl_compiles() {
    let out = compile_and_run(
        r#"<?php
#[Memoized]
function double(int $x): int {
    return $x * 2;
}
echo double(7);
"#,
    );
    assert_eq!(out, "14");
}

/// Verifies that fully-qualified attribute names (e.g. `#[\App\Annotations\Mark]`,
/// `#[\Symfony\Contracts\Service\Attribute\Required]`) are accepted by the parser
/// and pass through codegen unchanged. Symfony-style attributes must not affect
/// runtime behavior.
#[test]
fn test_qualified_attribute_name_compiles() {
    let out = compile_and_run(
        r#"<?php
#[\App\Annotations\Mark]
class Tagged {
    #[\Symfony\Contracts\Service\Attribute\Required]
    public function setUp(): void {
    }
}

$t = new Tagged();
$t->setUp();
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that PHP-style `# comment` lines are treated as ordinary line
/// comments and do not produce syntax errors or alter output. Both a
/// mid-line comment and a trailing comment without a trailing newline are
/// tested.
#[test]
fn test_php_hash_line_comment_is_ignored() {
    let out = compile_and_run(
        r#"<?php
# this is a PHP-style line comment
echo 1;
# trailing comment with no newline at end"#,
    );
    assert_eq!(out, "1");
}

/// Verifies that attributes on function parameters (`#[Sensitive]`) compile
/// identically to the bare parameter version. The function body must still
/// execute correctly with the parameter value.
#[test]
fn test_parameter_attribute_compiles() {
    let out = compile_and_run(
        r#"<?php
function hash_password(#[Sensitive] string $pw): string {
    return $pw . "_hashed";
}
echo hash_password("secret");
"#,
    );
    assert_eq!(out, "secret_hashed");
}

/// Verifies that promoted constructor parameters (`#[Inject] public string $prefix`)
/// compile correctly and the promoted property is accessible on the constructed object.
#[test]
fn test_promoted_property_attribute_compiles() {
    let out = compile_and_run(
        r#"<?php
class Logger {
    public function __construct(#[Inject] public string $prefix) {}
}
$l = new Logger("[L] ");
echo $l->prefix;
"#,
    );
    assert_eq!(out, "[L] ");
}

/// Verifies that an attribute (`#[Pure]`) on an anonymous function expression
/// compiles without error and the closure is callable.
#[test]
fn test_closure_attribute_compiles() {
    let out = compile_and_run(
        r#"<?php
$double = #[Pure] function (int $x): int { return $x * 2; };
echo $double(21);
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that an attribute (`#[Pure]`) on an arrow function (`fn`)
/// compiles without error and the arrow function is callable.
#[test]
fn test_arrow_function_attribute_compiles() {
    let out = compile_and_run(
        r#"<?php
$inc = #[Pure] fn (int $x) => $x + 1;
echo $inc(41);
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that an attribute (`#[Pure]`) on a `static function` anonymous
/// closure compiles without error and the closure is callable.
#[test]
fn test_static_closure_attribute_compiles() {
    let out = compile_and_run(
        r#"<?php
$triple = #[Pure] static function (int $x): int { return $x * 3; };
echo $triple(14);
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that an attribute on a parameter inside a closure (`#[Sensitive]`)
/// compiles without error and the closure is callable.
#[test]
fn test_closure_parameter_attribute_compiles() {
    let out = compile_and_run(
        r#"<?php
$mask = function (#[Sensitive] string $pw): string { return "***"; };
echo $mask("secret");
"#,
    );
    assert_eq!(out, "***");
}

/// Verifies that `#[\Override]` on a method that genuinely overrides a parent
/// method compiles without error and the method behaves identically to the
/// attribute-free version.
#[test]
fn test_override_attribute_on_valid_override_compiles() {
    let out = compile_and_run(
        r#"<?php
class Animal {
    public function name(): string { return "Animal"; }
}
class Dog extends Animal {
    #[\Override]
    public function name(): string { return "Dog"; }
}
$d = new Dog();
echo $d->name();
"#,
    );
    assert_eq!(out, "Dog");
}

/// Verifies that `#[\Override]` on a method that implements an interface
/// (not a direct parent class override) is accepted by the compiler.
#[test]
fn test_override_attribute_through_interface_compiles() {
    let out = compile_and_run(
        r#"<?php
interface Greeter {
    public function hello(): string;
}
class Hi implements Greeter {
    #[\Override]
    public function hello(): string { return "hi"; }
}
$g = new Hi();
echo $g->hello();
"#,
    );
    assert_eq!(out, "hi");
}

// --- #[\AllowDynamicProperties] runtime support (PHP 8.2) ---

/// Verifies that a class with `#[\AllowDynamicProperties]` permits dynamic
/// integer property assignment and reading without error.
#[test]
fn test_allow_dynamic_properties_basic_int() {
    let out = compile_and_run(
        r#"<?php
#[\AllowDynamicProperties]
class Bag {}
$b = new Bag();
$b->n = 42;
echo $b->n;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that `#[\AllowDynamicProperties]` permits dynamic string property
/// assignment and reading.
#[test]
fn test_allow_dynamic_properties_string_value() {
    let out = compile_and_run(
        r#"<?php
#[\AllowDynamicProperties]
class Bag {}
$b = new Bag();
$b->msg = "hello";
echo $b->msg;
"#,
    );
    assert_eq!(out, "hello");
}

/// Verifies that `#[\AllowDynamicProperties]` permits repeated assignment to
/// the same dynamic key, keeping only the final value.
#[test]
fn test_allow_dynamic_properties_overwrite() {
    let out = compile_and_run(
        r#"<?php
#[\AllowDynamicProperties]
class Bag {}
$b = new Bag();
$b->v = 1;
$b->v = 2;
$b->v = 3;
echo $b->v;
"#,
    );
    assert_eq!(out, "3");
}

/// Verifies that `#[\AllowDynamicProperties]` works when a class also has
/// declared (typed) properties — both declared and dynamic properties are
/// accessible independently.
#[test]
fn test_allow_dynamic_properties_mixed_with_declared() {
    let out = compile_and_run(
        r#"<?php
#[\AllowDynamicProperties]
class Bag {
    public int $declared = 7;
}
$b = new Bag();
$b->extra = 13;
echo $b->declared;
echo "|";
echo $b->extra;
"#,
    );
    assert_eq!(out, "7|13");
}

/// Verifies that `#[\AllowDynamicProperties]` is accepted without the
/// leading backslash (unqualified form) on the class.
#[test]
fn test_allow_dynamic_properties_unqualified_form() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class Bag {}
$b = new Bag();
$b->x = 99;
echo $b->x;
"#,
    );
    assert_eq!(out, "99");
}

/// Verifies that `#[\AllowDynamicProperties]` can be applied via a use-group
/// import alias (`use AllowDynamicProperties as DynamicBag; #[DynamicBag]`).
#[test]
fn test_allow_dynamic_properties_import_alias() {
    let out = compile_and_run(
        r#"<?php
use AllowDynamicProperties as DynamicBag;
#[DynamicBag]
class Bag {}
$b = new Bag();
$b->x = 55;
echo $b->x;
"#,
    );
    assert_eq!(out, "55");
}

/// Verifies that `#[\AllowDynamicProperties]` is inherited by child classes
/// — a dynamic property set on a child instance of a parent marked with
/// `#[\AllowDynamicProperties]` must work.
#[test]
fn test_allow_dynamic_properties_is_inherited() {
    let out = compile_and_run(
        r#"<?php
#[\AllowDynamicProperties]
class Base {}
class Child extends Base {}
$c = new Child();
$c->x = 7;
echo $c->x;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies that `#[\AllowDynamicProperties]` permits multiple distinct dynamic
/// properties on the same object, preserving each key's value independently.
#[test]
fn test_allow_dynamic_properties_multiple_keys() {
    let out = compile_and_run(
        r#"<?php
#[\AllowDynamicProperties]
class Cfg {}
$c = new Cfg();
$c->host = "localhost";
$c->port = 8080;
$c->ssl = 1;
echo $c->host;
echo ":";
echo $c->port;
echo "/";
echo $c->ssl;
"#,
    );
    assert_eq!(out, "localhost:8080/1");
}

// --- class_attribute_names() reflection-style builtin ---

/// Verifies that `class_attribute_names()` returns the short names (without
/// namespace) of all attributes decorating a class, in source order.
#[test]
fn test_class_attribute_names_returns_decorated_attributes() {
    let out = compile_and_run(
        r#"<?php
#[Author("Ada"), Version(1)]
class Greeter {}
$names = class_attribute_names('Greeter');
foreach ($names as $n) {
    echo $n;
    echo "\n";
}
"#,
    );
    assert_eq!(out, "Author\nVersion\n");
}

/// Verifies that `class_attribute_names()` returns the canonical short name
/// (no leading backslash) for a fully-qualified attribute, matching PHP's
/// `ReflectionAttribute::getName()` normalization.
#[test]
fn test_class_attribute_names_normalises_fully_qualified_form() {
    let out = compile_and_run(
        r#"<?php
#[\Override]
class A {}
$names = class_attribute_names('A');
echo $names[0];
"#,
    );
    assert_eq!(out, "Override");
}

/// Verifies that `class_attribute_names()` returns an empty array for a
/// class with no attributes.
#[test]
fn test_class_attribute_names_returns_empty_array_for_undecorated_class() {
    let out = compile_and_run(
        r#"<?php
class Bare {}
$names = class_attribute_names('Bare');
echo "count=";
echo count($names);
"#,
    );
    assert_eq!(out, "count=0");
}

/// Verifies that `class_attribute_names()` returns attributes in the exact
/// source order, not sorted alphabetically.
#[test]
fn test_class_attribute_names_preserves_source_order() {
    let out = compile_and_run(
        r#"<?php
#[Z]
#[A]
#[M]
class Mixed {}
$names = class_attribute_names('Mixed');
echo implode("|", $names);
"#,
    );
    assert_eq!(out, "Z|A|M");
}

/// Verifies that `class_attribute_names()` is per-class — a class with one
/// attribute and a class with two attributes each return the correct counts
/// and names independently.
#[test]
fn test_class_attribute_names_per_class_isolation() {
    let out = compile_and_run(
        r#"<?php
#[Foo]
class X {}
#[Bar]
#[Baz]
class Y {}
$xs = class_attribute_names('X');
$ys = class_attribute_names('Y');
echo count($xs);
echo "/";
echo count($ys);
echo "/";
echo $xs[0];
echo ",";
echo $ys[0];
echo ",";
echo $ys[1];
"#,
    );
    assert_eq!(out, "1/2/Foo,Bar,Baz");
}

/// Verifies that `class_attribute_names()` performs case-insensitive class
/// lookup (lowercase `'greeter'` resolves to `Greeter`).
#[test]
fn test_class_attribute_names_class_lookup_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
#[Foo]
class Greeter {}
$names = class_attribute_names('greeter');
echo $names[0];
"#,
    );
    assert_eq!(out, "Foo");
}

/// Verifies that `class_attribute_names()` accepts a fully-qualified class
/// name with a leading backslash (`'\App\Greeter'`).
#[test]
fn test_class_attribute_names_accepts_leading_global_class_string() {
    let out = compile_and_run(
        r#"<?php
namespace App;
#[Foo]
class Greeter {}
$names = class_attribute_names('\App\Greeter');
echo $names[0];
"#,
    );
    assert_eq!(out, "App\\Foo");
}

/// Verifies that `class_attribute_names()` does not require attribute
/// arguments to be reflectable — a constant expression argument (1 + 2)
/// is accepted and the attribute name is still returned.
#[test]
fn test_class_attribute_names_does_not_require_reflectable_args() {
    let out = compile_and_run(
        r#"<?php
#[Foo(1 + 2)]
class C {}
$names = class_attribute_names('C');
echo $names[0];
"#,
    );
    assert_eq!(out, "Foo");
}

/// Verifies that an attribute class declaration using `Attribute::TARGET_CLASS`
/// compiles without error and the resulting class is instantiable.
#[test]
fn test_attribute_class_declaration_with_constant_arg_compiles_without_reflection_query() {
    let out = compile_and_run(
        r#"<?php
#[Attribute(Attribute::TARGET_CLASS)]
class MyAttr {}
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that `class_attribute_names()` supports named argument syntax
/// (`class_name: 'Greeter'`) in addition to positional arguments.
#[test]
fn test_class_attribute_names_supports_named_argument_planning() {
    let out = compile_and_run(
        r#"<?php
#[Foo]
class Greeter {}
$names = class_attribute_names(class_name: 'Greeter');
echo $names[0];
"#,
    );
    assert_eq!(out, "Foo");
}

// --- class_attribute_args() reflection-style builtin ---

/// Verifies that `class_attribute_args()` returns all positional arguments
/// of a named attribute on a class, in source order, as an array of strings.
#[test]
fn test_class_attribute_args_returns_string_args_in_order() {
    let out = compile_and_run(
        r#"<?php
#[Route("/api/users", "GET")]
class UserController {}
$args = class_attribute_args('UserController', 'Route');
echo count($args);
echo "/";
echo $args[0];
echo ",";
echo $args[1];
"#,
    );
    assert_eq!(out, "2//api/users,GET");
}

/// Verifies that `class_attribute_args()` returns an empty array when the
/// attribute has no arguments.
#[test]
fn test_class_attribute_args_returns_empty_when_no_args() {
    let out = compile_and_run(
        r#"<?php
#[Marker]
class X {}
$args = class_attribute_args('X', 'Marker');
echo count($args);
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies that `class_attribute_args()` returns an empty array when the
/// named attribute is not present on the class (the class has `Foo` and
/// `Bar` but we query for `Missing`).
#[test]
fn test_class_attribute_args_returns_empty_when_attr_missing() {
    let out = compile_and_run(
        r#"<?php
#[Foo("a"), Bar("b")]
class X {}
$args = class_attribute_args('X', 'Missing');
echo count($args);
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies that `class_attribute_args()` preserves integer and string
/// literal arguments as strings in the returned array, matching PHP's
/// standard echo conversion for those types.
#[test]
fn test_class_attribute_args_preserves_int_and_string_literals() {
    let out = compile_and_run(
        r#"<?php
#[Mixed("kept", 42, "also-kept")]
class X {}
$args = class_attribute_args('X', 'Mixed');
echo count($args);
echo "/";
echo $args[0];
echo ",";
echo $args[1];
echo ",";
echo $args[2];
"#,
    );
    assert_eq!(out, "3/kept,42,also-kept");
}

/// Verifies that boolean (`true`, `false`) and `null` literal arguments are
/// preserved by `class_attribute_args()` as strings. Booleans render as
/// PHP echo would (true → "1", false → ""), null renders as empty string,
/// pinning down the runtime shape preservation.
#[test]
fn test_class_attribute_args_preserves_bool_and_null_literals() {
    let out = compile_and_run(
        r#"<?php
#[Status(true, false, null)]
class X {}
$args = class_attribute_args('X', 'Status');
echo count($args);
echo "|";
echo "[" . $args[0] . "]";
echo "[" . $args[1] . "]";
echo "[" . $args[2] . "]";
"#,
    );
    assert_eq!(out, "3|[1][][]");
}

/// Verifies that `class_attribute_args()` preserves negated integer literals
/// (`-1`, `-42`) as strings in the returned array.
#[test]
fn test_class_attribute_args_preserves_negated_int_literals() {
    let out = compile_and_run(
        r#"<?php
#[Code(-1, -42)]
class X {}
$args = class_attribute_args('X', 'Code');
echo $args[0];
echo "/";
echo $args[1];
"#,
    );
    assert_eq!(out, "-1/-42");
}

/// Verifies that `class_attribute_args()` performs case-insensitive class
/// lookup (lowercase `'controller'` resolves to `Controller`).
#[test]
fn test_class_attribute_args_class_lookup_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
#[Route("/x")]
class Controller {}
$args = class_attribute_args('controller', 'Route');
echo $args[0];
"#,
    );
    assert_eq!(out, "/x");
}

/// Verifies that `class_attribute_args()` accepts a fully-qualified class
/// name with a leading backslash (`'\Controller'`).
#[test]
fn test_class_attribute_args_accepts_leading_global_class_string() {
    let out = compile_and_run(
        r#"<?php
#[Route("/x")]
class Controller {}
$args = class_attribute_args('\Controller', 'Route');
echo $args[0];
"#,
    );
    assert_eq!(out, "/x");
}

/// Verifies that `class_attribute_args()` supports named argument syntax
/// with reversed parameter order (`attribute_name:`, `class_name:`) in
/// addition to the default positional order.
#[test]
fn test_class_attribute_args_supports_named_argument_reordering() {
    let out = compile_and_run(
        r#"<?php
#[Route("/x")]
class Controller {}
$args = class_attribute_args(attribute_name: 'Route', class_name: 'Controller');
echo $args[0];
"#,
    );
    assert_eq!(out, "/x");
}

// --- ReflectionAttribute synthetic class + class_get_attributes() ---

/// Verifies that `class_get_attributes()` returns an array of
/// `ReflectionAttribute` objects with correct `getName()` and
/// `getArguments()` for a class decorated with two attributes.
#[test]
fn test_class_get_attributes_returns_reflection_attribute_array() {
    let out = compile_and_run(
        r#"<?php
#[Author("Ada", 1815), Version("1.0", true)]
class Greeter {}
$attrs = class_get_attributes('Greeter');
echo "count=" . count($attrs) . "\n";
foreach ($attrs as $attr) {
    echo $attr->getName() . ":";
    foreach ($attr->getArguments() as $arg) {
        echo "[" . $arg . "]";
    }
    echo "\n";
}
"#,
    );
    assert_eq!(out, "count=2\nAuthor:[Ada][1815]\nVersion:[1.0][1]\n");
}

/// Verifies that `class_get_attributes()` returns an empty array for a
/// class with no attributes.
#[test]
fn test_class_get_attributes_returns_empty_for_undecorated_class() {
    let out = compile_and_run(
        r#"<?php
class Bare {}
$attrs = class_get_attributes('Bare');
echo count($attrs);
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies that `class_get_attributes()` returns the resolved short name
/// (no leading backslash) from a fully-qualified attribute like `#[\Override]`,
/// matching PHP's `ReflectionAttribute::getName()` normalization.
#[test]
fn test_class_get_attributes_normalises_fully_qualified_name() {
    let out = compile_and_run(
        r#"<?php
#[\Override]
class A {}
$attrs = class_get_attributes('A');
echo $attrs[0]->getName();
"#,
    );
    assert_eq!(out, "Override");
}

/// Verifies that `class_get_attributes()` correctly handles an attribute
/// with no arguments — `getName()` returns the name and `getArguments()`
/// returns an empty array.
#[test]
fn test_class_get_attributes_handles_attribute_without_args() {
    let out = compile_and_run(
        r#"<?php
#[Marker]
class C {}
$attrs = class_get_attributes('C');
$attr = $attrs[0];
echo $attr->getName();
echo "/";
echo count($attr->getArguments());
"#,
    );
    assert_eq!(out, "Marker/0");
}

/// Verifies that `class_get_attributes()` performs case-insensitive class
/// lookup (lowercase `'greeter'` resolves to `Greeter`).
#[test]
fn test_class_get_attributes_class_lookup_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
#[Foo("bar")]
class Greeter {}
$attrs = class_get_attributes('greeter');
echo $attrs[0]->getName();
echo "/";
echo $attrs[0]->getArguments()[0];
"#,
    );
    assert_eq!(out, "Foo/bar");
}

/// Verifies that `class_get_attributes()` accepts a fully-qualified class
/// name with a leading backslash (`'\Greeter'`).
#[test]
fn test_class_get_attributes_accepts_leading_global_class_string() {
    let out = compile_and_run(
        r#"<?php
#[Foo("bar")]
class Greeter {}
$attrs = class_get_attributes('\Greeter');
echo $attrs[0]->getName();
echo "/";
echo $attrs[0]->getArguments()[0];
"#,
    );
    assert_eq!(out, "Foo/bar");
}

/// Verifies that `class_get_attributes()` supports static associative
/// array spread syntax (`...["class_name" => "Greeter"]`) for named argument
/// passing.
#[test]
fn test_class_get_attributes_supports_static_assoc_spread() {
    let out = compile_and_run(
        r#"<?php
#[Foo("bar")]
class Greeter {}
$attrs = class_get_attributes(...["class_name" => "Greeter"]);
echo $attrs[0]->getName();
"#,
    );
    assert_eq!(out, "Foo");
}

/// Verifies that `CLASS_ATTRIBUTE_NAMES`, `Class_Attribute_Args`, and
/// `Class_Get_Attributes` are case-insensitive and correctly resolve
/// namespaced attributes on a namespaced class.
#[test]
fn test_attribute_reflection_builtins_are_case_insensitive_and_namespaced() {
    let out = compile_and_run(
        r#"<?php
namespace App;
#[Route("/home")]
class Controller {}
$names = CLASS_ATTRIBUTE_NAMES('App\Controller');
echo $names[0];
echo "/";
$args = Class_Attribute_Args('App\Controller', 'App\Route');
echo $args[0];
echo "/";
$attrs = Class_Get_Attributes('App\Controller');
echo $attrs[0]->getName();
"#,
    );
    assert_eq!(out, "App\\Route//home/App\\Route");
}

/// Verifies that `class_attribute_args()` performs case-insensitive
/// attribute name lookup (lowercase `'route'` matches `Route`).
#[test]
fn test_class_attribute_args_matches_attribute_name_case_insensitively() {
    let out = compile_and_run(
        r#"<?php
#[Route("/x")]
class C {}
$args = class_attribute_args('C', 'route');
echo $args[0];
"#,
    );
    assert_eq!(out, "/x");
}

/// Verifies that when the same attribute appears multiple times on a class,
/// `class_attribute_args()` returns the arguments of the first occurrence.
#[test]
fn test_class_attribute_args_picks_first_matching_attribute() {
    let out = compile_and_run(
        r#"<?php
#[Tag("first"), Tag("second")]
class X {}
$args = class_attribute_args('X', 'Tag');
echo count($args);
echo "/";
echo $args[0];
"#,
    );
    assert_eq!(out, "1/first");
}

/// Verifies that when the same attribute appears multiple times and a later
/// occurrence has unsupported argument expressions, `class_attribute_args()`
/// returns the args of the first matching occurrence and ignores the later one.
#[test]
fn test_class_attribute_args_ignores_later_unsupported_duplicate_match() {
    let out = compile_and_run(
        r#"<?php
#[Tag("first"), Tag(1 + 2)]
class X {}
$args = class_attribute_args('X', 'Tag');
echo count($args);
echo "/";
echo $args[0];
"#,
    );
    assert_eq!(out, "1/first");
}

/// Verifies that `ReflectionClass::getAttributes()` returns an array of
/// `ReflectionAttribute` objects with correct `getName()` and
/// `getArguments()` for a class decorated with two attributes.
#[test]
fn test_reflection_class_get_attributes_returns_reflection_attribute_array() {
    let out = compile_and_run(
        r#"<?php
#[Author("Ada", 1815), Version("1.0", true)]
class Greeter {}
$ref = new ReflectionClass('Greeter');
$attrs = $ref->getAttributes();
echo count($attrs) . "\n";
foreach ($attrs as $attr) {
    echo $attr->getName() . ":";
    foreach ($attr->getArguments() as $arg) {
        echo "[" . $arg . "]";
    }
    echo "\n";
}
"#,
    );
    assert_eq!(out, "2\nAuthor:[Ada][1815]\nVersion:[1.0][1]\n");
}

/// Verifies that `ReflectionClass::getName()` returns the declared class name
/// for a regular class reflector.
#[test]
fn test_reflection_class_get_name_returns_class_name() {
    let out = compile_and_run(
        r#"<?php
class Plain {}
$ref = new ReflectionClass('Plain');
echo $ref->getName() . ":";
echo $ref->getShortName() . ":";
echo $ref->getNamespaceName() . ":";
echo $ref->inNamespace() ? "Y" : "N";
"#,
    );
    assert_eq!(out, "Plain:Plain::N");
}

/// Verifies that `ReflectionClass` reports final and abstract flags for static metadata.
#[test]
fn test_reflection_class_reports_modifier_flags() {
    let out = compile_and_run(
        r#"<?php
abstract class StaticAbstractReflect {}
final class StaticFinalReflect {}
interface StaticIfaceReflect {}
trait StaticTraitReflect {}
enum StaticEnumReflect { case Ready; }
echo (new ReflectionClass(StaticAbstractReflect::class))->isAbstract() ? "A" : "a";
echo (new ReflectionClass(StaticAbstractReflect::class))->isFinal() ? "F" : "f";
echo (new ReflectionClass(StaticAbstractReflect::class))->isInterface() ? "I" : "i";
echo (new ReflectionClass(StaticAbstractReflect::class))->isTrait() ? "T" : "t";
echo (new ReflectionClass(StaticAbstractReflect::class))->isEnum() ? "E" : "e";
echo ":";
echo (new ReflectionClass(StaticFinalReflect::class))->isAbstract() ? "A" : "a";
echo (new ReflectionClass(StaticFinalReflect::class))->isFinal() ? "F" : "f";
echo (new ReflectionClass(StaticFinalReflect::class))->isInterface() ? "I" : "i";
echo (new ReflectionClass(StaticFinalReflect::class))->isTrait() ? "T" : "t";
echo (new ReflectionClass(StaticFinalReflect::class))->isEnum() ? "E" : "e";
echo ":";
echo (new ReflectionClass(StaticEnumReflect::class))->isAbstract() ? "A" : "a";
echo (new ReflectionClass(StaticEnumReflect::class))->isFinal() ? "F" : "f";
echo (new ReflectionClass(StaticEnumReflect::class))->isInterface() ? "I" : "i";
echo (new ReflectionClass(StaticEnumReflect::class))->isTrait() ? "T" : "t";
echo (new ReflectionClass(StaticEnumReflect::class))->isEnum() ? "E" : "e";
echo ":";
$iface = new ReflectionClass("staticifacereflect");
echo $iface->getName() . "/";
echo $iface->isAbstract() ? "A" : "a";
echo $iface->isFinal() ? "F" : "f";
echo $iface->isInterface() ? "I" : "i";
echo $iface->isTrait() ? "T" : "t";
echo $iface->isEnum() ? "E" : "e";
echo ":";
$trait = new ReflectionClass("STATICTRAITREFLECT");
echo $trait->getName() . "/";
echo $trait->isAbstract() ? "A" : "a";
echo $trait->isFinal() ? "F" : "f";
echo $trait->isInterface() ? "I" : "i";
echo $trait->isTrait() ? "T" : "t";
echo $trait->isEnum() ? "E" : "e";
"#,
    );
    assert_eq!(
        out,
        "Afite:aFite:aFitE:StaticIfaceReflect/afIte:StaticTraitReflect/afiTe"
    );
}

/// Verifies that `ReflectionClass::getModifiers()` reports PHP modifier bitmasks.
#[test]
fn test_reflection_class_get_modifiers_reports_php_bitmask() {
    let out = compile_and_run(
        r#"<?php
abstract class StaticModifierAbstract {}
final class StaticModifierFinal {}
readonly class StaticModifierReadonly {}
final readonly class StaticModifierFinalReadonly {}
enum StaticModifierEnum { case Ready; }
interface StaticModifierIface {}
trait StaticModifierTrait {}
echo (new ReflectionClass(StaticModifierAbstract::class))->getModifiers() . ":";
echo (new ReflectionClass(StaticModifierFinal::class))->getModifiers() . ":";
echo (new ReflectionClass(StaticModifierReadonly::class))->getModifiers() . ":";
echo (new ReflectionClass(StaticModifierFinalReadonly::class))->getModifiers() . ":";
echo (new ReflectionClass(StaticModifierEnum::class))->getModifiers() . ":";
echo (new ReflectionClass(StaticModifierIface::class))->getModifiers() . ":";
echo (new ReflectionClass(StaticModifierTrait::class))->getModifiers() . ":";
echo ReflectionClass::IS_IMPLICIT_ABSTRACT . ":";
echo ReflectionClass::IS_FINAL . ":";
echo ReflectionClass::IS_EXPLICIT_ABSTRACT . ":";
echo ReflectionClass::IS_READONLY;
"#,
    );
    assert_eq!(out, "64:32:65536:65568:32:0:0:16:32:64:65536");
}

/// Verifies that `ReflectionClass::isReadOnly()` reports readonly class metadata.
#[test]
fn test_reflection_class_is_readonly_reports_class_metadata() {
    let out = compile_and_run(
        r#"<?php
class StaticReadonlyPlain {}
readonly class StaticReadonlyReflect {}
final readonly class StaticReadonlyFinalReflect {}
enum StaticReadonlyEnumReflect { case Ready; }
interface StaticReadonlyIface {}
trait StaticReadonlyTrait {}
echo (new ReflectionClass(StaticReadonlyPlain::class))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass(StaticReadonlyReflect::class))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass(StaticReadonlyFinalReflect::class))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass(StaticReadonlyEnumReflect::class))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass(StaticReadonlyIface::class))->isReadOnly() ? "R" : "r";
echo (new ReflectionClass(StaticReadonlyTrait::class))->isReadOnly() ? "R" : "r";
"#,
    );
    assert_eq!(out, "rRRrrr");
}

/// Verifies that `ReflectionClass::hasMethod()` and `hasProperty()` report
/// PHP-visible members for static class-like metadata.
#[test]
fn test_reflection_class_reports_member_existence() {
    let out = compile_and_run(
        r#"<?php
class StaticMemberParent {
    private function hiddenParent() {}
    protected static function parentStatic() {}
    private $hiddenProp;
    protected static $parentStaticProp;
}
class StaticMemberChild extends StaticMemberParent {
    public function ChildMethod() {}
    public $childProp;
}
interface StaticMemberIfaceParent {
    public function parentRequirement();
}
interface StaticMemberIface extends StaticMemberIfaceParent {
    public function childRequirement();
    public string $hook { get; }
}
trait StaticMemberTrait {
    private function traitHidden() {}
    public $traitProp;
}
enum StaticMemberPureEnum {
    case Ready;
    public function label() { return "ok"; }
}
enum StaticMemberBackedEnum: string {
    case Ready = "ready";
}
$child = new ReflectionClass(StaticMemberChild::class);
echo $child->hasMethod("childmethod") ? "M" : "m";
echo $child->hasMethod("HIDDENPARENT") ? "P" : "p";
echo $child->hasMethod("parentStatic") ? "S" : "s";
echo $child->hasMethod("missing") ? "X" : "x";
echo ":";
echo $child->hasProperty("childProp") ? "C" : "c";
echo $child->hasProperty("hiddenProp") ? "H" : "h";
echo $child->hasProperty("parentStaticProp") ? "T" : "t";
echo $child->hasProperty("childprop") ? "W" : "w";
echo ":";
$iface = new ReflectionClass(StaticMemberIface::class);
echo $iface->hasMethod("parentrequirement") ? "I" : "i";
echo $iface->hasMethod("childRequirement") ? "J" : "j";
echo $iface->hasProperty("hook") ? "K" : "k";
echo ":";
$trait = new ReflectionClass(StaticMemberTrait::class);
echo $trait->hasMethod("traithidden") ? "R" : "r";
echo $trait->hasProperty("traitProp") ? "U" : "u";
echo ":";
$pure = new ReflectionClass(StaticMemberPureEnum::class);
echo $pure->hasMethod("cases") ? "E" : "e";
echo $pure->hasMethod("label") ? "L" : "l";
echo $pure->hasProperty("name") ? "N" : "n";
echo $pure->hasProperty("value") ? "V" : "v";
echo ":";
$backed = new ReflectionClass(StaticMemberBackedEnum::class);
echo $backed->hasMethod("tryfrom") ? "B" : "b";
echo $backed->hasProperty("name") ? "N" : "n";
echo $backed->hasProperty("value") ? "Y" : "y";
"#,
    );
    assert_eq!(out, "MPSx:ChTw:IJK:RU:ELNv:BNY");
}

/// Verifies that `ReflectionClass` reports implemented interface and used trait names.
#[test]
fn test_reflection_class_reports_relation_names() {
    let out = compile_and_run(
        r#"<?php
interface StaticRelationIface {}
trait StaticRelationTrait {}
class StaticRelationTarget implements StaticRelationIface {
    use StaticRelationTrait;
}
interface StaticRelationParent {}
interface StaticRelationChild extends StaticRelationParent {}
$ref = new ReflectionClass(StaticRelationTarget::class);
$interfaces = $ref->getInterfaceNames();
$traits = $ref->getTraitNames();
echo count($interfaces) . ":" . $interfaces[0] . ":";
echo count($traits) . ":" . $traits[0] . ":";
$parentInterfaces = (new ReflectionClass(StaticRelationChild::class))->getInterfaceNames();
echo count($parentInterfaces) . ":" . $parentInterfaces[0];
"#,
    );
    assert_eq!(
        out,
        "1:StaticRelationIface:1:StaticRelationTrait:1:StaticRelationParent"
    );
}

/// Verifies that `ReflectionClass::getName()` returns the canonical declared
/// name after case-insensitive class-string construction.
#[test]
fn test_reflection_class_get_name_uses_resolved_class_case() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;
class Thing {}
$ref = new \ReflectionClass('demo\thing');
echo $ref->getName() . ":";
echo $ref->getShortName() . ":";
echo $ref->getNamespaceName() . ":";
echo $ref->inNamespace() ? "Y" : "N";
"#,
    );
    assert_eq!(out, "Demo\\Thing:Thing:Demo:Y");
}

/// Verifies that `ReflectionClass::getName()` works for a class discovered
/// through `class_exists()` autoload resolution before the reflector is built.
#[test]
fn test_reflection_class_get_name_for_autoloaded_class() {
    let out = compile_and_run_files(
        &[
            ("DemoThing.php", "<?php\nnamespace Demo;\nclass Thing {}\n"),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function ($name) {
    if (strtolower($name) === "demo\\thing") {
        require __DIR__ . "/DemoThing.php";
    }
});

if (class_exists("demo\\thing")) {
    $ref = new ReflectionClass("DEMO\\THING");
    echo $ref->getName();
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "Demo\\Thing");
}

/// Verifies that a lowercased `class_exists()` autoload demand, an
/// differently-cased `ReflectionClass` constructor, and attribute lookup all
/// resolve to the same autoloaded class declaration.
#[test]
fn test_autoload_reflection_case_insensitive_class_lookup_reads_attributes() {
    let out = compile_and_run_files(
        &[
            (
                "DemoThing.php",
                "<?php\nnamespace Demo;\n#[Marker]\nclass Thing {}\n",
            ),
            (
                "main.php",
                r#"<?php
spl_autoload_register(function ($name) {
    if ($name === "Demo\\Thing") {
        require __DIR__ . "/DemoThing.php";
    }
});

if (class_exists("demo\\thing")) {
    $r = new ReflectionClass("DEMO\\THING");
    $attrs = $r->getAttributes();
    echo $r->getName() . "\n";
    echo count($attrs) . "\n";
    echo $attrs[0]->getName() . "\n";
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "Demo\\Thing\n1\nDemo\\Marker\n");
}

/// Verifies that `ReflectionClass::getAttributes()` works when called on
/// a temporary `ReflectionClass` object directly (without storing the
/// reflector in a variable), and returns the correct attribute name and
/// argument.
#[test]
fn test_reflection_get_attributes_survives_temporary_reflector() {
    let out = compile_and_run(
        r#"<?php
#[Marker("owned")]
class Greeter {}
$attrs = (new ReflectionClass('Greeter'))->getAttributes();
echo $attrs[0]->getName() . "/";
echo $attrs[0]->getArguments()[0];
"#,
    );
    assert_eq!(out, "Marker/owned");
}

/// Verifies that `ReflectionMethod::getAttributes()` returns attribute
/// name and arguments for a method decorated with `#[Route("/home", "GET")]`.
#[test]
fn test_reflection_method_get_attributes_returns_method_attributes() {
    let out = compile_and_run(
        r#"<?php
class Controller {
    #[Route("/home", "GET")]
    public function index() {}
}
$ref = new ReflectionMethod('Controller', 'index');
$attrs = $ref->getAttributes();
echo $ref->getName() . "/";
echo count($attrs) . "/";
echo $attrs[0]->getName() . "/";
echo $attrs[0]->getArguments()[0] . "/";
echo $attrs[0]->getArguments()[1];
"#,
    );
    assert_eq!(out, "index/1/Route//home/GET");
}

/// Verifies that `ReflectionMethod`'s constructor accepts named arguments
/// (`method_name:`, `class_name:`) and `getAttributes()` returns the
/// correct attribute data.
#[test]
fn test_reflection_method_constructor_supports_named_arguments() {
    let out = compile_and_run(
        r#"<?php
class Controller {
    #[Route("/home")]
    public function index() {}
}
$ref = new ReflectionMethod(method_name: 'index', class_name: 'Controller');
$attrs = $ref->getAttributes();
echo $ref->getName() . "/";
echo count($attrs) . "/";
echo $attrs[0]->getName() . "/";
echo $attrs[0]->getArguments()[0];
"#,
    );
    assert_eq!(out, "index/1/Route//home");
}

/// Verifies that `ReflectionProperty::getAttributes()` works when the class
/// is specified via `User::class` (a class constant) and returns the correct
/// attribute name and argument.
#[test]
fn test_reflection_property_get_attributes_accepts_class_constant() {
    let out = compile_and_run(
        r#"<?php
class User {
    #[Column("id")]
    public int $id = 0;
}
$ref = new ReflectionProperty(User::class, 'id');
$attrs = $ref->getAttributes();
echo $ref->getName() . "/";
echo count($attrs) . "/";
echo $attrs[0]->getName() . "/";
echo $attrs[0]->getArguments()[0];
"#,
    );
    assert_eq!(out, "id/1/Column/id");
}

/// Verifies that `ReflectionProperty`'s constructor accepts static associative
/// array spread syntax (`...["property_name" => ..., "class_name" => ...]`) for
/// named argument passing.
#[test]
fn test_reflection_property_constructor_supports_static_assoc_spread() {
    let out = compile_and_run(
        r#"<?php
class User {
    #[Column("id")]
    public int $id = 0;
}
$ref = new ReflectionProperty(...["property_name" => "id", "class_name" => "User"]);
$attrs = $ref->getAttributes();
echo $ref->getName() . "/";
echo count($attrs) . "/";
echo $attrs[0]->getName() . "/";
echo $attrs[0]->getArguments()[0];
"#,
    );
    assert_eq!(out, "id/1/Column/id");
}

/// Verifies that `ReflectionMethod` and `ReflectionProperty` expose member
/// visibility, staticity, finality, and abstractness predicates.
#[test]
fn test_reflection_member_predicates_report_method_and_property_flags() {
    let out = compile_and_run(
        r#"<?php
abstract class ReflectMemberBase {
    protected static function baseStatic() {}
    abstract protected function mustImplement();
    final public function locked() {}
}
class ReflectMemberChild extends ReflectMemberBase {
    public function mustImplement() {}
    private static string $token = "x";
    protected int $visible = 2;
}
$baseStatic = new ReflectionMethod(ReflectMemberChild::class, "baseStatic");
echo $baseStatic->isStatic() ? "S" : "s";
echo $baseStatic->isProtected() ? "P" : "p";
echo $baseStatic->isPublic() ? "U" : "u";
echo $baseStatic->isPrivate() ? "R" : "r";
echo $baseStatic->isFinal() ? "F" : "f";
echo $baseStatic->isAbstract() ? "A" : "a";
echo ":";
$abstractMethod = new ReflectionMethod(ReflectMemberBase::class, "mustImplement");
echo $abstractMethod->isAbstract() ? "A" : "a";
echo $abstractMethod->isProtected() ? "P" : "p";
echo $abstractMethod->isStatic() ? "S" : "s";
echo ":";
$finalMethod = new ReflectionMethod(ReflectMemberChild::class, "locked");
echo $finalMethod->isFinal() ? "F" : "f";
echo $finalMethod->isPublic() ? "U" : "u";
echo $finalMethod->isStatic() ? "S" : "s";
echo ":";
$staticProp = new ReflectionProperty(ReflectMemberChild::class, "token");
echo $staticProp->isStatic() ? "S" : "s";
echo $staticProp->isPrivate() ? "R" : "r";
echo $staticProp->isProtected() ? "P" : "p";
echo ":";
$visibleProp = new ReflectionProperty(ReflectMemberChild::class, "visible");
echo $visibleProp->isStatic() ? "S" : "s";
echo $visibleProp->isProtected() ? "P" : "p";
echo $visibleProp->isPublic() ? "U" : "u";
"#,
    );
    assert_eq!(out, "SPurfa:APs:FUs:SRp:sPu");
}

/// Verifies that `ReflectionClass::getMethods()` and `getProperties()` return
/// populated ReflectionMethod/ReflectionProperty objects with member metadata.
#[test]
fn test_reflection_class_get_methods_and_properties_return_member_objects() {
    let out = compile_and_run_capture(
        r#"<?php
#[Attribute]
class ListMarker {}
class ReflectListTarget {
    #[ListMarker]
    public function first() {}
    private static function helper() {}
    #[ListMarker]
    protected int $visible = 1;
    private static string $token = "x";
}
$ref = new ReflectionClass(ReflectListTarget::class);
$methods = $ref->getMethods();
$properties = $ref->getProperties();
echo count($methods) . ":" . count($properties) . ":";
foreach ($methods as $method) {
    if ($method->getName() === "first") {
        echo "F" . count($method->getAttributes());
    }
    if ($method->getName() === "helper") {
        echo $method->isStatic() ? "S" : "s";
        echo $method->isPrivate() ? "R" : "r";
    }
}
echo ":";
foreach ($properties as $property) {
    if ($property->getName() === "visible") {
        echo "V" . count($property->getAttributes());
        echo $property->isProtected() ? "P" : "p";
    }
    if ($property->getName() === "token") {
        echo $property->isStatic() ? "T" : "t";
        echo $property->isPrivate() ? "R" : "r";
    }
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "2:2:F1SR:V1PTR");
}

/// Verifies that `ReflectionMethod::getParameters()` returns populated
/// ReflectionParameter objects for typed, by-reference, defaulted, and variadic parameters.
#[test]
fn test_reflection_method_get_parameters_returns_parameter_metadata() {
    let out = compile_and_run_capture(
        r##"<?php
class ReflectParamTarget {
    public function run(int $id, &$name, string $mode = "x", ...$rest) {}
}
interface ReflectParamInterface {
    public function iface(int $id, $name = "x");
}
trait ReflectParamTrait {
    private static function traitRun(int $id, $name = "x", string ...$rest) {}
}
$method = new ReflectionMethod(ReflectParamTarget::class, "run");
echo $method->getNumberOfParameters() . "/";
echo $method->getNumberOfRequiredParameters() . ":";
$params = $method->getParameters();
foreach ($params as $param) {
    echo $param->getName() . "#" . $param->getPosition();
    echo ($param->hasType() ? "T" : "t");
    echo ($param->isOptional() ? "O" : "R");
    echo ($param->isPassedByReference() ? "B" : "b");
    echo ($param->isVariadic() ? "V" : "v");
    echo "|";
}
echo "\n";
$iface = new ReflectionMethod(ReflectParamInterface::class, "iface");
echo $iface->getNumberOfParameters() . "/";
echo $iface->getNumberOfRequiredParameters() . ":";
$ifaceParams = $iface->getParameters();
echo $ifaceParams[0]->getName() . ($ifaceParams[0]->hasType() ? "T" : "t");
echo ":" . $ifaceParams[1]->getName() . ($ifaceParams[1]->isOptional() ? "O" : "R");
echo "\n";
$trait = new ReflectionMethod(ReflectParamTrait::class, "traitRun");
echo $trait->getNumberOfParameters() . "/";
echo $trait->getNumberOfRequiredParameters() . ":";
echo ($trait->isStatic() ? "S" : "s");
echo ($trait->isPrivate() ? "R" : "r");
$traitParams = $trait->getParameters();
echo ":" . $traitParams[2]->getName() . ($traitParams[2]->isVariadic() ? "V" : "v");
echo ($traitParams[2]->hasType() ? "T" : "t");
"##,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "4/2:id#0TRbv|name#1tRBv|mode#2TObv|rest#3tObV|\n2/1:idT:nameO\n3/1:SR:restVT"
    );
}

/// Verifies that ReflectionMethod objects returned from `ReflectionClass::getMethods()`
/// carry the same parameter metadata as directly constructed method reflectors.
#[test]
fn test_reflection_class_get_methods_preserves_parameter_metadata() {
    let out = compile_and_run_capture(
        r##"<?php
class ReflectListedParamTarget {
    public function listed(int $first, $second = 2) {}
}
trait ReflectListedParamTrait {
    protected static function traitListed(int $first, $second = 2, string ...$rest) {}
}
$methods = (new ReflectionClass(ReflectListedParamTarget::class))->getMethods();
foreach ($methods as $method) {
    if ($method->getName() === "listed") {
        $params = $method->getParameters();
        echo $method->getNumberOfParameters() . "/";
        echo $method->getNumberOfRequiredParameters() . ":";
        echo $params[0]->getName() . ($params[0]->hasType() ? "T" : "t");
        echo ":";
        echo $params[1]->getName() . ($params[1]->isOptional() ? "O" : "R");
    }
}
echo "|";
$traitMethods = (new ReflectionClass(ReflectListedParamTrait::class))->getMethods();
foreach ($traitMethods as $method) {
    if ($method->getName() === "traitlisted") {
        $params = $method->getParameters();
        echo $method->getNumberOfParameters() . "/";
        echo $method->getNumberOfRequiredParameters() . ":";
        echo ($method->isStatic() ? "S" : "s");
        echo ($method->isProtected() ? "P" : "p");
        echo ":";
        echo $params[0]->getName() . ($params[0]->hasType() ? "T" : "t");
        echo ":";
        echo $params[1]->getName() . ($params[1]->isOptional() ? "O" : "R");
        echo ":";
        echo $params[2]->getName() . ($params[2]->isVariadic() ? "V" : "v");
        echo ($params[2]->hasType() ? "T" : "t");
    }
}
"##,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "2/1:firstT:secondO|3/1:SP:firstT:secondO:restVT"
    );
}

/// Verifies that `ReflectionClass::getConstructor()` returns a ReflectionMethod
/// for direct, inherited, interface, and trait constructors, and null otherwise.
#[test]
fn test_reflection_class_get_constructor_returns_method_or_null() {
    let out = compile_and_run(
        r#"<?php
class ReflectCtorBase {
    public function __construct($required, $optional = 2) {}
}
class ReflectCtorChild extends ReflectCtorBase {}
class ReflectCtorPlain {}
interface ReflectCtorInterface {
    public function __construct($required);
}
trait ReflectCtorTrait {
    public function __construct($required, $optional = null, ...$rest) {}
}

$base = (new ReflectionClass(ReflectCtorBase::class))->getConstructor();
if ($base instanceof ReflectionMethod) {
    echo $base->getName();
    echo "/" . $base->getNumberOfParameters();
    echo "/" . $base->getNumberOfRequiredParameters();
} else {
    echo "null";
}
echo ":";

$child = (new ReflectionClass(ReflectCtorChild::class))->getConstructor();
if ($child instanceof ReflectionMethod) {
    echo $child->getName();
    echo "/" . $child->getNumberOfParameters();
    echo "/" . $child->getNumberOfRequiredParameters();
} else {
    echo "null";
}
echo ":";

$plain = (new ReflectionClass(ReflectCtorPlain::class))->getConstructor();
if ($plain instanceof ReflectionMethod) {
    echo $plain->getName();
} else {
    echo "null";
}
echo ":";

$interface = (new ReflectionClass(ReflectCtorInterface::class))->getConstructor();
if ($interface instanceof ReflectionMethod) {
    echo $interface->getName();
    echo "/" . $interface->getNumberOfParameters();
    echo "/" . $interface->getNumberOfRequiredParameters();
} else {
    echo "null";
}
echo ":";

$trait = (new ReflectionClass(ReflectCtorTrait::class))->getConstructor();
if ($trait instanceof ReflectionMethod) {
    echo $trait->getName();
    echo "/" . $trait->getNumberOfParameters();
    echo "/" . $trait->getNumberOfRequiredParameters();
} else {
    echo "null";
}
"#,
    );
    assert_eq!(
        out,
        "__construct/2/1:__construct/2/1:null:__construct/1/1:__construct/3/1"
    );
}

/// Verifies that `ReflectionClass::newInstance()` constructs reflected classes
/// and forwards direct and statically-unpacked constructor arguments.
#[test]
fn test_reflection_class_new_instance_constructs_reflected_class() {
    let out = compile_and_run(
        r#"<?php
class ReflectNewTarget {
    public string $label = "";
    public function __construct(string $left, string $right) {
        $this->label = $left . $right;
    }
    public function label(): string {
        return $this->label;
    }
}
$ref = new ReflectionClass(ReflectNewTarget::class);
$first = $ref->newInstance("A", "B");
echo $first->label() . ":";
$second = $ref->newInstance(...["C", "D"]);
echo $second->label();
"#,
    );
    assert_eq!(out, "AB:CD");
}

/// Verifies that `ReflectionClassConstant` and enum-case reflectors expose
/// attribute name, arguments, `getName()`, and `newInstance()` data.
#[test]
fn test_reflection_constant_and_enum_case_get_attributes() {
    let out = compile_and_run(
        r#"<?php
class Marker {
    public function __construct(public string $label) {}
    public function label(): string { return $this->label; }
}
class ConstTarget {
    #[Marker("const")]
    public const ANSWER = 42;
}
enum CaseTarget: string {
    #[Marker("case")]
    case Ready = "ready";
}
$const = new ReflectionClassConstant(ConstTarget::class, "ANSWER");
$constAttrs = $const->getAttributes();
echo $const->getName() . "/";
echo count($constAttrs) . "/";
echo $constAttrs[0]->getName() . "/";
echo $constAttrs[0]->getArguments()[0] . "/";
echo $constAttrs[0]->newInstance()->label() . "\n";
$case = new ReflectionClassConstant(CaseTarget::class, "Ready");
$caseAttrs = $case->getAttributes();
echo $case->getName() . "/";
echo count($caseAttrs) . "/";
echo $caseAttrs[0]->getName() . "/";
echo $caseAttrs[0]->getArguments()[0] . "/";
echo $caseAttrs[0]->newInstance()->label() . "\n";
$unit = new ReflectionEnumUnitCase(CaseTarget::class, "Ready");
$unitAttrs = $unit->getAttributes();
echo $unit->getName() . "/";
echo $unitAttrs[0]->newInstance()->label() . "\n";
$backed = new ReflectionEnumBackedCase(CaseTarget::class, "Ready");
$backedAttrs = $backed->getAttributes();
echo $backed->getName() . "/";
echo $backedAttrs[0]->newInstance()->label();
"#,
    );
    assert_eq!(
        out,
        "ANSWER/1/Marker/const/const\nReady/1/Marker/case/case\nReady/case\nReady/case"
    );
}

/// Verifies that `ReflectionClass` accepts `user::class` (lowercase class
/// constant) for case-insensitive class resolution and `getAttributes()`
/// returns the correct attribute data.
#[test]
fn test_reflection_class_constant_lookup_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
#[Marker("ok")]
class User {}
$ref = new ReflectionClass(user::class);
$attrs = $ref->getAttributes();
echo count($attrs) . "/";
echo $attrs[0]->getName() . "/";
echo $attrs[0]->getArguments()[0];
"#,
    );
    assert_eq!(out, "1/Marker/ok");
}

/// Verifies that `ReflectionAttribute::newInstance()` invokes the attribute
/// class constructor lazily (on demand) and returns an instance of the
/// attribute class. The test also verifies that the reflector is not
/// destructed before the `newInstance()` call completes.
#[test]
fn test_reflection_attribute_new_instance_runs_on_demand() {
    let out = compile_and_run(
        r#"<?php
class Route {
    public function __construct(string $path) {
        echo "ctor:" . $path . "\n";
    }
}
#[Route("/lazy")]
class Controller {}
$ref = new ReflectionClass('Controller');
echo "before\n";
$attrs = $ref->getAttributes();
echo "middle\n";
$instance = $attrs[0]->newInstance();
echo ($instance instanceof Route) ? "instance\n" : "bad\n";
"#,
    );
    assert_eq!(out, "before\nmiddle\nctor:/lazy\ninstance\n");
}

/// Verifies that a bare expression statement calling
/// `$attrs[0]->newInstance()` (without using the return value) compiles
/// correctly and the side effect (constructor echo) is preserved.
#[test]
fn test_reflection_attribute_new_instance_expression_statement_is_preserved() {
    let out = compile_and_run(
        r#"<?php
class Route {
    public function __construct(string $path) {
        echo "ctor:" . $path;
    }
}
#[Route("/effect")]
class Controller {}
$attrs = (new ReflectionClass('Controller'))->getAttributes();
$attrs[0]->newInstance();
"#,
    );
    assert_eq!(out, "ctor:/effect");
}

/// Verifies that `ReflectionAttribute::getArguments()` and
/// `ReflectionAttribute::newInstance()` both preserve large negative integer
/// arguments (-65537) without truncation or sign flipping.
#[test]
fn test_reflection_attribute_new_instance_preserves_large_negative_int_args() {
    let out = compile_and_run(
        r#"<?php
class Code {
    public function __construct(int $value) {
        echo $value;
    }
}
#[Code(-65537)]
class Controller {}
$attrs = (new ReflectionClass('Controller'))->getAttributes();
echo $attrs[0]->getArguments()[0] . "/";
$attrs[0]->newInstance();
"#,
    );
    assert_eq!(out, "-65537/-65537");
}
