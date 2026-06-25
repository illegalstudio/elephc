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

/// Verifies that `ReflectionAttribute::getTarget()` and `isRepeated()` report
/// PHP-compatible owner target bits and duplicate-owner metadata.
#[test]
fn test_reflection_attribute_target_and_repetition_metadata() {
    let out = compile_and_run(
        r#"<?php
class TargetMarker {
    public function __construct(string $name = "") {}
}
#[TargetMarker("class-a"), TargetMarker("class-b")]
class ReflectAttributeTarget {
    #[TargetMarker("method")]
    public function run(#[TargetMarker("param")] $id): void {}
    #[TargetMarker("property")]
    public int $id = 1;
    #[TargetMarker("const")]
    public const ANSWER = 42;
}
enum ReflectAttributeEnum {
    #[TargetMarker("case")]
    case Ready;
}
$classAttrs = (new ReflectionClass(ReflectAttributeTarget::class))->getAttributes();
echo $classAttrs[0]->getTarget() . "/" . ($classAttrs[0]->isRepeated() ? "R" : "r") . ":";
echo $classAttrs[1]->getTarget() . "/" . ($classAttrs[1]->isRepeated() ? "R" : "r") . ":";
$methodAttr = (new ReflectionMethod(ReflectAttributeTarget::class, "run"))->getAttributes()[0];
echo $methodAttr->getTarget() . "/" . ($methodAttr->isRepeated() ? "R" : "r") . ":";
$propertyAttr = (new ReflectionProperty(ReflectAttributeTarget::class, "id"))->getAttributes()[0];
echo $propertyAttr->getTarget() . "/" . ($propertyAttr->isRepeated() ? "R" : "r") . ":";
$paramAttr = (new ReflectionMethod(ReflectAttributeTarget::class, "run"))->getParameters()[0]->getAttributes()[0];
echo $paramAttr->getTarget() . "/" . ($paramAttr->isRepeated() ? "R" : "r") . ":";
$constAttr = (new ReflectionClassConstant(ReflectAttributeTarget::class, "ANSWER"))->getAttributes()[0];
echo $constAttr->getTarget() . "/" . ($constAttr->isRepeated() ? "R" : "r") . ":";
$caseAttr = (new ReflectionEnumUnitCase(ReflectAttributeEnum::class, "Ready"))->getAttributes()[0];
echo $caseAttr->getTarget() . "/" . ($caseAttr->isRepeated() ? "R" : "r");
"#,
    );
    assert_eq!(out, "1/R:1/R:4/r:8/r:32/r:16/r:16/r");
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

/// Verifies that `ReflectionProperty::IS_*` constants use PHP modifier values.
#[test]
fn test_reflection_property_modifier_constants_report_php_values() {
    let out = compile_and_run(
        r#"<?php
echo ReflectionProperty::IS_STATIC . ":";
echo ReflectionProperty::IS_READONLY . ":";
echo ReflectionProperty::IS_PUBLIC . ":";
echo ReflectionProperty::IS_PROTECTED . ":";
echo ReflectionProperty::IS_PRIVATE . ":";
echo ReflectionProperty::IS_ABSTRACT . ":";
echo ReflectionProperty::IS_PROTECTED_SET . ":";
echo ReflectionProperty::IS_PRIVATE_SET . ":";
echo ReflectionProperty::IS_VIRTUAL . ":";
echo ReflectionProperty::IS_FINAL;
"#,
    );
    assert_eq!(out, "16:128:1:2:4:64:2048:4096:512:32");
}

/// Verifies that property asymmetric visibility contributes PHP modifier bits.
#[test]
fn test_reflection_property_get_modifiers_reports_asymmetric_visibility() {
    let out = compile_and_run(
        r#"<?php
class ReflectSetVisibility {
    public private(set) int $privateSet = 1;
    public protected(set) int $protectedSet = 2;
}
$private = new ReflectionProperty(ReflectSetVisibility::class, "privateSet");
echo $private->isPrivateSet() ? "P" : "p";
echo $private->isProtectedSet() ? "T" : "t";
echo $private->getModifiers() . ":";
$protected = new ReflectionProperty(ReflectSetVisibility::class, "protectedSet");
echo $protected->isPrivateSet() ? "P" : "p";
echo $protected->isProtectedSet() ? "T" : "t";
echo $protected->getModifiers();
echo ":";
echo $protected->isDynamic() ? "D" : "d";
echo ":";
$object = new ReflectSetVisibility();
echo $protected->isLazy($object) ? "L" : "l";
$protected->skipLazyInitialization($object);
echo ":ok";
"#,
    );
    assert_eq!(out, "Pt4129:pT2049:d:l:ok");
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

/// Verifies that `ReflectionClass::isInstantiable()` reports PHP constructor
/// visibility and class-kind rules for static metadata.
#[test]
fn test_reflection_class_is_instantiable() {
    let out = compile_and_run(
        r#"<?php
abstract class ReflectInstAbstract {}
class ReflectInstPublic {}
final class ReflectInstFinal {}
class ReflectInstPrivate { private function __construct() {} }
class ReflectInstProtected { protected function __construct() {} }
interface ReflectInstIface {}
trait ReflectInstTrait {}
enum ReflectInstEnum { case Ready; }
echo (new ReflectionClass(ReflectInstAbstract::class))->isInstantiable() ? "A" : "a";
echo (new ReflectionClass(ReflectInstPublic::class))->isInstantiable() ? "B" : "b";
echo (new ReflectionClass(ReflectInstFinal::class))->isInstantiable() ? "C" : "c";
echo (new ReflectionClass(ReflectInstPrivate::class))->isInstantiable() ? "P" : "p";
echo (new ReflectionClass(ReflectInstProtected::class))->isInstantiable() ? "R" : "r";
echo (new ReflectionClass(ReflectInstIface::class))->isInstantiable() ? "I" : "i";
echo (new ReflectionClass(ReflectInstTrait::class))->isInstantiable() ? "T" : "t";
echo (new ReflectionClass(ReflectInstEnum::class))->isInstantiable() ? "E" : "e";
"#,
    );
    assert_eq!(out, "aBCprite");
}

/// Verifies that `ReflectionClass::isCloneable()` reports PHP clone visibility,
/// class-kind, and elephc runtime-storage rules for static metadata.
#[test]
fn test_reflection_class_is_cloneable() {
    let out = compile_and_run(
        r#"<?php
abstract class ReflectCloneAbstract {}
class ReflectClonePlain {}
final class ReflectCloneFinal {}
class ReflectClonePrivate { private function __clone() {} }
class ReflectCloneProtected { protected function __clone() {} }
class ReflectClonePublic { public function __clone() {} }
interface ReflectCloneIface {}
trait ReflectCloneTrait {}
enum ReflectCloneEnum { case Ready; }
echo (new ReflectionClass(ReflectCloneAbstract::class))->isCloneable() ? "A" : "a";
echo (new ReflectionClass(ReflectClonePlain::class))->isCloneable() ? "P" : "p";
echo (new ReflectionClass(ReflectCloneFinal::class))->isCloneable() ? "F" : "f";
echo (new ReflectionClass(ReflectClonePrivate::class))->isCloneable() ? "V" : "v";
echo (new ReflectionClass(ReflectCloneProtected::class))->isCloneable() ? "R" : "r";
echo (new ReflectionClass(ReflectClonePublic::class))->isCloneable() ? "U" : "u";
echo (new ReflectionClass(ReflectCloneIface::class))->isCloneable() ? "I" : "i";
echo (new ReflectionClass(ReflectCloneTrait::class))->isCloneable() ? "T" : "t";
echo (new ReflectionClass(ReflectCloneEnum::class))->isCloneable() ? "E" : "e";
echo (new ReflectionClass(stdClass::class))->isCloneable() ? "S" : "s";
echo (new ReflectionClass(ReflectionClass::class))->isCloneable() ? "C" : "c";
"#,
    );
    assert_eq!(out, "aPFvrUiteSc");
}

/// Verifies that `ReflectionClass::isIterable()` and its historical alias
/// report PHP Traversable-compatible class metadata.
#[test]
fn test_reflection_class_is_iterable() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
class ReflectIterablePlain {}
abstract class ReflectIterableAbstract implements Iterator {}
interface ReflectIterableIface extends Iterator {}
trait ReflectIterableTrait {}
enum ReflectIterableEnum { case Ready; }
class ReflectIterableIterator implements Iterator {
    public function current(): mixed { return null; }
    public function key(): mixed { return null; }
    public function next(): void {}
    public function valid(): bool { return false; }
    public function rewind(): void {}
}
class ReflectIterableAggregate implements IteratorAggregate {
    public function getIterator(): Traversable { return $this; }
}
echo (new ReflectionClass(ReflectIterablePlain::class))->isIterable() ? "P" : "p";
$iter = new ReflectionClass(ReflectIterableIterator::class);
echo $iter->isIterable() ? "I" : "i";
echo $iter->isIterateable() ? "A" : "a";
echo (new ReflectionClass(ReflectIterableAggregate::class))->isIterable() ? "G" : "g";
echo (new ReflectionClass(ReflectIterableAbstract::class))->isIterable() ? "B" : "b";
echo (new ReflectionClass(ReflectIterableIface::class))->isIterable() ? "F" : "f";
echo (new ReflectionClass(Iterator::class))->isIterable() ? "T" : "t";
echo (new ReflectionClass(ArrayIterator::class))->isIterable() ? "R" : "r";
echo (new ReflectionClass(stdClass::class))->isIterable() ? "S" : "s";
echo (new ReflectionClass(ReflectIterableEnum::class))->isIterable() ? "E" : "e";
echo (new ReflectionClass(ReflectIterableTrait::class))->isIterable() ? "H" : "h";
"#,
        67_108_864,
    );
    assert_eq!(out, "pIAGbftRseh");
}

/// Verifies that `ReflectionClass::isInternal()` and `isUserDefined()` report
/// whether class-like metadata came from compiler built-ins or user code.
#[test]
fn test_reflection_class_internal_user_defined_predicates() {
    let out = compile_and_run(
        r#"<?php
class ReflectOriginClass {}
interface ReflectOriginIface {}
trait ReflectOriginTrait {}
enum ReflectOriginEnum { case Ready; }
$class = new ReflectionClass(ReflectOriginClass::class);
echo $class->isInternal() ? "I" : "i"; echo $class->isUserDefined() ? "U" : "u"; echo ":";
$iface = new ReflectionClass(ReflectOriginIface::class);
echo $iface->isInternal() ? "I" : "i"; echo $iface->isUserDefined() ? "U" : "u"; echo ":";
$trait = new ReflectionClass(ReflectOriginTrait::class);
echo $trait->isInternal() ? "I" : "i"; echo $trait->isUserDefined() ? "U" : "u"; echo ":";
$enum = new ReflectionClass(ReflectOriginEnum::class);
echo $enum->isInternal() ? "I" : "i"; echo $enum->isUserDefined() ? "U" : "u"; echo ":";
$std = new ReflectionClass(stdClass::class);
echo $std->isInternal() ? "I" : "i"; echo $std->isUserDefined() ? "U" : "u"; echo ":";
$reflection = new ReflectionClass(ReflectionClass::class);
echo $reflection->isInternal() ? "I" : "i"; echo $reflection->isUserDefined() ? "U" : "u"; echo ":";
$iterator = new ReflectionClass(Iterator::class);
echo $iterator->isInternal() ? "I" : "i"; echo $iterator->isUserDefined() ? "U" : "u"; echo ":";
"#,
    );
    assert_eq!(out, "iU:iU:iU:iU:Iu:Iu:Iu:");
}

/// Verifies that `ReflectionClass::hasMethod()`, `hasProperty()`, and
/// `hasConstant()` report PHP-visible members for static class-like metadata.
#[test]
fn test_reflection_class_reports_member_existence() {
    let out = compile_and_run(
        r#"<?php
class StaticMemberParent {
    const PARENT_CONST = 1;
    private function hiddenParent() {}
    protected static function parentStatic() {}
    private $hiddenProp;
    protected static $parentStaticProp;
}
interface StaticMemberClassIface {
    const CLASS_LIMIT = 10;
}
class StaticMemberChild extends StaticMemberParent implements StaticMemberClassIface {
    const CHILD_CONST = 2;
    public function ChildMethod() {}
    public $childProp;
}
interface StaticMemberIfaceParent {
    const PARENT_LIMIT = 10;
    public function parentRequirement();
}
interface StaticMemberIface extends StaticMemberIfaceParent {
    const CHILD_LIMIT = 20;
    public function childRequirement();
    public string $hook { get; }
}
trait StaticMemberTrait {
    const TRAIT_CONST = 30;
    private function traitHidden() {}
    public $traitProp;
}
enum StaticMemberPureEnum {
    case Ready;
    const LEVEL = 40;
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
echo $child->hasConstant("CHILD_CONST") ? "D" : "d";
echo $child->hasConstant("PARENT_CONST") ? "P" : "p";
echo $child->hasConstant("CLASS_LIMIT") ? "A" : "a";
echo $child->hasConstant("child_const") ? "Z" : "z";
echo ":";
$iface = new ReflectionClass(StaticMemberIface::class);
echo $iface->hasMethod("parentrequirement") ? "I" : "i";
echo $iface->hasMethod("childRequirement") ? "J" : "j";
echo $iface->hasProperty("hook") ? "K" : "k";
echo $iface->hasConstant("PARENT_LIMIT") ? "L" : "l";
echo $iface->hasConstant("CHILD_LIMIT") ? "C" : "c";
echo ":";
$trait = new ReflectionClass(StaticMemberTrait::class);
echo $trait->hasMethod("traithidden") ? "R" : "r";
echo $trait->hasProperty("traitProp") ? "U" : "u";
echo $trait->hasConstant("TRAIT_CONST") ? "K" : "k";
echo ":";
$pure = new ReflectionClass(StaticMemberPureEnum::class);
echo $pure->hasMethod("cases") ? "E" : "e";
echo $pure->hasMethod("label") ? "L" : "l";
echo $pure->hasProperty("name") ? "N" : "n";
echo $pure->hasProperty("value") ? "V" : "v";
echo $pure->hasConstant("Ready") ? "G" : "g";
echo $pure->hasConstant("LEVEL") ? "F" : "f";
echo $pure->hasConstant("ready") ? "R" : "r";
echo ":";
$backed = new ReflectionClass(StaticMemberBackedEnum::class);
echo $backed->hasMethod("tryfrom") ? "B" : "b";
echo $backed->hasProperty("name") ? "N" : "n";
echo $backed->hasProperty("value") ? "Y" : "y";
echo $backed->hasConstant("Ready") ? "Q" : "q";
"#,
    );
    assert_eq!(out, "MPSx:ChTwDPAz:IJKLC:RUK:ELNvGFr:BNYQ");
}

/// Verifies that `ReflectionClass::getConstant()` and `getConstants()` expose
/// class, parent, interface, trait, private, and enum-case constants.
#[test]
fn test_reflection_class_returns_constant_values() {
    let out = compile_and_run(
        r#"<?php
class ReflectConstBase {
    public const BASE = 1;
}
interface ReflectConstIface {
    public const LIMIT = 2;
}
trait ReflectConstTrait {
    public const TRAIT_VALUE = 3;
}
class ReflectConstChild extends ReflectConstBase implements ReflectConstIface {
    private const SECRET = 9;
    public const OWN = "own";
    public const SUM = parent::BASE + 4;
    public const NAME = self::class;
}
enum ReflectConstEnum {
    case Ready;
    public const LEVEL = 40;
}
$ref = new ReflectionClass(ReflectConstChild::class);
$all = $ref->getConstants();
echo $ref->getConstant("OWN") . ":";
echo $ref->getConstant("BASE") . ":";
echo $ref->getConstant("LIMIT") . ":";
echo $ref->getConstant("SECRET") . ":";
echo $ref->getConstant("SUM") . ":";
echo $ref->getConstant("NAME") . ":";
echo $ref->getConstant("own") ? "bad" : "missing";
echo ":" . count($all) . ":" . $all["OWN"] . ":" . $all["BASE"] . ":" . $all["LIMIT"];
$trait = new ReflectionClass(ReflectConstTrait::class);
$traitAll = $trait->getConstants();
echo ":" . $trait->getConstant("TRAIT_VALUE") . ":" . count($traitAll) . ":" . $traitAll["TRAIT_VALUE"];
$enum = new ReflectionClass(ReflectConstEnum::class);
$case = $enum->getConstant("Ready");
$enumAll = $enum->getConstants();
echo ":" . $case->name;
echo ":" . $enum->getConstant("LEVEL") . ":" . $enumAll["LEVEL"] . ":" . count($enumAll);
"#,
    );
    assert_eq!(
        out,
        "own:1:2:9:5:ReflectConstChild:missing:6:own:1:2:3:1:3:Ready:40:40:2"
    );
}

/// Verifies that `ReflectionClass::getReflectionConstant()` and
/// `getReflectionConstants()` expose constant and enum-case reflector objects.
#[test]
fn test_reflection_class_returns_constant_reflector_objects() {
    let out = compile_and_run(
        r#"<?php
class ReflectConstMarker {
    public function __construct(public string $label) {}
    public function label(): string { return $this->label; }
}
class ReflectConstObjectTarget {
    #[ReflectConstMarker("const")]
    public const ANSWER = 42;
}
enum ReflectConstObjectEnum {
    #[ReflectConstMarker("case")]
    case Ready;
    public const LEVEL = 7;
}
$ref = new ReflectionClass(ReflectConstObjectTarget::class);
$single = $ref->getReflectionConstant("ANSWER");
$all = $ref->getReflectionConstants();
echo $single->getName() . ":";
echo count($all) . ":" . $all[0]->getName() . ":";
$singleAttrs = $all[0]->getAttributes();
echo $singleAttrs[0]->newInstance()->label() . ":";
echo $ref->getReflectionConstant("answer") ? "bad" : "missing";
$enum = new ReflectionClass(ReflectConstObjectEnum::class);
$enumAll = $enum->getReflectionConstants();
$case = $enum->getReflectionConstant("Ready");
$level = $enum->getReflectionConstant("LEVEL");
echo ":" . count($enumAll) . ":" . $enumAll[0]->getName() . ":" . $enumAll[1]->getName();
$caseAttrs = $enumAll[0]->getAttributes();
$levelAttrs = $enumAll[1]->getAttributes();
echo ":" . $caseAttrs[0]->newInstance()->label() . ":";
echo count($levelAttrs);
"#,
    );
    assert_eq!(out, "ANSWER:1:ANSWER:const:missing:2:Ready:LEVEL:case:0");
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
echo count($parentInterfaces) . ":" . $parentInterfaces[0] . ":";
$interfaceObjects = $ref->getInterfaces();
echo count($interfaceObjects) . ":" . $interfaceObjects["StaticRelationIface"]->getName() . ":";
$traitObjects = $ref->getTraits();
echo count($traitObjects) . ":" . $traitObjects["StaticRelationTrait"]->getName() . ":";
$parentInterfaceObjects = (new ReflectionClass(StaticRelationChild::class))->getInterfaces();
echo count($parentInterfaceObjects) . ":" . $parentInterfaceObjects["StaticRelationParent"]->getName();
"#,
    );
    assert_eq!(
        out,
        "1:StaticRelationIface:1:StaticRelationTrait:1:StaticRelationParent:1:StaticRelationIface:1:StaticRelationTrait:1:StaticRelationParent"
    );
}

/// Verifies that `ReflectionClass::getTraitAliases()` reports direct trait
/// method aliases as PHP's alias-name to `Trait::method` map.
#[test]
fn test_reflection_class_reports_trait_aliases() {
    let out = compile_and_run(
        r#"<?php
trait StaticAliasOne {
    public function first(): string { return "first"; }
}
trait StaticAliasTwo {
    public function second(): string { return "second"; }
}
class StaticAliasTarget {
    use StaticAliasOne, StaticAliasTwo {
        StaticAliasOne::first as relationAlias;
        StaticAliasTwo::second as private hiddenOther;
    }
}
$aliases = (new ReflectionClass(StaticAliasTarget::class))->getTraitAliases();
echo count($aliases) . ":" . $aliases["relationAlias"] . ":" . $aliases["hiddenOther"];
"#,
    );
    assert_eq!(
        out,
        "2:StaticAliasOne::first:StaticAliasTwo::second"
    );
}

/// Verifies that `ReflectionClass::implementsInterface()` reports class, enum,
/// and interface metadata using case-insensitive interface names.
#[test]
fn test_reflection_class_implements_interface() {
    let out = compile_and_run_capture(
        r#"<?php
interface StaticImplBase {}
interface StaticImplChild extends StaticImplBase {}
class StaticImplTarget implements StaticImplChild {}
enum StaticImplEnum implements StaticImplBase { case Ready; }
trait StaticImplTrait {}
echo (new ReflectionClass(StaticImplTarget::class))->implementsInterface(StaticImplChild::class) ? "C" : "c";
echo (new ReflectionClass(StaticImplTarget::class))->implementsInterface("staticimplbase") ? "B" : "b";
echo (new ReflectionClass(StaticImplEnum::class))->implementsInterface(StaticImplBase::class) ? "E" : "e";
echo (new ReflectionClass(StaticImplChild::class))->implementsInterface(StaticImplChild::class) ? "I" : "i";
echo (new ReflectionClass(StaticImplChild::class))->implementsInterface(StaticImplBase::class) ? "P" : "p";
echo (new ReflectionClass(StaticImplTrait::class))->implementsInterface(StaticImplBase::class) ? "T" : "t";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "CBEIPt");
}

/// Verifies that `ReflectionClass::implementsInterface()` throws PHP-compatible
/// ReflectionException objects for missing or non-interface argument names.
#[test]
fn test_reflection_class_implements_interface_rejects_non_interfaces() {
    let out = compile_and_run_capture(
        r#"<?php
interface StaticImplRejectIface {}
interface StaticImplRejectOther {}
class StaticImplRejectTarget implements StaticImplRejectIface {}
class StaticImplRejectClass {}
trait StaticImplRejectTrait {}
enum StaticImplRejectEnum { case Ready; }
$ref = new ReflectionClass(StaticImplRejectTarget::class);
echo $ref->implementsInterface(StaticImplRejectOther::class) ? "T" : "F";
try {
    $ref->implementsInterface("StaticImplRejectClass");
    echo ":ok";
} catch (ReflectionException $e) {
    echo ":" . get_class($e) . ":" . $e->getMessage();
}
try {
    $ref->implementsInterface("StaticImplRejectTrait");
    echo ":ok";
} catch (ReflectionException $e) {
    echo ":" . get_class($e) . ":" . $e->getMessage();
}
try {
    $ref->implementsInterface("StaticImplRejectEnum");
    echo ":ok";
} catch (ReflectionException $e) {
    echo ":" . get_class($e) . ":" . $e->getMessage();
}
try {
    $ref->implementsInterface("StaticImplRejectMissing");
    echo ":ok";
} catch (ReflectionException $e) {
    echo ":" . get_class($e) . ":" . $e->getMessage();
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "F:ReflectionException:StaticImplRejectClass is not an interface:ReflectionException:StaticImplRejectTrait is not an interface:ReflectionException:StaticImplRejectEnum is not an interface:ReflectionException:Interface \"StaticImplRejectMissing\" does not exist"
    );
}

/// Verifies that `ReflectionClass::isSubclassOf()` reports parent classes and
/// inherited interfaces while excluding self and accepting trait/enum targets as false.
#[test]
fn test_reflection_class_is_subclass_of() {
    let out = compile_and_run_capture(
        r#"<?php
interface StaticSubclassIface {}
interface StaticSubclassChildIface extends StaticSubclassIface {}
class StaticSubclassBase {}
class StaticSubclassParent extends StaticSubclassBase {}
class StaticSubclassChild extends StaticSubclassParent implements StaticSubclassChildIface {}
trait StaticSubclassTrait {}
enum StaticSubclassEnum implements StaticSubclassIface { case Ready; }
$ref = new ReflectionClass(StaticSubclassChild::class);
echo $ref->isSubclassOf(StaticSubclassParent::class) ? "P" : "p";
echo $ref->isSubclassOf("staticsubclassbase") ? "B" : "b";
echo $ref->isSubclassOf(StaticSubclassIface::class) ? "I" : "i";
echo $ref->isSubclassOf(StaticSubclassChild::class) ? "S" : "s";
echo (new ReflectionClass(StaticSubclassChildIface::class))->isSubclassOf(StaticSubclassIface::class) ? "J" : "j";
echo (new ReflectionClass(StaticSubclassIface::class))->isSubclassOf(StaticSubclassIface::class) ? "X" : "x";
echo $ref->isSubclassOf(StaticSubclassTrait::class) ? "T" : "t";
echo $ref->isSubclassOf(StaticSubclassEnum::class) ? "Q" : "q";
echo (new ReflectionClass(StaticSubclassEnum::class))->isSubclassOf(StaticSubclassIface::class) ? "E" : "e";
try {
    $ref->isSubclassOf("StaticSubclassMissing");
    echo ":bad";
} catch (ReflectionException $e) {
    echo ":missing";
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "PBIsJxtqE:missing");
}

/// Verifies that `ReflectionClass::isInstance()` reports class, interface,
/// trait, and enum instance relations using runtime object metadata.
#[test]
fn test_reflection_class_is_instance() {
    let out = compile_and_run_capture(
        r#"<?php
interface StaticInstanceIface {}
class StaticInstanceBase {}
class StaticInstanceChild extends StaticInstanceBase implements StaticInstanceIface {}
trait StaticInstanceTrait {}
enum StaticInstanceEnum implements StaticInstanceIface { case Ready; }
$base = new ReflectionClass(StaticInstanceBase::class);
$child = new ReflectionClass(StaticInstanceChild::class);
$iface = new ReflectionClass(StaticInstanceIface::class);
$trait = new ReflectionClass(StaticInstanceTrait::class);
$enum = new ReflectionClass(StaticInstanceEnum::class);
$childObj = new StaticInstanceChild();
echo $base->isInstance($childObj) ? "B" : "b";
echo $child->isInstance(new StaticInstanceBase()) ? "C" : "c";
echo $iface->isInstance($childObj) ? "I" : "i";
echo $trait->isInstance($childObj) ? "T" : "t";
echo $enum->isInstance(StaticInstanceEnum::Ready) ? "E" : "e";
echo $iface->isInstance(StaticInstanceEnum::Ready) ? "N" : "n";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "BcItEN");
}

/// Verifies that `ReflectionClass::getParentClass()` returns a ReflectionClass
/// object for subclasses and `false` for parentless classes.
#[test]
fn test_reflection_class_get_parent_class() {
    let out = compile_and_run(
        r#"<?php
class StaticParentBase {}
class StaticParentChild extends StaticParentBase {}
$parent = (new ReflectionClass(StaticParentChild::class))->getParentClass();
echo $parent instanceof ReflectionClass ? $parent->getName() : "missing";
echo ":";
$root = (new ReflectionClass(StaticParentBase::class))->getParentClass();
echo $root === false ? "false" : "bad";
"#,
    );
    assert_eq!(out, "StaticParentBase:false");
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
abstract class ReflectAbstractProperty {
    abstract public int $mustRead { get; }
}
readonly class ReflectReadonlyClass {
    public int $classReadonly;
}
class ReflectMemberChild extends ReflectMemberBase {
    public function mustImplement() {}
    private static string $token = "x";
    final public static string $staticSeal = "x";
    protected int $visible = 2;
    public readonly int $locked;
    final public int $sealed;
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
echo $staticProp->isFinal() ? "F" : "f";
echo $staticProp->isAbstract() ? "A" : "a";
echo $staticProp->isReadOnly() ? "R" : "r";
echo $staticProp->getModifiers();
echo ":";
$visibleProp = new ReflectionProperty(ReflectMemberChild::class, "visible");
echo $visibleProp->isStatic() ? "S" : "s";
echo $visibleProp->isProtected() ? "P" : "p";
echo $visibleProp->isPublic() ? "U" : "u";
echo $visibleProp->isFinal() ? "F" : "f";
echo $visibleProp->isAbstract() ? "A" : "a";
echo $visibleProp->isReadOnly() ? "R" : "r";
echo $visibleProp->getModifiers();
echo ":";
$readonlyProp = new ReflectionProperty(ReflectMemberChild::class, "locked");
echo $readonlyProp->isReadOnly() ? "R" : "r";
echo $readonlyProp->isPublic() ? "U" : "u";
echo $readonlyProp->getModifiers();
echo ":";
$sealedProp = new ReflectionProperty(ReflectMemberChild::class, "sealed");
echo $sealedProp->isFinal() ? "F" : "f";
echo $sealedProp->isPublic() ? "U" : "u";
echo $sealedProp->getModifiers();
echo ":";
$staticFinalProp = new ReflectionProperty(ReflectMemberChild::class, "staticSeal");
echo $staticFinalProp->isFinal() ? "F" : "f";
echo $staticFinalProp->isStatic() ? "S" : "s";
echo $staticFinalProp->getModifiers();
echo ":";
$abstractProp = new ReflectionProperty(ReflectAbstractProperty::class, "mustRead");
echo $abstractProp->isAbstract() ? "A" : "a";
echo $abstractProp->isFinal() ? "F" : "f";
echo $abstractProp->getModifiers();
echo ":";
$classReadonlyProp = new ReflectionProperty(ReflectReadonlyClass::class, "classReadonly");
echo $classReadonlyProp->isReadOnly() ? "C" : "c";
echo $classReadonlyProp->getModifiers();
"#,
    );
    assert_eq!(
        out,
        "SPurfa:APs:FUs:SRpfar20:sPufar2:RU2177:FU33:FS49:Af577:C2177"
    );
}

/// Verifies that `ReflectionProperty::isPromoted()` reports constructor property
/// promotion metadata for direct, inherited, and listed reflected properties.
#[test]
fn test_reflection_property_is_promoted() {
    let out = compile_and_run(
        r#"<?php
class ReflectPromotedBase {
    public function __construct(public int $id, protected string $name = "Ada") {}
}
class ReflectPromotedChild extends ReflectPromotedBase {}
class ReflectPromotedPlain {
    public int $id = 0;
    public static int $count = 0;
}
$id = new ReflectionProperty(ReflectPromotedBase::class, "id");
echo $id->isPromoted() ? "I" : "i";
$name = new ReflectionProperty(ReflectPromotedBase::class, "name");
echo $name->isPromoted() ? "N" : "n";
$child = new ReflectionProperty(ReflectPromotedChild::class, "id");
echo $child->isPromoted() ? "C" : "c";
$plain = new ReflectionProperty(ReflectPromotedPlain::class, "id");
echo $plain->isPromoted() ? "P" : "p";
$static = new ReflectionProperty(ReflectPromotedPlain::class, "count");
echo $static->isPromoted() ? "S" : "s";
echo ":";
foreach ((new ReflectionClass(ReflectPromotedBase::class))->getProperties() as $property) {
    if ($property->getName() === "id") {
        echo $property->isPromoted() ? "L" : "l";
    }
    if ($property->getName() === "name") {
        echo $property->isPromoted() ? "M" : "m";
    }
}
"#,
    );
    assert_eq!(out, "INCps:LM");
}

/// Verifies that `ReflectionMethod::isConstructor()` and `isDestructor()` derive
/// their result from the reflected method name.
#[test]
fn test_reflection_method_reports_constructor_and_destructor() {
    let out = compile_and_run(
        r#"<?php
class ReflectLifecycle {
    public function __construct() {}
    public function __destruct() {}
    public function run() {}
}
$ctor = new ReflectionMethod(ReflectLifecycle::class, "__CONSTRUCT");
echo $ctor->isConstructor() ? "C" : "c";
echo $ctor->isDestructor() ? "D" : "d";
echo ":";
$dtor = new ReflectionMethod(ReflectLifecycle::class, "__destruct");
echo $dtor->isConstructor() ? "C" : "c";
echo $dtor->isDestructor() ? "D" : "d";
echo ":";
$run = new ReflectionMethod(ReflectLifecycle::class, "run");
echo $run->isConstructor() ? "C" : "c";
echo $run->isDestructor() ? "D" : "d";
echo ":";
$listed = (new ReflectionClass(ReflectLifecycle::class))->getConstructor();
echo $listed->isConstructor() ? "C" : "c";
echo $listed->isDestructor() ? "D" : "d";
"#,
    );
    assert_eq!(out, "Cd:cD:cd:Cd");
}

/// Verifies member and enum-case reflectors expose their declaring class object.
#[test]
fn test_reflection_members_report_declaring_class() {
    let out = compile_and_run(
        r#"<?php
class ReflectDeclaringBase {
    public int $baseProp = 1;
    public function inherited(): string { return "base"; }
    public const BASE_CONST = 10;
}
class ReflectDeclaringChild extends ReflectDeclaringBase {
    public int $childProp = 2;
    public function own(): string { return "child"; }
    public const CHILD_CONST = 20;
}
enum ReflectDeclaringEnum: string {
    case Ready = "ready";
    public const LEVEL = 3;
}
echo (new ReflectionMethod(ReflectDeclaringChild::class, "inherited"))->getDeclaringClass()->getName() . ":";
echo (new ReflectionClass(ReflectDeclaringChild::class))->getMethod("own")->getDeclaringClass()->getName() . ":";
echo (new ReflectionProperty(ReflectDeclaringChild::class, "baseProp"))->getDeclaringClass()->getName() . ":";
echo (new ReflectionClass(ReflectDeclaringChild::class))->getProperty("childProp")->getDeclaringClass()->getName() . ":";
echo (new ReflectionClass(ReflectDeclaringChild::class))->getReflectionConstant("BASE_CONST")->getDeclaringClass()->getName() . ":";
echo (new ReflectionClassConstant(ReflectDeclaringChild::class, "BASE_CONST"))->getDeclaringClass()->getName() . ":";
echo (new ReflectionClass(ReflectDeclaringEnum::class))->getReflectionConstant("Ready")->getDeclaringClass()->getName() . ":";
echo (new ReflectionEnumBackedCase(ReflectDeclaringEnum::class, "Ready"))->getDeclaringClass()->getName();
"#,
    );
    assert_eq!(
        out,
        "ReflectDeclaringBase:ReflectDeclaringChild:ReflectDeclaringBase:ReflectDeclaringChild:ReflectDeclaringBase:ReflectDeclaringBase:ReflectDeclaringEnum:ReflectDeclaringEnum"
    );
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
echo ReflectionMethod::IS_STATIC . ":" . ReflectionMethod::IS_PRIVATE . ":";
$direct = new ReflectionMethod(ReflectListTarget::class, "helper");
echo "D" . $direct->getModifiers() . ":";
foreach ($methods as $method) {
    if ($method->getName() === "first") {
        echo "F" . count($method->getAttributes());
        echo "M" . $method->getModifiers();
    }
    if ($method->getName() === "helper") {
        echo $method->isStatic() ? "S" : "s";
        echo $method->isPrivate() ? "R" : "r";
        echo "M" . $method->getModifiers();
    }
}
echo ":";
foreach ($properties as $property) {
    if ($property->getName() === "visible") {
        echo "V" . count($property->getAttributes());
        echo $property->isProtected() ? "P" : "p";
        echo "M" . $property->getModifiers();
    }
    if ($property->getName() === "token") {
        echo $property->isStatic() ? "T" : "t";
        echo $property->isPrivate() ? "R" : "r";
        echo "M" . $property->getModifiers();
    }
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "2:2:16:4:D20:F1M1SRM20:V1PM2TRM20");
}

/// Verifies that `ReflectionClass::getMethod()` and `getProperty()` return
/// single member objects and throw ReflectionException for missing members.
#[test]
fn test_reflection_class_get_method_and_property_lookup_members() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectLookupTarget {
    public function first() {}
    private static function helper() {}
    protected int $visible = 1;
    private static string $token = "x";
}
$ref = new ReflectionClass(ReflectLookupTarget::class);
$method = $ref->getMethod("FIRST");
echo $method->getName() . ":";
echo $method->isPublic() ? "U" : "u";
echo ":";
$helper = $ref->getMethod("helper");
echo $helper->isPrivate() ? "P" : "p";
echo $helper->isStatic() ? "S" : "s";
echo ":";
$property = $ref->getProperty("visible");
echo $property->getName() . ":";
echo $property->isProtected() ? "R" : "r";
echo ":";
try {
    $ref->getProperty("Visible");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
echo ":";
try {
    $ref->getMethod("missing");
    echo "bad";
} catch (ReflectionException $e) {
    echo $e->getMessage();
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "first:U:PS:visible:R:Property ReflectLookupTarget::$Visible does not exist:Method ReflectLookupTarget::missing() does not exist"
    );
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
    echo ($param->canBePassedByValue() ? "P" : "p");
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
        "4/2:id#0TRbPv|name#1tRBpv|mode#2TObPv|rest#3tObPV|\n2/1:idT:nameO\n3/1:SR:restVT"
    );
}

/// Verifies `ReflectionParameter::getDeclaringClass()` reports method owners and null for functions.
#[test]
fn test_reflection_parameter_get_declaring_class_reports_method_owner() {
    let out = compile_and_run_capture(
        r#"<?php
function reflect_declaring_function($value) {}
class ReflectDeclaringParamBase {
    public function inherited(int $base) {}
}
class ReflectDeclaringParamChild extends ReflectDeclaringParamBase {
    public function own(string $child) {}
}
$inherited = new ReflectionParameter([ReflectDeclaringParamChild::class, "inherited"], "base");
echo $inherited->getDeclaringClass()->getName() . ":";
echo $inherited->getDeclaringFunction()->getName() . ":";
echo $inherited->getDeclaringFunction()->getDeclaringClass()->getName() . ":";
$listed = (new ReflectionMethod(ReflectDeclaringParamChild::class, "own"))->getParameters()[0];
echo $listed->getDeclaringClass()->getName() . ":";
echo $listed->getDeclaringFunction()->getName() . ":";
$function = new ReflectionParameter("reflect_declaring_function", "value");
echo $function->getDeclaringFunction()->getName() . ":";
echo is_null($function->getDeclaringClass()) ? "null" : "bad";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "ReflectDeclaringParamBase:inherited:ReflectDeclaringParamBase:ReflectDeclaringParamChild:own:reflect_declaring_function:null"
    );
}

/// Verifies that `ReflectionParameter::getType()` returns named, union, and intersection metadata.
#[test]
fn test_reflection_parameter_get_type_returns_named_type_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectParamTypeDep {}
interface ReflectParamTypeA {}
interface ReflectParamTypeB {}
class ReflectParamTypeTarget {
    public function run(int $id, ?string $name, ReflectParamTypeDep $dep, $plain, int|string $union, int|string|null $nullableUnion, ReflectParamTypeA&ReflectParamTypeB $intersection) {}
}
$params = (new ReflectionMethod(ReflectParamTypeTarget::class, "run"))->getParameters();
foreach ($params as $param) {
    echo $param->getName() . ":";
    echo $param->hasType() ? "T:" : "t:";
    echo $param->allowsNull() ? "N:" : "n:";
    $type = $param->getType();
    if ($type instanceof ReflectionNamedType) {
        echo $type->getName();
        echo $type->allowsNull() ? "?" : "!";
        echo $type->isBuiltin() ? "B" : "C";
    } elseif ($type instanceof ReflectionUnionType) {
        echo "union";
        echo $type->allowsNull() ? "?" : "!";
        foreach ($type->getTypes() as $memberType) {
            echo ":" . $memberType->getName();
            echo $memberType->isBuiltin() ? "B" : "C";
        }
    } elseif ($type instanceof ReflectionIntersectionType) {
        echo "intersection";
        echo $type->allowsNull() ? "?" : "!";
        foreach ($type->getTypes() as $memberType) {
            echo ":" . $memberType->getName();
            echo $memberType->isBuiltin() ? "B" : "C";
        }
    } else {
        echo "null";
    }
    echo "|";
}
$direct = new ReflectionParameter([ReflectParamTypeTarget::class, "run"], "dep");
$directType = $direct->getType();
if ($directType instanceof ReflectionNamedType) {
    echo "direct:" . $directType->getName();
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "id:T:n:int!B|name:T:N:string?B|dep:T:n:ReflectParamTypeDep!C|plain:t:N:null|union:T:n:union!:intB:stringB|nullableUnion:T:N:union?:intB:stringB|intersection:T:n:intersection!:ReflectParamTypeAC:ReflectParamTypeBC|direct:ReflectParamTypeDep"
    );
}

/// Verifies `ReflectionType::__toString()` formats retained type metadata.
#[test]
fn test_reflection_type_to_string_formats_retained_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectTypeStringDep {}
interface ReflectTypeStringLeft {}
interface ReflectTypeStringRight {}
class ReflectTypeStringTarget {
    public function run(?ReflectTypeStringDep $dep, int|string|null $union, ReflectTypeStringLeft&ReflectTypeStringRight $both, mixed $mixed, ?array $items) {}
}
$params = (new ReflectionMethod(ReflectTypeStringTarget::class, "run"))->getParameters();
foreach ($params as $param) {
    $type = $param->getType();
    echo $param->getName() . ":";
    echo $type->__toString() . "|";
}
$unionType = (new ReflectionParameter([ReflectTypeStringTarget::class, "run"], "union"))->getType();
echo "cast:" . (string)$unionType . "|";
echo "concat:" . $unionType . "|";
echo "echo:";
echo $unionType;
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "dep:?ReflectTypeStringDep|union:int|string|null|both:ReflectTypeStringLeft&ReflectTypeStringRight|mixed:mixed|items:?array|cast:int|string|null|concat:int|string|null|echo:int|string|null"
    );
}

/// Verifies that `ReflectionParameter::getClass()` exposes legacy object-type metadata.
#[test]
fn test_reflection_parameter_get_class_returns_named_object_type() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectParamClassDep {}
interface ReflectParamClassA {}
interface ReflectParamClassB {}
class ReflectParamClassTarget {
    public function run(ReflectParamClassDep $dep, ?ReflectParamClassDep $nullable, int $id, ReflectParamClassDep|int $unionObject, ReflectParamClassA&ReflectParamClassB $intersection, $plain) {}
}
function reflect_param_class_function(ReflectParamClassDep $dep) {}
$params = (new ReflectionMethod(ReflectParamClassTarget::class, "run"))->getParameters();
foreach ($params as $param) {
    $class = $param->getClass();
    echo $param->getName() . ":" . ($class ? $class->getName() : "null") . "|";
}
$direct = new ReflectionParameter([ReflectParamClassTarget::class, "run"], "nullable");
echo "direct:" . $direct->getClass()->getName() . "|";
$functionParam = new ReflectionParameter("reflect_param_class_function", "dep");
echo "function:" . $functionParam->getClass()->getName();
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "dep:ReflectParamClassDep|nullable:ReflectParamClassDep|id:null|unionObject:null|intersection:null|plain:null|direct:ReflectParamClassDep|function:ReflectParamClassDep"
    );
}

/// Verifies that `ReflectionProperty::getType()` and `getSettableType()` return type metadata.
#[test]
fn test_reflection_property_get_type_returns_type_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectPropertyTypeDep {}
class ReflectPropertyTypeTarget {
    public int $id;
    public ?string $name;
    public ReflectPropertyTypeDep $dep;
    public $plain;
    public int|string $union;
}
$properties = (new ReflectionClass(ReflectPropertyTypeTarget::class))->getProperties();
foreach ($properties as $property) {
    echo $property->getName() . ":";
    echo $property->hasType() ? "T:" : "t:";
    $type = $property->getType();
    if ($type instanceof ReflectionNamedType) {
        echo $type->getName();
        echo $type->allowsNull() ? "?" : "!";
        echo $type->isBuiltin() ? "B" : "C";
    } elseif ($type instanceof ReflectionUnionType) {
        echo "union";
        echo $type->allowsNull() ? "?" : "!";
        foreach ($type->getTypes() as $memberType) {
            echo ":" . $memberType->getName();
            echo $memberType->isBuiltin() ? "B" : "C";
        }
    } else {
        echo "null";
    }
    echo "|";
}
$direct = new ReflectionProperty(ReflectPropertyTypeTarget::class, "dep");
$directType = $direct->getType();
if ($directType instanceof ReflectionNamedType) {
    echo "direct:" . $directType->getName();
}
$directSettableType = $direct->getSettableType();
if ($directSettableType instanceof ReflectionNamedType) {
    echo ":set:" . $directSettableType->getName();
}
$plain = new ReflectionProperty(ReflectPropertyTypeTarget::class, "plain");
echo ":plainSet:" . ($plain->getSettableType() === null ? "N" : "n");
$directUnion = new ReflectionProperty(ReflectPropertyTypeTarget::class, "union");
$directUnionSettableType = $directUnion->getSettableType();
if ($directUnionSettableType instanceof ReflectionUnionType) {
    echo ":unionSet:" . count($directUnionSettableType->getTypes());
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "id:T:int!B|name:T:string?B|dep:T:ReflectPropertyTypeDep!C|plain:t:null|union:T:union!:intB:stringB|direct:ReflectPropertyTypeDep:set:ReflectPropertyTypeDep:plainSet:N:unionSet:2"
    );
}

/// Verifies that `ReflectionProperty` exposes supported property defaults.
#[test]
fn test_reflection_property_get_default_value_returns_property_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectPropertyDefaultTarget {
    public $implicit;
    public int $typed;
    public ?string $nullableTyped;
    public $explicitNull = null;
    public int $count = 7;
    public static string $label = "ok";
    public array $items = [2, "b", null];
	    public $assoc = ["name" => "Ada", "1" => "one", false => "zero"];
	}
	$obj = new ReflectPropertyDefaultTarget();
	echo "runtime:" . $obj->assoc["name"] . ":" . $obj->assoc[1] . "|";
	$implicit = new ReflectionProperty(ReflectPropertyDefaultTarget::class, "implicit");
	echo $implicit->getName() . ":";
echo $implicit->isDefault() ? "Y:" : "N:";
echo $implicit->hasDefaultValue() ? "D:" : "d:";
echo $implicit->getDefaultValue() === null ? "null" : $implicit->getDefaultValue();
echo "|";
$typed = new ReflectionProperty(ReflectPropertyDefaultTarget::class, "typed");
echo $typed->getName() . ":";
echo $typed->isDefault() ? "Y:" : "N:";
echo $typed->hasDefaultValue() ? "D:" : "d:";
echo $typed->getDefaultValue() === null ? "null" : $typed->getDefaultValue();
echo "|";
$nullableTyped = new ReflectionProperty(ReflectPropertyDefaultTarget::class, "nullableTyped");
echo $nullableTyped->getName() . ":";
echo $nullableTyped->isDefault() ? "Y:" : "N:";
echo $nullableTyped->hasDefaultValue() ? "D:" : "d:";
echo $nullableTyped->getDefaultValue() === null ? "null" : $nullableTyped->getDefaultValue();
echo "|";
$explicitNull = new ReflectionProperty(ReflectPropertyDefaultTarget::class, "explicitNull");
echo $explicitNull->getName() . ":";
echo $explicitNull->isDefault() ? "Y:" : "N:";
echo $explicitNull->hasDefaultValue() ? "D:" : "d:";
echo $explicitNull->getDefaultValue() === null ? "null" : $explicitNull->getDefaultValue();
echo "|";
$count = new ReflectionProperty(ReflectPropertyDefaultTarget::class, "count");
echo $count->getName() . ":";
echo $count->isDefault() ? "Y:" : "N:";
echo $count->hasDefaultValue() ? "D:" : "d:";
echo $count->getDefaultValue() === null ? "null" : $count->getDefaultValue();
echo "|";
$label = new ReflectionProperty(ReflectPropertyDefaultTarget::class, "label");
echo $label->getName() . ":";
echo $label->isDefault() ? "Y:" : "N:";
echo $label->hasDefaultValue() ? "D:" : "d:";
echo $label->getDefaultValue() === null ? "null" : $label->getDefaultValue();
echo "|";
$items = new ReflectionProperty(ReflectPropertyDefaultTarget::class, "items");
$itemDefault = $items->getDefaultValue();
echo $items->getName() . ":";
echo $items->isDefault() ? "Y:" : "N:";
echo $items->hasDefaultValue() ? "D:" : "d:";
echo count($itemDefault) . ":" . $itemDefault[0] . ":" . $itemDefault[1] . ":";
echo $itemDefault[2] === null ? "null" : "value";
echo "|";
$assoc = new ReflectionProperty(ReflectPropertyDefaultTarget::class, "assoc");
$assocDefault = $assoc->getDefaultValue();
echo $assoc->getName() . ":";
echo $assoc->isDefault() ? "Y:" : "N:";
echo $assoc->hasDefaultValue() ? "D:" : "d:";
echo count($assocDefault) . ":" . $assocDefault["name"] . ":" . $assocDefault[1] . ":";
echo $assocDefault[0];
echo "|";
$listed = (new ReflectionClass(ReflectPropertyDefaultTarget::class))->getProperty("implicit");
echo "listed:";
echo $listed->isDefault() ? "Y:" : "N:";
echo $listed->hasDefaultValue() ? "D:" : "d:";
echo $listed->getDefaultValue() === null ? "null" : "bad";
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
	        out.stdout,
	        "runtime:Ada:one|implicit:Y:D:null|typed:Y:d:null|nullableTyped:Y:d:null|explicitNull:Y:D:null|count:Y:D:7|label:Y:D:ok|items:Y:D:3:2:b:null|assoc:Y:D:3:Ada:one:zero|listed:Y:D:null"
	    );
}

/// Verifies that `ReflectionClass::getDefaultProperties()` exposes supported property defaults.
#[test]
fn test_reflection_class_get_default_properties_returns_property_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectClassDefaultBase {
    public int $base = 1;
    protected string $prot = "p";
    private int $shadow = 3;
    public $implicit;
    public int $typed;
    public static string $baseStatic = "bs";
}
class ReflectClassDefaultChild extends ReflectClassDefaultBase {
	    public int $child = 5;
	    private int $shadow = 9;
	    public static int $childStatic = 7;
	    public static $assocStatic = ["s" => "S2", "2" => "two"];
	    public $explicitNull = null;
	    public array $items = [8, "i"];
	    public $assoc = ["side" => "S", "4" => "four"];
}
$defaults = (new ReflectionClass(ReflectClassDefaultChild::class))->getDefaultProperties();
echo $defaults["childStatic"] . ":";
echo $defaults["baseStatic"] . ":";
echo $defaults["child"] . ":";
echo $defaults["shadow"] . ":";
	echo $defaults["base"] . ":";
	echo $defaults["prot"] . ":";
	echo ReflectClassDefaultChild::$assocStatic["s"] . ":";
	echo count($defaults["items"]) . ":";
echo $defaults["items"][0] . ":";
echo $defaults["items"][1] . ":";
echo count($defaults["assoc"]) . ":";
echo $defaults["assoc"]["side"] . ":";
echo $defaults["assoc"][4] . ":";
$implicit = "i";
$explicitNull = "e";
$typed = "t";
foreach ($defaults as $key => $value) {
    if ($key === "implicit" && $value === null) {
        $implicit = "I";
    }
    if ($key === "explicitNull" && $value === null) {
        $explicitNull = "E";
    }
    if ($key === "typed") {
        $typed = "T";
    }
}
echo $implicit . ":" . $explicitNull . ":" . $typed . ":" . count($defaults);
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "7:bs:5:9:1:p:S2:2:8:i:2:S:four:I:E:t:11");
}

/// Verifies `ReflectionClass::getStaticPropertyValue()` and
/// `setStaticPropertyValue()` use live static-property storage for known classes.
#[test]
fn test_reflection_class_static_property_value_accesses_live_storage() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectStaticValueTarget {
    public static int $count = 7;
}
echo (new ReflectionClass(ReflectStaticValueTarget::class))->getStaticPropertyValue("count");
ReflectStaticValueTarget::$count = 9;
echo ":" . (new ReflectionClass(ReflectStaticValueTarget::class))->getStaticPropertyValue("count");
(new ReflectionClass(ReflectStaticValueTarget::class))->setStaticPropertyValue("count", 11);
echo ":" . ReflectStaticValueTarget::$count;
echo ":" . (new ReflectionClass(ReflectStaticValueTarget::class))->getStaticPropertyValue("missing", "fallback");
try {
    (new ReflectionClass(ReflectStaticValueTarget::class))->getStaticPropertyValue("missing");
    echo ":bad";
} catch (ReflectionException $e) {
    echo ":" . get_class($e) . ":" . $e->getMessage();
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "7:9:11:fallback:ReflectionException:Property ReflectStaticValueTarget::$missing does not exist"
    );
}

/// Verifies a local `ReflectionClass` receiver keeps statically-known class metadata.
#[test]
fn test_reflection_class_tracked_local_receiver_uses_static_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectTrackedClassTarget {
    public static int $count = 7;

    public function __construct(public int $id = 1, public string $name = "unset") {}
}

$ref = new ReflectionClass(ReflectTrackedClassTarget::class);
echo $ref->getStaticPropertyValue("count");
ReflectTrackedClassTarget::$count = 9;
echo ":" . $ref->getStaticPropertyValue("count");
$ref->setStaticPropertyValue("count", 11);
echo ":" . ReflectTrackedClassTarget::$count;
echo ":" . $ref->getStaticPropertyValue("missing", "fallback");
$obj = $ref->newInstance(id: 5, name: "Ada");
echo ":" . $obj->id . ":" . $obj->name;
$obj2 = $ref->newInstanceArgs(["name" => "Bob", "id" => 6]);
echo ":" . $obj2->id . ":" . $obj2->name;
try {
    $ref->getStaticPropertyValue("missing");
    echo ":bad";
} catch (ReflectionException $e) {
    echo ":" . get_class($e) . ":" . $e->getMessage();
}
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "7:9:11:fallback:5:Ada:6:Bob:ReflectionException:Property ReflectTrackedClassTarget::$missing does not exist"
    );
}

/// Verifies `ReflectionClass::getStaticProperties()` reads current AOT static values.
#[test]
fn test_reflection_class_get_static_properties_reads_live_storage() {
    let out = compile_and_run_capture(
        r#"<?php
class ReflectStaticPropertiesBase {
    public static string $base = "base";
}

class ReflectStaticPropertiesChild extends ReflectStaticPropertiesBase {
    public static int $count = 1;
    public static string $label = "old";
}

$ref = new ReflectionClass(ReflectStaticPropertiesChild::class);
ReflectStaticPropertiesBase::$base = "B2";
ReflectStaticPropertiesChild::$count = 4;
ReflectStaticPropertiesChild::$label = "new";
$props = $ref->getStaticProperties();
echo $props["base"] . ":" . $props["count"] . ":" . $props["label"];
$inline = (new ReflectionClass(ReflectStaticPropertiesChild::class))->getStaticProperties();
echo ":" . $inline["count"];
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "B2:4:new:4");
}

/// Verifies `ReflectionParameter::getAttributes()` exposes parameter attributes.
#[test]
fn test_reflection_parameter_get_attributes_returns_parameter_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
#[Attribute]
class ReflectParamTag {
    public function __construct(public string $name = "") {}
}
class ReflectParamAttrTarget {
    public function run(#[ReflectParamTag("id")] int $id, #[ReflectParamTag("name")] string $name, $plain) {}
}
$params = (new ReflectionMethod(ReflectParamAttrTarget::class, "run"))->getParameters();
foreach ($params as $param) {
    $attrs = $param->getAttributes();
    echo $param->getName() . ":" . count($attrs);
    if (count($attrs) > 0) {
        echo ":" . $attrs[0]->getName();
        echo ":" . $attrs[0]->getArguments()[0];
    }
    echo "|";
}
$direct = new ReflectionParameter([ReflectParamAttrTarget::class, "run"], "name");
$directAttrs = $direct->getAttributes();
echo "direct:" . count($directAttrs) . ":" . $directAttrs[0]->getName() . ":" . $directAttrs[0]->getArguments()[0];
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "id:1:ReflectParamTag:id|name:1:ReflectParamTag:name|plain:0|direct:1:ReflectParamTag:name"
    );
}

/// Verifies `ReflectionFunction` and direct `ReflectionParameter` expose attributes on
/// top-level function parameters.
#[test]
fn test_reflection_function_parameter_get_attributes_returns_parameter_metadata() {
    let out = compile_and_run_capture(
        r#"<?php
#[Attribute]
class ReflectFunctionParamTag {
    public function __construct(public string $name = "") {}
}
function reflect_function_param_attrs(
    #[ReflectFunctionParamTag("id")] int $id,
    #[ReflectFunctionParamTag("name")] string $name,
    $plain
) {}
$params = (new ReflectionFunction("reflect_function_param_attrs"))->getParameters();
foreach ($params as $param) {
    $attrs = $param->getAttributes();
    echo $param->getName() . ":" . count($attrs);
    if (count($attrs) > 0) {
        echo ":" . $attrs[0]->getName();
        echo ":" . $attrs[0]->getArguments()[0];
    }
    echo "|";
}
$direct = new ReflectionParameter("reflect_function_param_attrs", "name");
$directAttrs = $direct->getAttributes();
echo "direct:" . count($directAttrs) . ":" . $directAttrs[0]->getName() . ":" . $directAttrs[0]->getArguments()[0];
"#,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "id:1:ReflectFunctionParamTag:id|name:1:ReflectFunctionParamTag:name|plain:0|direct:1:ReflectFunctionParamTag:name"
    );
}

/// Verifies that `ReflectionParameter` exposes supported scalar/null/array defaults.
#[test]
fn test_reflection_parameter_exposes_default_values() {
    let out = compile_and_run_capture(
        r##"<?php
function reflect_default_function($required, int $id = 7, ?string $name = null, string $label = "ok", array $items = [1, "two", null, [3, false]], array $assoc = ["name" => "Ada", "1" => "one", false => "zero", 3 => ["deep" => 4]]) {}
$params = (new ReflectionFunction("reflect_default_function"))->getParameters();
echo $params[0]->isDefaultValueAvailable() ? "D" : "d";
try {
    $params[0]->getDefaultValue();
} catch (ReflectionException $e) {
    echo ":E";
}
echo "|";
echo $params[1]->isDefaultValueAvailable() ? "D:" : "d:";
echo $params[1]->getDefaultValue();
echo "|";
echo $params[2]->isDefaultValueAvailable() ? "D:" : "d:";
echo $params[2]->getDefaultValue() === null ? "null" : "value";
echo "|";
$direct = new ReflectionParameter("reflect_default_function", "label");
echo $direct->isDefaultValueAvailable() ? "D:" : "d:";
echo $direct->getDefaultValue();
echo "|";
$items = $params[4]->getDefaultValue();
echo $params[4]->isDefaultValueAvailable() ? "D:" : "d:";
echo count($items) . ":" . $items[0] . ":" . $items[1] . ":";
echo $items[2] === null ? "null" : "value";
echo ":" . count($items[3]) . ":" . $items[3][0] . ":" . ($items[3][1] ? "T" : "F");
echo "|";
$directItems = (new ReflectionParameter("reflect_default_function", "items"))->getDefaultValue();
echo count($directItems) . ":" . $directItems[1];
echo "|";
$assoc = $params[5]->getDefaultValue();
echo $params[5]->isDefaultValueAvailable() ? "D:" : "d:";
echo count($assoc) . ":" . $assoc["name"] . ":" . $assoc[1] . ":";
echo $assoc[0] . ":" . $assoc[3]["deep"];
"##,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "d:E|D:7|D:null|D:ok|D:4:1:two:null:2:3:F|4:two|D:4:Ada:one:zero:4"
    );
}

/// Verifies `ReflectionParameter::getDefaultValue()` materializes object defaults lazily.
#[test]
fn test_reflection_parameter_exposes_object_default_values() {
    let out = compile_and_run_capture(
        r##"<?php
class ReflectObjectDefaultValue {
    const LABEL = "ctor";
    const EXTRA = "default-extra";
    public mixed $label;
    public mixed $extra;
    public function __construct(mixed $label = self::LABEL, mixed $extra = self::EXTRA) {
        $this->label = $label;
        $this->extra = $extra;
    }
}
class ReflectObjectDefaultValueArgs {
    const ARG_LABEL = "arg";
    const ARG_EXTRA = 42;
    const METHOD_LABEL = "method-arg";
    const METHOD_EXTRA = "M";
    public mixed $label;
    public mixed $extra;
    public function __construct(mixed $label, mixed $extra) {
        $this->label = $label;
        $this->extra = $extra;
    }
}
function reflect_object_default(ReflectObjectDefaultValue $value = new ReflectObjectDefaultValue()) {}
function reflect_object_default_args(ReflectObjectDefaultValueArgs $value = new ReflectObjectDefaultValueArgs(ReflectObjectDefaultValueArgs::ARG_LABEL, ReflectObjectDefaultValueArgs::ARG_EXTRA)) {}
class ReflectObjectDefaultMethod {
    public function run(ReflectObjectDefaultValue $value = new ReflectObjectDefaultValue()) {}
    public function withArgs(ReflectObjectDefaultValueArgs $value = new ReflectObjectDefaultValueArgs(ReflectObjectDefaultValueArgs::METHOD_LABEL, ReflectObjectDefaultValueArgs::METHOD_EXTRA)) {}
}
$param = (new ReflectionFunction("reflect_object_default"))->getParameters()[0];
echo $param->isDefaultValueAvailable() ? "D:" : "d:";
$first = $param->getDefaultValue();
$second = $param->getDefaultValue();
if ($first instanceof ReflectObjectDefaultValue) {
    echo "object:" . $first->label . ":" . $first->extra . ":";
} else {
    echo "not-object:";
}
echo $first === $second ? "same" : "diff";
$direct = (new ReflectionParameter("reflect_object_default", "value"))->getDefaultValue();
echo $direct instanceof ReflectObjectDefaultValue ? ":direct:" . $direct->label : ":direct:bad";
$method = (new ReflectionMethod(ReflectObjectDefaultMethod::class, "run"))->getParameters()[0]->getDefaultValue();
echo $method instanceof ReflectObjectDefaultValue ? ":method:" . $method->label : ":method:bad";
$directMethod = (new ReflectionParameter([ReflectObjectDefaultMethod::class, "run"], "value"))->getDefaultValue();
echo $directMethod instanceof ReflectObjectDefaultValue ? ":direct-method:" . $directMethod->label : ":direct-method:bad";
$args = (new ReflectionFunction("reflect_object_default_args"))->getParameters()[0]->getDefaultValue();
echo $args instanceof ReflectObjectDefaultValueArgs ? ":args:" . $args->label . ":" . $args->extra : ":args:bad";
$methodArgs = (new ReflectionMethod(ReflectObjectDefaultMethod::class, "withArgs"))->getParameters()[0]->getDefaultValue();
echo $methodArgs instanceof ReflectObjectDefaultValueArgs ? ":method-args:" . $methodArgs->label . ":" . $methodArgs->extra : ":method-args:bad";
$directMethodArgs = (new ReflectionParameter([ReflectObjectDefaultMethod::class, "withArgs"], "value"))->getDefaultValue();
echo $directMethodArgs instanceof ReflectObjectDefaultValueArgs ? ":direct-method-args:" . $directMethodArgs->label . ":" . $directMethodArgs->extra : ":direct-method-args:bad";
"##,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "D:object:ctor:default-extra:diff:direct:ctor:method:ctor:direct-method:ctor:args:arg:42:method-args:method-arg:M:direct-method-args:method-arg:M"
    );
}

/// Verifies `ReflectionParameter` exposes class-constant default metadata.
#[test]
fn test_reflection_parameter_exposes_default_constant_metadata() {
    let out = compile_and_run_capture(
        r##"<?php
class ReflectDefaultConstBase {
    const BASE = "B";
}
class ReflectDefaultConstTarget extends ReflectDefaultConstBase {
    const LABEL = "L";
    public function run($self = self::LABEL, $parent = parent::BASE, $class = self::class, $literal = 7) {}
}
$params = (new ReflectionMethod(ReflectDefaultConstTarget::class, "run"))->getParameters();
foreach ($params as $param) {
    echo $param->getName() . ":";
    echo $param->isDefaultValueAvailable() ? "D:" : "d:";
    if ($param->isDefaultValueConstant()) {
        echo "C:";
        echo $param->getDefaultValueConstantName();
        echo ":";
    } else {
        echo "c:null:";
    }
    echo $param->getDefaultValue();
    echo "|";
}
$direct = new ReflectionParameter([ReflectDefaultConstTarget::class, "run"], "parent");
echo "direct:";
echo $direct->isDefaultValueConstant() ? "C:" : "c:";
echo $direct->getDefaultValueConstantName();
echo ":";
echo $direct->getDefaultValue();
"##,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "self:D:C:self::LABEL:L|parent:D:C:parent::BASE:B|class:D:c:null:ReflectDefaultConstTarget|literal:D:c:null:7|direct:C:parent::BASE:B"
    );
}

/// Verifies direct `new ReflectionParameter()` construction for statically known
/// class and interface method targets.
#[test]
fn test_reflection_parameter_constructor_reflects_method_parameters() {
    let out = compile_and_run_capture(
        r##"<?php
class ReflectDirectParamTarget {
    public function run(int $id, &$name, string $mode = "x", ...$rest) {}
}
interface ReflectDirectParamInterface {
    public static function build(int $id, $name = "x"): void;
}
trait ReflectDirectParamTrait {
    protected static function traitRun(int $first, $second = 2, string ...$rest) {}
}
$byName = new ReflectionParameter([ReflectDirectParamTarget::class, "run"], "name");
echo $byName->getName() . "#" . $byName->getPosition();
echo ($byName->hasType() ? "T" : "t");
echo ($byName->isOptional() ? "O" : "R");
echo ($byName->isPassedByReference() ? "B" : "b");
echo ($byName->isVariadic() ? "V" : "v");
echo "|";
$byPosition = new ReflectionParameter(["reflectdirectparamtarget", "run"], 3);
echo $byPosition->getName() . "#" . $byPosition->getPosition();
echo ($byPosition->hasType() ? "T" : "t");
echo ($byPosition->isOptional() ? "O" : "R");
echo ($byPosition->isPassedByReference() ? "B" : "b");
echo ($byPosition->isVariadic() ? "V" : "v");
echo "|";
$object = new ReflectDirectParamTarget();
$byObject = new ReflectionParameter([$object, "run"], "mode");
echo $byObject->getName() . "#" . $byObject->getPosition();
echo ($byObject->hasType() ? "T" : "t");
echo ($byObject->isOptional() ? "O" : "R");
echo ($byObject->isPassedByReference() ? "B" : "b");
echo ($byObject->isVariadic() ? "V" : "v");
echo "|";
$iface = new ReflectionParameter([ReflectDirectParamInterface::class, "build"], 1);
echo $iface->getName() . "#" . $iface->getPosition();
echo ($iface->isOptional() ? "O" : "R");
echo "|";
$trait = new ReflectionParameter([ReflectDirectParamTrait::class, "traitRun"], "rest");
echo $trait->getName() . "#" . $trait->getPosition();
echo ($trait->hasType() ? "T" : "t");
echo ($trait->isOptional() ? "O" : "R");
echo ($trait->isVariadic() ? "V" : "v");
echo "|";
$named = new ReflectionParameter(param: "id", function: [ReflectDirectParamTarget::class, "run"]);
echo $named->getName() . "#" . $named->getPosition();
"##,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "name#1tRBv|rest#3tObV|mode#2TObv|name#1O|rest#2TOV|id#0"
    );
}

/// Verifies direct `new ReflectionParameter()` construction for statically known
/// user function targets.
#[test]
fn test_reflection_parameter_constructor_reflects_function_parameters() {
    let out = compile_and_run_capture(
        r##"<?php
function reflect_direct_function(int $id, &$name, string $mode = "x", string ...$rest) {}
$byName = new ReflectionParameter("reflect_direct_function", "name");
echo $byName->getName() . "#" . $byName->getPosition();
echo ($byName->hasType() ? "T" : "t");
echo ($byName->isOptional() ? "O" : "R");
echo ($byName->isPassedByReference() ? "B" : "b");
echo ($byName->isVariadic() ? "V" : "v");
echo "|";
$byPosition = new ReflectionParameter("REFLECT_DIRECT_FUNCTION", 3);
echo $byPosition->getName() . "#" . $byPosition->getPosition();
echo ($byPosition->hasType() ? "T" : "t");
echo ($byPosition->isOptional() ? "O" : "R");
echo ($byPosition->isPassedByReference() ? "B" : "b");
echo ($byPosition->isVariadic() ? "V" : "v");
echo "|";
$named = new ReflectionParameter(param: "id", function: "\\reflect_direct_function");
echo $named->getName() . "#" . $named->getPosition();
echo ($named->hasType() ? "T" : "t");
"##,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "name#1tRBv|rest#3TObV|id#0T");
}

/// Verifies `ReflectionFunction` exposes user-function name and parameter metadata.
#[test]
fn test_reflection_function_reflects_user_function_parameters() {
    let out = compile_and_run_capture(
        r##"<?php
function reflect_function_target(int $id, &$name, string $mode = "x", string ...$rest) {}
$ref = new ReflectionFunction("REFLECT_FUNCTION_TARGET");
echo $ref->getName() . "#";
echo $ref->getNumberOfParameters() . "#";
echo $ref->getNumberOfRequiredParameters() . ":";
$params = $ref->getParameters();
foreach ($params as $param) {
    echo $param->getName() . "#" . $param->getPosition();
    echo ($param->hasType() ? "T" : "t");
    echo ($param->isOptional() ? "O" : "R");
    echo ($param->isPassedByReference() ? "B" : "b");
    echo ($param->isVariadic() ? "V" : "v");
    echo "|";
}
$named = new ReflectionFunction(function: "\\reflect_function_target");
echo ":" . $named->getName();
"##,
    );
    assert!(
        out.success,
        "program failed: stdout={:?} stderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(
        out.stdout,
        "reflect_function_target#4#2:id#0TRbv|name#1tRBv|mode#2TObv|rest#3TObV|:reflect_function_target"
    );
}

/// Verifies `ReflectionFunction::getAttributes()` exposes function attributes
/// and assigns factory ids that make `ReflectionAttribute::newInstance()` work.
#[test]
fn test_reflection_function_get_attributes_returns_function_attributes() {
    let out = compile_and_run(
        r##"<?php
class FunctionMarker {
    public function __construct(public string $name, public int $rank) {}
    public function label(): string { return $this->name . "#" . $this->rank; }
}
class FlagMarker {}

#[FunctionMarker("target", 7), FlagMarker]
function reflected_function_attrs() {}

$ref = new ReflectionFunction("REFLECTED_FUNCTION_ATTRS");
$attrs = $ref->getAttributes();
echo count($attrs) . "/";
echo $attrs[0]->getName() . "/";
echo $attrs[0]->getArguments()[0] . "/";
echo $attrs[0]->getArguments()[1] . "/";
echo $attrs[0]->newInstance()->label() . "/";
echo $attrs[1]->getName() . "/";
echo count($attrs[1]->getArguments());
"##,
    );
    assert_eq!(out, "2/FunctionMarker/target/7/target#7/FlagMarker/0");
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

/// Verifies that `ReflectionClass::newInstanceArgs()` constructs reflected
/// classes from static positional and named argument arrays.
#[test]
fn test_reflection_class_new_instance_args_constructs_reflected_class() {
    let out = compile_and_run(
        r#"<?php
class ReflectNewArgsTarget {
    private string $label = "";
    public function __construct(string $left, string $right = "B") {
        $this->label = $left . $right;
    }
    public function label(): string {
        return $this->label;
    }
}
class ReflectEmptyNewArgsTarget {
    public function label(): string {
        return "empty";
    }
}
$ref = new ReflectionClass(ReflectNewArgsTarget::class);
$first = (new ReflectionClass(ReflectNewArgsTarget::class))->newInstanceArgs(["right" => "Y", "left" => "X"]);
echo $first->label() . ":";
$second = $ref->newInstanceArgs(["Q", "R"]);
echo $second->label() . ":";
$third = (new ReflectionClass(ReflectNewArgsTarget::class))->newInstanceArgs(args: ["left" => "L"]);
echo $third->label() . ":";
$fourth = (new ReflectionClass(ReflectNewArgsTarget::class))->newInstanceArgs(...[["left" => "M", "right" => "N"]]);
echo $fourth->label() . ":";
$localArgs = ["right" => "P", "left" => "O"];
$fifth = (new ReflectionClass(ReflectNewArgsTarget::class))->newInstanceArgs($localArgs);
echo $fifth->label() . ":";
$empty = (new ReflectionClass(ReflectEmptyNewArgsTarget::class))->newInstanceArgs();
echo $empty->label();
"#,
    );
    assert_eq!(out, "XY:QR:LB:MN:OP:empty");
}

/// Verifies that `ReflectionClass::newInstance()` accepts zero constructor
/// arguments for classes with no-argument or absent constructors.
#[test]
fn test_reflection_class_new_instance_allows_zero_constructor_args() {
    let out = compile_and_run(
        r#"<?php
class ReflectNoArgNewTarget {
    public string $label = "default";
    public function __construct() {
        $this->label = "ctor";
    }
    public function label(): string {
        return $this->label;
    }
}
class ReflectNoCtorNewTarget {
    public string $label = "plain";
    public function label(): string {
        return $this->label;
    }
}
$first = (new ReflectionClass(ReflectNoArgNewTarget::class))->newInstance();
echo $first->label() . ":";
$ref = new ReflectionClass(ReflectNoCtorNewTarget::class);
$second = $ref->newInstance();
echo $second->label();
"#,
    );
    assert_eq!(out, "ctor:plain");
}

/// Verifies that `ReflectionClass::newInstanceWithoutConstructor()` allocates
/// reflected classes while preserving property defaults and skipping `__construct()`.
#[test]
fn test_reflection_class_new_instance_without_constructor_skips_constructor() {
    let out = compile_and_run(
        r#"<?php
class ReflectNoCtorTarget {
    public string $label = "default";
    private string $secret = "hidden";
    public function __construct() {
        $this->label = "ctor";
    }
    public function label(): string {
        return $this->label;
    }
    public function secret(): string {
        return $this->secret;
    }
}
$ref = new ReflectionClass(ReflectNoCtorTarget::class);
$without = $ref->newInstanceWithoutConstructor();
echo $without->label() . ":" . $without->secret() . ":";
$with = new ReflectNoCtorTarget();
echo $with->label() . ":";
$inline = (new ReflectionClass("reflectnoctortarget"))->newInstanceWithoutConstructor();
echo $inline->label();
"#,
    );
    assert_eq!(out, "default:hidden:ctor:default");
}

/// Verifies inline `ReflectionClass::newInstance()` forwards named constructor
/// arguments through the reflected constructor signature.
#[test]
fn test_reflection_class_new_instance_forwards_named_constructor_args() {
    let out = compile_and_run(
        r#"<?php
class ReflectNamedNewTarget {
    public string $label = "";
    public function __construct(string $left, string $right) {
        $this->label = $left . $right;
    }
    public function label(): string {
        return $this->label;
    }
}
$first = (new ReflectionClass(ReflectNamedNewTarget::class))->newInstance(right: "B", left: "A");
echo $first->label() . ":";
$second = (new ReflectionClass(class_name: "reflectnamednewtarget"))->newInstance(...["right" => "D", "left" => "C"]);
echo $second->label();
"#,
    );
    assert_eq!(out, "AB:CD");
}

/// Verifies inline `ReflectionClass::newInstance()` uses constructor defaults
/// when named arguments leave optional parameters unspecified.
#[test]
fn test_reflection_class_new_instance_named_args_use_constructor_defaults() {
    let out = compile_and_run(
        r#"<?php
class ReflectDefaultNewTarget {
    public string $label = "";
    public function __construct(string $left = "L", string $right = "R") {
        $this->label = $left . $right;
    }
    public function label(): string {
        return $this->label;
    }
}
$value = (new ReflectionClass(ReflectDefaultNewTarget::class))->newInstance(right: "B");
echo $value->label();
"#,
    );
    assert_eq!(out, "LB");
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
    final public const ANSWER = 42;
}
enum CaseTarget: string {
    #[Marker("case")]
    case Ready = "ready";
    final public const LEVEL = 7;
}
$const = new ReflectionClassConstant(ConstTarget::class, "ANSWER");
$constAttrs = $const->getAttributes();
echo $const->getName() . "/";
echo ($const->isFinal() ? "final" : "open") . "/";
echo ($const->isEnumCase() ? "enum" : "plain") . "/";
echo count($constAttrs) . "/";
echo $constAttrs[0]->getName() . "/";
echo $constAttrs[0]->getArguments()[0] . "/";
echo $constAttrs[0]->newInstance()->label() . "\n";
$listed = (new ReflectionClass(ConstTarget::class))->getReflectionConstants()[0];
echo ($listed->isFinal() ? "listed-final" : "listed-open") . "\n";
$case = new ReflectionClassConstant(CaseTarget::class, "Ready");
$caseAttrs = $case->getAttributes();
echo $case->getName() . "/";
echo ($case->isFinal() ? "final" : "open") . "/";
echo ($case->isEnumCase() ? "enum" : "plain") . "/";
echo count($caseAttrs) . "/";
echo $caseAttrs[0]->getName() . "/";
echo $caseAttrs[0]->getArguments()[0] . "/";
echo $caseAttrs[0]->newInstance()->label() . "\n";
foreach ((new ReflectionClass(CaseTarget::class))->getReflectionConstants() as $constant) {
    if ($constant->getName() === "Ready") {
        echo ($constant->isEnumCase() ? "listed-enum" : "listed-plain") . "\n";
    }
}
$level = new ReflectionClassConstant(CaseTarget::class, "LEVEL");
echo ($level->isFinal() ? "level-final" : "level-open") . "/";
echo ($level->isEnumCase() ? "level-enum" : "level-plain") . "\n";
$unit = new ReflectionEnumUnitCase(CaseTarget::class, "Ready");
$unitAttrs = $unit->getAttributes();
echo $unit->getName() . "/";
echo ($unit->getValue() === CaseTarget::Ready ? "unit-value" : "unit-bad") . "/";
echo $unitAttrs[0]->newInstance()->label() . "\n";
$backed = new ReflectionEnumBackedCase(CaseTarget::class, "Ready");
$backedAttrs = $backed->getAttributes();
echo $backed->getName() . "/";
echo ($backed->getValue() === CaseTarget::Ready ? "backed-value" : "backed-bad") . "/";
echo $backed->getBackingValue() . "/";
echo $backedAttrs[0]->newInstance()->label();
"#,
    );
    assert_eq!(
        out,
        "ANSWER/final/plain/1/Marker/const/const\nlisted-final\nReady/open/enum/1/Marker/case/case\nlisted-enum\nlevel-final/level-plain\nReady/unit-value/case\nReady/backed-value/ready/case"
    );
}

/// Verifies `ReflectionEnum` exposes AOT enum name and backing metadata.
#[test]
fn test_reflection_enum_owner_metadata() {
    let out = compile_and_run(
        r#"<?php
enum ReflectPureEnum {
    case Ready;
    case Done;
}
enum ReflectBackedEnum: int {
    case One = 1;
    case Two = 2;
}
$pure = new ReflectionEnum(ReflectPureEnum::class);
echo $pure->getName() . ":";
echo ($pure->isBacked() ? "B" : "b") . ":";
echo ($pure->getBackingType() === null ? "N" : "n") . ":";
$backed = new ReflectionEnum(ReflectBackedEnum::class);
$type = $backed->getBackingType();
echo ($backed->isBacked() ? "B" : "b") . ":";
echo $type->getName() . ":";
echo ($type->isBuiltin() ? "I" : "i");
"#,
    );
    assert_eq!(
        out,
        "ReflectPureEnum:b:N:B:int:I"
    );
}

/// Verifies `ReflectionClassConstant` exposes visibility predicates and modifiers.
#[test]
fn test_reflection_class_constant_visibility_and_modifiers() {
    let out = compile_and_run(
        r#"<?php
class ConstVisibilityTarget {
    private const SECRET = 1;
    protected const LIMIT = 2;
    final public const ANSWER = 3;
}
enum ConstVisibilityEnum {
    case Ready;
}
$secret = new ReflectionClassConstant(ConstVisibilityTarget::class, "SECRET");
echo "SECRET:";
echo $secret->isPrivate() ? "R" : "r";
echo $secret->isProtected() ? "P" : "p";
echo $secret->isPublic() ? "U" : "u";
echo $secret->isFinal() ? "F" : "f";
echo ":" . $secret->getModifiers() . "\n";
$limit = new ReflectionClassConstant(ConstVisibilityTarget::class, "LIMIT");
echo "LIMIT:";
echo $limit->isPrivate() ? "R" : "r";
echo $limit->isProtected() ? "P" : "p";
echo $limit->isPublic() ? "U" : "u";
echo $limit->isFinal() ? "F" : "f";
echo ":" . $limit->getModifiers() . "\n";
$answer = new ReflectionClassConstant(ConstVisibilityTarget::class, "ANSWER");
echo "ANSWER:";
echo $answer->isPrivate() ? "R" : "r";
echo $answer->isProtected() ? "P" : "p";
echo $answer->isPublic() ? "U" : "u";
echo $answer->isFinal() ? "F" : "f";
echo ":" . $answer->getModifiers() . "\n";
$case = new ReflectionClassConstant(ConstVisibilityEnum::class, "Ready");
echo "Ready:";
echo $case->isPrivate() ? "R" : "r";
echo $case->isProtected() ? "P" : "p";
echo $case->isPublic() ? "U" : "u";
echo $case->isFinal() ? "F" : "f";
echo ":" . $case->getModifiers() . "\n";
echo ReflectionClassConstant::IS_PUBLIC . ":";
echo ReflectionClassConstant::IS_PROTECTED . ":";
echo ReflectionClassConstant::IS_PRIVATE . ":";
echo ReflectionClassConstant::IS_FINAL . "\n";
echo "VALUES:" . $secret->getValue() . ":" . $limit->getValue() . ":" . $answer->getValue() . ":";
echo $case->getValue() === ConstVisibilityEnum::Ready ? "E" : "e";
echo "\n";
foreach ((new ReflectionClass(ConstVisibilityTarget::class))->getReflectionConstants() as $constant) {
    if ($constant->getName() === "ANSWER") {
        echo "LIST:" . $constant->getValue();
    }
}
"#,
    );
    assert_eq!(
        out,
        "SECRET:Rpuf:4\nLIMIT:rPuf:2\nANSWER:rpUF:33\nReady:rpUf:1\n1:2:4:32\nVALUES:1:2:3:E\nLIST:3"
    );
}

/// Verifies trait constants expose final metadata through direct and listed reflection.
#[test]
fn test_reflection_trait_constant_final_metadata() {
    let out = compile_and_run(
        r#"<?php
trait TraitConstTarget {
    final public const FLAG = 1;
    public const OPEN = 2;
}
interface InterfaceConstTarget {
    final public const LIMIT = 3;
    public const OPEN = 4;
}
$direct = new ReflectionClassConstant(TraitConstTarget::class, "FLAG");
echo $direct->getDeclaringClass()->getName() . ":";
echo $direct->isFinal() ? "F" : "f";
$flag = "?";
$open = "?";
foreach ((new ReflectionClass(TraitConstTarget::class))->getReflectionConstants() as $constant) {
    if ($constant->getName() === "FLAG") {
        $flag = $constant->isFinal() ? "F" : "f";
    }
    if ($constant->getName() === "OPEN") {
        $open = $constant->isFinal() ? "O" : "o";
    }
}
echo ":" . $flag . $open;
$ifaceDirect = new ReflectionClassConstant(InterfaceConstTarget::class, "LIMIT");
echo ":" . $ifaceDirect->getDeclaringClass()->getName() . ":";
echo $ifaceDirect->isFinal() ? "I" : "i";
$limit = "?";
$ifaceOpen = "?";
foreach ((new ReflectionClass(InterfaceConstTarget::class))->getReflectionConstants() as $constant) {
    if ($constant->getName() === "LIMIT") {
        $limit = $constant->isFinal() ? "I" : "i";
    }
    if ($constant->getName() === "OPEN") {
        $ifaceOpen = $constant->isFinal() ? "P" : "p";
    }
}
echo ":" . $limit . $ifaceOpen;
"#,
    );
    assert_eq!(out, "TraitConstTarget:F:Fo:InterfaceConstTarget:I:Ip");
}

/// Verifies interface constant reflection keeps the interface that declared each constant.
#[test]
fn test_reflection_interface_constant_declaring_metadata() {
    let out = compile_and_run(
        r#"<?php
interface InterfaceConstBase {
    public const ROOT = 1;
    public const SHARED = 2;
    final public const LOCK = 5;
}
interface InterfaceConstChild extends InterfaceConstBase {
    public const SHARED = 3;
}
class InterfaceConstImpl implements InterfaceConstChild {}
$root = new ReflectionClassConstant(InterfaceConstChild::class, "ROOT");
$shared = new ReflectionClassConstant(InterfaceConstChild::class, "SHARED");
$implRoot = new ReflectionClassConstant(InterfaceConstImpl::class, "ROOT");
$implShared = new ReflectionClassConstant(InterfaceConstImpl::class, "SHARED");
$implLock = new ReflectionClassConstant(InterfaceConstImpl::class, "LOCK");
echo $root->getDeclaringClass()->getName() . ":";
echo $shared->getDeclaringClass()->getName() . ":";
echo $implRoot->getDeclaringClass()->getName() . ":";
echo $implShared->getDeclaringClass()->getName() . ":";
echo $implLock->getDeclaringClass()->getName() . ":";
echo $implLock->isFinal() ? "F" : "f";
$all = (new ReflectionClass(InterfaceConstImpl::class))->getConstants();
echo ":" . $all["ROOT"] . ":" . $all["SHARED"] . ":" . $all["LOCK"];
$decls = ["ROOT" => "?", "SHARED" => "?", "LOCK" => "?"];
$finals = ["ROOT" => "?", "SHARED" => "?", "LOCK" => "?"];
foreach ((new ReflectionClass(InterfaceConstImpl::class))->getReflectionConstants() as $constant) {
    $name = $constant->getName();
    $decls[$name] = $constant->getDeclaringClass()->getName();
    $finals[$name] = $constant->isFinal() ? "F" : "f";
}
echo ":" . $decls["ROOT"] . ":" . $decls["SHARED"] . ":" . $decls["LOCK"];
echo ":" . $finals["ROOT"] . ":" . $finals["SHARED"] . ":" . $finals["LOCK"];
"#,
    );
    assert_eq!(
        out,
        "InterfaceConstBase:InterfaceConstChild:InterfaceConstBase:InterfaceConstChild:InterfaceConstBase:F:1:3:5:InterfaceConstBase:InterfaceConstChild:InterfaceConstBase:f:f:F"
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
