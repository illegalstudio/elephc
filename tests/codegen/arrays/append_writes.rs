//! Purpose:
//! Regressions for write-only empty dimensions in assignments and updates.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Append uses the runtime next-index operation rather than a guessed numeric key.
//! - Expression results and receiver evaluation must preserve PHP source order.

use crate::support::{compile_and_run_with_heap_debug, without_ir_opt};

/// Prefix decrement of an append's property appends null before a catchable property Error.
#[test]
fn test_append_oct9_property_prefix_decrement() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
try { --$items[]->x; } catch (Error $error) { echo $error->getMessage(), '|'; }
try { echo --$items[]->x->y; } catch (Error $error) { echo $error->getMessage(), '|'; }
echo json_encode($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Attempt to increment/decrement property \"x\" on null|Attempt to modify property \"x\" on null|[null,null]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Provably zero keys preserve caller references when their new value is already boxed Mixed.
#[test]
fn test_append_oct9_variadic_zero_mixed_write() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function integer(&...$items): void { $items[0] = $items[1]; echo $items[0], '|'; }
function boolean(&...$items): void { $items[false] = $items[1]; echo $items[0], '|'; }
function floating(&...$items): void { $alias =& $items; $alias[0.0] = $alias[1]; echo $items[0], '|'; }
$a = 'A'; $b = 'B'; integer($a, $b); echo $a, ':', $b, ';';
$a = 'A'; boolean($a, $b); echo $a, ':', $b, ';';
$a = 'A'; floating($a, $b); echo $a, ':', $b;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "B|B:B;B|B:B;B|B:B");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// JSON reads scalar, heap and boxed caller values without consuming their variadic references.
#[test]
fn test_append_latest_variadic_json_value_types() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function dump(&...$items): void {
    $alias =& $items;
    echo json_encode($alias), '|';
    $alias[9] = true;
    echo json_encode($items);
}
function boxed(): mixed { return str_repeat('m', 24); }
$a = 123; $b = str_repeat('b', 4); $c = 2.5; $d = true;
$e = [1, 2]; $f = (object)['x' => 1]; $g = boxed();
dump($a, $b, $c, $d, $e, $f, $g);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("[123,\"bbbb\",2.5,true,[1,2],{{\"x\":1}},\"{}\"]|{{\"0\":123,\"1\":\"bbbb\",\"2\":2.5,\"3\":true,\"4\":[1,2],\"5\":{{\"x\":1}},\"6\":\"{}\",\"9\":true}}", "m".repeat(24), "m".repeat(24)));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Failed property writes retire a heap RHS after the append and before entering the catch.
#[test]
fn test_append_latest_property_rhs_destructor() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function size(): int { global $items; return count($items); }
class DropValue { public function __destruct() { echo 'd', size(), ':'; } }
$items = [];
try { $items[]->x = new DropValue(); }
catch (Error $error) { echo 'caught', count($items); }
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "d1:caught1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Effectful base indices precede property selectors and RHS while nonnull assignment results stay valid.
#[test]
fn test_append_latest_property_receiver_order_control() {
    let out = compile_and_run_with_heap_debug(r#"<?php
namespace AppendControl;
function key(): int { echo 'k'; return 0; }
function name(): string { echo 'n'; return 'x'; }
function rhs(): int { echo 'r'; return 1; }
$items = [[]];
try { $items[key()][]->{name()} += rhs(); }
catch (\Error $error) { echo ':', $error->getMessage(), '|'; }
echo json_encode($items), '|';
class Box { public int $x = 0; }
$objects = [];
($objects[] = new Box())->x = 7;
echo $objects[0]->x;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "knr:Attempt to assign property \"x\" on null|[[null]]|7");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The new write-through and null-property failure paths do not rely on EIR optimization.
#[test]
fn test_append_latest_without_ir_opt() {
    let out = without_ir_opt(|| compile_and_run_with_heap_debug(r#"<?php
function update(&...$items): void { $alias =& $items; $alias[1] .= '-g'; $alias[-1] = [7]; echo count($alias), '|'; }
$a = 'A'; $b = 'B'; update($a, $b); echo $a, ':', $b, '|';
$items = [];
try { $items[]->x++; } catch (Error $error) { echo $error->getMessage(), '|'; }
echo json_encode($items);
"#));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3|A:B-g|Attempt to increment/decrement property \"x\" on null|[null]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Aliases of variadic reference arrays still write through the original caller slots.
#[test]
fn test_append_latest_variadic_reference_alias() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function update(&...$items): void { $alias =& $items; $alias[1] .= '-g'; echo $alias[1], '|'; }
$a = 'A'; $b = 'B'; update($a, $b); echo $a, ':', $b, '|';
$call = update(...); $call($a, $b); echo $a, ':', $b;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "B-g|A:B-g|B-g-g|A:B-g-g");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Negative and sparse variadic keys add only their own entries without detaching caller references.
#[test]
fn test_append_latest_variadic_sparse_keys() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function negative(&...$items): void {
    $items[-1] = 'x'; echo json_encode($items), ':', json_encode(array_keys($items)), ':', count($items), ':', $items[0], $items[1], ':', $items[-1], '|';
    $items[1] .= '-n';
}
function sparse(&...$items): void {
    $items[5] = [7]; echo json_encode($items), ':', json_encode(array_keys($items)), ':', count($items), ':', $items[0], $items[1], ':', json_encode($items[5]), '|';
    $items[1] .= '-s';
}
$a = 'A'; $b = 'B'; negative($a, $b); sparse($a, $b); echo $a, ':', $b;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "{\"0\":\"A\",\"1\":\"B\",\"-1\":\"x\"}:[0,1,-1]:3:AB:x|{\"0\":\"A\",\"1\":\"B-n\",\"5\":[7]}:[0,1,5]:3:AB-n:[7]|A:B-n-s");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Assignments, postfix updates and nested prefixes append null before catchable property failures.
#[test]
fn test_append_latest_property_write_forms() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
try { $items[]->x = 1; } catch (Error $error) { echo $error->getMessage(), '|'; }
try { $items[]->x++; } catch (Error $error) { echo $error->getMessage(), '|'; }
try { ++$items[]->x->y; } catch (Error $error) { echo $error->getMessage(), '|'; }
try { $items[]->x->y = 1; } catch (Error $error) { echo $error->getMessage(), '|'; }
echo json_encode($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Attempt to assign property \"x\" on null|Attempt to increment/decrement property \"x\" on null|Attempt to modify property \"x\" on null|Attempt to modify property \"x\" on null|[null,null,null,null]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Computed selectors and assignment RHS observe the pre-append array and run exactly once.
#[test]
fn test_append_latest_property_effect_order() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function selector(array $items, string $name): string { echo $name, count($items); return $name; }
function rhs(array $items): int { echo 'r', count($items); return 1; }
$items = [];
try { $items[]->{selector($items, 'x')} = rhs($items); }
catch (Error $error) { echo ':', $error->getMessage(), '|'; }
try { ++$items[]->{selector($items, 'x')}->{selector($items, 'y')}; }
catch (Error $error) { echo ':', $error->getMessage(), '|'; }
echo json_encode($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "x0r0:Attempt to assign property \"x\" on null|x1y1:Attempt to modify property \"x\" on null|[null,null]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Sparse heap keys and catchable append-property errors also work without EIR optimization.
#[test]
fn test_append_followup_without_ir_optimization() {
    let out = without_ir_opt(|| compile_and_run_with_heap_debug(r#"<?php
$index = $argc;
$items = []; $items[][$index] = [7];
echo json_encode($items), ':';
$properties = [];
try { ++$properties[]->x; echo 'bad'; }
catch (Error $error) { echo $error->getMessage(), ':'; }
echo json_encode($properties);
"#));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[{\"1\":[7]}]:Attempt to increment/decrement property \"x\" on null:[null]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A sparse variable key stores a heap value without manufacturing an invalid hole.
#[test]
fn test_append_followup_variable_heap_dimension() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$index = $argc;
$items = [];
$items[][$index] = [7];
echo 'Z', json_encode($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Z[{\"1\":[7]}]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Direct empty-array writes share the safe sparse storage used by nested append preludes.
#[test]
fn test_append_followup_direct_variable_heap_key() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$index = $argc;
$items = [];
$items[$index] = [7];
echo json_encode($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "{\"1\":[7]}");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Literal non-integer syntax retains sparse and null-key normalization inside new buckets.
#[test]
fn test_append_followup_static_dimension_shapes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
$items[][-1] = 'n';
$items[][1.5] = 'f';
$items[][true] = 'b';
$items[][null] = 'z';
echo json_encode($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[{\"-1\":\"n\"},{\"1\":\"f\"},{\"1\":\"b\"},{\"\":\"z\"}]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A suffix belongs to the increment target, not to an already incremented append value.
#[test]
fn test_append_followup_increment_property_suffix() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
try { ++$items[]->x; echo 'bad'; }
catch (Error $error) { echo $error->getMessage(), ':'; }
echo json_encode($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Attempt to increment/decrement property \"x\" on null:[null]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Unknown zero and negative keys preserve JSON shape without unsafe typed holes.
#[test]
fn test_append_followup_dynamic_key_shapes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$zero = $argc - 1;
$negative = -$argc;
$a = []; $a[$zero] = [7];
$b = []; $b[$negative] = [8];
echo json_encode($a), '|', json_encode($b);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[[7]]|{\"-1\":[8]}");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A computed property name runs once after the fresh append and before its Error.
#[test]
fn test_append_followup_increment_dynamic_property_suffix() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function property_name(): string { echo 'p'; return 'x'; }
$items = [];
try { ++$items[]->{property_name()}; echo 'bad'; }
catch (Error $error) { echo $error->getMessage(), ':'; }
echo json_encode($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "pAttempt to increment/decrement property \"x\" on null:[null]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Appending the first null creates materializable boxed storage and keeps value aliases intact.
#[test]
fn test_append_followup_first_null_preserves_alias() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = []; $copy = $items;
$items[] = null;
echo json_encode($items), '|', json_encode($copy);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[null]|[]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Synthetic nested-append preludes must pass through the ordinary statement checker.
#[test]
fn test_append_review_existing_index_checker() {
    for (source, expected) in [
        ("$items = [[]]; $items[0][] += 5; echo json_encode($items);", "[[5]]"),
        ("$items = [[]]; $items[0][]['k'] = 'v'; echo json_encode($items);", "[[{\"k\":\"v\"}]]"),
        ("class Box { public array $items = [[]]; } $box = new Box(); $box->items[0][] += 3; echo json_encode($box->items);", "[[3]]"),
        ("class Box { public static array $items = [[]]; } Box::$items[0][] += 3; echo json_encode(Box::$items);", "[[3]]"),
    ] {
        let out = compile_and_run_with_heap_debug(&format!("<?php {source}"));
        assert!(out.success, "{source}: {}", out.stderr);
        assert_eq!(out.stdout, expected, "{source}");
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{source}: {}", out.stderr);
    }
}

/// String-keyed missing buckets retain dynamic array storage through append write-back.
#[test]
fn test_append_review_string_bucket_autovivification() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$groups = [];
$key = $argc > 1 ? 'other' : 'k';
$groups[$key][] = 1;
$groups[$key][] = 2;
$groups['j'][] += 4;
echo count($groups), ':', count($groups[$key]), ':', $groups[$key][0],
    $groups[$key][1], ':', count($groups['j']), ':', $groups['j'][0];
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:2:12:1:4");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Separates constructor and nested-array ownership from the new append expression path.
#[test]
fn test_append_review_nested_property_ownership_control() {
    for source in [
        "class Box { public array $items = [[]]; } $box = new Box(); echo json_encode($box->items);",
        "class Box { public array $items = [[]]; } $box = new Box(); $box->items[0][] = 3; echo json_encode($box->items);",
        "class Box { public static array $items = [[]]; } echo json_encode(Box::$items);",
    ] {
        let out = compile_and_run_with_heap_debug(&format!("<?php {source}"));
        assert!(out.success, "{source}: {}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{source}: {}", out.stderr);
    }
}

/// Sparse literal integer dimensions must not introduce a missing key zero.
#[test]
fn test_append_review_sparse_integer_dimensions() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
$items[][1] = 'x';
$items[][1] .= 'y';
echo json_encode($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[{\"1\":\"x\"},{\"1\":\"y\"}]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Prefix increments append a fresh one and return the updated value.
#[test]
fn test_append_review_prefix_increment() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
++ $items[];
echo ++$items[], ':', ++$items[]['k'], ':';
class Box { public array $items = []; public static array $shared = []; }
$box = new Box();
++ $box->items[];
++ Box::$shared[];
echo ++$box->items[], ':', ++Box::$shared[], ':';
echo json_encode($items), ':', json_encode($box->items), ':', json_encode(Box::$shared);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:1:1:1:[1,1,{\"k\":1}]:[1,1]:[1,1]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The read and write of a compound append share one float-key conversion diagnostic.
#[test]
fn test_append_review_float_dimension_warns_once() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
$key = 1.5;
$warnings = 0;
set_error_handler(function ($code, $message) use (&$warnings) {
    $warnings++; return true;
});
$items[][$key] .= 'x';
$before = $warnings;
$numbers = [];
echo ++$numbers[][$key], ':';
restore_error_handler();
echo $before, ':', $warnings - $before, ':', $items[0][1];
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:2:2:x");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Numeric reads from fresh nested containers yield null rather than a Never operand.
#[test]
fn test_append_review_numeric_missing_leaf() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
$value = ++$items[][1];
$compound = [];
$sum = ($compound[][1] += 2);
echo $value, ':', $sum, ':', json_encode($items), ':', json_encode($compound);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:2:[{\"1\":1}]:[{\"1\":2}]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Statement and expression compound appends create a fresh element, not an array read.
#[test]
fn test_append_review_compound_and_assignment_values() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [1];
$items[] += 2;
echo ($items[] = 3), ':', ($items[] += 4), ':';
foreach ($items as $value) { echo $value, ','; }
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "3:4:1,2,3,4,");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested empty dimensions construct distinct containers and preserve expression results.
#[test]
fn test_append_review_nested_dimensions() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
$items[]['k'] = 'x';
echo ($items[]['k'] .= 'y'), ':';
$items[][] = 1;
echo $items[0]['k'], ':', $items[1]['k'], ':', $items[2][0];
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "y:x:y:1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Local, object-property and static-property appends share fresh-null compound semantics.
#[test]
fn test_append_review_property_receivers() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Box { public array $items = []; public static array $shared = []; }
$box = new Box();
$box->items[] += 2;
Box::$shared[] += 3;
echo $box->items[0], ':', Box::$shared[0];
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:3");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Post-increment of a fresh append yields null and stores one.
#[test]
fn test_append_review_post_increment() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
$items[]++;
echo ($items[]++), ':', $items[0], ':', $items[1];
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, ":1:1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Arrow functions capture the append receiver by value even when it lives in a prelude.
#[test]
fn test_append_review_arrow_capture() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Box { public array $items = [1]; }
$box = new Box();
$append = fn() => ($box->items[] += 2);
echo $append(), ':', count($box->items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:2");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Runtime-unknown RHS values have the same append behavior with EIR optimization disabled.
#[test]
fn test_append_review_without_ir_optimization() {
    let out = without_ir_opt(|| compile_and_run_with_heap_debug(r#"<?php
$items = [];
echo ($items[] += $argc), ':', $items[0], ':';
$items[][] = $argc;
echo $items[1][0];
"#));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:1:1");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A warning handler cannot redirect the write away from the key its compound read diagnosed.
#[test]
fn test_append_review_warning_handler_key_snapshot() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$key = 'a';
$items = [];
set_error_handler(function ($code, $message) use (&$key) {
    $key = 'b'; echo 'warn:'; return true;
});
$items[][$key] .= 'x';
restore_error_handler();
foreach ($items[0] as $name => $value) { echo $name, ':', $value; }
echo ':', $key;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "warn:a:x:b");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// RHS effects precede append and the runtime retains deleted-key history.
#[test]
fn test_append_review_rhs_order_and_key_history() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [];
$items[] += ($items[] = 7);
echo $items[0], ':', $items[1], ':';
$assoc = ['seed' => 1, 9 => 2];
unset($assoc[9]);
echo ($assoc[] += 3), ':', $assoc[10];
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "7:7:3:3");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
