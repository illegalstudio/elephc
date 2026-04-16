use crate::support::*;

// ========================================================================
// Math functions — trig, inverse trig, hyperbolic, log/exp, utility
// ========================================================================

#[test]
fn test_math_trig_basic() {
    let out = compile_and_run(
        r#"<?php
echo round(sin(0.0), 4) . "|" . round(cos(0.0), 4) . "|" . round(tan(0.0), 4);
"#,
    );
    assert_eq!(out, "0|1|0");
}

#[test]
fn test_math_trig_pi() {
    let out = compile_and_run(
        r#"<?php
echo round(sin(M_PI_2), 4) . "|" . round(cos(M_PI), 1) . "|" . round(tan(M_PI_4), 4);
"#,
    );
    assert_eq!(out, "1|-1|1");
}

#[test]
fn test_math_inverse_trig() {
    let out = compile_and_run(
        r#"<?php
echo round(asin(1.0), 4) . "|" . round(acos(0.0), 4) . "|" . round(atan(1.0), 4);
"#,
    );
    assert_eq!(out, "1.5708|1.5708|0.7854");
}

#[test]
fn test_math_atan2() {
    let out = compile_and_run(
        r#"<?php
echo round(atan2(1.0, 0.0), 4);
"#,
    );
    assert_eq!(out, "1.5708");
}

#[test]
fn test_math_hyperbolic() {
    let out = compile_and_run(
        r#"<?php
echo round(sinh(0.0), 4) . "|" . round(cosh(0.0), 4) . "|" . round(tanh(0.0), 4);
"#,
    );
    assert_eq!(out, "0|1|0");
}

#[test]
fn test_math_log_exp() {
    let out = compile_and_run(
        r#"<?php
echo round(log(M_E), 4) . "|" . log2(8.0) . "|" . log10(1000.0) . "|" . exp(0.0);
"#,
    );
    assert_eq!(out, "1|3|3|1");
}

#[test]
fn test_math_hypot() {
    let out = compile_and_run(
        r#"<?php
echo hypot(3.0, 4.0);
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_math_deg_rad() {
    let out = compile_and_run(
        r#"<?php
echo round(deg2rad(180.0), 4) . "|" . round(rad2deg(M_PI), 1);
"#,
    );
    assert_eq!(out, "3.1416|180");
}

#[test]
fn test_math_pi_function() {
    let out = compile_and_run(
        r#"<?php
echo round(pi(), 4);
"#,
    );
    assert_eq!(out, "3.1416");
}

#[test]
fn test_math_constants() {
    let out = compile_and_run(
        r#"<?php
echo round(M_E, 4) . "|" . round(M_SQRT2, 4) . "|" . round(M_PI_2, 4) . "|" . round(M_PI_4, 4);
"#,
    );
    assert_eq!(out, "2.7183|1.4142|1.5708|0.7854");
}

#[test]
fn test_math_int_coercion() {
    let out = compile_and_run(
        r#"<?php
echo sin(0) . "|" . cos(0) . "|" . log(1) . "|" . exp(0);
"#,
    );
    assert_eq!(out, "0|1|0|1");
}

#[test]
fn test_math_distance_calculation() {
    let out = compile_and_run(
        r#"<?php
$x1 = 1.0; $y1 = 2.0;
$x2 = 4.0; $y2 = 6.0;
$dist = hypot($x2 - $x1, $y2 - $y1);
echo round($dist, 4);
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_return_type_from_foreach() {
    let out = compile_and_run(
        r#"<?php
function find($arr, $target) {
    foreach ($arr as $v) {
        if ($v === $target) { return "found"; }
    }
    return "not found";
}
echo find([1, 2, 3], 2);
"#,
    );
    assert_eq!(out, "found");
}

#[test]
fn test_return_type_mixed_branches() {
    let out = compile_and_run(
        r#"<?php
function describe($n) {
    if ($n > 0) { return "positive"; }
    return 0;
}
$r = describe(5);
echo $r;
"#,
    );
    assert_eq!(out, "positive");
}

#[test]
fn test_return_type_switch_foreach() {
    let out = compile_and_run(
        r#"<?php
function classify($items) {
    foreach ($items as $item) {
        switch ($item) {
            case 0: return "zero";
            default: return "nonzero";
        }
    }
    return "empty";
}
echo classify([0]);
"#,
    );
    assert_eq!(out, "zero");
}

#[test]
fn test_return_string_from_else() {
    let out = compile_and_run(
        r#"<?php
function check($x) {
    if ($x > 10) {
        return "big";
    } else {
        return "small";
    }
}
echo check(5) . "|" . check(15);
"#,
    );
    assert_eq!(out, "small|big");
}

#[test]
fn test_log_natural() {
    let out = compile_and_run(
        r#"<?php
echo round(log(M_E), 4);
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_log_base_10() {
    let out = compile_and_run(
        r#"<?php
echo log(1000, 10);
"#,
    );
    assert_eq!(out, "3");
}

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
fn test_example_cow_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../examples/cow/main.php"));
    assert_eq!(
        out,
        "left: 1 2 3 \nright: 99 2 3 4 \nouterA inner: 10 20 \nouterB inner: 10 77 \n"
    );
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

#[test]
fn test_gc_heap_free_coalesces_adjacent_blocks() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
$a = array_fill(0, 2000, 1);
$b = array_fill(0, 2000, 2);
$keep = array_fill(0, 2000, 3);
unset($a);
unset($b);
$c = array_fill(0, 3000, 4);
echo $c[0] . "|" . count($c) . "|" . $keep[0];
"#,
        65_536,
    );
    assert_eq!(out, "4|3000|3");
}

#[test]
fn test_gc_heap_free_trims_free_tail_chain() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
$a = array_fill(0, 2000, 1);
$b = array_fill(0, 2000, 2);
$tail = array_fill(0, 2000, 3);
unset($b);
unset($tail);
$c = array_fill(0, 5000, 4);
echo $c[0] . "|" . count($c) . "|" . $a[0];
"#,
        65_536,
    );
    assert_eq!(out, "4|5000|1");
}

#[test]
fn test_gc_heap_alloc_splits_oversized_free_block() {
    let out = compile_and_run_with_heap_size(
        r#"<?php
$large = array_fill(0, 4000, 1);
$keep = array_fill(0, 2000, 2);
unset($large);
$small = array_fill(0, 1000, 3);
$mid = array_fill(0, 2500, 4);
echo $small[0] . "|" . count($mid) . "|" . $keep[0];
"#,
        65_536,
    );
    assert_eq!(out, "3|2500|2");
}

#[test]
fn test_gc_heap_alloc_walks_past_small_first_free_block() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    adrp x9, _heap_off@PAGE
    add x9, x9, _heap_off@PAGEOFF
    str xzr, [x9]
    adrp x9, _heap_free_list@PAGE
    add x9, x9, _heap_free_list@PAGEOFF
    str xzr, [x9]
    mov x0, #8
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #8
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #8
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    ldr x0, [sp, #48]
    bl __rt_heap_free
    ldr x0, [sp, #16]
    bl __rt_heap_free
    mov x0, #16
    bl __rt_heap_alloc
    ldr x9, [sp, #16]
    cmp x0, x9
    cset x0, eq
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80"#
        }
        Arch::X86_64 => {
            r#"    lea r9, [rip + _heap_off]
    mov QWORD PTR [r9], 0
    lea r9, [rip + _heap_free_list]
    mov QWORD PTR [r9], 0
    mov eax, 8
    call __rt_heap_alloc
    push rax
    mov eax, 8
    call __rt_heap_alloc
    push rax
    mov eax, 16
    call __rt_heap_alloc
    push rax
    mov eax, 8
    call __rt_heap_alloc
    push rax
    mov rax, QWORD PTR [rsp + 24]
    call __rt_heap_free
    mov rax, QWORD PTR [rsp + 8]
    call __rt_heap_free
    mov eax, 16
    call __rt_heap_alloc
    mov r9, QWORD PTR [rsp + 8]
    cmp rax, r9
    sete al
    movzx eax, al
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall"#
        }
    };
    let out = compile_harness_and_run(
        "<?php",
        256,
        harness,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_gc_heap_alloc_reuses_small_bin_before_bump() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    adrp x9, _heap_off@PAGE
    add x9, x9, _heap_off@PAGEOFF
    str xzr, [x9]
    adrp x9, _heap_free_list@PAGE
    add x9, x9, _heap_free_list@PAGEOFF
    str xzr, [x9]
    adrp x9, _heap_small_bins@PAGE
    add x9, x9, _heap_small_bins@PAGEOFF
    stp xzr, xzr, [x9]
    stp xzr, xzr, [x9, #16]
    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #24
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    ldr x0, [sp, #16]
    bl __rt_heap_free
    adrp x9, _heap_off@PAGE
    add x9, x9, _heap_off@PAGEOFF
    ldr x10, [x9]
    str x10, [sp, #-16]!
    mov x0, #12
    bl __rt_heap_alloc
    ldr x9, [sp, #32]
    cmp x0, x9
    cset x11, eq
    adrp x9, _heap_off@PAGE
    add x9, x9, _heap_off@PAGEOFF
    ldr x9, [x9]
    ldr x10, [sp]
    cmp x9, x10
    cset x12, eq
    and x0, x11, x12
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80"#
        }
        Arch::X86_64 => {
            r#"    lea r9, [rip + _heap_off]
    mov QWORD PTR [r9], 0
    lea r9, [rip + _heap_free_list]
    mov QWORD PTR [r9], 0
    lea r9, [rip + _heap_small_bins]
    mov QWORD PTR [r9], 0
    mov QWORD PTR [r9 + 8], 0
    mov QWORD PTR [r9 + 16], 0
    mov QWORD PTR [r9 + 24], 0
    mov eax, 16
    call __rt_heap_alloc
    push rax
    mov eax, 24
    call __rt_heap_alloc
    push rax
    mov rax, QWORD PTR [rsp + 8]
    call __rt_heap_free
    lea r9, [rip + _heap_off]
    mov r10, QWORD PTR [r9]
    push r10
    mov eax, 12
    call __rt_heap_alloc
    mov r9, QWORD PTR [rsp + 16]
    cmp rax, r9
    sete r11b
    lea r9, [rip + _heap_off]
    mov r9, QWORD PTR [r9]
    mov r10, QWORD PTR [rsp]
    cmp r9, r10
    sete r12b
    and r11b, r12b
    movzx eax, r11b
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall"#
        }
    };
    let out = compile_harness_and_run(
        "<?php",
        256,
        harness,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_heap_debug_double_free_reports_error() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #24
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    ldr x0, [sp, #16]
    bl __rt_heap_free
    ldr x0, [sp, #16]
    bl __rt_heap_free"#
        }
        Arch::X86_64 => {
            r#"    mov eax, 16
    call __rt_heap_alloc
    push rax
    mov eax, 24
    call __rt_heap_alloc
    push rax
    mov rax, QWORD PTR [rsp + 8]
    call __rt_heap_free
    mov rax, QWORD PTR [rsp + 8]
    call __rt_heap_free"#
        }
    };
    let err = compile_harness_expect_failure(
        "<?php",
        65_536,
        harness,
    );
    assert!(err.contains("heap debug detected double free"), "{err}");
}

#[test]
fn test_heap_debug_bad_refcount_reports_error() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    mov x0, #16
    bl __rt_heap_alloc
    str wzr, [x0, #-12]
    bl __rt_incref"#
        }
        Arch::X86_64 => {
            r#"    mov eax, 16
    call __rt_heap_alloc
    mov DWORD PTR [rax - 12], 0
    call __rt_incref"#
        }
    };
    let err = compile_harness_expect_failure(
        "<?php",
        65_536,
        harness,
    );
    assert!(err.contains("heap debug detected bad refcount"), "{err}");
}

#[test]
fn test_heap_debug_free_list_corruption_reports_error() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    mov x0, #24
    bl __rt_heap_alloc
    ldr x0, [sp], #16
    bl __rt_heap_free
    sub x9, x0, #16
    str x9, [x9, #16]
    mov x0, #8
    bl __rt_heap_alloc"#
        }
        Arch::X86_64 => {
            r#"    mov eax, 16
    call __rt_heap_alloc
    push rax
    mov eax, 24
    call __rt_heap_alloc
    mov rax, QWORD PTR [rsp]
    call __rt_heap_free
    mov r9, QWORD PTR [rsp]
    lea r9, [r9 - 16]
    mov QWORD PTR [r9 + 16], r9
    mov eax, 8
    call __rt_heap_alloc"#
        }
    };
    let err = compile_harness_expect_failure(
        "<?php",
        65_536,
        harness,
    );
    assert!(
        err.contains("heap debug detected free-list corruption"),
        "{err}"
    );
}

#[test]
fn test_heap_debug_reports_exit_summary() {
    let out = compile_and_run_with_heap_debug("<?php $a = [1, 2, 3]; unset($a);");
    assert!(out.success, "program failed: {}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: allocs="), "{}", out.stderr);
    assert!(out.stderr.contains("peak_live_bytes="), "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary:"),
        "{}",
        out.stderr
    );
}

#[test]
fn test_heap_debug_poison_freed_payload() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    mov x0, #16
    bl __rt_heap_alloc
    str x0, [sp, #-16]!
    bl __rt_heap_free
    ldr x0, [sp], #16
    ldrb w0, [x0, #8]
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80"#
        }
        Arch::X86_64 => {
            r#"    mov eax, 16
    call __rt_heap_alloc
    push rax
    call __rt_heap_free
    mov rax, QWORD PTR [rsp]
    add rsp, 8
    movzx eax, BYTE PTR [rax + 8]
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall"#
        }
    };
    let out = compile_harness_and_run_with_heap_debug(
        "<?php",
        65_536,
        harness,
    );
    assert_eq!(out, "165");
}

#[test]
fn test_array_literal_spread_grows_past_initial_capacity() {
    let out = compile_and_run(
        r#"<?php
$nums = [...range(1, 10), ...range(11, 20), ...range(21, 30)];
echo count($nums) . "|" . $nums[25];
"#,
    );
    assert_eq!(out, "30|26");
}

#[test]
fn test_array_literal_spread_refcounted_grows_past_initial_capacity() {
    let out = compile_and_run(
        r#"<?php
$inner = [1];
$a = array_fill(0, 10, $inner);
$b = array_fill(0, 10, $inner);
$c = [...$a, ...$b, ...$a];
echo count($c) . "|" . count($c[25]);
"#,
    );
    assert_eq!(out, "30|1");
}

#[test]
fn test_heap_kind_tags_raw_array_hash_and_string() {
    let harness = match target().arch {
        Arch::AArch64 => {
            r#"    mov x0, #16
    bl __rt_heap_alloc
    bl __rt_heap_kind
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80
    mov x0, #4
    mov x1, #8
    bl __rt_array_new
    bl __rt_heap_kind
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80
    mov x0, #4
    mov x1, #0
    bl __rt_hash_new
    bl __rt_heap_kind
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80
    adrp x1, _concat_buf@PAGE
    add x1, x1, _concat_buf@PAGEOFF
    mov w3, #65
    strb w3, [x1]
    mov w3, #66
    strb w3, [x1, #1]
    mov w3, #67
    strb w3, [x1, #2]
    mov x2, #3
    bl __rt_str_persist
    mov x0, x1
    bl __rt_heap_kind
    bl __rt_itoa
    mov x0, #1
    mov x16, #4
    svc #0x80"#
        }
        Arch::X86_64 => {
            r#"    mov eax, 16
    call __rt_heap_alloc
    call __rt_heap_kind
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall
    mov edi, 4
    mov esi, 8
    call __rt_array_new
    call __rt_heap_kind
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall
    mov edi, 4
    xor esi, esi
    call __rt_hash_new
    call __rt_heap_kind
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall
    lea rax, [rip + _concat_buf]
    mov BYTE PTR [rax], 65
    mov BYTE PTR [rax + 1], 66
    mov BYTE PTR [rax + 2], 67
    mov rdx, 3
    call __rt_str_persist
    call __rt_heap_kind
    call __rt_itoa
    mov rsi, rax
    mov edi, 1
    mov eax, 1
    syscall"#
        }
    };
    let out = compile_harness_and_run(
        "<?php",
        65_536,
        harness,
    );
    assert_eq!(out, "0231");
}

#[test]
fn test_new_object_codegen_sets_heap_kind() {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, _) = compile_source_to_asm_with_options(
        "<?php class Foo { public $x = 1; } $o = new Foo();",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(user_asm.contains("new Foo()"));
    match target().arch {
        Arch::AArch64 => assert!(user_asm.contains("str x9, [x0, #-8]"), "{user_asm}"),
        Arch::X86_64 => assert!(user_asm.contains("mov QWORD PTR [rax - 8], r10"), "{user_asm}"),
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_decref_hash_codegen_skips_gc_for_scalar_only_hashes() {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (_user_asm, _runtime_asm, _) = compile_source_to_asm_with_options(
        r#"<?php
$map = ["a" => 1, "b" => 2];
unset($map);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    // The scalar-only GC skip logic lives in the runtime, not user code.
    // Verify it exists in the runtime assembly.
    let runtime_asm = elephc::codegen::generate_runtime(8_388_608, target());
    assert!(
        runtime_asm.contains("__rt_hash_may_have_cyclic_values"),
        "runtime missing cyclic-value check"
    );
    match target().arch {
        Arch::AArch64 => {
            assert!(
                runtime_asm.contains("bl __rt_hash_may_have_cyclic_values"),
                "runtime missing cyclic-value call"
            );
            assert!(
                runtime_asm.contains("cbz x0, __rt_decref_hash_skip"),
                "runtime missing scalar-only skip branch"
            );
        }
        Arch::X86_64 => {
            assert!(
                runtime_asm.contains("call __rt_hash_may_have_cyclic_values"),
                "runtime missing cyclic-value call"
            );
            assert!(
                runtime_asm.contains("jz __rt_decref_hash_skip"),
                "runtime missing scalar-only skip branch"
            );
        }
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_log_base_2() {
    let out = compile_and_run(
        r#"<?php
echo log(256, 2);
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_log_base_custom() {
    let out = compile_and_run(
        r#"<?php
echo round(log(27, 3), 4);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_expr_call_returns_string() {
    let out = compile_and_run(
        r#"<?php
$greet = function($name) { return "Hello " . $name; };
echo $greet("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_expr_call_returns_float() {
    let out = compile_and_run(
        r#"<?php
$calc = function($x) { return $x * 3.14; };
echo $calc(2.0);
"#,
    );
    assert_eq!(out, "6.28");
}

#[test]
fn test_expr_call_returns_int() {
    let out = compile_and_run(
        r#"<?php
$double = function($x) { return $x * 2; };
echo $double(21);
"#,
    );
    assert_eq!(out, "42");
}

#[test]
fn test_expr_call_string_in_concat() {
    let out = compile_and_run(
        r#"<?php
$tag = function($s) { return "<b>" . $s . "</b>"; };
echo "Result: " . $tag("hello");
"#,
    );
    assert_eq!(out, "Result: <b>hello</b>");
}

#[test]
fn test_closure_call_returns_string() {
    let out = compile_and_run(
        r#"<?php
$fn = function() { return "test"; };
$result = $fn();
echo $result;
"#,
    );
    assert_eq!(out, "test");
}

