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

/// A declared property passed to a `mixed` by-reference parameter is validated against its
/// declared type after the call: an incompatible write raises PHP's reference-held `TypeError`
/// and leaves the property unchanged, while a compatible write is stored back.
#[test]
fn test_by_reference_typed_property_write_is_validated() {
    let out = compile_and_run(
        r#"<?php
class TypedPropertyReference { public string $value = "old"; }
function write_array(mixed &$slot): void { $slot = ["new"]; }
function write_string(mixed &$slot): void { $slot = "new"; }
$object = new TypedPropertyReference();
try {
    write_array($object->value);
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
echo $object->value, "\n";
write_string($object->value);
echo $object->value, "\n";
"#,
    );
    assert_eq!(
        out,
        "Cannot assign array to reference held by property TypedPropertyReference::$value of type string\nold\nnew\n"
    );
}

/// A by-reference array element of a declared property still binds the property's reference
/// cell: the callee replaces the element in place.
#[test]
fn test_by_reference_property_array_element_writes_through() {
    let out = compile_and_run(
        r#"<?php
class ReferenceArrayBox { public array $items = [1]; }
function replace_element(mixed &$slot): void { $slot = 99; }
$box = new ReferenceArrayBox();
replace_element($box->items[0]);
var_dump($box->items);
"#,
    );
    assert_eq!(out, "array(1) {\n  [0]=>\n  int(99)\n}\n");
}

/// Nested and nullable property receivers resolve to a single declared property, so the
/// boxed-temporary write-back reaches them instead of dropping the callee's write.
#[test]
fn test_by_reference_nested_and_nullable_property_write_back() {
    let out = compile_and_run(
        r#"<?php
class ReferenceInner { public string $value = "inner"; }
class ReferenceOuter { public ReferenceInner $inner; public function __construct() { $this->inner = new ReferenceInner(); } }
function replace_text(mixed &$slot): void { $slot = "changed"; }
$outer = new ReferenceOuter();
replace_text($outer->inner->value);
echo $outer->inner->value, "\n";
function replace_nullable(?ReferenceInner $inner): void {
    replace_text($inner->value);
    echo $inner->value, "\n";
}
replace_nullable(new ReferenceInner());
"#,
    );
    assert_eq!(out, "changed\nchanged\n");
}

/// The reference-held diagnostic names the class that DECLARES the property, not the
/// receiver's subclass.
#[test]
fn test_by_reference_typed_property_names_declaring_class() {
    let out = compile_and_run(
        r#"<?php
class ReferenceBase { public string $value = "base"; }
class ReferenceSubclass extends ReferenceBase {}
function replace_with_array(mixed &$slot): void { $slot = []; }
$value = new ReferenceSubclass();
try {
    replace_with_array($value->value);
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
echo $value->value, "\n";
"#,
    );
    assert_eq!(
        out,
        "Cannot assign array to reference held by property ReferenceBase::$value of type string\nbase\n"
    );
}

/// Verifies a by-reference parameter rooted in a concretely typed property writes back to the
/// declared property slot, preserving the property's int, string, and array types.
#[test]
fn test_by_reference_concretely_typed_property_parameter_write_back() {
    let out = compile_and_run(
        r#"<?php
class TypedReferenceBox { public int $number = 1; public string $text = "a"; public array $items = [1]; }
function increment_reference(int &$number): void { $number++; }
function extend_reference(string &$text): void { $text .= "!"; }
function append_reference(array &$items): void { $items[] = 2; }
$box = new TypedReferenceBox();
increment_reference($box->number);
extend_reference($box->text);
append_reference($box->items);
var_dump($box->number, $box->text, $box->items);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "int(2)\n",
            "string(2) \"a!\"\n",
            "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\n",
        )
    );
}

/// An untyped property is widened to boxed Mixed storage when passed by reference: a user
/// function and a re-typing builtin both write through the temporary back into the slot.
#[test]
fn test_by_reference_untyped_property_widens_storage() {
    let out = compile_and_run(
        r#"<?php
class UntypedReference { public $text = "x"; public $missing; }
function set_text(mixed &$slot): void { $slot = "42"; }
$box = new UntypedReference();
set_text($box->text);
var_dump($box->text);
settype($box->text, "int");
var_dump($box->text);
set_text($box->missing);
var_dump($box->missing);
"#,
    );
    assert_eq!(out, "string(2) \"42\"\nint(42)\nstring(2) \"42\"\n");
}

/// An untyped property's array element can also be passed by reference: the property is
/// widened and promoted so the element cell is published back into the property array.
#[test]
fn test_by_reference_untyped_property_array_element_write_back() {
    let out = compile_and_run(
        r#"<?php
class UntypedArrayReference { public $items = [1, 2]; public $missing; }
function bump_element(mixed &$element): void { $element = 99; }
$box = new UntypedArrayReference();
bump_element($box->items[0]);
var_dump($box->items);
$box->missing = [7];
bump_element($box->missing[0]);
var_dump($box->missing);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "array(2) {\n  [0]=>\n  int(99)\n  [1]=>\n  int(2)\n}\n",
            "array(1) {\n  [0]=>\n  int(99)\n}\n",
        )
    );
}

/// A property passed to an untyped by-reference parameter shares its live cell during the call,
/// so another view of the same object observes the callee's write immediately.
#[test]
fn test_property_reference_argument_is_live_during_the_call() {
    let out = compile_and_run_capture(
        r#"<?php
class RefLiveOwner { public $value = 1; }
function ref_live_bump(&$slot, RefLiveOwner $same): void {
    $slot = 3;
    echo "inside=", $same->value, "\n";
}
$owner = new RefLiveOwner();
ref_live_bump($owner->value, $owner);
echo "after=", $owner->value, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "inside=3\nafter=3\n", "{}", out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

/// A typed property whose storage cannot satisfy a declared by-reference parameter raises PHP's
/// call-site `TypeError` at runtime; the property keeps its previous value and no compile error
/// is reported for the call.
#[test]
fn test_property_reference_argument_type_error_is_raised_at_call_entry() {
    let out = compile_and_run_capture(
        r#"<?php
class RefTypedMismatch { public string $text = "x"; }
function ref_typed_int(int &$slot): void { $slot = 1; echo "not reached\n"; }
$owner = new RefTypedMismatch();
try {
    ref_typed_int($owner->text);
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
var_dump($owner->text);
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "ref_typed_int(): Argument #1 ($slot) must be of type int, string given\nstring(1) \"x\"\n",
        "{}",
        out.stderr
    );
}

/// A callee that writes through an untyped property reference and then throws publishes the
/// write. Typed slots instead use the typed-reference cell path, whose guarded write-back runs
/// on normal return.
#[test]
fn test_property_reference_argument_write_survives_a_throw() {
    let out = compile_and_run_capture(
        r#"<?php
class RefThrowUntyped { public $value = 1; }
function ref_throw_untyped(&$slot): void {
    $slot = 5;
    throw new Exception("untyped");
}
$untyped = new RefThrowUntyped();
try { ref_throw_untyped($untyped->value); } catch (Exception $error) { echo $error->getMessage(), "\n"; }
echo "untyped=", $untyped->value, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "untyped\nuntyped=5\n", "{}", out.stderr);
}

/// Method receivers and by-reference variadics share the same live property cell as direct
/// function calls.
#[test]
fn test_property_reference_arguments_stay_live_through_methods_and_variadics() {
    let out = compile_and_run_capture(
        r#"<?php
class RefLiveTarget { public $value = 1; }
class RefLiveMethods {
    public function bump(&$slot): void { $slot = 11; }
    public function bumpAll(&...$slots): void { $slots[0] = 13; }
}
$owner = new RefLiveTarget();
$methods = new RefLiveMethods();
$methods->bump($owner->value);
echo "method=", $owner->value, "\n";
$methods->bumpAll($owner->value);
echo "variadic=", $owner->value, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "method=11\nvariadic=13\n",
        "{}",
        out.stderr
    );
}

/// A union-typed property is checked by its CURRENT value at the by-reference call boundary:
/// a compatible member reaches the callee, while an incompatible member raises PHP's argument
/// `TypeError` with the runtime type and leaves the property unchanged.
#[test]
fn test_union_property_reference_argument_checks_the_current_value() {
    let out = compile_and_run_capture(
        r#"<?php
class RefUnionIntString { public int|string $value = 1; }
class RefUnionIntFloat { public int|float $value = 1; }
class RefStaticMismatch {
    public array $items = [];
    public static function set(int &$slot): void {}
}
function ref_union_inc(int &$slot): void { $slot++; }
$ints = new RefUnionIntString();
ref_union_inc($ints->value);
echo "int-string=", $ints->value, "\n";
$ints->value = "x";
try { ref_union_inc($ints->value); } catch (TypeError $error) { echo $error->getMessage(), "\n"; }
echo "int-string-after=", $ints->value, "\n";
$floats = new RefUnionIntFloat();
ref_union_inc($floats->value);
echo "int-float=", $floats->value, "\n";
$floats->value = 1.5;
try { ref_union_inc($floats->value); } catch (TypeError $error) { echo $error->getMessage(), "\n"; }
echo "int-float-after=", $floats->value, "\n";
$static = new RefStaticMismatch();
try { RefStaticMismatch::set($static->items); } catch (TypeError $error) { echo $error->getMessage(), "\n"; }
echo "static-after=", count($static->items), "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "int-string=2\nref_union_inc(): Argument #1 ($slot) must be of type int, string given\nint-string-after=x\nint-float=2\nref_union_inc(): Argument #1 ($slot) must be of type int, float given\nint-float-after=1.5\n\
RefStaticMismatch::set(): Argument #1 ($slot) must be of type int, array given\n\
static-after=0\n",
        "{}",
        out.stderr
    );
}

/// An UNTYPED property is PHP's ordinary writable zval: a weak call coerces its current value
/// to the declared by-reference parameter and the callee's result is written back. A value the
/// weak predicate refuses raises the call-site `TypeError` without changing the property.
/// A declared `mixed` property is different and keeps its strict current-value check.
#[test]
fn test_untyped_property_reference_argument_weakly_coerces_the_current_value() {
    let out = compile_and_run_capture(
        r#"<?php
class RefUntypedScalar { public $value; }
class RefUntypedInitialized { public $value = "7"; }
function ref_untyped_inc(int &$slot): void { $slot++; }
function ref_untyped_append(string &$slot): void { $slot .= "x"; }
$item = new RefUntypedScalar();
$item->value = "42";
ref_untyped_inc($item->value);
echo "int=", $item->value, "\n";
$item->value = "abc";
try { ref_untyped_inc($item->value); } catch (TypeError $error) { echo $error->getMessage(), "\n"; }
echo "after=", $item->value, "\n";
$item->value = 42;
ref_untyped_append($item->value);
echo "string=", $item->value, "\n";
$initialized = new RefUntypedInitialized();
ref_untyped_inc($initialized->value);
echo "initialized=", $initialized->value, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "int=43\nref_untyped_inc(): Argument #1 ($slot) must be of type int, string given\nafter=abc\nstring=42x\ninitialized=8\n",
        "{}",
        out.stderr
    );
}

/// Verifies a property by-reference `TypeError` is raised at the call boundary: every later
/// argument still runs (and its output is visible) before the callee entry check fires.
/// The property keeps its previous value in both the concrete-disjoint and untyped cases.
#[test]
fn test_property_reference_type_error_waits_for_later_arguments() {
    let out = compile_and_run_capture(
        r#"<?php
class RefDeferredBox { public string $text = "old"; }
class RefDeferredUntyped { public $value; }
function ref_deferred_take(int &$slot, int $other): void { echo "body|"; }
function ref_deferred_later(): int { echo "later|"; return 2; }
$box = new RefDeferredBox();
try { ref_deferred_take($box->text, ref_deferred_later()); } catch (TypeError $error) { echo $error->getMessage(), "|"; }
echo $box->text, "\n";
$untyped = new RefDeferredUntyped();
$untyped->value = "abc";
try { ref_deferred_take($untyped->value, ref_deferred_later()); } catch (TypeError $error) { echo $error->getMessage(), "|"; }
echo $untyped->value, "\n";
$untyped->value = "42";
ref_deferred_take($untyped->value, ref_deferred_later());
echo $untyped->value, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "later|ref_deferred_take(): Argument #1 ($slot) must be of type int, string given|old\nlater|ref_deferred_take(): Argument #1 ($slot) must be of type int, string given|abc\nlater|body|42\n",
        "{}",
        out.stderr
    );
}

/// Verifies a typed by-reference property argument accepts the current value when the
/// parameter is a union that contains it, and when a related class is involved in either
/// direction. PHP checks the live value at the call boundary; a representation difference
/// alone must not refuse the call.
#[test]
fn test_reference_property_related_and_union_parameters_use_the_current_value() {
    let out = compile_and_run(
        r#"<?php
class RefRelatedBase { }
class RefRelatedChild extends RefRelatedBase { }
class RefRelatedHolder {
    public int|string $union = 3;
    public RefRelatedChild $child;
    public RefRelatedBase $base;
    function __construct() { $this->child = new RefRelatedChild; $this->base = new RefRelatedChild; }
}
function ref_related_union(int|string &$slot) { $slot = 9; }
function ref_related_base(RefRelatedBase &$slot) { echo "base|"; }
function ref_related_child(RefRelatedChild &$slot) { echo "child|"; }
$holder = new RefRelatedHolder;
ref_related_union($holder->union);
echo $holder->union, "\n";
ref_related_base($holder->child);
ref_related_child($holder->base);
echo "\n";
"#,
    );
    assert_eq!(out, "9\nbase|child|\n");
}

/// Verifies a declared `mixed` or union property holding an int does not bind to `float &`:
/// PHP disables the int-to-float widening for typed references, while an untyped slot still
/// weakly coerces its current value.
#[test]
fn test_reference_float_parameter_rejects_int_in_typed_slots() {
    let out = compile_and_run(
        r#"<?php
class RefFloatMixed { public mixed $value = 3; }
class RefFloatUnion { public int|float $value = 3; }
class RefFloatUntyped { public $value = 3; }
function ref_float_take(float &$slot) { $slot = 3.5; }
$mixed = new RefFloatMixed;
try { ref_float_take($mixed->value); echo "ran|"; } catch (TypeError $error) { echo "typed|"; }
var_dump($mixed->value);
$union = new RefFloatUnion;
try { ref_float_take($union->value); echo "ran|"; } catch (TypeError $error) { echo "typed|"; }
var_dump($union->value);
$untyped = new RefFloatUntyped;
ref_float_take($untyped->value);
var_dump($untyped->value);
"#,
    );
    assert_eq!(
        out,
        "typed|int(3)\ntyped|int(3)\nfloat(3.5)\n"
    );
}

/// Verifies weak coercion of an untyped property into a `bool &` parameter uses PHP truthiness
/// rather than the merge-temp integer cast: `"x"` is true and `"0.0"` is false.
#[test]
fn test_reference_bool_parameter_coerces_untyped_property_truthiness() {
    let out = compile_and_run(
        r#"<?php
class RefBoolBox { public $value; }
function ref_bool_take(bool &$slot) { var_dump($slot); }
foreach (["x", "0.0", "a", " ", "", "0"] as $value) {
    $box = new RefBoolBox;
    $box->value = $value;
    ref_bool_take($box->value);
}
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\nbool(true)\nbool(false)\nbool(false)\n");
}

/// Verifies the deferred check re-reads the property at the call boundary, so a later
/// argument that rewrites the same property decides both the TypeError and the callee value.
#[test]
fn test_reference_property_guard_observes_later_argument_writes() {
    let out = compile_and_run(
        r#"<?php
class RefGuardBox { public $value; }
function ref_guard_one(int &$slot, int $other) { echo $slot, ":", $other, "\n"; }
function ref_guard_two(int &$slot, $other) { echo gettype($slot), ":"; var_dump($slot); }
$first = new RefGuardBox;
$first->value = "bad";
try { ref_guard_one($first->value, $first->value = 5); } catch (TypeError $error) { echo "bad|"; }
var_dump($first->value);
$second = new RefGuardBox;
$second->value = 5;
try { ref_guard_two($second->value, $second->value = "bad"); } catch (TypeError $error) { echo "bad|"; }
var_dump($second->value);
"#,
    );
    assert_eq!(out, "5:5\nint(5)\nbad|string(3) \"bad\"\n");
}

/// Verifies a trait body's by-reference argument guard is resolved for each using class,
/// even though the body's source spans are shared by every class that uses the trait.
#[test]
fn test_reference_trait_body_uses_each_using_class_property() {
    let out = compile_and_run(
        r#"<?php
function ref_trait_take(int &$slot) { $slot++; }
trait RefTraitBody { function run() { ref_trait_take($this->value); var_dump($this->value); } }
class RefTraitInt { use RefTraitBody; public int $value = 1; }
class RefTraitString { use RefTraitBody; public string $value = "s"; }
class RefTraitMixed { use RefTraitBody; public mixed $value = 5; }
(new RefTraitInt)->run();
try { (new RefTraitString)->run(); } catch (TypeError $error) { echo "typed|"; }
(new RefTraitMixed)->run();
"#,
    );
    assert_eq!(out, "int(2)\ntyped|int(6)\n");
}

/// Verifies widening an untyped property for a by-reference argument covers subclasses that
/// redeclare it, so inherited methods and later accesses see one consistent slot.
#[test]
fn test_reference_untyped_property_widening_covers_redeclaring_subclass() {
    let out = compile_and_run(
        r#"<?php
function ref_widen_take(int &$slot) { $slot++; }
function ref_widen_mixed(mixed &$slot) { $slot .= "!"; }
class RefWidenBase { public $value = "5"; function show() { var_dump($this->value); } function append() { $this->value .= "z"; } }
class RefWidenChild extends RefWidenBase { public $value = "abcdefgh"; }
$child = new RefWidenChild;
$child->show();
$child->append();
var_dump($child->value);
$base = new RefWidenBase;
ref_widen_take($base->value);
var_dump($base->value);
ref_widen_mixed($base->value);
var_dump($base->value);
$child = new RefWidenChild;
$child->value = "7";
ref_widen_take($child->value);
var_dump($child->value);
"#,
    );
    assert_eq!(
        out,
        "string(8) \"abcdefgh\"\nstring(9) \"abcdefghz\"\nint(6)\nstring(2) \"6!\"\nint(8)\n"
    );
}

/// Verifies taking a nested property by reference when an intermediate hop is null raises
/// PHP's modification Error before the callee, instead of warning and running the callee.
#[test]
fn test_reference_nested_null_receiver_raises_modify_error() {
    let out = compile_and_run(
        r#"<?php
class RefNullNode { public ?RefNullNode $next = null; public int $value = 0; }
function ref_null_take(&$slot) { echo "callee|"; }
$node = new RefNullNode;
try { ref_null_take($node->next->value); } catch (Error $error) { echo $error->getMessage(), "\n"; }
"#,
    );
    assert_eq!(out, "Attempt to modify property \"value\" on null\n");
}

/// Verifies a rejected by-reference property argument raises the argument TypeError without
/// converting the refreshed value first, so an array/object never emits a conversion warning
/// or runs `__toString()` before the throw.
#[test]
fn test_reference_rejected_refreshed_value_is_not_converted() {
    let out = compile_and_run(
        r#"<?php
class RefConvertBox { public mixed $value = "start"; }
class RefConvertStringable { public function __toString(): string { echo "tostring|"; return "x"; } }
function ref_convert_later(RefConvertBox $box): int { $box->value = new RefConvertStringable; return 1; }
function ref_convert_take(string &$slot, int $ignored): void { echo "body|"; }
$box = new RefConvertBox;
try { ref_convert_take($box->value, ref_convert_later($box)); }
catch (TypeError $error) { echo $error->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "ref_convert_take(): Argument #1 ($slot) must be of type string, RefConvertStringable given\n"
    );
}

/// Verifies a later argument that reassigns the receiver variable does not retarget the
/// by-reference binding, the call-boundary guard, or the write-back: PHP keeps the original
/// object, even when the replacement holds a different current type.
#[test]
fn test_reference_property_binding_keeps_the_original_receiver() {
    let out = compile_and_run(
        r#"<?php
class RefReceiverBox { public mixed $value; function __construct($value) { $this->value = $value; } }
function ref_receiver_take(int &$slot, $other) { $slot = 7; }
$box = new RefReceiverBox(1);
$original = $box;
$replacement = new RefReceiverBox("bad");
ref_receiver_take($box->value, $box = $replacement);
var_dump($original->value, $box->value);
"#,
    );
    assert_eq!(out, "int(7)\nstring(3) \"bad\"\n");
}

/// Verifies a by-reference property argument that needs a runtime guard evaluates its
/// receiver expression once, and uses that same object for both the guard and write-back.
#[test]
fn test_reference_property_guard_evaluates_the_receiver_once() {
    let out = compile_and_run(
        r#"<?php
class RefOnceBox {
    public static array $created = [];
    public mixed $value;
    public function __construct($value) { $this->value = $value; self::$created[] = $this; }
}
function ref_once_make(): RefOnceBox { echo "make|"; return new RefOnceBox(count(RefOnceBox::$created) + 1); }
function ref_once_take(int &$slot, $other) { $slot = 7; }
ref_once_take(ref_once_make()->value, ref_once_make());
echo RefOnceBox::$created[0]->value, "|", RefOnceBox::$created[1]->value, "\n";
"#,
    );
    assert_eq!(out, "make|make|7|2\n");
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

/// Verifies `settype()` on a typed by-reference parameter with the matching native payload is
/// accepted: the cell already has the requested representation, so the write goes through it
/// instead of failing as an unsupported retype.
#[test]
fn test_settype_matching_typed_reference_payload_writes_through() {
    let out = compile_and_run(
        r#"<?php
function keep_int(int &$x): void { var_dump(settype($x, 'int')); var_dump($x); }
function keep_float(float &$f): void { settype($f, 'float'); var_dump($f); }
function keep_string(string &$s): void { settype($s, 'string'); var_dump($s); }
function keep_bool(bool &$b): void { settype($b, 'bool'); var_dump($b); }
$n = 7; keep_int($n); var_dump($n);
$f = 1.5; keep_float($f); var_dump($f);
$s = 'x'; keep_string($s); var_dump($s);
$b = true; keep_bool($b); var_dump($b);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nint(7)\nint(7)\nfloat(1.5)\nfloat(1.5)\nstring(1) \"x\"\nstring(1) \"x\"\nbool(true)\nbool(true)\n"
    );
}

/// Verifies `settype(..., "string")` through a `mixed` reference publishes persisted storage:
/// scalar-to-string conversions may start in `_concat_buf`, which later string work reuses.
#[test]
fn test_settype_string_reference_persists_concat_scratch() {
    let out = compile_and_run(
        r#"<?php
function ref_string(mixed &$slot): void { var_dump(settype($slot, 'string')); }
$int = 42;
ref_string($int);
for ($i = 0; $i < 4000; $i++) { $scratch = (string) $i; }
var_dump($int);
$float = 1.5;
ref_string($float);
$scratch = str_repeat("z", 100) . "tail";
var_dump($float);
$bool = true;
ref_string($bool);
$scratch = "a" . "b" . "c";
var_dump($bool);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nstring(2) \"42\"\nbool(true)\nstring(3) \"1.5\"\nbool(true)\nstring(1) \"1\"\n"
    );
}

/// Verifies a declared `mixed` property passed to a scalar union by reference keeps the
/// call-boundary check: both sides use Mixed storage, but the union admits fewer values than
/// the property. The array must raise before the callee runs and leave the property unchanged.
#[test]
fn test_mixed_property_to_scalar_union_reference_checks_before_the_call() {
    let out = compile_and_run(
        r#"<?php
class RefUnionLiveBox { public mixed $value = []; }
function ref_union_live(int|string &$slot): void { echo "entered|"; }
$box = new RefUnionLiveBox();
try { ref_union_live($box->value); } catch (TypeError $error) { echo "TypeError|"; }
var_dump($box->value);
$box->value = "ok";
ref_union_live($box->value);
var_dump($box->value);
"#,
    );
    assert_eq!(
        out,
        "TypeError|array(0) {\n}\nentered|string(2) \"ok\"\n"
    );
}

/// Verifies reference-property widening walks through a parent that only inherits the slot:
/// `A` declares it, `B` inherits it, `C` redeclares it, and inherited methods in both `A` and
/// `B` must read the widened `C` slot instead of a stale narrow representation.
#[test]
fn test_reference_property_widening_passes_through_inheriting_parent() {
    let out = compile_and_run(
        r#"<?php
class WidenChainA {
    public $value = "a";
    public function showA(): void { var_dump($this->value); }
}
class WidenChainB extends WidenChainA {
    public function showB(): void { var_dump($this->value); }
}
class WidenChainC extends WidenChainB { public $value = "c"; }
function widen_chain_bump(int &$slot): void { $slot++; }
$c = new WidenChainC();
$c->value = "7";
widen_chain_bump($c->value);
$c->showA();
$c->showB();
$b = new WidenChainB();
$b->value = "9";
widen_chain_bump($b->value);
$b->showA();
$b->showB();
"#,
    );
    assert_eq!(out, "int(8)\nint(8)\nint(10)\nint(10)\n");
}

/// Verifies reference-property widening does not cross a private declaration: `A`'s private
/// `$p` and `B`'s public `$p` are separate slots, so widening `B`'s slot leaves `A::show()`
/// reading its own private string.
#[test]
fn test_reference_property_widening_stops_at_private_declarations() {
    let out = compile_and_run(
        r#"<?php
class WidenPrivateA { private $p = "old"; public function show(): void { var_dump($this->p); } }
class WidenPrivateB extends WidenPrivateA { public $p = 1; }
function widen_private_bump(int &$slot): void { $slot++; }
$b = new WidenPrivateB();
$b->p = "7";
widen_private_bump($b->p);
var_dump($b->p);
$b->show();
"#,
    );
    assert_eq!(out, "int(8)\nstring(3) \"old\"\n");
}
