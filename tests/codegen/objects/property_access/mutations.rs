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

/// Verifies runtime-name array append on a non-DatePeriod object through stdClass storage.
#[test]
fn test_dynamic_property_array_push_on_stdclass() {
    let out = compile_and_run(
        r#"<?php
$box = new stdClass();
$name = "items";
$box->$name = [1];
$box->$name[] = 2;
echo $box->items[0] . "," . $box->items[1];
"#,
    );
    assert_eq!(out, "1,2");
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

/// Verifies `unset($obj->prop)` on a declared (typed) property.
///
/// PHP leaves the property UNINITIALIZED rather than nulled: `isset()` answers false,
/// `print_r` omits it, reading it raises `Error: Typed property … must not be accessed
/// before initialization`, and assigning again brings it back. `unset($a, $b)` clears
/// both targets.
#[test]
fn test_unset_declared_typed_property_leaves_it_uninitialized() {
    let out = compile_and_run(
        r#"<?php
class T { public int $n = 3; public string $s = "x"; public array $a = [1, 2]; }
$t = new T();
unset($t->n, $t->s);
var_dump(isset($t->n), isset($t->s), isset($t->a));
print_r($t);
try { echo $t->n; } catch (\Error $e) { echo "ERR:", $e->getMessage(), "\n"; }
$t->n = 9;
var_dump(isset($t->n), $t->n);
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(false)\nbool(true)\n\
         T Object\n(\n    [a] => Array\n        (\n            [0] => 1\n            [1] => 2\n        )\n\n)\n\
         ERR:Typed property T::$n must not be accessed before initialization\n\
         bool(true)\nint(9)\n"
    );
}

/// Verifies `unset()` on a property the caller cannot see still routes to `__unset`.
///
/// PHP calls `__unset` only for an INACCESSIBLE (or absent) property; a property the
/// caller can see is removed directly and `__unset` is never consulted.
#[test]
fn test_unset_inaccessible_property_calls_magic_unset() {
    let out = compile_and_run(
        r#"<?php
class Pv {
    private $secret = 1;
    public int $open = 2;
    public function __unset($k) { echo "magic:$k\n"; }
}
$p = new Pv();
unset($p->secret);
unset($p->open);
var_dump(isset($p->open));
"#,
    );
    assert_eq!(out, "magic:secret\nbool(false)\n");
}

/// Verifies `unset($std->prop)` on a `stdClass` really REMOVES the dynamic property.
///
/// Every `stdClass` property is a hash entry, so PHP's removal semantics are exact here:
/// `isset()` answers false, `json_encode()` stops listing the key, unsetting the same key
/// again and unsetting a key that was never set are both no-ops, a later write re-appends
/// the key at the END of the property order, `unset($o->b, $o->c)` removes both, and a read
/// of the removed name answers null (observed through `??`, so the fixture does not depend
/// on the undefined-property warning elephc does not yet emit for `stdClass`).
///
/// Expected output is `LC_ALL=C php 8.4.20` verbatim. The fixture deliberately avoids
/// `var_dump($o)`/`print_r($o)`: elephc renders a `stdClass` body as empty regardless of
/// `unset()`, a separate pre-existing gap.
#[test]
fn test_unset_stdclass_dynamic_property_removes_it() {
    let out = compile_and_run(
        r#"<?php
$o = new stdClass();
$o->a = 1;
$o->b = "two";
$o->c = 3;
unset($o->a);
var_dump(isset($o->a), isset($o->b));
echo json_encode($o), "\n";
unset($o->a);
unset($o->never);
echo json_encode($o), "\n";
$o->a = 9;
echo json_encode($o), "\n";
echo $o->a, "|", $o->b, "\n";
unset($o->b, $o->c);
echo json_encode($o), "\n";
var_dump($o->b ?? "gone");
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(true)\n\
         {\"b\":\"two\",\"c\":3}\n\
         {\"b\":\"two\",\"c\":3}\n\
         {\"b\":\"two\",\"c\":3,\"a\":9}\n\
         9|two\n\
         {\"a\":9}\n\
         string(4) \"gone\"\n"
    );
}

/// Verifies `unset()` of an UNDECLARED name on an `#[AllowDynamicProperties]` class removes
/// the hash entry while leaving the class's fixed slots untouched.
///
/// The receiver mixes both storage shapes: `$fixed` is a declared typed slot and `$x`/`$y`
/// are dynamic hash entries. Unsetting the dynamic names must not disturb `$fixed`, and
/// repeat/absent unsets stay no-ops. Expected output is `LC_ALL=C php 8.4.20` verbatim.
#[test]
fn test_unset_dynamic_property_on_allow_dynamic_class() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class Bag { public int $fixed = 7; }
$b = new Bag();
$b->x = 1;
$b->y = "two";
unset($b->x);
var_dump(isset($b->x), isset($b->y), isset($b->fixed));
unset($b->x);
unset($b->missing);
$b->x = 5;
var_dump(isset($b->x));
echo $b->x, "|", $b->y, "|", $b->fixed, "\n";
unset($b->x, $b->y);
var_dump(isset($b->x), isset($b->y), isset($b->fixed));
echo $b->fixed, "\n";
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(true)\nbool(true)\n\
         bool(true)\n\
         5|two|7\n\
         bool(false)\nbool(false)\nbool(true)\n\
         7\n"
    );
}

/// Regression: repeatedly reading the SAME dynamic property must keep answering its value.
///
/// `__rt_hash_get` only borrows the stored `Mixed` cell, but the dynamic-property read
/// hands its result to a caller that releases it, so a missing retain made every read drop
/// a reference the program never took. After enough reads the live hash entry was freed and
/// further reads answered `NULL` — a use-after-free of the property's storage.
/// Expected output is `LC_ALL=C php 8.4.20` verbatim.
#[test]
fn test_repeated_dynamic_property_reads_keep_the_value_alive() {
    let out = compile_and_run(
        r#"<?php
#[AllowDynamicProperties]
class Slot {}
$s = new Slot();
$s->v = "kept";
echo $s->v, $s->v, $s->v, "\n";
var_dump($s->v, $s->v);
var_dump($s->v);
"#,
    );
    assert_eq!(
        out,
        "keptkeptkept\nstring(4) \"kept\"\nstring(4) \"kept\"\nstring(4) \"kept\"\n"
    );
}

/// Verifies `unset()` of an UNTYPED declared property is refused with a diagnostic that
/// names that shape, instead of silently leaving a stale value behind.
///
/// PHP genuinely removes such a property: a later read warns `Undefined property` and
/// answers `null`. elephc gives each declared property a fixed, monomorphically typed slot
/// (here `Int`), which has no encoding for "removed, and reading as null" — see
/// `docs/php/classes.md`. A loud compile error beats a wrong value.
#[test]
fn test_unset_untyped_declared_property_is_rejected() {
    let error = compile_source_expect_backend_error(
        r#"<?php
class M { public $foo = 1; }
$m = new M();
unset($m->foo);
echo "ok";
"#,
    );
    assert!(
        error.contains("An UNTYPED declared property"),
        "the diagnostic must name the untyped-property shape, got: {}",
        error
    );
}

/// Verifies `unset()` of a BY-REFERENCE property is refused rather than silently skipped.
///
/// The slot holds an object-owned ref-cell pointer that the destructor still frees and that
/// a later write would write THROUGH, reviving the alias PHP's `unset()` just broke. The
/// backend used to skip the shape quietly, which left `isset()` answering `true` after an
/// `unset()` where PHP answers `false`.
#[test]
fn test_unset_by_reference_property_is_rejected() {
    let error = compile_source_expect_backend_error(
        r#"<?php
class R { public function __construct(public int &$p) {} }
$v = 3;
$r = new R($v);
unset($r->p);
"#,
    );
    assert!(
        error.contains("unset() of by-reference property R::$p"),
        "the diagnostic must name the by-reference property, got: {}",
        error
    );
}

/// Verifies `unset()` of a dynamic name on a class that declares `__unset()` is refused.
///
/// PHP consults `__unset()` only when the dynamic property is ABSENT at the unset site and
/// removes the entry silently when it is present — a choice that depends on runtime state.
/// elephc picks the lowering statically, so it declines rather than guessing one of the two
/// behaviors.
#[test]
fn test_unset_dynamic_property_with_magic_unset_is_rejected() {
    let error = compile_source_expect_backend_error(
        r#"<?php
#[AllowDynamicProperties]
class Hooked { public function __unset($n) { echo "magic:$n\n"; } }
$h = new Hooked();
$h->a = 1;
unset($h->a);
"#,
    );
    assert!(
        error.contains("unset target shape"),
        "the runtime-dependent __unset shape must be refused, got: {}",
        error
    );
}

/// Verifies `isset()` on a never-initialized typed property answers false instead of
/// raising the uninitialized-read error, matching PHP.
#[test]
fn test_isset_on_uninitialized_typed_property_is_false() {
    let out = compile_and_run(
        r#"<?php
class U { public ?int $v; public int $w = 1; }
$u = new U();
var_dump(isset($u->v), isset($u->w));
$u->v = 5;
var_dump(isset($u->v), $u->v);
"#,
    );
    assert_eq!(out, "bool(false)\nbool(true)\nbool(true)\nint(5)\n");
}

/// Pins the `??=` container targets that reach the BORROWED-write scope, and the one that does
/// not.
///
/// `??=` writes its result temporary into the container and also hands it to the expression's
/// consumer, so the write only borrows it. A reviewer argued the borrow was unsound for a
/// target whose base is a temporary, and could not be settled by reading the code: the question
/// is which targets reach that scope at all. These three do — a fresh temporary base, a base
/// aliasing a live object, and a right-hand side that is itself an owned string — and each
/// matches `php -n`.
///
/// The refusal below is the other half. `mkArr()[2] ??= 5` is accepted by PHP and refused here,
/// so the accept set is bounded by a compile error rather than by an ownership argument. It is
/// pinned so that widening the set has to come through this test instead of silently opening
/// the borrowed-write scope to a shape nobody measured.
#[test]
fn test_coalesce_assign_borrowed_write_targets() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public array $a = [1, 2];
}
$shared = new Bag();
function mkShared(): Bag {
    global $shared;
    return $shared;
}
function mk(): Bag {
    return new Bag();
}
mkShared()->a[2] ??= 5;
mkShared()->a[3] ??= "o" . "wned";
mkShared()->a[1] ??= 9;
mk()->a[2] ??= 5;
echo implode(",", $shared->a);
"#,
    );
    assert_eq!(out, "1,2,5,owned");
}

/// The bound on that accept set: an element write through a call result the compiler cannot
/// place is refused, where PHP accepts it and prints 5. An over-rejection, not a wrong answer —
/// but it is the reason the shapes above are the whole of what the borrowed-write scope sees.
#[test]
fn test_coalesce_assign_on_a_returned_array_is_refused() {
    let error = compile_expect_type_error(
        r#"<?php
function mkArr(): array {
    return [1, 2];
}
mkArr()[2] ??= 5;
echo "unreachable";
"#,
    );
    assert!(
        error.contains("Nested array assignment requires a Mixed or ArrayAccess target"),
        "expected the nested-assignment refusal, got: {}",
        error
    );
}
