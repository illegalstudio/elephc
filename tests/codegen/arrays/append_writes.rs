//! Purpose:
//! Regressions for write-only empty dimensions in assignments and updates.
//!
//! Called from:
//! - The codegen integration test harness.
//!
//! Key details:
//! - Append uses the runtime next-index operation rather than a guessed numeric key.
//! - Expression results and receiver evaluation must preserve PHP source order.

use crate::support::{compile_and_run, compile_and_run_with_heap_debug, without_ir_opt};

/// An out-of-range negative string offset warns before rejecting an empty RHS.
#[test]
fn test_append_seventh_review_negative_empty_rhs() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = ['AB'];
set_error_handler(function($level, $message) { echo $level, ':', $message, '|'; return true; });
try { $items[0][-3] = ''; } catch (Error $e) { echo 'E:', $e->getMessage(), '|'; }
restore_error_handler(); echo json_encode($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:Illegal string offset -3|[\"AB\"]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Invalid negative offsets do not invoke RHS string conversion.
#[test]
fn test_append_seventh_review_negative_stringable_rhs() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Text { public function __toString(): string { echo 'S'; return ''; } }
$items = ['AB'];
set_error_handler(function($level, $message) { echo $level, ':', $message, '|'; return true; });
try { $items[0][-3] = new Text(); } catch (Error $e) { echo 'E:', $e->getMessage(), '|'; }
restore_error_handler(); echo json_encode($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:Illegal string offset -3|[\"AB\"]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A returning deprecation handler observes only the false parent's empty-array promotion.
#[test]
fn test_append_seventh_review_false_handler_intermediate_state() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [false, 'q']; $alias = $items;
set_error_handler(function($level, $message) use (&$items) { echo json_encode($items), '|'; return true; });
$items[0]['x'] = 1;
restore_error_handler(); echo json_encode($items), '|', json_encode($alias);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[[],\"q\"]|[{\"x\":1},\"q\"]|[false,\"q\"]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A throwing handler sees promotion before the keyed store finishes and rethrows the same object.
#[test]
fn test_append_seventh_review_false_throwing_handler_state() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$items = [false, 'q']; $raised = new Error('handler');
set_error_handler(function($level, $message) use (&$items, $raised) {
    echo json_encode($items), '|'; throw $raised;
});
try { $items[0]['x'] = 1; } catch (Error $e) { echo $e === $raised ? 'same|' : 'other|'; }
restore_error_handler(); echo json_encode($items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[[],\"q\"]|same|[{\"x\":1},\"q\"]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An owned object RHS survives the deferred store and is destroyed exactly once afterward.
#[test]
fn test_append_seventh_review_false_owned_rhs() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class Value { public function __destruct() { echo 'D'; } }
$items = [false, 'q'];
set_error_handler(function($level, $message) use (&$items) {
    echo json_encode($items), '|'; throw new Error('handler');
});
try { $items[0]['x'] = new Value(); } catch (Error $e) { echo 'C:'; }
restore_error_handler(); echo get_class($items[0]['x']), '|';
unset($items); echo 'done';
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "[[],\"q\"]|C:Value|Ddone", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Pending handler exceptions preserve PHP's key-conversion state and TypeError chaining.
#[test]
fn test_append_seventh_review_false_pending_key_kinds() {
    let out = compile_and_run_with_heap_debug(r#"<?php
foreach ([0, 'x', null, true, 1.0, 1.5, [1], new stdClass()] as $key) {
    $items = [false];
    set_error_handler(function($level, $message) { throw new Error('handler'); });
    try { $items[0][$key] = 1; }
    catch (Throwable $e) {
        echo gettype($key), ':', get_class($e), ':', $e->getMessage(), ':';
        $previous = $e->getPrevious(); echo $previous === null ? '-' : $previous->getMessage();
        echo ':', json_encode($items), '|';
    }
    restore_error_handler();
}
"#);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, concat!(
        "integer:Error:handler:-:[[1]]|string:Error:handler:-:[{\"x\":1}]|",
        "NULL:Error:handler:-:[[]]|boolean:Error:handler:-:[{\"1\":1}]|",
        "double:Error:handler:-:[[]]|double:Error:handler:-:[[]]|",
        "array:TypeError:Cannot access offset of type array on array:handler:[[]]|",
        "object:TypeError:Cannot access offset of type stdClass on array:handler:[[]]|"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The seventh-review cases also retain their diagnostics and ownership without IR optimization.
#[test]
fn test_append_seventh_review_without_ir_opt() {
    without_ir_opt(|| {
        test_append_seventh_review_negative_empty_rhs();
        test_append_seventh_review_negative_stringable_rhs();
        test_append_seventh_review_false_handler_intermediate_state();
        test_append_seventh_review_false_throwing_handler_state();
        test_append_seventh_review_false_owned_rhs();
        test_append_seventh_review_false_pending_key_kinds();
    });
}

/// Runtime string writes preserve negative offsets, padding, cast warnings and invalid key types.
#[test]
fn test_append_sixth_review_string_offset_controls() {
    let out = compile_and_run_with_heap_debug(r#"<?php
set_error_handler(function(int $level, string $message) { echo $level, ':', $message, '|'; return true; });
function update(mixed $key): void {
    $items = ['AB'];
    try { $items[0][$key] = 'XYZ'; echo json_encode($items); }
    catch (Throwable $e) { echo get_class($e), ':', $e->getMessage(); }
    echo ';';
}
foreach ([-1, -3, 4, null, true, 1.5, '01', '1.5', 'X', [1], new stdClass()] as $key) { update($key); }
restore_error_handler();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, concat!(
        "2:Only the first byte will be assigned to the string offset|[\"AX\"];",
        "2:Illegal string offset -3|[\"AB\"];",
        "2:Only the first byte will be assigned to the string offset|[\"AB  X\"];",
        "2:String offset cast occurred|2:Only the first byte will be assigned to the string offset|[\"XB\"];",
        "2:String offset cast occurred|2:Only the first byte will be assigned to the string offset|[\"AX\"];",
        "2:String offset cast occurred|2:Only the first byte will be assigned to the string offset|[\"AX\"];",
        "2:Only the first byte will be assigned to the string offset|[\"AX\"];",
        "TypeError:Cannot access offset of type string on string;",
        "TypeError:Cannot access offset of type string on string;",
        "TypeError:Cannot access offset of type array on string;",
        "TypeError:Cannot access offset of type stdClass on string;"));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// False conversion remains attached to caller storage reached through reference variadics.
#[test]
fn test_append_sixth_review_false_reference_parent() {
    let out = compile_and_run_with_heap_debug(r#"<?php
set_error_handler(function(int $level, string $message) { echo $level, ':', $message, '|'; return true; });
function update(string $key, &...$items): void {
    $items[$key]['x'] = 1; echo json_encode($items), '|';
}
$a = false; $b = 'q'; update('0', $a, $b); echo json_encode($a), ':', $b;
restore_error_handler();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "8192:Automatic conversion of false to array is deprecated|[{\"x\":1},\"q\"]|{\"x\":1}:q");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// String offset replacement also writes through boxed reference-variadic caller slots.
#[test]
fn test_append_sixth_review_string_reference_parent() {
    let out = compile_and_run_with_heap_debug(r#"<?php
set_error_handler(function(int $level, string $message) { echo $level, ':', $message, '|'; return true; });
function update(string $key, &...$items): void {
    $items[$key]['1abc'] = 'Z'; echo json_encode($items), '|';
}
$a = 'AB'; $b = 'q'; update('0', $a, $b); echo $a, ':', $b;
restore_error_handler();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:Illegal string offset \"1abc\"|[\"AZ\",\"q\"]|AZ:q");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Catching a throwing false-conversion handler observes the completed assignment and releases roots.
#[test]
fn test_append_sixth_review_throwing_conversion_handler() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function update(string $key): void {
    $items = [false, 'q'];
    set_error_handler(function(int $level, string $message) { throw new Exception($message); });
    try { $items[$key]['x'] = 1; }
    catch (Exception $error) { echo $error->getMessage(), '|', json_encode($items); }
    restore_error_handler();
}
update('0');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Automatic conversion of false to array is deprecated|[{\"x\":1},\"q\"]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Computed nonnumeric string offsets throw instead of reaching a concrete string array setter.
#[test]
fn test_append_sixth_review_computed_string_offset() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function update(): void {
    $items = ['A', 'B', 'C']; $key = '1';
    try { $items[$key][strtolower('X')] = 1; echo 'NOERR'; }
    catch (TypeError $e) { echo $e->getMessage(); }
    echo '|', json_encode($items);
}
update();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Cannot access offset of type string on string|[\"A\",\"B\",\"C\"]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Runtime parent and leaf keys cannot silently discard invalid string offset writes.
#[test]
fn test_append_sixth_review_runtime_string_offset() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function update(string $key, string $inner): void {
    $items = ['A', 'B', 'C'];
    try { $items[$key][$inner] = 1; echo 'NOERR'; }
    catch (TypeError $e) { echo $e->getMessage(); }
    echo '|', json_encode($items);
}
update('1', 'x');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Cannot access offset of type string on string|[\"A\",\"B\",\"C\"]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Leading-numeric string offsets warn once, update the stored string, and preserve its COW copy.
#[test]
fn test_append_sixth_review_leading_numeric_string_offset() {
    let out = compile_and_run_with_heap_debug(r#"<?php
set_error_handler(function(int $level, string $message) { echo $level, ':', $message, '|'; return true; });
function update(): void {
    $items = ['AB', 'CD']; $key = '0'; $alias = $items;
    $items[$key]['1abc'] = 'Z';
    echo json_encode($items), '|', json_encode($alias);
}
update(); restore_error_handler();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2:Illegal string offset \"1abc\"|[\"AZ\",\"CD\"]|[\"AB\",\"CD\"]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Both statically known and runtime string parent keys autovivify false without changing aliases.
#[test]
fn test_append_sixth_review_false_parent() {
    let out = compile_and_run_with_heap_debug(r#"<?php
set_error_handler(function(int $level, string $message) { echo $level, ':', $message, '|'; return true; });
function dynamic(string $key): void {
    $items = [false, 'q']; $alias = $items; $items[$key]['x'] = 1;
    echo json_encode($items), '|', json_encode($alias), '|';
}
function concrete(): void {
    $items = [false, 'q']; $key = '0'; $items[$key]['x'] = 1;
    echo json_encode($items);
}
dynamic('0'); concrete(); restore_error_handler();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout,
        "8192:Automatic conversion of false to array is deprecated|[{\"x\":1},\"q\"]|[false,\"q\"]|8192:Automatic conversion of false to array is deprecated|[{\"x\":1},\"q\"]");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The latest scalar-parent regressions retain identical semantics without EIR optimization.
#[test]
fn test_append_sixth_review_without_ir_opt() {
    without_ir_opt(|| {
        test_append_sixth_review_computed_string_offset();
        test_append_sixth_review_runtime_string_offset();
        test_append_sixth_review_leading_numeric_string_offset();
        test_append_sixth_review_string_offset_controls();
        test_append_sixth_review_false_parent();
        test_append_sixth_review_false_reference_parent();
        test_append_sixth_review_string_reference_parent();
        test_append_sixth_review_throwing_conversion_handler();
    });
}

/// Ordinary String-key writes keep associative return storage and independent COW aliases.
#[test]
fn test_append_mixed_string_key_hash_return_contract() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function build(string $key, mixed $value): array {
    $items = [$value];
    $items[$key] = $value;
    return $items;
}
for ($i = 0; $i < 4; $i++) {
    $items = build('key', str_repeat('x', 4));
    $alias = $items;
    $items['key'] = 'changed';
    echo json_encode($alias), '/', json_encode($items), '|';
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "{\"0\":\"xxxx\",\"key\":\"xxxx\"}/{\"0\":\"xxxx\",\"key\":\"changed\"}|".repeat(4));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Numeric string keys preserve variadic reference markers for ordinary reads as well as JSON.
#[test]
fn test_append_second_review_variadic_string_keys() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function one(&...$items): void {
    $items["1"] = 'Q';
    echo strlen($items[1]), ':', $items[0] . '/' . $items[1] . '/' . $items[2], ':', json_encode($items), '|';
}
function zero(&...$items): void {
    $alias =& $items; $alias["0"] = $alias[1];
    echo $items[0], ':', $items[1], ':', $items[2], '|';
}
$a = 'A'; $b = 'B'; $c = 'C'; one($a, $b, $c); echo $a, ':', $b, ':', $c, '|';
$a = 'A'; $b = 'B'; zero($a, $b, $c); echo $a, ':', $b, ':', $c;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:A/Q/C:[\"A\",\"Q\",\"C\"]|A:Q:C|B:B:C|B:B:C");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Computed numeric string keys also write through reference cells via first-class calls.
#[test]
fn test_append_second_review_variadic_computed_string_key() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function update(string $key, &...$items): void {
    $items[$key] = 'Q';
    echo $items[0], '/', $items[1], '/', $items[2], '|';
}

$a = 'A'; $b = 'B'; $c = 'C';
$call = update(...); $call('1', $a, $b, $c);
echo $a, '/', $b, '/', $c;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "A/Q/C|A/Q/C");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Real string-key promotion retains reads and caller references for preexisting entries.
#[test]
fn test_append_second_review_variadic_named_key_control() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function update(&...$items): void {
    $items['key'] = 'X';
    echo $items[0], '/', $items[1], '/', $items['key'], ':', json_encode($items), '|';
    $items['1'] = 'Q';
    echo $items[0], '/', $items[1], '/', $items['key'], '|';
}
$a = 'A'; $b = 'B'; update($a, $b); echo $a, '/', $b;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "A/B/X:{\"0\":\"A\",\"1\":\"B\",\"key\":\"X\"}|A/Q/X|A/Q");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

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

/// A failed compound append preserves the writable null slot after eager RHS evaluation.
#[test]
fn test_append_compound_error_preserves_new_slot() {
    for (source, expected, warnings) in [
        (r#"<?php
$a = [];
try { $a[] /= ($argc - 1); } catch (DivisionByZeroError $e) { echo get_class($e), "\n"; }
var_dump($a);
"#, r#"DivisionByZeroError
array(1) {
  [0]=>
  NULL
}
"#, 0),
        (r#"<?php
$a = [10 => 3];
try { $a[] %= ($argc - 1); } catch (DivisionByZeroError $e) { echo get_class($e), "\n"; }
var_dump($a);
"#, r#"DivisionByZeroError
array(2) {
  [10]=>
  int(3)
  [11]=>
  NULL
}
"#, 0),
        (r#"<?php
$a = [];
try { $a[][] /= ($argc - 1); } catch (DivisionByZeroError $e) { echo get_class($e), "\n"; }
var_dump($a);
"#, r#"DivisionByZeroError
array(1) {
  [0]=>
  array(1) {
    [0]=>
    NULL
  }
}
"#, 0),
        (r#"<?php
$a = [];
try { $a[]["slot"] /= ($argc - 1); } catch (DivisionByZeroError $e) { echo get_class($e), "\n"; }
var_dump($a);
"#, r#"DivisionByZeroError
array(1) {
  [0]=>
  array(1) {
    ["slot"]=>
    NULL
  }
}
"#, 1),
        (r#"<?php
class Holder { public array $items = []; }
$h = new Holder();
try { $h->items[] /= ($argc - 1); } catch (DivisionByZeroError $e) { echo get_class($e), "\n"; }
var_dump($h->items);
"#, r#"DivisionByZeroError
array(1) {
  [0]=>
  NULL
}
"#, 0),
        (r#"<?php
class Holder { public static array $items = []; }
try { Holder::$items[] /= ($argc - 1); } catch (DivisionByZeroError $e) { echo get_class($e), "\n"; }
var_dump(Holder::$items);
"#, r#"DivisionByZeroError
array(1) {
  [0]=>
  NULL
}
"#, 0),
        (r#"<?php
function bad(): int { echo "rhs\n"; throw new RuntimeException("rhs"); }
$a = [];
try { $a[] /= bad(); } catch (RuntimeException $e) { echo get_class($e), "\n"; }
var_dump($a);
"#, r#"rhs
RuntimeException
array(0) {
}
"#, 0),
        (r#"<?php
$a = [7]; $copy = $a;
try { $a[] /= ($argc - 1); } catch (DivisionByZeroError $e) { echo get_class($e), "\n"; }
var_dump($a); var_dump($copy);
"#, r#"DivisionByZeroError
array(2) {
  [0]=>
  int(7)
  [1]=>
  NULL
}
array(1) {
  [0]=>
  int(7)
}
"#, 0),
    ] {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, expected, "{}", out.stderr);
        assert_eq!(out.stderr.matches("Undefined array key").count(), warnings, "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
}

/// Throwing key diagnostics preserve published parents and release intermediate receivers.
#[test]
fn test_append_compound_throwing_key_handlers_and_namespace() {
    for (source, expected) in [
        (r#"<?php
$items = [];
set_error_handler(function($code, $message) { throw new RuntimeException("key"); });
try { $items[][1.5] += 1; } catch (RuntimeException $e) { echo "caught\n"; }
restore_error_handler(); var_dump($items);
"#, r#"caught
array(1) {
  [0]=>
  array(0) {
  }
}
"#),
        (r#"<?php
$items = [];
set_error_handler(function($code, $message) { throw new RuntimeException("key"); });
try { $items[]["missing"] += 1; } catch (RuntimeException $e) { echo "caught\n"; }
restore_error_handler(); var_dump($items);
"#, r#"caught
array(1) {
  [0]=>
  array(0) {
  }
}
"#),
        (r#"<?php
namespace App;
function array_key_last(array $a): mixed { throw new \RuntimeException("shadow"); }
function intval(mixed $v): int { throw new \RuntimeException("shadow"); }
$a = [];
try { $a[] /= ($argc - 1); } catch (\DivisionByZeroError $e) { echo get_class($e), "\n"; }
var_dump($a);
"#, r#"DivisionByZeroError
array(1) {
  [0]=>
  NULL
}
"#),
        (r#"<?php
$items = [[]];
set_error_handler(function($code, $message) { throw new RuntimeException("key"); });
try { echo $items[0]["missing"]; } catch (RuntimeException $e) { echo "caught\n"; }
restore_error_handler(); var_dump($items);
"#, r#"caught
array(1) {
  [0]=>
  array(0) {
  }
}
"#),
    ] {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
        assert_eq!(out.stdout, expected, "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    }
}

/// Diagnostic callbacks can replace parents while the selected detached place stays alive.
#[test]
fn test_append_compound_reentrant_parent_cell_and_owned_handler_values() {
    let cases = [
        (r#"<?php
$items = [];
set_error_handler(function($code, $message) use (&$items) {
    if ($code === E_DEPRECATED) {  }
    return true;
});
$items[][1.5]["leaf"] += 2;
restore_error_handler(); var_dump($items);
"#, "array(1) {\n  [0]=>\n  array(1) {\n    [1]=>\n    array(1) {\n      [\"leaf\"]=>\n      int(2)\n    }\n  }\n}\n"),
        (r#"<?php
$items = [];
set_error_handler(function($code, $message) use (&$items) {
    if ($code === E_DEPRECATED) { $items[0][1]=9; }
    return true;
});
$items[][1.5]["leaf"] += 2;
restore_error_handler(); var_dump($items);
"#, "array(1) {\n  [0]=>\n  array(1) {\n    [1]=>\n    int(9)\n  }\n}\n"),
        (r#"<?php
$items = [];
set_error_handler(function($code, $message) use (&$items) {
    if ($code === E_DEPRECATED) { $items[0][1]=[]; }
    return true;
});
$items[][1.5]["leaf"] += 2;
restore_error_handler(); var_dump($items);
"#, "array(1) {\n  [0]=>\n  array(1) {\n    [1]=>\n    array(0) {\n    }\n  }\n}\n"),
        (r#"<?php
$items = [];
set_error_handler(function($code, $message) use (&$items) {
    if ($code === E_DEPRECATED) { $items=[]; }
    return true;
});
$items[][1.5]["leaf"] += 2;
restore_error_handler(); var_dump($items);
"#, "array(0) {\n}\n"),
        (r#"<?php
$items = [];
set_error_handler(function($code, $message) use (&$items) {
    if ($code === E_DEPRECATED) { $items[0][1] = ["held" => 9]; }
    return true;
});
$items[][1.5]["leaf"] += 2;
restore_error_handler(); var_dump($items);
"#, "array(1) {\n  [0]=>\n  array(1) {\n    [1]=>\n    array(1) {\n      [\"held\"]=>\n      int(9)\n    }\n  }\n}\n"),
    ];
    for (source, expected) in cases {
        for optimize in [true, false] {
            let out = if optimize { compile_and_run_with_heap_debug(source) }
                else { without_ir_opt(|| compile_and_run_with_heap_debug(source)) };
            assert!(out.success, "{}", out.stderr);
            assert_eq!(out.stdout, expected, "{source}");
            assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        }
    }
}

/// Appending to an `ArrayAccess` object dispatches `offsetSet(null, $value)`, never an
/// `append()` method (#1478).
#[test]
fn test_arrayaccess_append_calls_offset_set() {
    let out = compile_and_run(
        r#"<?php
class A implements ArrayAccess {
    public static array $log = [];
    public function offsetSet(mixed $o, mixed $v): void { A::$log[] = ($o === null ? "null" : "key") . "=" . $v; }
    public function offsetGet(mixed $o): mixed { return null; }
    public function offsetExists(mixed $o): bool { return false; }
    public function offsetUnset(mixed $o): void {}
}
function f(A $x): string { $x[] = 5; return implode(",", A::$log); }
echo f(new A()), "\n";
"#,
    );
    assert_eq!(out, "null=5\n");
}

/// The same dispatch for a union of `ArrayAccess` classes, which `codegen_repr` boxes into
/// `Mixed` (#1497).
#[test]
fn test_union_arrayaccess_append_calls_offset_set() {
    let out = compile_and_run(
        r#"<?php
class A implements ArrayAccess {
    public static array $log = [];
    public function offsetSet(mixed $o, mixed $v): void { A::$log[] = ($o === null ? "null" : "key") . "=" . $v; }
    public function offsetGet(mixed $o): mixed { return null; }
    public function offsetExists(mixed $o): bool { return false; }
    public function offsetUnset(mixed $o): void {}
}
class B implements ArrayAccess {
    public static array $log = [];
    public function offsetSet(mixed $o, mixed $v): void { B::$log[] = ($o === null ? "null" : "key") . "=" . $v; }
    public function offsetGet(mixed $o): mixed { return null; }
    public function offsetExists(mixed $o): bool { return false; }
    public function offsetUnset(mixed $o): void {}
}
function f(A|B $x): string { $x[] = 5; return implode(",", A::$log) . "/" . implode(",", B::$log); }
echo f(new A()), " ", f(new B()), "\n";
"#,
    );
    assert_eq!(out, "null=5/ null=5/null=5\n");
}

/// A throwing `offsetSet` must not leak the value the append could not store (#1478 review).
#[test]
fn test_arrayaccess_append_throwing_offset_set_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class A implements ArrayAccess {
    public function offsetSet(mixed $o, mixed $v): void { throw new RuntimeException("boom"); }
    public function offsetGet(mixed $o): mixed { return null; }
    public function offsetExists(mixed $o): bool { return false; }
    public function offsetUnset(mixed $o): void {}
}
function mk(): string { return str_repeat("ab", 200); }
function f(A $a): string {
    for ($i = 0; $i < 20; $i++) { try { $a[] = mk(); } catch (RuntimeException $e) {} }
    return "done";
}
echo f(new A()), "\n";
"#,
    );
    assert_eq!(out.stdout, "done\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}
