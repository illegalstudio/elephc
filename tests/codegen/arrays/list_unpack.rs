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
