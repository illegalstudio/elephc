//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of object static properties, including class static method string param, class static and instance, and static property read write.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures compile to native binaries while malformed or fatal cases assert captured failures.

use super::*;

/// Checks one rejected string-parent update without mutating or retaining its static storage.
fn verify_static_string_parent_error(operation: &str, effects: &str, message: &str) {
    let out = compile_and_run_with_heap_debug(&format!(r#"<?php
class T {{ public static array $items = ['ab', 10]; }}
function keyA(): int {{ echo 'a'; return 0; }}
function keyB(): int {{ echo 'b'; return 1; }}
try {{ {operation}; }}
catch (Error $e) {{ echo $e->getMessage(), '|', json_encode(T::$items); }}
"#));
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("{effects}{message}|[\"ab\",10]"), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A compound operation on a string offset uses the assign-op diagnostic.
#[test]
fn test_static_prefix_seventh_review_string_compound_leaf() {
    verify_static_string_parent_error("T::$items[0][0] += 1", "",
        "Cannot use assign-op operators with string offsets");
}

/// A string used as an intermediate increment dimension is not a direct offset increment.
#[test]
fn test_static_prefix_seventh_review_string_intermediate_incdec() {
    verify_static_string_parent_error("++T::$items[0][0][1]", "",
        "Cannot use string offset as an array");
}

/// A deeper compound target also rejects the intermediate string as an array.
#[test]
fn test_static_prefix_seventh_review_string_intermediate_compound() {
    verify_static_string_parent_error("T::$items[0][0][1] += 1", "",
        "Cannot use string offset as an array");
}

/// Every computed dimension is evaluated before an intermediate string-parent error.
#[test]
fn test_static_prefix_seventh_review_string_intermediate_key_effects() {
    verify_static_string_parent_error("++T::$items[0][keyA()][keyB()]", "ab",
        "Cannot use string offset as an array");
}

/// Postfix reads create null parents before invoking the undefined-leaf warning handler.
#[test]
fn test_static_prefix_latest_review_postfix_autovivification() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class T { public static array $items = [[null]]; }
set_error_handler(function(int $level, string $message) { throw new Error($message); });
try { $old = T::$items[0][0][1]++; }
catch (Error $e) { echo $e->getMessage(), '|', json_encode(T::$items); }
restore_error_handler();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "Undefined array key 1|[[[]]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A nested postfix result does not repeat the converted parent's key warning.
#[test]
fn test_static_prefix_latest_review_postfix_fractional_key() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array $items = [[1], [2]]; }
set_error_handler(function($level, $message) {
    if ($level === E_DEPRECATED) { echo 'W'; }
    return true;
});
echo H::$items[1.5][0]++, '|', json_encode(H::$items);
restore_error_handler();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "W2|[[1],[3]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Captured string, null, and integer postfix results remain independent of their updates.
#[test]
fn test_static_prefix_latest_review_postfix_old_values() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class P { public static array $items = [['az', null, 9]]; }
set_error_handler(function($level, $message) { return true; });
$s = P::$items[0][0]++; $n = P::$items[0][1]++; $d = P::$items[0][2]--;
restore_error_handler();
echo json_encode([$s, $n, $d]), '|', json_encode(P::$items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[\"az\",null,9]|[[\"ba\",1,8]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// String offset incdec errors differ from integer-parent errors and leave storage unchanged.
#[test]
fn test_static_prefix_latest_review_string_offset_errors() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class T { public static array $items = ['ab', 10]; }
try { ++T::$items[0][0]; }
catch (Error $e) { echo $e->getMessage(), '|', json_encode(T::$items), '|'; }
try { --T::$items[1][0]; }
catch (Error $e) { echo $e->getMessage(), '|', json_encode(T::$items); }
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout,
        "Cannot increment/decrement string offsets|[\"ab\",10]|Cannot use a scalar value as an array|[\"ab\",10]",
        "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Literal null promotion remains writable by later null and numeric keys without breaking COW.
#[test]
fn test_static_prefix_fifth_review_repeated_literal_null() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array<mixed> $items = [[5], [7]]; }
$alias = H::$items;
set_error_handler(function($level, $message) { return true; });
++H::$items[null][0]; ++H::$items[null][0];
++H::$items[0][0]; ++H::$items['1'][0];
restore_error_handler();
echo json_encode(H::$items), '|', json_encode($alias);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "{\"0\":[6],\"1\":[8],\"\":[2]}|[[5],[7]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A static promoted in another statement keeps its hash protocol for typed integer calls.
#[test]
fn test_static_prefix_fifth_review_integer_after_promotion() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array<mixed> $items = [[5], [7]]; }
function bump(int $key): void { ++H::$items[$key][0]; }
$alias = H::$items;
set_error_handler(function($level, $message) { return true; });
++H::$items['k'][0]; bump(0); bump(1);
restore_error_handler();
echo json_encode(H::$items), '|', json_encode($alias);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "{\"0\":[6],\"1\":[8],\"k\":[1]}|[[5],[7]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Flat declared indexed and associative mixed statics accept literal string and null keys.
#[test]
fn test_static_prefix_fifth_review_flat_literal_and_assoc_keys() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H {
    public static array<mixed> $flat = [5, 7];
    public static array<string, mixed> $assoc = ['k' => 5];
}
function bump(string $key): void { ++H::$assoc[$key]; }
$flat = H::$flat; $assoc = H::$assoc;
set_error_handler(function($level, $message) { return true; });
++H::$flat['k']; ++H::$flat[null]; ++H::$flat[0];
++H::$assoc['k']; bump('k'); ++H::$assoc[null];
restore_error_handler();
echo json_encode(H::$flat), '|', json_encode(H::$assoc), '|', json_encode($flat), '|', json_encode($assoc);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "{\"0\":6,\"1\":7,\"k\":1,\"\":1}|{\"k\":7,\"\":1}|[5,7]|{\"k\":5}", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested prefix expression results reuse each fractional dimension's single diagnosis.
#[test]
fn test_static_prefix_fifth_review_nested_expression_float_warning() {
    let out = compile_and_run(r#"<?php
class H { public static array $items = [[10]]; }
set_error_handler(function($level, $message) { if ($level === E_DEPRECATED) { echo 'W'; } return true; });
echo ++H::$items[1.5][0], '|', json_encode(H::$items);
restore_error_handler();
"#);
    assert_eq!(out, "W1|[[10],[1]]");
}

/// Prefix results keep the pending value when a warning handler replaces the stored parent.
#[test]
fn test_static_prefix_fifth_review_nested_expression_handler_result() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class T { public static array $items = [[null]]; }
set_error_handler(function($level, $message) {
    echo json_encode(T::$items), ':', $message, '|';
    T::$items[0][0]['extra'] = 9;
    return true;
});
echo ++T::$items[0][0][1], '|';
restore_error_handler();
echo json_encode(T::$items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[[[]]]:Undefined array key 1|1|[[{\"extra\":9}]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Numeric-only writes preserve list shape and subsequent concrete array consumers.
#[test]
fn test_static_prefix_fifth_review_numeric_list_consumers() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array<mixed> $items = [1, 2]; }
++H::$items[0];
echo json_encode(H::$items), '|', json_encode(array_values(H::$items)), '|';
echo json_encode(array_keys(H::$items)), '|', array_is_list(H::$items) ? 'L' : 'N', '|';
$sum = 0;
foreach (H::$items as $value) { $sum += $value; }
echo $sum;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[2,2]|[2,2]|[0,1]|L|4", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nullable integer parents preserve null's empty key and the original COW sibling.
#[test]
fn test_static_prefix_fourth_review_nested_nullable_int() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array<mixed> $items = [[5], [7]]; }
function bump(?int $key): void { ++H::$items[$key][0]; }
$alias = H::$items;
set_error_handler(function($level, $message) {
    if ($level === E_WARNING) { echo 'W:', $message, '|'; }
    return true;
});
bump(1); echo json_encode(H::$items), '|';
bump(null); bump(null); restore_error_handler();
echo json_encode(H::$items), '|', json_encode($alias);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[[5],[8]]|W:Undefined array key \"\"|W:Undefined array key 0|{\"0\":[5],\"1\":[8],\"\":[2]}|[[5],[7]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nullable integer and boolean flat keys insert an empty key instead of updating zero.
#[test]
fn test_static_prefix_fourth_review_flat_nullable_scalars() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H {
    public static array<mixed> $flat = [5, 7];
    public static array<mixed> $flags = [5, 7];
}
function bump(?int $key): void { ++H::$flat[$key]; }
function flag(?bool $key): void { ++H::$flags[$key]; }
$flat = H::$flat; $flags = H::$flags;
set_error_handler(function($level, $message) {
    if ($level === E_WARNING) { echo 'W:', $message, '|'; }
    return true;
});
bump(1); echo json_encode(H::$flat), '|';
bump(null); bump(null); echo json_encode(H::$flat), '|';
flag(false); flag(null); restore_error_handler();
echo json_encode(H::$flags), '|', json_encode($flat), '|', json_encode($flags);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[5,8]|W:Undefined array key \"\"|{\"0\":5,\"1\":8,\"\":2}|W:Undefined array key \"\"|{\"0\":6,\"1\":7,\"\":1}|[5,7]|[5,7]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested mixed dimensions promote the static root without changing its indexed alias.
#[test]
fn test_static_prefix_third_review_nested_mixed() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array<mixed> $items = [[5], [7]]; }
function bump(mixed $key): void { ++H::$items[$key][0]; }
$alias = H::$items;
set_error_handler(function($level, $message) { echo 'W:', $message, '|'; return true; });
bump('k'); restore_error_handler();
echo json_encode(H::$items), '|', json_encode($alias);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "W:Undefined array key \"k\"|W:Undefined array key 0|{\"0\":[5],\"1\":[7],\"k\":[1]}|[[5],[7]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// One-level String dimensions insert their actual key in a declared mixed static array.
#[test]
fn test_static_prefix_third_review_flat_string() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array<mixed> $flat = [5, 7]; }
function bump(string $key): void { ++H::$flat[$key]; }
$alias = H::$flat;
set_error_handler(function($level, $message) { echo 'W:', $message, '|'; return true; });
bump('k'); restore_error_handler();
echo json_encode(H::$flat), '|', json_encode($alias);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "W:Undefined array key \"k\"|{\"0\":5,\"1\":7,\"k\":1}|[5,7]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// One-level Mixed dimensions preserve the original static elements and by-value alias.
#[test]
fn test_static_prefix_third_review_flat_mixed() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array<mixed> $flat = [5, 7]; }
function bump(mixed $key): void { ++H::$flat[$key]; }
$alias = H::$flat;
set_error_handler(function($level, $message) { echo 'W:', $message, '|'; return true; });
bump('k'); restore_error_handler();
echo json_encode(H::$flat), '|', json_encode($alias);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "W:Undefined array key \"k\"|{\"0\":5,\"1\":7,\"k\":1}|[5,7]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// An associative static parent reuses its first fractional-key diagnosis during writeback.
#[test]
fn test_static_prefix_third_review_assoc_float() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array<string, mixed> $items = ['k' => [5]]; }
function bump(float $key): void { ++H::$items[$key][0]; }
$alias = H::$items;
set_error_handler(function($level, $message) { echo 'W:', $message, '|'; return true; });
bump(1.5); restore_error_handler();
echo json_encode(H::$items), '|', json_encode($alias);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "W:Implicit conversion from float 1.5 to int loses precision|W:Undefined array key 1|W:Undefined array key 0|{\"k\":[5],\"1\":[1]}|{\"k\":[5]}", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}


/// Mixed numeric keys retain list semantics and flat writes read bare keys after their RHS.
#[test]
fn test_static_prefix_third_review_numeric_and_store_order() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array<mixed> $nested = [[5], [7]]; public static array<mixed> $flat = [5, 7]; }
function nested(mixed $key): void { ++H::$nested[$key][0]; }
function flat(mixed $key): void { ++H::$flat[$key]; }
function write(string $key): void { H::$flat[$key] = ($key = 'late'); }
$n = H::$nested; $f = H::$flat;
set_error_handler(function($level, $message) { echo 'W:', $message, '|'; return true; });
nested(0); nested('1'); flat(0); flat('1'); flat(1.5);
restore_error_handler();
echo json_encode(H::$nested), '|', json_encode(H::$flat), '|', json_encode($n), '|', json_encode($f), '|';
write('early'); echo json_encode(H::$flat);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "W:Implicit conversion from float 1.5 to int loses precision|[[6],[8]]|[6,9]|[[5],[7]]|[5,7]|{\"0\":6,\"1\":9,\"late\":\"late\"}", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Captured variable string keys release each projected string on parent and leaf updates.
#[test]
fn test_static_prefix_second_review_variable_string_key_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array $items = ['k' => [5]]; }
$key = 'k';
++H::$items[$key][0];
echo json_encode(H::$items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "{\"k\":[6]}", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Declared mixed-valued hash roots publish nested incdec and plain stores with COW.
#[test]
fn test_static_prefix_second_review_declared_assoc_writeback() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array<string, mixed> $items = ['k' => [5]]; }
$alias = H::$items;
++H::$items['k'][0]; echo json_encode(H::$items), '|';
H::$items['k'][0] = 9;
echo json_encode(H::$items), '|', json_encode($alias);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "{\"k\":[6]}|{\"k\":[9]}|{\"k\":[5]}", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// String variable dimensions on declared mixed arrays promote storage instead of selecting zero.
#[test]
fn test_static_prefix_second_review_declared_mixed_string_key() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array<mixed> $items = [[5], [7]]; }
$alias = H::$items; $key = 'k';
set_error_handler(function($level, $message) { echo 'W:', $message, '|'; return true; });
++H::$items[$key][0];
restore_error_handler();
echo json_encode(H::$items), '|', json_encode($alias);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "W:Undefined array key \"k\"|W:Undefined array key 0|{\"0\":[5],\"1\":[7],\"k\":[1]}|[[5],[7]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// All direct static incdec expression forms evaluate and diagnose their float key once.
#[test]
fn test_static_prefix_ci_float_expression_forms() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array $items = [null, 10, 20, 30, 40]; }
function dimensionKey(float $value): float { echo 'K'; return $value; }
set_error_handler(function($level, $message) { echo 'W'; return true; });
echo ++H::$items[dimensionKey(1.5)], '|';
echo H::$items[dimensionKey(2.5)]++, '|';
echo --H::$items[dimensionKey(3.5)], '|';
echo H::$items[dimensionKey(4.5)]--, '|';
echo json_encode(H::$items);
restore_error_handler();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "KW11|KW20|KW29|KW40|[null,11,21,29,39]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Explicit mixed-element static roots keep nested writes attached without mutating aliases.
#[test]
fn test_static_prefix_oct9_declared_mixed_nested_writeback() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array<mixed> $items = [1, [5]]; }
$alias = H::$items;
echo json_encode($alias), '|';
++H::$items[1][0];
echo json_encode(H::$items), '|', json_encode($alias);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[1,[5]]|[1,[6]]|[1,[5]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Concrete roots evaluate and diagnose a fractional computed parent dimension once.
#[test]
fn test_static_prefix_oct9_declared_mixed_float_dimension() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array<mixed> $items = [1, [5]]; }
function dimensionKey(): float { echo 'K'; return 1.5; }
set_error_handler(function($level, $message) { echo 'W'; return true; });
++H::$items[dimensionKey()][0];
echo '|', json_encode(H::$items);
restore_error_handler();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "KW|[1,[6]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Prefix and postfix expression forms use the same PHP incdec semantics as statements.
#[test]
fn test_static_prefix_oct9_expression_string_null() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array $items = ['az', 'az', null, '9', 'zz', null]; }
echo ++H::$items[0], '|', --H::$items[1], '|', is_null(--H::$items[2]) ? 'null' : 'bad', '|';
echo H::$items[3]++, ':', H::$items[3], '|', H::$items[4]++, ':', H::$items[4], '|';
echo is_null(H::$items[5]++) ? 'null' : 'bad', ':', H::$items[5], '|', json_encode(H::$items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "ba|az|null|9:10|zz:aaa|null:1|[\"ba\",\"az\",null,10,\"aaa\",1]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Inferred concrete and homogeneous static roots keep nested mutations attached with COW.
#[test]
fn test_static_prefix_followup_concrete_nested_writeback() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static $items = [1, [5]]; }
class N { public static $items = [[5], [6]]; }
$alias = H::$items;
++H::$items[1][0];
++N::$items[0][0];
--N::$items[1][0];
echo json_encode(H::$items), '|', json_encode(N::$items), '|', json_encode($alias);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[1,[6]]|[[6],[5]]|[1,[5]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Direct static elements use PHP string, null, float and numeric-string incdec semantics.
#[test]
fn test_static_prefix_followup_string_null() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array $items = ['az', 'az', null, '9', 1.5]; }
++H::$items[0];
--H::$items[1];
--H::$items[2];
++H::$items[3];
++H::$items[4];
echo json_encode(H::$items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[\"ba\",\"az\",null,10,2.5]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Nested static element updates share the same non-numeric incdec kernel.
#[test]
fn test_static_prefix_followup_nested_string_null() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array $items = [['az', null]]; }
++H::$items[0][0];
--H::$items[0][1];
echo json_encode(H::$items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[[\"ba\",null]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A scalar parent throws only after the final computed key runs, without a spurious warning.
#[test]
fn test_static_prefix_followup_scalar_parent_error_order() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H {
    public static array $items = [[1 => 10, 2 => 20]];
    public static function bump(): int { echo 'b'; return 1; }
    public static function last(): int { echo 'f'; return 1; }
}
set_error_handler(function($level, $message) { echo 'warning:', $message; return true; });
$key = 0;
try { ++H::$items[$key][H::bump()][H::last()]; echo 'bad'; }
catch (Error $error) { echo ':', $error->getMessage(), '|'; }
restore_error_handler();
echo json_encode(H::$items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bf:Cannot use a scalar value as an array|[{\"1\":10,\"2\":20}]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A throwing final key takes precedence over the scalar-parent Error and unwinds the parent.
#[test]
fn test_static_prefix_followup_scalar_parent_throwing_key() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class H { public static array $items = [[10]]; }
function fail(): int { echo 'f'; throw new Error('key'); }
try { ++H::$items[0][0][fail()]; echo 'bad'; }
catch (Error $error) { echo ':', $error->getMessage(); }
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "f:key", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Captured incdec reads resolve namespace imports before matching their stored target.
#[test]
fn test_static_prefix_followup_namespaced_capture() {
    let out = compile_and_run_with_heap_debug(r#"<?php
namespace Storage { class H { public static $items = [[5]]; } }
namespace Consumer {
    use Storage\H as Box;
    function key(): int { echo 'k'; return 0; }
    ++Box::$items[key()][0];
    echo json_encode(Box::$items);
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "k[[6]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A throwing warning handler releases the pending nested update's retained parents.
#[test]
fn test_static_prefix_review_throwing_handler_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class T { public static array $items = [[null]]; }
set_error_handler(function($level, $message) { throw new Error("stop"); });
try { ++T::$items[0][0][1]; } catch (Error $e) { echo "caught:"; }
restore_error_handler();
echo json_encode(T::$items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "caught:[[[]]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A throwing compound RHS releases the write-context parent without changing the leaf.
#[test]
fn test_static_prefix_review_throwing_rhs_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class T { public static array $items = [[7]]; }
function fail(): int { throw new Error("stop"); }
try { T::$items[0][0] += fail(); } catch (Error $e) { echo "caught:"; }
echo json_encode(T::$items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "caught:[[7]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A null intermediate becomes an array before the leaf warning handler runs.
#[test]
fn test_static_prefix_review_autovivifies_null_before_handler() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class T { public static array $items = [[null]]; }
set_error_handler(function($level, $message) {
    echo json_encode(T::$items), ":", $message, "|";
    T::$items[0][0]["extra"] = 9;
    return true;
});
++T::$items[0][0][1];
restore_error_handler();
echo json_encode(T::$items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[[[]]]:Undefined array key 1|[[{\"extra\":9}]]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A handler's replacement of an autovivified parent is not overwritten by the pending update.
#[test]
fn test_static_prefix_review_missing_parent_preserves_handler_write() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class T { public static array $items = [[1 => 10]]; }
set_error_handler(function($level, $message) {
    echo json_encode(T::$items), ":", $message, "|";
    T::$items[1]["seen"] = 1;
    return true;
});
++T::$items[1][1];
restore_error_handler();
echo json_encode(T::$items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[{\"1\":10}]:Undefined array key 1|[{\"1\":10},{\"seen\":1}]:Undefined array key 1|[{\"1\":10},{\"seen\":1}]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A literal string key used by both halves of a nested update leaves no heap owners behind.
#[test]
fn test_static_prefix_review_literal_string_key_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class T { public static array $missing = [[]]; }
set_error_handler(function($level, $message) { return true; });
++T::$missing[0]["before"];
restore_error_handler();
echo json_encode(T::$missing);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[{\"before\":1}]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A later effectful dimension cannot change an earlier mutable static-property key.
#[test]
fn test_static_property_array_prefix_update_nested_effectful_key_order() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NestedPrefixEffectful {
    public static array $items = [[1 => 10, 2 => 20]];
    public static int $key = 0;
    public static function bump(): int { self::$key = 1; echo "f"; return 1; }
}
++NestedPrefixEffectful::$items[NestedPrefixEffectful::$key][NestedPrefixEffectful::bump()];
echo json_encode(NestedPrefixEffectful::$items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "f[{\"1\":11,\"2\":20}]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Bare variable dimensions keep PHP's deferred lookup even beside an effectful key.
#[test]
fn test_static_property_array_prefix_update_nested_bare_variable_key_is_deferred() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NestedPrefixVariable {
    public static array $items = [[1 => 10]];
    public static function bump(): int { global $key; $key = 1; echo "f"; return 1; }
}
$key = 0;
set_error_handler(function($level, $message) { return true; });
++NestedPrefixVariable::$items[$key][NestedPrefixVariable::bump()];
restore_error_handler();
echo json_encode(NestedPrefixVariable::$items);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "f[{\"1\":10},{\"1\":1}]", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Prefix static-array updates preserve scoped receivers and evaluate effectful indices once.
#[test]
fn test_static_property_array_prefix_updates() {
    let out = compile_and_run(r#"<?php
class PrefixStaticBase {
    public static array $items = [10, 20];
    public static function key(): int { echo "k"; return 0; }
    public static function update(): void {
        ++self::$items[0];
        --static::$items[1];
    }
}
class PrefixStaticChild extends PrefixStaticBase {
    public function updateParent(): void {
        ++parent::$items[0];
        --parent::$items[1];
    }
}
++PrefixStaticBase::$items[PrefixStaticBase::key()];
--PrefixStaticBase::$items[PrefixStaticBase::key()];
++PrefixStaticBase::$items[0];
--PrefixStaticBase::$items[1];
PrefixStaticBase::update();
(new PrefixStaticChild())->updateParent();
echo "|", PrefixStaticBase::$items[0], "|", PrefixStaticBase::$items[1];
"#);
    assert_eq!(out, "kk|13|17");
}

/// A key changed by a float-key warning handler cannot redirect the update's write half.
#[test]
fn test_static_property_array_prefix_update_snapshots_warning_index() {
    let out = compile_and_run(r#"<?php
class PrefixStaticSnapshot { public static array $items = [10, 20]; }
$key = 0.5;
set_error_handler(function($level, $message) use (&$key) { $key = 1.0; return true; });
++PrefixStaticSnapshot::$items[$key];
restore_error_handler();
echo PrefixStaticSnapshot::$items[0], "|", PrefixStaticSnapshot::$items[1], "|", $key;
"#);
    assert_eq!(out, "11|20|1");
}

/// A nested update keeps the original leaf key when its warning handler mutates that key.
#[test]
fn test_static_property_array_prefix_update_nested_warning_index() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NestedPrefixSnapshot { public static array $items = [[1 => 10, 2 => 20]]; }
$key = 1.9;
$warnings = 0;
set_error_handler(function($level, $message) use (&$key, &$warnings) { $key = 2.9; ++$warnings; return true; });
++NestedPrefixSnapshot::$items[0][$key];
restore_error_handler();
echo json_encode(NestedPrefixSnapshot::$items), "|", $warnings, "|", $key;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[{\"1\":11,\"2\":20}]|1|2.9", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Literal nested float keys are diagnosed once and an undefined leaf keeps its original key.
#[test]
fn test_static_property_array_prefix_update_nested_literal_and_missing_keys() {
    let out = compile_and_run(r#"<?php
class NestedPrefixKeys { public static array $literal = [[1 => 10]]; public static array $missing = [[]]; }
$warnings = 0;
set_error_handler(function($level, $message) use (&$warnings) { ++$warnings; return true; });
++NestedPrefixKeys::$literal[0][1.9];
restore_error_handler();
echo json_encode(NestedPrefixKeys::$literal), "|", $warnings, "|";
$key = 5;
set_error_handler(function($level, $message) use (&$key) { $key = 6; return true; });
++NestedPrefixKeys::$missing[0][$key];
restore_error_handler();
echo json_encode(NestedPrefixKeys::$missing);
"#);
    assert_eq!(out, "[{\"1\":11}]|1|[{\"5\":1}]");
}

/// An outer-key handler runs before the inner key is captured, and is not called again on write.
#[test]
fn test_static_property_array_prefix_update_nested_key_capture_order() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NestedPrefixOrder { public static array $items = [[1 => 10, 2 => 20]]; }
$row = 0.5;
$key = 1.9;
$warnings = 0;
set_error_handler(function($level, $message) use (&$key, &$warnings) { $key = 2.0; ++$warnings; return true; });
++NestedPrefixOrder::$items[$row][$key];
restore_error_handler();
echo json_encode(NestedPrefixOrder::$items), "|", $warnings;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[{\"1\":10,\"2\":21}]|1", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Captured nested update keys preserve detached aliases and release string-key snapshots.
#[test]
fn test_static_property_array_prefix_update_nested_cow_and_string_key() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class NestedPrefixCow { public static array $items = [[1 => 10]]; public static array $missing = [[]]; }
$alias = NestedPrefixCow::$items;
++NestedPrefixCow::$items[0][1];
echo json_encode($alias), "|", json_encode(NestedPrefixCow::$items), "|";
$key = "before";
set_error_handler(function($level, $message) use (&$key) { $key = "after"; return true; });
++NestedPrefixCow::$missing[0][$key];
restore_error_handler();
echo json_encode(NestedPrefixCow::$missing), "|", $key;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[{\"1\":10}]|[{\"1\":11}]|[{\"before\":1}]|after", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Shutdown frees inherited static strings, containers, objects, and captured callbacks exactly once.
#[test]
fn test_class_static_properties_release_last_owners_at_shutdown() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ShutdownStaticPayload { public function __destruct() { echo "D"; } }
class ShutdownStaticOwner {
    public static string $text = "";
    public static string $uninitialized;
    public static mixed $list = null;
    public static mixed $map = null;
    public static mixed $object = null;
    public static mixed $callback = null;
}
class ShutdownStaticChild extends ShutdownStaticOwner {}
ShutdownStaticChild::$text = str_repeat("s", 32);
ShutdownStaticChild::$list = [str_repeat("l", 32)];
ShutdownStaticChild::$map = ["key" => str_repeat("m", 32)];
ShutdownStaticChild::$object = new ShutdownStaticPayload();
$capture = str_repeat("c", 32);
ShutdownStaticChild::$callback = static function () use ($capture): void { echo $capture; };
unset($capture);
echo isset(ShutdownStaticChild::$uninitialized) ? "bad" : "ready";
"#);
    assert!(out.success, "stdout: {} stderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "readyD", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Checked increments of a typed static integer retire every intermediate Mixed cell.
#[test]
fn test_static_integer_increment_releases_checked_boxes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class StaticIncrementOwner {
    public static int $count = 0;
    public static function bump(): void { self::$count++; }
}
for ($i = 0; $i < 20; $i = (int)($i + 1)) { StaticIncrementOwner::bump(); }
echo StaticIncrementOwner::$count;
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "20", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Borrowed boxed scalar conversions neither leak a retain nor consume the caller's source.
#[test]
fn test_static_scalar_stores_release_only_owned_source_boxes() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class StaticScalarOwner {
    public static int $integer = 0;
    public static bool $flag = false;
    public static float $fraction = 0.0;
    public static string $text = "";
}
function storeStaticScalars(mixed $value): void {
    StaticScalarOwner::$integer = $value;
    StaticScalarOwner::$flag = $value;
    StaticScalarOwner::$fraction = $value;
    StaticScalarOwner::$text = $value;
    echo StaticScalarOwner::$integer, ":", StaticScalarOwner::$flag ? "1" : "0",
        ":", StaticScalarOwner::$fraction, ":", StaticScalarOwner::$text, ":", $value, "|";
}
storeStaticScalars($argc);
storeStaticScalars(3);
StaticScalarOwner::$text = "";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1:1:1:1:1|3:1:3:3:3|", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Persisting boxed string payloads creates exactly one static owner and leaves the source usable.
#[test]
fn test_static_string_stores_persist_boxed_payload_once() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class StaticStringOwner { public static string $text = ""; }
function storeStaticString(mixed $value): void {
    StaticStringOwner::$text = $value;
    echo strlen(StaticStringOwner::$text), ":", $value, "|";
}
$text = str_repeat("x", 24);
for ($i = 0; $i < 4; $i = (int)($i + 1)) { storeStaticString($text); }
unset($text);
echo strlen(StaticStringOwner::$text);
StaticStringOwner::$text = "";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("{}24", "24:xxxxxxxxxxxxxxxxxxxxxxxx|".repeat(4)), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Verifies a declared static object property releases the Mixed box it was handed ownership of.
///
/// A runtime-shaped write to a static object slot unboxes the payload and retains the OBJECT on
/// its own, so the cell EIR expects this consumer to adopt keeps no owner afterwards. Without that
/// release the slot leaked one boxed cell and one payload reference per accepted write, which is
/// why the loop repeats the write instead of storing once.
///
/// The write goes through `ReflectionProperty::setValue()` because a direct `Class::$prop = <mixed>`
/// assignment into a declared OBJECT static property is refused by the type checker: Mixed is only
/// statically compatible with the scalar targets that have boxed cast funnels. Reflection is the
/// PHP-visible route that reaches this lowering with a boxed source, and the reflector is built
/// inline at each write because a static-property `setValue()` requires an inline known slot.
///
/// That route is also why the run cannot assert `HEAP DEBUG: leak summary: clean`: the inline
/// reflectors leave Reflection allocations of their own live at exit, unrelated to the store under
/// test, which would swamp the one-cell-per-write signal. The emitted store sequence carries the
/// evidence instead. Only this lowering emits a `prop_store_mixed_value_done` label, and no other
/// write in the fixture reaches it, so each occurrence marks one static Mixed-to-object store and
/// the needles around it are local to the changed code: `__rt_mixed_unbox` feeding the store just
/// above the label, the independent object `__rt_incref` right at it, then `__rt_decref_mixed`
/// retiring the adopted cell. Deleting the release deletes that last call and fails the test.
#[test]
fn test_static_object_property_releases_adopted_mixed_box() {
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(r#"<?php
class StaticAnimal {
    public string $name = "animal";
}
class StaticDog extends StaticAnimal {
    public string $name = "dog";
}
class StaticPen {
    public static StaticAnimal $pet;
}

function pickStaticPet(mixed $value): mixed { return $value; }

for ($i = 0; $i < 40; $i++) {
    (new ReflectionProperty(StaticPen::class, "pet"))->setValue(null, pickStaticPet(new StaticAnimal()));
    (new ReflectionProperty(StaticPen::class, "pet"))->setValue(null, pickStaticPet(new StaticDog()));
}
echo StaticPen::$pet->name, "|", $i;
"#);
    assert!(out.success, "stdout: {} stderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "dog|40", "{}", out.stderr);

    let lines: Vec<&str> = assembly.lines().collect();
    let stores: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            line.contains("prop_store_mixed_value_done") && line.trim_end().ends_with(':')
        })
        .map(|(index, _)| index)
        .collect();
    assert!(
        stores.len() >= 2,
        "expected one Mixed-to-object static store per write, found {}\n{assembly}",
        stores.len()
    );
    for store in stores {
        assert!(
            lines[store.saturating_sub(16)..store]
                .iter()
                .any(|line| line.contains("__rt_mixed_unbox")),
            "static object store at line {store} is not fed by a Mixed unbox\n{assembly}"
        );
        let tail = &lines[store + 1..(store + 25).min(lines.len())];
        let retain = tail
            .iter()
            .position(|line| line.contains("__rt_incref"))
            .unwrap_or_else(|| {
                panic!("static object store at line {store} never retains the object\n{assembly}")
            });
        let release = tail
            .iter()
            .position(|line| line.contains("__rt_decref_mixed"))
            .unwrap_or_else(|| {
                panic!("static object store at line {store} never releases the adopted Mixed box\n{assembly}")
            });
        assert!(
            retain < release,
            "static object store at line {store} releases the adopted Mixed box before retaining the object\n{assembly}"
        );
    }
}

/// Tests calling a class static method with a string parameter and concatenating the result.
#[test]
fn test_class_static_method_string_param() {
    let out = compile_and_run(
        r#"<?php
class Utils {
    public static function greet($name) { return "Hello " . $name; }
}
echo Utils::greet("World");
"#,
    );
    assert_eq!(out, "Hello World");
}

/// Tests calling a class static method that returns a new instance via `new`, then
/// invoking an instance method on the returned object.
#[test]
fn test_class_static_and_instance() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public $n;
    public function __construct($n) { $this->n = $n; }
    public function next() { return $this->n + 1; }
    public static function make($n) { return new Counter($n); }
}
$c = Counter::make(4);
echo $c->next();
"#,
    );
    assert_eq!(out, "5");
}

// === Nested array access tests ===

/// Tests static property read of an `int` typed static, followed by a write, then another read.
#[test]
fn test_static_property_read_write() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public static int $count = 1;
}
echo Counter::$count;
Counter::$count = 5;
echo Counter::$count;
"#,
    );
    assert_eq!(out, "15");
}

/// Tests `self::$prop` access and mutation of an `int` typed static within a static method.
/// Verifies that `self::` resolves to the declaring class, not the called class.
#[test]
fn test_static_property_self_access_in_static_method() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public static int $count = 1;
    public static function bump() {
        self::$count = self::$count + 1;
        return self::$count;
    }
}
echo Counter::bump();
echo Counter::bump();
"#,
    );
    assert_eq!(out, "23");
}

/// Tests `parent::$prop` access to a `protected` static property from a child class.
#[test]
fn test_static_property_parent_access_in_static_method() {
    let out = compile_and_run(
        r#"<?php
class Base {
    protected static int $seed = 4;
}
class Child extends Base {
    public static function read() {
        return parent::$seed;
    }
}
echo Child::read();
"#,
    );
    assert_eq!(out, "4");
}

/// Tests that a non-redeclared static property has a single shared slot across inheritance
/// when accessed via `static::$prop` from a parent class method.
#[test]
fn test_static_property_inherited_storage_is_shared() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public static int $count = 2;
    public static function set($value) {
        static::$count = $value;
    }
}
class Child extends Base {}
Child::set(9);
echo Base::$count;
echo Child::$count;
"#,
    );
    assert_eq!(out, "99");
}

/// Tests that a redeclared static property creates a separate storage slot per class,
/// and that `static::$prop` and `$obj::$prop` both dispatch to the late-bound class's slot.
#[test]
fn test_static_property_redeclaration_uses_child_storage() {
    let out = compile_and_run(
        r#"<?php
class Base {
    public static int $count = 1;
    public static function get() {
        return static::$count;
    }
    public static function set($value) {
        static::$count = $value;
    }
}
class Child extends Base {
    public static int $count = 2;
}
echo Base::get() . ":" . Child::get() . ":";
Child::set(9);
echo Base::$count . ":" . Child::$count;
"#,
    );
    assert_eq!(out, "1:2:1:9");
}

/// Tests appending to and updating a static `array` property directly via `$Class::$prop[idx]`.
#[test]
fn test_static_property_direct_array_writes() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static array $items = [];
}
Registry::$items[] = 4;
Registry::$items[] = 5;
Registry::$items[1] = 8;
echo Registry::$items[0] . ":" . Registry::$items[1];
"#,
    );
    assert_eq!(out, "4:8");
}

/// Tests `+=` and `*=` compound assignment on an `int` typed static property.
#[test]
fn test_static_property_compound_assign() {
    let out = compile_and_run(
        r#"<?php
class Counter {
    public static int $count = 4;
}
Counter::$count += 6;
Counter::$count *= 2;
echo Counter::$count;
"#,
    );
    assert_eq!(out, "20");
}

/// Tests `+=` and `-=` compound assignment on individual elements of a static `array` property.
#[test]
fn test_static_property_array_compound_assign() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static $items = [3, 5, 7];
}
Registry::$items[0] += 9;
Registry::$items[2] -= 4;
echo Registry::$items[0] . ":" . Registry::$items[2];
"#,
    );
    assert_eq!(out, "12:3");
}

/// Tests that the index expression in `Registry::$items[idx()] += 6` is evaluated exactly once.
/// The side-effect function `idx()` echoes "i" and the result proves no double-evaluation.
#[test]
fn test_static_property_array_compound_assign_evaluates_index_once() {
    let out = compile_and_run(
        r#"<?php
class Registry {
    public static $items = [3, 5, 7];
}

function idx() {
    echo "i";
    return 1;
}

Registry::$items[idx()] += 6;
echo ":" . Registry::$items[1];
"#,
    );
    assert_eq!(out, "i:11");
}

/// Tests that `static::$items[]` and `static::$items[0]` in a parent method write to the
/// late-bound class's redeclared static array, not the parent's, when called on a child.
#[test]
fn test_static_property_redeclared_array_writes_are_late_bound() {
    let out = compile_and_run(
        r#"<?php
class BaseBag {
    public static array $items = [];
    public static function add($value) {
        static::$items[] = $value;
    }
    public static function replaceFirst($value) {
        static::$items[0] = $value;
    }
    public static function first() {
        return static::$items[0];
    }
}
class ChildBag extends BaseBag {
    public static array $items = [];
}
BaseBag::add(1);
ChildBag::add(2);
ChildBag::replaceFirst(7);
echo BaseBag::first() . ":" . ChildBag::first();
"#,
    );
    assert_eq!(out, "1:7");
}

/// Tests that `static::$count` inside a parent static method causes a fatal error when
/// the calling class has a private redeclaration of the same static property.
#[test]
fn test_static_property_late_bound_private_redeclaration_read_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Base {
    private static int $count = 1;
    public static function read() {
        echo static::$count;
    }
}
class Child extends Base {
    private static int $count = 2;
}
Child::read();
"#,
    );
    assert!(
        err.contains("Cannot access private static property"),
        "{err}"
    );
}

/// Tests that `static::$count` inside a parent static method causes a fatal error when
/// the calling class has a private redeclaration and the property is being written.
#[test]
fn test_static_property_late_bound_private_redeclaration_write_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
class Base {
    private static int $count = 1;
    public static function write() {
        static::$count = 3;
    }
}
class Child extends Base {
    private static int $count = 2;
}
Child::write();
"#,
    );
    assert!(
        err.contains("Cannot access private static property"),
        "{err}"
    );
}

/// Tests typed `string` static property read and write.
#[test]
fn test_static_string_property_assignment() {
    let out = compile_and_run(
        r#"<?php
class Labels {
    public static string $name = "a";
}
echo Labels::$name;
Labels::$name = "bc";
echo Labels::$name;
"#,
    );
    assert_eq!(out, "abc");
}

/// Tests post-increment `A::$x++` on a static property (regression for #372).
#[test]
fn test_static_property_post_increment() {
    let out = compile_and_run(
        r#"<?php
class A { public static $x = 0; }
A::$x++;
echo A::$x;
"#,
    );
    assert_eq!(out, "1");
}

/// Tests pre-increment `++A::$x` on a static property (regression for #372).
#[test]
fn test_static_property_pre_increment() {
    let out = compile_and_run(
        r#"<?php
class A { public static $x = 0; }
++A::$x;
echo A::$x;
"#,
    );
    assert_eq!(out, "1");
}

/// Tests post-decrement and pre-decrement `--A::$x` / `A::$x--` on a static property.
#[test]
fn test_static_property_decrement() {
    let out = compile_and_run(
        r#"<?php
class A { public static $x = 5; }
A::$x--;
echo A::$x;
echo "\n";
--A::$x;
echo A::$x;
"#,
    );
    assert_eq!(out, "4\n3");
}

/// Tests `self::$x++` and `++self::$x` inside a static method.
#[test]
fn test_static_property_self_increment_in_method() {
    let out = compile_and_run(
        r#"<?php
class A {
    public static $x = 0;
    public static function inc(): void {
        self::$x++;
        ++self::$x;
    }
}
A::inc();
echo A::$x;
"#,
    );
    assert_eq!(out, "2");
}

/// Tests `static::$x++` and `++static::$x` inside a static method use late-static storage.
#[test]
fn test_static_property_static_increment_in_method() {
    let out = compile_and_run(
        r#"<?php
class A {
    public static $x = 0;
    public static function inc(): void {
        static::$x++;
        ++static::$x;
    }
}
A::inc();
echo A::$x;
"#,
    );
    assert_eq!(out, "2");
}

/// Tests `parent::$x++` and `++parent::$x` inside a child static method.
#[test]
fn test_static_property_parent_increment_in_method() {
    let out = compile_and_run(
        r#"<?php
class A { public static $x = 0; }
class B extends A {
    public static function inc(): void {
        parent::$x++;
        ++parent::$x;
    }
}
B::inc();
echo A::$x;
"#,
    );
    assert_eq!(out, "2");
}

/// Regression: storing a *borrowed* object (an interface-typed parameter) into a
/// `static` class property, then reading it back in a different scope and
/// dispatching methods on it must work. The store consumes (moves) its operand,
/// so a borrowed value must be acquired first; without the acquire the property
/// dangled once the caller released the borrow, and the later dispatch crashed
/// with a fatal "Call to a member function ... on null". Also exercises multiple
/// method calls plus a cross-interface `instanceof` downcast to confirm the
/// class tag survives every load (not just the first).
#[test]
fn test_static_property_holds_borrowed_object_for_later_dispatch() {
    let out = compile_and_run(
        r#"<?php
interface Store { public function get(): string; }
interface Named { public function name(): string; }
class Holder { public static ?Store $s = null; }
class Impl implements Store, Named {
    public function get(): string { return "V"; }
    public function name(): string { return "N"; }
}
function register(?Store $x): void { Holder::$s = $x; }
register(new Impl());
$o = Holder::$s;
if ($o !== null) {
    echo $o->get();
    echo $o->get();
    if ($o instanceof Named) { echo $o->name(); }
    echo $o->get();
}
"#,
    );
    assert_eq!(out, "VVNV");
}
