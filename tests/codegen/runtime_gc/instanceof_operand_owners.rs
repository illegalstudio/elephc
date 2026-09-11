//! Purpose:
//! End-to-end heap-balance regressions for `instanceof` operand ownership.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - The backend `instanceof` entry and the eval introspection adapters only
//!   borrow the value/target operands, so the lowering must retire independently
//!   owned temporaries (a class-name string detached from boxed `Mixed` storage,
//!   a freshly constructed value operand) without freeing borrowed objects early.
//! - Every case runs under heap debug and the tagged null representation so both
//!   a missing release (leak) and an over-release (double free) fail the suite.

use crate::support::*;

/// Returns one bounded user-function region for ownership diagnostics on CI failures.
fn bounded_user_symbol_assembly(assembly: &str, symbol: &str) -> String {
    let label = format!("{symbol}:\n");
    let Some((_, tail)) = assembly.split_once(&label) else {
        return "<function assembly not found>".to_string();
    };
    let mut body = label;
    let mut eval_context_ready = None;
    for line in tail.lines() {
        let line = line.split_once(" // ").map_or(line, |(instruction, _)| instruction);
        let line = line.split_once(" # ").map_or(line, |(instruction, _)| instruction);
        let trimmed = line.trim_start();
        if matches!(
            trimmed.split_whitespace().next(),
            Some(".globl" | ".global")
        ) {
            break;
        }
        if let Some(ready) = eval_context_ready.as_deref() {
            if trimmed == format!("{ready}:") {
                body.push_str("    <eval context initialization omitted>\n");
                body.push_str(line);
                body.push('\n');
                eval_context_ready = None;
            }
            continue;
        }
        if trimmed
            .split_whitespace()
            .last()
            .is_some_and(|target| target.contains("_eval_context_ready_"))
        {
            eval_context_ready = trimmed
                .split_whitespace()
                .last()
                .map(str::to_string);
        }
        if !line.trim().is_empty()
            && !line.trim_start().starts_with("//")
            && !matches!(line.trim_start().as_bytes().first(), Some(b'#'))
        {
            body.push_str(line);
            body.push('\n');
        }
    }
    if body.chars().count() <= 128_000 {
        return body;
    }
    let head = body.chars().take(63_500).collect::<String>();
    let tail = body
        .chars()
        .rev()
        .take(63_500)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("{head}\n<user function middle omitted>\n{tail}")
}

/// A post-eval dynamic class-name target retires its detached string without leaking on repeat.
///
/// The target exists before opaque `eval()` widens live locals to boxed `Mixed` storage.
/// The later `$target = get_parent_class($object)` keeps that widened slot. Reading it for
/// `$object instanceof $target` detaches an owned string copy the backend entry and the eval
/// introspection adapter only borrow. Without the operand retirement this leaked one string per
/// call; the destructor and borrowed object owner must stay balanced across three iterations.
#[test]
fn test_instanceof_post_eval_dynamic_class_name_target_balances_detached_strings() {
    let source = r#"<?php
class InstGcEvalBase { public int $value = 5; }
class InstGcEvalChild extends InstGcEvalBase {
    public function __destruct() { echo "D|"; }
    public function classifyStatic(string $source): bool {
        eval($source);
        return $this instanceof static;
    }
}
function probeEvalInstanceofOwners(InstGcEvalChild $object, string $source): string {
    $target = "";
    eval($source);
    $target = get_parent_class($object);
    $named = $object instanceof InstGcEvalBase ? "n" : "-";
    $dynamic = $object instanceof $target ? "d" : "-";
    return $named . $dynamic . ":" . $target;
}
$source = 'return null; // ' . $argc;
for ($i = 0; $i < 3; $i++) {
    $object = new InstGcEvalChild();
    echo probeEvalInstanceofOwners($object, $source), "|";
    echo $object->classifyStatic($source) ? "static|" : "bad|";
    unset($object);
}
unset($source);
"#;
    let expected = "nd:InstGcEvalBase|static|D|".repeat(3);
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    let probe = bounded_user_symbol_assembly(&assembly, "_fn_probeEvalInstanceofOwners");
    let classify = bounded_user_symbol_assembly(&assembly, "_method_InstGcEvalChild_classifystatic");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}\nprobeEvalInstanceofOwners assembly:\n{}\nclassifyStatic assembly:\n{}",
        out.stderr, probe, classify);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// A freshly constructed owned value operand is freed exactly once across a dynamic target.
///
/// `(new InstGcOwnedChild()) instanceof $cls` makes the value operand an owned temporary. The
/// lowering roots it across the dynamic target evaluation and retires it after the predicate, so
/// the object is destroyed exactly once per call: a missing retirement leaks it and a double
/// retirement double-frees it, both caught by heap debug over three iterations.
#[test]
fn test_instanceof_owned_value_operand_across_dynamic_target_is_freed_once() {
    let source = r#"<?php
class InstGcOwnedBase {}
class InstGcOwnedChild extends InstGcOwnedBase { public function __destruct() {} }
function probeOwnedInstanceof(string $cls): string {
    return (new InstGcOwnedChild()) instanceof $cls ? "y" : "n";
}
for ($i = 0; $i < 3; $i++) {
    echo probeOwnedInstanceof("InstGcOwnedBase");
}
"#;
    let expected = "yyy";
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// A borrowed object operand survives the predicate and is destroyed only when its owner clears.
///
/// The tested object is a by-value (borrowed) parameter, so neither the named nor the dynamic
/// `instanceof` may free it. The caller re-tests the object after the call to prove it is still
/// alive. Only `unset()` runs the destructor, in that deterministic order, with a clean heap
/// across three iterations.
#[test]
fn test_instanceof_borrowed_object_operand_survives_until_owner_unset() {
    let source = r#"<?php
class InstGcBorrowBase {}
class InstGcBorrowChild extends InstGcBorrowBase { public function __destruct() { echo "D|"; } }
function classifyBorrowedInstanceof(InstGcBorrowChild $object, string $cls): string {
    $named = $object instanceof InstGcBorrowBase ? "n" : "-";
    $dynamic = $object instanceof $cls ? "d" : "-";
    return $named . $dynamic;
}
for ($i = 0; $i < 3; $i++) {
    $object = new InstGcBorrowChild();
    echo classifyBorrowedInstanceof($object, "InstGcBorrowBase"), "|";
    echo $object instanceof InstGcBorrowChild ? "alive|" : "dead|";
    unset($object);
}
"#;
    let expected = "nd|alive|D|".repeat(3);
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}

/// Unwinding during target evaluation retires both fresh and detached value operands.
#[test]
fn test_instanceof_throwing_target_retires_owned_and_detached_value_operands() {
    let source = r#"<?php
class InstGcThrowValue {}
function throwingInstGcTarget(): string { throw new RuntimeException("target"); }
function freshInstGcValue(): bool {
    return (new InstGcThrowValue()) instanceof (throwingInstGcTarget());
}
function detachedInstGcValue(string $source): bool {
    $value = "";
    eval($source);
    $value = str_repeat("value", 3);
    return $value instanceof (throwingInstGcTarget());
}
$source = 'return null; // ' . $argc;
for ($i = 0; $i < 3; $i++) {
    try { freshInstGcValue(); } catch (RuntimeException $error) { echo "object|"; }
    unset($error);
    try { detachedInstGcValue($source); } catch (RuntimeException $error) { echo "string|"; }
    unset($error);
}
unset($source);
"#;
    let expected = "object|string|".repeat(3);
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}
