//! Purpose:
//! Regression tests: a read larger than the concat arena's free space takes an owned
//! heap block instead of overrunning the arena, and that block must be reclaimed.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture runs under `--heap-debug` and asserts `leak summary: clean`.
//! - The leak needed two independent omissions to appear, so a fixture that exercises
//!   only one of them passes either way: the EIR already emitted a `release` for these
//!   values but the backend discarded it (`value_is_scratch_string` treated any
//!   non-`Fresh` runtime call as arena scratch), *and* the block carried heap kind 0,
//!   which every `__rt_decref_any` in the runtime's own read paths skips as raw.
//! - The fixtures use a bare relative filename rather than `sys_get_temp_dir()`. That
//!   helper leaks a small block on Windows (one 54-byte path survived a run that had
//!   already released every 100 KB read block), which is a separate defect: dragging it
//!   in would make this file fail for a reason that has nothing to do with what it tests.
//! - Sizes straddle the 64 KiB arena deliberately: a read that fits returns a borrowed
//!   arena slice and must NOT be freed, which is what the small-read fixture pins.

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

/// Draining a file with reads larger than the arena must not strand one block per read.
#[test]
fn test_oversized_fread_releases_its_block_every_iteration() {
    // Each iteration asks for 100000 bytes, well past what the 64 KiB arena can hold,
    // so every read takes its own block. The result is consumed by strlen() and never
    // stored, which is the shape that leaked: five blocks and half a megabyte survived
    // a 500 KB file, and they accumulated for the life of the process.
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$path = "elephc-oversized-read-release.bin";
$sink = fopen($path, "w");
$chunk = str_repeat("Q", 1000);
for ($i = 0; $i < 500; $i++) { fwrite($sink, $chunk); }
fclose($sink);

$source = fopen($path, "r");
$total = 0;
while (!feof($source)) { $total += strlen(fread($source, 100000)); }
fclose($source);
unlink($path);
echo $total;
"#,
    );
    assert_clean(out, "500000");
}

/// A read that still fits the arena is borrowed storage and must not be released.
#[test]
fn test_arena_sized_fread_is_borrowed_and_stays_clean() {
    // The negative control for the fixture above. Releasing an arena slice would be a
    // wild free rather than a leak, and the guard against it is not a heuristic:
    // `_concat_buf` and `_heap_buf` are separate `.comm` objects, so an arena pointer
    // can never satisfy `__rt_heap_free_safe`'s managed-heap window test. If that ever
    // stops holding, this fixture is what notices.
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$path = "elephc-arena-read-release.bin";
$sink = fopen($path, "w");
fwrite($sink, str_repeat("R", 4096));
fclose($sink);

$source = fopen($path, "r");
$total = 0;
while (!feof($source)) { $total += strlen(fread($source, 512)); }
fclose($source);
unlink($path);
echo $total;
"#,
    );
    assert_clean(out, "4096");
}
