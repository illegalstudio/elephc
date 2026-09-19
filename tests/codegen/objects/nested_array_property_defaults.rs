//! Purpose:
//! Integration tests for property defaults whose ELEMENTS are themselves array literals:
//! `public array $x = [[1], [2]];` and its keyed, nullable, mixed and static spellings. Every
//! one of these was refused at compile time until the literal-default form became recursive,
//! even though the same literal was already accepted as a local, a parameter default and a
//! class constant.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout.
//! - Expected values are real `LC_ALL=C php` 8.5 output for the same fixtures.
//! - Ownership of the nested containers is covered separately under `runtime_gc/`.

use super::*;

/// The issue's headline shape: an indexed default whose elements are indexed literals.
#[test]
fn test_indexed_property_default_with_indexed_literal_elements() {
    let out = compile_and_run(
        r#"<?php
class C { public array $x = [[1], [2]]; }
$c = new C();
echo count($c->x), "|", $c->x[0][0], "|", $c->x[1][0];
"#,
    );
    assert_eq!(out, "2|1|2");
}

/// The same default on the nullable and `mixed` slots, which box the outer container.
///
/// The issue's table lists all three, and notes that a plain `array` property failed exactly
/// as a `?array` or `mixed` one did -- it was never about the slot.
#[test]
fn test_nested_literal_default_on_nullable_and_mixed_slots() {
    let out = compile_and_run(
        r#"<?php
class N { public ?array $x = [[1], [2]]; }
class M { public mixed $x = [[1], [2]]; }
$n = new N();
$m = new M();
echo count($n->x), $n->x[1][0], "|", count($m->x), $m->x[1][0];
"#,
    );
    assert_eq!(out, "22|22");
}

/// The keyed spellings, on the plain `array` slot and on the boxed ones.
///
/// `public ?array $x = ["k" => 1];` was refused on its own too -- a keyed literal had no boxed
/// form at all, only the positional one -- so the keyed nullable row of the issue's table was
/// two independent gaps, not one.
#[test]
fn test_keyed_property_defaults_including_nested_and_boxed() {
    let out = compile_and_run(
        r#"<?php
class A { public array $x = ["k" => [1]]; }
class B { public ?array $x = ["k" => 1]; }
class D { public mixed $x = ["k" => [1]]; }
class E { public array $x = ["k" => ["j" => 7]]; }
$a = new A();
$b = new B();
$d = new D();
$e = new E();
echo $a->x["k"][0], "|", $b->x["k"], "|", $d->x["k"][0], "|", $e->x["k"]["j"];
"#,
    );
    assert_eq!(out, "1|1|1|7");
}

/// Depth beyond one level, and elements that mix containers with scalars.
///
/// The recursion has no depth limit of its own, and an element list does not have to be
/// uniform: `[[1], 2]` is an array element beside an int element in the same literal.
#[test]
fn test_nested_property_defaults_nest_deeply_and_mix_element_kinds() {
    let out = compile_and_run(
        r#"<?php
class C {
    public array $deep = [[[1]]];
    public array $mixedKinds = [[1], 2];
    public array $scalars = [[1, "s", 2.5, null, true]];
    public array $empty = [[]];
}
$c = new C();
echo $c->deep[0][0][0], "|", count($c->mixedKinds), $c->mixedKinds[1], "|",
     count($c->scalars[0]), $c->scalars[0][1], "|", count($c->empty[0]);
"#,
    );
    assert_eq!(out, "1|22|5s|0");
}

/// A static property takes the same nested default.
///
/// Static and instance defaults go through separate emitters (`block_emit` and
/// `property_defaults`), so a fix applied to one does not reach the other.
#[test]
fn test_static_property_default_with_nested_literal_elements() {
    let out = compile_and_run(
        r#"<?php
class S { public static array $s = [[1], [2]]; }
echo count(S::$s), "|", S::$s[1][0];
"#,
    );
    assert_eq!(out, "2|2");
}

/// A STATIC property with a KEYED default on a boxed slot takes the `BoxedAssocArray` path.
///
/// Raised in review: the static fixture above uses a positional default on a plain `array`
/// slot, which never reaches the keyed boxing emitter in `block_emit`. That emitter is separate
/// from the instance one in `property_defaults`, and separate again from the positional
/// `BoxedArray` arm beside it, so nothing here covered it. The flat keyed spelling is included
/// because it was refused on its own before this change, independently of any nesting.
#[test]
fn test_static_keyed_defaults_on_boxed_slots() {
    let out = compile_and_run(
        r#"<?php
class S {
    public static ?array $flat = ["k" => 1];
    public static mixed $nested = ["k" => [1, 2]];
    public static ?array $deep = ["a" => ["b" => "c"]];
}
echo S::$flat["k"], "|",
     count(S::$nested["k"]), S::$nested["k"][1], "|",
     S::$deep["a"]["b"];
"#,
    );
    assert_eq!(out, "1|22|c");
}

/// Two instances hold separate storage, and a copy outlives the object it came from.
///
/// Each nested container is allocated per object, so a write through one instance's default
/// must not reach another's. PHP's value semantics say the same, and the copy taken out of a
/// destroyed object has to stay readable -- which it cannot if the object's release freed a
/// child the copy still holds.
#[test]
fn test_nested_property_defaults_do_not_share_storage_between_instances() {
    let out = compile_and_run(
        r#"<?php
class A { public array $x = [[1], [2]]; }
$a = new A();
$b = new A();
$a->x[0][] = 99;
echo count($a->x[0]), count($b->x[0]), "|";
$c = new A();
$inner = $c->x[0];
unset($c);
$inner[] = 7;
$d = new A();
echo $inner[0], count($inner), count($d->x[0]);
"#,
    );
    assert_eq!(out, "21|121");
}
