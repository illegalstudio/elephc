//! Purpose:
//! Executes guarded counters and cast-only index locals at PHP integer boundaries.
//!
//! Called from:
//! - The focused optimizer codegen integration suite.
//!
//! Key details:
//! - Both IR modes use strict heap validation and a real PHP oracle when requested.

use super::*;

/// Owns the fixture directory even if a compiler or runtime assertion fails.
struct Fixture(std::path::PathBuf);
impl Drop for Fixture {
    /// Removes only this test's explicitly created directory.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Runs one fixture in both IR modes, checking heap cleanup and optional PHP equivalence.
pub(super) fn equivalent(source: &str) {
    let fixture = Fixture(make_cli_test_dir("elephc_counter_fast_paths"));
    let path = fixture.0.join("main.php");
    fs::write(&path, source).unwrap();
    let mut previous: Option<Vec<u8>> = None;
    for optimized in [false, true] {
        let mut command = elephc_cli_command(&fixture.0);
        command.args(["--heap-debug", "--gc-stats"]);
        if !optimized { command.arg("--no-ir-opt"); }
        let compile = command.arg(&path).output().unwrap();
        assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
        let output = run_binary(&path.with_extension(""), &fixture.0);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"), "optimized={optimized}: {}", String::from_utf8_lossy(&output.stderr));
        if let Some(previous) = &previous { assert_eq!(&output.stdout, previous, "IR modes disagree"); }
        if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
            let php = Command::new("php").args(["-n", "-d", "display_errors=stderr"]).arg(&path).output().unwrap();
            assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
            assert_eq!(output.stdout, php.stdout, "PHP oracle");
        }
        previous = Some(output.stdout);
    }
}

/// Guards must retain int/float rounding, overflowing updates, zero-trip seeds and live exits.
#[test]
fn test_counter_fast_paths_boundaries_match_php() {
    equivalent(r#"<?php
function up(int $start, mixed $end): string {
    $steps = 0;
    for ($i = $start; $i <= $end; $i++) {
        $steps++;
        if ($steps === 4) { break; }
    }
    return $steps . ':' . gettype($i) . ':' . $i . '|';
}
function down(int $start, mixed $end): string {
    $steps = 0;
    for ($i = $start; $i >= $end; $i--) {
        $steps++;
        if ($steps === 4) { break; }
    }
    return $steps . ':' . gettype($i) . ':' . $i . '|';
}
function deadSeed(int $n): int {
    $steps = 0;
    for ($box = $n - 1; $box >= 0; $box--) { $steps++; }
    return $steps;
}
function boundEffect(): int { echo 'bound|'; return 0; }
function seedEffect(int $n): void {
    for ($box = $n - 1; $box >= boundEffect(); $box--) {}
    echo gettype($box), '|';
}
echo up(PHP_INT_MAX - 1, PHP_INT_MAX - 1), up(PHP_INT_MAX - 1, PHP_INT_MAX);
echo up(PHP_INT_MAX - 1, (float)PHP_INT_MAX), up(0, 2.5), up(2, 1);
echo down(PHP_INT_MIN + 1, PHP_INT_MIN + 1), down(PHP_INT_MIN + 1, PHP_INT_MIN);
echo down(PHP_INT_MIN + 1, (float)PHP_INT_MIN), down(2, 0.5);
echo deadSeed(PHP_INT_MIN), '|', deadSeed($argc + 2), '|';
seedEffect(PHP_INT_MIN);
"#);
}

/// Disjoint nested bindings remain independent while live and reference-visible names stay shared.
#[test]
fn test_counter_fast_paths_nested_bindings_and_aliases_match_php() {
    equivalent(r#"<?php
function nested(int $n): int {
    $sum = 0;
    for ($outer = 0; $outer < $n; $outer++) {
        for ($row = $n - 1; $row >= 0; $row--) { $sum += 1; }
        for ($row = 0; $row < $n; $row++) { $sum += 1; }
    }
    return $sum;
}
function live(int $n): string {
    $out = '';
    for ($outer = 0; $outer < $n; $outer++) {
        for ($row = $n - 1; $row >= 0; $row--) {}
        $out .= $row . ',';
        for ($row = 0; $row < $n; $row++) {}
        $out .= $row . ',';
    }
    return $out;
}
function alias(int $n): int {
    $row = 0;
    $ref =& $row;
    for ($outer = 0; $outer < $n; $outer++) {
        for ($row = $n - 1; $row >= 0; $row--) {}
        for ($row = 0; $row < $n; $row++) {}
    }
    return $ref;
}
function sharedDeclarations(mixed $end): int {
    $sum = 0;
    for ($i = 0; $i <= $end; $i++) {
        static $visits = 0;
        $visits++;
        $value = static fn(): int => 7;
        $sum += $value() + $visits;
    }
    return $sum;
}
echo nested($argc + 2), '|', live($argc + 1), '|', alias($argc + 2), '|';
echo sharedDeclarations($argc + 2), '|', sharedDeclarations(3.5), "\n";
"#);
}

/// A cast-only local preserves float promotion through arithmetic that follows its load.
#[test]
fn test_counter_fast_paths_index_locals_preserve_float_suffixes() {
    equivalent(r#"<?php
function indexValue(int $row, int $columns, int $column): void {
    $cell = $row * $columns + $column;
    echo (int)$cell, ':', (int)($cell + 1), ':', (int)($cell - 7), '|';
}
indexValue($argc + 1, 31, 4);
indexValue(PHP_INT_MAX, 2, 1);
indexValue(PHP_INT_MIN, 2, -1);
indexValue(9007199254740993, 1024, 1);
function visibleIndexAfter(int $end, int $column): int {
    $cell = 7;
    for ($row = 0; $row <= $end; $row++) {
        $cell = $row * 3 + $column;
        echo (int)$cell, ',';
    }
    return (int)$cell;
}
echo visibleIndexAfter($argc - 2, 4), '|', visibleIndexAfter($argc + 1, 4), "\n";
function guardedIndexOutput(mixed $end, int $columns): void {
    for ($row = 0; $row <= $end; $row++) {
        $cell = $row * $columns + 7;
        echo (int)($cell + 1), ',';
    }
    echo '|';
}
guardedIndexOutput($argc + 1, PHP_INT_MAX);
guardedIndexOutput(2.5, PHP_INT_MAX);
"#);
}

/// Existing emitters lower the new guarded CFG and fused arithmetic on all supported targets.
#[test]
fn test_counter_fast_paths_emit_for_every_supported_target() {
    let fixture = Fixture(make_cli_test_dir("elephc_counter_fast_paths_targets"));
    let path = fixture.0.join("main.php");
    fs::write(&path, r#"<?php
function targetCounter(int $n): int {
    $sum = 0;
    for ($i = 0; $i <= $n; $i++) { $sum += 1; }
    return $sum;
}
function targetIndex(int $row, int $columns, int $column): int {
    $cell = $row * $columns + $column;
    return (int)($cell + 1);
}
function targetSeed(int $n): int {
    $count = 0;
    for ($i = $n - 1; $i >= 0; $i--) { $count += 1; }
    return $count;
}
echo targetCounter($argc + 2), targetIndex($argc, 31, 7), targetSeed($argc + 2);
"#).unwrap();
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut command = elephc_cli_command(&fixture.0);
        command.args(["--emit-asm", "--target", target]);
        if target.starts_with("ios-") { command.args(["--emit", "staticlib"]); }
        let output = command.arg(&path).output().unwrap();
        assert!(output.status.success(), "{target}: {}", String::from_utf8_lossy(&output.stderr));
        let assembly = fs::read_to_string(path.with_extension("s")).unwrap();
        assert!(assembly.contains("counter.fast") && assembly.contains("op=iadd"), "{target}: missing integer version");
        assert!(assembly.contains("op=ichecked_numeric_chain_to_int") && assembly.contains("op=isub"), "{target}: missing index or seed specialization");
    }
}

/// Integer versions keep numeric allocation counts independent of the trip count.
#[test]
fn test_counter_fast_paths_inclusive_allocations_stay_bounded() {
    let mut counts = Vec::new();
    for trips in [10, 1000] {
        let fixture = Fixture(make_cli_test_dir("elephc_counter_fast_paths_allocations"));
        let path = fixture.0.join("main.php");
        fs::write(&path, format!(r#"<?php
function allocationCounter(int $end): int {{
    $count = 0;
    for ($i = 0; $i <= $end; $i++) {{ $count += 1; }}
    return $count;
}}
function allocationIndex(int $end, int $columns): int {{
    $sum = 0;
    for ($row = 0; $row <= $end; $row++) {{
        $cell = $row * $columns + 1;
        $sum += (int)($cell + 1);
    }}
    return $sum;
}}
echo allocationCounter($argc * {trips}), ':', allocationIndex($argc * {trips}, 31);
"#)).unwrap();
        let compile = elephc_cli_command(&fixture.0).args(["--heap-debug", "--gc-stats"]).arg(&path).output().unwrap();
        assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
        let output = run_binary(&path.with_extension(""), &fixture.0);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success() && stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
        assert_eq!(String::from_utf8(output.stdout).unwrap(),
            format!("{}:{}", trips + 1, 31 * trips * (trips + 1) / 2 + 2 * (trips + 1)));
        let (allocations, frees) = parse_gc_stats(&stderr);
        assert_eq!(allocations, frees, "{stderr}");
        println!("counter/index trips={trips}: allocations={allocations}, frees={frees}");
        counts.push(allocations);
    }
    assert!(counts[0].abs_diff(counts[1]) <= 8, "allocation count grows with iterations: {counts:?}");
}

/// Pre-loop and post-update tests cannot prove every update safe, nor can an outer test bound inner updates.
#[test]
fn test_counter_fast_paths_noncanonical_overflow_matches_php() {
    equivalent(r#"<?php
function postTestOverflow(int $start, int $end): mixed {
    $i = $start;
    do { $i++; } while ($i < $end);
    return $i;
}
$value = postTestOverflow(PHP_INT_MAX - 1 + $argc, PHP_INT_MIN);
echo gettype($value), ':', $value, '|';
function invariantOverflow(int $n, int $start): void {
    $i = $start + 0 * $n;
    $c = $i < $n;
    $k = 0;
    while ($c) { $i++; $k++; if ($k >= 3) { break; } }
    echo gettype($i), ':', $i, '|';
}
invariantOverflow(PHP_INT_MAX, PHP_INT_MAX - 2 + $argc);
function nestedUpdateOverflow(int $n, int $lo): int {
    $count = 0;
    for ($i = $n - 1; $i > $lo; $count++) {
        for ($k = 0; $k < 2; $k++) { $i--; }
        if ($count > 2) { break; }
    }
    return $count;
}
echo nestedUpdateOverflow(PHP_INT_MIN + 1 + $argc, PHP_INT_MIN), "\n";
"#);
}

/// Value-only consumers preserve representation and ownership in fast, fallback and seed-guard bodies.
#[test]
fn test_counter_fast_paths_consumers_match_php() {
    equivalent(r#"<?php
function consumers(int $s, mixed $e, int $m): string {
    $out = '';
    $arr = [];
    $hits = 0; $steps = 0;
    for ($i = $s; $i <= $e; $i++) {
        echo $i, ',';
        $out = $out . 'x' . $i;
        $arr[] = $i;
        if ($i == $m) { $hits++; }
        if ($i === $m) { $hits += 10; }
        $hits += ($i <=> $m);
        if ($i) { $hits += 100; }
        echo (float)($i / 2), ';';
        $steps++; if ($steps === 5) { break; }
    }
    return $out . '|' . count($arr) . '|' . $hits . '|' . $i;
}
echo consumers($argc, $argc + 3, $argc + 1), "\n";
echo consumers($argc, 2.5, 2), "\n";
echo consumers(PHP_INT_MAX - 1, PHP_INT_MAX, PHP_INT_MAX), "\n";
function seedConsumers(int $n, int $m): int {
    $hits = 0;
    for ($box = $n - 1; $box >= 0; $box--) {
        echo $box, ':';
        if ($box == $m) { $hits++; }
        $hits += ($box <=> $m);
    }
    return $hits;
}
echo seedConsumers($argc + 3, 2), '|', seedConsumers(PHP_INT_MIN, 0), "\n";
"#);
}

/// Guarded scalarization must preserve conversions required by typed static-property stores.
#[test]
fn test_counter_fast_paths_coercing_static_stores_match_php() {
    equivalent(r#"<?php
class CounterStore {
    public static float $number = 0.0;
    public static string $text = '';
    public static bool $flag = false;
}
function counterStoreBound(mixed $end): int {
    $sum = 0;
    for ($i = 0; $i < $end; $i++) {
        CounterStore::$number = $i;
        CounterStore::$text = $i;
        CounterStore::$flag = $i;
        $sum++;
    }
    return $sum;
}
function counterStoreSeed(int $n): int {
    $sum = 0;
    for ($box = $n - 1; $box >= 0; $box--) {
        CounterStore::$number = $box;
        CounterStore::$text = $box;
        CounterStore::$flag = $box;
        $sum++;
    }
    return $sum;
}
echo counterStoreBound($argc + 2), '|';
var_dump(CounterStore::$number, CounterStore::$text, CounterStore::$flag);
echo counterStoreBound(2.5), '|';
var_dump(CounterStore::$number, CounterStore::$text, CounterStore::$flag);
echo counterStoreSeed($argc + 3), '|';
var_dump(CounterStore::$number, CounterStore::$text, CounterStore::$flag);
echo counterStoreSeed(PHP_INT_MIN), '|';
var_dump(CounterStore::$number, CounterStore::$text, CounterStore::$flag);
"#);
}

/// Array casts still receive boxed counters in versioned and directional-seed loop shapes.
#[test]
fn test_counter_fast_paths_array_casts_preserve_boxed_inputs() {
    equivalent(r#"<?php
function counterArrayBound(int $start, mixed $end): int {
    $hits = 0;
    for ($i = $start; $i <= $end; $i++) { $a = (array)$i; $hits += $a[0]; }
    return $hits;
}
function counterArraySeed(int $n): int {
    $hits = 0;
    for ($box = $n - 1; $box >= 0; $box--) { $a = (array)$box; $hits += $a[0]; }
    return $hits;
}
echo counterArrayBound($argc, $argc + 3), '|', counterArrayBound($argc, 3.5), '|';
echo counterArraySeed($argc + 3), '|', counterArraySeed(PHP_INT_MIN), "\n";
"#);
}

/// Spaceship preserves PHP boolean/null coercion for negative, zero and positive counter values.
#[test]
fn test_counter_fast_paths_spaceship_bool_null_match_php() {
    equivalent(r#"<?php
function counterCompareBound(int $start, mixed $end): void {
    $truth = 0; $nil = 0;
    for ($i = $start; $i <= $end; $i++) { $truth += ($i <=> true); $nil += ($i <=> null); }
    echo $truth, ':', $nil, '|';
}
function counterCompareSeed(int $n, int $lo): void {
    $truth = 0; $nil = 0;
    for ($box = $n - 1; $box > $lo; $box--) { $truth += ($box <=> true); $nil += ($box <=> null); }
    echo $truth, ':', $nil, '|';
}
counterCompareBound(-2 + $argc, $argc + 3);
counterCompareBound(-2 + $argc, 3.5);
counterCompareSeed($argc + 3, -2);
counterCompareSeed(PHP_INT_MIN, -2);
"#);
}

/// Raw pointer partners keep strict identity consistent between fast, fallback and seed paths.
#[test]
fn test_counter_fast_paths_pointer_identity_keeps_mixed_semantics() {
    let source = r#"<?php
function pointerCounterBound(int $start, mixed $end): void {
    $p = ptr_null(); $equal = 0; $different = 0;
    for ($i = $start; $i <= $end; $i++) {
        if ($i === $p) { $equal++; }
        if ($i !== $p) { $different++; }
    }
    echo $equal, ':', $different, '|';
}
function pointerCounterSeed(int $n): void {
    $p = ptr_null(); $equal = 0; $different = 0;
    for ($box = $n - 1; $box >= 0; $box--) {
        if ($box === $p) { $equal++; }
        if ($box !== $p) { $different++; }
    }
    echo $equal, ':', $different, '|';
}
pointerCounterBound($argc - 1, $argc + 1);
pointerCounterBound($argc - 1, 2.5);
pointerCounterSeed($argc + 2);
pointerCounterSeed(PHP_INT_MIN);
"#;
    for optimized in [false, true] {
        let output = if optimized { compile_and_run_with_heap_debug_tagged(source) }
            else { without_ir_opt(|| compile_and_run_with_heap_debug_tagged(source)) };
        assert_eq!(output.stdout, "1:2|1:2|1:2|0:0|", "optimized={optimized}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "optimized={optimized}: {}", output.stderr);
    }
}

/// Several dead-seed guards preserve normal, empty and overflowing entry behavior together.
#[test]
fn test_counter_fast_paths_multiple_seed_guards_match_php() {
    let loops: String = (0..10).map(|index| format!(
        "for ($b{index} = $n - 1; $b{index} >= 0; $b{index}--) {{ $count += 1; }}\n"
    )).collect();
    let source = format!("<?php function multipleSeedRuntime(int $n): int {{ $count = 0; {loops} return $count; }}
        echo multipleSeedRuntime(PHP_INT_MIN), '|', multipleSeedRuntime(0), '|',
            multipleSeedRuntime(1), '|', multipleSeedRuntime($argc + 2);");
    equivalent(&source);
}

/// Boxed object partners retain PHP loose equality in bound and directional-seed loops.
#[test]
fn test_counter_fast_paths_loose_object_comparisons_match_php() {
    equivalent(r#"<?php
function looseObjectBound(mixed $end, mixed $other): void {
    $equal = 0; $different = 0;
    for ($i = 0; $i < $end; $i++) {
        if ($i == $other) { $equal++; }
        if ($i != $other) { $different++; }
    }
    echo $equal, ':', $different, '|';
}
function looseObjectSeed(int $n, ?stdClass $other): void {
    $equal = 0; $different = 0;
    for ($box = $n - 1; $box >= 0; $box--) {
        if ($box == $other) { $equal++; }
        if ($box != $other) { $different++; }
    }
    echo $equal, ':', $different, '|';
}
$object = new stdClass;
looseObjectBound($argc + 2, $object); looseObjectBound(2.5, $object);
looseObjectBound(0, $object); looseObjectBound($argc + 2, $argc);
looseObjectSeed($argc + 2, $object); looseObjectSeed(PHP_INT_MIN, $object);
looseObjectSeed($argc + 2, null); echo "\n";
"#);
}
