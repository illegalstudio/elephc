//! Purpose:
//! Integration tests for list destructuring from narrowed and associative array values.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Null guards ending in `continue` or `break` must preserve the non-null complement.
//! - Associative-array storage remains a valid RHS when positional integer keys are present.

use crate::support::*;

/// Verifies a list unpack whose TARGET is also the variable it reads from. PHP evaluates the
/// right-hand side once and assigns from that value; the simple unpack form read each element
/// straight out of the source variable, so storing element 0 into it destroyed the source and the
/// next read looked for element 1 in the scalar just written — `Warning: Undefined array key 1`
/// and an empty second target. Symfony's `PhpFilesAdapter` unpacks exactly this way
/// (`[$expiresAt, $value] = $expiresAt;`). PHP outputs "arr:10:v|mixed:20:w".
#[test]
fn test_list_unpack_target_that_shadows_its_own_source() {
    let out = compile_and_run(
        r#"<?php
function mkArray(int $n): array { $o = []; $o[] = 10; $o[] = 'v'; return $o; }
function mkMixed(int $n): mixed { $o = []; $o[] = 20; $o[] = 'w'; return $o; }

$x = mkArray(1);
[$x, $y] = $x;
echo "arr:$x:$y|";

$m = mkMixed(1);
[$m, $z] = $m;
echo "mixed:$m:$z";
"#,
    );
    assert_eq!(out, "arr:10:v|mixed:20:w");
}

/// Declared arrays keep numeric-key semantics and retain the RHS while a destination replaces it.
#[test]
fn test_list_unpack_boxed_array_preserves_source_and_numeric_keys() {
    let out = compile_and_run(
        r#"<?php
function unpackBoxedArray(array $items): string {
    [$items, $tail] = $items;
    return $items . ":" . $tail;
}
function unpackNullableArray(?array $items): string {
    if ($items === null) { return "null"; }
    [$first, $second] = $items;
    return $first . ":" . $second;
}
echo unpackBoxedArray([str_repeat("a", 3), str_repeat("b", 3)]), "|";
echo unpackBoxedArray([1 => "second", 0 => "first"]), "|";
echo unpackNullableArray([1 => "one", 0 => "zero"]), "|", unpackNullableArray(null);
"#,
    );
    assert_eq!(out, "aaa:bbb|first:second|zero:one|null");
}

/// Temporary RHS containers release their owners once every destructured value is retained.
#[test]
fn test_list_unpack_releases_temporary_boxed_and_packed_arrays() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function makeUnpackRow(): array { return [str_repeat("a", 48), str_repeat("b", 49)]; }
for ($i = 0; $i < 3; $i++) {
    [$first, $second] = makeUnpackRow();
    [$third, $fourth] = [str_repeat("c", 50), str_repeat("d", 51)];
    echo strlen($first), ":", strlen($second), ":", strlen($third), ":", strlen($fourth), "|";
    unset($first, $second, $third, $fourth);
}
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "48:49:50:51|48:49:50:51|48:49:50:51|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies a null guard ending in `continue` narrows `?array` to `Array` before list unpacking.
#[test]
fn test_null_guard_continue_narrows_list_unpack_rhs() {
    let out = compile_and_run(
        r#"<?php
final class R {
    private function mk(int $n): ?array {
        if ($n < 0) { return null; }
        return ["k" . $n, "v" . $n];
    }
    public function run(): string {
        $out = "";
        foreach ([1, -1, 2] as $n) {
            $entry = $this->mk($n);
            if ($entry === null) { continue; }
            [$key, $value] = $entry;
            $out .= $key . "=" . $value . ";";
        }
        return $out;
    }
}
echo (new R())->run();
"#,
    );
    assert_eq!(out, "k1=v1;k2=v2;");
}

/// Verifies a null guard ending in `break` narrows `?array` to `Array` before list unpacking.
#[test]
fn test_null_guard_break_narrows_list_unpack_rhs() {
    let out = compile_and_run(
        r#"<?php
function row(int $n): ?array {
    if ($n < 0) { return null; }
    return ["k" . $n, "v" . $n];
}
$out = "";
foreach ([1, -1, 2] as $n) {
    $entry = row($n);
    if ($entry === null) { break; }
    [$key, $value] = $entry;
    $out .= $key . "=" . $value . ";";
}
echo $out;
"#,
    );
    assert_eq!(out, "k1=v1;");
}

/// Verifies positional list unpacking accepts associative storage with integer keys.
#[test]
fn test_list_unpack_assoc_array_rhs() {
    let out = compile_and_run(
        r#"<?php
$row = [0 => "left", 1 => "right", "label" => "ignored"];
[$left, $right] = $row;
echo $left . ":" . $right;
"#,
    );
    assert_eq!(out, "left:right");
}

/// Verifies positional list unpacking validates and reads a boxed Mixed array result at runtime.
#[test]
fn test_list_unpack_mixed_function_result() {
    let out = compile_and_run(
        r#"<?php
function resolveRuntime(): mixed {
    return ["app", "args"];
}
[$app, $args] = resolveRuntime();
echo $app . ":" . $args;
"#,
    );
    assert_eq!(out, "app:args");
}

/// Verifies a nullable array can be destructured without prior narrowing and that a null value
/// follows PHP's missing-element behavior instead of being rejected statically.
#[test]
fn test_list_unpack_nullable_array_without_guard() {
    let out = compile_and_run(
        r#"<?php
function row(bool $present): ?array {
    return $present ? ["left", "right"] : null;
}
[$left, $right] = row(true);
echo $left . ":" . $right . ";";
[$missingLeft, $missingRight] = row(false);
var_dump($missingLeft, $missingRight);
"#,
    );
    assert_eq!(out, "left:right;NULL\nNULL\n");
}

/// Verifies destructuring is valid inside a condition, evaluates its RHS once, and yields that
/// original array before the surrounding boolean operators inspect it.
#[test]
fn test_list_unpack_assignment_expression_in_condition() {
    let out = compile_and_run(
        r#"<?php
function serviceEntry(): array {
    echo "E";
    return [null, "ok"];
}
if (!([$file, $code] = serviceEntry()) || null !== $file) {
    echo "bad";
} else {
    echo $code;
}
"#,
    );
    assert_eq!(out, "Eok");
}

/// Verifies `foreach` value destructuring in both spellings PHP accepts (`[...]` and
/// `list(...)`), plus the `$key => [...]` form. Expected output matches `php -r` on 8.4.
#[test]
fn test_foreach_value_destructuring() {
    let out = compile_and_run(
        r#"<?php
$m = [[1, 2], [3, 4]];
foreach ($m as [$a, $b]) { echo $a, "-", $b, ";"; }
foreach ($m as list($a, $b)) { echo $a, "+", $b, ";"; }
foreach ($m as $k => [$a, $b]) { echo $k, ":", $a, ",", $b, ";"; }
"#,
    );
    assert_eq!(out, "1-2;3-4;1+2;3+4;0:1,2;1:3,4;");
}

/// Verifies keyed, skipped-element, and nested `foreach` destructuring patterns, which all
/// reuse the same lowering as a standalone `[...] = $value;` assignment.
#[test]
fn test_foreach_destructuring_keyed_skipped_and_nested() {
    let out = compile_and_run(
        r#"<?php
$pairs = [["name" => "ann", "age" => 30], ["name" => "bob", "age" => 40]];
foreach ($pairs as ["name" => $n, "age" => $g]) { echo $n, "=", $g, ";"; }
$skip = [[1, 2, 3], [4, 5, 6]];
foreach ($skip as [, $second]) { echo $second, ";"; }
$nested = [[1, [2, 3]], [4, [5, 6]]];
foreach ($nested as [$x, [$y, $z]]) { echo $x, $y, $z, ";"; }
"#,
    );
    assert_eq!(out, "ann=30;bob=40;2;5;123;456;");
}

/// Verifies that a heterogeneous tuple yielded after a callback sort stays boxed while a
/// `foreach` destructuring pattern reads its later positional elements.
///
/// The iterator cannot expose the tuple's raw array representation to the synthetic
/// destructuring local: that local is gradual because array elements are refcounted.  Each
/// generated `$row[$offset]` therefore must receive a boxed `Mixed` cell, exactly as a direct
/// gradual list-unpack read does.
#[test]
fn test_foreach_destructuring_after_uasort_of_heterogeneous_tuples() {
    let out = compile_and_run(
        r#"<?php
$services = [];
$services[] = [0, 0, null, "first", null];
$services[] = [1, 1, "named", "second", "ServiceClass"];
uasort($services, static fn ($a, $b) => $b[0] <=> $a[0] ?: $a[1] <=> $b[1]);
foreach ($services as [, , $index, $serviceId, $class]) {
    echo ($index ?? "none"), ":", $serviceId, ":", ($class ?? "none"), ";";
}
"#,
    );
    assert_eq!(out, "named:second:ServiceClass;none:first:none;");
}

/// Verifies destructuring `foreach` loops nest, and that the pattern also works with a
/// single-statement body and inside a function over an `array`-hinted parameter.
#[test]
fn test_foreach_destructuring_nested_loops_and_bodies() {
    let out = compile_and_run(
        r#"<?php
$m = [[1, 2], [3, 4]];
foreach ($m as [$a, $b]) { foreach ($m as [$c, $d]) { echo $a, $b, $c, $d, "|"; } }
foreach ($m as [$a, $b]) echo $a, $b, ";";
function total(array $rows): int { $t = 0; foreach ($rows as [$x, $y]) { $t += $x * $y; } return $t; }
echo total([[1, 2], [3, 4]]);
"#,
    );
    assert_eq!(out, "1212|1234|3412|3434|12;34;14");
}

/// Verifies an empty array that a loop both GROWS and ITERATES is readable on the iterations
/// after the first, with its elements destructured.
///
/// `$seen = []` enters the loop as `array<never>`, and the only statement that says what it holds
/// is the `$seen[] = ...` that comes AFTER the read textually. The read is still reached with
/// elements on the second iteration, so leaving the element type at `never` refused a program
/// `php -n` runs — and typing it without a storage contract read the elements back as null
/// ("Trying to access array offset on null"). The loop-carried contract now boxes the payload
/// before the loop, which is the promotion `apply_loop_storage_contracts` can materialize.
///
/// Symfony's `CompiledUrlMatcherDumper::groupStaticRoutes` is the shape this was found on
/// (`foreach ($dynamicRegex as [$hostRx, $rx, $prefix])` above `$dynamicRegex[] = [...]`).
/// Reference PHP 8.5 prints "add:x;add:y;hit:x/X;".
#[test]
fn test_empty_array_grown_and_destructured_in_the_same_loop() {
    let out = compile_and_run(
        r#"<?php
function group(array $rows): string
{
    $out = '';
    $seen = [];

    foreach ($rows as $row) {
        foreach ($seen as [$a, $b]) {
            if ($a === $row) {
                $out .= "hit:$a/$b;";
                continue 2;
            }
        }
        $seen[] = [$row, strtoupper($row)];
        $out .= "add:$row;";
    }

    return $out;
}

echo group(['x', 'y', 'x']);
"#,
    );
    assert_eq!(out, "add:x;add:y;hit:x/X;");
}

/// Verifies the same loop-carried read with SCALAR elements, which take the identical contract
/// path but exercise no destructuring — the element type still has to leave `never`.
///
/// Reference PHP 8.5 prints "1|1,2|1,2,3|".
#[test]
fn test_empty_array_grown_and_iterated_in_the_same_loop() {
    let out = compile_and_run(
        r#"<?php
$seen = [];
$out = '';
foreach ([1, 2, 3] as $n) {
    $seen[] = $n;
    foreach ($seen as $s) {
        $out .= $s;
        $out .= ',';
    }
    $out = rtrim($out, ',') . '|';
}
echo $out;
"#,
    );
    assert_eq!(out, "1|1,2|1,2,3|");
}

/// Verifies a plain accumulator — grown but never read inside the loop — still works.
///
/// This is the case the widening deliberately does NOT touch: nothing in the body consumes the
/// element type, so the array keeps its precise element rather than paying for a boxed payload.
/// The assertion is behavioural; what it guards is that the gate did not change the answer.
#[test]
fn test_accumulator_grown_without_being_read_in_the_loop() {
    let out = compile_and_run(
        r#"<?php
$out = [];
foreach ([1, 2, 3] as $n) {
    $out[] = $n * 2;
}
echo implode(',', $out), ':', count($out);
"#,
    );
    assert_eq!(out, "2,4,6:3");
}

/// Destructures an `ArrayAccess` object through `offsetGet()`, as php does.
///
/// Laravel's `Inspiring::formatForConsole()` writes `[$text, $author] = (new Stringable($quote))
/// ->explode('-')`, which returns a Collection; the checker refused any object on the right-hand
/// side. Expected output is `php -n` 8.5's.
#[test]
fn test_list_unpack_of_an_array_access_object() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
class Coll implements ArrayAccess {
    public function __construct(private array $items) {}
    public function offsetExists(mixed $o): bool { return isset($this->items[$o]); }
    public function offsetGet(mixed $o): mixed { return $this->items[$o]; }
    public function offsetSet(mixed $o, mixed $v): void { $this->items[$o] = $v; }
    public function offsetUnset(mixed $o): void { unset($this->items[$o]); }
}
function viaMixed(mixed $c): string { [$a, $b] = $c; return "$a|$b"; }
function viaObject(Coll $c): string { [$a, $b] = $c; return "$a|$b"; }
function make(string $s): Coll { return new Coll(explode('-', $s)); }
echo viaMixed(new Coll(['x', 'y'])), "\n";
echo viaObject(new Coll(['p', 'q'])), "\n";
[$t, $u] = make("left - right");
echo trim($t), "/", trim($u), "\n";
"#,
    );
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "x|y\np|q\nleft/right\n");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Reads and writes locals through variable variables (`$$name`, `${expr}`).
///
/// Carbon's `Date::set()`, `Localization::translateTimeString()` and `CarbonInterval`'s
/// constructor use them, and the parser refused the files, so every Carbon class (and Laravel's
/// `Illuminate\Support\Carbon` with them) fell out of the compiled program. Expected output is
/// `php -n` 8.5's.
#[test]
fn test_variable_variables_read_and_write_locals() {
    let out = compile_and_run(
        r#"<?php
function setPart(string $name, int $value): string {
    [$year, $month, $day] = [2024, 1, 15];
    switch ($name) {
        case 'month':
        case 'day':
            $$name = $value;
            break;
    }
    return "$year-$month-$day";
}
function translate(string $from, string $to): string {
    $out = [];
    foreach (['from', 'to'] as $key) {
        $language = $$key;
        $out[] = strtoupper($language);
    }
    return implode(">", $out);
}
function units(int $years = 0, int $days = 0): string {
    $parts = [];
    foreach (['years', 'days'] as $unit) {
        $value = $$unit;
        $parts[] = "$unit=$value";
    }
    $n = 'years';
    $parts[] = ${$n} + ${'days'};
    return implode(",", $parts);
}
echo setPart('month', 7), " ", setPart('day', 3), " ", setPart('x', 9), "\n";
echo translate('en', 'fr'), "\n";
echo units(2, 5), "\n";
$f = function (string $k) { $alpha = 'A'; $beta = 'B'; return $$k; };
echo $f('beta'), $f('alpha'), "\n";
"#,
    );
    assert_eq!(out, "2024-7-15 2024-1-3 2024-1-15\nEN>FR\nyears=2,days=5,7\nBA\n");
}

/// Verifies a `foreach` destructuring pattern can bind elements BY REFERENCE (`[$a, &$b]`), keyed
/// or not, and that php iterates such a loop by reference: writes through the `&` entry reach
/// the source array. Symfony's `TransportResponseTrait::stream()` uses this shape, and refusing
/// it sent the whole HTTP client to the interpreter. Expected output is php 8.5's.
#[test]
fn test_foreach_destructuring_binds_reference_entries() {
    let out = compile_and_run(
        r#"<?php
$rows = [[1, [10, 20]], [2, [30]]];
foreach ($rows as $i => [$id, &$list]) {
    $list[] = $id * 100;
}
echo json_encode($rows), "\n";
$pairs = [['k' => 'a', 'v' => 1], ['k' => 'b', 'v' => 2]];
foreach ($pairs as ['k' => $k, 'v' => &$v]) {
    $v = $v . $k;
}
unset($v);
echo json_encode($pairs), "\n";
$m = ['x' => [5, 6]];
foreach ($m as [&$first, $second]) {
    $first += $second;
}
echo json_encode($m), "\n";
"#,
    );
    assert_eq!(
        out,
        "[[1,[10,20,100]],[2,[30,200]]]\n[{\"k\":\"a\",\"v\":\"1a\"},{\"k\":\"b\",\"v\":\"2b\"}]\n{\"x\":[11,6]}\n"
    );
}

/// An empty array a loop pushes onto in one branch and pops in another is read with the elements
/// an earlier iteration appended. Symfony's expression-language `Lexer::tokenize()` keeps its
/// bracket stack this way: the stack stayed `array<never>` at the loop header, so `array_pop()`
/// returned null (a silent wrong answer) and the destructuring form was refused outright.
#[test]
fn test_loop_stack_pushed_and_popped_in_branches_keeps_its_elements() {
    let out = compile_and_run(
        r#"<?php
function lex(string $e): string {
    $brackets = [];
    $out = "";
    for ($i = 0; $i < strlen($e); $i++) {
        $c = $e[$i];
        if ($c === "(") {
            $brackets[] = [$c, $i];
        } elseif ($c === ")") {
            [$expect, $cur] = array_pop($brackets);
            $out .= $expect . $cur . ";";
        }
    }
    return $out;
}
function kinds(string $s): string {
    $b = [];
    $o = "";
    foreach (str_split($s) as $i => $ch) {
        if ($ch === "(") {
            $b[] = [$ch, $i];
        } else {
            $o .= gettype(array_pop($b)) . ",";
        }
    }
    return $o;
}
echo lex("(a(b))"), "|", kinds("(a)"), "\n";
"#,
    );
    assert_eq!(out, "(2;(0;|array,NULL,\n");
}
