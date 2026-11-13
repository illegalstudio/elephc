//! Purpose:
//! Verifies shared boxed operator graphs across scalar and native XML operands.
//!
//! Called from:
//! - The managed-libxml2 SimpleXML codegen suite.
//!
//! Key details:
//! - Both operand orders, scalar fallbacks and exceptional exits retain heap ownership.
//! - Opaque eval calls the same compiled arithmetic functions as native PHP code.

use crate::support::*;

/// Exercises native handlers and scalar fallbacks with independent owner diagnostics.
fn assert_shared_boxed_operator_graphs() {
    if skip_without_xml_native("test_simplexml_shared_boxed_operator_graphs_bundle") { return; }
    let source = r#"<?php
set_error_handler(function (int $level, string $message): bool { return true; });
function sharedXmlAdd(mixed $left, mixed $right): mixed { return $left + $right; }
function sharedXmlSub(mixed $left, mixed $right): mixed { return $left - $right; }
function sharedXmlMul(mixed $left, mixed $right): mixed { return $left * $right; }
function sharedXmlDiv(mixed $left, mixed $right): mixed { return $left / $right; }
function sharedXmlMod(mixed $left, mixed $right): mixed { return $left % $right; }
function sharedXmlPow(mixed $left, mixed $right): mixed { return $left ** $right; }
function sharedXmlAnd(mixed $left, mixed $right): mixed { return $left & $right; }
function sharedXmlShift(mixed $left, mixed $right): mixed { return $left << $right; }
function sharedXmlIntRight(mixed $left, int $right): mixed { return $left + $right; }
function sharedXmlStringLeft(string $left, mixed $right): mixed { return $left + $right; }
function sharedXmlFloatRight(mixed $left, float $right): mixed { return $left + $right; }
function sharedXmlIncrement(mixed $value): mixed { return ++$value; }
function sharedXmlDecrement(mixed $value): mixed { return --$value; }
function sharedXmlPost(mixed $value): mixed { $old = $value++; echo $value, '|'; return $old; }
function sharedXmlLess(mixed $left, mixed $right): bool { return $left < $right; }
function sharedXmlEqual(mixed $left, mixed $right): bool { return $left == $right; }
function sharedXmlCompare(mixed $left, mixed $right): int { return $left <=> $right; }
function sharedXmlOrder(int $value): mixed { echo $value, '|'; return $value; }
$node = new SimpleXMLElement('<r>6</r>');
echo sharedXmlAdd($node, 2), ':', sharedXmlAdd(2, $node), ':', sharedXmlAdd('6', 2), '|';
echo sharedXmlSub($node, 2), ':', sharedXmlMul(2, $node), '|';
echo sharedXmlDiv($node, 2), ':', sharedXmlMod($node, 4), ':', sharedXmlPow($node, 2), '|';
echo sharedXmlAnd($node, 3), ':', sharedXmlShift($node, 1), '|';
echo sharedXmlIntRight($node, 2), ':', sharedXmlStringLeft(str_repeat('2', 1), $node),
    ':', sharedXmlFloatRight($node, 0.5), '|';
echo sharedXmlIncrement('az'), ':', sharedXmlDecrement('9'), '|';
echo sharedXmlPost('az'), '|';
echo sharedXmlLess($node, 8), ':', sharedXmlEqual($node, 6), ':', sharedXmlCompare($node, 5), '|';
$shared = [1]; $alias = $shared;
echo json_encode($shared), ':', json_encode($alias), ':', (int)sharedXmlEqual($shared, $alias), '|';
echo is_float(sharedXmlAdd(PHP_INT_MAX, 1)), '|';
echo sharedXmlAdd(sharedXmlOrder(1), sharedXmlOrder(2)), '|';
try { sharedXmlDiv($node, $argc - 1); } catch (DivisionByZeroError $error) { echo 'caught|'; }
echo sharedXmlAdd($node, 3), '|';
$code = $argc > 0 ? 'echo sharedXmlAdd($node, 4), ":", sharedXmlLess($node, 9), "|";' : '';
eval($code);
$hiddenName = $argc > 0 ? '__eir_boxed_operator@add' : '';
echo (int)function_exists($hiddenName), ':', (int)is_callable($hiddenName), '|';
eval('echo (int)function_exists($hiddenName), ":", (int)is_callable($hiddenName), "|";');
restore_error_handler();
unset($node, $code, $error, $hiddenName, $shared, $alias);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "8:8:8|4:12|3:2:36|2:12|8:8:6.5|ba:8|ba|az|1:1:1|[1]:[1]:1|1|1|2|3|caught|9|10:1|0:0|0:0|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Shared graphs preserve native handlers, scalar/array behavior and catchable failures.
#[test]
fn test_simplexml_shared_boxed_operator_graphs_bundle() {
    assert_shared_boxed_operator_graphs();
}

/// Unoptimized shared graphs retire temporary boxes and thrown-call owners independently.
#[test]
fn test_simplexml_shared_boxed_operator_graphs_unoptimized_bundle() {
    without_ir_opt(assert_shared_boxed_operator_graphs);
}
