//! Purpose:
//! End-to-end coverage for scalar `buffer<T>` element `++`/`--` lowered without Mixed staging.
//!
//! Called from:
//! - `cargo test --test codegen_tests optimizer::buffer_element_incdec`.
//!
//! Key details:
//! - Every runtime fixture compiles through the CLI with the EIR optimizer on and off, heap
//!   debugging and GC statistics, and runs once per argument list to vary the visit count.
//! - Values are checked against the same updates on PHP arrays, against a direct float write for
//!   the int overflow boundary, and against the width rules of narrow and `f32` elements.
//! - The allocation fixtures compare a small and a large visit count, so a boxed cell per update
//!   shows up as thousands of extra allocations.
//! - `examples/buffer-counting-sort` is run here as well, so its printed output stays pinned.

use super::*;

/// Compiles a fixture in one IR mode with heap debugging and GC statistics, then runs it once per
/// argument list and returns each run's success flag, stdout and stderr.
fn run_variants(source: &str, optimized: bool, argvs: &[&[&str]]) -> Vec<(bool, String, String)> {
    let dir = make_cli_test_dir("elephc_buffer_incdec");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write buffer increment fixture");
    let mode = if optimized { "--ir-opt=on" } else { "--ir-opt=off" };
    let compiled = elephc_cli_command(&dir)
        .args([mode, "--heap-debug", "--gc-stats"])
        .arg(&path)
        .output()
        .expect("compile buffer increment fixture");
    assert!(compiled.status.success(), "{mode}: {}", String::from_utf8_lossy(&compiled.stderr));
    let runs = argvs
        .iter()
        .map(|args| {
            let output = run_binary_with_args(&path.with_extension(""), &dir, args);
            (
                output.status.success(),
                String::from_utf8_lossy(&output.stdout).into_owned(),
                String::from_utf8_lossy(&output.stderr).into_owned(),
            )
        })
        .collect();
    fs::remove_dir_all(dir).expect("remove buffer increment fixture");
    runs
}

/// Runs a fixture once in one IR mode, requiring success and a clean heap, and returns stdout.
fn run_clean(source: &str, optimized: bool) -> String {
    let (success, stdout, stderr) = run_variants(source, optimized, &[&[]]).remove(0);
    assert!(success, "ir-opt={optimized}: {stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "ir-opt={optimized}: {stderr}");
    stdout
}

/// Emits the fixture's EIR in one IR mode.
fn emit_ir(source: &str, optimized: bool) -> String {
    let dir = make_cli_test_dir("elephc_buffer_incdec_ir");
    let path = dir.join("main.php");
    fs::write(&path, source).expect("write buffer increment EIR fixture");
    let mode = if optimized { "--ir-opt=on" } else { "--ir-opt=off" };
    let output = elephc_cli_command(&dir)
        .args([mode, "--emit-ir"])
        .arg(&path)
        .output()
        .expect("emit buffer increment EIR");
    assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
    fs::remove_dir_all(dir).expect("remove buffer increment EIR fixture");
    String::from_utf8(output.stdout).expect("EIR is UTF-8")
}

/// Returns one printed function's body, from its header to the next function.
fn function_body<'a>(ir: &'a str, name: &str) -> &'a str {
    ir.split(&format!("\n  function {name}("))
        .nth(1)
        .unwrap_or_else(|| panic!("EIR has no function {name}"))
        .split("\n  function ")
        .next()
        .expect("function body")
}

/// The Ivory grid shape: property and local counting loops over a counting sort.
const GRID: &str = r#"<?php
final class CountingGrid {
    private buffer<int> $cellStart;
    private buffer<int> $entries;
    public function __construct(private int $cells, int $capacity) {
        $this->cellStart = buffer_new<int>($cells);
        $this->entries = buffer_new<int>($capacity);
    }
    public function fill(buffer<int> $boxCells, buffer<int> $runEnds, int $count): int {
        for ($cell = 0; $cell < $this->cells; $cell++) {
            $this->cellStart[$cell] = 0;
        }
        for ($box = 0; $box < $count; $box++) {
            $this->cellStart[$boxCells[$box]]++;
        }
        $counted = $this->cellStart[0];
        for ($cell = 0; $cell < $this->cells; $cell++) {
            $this->cellStart[$cell] = $runEnds[$cell];
        }
        for ($box = 0; $box < $count; $box++) {
            $target = $boxCells[$box];
            $this->cellStart[$target]--;
            $this->entries[$this->cellStart[$target]] = $box;
        }
        return $counted | $this->entries[0] | $this->cellStart[1];
    }
    public function release(): void {
        $cellStart = $this->cellStart;
        buffer_free($cellStart);
        $entries = $this->entries;
        buffer_free($entries);
    }
}
function histogram(buffer<int> $boxCells, int $count): int {
    $counts = buffer_new<int>(8);
    for ($box = 0; $box < $count; $box++) {
        $counts[$boxCells[$box]]++;
    }
    $peak = 0;
    for ($cell = 0; $cell < 8; $cell++) {
        $peak = $peak | $counts[$cell];
    }
    buffer_free($counts);
    return $peak;
}
$count = $argc > 1 ? 1000 : 16;
$per = $count >> 3;
$boxCells = buffer_new<int>($count);
for ($box = 0; $box < $count; $box++) {
    $boxCells[$box] = $box & 7;
}
$runEnds = buffer_new<int>(8);
for ($cell = 0; $cell < 8; $cell++) {
    $runEnds[$cell] = ($cell + 1) * $per;
}
$grid = new CountingGrid(8, $count);
echo $grid->fill($boxCells, $runEnds, $count), ' ', histogram($boxCells, $count), "\n";
$grid->release();
buffer_free($boxCells);
buffer_free($runEnds);
"#;

/// Every loop counter lives in a buffer as well, so no visit boxes a value in either IR mode.
const TALLY: &str = r#"<?php
final class Tally {
    public buffer<int> $counts;
    public buffer<f32> $weights;
    public function __construct(int $cells) {
        $this->counts = buffer_new<int>($cells);
        $this->weights = buffer_new<f32>($cells);
    }
    public function add(int $cell): void {
        $this->counts[$cell]++;
        ++$this->weights[$cell];
    }
    public function take(int $cell): int {
        return $this->counts[$cell]--;
    }
    public function release(): void {
        $counts = $this->counts;
        buffer_free($counts);
        $weights = $this->weights;
        buffer_free($weights);
    }
}
function visit(Tally $tally, int $visits): int {
    $at = buffer_new<int>(1);
    $local = buffer_new<int>(8);
    $seen = buffer_new<int>(1);
    for ($at[0] = 0; $at[0] < $visits; $at[0]++) {
        $cell = $at[0] & 7;
        $tally->add($cell);
        --$local[$cell];
    }
    while ($at[0] > 0) {
        $at[0]--;
        $cell = $at[0] & 7;
        $seen[0] = $seen[0] | $tally->take($cell);
        $local[$cell]++;
    }
    $result = $seen[0] | $local[3] | $tally->counts[5];
    buffer_free($at);
    buffer_free($local);
    buffer_free($seen);
    return $result;
}
$tally = new Tally(8);
echo visit($tally, $argc > 1 ? 1000 : 16), ' ', (int) $tally->weights[0], "\n";
$tally->release();
"#;

/// Reads the allocation count of a run and requires that every allocation was freed.
fn balanced_allocations(stderr: &str) -> u64 {
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    let (allocs, frees) = parse_gc_stats(stderr);
    assert_eq!(allocs, frees, "{stderr}");
    allocs
}

/// The grid's property and local updates allocate nothing per visit once loop counters are scalar.
#[test]
fn test_buffer_element_incdec_grid_allocations_stay_flat() {
    let runs = run_variants(GRID, true, &[&[], &["many"]]);
    for (success, _, stderr) in &runs {
        assert!(*success, "{stderr}");
    }
    assert_eq!(runs[0].1, "10 2\n");
    assert_eq!(runs[1].1, "1021 125\n");
    let small = balanced_allocations(&runs[0].2);
    let large = balanced_allocations(&runs[1].2);
    assert!(large.abs_diff(small) <= 8,
        "buffer element updates must not allocate per visit: small={small}, large={large}");
    let unoptimized = run_variants(GRID, false, &[&[], &["many"]]);
    for ((success, stdout, stderr), expected) in unoptimized.iter().zip(["10 2\n", "1021 125\n"]) {
        assert!(*success, "{stderr}");
        assert_eq!(stdout.as_str(), expected);
        balanced_allocations(stderr);
    }
}

/// With buffer-held loop counters, updates stay allocation-free in both IR modes.
#[test]
fn test_buffer_element_incdec_allocations_stay_flat_without_ir_opt() {
    for optimized in [false, true] {
        let runs = run_variants(TALLY, optimized, &[&[], &["many"]]);
        for (success, _, stderr) in &runs {
            assert!(*success, "ir-opt={optimized}: {stderr}");
        }
        assert_eq!(runs[0].1, "3 2\n", "ir-opt={optimized}");
        assert_eq!(runs[1].1, "127 125\n", "ir-opt={optimized}");
        let small = balanced_allocations(&runs[0].2);
        let large = balanced_allocations(&runs[1].2);
        assert!(large.abs_diff(small) <= 8,
            "ir-opt={optimized}: updates must not allocate per visit: small={small}, large={large}");
    }
}

/// Buffer updates stage no boxed value in either IR mode, and their loops can be hoisted.
///
/// The tally keeps every counter in a buffer, so any boxed or checked `mixed` instruction in its
/// functions would come from an element update. The grid's int loop counters are boxed without
/// the optimizer, so there it only checks the typed steps and the optimized handle hoisting.
#[test]
fn test_buffer_element_incdec_eir_has_no_mixed_staging() {
    for optimized in [false, true] {
        let ir = emit_ir(TALLY, optimized);
        for name in ["Tally::add", "Tally::take", "visit"] {
            let body = function_body(&ir, name);
            for staged in [
                "= str_inc_dec ", "= mixed_clone ", "= mixed_box ", "= ichecked_add ", "= ichecked_sub ",
            ] {
                assert!(!body.contains(staged), "ir-opt={optimized} {name}: {staged}\n{body}");
            }
        }
        if !optimized {
            for (name, steps) in [
                ("Tally::add", &["= ichecked_add_to_int ", "= fadd "][..]),
                ("Tally::take", &["= ichecked_sub_to_int "][..]),
                ("visit", &["= ichecked_add_to_int ", "= ichecked_sub_to_int "][..]),
            ] {
                let body = function_body(&ir, name);
                for step in steps {
                    assert!(body.contains(*step), "{name}: {step}\n{body}");
                }
            }
            let grid = emit_ir(GRID, false);
            assert!(function_body(&grid, "CountingGrid::fill").contains("= ichecked_sub_to_int "));
            assert!(function_body(&grid, "histogram").contains("= ichecked_add_to_int "));
        }
    }
    // The boxed staging allocated, which kept `buffer_access` from hoisting any handle of the
    // loop. With a typed update the loop qualifies, so the parameter's handle is resolved once;
    // `$counts` itself stays per-access because `address_escaping_slots` counts its later
    // `buffer_free` call as an escaping use of the slot.
    let optimized = emit_ir(GRID, true);
    let histogram = function_body(&optimized, "histogram");
    for staged in ["= str_inc_dec ", "= mixed_clone ", "= mixed_box "] {
        assert!(!histogram.contains(staged), "optimized histogram: {staged}\n{histogram}");
    }
    assert!(histogram.contains("= buffer_resolve ") && histogram.contains("= buffer_get_resolved "),
        "a typed update leaves the counting loop eligible for handle hoisting\n{histogram}");
}

/// Statement and expression updates of local and property buffers match the same PHP array code.
#[test]
fn test_buffer_element_incdec_values_match_array_updates() {
    let source = r#"<?php
class BufferCells {
    public buffer<int> $cells;
    public function __construct(int $length) { $this->cells = buffer_new<int>($length); }
}
class ArrayCells {
    public array $cells = [0, 0, 0];
}
function bufferTrace(int $seed): string {
    $local = buffer_new<int>(3);
    $owner = new BufferCells(3);
    $local[0] = $seed;
    $owner->cells[0] = $seed;
    $local[0]++; ++$local[0]; $local[0]--; --$local[0]; $local[0]++;
    $owner->cells[0]++; ++$owner->cells[0]; $owner->cells[0]--;
    $trace = $local[0] . ',' . $owner->cells[0] . ';';
    $trace .= $local[1]++ . ',' . ++$local[1] . ',' . $local[1]-- . ',' . --$local[1] . ',' . $local[1] . ';';
    $trace .= $owner->cells[2]++ . ',' . ++$owner->cells[2] . ',' . $owner->cells[2]-- . ','
        . --$owner->cells[2] . ',' . $owner->cells[2];
    buffer_free($local);
    $cells = $owner->cells;
    buffer_free($cells);
    return $trace;
}
function arrayTrace(int $seed): string {
    $local = [0, 0, 0];
    $owner = new ArrayCells();
    $local[0] = $seed;
    $owner->cells[0] = $seed;
    $local[0]++; ++$local[0]; $local[0]--; --$local[0]; $local[0]++;
    $owner->cells[0]++; ++$owner->cells[0]; $owner->cells[0]--;
    $trace = $local[0] . ',' . $owner->cells[0] . ';';
    $trace .= $local[1]++ . ',' . ++$local[1] . ',' . $local[1]-- . ',' . --$local[1] . ',' . $local[1] . ';';
    $trace .= $owner->cells[2]++ . ',' . ++$owner->cells[2] . ',' . $owner->cells[2]-- . ','
        . --$owner->cells[2] . ',' . $owner->cells[2];
    return $trace;
}
echo bufferTrace($argc), "\n", arrayTrace($argc), "\n";
$echoed = buffer_new<int>(1);
echo ++$echoed[0], ' ', ++$echoed[0], ' ', $echoed[0]--, ' ', $echoed[0], "\n";
buffer_free($echoed);
$listed = [0];
echo ++$listed[0], ' ', ++$listed[0], ' ', $listed[0]--, ' ', $listed[0], "\n";
"#;
    let expected = "2,2;0,2,2,0,0;0,2,2,0,0\n2,2;0,2,2,0,0;0,2,2,0,0\n1 2 2 1\n1 2 2 1\n";
    for optimized in [false, true] {
        assert_eq!(run_clean(source, optimized), expected, "ir-opt={optimized}");
    }
}

/// An overflowing int element stores what a direct float write stores; the prefix value is PHP's.
#[test]
fn test_buffer_element_incdec_overflow_matches_a_float_write() {
    let source = r#"<?php
class WideCells {
    public buffer<int> $cells;
    public function __construct() { $this->cells = buffer_new<int>(2); }
}
$one = $argc;
$b = buffer_new<int>(6);
$b[0] = PHP_INT_MAX; $b[0]++;
$b[1] = PHP_INT_MIN; --$b[1];
$b[2] = PHP_INT_MAX; $old = $b[2]++;
$b[3] = PHP_INT_MAX; $new = ++$b[3];
$b[4] = PHP_INT_MAX + $one;
$b[5] = PHP_INT_MIN - $one;
echo $b[0], '|', $b[1], '|', $old, '|', $b[2], '|', is_float($new) ? 'float' : 'int', '|',
    $new === (float) PHP_INT_MAX ? 'max' : 'other', '|', $b[3], '|', $b[4], '|', $b[5], "\n";
$w = new WideCells();
$w->cells[0] = PHP_INT_MAX; $w->cells[0]++;
$w->cells[1] = PHP_INT_MIN; $before = $w->cells[1]--;
echo $w->cells[0], '|', $before, '|', $w->cells[1], "\n";
buffer_free($b);
$cells = $w->cells;
buffer_free($cells);
"#;
    let min = "-9223372036854775808";
    let expected = format!(
        "{min}|{min}|9223372036854775807|{min}|float|max|{min}|{min}|{min}\n{min}|{min}|{min}\n"
    );
    for optimized in [false, true] {
        assert_eq!(run_clean(source, optimized), expected, "ir-opt={optimized}");
    }
}

/// Narrow integer elements keep their low bits, and `f32` rounds the stored step, not the value.
#[test]
fn test_buffer_element_incdec_narrow_and_float_widths() {
    let source = r#"<?php
class Pixels {
    public buffer<u8> $bytes;
    public function __construct() { $this->bytes = buffer_new<u8>(2); }
}
$u8 = buffer_new<u8>(3);
$i8 = buffer_new<i8>(1);
$u16 = buffer_new<u16>(1);
$i32 = buffer_new<i32>(1);
$u32 = buffer_new<u32>(1);
$u8[0] = 255; $u8[0]++;
$i8[0] = 127; ++$i8[0];
$u16[0]--;
$i32[0] = 2147483647; $i32[0]++;
--$u32[0];
echo $u8[0], ' ', $i8[0], ' ', $u16[0], ' ', $i32[0], ' ', $u32[0], "\n";
$u8[1] = 255;
$u8[2] = 255;
$old = $u8[1]++;
$new = ++$u8[2];
echo $old, ' ', $u8[1], ' ', $new, ' ', $u8[2], "\n";
$p = new Pixels();
$p->bytes[0] = 255; $p->bytes[0]++; --$p->bytes[1];
echo $p->bytes[0], ' ', $p->bytes[1], "\n";
$f32 = buffer_new<f32>(3);
$f32[0] = 16777216.0; $f32[0]++;
$f32[1] = 16777216.0;
$sum = ++$f32[1];
$f32[2] = 2.5; $f32[2]--; --$f32[2];
echo $f32[0], ' ', $sum, ' ', $f32[1], ' ', $f32[2], "\n";
$f = buffer_new<float>(2);
$f[0] = 0.5; $f[0]++; $f[0]++; --$f[0];
$before = $f[1]--;
echo $f[0], ' ', $before, ' ', $f[1], "\n";
$bytes = $p->bytes;
buffer_free($bytes);
buffer_free($u8);
buffer_free($i8);
buffer_free($u16);
buffer_free($i32);
buffer_free($u32);
buffer_free($f32);
buffer_free($f);
"#;
    let expected = "0 -128 65535 -2147483648 4294967295\n255 0 256 0\n0 255\n\
16777216 16777217 16777216 0.5\n1.5 0 -1\n";
    for optimized in [false, true] {
        assert_eq!(run_clean(source, optimized), expected, "ir-opt={optimized}");
    }
}

/// An update past the end or through a freed alias still stops at the update.
#[test]
fn test_buffer_element_incdec_keeps_bounds_and_stale_handle_fatals() {
    let cases = [
        (
            "<?php $b = buffer_new<int>(2); echo 'before'; $b[2]++; echo 'after';",
            "buffer index out of bounds",
        ),
        (
            "<?php $b = buffer_new<float>(2); echo 'before'; echo $b[$argc + 1]--; echo 'after';",
            "buffer index out of bounds",
        ),
        (
            r#"<?php
class Cells { public buffer<int> $cells; public function __construct() { $this->cells = buffer_new<int>(1); } }
$owner = new Cells();
echo 'before';
--$owner->cells[$argc];
echo 'after';
"#,
            "buffer index out of bounds",
        ),
        (
            "<?php $owner = buffer_new<int>(2); $alias = $owner; buffer_free($owner); echo 'before'; $alias[0]--; echo 'after';",
            "use of buffer after buffer_free()",
        ),
    ];
    for (source, fatal) in cases {
        for optimized in [false, true] {
            let (success, stdout, stderr) = run_variants(source, optimized, &[&[]]).remove(0);
            assert!(!success, "ir-opt={optimized}: {source}");
            assert_eq!(stdout, "before", "ir-opt={optimized}: {source}");
            assert!(stderr.contains(fatal), "ir-opt={optimized}: {stderr}");
        }
    }
}

/// The counting-sort example prints the same counts, order and `f32` rounding in both IR modes.
#[test]
fn test_buffer_counting_sort_example_runs_in_both_ir_modes() {
    let source = include_str!("../../../examples/buffer-counting-sort/main.php");
    let expected = "cell counts: 2 1 3 2; sorted items: 1 5 4 0 3 6 2 7\nf32 after ++: 16777216\n";
    for optimized in [false, true] {
        assert_eq!(run_clean(source, optimized), expected, "ir-opt={optimized}");
    }
}

/// A get hook that rebinds the receiver during the element read.
///
/// The receiver is a reference to a typed holder property, so it stays a `HookedCells` (a closure
/// assigning a by-reference capture would widen it to `mixed` and take the generic path), and the
/// hook, armed once by a flag, replaces the holder's property (the static callback is never reset
/// to null, which a `callable` static cannot store). An expression reads the element where the
/// parser's prelude reads it, before the write loads its receiver, so the write lands on the
/// receiver the hook bound; a statement loads its write receiver before the value, so it keeps
/// the original. Both orders are the generic paths'. The EIR check keeps the fixture typed.
#[test]
fn test_buffer_element_incdec_receiver_rebound_by_a_get_hook() {
    let source = r#"<?php
class HookedCells {
    public static bool $armed = false;
    public static $onRead = null;
    public buffer<int> $cells {
        get {
            if (self::$armed) {
                self::$armed = false;
                $swap = self::$onRead;
                $swap();
            }
            return $this->cells;
        }
        set {
            $this->cells = $value;
        }
    }
    public function __construct(int $seed) {
        $this->cells = buffer_new<int>(1);
        $this->cells[0] = $seed;
    }
    public function release(): void {
        $cells = $this->cells;
        buffer_free($cells);
    }
}
class Holder {
    public HookedCells $current;
    public function __construct(HookedCells $current) { $this->current = $current; }
}
function expressionAfterRebind(HookedCells $first, HookedCells $spare): string {
    $holder = new Holder($first);
    $owner = &$holder->current;
    HookedCells::$onRead = function () use ($holder, $spare) { $holder->current = $spare; };
    HookedCells::$armed = true;
    $old = $owner->cells[0]++;
    return $old . ' ' . $first->cells[0] . ' ' . $spare->cells[0];
}
function statementAfterRebind(HookedCells $first, HookedCells $spare): string {
    $holder = new Holder($first);
    $owner = &$holder->current;
    HookedCells::$onRead = function () use ($holder, $spare) { $holder->current = $spare; };
    HookedCells::$armed = true;
    $owner->cells[0]++;
    return $first->cells[0] . ' ' . $spare->cells[0];
}
$first = new HookedCells(10);
$spare = new HookedCells(100);
echo expressionAfterRebind($first, $spare), "\n";
$first->release();
$spare->release();
$first = new HookedCells(10);
$spare = new HookedCells(100);
echo statementAfterRebind($first, $spare), "\n";
$first->release();
$spare->release();
"#;
    let ir = emit_ir(source, false);
    for name in ["expressionAfterRebind", "statementAfterRebind"] {
        let body = function_body(&ir, name);
        assert!(body.contains("= ichecked_add_to_int ") && !body.contains("= str_inc_dec "),
            "{name} must take the typed update path\n{body}");
    }
    for optimized in [false, true] {
        assert_eq!(run_clean(source, optimized), "10 10 11\n11 100\n", "ir-opt={optimized}");
    }
}
