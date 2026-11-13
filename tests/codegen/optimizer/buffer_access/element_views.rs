//! Purpose:
//! Verifies buffer descriptor hoisting through a packed element-view local
//! (`$p = $points[$i]`), the spelling that used to keep one handle resolution per element.
//!
//! Called from:
//! - The buffer-access codegen regression module.
//!
//! Key details:
//! - The view loop must reach the same optimized shape as the direct `$points[$i]->x` loop:
//!   one preheader resolution, one record address per iteration and no read of the view slot.
//! - Both optimizer modes must print the same result and die with the same fatal, including a
//!   stale handle in an empty loop (no report) and a partially applied out-of-range loop.
//! - A loop that can free the buffer keeps the per-access resolution, and a view reassigned
//!   on a branch is still read through its slot after the join.

use super::{assert_both_modes_fail_after, function_assembly, function_ir, run_variant};

/// The direct and the element-view spelling of one packed integration step.
const VIEW_SOURCE: &str = r#"<?php
packed class Point { public float $x; public float $y; public float $vx; public float $vy; }
function direct(buffer<Point> $points, int $count, float $dt): void {
    for ($i = 0; $i < $count; $i++) {
        $points[$i]->x = $points[$i]->x + $points[$i]->vx * $dt;
        $points[$i]->y = $points[$i]->y + $points[$i]->vy * $dt;
    }
}
function view(buffer<Point> $points, int $count, float $dt): void {
    for ($i = 0; $i < $count; $i++) {
        $point = $points[$i];
        $point->x = $point->x + $point->vx * $dt;
        $point->y = $point->y + $point->vy * $dt;
    }
}
function seed(buffer<Point> $points, int $count): void {
    for ($i = 0; $i < $count; $i++) {
        $p = $points[$i];
        $p->x = $i * 0.5;
        $p->y = $i * 0.25;
        $p->vx = 2.0;
        $p->vy = -1.0;
    }
}
function checksum(buffer<Point> $points, int $count): float {
    $sum = 0.0;
    for ($i = 0; $i < $count; $i++) {
        $p = $points[$i];
        $sum = $sum + $p->x + $p->y;
    }
    return $sum;
}
$points = buffer_new<Point>(8);
seed($points, 8);
direct($points, 8, 0.5);
echo checksum($points, 8), "\n";
seed($points, 8);
$before = memory_get_allocations();
view($points, 8, 0.5);
$after = memory_get_allocations();
echo checksum($points, 8), "\n";
echo "allocations=", $after - $before, "\n";
buffer_free($points);
"#;

/// The two checksums both spellings must print for `VIEW_SOURCE` in either optimizer mode.
const VIEW_CHECKSUMS: &str = "25\n25\n";

/// View loops over a freed handle, past the end, and with an observable partial update.
const VIEW_SAFETY_SOURCE: &str = r#"<?php
packed class Point { public float $x; public float $y; }
function view(buffer<Point> $points, int $count): void {
    for ($i = 0; $i < $count; $i++) {
        $p = $points[$i];
        $p->x = $p->x + 1.0;
        $p->y = $p->y + 2.0;
    }
}
$kind = $argc > 1 ? $argv[1] : 'KIND';
$points = buffer_new<Point>(2);
if ($kind === 'bounds') {
    view($points, 2);
    echo $points[1]->x + $points[1]->y, "\n";
    view($points, 3);
}
$alias = $points;
buffer_free($points);
view($alias, 0);
echo "empty\n";
view($alias, 1);
"#;

/// The view loop resolves its handle once in the preheader and reads no element through the
/// view slot: the record address feeds every field access, as in the direct loop.
#[test]
fn test_packed_element_view_loop_hoists_like_the_direct_loop() {
    let direct = function_ir(VIEW_SOURCE, true, "direct");
    let view = function_ir(VIEW_SOURCE, true, "view");
    for ir in [&direct, &view] {
        let preheader = ir.split("for.cond(").next().expect("loop preheader");
        assert_eq!(ir.matches("= buffer_resolve ").count(), 1, "{ir}");
        assert!(preheader.contains("= buffer_resolve "), "{ir}");
        assert_eq!(ir.matches("= buffer_get_resolved ").count(), 1, "{ir}");
        assert!(!ir.contains("= buffer_get "), "{ir}");
    }
    let body = view.split("for.body:").nth(1).expect("view loop body");
    assert!(!body.contains("= load_local "), "{view}");
    assert_eq!(body.matches("prop_get ").count(), direct.matches("prop_get ").count(), "{view}");
}

/// Both spellings print the same integration result in either optimizer mode, and the
/// optimized view loop allocates nothing.
#[test]
fn test_packed_element_view_loop_matches_the_direct_loop() {
    for optimized in [true, false] {
        let output = run_variant(VIEW_SOURCE, optimized);
        assert!(output.starts_with(VIEW_CHECKSUMS), "optimized={optimized}: {output}");
        if optimized {
            assert!(output.ends_with("allocations=0\n"), "{output}");
        }
    }
}

/// The seeding and reading loops, which only ever use the view spelling, hoist as well.
#[test]
fn test_packed_element_view_seed_and_read_loops_hoist() {
    for name in ["seed", "checksum"] {
        let ir = function_ir(VIEW_SOURCE, true, name);
        assert_eq!(ir.matches("= buffer_resolve ").count(), 1, "{name}: {ir}");
        assert!(!ir.contains("= buffer_get "), "{name}: {ir}");
    }
}

/// Every supported target keeps the view loop body free of the faulting handle resolver.
#[test]
fn test_packed_element_view_loop_body_never_calls_the_resolver() {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let assembly = function_assembly(VIEW_SOURCE, target, "view");
        assert!(assembly.contains("__rt_buffer_try_resolve"), "{target}: {assembly}");
        assert!(!assembly.contains("__rt_buffer_resolve\n"), "{target}: {assembly}");
        assert_eq!(assembly.matches("__rt_buffer_try_resolve").count(), 1, "{target}: {assembly}");
    }
}

/// A stale handle is not reported by a view loop that never runs, and dies at the first access
/// of one that does.
#[test]
fn test_packed_element_view_loop_reports_stale_handle_at_the_access() {
    assert!(function_ir(VIEW_SAFETY_SOURCE, true, "view").contains("= buffer_resolve "));
    assert_both_modes_fail_after(VIEW_SAFETY_SOURCE, "empty\n", "use of buffer after buffer_free()");
}

/// An out-of-range view loop applies the in-range updates before it traps.
#[test]
fn test_packed_element_view_loop_still_traps_out_of_range_index() {
    let source = VIEW_SAFETY_SOURCE.replace("'KIND'", "'bounds'");
    assert_both_modes_fail_after(&source, "3\n", "buffer index out of bounds");
}

/// A loop that can free the buffer between the view and its field access keeps resolving the
/// handle on every access and still reads through the stored view address.
#[test]
fn test_packed_element_view_loop_calling_a_free_keeps_per_access_resolution() {
    let source = r#"<?php
packed class Point { public float $x; public float $y; }
function release(buffer<Point> $points): void { buffer_free($points); }
function drain(buffer<Point> $points, int $count): float {
    $sum = 0.0;
    for ($i = 0; $i < $count; $i++) {
        $p = $points[$i];
        $sum = $sum + $p->x;
        if ($i === 1) { release($points); }
    }
    return $sum;
}
$points = buffer_new<Point>(4);
$points[0]->x = 1.5;
$points[1]->x = 2.5;
echo drain($points, 2), "\n";
$again = buffer_new<Point>(4);
echo drain($again, 3), "\n";
"#;
    let ir = function_ir(source, true, "drain");
    assert!(!ir.contains("= buffer_resolve "), "{ir}");
    assert!(ir.contains("= buffer_get "), "{ir}");
    assert_both_modes_fail_after(source, "4\n", "use of buffer after buffer_free()");
}

/// A view reassigned on one branch is read through its slot after the join: forwarding never
/// crosses a block boundary, and the loop still resolves its handle once.
#[test]
fn test_packed_element_view_reassigned_on_a_branch_keeps_its_slot_read() {
    let source = r#"<?php
packed class Point { public float $x; public float $y; }
function pick(buffer<Point> $points, int $count): float {
    $sum = 0.0;
    for ($i = 0; $i < $count; $i++) {
        $p = $points[$i];
        if ($i % 2 === 1) { $p = $points[0]; }
        $sum = $sum + $p->x;
    }
    return $sum;
}
$points = buffer_new<Point>(3);
$points[0]->x = 4.0;
$points[1]->x = 100.0;
$points[2]->x = 200.0;
echo pick($points, 3), "\n";
"#;
    let ir = function_ir(source, true, "pick");
    assert_eq!(ir.matches("= buffer_resolve ").count(), 1, "{ir}");
    assert!(!ir.contains("= buffer_get "), "{ir}");
    for optimized in [true, false] {
        assert_eq!(run_variant(source, optimized), "208\n", "optimized={optimized}");
    }
}
