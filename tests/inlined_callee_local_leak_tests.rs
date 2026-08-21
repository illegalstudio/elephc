//! Purpose:
//! End-to-end heap-ownership tests for the values a CALLED function builds in its own locals,
//! and for `scandir()`'s per-entry listing strings.
//!
//! THE TWO BUGS THESE PIN. Both were found by the same question — "does calling this in a loop
//! grow the heap the way php does not?" — and both are unbounded growth, not a fixed cost.
//!
//! 1. THE INLINER DEFERRED THE CALLEE'S FREES TO THE HOST EPILOGUE. `src/ir_passes/inline.rs`
//!    splices a small callee's body into its caller and rewrites `Return` into `Br`, which
//!    bypasses the callee's epilogue cleanup. Its own module doc argued the residual difference
//!    was "unobservable" because inlining is restricted to destructor-free types. It is very
//!    observable: the host epilogue runs ONCE, so a spliced body inside a loop overwrote the
//!    same slot on every iteration and only the LAST value was ever freed.
//!
//!    ```text
//!    function f(): int { $a = ["alpha","bravo","charlie","delta"]; return count($a); }
//!    for ($i = 0; $i < 200; $i++) { $t += f(); }
//!    HEAP DEBUG: allocs=1402 frees=407  live_blocks=995   // before (4 strings + 1 array / call)
//!    HEAP DEBUG: allocs=1402 frees=1402 live_blocks=0     // after
//!    ```
//!
//!    The fix releases those slots at the continuation block, where the callee frame would have
//!    died. Parameters and directly-returned slots stay excluded, exactly as the callee epilogue
//!    excludes them: the caller owns the arguments, and a returned value's ownership has moved.
//!
//! 2. `scandir()` PERSISTED EVERY ENTRY NAME TWICE (AArch64 only). The AArch64 loop called
//!    `__rt_str_persist` on `d_name` and then `__rt_array_push_str`, which persists its
//!    `(pointer, length)` pair itself — so the first block was never stored anywhere and never
//!    freed. One orphan per directory entry, per call; the x86_64 loop always pushed the raw
//!    pair directly, so only one architecture leaked. The leak scaled EXACTLY with the entry
//!    count (4 entries -> 4 blocks/call, 6 -> 6, 8 -> 8), which is what identified the strings
//!    rather than the array or its box.
//!
//! Together these made `for (...) { $d = scandir($dir); }` die with `Fatal error: heap memory
//! exhausted` at 200 iterations where `php -n` runs to completion.
//!
//! Called from:
//! - `cargo test --test inlined_callee_local_leak_tests` through Rust's test harness.
//!
//! Key details:
//! - `--heap-debug` is the authoritative instrument for elephc's own arena; it reports on the
//!   program's STDERR after `main` returns, so stdout stays exactly what the PHP program
//!   printed. BOTH halves are asserted: a heap counter alone cannot tell a balanced program
//!   from one that freed the same block twice, so the stdout assertion is what proves the
//!   values survived every iteration intact.
//! - Every case sits in a LOOP. One call leaks a handful of blocks, which reads as a fixed
//!   startup cost; the loop is what shows the growth is unbounded, and it is the shape that
//!   matters for a long-running `--web` worker.
//! - The by-parameter and by-return shapes are CONTROLS for the opposite failure. Releasing a
//!   borrowed argument, or a value whose ownership moved into the return, is a use-after-free
//!   rather than a leak — the exact defect the inliner's `BorrowedTemp` exclusion exists to
//!   prevent — so they assert real output, not just a clean heap.
//! - The inline-in-main shape is a control for the harness itself: it was always clean, so a
//!   suite that only ever saw green there would prove nothing.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Keeps only elephc's own diagnostics from a compile's stderr.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("error")
                || line.starts_with("Error")
                || line.starts_with("warning")
                || line.starts_with("Warning: ")
                || line.starts_with("EIR backend error")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compiles `source` with `--heap-debug`, runs it, and asserts stdout plus a clean heap.
fn assert_program_output_and_clean_heap(prefix: &str, source: &str, expected_stdout: &str) {
    let dir = make_test_dir(prefix);
    let php = dir.join(format!("{}.php", prefix));
    fs::write(&php, source).unwrap();

    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(&dir);
    cmd.arg("--heap-debug");
    cmd.arg(&php);
    let compiled = cmd.output().expect("failed to spawn elephc");
    let diagnostics = elephc_diagnostics(&String::from_utf8_lossy(&compiled.stderr));
    assert!(
        compiled.status.success(),
        "elephc compile failed:\n{diagnostics}"
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected elephc diagnostic:\n{diagnostics}"
    );

    let bin = dir.join(prefix);
    let output = Command::new(&bin)
        .current_dir(&dir)
        .output()
        .expect("failed to run compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{stderr}",
        output.status.code()
    );
    assert_eq!(stdout, expected_stdout, "program stdout diverged:\n{stderr}");
    assert!(
        stderr.contains("live_blocks=0"),
        "program leaked heap blocks:\n{stderr}"
    );
    assert!(
        stderr.contains("leak summary: clean"),
        "heap summary is not clean:\n{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// The inlined callee's own locals
// ---------------------------------------------------------------------------

/// Headline shape: a function building a string array, called 200 times, leaves no live block.
///
/// This is the exact program that measured `live_blocks=995` — four persisted strings plus the
/// array container per call, freed only once at the host epilogue.
#[test]
fn test_a_called_function_building_a_string_array_leaves_no_live_block() {
    assert_program_output_and_clean_heap(
        "inlined_local_string_array",
        r#"<?php
function f(): int {
    $a = ["alpha", "bravo", "charlie", "delta"];
    return count($a);
}
$t = 0;
for ($i = 0; $i < 200; $i++) {
    $t += f();
}
echo $t, "\n";
"#,
        "800\n",
    );
}

/// The same body written INLINE in main, which was always clean.
///
/// Control for the harness: without it, a suite that only ever exercised the broken shape could
/// not tell "the fix works" from "the instrument never reported anything".
#[test]
fn test_the_same_body_inline_in_main_is_also_clean() {
    assert_program_output_and_clean_heap(
        "inlined_local_control_main",
        r#"<?php
$t = 0;
for ($i = 0; $i < 200; $i++) {
    $a = ["alpha", "bravo", "charlie", "delta"];
    $t += count($a);
}
echo $t, "\n";
"#,
        "800\n",
    );
}

/// A callee whose local is RETURNED: ownership moves to the caller, so the callee must not
/// release it.
///
/// Control for the opposite failure. Releasing a directly-returned slot is a use-after-free, not
/// a leak, so the assertion is the returned CONTENT — a corrupted or freed container would print
/// something else or crash long before the heap summary is read.
#[test]
fn test_a_returned_local_keeps_its_value_and_still_frees_once() {
    assert_program_output_and_clean_heap(
        "inlined_local_returned",
        r#"<?php
function build(): array {
    $a = ["alpha", "bravo", "charlie"];
    return $a;
}
$last = "";
for ($i = 0; $i < 50; $i++) {
    $b = build();
    $last = implode(",", $b);
}
echo $last, "\n";
"#,
        "alpha,bravo,charlie\n",
    );
}

/// A callee reading a BORROWED parameter: the caller owns the argument, so the callee must not
/// release it.
///
/// The other half of the same control. This is the shape whose over-eager release once made a
/// read-only `array` parameter in a loop die with `heap debug detected bad refcount`.
#[test]
fn test_a_borrowed_array_parameter_is_not_released_by_the_callee() {
    assert_program_output_and_clean_heap(
        "inlined_local_borrowed_param",
        r#"<?php
function total(array $a): int {
    return count($a);
}
$shared = ["alpha", "bravo", "charlie"];
$t = 0;
for ($i = 0; $i < 200; $i++) {
    $t += total($shared);
}
echo $t, ",", implode(",", $shared), "\n";
"#,
        "600,alpha,bravo,charlie\n",
    );
}

// ---------------------------------------------------------------------------
// scandir()'s per-entry listing strings
// ---------------------------------------------------------------------------

/// `scandir()` called in a loop leaves no live block, and still answers php's listing.
///
/// The double persist leaked one block per directory entry per call, so this program exhausted
/// the heap outright at 200 iterations. The stdout assertion covers the entries themselves:
/// dropping the redundant persist means the pushed pair now comes straight from `d_name`, and a
/// mis-measured or clobbered name would show up here rather than in the heap counter.
#[test]
fn test_scandir_in_a_loop_leaves_no_live_block() {
    assert_program_output_and_clean_heap(
        "scandir_listing_leak",
        r#"<?php
mkdir("sld");
file_put_contents("sld/a.txt", "1");
file_put_contents("sld/b.txt", "2");
$last = "";
for ($i = 0; $i < 200; $i++) {
    $d = scandir("sld");
    $last = implode(",", $d);
}
unlink("sld/a.txt");
unlink("sld/b.txt");
rmdir("sld");
echo $last, "\n";
"#,
        ".,..,a.txt,b.txt\n",
    );
}
