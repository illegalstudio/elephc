//! Purpose:
//! Manual microbenchmarks bounding the cost of the null representation on int-heavy code.
//! Compares plain-int hot loops against nullable-int (`?int` / array-miss / `??`) hot loops.
//!
//! Called from:
//! - `cargo test -- --ignored null_sentinel::benches` (manual, before/after representation work).
//!
//! Key details:
//! - Timings include compile+assemble+link overhead; fixtures loop enough iterations that the
//!   native run dominates. Compare like-for-like between NullRepr modes, not absolute numbers.

use super::*;
use std::time::Instant;

/// Runs a fixture through compile_and_run, asserting output and printing elapsed wall time.
/// The first run warms OS caches; the second run's timing is reported.
fn bench(label: &str, source: &str, expected: &str) {
    compile_and_run(source);
    let started = Instant::now();
    let out = compile_and_run(source);
    let elapsed = started.elapsed();
    assert_eq!(out, expected);
    println!("bench {label}: {:?}", elapsed);
}

/// Baseline: a plain-int hot loop with no null capability anywhere. Phase 3 requires this
/// to stay at parity with the pre-Tagged baseline (non-nullable ints must remain plain i64).
#[test]
#[ignore = "manual microbench: run before/after NullRepr changes"]
fn bench_plain_int_sum_loop() {
    let source = r#"<?php
$sum = 0;
for ($i = 0; $i < 100000000; $i++) {
    $sum = $sum + $i;
}
echo $sum;
"#;
    bench("plain_int_sum_loop", source, "4999999950000000");
}

/// Nullable-heavy: every iteration routes the accumulator through a `?int` function return
/// and a `??` default. Bounds the per-value cost of the wide null-capable representation.
#[test]
#[ignore = "manual microbench: run before/after NullRepr changes"]
fn bench_nullable_int_roundtrip_loop() {
    let source = r#"<?php
function step(int $i): ?int {
    if ($i < 0) {
        return null;
    }
    return $i;
}
$sum = 0;
for ($i = 0; $i < 5000000; $i++) {
    $sum = $sum + (step($i) ?? 0);
}
echo $sum;
"#;
    bench("nullable_int_roundtrip_loop", source, "12499997500000");
}

/// Array-read-heavy: sums array<int> elements read back by index. Array element reads are
/// miss-capable, so this bounds the tagged-read cost on the hottest null-capable producer.
#[test]
#[ignore = "manual microbench: run before/after NullRepr changes"]
fn bench_array_int_read_loop() {
    let source = r#"<?php
$a = [];
for ($i = 0; $i < 1000; $i++) {
    $a[] = $i;
}
$sum = 0;
for ($r = 0; $r < 100000; $r++) {
    for ($i = 0; $i < 1000; $i++) {
        $sum = $sum + $a[$i];
    }
}
echo $sum;
"#;
    bench("array_int_read_loop", source, "49950000000");
}
