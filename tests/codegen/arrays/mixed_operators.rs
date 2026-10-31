//! Purpose:
//! Regression coverage for PHP's `+`, `-` and `*` over operands whose static type is `mixed` and
//! that hold arrays at run time: `array + array` is the union, any other array operand raises
//! `TypeError: Unsupported operand types`.
//!
//! Called from:
//! - The native codegen suite's array module.
//!
//! Key details:
//! - Expected output was produced by PHP 8.5 from the same source, operands passed in variables.
//! - The union fixtures mix element storage (ints beside strings, lists beside maps) because a
//!   `mixed` operand's array may store any element type; the operands must stay unchanged.
//! - A throwing operator inside `try`, and one whose result is unused, must reach the `catch`.

use crate::support::compile_and_run_with_heap_debug;

/// `+` on two `mixed` arrays is PHP's union for every pairing of lists and maps, and leaves
/// both operands as they were.
#[test]
fn test_mixed_plus_unites_two_arrays() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function plus(mixed $a, mixed $b): mixed { return $a + $b; }
for ($i = 0; $i < 3; $i++) {
    $list = [1, 2];
    $words = ["a", "b", "c" . $i];
    $map = ["k" => 1, "n" => "w" . $i];
    $other = ["k" => "no", "z" => [$i]];
    echo json_encode(plus($map, $other)), json_encode(plus($list, [5, 6, 7])), json_encode(plus($list, $words)),
        json_encode(plus($map, $list)), json_encode(plus($list, $map)), json_encode(plus([], $words)),
        json_encode($list), json_encode($map), "\n";
}
"#,
    );
    assert!(output.success, "stdout={:?}\nstderr={}", output.stdout, output.stderr);
    let line = |i: u32| {
        format!(
            "{{\"k\":1,\"n\":\"w{i}\",\"z\":[{i}]}}[1,2,7][1,2,\"c{i}\"]{{\"k\":1,\"n\":\"w{i}\",\"0\":1,\"1\":2}}\
             {{\"0\":1,\"1\":2,\"k\":1,\"n\":\"w{i}\"}}[\"a\",\"b\",\"c{i}\"][1,2]{{\"k\":1,\"n\":\"w{i}\"}}\n"
        )
    };
    assert_eq!(output.stdout, format!("{}{}{}", line(0), line(1), line(2)), "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// An array beside any other operand, or two arrays under `-` or `*`, raises PHP's `TypeError`
/// naming both operands; numeric `mixed` arithmetic keeps its results, overflow included.
#[test]
fn test_mixed_operator_refuses_an_array_beside_another_operand() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
final class Point {}
function plus(mixed $a, mixed $b): mixed { return $a + $b; }
function minus(mixed $a, mixed $b): mixed { return $a - $b; }
function times(mixed $a, mixed $b): mixed { return $a * $b; }
function plusOne(mixed $a): mixed { return $a + 1; }
$cases = [
    fn () => plus([1], 2), fn () => plus(2, [1]), fn () => minus([1], [2]), fn () => times([1], 1.5),
    fn () => times("s", [1]), fn () => plus([1], new Point()), fn () => minus(null, ["k" => 1]),
    fn () => plus(true, [1]), fn () => plusOne([1]), fn () => plusOne(["k" => 1]),
];
foreach ($cases as $case) {
    try { $case(); echo "no error\n"; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
echo json_encode(plus(2, 3)), json_encode(plus(1.5, 2)), json_encode(times("3", 4)), json_encode(plusOne(PHP_INT_MAX)), "\n";
"#,
    );
    assert!(output.success, "stdout={:?}\nstderr={}", output.stdout, output.stderr);
    assert_eq!(
        output.stdout,
        "Unsupported operand types: array + int\nUnsupported operand types: int + array\n\
         Unsupported operand types: array - array\nUnsupported operand types: array * float\n\
         Unsupported operand types: string * array\nUnsupported operand types: array + Point\n\
         Unsupported operand types: null - array\nUnsupported operand types: bool + array\n\
         Unsupported operand types: array + int\nUnsupported operand types: array + int\n\
         53.5129.223372036854776e+18\n",
        "{}",
        output.stderr
    );
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// The `TypeError` reaches a `try` around a direct call, also when the call's result is unused,
/// so the optimizer may neither drop the call nor the `catch`.
#[test]
fn test_mixed_operator_type_error_reaches_the_callers_catch() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
function u(mixed $a, mixed $b): mixed { return $a + $b; }
try { u([1], 2); echo "A: no error\n"; } catch (TypeError $e) { echo "A: ", $e->getMessage(), "\n"; }
try { $r = u([1], 2); echo "B: no error ", json_encode($r), "\n"; } catch (TypeError $e) { echo "B: ", $e->getMessage(), "\n"; }
try { echo json_encode(u([1], 2)), "\n"; } catch (TypeError $e) { echo "C: ", $e->getMessage(), "\n"; }
"#,
    );
    assert!(output.success, "stdout={:?}\nstderr={}", output.stdout, output.stderr);
    assert_eq!(
        output.stdout,
        "A: Unsupported operand types: array + int\nB: Unsupported operand types: array + int\n\
         C: Unsupported operand types: array + int\n",
        "{}",
        output.stderr
    );
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
