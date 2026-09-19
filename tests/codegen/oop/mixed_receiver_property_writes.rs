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
fn test_mixed_receiver_writes_every_declared_property_kind() {
    let out = compile_and_run(
        r#"<?php
class T { public string $s = "a"; public $u = 1; public float $f = 1.5; }
function ws(mixed $o): void { $o->s = "z"; }
function wu(mixed $o): void { $o->u = 9; }
function wf(mixed $o): void { $o->f = 2.5; }
$t = new T();
ws($t);
wu($t);
wf($t);
var_dump($t->s, $t->u, $t->f);
"#,
    );
    assert_eq!(out, "string(1) \"z\"\nint(9)\nfloat(2.5)\n");
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
