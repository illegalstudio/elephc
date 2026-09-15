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
