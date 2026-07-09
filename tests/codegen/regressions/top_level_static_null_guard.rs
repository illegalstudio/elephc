//! Purpose:
//! Regression tests for the null-initialized `static` null-guard pattern at *file
//! scope* (top level), the mirror of the function-scoped null-guard tests. A
//! top-level `static $c = null; if ($c === null) { $c = [...]; }` previously
//! SIGSEGV'd (exit 139): at file scope the static's name is pre-seeded in
//! `local_types` from the collapsed global-env type, so the guard read kept that
//! stale concrete type instead of the widened `Mixed` slot. `$c === null` then
//! folded to a compile-time `false`, the initializer branch was skipped, and the
//! following element access dereferenced the still-null slot.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and stdout is compared.
//! - A top-level script runs once, so these pin the crash-free first-run value
//!   (a non-zero exit would fail `compile_and_run`); cross-request persistence of
//!   the same pattern is covered by the `--web-worker=script` e2e web tests.

use super::*;

/// Verifies a top-level `static $c = null` guarded by `if ($c === null)` builds its
/// array once and reads back a mutated element instead of crashing. Before the fix
/// this exact program exited 139 (null slot dereferenced by `hash_get`).
#[test]
fn test_top_level_static_null_guard_array() {
    let out = compile_and_run(
        r#"<?php
static $c = null;
if ($c === null) {
    $c = ['calls' => 0];
}
$c['calls']++;
echo $c['calls'];
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies a top-level `static $s = null` string null-guard initializes and reads
/// back without crashing, matching PHP's single-run result.
#[test]
fn test_top_level_static_null_guard_string() {
    let out = compile_and_run(
        r#"<?php
static $s = null;
if ($s === null) {
    $s = "x";
}
echo $s;
"#,
    );
    assert_eq!(out, "x");
}

/// Verifies a top-level `static $n = null` int null-guard (previously a hard
/// compile error for the function form, a SIGSEGV for the top-level form).
#[test]
fn test_top_level_static_null_guard_int() {
    let out = compile_and_run(
        r#"<?php
static $n = null;
if ($n === null) {
    $n = 42;
}
echo $n;
"#,
    );
    assert_eq!(out, "42");
}

/// Verifies a top-level `static $f = null` float null-guard initializes and reads
/// back the seeded value.
#[test]
fn test_top_level_static_null_guard_float() {
    let out = compile_and_run(
        r#"<?php
static $f = null;
if ($f === null) {
    $f = 2.5;
}
echo $f;
"#,
    );
    assert_eq!(out, "2.5");
}

/// Verifies a top-level `static $o = null` object null-guard constructs once and
/// reads back a mutated property instead of dereferencing the null slot.
#[test]
fn test_top_level_static_null_guard_object() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public int $n = 0;
}
static $o = null;
if ($o === null) {
    $o = new Box();
}
$o->n++;
echo $o->n;
"#,
    );
    assert_eq!(out, "1");
}

/// Verifies a top-level `static $x = null` that is never reassigned still reads back
/// as null (the null-guard's `=== null` must remain honest, not fold to a constant).
#[test]
fn test_top_level_static_null_guard_stays_null() {
    let out = compile_and_run(
        r#"<?php
static $x = null;
echo $x === null ? "isnull" : "notnull";
"#,
    );
    assert_eq!(out, "isnull");
}
