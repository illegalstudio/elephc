//! Purpose:
//! Integration tests for `array_chunk()` over an ASSOCIATIVE receiver, which the checker refused
//! outright until `__rt_hash_chunk` existed. Both `preserve_keys` modes are covered, because the
//! flag's rule here is not `array_slice()`'s: chunk drops STRING keys when it is false and
//! restarts the numbering at 0 inside every chunk, where slice renumbers integer keys only and
//! leaves string keys alone.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout.
//! - Expected values are real `LC_ALL=C php` 8.5 output for the same fixtures.
//! - Ownership of the copied payloads is covered separately under `runtime_gc/`.

use crate::support::*;

/// The issue's reported call: a string-keyed source, keys preserved.
#[test]
fn test_assoc_chunk_preserves_string_keys() {
    let out = compile_and_run(
        r#"<?php
$c = array_chunk(["x" => 1, "y" => 2, "z" => 3], 2, true);
echo count($c), "|", count($c[0]), count($c[1]), "|", $c[0]["x"], $c[0]["y"], $c[1]["z"];
"#,
    );
    assert_eq!(out, "2|21|123");
}

/// Without the flag, chunk drops string keys entirely and renumbers each chunk from 0.
///
/// This is the rule that differs from `array_slice()`, which would have kept `"x"`, `"y"` and
/// `"z"`. PHP restarts at 0 per chunk, so the third element is `[0]` of the second chunk rather
/// than `[2]` of a running sequence.
#[test]
fn test_assoc_chunk_without_preserve_keys_drops_string_keys() {
    let out = compile_and_run(
        r#"<?php
$c = array_chunk(["x" => 1, "y" => 2, "z" => 3], 2);
echo count($c), "|", $c[0][0], $c[0][1], "|", $c[1][0], "|",
     isset($c[0]["x"]) ? "kept" : "dropped", "|", isset($c[1][2]) ? "running" : "restarted";
"#,
    );
    assert_eq!(out, "2|12|3|dropped|restarted");
}

/// Integer-keyed and mixed-key associative sources take the same path.
///
/// `[5 => 1, 9 => 2]` is associative because its keys are sparse, and a mixed source proves the
/// key is copied verbatim rather than reconstructed as one kind.
#[test]
fn test_assoc_chunk_carries_integer_and_mixed_keys() {
    let out = compile_and_run(
        r#"<?php
$s = array_chunk([5 => 1, 9 => 2], 1, true);
$m = array_chunk([3 => "x", "k" => "y", 7 => "z"], 2, true);
echo $s[0][5], $s[1][9], "|", $m[0][3], $m[0]["k"], $m[1][7];
"#,
    );
    assert_eq!(out, "12|xyz");
}

/// String VALUES work, which is what the indexed helpers cannot do.
///
/// An indexed `array<string>` still reports unsupported here (issue #675): its 16-byte
/// `{pointer, length}` slots do not fit the pointer-sized chunk helpers. Building hash chunks
/// sidesteps that, so the associative path carries string values in both modes.
#[test]
fn test_assoc_chunk_carries_string_values() {
    let out = compile_and_run(
        r#"<?php
$k = array_chunk(["a" => "one", "b" => "two", "c" => "three"], 2, true);
$r = array_chunk(["a" => "one", "b" => "two", "c" => "three"], 2);
echo $k[0]["a"], "/", $k[1]["c"], "|", $r[0][1], "/", $r[1][0];
"#,
    );
    assert_eq!(out, "one/three|two/three");
}

/// Heap-backed values are retained, so a chunk outlives the source it was cut from.
///
/// The chunk becomes a second owner of each nested array. If it did not retain them, dropping
/// the source would free children the chunk still points at.
#[test]
fn test_assoc_chunk_values_outlive_the_source() {
    let out = compile_and_run(
        r#"<?php
$src = ["p" => [1, 2], "q" => [3], "r" => [4, 5, 6]];
$c = array_chunk($src, 2, true);
unset($src);
echo count($c), "|", count($c[0]["p"]), $c[0]["p"][1], "|", count($c[1]["r"]), $c[1]["r"][2];
"#,
    );
    assert_eq!(out, "2|22|36");
}

/// Edge sizes: a chunk larger than the source, one that divides it exactly, and size 1.
///
/// The "divides exactly" case is the one with no short final chunk, so it exercises the path
/// where the walk ends with no chunk still open.
#[test]
fn test_assoc_chunk_edge_sizes() {
    let out = compile_and_run(
        r#"<?php
$big = array_chunk(["a" => 1, "b" => 2], 10, true);
$exact = array_chunk(["a" => 1, "b" => 2, "c" => 3, "d" => 4], 2, true);
$one = array_chunk(["a" => 1, "b" => 2], 1, true);
echo count($big), count($big[0]), "|", count($exact), count($exact[0]), count($exact[1]),
     "|", count($one), count($one[0]);
"#,
    );
    assert_eq!(out, "12|222|21");
}

/// An empty associative source chunks into an empty outer array rather than one empty chunk.
#[test]
fn test_assoc_chunk_of_an_empty_source_is_empty() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => 1];
unset($m["a"]);
echo count(array_chunk($m, 2, true)), count(array_chunk($m, 2));
"#,
    );
    assert_eq!(out, "00");
}

/// The source survives the call unchanged, in both modes.
///
/// `array_chunk()` copies; nothing about building the chunks may consume the receiver's own
/// entries or renumber them in place.
#[test]
fn test_assoc_chunk_leaves_its_source_intact() {
    let out = compile_and_run(
        r#"<?php
$src = ["p" => 10, "q" => 20, "r" => 30];
$a = array_chunk($src, 2, true);
$b = array_chunk($src, 2);
echo count($src), $src["p"], $src["r"], "|", count($a), count($b);
"#,
    );
    assert_eq!(out, "31030|22");
}

/// A zero or negative chunk size is still PHP's ValueError on the associative path.
///
/// The guard sits in the shared call prologue, so it must cover the hash helper the same way it
/// covers the indexed ones — a zero length would otherwise never advance the walk.
#[test]
fn test_assoc_chunk_rejects_a_non_positive_length() {
    let out = compile_and_run_expect_failure(
        r#"<?php
$m = ["a" => 1, "b" => 2];
array_chunk($m, 0, true);
"#,
    );
    assert!(
        out.contains("array_chunk(): Argument #2 ($length) must be greater than 0"),
        "expected PHP's ValueError message, got: {out}"
    );
}

/// The indexed forms are untouched by the associative path.
#[test]
fn test_indexed_chunk_is_unchanged() {
    let out = compile_and_run(
        r#"<?php
$r = array_chunk([1, 2, 3], 2);
$k = array_chunk([1, 2, 3], 2, true);
echo count($r), $r[0][0], $r[0][1], $r[1][0], "|", count($k), $k[0][0], $k[1][2];
"#,
    );
    assert_eq!(out, "2123|213");
}
