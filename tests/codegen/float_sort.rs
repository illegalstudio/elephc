//! Purpose:
//! Regression tests for raw-float sort/rsort ordering, results and ownership.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Fixtures cover NAN-sensitive PHP sorting schedules and stable signed zeroes.
//! - Unsupported key-preserving sorts retain explicit backend diagnostics.

use crate::support::*;

/// Verifies `sort()` and `rsort()` over FLOAT elements, which used to be a compile error.
///
/// A float array's slots hold the values themselves rather than pointers, so neither the
/// integer helper (which would order the bit patterns) nor the string one could sort them, and
/// the backend refused with "sort indexed-array element PHP type Float". `sort([2.5, 1.5])`
/// did not compile.
///
/// The ordering comes from `__rt_php_compare`, the same table `<` and `<=>` use, so the two
/// zeroes PHP considers equal stay adjacent in input order and the infinities land at the ends.
/// Integral-valued floats are here because a string sort would put `100.0` between `10.0` and
/// `9.0`, which is the mistake a wrong helper makes.
#[test]
fn sort_orders_float_elements_the_way_php_does() {
    let out = compile_and_run(
        r#"<?php
function show(array $a): void {
    $parts = [];
    foreach ($a as $v) { $parts[] = var_export($v, true); }
    echo implode(",", $parts), "|";
}
$a = [2.5, 1.5, 3.5];
sort($a);
show($a);
rsort($a);
show($a);
$b = [1.0, -2.5, 0.0, -0.0, 3.25, -7.75];
sort($b);
show($b);
$d = [10.0, 9.0, 100.0, 2.0];
sort($d);
show($d);
rsort($d);
show($d);
$f = [1.0, INF, -INF, 0.0];
sort($f);
show($f);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "1.5,2.5,3.5|",
            "3.5,2.5,1.5|",
            "-7.75,-2.5,0.0,-0.0,1.0,3.25|",
            "2.0,9.0,10.0,100.0|",
            "100.0,10.0,9.0,2.0|",
            "-INF,0.0,1.0,INF|",
        )
    );
}

/// Verifies a float sort reaches the storage it was handed, whatever names it.
///
/// Sorting reaches the caller's storage, while a copy taken beforehand keeps its original order.
#[test]
fn sort_orders_float_elements_through_every_receiver() {
    let out = compile_and_run(
        r#"<?php
function show(array $a): void {
    $parts = [];
    foreach ($a as $v) { $parts[] = var_export($v, true); }
    echo implode(",", $parts), "|";
}
$grown = [];
for ($i = 0; $i < 4; $i++) { $grown[] = 3.5 - $i * 0.5; }
sort($grown);
show($grown);

function sortInPlace(array &$values): void { rsort($values); }
$byRef = [1.5, 3.5, 2.5];
sortInPlace($byRef);
show($byRef);

class Series { public array $points = [3.5, 1.25, 2.0]; }
$s = new Series();
sort($s->points);
show($s->points);

$original = [3.0, 1.0, 2.0];
$copy = $original;
sort($copy);
show($original);
show($copy);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "2.0,2.5,3.0,3.5|",
            "3.5,2.5,1.5|",
            "1.25,2.0,3.5|",
            "3.0,1.0,2.0|",
            "1.0,2.0,3.0|",
        )
    );
}

/// Verifies repeated float sorts preserve their payloads and leave no PHP heap blocks behind.
///
/// Raw float slots stay unboxed. The sorter owns a separate original-position sidecar;
/// the PHP heap retains only the array and releases it normally after each iteration.
#[test]
fn sort_over_float_elements_stays_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$n = 0;
for ($i = 0; $i < 200; $i++) {
    $a = [3.5, 1.5, 2.5, 0.5];
    sort($a);
    $n += (int) ($a[0] * 2);
    rsort($a);
    $n += (int) ($a[0] * 2);
}
echo $n;
"#,
    );
    assert_eq!(out.stdout, "1600", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("leak summary: clean"),
        "a float sort must not allocate per element: {}",
        out.stderr
    );
}

/// Guard for the sorts that still refuse a float, so the refusal stays deliberate.
///
/// `asort` and the natural-order sorts keep their keys and have no comparator to hand a slot
/// to, which is also why they already refuse strings and boxed `Mixed`. Admitting floats to
/// `sort`/`rsort` must not quietly admit them here.
#[test]
fn key_preserving_sorts_still_refuse_float_elements() {
    for name in ["asort", "arsort", "natsort", "natcasesort"] {
        let error = compile_source_expect_backend_error(&format!(
            "<?php $a = [2.5, 1.5]; {name}($a);"
        ));
        assert!(
            error.contains(&format!("{name} indexed-array element PHP type Float")),
            "{name} must refuse a float array by name, got: {error}"
        );
    }
}


/// A float sort over `NAN` elements terminates, and so does `usort()` with a comparator that
/// answers "greater" both ways. `<=>` against `NAN` is 1 in both directions, which made the
/// runtime's bubble sort swap the same pair forever. Both float-sort directions now match
/// PHP's exact NAN placement, while arbitrary callbacks remain bounded.
/// Regression for #1353.
#[test]
fn sort_over_nan_elements_and_an_inconsistent_comparator_terminates() {
    let out = compile_and_run(
        r#"<?php
$a = [3.5, NAN, 1.0, -2.0, NAN, 0.5];
$b = $a;
sort($a);
echo count($a), ":";
$separator = "";
foreach ($a as $v) { echo $separator, is_nan($v) ? "nan" : $v; $separator = ","; }
echo "\n";
rsort($b);
$separator = "";
foreach ($b as $v) { echo $separator, is_nan($v) ? "nan" : $v; $separator = ","; }
echo "\n";
$d = [1.0, INF, -INF, 0.0];
sort($d);
echo implode(",", $d), "\n";
rsort($d);
echo implode(",", $d), "\n";
$u = [3, 1, 2];
usort($u, fn($x, $y) => 1);
echo count($u), "\n";
"#,
    );
    assert_eq!(
        out,
        "6:nan,-2,0.5,1,nan,3.5\n3.5,nan,1,-2,nan,0.5\n-INF,0,1,INF\nINF,1,0,-INF\n3\n"
    );
}

/// `array_multisort()` over boxed `mixed` arrays terminates when a key is `NAN`. Its bubble sort
/// had the same unbounded "repeat until no swap" loop as `__rt_usort` (#1353); it is now bounded
/// to `length - 1` passes too, and a consistent comparison still orders the rows like PHP.
#[test]
fn array_multisort_over_a_nan_key_terminates() {
    let out = compile_and_run(
        r#"<?php
function rank(array $keys, array $rows): int { array_multisort($keys, $rows); return count($rows); }
echo rank([NAN, 1.0, NAN], [10, 20, 30]), "\n";
function order(array $keys, array $rows): array { array_multisort($keys, $rows); return $rows; }
echo implode(",", order([3, 1, 2], ["c", "a", "b"])), "\n";
"#,
    );
    assert_eq!(out, "3\na,b,c\n");
}


/// Float sorting returns canonical true through direct, case-insensitive and namespaced calls.
#[test]
fn float_sort_returns_canonical_true() {
    let out = compile_and_run(
        r#"<?php
$a = [2.5, 1.5];
$ok = \SoRt($a);
var_dump($ok, $ok === true, (int) $ok);
$ok = \rSoRt($a);
var_dump($ok, $ok === true, (int) $ok);
$single = [NAN];
var_dump(sort($single), rsort($single));
$empty = [];
var_dump(sort($empty), rsort($empty));
"#,
    );
    assert_eq!(out, concat!(
        "bool(true)\nbool(true)\nint(1)\nbool(true)\nbool(true)\nint(1)\n",
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\n",
    ));
}

/// PHP's NAN placement depends on its small-sort, insertion and partition comparison schedules.
/// Includes both sides of the sixteen-element and five-sample-pivot boundaries.
#[test]
fn float_sort_nan_order_matches_php_across_algorithm_boundaries() {
    let out = compile_and_run(
        r#"<?php
foreach ([2,3,4,5,6,7,8,15,16,17,18,31,32,63,64,1023,1024,1025] as $n) {
    foreach ([false, true] as $descending) {
        $a = [];
        for ($i = 0; $i < $n; $i++) {
            $a[] = $i % 7 === 1 ? NAN : (float) (($i * 37) % 101 - 50);
        }
        if ($descending) { rsort($a); } else { sort($a); }
        $checksum = 0;
        foreach ($a as $v) {
            $checksum = ($checksum * 33 + (is_nan($v) ? 1001 : (int) $v + 100)) % 1000003;
        }
        echo $checksum, ",";
    }
    echo "|";
}
"#,
    );
    assert_eq!(out, concat!(
        "33083,2651,|91860,87607,|29383,891085,|968872,406999,|972817,820982,|",
        "102970,92715,|379292,448749,|953825,307056,|130646,133819,|168823,910831,|",
        "453068,208864,|40980,866289,|789589,779449,|414763,914251,|344399,778181,|",
        "497730,549091,|392185,539836,|416983,703591,|",
    ));
}

/// Equal signed zeroes retain their original order across quicksort partition exchanges.
#[test]
fn float_sort_signed_zero_stays_stable_across_partitions() {
    let out = compile_and_run(
        r#"<?php
$a = [];
for ($i = 0; $i < 32; $i++) { $a[] = $i % 2 === 0 ? -0.0 : 0.0; }
$a[] = 1.0;
$copy = $a;
sort($a);
foreach ($a as $v) { echo var_export($v, true), ","; }
echo "|";
rsort($copy);
foreach ($copy as $v) { echo var_export($v, true), ","; }
"#,
    );
    assert_eq!(out, format!("{}1.0,|1.0,{}", "-0.0,0.0,".repeat(16), "-0.0,0.0,".repeat(16)));
}

/// Declared array parameters and properties keep the same NAN ordering and preserve aliases.
#[test]
fn float_sort_nan_order_through_declared_array_receivers() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function sortNanValues(array &$values, bool $reverse): void {
    if ($reverse) { rsort($values); } else { sort($values); }
}
function showNanValues(array $values): void {
    $separator = "";
    foreach ($values as $value) {
        echo $separator, is_nan($value) ? "nan" : $value;
        $separator = ",";
    }
    echo "|";
}
$values = [3.5, NAN, 1.0, -2.0, NAN, 0.5];
$copy = $values;
sortNanValues($values, false);
showNanValues($values);
showNanValues($copy);
sortNanValues($copy, true);
showNanValues($copy);
class FloatSeries { public array $values = [3.5, NAN, 1.0, -2.0, NAN, 0.5]; }
$series = new FloatSeries();
sort($series->values);
showNanValues($series->values);
"#,
    );
    assert_eq!(out.stdout, concat!(
        "nan,-2,0.5,1,nan,3.5|3.5,nan,1,-2,nan,0.5|",
        "3.5,nan,1,-2,nan,0.5|nan,-2,0.5,1,nan,3.5|",
    ), "stderr: {}", out.stderr);
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}
