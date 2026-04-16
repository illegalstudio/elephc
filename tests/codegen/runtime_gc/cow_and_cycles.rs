use crate::support::*;

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
    assert_eq!(cyclic_allocs, acyclic_allocs + 1);
    assert_eq!(cyclic_frees, acyclic_frees + 1);
}

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
