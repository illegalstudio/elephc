//! Purpose:
//! Regression tests for reading an array offset on a scalar receiver.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - PHP warns "Trying to access array offset on <type>" and answers null. The warning is an
//!   ordinary suppressible warning, so `isset` and `??` reads stay silent.

use crate::support::*;

/// A scalar receiver answers PHP's null and names its runtime type in the warning instead of
/// handing the scalar to the container runtime as a pointer (#2025).
#[test]
fn test_scalar_array_offset_read_warns_and_returns_null() {
    let out = compile_and_run_capture(
        r#"<?php
function f(): string {
    $i = 42; $fl = 4.5; $t = true; $fa = false; $n = null;
    return gettype($i["a"]) . "|" . gettype($fl["a"]) . "|" . gettype($t["a"]) . "|"
        . gettype($fa["a"]) . "|" . gettype($n["a"]);
}
echo f(), "\n";
"#,
    );
    assert_eq!(out.stdout, "NULL|NULL|NULL|NULL|NULL\n");
    for kind in ["int", "float", "true", "false", "null"] {
        assert!(
            out.stderr.contains(&format!("Trying to access array offset on {kind}")),
            "missing {kind} warning: {}",
            out.stderr
        );
    }
}

/// The offset warning is suppressible: `isset` and `??` must not emit it.
#[test]
fn test_scalar_array_offset_read_is_silent_under_isset_and_coalesce() {
    let out = compile_and_run_capture(
        r#"<?php
function f(): string {
    $i = 42;
    return (isset($i["a"]) ? "set" : "unset") . "|" . ($i["a"] ?? "dflt");
}
echo f(), "\n";
"#,
    );
    assert_eq!(out.stdout, "unset|dflt\n");
    assert!(
        !out.stderr.contains("array offset"),
        "unexpected warning: {}",
        out.stderr
    );
}

/// A chained read keeps PHP's order: the scalar receiver warns and yields null, then the null
/// intermediate warns again.
#[test]
fn test_scalar_array_offset_chain_warns_twice() {
    let out = compile_and_run_capture(
        r#"<?php
function f(): string { $i = 42; return gettype($i["a"]["b"]); }
echo f(), "\n";
"#,
    );
    assert_eq!(out.stdout, "NULL\n");
    assert_eq!(
        out.stderr.matches("Trying to access array offset on").count(),
        2,
        "{}",
        out.stderr
    );
}

/// A resource receiver names `resource`, not the integer storage its repr collapses to.
/// `STDIN` is statically a resource; a `fopen()` result is boxed `mixed` and takes the runtime
/// path instead.
#[test]
fn test_scalar_array_offset_read_names_a_resource() {
    let out = compile_and_run_capture(
        r#"<?php
function f(): string { $y = STDIN["a"]; return gettype($y); }
echo f(), "\n";
"#,
    );
    assert_eq!(out.stdout, "NULL\n");
    assert!(
        out.stderr.contains("Trying to access array offset on resource"),
        "missing resource warning: {}",
        out.stderr
    );
}

/// An owned callable key is pinned across the warning and retired afterwards, so a scalar
/// receiver read neither leaks the closure nor its captures (#2025 review).
#[test]
fn test_scalar_array_offset_read_releases_a_callable_key() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class Box { public function __construct(public int $v) {} }
function f(): string {
    $i = 42;
    $seen = 0;
    for ($n = 0; $n < 50; $n++) {
        $o = new Box($n);
        $y = $i[function () use ($o) { return $o->v; }];
        $seen += ($y === null) ? 1 : 0;
    }
    return (string) $seen;
}
echo f(), "\n";
"#,
    );
    assert_eq!(out.stdout, "50\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}
