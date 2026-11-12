//! Purpose:
//! Checks that integer and float seeds of boxed loop counters stay in frame-local numeric cells.
//!
//! Called from:
//! - The focused runtime GC codegen suite.
//!
//! Key details:
//! - Allocation windows include loop re-entry and returned sums, excluding setup and output.
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

/// A computed cell key held in a local and snapshotted by `$starts[$cell]--` stays off the heap
/// (Ivory grid-index-consumer probe): the key copy and the cell local both use frame cells.
#[test]
fn test_numeric_seed_cells_computed_key_snapshot_does_not_allocate() {
    let source = r#"<?php
packed class GridRange { public int $firstRow; public int $lastRow; public int $firstColumn; public int $lastColumn; }
function scatter(buffer<GridRange> $ranges, buffer<int> $starts, buffer<int> $entries, int $columns, int $count): void
{
    for ($box = $count - 1; $box >= 0; $box--) {
        for ($row = $ranges[$box]->firstRow; $row <= $ranges[$box]->lastRow; $row++) {
            for ($column = $ranges[$box]->firstColumn; $column <= $ranges[$box]->lastColumn; $column++) {
                $cell = $row * $columns + $column;
                $starts[$cell]--;
                $entries[$starts[$cell]] = $cell + 1;
            }
        }
    }
}
function run(int $count): void
{
    $ranges = buffer_new<GridRange>($count);
    $starts = buffer_new<int>(4);
    $entries = buffer_new<int>(4 * $count);
    for ($i = 0; $i < $count; $i++) {
        $ranges[$i]->firstRow = 0; $ranges[$i]->lastRow = 1;
        $ranges[$i]->firstColumn = 0; $ranges[$i]->lastColumn = 1;
    }
    for ($i = 0; $i < 4; $i++) { $starts[$i] = ($i + 1) * $count; }
    $before = memory_get_allocations();
    scatter($ranges, $starts, $entries, 2, $count);
    $after = memory_get_allocations();
    $checksum = 0;
    for ($i = 0; $i < 4 * $count; $i++) { $checksum += $entries[$i]; }
    echo "allocations=", $after - $before, " checksum=", $checksum, " first=", $starts[0], " last=", $starts[3];
    buffer_free($entries);
    buffer_free($starts);
    buffer_free($ranges);
}
run($argc + 1023);
"#;
    let (optimized, plain) = both_modes(source);
    assert_eq!(optimized, "allocations=0 checksum=10240 first=0 last=3072");
    // `--no-ir-opt` still boxes on the 4096 visits (about 3.5 allocations each); pinning the
    // count catches a plain-mode regression as well as the optimized zero.
    assert_eq!(plain, "allocations=14338 checksum=10240 first=0 last=3072");
}

/// Key snapshots of an `int|float` local into an ordinary array keep PHP's updates and keys.
#[test]
fn test_numeric_seed_cells_array_key_snapshot_keeps_php_semantics() {
    let source = r#"<?php
function histogram(int $limit): string
{
    $counts = [0 => 0, 2 => 0, 4 => 0];
    for ($i = 0; $i < $limit; $i++) {
        $key = ($i % 3) * 2;
        $counts[$key]++;
        $counts[$key] += 10;
    }
    return implode(',', $counts) . ' ' . implode(',', array_keys($counts));
}
echo histogram($argc + 5);
"#;
    let (optimized, plain) = both_modes(source);
    assert_eq!(optimized, "22,22,22 0,2,4");
    assert_eq!(plain, "22,22,22 0,2,4");
}

/// A buffer sum keeps its seed and final publication in frame cells, including an empty loop.
#[test]
fn test_numeric_seed_cells_lazy_buffer_sum_does_not_allocate() {
    let source = r#"<?php
function sumEntries(buffer<int> $entries, int $n): int {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += $entries[$i]; }
    return $sum;
}
function measureSum(int $n): void {
    $entries = buffer_new<int>($n);
    for ($i = 0; $i < $n; $i++) { $entries[$i] = 1; }
    $before = memory_get_allocations();
    $sum = sumEntries($entries, $n);
    $after = memory_get_allocations();
    echo $after - $before, ':', $sum, "\n";
    buffer_free($entries);
}
measureSum($argc + 999);
measureSum(0);
measureSum($argc);
"#;
    let (optimized, plain) = both_modes(source);
    assert_eq!(optimized, "0:1000\n0:0\n0:1\n");
    assert_eq!(plain, "2001:1000\n1:0\n3:1\n");
}

/// A lazy buffer sum retains frame cells through overflow and subsequent float updates.
#[test]
fn test_numeric_seed_cells_lazy_buffer_sum_overflow_does_not_allocate() {
    let source = r#"<?php
function sumOverflow(buffer<int> $entries, int $n): float {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += $entries[$i]; }
    return (float)$sum;
}
$entries = buffer_new<int>(4);
$entries[0] = PHP_INT_MAX;
$entries[1] = $argc;
$entries[2] = -PHP_INT_MAX;
$entries[3] = $argc;
$before = memory_get_allocations();
$sum = sumOverflow($entries, $argc + 3);
$after = memory_get_allocations();
echo $after - $before, ':';
var_dump($sum);
buffer_free($entries);
"#;
    let (optimized, plain) = both_modes(source);
    assert_eq!(optimized, "0:float(1)\n");
    assert_eq!(plain, "9:float(1)\n");
}

/// Ivory's constant-index scatter keeps packed-field row and column seeds off the heap on re-entry.
#[test]
fn test_numeric_seed_cells_lazy_packed_scatter_does_not_allocate() {
    let source = r#"<?php
packed class ScatterRange {
    public int $firstRow; public int $lastRow;
    public int $firstColumn; public int $lastColumn;
}
function constantIndex(buffer<ScatterRange> $ranges, buffer<int> $starts, buffer<int> $entries, int $count): int {
    for ($box = $count - 1; $box >= 0; $box--) {
        for ($row = $ranges[$box]->firstRow; $row <= $ranges[$box]->lastRow; $row++) {
            for ($column = $ranges[$box]->firstColumn; $column <= $ranges[$box]->lastColumn; $column++) {
                $starts[0]--;
                $entries[$starts[0]] = 1;
            }
        }
    }
    return $entries[0] + $entries[4 * $count - 1];
}
function measureScatter(int $count): void {
    $ranges = buffer_new<ScatterRange>($count);
    $starts = buffer_new<int>(1);
    $entries = buffer_new<int>(4 * $count);
    try {
        for ($i = 0; $i < $count; $i++) {
            $ranges[$i]->lastRow = 1;
            $ranges[$i]->lastColumn = 1;
        }
        $starts[0] = 4 * $count;
        $before = memory_get_allocations();
        $checksum = constantIndex($ranges, $starts, $entries, $count);
        $after = memory_get_allocations();
        echo $after - $before, ':', $checksum;
    } finally {
        buffer_free($entries);
        buffer_free($starts);
        buffer_free($ranges);
    }
}
measureScatter($argc + 4095);
"#;
    let (optimized, plain) = both_modes(source);
    assert_eq!(optimized, "0:2");
    assert_eq!(plain, "40962:2");
}
