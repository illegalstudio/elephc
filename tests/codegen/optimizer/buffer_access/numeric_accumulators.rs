//! Purpose:
//! Verifies buffer descriptor hoisting beside integer accumulators that may promote to float.
//!
//! Called from:
//! - The buffer-access codegen regression module.
//!
//! Key details:
//! - Fixtures retain boxed arithmetic in EIR and compare both optimizer modes.
//! - All supported target emitters must resolve outside the loop without weakening fatals.

use super::{assert_both_modes_fail_after, function_assembly, function_ir, run_variant};

/// Packed collision scans and integer sums whose accumulators cannot be range-proven.
const COUNTERS: &str = r#"<?php
packed class Box { public int $id; public float $minX; public float $maxX; }
function pairs(buffer<Box> $boxes, int $n): int {
    $found = 0;
    for ($i = 0; $i < $n; $i++) {
        for ($j = 0; $j < $n; $j++) {
            if ($boxes[$i]->minX < $boxes[$j]->maxX && $boxes[$j]->minX < $boxes[$i]->maxX) {
                $found++;
            }
        }
    }
    return $found;
}
function ids(buffer<Box> $boxes, int $n): mixed {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += $boxes[$i]->id; }
    return $sum;
}
$b = buffer_new<Box>(3);
$b[0]->id = 2; $b[0]->minX = 0.0; $b[0]->maxX = 2.0;
$b[1]->id = 3; $b[1]->minX = 1.0; $b[1]->maxX = 3.0;
$b[2]->id = 5; $b[2]->minX = 4.0; $b[2]->maxX = 5.0;
echo pairs($b, 3), " ", ids($b, 3), "\n";
buffer_free($b);
"#;

/// Numeric seeds force both increment and compound addition across the integer limit.
const OVERFLOW: &str = r#"<?php
function increment(buffer<int> $b, int $n, int $seed): mixed {
    $found = $seed;
    for ($i = 0; $i < $n; $i++) { if ($b[$i] > 0) { $found++; } }
    return $found;
}
function accumulate(buffer<int> $b, int $n, int $seed): mixed {
    $sum = $seed;
    for ($i = 0; $i < $n; $i++) { $sum += $b[$i]; }
    return $sum;
}
$b = buffer_new<int>(2);
$b[0] = 1; $b[1] = 2;
$a = increment($b, 2, PHP_INT_MAX);
$c = accumulate($b, 2, PHP_INT_MAX);
echo is_float($a), " ", is_float($c), " ", $a == 9223372036854775808.0, " ", $c == $a, "\n";
buffer_free($b);
"#;

/// Requires boxed accumulation to survive while all accesses use one preheader descriptor.
fn assert_numeric_hoisting(source: &str, name: &str) {
    let ir = function_ir(source, true, name);
    assert!(ir.contains("str_inc_dec") || ir.contains("mixed_numeric_binop"), "{ir}");
    assert_eq!(ir.matches("= buffer_resolve ").count(), 1, "{ir}");
    assert!(ir.split("for.cond(").next().unwrap().contains("= buffer_resolve "), "{ir}");
    assert!(ir.contains("= buffer_get_resolved "), "{ir}");
    assert!(!ir.contains("= buffer_get "), "{ir}");
}

/// Nested pair counting and packed-field compound sums resolve once without changing results.
#[test]
fn test_buffer_numeric_accumulator_hoists_nested_and_compound_loops() {
    for name in ["pairs", "ids"] { assert_numeric_hoisting(COUNTERS, name); }
    for optimized in [true, false] { assert_eq!(run_variant(COUNTERS, optimized), "5 10\n"); }
}

/// Crossing PHP_INT_MAX still promotes to float when the buffer descriptor is hoisted.
#[test]
fn test_buffer_numeric_accumulator_preserves_overflow_to_float() {
    for name in ["increment", "accumulate"] { assert_numeric_hoisting(OVERFLOW, name); }
    for optimized in [true, false] { assert_eq!(run_variant(OVERFLOW, optimized), "1 1 1 1\n"); }
}

/// Every target emits one non-faulting preheader resolver, without per-access registry calls.
#[test]
fn test_buffer_numeric_accumulator_resolves_once_on_every_target() {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let assembly = function_assembly(COUNTERS, target, "pairs");
        assert_eq!(assembly.matches("__rt_buffer_try_resolve").count(), 1, "{target}: {assembly}");
        assert!(!assembly.contains("__rt_buffer_resolve"), "{target}: {assembly}");
    }
}

/// Numeric loops keep zero-trip stale handles silent and retain stale-handle and bounds fatals.
#[test]
fn test_buffer_numeric_accumulator_preserves_access_fatals() {
    let prefix = r#"<?php
function total(buffer<int> $b, int $n): mixed {
    $sum = 0;
    for ($i = 0; $i < $n; $i++) { $sum += $b[$i]; }
    return $sum;
}
$b = buffer_new<int>(2); $b[0] = 3; $b[1] = 4;
echo total($b, 2), "\n";
"#;
    let stale = format!("{prefix}$alias = $b; buffer_free($b); echo total($alias, 0), \"\\n\"; echo total($alias, 1);");
    assert_numeric_hoisting(&stale, "total");
    assert_both_modes_fail_after(&stale, "7\n0\n", "use of buffer after buffer_free()");
    let bounds = format!("{prefix}echo total($b, 3);");
    assert_both_modes_fail_after(&bounds, "7\n", "buffer index out of bounds");
}

/// A numeric replacement cannot hide an old object's destructor that frees the buffer.
#[test]
fn test_buffer_numeric_accumulator_rejects_destructor_capable_slot() {
    let source = r#"<?php
class Releaser {
    public buffer<int> $b;
    public function __construct(buffer<int> $b) { $this->b = $b; }
    public function __destruct() { $b = $this->b; buffer_free($b); }
}
function make_releaser(buffer<int> $b): mixed { return new Releaser($b); }
function walk(buffer<int> $b, int $n): void {
    $value = make_releaser($b);
    for ($i = 0; $i < $n; $i++) { $value = $b[$i]; }
}
$b = buffer_new<int>(2);
echo "before\n";
walk($b, 2);
"#;
    let ir = function_ir(source, true, "walk");
    assert!(!ir.contains("buffer_resolve"), "{ir}");
    assert_both_modes_fail_after(source, "before\n", "use of buffer after buffer_free()");
}

/// A raw float cast's range warning may free a buffer before the pending element store.
#[test]
fn test_buffer_numeric_accumulator_rejects_float_cast_warning_handler() {
    let source = r#"<?php
class CastBufferOwner {
    public buffer<int> $value;
    public function __construct(buffer<int> $value) { $this->value = $value; }
}
function cast_into_buffer(buffer<float> $input, buffer<int> $out, int $n): void {
    for ($i = 0; $i < $n; $i++) { $out[$i] = (int)$input[$i]; }
}
$input = buffer_new<float>(1);
$out = buffer_new<int>(1);
$input[0] = 1e20;
$owner = new CastBufferOwner($out);
set_error_handler(function (int $errno, string $message) use ($owner): bool {
    $buffer = $owner->value;
    buffer_free($buffer);
    return $errno > 0 || $message !== '';
});
echo "before\n";
cast_into_buffer($input, $out, 1);
echo "after\n";
"#;
    let cached_bound = source.replace(
        "    for ($i = 0; $i < $n; $i++)",
        "    $n = buffer_len($out);\n    for ($i = 0; $i < $n; $i++)",
    );
    for fixture in [source, cached_bound.as_str()] {
        let ir = function_ir(fixture, true, "cast_into_buffer");
        assert!(!ir.contains("buffer_resolve"), "{ir}");
        assert_both_modes_fail_after(fixture, "before\n", "use of buffer after buffer_free()");
    }
}

/// A NaN-to-bool warning must not leave a pending store using an invalidated descriptor.
#[test]
fn test_buffer_numeric_accumulator_rejects_nan_bool_warning_handler() {
    let source = r#"<?php
class NanBufferOwner {
    public buffer<int> $value;
    public function __construct(buffer<int> $value) { $this->value = $value; }
}
function bool_into_buffer(buffer<float> $input, buffer<int> $out, int $n): void {
    for ($i = 0; $i < $n; $i++) { $out[$i] = (bool)$input[$i] ? 7 : 3; }
}
$input = buffer_new<float>(1);
$out = buffer_new<int>(1);
$input[0] = NAN;
$owner = new NanBufferOwner($out);
set_error_handler(function (int $errno, string $message) use ($owner): bool {
    $buffer = $owner->value;
    buffer_free($buffer);
    return $errno > 0 || $message !== '';
});
echo "before\n";
bool_into_buffer($input, $out, 1);
echo "after\n";
"#;
    let ir = function_ir(source, true, "bool_into_buffer");
    assert!(!ir.contains("buffer_resolve"), "{ir}");
    assert_both_modes_fail_after(source, "before\n", "use of buffer after buffer_free()");
}

/// Implicit float truthiness can warn before a later iteration reads the same buffer.
const NAN_TRUTHINESS: &str = r#"<?php
class TruthyBufferOwner {
    public buffer<float> $value;
    public function __construct(buffer<float> $value) { $this->value = $value; }
}
function scan_truthy(buffer<float> $input, int $n): mixed {
    $hits = 0;
    for ($i = 0; $i < $n; $i++) { if ($input[$i]) { $hits++; } }
    return $hits;
}
$input = buffer_new<float>(3);
$input[0] = NAN; $input[1] = 1.0; $input[2] = 1.0;
$owner = new TruthyBufferOwner($input);
set_error_handler(function (int $errno, string $message) use ($owner): bool {
    $buffer = $owner->value;
    buffer_free($buffer);
    return $errno > 0 || $message !== '';
});
echo "before\n";
scan_truthy($input, 3);
echo "after\n";
"#;

/// Integer multiplication promotes a boxed accumulator to infinity, then subtraction yields NaN.
const BOXED_NAN_TRUTHINESS: &str = r#"<?php
class BoxedTruthyBufferOwner {
    public buffer<int> $value;
    public function __construct(buffer<int> $value) { $this->value = $value; }
}
function scan_boxed_truthy(buffer<int> $input, int $n): mixed {
    $hits = 0; $product = 1;
    for ($i = 0; $i < $n; $i++) {
        $product *= $input[$i];
        if ($product - $product) { $hits++; }
    }
    return $hits;
}
$input = buffer_new<int>(20);
for ($j = 0; $j < 20; $j++) { $input[$j] = PHP_INT_MAX; }
$owner = new BoxedTruthyBufferOwner($input);
set_error_handler(function (int $errno, string $message) use ($owner): bool {
    $buffer = $owner->value;
    buffer_free($buffer);
    return $errno > 0 || $message !== '';
});
echo "before\n";
scan_boxed_truthy($input, 20);
echo "after\n";
"#;

/// A float condition's NaN warning handler must invalidate subsequent counter-loop accesses.
#[test]
fn test_buffer_numeric_accumulator_rejects_float_truthiness_warning_handler() {
    let ir = function_ir(NAN_TRUTHINESS, true, "scan_truthy");
    assert!(ir.contains("is_truthy") && ir.contains("str_inc_dec"), "{ir}");
    assert!(!ir.contains("buffer_resolve"), "{ir}");
    assert_both_modes_fail_after(NAN_TRUTHINESS, "before\n", "use of buffer after buffer_free()");
}

/// A boxed result that becomes NaN cannot keep a descriptor across its implicit bool warning.
#[test]
fn test_buffer_numeric_accumulator_rejects_boxed_truthiness_warning_handler() {
    let ir = function_ir(BOXED_NAN_TRUTHINESS, true, "scan_boxed_truthy");
    assert!(ir.contains("is_truthy") && ir.contains("mixed_numeric_binop"), "{ir}");
    assert!(!ir.contains("buffer_resolve"), "{ir}");
    assert_both_modes_fail_after(BOXED_NAN_TRUTHINESS, "before\n", "use of buffer after buffer_free()");
}

/// A non-inlined predicate's summary must expose its warning to the numeric caller loop.
#[test]
fn test_buffer_numeric_accumulator_rejects_callee_truthiness_warning_handler() {
    let source = NAN_TRUTHINESS.replace(
        "function scan_truthy(",
        "function warning_predicate(float $value, int $depth): bool {\n    if ($depth === 0) { return $value ? true : false; }\n    return warning_predicate($value, $depth >> 1);\n}\nfunction scan_truthy(",
    ).replace("if ($input[$i])", "if (warning_predicate($input[$i], 1))");
    let ir = function_ir(&source, true, "scan_truthy");
    let call = ir.lines().find(|line| line.contains("= call ")).expect("non-inlined predicate call");
    assert!(call.contains("may_warn"), "{ir}");
    assert!(!ir.contains("buffer_resolve"), "{ir}");
    assert_both_modes_fail_after(&source, "before\n", "use of buffer after buffer_free()");
}

/// A borrowed boxed argument's call-boundary bool warning must invalidate the descriptor.
#[test]
fn test_buffer_numeric_accumulator_rejects_mixed_bool_parameter_warning() {
    let source = r#"<?php
class MixedBoolBufferOwner {
    public buffer<int> $value;
    public function __construct(buffer<int> $value) { $this->value = $value; }
}
function flag_predicate(int $depth, bool $flag): bool {
    if ($depth === 0) { return $flag; }
    return flag_predicate($depth >> 1, $flag);
}
function scan_flagged(buffer<int> $input, int $n, mixed $flag): mixed {
    $hits = 0;
    for ($i = 0; $i < $n; $i++) {
        $hits += $input[$i];
        if (flag_predicate(1, $flag)) { $hits++; }
    }
    return $hits;
}
$input = buffer_new<int>(3);
$input[0] = 1; $input[1] = 2; $input[2] = 3;
$owner = new MixedBoolBufferOwner($input);
set_error_handler(function (int $errno, string $message) use ($owner): bool {
    $buffer = $owner->value;
    buffer_free($buffer);
    return $errno > 0 || $message !== '';
});
echo "before\n";
scan_flagged($input, 3, NAN);
echo "after\n";
"#;
    let ir = function_ir(source, true, "scan_flagged");
    let call = ir.lines().find(|line| line.contains("= call ")).expect("non-inlined predicate call");
    assert!(call.contains("may_warn"), "{ir}");
    assert!(!ir.contains("buffer_resolve"), "{ir}");
    assert_both_modes_fail_after(source, "before\n", "use of buffer after buffer_free()");
}

/// A preheader warning must finish before the loop's descriptor is resolved.
#[test]
fn test_buffer_numeric_accumulator_positions_resolver_after_truthiness_warning() {
    let source = NAN_TRUTHINESS.replace(
        "    $hits = 0;\n    for ($i = 0; $i < $n; $i++) { if ($input[$i]) { $hits++; } }\n    return $hits;",
        "    $flag = !$value;\n    for ($i = 0; $i < $n; $i++) { $input[$i] = 1.0; }\n    return $flag;",
    ).replace("buffer<float> $input, int $n): mixed", "buffer<float> $input, int $n, float $value): bool")
        .replace("scan_truthy($input, 3);", "scan_truthy($input, 3, NAN);");
    let ir = function_ir(&source, true, "scan_truthy");
    let preheader = ir.split("for.cond(").next().expect("loop preheader");
    let truthiness = preheader.find("= is_truthy ").expect("preheader warning");
    let resolver = preheader.find("= buffer_resolve ").expect("hoisted resolver");
    assert!(truthiness < resolver, "{ir}");
    assert_both_modes_fail_after(&source, "before\n", "use of buffer after buffer_free()");
}

/// Straight-line property accesses cannot share a descriptor across a truthiness warning.
#[test]
fn test_buffer_numeric_accumulator_blocks_property_reuse_across_truthiness_warning() {
    let source = r#"<?php
packed class TruthyCell { public int $value; }
class TruthyPropertyReader {
    public buffer<TruthyCell> $cells;
    public float $ratio;
    public function __construct(buffer<TruthyCell> $cells) { $this->cells = $cells; $this->ratio = NAN; }
    public function read(): int {
        $first = $this->cells[0]->value;
        $flag = !$this->ratio;
        $second = $this->cells[1]->value;
        return $flag ? $first : $second;
    }
}
$cells = buffer_new<TruthyCell>(2);
$reader = new TruthyPropertyReader($cells);
set_error_handler(function (int $errno, string $message) use ($reader): bool {
    $buffer = $reader->cells;
    buffer_free($buffer);
    return $errno > 0 || $message !== '';
});
echo "before\n";
$reader->read();
echo "after\n";
"#;
    let ir = function_ir(source, true, "TruthyPropertyReader::read");
    assert!(ir.contains("= is_truthy "), "{ir}");
    assert_eq!(ir.matches("= buffer_get ").count(), 2, "{ir}");
    assert!(!ir.contains("buffer_resolve"), "{ir}");
    assert_both_modes_fail_after(source, "before\n", "use of buffer after buffer_free()");
}
