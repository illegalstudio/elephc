//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object classes, including class empty, class object aliasing, and class constructor calls method.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

#[test]
fn test_class_empty() {
    // Empty class with no properties or methods
    let out = compile_and_run(
        r#"<?php
class Blank {}
$e = new Blank();
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_class_object_aliasing() {
    // Assigning object to another variable shares the same instance
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

#[test]
fn test_class_constructor_calls_method() {
    // Constructor calling another method on the same object
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

#[test]
fn test_class_multiple_classes_composing() {
    // Two classes where one holds an instance of the other
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

#[test]
fn test_class_empty_string_property() {
    // Empty string property and strlen on it
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

#[test]
fn test_class_long_string_property() {
    // String property holding a long (1000 char) string
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

#[test]
fn test_class_string_concat_in_method() {
    // Method concatenating multiple string properties
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

#[test]
fn test_class_bool_property() {
    // Boolean property used in ternary
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

#[test]
fn test_class_array_property() {
    // Array property with count()
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

#[test]
fn test_class_1000_objects_in_loop() {
    // Stress test: create 1000 objects in a loop
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

#[test]
fn test_class_many_properties() {
    // Object with 10 properties and a method summing them
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

#[test]
fn test_deeply_nested_string_function_calls() {
    // Deeply nested function calls building nested HTML tags
    let out = compile_and_run(
        r#"<?php
function wrap($s, $tag) { return "<" . $tag . ">" . $s . "</" . $tag . ">"; }
echo wrap(wrap(wrap("hello", "b"), "i"), "p");
"#,
    );
    assert_eq!(out, "<p><i><b>hello</b></i></p>");
}

#[test]
fn test_recursive_string_building() {
    // Recursive function that builds a string via concatenation
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

#[test]
fn test_closure_capturing_object() {
    // Closure capturing an object via use()
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
