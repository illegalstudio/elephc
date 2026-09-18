//! Purpose:
//! Integration regressions for PHP 8.5 weak-mode typed-property assignment when the assigned
//! value is only known as a runtime `mixed`, covering accepted coercions, catchable
//! `TypeError` rejections, nullable and union targets, class constraints, and ownership.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every fixture routes its value through a `mixed`-returning function so the value shape is
//!   decided at run time, which is the only way to reach the weak-mode guard.
//! - Expected outputs are the reference PHP 8.5 outputs for the same program.

use super::*;

/// Verifies the scalar coercions PHP performs for a typed property write.
#[test]
fn test_mixed_value_coerces_into_scalar_typed_properties() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $n = 0;
    public float $f = 0.0;
    public string $s = "";
    public bool $b = false;
}

function pick(string $k): mixed {
    if ($k === "numeric") { return "12"; }
    if ($k === "float") { return 3.0; }
    if ($k === "int") { return 7; }
    return true;
}

$box = new Box();
$box->n = pick("numeric");
$box->f = pick("numeric");
$box->s = pick("int");
$box->b = pick("int");
echo $box->n, "|", $box->f, "|", $box->s, "|", $box->b ? "y" : "n", "\n";
$box->n = pick("float");
$box->f = pick("int");
$box->s = pick("float");
echo $box->n, "|", $box->f, "|", $box->s;
"#,
    );
    assert_eq!(out, "12|12|7|y\n3|7|3");
}

/// Verifies that values PHP refuses for a typed property raise a catchable `TypeError`
/// naming the runtime source type and the target class, property, and declared type.
#[test]
fn test_mixed_value_rejected_by_typed_property_throws_type_error() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $n = 1;
    public string $s = "keep";
}

function pick(string $k): mixed {
    if ($k === "words") { return "abc"; }
    if ($k === "list") { return [1, 2]; }
    return null;
}

$box = new Box();
foreach (["words", "list", "null"] as $k) {
    try {
        $box->n = pick($k);
        echo "accepted\n";
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
try {
    $box->s = pick("list");
    echo "accepted\n";
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
echo $box->n, "|", $box->s;
"#,
    );
    assert_eq!(
        out,
        "Cannot assign string to property Box::$n of type int\n\
Cannot assign array to property Box::$n of type int\n\
Cannot assign null to property Box::$n of type int\n\
Cannot assign array to property Box::$s of type string\n\
1|keep"
    );
}

/// Verifies nullable typed properties accept null and narrow other scalars, and still reject
/// values PHP rejects.
#[test]
fn test_mixed_value_into_nullable_int_property() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public ?int $n = 5;
}

function pick(string $k): mixed {
    if ($k === "null") { return null; }
    if ($k === "numeric") { return "12"; }
    if ($k === "list") { return [1]; }
    return 4.0;
}

$box = new Box();
$box->n = pick("null");
echo ($box->n === null) ? "null" : "value", "\n";
$box->n = pick("numeric");
echo $box->n, "\n";
$box->n = pick("float");
echo $box->n, "\n";
try {
    $box->n = pick("list");
    echo "accepted\n";
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
echo $box->n;
"#,
    );
    assert_eq!(
        out,
        "null\n12\n4\nCannot assign array to property Box::$n of type ?int\n4"
    );
}

/// Verifies a union-typed property keeps an exact member unchanged and rejects a member it
/// does not declare.
#[test]
fn test_mixed_value_into_union_typed_property() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int|string $u = 0;
}

function pick(string $k): mixed {
    if ($k === "words") { return "abc"; }
    if ($k === "int") { return 9; }
    return [1];
}

$box = new Box();
$box->u = pick("words");
echo $box->u, "|", is_string($box->u) ? "string" : "other", "\n";
$box->u = pick("int");
echo $box->u, "|", is_int($box->u) ? "int" : "other", "\n";
try {
    $box->u = pick("list");
    echo "accepted\n";
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
echo $box->u;
"#,
    );
    assert_eq!(
        out,
        "abc|string\n9|int\nCannot assign array to property Box::$u of type string|int\n9"
    );
}

/// Verifies an `array` typed property accepts both runtime container shapes and rejects a
/// scalar, which is the case the declared-slot dispatch used to drop entirely.
#[test]
fn test_mixed_value_into_array_typed_property() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public array $items = [];
}

function pick(string $k): mixed {
    if ($k === "list") { return [1, 2, 3]; }
    if ($k === "hash") { return ["a" => 1]; }
    return "nope";
}

$box = new Box();
$box->items = pick("list");
echo count($box->items), "\n";
$box->items = pick("hash");
echo count($box->items), "\n";
try {
    $box->items = pick("scalar");
    echo "accepted\n";
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
echo count($box->items);
"#,
    );
    assert_eq!(
        out,
        "3\n1\nCannot assign string to property Box::$items of type array\n1"
    );
}

/// Verifies class-typed properties accept subclasses and reject unrelated classes and null.
#[test]
fn test_mixed_object_into_class_typed_property() {
    let out = compile_and_run(
        r#"<?php
class Animal {
    public string $name = "animal";
}
class Dog extends Animal {
    public string $name = "dog";
}
class Rock {
}

class Pen {
    public Animal $pet;
}

function pick(string $k): mixed {
    if ($k === "dog") { return new Dog(); }
    if ($k === "rock") { return new Rock(); }
    return null;
}

$pen = new Pen();
$pen->pet = pick("dog");
echo $pen->pet->name, "\n";
foreach (["rock", "null"] as $k) {
    try {
        $pen->pet = pick($k);
        echo "accepted\n";
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
echo $pen->pet->name;
"#,
    );
    assert_eq!(
        out,
        "dog\nCannot assign Rock to property Pen::$pet of type Animal\n\
Cannot assign null to property Pen::$pet of type Animal\ndog"
    );
}

/// Verifies repeated string and array writes through the guard release the previous slot
/// contents exactly once and keep the surviving value intact after a rejected write.
#[test]
fn test_weak_typed_property_writes_preserve_ownership() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public string $s = "start";
    public array $items = [];
}

function pick(int $i): mixed {
    if ($i % 3 === 0) { return "value-" . $i; }
    if ($i % 3 === 1) { return $i; }
    return [$i, $i + 1];
}

$box = new Box();
$rejected = 0;
for ($i = 0; $i < 30; $i++) {
    try {
        $box->s = pick($i);
    } catch (TypeError $e) {
        $rejected++;
    }
    try {
        $box->items = pick($i);
    } catch (TypeError $e) {
        $rejected++;
    }
}
echo $box->s, "|", count($box->items), "|", $rejected;
"#,
    );
    assert_eq!(out, "28|2|30");
}

/// Verifies a static-name write through a `mixed` receiver reaches the declared slot of
/// whichever class the receiver actually holds, enforcing each class's own declared type for
/// the same property name.
#[test]
fn test_same_property_name_with_different_declared_types_on_mixed_receiver() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public int $value = 0;
}
class Label {
    public string $value = "";
}

function make(bool $counter): mixed {
    if ($counter) { return new Counter(); }
    return new Label();
}

function assign(mixed $target, mixed $value): string {
    try {
        $target->value = $value;
        return "ok";
    } catch (TypeError $e) {
        return $e->getMessage();
    }
}

$counter = make(true);
$label = make(false);
echo assign($counter, "41"), "|", $counter->value, "\n";
echo assign($label, 41), "|", $label->value, "\n";
echo assign($counter, [1]), "\n";
echo assign($label, [1]);
"#,
    );
    assert_eq!(
        out,
        "ok|41\nok|41\nCannot assign array to property Counter::$value of type int\n\
Cannot assign array to property Label::$value of type string"
    );
}

/// Verifies a rejected object names its RUNTIME class in the `TypeError`, for every declared
/// destination shape, exactly as php-src words it.
///
/// php-src never prints the word `object` here: `Cannot assign T to property ...`. The class is
/// only known at run time, so the message is composed from the same dense class-name table
/// `get_class()` reads.
#[test]
fn test_rejected_object_type_error_names_the_runtime_class() {
    let out = compile_and_run(
        r#"<?php
class Alpha {}
class Beta {}
class Box {
    public int $n = 1;
    public string $s = "keep";
    public array $a = [];
    public int|string $u = 0;
}

function pick(bool $alpha): mixed {
    if ($alpha) { return new Alpha(); }
    return new Beta();
}

$box = new Box();
foreach ([true, false] as $alpha) {
    foreach (["n", "s", "a", "u"] as $name) {
        try {
            $box->$name = pick($alpha);
            echo "accepted\n";
        } catch (TypeError $e) {
            echo $e->getMessage(), "\n";
        }
    }
}
"#,
    );
    assert_eq!(
        out,
        "Cannot assign Alpha to property Box::$n of type int\n\
Cannot assign Alpha to property Box::$s of type string\n\
Cannot assign Alpha to property Box::$a of type array\n\
Cannot assign Alpha to property Box::$u of type string|int\n\
Cannot assign Beta to property Box::$n of type int\n\
Cannot assign Beta to property Box::$s of type string\n\
Cannot assign Beta to property Box::$a of type array\n\
Cannot assign Beta to property Box::$u of type string|int\n"
    );
}

/// Verifies union spellings in a `TypeError` use PHP's canonical order, not the declared order.
///
/// `zend_type_to_string` walks a fixed type mask instead of the declaration, so `int|string` and
/// `string|int` both print as `string|int`, and `null` always comes last. Every expectation here
/// is the exact PHP 8.5 message for the same program.
#[test]
fn test_union_type_error_spelling_uses_php_canonical_order() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int|string $a = 0;
    public string|int $b = 0;
    public int|float $c = 0;
    public float|int $d = 0;
    public ?string $f = null;
    public array|string $g = "";
}

function pick(): mixed { return [1, 2]; }

$box = new Box();
foreach (["a", "b", "c", "d", "f", "g"] as $name) {
    try {
        $box->$name = pick();
        echo "accepted\n";
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
"#,
    );
    assert_eq!(
        out,
        "Cannot assign array to property Box::$a of type string|int\n\
Cannot assign array to property Box::$b of type string|int\n\
Cannot assign array to property Box::$c of type int|float\n\
Cannot assign array to property Box::$d of type int|float\n\
Cannot assign array to property Box::$f of type ?string\n\
accepted\n"
    );
}

/// Verifies an object publishing `__toString` is accepted by a string-shaped property, and that
/// a `__toString` that throws leaves the destination untouched.
///
/// PHP accepts a Stringable for `string` AND for any union that declares `string`. When the call
/// throws, the assignment never happens: the previous value is still there afterwards.
#[test]
fn test_stringable_object_into_string_shaped_properties() {
    let out = compile_and_run(
        r#"<?php
class Label {
    public string $text = "";
    public function __construct(string $text) { $this->text = $text; }
    public function __toString(): string { return "label-" . $this->text; }
}
class Boom {
    public function __toString(): string { throw new RuntimeException("no string"); }
}
class Plain {}
class Box {
    public string $s = "start";
    public int|string $u = 0;
    public ?string $n = null;
}

function pick(mixed $value): mixed { return $value; }

$box = new Box();
$box->s = pick(new Label("a"));
$box->u = pick(new Label("b"));
$box->n = pick(new Label("c"));
echo $box->s, "|", $box->u, "|", $box->n, "\n";
try {
    $box->s = pick(new Boom());
    echo "accepted\n";
} catch (RuntimeException $e) {
    echo "caught ", $e->getMessage(), "\n";
}
echo $box->s, "\n";
try {
    $box->s = pick(new Plain());
    echo "accepted\n";
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
echo $box->s;
"#,
    );
    assert_eq!(
        out,
        "label-a|label-b|label-c\n\
caught no string\n\
label-a\n\
Cannot assign Plain to property Box::$s of type string\n\
label-a"
    );
}

/// Verifies repeated Stringable and coercing writes leave no allocation behind.
///
/// `__toString` hands back an owned string and the boxed source is one EIR expects the slot to
/// adopt, so both have to be released by the arm that stores something else. Each of those
/// leaked once per write before the releases existed, which only a heap-debug run can see.
///
/// The `int|float` arm has the same obligation: it boxes the value the runtime classifier
/// parsed, so the source box it did not hand over has to be released too, and the classifier
/// itself borrows a length-sized copy of the string that it must give back.
#[test]
fn test_weak_typed_property_writes_leave_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Label {
    public string $text = "x";
    public function __toString(): string { return "label-" . $this->text; }
}
class Box {
    public string $s = "start";
    public int|string $u = 0;
    public int|float $n = 0;
    public ?int $q = null;
    public mixed $m = 0;
}

function pick(mixed $value): mixed { return $value; }

$box = new Box();
for ($i = 0; $i < 40; $i++) {
    $box->s = pick(new Label());
    $box->s = pick($i);
    $box->u = pick(new Label());
    $box->u = pick(true);
    $box->n = pick("7");
    $box->n = pick("7.5");
    $box->q = pick("7");
    $box->q = pick(2.0);
    $box->q = pick(null);
    $box->m = pick([$i]);
}
echo $box->s, "|", $box->u, "|", count($box->m), "|", $box->n, "|", is_float($box->n) ? "f" : "i";
"#,
    );
    assert_eq!(out.stdout, "39|1|1|7.5|f");
    assert!(
        out.stderr.contains("leak summary: clean"),
        "expected a clean heap, got:\n{}",
        out.stderr
    );
}

/// Verifies a declared object property releases the Mixed box it was handed ownership of.
///
/// A runtime-shaped write to an object slot unboxes the payload and retains the OBJECT on its
/// own, so the cell EIR expects this consumer to adopt keeps no owner afterwards. Without that
/// release the slot leaked one boxed cell and one payload reference per accepted write, which
/// is why the loop repeats the assignment instead of writing once.
#[test]
fn test_declared_object_property_releases_adopted_mixed_box() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Animal {
    public string $name = "animal";
}
class Dog extends Animal {
    public string $name = "dog";
}
class Pen {
    public Animal $pet;
}

function pick(mixed $value): mixed { return $value; }

$pen = new Pen();
for ($i = 0; $i < 40; $i++) {
    $pen->pet = pick(new Animal());
    $pen->pet = pick(new Dog());
}
echo $pen->pet->name, "|", $i;
"#,
    );
    assert!(
        out.success,
        "stdout={:?}\nstderr={}",
        out.stdout, out.stderr
    );
    assert_eq!(out.stdout, "dog|40");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got:\n{}",
        out.stderr
    );
}

/// Verifies PHP 8.5's implicit int-conversion diagnostics for a typed property write.
///
/// A lossy float is DEPRECATED and stored truncated, an exact one is silent, and a float PHP
/// cannot represent as an int is a `TypeError` rather than a wrapped integer. The float-string
/// deprecation quotes the original string verbatim, which is why the source bytes survive the
/// numeric conversion.
#[test]
fn test_lossy_numeric_assignment_to_int_property_deprecates() {
    let out = compile_and_run_capture(
        r#"<?php
class Box { public int $n = 0; }

function pick(string $which): mixed {
    if ($which === "lossy") { return 3.7; }
    if ($which === "negative") { return -2.5; }
    if ($which === "exact") { return 4.0; }
    if ($which === "lossy_string") { return "2.5"; }
    if ($which === "exact_string") { return "6"; }
    if ($which === "huge") { return 1e30; }
    if ($which === "nan") { return NAN; }
    return INF;
}

$box = new Box();
foreach (["lossy", "negative", "exact", "lossy_string", "exact_string"] as $which) {
    $box->n = pick($which);
    echo $box->n, "\n";
}
foreach (["huge", "nan", "inf"] as $which) {
    try {
        $box->n = pick($which);
        echo "accepted\n";
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
echo $box->n;
"#,
    );
    assert_eq!(
        out.stdout,
        "3\n-2\n4\n2\n6\n\
Cannot assign float to property Box::$n of type int\n\
Cannot assign float to property Box::$n of type int\n\
Cannot assign float to property Box::$n of type int\n6"
    );
    assert_eq!(
        out.stderr,
        "Deprecated: Implicit conversion from float 3.7 to int loses precision\n\
Deprecated: Implicit conversion from float -2.5 to int loses precision\n\
Deprecated: Implicit conversion from float-string \"2.5\" to int loses precision\n"
    );
}

/// Verifies PHP 8's leading-numeric string rule for a numeric typed property.
///
/// `"5abc"` is NOT accepted with its numeric prefix: since the saner-string-to-number change a
/// typed destination refuses it outright, and only a fully numeric string (PHP whitespace on
/// either side allowed, exponents included) is converted.
#[test]
fn test_leading_numeric_strings_are_refused_by_numeric_properties() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $n = 0;
    public float $f = 0.0;
}

function pick(string $which): mixed {
    if ($which === "leading") { return "5abc"; }
    if ($which === "trailing") { return "12 "; }
    if ($which === "hex") { return "0x1A"; }
    if ($which === "exponent") { return "1e3"; }
    if ($which === "empty") { return ""; }
    return "  8  ";
}

$box = new Box();
foreach (["leading", "trailing", "hex", "exponent", "empty", "padded"] as $which) {
    try {
        $box->n = pick($which);
        echo $box->n, "\n";
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
try {
    $box->f = pick("leading");
    echo "accepted\n";
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
$box->f = pick("exponent");
echo $box->f;
"#,
    );
    assert_eq!(
        out,
        "Cannot assign string to property Box::$n of type int\n\
12\n\
Cannot assign string to property Box::$n of type int\n\
1000\n\
Cannot assign string to property Box::$n of type int\n\
8\n\
Cannot assign string to property Box::$f of type float\n\
1000"
    );
}

/// Verifies the declared types the guard had no plan for at all: `mixed` and the bare `object`.
///
/// A `mixed` slot holds every PHP value, and answering "no plan" for it made the runtime-class
/// dispatch drop the class and lose the write silently instead of accepting it. A bare `object`
/// accepts every class and refuses every non-object.
#[test]
fn test_mixed_and_bare_object_declared_properties_accept_runtime_values() {
    let out = compile_and_run(
        r#"<?php
class Alpha {}
class Beta {}
class Box {
    public mixed $m = 0;
    public object $o;
}

function pick(string $which): mixed {
    if ($which === "alpha") { return new Alpha(); }
    if ($which === "beta") { return new Beta(); }
    if ($which === "array") { return [1, 2, 3]; }
    if ($which === "string") { return "text"; }
    if ($which === "null") { return null; }
    return 9;
}

$box = new Box();
foreach (["alpha", "array", "string", "null", "int"] as $which) {
    $box->m = pick($which);
    echo gettype($box->m), "\n";
}
$box->o = pick("alpha");
echo get_class($box->o), "\n";
$box->o = pick("beta");
echo get_class($box->o), "\n";
foreach (["array", "string", "null", "int"] as $which) {
    try {
        $box->o = pick($which);
        echo "accepted\n";
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
echo get_class($box->o);
"#,
    );
    assert_eq!(
        out,
        "object\narray\nstring\nNULL\ninteger\n\
Alpha\nBeta\n\
Cannot assign array to property Box::$o of type object\n\
Cannot assign string to property Box::$o of type object\n\
Cannot assign null to property Box::$o of type object\n\
Cannot assign int to property Box::$o of type object\n\
Beta"
    );
}

/// Verifies a declared `iterable` property accepts exactly what PHP's `array|Traversable` accepts,
/// through a plain assignment rather than through `clone()`.
///
/// `iterable` had no declared-property slot contract at all: even the fully static `$box->it = [1]`
/// failed EIR lowering, so neither a static nor a runtime-shaped write could reach the slot. Both
/// halves are checked here because the acceptance lives in the SHARED instance-property
/// compatibility and store path, not in one caller. Expected output follows PHP 8.5 semantics; no
/// reference `php` command was run to produce it.
#[test]
fn test_declared_iterable_property_accepts_arrays_and_traversable_writes() {
    let out = compile_and_run(
        r#"<?php
class Range implements Iterator {
    private int $current = 0;
    public function rewind(): void { $this->current = 0; }
    public function valid(): bool { return $this->current < 2; }
    public function current(): int { return $this->current; }
    public function key(): int { return $this->current; }
    public function next(): void { $this->current = $this->current + 1; }
}
class Plain { public int $v = 1; }
class Box { public iterable $it; }
function pick(mixed $value): mixed { return $value; }
function dump(iterable $items): void {
    foreach ($items as $k => $v) {
        echo $k;
        echo '=';
        echo $v;
        echo ';';
    }
    echo '|';
}
$box = new Box();
$box->it = [10, 20];
dump($box->it);
$box->it = ["a" => 1];
dump($box->it);
$box->it = new Range();
dump($box->it);
$box->it = pick([30, 40]);
dump($box->it);
$box->it = pick(["b" => 2]);
dump($box->it);
$box->it = pick(new Range());
dump($box->it);
try { $box->it = pick(5); } catch (TypeError $e) { echo $e->getMessage() . ";"; }
try { $box->it = pick("x"); } catch (TypeError $e) { echo $e->getMessage() . ";"; }
try { $box->it = pick(new Plain()); } catch (TypeError $e) { echo $e->getMessage() . ";"; }
dump($box->it);
"#,
    );
    assert_eq!(
        out,
        "0=10;1=20;|a=1;|0=0;1=1;|0=30;1=40;|b=2;|0=0;1=1;|\
Cannot assign int to property Box::$it of type Traversable|array;\
Cannot assign string to property Box::$it of type Traversable|array;\
Cannot assign Plain to property Box::$it of type Traversable|array;\
0=0;1=1;|"
    );
}

/// Pins the declared property types this backend does NOT support a runtime-shaped write for.
///
/// This is the recorded decision behind the guard's `None` answer for them, and the reason it is
/// a refusal rather than a silently dropped write: `ptr<T>` is an elephc systems extension whose
/// slot holds a raw address, so turning its write into a runtime coercion would hide the
/// compile-time error the contract exists to raise.
#[test]
fn test_declared_types_without_a_runtime_shaped_write_are_refused_at_compile_time() {
    let pointer = compile_source_expect_backend_error(
        r#"<?php
class Box { public ptr<int> $p; }
function pick(mixed $value): mixed { return $value; }
$box = new Box();
$box->p = pick(1);
echo "unreachable";
"#,
    );
    assert!(
        pointer.contains("prop_set") && pointer.contains("Pointer"),
        "a ptr<T> property write must be refused by the backend, got: {pointer}"
    );
}

/// Verifies a refused bool is named BY VALUE, and that a union falls through to its next member
/// when the preferred one cannot represent the value.
///
/// php-src prints `true`/`false` rather than `bool` in this `TypeError`, the same way `count()`
/// does. And a union does not stop at its preferred member: `1e30` is outside the int range, so
/// `int|string` stores the STRING and `int|bool` stores `true`, instead of raising.
#[test]
fn test_bool_naming_and_union_member_fallback_match_php() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public array $a = [];
    public object $o;
    public int|string $u = 0;
    public int|bool $ib = 0;
    public int $n = 0;
}

function pick(string $which): mixed {
    if ($which === "true") { return true; }
    if ($which === "false") { return false; }
    return 1e30;
}

$box = new Box();
foreach (["true", "false"] as $which) {
    foreach (["a", "o"] as $name) {
        try {
            $box->$name = pick($which);
            echo "accepted\n";
        } catch (TypeError $e) {
            echo $e->getMessage(), "\n";
        }
    }
}
$box->u = pick("huge");
echo $box->u, "|", is_string($box->u) ? "string" : "other", "\n";
$box->ib = pick("huge");
echo $box->ib ? "true" : "false", "|", is_bool($box->ib) ? "bool" : "other", "\n";
try {
    $box->n = pick("huge");
    echo "accepted\n";
} catch (TypeError $e) {
    echo $e->getMessage();
}
"#,
    );
    assert_eq!(
        out,
        "Cannot assign true to property Box::$a of type array\n\
Cannot assign true to property Box::$o of type object\n\
Cannot assign false to property Box::$a of type array\n\
Cannot assign false to property Box::$o of type object\n\
1.0E+30|string\n\
true|bool\n\
Cannot assign float to property Box::$n of type int"
    );
}

/// Verifies a union declaring BOTH `int` and `float` picks its member from the numeric string's
/// own spelling, the way php-src's `is_numeric_string` does.
///
/// The union does not simply prefer `int`: `"2"` is an integer spelling, while `"2.0"`, `"2.5"`
/// and every exponent form classify as double, and an integer-spelled value the platform int
/// range cannot hold classifies as double too. `PHP_INT_MAX` still lands on `int` and one more
/// than it lands on `float`, which is the boundary a byte scan for `.`/`e` would get wrong.
/// Every expectation is the reference PHP 8.5 output for the same program.
#[test]
fn test_numeric_string_into_int_or_float_union_picks_the_php_member() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int|float $v = 0;
    public int|float|bool $w = 0;
}

function pick(string $spelling): mixed { return $spelling; }

$box = new Box();
foreach ([
    "2", "+2", "-2", " 12 ", "2.0", "2.5", "1e3", "1e-3",
    "9223372036854775807", "9223372036854775808",
    "-9223372036854775808", "-9223372036854775809",
] as $spelling) {
    $box->v = pick($spelling);
    echo is_int($box->v) ? "int" : "float", "=", $box->v, "\n";
}
foreach (["5abc", "0x1A", "", " "] as $spelling) {
    try {
        $box->v = pick($spelling);
        echo "accepted\n";
    } catch (TypeError $e) {
        echo $e->getMessage(), "\n";
    }
}
$box->w = pick("7");
echo is_int($box->w) ? "int" : "float", "=", $box->w, "\n";
$box->w = pick("7.5");
echo is_int($box->w) ? "int" : "float", "=", $box->w, "\n";
echo is_float($box->v) ? "float" : "other", "=", $box->v;
"#,
    );
    assert_eq!(
        out,
        "int=2\nint=2\nint=-2\nint=12\n\
float=2\nfloat=2.5\nfloat=1000\nfloat=0.001\n\
int=9223372036854775807\nfloat=9.2233720368548E+18\n\
int=-9223372036854775808\nfloat=-9.2233720368548E+18\n\
Cannot assign string to property Box::$v of type int|float\n\
Cannot assign string to property Box::$v of type int|float\n\
Cannot assign string to property Box::$v of type int|float\n\
Cannot assign string to property Box::$v of type int|float\n\
int=7\nfloat=7.5\n\
float=-9.2233720368548E+18"
    );
}
