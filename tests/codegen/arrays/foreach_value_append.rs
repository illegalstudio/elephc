//! Purpose:
//! Regression tests for appending a by-value `foreach` loop variable into
//! another array that crosses a function-return boundary (issue #405).
//!
//! Called from:
//! - `cargo test` through the `codegen_tests` harness via `crate::support`.
//!
//! Key details:
//! - `foreach_value_type` now keeps concrete scalar element types (`int`,
//!   `float`, `bool`, `string`) for the by-value loop variable instead of
//!   degrading them to a boxed `Mixed` local. Previously the appended box
//!   widened the array under construction to `array<mixed>` at runtime while
//!   the checker-side function signature kept the concrete element type, so
//!   the caller read box pointers through the concrete element layout
//!   (garbage ints, or a fatal "heap memory exhausted" on string reads).
//! - Concrete `Str` results of `iter_current_value` are borrowed pointers
//!   into the source array's payload (like `ArrayGet` string results), NOT
//!   owning temporaries: the retaining store must not release them, or the
//!   source array's string block is freed while still referenced and the
//!   next `__rt_str_persist` reuses it (use-after-free corruption).

use crate::support::*;

/// Issue #405 minimal repro: appending the foreach value of an exploded CSV
/// and returning the array previously printed nothing and exhausted the heap
/// when the caller read it back.
#[test]
fn test_foreach_string_value_append_survives_function_return() {
    let out = compile_and_run(
        r#"<?php
function collect(string $csv): array {
    $out = [];
    foreach (explode(',', $csv) as $item) {
        $out[] = $item;
    }
    return $out;
}
$r = collect('a,b,c');
echo $r[0], "\n";
foreach ($r as $e) {
    echo $e;
}"#,
    );
    assert_eq!(out, "a\nabc");
}

/// Same shape with int elements: the caller previously printed the address of
/// the Mixed box instead of the element value.
#[test]
fn test_foreach_int_value_append_survives_function_return() {
    let out = compile_and_run(
        r#"<?php
function collect(): array {
    $out = [];
    foreach ([1, 2, 3] as $item) {
        $out[] = $item;
    }
    return $out;
}
$r = collect();
echo $r[0], $r[1], $r[2];"#,
    );
    assert_eq!(out, "123");
}

/// Float elements keep their concrete register and array-slot representation
/// when the collected array crosses the function-return boundary.
#[test]
fn test_foreach_float_value_append_survives_function_return() {
    let out = compile_and_run(
        r#"<?php
function collect(): array {
    $out = [];
    foreach ([1.25, 2.5, 3.75] as $item) {
        $out[] = $item;
    }
    return $out;
}
$r = collect();
echo $r[0], "|", $r[1], "|", $r[2];"#,
    );
    assert_eq!(out, "1.25|2.5|3.75");
}

/// Bool elements remain unboxed across the function-return boundary instead
/// of being read back through a widened `Mixed` array layout.
#[test]
fn test_foreach_bool_value_append_survives_function_return() {
    let out = compile_and_run(
        r#"<?php
function collect(): array {
    $out = [];
    foreach ([true, false] as $item) {
        $out[] = $item;
    }
    return $out;
}
$r = collect();
if ($r[0]) { echo "T"; }
if (!$r[1]) { echo "F"; }"#,
    );
    assert_eq!(out, "TF");
}

/// The string ownership fix remains clean under allocator poisoning and leak
/// reporting, guarding both the original UAF and a future missing release.
#[test]
fn test_foreach_string_value_append_heap_debug_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function collect(string $csv): array {
    $out = [];
    foreach (explode(',', $csv) as $item) {
        $out[] = $item;
    }
    return $out;
}
$r = collect('a,b,c');
echo $r[0], $r[1], $r[2];"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "abc");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Appending the foreach value of an inline literal source: the borrowed
/// current-value string must not be released into the source array (its block
/// was reused by the append's str_persist and then zeroed by the source's
/// deep-free at loop exit).
#[test]
fn test_foreach_string_value_append_inline_literal_source() {
    let out = compile_and_run(
        r#"<?php
$out = [];
foreach (["a", "b"] as $item) {
    $out[] = $item;
}
echo $out[0], $out[1];"#,
    );
    assert_eq!(out, "ab");
}

/// The concrete-typed loop variable still supports reads after the loop and
/// owned reassignment inside the body.
#[test]
fn test_foreach_string_value_local_reads_and_reassignment() {
    let out = compile_and_run(
        r#"<?php
$out = [];
foreach (["a", "b"] as $item) {
    $item = $item . "!";
    $out[] = $item;
}
echo $out[0], $out[1], "|", $item;"#,
    );
    assert_eq!(out, "a!b!|b!");
}

/// A generic `array` return with an empty branch specializes to the associative
/// shape built by concrete string values from `foreach`. The empty branch must
/// be converted to the same hash ABI instead of requesting an unsupported
/// `AssocArray -> Array(Mixed)` runtime conversion (the web `ini_get_all()` CI
/// regression exposed by issue #405).
#[test]
fn test_foreach_string_value_key_preserves_assoc_return_with_empty_branch() {
    let out = compile_and_run(
        r#"<?php
function collect(bool $empty): array {
    if ($empty) { return []; }
    $out = [];
    foreach (["name", "role"] as $key) {
        $out[$key] = strtoupper($key);
    }
    return $out;
}
$empty = collect(true);
$values = collect(false);
echo count($empty), "|", $values["name"], "|", $values["role"];
"#,
    );
    assert_eq!(out, "0|NAME|ROLE");
}
