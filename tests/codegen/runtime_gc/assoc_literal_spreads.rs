//! Purpose:
//! Ownership tests for issue #1049: a spread inside an associative literal now actually runs, and
//! it promotes its source through a CONSUMING conversion — so the reference accounting on both
//! sides of that promotion is what these pin.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`.
//! - The two source kinds are covered separately because they take opposite paths: a source loaded
//!   from a local is retained before the promotion consumes it, a source that is an owning
//!   temporary is not.

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

/// A source loaded from a local: the slot keeps its own reference, so the promotion is given a
/// separate one. Without it the source was freed under the caller; with an unconditional one the
/// literal-source case below leaked instead.
#[test]
fn test_local_source_spread_into_an_associative_literal_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$total = 0;
for ($i = 0; $i < 40; $i++) {
    $idx = [$i, $i + 1];
    $a = [...$idx, "c" => $i];
    $total = $total + count($a) + count($idx);
}
echo $total, "\n";
"#,
    );
    assert_clean(out, "200\n");
}

/// A source that is an owning temporary: it already owns the only reference there is, so acquiring
/// another leaked one block per iteration.
#[test]
fn test_temporary_source_spread_into_an_associative_literal_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$total = 0;
for ($i = 0; $i < 40; $i++) {
    $a = [...[$i, $i + 1], "c" => $i];
    $total = $total + count($a);
}
echo $total, "\n";
"#,
    );
    assert_clean(out, "120\n");
}

/// A STRING-valued source, whose entries are heap blocks the destination must own independently of
/// the source it was spread from.
#[test]
fn test_string_valued_spread_into_an_associative_literal_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$total = 0;
for ($i = 0; $i < 40; $i++) {
    $src = ["a" . $i, "b" . $i];
    $a = [...$src, "c" => "c" . $i];
    $total = $total + count($a) + count($src);
}
echo $total, "\n";
"#,
    );
    assert_clean(out, "200\n");
}

/// An ASSOCIATIVE source, which is spread as-is with no promotion at all — the other side of the
/// branch the retain sits on.
#[test]
fn test_assoc_source_spread_into_an_associative_literal_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$total = 0;
for ($i = 0; $i < 40; $i++) {
    $src = ["a" => $i, "b" => $i + 1];
    $a = [...$src, "c" => $i];
    $total = $total + count($a) + count($src);
}
echo $total, "\n";
"#,
    );
    assert_clean(out, "200\n");
}

/// A TERNARY source hands over the only reference it has. Acquiring another leaked one array per
/// evaluation — 40 live blocks over 40 iterations — which is why the gate asks the slot's KIND and
/// not just the defining op.
#[test]
fn test_ternary_source_spread_into_an_associative_literal_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$total = 0;
for ($i = 0; $i < 40; $i++) {
    $c = $i % 2 === 0;
    $a = [...($c ? [$i, $i + 1] : [$i + 2, $i + 3]), "c" => $i];
    $total = $total + count($a);
}
echo $total, "\n";
"#,
    );
    assert_clean(out, "120\n");
}

/// A typed INSTANCE PROPERTY source: the object goes on owning its array, and the literal owns its
/// own copy of the entries.
#[test]
fn test_property_source_spread_into_an_associative_literal_is_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class C { public array $items = [0, 1]; }
$total = 0;
for ($i = 0; $i < 40; $i++) {
    $o = new C();
    $a = [...$o->items, "c" => $i];
    $total = $total + count($a) + count($o->items);
}
echo $total, "\n";
"#,
    );
    assert_clean(out, "200\n");
}
