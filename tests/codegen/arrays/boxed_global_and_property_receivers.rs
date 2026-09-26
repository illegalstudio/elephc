//! Purpose:
//! Regression tests for boxed array receivers that are NOT a local: a `global` symbol and a
//! declared object property, each still aliased by another variable when it is mutated or
//! when one of its elements becomes a reference.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `__rt_array_cell_ensure_unique` consumes the receiver's owner when it splits a shared
//!   cell. The global write-back used to retire the previous cell on top of that, so the alias
//!   was left holding freed memory: `count($alias)` then raised a TypeError on x86 and heap
//!   debug reported a bad refcount on aarch64.
//! - An element reference through a `mixed` property read arrives as an owning `PropGet`
//!   temporary that was never released, leaking one boxed array per reference.
//! - Every expected value is real `php` 8.x output for the same fixture.

use crate::support::*;

/// A global aliased by a local must keep its value when an element of the global becomes a
/// reference, and the write through the reference must reach the global.
#[test]
fn test_element_reference_through_aliased_global_separates_the_alias() {
    let out = compile_and_run(
        r#"<?php
function f(): void { global $a; $b = $a; $r = &$a[0]; $r = 2; echo $b[0], "|", $a[0], "\n"; }
$a = [1]; f(); echo $a[0], "\n";
function u(): void { global $g; $r = &$g[0]; $r = 7; echo $g[0], "\n"; }
$g = [1]; u(); echo $g[0], "\n";
"#,
    );
    assert_eq!(out, "1|2\n2\n7\n7\n");
}

/// The mutating builtins split the same cell; the alias must survive `array_push()` and
/// `sort()` on the global.
#[test]
fn test_mutating_builtins_on_aliased_global_keep_the_alias_alive() {
    let out = compile_and_run(
        r#"<?php
function g(): void { global $c; $d = $c; array_push($c, 9); echo count($d), "|", count($c), "\n"; }
$c = [1]; g(); echo count($c), "\n";
function h(): void { global $e; $f = $e; sort($e); echo implode(",", $f), "|", implode(",", $e), "\n"; }
$e = [3, 1, 2]; h(); echo implode(",", $e), "\n";
"#,
    );
    assert_eq!(out, "1|2\n2\n3,1,2|1,2,3\n1,2,3\n");
}

/// A `mixed` property aliased by a local: the reference separates the alias and the write
/// reaches the property.
#[test]
fn test_element_reference_through_aliased_mixed_property_separates_the_alias() {
    let out = compile_and_run(
        r#"<?php
class P { public mixed $m = [1]; }
$p = new P(); $q = $p->m; $r = &$p->m[0]; $r = 2; echo $q[0], "|", $p->m[0], "\n";
$s = new P(); $r2 = &$s->m[0]; $r2 = 5; echo $s->m[0], "\n";
"#,
    );
    assert_eq!(out, "1|2\n5\n");
}

/// Splitting a global or a property cell adds one owner and consumes one; both must be
/// balanced, and the property read that carries the reference must be released.
#[test]
fn test_boxed_global_and_property_receivers_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public mixed $m = [1]; }
function f(): int { global $a; $b = $a; $r = &$a[0]; $r = 2; return $b[0] + $a[0]; }
function g(): int { global $c; $d = $c; array_push($c, 9); return count($d) + count($c); }
$t = 0;
for ($i = 0; $i < 3; $i++) {
    $a = [1]; $c = [1];
    $t += f() + g();
    $p = new P(); $q = $p->m; $r = &$p->m[0]; $r = 2; $t += $q[0] + $p->m[0];
    $s = new P(); $r2 = &$s->m[0]; $r2 = 5; $t += $s->m[0];
}
echo $t, "\n";
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "42\n", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A property array reaching a parameter compiled for RAW array storage must be unboxed first.
///
/// `prop_get` on a declared `array` yields a boxed `Heap(Mixed)` cell, while the SPL and prelude
/// classes declare `array: Heap(Array)` — a raw pointer. Nothing converted between the two, so the
/// callee read the cell's header words as the array's, silently: this fixture printed
/// `4||5|01234|2` where php prints `2|34|2|a1b2|2`. The associative half is the clearer tell —
/// `count()` answered 5 for two pairs and the `foreach` walked the cell's own words as elements.
///
/// A user function was never affected, and that is the diagnostic: `takeCount()` has a
/// `Heap(Mixed)` parameter slot, so both sides already agreed. Only a callee compiled for raw
/// storage could see the difference, which is why it is in the fixture — it passed before the fix
/// and must keep passing after it.
///
/// Both array shapes are covered because they unbox through different runtime helpers, and the
/// assertion is the CONTENT rather than only the count: a wrong header word still yields a
/// plausible number.
#[test]
fn test_property_array_unboxes_into_a_raw_array_parameter() {
    let out = compile_and_run(
        r#"<?php
class Holder {
    public array $items = [3, 4];
    public array $pairs = ['a' => 1, 'b' => 2];
}
function takeCount(array $a): int { return count($a); }
$h = new Holder();
$ai = new ArrayIterator($h->items);
echo $ai->count(), "|";
foreach ($ai as $v) { echo $v; }
echo "|", count(new ArrayObject($h->pairs)), "|";
foreach (new ArrayIterator($h->pairs) as $k => $v) { echo $k, $v; }
echo "|", takeCount($h->items);
"#,
    );
    assert_eq!(out, "2|34|2|a1b2|2");
}

/// The same crossing reached through argument SPREADS, and the layout trap it guards against.
///
/// A static positional spread and a named spread both end in `coerce_operands_to_params`, so they
/// take the same unbox; `origin/main` printed `4||5|01234` for this fixture where php prints
/// `2|34|2|a1b2`.
///
/// The named half is also the regression test for a tempting "improvement". Routing the unbox
/// through the shared container conversion that merge temps use — it canonicalizes slots and
/// carries the boxed-slot guard, which a reviewer recommended — made `new ArrayObject(...)` die
/// with SIGBUS: that conversion trusts the STATIC layout, the SPL parameter says `array<mixed>`,
/// and it ran the indexed-array conversion over a hash payload. The unbox has to stay
/// layout-agnostic.
#[test]
fn test_property_array_reaches_a_raw_array_parameter_through_spreads() {
    let out = compile_and_run(
        r#"<?php
class Holder { public array $items = [3, 4]; public array $pairs = ['a' => 1, 'b' => 2]; }
$h = new Holder();
$args = [$h->items];
$ai = new ArrayIterator(...$args);
echo $ai->count(), "|";
foreach ($ai as $v) { echo $v; }
echo "|";
$named = ['array' => $h->pairs];
$bo = new ArrayObject(...$named);
echo count($bo), "|";
foreach ($bo as $k => $v) { echo $k, $v; }
"#,
    );
    assert_eq!(out, "2|34|2|a1b2");
}
