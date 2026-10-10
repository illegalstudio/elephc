//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of a property write whose
//! receiver is statically `mixed` or an object union, which must reach the declared slot of
//! whatever class the payload turns out to be rather than only stdClass.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout.
//! - Expected output is verbatim `LC_ALL=C php` 8.5.10.

use super::*;

/// Issue #1094's own repro: the write is not landing in a copy, it is not landing at all, so the
/// callee's own read-back is already wrong.
#[test]
fn test_mixed_receiver_property_write_reaches_a_declared_class() {
    let out = compile_and_run(
        r#"<?php
class T { public int $n = 1; }
function w(mixed $o): void { $o->n = 9; var_dump($o->n); }
$t = new T();
w($t);
var_dump($t->n);
"#,
    );
    assert_eq!(out, "int(9)\nint(9)\n");
}

/// An object UNION receiver takes the same lowering path as `mixed`.
#[test]
fn test_object_union_receiver_property_write_reaches_the_declared_class() {
    let out = compile_and_run(
        r#"<?php
class T { public int $n = 1; }
function w(T|int $o): void { $o->n = 9; }
$t = new T();
w($t);
var_dump($t->n);
"#,
    );
    assert_eq!(out, "int(9)\n");
}

/// The property's own declaration does not decide this: a typed `string`, an untyped property and
/// a `float` all took the same discarded path.
#[test]
fn test_mixed_receiver_writes_typed_string_and_float_properties() {
    let out = compile_and_run(
        r#"<?php
class T { public string $s = "a"; public float $f = 1.5; }
function ws(mixed $o): void { $o->s = "z"; }
function wf(mixed $o): void { $o->f = 2.5; }
$t = new T();
ws($t);
wf($t);
var_dump($t->s, $t->f);
"#,
    );
    assert_eq!(out, "string(1) \"z\"\nfloat(2.5)\n");
}

/// A same-type write into a refined untyped slot now lands: PHP stores the value as-is, and the
/// runtime tag matches the slot's refined representation.
#[test]
fn test_mixed_receiver_write_accepts_a_matching_refined_untyped_property() {
    let out = compile_and_run(
        r#"<?php
class T { public $u = 1; }
function write(mixed $o): void { $o->u = 9; }
$t = new T();
write($t);
var_dump($t->u);
"#,
    );
    assert_eq!(out, "int(9)\n");
}

/// The issue's reproduction: int- and string-refined untyped properties both accept a same-type
/// write through a `mixed` receiver, matching PHP's `6b`.
#[test]
fn test_mixed_receiver_write_reaches_untyped_refined_scalars() {
    let out = compile_and_run(
        r#"<?php
class T { public $pub = 3; public $s = "a"; }
function direct(mixed $o): void { $o->pub = 6; $o->s = "b"; }
$t = new T();
direct($t);
echo $t->pub, $t->s, "\n";
"#,
    );
    assert_eq!(out, "6b\n");
}

/// A runtime type the refined slot cannot represent still fails closed rather than coercing it,
/// which would diverge from PHP's store-as-is rule for an untyped property.
#[test]
fn test_mixed_receiver_write_refuses_a_mismatched_refined_untyped_property() {
    let out = compile_and_run_capture(
        r#"<?php
class T { public $u = 1; }
function write(mixed $o): void { $o->u = "z"; }
        write(new T());
"#,
    );
    let diagnostic = format!("{}{}", out.stdout, out.stderr);
    assert!(
        !out.success,
        "mismatched refined untyped property write unexpectedly succeeded"
    );
    assert!(
        diagnostic.contains("Unsupported dynamic property write: runtime Mixed value cannot be stored safely in the refined untyped property T::$u"),
        "output: {}",
        diagnostic
    );
}

/// A runtime value of an UNRELATED class must be refused, not stored and then read back through
/// the refined class's offsets (#1319 review). The slot's layout belongs to the refined class
/// alone, so a foreign object would read the wrong words.
#[test]
fn test_mixed_receiver_write_refuses_a_mismatched_refined_untyped_object_property() {
    let out = compile_and_run_capture(
        r#"<?php
class A { public $x = 10; public $n = 3; }
class B { public $n = 2; }
class H { public $o; function __construct() { $this->o = new A(); } }
function w(mixed $h, mixed $v): void { $h->o = $v; }
$h = new H();
w($h, new B());
var_dump($h->o->n);
"#,
    );
    let diagnostic = format!("{}{}", out.stdout, out.stderr);
    assert!(
        !out.success,
        "an unrelated class into a refined object slot unexpectedly succeeded"
    );
    assert!(
        diagnostic.contains("Unsupported dynamic property write: runtime Mixed value cannot be stored safely in the refined untyped property H::$o"),
        "output: {}",
        diagnostic
    );
}

/// A same-class or subclass runtime value still lands in the refined untyped object slot: the
/// slot's layout is the refined class's, and a subclass shares that prefix (#1319 review).
#[test]
fn test_mixed_receiver_write_accepts_same_and_subclass_into_a_refined_untyped_object_property() {
    let out = compile_and_run(
        r#"<?php
class A { public $x = 10; public $n = 3; }
class Sub extends A { public $m = 5; }
class H { public $o; function __construct() { $this->o = new A(); } }
function w(mixed $h, mixed $v): void { $h->o = $v; }
$h = new H();
w($h, new A());
var_dump($h->o->n);
w($h, new Sub());
var_dump($h->o->n, $h->o->x);
"#,
    );
    assert_eq!(out, "int(3)\nint(3)\nint(10)\n");
}

/// Two classes declaring the same property name is what makes this a runtime dispatch rather than
/// a static resolution: each receiver must reach ITS own slot.
#[test]
fn test_mixed_receiver_write_dispatches_on_the_runtime_class() {
    let out = compile_and_run(
        r#"<?php
class T { public int $n = 1; }
class U { public int $n = 2; public int $m = 3; }
function w(mixed $o): void { $o->n = 9; }
$t = new T();
$u = new U();
w($t);
w($u);
var_dump($t->n, $u->n, $u->m);
"#,
    );
    assert_eq!(out, "int(9)\nint(9)\nint(3)\n");
}

/// A property declared on the PARENT, written through a child instance.
#[test]
fn test_mixed_receiver_write_reaches_an_inherited_property() {
    let out = compile_and_run(
        r#"<?php
class P { public int $n = 1; }
class C extends P {}
function w(mixed $o): void { $o->n = 9; }
$c = new C();
w($c);
var_dump($c->n);
"#,
    );
    assert_eq!(out, "int(9)\n");
}

/// A whole-property array assignment through the same receiver.
#[test]
fn test_mixed_receiver_writes_a_whole_array_property() {
    let out = compile_and_run(
        r#"<?php
class T { public array $items = [1]; }
function w(mixed $o): void { $o->items = [9, 9, 9]; }
$t = new T();
w($t);
var_dump(count($t->items), $t->items[0]);
"#,
    );
    assert_eq!(out, "int(3)\nint(9)\n");
}

/// The loop shape from the issue: `$o` is `T|null` at the write, so it is boxed, and the write
/// went to the same discarded path with no `mixed` written anywhere in the source.
#[test]
fn test_loop_widened_receiver_property_write_survives() {
    let out = compile_and_run(
        r#"<?php
class T { public int $n = 1; }
$o = null;
for ($i = 0; $i < 1; $i++) { $o = new T(); $o->n = 9; }
var_dump($o->n);
"#,
    );
    assert_eq!(out, "int(9)\n");
}

/// Rebinding an EXISTING object inside the loop: the original owner must observe the write too,
/// which is what rules out "the write landed in a copy".
#[test]
fn test_loop_rebound_shared_object_sees_the_write() {
    let out = compile_and_run(
        r#"<?php
class T { public int $n = 1; }
$o = null;
$t = new T();
for ($i = 0; $i < 1; $i++) { $o = $t; $o->n = 9; }
var_dump($t->n, $o->n);
"#,
    );
    assert_eq!(out, "int(9)\nint(9)\n");
}

/// A BORROWED source — a caller's local passed in as a parameter — must survive the write.
///
/// Both backend paths for a runtime-typed receiver RETAIN what they store, so the store never
/// consumes its source. An earlier cut of this fix cancelled that retain in the backend on a
/// type-only test, which destroyed the property's own reference for a borrowed source: `count($a)`
/// printed a pointer and `--heap-debug` reported a bad refcount. The release now lives in lowering,
/// gated on the source actually owning something.
#[test]
fn test_mixed_receiver_write_of_a_borrowed_source_keeps_both_alive() {
    let out = compile_and_run(
        r#"<?php
class T { public array $items = [0]; }
function w(mixed $o, array $a): void { $o->items = $a; }
$t = new T();
$a = [1, 2, 3];
w($t, $a);
var_dump(count($a), count($t->items), $t->items[1]);
"#,
    );
    assert_eq!(out, "int(3)\nint(3)\nint(2)\n");
}

/// The control the fix must not disturb: a stdClass payload keeps going through
/// `__rt_mixed_property_set`, which writes it as a dynamic property.
#[test]
fn test_mixed_receiver_stdclass_write_is_unchanged() {
    let out = compile_and_run(
        r#"<?php
function w(mixed $o): void { $o->n = 9; $o->fresh = 7; }
$t = new stdClass();
$t->n = 1;
w($t);
var_dump($t->n, $t->fresh);
"#,
    );
    assert_eq!(out, "int(9)\nint(7)\n");
}

/// The other control: a NON-object payload must still drop the write rather than fault, which is
/// PHP's "attempt to assign property on non-object" behaviour this backend models as a no-op.
#[test]
fn test_mixed_receiver_non_object_payload_drops_the_write() {
    let out = compile_and_run(
        r#"<?php
function w(mixed $o): void { $o->n = 9; }
w(7);
w("x");
w(null);
echo "survived";
"#,
    );
    assert_eq!(out, "survived");
}

/// The runtime-name spelling has always dispatched this way; pinned so the two paths cannot drift
/// apart again — the static spelling being the odd one out is exactly what #1094 was.
#[test]
fn test_runtime_name_and_static_name_mixed_writes_agree() {
    let out = compile_and_run(
        r#"<?php
class T { public int $n = 1; }
function ws(mixed $o): void { $o->n = 8; }
function wd(mixed $o, string $p): void { $o->{$p} = 9; }
$t = new T();
ws($t);
$a = $t->n;
wd($t, "n");
var_dump($a, $t->n);
"#,
    );
    assert_eq!(out, "int(8)\nint(9)\n");
}

/// A runtime class-id match still enforces property visibility from the current lexical scope.
#[test]
fn test_mixed_receiver_write_does_not_bypass_private_property_visibility() {
    let out = compile_and_run(
        r#"<?php
class T {
    private int $secret = 1;
    public function writeInside(mixed $o): void { $o->secret = 7; }
    public function readSecret(): int { return $this->secret; }
}
function writeOutside(mixed $o): void { $o->secret = 9; }
$t = new T();
try { writeOutside($t); } catch (Error $e) { echo "blocked|"; }
$t->writeInside($t);
echo $t->readSecret();
"#,
    );
    assert_eq!(out, "blocked|7");
}

#[test]
/// Verifies the mixed-property-write example preserves its documented output.
fn test_example_mixed_property_write_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/mixed-property-write/main.php"));
    assert_eq!(
        out,
        "Ada\n"
    );
}
