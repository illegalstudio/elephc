//! Purpose:
//! End-to-end heap-ownership tests for builtins whose runtime helper returns a FRESH owned
//! heap string: `tempnam()`, `getcwd()`, `microtime()`, `json_encode()`, `date()` and
//! `gmdate()`.
//!
//! THE BUG THESE PIN. `value_is_scratch_string`
//! (`src/codegen/lower_inst/ownership.rs`) decides whether an `Op::Release` on a `Str`
//! actually emits a `__rt_heap_free_safe`. For an `Op::RuntimeCall` it releases ONLY when the
//! callee declares `BuiltinResultOwnership::Fresh`; everything else falls into the default
//! `MayAliasArguments` bucket and is treated as borrowed concat scratch, so the release is
//! skipped silently. All six builtins here produce independently owned results yet sat in
//! that default bucket, so each leaked one heap block PER CALL:
//!
//! ```text
//! for ($i = 0; $i < 5; $i++) { $x = tempnam("/tmp", "p"); }
//! HEAP DEBUG: allocs=16 frees=11 live_blocks=5      // before
//! HEAP DEBUG: allocs=16 frees=16 live_blocks=0      // after
//! ```
//!
//! The EIR was already correct — `--emit-ir` shows `release v2` with `own=maybe_owned` — and
//! the emitted assembly contained ZERO `__rt_decref_any`. That gap between a correct IR and a
//! no-op backend is why this was invisible to every existing suite.
//!
//! WHY THIS FILE AND NOT SIX SEPARATE ONES. This is a recurring FAMILY, not six incidents:
//! `RuntimeFnId::result_ownership` already carries three in-line comments documenting the
//! identical mistake being fixed for `array_flip`, `print_r` and `strstr`. Pinning the family
//! in one place is what makes each recurrence visible as a pattern.
//!
//! Called from:
//! - `cargo test --test fresh_result_ownership_leak_tests` through Rust's test harness.
//!
//! Key details:
//! - `--heap-debug` is the authoritative instrument for elephc's own arena; it reports on the
//!   program's STDERR after `main` returns, so stdout stays exactly what the PHP program
//!   printed. Both halves are asserted — a program that silently stopped producing output
//!   would otherwise "leak nothing" and pass.
//! - Every call sits in a LOOP. A single call leaks one block, which is easy to mistake for a
//!   fixed startup allocation; the loop is what shows the leak is unbounded, and it is the
//!   shape that matters for a long-running `--web` worker.
//! - `sys_get_temp_dir()` and `tmpfile()` are pinned as CONTROLS. The report that prompted
//!   this work named them alongside `tempnam()`, but measurement showed both were already
//!   clean; keeping them here stops a future "fix" from marking them Fresh on the strength of
//!   that report.
//! - The aliasing test is the safety half. Marking a result `Fresh` asserts it aliases no
//!   argument; if that were wrong the release would free an argument's storage, so each
//!   builtin is called with a heap-string argument that is READ AFTER the call.
//! - Host-target only: this is an IR-classification change with no architecture-specific
//!   assembly. Reference values captured from PHP 8.5.6 with `php -d xdebug.mode=off`.

use std::fs;
use std::path::{Path, PathBuf};
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
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather than
/// anything elephc emitted.
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

/// Compiles `source` with `--heap-debug`, asserting elephc reported no diagnostic.
fn compile_with_heap_debug(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg("--heap-debug");
    cmd.arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let diagnostics = elephc_diagnostics(&stderr);
    // Report RAW stderr when the filter matched nothing: a `native project error` starts with
    // none of the recognised prefixes, and filtering first would assert with an empty message.
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        if diagnostics.is_empty() {
            stderr.trim()
        } else {
            &diagnostics
        }
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected elephc diagnostic:\n{diagnostics}"
    );
    dir.join(stem)
}

/// Compiles `source` with `--heap-debug`, runs it, and asserts stdout plus a clean heap.
fn assert_output_and_clean_heap(prefix: &str, source: &str, expected_stdout: &str) {
    let dir = make_test_dir(prefix);
    let bin = compile_with_heap_debug(&dir, source, prefix);
    let output = Command::new(&bin)
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
        "{prefix} leaked heap blocks:\n{stderr}"
    );
    assert!(
        stderr.contains("leak summary: clean"),
        "{prefix} heap summary is not clean:\n{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// The six builtins that leaked, each in the repeated-call shape that showed it.
// ---------------------------------------------------------------------------

/// Verifies repeated `tempnam()` calls reclaim every generated path string.
///
/// The declared debt reported this as a constant 48-byte leak in
/// `tempnam()`/`sys_get_temp_dir()`. Measurement corrected both halves: the leak is PER CALL
/// (10 calls left 10 live blocks), and `sys_get_temp_dir()` was never involved — the same leak
/// reproduces with a literal `"/tmp"` directory argument, which is the shape used here.
#[test]
fn repeated_tempnam_calls_leave_a_clean_heap() {
    let source = r#"<?php
for ($i = 0; $i < 5; $i++) {
    $path = tempnam("/tmp", "elephc_leakprobe");
    @unlink($path);
}
echo "done\n";
"#;
    assert_output_and_clean_heap("elephc_leak_tempnam", source, "done\n");
}

/// Verifies repeated `getcwd()` calls reclaim every returned path string.
///
/// `getcwd()` takes NO arguments, so `Fresh` is airtight for it: there is nothing its result
/// could alias.
#[test]
fn repeated_getcwd_calls_leave_a_clean_heap() {
    let source = r#"<?php
$total = 0;
for ($i = 0; $i < 5; $i++) {
    $dir = getcwd();
    $total += strlen($dir) > 0 ? 1 : 0;
}
echo $total, "\n";
"#;
    assert_output_and_clean_heap("elephc_leak_getcwd", source, "5\n");
}

/// Verifies repeated string-mode `microtime()` calls reclaim every formatted string.
///
/// Only the STRING mode allocates; `microtime(true)` returns a float and never reached the
/// leaking path.
#[test]
fn repeated_microtime_calls_leave_a_clean_heap() {
    let source = r#"<?php
$total = 0;
for ($i = 0; $i < 5; $i++) {
    $now = microtime();
    $total += strpos($now, " ") !== false ? 1 : 0;
}
echo $total, "\n";
"#;
    assert_output_and_clean_heap("elephc_leak_microtime", source, "5\n");
}

/// Verifies repeated `json_encode()` calls reclaim every rendered document.
#[test]
fn repeated_json_encode_calls_leave_a_clean_heap() {
    let source = r#"<?php
$total = 0;
for ($i = 0; $i < 5; $i++) {
    $json = json_encode([1, 2, 3]);
    $total += strlen($json);
}
echo $total, "\n";
"#;
    assert_output_and_clean_heap("elephc_leak_json_encode", source, "35\n");
}

/// Verifies repeated `date()` and `gmdate()` calls reclaim every rendered string.
///
/// Both runtime helpers render new bytes and therefore cannot alias their heap-built format
/// argument. Reading that argument after every call also pins the safety half of the `Fresh`
/// contract: releasing a result must not invalidate argument storage.
#[test]
fn repeated_date_and_gmdate_calls_leave_a_clean_heap() {
    let source = r#"<?php
$format = str_repeat("Y-m-d", 1);
$total = 0;
for ($i = 0; $i < 40; $i++) {
    $local = date($format, 0);
    $utc = gmdate($format, 0);
    $total += strlen($local) + strlen($utc);
}
echo $format, "|", $total, "\n";
"#;
    assert_output_and_clean_heap(
        "elephc_leak_date_gmdate",
        source,
        "Y-m-d|800\n",
    );
}

// ---------------------------------------------------------------------------
// Controls: already clean before this change, and must stay out of `Fresh`.
// ---------------------------------------------------------------------------

/// Pins `sys_get_temp_dir()` and `tmpfile()` as clean WITHOUT being `Fresh`-classified.
///
/// Both were named in the report that prompted this work. Measuring them showed
/// `sys_get_temp_dir()` returns borrowed storage and `tmpfile()` allocates nothing that
/// outlives its handle, so marking either `Fresh` would add a free the runtime does not own.
/// This test is what makes that regression visible rather than silent.
#[test]
fn temp_dir_and_tmpfile_stay_clean_without_being_fresh() {
    let source = r#"<?php
$total = 0;
for ($i = 0; $i < 5; $i++) {
    $dir = sys_get_temp_dir();
    $total += strlen($dir) > 0 ? 1 : 0;
    $handle = tmpfile();
    $total += is_resource($handle) ? 1 : 0;
    fclose($handle);
}
echo $total, "\n";
"#;
    assert_output_and_clean_heap("elephc_leak_control_tempdir", source, "10\n");
}

// ---------------------------------------------------------------------------
// Safety: `Fresh` asserts the result aliases NO argument.
// ---------------------------------------------------------------------------

/// Verifies each newly-`Fresh` builtin leaves its heap-string ARGUMENTS intact.
///
/// This is the half that would fail if `Fresh` were the wrong classification: releasing a
/// result that actually aliased an argument would free storage the caller still owns, and the
/// argument would read back as corrupt bytes rather than its original value. Every argument
/// here is a runtime-built heap string (never a literal, which would live in `.rodata` and
/// mask the bug) and is READ AFTER the call.
///
/// Expected output captured from PHP 8.5.6 with `php -d xdebug.mode=off`.
#[test]
fn fresh_results_do_not_alias_their_heap_string_arguments() {
    let source = r#"<?php
$payload = str_repeat("ab", 8);
$encoded = json_encode($payload);
echo $payload, "|", $encoded, "|", strlen($payload), "\n";

$directory = str_repeat("/tmp", 1);
$path = tempnam($directory, "elephc_alias");
echo $directory, "|", (strlen($path) > strlen($directory) ? "fresh" : "aliased"), "\n";
@unlink($path);
"#;
    let expected = "abababababababab|\"abababababababab\"|16\n/tmp|fresh\n";
    assert_output_and_clean_heap("elephc_leak_alias", source, expected);
}
