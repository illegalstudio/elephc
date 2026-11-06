//! Purpose:
//! Regression coverage for `array_values()`, `array_flip()`, `in_array()` and `array_slice()`
//! over a receiver whose static type is `mixed` (issues #630 and #1348).
//!
//! Called from:
//! - The native codegen suite's array module.
//!
//! Key details:
//! - Expected output was produced by PHP 8.5 from the same source.
//! - `json_decode()` fixtures are not heap-checked: the decoder leaks its result on its own,
//!   independently of these builtins. The heap fixture reaches `mixed` through typed parameters.
//! - A non-array payload raises a `TypeError` whose message starts with PHP's wording; elephc
//!   omits PHP's trailing `, <type> given`, like the other boxed array builtins, so fixtures
//!   compare the message prefix.

use crate::support::{compile_and_run, compile_and_run_with_heap_debug};

/// The three reproductions from issue #630 compile and print what PHP prints.
#[test]
fn test_mixed_receiver_issue_630_reproductions() {
    let out = compile_and_run(
        r#"<?php
$value = json_decode("[1,2]");
echo count(array_values($value)), "\n";
$value = json_decode("[\"a\",\"b\"]");
echo array_flip($value)["b"], "\n";
$value = json_decode("[1,2]");
var_dump(in_array(2, $value));
"#,
    );
    assert_eq!(out, "2\n1\nbool(true)\n");
}

/// `array_values()` over a `mixed` receiver renumbers indexed, sparse, associative, nested and
/// empty arrays, including a `json_decode()` result.
#[test]
fn test_array_values_on_mixed_receiver_shapes() {
    let out = compile_and_run(
        r#"<?php
function show(mixed $v): void { echo json_encode(array_values($v)), "|"; }
show([3 => "p", 7 => "q"]);
show(["k" => "v", "n" => 5, "f" => 1.5]);
show([[1, 2], ["x" => [3]]]);
show([]);
show(json_decode("{\"a\":1,\"b\":{\"c\":2}}", true));
$m = json_decode("[\"a\",\"b\"]");
echo count(array_values($m)), "|", array_values($m)[1];
"#,
    );
    assert_eq!(
        out,
        r#"["p","q"]|["v",5,1.5]|[[1,2],{"x":[3]}]|[]|[1,{"c":2}]|2|b"#
    );
}

/// `array_flip()` over a `mixed` receiver swaps keys and values for indexed, sparse,
/// associative and empty arrays.
#[test]
fn test_array_flip_on_mixed_receiver_shapes() {
    let out = compile_and_run(
        r#"<?php
function show(mixed $v): void { echo json_encode(array_flip($v)), "|"; }
show(["a", "b", "c"]);
show([3 => "p", 7 => "q"]);
show(["k" => "v", "n" => 5]);
show([]);
$m = json_decode("[\"a\",\"b\"]");
echo array_flip($m)["b"], "|", count(array_flip($m));
"#,
    );
    assert_eq!(out, r#"{"a":0,"b":1,"c":2}|{"p":3,"q":7}|{"v":"k","5":"n"}|[]|1|2"#);
}

/// `in_array()` over a `mixed` haystack answers PHP's loose and strict membership, including
/// array needles compared structurally against boxed elements, and reads an untyped `$strict`
/// flag by PHP truthiness rather than by its boxed cell pointer.
#[test]
fn test_in_array_on_mixed_haystack_loose_and_strict() {
    let out = compile_and_run(
        r#"<?php
function has(mixed $needle, mixed $haystack, bool $strict): string {
    return in_array($needle, $haystack, $strict) ? "y" : "n";
}
function hasUntyped($needle, $haystack, $strict) {
    return in_array($needle, $haystack, $strict) ? "y" : "n";
}
$list = [10, "20", 3.5, [1, 2], null];
$assoc = ["k" => "v", "n" => 5, "a" => ["x" => 1]];
foreach ([false, true] as $strict) {
    echo has(10, $list, $strict), has("10", $list, $strict), has(20, $list, $strict),
        has("3.5", $list, $strict), has([1, 2], $list, $strict), has(["1", 2], $list, $strict),
        has([2, 1], $list, $strict), has(null, $list, $strict), has(0, $list, $strict), "|";
    echo has("v", $assoc, $strict), has("5", $assoc, $strict), has(["x" => 1], $assoc, $strict),
        has(["x" => "1"], $assoc, $strict), has(["y" => 1], $assoc, $strict), "|";
}
echo hasUntyped("5", $assoc, false), hasUntyped("5", $assoc, true),
    hasUntyped("5", $assoc, 0), hasUntyped("5", $assoc, "1"), "|";
$json = json_decode("[1,[4,5]]", true);
var_dump(in_array(2, json_decode("[1,2]")), in_array([4, 5], $json), in_array([5, 4], $json));
"#,
    );
    assert_eq!(
        out,
        "yyyyyynyy|yyyyn|ynnnynnyn|ynynn|ynyn|bool(true)\nbool(true)\nbool(false)\n"
    );
}

/// A non-array payload raises a catchable `TypeError` from all three builtins, a union with an
/// array member is accepted, and namespaced, case-insensitive and callable spellings share the
/// same contract.
#[test]
fn test_mixed_receiver_non_array_type_error_union_and_call_forms() {
    let out = compile_and_run(
        r#"<?php
namespace App;

function maybe(int $n): array|false { return $n > 0 ? ["a" => "x", "b" => "y"] : false; }
function probe(mixed $value): void {
    try { \array_values($value); echo "ok,"; } catch (\TypeError $e) { echo str_starts_with($e->getMessage(), 'array_values(): Argument #1 ($array) must be of type array') ? "V," : "?,"; }
    try { ARRAY_FLIP($value); echo "ok,"; } catch (\TypeError $e) { echo str_starts_with($e->getMessage(), 'array_flip(): Argument #1 ($array) must be of type array') ? "F," : "?,"; }
    try { In_Array(1, $value); echo "ok|"; } catch (\TypeError $e) { echo str_starts_with($e->getMessage(), 'in_array(): Argument #2 ($haystack) must be of type array') ? "I|" : "?|"; }
}
foreach ([5, "s", null, true, false, 1.5, ["z"]] as $value) { probe($value); }
$u = maybe($argc);
echo json_encode(array_values($u)), json_encode(array_flip($u)), in_array("y", $u, true) ? "y" : "n", "|";
probe(maybe($argc - 1));
$values = array_values(...);
$flip = 'array_flip';
echo json_encode($values(json_decode("{\"p\":1,\"q\":2}", true))), json_encode($flip(json_decode("[\"m\"]")));
"#,
    );
    assert_eq!(
        out,
        r#"V,F,I|V,F,I|V,F,I|V,F,I|V,F,I|V,F,I|ok,ok,ok|["x","y"]{"x":"a","y":"b"}y|V,F,I|[1,2]{"m":0}"#
    );
}

/// Results, boxed membership scans and the non-array `TypeError` paths release every
/// allocation across repeated calls with `mixed` receivers.
#[test]
fn test_mixed_receiver_builtins_heap_clean() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function run(mixed $list, mixed $assoc, mixed $nested, mixed $bad, $strict): string {
    $out = count(array_values($list)) . ":" . implode("/", array_values($assoc));
    $out .= ":" . implode("/", array_keys(array_flip($list))) . ":" . array_flip($assoc)["v"];
    $out .= ":" . count(array_values($nested)) . ":";
    $out .= in_array("b", $list) ? "y" : "n";
    $out .= in_array(["x" => 1], $nested) ? "y" : "n";
    $out .= in_array(["x" => "1"], $nested, $strict) ? "y" : "n";
    try { array_values($bad); } catch (TypeError $e) { $out .= "V"; }
    try { array_flip($bad); } catch (TypeError $e) { $out .= "F"; }
    try { in_array(1, $bad, true); } catch (TypeError $e) { $out .= "I"; }
    return $out;
}
for ($i = 0; $i < 3; $i++) {
    $list = ["a", "b", str_repeat("c", $i + 1)];
    $assoc = ["k" => "v", "n" => "w" . $i];
    $nested = [[1, 2], ["x" => 1], "s" . $i];
    echo run($list, $assoc, $nested, $i === 1 ? "s" . $i : $i, $i === 2), ",";
}
"#,
    );
    assert!(output.success, "stdout={:?}\nstderr={}", output.stdout, output.stderr);
    assert_eq!(
        output.stdout,
        "3:v/w0:a/b/c:k:3:yyyVFI,3:v/w1:a/b/cc:k:3:yyyVFI,3:v/w2:a/b/ccc:k:3:yynVFI,",
        "{}",
        output.stderr
    );
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        output.stderr
    );
}

/// `array_slice()` over a `mixed` receiver slices a hash payload instead of answering an empty
/// array: string keys survive and integer keys are renumbered, while a list is sliced as before
/// (issue #1348). Expected output is verbatim PHP 8.5.10.
#[test]
fn test_array_slice_on_mixed_receiver_shapes() {
    let out = compile_and_run(
        r#"<?php
function pick(): mixed { return ["a" => 1, "b" => 2, "c" => 3]; }
function show(mixed $v): void { echo json_encode(array_slice($v, 1, 2)), "|"; }
echo count(array_slice(pick(), 1, 2)), "|";
show(["a" => 1, "b" => 2, "c" => 3]);
show([5 => "p", 9 => "q", 12 => "r"]);
show(["x" => 1, 7 => 2, "y" => 3]);
show([10, 20, 30]);
show([]);
var_dump(array_slice(pick(), -1));
"#,
    );
    assert_eq!(
        out,
        concat!(
            r#"2|{"b":2,"c":3}|["q","r"]|{"0":2,"y":3}|[20,30]|[]|"#,
            "array(1) {\n  [\"c\"]=>\n  int(3)\n}\n",
        )
    );
}

/// A declared `array` parameter holding a hash is sliced by its runtime storage as well: the
/// parameter is a boxed PHP array, so it took the same list-only path and answered `[]`.
#[test]
fn test_array_slice_on_declared_array_parameter_holding_a_hash() {
    let out = compile_and_run(
        r#"<?php
function tail(array $a): array { return array_slice($a, 1); }
print_r(tail(["k" => "v", "m" => [1, 2], "n" => 3.5]));
print_r(tail([4, 5, 6]));
foreach (tail(["p" => "P", "q" => "Q", "r" => "R"]) as $key => $value) { echo $key, "=", $value, ";"; }
echo "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "Array\n(\n    [m] => Array\n        (\n            [0] => 1\n            [1] => 2\n        )\n\n    [n] => 3.5\n)\n",
            "Array\n(\n    [0] => 5\n    [1] => 6\n)\n",
            "q=Q;r=R;\n",
        )
    );
}

/// An untyped parameter that call-site specialization types as a hash still reaches EIR as a
/// boxed value, so its slice takes the boxed path too instead of the backend refusal
/// `array_slice result PHP type AssocArray`.
#[test]
fn test_array_slice_on_specialized_untyped_parameter_holding_a_hash() {
    let out = compile_and_run(
        r#"<?php
function top($scores) { return array_slice($scores, 0, 2); }
for ($i = 0; $i < 2; $i++) {
    echo json_encode(top(["a" => 1 + $i, "b" => 2, "c" => 3])), "|";
}
"#,
    );
    assert_eq!(out, r#"{"a":1,"b":2}|{"a":2,"b":2}|"#);
}

/// A key-preserving slice of a boxed payload through the callable wrapper owns its elements.
///
/// `call_user_func_array(array_slice(...), $args)` passes `$preserve_keys` at run time, and a
/// list payload is then sliced into a hash by `__rt_array_slice_to_hash`. On x86_64 that helper
/// handed its retain the wrong register, so the result shared its arrays with the source without
/// owning them. Hash payloads go through `__rt_hash_slice` with the same flag. Expected output
/// is verbatim PHP 8.5.10.
#[test]
fn test_callable_array_slice_keeping_keys_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$f = array_slice(...);
for ($i = 0; $i < 3; $i++) {
    $rows = [["id" => $i], ["id" => $i + 1], "tail" . $i];
    foreach ([true, false] as $keep) {
        $args = [$rows, 1, null, $keep];
        $slice = call_user_func_array($f, $args);
        echo json_encode($slice), "|";
    }
    $args = [["k" => [$i], 4 => "v" . $i, 8 => [$i, $i]], 1, 2, true];
    echo json_encode(call_user_func_array($f, $args)), "|";
}
"#,
    );
    assert_eq!(
        out.stdout,
        concat!(
            r#"{"1":{"id":1},"2":"tail0"}|[{"id":1},"tail0"]|{"4":"v0","8":[0,0]}|"#,
            r#"{"1":{"id":2},"2":"tail1"}|[{"id":2},"tail1"]|{"4":"v1","8":[1,1]}|"#,
            r#"{"1":{"id":3},"2":"tail2"}|[{"id":3},"tail2"]|{"4":"v2","8":[2,2]}|"#,
        )
    );
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}

/// Slicing boxed hash and list payloads in a loop releases every intermediate array and box.
#[test]
fn test_array_slice_on_mixed_receiver_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function pick(mixed $v): mixed { return $v; }
for ($i = 0; $i < 3; $i++) {
    $slice = array_slice(pick(["a" => "x" . $i, "b" => [$i], 3 => "z"]), 1);
    echo count($slice), json_encode($slice), "|";
    $list = array_slice(pick(["s" . $i, "t", "u"]), 1, 1);
    echo json_encode($list), "|";
}
"#,
    );
    assert_eq!(
        out.stdout,
        r#"2{"b":[0],"0":"z"}|["t"]|2{"b":[1],"0":"z"}|["t"]|2{"b":[2],"0":"z"}|["t"]|"#
    );
    assert!(out.stderr.contains("leak summary: clean"), "{}", out.stderr);
}
