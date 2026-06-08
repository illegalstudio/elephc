//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object classes, including class empty, class object aliasing, and class constructor calls method.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies that PHP 8 semi-reserved keywords are usable as member names end-to-end: an
/// instance method (`self`), a static method (`new`), an instance method via `->parent()`,
/// a property accessed as `->list`, and a class constant `FN` accessed via `::`. Mirrors PHP,
/// which outputs "3|7|9|11|5".
#[test]
fn test_keyword_named_members() {
    let out = compile_and_run(
        r#"<?php
class Widget {
    public $list = 7;
    const FN = 5;
    public function self() { return 3; }
    public function parent() { return 9; }
    public static function new() { return 11; }
}
$w = new Widget();
echo $w->self(), "|";
echo $w->list, "|";
echo $w->parent(), "|";
echo Widget::new(), "|";
echo Widget::FN;
"#,
    );
    assert_eq!(out, "3|7|9|11|5");
}

/// Verifies that an empty class (no properties or methods) can be instantiated and
/// emits the expected "ok" output, confirming object allocation works for minimal classes.
#[test]
fn test_class_empty() {
    let out = compile_and_run(
        r#"<?php
class Blank {}
$e = new Blank();
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that assigning an object to a second variable shares the same instance.
/// Both variables reference the same heap object, so mutating via one is visible via the other.
#[test]
fn test_class_dynamic_instantiation() {
    // `new $variable()` dispatches known class names through the same AOT
    // allocation path as `new ClassName()`. A known name yields an object
    // Mixed cell; an unknown name currently preserves the legacy PHP null
    // fallback until the missing-class fatal path is tightened.
    let out = compile_and_run(
        r#"<?php
class Foo {}
class Bar {}
$cls = "Foo";
$obj = new $cls();
$cls2 = "Bar";
$obj2 = new $cls2();
$missing = "NoSuchClass";
$bad = new $missing();
echo gettype($obj) . "|" . gettype($obj2) . "|" . gettype($bad);
"#,
    );
    assert_eq!(out, "object|object|NULL");
}

/// Verifies compiled PHP output for class dynamic instantiation runs property defaults.
#[test]
fn test_class_dynamic_instantiation_runs_property_defaults() {
    // `new $var()` must apply declared property defaults through the same
    // allocation path as `new ClassName()`. Previously these read back as
    // 0/null.
    let out = compile_and_run(
        r#"<?php
class C {
    public int $n = 7;
    public string $s = "hi";
    public float $f = 1.5;
    public bool $b = true;
    public array $a = [1, 2, 3];
}
$cls = "C";
$o = new $cls();
echo $o->n . "|" . $o->s . "|" . $o->f . "|" . ($o->b ? "T" : "F") . "|" . count($o->a);
"#,
    );
    assert_eq!(out, "7|hi|1.5|T|3");
}

/// Verifies that dynamic instantiation forwards constructor arguments.
#[test]
fn test_class_dynamic_instantiation_runs_constructor_args() {
    let out = compile_and_run(
        r#"<?php
class C {
    public int $x = 1;
    public function __construct(int $x) { $this->x = $x; }
}
$cls = "C";
$o = new $cls(7);
echo $o->x;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies that dynamic instantiation uses SPL-specific runtime storage initialization.
#[test]
fn test_class_dynamic_instantiation_uses_spl_storage() {
    let out = compile_and_run(
        r#"<?php
$cls = "SplFixedArray";
$a = new $cls(2);
$a[0] = "x";
echo count($a) . ":" . $a[0];
"#,
    );
    assert_eq!(out, "2:x");
}

/// Verifies that dynamic class-string lookup follows PHP's case-insensitive class rules.
#[test]
fn test_class_dynamic_instantiation_is_case_insensitive() {
    let out = compile_and_run(
        r#"<?php
function pick_class(): string {
    return "mixedcase";
}
class MixedCase {
    public int $x = 0;
    public function __construct(int $x) { $this->x = $x; }
}
$cls = pick_class();
$o = new $cls(12);
echo gettype($o) . ":" . $o->x;
"#,
    );
    assert_eq!(out, "object:12");
}

/// Verifies compiled PHP output for dynamic instantiation missing class skips propinit.
#[test]
fn test_dynamic_instantiation_missing_class_skips_propinit() {
    // An unknown class name must still return null and must NOT dispatch a
    // property-init thunk for a missing class_id (regression for the
    // _class_propinit_ptrs miss path).
    let out = compile_and_run(
        r#"<?php
class Has { public int $x = 9; }
$missing = "Nope";
$bad = new $missing();
echo gettype($bad);
$ok = "Has";
$o = new $ok();
echo "|" . $o->x;
"#,
    );
    assert_eq!(out, "NULL|9");
}

/// Verifies compiled PHP output for class object aliasing.
#[test]
fn test_class_object_aliasing() {
    let out = compile_and_run(
        r#"<?php
class Box { public $val = 0; }
$a = new Box();
$a->val = 42;
$b = $a;
echo $b->val;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies that a constructor can call another method on the same object,
/// ensuring that `$this` is valid and method dispatch works during construction.
#[test]
fn test_class_constructor_calls_method() {
    let out = compile_and_run(
        r#"<?php
class Init { public $ready = 0;
    public function __construct() { $this->setup(); }
    public function setup() { $this->ready = 1; }
}
$i = new Init();
echo $i->ready;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies that two classes composing each other (Address held inside Person) work correctly,
/// including cross-object method calls and string concatenation with an embedded object property.
#[test]
fn test_class_multiple_classes_composing() {
    let out = compile_and_run(
        r#"<?php
class Address { public $city;
    public function __construct($c) { $this->city = $c; }
}
class Person { public $name; public $address;
    public function __construct($n, $addr) { $this->name = $n; $this->address = $addr; }
    public function info() { return $this->name . " from " . $this->address->city; }
}
$addr = new Address("Rome");
$p = new Person("Marco", $addr);
echo $p->info();
"#,
    );
    assert_eq!(out, "Marco from Rome");
}

/// Verifies that a class property initialized to an empty string behaves correctly:
/// `strlen()` returns 0, concatenation produces the expected pipe-delimited output.
#[test]
fn test_class_empty_string_property() {
    let out = compile_and_run(
        r#"<?php
class Tag { public $label = "";
    public function __construct($l) { $this->label = $l; }
}
$t = new Tag("");
echo strlen($t->label) . "|" . $t->label . "|done";
"#,
    );
    assert_eq!(out, "0||done");
}

/// Verifies that a class property holding a 1000-character string is stored and retrieved
/// correctly, with `strlen()` returning the correct length.
#[test]
fn test_class_long_string_property() {
    let out = compile_and_run(
        r#"<?php
class Buffer { public $data;
    public function __construct($d) { $this->data = $d; }
}
$b = new Buffer(str_repeat("x", 1000));
echo strlen($b->data);
"#,
    );
    assert_eq!(out, "1000");
}

/// Verifies that a method can concatenate multiple string properties and return the result,
/// ensuring `$this` property reads and string concatenation work inside methods.
#[test]
fn test_class_string_concat_in_method() {
    let out = compile_and_run(
        r#"<?php
class Row { public $a; public $b; public $c;
    public function __construct($a, $b, $c) { $this->a = $a; $this->b = $b; $this->c = $c; }
    public function csv() { return $this->a . "," . $this->b . "," . $this->c; }
}
$r = new Row("x", "y", "z");
echo $r->csv();
"#,
    );
    assert_eq!(out, "x,y,z");
}

/// Verifies that a boolean property can be used in a ternary expression,
/// returning the correct branch ("yes" / "no") based on the stored `true` value.
#[test]
fn test_class_bool_property() {
    let out = compile_and_run(
        r#"<?php
class Flag { public $on;
    public function __construct($v) { $this->on = $v; }
}
$f = new Flag(true);
echo $f->on ? "yes" : "no";
"#,
    );
    assert_eq!(out, "yes");
}

/// Verifies that a class property holding an array works with `count()` inside a method,
/// confirming array property reads and the builtin `count()` function work correctly.
#[test]
fn test_class_array_property() {
    let out = compile_and_run(
        r#"<?php
class Stack { public $items;
    public function __construct() { $this->items = [1, 2, 3]; }
    public function size() { return count($this->items); }
}
$s = new Stack();
echo $s->size();
"#,
    );
    assert_eq!(out, "3");
}

/// Stress test: creates 1000 object instances in a loop, updating a reference each time.
/// Verifies that object allocation and last-instance tracking work correctly across many iterations.
#[test]
fn test_class_1000_objects_in_loop() {
    let out = compile_and_run(
        r#"<?php
class Obj { public $id;
    public function __construct($id) { $this->id = $id; }
}
$last = new Obj(0);
for ($i = 1; $i < 1000; $i++) {
    $last = new Obj($i);
}
echo $last->id;
"#,
    );
    assert_eq!(out, "999");
}

/// Verifies that a class with 10 properties initialized in the constructor
/// sums them correctly via a method, ensuring multi-property reads and integer arithmetic.
#[test]
fn test_class_many_properties() {
    let out = compile_and_run(
        r#"<?php
class Big { public $a; public $b; public $c; public $d; public $e;
    public $f; public $g; public $h; public $i; public $j;
    public function __construct() {
        $this->a = 1; $this->b = 2; $this->c = 3; $this->d = 4; $this->e = 5;
        $this->f = 6; $this->g = 7; $this->h = 8; $this->i = 9; $this->j = 10;
    }
    public function sum() {
        return $this->a + $this->b + $this->c + $this->d + $this->e +
               $this->f + $this->g + $this->h + $this->i + $this->j;
    }
}
$b = new Big();
echo $b->sum();
"#,
    );
    assert_eq!(out, "55");
}

/// Verifies deeply nested function calls that build nested HTML tags via string concatenation,
/// ensuring argument evaluation order, nested calls, and string concat work correctly.
#[test]
fn test_deeply_nested_string_function_calls() {
    let out = compile_and_run(
        r#"<?php
function wrap($s, $tag) { return "<" . $tag . ">" . $s . "</" . $tag . ">"; }
echo wrap(wrap(wrap("hello", "b"), "i"), "p");
"#,
    );
    assert_eq!(out, "<p><i><b>hello</b></i></p>");
}

/// Verifies a recursive function that builds a string via repeated concatenation,
/// ensuring recursion, base-case handling, and string concat work correctly.
#[test]
fn test_recursive_string_building() {
    let out = compile_and_run(
        r#"<?php
function repeat_str($s, $n) {
    if ($n <= 0) { return ""; }
    return $s . repeat_str($s, $n - 1);
}
echo repeat_str("ab", 5);
"#,
    );
    assert_eq!(out, "ababababab");
}

/// Verifies that a closure can capture an object via `use($c)` and that the captured
/// reference remains valid after multiple method calls on the object.
#[test]
fn test_closure_capturing_object() {
    let out = compile_and_run(
        r#"<?php
class Counter { public $n = 0; public function inc() { $this->n = $this->n + 1; } }
$c = new Counter();
$c->inc();
$c->inc();
$fn = function() use ($c) { return $c; };
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that a class property storing a float is read correctly inside a method
/// and used in a floating-point arithmetic expression, producing the correct area result.
#[test]
fn test_class_float_property_via_method() {
    let out = compile_and_run(
        r#"<?php
class Circle {
    public $radius;
    public function __construct($r) { $this->radius = $r; }
    public function area() { return 3.14159 * $this->radius * $this->radius; }
}
$c = new Circle(5.0);
echo $c->area();
"#,
    );
    assert_eq!(out, "78.53975");
}

/// Verifies that a method returning a float property emits the value correctly,
/// ensuring float return types and property reads from methods work end-to-end.
#[test]
fn test_class_method_returns_float_property() {
    let out = compile_and_run(
        r#"<?php
class Foo {
    public $x;
    public function __construct($v) { $this->x = $v; }
    public function getX() { return $this->x; }
}
$f = new Foo(3.14);
echo $f->getX();
"#,
    );
    assert_eq!(out, "3.14");
}

/// Verifies that a method returning `$this` enables fluent chaining:
/// after `->add()` the object is returned and subsequent calls succeed.
#[test]
fn test_class_method_returns_this() {
    let out = compile_and_run(
        r#"<?php
class Builder {
    public $parts = "";
    public function add($s) { $this->parts = $this->parts . $s; return $this; }
}
$b = new Builder();
$b->add("hello");
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that a private property is inaccessible from outside the class
/// but can be read via a public accessor method, ensuring visibility rules are enforced.
#[test]
fn test_class_private_property_via_method() {
    let out = compile_and_run(
        r#"<?php
class Secret {
    private $value;
    public function __construct($value) { $this->value = $value; }
    public function reveal() { return $this->value; }
}
$s = new Secret("ok");
echo $s->reveal();
"#,
    );
    assert_eq!(out, "ok");
}

/// Verifies that a `readonly` property can be initialized in the constructor
/// and read via a public accessor method, ensuring readonly semantics are respected.
#[test]
fn test_class_readonly_property() {
    let out = compile_and_run(
        r#"<?php
class User {
    public readonly $id;
    public function __construct($id) { $this->id = $id; }
    public function id() { return $this->id; }
}
$u = new User(7);
echo $u->id();
"#,
    );
    assert_eq!(out, "7");
}
