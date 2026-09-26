//! Purpose:
//! Regression coverage for `array_values()`, `array_flip()` and `in_array()` over a receiver
//! whose static type is `mixed` (issue #630).
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
