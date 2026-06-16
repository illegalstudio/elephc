//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object property mutations, including class array of objects property access, class property array push, and class property array assign.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Compiles a loop over an array of class instances, reading the `price` field
/// of each `Item` object via `$items[$i]->price` and accumulating the sum.
#[test]
fn test_class_array_of_objects_property_access() {
    let out = compile_and_run(
        r#"<?php
class Item {
    public $name;
    public $price;
    public function __construct($n, $p) { $this->name = $n; $this->price = $p; }
}
$items = [];
$items[] = new Item("Apple", 1);
$items[] = new Item("Banana", 2);
$total = 0;
for ($i = 0; $i < count($items); $i++) {
    $total = $total + $items[$i]->price;
}
echo $total;
"#,
    );
    assert_eq!(out, "3");
}

/// Exercises `$this->items[] = $value` (push operator) on a class property
/// that holds an array, verifying the pushed element is retrievable at the
/// correct index.
#[test]
fn test_class_property_array_push() {
    let out = compile_and_run(
        r#"<?php
class Bucket {
    public $items;

    public function __construct() {
        $this->items = [1, 2];
    }

    public function add($value) {
        $this->items[] = $value;
    }

    public function last(): int {
        return $this->items[2];
    }
}

$bucket = new Bucket();
$bucket->add(7);
echo $bucket->last();
"#,
    );
    assert_eq!(out, "7");
}

/// Exercises indexed write `$this->items[0] = $value` on a class property
/// that holds an array, verifying the replaced element is retrieved correctly.
#[test]
fn test_class_property_array_assign() {
    let out = compile_and_run(
        r#"<?php
class Bucket {
    public $items;

    public function __construct() {
        $this->items = [1, 2, 3];
    }

    public function replaceFirst($value) {
        $this->items[0] = $value;
    }

    public function first(): int {
        return $this->items[0];
    }
}

$bucket = new Bucket();
$bucket->replaceFirst(9);
echo $bucket->first();
"#,
    );
    assert_eq!(out, "9");
}

/// Verifies assigning an untyped function parameter into a typed object property.
#[test]
fn test_typed_int_property_accepts_untyped_function_param_assignment() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $n = 0;
}

function set_n(Box $box, $value): void {
    $box->n = $value;
}

$box = new Box();
set_n($box, 7);
echo $box->n;
"#,
    );
    assert_eq!(out, "7");
}

/// Verifies that a typed `public array $headers` property (initialized to `[]`)
/// accepts a string-keyed assignment (`$this->headers["Host"] = ...`) and the
/// value is retrievable via the same key.
#[test]
fn test_typed_array_property_accepts_string_key_assignment() {
    let out = compile_and_run(
        r#"<?php
class Req {
    public array $headers;

    public function __construct() {
        $this->headers = [];
        $this->headers["Host"] = "example.com";
    }
}

$r = new Req();
echo $r->headers["Host"];
"#,
    );
    assert_eq!(out, "example.com");
}

/// Verifies dynamic property writes through a `mixed` receiver can assign
/// method-computed names and values into declared mixed object properties.
#[test]
fn test_dynamic_property_set_on_mixed_receiver_from_method_values() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
    public mixed $name;
}

class Hydrator {
    private function value(int $i): mixed {
        if ($i == 0) {
            return 1;
        }
        return "Ada";
    }

    private function column(int $i): string {
        if ($i == 0) {
            return "id";
        }
        return "name";
    }

    public function fill(mixed $object): mixed {
        $_name = $this->column(0);
        $object->{$_name} = $this->value(0);
        $_name = $this->column(1);
        $object->{$_name} = $this->value(1);
        return $object;
    }
}

$row = (new Hydrator())->fill(new Row());
echo (($row instanceof Row) ? "Row" : "not-row") . ":" . $row->id . ":" . $row->name;
"#,
    );
    assert_eq!(out, "Row:1:Ada");
}

/// Verifies dynamic property writes through a `mixed` receiver preserve mixed
/// string values built by repeated concatenation before assignment.
#[test]
fn test_dynamic_property_set_on_mixed_receiver_with_concat_built_string() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
    public mixed $name;
}

class Hydrator {
    private function value(int $i): mixed {
        if ($i == 0) {
            return 1;
        }
        $_out = "";
        $_out = $_out . chr(65);
        $_out = $_out . chr(100);
        $_out = $_out . chr(97);
        return $_out;
    }

    private function column(int $i): string {
        if ($i == 0) {
            return "id";
        }
        return "name";
    }

    public function fill(mixed $object): mixed {
        $_name = $this->column(0);
        $object->{$_name} = $this->value(0);
        $_name = $this->column(1);
        $object->{$_name} = $this->value(1);
        return $object;
    }
}

$row = (new Hydrator())->fill(new Row());
echo (($row instanceof Row) ? "Row" : "not-row") . ":" . $row->id . ":" . $row->name;
"#,
    );
    assert_eq!(out, "Row:1:Ada");
}

/// Verifies dynamic property writes accept runtime-built property names and
/// runtime-built mixed string values when hydrating a declared object.
#[test]
fn test_dynamic_property_set_on_mixed_receiver_with_runtime_name_and_value() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
    public mixed $name;
}

class Hydrator {
    private function value(int $i): mixed {
        if ($i == 0) {
            return 1;
        }
        $_out = "";
        $_out = $_out . chr(65);
        $_out = $_out . chr(100);
        $_out = $_out . chr(97);
        return $_out;
    }

    private function column(int $i): string {
        $_name = "";
        if ($i == 0) {
            $_name = $_name . chr(105);
            $_name = $_name . chr(100);
            return $_name;
        }
        $_name = $_name . chr(110);
        $_name = $_name . chr(97);
        $_name = $_name . chr(109);
        $_name = $_name . chr(101);
        return $_name;
    }

    public function fill(mixed $object): mixed {
        $_name = $this->column(0);
        $object->{$_name} = $this->value(0);
        $_name = $this->column(1);
        $object->{$_name} = $this->value(1);
        return $object;
    }
}

$row = (new Hydrator())->fill(new Row());
echo (($row instanceof Row) ? "Row" : "not-row") . ":" . $row->id . ":" . $row->name;
"#,
    );
    assert_eq!(out, "Row:1:Ada");
}

/// Verifies a prelude-style hydrator can instantiate from a mixed class-string
/// parameter and then assign runtime dynamic property names into the object.
#[test]
fn test_dynamic_property_set_after_mixed_dynamic_instantiation() {
    let out = compile_and_run(
        r#"<?php
class Row {
    public mixed $id;
    public mixed $name;
}

class Hydrator {
    private function value(int $i): mixed {
        if ($i == 0) {
            return 1;
        }
        $_out = "";
        $_out = $_out . chr(65);
        $_out = $_out . chr(100);
        $_out = $_out . chr(97);
        return $_out;
    }

    private function column(int $i): string {
        $_name = "";
        if ($i == 0) {
            $_name = $_name . chr(105);
            $_name = $_name . chr(100);
            return $_name;
        }
        $_name = $_name . chr(110);
        $_name = $_name . chr(97);
        $_name = $_name . chr(109);
        $_name = $_name . chr(101);
        return $_name;
    }

    private function assign(mixed $object): mixed {
        $_name = $this->column(0);
        $object->{$_name} = $this->value(0);
        $_name = $this->column(1);
        $object->{$_name} = $this->value(1);
        return $object;
    }

    public function fetch(mixed $classOrObject = null): mixed {
        return $this->assign(new $classOrObject());
    }
}

$row = (new Hydrator())->fetch(Row::class);
echo (($row instanceof Row) ? "Row" : "not-row") . ":" . $row->id . ":" . $row->name;
"#,
    );
    assert_eq!(out, "Row:1:Ada");
}

/// Verifies that an untyped `public $headers = []` property (array default)
/// accepts a string-keyed assignment (`$r->headers["Host"] = ...`) and the
/// value is retrievable via the same key.
#[test]
fn test_empty_array_property_default_accepts_string_key_assignment() {
    let out = compile_and_run(
        r#"<?php
class Req {
    public $headers = [];
}

$r = new Req();
$r->headers["Host"] = "example.com";
echo $r->headers["Host"];
"#,
    );
    assert_eq!(out, "example.com");
}

/// Exercises `+=` and `*=` compound assignment on a `public $value` property,
/// verifying the result is `10 + 5 = 15`, then `15 * 3 = 45`.
#[test]
fn test_class_property_compound_assign() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public $value = 10;
}

$counter = new Counter();
$counter->value += 5;
$counter->value *= 3;
echo $counter->value;
"#,
    );
    assert_eq!(out, "45");
}

/// Regression test: when the receiver of a compound property assignment is a
/// function call (`passthrough($counter)->value += 5`), the function must be
/// evaluated exactly once, not twice. Verifies output is `"r:15"` (not `"rr:15"`).
#[test]
fn test_class_property_compound_assign_evaluates_receiver_once() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public $value = 10;
}

function passthrough($counter) {
    echo "r";
    return $counter;
}

$counter = new Counter();
passthrough($counter)->value += 5;
echo ":" . $counter->value;
"#,
    );
    assert_eq!(out, "r:15");
}

/// Exercises `+=` and `>>=` compound assignment on an indexed class property
/// (`$bucket->items[1] += 6` and `$bucket->items[2] >>= 1`), verifying the
/// results are `4 + 6 = 10` and `8 >> 1 = 4`.
#[test]
fn test_class_property_array_compound_assign() {
    let out = compile_and_run(
        r#"<?php
class Bucket {
    public $items = [2, 4, 8];
}

$bucket = new Bucket();
$bucket->items[1] += 6;
$bucket->items[2] >>= 1;
echo $bucket->items[1] . "|" . $bucket->items[2];
"#,
    );
    assert_eq!(out, "10|4");
}

/// Regression test: when the receiver of an indexed compound property assignment
/// is a function call (`passthrough($bucket)->items[idx()] -= 3`), both the
/// receiver and the index expression must be evaluated exactly once each.
/// Verifies output is `"ri:5"` (not `"riri:5"` or similar).
#[test]
fn test_class_property_array_compound_assign_evaluates_receiver_and_index_once() {
    let out = compile_and_run(
        r#"<?php
class Bucket {
    public $items = [2, 4, 8];
}

function passthrough($bucket) {
    echo "r";
    return $bucket;
}

function idx() {
    echo "i";
    return 2;
}

$bucket = new Bucket();
passthrough($bucket)->items[idx()] -= 3;
echo ":" . $bucket->items[2];
"#,
    );
    assert_eq!(out, "ri:5");
}

/// Verifies that `??=` on a `readonly` property that has already been initialized
/// does not invoke the fallback expression and preserves the existing value (`7`).
#[test]
fn test_readonly_property_null_coalesce_assignment_keeps_initialized_value() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public readonly int $value;

    public function __construct() {
        $this->value = 7;
    }
}

function fallback() {
    echo "fallback";
    return 9;
}

$box = new Box();
$box->value ??= fallback();
echo $box->value;
"#,
    );
    assert_eq!(out, "7");
}
