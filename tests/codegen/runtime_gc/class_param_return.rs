//! Purpose:
//! End-to-end regressions for issue #1203: a function that returns a CLASS-typed parameter made
//! the caller release storage it only lent.
//!
//! Called from:
//! - `cargo test --test codegen_tests_runtime class_param_return` through Rust's test harness.
//!
//! Key details:
//! - The summary was always right; the CONSUMER never asked it.
//!   `value_is_borrowed_user_call_result` refuses when the argument is an owning temporary, and
//!   `value_is_owned_unboxed_local_load` answers true for any `LoadLocal` of a `PhpLocal` slot
//!   holding an object, array, assoc array or iterable. That answer is provisional — it exists so
//!   a later store can widen the slot — and the caller owes no release on the load itself.
//! - A `mixed`-typed load matches neither clause of that predicate and stays `maybe_owned`, which
//!   is why the same shape was already correct for `mixed` (#992) and not for a class type.
//! - The predicate covers `Array`, `AssocArray`, `Object` and `Iterable`, but only the OBJECT
//!   form was observably broken: the array fixture below passes with the fix reverted. It is kept
//!   as a guard that the fix did not disturb the shapes that were already right.
//! - Two call sites are required: a single call is inlined, which removes the boundary. The
//!   argument must reach the callee through a LOCAL; nested directly (`ident(pick($i))`) it is a
//!   genuine owning temporary and takes the other path, which must keep refusing (#486).

use crate::support::compile_and_run_with_heap_debug;

/// Verifies a class-typed parameter handed back to the caller is not released by it.
///
/// Measured before the fix: `Fatal error: heap debug detected bad refcount`, where PHP prints
/// `ok`. Without `--heap-debug` the refcount is silently one too low.
#[test]
fn test_a_returned_class_parameter_is_not_released_by_the_caller() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Tag { public string $s = "tag"; }

function pick(int $i): Tag { return new Tag(); }
function ident(Tag $value): Tag {
    if ($value->s === "tag") { return $value; }
    return $value;
}

for ($i = 0; $i < 5; $i++) {
    $tmp = pick($i);
    $value = ident($tmp);
}
echo "ok";
"#,
    );

    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ok");
    assert!(
        !out.stderr.contains("bad refcount"),
        "the caller released storage it only lent: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies the array and assoc-array shapes stay clean, which they already were.
///
/// `value_is_owned_unboxed_local_load` answers true for `Array`, `AssocArray`, `Object` and
/// `Iterable` alike, so all four looked equally exposed. Measured, they are not: this fixture
/// passes with the fix REVERTED, while the class one fails. Only the object form was observably
/// broken, and this pins that the fix did not disturb the rest. Distinct locals per shape,
/// because the checker refuses reassigning one across types.
#[test]
fn test_a_returned_array_parameter_is_not_released_by_the_caller() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function pickArr(int $i): array { return [1, 2, 3]; }
function pickAssoc(int $i): array { return ["a" => 1]; }
function identArr(array $v): array { if (count($v) > 0) { return $v; } return $v; }
function identAssoc(array $v): array { if (count($v) > 0) { return $v; } return $v; }

for ($i = 0; $i < 5; $i++) { $a = pickArr($i);   $ra = identArr($a); }
for ($i = 0; $i < 5; $i++) { $s = pickAssoc($i); $rs = identAssoc($s); }
echo "ok";
"#,
    );

    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ok");
    assert!(
        !out.stderr.contains("bad refcount"),
        "the caller released array storage it only lent: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies a genuinely owning temporary still refuses, so the fix did not widen too far.
///
/// `ident(pick($i))` passes the callee's result straight through with no local in between. That
/// argument IS a temporary the caller owes a release on, and treating the result as borrowed
/// would leave nobody to release it (#486).
#[test]
fn test_a_nested_owning_temporary_still_refuses() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Tag { public string $s = "tag"; }

function pick(int $i): Tag { return new Tag(); }
function ident(Tag $value): Tag {
    if ($value->s === "tag") { return $value; }
    return $value;
}

for ($i = 0; $i < 5; $i++) {
    $value = ident(pick($i));
}
echo "ok";
"#,
    );

    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "ok");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "a nested owning temporary must still be released exactly once: {}",
        out.stderr
    );
}
