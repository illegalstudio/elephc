//! Purpose:
//! Integration tests for null-capable int properties (`?int` / `int|null`), whose slots use the
//! inline two-word `{payload, tag}` TaggedScalar storage under `NullRepr::Tagged`. Pins the
//! literal-default initializer, the read paths, and the sibling nullable scalar types that must
//! stay on their existing representations.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Regression for the silent miscompile where a non-null literal default (`public ?int $p = 1;`)
//!   was boxed into a Mixed cell and written into the payload word of a TaggedScalar slot, so the
//!   reader handed the cell pointer back as an integer.
//! - Also pins the mixed-receiver property read (an object reached through a heterogeneous array),
//!   which loaded the payload into the register still holding the object pointer and then
//!   dereferenced it as the tag address.
//! - Every fixture is compiled with `compile_and_run_tagged` so the tagged representation is
//!   exercised regardless of `ELEPHC_NULL_REPR`; expected outputs are `LC_ALL=C php` 8.4 output.

use super::*;

/// Verifies the reported repro: a `?int` property with a non-null literal default reads back as
/// that integer instead of the address of a boxed Mixed cell.
#[test]
fn test_nullable_int_property_non_null_default_reads_back() {
    let out = compile_and_run_tagged(
        r#"<?php
class M { public ?int $foo = 1; }
$m = new M();
var_dump($m->foo);
"#,
    );
    assert_eq!(out, "int(1)\n");
}

/// Verifies negative and zero literal defaults on `?int` slots, since a negated literal takes a
/// different arm of the literal-default classifier than a plain integer literal.
#[test]
fn test_nullable_int_property_negative_and_zero_defaults() {
    let out = compile_and_run_tagged(
        r#"<?php
class M { public ?int $a = -7; public ?int $b = 0; }
$m = new M();
var_dump($m->a);
var_dump($m->b);
"#,
    );
    assert_eq!(out, "int(-7)\nint(0)\n");
}

/// Verifies the explicit `int|null` spelling takes the same inline tagged-scalar storage as `?int`,
/// on both an instance and a static property.
#[test]
fn test_explicit_int_null_union_property_defaults() {
    let out = compile_and_run_tagged(
        r#"<?php
class M { public int|null $a = 1; public static int|null $b = -2; }
$m = new M();
var_dump($m->a);
var_dump(M::$b);
$m->a = null;
var_dump($m->a);
"#,
    );
    assert_eq!(out, "int(1)\nint(-2)\nNULL\n");
}

/// Verifies an explicit `= null` default on a `?int` slot still reads as null and is not `isset`.
#[test]
fn test_nullable_int_property_null_default() {
    let out = compile_and_run_tagged(
        r#"<?php
class M { public ?int $foo = null; }
$m = new M();
var_dump($m->foo);
var_dump(isset($m->foo));
"#,
    );
    assert_eq!(out, "NULL\nbool(false)\n");
}

/// Verifies a `?int` slot round-trips through runtime assignments in both directions: int, back to
/// null, and back to another int.
#[test]
fn test_nullable_int_property_runtime_assignment_round_trip() {
    let out = compile_and_run_tagged(
        r#"<?php
class M { public ?int $foo = 1; }
$m = new M();
$m->foo = 99;
var_dump($m->foo);
$m->foo = null;
var_dump($m->foo);
$m->foo = 7;
var_dump($m->foo);
"#,
    );
    assert_eq!(out, "int(99)\nNULL\nint(7)\n");
}

/// Verifies every ordinary reader over a non-null `?int` property: echo, print_r, interpolation,
/// arithmetic, strict comparison, is_null, `??`, string cast, and isset.
#[test]
fn test_nullable_int_property_readers_when_set() {
    let out = compile_and_run_tagged(
        r#"<?php
class M { public ?int $foo = 1; }
$m = new M();
echo $m->foo, "\n";
print_r($m->foo);
echo "\n";
echo "v={$m->foo}\n";
var_dump($m->foo + 1);
var_dump($m->foo === 1);
var_dump(is_null($m->foo));
var_dump($m->foo ?? 99);
var_dump((string) $m->foo);
var_dump(isset($m->foo));
"#,
    );
    assert_eq!(
        out,
        "1\n1\nv=1\nint(2)\nbool(true)\nbool(false)\nint(1)\nstring(1) \"1\"\nbool(true)\n"
    );
}

/// Verifies the same readers over a `?int` property holding null: PHP renders null as the empty
/// string in echo/print_r/interpolation and reports it through the null-aware predicates.
#[test]
fn test_nullable_int_property_readers_when_null() {
    let out = compile_and_run_tagged(
        r#"<?php
class M { public ?int $foo = 1; }
$m = new M();
$m->foo = null;
echo "[", $m->foo, "]\n";
print_r($m->foo);
echo "\n";
echo "v={$m->foo}\n";
var_dump($m->foo === null);
var_dump(is_null($m->foo));
var_dump($m->foo ?? 99);
var_dump(isset($m->foo));
"#,
    );
    assert_eq!(
        out,
        "[]\n\nv=\nbool(true)\nbool(true)\nint(99)\nbool(false)\n"
    );
}

/// Verifies a promoted constructor property typed `?int`: the promoted default, an explicit int
/// argument, and an explicit null argument.
#[test]
fn test_nullable_int_promoted_constructor_property() {
    let out = compile_and_run_tagged(
        r#"<?php
class P { public function __construct(public ?int $v = 5) {} }
var_dump((new P())->v);
var_dump((new P(9))->v);
var_dump((new P(null))->v);
"#,
    );
    assert_eq!(out, "int(5)\nint(9)\nNULL\n");
}

/// Verifies `?int` static properties: both literal default forms and assignments in both
/// directions. Static slots take the same literal-default classifier as instance slots.
#[test]
fn test_nullable_int_static_property_defaults_and_assignment() {
    let out = compile_and_run_tagged(
        r#"<?php
class S { public static ?int $n = 42; public static ?int $z = null; }
var_dump(S::$n);
var_dump(S::$z);
S::$n = null;
var_dump(S::$n);
S::$z = -3;
var_dump(S::$z);
"#,
    );
    assert_eq!(out, "int(42)\nNULL\nNULL\nint(-3)\n");
}

/// Verifies `clone` copies a `?int` slot faithfully for the default value, an assigned int, and
/// null — the whole two-word slot has to survive the copy, not just the payload.
#[test]
fn test_nullable_int_property_survives_clone() {
    let out = compile_and_run_tagged(
        r#"<?php
class M { public ?int $foo = 1; }
$a = new M();
$b = clone $a;
var_dump($b->foo);
$a->foo = 123;
$c = clone $a;
var_dump($c->foo);
$a->foo = null;
$d = clone $a;
var_dump($d->foo);
"#,
    );
    assert_eq!(out, "int(1)\nint(123)\nNULL\n");
}

/// Verifies the sibling nullable scalar property types and `mixed` keep their existing
/// representations: only `?int` moves to the inline tagged-scalar storage.
#[test]
fn test_sibling_nullable_scalar_property_defaults_unaffected() {
    let out = compile_and_run_tagged(
        r#"<?php
class F { public ?float $v = 1.5; }
class B { public ?bool $v = true; }
class T { public ?string $v = "hi"; }
class X { public mixed $v = 1; }
var_dump((new F())->v);
var_dump((new B())->v);
var_dump((new T())->v);
var_dump((new X())->v);
"#,
    );
    assert_eq!(out, "float(1.5)\nbool(true)\nstring(2) \"hi\"\nint(1)\n");
}

/// Verifies a `?int` property read through an object stored in an array, both when the array is
/// homogeneous (a direct object slot) and when it is heterogeneous (a Mixed element whose read
/// goes through the runtime class dispatch).
#[test]
fn test_nullable_int_property_read_through_object_in_array() {
    let out = compile_and_run_tagged(
        r#"<?php
class A { public ?int $a = 1; }
class B { public ?int $b = null; }
$same = [new A(), new A()];
var_dump($same[0]->a);
$mixed = [new A(), new B()];
var_dump($mixed[0]->a);
var_dump($mixed[1]->b);
"#,
    );
    assert_eq!(out, "int(1)\nint(1)\nNULL\n");
}

/// Verifies the whole point of the tagged representation on property slots: the integer that
/// collides with the legacy in-band null sentinel (`PHP_INT_MAX - 1`) is a real value in an
/// instance and a static `?int` property, not null.
#[test]
fn test_nullable_int_property_default_at_sentinel_bit_pattern() {
    let out = compile_and_run_tagged(
        r#"<?php
class M { public ?int $foo = 9223372036854775806; public static ?int $bar = 9223372036854775806; }
$m = new M();
var_dump($m->foo);
var_dump(M::$bar);
var_dump($m->foo === 9223372036854775806);
var_dump(is_null($m->foo));
"#,
    );
    assert_eq!(
        out,
        "int(9223372036854775806)\nint(9223372036854775806)\nbool(true)\nbool(false)\n"
    );
}

/// Verifies every whole-object reader renders a `?int` property out of its own two-word slot.
///
/// The per-class descriptor tables carry one static runtime value tag per property, and a
/// `TaggedScalar` slot has no such tag — its tag lives in the slot's second word. Claiming
/// tag 7 ("the slot holds a boxed Mixed cell pointer") made every reader below take the raw
/// payload for a cell address and dereference `NULL_SENTINEL` (`0x7ffffffffffffffe`), which
/// is why one fixture covers all of them: they share the descriptor, not the code.
/// Expected output is `LC_ALL=C php -n` 8.4 output, byte for byte.
#[test]
fn test_nullable_int_property_renders_through_every_object_reader() {
    let out = compile_and_run_tagged(
        r#"<?php
class A { public ?int $n = null; public ?int $v = 5; }
$a = new A();
var_dump($a);
print_r($a);
var_export($a);
echo "\n";
echo json_encode($a), "\n";
echo serialize($a), "\n";
var_dump(get_object_vars($a));
var_dump((array) $a);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "object(A)#1 (2) {\n  [\"n\"]=>\n  NULL\n  [\"v\"]=>\n  int(5)\n}\n",
            "A Object\n(\n    [n] => \n    [v] => 5\n)\n",
            "\\A::__set_state(array(\n   'n' => NULL,\n   'v' => 5,\n))\n",
            "{\"n\":null,\"v\":5}\n",
            "O:1:\"A\":2:{s:1:\"n\";N;s:1:\"v\";i:5;}\n",
            "array(2) {\n  [\"n\"]=>\n  NULL\n  [\"v\"]=>\n  int(5)\n}\n",
            "array(2) {\n  [\"n\"]=>\n  NULL\n  [\"v\"]=>\n  int(5)\n}\n",
        )
    );
}

/// Verifies the `int|null` union spelling reaches the same readers as `?int`, and that the
/// sibling nullable scalar types — which stay on the boxed Mixed representation, so they were
/// never affected — keep rendering the same way beside it.
#[test]
fn test_explicit_int_null_union_and_sibling_nullable_types_render_together() {
    let out = compile_and_run_tagged(
        r#"<?php
class A {
    public int|null $u = null;
    public int|null $w = 7;
    public ?float $f = null;
    public ?string $s = null;
    public ?bool $b = null;
    public ?array $a = null;
}
$o = new A();
echo json_encode($o), "\n";
echo serialize($o), "\n";
foreach ((array) $o as $k => $v) { echo $k, "="; var_dump($v); }
"#,
    );
    assert_eq!(
        out,
        concat!(
            "{\"u\":null,\"w\":7,\"f\":null,\"s\":null,\"b\":null,\"a\":null}\n",
            "O:1:\"A\":6:{s:1:\"u\";N;s:1:\"w\";i:7;s:1:\"f\";N;s:1:\"s\";N;s:1:\"b\";N;s:1:\"a\";N;}\n",
            "u=NULL\nw=int(7)\nf=NULL\ns=NULL\nb=NULL\na=NULL\n",
        )
    );
}

/// Verifies the exact `__serialize()` shape Symfony's `DependencyInjection\Definition` uses,
/// which is what turned this descriptor bug into a SIGSEGV on every `GET /`: the `(array)`
/// cast boxes each property slot and `!$v` then casts that box to bool. A null `?int` takes
/// the `continue` arm and a non-null one is kept, so both tagged payloads reach the cast.
#[test]
fn test_nullable_int_property_survives_the_cast_then_bool_serialize_shape() {
    let out = compile_and_run_tagged(
        r#"<?php
class Definition
{
    private ?int $factory = null;
    private ?int $priority = 3;
    private bool $shared = false;
    private ?string $class = null;
    private array $arguments = [];

    public function __serialize(): array
    {
        $serialized = [];
        foreach ((array) $this as $k => $v) {
            if (!$v xor 'shared' === $k) {
                continue;
            }
            $k = str_replace("\0Definition\0", '', $k);
            $serialized[$k] = $v;
        }
        return $serialized;
    }
}
$d = new Definition();
print_r($d->__serialize());
var_dump(serialize($d));
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Array\n(\n    [priority] => 3\n)\n",
            "string(41) \"O:10:\"Definition\":1:{s:8:\"priority\";i:3;}\"\n",
        )
    );
}

/// Verifies PHP's `uninitialized(...)` line for a typed, default-less `?int` property, and that
/// the readers that omit an uninitialized property still omit it now that a tagged scalar's own
/// tag word is the same word the uninitialized marker lives in.
#[test]
fn test_uninitialized_nullable_int_property_is_named_and_omitted() {
    let out = compile_and_run_tagged(
        r#"<?php
class U { public ?int $n; public ?string $s; public int $seen = 1; }
$u = new U();
var_dump($u);
var_dump((array) $u);
var_dump(get_object_vars($u));
$u->n = 4;
var_dump($u);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "object(U)#1 (1) {\n  [\"n\"]=>\n  uninitialized(?int)\n",
            "  [\"s\"]=>\n  uninitialized(?string)\n  [\"seen\"]=>\n  int(1)\n}\n",
            "array(1) {\n  [\"seen\"]=>\n  int(1)\n}\n",
            "array(1) {\n  [\"seen\"]=>\n  int(1)\n}\n",
            "object(U)#1 (2) {\n  [\"n\"]=>\n  int(4)\n",
            "  [\"s\"]=>\n  uninitialized(?string)\n  [\"seen\"]=>\n  int(1)\n}\n",
        )
    );
}

/// Verifies `==` between two instances compares `?int` properties by value. `__rt_obj_loose_eq`
/// walks the same descriptor through `__rt_obj_prop_value`, so it used to segfault on the first
/// property instead of reporting a comparison.
#[test]
fn test_nullable_int_properties_compare_loosely_by_value() {
    let out = compile_and_run_tagged(
        r#"<?php
class A { public ?int $n = null; public ?int $v = 5; }
$a = new A();
$b = new A();
var_dump($a == $b);
$b->n = 3;
var_dump($a == $b);
$b->n = null;
var_dump($a == $b);
$b->v = null;
var_dump($a == $b);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(false)\nbool(true)\nbool(false)\n");
}

/// Verifies the readers a `?int` STATIC property reaches. A static slot is addressed by its
/// compile-time type rather than through the per-class descriptor tables, so this pins that the
/// two storage paths agree on what a `?int` holding null and one holding a value print.
#[test]
fn test_nullable_int_static_property_renders_through_the_readers() {
    let out = compile_and_run_tagged(
        r#"<?php
class S { public static ?int $n = null; public static ?int $v = 9; }
var_dump(S::$n);
var_dump(S::$v);
print_r(S::$n);
echo "|";
print_r(S::$v);
echo "|\n";
var_export(S::$n);
echo "\n";
var_export(S::$v);
echo "\n";
S::$n = 12;
S::$v = null;
var_dump(S::$n === 12, S::$v === null);
"#,
    );
    assert_eq!(out, "NULL\nint(9)\n|9|\nNULL\n9\nbool(true)\nbool(true)\n");
}

/// Verifies a `?int` property slot owns nothing on the heap.
///
/// The gc descriptor used to call a `?int` slot a boxed Mixed cell too, which handed the raw
/// payload — a PHP int, or the null sentinel — to `__rt_decref_any` as a heap pointer on every
/// object release.
#[test]
fn test_nullable_int_property_readers_leave_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class A { public ?int $n = null; public ?int $v = 5; }
$a = new A();
var_dump($a);
print_r($a);
var_export($a);
echo json_encode($a), serialize($a), "\n";
var_dump(get_object_vars($a));
var_dump((array) $a);
"#,
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}
