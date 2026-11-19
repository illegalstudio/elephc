//! Purpose:
//! End-to-end tests for PHP reference machinery: aliasing a local to an object
//! property (`$x = &$obj->prop`) with write-through in both directions, by-reference
//! function/method returns (`function &f()`, `function &m()`), capturing them with
//! `$x = &call()`, object properties passed to by-reference method/constructor parameters,
//! and the constant-propagation soundness fix for reference-bound locals.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A reference property's slot holds a pointer to a 16-byte ref-cell; reads and writes
//!   on either the local alias or the property dereference the shared cell, so a write
//!   through one side is observed through the other.
//! - Resolved by-reference returns transfer a managed cell owner to `$x = &call()`;
//!   borrowed cells remain non-owning. Ordinary value calls copy the referenced value.
//! - Every raw cell pointer travels in the integer result register, including cells
//!   containing a string pair or a float, without using string/float result registers.

use crate::support::*;

/// A fresh by-reference foreach value must be canonical null when the source is empty.
/// Capturing it by reference immediately afterwards exercises the raw-to-ref-cell path,
/// which previously promoted an uninitialized frame word when the loop ran zero times.
#[test]
fn test_empty_foreach_initializes_fresh_by_ref_value_before_capture() {
    let out = compile_and_run(
        r#"<?php
$items = [1];
array_pop($items);
foreach ($items as &$value) {
}
$read = function () use (&$value): mixed {
    return $value;
};
echo is_null($read()) ? "null" : "not-null";
$value = "updated";
echo "|", $read();
"#,
    );
    assert_eq!(out, "null|updated");
}

/// `$x = &$obj->prop` aliases a scalar property: writing the local updates the property
/// and writing the property updates the local (write-through in both directions).
#[test]
fn test_reference_to_scalar_property_writes_through_both_ways() {
    let out = compile_and_run(
        "<?php
        class C { public int $v = 1; }
        $o = new C();
        $r = &$o->v;
        $r = 5;
        echo $o->v, \"\\n\";
        $o->v = 9;
        echo $r, \"\\n\";",
    );
    assert_eq!(out, "5\n9\n");
}

/// A later local alias promotes the unbound path after a conditional property reference.
#[test]
fn test_conditional_property_reference_then_local_alias() {
    let out = compile_and_run(
        r#"<?php
class ConditionalRefBox { public int $value = 1; }
function update_conditional_ref(bool $bind): int {
    $local = 1;
    $box = new ConditionalRefBox();
    if ($bind) {
        $local =& $box->value;
    }
    $alias =& $local;
    $alias = 7;
    return $local;
}
echo update_conditional_ref(false) . '|' . update_conditional_ref(true);
"#,
    );
    assert_eq!(out, "7|7");
}

/// `$x = &$obj->prop` aliases an array property: appends through the alias are observed
/// through the property, and clearing the alias to `[]` empties the property (the shape
/// used by `$instanceof = []` after capturing a reference).
#[test]
fn test_reference_to_array_property_appends_and_clears() {
    let out = compile_and_run(
        "<?php
        class C { public array $v = []; }
        $o = new C();
        $r = &$o->v;
        $r[] = 1;
        $r[] = 2;
        echo implode(',', $o->v), \"\\n\";
        $r = [];
        echo count($o->v), \"\\n\";
        $r[] = 9;
        echo $o->v[0], \"\\n\";",
    );
    assert_eq!(out, "1,2\n0\n9\n");
}

/// Reassigning the reference to a non-empty, differently-typed array literal boxes the
/// literal's elements so the property's `Array(Mixed)` reads stay valid (regression: the
/// raw `Array(Int)`/`Array(Str)` payload was stored unboxed and read back as garbage).
#[test]
fn test_reference_array_reassigned_to_typed_literal_boxes_elements() {
    let out = compile_and_run(
        "<?php
        class C { public array $v = []; }
        $o = new C();
        $r = &$o->v;
        $r[] = 1;
        echo implode(',', $o->v), \"\\n\";
        $r = [42, 43];
        echo implode(',', $o->v), \"\\n\";
        echo $o->v[0], \"\\n\";
        echo count($o->v), \"\\n\";
        $r = ['a', 'b', 'c'];
        echo implode('-', $o->v), \"\\n\";",
    );
    assert_eq!(out, "1\n42,43\n42\n2\na-b-c\n");
}

/// The property keeps its declared default until the reference writes through it.
#[test]
fn test_reference_property_keeps_default_until_written() {
    let out = compile_and_run(
        "<?php
        class C { public int $v = 7; }
        $o = new C();
        echo $o->v, \"\\n\";
        $r = &$o->v;
        echo $r, \"\\n\";
        $r = 11;
        echo $o->v, \"\\n\";",
    );
    assert_eq!(out, "7\n7\n11\n");
}

/// A by-reference free function returns a reference to a property; `$x = &f()` aliases it
/// and a write through `$x` updates the property.
#[test]
fn test_by_reference_function_return_aliases_property() {
    let out = compile_and_run(
        "<?php
        class C { public int $v = 10; }
        function &getv(C $o) { return $o->v; }
        $o = new C();
        $r = &getv($o);
        $r = 77;
        echo $o->v, \"\\n\";",
    );
    assert_eq!(out, "77\n");
}

/// A by-reference method returns a reference to `$this->prop`; `$x = &$o->m()` aliases it
/// and appends through the alias update the property's array.
#[test]
fn test_by_reference_method_return_aliases_property() {
    let out = compile_and_run(
        "<?php
        class Box {
            public array $items = [];
            public function &ref() { return $this->items; }
        }
        $b = new Box();
        $r = &$b->ref();
        $r[] = 'x';
        $r[] = 'y';
        echo implode(',', $b->items), \"\\n\";",
    );
    assert_eq!(out, "x,y\n");
}

/// Plain local-to-local aliasing writes through in both directions, even when a write
/// goes through the other alias between reads (regression: reference-bound locals must
/// not carry stale propagated constants).
#[test]
fn test_local_alias_write_through_not_constant_folded() {
    let out = compile_and_run(
        "<?php
        $a = 1;
        $b = &$a;
        $b = 5;
        $a = 7;
        echo $b, \"\\n\";",
    );
    assert_eq!(out, "7\n");
}

/// A by-reference closure returning a captured object's property, called through a variable,
/// aliases the property so appends through the captured reference reach it.
#[test]
fn test_by_reference_closure_return_via_variable() {
    let out = compile_and_run(
        "<?php
        class C { public array $items = []; }
        $o = new C();
        $f = function &() use ($o) { return $o->items; };
        $ref = &$f();
        $ref[] = 'a';
        $ref[] = 'b';
        echo implode(',', $o->items), \"\\n\";",
    );
    assert_eq!(out, "a,b\n");
}

/// An immediately-invoked by-reference closure returning a captured object's property
/// aliases that property.
#[test]
fn test_by_reference_closure_immediate_invoke() {
    let out = compile_and_run(
        "<?php
        class C { public array $items = []; }
        $o = new C();
        $ref = &(function &() use ($o) { return $o->items; })();
        $ref[] = 'x';
        echo implode(',', $o->items), \"\\n\";",
    );
    assert_eq!(out, "x\n");
}

/// The Symfony KernelTrait::configureContainer shape: bind an arrow closure that returns a
/// reference to `$this->prop` to a loader, capture the reference, mutate it through the
/// reference, and clear it — all observed through the loader's property.
#[test]
fn test_closure_bind_by_reference_return_writes_through() {
    let out = compile_and_run(
        "<?php
        class Loader { public array $instanceof = []; }
        $loader = new Loader();
        $instanceof = &\\Closure::bind(fn &() => $this->instanceof, $loader, $loader)();
        $instanceof[] = 'RouteA';
        echo implode(',', $loader->instanceof), \"\\n\";
        $instanceof = [];
        echo count($loader->instanceof), \"\\n\";",
    );
    assert_eq!(out, "RouteA\n0\n");
}

/// `$x = &$obj->prop` aliases a `string` property: the cell pointer is one word, so the
/// write-through works despite the string ABI normally using a `{ptr,len}` register pair.
#[test]
fn test_reference_to_string_property_writes_through_both_ways() {
    let out = compile_and_run(
        "<?php
        class C { public string $s = \"init\"; }
        $o = new C();
        $r = &$o->s;
        $r = \"viaref\";
        echo $o->s, \"\\n\";
        $o->s = \"viaprop\";
        echo $r, \"\\n\";",
    );
    assert_eq!(out, "viaref\nviaprop\n");
}

/// `$x = &$obj->prop` aliases a `float` property: the cell pointer is one word, so the
/// write-through works despite floats normally returning in a floating-point register.
#[test]
fn test_reference_to_float_property_writes_through_both_ways() {
    let out = compile_and_run(
        "<?php
        class C { public float $f = 1.5; }
        $o = new C();
        $r = &$o->f;
        $r = 3.25;
        echo $o->f, \"\\n\";
        $o->f = 9.75;
        echo $r, \"\\n\";",
    );
    assert_eq!(out, "3.25\n9.75\n");
}

/// A by-reference free function returning a `string` property: the caller binds the cell
/// pointer (one word) and a write through the alias updates the property.
#[test]
fn test_by_reference_function_returns_string_property() {
    let out = compile_and_run(
        "<?php
        class C { public string $s = \"init\"; }
        function &slot(C $o): string { return $o->s; }
        $o = new C();
        $r = &slot($o);
        $r = \"viafunc\";
        echo $o->s, \"\\n\";",
    );
    assert_eq!(out, "viafunc\n");
}

/// A by-reference free function returning a `float` property aliases it through the cell
/// pointer rather than the float result register.
#[test]
fn test_by_reference_function_returns_float_property() {
    let out = compile_and_run(
        "<?php
        class C { public float $f = 1.5; }
        function &slot(C $o): float { return $o->f; }
        $o = new C();
        $r = &slot($o);
        $r = 9.75;
        echo $o->f, \"\\n\";",
    );
    assert_eq!(out, "9.75\n");
}

/// A by-reference method returning a `string` property aliases `$this->prop`; the method
/// call result is stored single-word so the alias dereferences the right cell.
#[test]
fn test_by_reference_method_returns_string_property() {
    let out = compile_and_run(
        "<?php
        class Holder {
            public string $tag = \"h0\";
            public function &tagSlot(): string { return $this->tag; }
        }
        $h = new Holder();
        $t = &$h->tagSlot();
        $t = \"viamethod\";
        echo $h->tag, \"\\n\";",
    );
    assert_eq!(out, "viamethod\n");
}

/// The Symfony-shaped immediate-invoke `Closure::bind` over a `string` property: the bound
/// closure returns a reference to `$this->prop`, captured and mutated through the alias.
#[test]
fn test_closure_bind_by_reference_string_property() {
    let out = compile_and_run(
        "<?php
        class C { public string $s = \"init\"; }
        $c = new C();
        $ref = &\\Closure::bind(fn &() => $this->s, $c, $c)();
        $ref = \"bound\";
        echo $c->s, \"\\n\";
        $c->s = \"viaprop\";
        echo $ref, \"\\n\";",
    );
    assert_eq!(out, "bound\nviaprop\n");
}

/// A by-reference `Closure::bind` stored in a variable and called separately (not invoked
/// immediately) still aliases the bound property: the assignment tracks the bound closure as a
/// static callable so `$bound()` lowers to a direct call carrying the cell pointer, instead of
/// the generic descriptor invoker which would box the result.
#[test]
fn test_closure_bind_by_reference_stored_in_variable() {
    let out = compile_and_run(
        "<?php
        class C { public array $items = []; }
        $o = new C();
        $bound = \\Closure::bind(fn &() => $this->items, $o, $o);
        $ref = &$bound();
        $ref[] = 'x';
        $ref[] = 'y';
        echo implode(',', $o->items), \"\\n\";
        $ref = [];
        echo count($o->items), \"\\n\";",
    );
    assert_eq!(out, "x,y\n0\n");
}

/// The same variable-stored by-reference `Closure::bind` over a `string` property: the cell
/// pointer survives the call boundary and write-through works both ways.
#[test]
fn test_closure_bind_by_reference_stored_in_variable_string() {
    let out = compile_and_run(
        "<?php
        class C { public string $s = \"init\"; }
        $o = new C();
        $bound = \\Closure::bind(fn &() => $this->s, $o, $o);
        $ref = &$bound();
        $ref = \"changed\";
        echo $o->s, \"\\n\";
        $o->s = \"viaprop\";
        echo $ref, \"\\n\";",
    );
    assert_eq!(out, "changed\nviaprop\n");
}

/// Two locals aliasing the same property both observe a write through either side.
#[test]
fn test_two_locals_aliasing_same_property() {
    let out = compile_and_run(
        "<?php
        class C { public int $v = 0; }
        $o = new C();
        $a = &$o->v;
        $b = &$o->v;
        $a = 3;
        echo $b, \"\\n\";
        $b = 8;
        echo $o->v, \"\\n\";",
    );
    assert_eq!(out, "3\n8\n");
}

/// `$b =& $a[0]` aliases an indexed-array int element: writing the local updates the array
/// element in place (write-through from the alias to the array).
#[test]
fn test_ref_alias_array_element_int() {
    let out = compile_and_run(r#"<?php $a = [1, 2]; $b =& $a[0]; $b = 9; echo $a[0];"#);
    assert_eq!(out, "9");
}

/// `$b =& $a[0]` aliases an indexed-array int element: reading the local after the array is
/// mutated through another path reflects the change (write-through from the array to the alias).
#[test]
fn test_ref_alias_array_element_int_readback() {
    let out = compile_and_run(
        r#"<?php $a = [1, 2]; $b =& $a[0]; $a[0] = 7; echo $b;"#,
    );
    assert_eq!(out, "7");
}

/// `$b =& $a[0]` aliases an indexed-array string element: writing the local updates the array
/// element's pointer and length in place.
#[test]
fn test_ref_alias_array_element_string() {
    let out = compile_and_run(
        r#"<?php $a = ["hello", "world"]; $b =& $a[0]; $b = "HEY"; echo $a[0];"#,
    );
    assert_eq!(out, "HEY");
}

/// `$b =& $a[1]` aliases a non-zero indexed-array int element: the address computation must
/// scale by the element size and skip the header correctly.
#[test]
fn test_ref_alias_array_element_nonzero_index() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30]; $b =& $a[1]; $b = 99; echo $a[1];"#,
    );
    assert_eq!(out, "99");
}

/// Keeps a captured reference and its originally typed array on the same boxed storage representation.
#[test]
fn test_ref_alias_array_element_capture_retyping() {
    let out = compile_and_run(r#"<?php
$a = [1, 2];
$copy = $a;
$b =& $a[0];
$set = function (mixed $value) use (&$b): void { $b = $value; };
$set(9);
echo $a[0], ":", $b, "\n";
$set("updated");
echo $a[0], ":", $b, "\n";
$set(null);
echo is_null($a[0]) ? "null" : "wrong", ":", is_null($b) ? "null" : "wrong", "\n";
echo $copy[0], ":", $copy[1], "\n";
"#);
    assert_eq!(out, "9:9\nupdated:updated\nnull:null\n1:2\n");
}

/// An object property may be passed to a user method's by-reference parameter.
///
/// The call is lowered as a hidden temporary plus a write-back, so the callee's mutation
/// reaches the property without evaluating the receiver twice.
#[test]
fn test_property_argument_binds_to_method_by_reference_parameter() {
    let out = compile_and_run(
        r#"<?php
class Value { public function __construct(public int $number) {} }
class Box { public function __construct(public Value $value) {} }
class Ops {
    public function bump(Value &$value): void {
        $value = new Value($value->number + 1);
    }
    public function swap(Value &$left, Value &$right): void {
        $temp = $left;
        $left = $right;
        $right = $temp;
    }
}
$first = new Box(new Value(1));
$second = new Box(new Value(3));
$ops = new Ops();
$ops->bump($first->value);
$ops->swap($first->value, $second->value);
echo $first->value->number, ",", $second->value->number, "\n";
"#,
    );
    assert_eq!(out, "3,2\n");
}

/// A constructor by-reference parameter accepts an object property place too.
#[test]
fn test_property_argument_binds_to_constructor_by_reference_parameter() {
    let out = compile_and_run(
        r#"<?php
class Value { public function __construct(public int $number) {} }
class Box { public Value $value; }
class Reset { public function __construct(Value &$value) { $value = new Value(9); } }
$box = new Box();
$box->value = new Value(1);
new Reset($box->value);
echo $box->value->number, "\n";
"#,
    );
    assert_eq!(out, "9\n");
}

/// A union holding the declared class plus `false`/`null` is guarded at the call boundary.
#[test]
fn test_object_union_arguments_throw_type_error_for_non_object_alternative() {
    let out = compile_and_run(
        r#"<?php
class Marker {}
class Provider {
    public function absent(): Marker|false { return false; }
    public function nullValue(): ?Marker { return null; }
    public function present(): Marker|false { return new Marker(); }
}
class Sink {
    public function __construct(Marker $value) {}
    public function take(Marker $value): string { return "taken"; }
}
$sink = new Sink(new Marker());
$provider = new Provider();
try {
    $sink->take($provider->absent());
} catch (TypeError $error) {
    $message = $error->getMessage();
    echo str_starts_with($message, 'Sink::take(): Argument #1 ($value) must be of type Marker, false given, called in ') ? "prefix-ok" : $message;
    echo str_contains($message, " on line ") ? ":line-ok\n" : ":no-line\n";
}
try {
    $sink->take($provider->nullValue());
} catch (TypeError $error) {
    $message = $error->getMessage();
    echo str_starts_with($message, 'Sink::take(): Argument #1 ($value) must be of type Marker, null given, called in ') ? "prefix-ok" : $message;
    echo str_contains($message, " on line ") ? ":line-ok\n" : ":no-line\n";
}
try {
    new Sink($provider->absent());
} catch (TypeError $error) {
    $message = $error->getMessage();
    echo str_starts_with($message, 'Sink::__construct(): Argument #1 ($value) must be of type Marker, false given, called in ') ? "prefix-ok" : $message;
    echo str_contains($message, " on line ") ? ":line-ok\n" : ":no-line\n";
}
echo $sink->take($provider->present()), "\n";
"#,
    );
    assert_eq!(
        out,
        "prefix-ok:line-ok\nprefix-ok:line-ok\nprefix-ok:line-ok\ntaken\n"
    );
}

/// Guards a union-typed local even after its storage widened to a boxed Mixed cell.
///
/// The checker accepted `Marker|false`, but lowering saw only Mixed and skipped the guard,
/// then passed `false` as a raw object pointer. The catch block must also stay reachable:
/// the callee is pure, so the exception analysis has to treat the guard as a throw site.
#[test]
fn test_object_union_locals_throw_catchable_type_error() {
    let out = compile_and_run(
        r#"<?php
class Marker {}
function take(Marker $m): string { return "taken"; }
$w = $argc > 5 ? new Marker() : false;
try {
    take($w);
} catch (TypeError $error) {
    $message = $error->getMessage();
    echo str_starts_with($message, 'take(): Argument #1 ($m) must be of type Marker, false given, called in ') ? "prefix-ok" : $message;
    echo str_contains($message, " on line ") ? ":line-ok\n" : ":no-line\n";
}
$v = (function (): Marker|false { return false; })();
try {
    take($v);
} catch (TypeError $error) {
    echo str_starts_with($error->getMessage(), 'take(): Argument #1 ($m) must be of type Marker, false given, called in ') ? "prefix-ok\n" : $error->getMessage();
}
$ok = $argc < 5 ? new Marker() : false;
echo take($ok), "\n";
"#,
    );
    assert_eq!(out, "prefix-ok:line-ok
prefix-ok
taken
");
}

/// A nullsafe method call rewrites object-property by-reference arguments in its call branch.
#[test]
fn test_property_argument_binds_through_nullsafe_method_call() {
    let out = compile_and_run(
        r#"<?php
class Value { public function __construct(public int $number) {} }
class Box { public function __construct(public Value $value) {} }
class Ops {
    public function swap(Value &$left, Value &$right): void {
        $temp = $left;
        $left = $right;
        $right = $temp;
    }
}
$ops = new Ops();
$first = new Box(new Value(1));
$second = new Box(new Value(2));
$ops?->swap($first->value, $second->value);
echo $first->value->number, ",", $second->value->number, "\n";
"#,
    );
    assert_eq!(out, "2,1\n");
}

/// Scalar, string, array, and mixed properties bind by reference through a method call.
#[test]
fn test_scalar_and_array_property_arguments_bind_by_reference() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $number = 1;
    public string $label = "a";
    public array $items = [1];
    public mixed $mixed = 0;
}
class Ops {
    public function bump(int &$number): void { $number++; }
    public function append(string &$label): void { $label .= "!"; }
    public function addItem(array &$items): void { $items[] = 9; }
    public function setMixed(mixed &$mixed): void { $mixed = "x"; }
}
$box = new Box();
$ops = new Ops();
$ops->bump($box->number);
$ops->append($box->label);
$ops->addItem($box->items);
$ops->setMixed($box->mixed);
echo $box->number, ",", $box->label, ",", count($box->items), ",", $box->mixed, "\n";
"#,
    );
    assert_eq!(out, "2,a!,2,x\n");
}

/// Union and nullable properties bind by reference with their runtime type preserved.
#[test]
fn test_union_and_nullable_property_arguments_bind_by_reference() {
    let out = compile_and_run(
        r#"<?php
class Box { public int|string $value = 1; public ?int $maybe = 1; }
class Ops {
    public function setUnion(int|string &$value): void { $value = "x"; }
    public function setNullable(?int &$value): void { $value = 5; }
}
$box = new Box();
$ops = new Ops();
$ops->setUnion($box->value);
$ops->setNullable($box->maybe);
var_dump($box->value, $box->maybe);
"#,
    );
    assert_eq!(out, "string(1) \"x\"\nint(5)\n");
}

/// A free function accepts an object property as a by-reference argument.
#[test]
fn test_property_argument_binds_to_free_function_by_reference_parameter() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $number = 1; }
function bump(int &$number): void { $number++; }
$box = new Box();
bump($box->number);
echo $box->number, "\n";
"#,
    );
    assert_eq!(out, "2\n");
}

/// A static property binds to a static method's by-reference parameter.
#[test]
fn test_static_property_argument_binds_to_static_method_by_reference_parameter() {
    let out = compile_and_run(
        r#"<?php
class Box { public static int $number = 1; public static array $items = [1]; }
class Ops {
    public static function bump(int &$number): void { $number++; }
    public static function addItem(array &$items): void { $items[] = 9; }
}
Ops::bump(Box::$number);
Ops::addItem(Box::$items);
echo Box::$number, ",", count(Box::$items), "\n";
"#,
    );
    assert_eq!(out, "2,2\n");
}

/// By-reference variadics accept scalar, string, mixed, object, static, union, and nullable
/// property places through the same hidden write-back.
#[test]
fn test_variadic_by_ref_property_arguments() {
    let out = compile_and_run(
        r#"<?php
class Value { public function __construct(public int $number) {} }
class Box {
    public int $integer = 1;
    public string $label = "a";
    public mixed $mixed = 0;
    public Value $value;
    public int|string $union = 1;
    public ?int $nullable = 1;
    public static int $static = 1;
}
function collect(&...$refs): void {
    $refs[0] = 9;
    $refs[1] = "b";
    $refs[2] = "x";
    $refs[3] = new Value(7);
    $refs[4] = "u";
    $refs[5] = 5;
    $refs[6] = 6;
}
$box = new Box();
$box->value = new Value(1);
collect(
    $box->integer,
    $box->label,
    $box->mixed,
    $box->value,
    $box->union,
    $box->nullable,
    Box::$static,
);
var_dump($box->integer, $box->label, $box->mixed, $box->value->number, $box->union, $box->nullable, Box::$static);
"#,
    );
    assert_eq!(
        out,
        "int(9)\nstring(1) \"b\"\nstring(1) \"x\"\nint(7)\nstring(1) \"u\"\nint(5)\nint(6)\n"
    );
}

/// Rejects a variadic by-reference write that violates the target property's declared type.
///
/// The callee receives boxed Mixed ref cells, so the write-back must apply php-src's typed
/// property check before unboxing; otherwise an int assigned through a `Value` property is
/// stored as a raw object pointer.
#[test]
fn test_by_reference_variadic_property_writeback_rejects_mismatched_types() {
    let out = compile_and_run(
        r#"<?php
class Value { public function __construct(public int $number) {} }
class Box { public Value $v; public int $n = 0; function __construct() { $this->v = new Value(0); } }
function setIdx(&...$refs): void { $refs[0] = 7; }
function setStr(&...$refs): void { $refs[0] = "abc"; }
function setVal(&...$refs): void { $refs[0] = new Value(5); }
function setInt(&...$refs): void { $refs[0] = 7; }
$b = new Box();
try { setIdx($b->v); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$d = new Box();
try { setStr($d->n); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
setVal($b->v); echo $b->v->number, "\n";
setInt($d->n); echo $d->n, "\n";
"#,
    );
    assert_eq!(
        out,
        "Cannot assign int to reference held by property Box::$v of type Value\n\
Cannot assign string to reference held by property Box::$n of type int\n\
5\n\
7\n"
    );
}

/// Writes back through property receivers nested below another property or an array element.
#[test]
fn test_by_reference_nested_property_receivers_write_back() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $n = 1; }
class Outer { public Box $box; function __construct() { $this->box = new Box(); } }
function inc(int &$x): void { $x++; }
$o = new Outer(); inc($o->box->n); echo $o->box->n, "\n";
$arr = [new Box()]; inc($arr[0]->n); echo $arr[0]->n, "\n";
"#,
    );
    assert_eq!(out, "2\n2\n");
}

/// Verifies property references bind at their source position and honor later property writes.
#[test]
fn test_by_reference_property_source_order_and_callee_mutation() {
    let out = compile_and_run(
        r#"<?php
class Box { public int $n = 1; }
$holder = null;
function noise(): int { global $holder; $holder->n = 50; return 2; }
function g(int $a, int &$b): void { $b = $b + 1; }
$box = new Box(); $holder = $box;
g(noise(), $box->n);
echo $box->n, "\n";

class Bump { public int $n = 1; public function bump(int &$v): void { $v = 9; $this->n = 7; } }
$bump = new Bump(); $bump->bump($bump->n);
echo $bump->n, "\n";

class Grab { public int $v = 5; }
function &grab(int &$x): int { return $x; }
$grab = new Grab(); $r = &grab($grab->v); $r = 99;
echo $grab->v, "\n";
"#,
    );
    assert_eq!(out, "51\n7\n99\n");
}

/// Verifies a numeric string outside the i64 range is rejected by a typed property reference.
#[test]
fn test_by_reference_property_writeback_rejects_out_of_range_numeric_strings() {
    let out = compile_and_run(
        r#"<?php
class C { public int $n = 0; }
$payload = null;
function set(&...$refs): void { global $payload; $refs[0] = $payload; }
$c = new C();
foreach (["42", "999999999999999999999999", "9223372036854775807", "-9223372036854775809", 1e30] as $v) {
    $payload = $v;
    try { set($c->n); echo "ok ", $c->n, "\n"; }
    catch (Throwable $e) { echo "err ", $e->getMessage(), "\n"; }
}
"#,
    );
    assert_eq!(
        out,
        "ok 42\n\
err Cannot assign string to reference held by property C::$n of type int\n\
ok 9223372036854775807\n\
ok -9223372036854775808\n\
err Cannot assign float to reference held by property C::$n of type int\n"
    );
}

/// Verifies a named argument still receives the object-union runtime guard and unbox.
#[test]
fn test_named_object_union_argument_throws_and_unboxes() {
    let out = compile_and_run(
        r#"<?php
class Marker {}
function take(Marker $m): string { return get_class($m); }
function id(Marker|false $v): Marker|false { return $v; }
$v = id(false);
try { take(m: $v); } catch (TypeError $e) {
    $message = $e->getMessage();
    echo str_starts_with($message, 'take(): Argument #1 ($m) must be of type Marker, false given, called in ') ? "prefix-ok" : $message;
    echo str_contains($message, " on line ") ? ":line-ok\n" : ":no-line\n";
}
$ok = id(new Marker());
echo take(m: $ok), "\n";
"#,
    );
    assert_eq!(out, "prefix-ok:line-ok\nMarker\n");
}

/// Verifies a by-reference variadic binds and mutates an array property.
#[test]
fn test_variadic_by_ref_array_property_binds_by_reference() {
    let out = compile_and_run(
        r#"<?php
class Box { public array $items = [1]; }
function f(&...$refs): void { $refs[0][0] = 9; }
$box = new Box();
f($box->items);
var_export($box->items);
"#,
    );
    assert_eq!(out, "array (\n  0 => 9,\n)");
}

/// Verifies the plain typed-property reference path rejects a mismatched callee write.
#[test]
fn test_by_reference_typed_property_writeback_rejects_a_mismatched_callee_write() {
    let out = compile_and_run(
        r#"<?php
class C { public int $n = 0; }
function w(int &$x, mixed $v): void { $x = $v; }
$c = new C();
try { w($c->n, "abc"); } catch (TypeError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
var_dump($c->n);
$d = new C();
w($d->n, "42");
var_dump($d->n);
"#,
    );
    assert_eq!(
        out,
        "TypeError: Cannot assign string to reference held by property C::$n of type int\nint(0)\nint(42)\n"
    );
}

/// Verifies the typed-property reference guard classifies numeric strings by value: PHP's six
/// leading-whitespace bytes, float forms, and the asymmetric `i64` overflow boundary where
/// `-9223372036854775809` rounds to `-2^63` but positive `2^63` is rejected.
#[test]
fn test_by_reference_typed_property_numeric_string_range_and_whitespace() {
    let out = compile_and_run(
        r#"<?php
class C { public int $n = 0; }
class F { public float $f = 0.0; }
function w(int &$x, mixed $v): void { $x = $v; }
function wf(float &$x, mixed $v): void { $x = $v; }
$cases = [
    "999999999999999999999999",
    "9223372036854775808",
    "9223372036854775807",
    "-9223372036854775808",
    "-9223372036854775809",
    "1e2",
    "1e18",
    "1e19",
    "\t42",
    "\n42\t",
];
foreach ($cases as $i => $text) {
    $c = new C();
    try {
        w($c->n, $text);
        echo $i, "=", $c->n, "\n";
    } catch (TypeError $e) {
        echo $i, "=TypeError\n";
    }
}
$f = new F();
wf($f->f, "1e30");
echo $f->f > 1e29 ? "float-big\n" : "float-small\n";
"#,
    );
    assert_eq!(
        out,
        "0=TypeError\n1=TypeError\n2=9223372036854775807\n3=-9223372036854775808\n\
         4=-9223372036854775808\n5=100\n6=1000000000000000000\n7=TypeError\n\
         8=42\n9=42\nfloat-big\n"
    );
}

/// Verifies a raw scalar reference cell converts between PHP's int and float representations
/// instead of publishing the source register that belongs to the other type.
#[test]
fn test_typed_reference_numeric_representation_conversion() {
    let out = compile_and_run(
        r#"<?php
class C { public int $n = 0; public float $f = 0.0; }
function w(int &$x, float &$y): void { $x = 2.5; $y = 3; }
$c = new C();
w($c->n, $c->f);
var_dump($c->n, $c->f);
$n = 0;
$f = 0.0;
w($n, $f);
var_dump($n, $f);
"#,
    );
    assert_eq!(out, "int(2)\nfloat(3)\nint(2)\nfloat(3)\n");
}

/// Verifies the string, bool, false, and float typed-property reference guards accept PHP's
/// weak scalar conversions and reject container/null sources with the reference-form message.
#[test]
fn test_typed_property_reference_scalar_guard_matrix() {
    let out = compile_and_run(
        r#"<?php
class S { public string $s = "x"; }
class B { public bool $b = false; }
class Fal { public false $flag = false; }
class F { public float $f = 0.0; }
class Str { public function __toString(): string { return "ok"; } }
function ws(string &$x, mixed $v): void { $x = $v; }
function wb(bool &$x, mixed $v): void { $x = $v; }
function wf(false &$x, mixed $v): void { $x = $v; }
function wl(float &$x, mixed $v): void { $x = $v; }
$s = new S();
ws($s->s, 42); echo $s->s, ":";
ws($s->s, true); echo $s->s, ":";
ws($s->s, new Str()); echo $s->s, ":";
try { ws($s->s, []); } catch (TypeError $e) { echo $e->getMessage(), ":"; }
try { ws($s->s, null); } catch (TypeError $e) { echo $e->getMessage(), ":"; }
$b = new B();
wb($b->b, 2); var_dump($b->b);
wb($b->b, ""); var_dump($b->b);
try { wb($b->b, []); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$fal = new Fal();
wf($fal->flag, false); var_dump($fal->flag);
try { wf($fal->flag, true); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
$f = new F();
wl($f->f, 3); var_dump($f->f);
wl($f->f, "2.5"); var_dump($f->f);
try { wl($f->f, "abc"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "42:1:ok:\
Cannot assign array to reference held by property S::$s of type string:\
Cannot assign null to reference held by property S::$s of type string:\
bool(true)\n\
bool(false)\n\
Cannot assign array to reference held by property B::$b of type bool\n\
bool(false)\n\
Cannot assign true to reference held by property Fal::$flag of type false\n\
float(3)\n\
float(2.5)\n\
Cannot assign string to reference held by property F::$f of type float\n"
    );
}

/// A `mixed &$out` parameter can bind a typed scalar property: a compatible write updates
/// it, while an incompatible write raises PHP's typed-reference `TypeError` and leaves the
/// property unchanged.
#[test]
fn test_typed_property_by_ref_writeback_guard() {
    let out = compile_and_run(
        r#"<?php
class Box { public string $value = "unchanged"; }
function replaceWithArray(mixed &$out): void { $out = ["a" => 1]; }
function replaceWithString(mixed &$out): void { $out = "changed"; }
$box = new Box();
try {
    replaceWithArray($box->value);
    echo "unguarded|";
} catch (TypeError $error) {
    echo $error->getMessage(), "|";
}
echo $box->value, "|";
replaceWithString($box->value);
echo $box->value;
"#,
    );
    assert_eq!(
        out,
        "Cannot assign array to reference held by property Box::$value of type string|unchanged|changed"
    );
}

/// A `mixed &$out` parameter also binds a typed static property through the same
/// hidden-temporary write-back, with the same typed-reference guard.
#[test]
fn test_typed_static_property_by_ref_writeback_guard() {
    let out = compile_and_run(
        r#"<?php
class StaticBox { public static string $value = "unchanged"; }
function replaceStaticWithArray(mixed &$out): void { $out = ["a" => 1]; }
function replaceStaticWithString(mixed &$out): void { $out = "changed"; }
try {
    replaceStaticWithArray(StaticBox::$value);
    echo "unguarded|";
} catch (TypeError $error) {
    echo $error->getMessage(), "|";
}
echo StaticBox::$value, "|";
replaceStaticWithString(StaticBox::$value);
echo StaticBox::$value;
"#,
    );
    assert_eq!(
        out,
        "Cannot assign array to reference held by property StaticBox::$value of type string|unchanged|changed"
    );
}

/// A straight-line `unset($GLOBALS['x'])` detaches a `global $x` alias: the removed value
/// stays readable locally, later writes stay local, and the global remains unset.
#[test]
fn test_global_alias_detaches_after_globals_unset() {
    let out = compile_and_run(
        r#"<?php
function detachAlias(): void {
    global $z;
    $z = "inner";
    unset($GLOBALS["z"]);
    var_dump(isset($z), $z, isset($GLOBALS["z"]));
    $z = "after";
    var_dump(isset($z), $z, isset($GLOBALS["z"]), $GLOBALS["z"] ?? "missing");
}
detachAlias();
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nstring(5) \"inner\"\nbool(false)\nbool(true)\nstring(5) \"after\"\nbool(false)\nstring(7) \"missing\"\n"
    );
}

/// A closure created after the unset captures the detached local, not the removed global.
#[test]
fn test_global_alias_detach_closure_capture() {
    let out = compile_and_run(
        r#"<?php
function captureDetached(): void {
    global $x;
    $x = "inner";
    unset($GLOBALS["x"]);
    $read = function () use ($x): string { return $x; };
    $x = "after";
    echo $read(), "|", $x, "|", isset($GLOBALS["x"]) ? "set" : "unset";
}
captureDetached();
"#,
    );
    assert_eq!(out, "inner|after|unset");
}
/// A conditional `unset($GLOBALS['x'])` detaches a `global $x` alias on the path that ran it and
/// leaves the alias bound to the global on the other path.
///
/// The removed value stays reachable through the local, later writes stay local, and the global
/// keeps receiving assignments on the path where the unset did not run. Coverage also spans a
/// detachment inside a loop, an indexed/append write, `isset`/`empty`, and a second unset on a
/// later path; the expected output is php 8.5's for the same fixtures.
#[test]
fn test_global_alias_conditional_unset_detaches() {
    let out = compile_and_run(
        r#"<?php
function detach(string $mode): void {
    global $value;
    $value = 'g';
    if ($mode === 'yes') {
        unset($GLOBALS['value']);
    }
    echo $mode, ':', $value, '|', isset($value) ? 'y' : 'n', '|', empty($value) ? 'e' : 'f', '|', $GLOBALS['value'] ?? 'missing', "\n";
    $value = 'w';
    echo $mode, ':', $value, '|', $GLOBALS['value'] ?? 'missing', "\n";
}
function bump(bool $detachIt): void {
    global $count;
    $count = 1;
    for ($i = 0; $i < 3; $i++) {
        if ($detachIt && $i === 1) {
            unset($GLOBALS['count']);
        }
        $count++;
    }
    echo $detachIt ? 'bump-detached:' : 'bump:', $count, '|', $GLOBALS['count'] ?? 'missing', "\n";
}
function grow(bool $detachIt): void {
    global $items;
    $items = ['a'];
    if ($detachIt) {
        unset($GLOBALS['items']);
    }
    $items[] = 'b';
    $items['k'] = 'c';
    echo implode(',', $items), '|', isset($GLOBALS['items']) ? 'set' : 'unset', "\n";
}
function twice(bool $first, bool $second): void {
    global $pair;
    $pair = 'v';
    if ($first) {
        unset($GLOBALS['pair']);
    }
    if ($second) {
        unset($GLOBALS['pair']);
        $pair = 'after';
    }
    echo $pair, '|', $GLOBALS['pair'] ?? 'missing', "\n";
}
$value = 'init';
detach('no');
detach('yes');
echo 'final:', $GLOBALS['value'] ?? 'missing', "\n";
$count = 0;
bump(false);
bump(true);
$items = [];
grow(false);
grow(true);
echo 'items:', $GLOBALS['items'] ?? 'missing', "\n";
$pair = 'P';
twice(false, false);
twice(true, false);
twice(false, true);
twice(true, true);
"#,
    );
    assert_eq!(
        out,
        "no:g|y|f|g\nno:w|w\nyes:g|y|f|missing\nyes:w|missing\nfinal:missing\n\
         bump:4|4\nbump-detached:4|missing\na,b,c|set\na,b,c|unset\nitems:missing\n\
         v|v\nv|missing\nafter|missing\nafter|missing\n"
    );
}


/// Container and property writes on a detached alias target the detached copy, so the unset
/// global is never recreated and the local keeps its value. Expected output is php 8.5's.
#[test]
fn test_global_alias_conditional_unset_splits_container_writes() {
    let out = compile_and_run(
        r#"<?php
class DetachBox { public string $v = 'init'; public array $list = []; }
function nested(bool $c): void {
    global $tree;
    $tree = ['a' => ['b' => 1]];
    if ($c) { unset($GLOBALS['tree']); }
    $tree['a']['b'] = 9;
    echo "tree={$tree['a']['b']}|", isset($GLOBALS['tree']) ? 'set' : 'unset', "\n";
}
function props(bool $c): void {
    global $box;
    $box = new DetachBox();
    if ($c) { unset($GLOBALS['box']); }
    $box->v = 'set';
    $box->list[] = 'a';
    $box->list['k'] = 'b';
    echo $box->v, '|', implode(',', $box->list), '|', isset($GLOBALS['box']) ? 'set' : 'unset', "\n";
}
$proto = new DetachBox();
nested(false); nested(true);
props(false); props(true);
"#,
    );
    assert_eq!(
        out,
        "tree=9|set\ntree=9|unset\nset|a,b|set\nset|a,b|unset\n"
    );
}
