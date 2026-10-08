//! Purpose:
//! Regression tests for builtins that read only the VALUES of a bare `array` argument.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A declared `array` is `array<mixed>|array<mixed, mixed>`: boxed storage, packed or hash, known
//!   only at run time. `max`, `min`, `array_count_values` and `array_chunk` refused it with
//!   "must be of type array, array given" — a message that contradicts itself — and `vsprintf`
//!   accepted it and printed `0-0` for `["x", "y"]`, reading the boxed cell as a raw array.
//! - The fix is `BuiltinArgumentLowering::BareArrayValues`: the argument reaches the builtin as the
//!   owned `array<mixed>` list `array_values()` builds, which already dispatches on the runtime
//!   layout. So every case below runs a HASH as well as a list, since the layout is what hid it.
//! - Every expected value is real `php` 8.x output for the same fixture.

use crate::support::*;

/// A bare `array` PARAMETER — the most ordinary PHP there is — reaches each builtin by value.
#[test]
fn test_value_only_builtins_accept_a_bare_array_parameter() {
    let out = compile_and_run(
        r#"<?php
function t(array $n, array $m, array $s): void {
    echo max($n), "|", min($n), "|", max($m), "|", min($m), "|";
    echo vsprintf("%s-%s", $s), "|", vsprintf("%s+%s+%s", $m), "|";
    echo json_encode(array_count_values($n)), "|", json_encode(array_count_values($m)), "|";
    echo json_encode(array_chunk($n, 2)), "|", json_encode(array_chunk($m, 2)), "|";
    vprintf("%s/%s", $s);
}
t([3, 1, 2, 3], ['a' => 5, 'b' => 9, 'c' => 5], ['x', 'y']);
"#,
    );
    assert_eq!(
        out,
        "3|1|9|5|x-y|5+9+5|{\"3\":2,\"1\":1,\"2\":1}|{\"5\":2,\"9\":1}|[[3,1],[2,3]]|[[5,9],[5]]|x/y"
    );
}

/// The same crossing from a PROPERTY, whose read is itself a boxed cell, and the heap stays clean.
///
/// The loop is there to turn a per-call leak of the substituted list into a count the heap-debug
/// summary cannot report as clean.
#[test]
fn test_value_only_builtins_on_a_bare_array_property_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class H { public array $m = ['a' => 5, 'b' => 9, 'c' => 5]; }
$h = new H();
echo max($h->m), "|", vsprintf("%s,%s,%s", $h->m), "|", json_encode(array_count_values($h->m));
$t = 0;
for ($i = 0; $i < 50; $i++) { $t = max($h->m) + count(array_chunk($h->m, 2)); }
echo "|", $t;
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "9|5,9,5|{\"5\":2,\"9\":1}|11", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A string transform named at run time maps over a bare array's boxed elements.
///
/// The bare array hands its callback `mixed` elements, and the transforms' callable policy only
/// admitted a concrete string source, so `strtoupper` was left out of the runtime case table:
/// "array_map callback string does not name a supported callable". The wrapper's call converts
/// the element exactly as a direct call would, numbers included.
#[test]
fn test_runtime_named_string_transform_maps_a_bare_array() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function pick(int $i): string { return ["strtoupper", "strrev"][$i % 2]; }
function t(array $a, int $i): string { $f = pick($i); return implode(",", array_map($f, $a)); }
$r = "";
for ($i = 0; $i < 40; $i++) { $r = t(["ab", 7, 2.5], $i); }
echo t(["ab", "cd"], 0), "|", $r;
"#,
    );
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "AB,CD|ba,7,5.2", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
