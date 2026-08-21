//! Purpose:
//! Regression tests for mutating array/hash builtins whose by-reference receiver is a
//! by-reference PARAMETER (`function f(array &$a)`). Every one of these lost the write-back:
//! the backend's slot resolver only recognized `load_local`, so a receiver read with
//! `load_ref_cell` had nowhere to publish the copy-on-write split or the growth relocation.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value is verbatim `LC_ALL=C php` 8.4 output for the same fixture.
//! - The `$alias = $x;` lines are load-bearing: they make the receiver shared, so the runtime's
//!   ensure-unique separates a private copy. Without the write-back that copy was mutated and
//!   thrown away, and the caller observed the original array — a silent wrong answer.
//! - `array_unshift` fails even WITHOUT an alias, because prepending reaches `__rt_array_grow`
//!   and the caller then held a pointer to storage the growth had already freed. The
//!   nine-value fixture forces that growth.
//! - The heap-debug assertion pins that republishing the relocated pointer does not double
//!   release the previous storage.

use crate::support::*;

/// Verifies `array_unshift()` on a by-reference parameter reaches the caller's array.
///
/// Nine prepends into a two-element array force at least one `__rt_array_grow`, so this is the
/// use-after-free case: the caller used to print nothing at all because it read the freed
/// pre-growth storage.
#[test]
fn test_array_unshift_on_by_ref_parameter_reaches_caller() {
    let out = compile_and_run(
        r#"<?php
function f(array &$a) { array_unshift($a, 9,8,7,6,5,4,3,2,1); }
$x = [1,2]; f($x); echo implode(",", $x), "\n";
"#,
    );
    assert_eq!(out, "9,8,7,6,5,4,3,2,1,1,2\n");
}

/// Verifies the shape-changing indexed builtins publish their copy-on-write split through a
/// by-reference parameter, and that an alias taken beforehand keeps the original order.
#[test]
fn test_shape_changing_builtins_on_by_ref_parameter_match_php() {
    let out = compile_and_run(
        r#"<?php
function g(array &$a) { $v = array_shift($a); echo $v, "\n"; }
$y = [1,2,3]; $ya = $y; g($y); echo implode(",", $y), "|", implode(",", $ya), "\n";
function h(array &$a) { echo array_pop($a), "\n"; }
$z = [1,2,3]; $za = $z; h($z); echo implode(",", $z), "|", implode(",", $za), "\n";
"#,
    );
    assert_eq!(
        out,
        r#"1
2,3|1,2,3
3
1,2|1,2,3
"#
    );
}

/// Verifies the sort family publishes its copy-on-write split through a by-reference parameter.
///
/// `sort`, `usort`, `ksort` (an insertion-order relink), and `array_multisort` all resolve their
/// receiver the same way, so one missing case would leave the caller unsorted with no diagnostic.
#[test]
fn test_sort_family_on_by_ref_parameter_matches_php() {
    let out = compile_and_run(
        r#"<?php
function s(array &$a) { sort($a); }
$w = [3,1,2]; $wa = $w; s($w); echo implode(",", $w), "|", implode(",", $wa), "\n";
function u(array &$a) { usort($a, fn(int $p, int $q): int => $q <=> $p); }
$v = [3,1,2]; $va = $v; u($v); echo implode(",", $v), "|", implode(",", $va), "\n";
function k(array &$a) { ksort($a); }
$m = ["b"=>2,"a"=>1]; $ma = $m; k($m); echo implode(",", array_keys($m)), "|", implode(",", array_keys($ma)), "\n";
function ms(array &$p, array &$q) { array_multisort($p, $q); }
$o = [3,1,2]; $oo = [30,10,20]; $oa = $o; ms($o, $oo); echo implode(",", $o), "|", implode(",", $oa), "\n";
"#,
    );
    assert_eq!(
        out,
        r#"1,2,3|3,1,2
3,2,1|3,1,2
a,b|b,a
1,2,3|3,1,2
"#
    );
}

/// Verifies an associative insert through a by-reference parameter reaches the caller's table.
///
/// `$a["c"] = 3` splits the shared table with `__rt_hash_ensure_unique` and can reallocate it,
/// so the hash lowering needs the same ref-cell write-back the indexed builtins do.
#[test]
fn test_hash_insert_on_by_ref_parameter_matches_php() {
    let out = compile_and_run(
        r#"<?php
function hs(array &$a) { $a["c"] = 3; }
$n = ["a"=>1,"b"=>2]; $na = $n; hs($n); echo implode(",", array_keys($n)), "|", implode(",", array_keys($na)), "\n";
"#,
    );
    assert_eq!(out, "a,b,c|a,b\n");
}

/// Verifies the whole by-reference receiver matrix leaves the heap balanced.
///
/// Republishing a relocated pointer through a ref cell releases whatever the slot held before,
/// so a write-back that dropped or double-counted the previous owner shows up here.
#[test]
fn test_by_ref_parameter_receivers_leave_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function f(array &$a) { array_unshift($a, 9,8,7,6,5,4,3,2,1); }
$x = [1,2]; f($x); echo implode(",", $x), "\n";
function g(array &$a) { $v = array_shift($a); echo $v, "\n"; }
$y = [1,2,3]; $ya = $y; g($y); echo implode(",", $y), "|", implode(",", $ya), "\n";
function h(array &$a) { echo array_pop($a), "\n"; }
$z = [1,2,3]; $za = $z; h($z); echo implode(",", $z), "|", implode(",", $za), "\n";
function s(array &$a) { sort($a); }
$w = [3,1,2]; $wa = $w; s($w); echo implode(",", $w), "|", implode(",", $wa), "\n";
"#,
    );
    assert_eq!(
        out.stdout,
        r#"9,8,7,6,5,4,3,2,1,1,2
1
2,3|1,2,3
3
1,2|1,2,3
1,2,3|3,1,2
"#,
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a callee that WIDENS a by-reference array's elements is seen by its caller.
///
/// `function f(array &$a) { foreach ($a as $k => $v) { $a[$k] = $v * 2; } }` over `[1, 2, 3]`
/// printed three ADDRESSES. The checker specialized the declared `array` to the call site's
/// `array<int>`; the body then re-typed it to `array<mixed>` through a loop storage contract and
/// wrote that back through the reference cell, so the caller — still compiled for raw slots —
/// read the boxes as ints. Every expected value here is verbatim `php -n` 8.5.6 output.
///
/// The shapes are the ones that told the radius apart: an int list, an int-valued HASH (which
/// needs `Op::HashToMixed` rather than `Op::ArrayToMixed`), a method receiver, an UNTYPED `&$a`,
/// and a write WIDER than the array started with.
#[test]
fn test_by_ref_array_parameter_widening_reaches_the_caller() {
    let out = compile_and_run(
        r#"<?php
function ints(array &$a): void { foreach ($a as $k => $v) { $a[$k] = $v * 2; } }
$i = [1, 2, 3]; ints($i); echo "ints: ", implode(",", $i), "\n";

function assoc(array &$a): void { foreach ($a as $k => $v) { $a[$k] = $v * 2; } }
$h = ["x" => 1, "y" => 2]; assoc($h); echo "assoc: ", $h["x"], ",", $h["y"], "\n";

class C { public function go(array &$a): void { foreach ($a as $k => $v) { $a[$k] = $v * 3; } } }
$m = [1, 2]; (new C())->go($m); echo "method: ", implode(",", $m), "\n";

function untyped(&$a): void { foreach ($a as $k => $v) { $a[$k] = $v * 5; } }
$u = [1, 2]; untyped($u); echo "untyped: ", implode(",", $u), "\n";

function widen(array &$a): void { $a[0] = "text"; $a[] = 4.5; }
$w = [1, 2]; widen($w); echo "widen: ", json_encode($w), "\n";
"#,
    );
    assert_eq!(
        out,
        "ints: 2,4,6\nassoc: 2,4\nmethod: 3,6\nuntyped: 5,10\nwiden: [\"text\",2,4.5]\n"
    );
}

/// Verifies the caller's own type follows the widening, so a LATER by-value call is compiled
/// for the storage it will actually receive.
///
/// The lowering converted the caller's local while the checker still called it `array<int>`, so
/// the next by-value callee was specialized for raw slots and read the boxes back: `total($t)`
/// printed one address. The `count`/`json_encode`/`array_sum` reads are here for the same
/// reason — each takes a different path to the elements.
#[test]
fn test_by_ref_array_widening_retypes_the_caller_for_later_calls() {
    let out = compile_and_run(
        r#"<?php
function bump(array &$a): void { foreach ($a as $k => $v) { $a[$k] = $v + 1; } }
function total(array $a): int { $t = 0; foreach ($a as $v) { $t = $t + (int) $v; } return $t; }
$r = [10, 20]; bump($r);
echo $r[0], "|", $r[1], "|", count($r), "|", json_encode($r), "|", array_sum($r), "\n";
$t = [1, 2, 3]; bump($t); echo total($t), "\n";
"#,
    );
    assert_eq!(out, "11|21|2|[11,21]|32\n9\n");
}

/// Verifies a body that does NOT widen keeps its narrow element type, so `sort()` still compiles.
///
/// Reporting every by-reference array as `array<mixed>` is sound and was the first fix tried; it
/// turned this fixture into `unsupported EIR backend feature: sort indexed-array element PHP
/// type Mixed`, because the backend has no Mixed-element sort. Only a body that ACTUALLY widens
/// re-types its parameter, which is what keeps a sorting callee on raw slots.
#[test]
fn test_by_ref_array_parameter_without_widening_keeps_its_element_type() {
    let out = compile_and_run(
        r#"<?php
function order(array &$a): void { sort($a); }
$s = [3, 1, 2]; order($s); echo implode(",", $s), "\n";
function readonly_sum(array &$a): int { $t = 0; foreach ($a as $v) { $t = $t + $v; } return $t; }
$r = [1, 2, 3]; echo readonly_sum($r), "|", implode(",", $r), "\n";
"#,
    );
    assert_eq!(out, "1,2,3\n6|1,2,3\n");
}

/// Verifies an EMPTY array passed by reference receives what the callee fills it with.
///
/// `$e = []; fill($e);` SEGFAULTED before the by-reference widening landed, and answered an
/// empty array after it: the caller kept reading `array<never>` storage the callee had already
/// appended to. An empty array has no element slots to convert, so the caller only needs its
/// local re-typed — which is why this case emits no conversion op, unlike the boxed-slot one.
///
/// The keyed fill is the same defect one layer out: writing a string key turns the empty list
/// into a HASH, so the caller must convert with `Op::ArrayToHash` and the empty literal has to
/// be accepted for a hash parameter in the first place. It printed `[10, 13]` without that.
#[test]
fn test_empty_array_by_ref_parameter_receives_what_the_callee_fills() {
    let out = compile_and_run(
        r#"<?php
function fill(array &$a): void { $a[] = 1; $a[] = 2; }
$e = []; fill($e); echo count($e), ":", implode(",", $e), "\n";

function fill_str(array &$a): void { $a[] = "x"; $a[] = "y"; }
$f = []; fill_str($f); echo implode(",", $f), "\n";

function fill_mixed(array &$a): void { $a[] = 1; $a[] = "s"; $a[] = 2.5; }
$g = []; fill_mixed($g); echo json_encode($g), "\n";

function fill_keyed(array &$a): void { $a["k"] = 1; $a["j"] = 2; }
$h = []; fill_keyed($h); echo json_encode($h), "\n";

$i = []; fill($i); fill($i); echo implode(",", $i), "\n";
"#,
    );
    assert_eq!(
        out,
        "2:1,2\nx,y\n[1,\"s\",2.5]\n{\"k\":1,\"j\":2}\n1,2,1,2\n"
    );
}

/// Verifies a PROPERTY can be passed by reference to a user function.
///
/// `bump($obj->items)` was `Function 'bump' parameter $a must be passed a variable` — php has
/// allowed a property there since forever. The rewrite that reads the place into a hidden
/// temporary, calls with it, and writes it back already existed for builtin callees
/// (`ir_lower::expr::ref_place_args`); it was gated off for user ones, and the checker refused
/// the shape before it could run. Both halves now accept exactly what that rewrite can lower:
/// array-typed properties and static properties.
#[test]
fn test_property_can_be_passed_by_reference_to_a_user_function() {
    let out = compile_and_run(
        r#"<?php
class Holder {
    public array $items = [1, 2];
    public int $count = 0;
    public string $label = "a";
    public static array $shared = [3, 4];
    public static int $total = 10;
}
function bump(array &$a): void { foreach ($a as $k => $v) { $a[$k] = $v + 1; } }
function push_one(array &$a): void { $a[] = 99; }

$o = new Holder(); bump($o->items); echo implode(",", $o->items), "\n";
bump(Holder::$shared); echo implode(",", Holder::$shared), "\n";
$p = new Holder(); push_one($p->items); echo implode(",", $p->items), "\n";
$q = new Holder(); $alias = $q; bump($q->items); echo implode(",", $alias->items), "\n";

// A SCALAR property is carried too, because a USER parameter declares the type the hidden
// temporary and the write-back must agree on. A builtin that RE-TYPES its by-reference
// argument (`settype`) is why the same shape stays off the builtin path.
function inc(int &$n): void { $n = $n + 1; }
function append(string &$s): void { $s = $s . "!"; }
$s = new Holder(); inc($s->count); inc($s->count); append($s->label);
echo $s->count, "|", $s->label, "\n";
inc(Holder::$total); echo Holder::$total, "\n";
"#,
    );
    assert_eq!(out, "2,3\n4,5\n1,2,99\n2,3\n2|a!\n11\n");
}
