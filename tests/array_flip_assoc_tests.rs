//! Purpose:
//! End-to-end array_flip coverage for associative and packed function results.
//! Pins key normalization, duplicate replacement, warnings and independent result ownership.
//!
//! Called from:
//! - `cargo test --test array_flip_assoc_tests` through Rust's test harness.
//!
//! Key details:
//! - Fixtures compile and run in isolated directories on the host target.
//! - Declared array returns exercise boxed runtime dispatch instead of constant folding.
//! - Heap fixtures assert warnings from program stderr separately from compiler diagnostics.

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

/// Runs the compiler on `source` with `extra_args` and returns its raw output.
fn compile_raw(
    dir: &Path,
    source: &str,
    stem: &str,
    extra_args: &[&str],
) -> std::process::Output {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.args(extra_args);
    cmd.arg(&php);
    cmd.output().expect("failed to spawn elephc")
}

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather than
/// anything elephc emitted: GNU `ld` on Linux reports the static-`getaddrinfo` glibc notes and
/// the `.note.GNU-stack` deprecation, while Apple's linker stays silent. elephc's own lines
/// start with `error`/`warning`, or with `EIR backend error` for a backend refusal — that last
/// prefix matters here, because every deliberate `array_flip` refusal in this file is reported
/// through it and a filter that dropped it would let a regression compile silently.
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

/// Compiles `source` to a plain executable, asserting elephc reported no diagnostic.
fn compile(dir: &Path, source: &str, stem: &str, extra_args: &[&str]) -> PathBuf {
    let output = compile_raw(dir, source, stem, extra_args);
    let diagnostics = elephc_diagnostics(&String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "elephc compile failed:\n{diagnostics}"
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected elephc diagnostic:\n{diagnostics}"
    );
    dir.join(stem)
}

/// Runs a compiled executable and returns its stdout, asserting a clean exit.
fn run_binary(bin: &Path) -> String {
    let output = Command::new(bin)
        .output()
        .expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Compiles `source`, runs it, and asserts stdout equals `expected`.
fn assert_program_output(prefix: &str, source: &str, expected: &str) {
    let dir = make_test_dir(prefix);
    let bin = compile(&dir, source, prefix, &[]);
    assert_eq!(run_binary(&bin), expected);
    let _ = fs::remove_dir_all(&dir);
}

/// Compiles `source` with `--heap-debug`, runs it, and asserts stdout plus a clean heap.
///
/// `--heap-debug` reports on the program's STDERR after `main` returns, so stdout stays exactly
/// what the PHP program printed. Both halves are asserted: a program that silently stopped
/// producing output would otherwise "leak nothing" and pass.
fn assert_program_output_and_clean_heap(prefix: &str, source: &str, expected_stdout: &str) {
    assert_program_output_warnings_and_clean_heap(prefix, source, expected_stdout, 0);
}

/// Checks skipped-entry diagnostics alongside the flipped values and final heap balance.
fn assert_program_output_warnings_and_clean_heap(
    prefix: &str, source: &str, expected_stdout: &str, warnings: usize,
) {
    let dir = make_test_dir(prefix);
    let bin = compile(&dir, source, prefix, &["--heap-debug"]);
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
    assert_eq!(stderr.matches("Can only flip string and integer values, entry skipped").count(), warnings, "{stderr}");
    assert!(
        stderr.contains("live_blocks=0"),
        "array_flip leaked heap blocks:\n{stderr}"
    );
    assert!(
        stderr.contains("leak summary: clean"),
        "array_flip heap summary is not clean:\n{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Flipping an ASSOCIATIVE source — the `__rt_hash_flip` path
// ---------------------------------------------------------------------------

/// Headline shape: a `string => int` hash flips into an `int => string` hash.
///
/// Source keys become values and source values become keys, in insertion order. The source is a
/// function return so the flip runs at RUNTIME rather than being folded away.
/// Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn an_associative_string_to_int_source_flips_into_int_keys() {
    assert_program_output(
        "flip_assoc_str_int",
        r#"<?php
function build(): array { return ["a" => 1, "b" => 2]; }
var_dump(array_flip(build()));
"#,
        "array(2) {\n  [1]=>\n  string(1) \"a\"\n  [2]=>\n  string(1) \"b\"\n}\n",
    );
}

/// A `string => string` hash flips into a `string => string` hash.
///
/// This is the arm that runs the flipped key through `__rt_hash_normalize_key` and the flipped
/// value through `__rt_str_persist`, so both halves of a string entry are exercised at once.
/// Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn an_associative_string_to_string_source_flips_both_halves() {
    assert_program_output(
        "flip_assoc_str_str",
        r#"<?php
function build(): array { return ["a" => "x", "b" => "y"]; }
var_dump(array_flip(build()));
"#,
        "array(2) {\n  [\"x\"]=>\n  string(1) \"a\"\n  [\"y\"]=>\n  string(1) \"b\"\n}\n",
    );
}

/// An `int => string` hash flips into a `string => int` hash.
///
/// The mirror of the first test: here the INLINE INTEGER key path becomes the flipped value and
/// the string value becomes the flipped key. Those are separate arms in `__rt_hash_flip`
/// (`__rt_hash_flip_value_int` vs `__rt_hash_flip_key_str`), so neither can be fixed by the
/// other's code. Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn an_associative_int_to_string_source_flips_into_string_keys() {
    assert_program_output(
        "flip_assoc_int_str",
        r#"<?php
function build(): array { return [5 => "x", 7 => "y"]; }
var_dump(array_flip(build()));
"#,
        "array(2) {\n  [\"x\"]=>\n  int(5)\n  [\"y\"]=>\n  int(7)\n}\n",
    );
}

/// php-src collapses a NUMERIC-STRING value into an integer key; the flip must do the same.
///
/// `array_flip(["a" => "5"]) === [5 => "a"]` — the flipped key is `int(5)`, not `string "5"`.
/// `__rt_hash_flip` gets this for free by routing string values through
/// `__rt_hash_normalize_key`, and this test is what keeps that routing in place: storing the raw
/// string would produce a key that `$flipped[5]` could never find.
/// Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn a_numeric_string_value_collapses_to_an_integer_key() {
    assert_program_output(
        "flip_assoc_numeric_string",
        r#"<?php
function build(): array { return ["a" => "5", "b" => "07"]; }
var_dump(array_flip(build()));
"#,
        "array(2) {\n  [5]=>\n  string(1) \"a\"\n  [\"07\"]=>\n  string(1) \"b\"\n}\n",
    );
}

/// Insertion order survives the flip, and a duplicate value keeps the LAST source key.
///
/// php-src overwrites an existing flipped key rather than skipping it, so `["a" => 1, "b" => 1]`
/// flips to `[1 => "b"]`. `__rt_hash_set` reaches that by updating in place, which also means it
/// must RELEASE the overwritten value — a path no other test in this file takes.
/// Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn a_duplicate_value_keeps_the_last_source_key() {
    assert_program_output(
        "flip_assoc_duplicate_value",
        r#"<?php
function build(): array { return ["a" => 1, "b" => 1, "c" => 2]; }
var_dump(array_flip(build()));
"#,
        "array(2) {\n  [1]=>\n  string(1) \"b\"\n  [2]=>\n  string(1) \"c\"\n}\n",
    );
}

/// An empty associative source flips to an empty array without touching the loop body.
///
/// Reference PHP 8.5.6 prints `array(0) {\n}`.
#[test]
fn an_empty_associative_source_flips_to_an_empty_array() {
    assert_program_output(
        "flip_assoc_empty",
        r#"<?php
function build(): array { $a = ["a" => 1]; unset($a["a"]); return $a; }
var_dump(array_flip(build()));
"#,
        "array(0) {\n}\n",
    );
}

// ---------------------------------------------------------------------------
// Indexed sources — regression pins for the pre-existing helpers
// ---------------------------------------------------------------------------

/// A boxed indexed string result flips values into keys and positions into values.
#[test]
fn an_indexed_string_source_still_flips_to_value_keyed_positions() {
    assert_program_output(
        "flip_indexed_str",
        r#"<?php
function build(): array { return ["p", "q"]; }
var_dump(array_flip(build()));
"#,
        "array(2) {\n  [\"p\"]=>\n  int(0)\n  [\"q\"]=>\n  int(1)\n}\n",
    );
}

/// A boxed indexed integer result preserves positions as flipped values.
#[test]
fn an_indexed_int_source_still_flips_to_value_keyed_positions() {
    assert_program_output(
        "flip_indexed_int",
        r#"<?php
function build(): array { return [10, 20]; }
var_dump(array_flip(build()));
"#,
        "array(2) {\n  [10]=>\n  int(0)\n  [20]=>\n  int(1)\n}\n",
    );
}

// ---------------------------------------------------------------------------
// Runtime tags and skipped entries in declared array returns
// ---------------------------------------------------------------------------

/// Mixed integer/string entries retain their runtime tags instead of becoming pointer keys.
#[test]
fn a_mixed_valued_associative_source_flips_runtime_tags() {
    assert_program_output_and_clean_heap(
        "flip_assoc_mixed",
        r#"<?php
function build(): array { return ["a" => 1, "b" => "s"]; }
var_dump(array_flip(build()));
"#,
        "array(2) {\n  [1]=>\n  string(1) \"a\"\n  [\"s\"]=>\n  string(1) \"b\"\n}\n",
    );
}

/// A boxed float-valued source warns for each skipped entry and returns an empty array.
#[test]
fn a_float_valued_associative_source_is_warn_skipped() {
    assert_program_output_warnings_and_clean_heap(
        "flip_assoc_float",
        r#"<?php
function build(): array { return ["a" => 1.5, "b" => 2.5]; }
var_dump(array_flip(build()));
"#,
        "array(0) {\n}\n", 2,
    );
}

/// A boxed bool-valued source warns without coercing its value into an integer key.
#[test]
fn a_bool_valued_associative_source_is_warn_skipped() {
    assert_program_output_warnings_and_clean_heap(
        "flip_assoc_bool",
        r#"<?php
function build(bool $flag): array { return ["a" => $flag]; }
var_dump(array_flip(build(true)));
"#,
        "array(0) {\n}\n", 1,
    );
}

/// Nested array values are skipped with warnings and their source ownership is released.
#[test]
fn an_array_valued_associative_source_is_warn_skipped() {
    assert_program_output_warnings_and_clean_heap(
        "flip_assoc_array",
        r#"<?php
function build(): array { return ["a" => [1], "b" => [2]]; }
var_dump(array_flip(build()));
"#,
        "array(0) {\n}\n", 2,
    );
}

// ---------------------------------------------------------------------------
// Result ownership — `RuntimeFnId::ArrayFlip` must stay `Fresh`
// ---------------------------------------------------------------------------

/// THE LEAKING SHAPE: `array_flip(build())` over a hash must end with `live_blocks=0`.
///
/// The argument is an owned TEMPORARY, which is the only shape the ownership bug reached: while
/// `ArrayFlip` sat in the `MayAliasArguments` bucket the source table was never released, so
/// each iteration leaked the whole hash. The loop runs enough times that a per-call leak cannot
/// hide inside allocator noise, and the printed total proves the flips really happened.
#[test]
fn flipping_an_owned_temporary_hash_leaves_the_heap_clean() {
    assert_program_output_and_clean_heap(
        "flip_assoc_heap_temp",
        r#"<?php
function build(): array { return ["a" => 1, "b" => 2]; }
$n = 0;
for ($i = 0; $i < 50; $i++) {
    $flipped = array_flip(build());
    $n += count($flipped);
}
echo $n, "\n";
"#,
        "100\n",
    );
}

/// The same flip through a NAMED LOCAL was already clean; it must stay that way.
///
/// This is the control that separates the two halves of the ownership fix: the named-local shape
/// releases the source through the local's own lifetime, so it never depended on the result
/// classification. A `Fresh` result that started double-releasing the source would show up HERE
/// first, as a crash or a negative live-block count, rather than in the temporary shape above.
#[test]
fn flipping_a_named_local_hash_leaves_the_heap_clean() {
    assert_program_output_and_clean_heap(
        "flip_assoc_heap_local",
        r#"<?php
function build(): array { return ["a" => 1, "b" => 2]; }
$n = 0;
for ($i = 0; $i < 50; $i++) {
    $source = build();
    $flipped = array_flip($source);
    $n += count($flipped);
}
echo $n, "\n";
"#,
        "100\n",
    );
}

/// The temporary-argument shape over a STRING-valued hash must also end clean.
///
/// This flip persists one heap string per entry (`__rt_str_persist` for the flipped value) on
/// top of the source table, so it is the shape where a missing release shows up fastest.
#[test]
fn flipping_an_owned_temporary_string_valued_hash_leaves_the_heap_clean() {
    assert_program_output_and_clean_heap(
        "flip_assoc_heap_str",
        r#"<?php
function build(): array { return ["a" => "x", "b" => "y"]; }
$n = 0;
for ($i = 0; $i < 50; $i++) {
    $flipped = array_flip(build());
    $n += count($flipped);
}
echo $n, "\n";
"#,
        "100\n",
    );
}

/// The INDEXED temporary shape must end clean too — `ArrayFlip` is one ownership entry.
///
/// `result_ownership()` is per RUNTIME OPERATION, not per lowering, so the same `Fresh`
/// classification governs `__rt_array_flip` / `__rt_array_flip_string`. Pinning the indexed side
/// keeps a future "only the hash path allocates" argument from re-narrowing the entry.
#[test]
fn flipping_an_owned_temporary_indexed_array_leaves_the_heap_clean() {
    assert_program_output_and_clean_heap(
        "flip_indexed_heap_temp",
        r#"<?php
function build(): array { return ["p", "q"]; }
$n = 0;
for ($i = 0; $i < 50; $i++) {
    $flipped = array_flip(build());
    $n += count($flipped);
}
echo $n, "\n";
"#,
        "100\n",
    );
}
