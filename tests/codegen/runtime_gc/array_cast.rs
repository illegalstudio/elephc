//! Purpose:
//! Integration or regression tests for the OWNERSHIP paths of PHP's `(array)` cast: an array
//! source is handed back unchanged, a scalar is pushed into a fresh one-element array, `null`
//! allocates an empty one, and every runtime-typed source is boxed (when it is not already) and
//! dispatched by `__rt_mixed_cast_array`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every loop bound derives from `$argc`, and the values come from function returns and
//!   parameters, so constant folding cannot erase the allocation under test.
//! - Each assertion checks `HEAP DEBUG: leak summary: clean` alongside the output. A missing
//!   release leaks per iteration; an extra one frees an array the caller still reads, which
//!   shows up as a wrong count or a corrupted source array.
//! - Expected stdout values are real PHP 8.5 output for the same fixtures (issue #707).

use crate::support::*;

/// Asserts the program printed `expected` and left a clean heap under heap debug.
fn assert_clean(out: ProgramOutput, expected: &str) {
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, expected, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// Owned temporaries of every source kind are released exactly once by each cast path:
/// strings, arrays, objects, boxed `mixed` values, unions, nullable arrays, and `null`.
#[test]
fn test_array_cast_of_every_source_kind_leaves_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class P { public $a = 1; protected $b = "two"; private $c = [3]; }
function str_of(int $i): string { return "s" . $i; }
function list_of(int $i): array { return [$i, $i + 1]; }
function mixed_of(int $i): mixed {
    return match ($i % 5) { 0 => "m" . $i, 1 => [$i], 2 => null, 3 => new P(), default => $i * 1.5 };
}
function via_mixed(mixed $v): int { return count((array) $v); }
function via_union(int|string $v): array { return (array) $v; }
function via_nullable(?array $v): array { return (array) $v; }
$total = 0;
for ($i = 0; $i < 100 * $argc; $i++) {
    $total += count((array) str_of($i)) + count((array) list_of($i)) + count((array) mixed_of($i));
    $total += count((array) ($i % 2 ? null : $i)) + count((array) new P());
    $total += via_mixed(str_of($i)) + via_mixed(new P());
    $total += count(via_union($i)) + count(via_union(str_of($i)));
    $total += count(via_nullable(null)) + count(via_nullable(list_of($i)));
    foreach ((array) str_of($i) as $v) { $total += strlen($v); }
    foreach ((array) mixed_of($i) as $v) { $total += 1; }
    $total += count(array_merge((array) $i, (array) list_of($i)));
}
echo $total;
"#,
    );
    assert_clean(out, "2280");
}

/// An array source is handed back unchanged, so the result shares the source's storage:
/// a write to the result must separate it (copy-on-write) and leave the source intact, and a
/// cast returned from a function must hand the caller a reference of its own.
#[test]
fn test_array_cast_identity_keeps_the_source_and_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function ident(array $a): array { return (array) $a; }
function ident_mixed(mixed $a) { return (array) $a; }
function list_of(int $i): array { return [$i, $i + 1]; }
class Holder {
    public array $data = [1, 2];
    public function get(): array { return (array) $this->data; }
}
$src = [1, 2, 3];
$h = new Holder();
$total = 0;
for ($i = 0; $i < 100 * $argc; $i++) {
    $r = ident($src);
    $r[] = $i;
    $m = ident_mixed($src);
    $m[] = 5;
    $t = ident([$i, 2]);
    $t[] = 1;
    $u = ident_mixed(list_of($i));
    $u[] = 7;
    $g = $h->get();
    $g[] = 3;
    $w = (array) ("w" . $i);
    $w[] = "tail";
    $total += count($r) + count($m) + count($t) + count($u) + count($g) + count($w);
    $total += count(ident(list_of($i))) + count($src) + count($h->data);
}
echo $total, " ", implode(",", $src), " ", implode(",", $h->data);
"#,
    );
    assert_clean(out, "2600 1,2,3 1,2");
}
