//! Purpose:
//! Preserve missing PHP array elements through lazy expression merge storage.
//!
//! Called from:
//! - The types group in the codegen integration harness.
//!
//! Key details:
//! - Exact PHP output, warning counts and clean heaps cover hits, misses and ownership.
//! - A string cast of a stored missing typed-array entry is a real empty string.
//!   The bare read stays null. Both shapes are checked with IR optimization on and off.
//! - A cast inside a loop sees a miss stored later in that loop, and a miss returned
//!   from a call or ternary and then stored casts to an empty string too.
//! - A loop that stores an untyped array element does not leak the previous string.
//!   Mixed locals, `??`, and nested indexes print the same bytes as PHP.
//! - An empty cast of a mixed local does not allocate when it is passed through
//!   `trim` into a call, concatenation, a hash key, `yield`, or `try`.

use crate::support::{compile_and_run_with_heap_debug, without_ir_opt};

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

/// Direct and stored missing string entries cast to an empty string and dump as null when unread.
///
/// PHP 8.2–8.5 prints `string(0) ""` for `(string)` of a missing typed-array entry whether the
/// read is inline, stored, copied, or returned, and prints `NULL` for the bare read. A present
/// empty string, a heap string that shares the array, a concatenation, and a real function
/// result stay strings. `$argc` keeps the index runtime-unknown so neither cast is folded away.
#[test]
fn test_stored_missing_string_cast_is_empty() {
    assert_stored_missing_string_cast();
}

/// The stored missing-entry string cast stays an empty string when EIR optimization is off.
#[test]
fn test_stored_missing_string_cast_is_empty_no_ir_opt() {
    without_ir_opt(assert_stored_missing_string_cast);
}

/// Runs the stored missing-entry cast fixture and checks PHP's stdout, warnings, and heap.
fn assert_stored_missing_string_cast() {
    assert_merge(
        STORED_MISSING_STRING_CAST,
        STORED_MISSING_STRING_CAST_OUT,
        6,
    );
}

/// Fixture for direct, stored, copied, and returned casts of missing typed-array strings.
const STORED_MISSING_STRING_CAST: &str = r#"<?php
function label(): string { return "ok"; }
function pass($value) { return $value; }
function probe(int $key): void {
    $values = [""];
    $stored = $values[$key];
    $again = $stored;
    var_dump($values[$key]);
    var_dump($stored);
    var_dump($again);
    var_dump((string)$values[$key]);
    var_dump((string)$stored);
    var_dump((string)$again);
    var_dump(is_null($stored));
    var_dump(is_null((string)$stored));
    var_dump(is_null((string)$again));
    var_dump((string)$stored === "");
    $present = $values[0];
    var_dump($present);
    var_dump((string)$present);
    var_dump(is_null($present));
    var_dump(is_null((string)$present));
    $hash = ["hit" => ""];
    $name = "missing";
    $hstored = $hash[$name];
    var_dump($hash[$name]);
    var_dump((string)$hstored);
    var_dump($hash["hit"]);
    var_dump((string)$hash["hit"]);
    $bag = [str_repeat("z", 3)];
    $miss = $bag[$key];
    var_dump($miss);
    var_dump((string)$miss);
    var_dump((string)$bag[0]);
    var_dump($bag[0]);
    var_dump((string)("ab" . "cd"));
    $nulls = [null, null];
    $slot = $key == 10 ? 0 : 1;
    $n = $nulls[$slot];
    var_dump((string)$n);
    var_dump(is_null((string)$n));
    var_dump((string)($key == 9));
    var_dump(is_null((string)($key == 9)));
    var_dump((string)($key == 10));
    var_dump((string)@$values[$key]);
    var_dump(pass($stored));
    var_dump((string)pass($stored));
    var_dump(is_null((string)pass($stored)));
    echo "[", (string)$stored, "]\n";
    echo strlen((string)$stored), "\n";
}
probe($argc + 9);
var_dump((string)label());
"#;

/// PHP 8.5 stdout for [`STORED_MISSING_STRING_CAST`], stable across PHP 8.2–8.5.
const STORED_MISSING_STRING_CAST_OUT: &str = "\
NULL
NULL
NULL
string(0) \"\"
string(0) \"\"
string(0) \"\"
bool(true)
bool(false)
bool(false)
bool(true)
string(0) \"\"
string(0) \"\"
bool(false)
bool(false)
NULL
string(0) \"\"
string(0) \"\"
string(0) \"\"
NULL
string(0) \"\"
string(3) \"zzz\"
string(3) \"zzz\"
string(4) \"abcd\"
string(0) \"\"
bool(false)
string(0) \"\"
bool(false)
string(1) \"1\"
string(0) \"\"
NULL
string(0) \"\"
bool(false)
[]
0
string(2) \"ok\"
";

/// A cast before a later store in the same loop still becomes an empty string on the next iteration.
///
/// The store is lowered after the cast, but the back-edge runs it first. PHP 8.2–8.5 prints the
/// original string once, then `string(0) ""`. A later assignment of a real string stays that string.
#[test]
fn test_loop_carried_missing_string_cast_is_empty() {
    assert_loop_carried_missing_string_cast();
}

/// The loop-carried missing-entry cast stays an empty string when EIR optimization is off.
#[test]
fn test_loop_carried_missing_string_cast_is_empty_no_ir_opt() {
    without_ir_opt(assert_loop_carried_missing_string_cast);
}

/// Runs the loop back-edge fixture and checks PHP's stdout, warnings, and heap.
fn assert_loop_carried_missing_string_cast() {
    assert_merge(
        LOOP_CARRIED_MISSING_STRING_CAST,
        LOOP_CARRIED_MISSING_STRING_CAST_OUT,
        10,
    );
}

/// Fixture for casts that run before the missing store in source order and after it at runtime.
const LOOP_CARRIED_MISSING_STRING_CAST: &str = r#"<?php
function probe(int $key): void {
    $values = [""];
    $s = "seed";
    for ($i = 0; $i < 2; $i++) {
        var_dump((string)$s);
        $s = $values[$key];
    }
    $copy = "seed";
    for ($i = 0; $i < 2; $i++) {
        var_dump((string)$copy);
        $t = $values[$key];
        $copy = $t;
    }
    $again = "seed";
    $i = 0;
    while ($i < 2) {
        var_dump((string)$again);
        $again = $values[$key];
        $i++;
    }
    $rolled = "seed";
    $i = 0;
    do {
        var_dump((string)$rolled);
        $rolled = $values[$key];
        $i++;
    } while ($i < 2);
    $rolled = "seed";
    foreach ([0, 1] as $i) {
        var_dump((string)$rolled);
        $rolled = $values[$key];
    }
    $kept = "stay";
    for ($i = 0; $i < 2; $i++) {
        var_dump((string)$kept);
        $kept = "ok";
    }
}
probe($argc + 9);
"#;

/// PHP 8.5 stdout for [`LOOP_CARRIED_MISSING_STRING_CAST`], stable across PHP 8.2–8.5.
const LOOP_CARRIED_MISSING_STRING_CAST_OUT: &str = "\
string(4) \"seed\"
string(0) \"\"
string(4) \"seed\"
string(0) \"\"
string(4) \"seed\"
string(0) \"\"
string(4) \"seed\"
string(0) \"\"
string(4) \"seed\"
string(0) \"\"
string(4) \"stay\"
string(2) \"ok\"
";

/// A loop over an untyped array parameter does not leak the string it casts.
///
/// The cast runs before the store, and the store reads an untyped `array`, a `match`,
/// a `foreach` that `continue`s, or `$a[$k] ?? "dd"`. PHP 8.2–8.5 keeps the original
/// string for the first iteration and the assigned element after that. Fifty calls
/// must leave the heap clean. `return (string)$s` from the loop returns the last cast.
#[test]
fn test_untyped_array_loop_string_cast_is_heap_clean() {
    assert_untyped_array_loop_string_cast();
}

/// The untyped-array loop cast stays heap-clean when EIR optimization is off.
#[test]
fn test_untyped_array_loop_string_cast_is_heap_clean_no_ir_opt() {
    without_ir_opt(assert_untyped_array_loop_string_cast);
}

/// Runs the untyped-array loop fixture and checks PHP's stdout and a clean heap.
fn assert_untyped_array_loop_string_cast() {
    assert_merge(
        UNTYPED_ARRAY_LOOP_STRING_CAST,
        UNTYPED_ARRAY_LOOP_STRING_CAST_OUT,
        0,
    );
}

/// Fixture for `(string)` of a local overwritten by an untyped array element inside a loop.
const UNTYPED_ARRAY_LOOP_STRING_CAST: &str = r#"<?php
function loop_param(array $a): int {
    $s = str_repeat("b", 30); $n = 0;
    for ($i = 0; $i < 3; $i++) { $n += strlen((string)$s); $s = $a[1]; }
    return $n;
}
function loop_match(array $a): int {
    $s = str_repeat("b", 30); $n = 0;
    for ($i = 0; $i < 3; $i++) {
        $n += strlen((string)$s);
        $s = match ($i) { 0 => $a[1], default => $a[1] };
    }
    return $n;
}
function loop_foreach(array $a): int {
    $s = str_repeat("b", 30); $n = 0;
    foreach ([0, 1, 2] as $i) {
        $n += strlen((string)$s);
        if ($i === 0) { continue; }
        $s = $a[1];
    }
    return $n;
}
function loop_return(array $a): string {
    $s = str_repeat("b", 30);
    $out = "";
    for ($i = 0; $i < 3; $i++) {
        $out = (string)$s;
        $s = $a[1];
    }
    return $out;
}
function loop_coalesce(array $a, int $key): int {
    $s = str_repeat("b", 30); $n = 0;
    for ($i = 0; $i < 3; $i++) {
        $n += strlen((string)$s);
        $s = $a[$key] ?? "dd";
    }
    return $n;
}
$a = ["p", "q"];
$key = $argc > 0 ? 1 : 0;
$sum = 0;
for ($j = 0; $j < 50; $j++) {
    $sum += loop_param($a);
    $sum += loop_match($a);
    $sum += loop_foreach($a);
    $sum += strlen(loop_return($a));
    $sum += loop_coalesce($a, $key);
}
echo $sum, "\n";
"#;

/// PHP 8.5 stdout for [`UNTYPED_ARRAY_LOOP_STRING_CAST`], stable across PHP 8.2–8.5.
const UNTYPED_ARRAY_LOOP_STRING_CAST_OUT: &str = "7900\n";

/// Casts that used to free their source, and nested misses, print PHP's bytes.
///
/// A mixed local (`match` / `@`), `(string)($m["k"] ?? "n")`, and `$grid[0][$key]`
/// inside a loop must echo the characters PHP 8.2–8.5 prints. The untyped array
/// parameter is also called 50 times so a per-iteration leak fails the heap check.
#[test]
fn test_string_cast_bytes_survive_mixed_coalesce_and_nested_index() {
    assert_string_cast_bytes();
}

/// Those casts print the same bytes when EIR optimization is off.
#[test]
fn test_string_cast_bytes_survive_mixed_coalesce_and_nested_index_no_ir_opt() {
    without_ir_opt(assert_string_cast_bytes);
}

/// Runs the byte-level cast fixture and checks PHP's stdout, warnings, and heap.
fn assert_string_cast_bytes() {
    assert_merge(STRING_CAST_BYTES, STRING_CAST_BYTES_OUT, 2);
}

/// Fixture for cast bytes, nested misses, and the untyped-array leak repro.
const STRING_CAST_BYTES: &str = r#"<?php
function bytes_mixed(int $key): void {
    $values = ["a"];
    $s = "i";
    for ($i = 0; $i < 3; $i++) {
        echo "[", (string)$s, "]\n";
        $s = match ($i) { 0 => @$values[$key], default => "zz" };
    }
}
function bytes_coalesce(): void {
    $m = ["k" => str_repeat("w", 6)];
    echo (string)($m["k"] ?? "n"), "\n";
    echo (string)($m["missing"] ?? "n"), "\n";
}
function bytes_nested(int $key): void {
    $grid = [["a"]];
    $s = "i";
    for ($i = 0; $i < 2; $i++) {
        echo "[", (string)$s, "]\n";
        $s = $grid[0][$key];
    }
}
function leak_param(array $a): int {
    $s = str_repeat("b", 30);
    $n = 0;
    for ($i = 0; $i < 3; $i++) {
        $n += strlen((string)$s);
        $s = $a[1];
    }
    return $n;
}
bytes_mixed($argc + 5);
bytes_coalesce();
bytes_nested($argc + 5);
$sum = 0;
for ($j = 0; $j < 50; $j++) {
    $sum += leak_param(["p", "q"]);
}
echo $sum, "\n";
"#;

/// PHP 8.5 stdout for [`STRING_CAST_BYTES`], stable across PHP 8.2–8.5.
const STRING_CAST_BYTES_OUT: &str = "\
[i]
[]
[zz]
wwwwww
n
[i]
[]
1600
";

/// `(string)` passed to a function or `trim` does not copy a live string each iteration.
///
/// `$vals` is a typed string array, so a present element is a normal string and a
/// suppressed miss is the empty cast. PHP 8.2–8.5 prints `<ggg>ggg` then `<>`.
/// One hundred iterations must not leave a block per call.
#[test]
fn test_string_cast_call_argument_is_heap_clean() {
    assert_string_cast_call_argument();
}

/// The call-argument cast stays heap-clean when EIR optimization is off.
#[test]
fn test_string_cast_call_argument_is_heap_clean_no_ir_opt() {
    without_ir_opt(assert_string_cast_call_argument);
}

/// Runs the call-argument cast fixture and checks PHP's bytes and a clean heap.
fn assert_string_cast_call_argument() {
    assert_merge(STRING_CAST_CALL_ARGUMENT, STRING_CAST_CALL_ARGUMENT_OUT, 0);
}

/// Fixture for `id((string)$s)` and `trim((string)$s)` over a typed string array.
const STRING_CAST_CALL_ARGUMENT: &str = r#"<?php
function id(string $x): string { return "<".$x.">"; }
class Box {
    function id(string $x): string { return "<".$x.">"; }
}
function show(int $key): void {
    $vals = ["a"];
    $box = new Box();
    $s = str_repeat("g", 3);
    for ($i = 0; $i < 100; $i++) {
        if ($i < 4) {
            echo id((string)$s);
            echo $box->id((string)$s);
            echo trim((string)$s), "\n";
        } else {
            id((string)$s);
            $box->id((string)$s);
            trim((string)$s);
        }
        $s = $i % 2 ? str_repeat("g", 3) : @$vals[$key];
    }
}
show($argc + 5);
"#;

/// PHP 8.5 stdout for [`STRING_CAST_CALL_ARGUMENT`], stable across PHP 8.2–8.5.
const STRING_CAST_CALL_ARGUMENT_OUT: &str = "\
<ggg><ggg>ggg
<><>
<ggg><ggg>ggg
<><>
";

/// An empty cast of a mixed local does not leave a block inside `trim`, concat, a key, `yield`, or `try`.
///
/// Once `$s` holds a missing typed-array element it is `null|int|string`. `(string)$s`
/// is `""`. Copying that empty string allocated one block per use when the copy was
/// nested under `id(trim(...))`, concatenation, a hash key, `yield`, or `try`.
/// PHP 8.5 prints the real string on the first iteration and the empty cast after that.
#[test]
fn test_empty_mixed_string_cast_copy_is_heap_clean() {
    assert_empty_mixed_string_cast_copy();
}

/// The empty mixed cast stays heap-clean when EIR optimization is off.
#[test]
fn test_empty_mixed_string_cast_copy_is_heap_clean_no_ir_opt() {
    without_ir_opt(assert_empty_mixed_string_cast_copy);
}

/// Runs the nested empty-cast fixture and checks PHP's bytes and a clean heap.
fn assert_empty_mixed_string_cast_copy() {
    assert_merge(
        EMPTY_MIXED_STRING_CAST_COPY,
        EMPTY_MIXED_STRING_CAST_COPY_OUT,
        0,
    );
}

/// Fixture for an empty `(string)` of a mixed local used beyond a direct call.
const EMPTY_MIXED_STRING_CAST_COPY: &str = r#"<?php
function id(string $x): string { return "<".$x.">"; }
function show_call(int $key): void {
    $vals = ["a"];
    $s = str_repeat("g", 3);
    for ($i = 0; $i < 100; $i++) {
        $a = id(trim((string)$s));
        if ($i < 2) { echo "call:", $a, "\n"; }
        $s = $key == 99 ? 42 : @$vals[$key];
    }
}
function show_concat(int $key): void {
    $vals = ["a"];
    $s = str_repeat("g", 3);
    for ($i = 0; $i < 100; $i++) {
        $a = trim((string)$s) . trim((string)$s);
        if ($i < 2) { echo "cat:", $a, "\n"; }
        $s = $key == 99 ? 42 : @$vals[$key];
    }
}
function show_key(int $key): void {
    $vals = ["a"];
    $keys = ["ggg" => "hit", "" => "empty"];
    $s = str_repeat("g", 3);
    for ($i = 0; $i < 100; $i++) {
        $a = $keys[(string)$s] ?? "x";
        if ($i < 2) { echo "key:", $a, "\n"; }
        $s = $key == 99 ? 42 : @$vals[$key];
    }
}
function gen(int $key): Generator {
    $vals = ["a"];
    $s = str_repeat("g", 3);
    for ($i = 0; $i < 100; $i++) {
        yield trim((string)$s);
        $s = $key == 99 ? 42 : @$vals[$key];
    }
}
function show_try(int $key): void {
    $vals = ["a"];
    $s = str_repeat("g", 3);
    for ($i = 0; $i < 100; $i++) {
        try {
            $a = id(trim((string)$s));
        } catch (Throwable $e) {
            $a = $e->getMessage();
        }
        if ($i < 2) { echo "try:", $a, "\n"; }
        $s = $key == 99 ? 42 : @$vals[$key];
    }
}
show_call($argc + 5);
show_concat($argc + 5);
show_key($argc + 5);
$n = 0;
foreach (gen($argc + 5) as $v) {
    if ($n < 2) { echo "yield:", $v, "\n"; }
    $n++;
}
show_try($argc + 5);
"#;

/// PHP 8.5 stdout for [`EMPTY_MIXED_STRING_CAST_COPY`], stable across PHP 8.2–8.5.
const EMPTY_MIXED_STRING_CAST_COPY_OUT: &str = "\
call:<ggg>
call:<>
cat:gggggg
cat:
key:hit
key:empty
yield:ggg
yield:
try:<ggg>
try:<>
";

/// A miss returned by a call or ternary and then stored casts to an empty string.
///
/// PHP 8.2–8.5 prints `NULL` for the stored return itself and `string(0) ""` for `(string)` of
/// it. `is_null` of that cast is false. `$argc` keeps the chosen ternary arm runtime-unknown.
#[test]
fn test_stored_call_and_ternary_missing_string_cast() {
    assert_stored_call_and_ternary_missing_string_cast();
}

/// The stored call and ternary casts stay empty strings when EIR optimization is off.
#[test]
fn test_stored_call_and_ternary_missing_string_cast_no_ir_opt() {
    without_ir_opt(assert_stored_call_and_ternary_missing_string_cast);
}

/// Runs the stored call and ternary fixture and checks PHP's stdout, warnings, and heap.
fn assert_stored_call_and_ternary_missing_string_cast() {
    assert_merge(
        STORED_CALL_AND_TERNARY_MISSING_STRING_CAST,
        STORED_CALL_AND_TERNARY_MISSING_STRING_CAST_OUT,
        1,
    );
}

/// Fixture for `(string)` of a call result and a ternary result saved in a local.
const STORED_CALL_AND_TERNARY_MISSING_STRING_CAST: &str = r#"<?php
function pass($value) { return $value; }
function probe(int $key): void {
    $values = [""];
    $stored = $values[$key];
    $y = pass($stored);
    $z = ($key == 10) ? pass($stored) : "keep";
    var_dump($y);
    var_dump((string)$y);
    var_dump(is_null((string)$y));
    var_dump($z);
    var_dump((string)$z);
    var_dump(is_null((string)$z));
}
probe($argc + 9);
"#;

/// PHP 8.5 stdout for [`STORED_CALL_AND_TERNARY_MISSING_STRING_CAST`], stable across PHP 8.2–8.5.
const STORED_CALL_AND_TERNARY_MISSING_STRING_CAST_OUT: &str = "\
NULL
string(0) \"\"
bool(false)
NULL
string(0) \"\"
bool(false)
";

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
