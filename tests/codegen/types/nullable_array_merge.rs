//! Purpose:
//! Preserve missing PHP array elements through lazy expression merge storage.
//!
//! Called from:
//! - The types group in the codegen integration harness.
//!
//! Key details:
//! - Exact PHP output, warning counts and clean heaps cover hits, misses and ownership.

use crate::support::compile_and_run_with_heap_debug;

/// Checks each PHP observation together with warning scope and runtime ownership.
fn assert_merge(source: &str, expected: &str, warnings: usize) {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert_eq!(output.stderr.matches("Warning:").count(), warnings, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Missing and present object elements retain their value through a match.
#[test]
fn test_nullable_array_merge_object_match_indexed() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
function check(bool $choose, int $key): void {
    $boxes = [new Box()];
    $value = match ($choose) { true => $boxes[$key], default => new Box() };
    if ($value === null) { echo "null\n"; } else { echo "object:", $value->n, "\n"; }
}
check(true, 3); check(true, 0); check(false, 3);
"#, "null\nobject:7\nobject:7\n", 1);
}

/// Missing and present object elements retain their value through a match.
#[test]
fn test_nullable_array_merge_object_match_hash() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
function check(bool $choose, string $key): void {
    $boxes = ["hit" => new Box()];
    $value = match ($choose) { true => $boxes[$key], default => new Box() };
    if ($value === null) { echo "null\n"; } else { echo "object:", $value->n, "\n"; }
}
check(true, "missing"); check(true, "hit"); check(false, "missing");
"#, "null\nobject:7\nobject:7\n", 1);
}

/// Missing and present object elements retain their value through a ternary.
#[test]
fn test_nullable_array_merge_object_ternary_indexed() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
function check(bool $choose, int $key): void {
    $boxes = [new Box()];
    $value = $choose ? $boxes[$key] : new Box();
    if ($value === null) { echo "null\n"; } else { echo "object:", $value->n, "\n"; }
}
check(true, 3); check(true, 0); check(false, 3);
"#, "null\nobject:7\nobject:7\n", 1);
}

/// Missing and present object elements retain their value through a ternary.
#[test]
fn test_nullable_array_merge_object_ternary_hash() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
function check(bool $choose, string $key): void {
    $boxes = ["hit" => new Box()];
    $value = $choose ? $boxes[$key] : new Box();
    if ($value === null) { echo "null\n"; } else { echo "object:", $value->n, "\n"; }
}
check(true, "missing"); check(true, "hit"); check(false, "missing");
"#, "null\nobject:7\nobject:7\n", 1);
}

/// Both short-ternary operands preserve missing-element semantics and lazy warnings.
#[test]
fn test_nullable_array_merge_object_short_ternary() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
function left(int $key): void {
    $boxes = [new Box()];
    $value = $boxes[$key] ?: new Box();
    if ($value === null) { echo "null
"; } else { echo "object:", $value->n, "
"; }
}
function right(?Box $input, int $key): void {
    $boxes = [new Box()];
    $value = $input ?: $boxes[$key];
    if ($value === null) { echo "null
"; } else { echo "object:", $value->n, "
"; }
}
left(3); left(0); right(null, 3); right(null, 0); right(new Box(), 3);
"#, "object:7\nobject:7\nnull\nobject:7\nobject:7\n", 2);
}

/// Callable array misses remain null and live closure payloads remain callable.
#[test]
fn test_nullable_array_merge_callable_merges() {
    assert_merge(r#"<?php
function check(bool $choose, string $key): void {
    $callbacks = ["hit" => static fn(): string => "called"];
    $a = match ($choose) { true => $callbacks[$key], default => static fn(): string => "default" };
    $b = $choose ? $callbacks[$key] : static fn(): string => "default";
    if ($a === null) { echo "null
"; } else { echo $a(), "
"; }
    if ($b === null) { echo "null
"; } else { echo $b(), "
"; }
}
check(true, "missing"); check(true, "hit"); check(false, "missing");
"#, "null\nnull\ncalled\ncalled\ndefault\ndefault\n", 2);
}

/// Nested array element results keep null separate from live container values.
#[test]
fn test_nullable_array_merge_array_merges() {
    assert_merge(r#"<?php
function check(bool $choose, int $key): void {
    $arrays = [[1, 2]];
    $a = match ($choose) { true => $arrays[$key], default => [3] };
    $b = $choose ? $arrays[$key] : [3];
    if ($a === null) { echo "null
"; } else { echo count($a), "
"; }
    if ($b === null) { echo "null
"; } else { echo count($b), "
"; }
}
check(true, 3); check(true, 0); check(false, 3);
"#, "null\nnull\n2\n2\n1\n1\n", 2);
}

/// Scalar array misses retain null while falsy and non-null scalar hits keep their types.
#[test]
fn test_nullable_array_merge_scalar_merges() {
    assert_merge(r#"<?php
function ints(bool $choose, int $key): void {
    $values = [7];
    $a = match ($choose) { true => $values[$key], default => 8 };
    $b = $choose ? $values[$key] : 8;
    var_dump($a, $b);
}
ints(true, 3); ints(true, 0); ints(false, 3);
function strings(bool $choose, int $key): void {
    $values = [""];
    $a = match ($choose) { true => $values[$key], default => "default" };
    $b = $choose ? $values[$key] : "default";
    var_dump($a, $b);
}
strings(true, 3); strings(true, 0); strings(false, 3);
function floats(bool $choose, int $key): void {
    $values = [1.25];
    $a = match ($choose) { true => $values[$key], default => 2.5 };
    $b = $choose ? $values[$key] : 2.5;
    var_dump($a, $b);
}
floats(true, 3); floats(true, 0); floats(false, 3);
function bools(bool $choose, int $key): void {
    $values = [false];
    $a = match ($choose) { true => $values[$key], default => true };
    $b = $choose ? $values[$key] : true;
    var_dump($a, $b);
}
bools(true, 3); bools(true, 0); bools(false, 3);
"#, "NULL\nNULL\nint(7)\nint(7)\nint(8)\nint(8)\nNULL\nNULL\nstring(0) \"\"\nstring(0) \"\"\nstring(7) \"default\"\nstring(7) \"default\"\nNULL\nNULL\nfloat(1.25)\nfloat(1.25)\nfloat(2.5)\nfloat(2.5)\nNULL\nNULL\nbool(false)\nbool(false)\nbool(true)\nbool(true)\n", 8);
}

/// A bare typed-array miss stays null while its explicit string cast becomes empty.
#[test]
fn test_nullable_array_merge_typed_string_miss_and_cast() {
    assert_merge(r#"<?php
function check(int $key): void {
    $values = [""];
    var_dump($values[$key]);
    var_dump((string)$values[$key]);
}
check(3); check(0);
"#, "NULL\nstring(0) \"\"\nstring(0) \"\"\nstring(0) \"\"\n", 2);
}

/// Repeated object merges preserve string payloads and release each source and result owner.
#[test]
fn test_nullable_array_merge_owned_object_loop() {
    assert_merge(r#"<?php
class Box {
    public string $text;
    public function __construct(string $text) { $this->text = $text; }
}
function pick(int $key, string $text): ?Box {
    $boxes = [new Box($text)];
    return match ($key) { 0 => $boxes[0], default => $boxes[3] };
}
function run_case(): void {
    $nulls = 0; $length = 0;
    for ($i = 0; $i < 40; $i++) {
        $value = pick($i % 2, "value-" . $i);
        if ($value === null) { $nulls++; } else { $length += strlen($value->text); }
    }
    echo $nulls, ":", $length, "
";
}
run_case();
"#, "20:155\n", 20);
}

/// Nested merges and error suppression keep the same nullable value and warning scope.
#[test]
fn test_nullable_array_merge_nested_suppressed() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
function check(bool $outer, bool $inner, string $key): void {
    $boxes = ["hit" => new Box()];
    $value = @($outer ? match ($inner) { true => $boxes[$key], default => new Box() } : new Box());
    if ($value === null) { echo "null
"; } else { echo "object:", $value->n, "
"; }
}
check(true, true, "missing"); check(true, true, "hit");
check(true, false, "missing"); check(false, true, "missing");
"#, "null\nobject:7\nobject:7\nobject:7\n", 0);
}

/// A missing object element reports null and reaches the ordinary get_class TypeError.
#[test]
fn test_nullable_array_merge_get_class_rejection() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
function check(int $key): void {
    $boxes = [new Box()];
    $value = match ($key) { 0 => new Box(), default => $boxes[3] };
    echo gettype($value), "
";
    try { echo get_class($value), "
"; } catch (TypeError $e) { echo $e->getMessage(), "
"; }
}
check(1); check(0);
"#, "NULL\nget_class(): Argument #1 ($object) must be of type object, null given\nobject\nBox\n", 1);
}

/// Inferred function returns preserve the nullable result of an object array merge.
#[test]
fn test_nullable_array_merge_inferred_returns() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
function pick_match(bool $choose, int $key) {
    $boxes = [new Box()];
    return match ($choose) { true => $boxes[$key], default => new Box() };
}
function pick_ternary(bool $choose, int $key) {
    $boxes = [new Box()];
    return $choose ? $boxes[$key] : new Box();
}
function describe(?Box $value): void {
    if ($value === null) { echo "null
"; } else { echo "object:", $value->n, "
"; }
}
describe(pick_match(true, 3)); describe(pick_match(true, 0)); describe(pick_match(false, 3));
describe(pick_ternary(true, 3)); describe(pick_ternary(true, 0)); describe(pick_ternary(false, 3));
"#, "null\nobject:7\nobject:7\nnull\nobject:7\nobject:7\n", 2);
}

/// A missing string offset remains an empty string rather than an array-style null.
#[test]
fn test_nullable_array_merge_string_offset_control() {
    assert_merge(r#"<?php
function check(bool $choose, int $key): void {
    $text = "abc";
    $a = match ($choose) { true => $text[$key], default => "default" };
    $b = $choose ? $text[$key] : "default";
    var_dump($a, $b);
}
check(true, 8); check(true, 0); check(false, 8);
"#, "string(0) \"\"\nstring(0) \"\"\nstring(1) \"a\"\nstring(1) \"a\"\nstring(7) \"default\"\nstring(7) \"default\"\n", 2);
}

/// Explicit casts still convert a missing value instead of preserving an implicit null.
#[test]
fn test_nullable_array_merge_explicit_cast_controls() {
    assert_merge(r#"<?php
function check(bool $choose, int $key): void {
    $strings = [""]; $ints = [7];
    $a = match ($choose) { true => (string)$strings[$key], default => false };
    $b = $choose ? (int)$ints[$key] : 8;
    var_dump($a, $b);
}
check(true, 3); check(true, 0); check(false, 3);
"#, "string(0) \"\"\nint(0)\nstring(0) \"\"\nint(7)\nbool(false)\nint(8)\n", 2);
}

/// Object reads used by literals and array callables retain values and receiver ownership.
#[test]
fn test_nullable_array_merge_object_read_consumers() {
    assert_merge(r#"<?php
class Box { public int $n = 7; public function read(): int { return $this->n; } }
function values(int $key): void {
    $boxes = [new Box()];
    $a = [$boxes[$key]]; $b = ["value" => $boxes[$key]];
    $first = $a[0]; $second = $b["value"];
    if ($first === null) { echo "null
"; } else { echo $first->n, "
"; }
    if ($second === null) { echo "null
"; } else { echo $second->n, "
"; }
}
function callback(int $key): void {
    $boxes = [new Box()];
    $call = [$boxes[$key], "read"];
    echo $call(), "
";
}
values(3); values(0); callback(0);
"#, "null\nnull\n7\n7\n7\n", 2);
}

/// String offsets retain their string conversion and fallback behavior in short ternaries.
#[test]
fn test_nullable_array_merge_short_ternary_string_offsets() {
    assert_merge(r#"<?php
function check(int $key): void { $s = "a"; var_dump($s[$key] ?: "fallback"); }
check(3); check(0);
"#, "string(8) \"fallback\"\nstring(1) \"a\"\n", 1);
}

/// Concrete object return boundaries retire the nullable merge box while keeping its payload alive.
#[test]
fn test_nullable_array_merge_hinted_object_returns() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
function pick_match(bool $choose): Box {
    $boxes = [new Box()];
    return match ($choose) { true => $boxes[0], default => new Box() };
}
function pick_ternary(bool $choose): Box {
    $boxes = [new Box()];
    return $choose ? $boxes[0] : new Box();
}
echo pick_match(true)->n, ":", pick_match(false)->n, ":", pick_ternary(true)->n, ":", pick_ternary(false)->n, "
";
"#, "7:7:7:7\n", 0);
}

/// The original constant-subject top-level match preserves a missing object as null.
#[test]
fn test_nullable_array_merge_constant_top_level_match() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
$boxes = [0 => new Box()];
$o = match (true) { true => $boxes[3], default => new Box() };
echo gettype($o), " ";
try { echo get_class($o), "
"; } catch (TypeError $e) { echo "TypeError
"; }
"#, "NULL TypeError\n", 1);
}

/// Constant lazy branches retain the implicit null of a selected array read.
#[test]
fn test_nullable_array_merge_constant_top_level_ternary() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
$boxes = [0 => new Box()];
$o = true ? $boxes[3] : null;
echo gettype($o), " ";
try { echo get_class($o), "
"; } catch (TypeError $e) { echo "TypeError
"; }
"#, "NULL TypeError\n", 1);
}

/// Constant lazy branches retain the implicit null of a selected array read.
#[test]
fn test_nullable_array_merge_constant_top_level_short_ternary() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
$boxes = [0 => new Box()];
$o = null ?: $boxes[3];
echo gettype($o), " ";
try { echo get_class($o), "
"; } catch (TypeError $e) { echo "TypeError
"; }
"#, "NULL TypeError\n", 1);
}

/// Constant lazy branches retain the implicit null of a selected array read.
#[test]
fn test_nullable_array_merge_constant_top_level_coalesce() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
$boxes = [0 => new Box()];
$o = null ?? $boxes[3];
echo gettype($o), " ";
try { echo get_class($o), "
"; } catch (TypeError $e) { echo "TypeError
"; }
"#, "NULL TypeError\n", 1);
}

/// A nullable child-object merge projects an independently owned payload through a parent return ABI.
#[test]
fn test_nullable_array_merge_inherited_object_return() {
    assert_merge(r#"<?php
class ParentBox { public int $n = 7; }
class Box extends ParentBox {}
function pick(bool $choose, int $key): ParentBox {
    $boxes = [new Box()];
    return match ($choose) { true => $boxes[$key], default => new Box() };
}
function run_case(): void {
    $sum = 0;
    for ($i = 0; $i < 40; $i++) { $value = pick($i % 2 === 0, 0); $sum += $value->n; }
    echo $sum, "
";
}
run_case();
"#, "280\n", 0);
}

/// A constant-null coalesce still evaluates a suppressed populated default read.
#[test]
fn test_nullable_array_merge_constant_top_level_coalesce_hit() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
$boxes = [0 => new Box()];
$o = null ?? @$boxes[0];
echo gettype($o), " ";
try { echo get_class($o), "
"; } catch (TypeError $e) { echo "TypeError
"; }
"#, "object Box\n", 0);
}

/// Object reads inside array literals keep a callee-compatible element storage representation.
#[test]
fn test_nullable_array_merge_object_array_argument() {
    assert_merge(r#"<?php
class Box { public int $n = 7; }
function read_items($items): int { return $items[0]->n; }
function check(): void { $boxes = [new Box()]; echo read_items([$boxes[0]]), "
"; }
check();
"#, "7\n", 0);
}

/// Typed scalar hash reads preserve null and populated values at lazy merges.
#[test]
fn test_nullable_array_merge_scalar_hash_merges() {
    assert_merge(r#"<?php
function ints(bool $choose, string $key): void {
    $values = ["hit" => 7];
    $a = match ($choose) { true => $values[$key], default => 8 };
    $b = $choose ? $values[$key] : 8;
    var_dump($a, $b);
}
ints(true, "missing"); ints(true, "hit"); ints(false, "missing");
function strings(bool $choose, string $key): void {
    $values = ["hit" => ""];
    $a = match ($choose) { true => $values[$key], default => "default" };
    $b = $choose ? $values[$key] : "default";
    var_dump($a, $b);
}
strings(true, "missing"); strings(true, "hit"); strings(false, "missing");
function floats(bool $choose, string $key): void {
    $values = ["hit" => 1.25];
    $a = match ($choose) { true => $values[$key], default => 2.5 };
    $b = $choose ? $values[$key] : 2.5;
    var_dump($a, $b);
}
floats(true, "missing"); floats(true, "hit"); floats(false, "missing");
function bools(bool $choose, string $key): void {
    $values = ["hit" => false];
    $a = match ($choose) { true => $values[$key], default => true };
    $b = $choose ? $values[$key] : true;
    var_dump($a, $b);
}
bools(true, "missing"); bools(true, "hit"); bools(false, "missing");
"#, "NULL\nNULL\nint(7)\nint(7)\nint(8)\nint(8)\nNULL\nNULL\nstring(0) \"\"\nstring(0) \"\"\nstring(7) \"default\"\nstring(7) \"default\"\nNULL\nNULL\nfloat(1.25)\nfloat(1.25)\nfloat(2.5)\nfloat(2.5)\nNULL\nNULL\nbool(false)\nbool(false)\nbool(true)\nbool(true)\n", 8);
}

/// Typed indexed closure reads preserve null and callable payload ownership.
#[test]
fn test_nullable_array_merge_callable_indexed_merges() {
    assert_merge(r#"<?php
function check(bool $choose, int $key): void {
    $callbacks = [static fn(): string => "called"];
    $a = match ($choose) { true => $callbacks[$key], default => static fn(): string => "default" };
    $b = $choose ? $callbacks[$key] : static fn(): string => "default";
    if ($a === null) { echo "null
"; } else { echo $a(), "
"; }
    if ($b === null) { echo "null
"; } else { echo $b(), "
"; }
}
check(true, 3); check(true, 0); check(false, 3);
"#, "null\nnull\ncalled\ncalled\ndefault\ndefault\n", 2);
}

/// Nullable merge reads still evaluate each offset once and preserve warning suppression.
#[test]
fn test_nullable_array_merge_null_receiver_offsets_run_once() {
    let out = crate::support::compile_and_run_with_heap_debug(
        r#"<?php
function offset(): int { echo "K"; return 0; }
$rows = [[1, 2]];
$r = $argc == 1 ? $rows[5] : ["a", "b"];
var_dump($r[offset()]);
var_dump(@$r[offset()]);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "KNULL\nKNULL\n");
    assert_eq!(out.stderr.matches("Warning: Trying to access array offset on null").count(), 1);
    assert_eq!(out.stderr.matches("Warning: Undefined array key 5").count(), 1);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Function, method and captured-closure spreads keep native read storage and unpack diagnostics.
#[test]
fn test_nullable_array_merge_call_spreads_keep_native_source_shape() {
    let out = crate::support::compile_and_run_with_heap_debug(
        r#"<?php
function spread_pair(int $a, int $b): void { echo "F:", $a, ":", $b, "\n"; }
class SpreadOps { public function pair(int $a, int $b): void { echo "M:", $a, ":", $b, "\n"; } }
function run_case(string $prefix): void {
    $rows = [[1, 2]];
    $ops = new SpreadOps();
    $call = function(int $a, int $b) use ($prefix): void { echo $prefix, ":", $a, ":", $b, "\n"; };
    spread_pair(...$rows[0]);
    $ops->pair(...$rows[0]);
    $call(...$rows[0]);
    try { spread_pair(...$rows[7]); } catch (Error $error) { echo $error->getMessage(), "\n"; unset($error); }
    try { $ops->pair(...$rows[7]); } catch (Error $error) { echo $error->getMessage(), "\n"; unset($error); }
    try { $call(...$rows[7]); } catch (Error $error) { echo $error->getMessage(), "\n"; unset($error); }
    try { spread_pair(...@$rows[7]); } catch (Error $error) { echo $error->getMessage(), "\n"; unset($error); }
    try { $ops->pair(...@$rows[7]); } catch (Error $error) { echo $error->getMessage(), "\n"; unset($error); }
    try { $call(...@$rows[7]); } catch (Error $error) { echo $error->getMessage(), "\n"; unset($error); }
}
run_case("C");
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "F:1:2\nM:1:2\nC:1:2\nOnly arrays and Traversables can be unpacked, null given\nOnly arrays and Traversables can be unpacked, null given\nOnly arrays and Traversables can be unpacked, null given\nOnly arrays and Traversables can be unpacked, null given\nOnly arrays and Traversables can be unpacked, null given\nOnly arrays and Traversables can be unpacked, null given\n");
    assert_eq!(out.stderr.matches("Warning: Undefined array key 7").count(), 3);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Undefined roots use canonical null receivers for silent scalar-key reads.
#[test]
fn test_nullable_array_merge_undefined_quiet_offsets_keep_null_cells() {
    let out = crate::support::compile_and_run_with_heap_debug(r#"<?php
function quiet_offset(): int { echo "K"; return 0; }
var_dump(isset($neverQuiet[quiet_offset()]));
echo $neverQuiet[quiet_offset()] ?? "fallback", "\n";
$null = null;
var_dump($null[0] ?? 1);
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "Kbool(false)\nKfallback\nint(1)\n");
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
