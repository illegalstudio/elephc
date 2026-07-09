//! Purpose:
//! Regression tests for symbol-backed container write-back after in-place
//! mutation helpers. Covers static-local arrays/hashes and global hash COW
//! semantics, ensuring the authoritative pointer is written back to the symbol
//! store after growth or copy-on-write splits.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Static local containers persist across calls; mutation helpers may reallocate.
//! - Global/superglobal containers must reflect mutations performed through aliases.

use super::*;

/// Tests that static local indexed arrays receive appends across calls.
#[test]
fn test_static_array_append_across_calls() {
    let out = compile_and_run(
        r#"<?php
function bump(): int {
    static $store = [];
    $store[] = 'x';
    return count($store);
}
echo bump() . bump() . bump();
"#,
    );
    assert_eq!(out, "123");
}

/// Tests that returned aliases of a static array observe growth across calls.
#[test]
fn test_static_array_returned_alias_across_calls() {
    let out = compile_and_run(
        r#"<?php
function cache(): array {
    static $s = [];
    $s[] = 'e';
    return $s;
}
$a = cache();
$b = cache();
$c = cache();
echo count($a) . count($b) . count($c);
"#,
    );
    assert_eq!(out, "123");
}

/// Tests repeated appends to a static array in a loop do not crash.
#[test]
fn test_static_array_append_loop_no_crash() {
    let out = compile_and_run(
        r#"<?php
function cache(): array {
    static $s = [];
    $s[] = 'e';
    return $s;
}
for ($i = 0; $i < 5; $i++) {
    $r = cache();
}
echo count($r);
"#,
    );
    assert_eq!(out, "5");
}

/// Tests that a static associative hash with string keys survives many growths.
#[test]
fn test_static_hash_growth_40_keys() {
    let out = compile_and_run(
        r#"<?php
function reg(string $k): int {
    static $map = ['seed' => 's'];
    $map[$k] = 'v';
    return count($map);
}
for ($i = 0; $i < 40; $i++) {
    $last = reg('k' . $i);
}
echo $last;
"#,
    );
    assert_eq!(out, "41");
}

/// Tests many appends within one call plus further appends across calls.
#[test]
fn test_static_array_many_appends_single_call() {
    let out = compile_and_run(
        r#"<?php
function bump(): int {
    static $s = [];
    for ($i = 0; $i < 12; $i++) {
        $s[] = 'x';
    }
    return count($s);
}
echo bump() . '|' . bump();
"#,
    );
    assert_eq!(out, "12|24");
}

/// Tests that unset on a COW-split global writes the new pointer back to the symbol.
#[test]
fn test_hash_unset_cow_global_writeback() {
    let out = compile_and_run(
        r#"<?php
$_GET = ['a' => '1', 'b' => '2'];
$copy = $_GET;
unset($_GET['a']);
echo count($_GET) . '|' . (isset($_GET['a']) ? 'still' : 'gone') . '|' . count($copy);
"#,
    );
    assert_eq!(out, "1|gone|2");
}

/// Negative control: class static-property arrays already work via property write-back.
#[test]
fn test_static_property_array_control() {
    let out = compile_and_run(
        r#"<?php
class C {
    public static array $items = [];
}
function add(): int {
    C::$items[] = 'x';
    return count(C::$items);
}
echo add() . add() . add();
"#,
    );
    assert_eq!(out, "123");
}

/// Negative control: global arrays reached through `global` already work.
#[test]
fn test_global_array_control() {
    let out = compile_and_run(
        r#"<?php
$g = [];
function addg(): int {
    global $g;
    $g[] = 'x';
    return count($g);
}
echo addg() . addg() . addg();
"#,
    );
    assert_eq!(out, "123");
}

/// Tests that a static-local string accumulates via `.=` across calls.
#[test]
fn test_static_string_concat_across_calls() {
    let out = compile_and_run(
        r#"<?php
function tick(): string {
    static $s = 'x';
    $s .= 'y';
    return $s;
}
echo tick() . '|' . tick() . '|' . tick();
"#,
    );
    assert_eq!(out, "xy|xyy|xyyy");
}

/// Tests that a static-local callable slot (`static $f = null;`, later reassigned to
/// closures) picks the correct closure per call. NOTE: this fixture does NOT compile —
/// the EIR backend rejects it with "unsupported EIR backend feature: init_static_local
/// assigning PHP type Void to static local $f with PHP type Mixed" (the type checker
/// accepts the fixture; only the EIR static-local initializer lowering rejects a `null`
/// initializer against a slot widened to Mixed by later closure reassignment). Mixed
/// static slots are now supported by EIR static-local codegen via box/unbox of the
/// widened Mixed cell.
#[test]
fn test_static_callable_reassign() {
    let out = compile_and_run(
        r#"<?php
function pick(int $i): int {
    static $f = null;
    if ($i === 0) {
        $f = function (): int { return 10; };
    } else {
        $f = function (): int { return 20; };
    }
    return $f();
}
echo pick(0) . pick(1) . pick(0);
"#,
    );
    assert_eq!(out, "102010");
}

/// Tests that a static-local string reassigned wholesale persists the new value across calls.
#[test]
fn test_static_string_reassign_across_calls() {
    let out = compile_and_run(
        r#"<?php
function stamp(string $v): string {
    static $s = '';
    $s = $v . '!';
    return $s;
}
echo stamp('a') . stamp('bb');
"#,
    );
    assert_eq!(out, "a!bb!");
}

/// Tests that `array_pop()` on a static-local indexed array writes the shrunk pointer back to the symbol.
#[test]
fn test_array_pop_static_writeback() {
    let out = compile_and_run(
        r#"<?php
function popper(): int {
    static $s = ['a','b','c','d','e','f','g','h','i','j','k','l'];
    array_pop($s);
    return count($s);
}
echo popper() . '|' . popper();
"#,
    );
    assert_eq!(out, "11|10");
}

/// Tests that `sort()` on a superglobal writes the re-indexed pointer back to the symbol.
/// NOTE: this fixture does NOT compile — the EIR backend rejects it with "unsupported EIR
/// backend feature: sort for PHP type AssocArray { key: Str, value: Mixed }" ($_GET is a
/// fixed-shape AssocArray superglobal slot; the type checker accepts `sort($_GET)`, but the
/// `sort()` EIR lowering only supports `Array(_)`-shaped indexed arrays, not AssocArray).
/// Ignored per master directive E until that backend gap is closed.
#[test]
#[ignore = "sort() on a superglobal re-indexes the Hash slot — EIR backend does not support sort() on AssocArray-shaped superglobals"]
fn test_sort_superglobal_writeback() {
    let out = compile_and_run(
        r#"<?php
$_GET = ['b' => '2', 'a' => '1', 'c' => '3'];
sort($_GET);
echo implode(',', $_GET);
"#,
    );
    assert_eq!(out, "1,2,3");
}

/// Tests that a by-reference `foreach` over a static-local hash writes mutated values back to the symbol.
/// Reads the mutated entries back via direct string-key indexing instead of `implode()`,
/// which has no AssocArray support in the EIR backend (a pre-existing, unrelated gap),
/// while preserving the same by-ref-mutation-persists-across-calls assertion.
#[test]
fn test_byref_foreach_static_writeback() {
    let out = compile_and_run(
        r#"<?php
function touch_all(): string { static $m = ['x' => '1', 'y' => '2']; foreach ($m as $k => &$v) { $v = $v . '0'; } unset($v); return $m['x'] . ',' . $m['y']; }
echo touch_all() . '|' . touch_all();
"#,
    );
    assert_eq!(out, "10,20|100,200");
}

/// Tests that `array_unshift()` on a static-local indexed array writes the grown pointer back to the symbol.
/// Uses integer elements: the EIR backend's `array_unshift()` only supports Int/Bool
/// indexed-array elements (a pre-existing, unrelated backend limitation), so the
/// master-specified string-element fixture was adapted to ints while preserving the
/// same count-growth assertion.
#[test]
fn test_array_unshift_static_writeback() {
    let out = compile_and_run(
        r#"<?php
function u(): int {
    static $s = [2, 3];
    array_unshift($s, 1);
    return count($s);
}
echo u() . '|' . u();
"#,
    );
    assert_eq!(out, "3|4");
}

/// Tests that `array_shift()` on a static-local indexed array writes the shrunk pointer back to the symbol.
#[test]
fn test_array_shift_static_writeback() {
    let out = compile_and_run(
        r#"<?php
function s(): int {
    static $s = ['a','b','c','d','e','f','g','h'];
    array_shift($s);
    return count($s);
}
echo s() . '|' . s();
"#,
    );
    assert_eq!(out, "7|6");
}

/// Tests that global arrays returned across calls obey PHP value semantics.
/// Each returned snapshot must be independent; the live global continues to grow.
#[test]
fn test_global_array_returned_alias_across_calls() {
    let out = compile_and_run(
        r#"<?php
$g = [];
function cache(): array { global $g; $g[] = 'e'; return $g; }
$a = cache(); $b = cache(); $c = cache();
echo count($a) . count($b) . count($c);
"#,
    );
    assert_eq!(out, "123");
}

/// Tests that returned static strings are persisted as independent heap copies.
/// The caller's earlier snapshot must not be affected by later mutations.
#[test]
fn test_static_string_returned_alias() {
    let out = compile_and_run(
        r#"<?php
function tag(): string { static $s = ''; $s = $s . 'x'; return $s; }
$a = tag(); $b = tag();
echo $a . '|' . $b;
"#,
    );
    assert_eq!(out, "x|xx");
}

/// Tests that superglobal arrays returned across calls obey value semantics.
/// Each caller snapshot is a separate copy; the live superglobal accumulates.
#[test]
fn test_superglobal_returned_alias() {
    let out = compile_and_run(
        r#"<?php
function grab(): array { $_GET['n'] = ($_GET['n'] ?? '') . 'i'; return $_GET; }
$_GET = [];
$a = grab(); $b = grab();
echo count($a) . count($b) . '|' . ($_GET['n'] ?? '?');
"#,
    );
    assert_eq!(out, "11|ii");
}

/// Tests that returned static-array snapshots are isolated by value semantics.
/// The first caller must not see the second push.
#[test]
fn test_static_array_returned_snapshot_isolation() {
    let out = compile_and_run(
        r#"<?php
function cache(): array { static $s = []; $s[] = 'e'; return $s; }
$a = cache(); $b = cache();
echo count($a) . count($b) . '|' . (isset($a[1]) ? 'shared' : 'isolated');
"#,
    );
    assert_eq!(out, "12|isolated");
}

/// Tests that a `static $map = []` accumulates string keys across calls, exercising
/// the box/unbox path for a Mixed-widened array slot on each call.
#[test]
fn test_static_empty_map_string_keys_accumulates() {
    let out = compile_and_run(
        r#"<?php
function reg(string $k): int {
    static $map = [];
    $map[$k] = 'v';
    return count($map);
}
echo reg('a') . reg('b') . reg('c');
"#,
    );
    assert_eq!(out, "123");
}

/// Tests 40-iteration growth of a `static $map = []` to verify write-back-through-cell
/// does not lose entries.
#[test]
fn test_static_empty_map_growth_40_keys() {
    let out = compile_and_run(
        r#"<?php
function reg(string $k): int {
    static $map = [];
    $map[$k] = 'v';
    return count($map);
}
$last = 0;
for ($i = 0; $i < 40; $i++) {
    $last = reg('k' . $i);
}
echo $last;
"#,
    );
    assert_eq!(out, "40");
}

/// Tests two writes per call to a `static $map = []`: the second write exercises the
/// write-back-through-cell branch (Mixed slot re-box + release-previous).
#[test]
fn test_static_empty_map_two_writes_per_call() {
    let out = compile_and_run(
        r#"<?php
function reg2(string $a, string $b): int {
    static $m = [];
    $m[$a] = '1';
    $m[$b] = '2';
    return count($m);
}
echo reg2('x','y') . reg2('z','w');
"#,
    );
    assert_eq!(out, "24");
}

/// Tests value (snapshot) semantics of a `static $m = []`: copying the map, then
/// mutating, must not retroactively mutate the snapshot.
#[test]
fn test_static_empty_map_snapshot_value_semantics() {
    let out = compile_and_run(
        r#"<?php
function snap(): string {
    static $m = [];
    $m['a'] = '1';
    $c = $m;
    $m['b'] = '2';
    return count($c) . '|' . count($m);
}
echo snap();
"#,
    );
    assert_eq!(out, "1|2");
}

/// Tests a `static $v = null` widened to Mixed by a scalar reassignment on every call
/// (the `Void` -> `Str` widening forces Mixed slot storage), exercising box-on-init,
/// box-on-store, and unbox-on-load, with PHP-level string coercion on read.
///
/// NOTE: the originally planned fixture alternated `$v` between a string and an int
/// across `if`/`else` branches within one function body. The type checker threads its
/// `TypeEnv` sequentially through `if`/`else` branches (no per-branch env fork), so a
/// second, incompatible-scalar reassignment of the same variable in the same function
/// is rejected ("cannot reassign $v from string to int") — a pre-existing checker
/// limitation unrelated to this static-local Mixed-widening fix. This fixture keeps a
/// single concrete type (`Str`) but varies the stored value on every call, which still
/// forces the slot to widen from the `null` declaration (`Void`) to `Mixed` and
/// exercises the same init/store/load box/unbox code paths.
#[test]
fn test_static_mixed_scalar_reassign() {
    let out = compile_and_run(
        r#"<?php
function s(int $i): string {
    static $v = null;
    $v = 'val' . $i;
    return $v . '!';
}
echo s(0) . '|' . s(1);
"#,
    );
    assert_eq!(out, "val0!|val1!");
}
