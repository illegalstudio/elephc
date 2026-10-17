//! Purpose:
//! Regressions for nullable scalar storage in coalesce and short-ternary merge locals.
//!
//! Called from:
//! - The types group in the codegen integration harness.
//!
//! Key details:
//! - Typed parameters keep both runtime branches visible to EIR lowering.
//! - Exact PHP output and clean heaps distinguish null from falsy scalar values.

use crate::support::{compile_and_run_with_heap_debug, ProgramOutput};

/// Checks complete observable output, successful execution and balanced runtime ownership.
fn assert_clean(output: ProgramOutput, expected: &str) {
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// A null fallback preserves null and every tested integer payload.
#[test]
fn test_nullable_scalar_coalesce_nullable_int() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
function check(?int $value): void { $result = $value ?? null; var_dump($result); }
check(null); check(0); check(-7); check(PHP_INT_MIN); check(PHP_INT_MAX); check(PHP_INT_MAX - 1);
"#), "NULL\nint(0)\nint(-7)\nint(-9223372036854775808)\nint(9223372036854775807)\nint(9223372036854775806)\n");
}

/// Nullable strings retain null separately from empty and zero strings.
#[test]
fn test_nullable_scalar_coalesce_nullable_string() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
function check(?string $value): void { $result = $value ?? null; var_dump($result); }
check(null); check(""); check("0"); check("kept");
"#), "NULL\nstring(0) \"\"\nstring(1) \"0\"\nstring(4) \"kept\"\n");
}

/// Nullable floats preserve null, signed zero, infinities and NaN.
#[test]
fn test_nullable_scalar_coalesce_nullable_float() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
function check(?float $value): void { $result = $value ?? null; var_dump($result); }
check(null); check(0.0); check(-0.0); check(1.25); check(INF); check(NAN);
"#), "NULL\nfloat(0)\nfloat(-0)\nfloat(1.25)\nfloat(INF)\nfloat(NAN)\n");
}

/// A false value is a coalesce hit while null reaches the null fallback.
#[test]
fn test_nullable_scalar_coalesce_nullable_bool() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
function check(?bool $value): void { $result = $value ?? null; var_dump($result); }
check(null); check(false); check(true);
"#), "NULL\nbool(false)\nbool(true)\n");
}

/// A null-returning fallback is lazy and a later coalesce sees the preserved null.
#[test]
fn test_nullable_scalar_coalesce_fallback_and_chaining() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
$calls = 0;
function fallback() { global $calls; $calls++; return null; }
function check(?string $value): void { $result = $value ?? fallback(); var_dump($result); }
function chained(?int $value): void { var_dump(($value ?? null) ?? "default"); }
check(null); check(""); check("0"); check("kept");
echo "calls:", $calls, "\n";
chained(null); chained(0); chained(7);
"#), "NULL\nstring(0) \"\"\nstring(1) \"0\"\nstring(4) \"kept\"\ncalls:1\nstring(7) \"default\"\nint(0)\nint(7)\n");
}

/// Short ternaries keep null fallbacks for nullable scalar inputs.
#[test]
fn test_nullable_scalar_coalesce_nullable_short_ternary() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
function ints(?int $value): void { $result = $value ?: null; var_dump($result); }
function strings(?string $value): void { $result = $value ?: null; var_dump($result); }
function floats(?float $value): void { $result = $value ?: null; var_dump($result); }
function bools(?bool $value): void { $result = $value ?: null; var_dump($result); }
ints(null); ints(0); ints(7);
strings(null); strings(""); strings("0"); strings("kept");
floats(null); floats(0.0); floats(1.25);
bools(null); bools(false); bools(true);
"#), "NULL\nNULL\nint(7)\nNULL\nNULL\nNULL\nstring(4) \"kept\"\nNULL\nNULL\nfloat(1.25)\nNULL\nNULL\nbool(true)\n");
}

/// Falsy non-null scalars still select a real null in short ternary merges.
#[test]
fn test_nullable_scalar_coalesce_scalar_short_ternary() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
function ints(int $value): void { $result = $value ?: null; var_dump($result); }
function strings(string $value): void { $result = $value ?: null; var_dump($result); }
function floats(float $value): void { $result = $value ?: null; var_dump($result); }
function bools(bool $value): void { $result = $value ?: null; var_dump($result); }
ints(0); ints(7);
strings(""); strings("0"); strings("kept");
floats(0.0); floats(1.25);
bools(false); bools(true);
"#), "NULL\nint(7)\nNULL\nNULL\nstring(4) \"kept\"\nNULL\nfloat(1.25)\nNULL\nbool(true)\n");
}

/// Local coalescing assignment preserves both its nullable destination and result.
#[test]
fn test_nullable_scalar_coalesce_local_assignment() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
function ints(?int $value): void { $result = ($value ??= null); var_dump($value, $result); }
function strings(?string $value): void { $result = ($value ??= null); var_dump($value, $result); }
function floats(?float $value): void { $result = ($value ??= null); var_dump($value, $result); }
function bools(?bool $value): void { $result = ($value ??= null); var_dump($value, $result); }
ints(null); ints(0); ints(7);
strings(null); strings(""); strings("kept");
floats(null); floats(0.0); floats(1.25);
bools(null); bools(false); bools(true);
"#), "NULL\nNULL\nint(0)\nint(0)\nint(7)\nint(7)\nNULL\nNULL\nstring(0) \"\"\nstring(0) \"\"\nstring(4) \"kept\"\nstring(4) \"kept\"\nNULL\nNULL\nfloat(0)\nfloat(0)\nfloat(1.25)\nfloat(1.25)\nNULL\nNULL\nbool(false)\nbool(false)\nbool(true)\nbool(true)\n");
}

/// Property coalescing assignment keeps null results and falsy stored values.
#[test]
fn test_nullable_scalar_coalesce_property_assignment() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class Values {
    public ?int $i;
    public ?string $s;
    public ?float $f;
    public ?bool $b;
}
$values = new Values();
var_dump($values->i ??= null, $values->s ??= null, $values->f ??= null, $values->b ??= null);
$values->i = 0; $values->s = "0"; $values->f = 0.0; $values->b = false;
var_dump($values->i ??= null, $values->s ??= null, $values->f ??= null, $values->b ??= null);
var_dump($values->i, $values->s, $values->f, $values->b);
$other = new Values();
$other->s ??= null;
$other->s = "stmt";
var_dump($other->s);
"#), "NULL\nNULL\nNULL\nNULL\nint(0)\nstring(1) \"0\"\nfloat(0)\nbool(false)\nint(0)\nstring(1) \"0\"\nfloat(0)\nbool(false)\nstring(4) \"stmt\"\n");
}

/// Silent scalar array misses yield null without changing each hit value type.
#[test]
fn test_nullable_scalar_coalesce_silent_array_misses() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
function ints(string $key): void { $a = ["hit" => 7]; $result = $a[$key] ?? null; var_dump($result); }
function strings(string $key): void { $a = ["hit" => "kept"]; $result = $a[$key] ?? null; var_dump($result); }
function floats(string $key): void { $a = ["hit" => 1.25]; $result = $a[$key] ?? null; var_dump($result); }
function bools(string $key): void { $a = ["hit" => false]; $result = $a[$key] ?? null; var_dump($result); }
ints("missing"); ints("hit"); strings("missing"); strings("hit");
floats("missing"); floats("hit"); bools("missing"); bools("hit");
"#), "NULL\nint(7)\nNULL\nstring(4) \"kept\"\nNULL\nfloat(1.25)\nNULL\nbool(false)\n");
}

/// Repeated nullable string returns release every merge owner and source payload.
#[test]
fn test_nullable_scalar_coalesce_owned_string_loop() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
function keep(?string $value): ?string { return $value ?? null; }
$nulls = 0; $length = 0;
for ($i = 0; $i < 40; $i++) {
    $source = $i % 2 === 0 ? null : "value-" . $i;
    $result = keep($source);
    if ($result === null) { $nulls++; } else { $length += strlen($result); }
}
echo $nulls, ":", $length, "\n";
"#), "20:155\n");
}

/// Existing nullable object, callable and array coalescing keeps its behavior and owners.
#[test]
fn test_nullable_scalar_coalesce_object_callable_array_controls() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class Box { public int $n = 7; }
function objects(?Box $value): void {
    $result = $value ?? null;
    if ($result === null) { echo "object:null\n"; } else { echo "object:", $result->n, "\n"; }
}
function callables(?callable $value): void {
    $result = $value ?? null;
    if ($result === null) { echo "callable:null\n"; } else { echo "callable:", $result(), "\n"; }
}
function arrays(?array $value): void {
    $result = $value ?? null;
    if ($result === null) { echo "array:null\n"; } else { echo "array:", count($result), "\n"; }
}
objects(null); objects(new Box());
callables(null); callables(static fn(): int => 9);
arrays(null); arrays([1, 2]);
"#), "object:null\nobject:7\ncallable:null\ncallable:9\narray:null\narray:2\n");
}

/// Inferred coalescing returns retain null and declared non-null scalar results remain valid.
#[test]
fn test_nullable_scalar_coalesce_inferred_returns() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
function ints(?int $value) { return $value ?? null; }
function strings(?string $value) { return $value ?? null; }
function floats(?float $value) { return $value ?? null; }
function bools(?bool $value) { return $value ?? null; }
function exact_int(int $value): int { return $value ?? null; }
function exact_string(string $value): string { return $value ?? null; }
function exact_float(float $value): float { return $value ?? null; }
function exact_bool(bool $value): bool { return $value ?? null; }
var_dump(ints(null), ints(0), strings(null), strings(""), floats(null), floats(-0.0), bools(null), bools(false));
var_dump(exact_int(7), exact_string("kept"), exact_float(1.25), exact_bool(false));
"#), "NULL\nint(0)\nNULL\nstring(0) \"\"\nNULL\nfloat(-0)\nNULL\nbool(false)\nint(7)\nstring(4) \"kept\"\nfloat(1.25)\nbool(false)\n");
}

/// Static nullable-property probes transfer their owner before later writes and cleanup.
#[test]
fn test_nullable_scalar_coalesce_static_assignment() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
class Values {
    public static ?int $i;
    public static ?string $s;
    public static ?float $f;
    public static ?bool $b;
    public static ?string $t;
}
function run_case(): void {
    var_dump(Values::$i ??= null, Values::$s ??= null, Values::$f ??= null, Values::$b ??= null);
    Values::$i = 0; Values::$s = "0"; Values::$f = 0.0; Values::$b = false;
    var_dump(Values::$i ??= null, Values::$s ??= null, Values::$f ??= null, Values::$b ??= null);
    var_dump(Values::$i, Values::$s, Values::$f, Values::$b);
    Values::$t ??= null;
    Values::$t = "stmt";
    var_dump(Values::$t);
}
run_case();
"#), "NULL\nNULL\nNULL\nNULL\nint(0)\nstring(1) \"0\"\nfloat(0)\nbool(false)\nint(0)\nstring(1) \"0\"\nfloat(0)\nbool(false)\nstring(4) \"stmt\"\n");
}

/// Declared non-null scalar returns reject null and still accept falsy coalesce hits.
#[test]
fn test_nullable_scalar_coalesce_declared_return_checks() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
function ints(?int $value): int { return $value ?? null; }
function strings(?string $value): string { return $value ?? null; }
function floats(?float $value): float { return $value ?? null; }
function bools(?bool $value): bool { return $value ?? null; }
try { ints(null); } catch (TypeError $e) { echo "int:null\n"; }
try { strings(null); } catch (TypeError $e) { echo "string:null\n"; }
try { floats(null); } catch (TypeError $e) { echo "float:null\n"; }
try { bools(null); } catch (TypeError $e) { echo "bool:null\n"; }
var_dump(ints(0), strings(""), floats(-0.0), bools(false));
"#), "int:null\nstring:null\nfloat:null\nbool:null\nint(0)\nstring(0) \"\"\nfloat(-0)\nbool(false)\n");
}

/// Array coalescing assignment keeps both the stored value and the expression result.
#[test]
fn test_nullable_scalar_coalesce_array_assignment() {
    assert_clean(compile_and_run_with_heap_debug(r#"<?php
function ints(string $key): void { $a = ["hit" => 7]; var_dump($a[$key] ??= null, $a[$key]); }
function strings(string $key): void { $a = ["hit" => "kept"]; var_dump($a[$key] ??= null, $a[$key]); }
function floats(string $key): void { $a = ["hit" => 1.25]; var_dump($a[$key] ??= null, $a[$key]); }
function bools(string $key): void { $a = ["hit" => false]; var_dump($a[$key] ??= null, $a[$key]); }
ints("missing"); ints("hit"); strings("missing"); strings("hit");
floats("missing"); floats("hit"); bools("missing"); bools("hit");
function indexed_ints(int $key): void { $a = [7]; var_dump($a[$key] ??= null, $a[$key]); }
function indexed_strings(int $key): void { $a = ["kept"]; var_dump($a[$key] ??= null, $a[$key]); }
function indexed_floats(int $key): void { $a = [1.25]; var_dump($a[$key] ??= null, $a[$key]); }
function indexed_bools(int $key): void { $a = [false]; var_dump($a[$key] ??= null, $a[$key]); }
indexed_ints(3); indexed_ints(0); indexed_strings(3); indexed_strings(0);
indexed_floats(3); indexed_floats(0); indexed_bools(3); indexed_bools(0);
"#), "NULL\nNULL\nint(7)\nint(7)\nNULL\nNULL\nstring(4) \"kept\"\nstring(4) \"kept\"\nNULL\nNULL\nfloat(1.25)\nfloat(1.25)\nNULL\nNULL\nbool(false)\nbool(false)\nNULL\nNULL\nint(7)\nint(7)\nNULL\nNULL\nstring(4) \"kept\"\nstring(4) \"kept\"\nNULL\nNULL\nfloat(1.25)\nfloat(1.25)\nNULL\nNULL\nbool(false)\nbool(false)\n");
}
