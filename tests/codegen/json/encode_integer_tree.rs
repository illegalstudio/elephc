//! Purpose:
//! Covers bounded JSON encoding for large indexed-array trees of integers.
//! Exercises the fast path, its ownership handoff, and conservative fallbacks end to end.
//!
//! Called from:
//! - The JSON codegen suite through `tests/codegen/json.rs`.
//!
//! Key details:
//! - Each semantic fixture runs with EIR optimization enabled and disabled.
//! - Heap-debug runs verify that the transient result is adopted and released exactly once.

use super::*;

/// Runs one JSON fixture under both optimizer modes and heap-debug ownership checks.
fn assert_json_integer_tree_modes(source: &str, expected: &str) {
    for ir_opt_off in [false, true] {
        let run = || {
            assert_eq!(compile_and_run(source), expected);
            let output = compile_and_run_with_heap_debug(source);
            assert!(output.success, "{}", output.stderr);
            assert_eq!(output.stdout, expected, "{}", output.stderr);
            assert!(output.stderr.contains("HEAP DEBUG:"), "heap debug did not run");
        };
        if ir_opt_off {
            without_ir_opt(run);
        } else {
            run();
        }
    }
}

/// A decoded integer tree larger than the shared 64 KiB scratch buffer encodes exactly.
#[test]
fn test_json_encode_large_decoded_integer_tree_is_bounded() {
    let source = r#"<?php
$row = '[' . str_repeat('123456,', 100) . '0]';
$json = '[' . str_repeat($row . ',', 120) . '[]]';
$value = json_decode($json, true);
$result = '';
for ($i = 0; $i < 3; $i++) {
    $encoded = json_encode($value);
    $result .= strlen($encoded) . ':' . ($encoded === $json ? 'same' : 'bad') . '|';
    unset($encoded);
}
$flat = range(0, 19999);
$flatJson = json_encode($flat);
$safe = array_fill(0, 3120, PHP_INT_MIN);
$overflow = array_fill(0, 3121, PHP_INT_MIN);
echo strlen($json), '|', $result, json_last_error(), '|', strlen($flatJson), ':';
echo substr($flatJson, 0, 8), ':', substr($flatJson, -8), '|';
echo strlen(json_encode($safe)), ':', strlen(json_encode($overflow)), ':', json_last_error();
"#;
    assert_json_integer_tree_modes(
        source,
        "84484|84484:same|84484:same|84484:same|0|108891:[0,1,2,3:8,19999]|65521:65542:0",
    );
}

/// Integer boundaries, aliases, references, flags, and depth errors retain PHP behavior.
#[test]
fn test_json_encode_integer_tree_boundaries_and_fallbacks() {
    let source = r#"<?php
$powers = [];
$power = 1;
for ($i = 0; $i < 19; $i++) {
    $powers[] = $power - 1;
    $powers[] = $power;
    $powers[] = -$power;
    $power *= 10;
}
echo json_encode($powers), "\n";
$powerJson = json_encode($powers);
$largePowers = [];
for ($repeat = 0; $repeat < 55; $repeat++) {
    foreach ($powers as $value) {
        $largePowers[] = $value;
    }
}
$powerBody = substr($powerJson, 1, -1);
$expectedPowers = '[' . str_repeat($powerBody . ',', 54) . $powerBody . ']';
$largePowerJson = json_encode($largePowers);
echo count($largePowers), ':', strlen($largePowerJson), ':';
echo $largePowerJson === $expectedPowers ? "same\n" : "bad\n";
$min = -PHP_INT_MAX - 1;
$values = [$min, PHP_INT_MAX, -1, 0, 1, [], [2, -3]];
echo json_encode($values), "\n";
$alias = $values;
$values[0] = 7;
echo json_encode($alias), "|", json_encode($values), "\n";
$ref = [1];
$slot =& $ref[0];
$before = json_encode($ref);
$slot = 2;
echo $before, "|", json_encode($ref), "\n";
echo json_encode([1, 2], JSON_FORCE_OBJECT), "\n";
echo json_encode([[1]], 0, 1) === false ? 'false:' . json_last_error() : 'bad';
"#;
    assert_json_integer_tree_modes(
        source,
        "[0,1,-1,9,10,-10,99,100,-100,999,1000,-1000,9999,10000,-10000,99999,100000,-100000,999999,1000000,-1000000,9999999,10000000,-10000000,99999999,100000000,-100000000,999999999,1000000000,-1000000000,9999999999,10000000000,-10000000000,99999999999,100000000000,-100000000000,999999999999,1000000000000,-1000000000000,9999999999999,10000000000000,-10000000000000,99999999999999,100000000000000,-100000000000000,999999999999999,1000000000000000,-1000000000000000,9999999999999999,10000000000000000,-10000000000000000,99999999999999999,100000000000000000,-100000000000000000,999999999999999999,1000000000000000000,-1000000000000000000]\n3135:34541:same\n[-9223372036854775808,9223372036854775807,-1,0,1,[],[2,-3]]\n[-9223372036854775808,9223372036854775807,-1,0,1,[],[2,-3]]|[7,9223372036854775807,-1,0,1,[],[2,-3]]\n[1]|[2]\n{\"0\":1,\"1\":2}\nfalse:1",
    );
}
