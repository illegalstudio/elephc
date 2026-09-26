//! Purpose:
//! Ownership tests for issue #1094: a property write through a `mixed` receiver now reaches a
//! declared slot instead of being discarded, so the value it stores is one the object owns and the
//! previous occupant is one it must release.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`.
//! - The loop count is high enough that a per-iteration leak of one block is unmistakable.
//! - Before the fix these programs were trivially clean because the store never happened; the
//!   point of the tests is that they stay clean now that it does.

use crate::support::compile_and_run_with_heap_debug;

/// Asserts the program printed `expected` and left a clean heap under heap debug.
fn assert_clean(out: crate::support::ProgramOutput, expected: &str) {
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// A string property overwritten through a `mixed` receiver: the incoming block is retained and the
/// previous one released, once each, every iteration.
#[test]
fn test_mixed_receiver_string_property_write_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public string $s = "seed"; }
function w(mixed $o, string $v): void { $o->s = $v; }
$t = new T();
for ($i = 0; $i < 40; $i++) { w($t, "value" . $i); }
echo $t->s, "\n";
"#,
    );
    assert_clean(out, "value39\n");
}

/// An array property overwritten the same way, so the released previous occupant owns elements of
/// its own.
#[test]
fn test_mixed_receiver_array_property_write_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $items = [0]; }
function w(mixed $o, int $v): void { $o->items = [$v, $v + 1]; }
$t = new T();
for ($i = 0; $i < 40; $i++) { w($t, $i); }
echo count($t->items), ":", $t->items[0], "\n";
"#,
    );
    assert_clean(out, "2:39\n");
}

/// The stdClass payload keeps its own path; pinned here so the fallback's ownership is covered by
/// the same sweep.
#[test]
fn test_mixed_receiver_stdclass_property_write_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function w(mixed $o, string $v): void { $o->s = $v; }
$t = new stdClass();
$t->s = "seed";
for ($i = 0; $i < 40; $i++) { w($t, "value" . $i); }
echo $t->s, "\n";
"#,
    );
    assert_clean(out, "value39\n");
}

/// Two classes declaring the same property, alternating through the same call site, so both arms
/// of the class-id chain run and both release their own previous occupant.
#[test]
fn test_mixed_receiver_write_dispatch_is_heap_clean_for_both_classes() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public string $s = "t"; }
class U { public string $s = "u"; }
function w(mixed $o, string $v): void { $o->s = $v; }
$t = new T();
$u = new U();
for ($i = 0; $i < 40; $i++) { w($t, "t" . $i); w($u, "u" . $i); }
echo $t->s, ":", $u->s, "\n";
"#,
    );
    assert_clean(out, "t39:u39\n");
}

/// A BORROWED source: the property retains it, the caller keeps its own reference, and neither is
/// released twice. This is the shape an earlier cut of the fix turned into a use-after-free.
#[test]
fn test_mixed_receiver_write_of_a_borrowed_source_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $items = [0]; }
function w(mixed $o, array $a): void { $o->items = $a; }
$t = new T();
for ($i = 0; $i < 40; $i++) { $a = [$i, $i + 1]; w($t, $a); echo count($a); }
echo "|", count($t->items), "\n";
"#,
    );
    assert!(out.stdout.ends_with("|2\n"), "stdout: {}", out.stdout);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// An OWNING temporary: the release lowering now emits must fire exactly once, or the temporary
/// leaks once per write — three blocks for an array literal and its two boxed elements.
#[test]
fn test_mixed_receiver_write_of_an_owning_temporary_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class T { public array $items = [0]; }
function w(mixed $o, int $v): void { $o->items = [$v, $v + 1]; }
$t = new T();
for ($i = 0; $i < 40; $i++) { w($t, $i); }
echo count($t->items), ":", $t->items[0], "\n";
"#,
    );
    assert_clean(out, "2:39\n");
}
