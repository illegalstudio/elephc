//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of runtime GC copy-on-write and cycle handling, including GC local alias survives original unset, copy-on-write indexed array alias write does not mutate source, and copy-on-write assoc array alias write does not mutate source.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Verifies that an alias to a local array element survives when the original
/// variable and its source are unset. Regression: a dangling pointer could
/// occur when the alias outlives both `$a` and `$inner`.
#[test]
fn test_gc_local_alias_survives_original_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = [21];
$a = $inner;
$b = $a;
unset($a);
unset($inner);
echo $b[0];
"#,
    );
    assert_eq!(out, "21");
}

/// Verifies that writing to an indexed array through an alias does not mutate
/// the source. Compiles `$a = [1,2,3]; $b = $a; $b[0] = 9;` and asserts `$a[0]` remains `1`.
#[test]
fn test_cow_indexed_array_alias_write_does_not_mutate_source() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$b = $a;
$b[0] = 9;
echo $a[0];
echo $b[0];
"#,
    );
    assert_eq!(out, "19");
}

/// Verifies that writing to an associative array through an alias does not mutate
/// the source. Compiles `$a = ["x" => 1]; $b = $a; $b["x"] = 2;` and asserts `$a["x"]` remains `1`.
#[test]
fn test_cow_assoc_array_alias_write_does_not_mutate_source() {
    let out = compile_and_run(
        r#"<?php
$a = ["x" => 1];
$b = $a;
$b["x"] = 2;
echo $a["x"];
echo $b["x"];
"#,
    );
    assert_eq!(out, "12");
}

/// Verifies that growing an array through an alias does not affect the source.
/// Compiles `$a = [1]; $b = $a; $b[4] = 5;` and asserts count and values.
#[test]
fn test_cow_array_growth_after_alias_keeps_source_unchanged() {
    let out = compile_and_run(
        r#"<?php
$a = [1];
$b = $a;
$b[4] = 5;
echo count($a);
echo count($b);
echo $a[0];
echo $b[4];
"#,
    );
    assert_eq!(out, "1515");
}

/// Verifies that `array_push` on a COW alias does not mutate the source array.
#[test]
fn test_cow_array_push_on_alias_keeps_source_unchanged() {
    let out = compile_and_run(
        r#"<?php
$a = [7];
$b = $a;
array_push($b, 9);
echo count($a);
echo count($b);
echo $a[0];
echo $b[1];
"#,
    );
    assert_eq!(out, "1279");
}

/// Verifies that passing an array by value to a function causes a copy-on-write
/// split so that mutations inside the callee do not affect the caller's copy.
#[test]
fn test_cow_pass_by_value_array_mutation_splits_in_callee() {
    let out = compile_and_run(
        r#"<?php
function rewrite($arr) {
    $arr[0] = 9;
    echo $arr[0];
}

$a = [1];
rewrite($a);
echo $a[0];
"#,
    );
    assert_eq!(out, "91");
}

/// Verifies that nested array alias mutation stays shallow until the inner array
/// is written through. Prevents incorrect full-depth COW splitting on outer write.
#[test]
fn test_cow_nested_array_mutation_stays_shallow_until_inner_write() {
    let out = compile_and_run(
        r#"<?php
$outer = [[1]];
$copy = $outer;
$inner = $copy[0];
$inner[0] = 9;
$copy[0] = $inner;
echo $outer[0][0];
echo $copy[0][0];
"#,
    );
    assert_eq!(out, "19");
}

/// Verifies that a COW split path balances GC allocs and frees, confirming no
/// leaked references or premature frees when an alias is written and then unset.
#[test]
fn test_cow_split_path_balances_gc_stats() {
    let baseline = compile_and_run_with_gc_stats("<?php");
    let out = compile_and_run_with_gc_stats(
        r#"<?php
$a = [1, 2, 3];
$b = $a;
$b[0] = 9;
unset($a);
unset($b);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (allocs, frees) = parse_gc_stats(&out.stderr);
    assert_eq!(allocs - baseline_allocs, frees - baseline_frees);
}

/// Verifies that a nested array alias returned from a function survives when all
/// callers and source arrays are unset. Regression: borrowed nested return
/// could become dangling after source unset.
#[test]
fn test_gc_return_borrowed_nested_array_alias_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
function pick_first($rows) {
    $first = $rows[0];
    return $first;
}

$inner = [31];
$rows = [$inner, [32]];
$picked = pick_first($rows);
unset($rows);
unset($inner);
echo $picked[0];
"#,
    );
    assert_eq!(out, "31");
}

/// Verifies that a borrowed-or-owned merge in control flow (if-branch borrowed,
/// else-branch owned) produces a return value that survives caller unset.
#[test]
fn test_gc_control_flow_merge_borrowed_or_owned_return_survives() {
    let out = compile_and_run(
        r#"<?php
function choose($flag, $borrowed) {
    if ($flag) {
        $value = $borrowed;
    } else {
        $value = [42];
    }
    return $value;
}

$inner = [41];
$picked = choose(true, $inner);
unset($inner);
echo $picked[0];
"#,
    );
    assert_eq!(out, "41");
}

/// Verifies that an owned-or-borrowed merge in control flow (if-branch owned,
/// else-branch borrowed) produces a return value that survives caller unset.
#[test]
fn test_gc_control_flow_merge_owned_or_borrowed_other_branch_survives() {
    let out = compile_and_run(
        r#"<?php
function choose($flag, $borrowed) {
    if ($flag) {
        $value = [51];
    } else {
        $value = $borrowed;
    }
    return $value;
}

$inner = [52];
$picked = choose(false, $inner);
unset($inner);
echo $picked[0];
"#,
    );
    assert_eq!(out, "52");
}

/// Verifies that a borrowed alias extracted inside a conditional block survives
/// scope exit and caller unset. Guards against premature collection on scope exit.
#[test]
fn test_gc_scope_exit_after_control_flow_borrowed_alias_survives() {
    let out = compile_and_run(
        r#"<?php
function pick_value($flag, $src) {
    if ($flag) {
        $tmp = $src[0];
    } else {
        $tmp = [0];
    }
    return $tmp;
}

$inner = [61];
$src = [$inner];
$picked = pick_value(true, $src);
unset($src);
unset($inner);
echo $picked[0];
"#,
    );
    assert_eq!(out, "61");
}

/// Verifies that an exhaustive if-else where both branches allocate an owned local
/// produces the same GC alloc/free counts as two separate function calls with
/// direct allocation. Guards against incorrect GC state on if-path merge.
#[test]
fn test_gc_scope_exit_after_exhaustive_if_owned_local_is_freed() {
    let baseline = compile_and_run_with_gc_stats(
        r#"<?php
function build_and_drop_direct() {
    $tmp = [11];
}

build_and_drop_direct();
build_and_drop_direct();
"#,
    );
    assert!(
        baseline.success,
        "baseline program failed: {}",
        baseline.stderr
    );
    let exhaustive = compile_and_run_with_gc_stats(
        r#"<?php
function build_and_drop($flag) {
    if ($flag) {
        $tmp = [11];
    } else {
        $tmp = [22];
    }
}

build_and_drop(true);
build_and_drop(false);
"#,
    );
    assert!(exhaustive.success, "program failed: {}", exhaustive.stderr);
    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (exhaustive_allocs, exhaustive_frees) = parse_gc_stats(&exhaustive.stderr);
    assert_eq!(baseline_allocs, exhaustive_allocs);
    assert_eq!(baseline_frees, exhaustive_frees);
}

/// Verifies that a nested associative alias survives outer and inner unset.
/// `$alias = $outer["box"]; unset($outer); unset($inner);` and reads through `$alias`.
#[test]
fn test_gc_nested_assoc_alias_survives_outer_unset() {
    let out = compile_and_run(
        r#"<?php
$inner = ["nums" => [71, 72]];
$outer = ["box" => $inner];
$alias = $outer["box"];
unset($outer);
unset($inner);
$nums = $alias["nums"];
echo $nums[1];
"#,
    );
    assert_eq!(out, "72");
}

/// Verifies that a self-referential object cycle is reclaimed by GC with the same
/// alloc/free counts as an acyclic reference. Confirms cycle collection does not
/// leak or over-collect compared to the acyclic baseline.
#[test]
fn test_gc_collect_cycles_reclaims_object_self_cycle() {
    let acyclic = compile_and_run_with_gc_stats(
        r#"<?php
class Node { public $next = null; }
$n = new Node();
unset($n);
"#,
    );
    assert!(
        acyclic.success,
        "acyclic program failed: {}",
        acyclic.stderr
    );

    let cyclic = compile_and_run_with_gc_stats(
        r#"<?php
class Node { public $next = null; }
$n = new Node();
$n->next = $n;
unset($n);
"#,
    );
    assert!(cyclic.success, "cyclic program failed: {}", cyclic.stderr);

    let (acyclic_allocs, acyclic_frees) = parse_gc_stats(&acyclic.stderr);
    let (cyclic_allocs, cyclic_frees) = parse_gc_stats(&cyclic.stderr);
    assert_eq!(acyclic.stdout, "");
    assert_eq!(cyclic.stdout, "");
    assert_eq!(acyclic_allocs, cyclic_allocs);
    assert_eq!(acyclic_frees, cyclic_frees);
}

/// Verifies that a cycle between an array and an object (`$a = [$n]; $n->next = $a`)
/// is reclaimed by GC with the same alloc/free counts as an acyclic array-object
/// reference pair.
#[test]
fn test_gc_collect_cycles_reclaims_array_object_cycle() {
    let acyclic = compile_and_run_with_gc_stats(
        r#"<?php
class Node { public $next = null; }
$n = new Node();
$a = [$n];
unset($a);
unset($n);
"#,
    );
    assert!(
        acyclic.success,
        "acyclic program failed: {}",
        acyclic.stderr
    );

    let cyclic = compile_and_run_with_gc_stats(
        r#"<?php
class Node { public $next = null; }
$n = new Node();
$a = [$n];
$n->next = $a;
unset($a);
unset($n);
"#,
    );
    assert!(cyclic.success, "cyclic program failed: {}", cyclic.stderr);

    let (acyclic_allocs, acyclic_frees) = parse_gc_stats(&acyclic.stderr);
    let (cyclic_allocs, cyclic_frees) = parse_gc_stats(&cyclic.stderr);
    assert_eq!(acyclic.stdout, "");
    assert_eq!(cyclic.stdout, "");
    assert_eq!(acyclic_allocs, cyclic_allocs);
    assert_eq!(acyclic_frees, cyclic_frees);
}

/// Verifies that forming a two-element cycle from indexed int arrays (`$a[0] = $b;
/// $b[0] = $a`) allocates 3 extra slots for boxed Mixed conversion compared to the
/// acyclic case. Detaches before cycle to avoid mutating the original.
#[test]
fn test_cow_array_array_assignment_detaches_before_forming_cycle() {
    let acyclic = compile_and_run_with_gc_stats(
        r#"<?php
$a = [0];
$b = [0];
$a[0] = $b;
unset($a);
unset($b);
"#,
    );
    assert!(
        acyclic.success,
        "acyclic program failed: {}",
        acyclic.stderr
    );

    let cyclic = compile_and_run_with_gc_stats(
        r#"<?php
$a = [0];
$b = [0];
$a[0] = $b;
$b[0] = $a;
unset($a);
unset($b);
"#,
    );
    assert!(cyclic.success, "cyclic program failed: {}", cyclic.stderr);

    let (acyclic_allocs, acyclic_frees) = parse_gc_stats(&acyclic.stderr);
    let (cyclic_allocs, cyclic_frees) = parse_gc_stats(&cyclic.stderr);
    assert_eq!(acyclic.stdout, "");
    assert_eq!(cyclic.stdout, "");
    // The second array-to-array assignment forms a cycle and now converts the
    // second indexed int array to boxed Mixed slots before storing the peer.
    assert_eq!(cyclic_allocs, acyclic_allocs + 3);
    assert_eq!(cyclic_frees, acyclic_frees + 3);
}

/// Verifies that forming a two-element cycle from empty arrays (`$a[0] = $b; $b[0] = $a`)
/// allocates 1 extra slot for boxed Mixed conversion compared to the acyclic case.
#[test]
fn test_cow_empty_array_assignment_detaches_before_forming_cycle() {
    let acyclic = compile_and_run_with_gc_stats(
        r#"<?php
$a = [];
$b = [];
$a[0] = $b;
unset($a);
unset($b);
"#,
    );
    assert!(
        acyclic.success,
        "acyclic program failed: {}",
        acyclic.stderr
    );

    let cyclic = compile_and_run_with_gc_stats(
        r#"<?php
$a = [];
$b = [];
$a[0] = $b;
$b[0] = $a;
unset($a);
unset($b);
"#,
    );
    assert!(cyclic.success, "cyclic program failed: {}", cyclic.stderr);

    let (acyclic_allocs, acyclic_frees) = parse_gc_stats(&acyclic.stderr);
    let (cyclic_allocs, cyclic_frees) = parse_gc_stats(&cyclic.stderr);
    assert_eq!(cyclic_allocs, acyclic_allocs + 1);
    assert_eq!(cyclic_frees, acyclic_frees + 1);
}

/// Verifies that a cycle between two Mixed hashes (`$a["peer"] = $b; $b["peer"] = $a`)
/// allocates 1 extra slot for boxed Mixed conversion compared to the acyclic case.
#[test]
fn test_cow_hash_assignment_detaches_before_forming_cycle() {
    let acyclic = compile_and_run_with_gc_stats(
        r#"<?php
$a = ["peer" => null];
$b = ["peer" => null];
$a["peer"] = $b;
unset($a);
unset($b);
"#,
    );
    assert!(
        acyclic.success,
        "acyclic program failed: {}",
        acyclic.stderr
    );

    let cyclic = compile_and_run_with_gc_stats(
        r#"<?php
$a = ["peer" => null];
$b = ["peer" => null];
$a["peer"] = $b;
$b["peer"] = $a;
unset($a);
unset($b);
"#,
    );
    assert!(cyclic.success, "cyclic program failed: {}", cyclic.stderr);

    let (acyclic_allocs, acyclic_frees) = parse_gc_stats(&acyclic.stderr);
    let (cyclic_allocs, cyclic_frees) = parse_gc_stats(&cyclic.stderr);
    assert_eq!(acyclic.stdout, "");
    assert_eq!(cyclic.stdout, "");
    assert_eq!(cyclic_allocs, acyclic_allocs + 1);
    assert_eq!(cyclic_frees, acyclic_frees + 1);
}

/// Verifies that a cycle between a Mixed-hash and an object (`$h = ["node" => $n];
/// $n->next = $h`) is reclaimed by GC with the same alloc/free counts as the acyclic
/// case. Mixed hash boxes prevent extra allocation on cycle formation.
#[test]
fn test_gc_collect_cycles_reclaims_mixed_object_hash_cycle() {
    let acyclic = compile_and_run_with_gc_stats(
        r#"<?php
class Node { public $next = null; }
$n = new Node();
$h = ["node" => $n];
unset($h);
unset($n);
"#,
    );
    assert!(
        acyclic.success,
        "acyclic program failed: {}",
        acyclic.stderr
    );

    let cyclic = compile_and_run_with_gc_stats(
        r#"<?php
class Node { public $next = null; }
$n = new Node();
$h = ["node" => $n];
$n->next = $h;
unset($h);
unset($n);
"#,
    );
    assert!(cyclic.success, "cyclic program failed: {}", cyclic.stderr);

    let (acyclic_allocs, acyclic_frees) = parse_gc_stats(&acyclic.stderr);
    let (cyclic_allocs, cyclic_frees) = parse_gc_stats(&cyclic.stderr);
    assert_eq!(acyclic.stdout, "");
    assert_eq!(cyclic.stdout, "");
    assert_eq!(acyclic_allocs, cyclic_allocs);
    assert_eq!(acyclic_frees, cyclic_frees);
}
