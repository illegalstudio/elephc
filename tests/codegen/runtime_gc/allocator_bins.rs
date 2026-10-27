//! Purpose:
//! Regression tests for the exact size-class bins and the consolidation pass that merges them.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Binned blocks are not coalesced when freed, so a heap full of small free blocks must still
//!   serve a larger request: the allocator consolidates before reporting exhaustion.
//! - Allocation cost must not grow with the number of free blocks; the timing test fails the old
//!   list-walking allocator by more than an order of magnitude.

use crate::support::*;
use std::time::{Duration, Instant};

/// Compiles `source` with the given CLI flags, runs it, and returns its captured output.
fn compile_and_run_with_flags(prefix: &str, source: &str, flags: &[&str]) -> ProgramOutput {
    let dir = make_cli_test_dir(prefix);
    let php_path = dir.join("main.php");
    fs::write(&php_path, source).unwrap();
    let compile = elephc_cli_command(&dir).args(flags).arg(&php_path).output().unwrap();
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let run = run_binary(&dir.join("main"), &dir);
    fs::remove_dir_all(&dir).unwrap();
    ProgramOutput {
        stdout: String::from_utf8_lossy(&run.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&run.stderr).into_owned(),
        success: run.status.success(),
    }
}

/// Small strings freed into their bins are merged when the heap runs out, so a later batch of
/// larger strings fits in the same 256 KB heap instead of dying with "heap memory exhausted". The
/// first batch leaves the bump region nearly full, so the second one only fits through consolidation.
#[test]
fn test_allocator_consolidates_binned_blocks_before_exhaustion() {
    let source = r#"<?php
$small = [];
for ($i = 0; $i < 1500; $i++) {
    $small[] = str_repeat("s", 100) . $i;
}
$small = [];
$large = [];
for ($i = 0; $i < 700; $i++) {
    $large[] = str_repeat("L", 200) . $i;
}
echo count($large), ":", strlen($large[699]), ":", substr($large[450], -3);
"#;
    let out = compile_and_run_with_heap_size(source, 262_144);
    assert_eq!(out, "700:203:450");
}

/// Free memory parked in a LARGER bin serves smaller requests once the bump end is full. Every
/// other 1000-byte string is freed, so the freed blocks are isolated and consolidation cannot
/// merge them; the 600 small strings that follow only fit by splitting those blocks. Exact bins
/// alone would report "heap memory exhausted" with about 100 KB still free.
#[test]
fn test_allocator_splits_larger_binned_blocks_when_the_heap_is_full() {
    let source = r#"<?php
$big = [];
for ($i = 0; $i < 200; $i++) {
    $big[] = str_repeat("B", 1000) . $i;
}
for ($i = 0; $i < 200; $i += 2) {
    $big[$i] = null;
}
$small = array_fill(0, 600, "");
for ($i = 0; $i < 600; $i++) {
    $small[$i] = str_repeat("s", 100) . $i;
}
echo count($small), ":", strlen($small[599]), ":", substr($big[199], -3);
"#;
    let out = compile_and_run_with_heap_size(source, 250_000);
    assert_eq!(out, "600:103:199");
}

/// The consolidation pass and the exact bins keep the allocator invariants that heap-debug mode
/// validates on every allocation and free, including a pass forced by a small heap and the one
/// `gc_mem_caches()` runs, and the program ends with no leak.
#[test]
fn test_allocator_bins_and_consolidation_validate_under_heap_debug() {
    let source = r#"<?php
$total = 0;
for ($round = 0; $round < 6; $round++) {
    $keep = [];
    for ($i = 0; $i < 300; $i++) {
        $keep[] = str_repeat("x", 8 + ($i * 37 + $round * 11) % 2100);
        if ($i % 3 === 0) {
            $keep[intdiv($i, 2)] = "";
        }
    }
    foreach ($keep as $value) {
        $total += strlen($value);
    }
    unset($keep);
    if ($round % 2 === 1) {
        gc_mem_caches();
    }
}
echo $total, ":", gc_mem_caches() >= 0 ? "ok" : "bad";
"#;
    let out = compile_and_run_with_flags(
        "elephc_allocator_bins_debug",
        source,
        &["--heap-debug", "--heap-size=1048576"],
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert!(out.stdout.ends_with(":ok"), "{}", out.stdout);
    assert!(!out.stderr.contains("Fatal error"), "{}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "allocator state was not clean: {}",
        out.stderr
    );
}

/// A heap that is genuinely full still reports exhaustion: consolidation that finds nothing to
/// merge must not retry forever.
#[test]
fn test_allocator_reports_exhaustion_when_nothing_merges() {
    let source = r#"<?php
$keep = [];
for ($i = 0; $i < 100000; $i++) {
    $keep[] = str_repeat("k", 64) . $i;
}
echo count($keep);
"#;
    let started = Instant::now();
    let out = compile_and_run_with_flags("elephc_allocator_full", source, &["--heap-size=262144"]);
    assert!(!out.success);
    assert!(out.stderr.contains("heap memory exhausted"), "{}", out.stderr);
    assert!(started.elapsed() < Duration::from_secs(60), "exhaustion took {:?}", started.elapsed());
}

/// Allocation and free stay O(1) with many free blocks: 100k allocations next to 20k freed,
/// non-adjacent blocks used to walk the ordered free list on every call (about 30 s); with the
/// exact bins they take well under a second.
#[test]
fn test_allocator_cost_does_not_grow_with_free_blocks() {
    let source = r#"<?php
$keep = [];
for ($i = 0; $i < 40000; $i++) {
    $keep[] = str_repeat("x", 100 + ($i % 50));
}
for ($i = 0; $i < 40000; $i += 2) {
    $keep[$i] = "";
}
$len = 0;
for ($j = 0; $j < 100000; $j++) {
    $s = str_repeat("y", 200 + ($j % 7));
    $a = [$s, $j, "k" . $j];
    $len += strlen($a[0]);
}
echo $len;
"#;
    let started = Instant::now();
    let out = compile_and_run_with_flags(
        "elephc_allocator_cost",
        source,
        &["--heap-size=67108864"],
    );
    let elapsed = started.elapsed();
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "20299995");
    assert!(elapsed < Duration::from_secs(20), "allocation degraded: {elapsed:?}");
}
