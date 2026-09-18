//! Purpose:
//! Verifies temporary operand ownership for writes through boxed PHP arrays.
//!
//! Called from:
//! - The runtime GC codegen suite on every executable target.
//!
//! Key details:
//! - Declared array returns keep writes on the boxed runtime path.
//! - Value aliases survive replacement and computed keys survive exceptional RHS evaluation.

use crate::support::*;

/// Replacing a callable in a boxed COW array retires the producer without losing either array owner.
#[test]
fn test_core_boxed_array_callable_replacement_retires_temporary() {
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(r#"<?php
class BoxedWriteCallback {
    public static function original(int $value): int { return $value + 1; }
    public static function replacement(int $value): int { return $value + 2; }
    public static function callbacks(): array { return [self::original(...)]; }
}
for ($i = 0; $i < 3; $i++) {
    $items = BoxedWriteCallback::callbacks();
    $copy = $items;
    $copy[0] = BoxedWriteCallback::replacement(...);
    $original = $items[0];
    $replacement = $copy[0];
    unset($items, $copy);
    echo $original(5), ":", $replacement(5), "|";
    unset($original, $replacement);
}
"#);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "6:7|6:7|6:7|", "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
}

/// Boxed writes retain borrowed values and retire fresh object, array, string and Mixed operands.
#[test]
fn test_core_boxed_array_writes_balance_fresh_and_borrowed_payloads() {
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(r#"<?php
class BoxedWriteObject { public int $value = 17; }
function boxedWriteItems(): array { return []; }
function boxedWriteValue(): mixed { return new BoxedWriteObject(); }
for ($i = 0; $i < 3; $i++) {
    $items = boxedWriteItems();
    $borrowed = new BoxedWriteObject();
    $items[0] = $borrowed;
    $items[0] = new BoxedWriteObject();
    $items[1] = [str_repeat("a", 12)];
    $items[2] = boxedWriteValue();
    $items[str_repeat("k", 8)] = str_repeat("v", 8);
    echo count($items), ":", $borrowed->value, "|";
    unset($items, $borrowed);
}
"#);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "4:17|4:17|4:17|", "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
}

/// A same-frame catch retires the computed key when the assignment RHS throws before the write.
#[test]
fn test_core_boxed_array_write_key_retires_on_rhs_throw() {
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(r#"<?php
function emptyBoxedWrite(): array { return []; }
function failedBoxedWriteValue(): mixed { throw new RuntimeException("write"); }
function catchBoxedWrite(): void {
    $items = emptyBoxedWrite();
    try { $items[str_repeat("k", 8)] = failedBoxedWriteValue(); }
    catch (RuntimeException $error) { echo count($items), "|"; unset($error); }
    unset($items);
}
for ($i = 0; $i < 3; $i++) { catchBoxedWrite(); }
"#);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "0|0|0|", "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
}

/// Taking a reference to a MISSING key of a Mixed-widened local inserts the entry instead of
/// writing into freed storage.
///
/// `$a` is promoted to a hash by its first string-key write, so its slot is boxed Mixed. The
/// missing-key path of `LoadArrayElemRefCell` publishes the receiver after `__rt_hash_to_mixed`
/// and then prepared it a second time before `__rt_hash_set`; on this slot the publish had moved
/// the load's owner into the new box, so the second prepare freed the table under the insert
/// (`requested array size exceeds the maximum allowed array size`). The second prepare now takes
/// the owner back before dropping the box. Output only: the element-reference path on this slot
/// shape still leaks its table (tracked separately), so the heap is not asserted clean here.
/// Reviewed on #893.
#[test]
fn test_boxed_hash_missing_key_reference_on_a_mixed_widened_local_inserts_the_entry() {
    let out = compile_and_run(r#"<?php
$t = "";
for ($i = 0; $i < 20; $i++) {
    $a = [];
    $a["x"] = 1;
    $r = &$a["k"];
    $r = 5;
    $t = ($a["k"] + count($a)) . ":" . implode(",", array_keys($a));
    unset($r);
}
echo $t, "|";
eval('echo "e";');
"#);
    assert_eq!(out, "7:x,k|e");
}

/// Repeated string-key promotion of a Mixed-widened local drops the slot's box exactly once.
///
/// `$a = []; $a["x"] = 1;` inside a loop of a program that also calls `eval()`: from the second
/// iteration on, the slot holds a Mixed box and the first write promotes the list to a hash.
/// Lowering used to release that box before `array_to_hash` while the backend releases it again
/// (`release_mutated_source_local_owner`, which also covers a slot that only widens later). The
/// second release freed the box a second time — silently on x86_64, whose heap-debug Mixed
/// release does not check liveness, and as `heap debug detected bad refcount` on aarch64, which
/// is where CI caught it. Lowering now leaves the drop to the backend for that conversion.
#[test]
fn test_boxed_string_key_promotion_in_a_loop_drops_the_previous_box_once() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$t = 0;
for ($i = 0; $i < 3; $i++) {
    $a = [];
    $a["x"] = 1;
    $a["y"] = 2;
    $t = count($a);
}
echo $t, "|";
eval('echo "e";');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "2|e", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

