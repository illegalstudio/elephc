//! Purpose:
//! Integration tests for untyped property defaults: PHP initializes untyped properties
//! without a default (and with an explicit `= null` default) to null, even when later
//! assignments give the slot a concrete scalar/array type.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.
//! - Untyped null-defaulted properties must ride the same nullable storage as typed `?T` properties.

use super::*;

/// Verifies that an untyped instance property with no default reads as null before any assignment.
#[test]
fn test_untyped_property_without_default_defaults_to_null() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $never;
}
$c = new Car();
var_dump($c->never);
"#,
    );
    assert_eq!(out, "NULL\n");
}

/// Verifies that an untyped no-default property later assigned an int still defaults to null.
#[test]
fn test_untyped_property_without_default_assigned_int_later() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $pippo;
}
$c = new Car();
var_dump($c->pippo);
$c->pippo = 5;
var_dump($c->pippo);
"#,
    );
    assert_eq!(out, "NULL\nint(5)\n");
}

/// Verifies that an untyped no-default property later assigned a string still defaults to null.
#[test]
fn test_untyped_property_without_default_assigned_string_later() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $s;
}
$c = new Car();
var_dump($c->s);
$c->s = "hi";
var_dump($c->s);
"#,
    );
    assert_eq!(out, "NULL\nstring(2) \"hi\"\n");
}

/// Verifies that an untyped `= null` property later assigned an int compiles and keeps its null default.
#[test]
fn test_untyped_property_explicit_null_default_assigned_int_later() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $pippo = null;
}
$c = new Car();
var_dump($c->pippo);
$c->pippo = 5;
var_dump($c->pippo);
"#,
    );
    assert_eq!(out, "NULL\nint(5)\n");
}

/// Verifies that an untyped `= null` property later assigned an array var_dumps NULL before the write.
#[test]
fn test_untyped_property_explicit_null_default_assigned_array_later() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $arr = null;
}
$c = new Car();
var_dump($c->arr);
$c->arr = [1, 2];
echo $c->arr[1], "\n";
"#,
    );
    assert_eq!(out, "NULL\n2\n");
}

/// Verifies bool and float assignments to untyped `= null` properties keep the null default.
#[test]
fn test_untyped_property_explicit_null_default_assigned_bool_and_float() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $b = null;
    public $f = null;
}
$c = new Car();
var_dump($c->b);
var_dump($c->f);
$c->b = true;
$c->f = 1.5;
var_dump($c->b);
var_dump($c->f);
"#,
    );
    assert_eq!(out, "NULL\nNULL\nbool(true)\nfloat(1.5)\n");
}

/// Verifies is_null()/`=== null` observe the null default of an untyped property before and after a write.
#[test]
fn test_untyped_property_null_default_strict_null_comparison() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $x = null;
}
$c = new Car();
echo is_null($c->x) ? "y" : "n";
echo ($c->x === null) ? "y" : "n";
$c->x = 5;
echo is_null($c->x) ? "y" : "n";
echo ($c->x === null) ? "y" : "n";
echo "\n";
"#,
    );
    assert_eq!(out, "yynn\n");
}

/// Verifies that an untyped static property with no default reads as null before any assignment.
#[test]
fn test_untyped_static_property_without_default_defaults_to_null() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public static $sp;
}
var_dump(Car::$sp);
Car::$sp = 5;
var_dump(Car::$sp);
"#,
    );
    assert_eq!(out, "NULL\nint(5)\n");
}

/// Verifies that an untyped `= null` static property later assigned a string keeps its null default.
#[test]
fn test_untyped_static_property_explicit_null_default_assigned_string_later() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public static $spn = null;
}
var_dump(Car::$spn);
Car::$spn = "hi";
var_dump(Car::$spn);
"#,
    );
    assert_eq!(out, "NULL\nstring(2) \"hi\"\n");
}

/// Verifies an object passed through an untyped static-method parameter can be stored in and
/// returned from an inferred object static-property slot without an EIR storage mismatch.
#[test]
fn test_untyped_static_property_assignment_expression_from_dynamic_parameter_returns_object() {
    let out = compile_and_run(
        r#"<?php
class Container {
    protected static $instance;

    public static function set($container) {
        return static::$instance = $container;
    }

    public static function get() {
        return static::$instance;
    }
}

$container = new Container();
var_dump(Container::set($container) === $container);
var_dump(Container::get() === $container);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

/// Verifies heterogeneous assignments to an untyped `= null` property follow last-write-wins like PHP.
#[test]
fn test_untyped_property_heterogeneous_assignments_last_value_wins() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $h = null;
}
$c = new Car();
var_dump($c->h);
$c->h = 1;
var_dump($c->h);
$c->h = "s";
var_dump($c->h);
"#,
    );
    assert_eq!(out, "NULL\nint(1)\nstring(1) \"s\"\n");
}

/// Verifies the already-working untyped object-holding property keeps its null default (regression guard).
#[test]
fn test_untyped_property_object_null_default_still_works() {
    let out = compile_and_run(
        r#"<?php
class Dep {
    public function id(): int {
        return 7;
    }
}
class Holder {
    public $obj = null;
}
$h = new Holder();
var_dump($h->obj);
$h->obj = new Dep();
echo $h->obj->id(), "\n";
"#,
    );
    assert_eq!(out, "NULL\n7\n");
}

/// Verifies untyped properties with a concrete non-null default are unaffected (regression guard).
#[test]
fn test_untyped_property_nonnull_default_unchanged() {
    let out = compile_and_run(
        r#"<?php
class Car {
    public $n = 1;
    public $s = "a";
}
$c = new Car();
var_dump($c->n);
var_dump($c->s);
"#,
    );
    assert_eq!(out, "int(1)\nstring(1) \"a\"\n");
}


/// Verifies an ARRAY LITERAL default on a `?array`, `mixed` or other union property compiles and
/// reads back (issue #688).
///
/// Declaring the class was enough to be refused — no read, no write, no iteration:
///
///     class C { public ?array $x = [1, 2]; }
///     EIR backend error: unsupported EIR backend feature:
///     object_new for default value of property $x with PHP type Union([Array(Mixed), Void])
///
/// The positional spelling was fixed when the `mixed` slot learned to box an indexed literal; the
/// KEYED spelling had no default form at all and kept failing with the same message. PHP has no
/// separate associative array type, so `["k" => 1]` in a `?array` slot is the same default as
/// `[1, 2]` is — only the storage a string key needs differs.
///
/// The matrix keeps the two spellings beside the shapes that already worked, so a future change
/// cannot fix one and lose the other: a plain `array` slot, a `mixed` slot holding a scalar, an
/// explicit `null` default, and the constructor-assignment workaround the issue documented.
///
/// Every expected value is verbatim host PHP 8.5.10 output for the same fixture.
#[test]
fn test_array_literal_defaults_on_union_and_mixed_properties() {
    let out = compile_and_run(
        r#"<?php
class A { public ?array $x = [1, 2]; }
class B { public ?array $x = []; }
class C { public mixed $x = [1, 2]; }
class D { public ?array $x = null; }
class E { public array $x = [1, 2]; }
class F { public mixed $x = 5; }
class G { public ?array $x = null; public function __construct() { $this->x = [1, 2]; } }
class H { public ?array $x = ["k" => 1, "j" => "s"]; }
class I { public static ?array $x = [1, 2]; }
class J { public array|string $x = [1, 2]; }
class K { public mixed $x = ["k" => 1]; }
$a = new A(); echo count($a->x), ":", implode(",", $a->x), "\n";
$b = new B(); echo count($b->x), "\n";
$c = new C(); echo count($c->x), ":", implode(",", $c->x), "\n";
$d = new D(); var_dump($d->x);
$e = new E(); echo count($e->x), "\n";
$f = new F(); var_dump($f->x);
$g = new G(); echo count($g->x), "\n";
$h = new H(); echo count($h->x), ":", $h->x["k"], ":", $h->x["j"], "\n";
echo count(I::$x), "\n";
$j = new J(); echo count($j->x), "\n";
$k = new K(); echo count($k->x), ":", $k->x["k"], "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "2:1,2\n",
            "0\n",
            "2:1,2\n",
            "NULL\n",
            "2\n",
            "int(5)\n",
            "2\n",
            "2:1:s\n",
            "2\n",
            "2\n",
            "1:1\n",
        )
    );
}

/// `Foo::class` is a compile-time string, so it is a legal property default: a static or an
/// instance property, typed or `mixed`, an array value or an array key, through an import
/// alias. Each of these failed in the backend, e.g. `static property initializer for default
/// value of static property ... with PHP type Str`.
#[test]
fn test_named_class_constant_is_a_literal_property_default() {
    let out = compile_and_run(
        r#"<?php
namespace App\Shapes;

use App\Shapes\Circle as Round;

final class Circle {}
final class Square {}

final class Holder
{
    public static string $fallback = Circle::class;
    public static ?string $maybe = Round::class;
    public string $name = Square::class;
    public mixed $any = \stdClass::class;
    public array $classes = ['sq' => Square::class, 'ci' => Circle::class];
    public array $list = [Square::class, Circle::class];
    public array $byClass = [Square::class => 4, Circle::class => 0];
    public static array $registry = [Circle::class => 'round'];
    public const KIND = Square::class;

    public function describe(string $default = Circle::class): string
    {
        return $default;
    }
}

echo Holder::$fallback, "\n";
echo Holder::$maybe, "\n";
$h = new Holder();
echo $h->name, "\n";
echo $h->any, "\n";
echo $h->classes['ci'], "\n";
echo implode(',', $h->list), "\n";
echo $h->byClass[Square::class], "\n";
echo Holder::$registry[Circle::class], "\n";
echo Holder::KIND, "\n";
echo $h->describe(), "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "App\\Shapes\\Circle\n",
            "App\\Shapes\\Circle\n",
            "App\\Shapes\\Square\n",
            "stdClass\n",
            "App\\Shapes\\Circle\n",
            "App\\Shapes\\Square,App\\Shapes\\Circle\n",
            "4\n",
            "round\n",
            "App\\Shapes\\Square\n",
            "App\\Shapes\\Circle\n",
        )
    );
}
