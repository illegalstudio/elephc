//! Purpose:
//! Regressions for nested update dispatch, scalar parent errors and false write inference.
//!
//! Called from:
//! - The objects codegen integration suite.
//!
//! Key details:
//! - Every case checks temporary ownership with and without EIR optimization.
//! - Coalescing remains lazy and write-only conversion never broadens ordinary reads.

use crate::support::*;

/// Checks output and complete owner cleanup for one nested write fixture.
fn assert_write(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A missing coalescing leaf converts false only after the lazy RHS runs.
#[test]
fn test_nested_write_eleventh_review_static_coalesce() {
    assert_write(r#"<?php
class D { public static array $items = [[false]]; }
function rhs(): int { echo 'R'; return 4; }
$copy = D::$items;
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
D::$items[0][0][1] ??= rhs();
D::$items[0][0][1] ??= rhs();
echo json_encode(D::$items), ':', json_encode($copy);
restore_error_handler(); D::$items = []; unset($copy);
"#, r#"R8192|[[{"1":4}]]:[[false]]"#);
}

/// Coalescing dispatch does not depend on EIR optimization.
#[test]
fn test_nested_write_eleventh_review_static_coalesce_no_ir_opt() {
    without_ir_opt(test_nested_write_eleventh_review_static_coalesce);
}

/// Local prefix updates convert their attached false parent before reading the missing leaf.
#[test]
fn test_nested_write_eleventh_review_local_increment() {
    assert_write(r#"<?php
$items = [[false], [null]]; $copy = $items;
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
++ $items[0][0][1];
echo json_encode($items), ':', json_encode($copy);
restore_error_handler(); unset($items, $copy);
"#, r#"8192|2|[[{"1":1}],[null]]:[[false],[null]]"#);
}

/// Local prefix updates use the same attached parent without optimization.
#[test]
fn test_nested_write_eleventh_review_local_increment_no_ir_opt() {
    without_ir_opt(test_nested_write_eleventh_review_local_increment);
}

/// Local compound updates retain the converted parent and preserve a by-value copy.
#[test]
fn test_nested_write_eleventh_review_local_compound() {
    assert_write(r#"<?php
$items = [[false], [null]]; $copy = $items;
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
$items[0][0][1] += 4;
echo json_encode($items), ':', json_encode($copy);
restore_error_handler(); unset($items, $copy);
"#, r#"8192|2|[[{"1":4}],[null]]:[[false],[null]]"#);
}

/// Local compound updates preserve false conversion without optimization.
#[test]
fn test_nested_write_eleventh_review_local_compound_no_ir_opt() {
    without_ir_opt(test_nested_write_eleventh_review_local_compound);
}

/// Integer, float and true parents throw after computed keys and the RHS have run.
#[test]
fn test_nested_write_eleventh_review_scalar_parents() {
    assert_write(r#"<?php
function leafKey(): int { echo 'K'; return 1; }
function rhs(): int { echo 'R'; return 9; }
function write(mixed $parent): void {
    $items = [[$parent]]; $copy = $items;
    try { $items[0][0][leafKey()] = rhs(); }
    catch (Error $e) { echo $e->getMessage(), ':'; }
    echo json_encode($items), ':', json_encode($copy), '|';
}
write(0); write(1.5); write(true);
"#, "KRCannot use a scalar value as an array:[[0]]:[[0]]|KRCannot use a scalar value as an array:[[1.5]]:[[1.5]]|KRCannot use a scalar value as an array:[[true]]:[[true]]|");
}

/// Scalar parent guards retain their behavior without optimization.
#[test]
fn test_nested_write_eleventh_review_scalar_parents_no_ir_opt() {
    without_ir_opt(test_nested_write_eleventh_review_scalar_parents);
}

/// Recursive target inference permits a statically false ancestor without mutating its copy.
#[test]
fn test_nested_write_eleventh_review_false_ancestor() {
    assert_write(r#"<?php
$items = [false]; $copy = $items;
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
$items[0][0][1] = 9;
echo json_encode($items), ':', json_encode($copy);
restore_error_handler(); unset($items, $copy);
"#, r#"8192|[[{"1":9}]]:[false]"#);
}

/// Recursive false-parent inference remains valid without optimization.
#[test]
fn test_nested_write_eleventh_review_false_ancestor_no_ir_opt() {
    without_ir_opt(test_nested_write_eleventh_review_false_ancestor);
}

/// A throwing local conversion callback stops the leaf read after the already evaluated RHS.
#[test]
fn test_nested_write_eleventh_review_local_update_throw() {
    assert_write(r#"<?php
function rhs(): int { echo 'R'; return 4; }
$items = [[false], [null]];
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; throw new Error('stop'); });
try { $items[0][0][1] += rhs(); } catch (Error $e) { echo $e->getMessage(), ':'; }
restore_error_handler(); echo json_encode($items); unset($items, $e);
"#, r#"R8192|stop:[[[]],[null]]"#);
}

/// Throwing conversion callbacks preserve update ordering without optimization.
#[test]
fn test_nested_write_eleventh_review_local_update_throw_no_ir_opt() {
    without_ir_opt(test_nested_write_eleventh_review_local_update_throw);
}

/// Coalescing retains dimensions selected before the RHS changes a variable key.
#[test]
fn test_nested_write_eleventh_review_coalesce_key_snapshot() {
    assert_write(r#"<?php
class D { public static array $items = [[false]]; }
$key = 1;
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
D::$items[0][0][$key] ??= ($key = 2);
echo json_encode(D::$items), ':', $key;
restore_error_handler(); D::$items = [];
"#, r#"8192|[[{"2":2}]]:2"#);
}

/// Coalescing key selection remains stable without optimization.
#[test]
fn test_nested_write_eleventh_review_coalesce_key_snapshot_no_ir_opt() {
    without_ir_opt(test_nested_write_eleventh_review_coalesce_key_snapshot);
}

/// Computed local dimensions and eager RHS captures run once in PHP source order.
#[test]
fn test_nested_write_eleventh_review_computed_local_update() {
    assert_write(r#"<?php
function parentKey(): int { echo 'P'; return 0; }
function leafKey(): int { echo 'K'; return 1; }
function rhs(): int { echo 'R'; return 4; }
$items = [[false], [null]]; $copy = $items;
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
$items[parentKey()][0][leafKey()] += rhs();
echo json_encode($items), ':', json_encode($copy);
restore_error_handler(); unset($items, $copy);
"#, r#"PKR8192|2|[[{"1":4}],[null]]:[[false],[null]]"#);
}

/// Captured local update inputs run once without optimization.
#[test]
fn test_nested_write_eleventh_review_computed_local_update_no_ir_opt() {
    without_ir_opt(test_nested_write_eleventh_review_computed_local_update);
}

/// Detaching a local parent during conversion avoids a stale undefined-key warning.
#[test]
fn test_nested_write_eleventh_review_local_update_detachment() {
    assert_write(r#"<?php
$items = [[false], [null]];
set_error_handler(function(int $level, string $message) use (&$items): bool {
    echo $level, '|'; $items = [[8]]; return true;
});
$items[0][0][1] += 4;
restore_error_handler(); echo json_encode($items); unset($items);
"#, "8192|[[8]]");
}

/// Local detached-parent handling does not depend on optimization.
#[test]
fn test_nested_write_eleventh_review_local_update_detachment_no_ir_opt() {
    without_ir_opt(test_nested_write_eleventh_review_local_update_detachment);
}

/// Numeric parent guards also cover intermediate fetches and direct deep appends.
#[test]
fn test_nested_write_eleventh_review_scalar_fetch_and_append() {
    assert_write(r#"<?php
function write(mixed $parent): void {
    $items = [[$parent]];
    try { $items[0][0][0][1] = 9; } catch (Error $e) { echo $e->getMessage(), '|'; }
    try { $items[0][0][] = 9; } catch (Error $e) { echo $e->getMessage(), '|'; }
    echo json_encode($items), ':';
}
write(0); write(1.5);
"#, "Cannot use a scalar value as an array|Cannot use a scalar value as an array|[[0]]:Cannot use a scalar value as an array|Cannot use a scalar value as an array|[[1.5]]:");
}

/// Intermediate numeric guards cover unoptimized stores and appends.
#[test]
fn test_nested_write_eleventh_review_scalar_fetch_and_append_no_ir_opt() {
    without_ir_opt(test_nested_write_eleventh_review_scalar_fetch_and_append);
}

/// Boxed Mixed and union float keys report their PHP value instead of raw payload bits.
#[test]
fn test_nested_write_twelfth_review_boxed_float_key_diagnostics() {
    assert_write(r#"<?php
function mixedKey(): mixed { return 0.5; }
function unionKey(): int|float { return 0.5; }
function updateMixed(mixed $key): void {
    $items = [[false]];
    $items[$key][0][1] += 4;
    echo json_encode($items), '|';
}
function updateUnion(int|float $key): void {
    $items = [[false]];
    $items[$key][0][1] += 4;
    echo json_encode($items), '|';
}
set_error_handler(function(int $level, string $message): bool {
    echo $level, ':', $message, '|'; return true;
});
updateMixed(mixedKey()); updateUnion(unionKey());
restore_error_handler();
"#, "8192:Implicit conversion from float 0.5 to int loses precision|8192:Automatic conversion of false to array is deprecated|2:Undefined array key 1|[[{\"1\":4}]]|".repeat(2).as_str());
}

/// Boxed float-key diagnostics remain value-correct without EIR optimization.
#[test]
fn test_nested_write_twelfth_review_boxed_float_key_diagnostics_no_ir_opt() {
    without_ir_opt(test_nested_write_twelfth_review_boxed_float_key_diagnostics);
}

/// Boxed integer keys report the integer payload in missing-key diagnostics.
#[test]
fn test_nested_write_twelfth_review_boxed_integer_key_diagnostics() {
    assert_write(r#"<?php
function mixedKey(): mixed { return 0; }
function unionKey(): int|float { return 0; }
function updateMixed(mixed $key): void {
    $items = [[false]]; ++$items[$key][0][1]; echo json_encode($items), '|';
}
function updateUnion(int|float $key): void {
    $items = [[false]]; ++$items[$key][0][1]; echo json_encode($items), '|';
}
set_error_handler(function(int $level, string $message): bool {
    echo $level, ':', $message, '|'; return true;
});
updateMixed(mixedKey()); updateUnion(unionKey());
restore_error_handler();
"#, "8192:Automatic conversion of false to array is deprecated|2:Undefined array key 1|[[{\"1\":1}]]|".repeat(2).as_str());
}

/// Boxed integer-key diagnostics remain value-correct without EIR optimization.
#[test]
fn test_nested_write_twelfth_review_boxed_integer_key_diagnostics_no_ir_opt() {
    without_ir_opt(test_nested_write_twelfth_review_boxed_integer_key_diagnostics);
}

/// A string used as an intermediate parent throws instead of silently dropping the write.
#[test]
fn test_nested_write_twelfth_review_string_intermediate_parent() {
    assert_write(r#"<?php
function write(mixed $parent): void {
    $items = [[$parent]]; $copy = $items;
    try { $items[0][0][1][2] = 9; }
    catch (Error $error) { echo $error->getMessage(), ':'; }
    echo json_encode($items), ':', json_encode($copy);
    unset($items, $copy, $error);
}
write('abc');
"#, "Cannot use string offset as an array:[[\"abc\"]]:[[\"abc\"]]");
}

/// Intermediate string-parent errors remain catchable without EIR optimization.
#[test]
fn test_nested_write_twelfth_review_string_intermediate_parent_no_ir_opt() {
    without_ir_opt(test_nested_write_twelfth_review_string_intermediate_parent);
}

/// Compound and incdec updates convert statically false local elements in write context.
#[test]
fn test_nested_write_twelfth_review_static_false_updates() {
    assert_write(r#"<?php
$items = [false, false]; $copy = $items;
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
$items[0][1] += 4;
++ $items[1][1];
echo json_encode($items), ':', json_encode($copy);
restore_error_handler(); unset($items, $copy);
"#, r#"8192|2|8192|2|[{"1":4},{"1":1}]:[false,false]"#);
}

/// Statically false local updates compile and run without EIR optimization.
#[test]
fn test_nested_write_twelfth_review_static_false_updates_no_ir_opt() {
    without_ir_opt(test_nested_write_twelfth_review_static_false_updates);
}

/// False-or-null write parents retain both legal autovivification paths.
#[test]
fn test_nested_write_twelfth_review_false_null_union_parent() {
    assert_write(r#"<?php
function parentValue(int $which): false|null { return $which ? false : null; }
$items = [parentValue(1), parentValue(0)]; $copy = $items;
set_error_handler(function(int $level, string $message): bool { echo $level, '|'; return true; });
$items[0][1] = 9;
$items[1][1] = 8;
echo json_encode($items), ':', json_encode($copy);
restore_error_handler(); unset($items, $copy);
"#, r#"8192|[{"1":9},{"1":8}]:[false,null]"#);
}

/// False-or-null write parents remain accepted without EIR optimization.
#[test]
fn test_nested_write_twelfth_review_false_null_union_parent_no_ir_opt() {
    without_ir_opt(test_nested_write_twelfth_review_false_null_union_parent);
}

/// Local nested incdec uses null and string value rules instead of arithmetic assignment.
#[test]
fn test_nested_write_twelfth_review_local_incdec_semantics() {
    assert_write(r#"<?php
$items = [[null, 'az']];
set_error_handler(function(int $level, string $message): bool {
    echo $level, ':', $message, '|'; return true;
});
--$items[0][0];
++$items[0][1];
echo json_encode($items);
restore_error_handler(); unset($items);
"#, "[[null,\"ba\"]]");
}

/// Local nested incdec semantics remain correct without EIR optimization.
#[test]
fn test_nested_write_twelfth_review_local_incdec_semantics_no_ir_opt() {
    without_ir_opt(test_nested_write_twelfth_review_local_incdec_semantics);
}
