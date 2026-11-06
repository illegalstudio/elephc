//! Purpose:
//! Checks strip_tags intermediate owners and foreach cleanup failures from review.
//!
//! Called from:
//! - The string encoding codegen integration tests.
//!
//! Key details:
//! - Native normalized arrays survive throwing element conversions without leaking.
//! - Large native allow-list intermediates are retired after their final use.
//! - Saved large results keep independent storage after runtime-result cleanup.
//! - Eval destructor failures retire displaced return and throw payloads.

use crate::support::*;

/// A throwing element conversion retires the independently normalized allow array.
#[test]
fn strip_tags_native_throwing_array_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ThrowingTag {
    public function __toString(): string { throw new Exception('tag'); }
}
function stripArray(mixed $allow): string {
    return strip_tags(str_repeat('<p>x</p>', 2), $allow);
}
function invokeArray(callable $callback, mixed $allow): string {
    return $callback('<p>x</p>', $allow);
}
for ($i = 0; $i < 4; $i++) {
    try { stripArray([new ThrowingTag()]); }
    catch (Throwable $error) { echo $error->getMessage(), '|'; }
    unset($error);
    try { invokeArray(strip_tags(...), [new ThrowingTag()]); }
    catch (Throwable $error) { echo $error->getMessage(), '|'; }
    unset($error);
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "tag|".repeat(8));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Throw cleanup is balanced with IR optimization disabled too.
#[test]
fn strip_tags_native_throwing_array_owners_no_ir_opt() {
    without_ir_opt(strip_tags_native_throwing_array_owners);
}

/// Large joined and wrapped allow strings release heap fallbacks on every call.
#[test]
fn strip_tags_native_large_allow_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function stripLarge(mixed $allow): string { return strip_tags('<b>x</b>', $allow); }
function invokeLarge(callable $callback, mixed $allow): string { return $callback('<b>x</b>', $allow); }
$width = 70000 + $argc;
for ($i = 0; $i < 4; $i++) {
    echo strip_tags('<b>x</b>', [str_repeat('p', $width)]);
    echo stripLarge([str_repeat('p', $width)]);
    echo invokeLarge(strip_tags(...), [str_repeat('p', $width)]);
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "xxxxxxxxxxxx");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Large allow-list cleanup does not depend on EIR optimization.
#[test]
fn strip_tags_native_large_allow_owners_no_ir_opt() {
    without_ir_opt(strip_tags_native_large_allow_owners);
}

/// Producing the large source array alone must not corrupt heap accounting.
#[test]
fn strip_tags_native_large_allow_generation_control() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$width = 70000 + $argc;
for ($i = 0; $i < 4; $i++) {
    $allow = [str_repeat('p', $width)];
    echo strlen($allow[0]) > 70000 ? 'C' : 'bad';
    unset($allow);
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "CCCC");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A large string allow-list isolates the strip helper from array joining.
#[test]
fn strip_tags_native_large_string_allow_control() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$allow = str_repeat('p', 70000 + $argc);
for ($i = 0; $i < 4; $i++) { echo strip_tags('<b>x</b>', $allow); }
unset($allow);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "xxxx");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Shared implode cleanup independently handles the large array normalization.
#[test]
fn strip_tags_native_large_implode_control() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function joinLarge(mixed $allow): string { return implode('><', $allow); }
$width = 70000 + $argc;
for ($i = 0; $i < 4; $i++) {
    $allow = [str_repeat('p', $width)];
    echo strlen(implode('><', $allow)) > 70000 ? 'I' : 'bad';
    echo strlen(joinLarge($allow)) > 70000 ? 'I' : 'bad';
    unset($allow);
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "IIIIIIII");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Saving a heap-backed result keeps its bytes after the runtime result owner is released.
#[test]
fn strip_tags_native_saved_large_result_is_independent() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$width = 70000 + $argc;
$saved = strip_tags(str_repeat('x', $width));
$overwrite = str_repeat('z', $width);
echo substr($saved, 0, 1), '|', substr($saved, -1), '|', strlen($saved), '|';
unset($overwrite, $saved);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "x|x|70001|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Saved heap-backed results remain independent with EIR optimization disabled too.
#[test]
fn strip_tags_native_saved_large_result_is_independent_no_ir_opt() {
    without_ir_opt(strip_tags_native_saved_large_result_is_independent);
}

/// A foreach receiver destructor retires any return or throw it displaces.
#[test]
fn strip_tags_eval_foreach_destructor_displaced_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$source = $argc > 0 ? '
class CleanupIter implements Iterator {
    public function current(): mixed { return "entry"; }
    public function key(): mixed { return 0; }
    public function next(): void {}
    public function rewind(): void {}
    public function valid(): bool { return true; }
    public function __destruct() { throw new Exception("cleanup"); }
}
function leaveLoop(bool $fail): string {
    foreach (new CleanupIter() as $entry) {
        if ($fail) { throw new Exception(str_repeat("original", 5)); }
        return str_repeat("return", 5);
    }
    return "";
}
for ($i = 0; $i < 4; $i++) {
    foreach ([false, true] as $fail) {
        try { leaveLoop($fail); }
        catch (Throwable $error) { echo $error->getMessage(), "|"; }
        unset($error);
    }
}
' : '';
eval($source);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "cleanup|".repeat(8));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Destructor displacement cleanup also holds without EIR optimization.
#[test]
fn strip_tags_eval_foreach_destructor_displaced_owners_no_ir_opt() {
    without_ir_opt(strip_tags_eval_foreach_destructor_displaced_owners);
}
