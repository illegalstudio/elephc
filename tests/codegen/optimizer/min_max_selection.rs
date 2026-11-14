//! Purpose:
//! Regression coverage for variadic `min()` / `max()` over runtime-typed operands: the
//! winner keeps its own value and type (task b9f7aae6), and a numeric selection allocates no
//! Mixed cell in a hot loop or behind a checked accumulator (task db21cb34).
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Expected output comes from PHP 8.5; `ELEPHC_PHP_CHECK=1` re-runs the oracle.
//! - Runtime `$argc` keeps the operands out of the AST constant folder, and every fixture
//!   runs in both EIR modes with heap checks.
//! - Allocation windows use `memory_get_allocations()` around the workload only.

use super::*;

/// Cross-checks PHP-compatible fixtures when the focused run opts into the PHP oracle.
pub(super) fn php_oracle(source: &str, expected: &str) {
    if std::env::var_os("ELEPHC_PHP_CHECK").is_none() { return; }
    let output = Command::new("php").args(["-n", "-r", source.trim_start_matches("<?php")])
        .output().expect("run PHP min/max oracle");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

/// Compiles one variant, validates its heap and returns stdout.
pub(super) fn run(source: &str, optimized: bool) -> String {
    let dir = make_cli_test_dir("elephc_min_max_selection");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.arg("--heap-debug");
    if !optimized { command.arg("--no-ir-opt"); }
    let compile = command.arg(&path).output().unwrap();
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let output = run_binary(&path.with_extension(""), &dir);
    fs::remove_dir_all(&dir).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    stdout
}

/// A checked sum that promoted to float, and a fractional `mixed` float, win `max()` as floats.
#[test]
fn test_min_max_selection_keeps_promoted_and_fractional_floats() {
    let source = r#"<?php
function selected(int $a, int $b): mixed { $total = 0; $total += $a; $total += $b; return max($total, 0); }
function mixedMax(mixed $value): mixed { return max($value, 0); }
function mixedMin(mixed $value): mixed { return min($value, 0); }
function sumMin(int $a, int $b): mixed { return min(0, $a + $b); }
var_dump(selected(PHP_INT_MAX, $argc), selected(3, $argc), selected(-9, $argc));
var_dump(mixedMax(5.5), mixedMax(-5.5), mixedMax(7), mixedMax(-7), mixedMax(PHP_INT_MAX + $argc));
var_dump(mixedMin(5.5), mixedMin(-5.5), mixedMin(7), mixedMin(-7), mixedMin(PHP_INT_MIN - $argc));
var_dump(sumMin(PHP_INT_MIN, -$argc), sumMin(3, $argc), sumMin(-3, -$argc));
"#;
    let expected = r#"float(9.223372036854776E+18)
int(4)
int(0)
float(5.5)
int(0)
int(7)
int(0)
float(9.223372036854776E+18)
int(0)
float(-5.5)
int(0)
int(-7)
float(-9.223372036854776E+18)
float(-9.223372036854776E+18)
int(0)
int(-4)
"#;
    php_oracle(source, expected);
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), expected, "optimized={optimized}");
    }
}

/// Ties keep the first argument, except the two-argument `min()`, which returns the second.
#[test]
fn test_min_max_selection_follows_php_tie_and_argument_order() {
    let source = r#"<?php
function mx(mixed $a, mixed $b): mixed { return max($a, $b); }
function mn(mixed $a, mixed $b): mixed { return min($a, $b); }
function mx3(mixed $a, mixed $b, mixed $c): mixed { return max($a, $b, $c); }
function mn3(mixed $a, mixed $b, mixed $c): mixed { return min($a, $b, $c); }
function mixInt(mixed $a, int $b): mixed { return max($a, $b); }
function intMix(int $a, mixed $b): mixed { return min($a, $b); }
function fltMix(float $a, mixed $b): mixed { return max($a, $b); }
var_dump(mx(1, 1.0), mx(1.0, 1), mn(1, 1.0), mn(1.0, 1), mx(-0.0, 0), mn(0, -0.0));
var_dump(mx(-3, -2.5), mx(-2.5, -3), mn(-3, -2.5), mn(-2.5, -3));
var_dump(mx(PHP_INT_MAX, 9.3e18), mn(PHP_INT_MAX, 9.3e18), mx(PHP_INT_MIN, -9.3e18), mn(PHP_INT_MIN, -9.3e18));
var_dump(mx3(1, 1.0, true), mn3(1, 1.0, true), mx3(true, 1, 1.0), mn3(1.0, 1, true), mx3(-1, -1.0, -2), mn3(3, 2.5, "4"));
var_dump(mixInt(5.5, 3), mixInt(2, 3), mixInt(3.0, 3), intMix(4, 3.5), intMix(4, 4.0), fltMix(1.5, 2), fltMix(2.0, 2));
"#;
    let expected = r#"int(1)
float(1)
float(1)
int(1)
float(-0)
float(-0)
float(-2.5)
float(-2.5)
int(-3)
int(-3)
float(9.3E+18)
int(9223372036854775807)
int(-9223372036854775808)
float(-9.3E+18)
int(1)
int(1)
bool(true)
float(1)
int(-1)
float(2.5)
float(5.5)
int(3)
float(3)
float(3.5)
float(4)
int(2)
float(2)
"#;
    php_oracle(source, expected);
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), expected, "optimized={optimized}");
    }
}

/// A two-argument call returns its right operand for `NAN`; a longer call keeps the earlier one.
#[test]
fn test_min_max_selection_handles_nan_like_php() {
    let source = r#"<?php
function mx(mixed $a, mixed $b): mixed { return max($a, $b); }
function mn(mixed $a, mixed $b): mixed { return min($a, $b); }
function mx3(mixed $a, mixed $b, mixed $c): mixed { return max($a, $b, $c); }
function mn3(mixed $a, mixed $b, mixed $c): mixed { return min($a, $b, $c); }
$nan = sqrt(-1.0 * $argc);
var_dump(mx($nan, 1), mx(1, $nan), mn($nan, 1), mn(1, $nan), mx($nan, 1.5), mn(1.5, $nan));
var_dump(mx3($nan, 1, 2), mx3(1, $nan, 2), mx3(1, 2, $nan), mn3($nan, 1, 2), mn3(1, $nan, 2), mn3(2, 1, $nan));
"#;
    let expected = r#"int(1)
float(NAN)
int(1)
float(NAN)
float(1.5)
float(NAN)
float(NAN)
int(2)
int(2)
float(NAN)
int(1)
int(1)
"#;
    php_oracle(source, expected);
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), expected, "optimized={optimized}");
    }
}

/// Strings, bools and null compare through PHP's ordering table and keep their own type.
#[test]
fn test_min_max_selection_compares_non_numeric_operands_like_php() {
    let source = r#"<?php
function mx(mixed $a, mixed $b): mixed { return max($a, $b); }
function mn(mixed $a, mixed $b): mixed { return min($a, $b); }
function strs(string $a, string $b): mixed { return max($a, $b); }
function strInt(string $a, int $b): mixed { return min($a, $b); }
function strFloat(string $a, float $b): mixed { return max($a, $b, 1); }
var_dump(mx("abc", 0), mx(0, "abc"), mn("abc", 0), mn(0, "abc"), mx("10", 9), mn("10", 9), mx("5", 5), mx(5, "5"));
var_dump(mx(null, -5), mn(null, -5), mx(true, 2), mx(2, true), mn(false, 0), mx("apple", "banana"));
var_dump(strs("apple", "banana"), strs("10", "9"), strs("b" . $argc, "a" . $argc), strInt("12", 7), strInt("x", 7), strFloat("2.5", 1.5));
"#;
    let expected = r#"string(3) "abc"
string(3) "abc"
int(0)
int(0)
string(2) "10"
int(9)
string(1) "5"
int(5)
int(-5)
NULL
bool(true)
int(2)
int(0)
string(6) "banana"
string(6) "banana"
string(2) "10"
string(2) "b1"
int(7)
int(7)
string(3) "2.5"
"#;
    php_oracle(source, expected);
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), expected, "optimized={optimized}");
    }
}

/// Owned operands and results of every representation are released exactly once.
#[test]
fn test_min_max_selection_releases_owned_operands_and_results() {
    let source = r#"<?php
function text(int $n): mixed { return str_repeat("z", $n); }
function number(int $n): mixed { return $n * 1.5; }
function items(int $n): array { return [$n, "k" . $n]; }
function run(int $n): void {
    for ($i = 0; $i < 3; $i++) {
        $best = max(text($n + $i), number($i), "m" . $i);
        $least = min(number($i), $n + $i, text(1));
        echo $best, " ", $least, " ", max($n . "0", $i), " ", min(items($i)[0], 1.5), "\n";
    }
}
run($argc);
"#;
    let expected = "z 0 10 0\nzz 1.5 10 1\nzzz 3 10 1.5\n";
    php_oracle(source, expected);
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), expected, "optimized={optimized}");
    }
}

/// A `min()` of a loop counter and a checked bound feeding a float sum allocates nothing.
#[test]
fn test_min_max_selection_hot_min_is_allocation_free() {
    let source = r#"<?php
function run(int $count): void {
    $sum = 0.0;
    $start = memory_get_allocations();
    for ($i = 0; $i < $count; $i++) { $sum += min($i, $count - 1); }
    $end = memory_get_allocations();
    echo "allocations=", $end - $start, " checksum=", $sum, "\n";
}
run(9999 + $argc);
"#;
    assert_eq!(run(source, true), "allocations=0 checksum=49995000\n");
    assert!(run(source, false).ends_with(" checksum=49995000\n"));
}

/// A fractional, a negative and an overflowing hot selection stay allocation-free and exact.
#[test]
fn test_min_max_selection_hot_float_and_overflow_operands_are_allocation_free() {
    let source = r#"<?php
function fractional(int $count): void {
    $sum = 0.0;
    $start = memory_get_allocations();
    for ($i = 0; $i < $count; $i++) { $sum += max($i * 0.5, $count - 9000); }
    $end = memory_get_allocations();
    echo "fractional allocations=", $end - $start, " checksum=", $sum, "\n";
}
function negative(int $count): void {
    $sum = 0.0;
    $start = memory_get_allocations();
    for ($i = 0; $i < $count; $i++) { $sum += min(-$i, 100 - $count); }
    $end = memory_get_allocations();
    echo "negative allocations=", $end - $start, " checksum=", $sum, "\n";
}
function overflow(int $count): void {
    $sum = 0.0;
    $start = memory_get_allocations();
    for ($i = 0; $i < $count; $i++) { $sum += max($i, PHP_INT_MAX + $count); }
    $end = memory_get_allocations();
    echo "overflow allocations=", $end - $start, " float=", is_float($sum) ? 1 : 0, " ratio=", $sum / $count, "\n";
}
fractional(9999 + $argc);
negative(999 + $argc);
overflow(9 + $argc);
"#;
    let expected = "fractional allocations=0 checksum=25998000\n\
negative allocations=0 checksum=-904950\n\
overflow allocations=0 float=1 ratio=9.2233720368548E+18\n";
    assert_eq!(run(source, true), expected);
    let unoptimized = run(source, false);
    for line in ["checksum=25998000\n", "checksum=-904950\n", "float=1 ratio=9.2233720368548E+18\n"] {
        assert!(unoptimized.contains(line), "{unoptimized}");
    }
}

/// A checked prefix accumulator observed by `max()` after the loop, on a cold branch, or in a
/// cold growth computation keeps its frame-local storage, like the plain accumulator.
#[test]
fn test_min_max_selection_keeps_prefix_accumulator_allocation_free() {
    let source = r#"<?php
function simple(buffer<int> $starts, int $cells): int {
    $total = 0;
    for ($cell = 0; $cell < $cells; $cell++) { $total += $starts[$cell]; $starts[$cell] = $total; }
    return $total;
}
function maxAfter(buffer<int> $starts, int $cells): int {
    $total = 0;
    for ($cell = 0; $cell < $cells; $cell++) { $total += $starts[$cell]; $starts[$cell] = $total; }
    return max($total, 0);
}
function coldMax(buffer<int> $starts, int $cells): int {
    $total = 0;
    for ($cell = 0; $cell < $cells; $cell++) { $total += $starts[$cell]; $starts[$cell] = $total; }
    if ($total < 0) { return max($total, 0); }
    return $total;
}
function coldGrowth(buffer<int> $starts, buffer<int> $entries, int $cells): int {
    $total = 0;
    for ($cell = 0; $cell < $cells; $cell++) { $total += $starts[$cell]; $starts[$cell] = $total; }
    if ($total > buffer_len($entries)) {
        $old = $entries; $entries = buffer_new<int>(max($total, 2 * buffer_len($old))); buffer_free($old);
    }
    $result = buffer_len($entries); buffer_free($entries);
    return $result;
}
function seed(buffer<int> $starts, int $cells): void { for ($i = 0; $i < $cells; $i++) { $starts[$i] = 1; } }
function run(int $cells): void {
    $starts = buffer_new<int>($cells); $entries = buffer_new<int>(2 * $cells);
    seed($starts, $cells);
    $before = memory_get_allocations(); $sum = simple($starts, $cells); $after = memory_get_allocations();
    echo "simple allocations=", $after - $before, " checksum=", $sum, "\n";
    seed($starts, $cells);
    $before = memory_get_allocations(); $sum = maxAfter($starts, $cells); $after = memory_get_allocations();
    echo "max-after allocations=", $after - $before, " checksum=", $sum, "\n";
    seed($starts, $cells);
    $before = memory_get_allocations(); $sum = coldMax($starts, $cells); $after = memory_get_allocations();
    echo "cold-max allocations=", $after - $before, " checksum=", $sum, "\n";
    seed($starts, $cells);
    $before = memory_get_allocations(); $sum = coldGrowth($starts, $entries, $cells); $after = memory_get_allocations();
    echo "cold-growth allocations=", $after - $before, " checksum=", $sum, "\n";
    buffer_free($starts);
}
run(3599 + $argc);
"#;
    let expected = "simple allocations=0 checksum=3600\n\
max-after allocations=0 checksum=3600\n\
cold-max allocations=0 checksum=3600\n\
cold-growth allocations=0 checksum=7200\n";
    assert_eq!(run(source, true), expected);
    let unoptimized = run(source, false);
    assert_eq!(unoptimized.matches(" checksum=3600\n").count(), 3, "{unoptimized}");
    assert!(unoptimized.ends_with(" checksum=7200\n"), "{unoptimized}");
}

/// Emits one named function's target assembly, using library mode on iOS.
pub(super) fn function_assembly(source: &str, target: &str, name: &str) -> String {
    let dir = make_cli_test_dir("elephc_min_max_selection_asm");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.args(["--emit-asm", "--target", target]);
    if target.starts_with("ios-") { command.args(["--emit", "staticlib"]); }
    let output = command.arg(&path).output().unwrap();
    assert!(output.status.success(), "{target}: {}", String::from_utf8_lossy(&output.stderr));
    let assembly = fs::read_to_string(path.with_extension("s")).unwrap();
    let start = assembly.find(&format!("@fn name={name} ")).expect("function start marker");
    let end = start + assembly[start..].find(&format!("@endfn name={name}")).expect("function end marker");
    let function = assembly[start..end].to_string();
    fs::remove_dir_all(&dir).unwrap();
    function
}

/// Every supported target sends runtime-typed operands to the selection helper and selects
/// proven numeric operands inline, with the helper present in its runtime.
#[test]
fn test_min_max_selection_emits_on_every_target() {
    let source = r#"<?php
function generic(mixed $a, mixed $b): mixed { return max($a, $b); }
function hot(int $count): void {
    $sum = 0.0;
    for ($i = 0; $i < $count; $i++) { $sum += min($i, $count - 1); }
    echo $sum;
}
echo generic($argc, 2.5);
hot($argc);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let generic = function_assembly(source, name, "generic");
        assert!(generic.contains("__rt_mixed_min_max_select"), "{name}: {generic}");
        let hot = function_assembly(source, name, "hot");
        assert!(!hot.contains("__rt_mixed_min_max_select"), "{name}: {hot}");
        assert!(hot.contains("fcmp d0, d1") || hot.contains("ucomisd xmm1, xmm0"), "{name}: {hot}");
        let target = Target::parse(name).expect("supported target");
        let runtime = elephc::codegen::generate_runtime(8_388_608, target);
        assert!(runtime.contains("__rt_mixed_min_max_select:"), "{name} runtime lacks the selection helper");
    }
}

/// An `int` beside a `float`, a bool, null and a nullable `int` return the winning argument
/// itself, so the result's type follows the winner instead of being fixed at compile time.
#[test]
fn test_min_max_selection_types_static_int_float_bool_and_null_like_php() {
    let source = r#"<?php
function mixNum(int $i, float $f): mixed { return max($i, $f); }
function minNum(float $f, int $i): mixed { return min($f, $i); }
function nullable(?int $n, int $i): mixed { return max($n, $i); }
function nullableMin(?int $n, int $i): mixed { return min($n, $i); }
function flag(bool $b, int $i): mixed { return max($b, $i); }
function three(int $i, float $f, bool $b): mixed { return max($i, $f, $b); }
function clampZero(float $f): mixed { return max($f, 0); }
var_dump(max(3, 2.5), max(2, 2.5), min(3, 2.5), min(2, 2.5), max(1, 1.0), min(1, 1.0), max(1.0, 1));
var_dump(mixNum(3 * $argc, 2.5), mixNum(2 * $argc, 2.5), minNum(2.5, 3 * $argc), minNum(2.5, 2 * $argc));
var_dump(nullable(null, -5), nullable(null, 5), nullable(4, -5), nullableMin(null, -5), nullableMin(null, 5), nullableMin(4, 5));
var_dump(max(null, -5), max(null, 0), min(null, 5), max(true, 2), max(2, true), max(false, -1), min(false, 0), max(true, false));
var_dump(flag(true, 2 * $argc), flag(false, 2 * $argc), flag(false, -2 * $argc), three(1 * $argc, 1.0, true), three(0 * $argc, 0.5, true));
var_dump(clampZero(-1.5), clampZero(1.5), max(-1.5 * $argc, 0) + 1, min(0.5 * $argc, 1) * 2);
"#;
    let expected = r#"int(3)
float(2.5)
float(2.5)
int(2)
int(1)
float(1)
float(1)
int(3)
float(2.5)
float(2.5)
int(2)
int(-5)
int(5)
int(4)
NULL
NULL
int(4)
int(-5)
NULL
NULL
bool(true)
int(2)
int(-1)
int(0)
bool(true)
bool(true)
int(2)
int(-2)
int(1)
float(0.5)
int(0)
float(1.5)
int(1)
float(1)
"#;
    php_oracle(source, expected);
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), expected, "optimized={optimized}");
    }
}

/// The all-`float` register path orders `NAN`, ties and signed zeros like PHP on every target.
#[test]
fn test_min_max_selection_static_floats_follow_php_on_nan_ties_and_signed_zero() {
    let source = r#"<?php
function fmax2(float $a, float $b): float { return max($a, $b); }
function fmin2(float $a, float $b): float { return min($a, $b); }
function fmax3(float $a, float $b, float $c): float { return max($a, $b, $c); }
function fmin3(float $a, float $b, float $c): float { return min($a, $b, $c); }
$nan = sqrt(-1.0 * $argc);
var_dump(fmax2($nan, 1.0), fmax2(1.0, $nan), fmin2($nan, 1.0), fmin2(1.0, $nan));
var_dump(fmax3($nan, 1.0, 2.0), fmax3(1.0, $nan, 2.0), fmax3(1.0, 2.0, $nan), fmin3($nan, 1.0, 2.0), fmin3(1.0, $nan, 2.0), fmin3(2.0, 1.0, $nan));
var_dump(fmax2(-0.0, 0.0), fmax2(0.0, -0.0), fmin2(-0.0, 0.0), fmin2(0.0, -0.0), fmax3(-0.0, 0.0, 0.0), fmin3(0.0, -0.0, -0.0));
var_dump(fmax2(1.5, 2.5), fmin2(1.5, 2.5), fmax3(1.5, 3.5, 2.5), fmin3(2.5, 0.5, 1.5));
"#;
    let expected = r#"float(1)
float(NAN)
float(1)
float(NAN)
float(NAN)
float(2)
float(2)
float(NAN)
float(1)
float(1)
float(-0)
float(0)
float(0)
float(-0)
float(-0)
float(0)
float(2.5)
float(1.5)
float(3.5)
float(0.5)
"#;
    php_oracle(source, expected);
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), expected, "optimized={optimized}");
    }
}

/// A right-hand winner keeps its own tag when the result's frame cell lies beyond the short
/// AArch64 frame range, where the store's address scratch is the register that held the tag.
#[test]
fn test_min_max_selection_right_winner_keeps_its_tag_in_a_wide_frame() {
    let source = r#"<?php
function wide(array $a, array $b, int $count): void {
    $seen = 0;
    foreach ($a as $x) { $seen = $x; }
    foreach ($b as $y) { $seen = $y; }
    $sum = 0.0;
    for ($i = 0; $i < $count; $i++) { $sum += min($count - 1, $i); }
    echo $seen, " ", $sum, "\n";
}
wide([1, 2], [3, 4], 4 + $argc);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64"] {
        let assembly = function_assembly(source, name, "wide");
        assert!(assembly.contains("stack_min_max_right"), "{name}: {assembly}");
        assert!(!assembly.contains("str x9, [x9]"), "{name}: the tag register was used as a store scratch");
    }
    for optimized in [false, true] {
        assert_eq!(run(source, optimized), "4 10\n", "optimized={optimized}");
    }
}

/// The all-`float` register path selects with an explicit comparison on every target instead
/// of `fmax`/`fmin` or `maxsd`/`minsd`, which disagree with PHP on `NAN` and signed zeros.
#[test]
fn test_min_max_selection_static_float_path_compares_explicitly_on_every_target() {
    let source = r#"<?php
function fmax2(float $a, float $b): float { return max($a, $b); }
echo fmax2(1.5 * $argc, 2.5);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let assembly = function_assembly(source, name, "fmax2");
        assert!(assembly.contains("fcsel d0, d0, d1, lt") || assembly.contains("ucomisd xmm1, xmm0"), "{name}: {assembly}");
        for rejected in ["fmax ", "fmin ", "maxsd ", "minsd "] {
            assert!(!assembly.contains(rejected), "{name}: {rejected} in {assembly}");
        }
    }
}

/// A program that declares GMP reduces runtime-typed operands through the prelude helper,
/// which follows the same tie and `NAN` rules, and a comparison the GMP adapter rejects
/// leaves no cell behind when the error is caught.
#[test]
fn test_min_max_selection_with_gmp_declared_follows_php_and_unwinds_cleanly() {
    if skip_without_gmp_native("gmp min/max matrix") {
        return;
    }
    let source = r#"<?php
function mx(mixed $a, mixed $b): mixed { return max($a, $b); }
function mn(mixed $a, mixed $b): mixed { return min($a, $b); }
function mx3(mixed $a, mixed $b, mixed $c): mixed { return max($a, $b, $c); }
function mn3(mixed $a, mixed $b, mixed $c): mixed { return min($a, $b, $c); }
$g = gmp_init(7);
echo gmp_strval(mx($g, 3)), gmp_strval(mn(9, $g)), gmp_strval(mx3(1, $g, 2)), "\n";
$nan = sqrt(-1.0 * $argc);
var_dump(mx($nan, 1), mx(1, $nan), mn($nan, 1), mn(1, $nan));
var_dump(mx3($nan, 1, 2), mx3(1, $nan, 2), mx3(1, 2, $nan), mn3($nan, 1, 2), mn3(1, $nan, 2), mn3(2, 1, $nan));
var_dump(mx(1, 1.0), mn(1, 1.0), mx3(1, 1.0, true), mn3(1.0, 1, true), mx(5.5, 0), mn(-2.5, -3));
for ($i = 0; $i < 3; $i++) {
    try { mx("nope" . $i, $g); } catch (\Throwable $e) { echo get_class($e), "\n"; }
    try { mx3("a" . $i, "b" . $i, $g); } catch (\Throwable $e) { echo get_class($e), "\n"; }
}
"#;
    let expected = "777\n\
int(1)\nfloat(NAN)\nint(1)\nfloat(NAN)\n\
float(NAN)\nint(2)\nint(2)\nfloat(NAN)\nint(1)\nint(1)\n\
int(1)\nfloat(1)\nint(1)\nfloat(1)\nfloat(5.5)\nint(-3)\n\
ValueError\nValueError\nValueError\nValueError\nValueError\nValueError\n";
    for optimized in [false, true] {
        let output = if optimized {
            compile_and_run_with_heap_debug(source)
        } else {
            without_ir_opt(|| compile_and_run_with_heap_debug(source))
        };
        assert!(output.success, "optimized={optimized}: {}", output.stderr);
        assert_eq!(output.stdout, expected, "optimized={optimized}");
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "optimized={optimized}: {}", output.stderr);
    }
}
