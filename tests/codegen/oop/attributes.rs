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

#[test]
fn test_attributes_do_not_alter_runtime_behavior() {
    // A class decorated with several attributes must compile to the same
    // observable behavior as the equivalent class without them.
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

#[test]
fn test_qualified_attribute_name_compiles() {
    // Symfony-style attributes use fully-qualified names; the parser must
    // accept them and the codegen must emit unchanged output.
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

#[test]
fn test_parameter_attribute_compiles() {
    // Attributes on function parameters must compile identically to the
    // bare version.
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

#[test]
fn test_override_attribute_on_valid_override_compiles() {
    // Method does override a parent — `#[\Override]` should pass and behave
    // identically to the same method without the attribute.
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

#[test]
fn test_override_attribute_through_interface_compiles() {
    // `#[\Override]` on an interface implementation must accept the inherited
    // signature.
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

#[test]
fn test_class_attribute_names_normalises_fully_qualified_form() {
    // Name resolution canonicalises `#[\Override]` to `Override` (no leading
    // backslash), matching PHP's `ReflectionAttribute::getName()`.
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

#[test]
fn test_class_attribute_args_preserves_int_and_string_literals() {
    // Strings, ints, booleans, and null literals are all preserved as
    // boxed mixed cells; unsupported argument shapes are rejected only when
    // a reflection helper needs to materialize them.
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

#[test]
fn test_class_attribute_args_preserves_bool_and_null_literals() {
    // Booleans render via PHP's standard echo conversion (true → "1",
    // false → ""). Null also renders as the empty string. The point is to
    // pin down the runtime preserves the *shape* of these payloads.
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
    assert_eq!(
        out,
        "count=2\nAuthor:[Ada][1815]\nVersion:[1.0][1]\n"
    );
}

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

#[test]
fn test_class_get_attributes_normalises_fully_qualified_name() {
    // ReflectionAttribute::getName() returns the resolved class name without
    // the source-level leading backslash, matching PHP semantics.
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

#[test]
fn test_class_attribute_args_picks_first_matching_attribute() {
    // When the same attribute is repeated, return the args of the first one.
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
echo count($attrs) . "/";
echo $attrs[0]->getName() . "/";
echo $attrs[0]->getArguments()[0] . "/";
echo $attrs[0]->getArguments()[1];
"#,
    );
    assert_eq!(out, "1/Route//home/GET");
}

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
echo count($attrs) . "/";
echo $attrs[0]->getName() . "/";
echo $attrs[0]->getArguments()[0];
"#,
    );
    assert_eq!(out, "1/Route//home");
}

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
echo count($attrs) . "/";
echo $attrs[0]->getName() . "/";
echo $attrs[0]->getArguments()[0];
"#,
    );
    assert_eq!(out, "1/Column/id");
}

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
echo count($attrs) . "/";
echo $attrs[0]->getName() . "/";
echo $attrs[0]->getArguments()[0];
"#,
    );
    assert_eq!(out, "1/Column/id");
}

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
