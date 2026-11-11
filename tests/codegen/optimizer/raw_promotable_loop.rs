//! Purpose:
//! Checks PHP overflow promotion for raw integer accumulator and counter loops.
//!
//! Called from:
//! - `cargo test --test codegen_tests raw_promotable`.
//!
//! Key details:
//! - Runtime `$argc` keeps the arithmetic out of the AST constant folder.
//! - Both IR optimization modes must print the same tags and values.
//! - The optimized assembly branches on signed overflow instead of calling the mixed helper
//!   on the integer path. After promotion the continuation uses a raw float, and the mixed
//!   loop remains for a seed that is not an integer.

use super::*;

/// Assembly comment emitted only by a `cond_br` fused with `IOverflows`.
const FUSED_OVERFLOW_BRANCH: &str = "branch on signed overflow flags";

/// Runs one fixture with heap checks and returns stdout.
fn run(source: &str) -> String {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}", output.stderr);
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        output.stderr
    );
    output.stdout
}

/// Sums `1..=n` and reports the result tag, including a sum that promotes to float.
#[test]
fn test_raw_promotable_sum_keeps_small_and_overflow_results() {
    let source = r#"<?php
function total(int $n): mixed {
    $sum = 0;
    $i = 1;
    while ($i <= $n) {
        $sum += $i;
        $i += 1;
    }
    return $sum;
}
echo total(100), "\n";
echo gettype(total(100)), "\n";
$sum = PHP_INT_MAX - 5;
$i = 1;
while ($i <= 4) {
    $sum += $i;
    $i += 1;
}
echo gettype($sum), "\n";
echo $i;
"#;
    let expected = "5050\ninteger\ndouble\n5";
    assert_eq!(run(source), expected);
    let dir = make_cli_test_dir("elephc_raw_promotable_off");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let compiled = elephc_cli_command(&dir)
        .args(["--no-ir-opt", "--heap-debug"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let output = run_binary(&path.with_extension(""), &dir);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
    fs::remove_dir_all(&dir).unwrap();
}

/// A counter that passes PHP_INT_MAX stays a float on the following steps.
#[test]
fn test_raw_promotable_counter_overflow_stays_float() {
    let source = r#"<?php
$i = PHP_INT_MAX - 1;
$n = 0;
while ($n < 3) {
    $i += 1;
    $n += 1;
}
echo gettype($i), "\n";
echo gettype($n), "\n";
echo $n;
"#;
    assert_eq!(run(source), "double\ninteger\n3");
}

/// Subtraction below PHP_INT_MIN and a product past the integer range promote once.
#[test]
fn test_raw_promotable_sub_and_mul_promote() {
    let source = r#"<?php
$i = PHP_INT_MIN + 1;
$n = 0;
while ($n < 3) {
    $i -= 1;
    $n += 1;
}
echo gettype($i), "\n";
$factor = 3037000499 + $argc;
$steps = 0;
$product = $factor;
while ($steps < 1) {
    $product *= $factor;
    $steps += 1;
}
echo gettype($product);
"#;
    assert_eq!(run(source), "double\ndouble");
}

/// The optimized sum loop branches on overflow and still frees every temporary cell.
#[test]
fn test_raw_promotable_sum_loop_branches_on_overflow() {
    let source = r#"<?php
function total(int $n): mixed {
    $sum = 0;
    $i = 1;
    while ($i <= $n) {
        $sum += $i;
        $i += 1;
    }
    return $sum;
}
echo total(20 + $argc);
"#;
    let (output, asm) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(output.success, "{}", output.stderr);
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        output.stderr
    );
    assert_eq!(output.stdout, "231");
    // The mixed fallback has overflow jumps of its own, so the fused branch is identified
    // by the comment its lowering emits.
    assert!(
        asm.contains(FUSED_OVERFLOW_BRANCH),
        "expected a fused signed overflow branch in the raw loop:\n{asm}"
    );
    if cfg!(target_arch = "x86_64") {
        assert!(asm.contains("jo ") || asm.contains("jno "), "{asm}");
    } else {
        assert!(asm.contains("b.vs") || asm.contains("b.vc"), "{asm}");
    }
}

/// A post-increment read used after its own store keeps the value from before the store.
#[test]
fn test_raw_promotable_post_increment_operand_reads_old_value() {
    let source = r#"<?php
function total(int $n): mixed {
    $sum = 0;
    $i = 1;
    while ($sum < $n) {
        $sum = $sum + $i++;
    }
    return $sum;
}
var_dump(total(5 + $argc));
"#;
    assert_eq!(run(source), "int(6)\n");
}

/// A local reassigned from other locals returns to an integer after one overflowing step.
#[test]
fn test_raw_promotable_reassigned_local_returns_to_integer() {
    let source = r#"<?php
function last(int $k): mixed {
    $x = 0;
    $j = 2 + $k;
    $big = PHP_INT_MAX - 2;
    while ($j > 0) {
        $x = $big + $j;
        $j -= 1;
    }
    return $x;
}
var_dump(last($argc));
"#;
    assert_eq!(run(source), "int(9223372036854775806)\n");
}

/// A sum whose addend overflows on the last iteration is still an integer at the exit.
#[test]
fn test_raw_promotable_exit_before_float_operand_is_read_keeps_integer() {
    let source = r#"<?php
function total(int $k): mixed {
    $a = 0;
    $b = 4611686018427387903 + $k;
    while ($b < PHP_INT_MAX) {
        $a = $a + $b;
        $b = $b * 2;
    }
    return $a;
}
var_dump(total($argc));
function later(int $k): mixed {
    $a = 0;
    $b = 4611686018427387903 + $k;
    $n = 0;
    while ($n < 2) {
        $a = $a + $b;
        $b = $b * 2;
        $n += 1;
    }
    return $a;
}
var_dump(later($argc));
"#;
    assert_eq!(run(source), "int(4611686018427387904)\nfloat(1.3835058055282164E+19)\n");
}

/// Five escaping accumulators exceed the loop copy's size limit and cannot use frame cells.
/// Their lazy companions still avoid per-iteration boxing without copying the loop.
#[test]
fn test_raw_promotable_five_updates_fall_back_to_lazy_accumulators() {
    let source = r#"<?php
function total(int $n): mixed {
    $a = 0;
    $b = 0;
    $c = 0;
    $d = 0;
    $e = 0;
    $i = 0;
    while ($i < $n) {
        $a += $i;
        $b += $i;
        $c += $i;
        $d += $i;
        $e += $i;
        $i += 1;
    }
    return [$a, $b, $c, $d, $e];
}
$totals = total(3 + $argc);
echo $totals[0] + $totals[1] + $totals[2] + $totals[3] + $totals[4];
"#;
    let (output, asm) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "30");
    assert!(
        asm.contains(FUSED_OVERFLOW_BRANCH) && !asm.contains("rawint."),
        "expected lazy accumulators and no loop copy:\n{asm}"
    );
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        output.stderr
    );
}

/// `for` loops whose counter is already a raw integer keep the accumulator raw too.
#[test]
fn test_raw_promotable_for_loops_with_integer_counters() {
    let source = r#"<?php
function up(int $n): mixed {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        $sum += $i;
    }
    return $sum;
}
function down(int $n): mixed {
    $sum = 0;
    for ($i = $n; $i > 0; $i--) {
        $sum += $i;
    }
    return $sum;
}
function fixed(): mixed {
    $sum = 0;
    for ($i = 0; $i < 1000; $i++) {
        $sum += $i;
    }
    return $sum;
}
function near_max(int $n): mixed {
    $sum = PHP_INT_MAX - 3;
    for ($i = 0; $i < $n; $i++) {
        $sum += $i;
    }
    return $sum;
}
function chained(int $n): mixed {
    $a = 0;
    $b = PHP_INT_MAX - 2;
    for ($i = 0; $i < $n; $i++) {
        $a += $i;
        $b += $a;
    }
    return $a;
}
var_dump(up(4 + $argc), down(4 + $argc), fixed(), near_max(2 + $argc), near_max(4 + $argc), chained(4 + $argc));
"#;
    assert_eq!(run(source), "int(10)\nint(15)\nint(499500)\nint(9223372036854775807)\nfloat(9.223372036854776E+18)\nint(10)\n");
}

/// An overflow-checked temporary such as `$i * $i` promotes the accumulator only when it overflows.
#[test]
fn test_raw_promotable_checked_temporary_operands() {
    let source = r#"<?php
function squares(int $n): mixed {
    $s = 0;
    for ($i = 0; $i < $n; $i++) {
        $s += $i * $i;
    }
    return $s;
}
function squares_near_max(int $n): mixed {
    $s = PHP_INT_MAX - 10;
    for ($i = 0; $i < $n; $i++) {
        $s += $i * $i;
    }
    return $s;
}
function big_product(int $n, int $big): mixed {
    $s = 5;
    for ($i = 1; $i < $n; $i++) {
        $s = $big * $i - $s;
    }
    return $s;
}
function factors(int $n, int $k): mixed {
    $s = 1;
    for ($i = 0; $i < $n; $i++) {
        $s *= $k + $i;
    }
    return $s;
}
var_dump(
    squares(9 + $argc),
    squares_near_max(3 + $argc),
    squares_near_max(5 + $argc),
    big_product(2 + $argc, PHP_INT_MAX - 1),
    big_product(3 + $argc, PHP_INT_MAX - 1),
    factors(5 + $argc, 2),
    factors(3 + $argc, PHP_INT_MAX)
);
"#;
    assert_eq!(run(source), "int(285)\nfloat(9.223372036854776E+18)\nfloat(9.223372036854776E+18)\nfloat(9.223372036854776E+18)\nfloat(1.8446744073709552E+19)\nint(5040)\nfloat(7.237005577332262E+75)\n");
}

/// The optimized `for` sum loop tests overflow with a fused branch, not the mixed helper.
#[test]
fn test_raw_promotable_for_loop_branches_on_overflow() {
    let source = r#"<?php
function total(int $n): mixed {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) {
        $sum += $i * $i;
    }
    return $sum;
}
echo total(4 + $argc);
"#;
    let (output, asm) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(output.success, "{}", output.stderr);
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        output.stderr
    );
    assert_eq!(output.stdout, "30");
    assert!(
        asm.matches(FUSED_OVERFLOW_BRANCH).count() >= 2,
        "expected fused overflow branches for the product and the sum:\n{asm}"
    );
}

/// A counter read after the loop receives its final value from the raw copy.
#[test]
fn test_raw_promotable_counter_used_after_the_loop() {
    let source = r#"<?php
function after(int $n): mixed {
    $s = 0;
    for ($i = 0; $i < $n; $i++) {
        $s += $i;
    }
    return $s + $i;
}
function skipped(int $n): mixed {
    $s = 1;
    for ($i = 7; $i < $n; $i++) {
        $s += $i;
    }
    return $s * $i;
}
var_dump(after(4 + $argc), skipped($argc), skipped(9 + $argc));
"#;
    assert_eq!(run(source), "int(15)\nint(7)\nint(250)\n");
}

/// `==` and `!=` loop tests compare integers, and floats once the counter has promoted.
#[test]
fn test_raw_promotable_loose_equality_conditions() {
    let source = r#"<?php
function until(int $n): mixed {
    $sum = 0;
    for ($i = 0; $i != $n; $i++) {
        $sum += $i;
    }
    return $sum;
}
function to_max(int $k): mixed {
    $c = 0;
    for ($i = PHP_INT_MAX - $k; $i != PHP_INT_MAX; $i++) {
        $c += 2;
    }
    return $c;
}
function past_max(int $k): mixed {
    $c = 0;
    $i = PHP_INT_MAX - $k;
    $n = 0;
    while ($n != 4) {
        $i += 1;
        $n += 1;
        $c += 1;
    }
    return $i;
}
function once(int $n): mixed {
    $s = 0;
    $i = $n;
    while ($i == $n) {
        $s += $i;
        $i += 1;
    }
    return $s;
}
var_dump(until(4 + $argc), until(0 * $argc), to_max(2 + $argc), past_max($argc), once(3 + $argc));
"#;
    assert_eq!(run(source), "int(10)\nint(0)\nint(6)\nfloat(9.223372036854776E+18)\nint(4)\n");
}

/// A bound the header recomputes is tested for overflow on every iteration.
#[test]
fn test_raw_promotable_bounds_recomputed_in_the_header() {
    let source = r#"<?php
function plus_two(int $n): mixed {
    $s = 0;
    $i = 0;
    while ($i < $n + 2) {
        $s += $i;
        $i += 1;
    }
    return $s;
}
function near_max(int $n): mixed {
    $s = 0;
    $i = PHP_INT_MAX - 3;
    while ($i < PHP_INT_MAX - 5 + $n) {
        $s += 1;
        $i += 1;
    }
    return $s;
}
function float_bound_never(int $n): mixed {
    $s = 0;
    $i = 5;
    while ($i > PHP_INT_MAX + $n) {
        $s += 1;
        $i += 1;
    }
    return $s;
}
function float_bound_reached(int $n): mixed {
    $s = 0;
    $i = 5;
    while ($i < PHP_INT_MAX + $n) {
        $s += 1;
        $i += PHP_INT_MAX;
    }
    return $s;
}
var_dump(plus_two(3 + $argc), near_max(3 + $argc), float_bound_never($argc), float_bound_reached($argc));
"#;
    assert_eq!(run(source), "int(15)\nint(2)\nint(0)\nint(1)\n");
}

/// An accumulator later passed to a call uses the raw copy; a real reference keeps the mixed loop.
#[test]
fn test_raw_promotable_accumulators_passed_to_calls_and_references() {
    let source = r#"<?php
function bump(&$x): void {
    $x += 1;
}
function show(int $n): void {
    $s = PHP_INT_MAX - 3;
    for ($i = 0; $i < $n; $i++) {
        $s += $i;
    }
    var_dump($s);
    printf("%d\n", $n);
}
function by_reference(int $n): mixed {
    $s = 0;
    bump($s);
    for ($i = 0; $i < $n; $i++) {
        $s += $i;
    }
    bump($s);
    return $s;
}
function captured(int $n): mixed {
    $s = 0;
    $add = function () use (&$s) {
        $s += 100;
    };
    $add();
    for ($i = 0; $i < $n; $i++) {
        $s += $i;
    }
    $add();
    return $s;
}
show(2 + $argc);
show(4 + $argc);
var_dump(by_reference(4 + $argc), captured(4 + $argc));
"#;
    assert_eq!(run(source), "int(9223372036854775807)\n3\nfloat(9.223372036854776E+18)\n5\nint(12)\nint(210)\n");
}

/// Top-level loops keep their counter in an integer local and still promote exactly.
#[test]
fn test_raw_promotable_top_level_loops() {
    let source = r#"<?php
$a = $argc;
$s = 0;
for ($i = 0; $i < 5 + $a; $i++) {
    $s += $i;
}
var_dump($s, $i);
$t = 0;
$j = 0;
while ($j != 4 + $a) {
    $t += $j;
    $j += 1;
}
var_dump($t, $j);
$n = 5 + $a;
$q = PHP_INT_MAX - 4;
for ($z = 0; $z < $n; $z++) {
    $q += $z;
}
var_dump($q, $z);
$w = 0;
$y = 0;
while ($y < $n) {
    $w += $y * $y;
    $y++;
}
echo $w, "\n";
"#;
    assert_eq!(run(source), "int(15)\nint(6)\nint(10)\nint(5)\nfloat(9.223372036854776E+18)\nint(6)\n55\n");
}

/// A step or bound read from an expression temporary before the loop keeps its value,
/// although lowering unsets the temporary right after the read.
#[test]
fn test_raw_promotable_operands_loaded_before_the_loop_keep_their_value() {
    let source = r#"<?php
$k = $argc > 10 ? 1 : 2;
$s = 0;
for ($i = 0; $i < 4; $i++) {
    $s += $k;
}
echo $s, "\n";
$n = $argc > 10 ? 1 : 4;
$t = 0;
for ($j = 0; $j < $n; $j++) {
    $t += $j;
}
echo $t, "\n";
function stepped(int $a): mixed {
    $step = $a > 10 ? 1 : 3;
    $limit = $a > 10 ? 1 : 5;
    $sum = PHP_INT_MAX - 7;
    for ($i = 0; $i < $limit; $i++) {
        $sum += $step;
    }
    return $sum;
}
var_dump(stepped($argc));
"#;
    assert_eq!(run(source), "8\n6\nfloat(9.223372036854776E+18)\n");
}
