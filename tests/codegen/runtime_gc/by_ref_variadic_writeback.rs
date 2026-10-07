//! Purpose:
//! Heap-debug coverage for the write-back a by-reference variadic callee performs through the
//! caller's reference cell. `$items[n] = <value>` on a `&...$items` parameter targets an
//! invoker ref-cell marker: the marker write-back detects the marker tag and, for a widened
//! caller cell, transfers the incoming boxed Mixed handle (fresh or retained) straight into the
//! caller's cell instead of going through `__rt_array_set_mixed`. That hand-written transfer has
//! to keep the same ownership discipline as the runtime setter it replaces; a retained handle at
//! a concrete-source marker falls back to that setter.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The cell is the caller variable's storage and OWNS the box it holds, so a write-back
//!   replaces a live value and must release it. Before issue #1062 was fixed the transfer was
//!   a bare store, orphaning the previous box and the payload it pinned on every call.
//! - Each fixture asserts a clean heap after the caller's local leaves scope. In particular,
//!   repeated write-backs must release every replaced box and its payload rather than leaking
//!   storage in proportion to the number of calls.
//! - The fixtures use a first-class callable rather than a direct call, and no loop. A direct
//!   call trips a pre-existing over-release of the invoker argument array under `--heap-debug`,
//!   and a by-reference variadic driven from a loop segfaults on `main` as well; both are
//!   separate defects, and asserting around them here would test them instead of this one.
//! - Every right-hand side reaches this write-back now: a concrete value is boxed fresh, a
//!   `Mixed`/union value arrives as a retained box (the `!value_is_fresh_box` case), and both
//!   keep the by-ref alias through the marker. Which arm runs depends on the marker's SOURCE tag,
//!   not the RHS: every `&...$items` call here widens the caller cell, so these fixtures take the
//!   handle-transfer arm (retained boxes included); the payload-transfer arm serves a
//!   concrete-source marker (the eval bridge's raw ref cells) and is not covered here.
//! - Expected stdout is real `LC_ALL=C php` 8.5 output for the same fixtures.

use crate::support::compile_and_run_with_heap_debug;

/// Asserts the program printed `expected` and left no heap allocations behind.
fn assert_heap_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Four write-backs, each replacing a heap string, release both the old box and its payload.
#[test]
fn test_by_ref_variadic_writeback_releases_the_replaced_box() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replace(&...$items): void { $items[0] = $items[0] . "x"; }
$c = replace(...);
$p = "a";
$c($p);
$c($p);
$c($p);
$c($p);
echo $p, "\n";
"#,
    );
    assert_heap_clean(out, "axxxx\n");
}

/// Three times as many writes also leave a clean heap, pinning repeated replacement rather
/// than only the first write.
#[test]
fn test_by_ref_variadic_writeback_does_not_grow_with_the_call_count() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replace(&...$items): void { $items[0] = $items[0] . "x"; }
$c = replace(...);
$p = "a";
$c($p);
$c($p);
$c($p);
$c($p);
$c($p);
$c($p);
$c($p);
$c($p);
$c($p);
$c($p);
$c($p);
$c($p);
echo $p, "\n";
"#,
    );
    assert_heap_clean(out, "axxxxxxxxxxxx\n");
}

/// A write-back that RETYPES the caller's local -- the issue #1062 miscompile -- is still
/// balanced: the `int` the local held is not refcounted, so the cell's first occupant needs no
/// release, and the string that replaces it is released by the following write-back.
#[test]
fn test_by_ref_variadic_retyping_writeback_leaves_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replace(&...$items): void { $items[0] = "position"; }
$c = replace(...);
$p = 1;
$c($p);
echo $p, "\n";
"#,
    );
    let out_stderr = out.stderr.clone();
    assert!(out.success, "program failed: {}", out_stderr);
    assert_eq!(out.stdout, "position\n", "stderr: {}", out_stderr);
    assert!(
        out_stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out_stderr
    );
}

/// A `Mixed` right-hand side (`take()` returns `mixed`) keeps the by-ref alias and writes back,
/// where the runtime setter it used to call had no marker check and severed it (#1287).
#[test]
fn test_by_ref_variadic_mixed_right_hand_side_keeps_the_alias() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function take(mixed $v): mixed { return $v; }
function replace(&...$items): void { $items[0] = take("z"); }
$c = replace(...);
$p = "a";
$c($p);
echo $p, "\n";
"#,
    );
    assert_heap_clean(out, "z\n");
}

/// The same for a union-typed right-hand side (`int|string`), whose value is boxed too.
#[test]
fn test_by_ref_variadic_union_right_hand_side_keeps_the_alias() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function pick(bool $b): int|string { return $b ? 1 : "z"; }
function replace(&...$items): void { $items[0] = pick(true); }
$c = replace(...);
$p = "a";
$c($p);
echo $p, "\n";
"#,
    );
    assert_heap_clean(out, "1\n");
}

/// A self-assignment keeps the alias and releases the replaced box exactly once: the read
/// through the marker clones the referenced cell, so the incoming handle is a distinct box (#1287).
#[test]
fn test_by_ref_variadic_mixed_self_assignment_stays_balanced() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function replace(&...$items): void { $items[0] = $items[0]; }
$c = replace(...);
$p = "a";
$c($p);
echo $p, "\n";
"#,
    );
    assert_heap_clean(out, "a\n");
}

/// A write from one aliased by-ref variadic element into another, via a `mixed` call
/// return, writes back through the destination cell and leaves the source unchanged.
#[test]
fn test_by_ref_variadic_cross_element_writeback_keeps_the_destination_alias() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function take(mixed $v): mixed { return $v; }
function replace(&...$items): void { $items[0] = take($items[1]); }
$c = replace(...);
$a = "first";
$b = "second";
$c($a, $b);
echo $a, ":", $b, "\n";
"#,
    );
    assert_heap_clean(out, "second:second\n");
}
