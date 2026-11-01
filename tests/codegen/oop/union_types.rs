//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object-oriented PHP union types, including union typed local gettype and reassignment, nullable typed local null coalesce, and union typed local truthiness dispatch.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Verifies a method call on a union of two INTERFACES resolves through the classes that implement
/// them, even when neither interface declares the method. A union of interfaces describes a
/// runtime object that may carry members none of them names, and codegen already dispatches such a
/// receiver on the runtime class id — but the checker refused the call outright with "Nullsafe
/// method call requires a single nullable object type". Symfony's `Router` declares
/// `protected UrlMatcherInterface|RequestMatcherInterface $matcher;` and calls
/// `addExpressionLanguageProvider()` on it behind a `method_exists()` guard.
/// PHP outputs "add:x;u:/p;ok".
#[test]
fn test_method_call_on_a_union_of_two_interfaces() {
    let out = compile_and_run(
        r#"<?php
interface UrlMatcher { public function match(string $path): string; }
interface RequestMatcher { public function matchRequest(string $path): string; }

class Compiled implements UrlMatcher, RequestMatcher {
    public function match(string $path): string { return "u:$path"; }
    public function matchRequest(string $path): string { return "r:$path"; }
    public function addProvider(string $p): void { echo "add:$p;"; }
}

class Router {
    protected UrlMatcher|RequestMatcher $matcher;

    public function getMatcher(): UrlMatcher|RequestMatcher {
        if (isset($this->matcher)) {
            return $this->matcher;
        }

        $this->matcher = new Compiled();
        if (method_exists($this->matcher, 'addProvider')) {
            $this->matcher->addProvider('x');
        }

        return $this->matcher;
    }
}

$router = new Router();
$matcher = $router->getMatcher();
echo $matcher->match('/p'), '|';
echo $matcher instanceof UrlMatcher ? 'ok' : 'bad';
"#,
    );
    assert_eq!(out, "add:x;u:/p|ok");
}

/// Verifies that gettype() reflects the narrowest runtime type of a union-typed local,
/// and that reassignment to an alternate union member updates the reported type.
/// Fixture: `int|string $value = 1` → gettype returns "integer", then `$value = "two"` → gettype returns "string".
#[test]
fn test_union_typed_local_gettype_and_reassignment() {
    let out = compile_and_run(
        r#"<?php
function demo() {
    int|string $value = 1;
    echo gettype($value);
    echo ":";
    $value = "two";
    echo gettype($value);
    echo ":";
    echo $value;
}

demo();
"#,
    );
    assert_eq!(out, "integer:string:two");
}

/// Verifies null coalesce on a nullable-typed local: fallback is used when the value is null,
/// and the actual value is returned after assignment to a non-null int.
/// Fixture: `?int $value = null` → `?? 41` yields 41; then `$value = 1` → `?? 41` yields 1.
#[test]
fn test_nullable_typed_local_null_coalesce() {
    let out = compile_and_run(
        r#"<?php
function demo() {
    ?int $value = null;
    echo $value ?? 41;
    $value = 1;
    echo $value ?? 41;
}

demo();
"#,
    );
    assert_eq!(out, "411");
}

/// Verifies a coalesce between compatible object classes retains their common interface, so a
/// chained method call uses the interface's declared string return type on either runtime arm.
#[test]
fn test_compatible_object_null_coalesce_preserves_interface_method_type() {
    let out = compile_and_run(
        r#"<?php
interface NamedPath {
    public function name(): string;
}

class RootPath implements NamedPath {
    public function name(): string { return "root"; }
}

class ChildPath implements NamedPath {
    public function __construct(private ?NamedPath $parent = null) {}
    public function name(): string { return "child"; }
    public function selectedName(): string {
        return ($this->parent ?? $this)->name();
    }
}

echo (new ChildPath())->selectedName();
echo ":";
echo (new ChildPath(new RootPath()))->selectedName();
"#,
    );
    assert_eq!(out, "child:root");
}

/// Verifies every array member of a flow union satisfies an `iterable` parameter, including the
/// empty-array arm retained after a conditional append.
#[test]
fn test_array_flow_union_binds_to_iterable_parameter() {
    let out = compile_and_run(
        r#"<?php
class FlowItem {}
function countFlowItems(iterable $items): int { return count($items); }
$items = [];
if ($argc > 1) {
    $items[] = new FlowItem();
}
echo countFlowItems($items);
"#,
    );
    assert_eq!(out, "0");
}

/// Verifies truthiness dispatch for a union-typed local: string "0" is falsy, int 7 is truthy.
/// Regression: ensures the codegen emits correct branch logic for both string and int payloads.
#[test]
fn test_union_typed_local_truthiness_dispatch() {
    let out = compile_and_run(
        r#"<?php
function demo() {
    int|string $value = "0";
    if ($value) {
        echo 1;
    } else {
        echo 0;
    }
    $value = 7;
    if ($value) {
        echo 1;
    } else {
        echo 0;
    }
}

demo();
"#,
    );
    assert_eq!(out, "01");
}

/// Verifies empty() dispatch for a union-typed local: string "0" is empty, string "7" is not.
/// Regression: ensures empty() correctly distinguishes falsy-but-non-empty strings from empty strings.
#[test]
fn test_union_typed_local_empty_dispatch() {
    let out = compile_and_run(
        r#"<?php
function demo() {
    int|string $value = "0";
    echo empty($value) ? 1 : 0;
    $value = "7";
    echo empty($value) ? 1 : 0;
}

demo();
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies that a union-typed property accepts an integer literal default, boxing it into the
/// property's Mixed storage. Regression: this previously failed codegen with an "object_new for
/// default value ... Union" error.
#[test]
fn test_union_property_int_literal_default() {
    let out = compile_and_run(
        "<?php class C { public int|string $v = 1; } $c = new C(); var_dump($c->v);",
    );
    assert_eq!(out, "int(1)\n");
}

/// Verifies that a union-typed property accepts a negative integer literal default.
#[test]
fn test_union_property_negative_int_default() {
    let out = compile_and_run(
        "<?php class C { public int|string $v = -7; } $c = new C(); var_dump($c->v);",
    );
    assert_eq!(out, "int(-7)\n");
}

/// Verifies that a union-typed property accepts a float literal default.
#[test]
fn test_union_property_float_literal_default() {
    let out = compile_and_run(
        "<?php class C { public float|int $v = 1.5; } $c = new C(); var_dump($c->v);",
    );
    assert_eq!(out, "float(1.5)\n");
}

/// Verifies that a union-typed property accepts a boolean literal default.
#[test]
fn test_union_property_bool_literal_default() {
    let out = compile_and_run(
        "<?php class C { public bool|int $v = true; } $c = new C(); var_dump($c->v);",
    );
    assert_eq!(out, "bool(true)\n");
}

/// Verifies that a string literal default for a union-typed property still works (it did before
/// this fix), exercising the sibling boxed-string path.
#[test]
fn test_union_property_string_literal_default() {
    let out = compile_and_run(
        "<?php class C { public string|int $v = \"hi\"; } $c = new C(); var_dump($c->v);",
    );
    assert_eq!(out, "string(2) \"hi\"\n");
}

/// Verifies method dispatch on an object-or-false union return (`A|false`): the
/// object branch dispatches the method on the single object class. Mirrors the
/// `DateTime::createFromFormat()` pattern (returns `DateTime|false`).
#[test]
fn test_object_false_union_method_dispatch() {
    let out = compile_and_run(
        r#"<?php
class A { public int $v = 5; function who(): string { return "A"; } }
function make(bool $b): A|false { return $b ? new A() : false; }
echo make(true)->who(), make(true)->v;
"#,
    );
    assert_eq!(out, "A5");
}

/// Verifies property and method access on a nullable object union (`A|null`)
/// still works after the union-receiver relaxation (regression guard).
#[test]
fn test_object_null_union_access() {
    let out = compile_and_run(
        r#"<?php
class A { public int $v = 7; function who(): string { return "A"; } }
function make(bool $b): A|null { return $b ? new A() : null; }
echo make(true)->who(), make(true)->v;
"#,
    );
    assert_eq!(out, "A7");
}

/// Verifies runtime class-id dispatch on a union of two distinct object classes
/// (`A|B`): the method and property resolve to whichever class the value holds.
#[test]
fn test_two_class_union_method_dispatch() {
    let out = compile_and_run(
        r#"<?php
class A { public int $v = 1; function who(): string { return "A"; } }
class B { public int $v = 2; function who(): string { return "B"; } }
function make(bool $b): A|B { return $b ? new A() : new B(); }
echo make(true)->who(), make(false)->who(), make(true)->v, make(false)->v;
"#,
    );
    assert_eq!(out, "AB12");
}

/// Verifies a three-member object union with a scalar sentinel (`A|B|false`)
/// dispatches the object branches by runtime class id.
#[test]
fn test_two_class_false_union_dispatch() {
    let out = compile_and_run(
        r#"<?php
class A { public int $v = 1; function who(): string { return "A"; } }
class B { public int $v = 2; function who(): string { return "B"; } }
function make(int $w): A|B|false { if ($w == 0) return new A(); if ($w == 1) return new B(); return false; }
echo make(0)->who(), make(1)->who(), make(0)->v, make(1)->v;
"#,
    );
    assert_eq!(out, "AB12");
}

/// Regression: a `mixed`/union-typed property accepts a scalar literal *default* (int, bool, float)
/// at object construction, boxing it into a `Mixed` cell. Previously `object_new` raised
/// "unsupported EIR backend feature: object_new for default value of property ... with PHP type
/// Mixed/Union". Each property must round-trip its default and a later reassignment.
#[test]
fn test_union_property_scalar_literal_defaults() {
    let out = compile_and_run(
        r#"<?php
class Bag {
    public int|false $count = false;
    public mixed $tag = 7;
    public int|float $ratio = 1.5;
}
$b = new Bag();
echo ($b->count === false) ? "F" : "T";
echo "|", $b->tag, "|", $b->ratio;
$b->count = 42;
echo "|", $b->count;
"#,
    );
    assert_eq!(out, "F|7|1.5|42");
}

/// Verifies a narrowed nullable integer remains loadable after its slot joins null and int paths.
#[test]
fn test_narrowed_nullable_int_local_loads_as_int() {
    let out = compile_and_run(
        r#"<?php
function advance(?int $candidate): int {
    $offset = 0;
    for ($i = 0; $i < 2; ++$i) {
        if (null !== $candidate) {
            $offset = $candidate;
        }
        $candidate = $offset;
    }
    return $offset;
}
echo advance(7), "|", advance(null);
"#,
    );
    assert_eq!(out, "7|0");
}

/// Verifies a gradual array read can populate an inline nullable-integer return value.
#[test]
fn test_mixed_array_read_returns_nullable_int() {
    let out = compile_and_run(
        r#"<?php
function selected(array $values): ?int {
    return $values["selected"] ?? null;
}
echo selected(["selected" => 9]), "|";
var_dump(selected([]));
"#,
    );
    assert_eq!(out, "9|NULL\n");
}

/// Verifies an ARRAY literal default in a `mixed`/union slot, instance and static.
///
/// `class T { public mixed $m = []; }` did not compile at all —
/// `unsupported EIR backend feature: object_new for default value of property $m with PHP type
/// Mixed` — and the same refusal on the dynamic allocation path is what made `ReflectionClass`,
/// whose `$__constants` is exactly this shape, unusable through `new $c`. The scalar arms already
/// boxed their payloads and the boxer already handled `Array` on both architectures; only the
/// recognition and its emission were missing.
///
/// The elements are typed `Mixed` because the slot is, so a later write of a different type finds
/// the right storage underneath rather than an `array<int>`. The reassignment and the non-empty
/// literal are here for that reason, not for symmetry.
#[test]
fn test_array_literal_default_in_a_mixed_slot() {
    let out = compile_and_run(
        r#"<?php
class T {
    public mixed $empty = [];
    public mixed $filled = [1, "b", 3];
    public static mixed $shared = [];
}
$t = new T();
var_dump($t->empty);
echo count($t->filled), ":", $t->filled[1], "\n";
$t->empty = [7];
echo count($t->empty), ":", $t->empty[0], "\n";
T::$shared = ["k" => 1];
echo count(T::$shared), "\n";
"#,
    );
    assert_eq!(out, "array(0) {\n}\n3:b\n1:7\n1\n");
}
