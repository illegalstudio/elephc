//! Purpose:
//! Checks that integer and float seeds of boxed loop counters stay in frame-local numeric cells.
//!
//! Called from:
//! - The focused runtime GC codegen suite.
//!
//! Key details:
//! - Allocation counts are measured only around the loops, so setup and output are excluded.
//! - Both EIR modes run under strict heap debugging; the mixed-seed expectations come from PHP 8.5.

use crate::support::{compile_and_run_with_heap_debug_tagged, without_ir_opt};

/// Runs one fixture in both EIR modes, requiring clean heaps and returning both stdouts.
fn both_modes(source: &str) -> (String, String) {
    let optimized = compile_and_run_with_heap_debug_tagged(source);
    let plain = without_ir_opt(|| compile_and_run_with_heap_debug_tagged(source));
    for (mode, output) in [("optimized", &optimized), ("--no-ir-opt", &plain)] {
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {}", output.stderr);
    }
    (optimized.stdout, plain.stdout)
}

/// Packed-field seeds of nested inclusive loops allocate nothing per loop entry (Ivory probe).
#[test]
fn test_numeric_seed_cells_packed_field_nested_seeds_do_not_allocate() {
    let source = r#"<?php
packed class SeedBounds { public int $first; public int $last; }
function seededCells(int $count): void {
    $bounds = buffer_new<SeedBounds>($count);
    $cells = buffer_new<int>(16);
    for ($i = 0; $i < $count; $i++) { $bounds[$i]->last = 3; }
    $sum = 0;
    $start = memory_get_allocations();
    for ($box = 0; $box < $count; $box++) {
        for ($row = $bounds[$box]->first; $row <= $bounds[$box]->last; $row++) {
            for ($column = $bounds[$box]->first; $column <= $bounds[$box]->last; $column++) {
                $cells[$row * 4 + $column]++;
            }
        }
    }
    $end = memory_get_allocations();
    for ($i = 0; $i < 16; $i++) { $sum += $cells[$i]; }
    echo "allocations=", $end - $start, " sum=", $sum;
    buffer_free($cells);
    buffer_free($bounds);
}
seededCells($argc + 999);
"#;
    let (optimized, plain) = both_modes(source);
    assert_eq!(optimized, "allocations=0 sum=16000");
    assert!(plain.ends_with(" sum=16000"), "{plain}");
}

/// A checked seed of a guarded counter and its exit publication stay off the heap, final value included.
#[test]
fn test_numeric_seed_cells_guarded_checked_seed_does_not_allocate() {
    let source = r#"<?php
function guardedSeeds(int $n, int $last): int {
    $sum = 0;
    $start = memory_get_allocations();
    for ($b = 0; $b < $n; $b++) {
        for ($y = $last - $b; $y <= $last; $y++) { $sum += (int)$y; }
        $sum += (int)$y;
    }
    $end = memory_get_allocations();
    // An integer return keeps the accumulator numeric; a string consumer would box its sums.
    echo $end - $start, ':';
    return $sum;
}
echo guardedSeeds($argc + 999, $argc + 1);
"#;
    let (optimized, plain) = both_modes(source);
    assert_eq!(optimized, "0:-165662500");
    assert!(plain.ends_with(":-165662500"), "{plain}");
}

/// Integer and float seeds sharing one counter, re-entry and post-loop overflow keep PHP results.
#[test]
fn test_numeric_seed_cells_mixed_seeds_match_php() {
    let source = r#"<?php
function seededWalk(int $n, float $start, int $last): string {
    $out = '';
    for ($b = 0; $b < $n; $b++) {
        if ($b % 2 === 1) { $x = $start; } else { $x = $b - 1; }
        for (; $x <= $last; $x++) { $out .= $x . ','; }
        $out .= '[' . $x . ']';
        for ($y = $last - $b; $y <= $last; $y++) { $out .= $y * 2 . ';'; }
        $out .= '<' . $y . '>';
    }
    return $out;
}
function edgeSeeds(int $edge): string {
    $out = '';
    for ($r = 0; $r < 3; $r++) {
        for ($z = $edge - $r; $z < $edge; $z++) { $out .= gettype($z) . ':' . $z . ' '; }
        $z++;
        $out .= gettype($z) . ':' . $z . '|';
    }
    return $out;
}
echo seededWalk($argc + 3, 1.5, $argc + 1), "\n";
echo edgeSeeds(PHP_INT_MAX - $argc + 1), "\n";
"#;
    let expected = "-1,0,1,2,[3]4;<3>1.5,[2.5]2;4;<3>1,2,[3]0;2;4;<3>1.5,[2.5]-2;0;2;4;<3>\n\
double:9.2233720368548E+18|integer:9223372036854775806 double:9.2233720368548E+18|\
integer:9223372036854775805 integer:9223372036854775806 double:9.2233720368548E+18|\n";
    let (optimized, plain) = both_modes(source);
    assert_eq!(optimized, expected);
    assert_eq!(plain, expected);
}
