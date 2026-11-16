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
