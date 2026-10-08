//! Purpose:
//! End-to-end heap-ownership tests for `array_map()`'s RESULT — both the container itself (the
//! boxing step in `box_array_result_for_mixed_builtin` / `box_hash_result_for_mixed_builtin`,
//! `src/codegen/lower_inst/builtins/arrays.rs`) and the STRING VALUES the callback produced into
//! it (the uniform callable invoker's return boxing, `emit_boxed_invoker_return` in
//! `src/codegen/runtime_callable_invoker.rs`).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Heap-debug assertions distinguish container ownership from callback-result ownership.
//! - Fixtures use integer inputs so the separate descriptor string-argument leak does not
//!   obscure the regressions pinned here.
//!
//! THE DEFECT THESE PIN. `array_map`'s EIR result slot is `Heap(Mixed)`, on purpose: a string or
//! descriptor callback picks its result ABI at runtime, so the slot cannot be the concrete
//! container type (see `eir_result_type` in `src/builtins/array/array_map.rs` — narrowing it
//! makes every `array_map('name', ...)` call site fail to compile). Because the slot is Mixed,
//! the lowering boxes the mapped container into a Mixed cell, and
//! `__rt_mixed_from_value` INCREFs a container-tagged payload
//! (`src/codegen_support/runtime/arrays/mixed_from_value.rs`, the `_retain` arm).
//!
//! That retain is the right contract for a BORROWED payload and the wrong one for this payload:
//! the container arrives fresh from `__rt_array_new`/`__rt_hash_new` at refcount 1, and once the
//! box happens the EIR value for the instruction is the MIXED CELL, so every EIR-emitted release
//! — including `emit_store_result_to_symbol(.., release_previous)` — acts on the cell and can
//! never reach the container's own reference. The container and its element buffer therefore
//! outlived the program: an indexed `[1]` source leaked 2 blocks per call, `[1,2,3]` leaked 4,
//! and the hash path leaked 3 for a single entry. The fix TRANSFERS the reference into the cell
//! (`emit_box_current_owned_value_as_mixed`) instead of sharing it.
//!
//! THE SECOND DEFECT THESE PIN — MAPPED STRING VALUES. With the container fixed, an associative
//! source whose callback returns a STRING still leaked 6 heap blocks per iteration for a two-entry
//! hash. That leak is NOT `array_map`'s: measured against a direct closure call with the SAME call
//! count and the SAME closure body, `array_map` adds exactly ZERO blocks, and the destination
//! hash's `value_type` tag is already correct (`runtime_value_tag("array_map", callback_elem_ty)`,
//! and `__rt_hash_free_deep` dispatches on the PER-ENTRY tag `__rt_hash_set` stamps anyway).
//!
//! The leak is in the uniform callable invoker, which every closure / arrow fn / first-class
//! callable / callable-array / string callback goes through once it is invoked inside a loop.
//! `call_target_with_pushed_args` persists the callable's `Str` return
//! (`restore_concat_offset_after_nested_call`), and the invoker then boxed that OWNED copy with
//! the BORROWED-payload boxer, whose `__rt_mixed_from_value` persists a SECOND copy for the Mixed
//! cell — orphaning the first. `emit_boxed_invoker_return` now moves the already-owned copy into
//! the cell instead. That ELIDES an allocation and adds no release, so the Mixed cell owns exactly
//! one string payload before and after and no double free is possible.
//!
//! STILL LEAKING, DELIBERATELY NOT PINNED AS CLEAN: passing a STRING ARGUMENT into a
//! descriptor-invoked callable still orphans one block per argument per call (the `Mixed -> Str`
//! coercion in `coerce_result_to_type` persists a copy the callee borrows and nobody frees). It
//! reproduces with zero `array_map` in the program, and closing it means ADDING a free on a path
//! shared by every callable call — so the cells below all use INT-valued sources.
//!
//! Called from:
//! - `cargo test --test array_map_result_heap_tests` through Rust's test harness.
//!
//! Key details:
//! - WHY EVERY TEST ASSERTS OUTPUT AS WELL AS `live_blocks=0`. The failure mode on the other side
//!   of this fix is a DOUBLE FREE / premature free, which on macOS-aarch64 also reports
//!   `live_blocks=0` and `leak summary: clean` — a counter alone cannot tell "balanced" from
//!   "freed twice". So each test reads the mapped array back on EVERY loop iteration and
//!   accumulates a checksum, then `var_dump`s the survivor after the loop: a container released
//!   too early shows up as wrong data or a crash rather than as a quietly passing heap counter.
//! - CONTROLS THAT WERE NEVER BROKEN are pinned too (`array_flip`, `array_column`), so the suite
//!   distinguishes "fixed" from "never broken". `array_column` matters most: it shares
//!   `box_array_result_for_mixed_builtin` with `array_map` but its EIR slot is the concrete
//!   container type, so it never boxes and never leaked — it is the regression guard for the
//!   branch the fix must leave untouched.
//! - Every expected value was captured from reference PHP 8.5.6 (`php -d xdebug.mode=off`); the
//!   host `php` loads Xdebug, which overloads `var_dump`, so that flag is mandatory.
//! - Sources come from a FUNCTION RETURN rather than a literal so the constant folder cannot
//!   answer in place of the runtime helper.
//! - Compile stderr is filtered through `elephc_diagnostics` so the HOST linker's environmental
//!   warnings (GNU `ld` on Linux, silent on macOS) cannot fail the suite.

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
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather than
/// anything elephc emitted. elephc's own lines start with `error`/`warning`, or with
/// `EIR backend error` for a backend refusal.
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
///
/// `--heap-debug` reports on the program's STDERR after `main` returns, so stdout stays exactly
/// what the PHP program printed. BOTH halves are asserted on purpose: a heap counter alone
/// cannot distinguish a balanced program from one that freed the same block twice, so the stdout
/// assertion is what actually proves the container survived every loop iteration intact.
fn assert_program_output_and_clean_heap(prefix: &str, source: &str, expected_stdout: &str) {
    let dir = make_test_dir(prefix);
    let php = dir.join(format!("{}.php", prefix));
    fs::write(&php, source).unwrap();

    let mut cmd = Command::new(elephc_bin());
    cmd.arg("build");
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
        "array_map leaked heap blocks:\n{stderr}"
    );
    assert!(
        stderr.contains("leak summary: clean"),
        "heap summary is not clean:\n{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The `var_dump` block reference PHP 8.5.6 prints for the mapped `[1,2,3]` source, plus the
/// per-iteration checksum `10 * (2 + 4 + 6)`.
const MAPPED_INDEXED_OUTPUT: &str =
    "array(3) {\n  [0]=>\n  int(2)\n  [1]=>\n  int(4)\n  [2]=>\n  int(6)\n}\n120\n";

// ---------------------------------------------------------------------------
// The indexed result container — `box_array_result_for_mixed_builtin`
// ---------------------------------------------------------------------------

/// Headline shape: a single-element indexed source, mapped in a loop, leaves no live block.
///
/// This is the smallest reproduction of the container leak — `[1]` leaked exactly 2 blocks per
/// call (the array descriptor and its element buffer), so ten iterations reported
/// `live_blocks=20`. Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn a_single_element_indexed_map_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_idx_one",
        r#"<?php
function build(): array { return [1]; }
function double(int $n): int { return $n * 2; }
$sum = 0;
for ($i = 0; $i < 10; $i++) {
    $r = array_map('double', build());
    $sum = $sum + $r[0];
}
var_dump($r);
echo $sum, "\n";
"#,
        "array(1) {\n  [0]=>\n  int(2)\n}\n20\n",
    );
}

/// The leak scaled with the source length, so a three-element source is pinned separately.
///
/// `[1,2,3]` leaked 4 blocks per call (`live_blocks=40` over ten iterations) against the 2 of the
/// single-element shape; a fix that only balanced the descriptor and not the buffer it owns would
/// pass the test above and fail here. Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn a_multi_element_indexed_map_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_idx_three",
        r#"<?php
function build(): array { return [1, 2, 3]; }
function double(int $n): int { return $n * 2; }
$sum = 0;
for ($i = 0; $i < 10; $i++) {
    $r = array_map('double', build());
    $sum = $sum + $r[0] + $r[1] + $r[2];
}
var_dump($r);
echo $sum, "\n";
"#,
        MAPPED_INDEXED_OUTPUT,
    );
}

// ---------------------------------------------------------------------------
// The associative result container — `box_hash_result_for_mixed_builtin`
// ---------------------------------------------------------------------------

/// An associative source goes through `__rt_hash_map` and a SEPARATE boxing helper.
///
/// The hash path leaked more per entry than the indexed one (3 blocks for a single entry, 7 for
/// three) because a hash allocates its table on top of the entry storage, and it is boxed by
/// `box_hash_result_for_mixed_builtin` rather than `box_array_result_for_mixed_builtin` — a fix
/// applied to only one of the two helpers passes the indexed tests and fails this one.
/// php-src's single-array `array_map()` PRESERVES keys, so the result stays keyed `a`/`b`.
/// Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn an_associative_map_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_assoc",
        r#"<?php
function build(): array { return ["a" => 1, "b" => 2]; }
function double(int $n): int { return $n * 2; }
$sum = 0;
for ($i = 0; $i < 10; $i++) {
    $r = array_map('double', build());
    $sum = $sum + $r["a"] + $r["b"];
}
var_dump($r);
echo $sum, "\n";
"#,
        "array(2) {\n  [\"a\"]=>\n  int(2)\n  [\"b\"]=>\n  int(4)\n}\n60\n",
    );
}

// ---------------------------------------------------------------------------
// Callback shapes — all four bind through the same result path
// ---------------------------------------------------------------------------

/// A CLOSURE callback maps cleanly.
///
/// All four callback shapes leaked identically (4 blocks per call on a three-element source),
/// which is what identified the result path rather than any callback binding as the culprit.
/// They are pinned individually so a future change to one binding cannot regress in silence.
/// Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn a_closure_callback_map_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_closure",
        r#"<?php
function build(): array { return [1, 2, 3]; }
$sum = 0;
for ($i = 0; $i < 10; $i++) {
    $r = array_map(function (int $n): int { return $n * 2; }, build());
    $sum = $sum + $r[0] + $r[1] + $r[2];
}
var_dump($r);
echo $sum, "\n";
"#,
        MAPPED_INDEXED_OUTPUT,
    );
}

/// An ARROW FUNCTION callback maps cleanly.
///
/// Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn an_arrow_fn_callback_map_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_arrow",
        r#"<?php
function build(): array { return [1, 2, 3]; }
$sum = 0;
for ($i = 0; $i < 10; $i++) {
    $r = array_map(fn (int $n): int => $n * 2, build());
    $sum = $sum + $r[0] + $r[1] + $r[2];
}
var_dump($r);
echo $sum, "\n";
"#,
        MAPPED_INDEXED_OUTPUT,
    );
}

/// A FIRST-CLASS CALLABLE callback maps cleanly.
///
/// Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn a_first_class_callable_map_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_fcc",
        r#"<?php
function build(): array { return [1, 2, 3]; }
function double(int $n): int { return $n * 2; }
$sum = 0;
for ($i = 0; $i < 10; $i++) {
    $r = array_map(double(...), build());
    $sum = $sum + $r[0] + $r[1] + $r[2];
}
var_dump($r);
echo $sum, "\n";
"#,
        MAPPED_INDEXED_OUTPUT,
    );
}

/// A CALLABLE-ARRAY (`[Class, method]`) callback maps cleanly.
///
/// Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn a_callable_array_callback_map_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_callable_array",
        r#"<?php
class Doubler { public static function twice(int $n): int { return $n * 2; } }
function build(): array { return [1, 2, 3]; }
$sum = 0;
for ($i = 0; $i < 10; $i++) {
    $r = array_map(['Doubler', 'twice'], build());
    $sum = $sum + $r[0] + $r[1] + $r[2];
}
var_dump($r);
echo $sum, "\n";
"#,
        MAPPED_INDEXED_OUTPUT,
    );
}

// ---------------------------------------------------------------------------
// Result disposition — the container leaked no matter what happened to it
// ---------------------------------------------------------------------------

/// A DISCARDED result leaves no live block.
///
/// `array_map(...)` as a bare statement still allocates and boxes its container, so it leaked the
/// full 4 blocks per call with nothing ever reading the result. Reference PHP 8.5.6 prints
/// `done`.
#[test]
fn a_discarded_map_result_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_discard",
        r#"<?php
function build(): array { return [1, 2, 3]; }
function double(int $n): int { return $n * 2; }
for ($i = 0; $i < 10; $i++) { array_map('double', build()); }
echo "done\n";
"#,
        "done\n",
    );
}

/// A result consumed DIRECTLY by another call leaves no live block.
///
/// The mapped container never reaches a named local here, so the release cannot be attributed to
/// local-slot teardown; it has to come from the boxing site itself. Reference PHP 8.5.6 prints
/// `30`.
#[test]
fn a_map_result_passed_straight_into_another_call_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_nested",
        r#"<?php
function build(): array { return [1, 2, 3]; }
function double(int $n): int { return $n * 2; }
$sum = 0;
for ($i = 0; $i < 10; $i++) { $sum = $sum + count(array_map('double', build())); }
echo $sum, "\n";
"#,
        "30\n",
    );
}

// ---------------------------------------------------------------------------
// Controls — shapes that were NEVER broken
// ---------------------------------------------------------------------------

/// CONTROL: `array_column()` was already clean and must stay clean.
///
/// This is the most load-bearing control in the file. `array_column` shares
/// `box_array_result_for_mixed_builtin` with `array_map`, but its EIR result slot is the concrete
/// container type rather than `Heap(Mixed)`, so the boxing branch is never taken and the EIR's
/// own release already owns its container. It therefore pins the branch the fix must leave
/// alone: an "improvement" that released the container OUTSIDE the Mixed-slot guard would double
/// free exactly here. Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn array_column_was_never_broken_and_stays_clean() {
    assert_program_output_and_clean_heap(
        "map_heap_control_column",
        r#"<?php
function build(): array { return [["id" => 1], ["id" => 2]]; }
$sum = 0;
for ($i = 0; $i < 10; $i++) {
    $r = array_column(build(), "id");
    $sum = $sum + $r[0] + $r[1];
}
var_dump($r);
echo $sum, "\n";
"#,
        "array(2) {\n  [0]=>\n  int(1)\n  [1]=>\n  int(2)\n}\n30\n",
    );
}

// ---------------------------------------------------------------------------
// Mapped STRING VALUES — the uniform invoker's `Str` return boxing
// ---------------------------------------------------------------------------

/// The `var_dump` block reference PHP 8.5.6 prints for a two-entry hash mapped to strings.
const MAPPED_ASSOC_STRING_DUMP: &str =
    "array(2) {\n  [\"a\"]=>\n  string(1) \"1\"\n  [\"b\"]=>\n  string(1) \"2\"\n}\n";

/// HEADLINE: an associative source mapped to STRING values leaves no live block.
///
/// This is the shape that leaked 6 blocks per iteration for a two-entry hash. Four of those came
/// from the invoker's duplicate `Str`-return persist, which `emit_boxed_invoker_return` removes;
/// the destination hash's own string payloads were always freed correctly, which is why the
/// `int`-valued sibling above was clean throughout. `array_map()` preserves string keys, so the
/// result stays keyed `a`/`b`. Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn an_associative_map_to_string_values_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_assoc_str",
        r#"<?php
function build(): array { return ["a" => 1, "b" => 2]; }
$acc = "";
for ($i = 0; $i < 10; $i++) {
    $r = array_map(function (int $n): string { return strval($n); }, build());
    $acc = $acc . $r["a"] . $r["b"];
}
var_dump($r);
echo strlen($acc), "\n";
"#,
        &format!("{MAPPED_ASSOC_STRING_DUMP}20\n"),
    );
}

/// The same associative cell under a STRING callback name.
///
/// A string callback resolves through a runtime descriptor rather than a static binding, so it
/// reaches the invoker by a different route and is pinned separately. Reference PHP 8.5.6 prints
/// the block asserted below.
#[test]
fn an_associative_string_callback_map_to_strings_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_assoc_str_named",
        r#"<?php
function build(): array { return ["a" => 1, "b" => 2]; }
function tag(int $n): string { return strval($n); }
$acc = "";
for ($i = 0; $i < 10; $i++) {
    $r = array_map('tag', build());
    $acc = $acc . $r["a"] . $r["b"];
}
var_dump($r);
echo strlen($acc), "\n";
"#,
        &format!("{MAPPED_ASSOC_STRING_DUMP}20\n"),
    );
}

/// The same associative cell under an ARROW FUNCTION.
///
/// Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn an_associative_arrow_fn_map_to_strings_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_assoc_str_arrow",
        r#"<?php
function build(): array { return ["a" => 1, "b" => 2]; }
$acc = "";
for ($i = 0; $i < 10; $i++) {
    $r = array_map(fn (int $n): string => strval($n), build());
    $acc = $acc . $r["a"] . $r["b"];
}
var_dump($r);
echo strlen($acc), "\n";
"#,
        &format!("{MAPPED_ASSOC_STRING_DUMP}20\n"),
    );
}

/// The same associative cell under a FIRST-CLASS CALLABLE.
///
/// Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn an_associative_first_class_callable_map_to_strings_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_assoc_str_fcc",
        r#"<?php
function build(): array { return ["a" => 1, "b" => 2]; }
function tag(int $n): string { return strval($n); }
$acc = "";
for ($i = 0; $i < 10; $i++) {
    $r = array_map(tag(...), build());
    $acc = $acc . $r["a"] . $r["b"];
}
var_dump($r);
echo strlen($acc), "\n";
"#,
        &format!("{MAPPED_ASSOC_STRING_DUMP}20\n"),
    );
}

/// The same associative cell under a CALLABLE ARRAY.
///
/// Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn an_associative_callable_array_map_to_strings_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_assoc_str_calarr",
        r#"<?php
class Tagger { public static function tag(int $n): string { return strval($n); } }
function build(): array { return ["a" => 1, "b" => 2]; }
$acc = "";
for ($i = 0; $i < 10; $i++) {
    $r = array_map(['Tagger', 'tag'], build());
    $acc = $acc . $r["a"] . $r["b"];
}
var_dump($r);
echo strlen($acc), "\n";
"#,
        &format!("{MAPPED_ASSOC_STRING_DUMP}20\n"),
    );
}

/// A CAPTURING closure mapped to strings leaves no live block.
///
/// A capture makes the lowering pick `__rt_array_map_str_owned` / `HashMapResultKind::Owned` over
/// the `Persist` pair used above, so the mapped string reaches the destination by the other of the
/// two ownership contracts. Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn a_capturing_closure_map_to_strings_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_assoc_str_capture",
        r#"<?php
function build(): array { return ["a" => 1, "b" => 2]; }
$bump = 10;
$acc = "";
for ($i = 0; $i < 10; $i++) {
    $r = array_map(function (int $n) use ($bump): string { return strval($n + $bump); }, build());
    $acc = $acc . $r["a"] . $r["b"];
}
var_dump($r);
echo strlen($acc), "\n";
"#,
        "array(2) {\n  [\"a\"]=>\n  string(2) \"11\"\n  [\"b\"]=>\n  string(2) \"12\"\n}\n40\n",
    );
}

/// INTEGER keys take the same hash destination and must also come out clean.
///
/// The destination reuses the source keys verbatim, and `__rt_hash_free_deep` skips the key half
/// for inline integer keys — so an integer-keyed hash exercises a different half of the same
/// teardown. Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn an_integer_keyed_associative_map_to_strings_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_assoc_str_intkey",
        r#"<?php
function build(): array { return [5 => 1, 9 => 2]; }
$acc = "";
for ($i = 0; $i < 10; $i++) {
    $r = array_map(function (int $n): string { return strval($n); }, build());
    $acc = $acc . $r[5] . $r[9];
}
var_dump($r);
echo strlen($acc), "\n";
"#,
        "array(2) {\n  [5]=>\n  string(1) \"1\"\n  [9]=>\n  string(1) \"2\"\n}\n20\n",
    );
}

/// The INDEXED destination leaked the mapped strings too, and is pinned separately.
///
/// `__rt_array_map_str` builds a list rather than a hash, so it frees its payloads through
/// `__rt_array_free_deep` instead of `__rt_hash_free_deep` — the leak was upstream of both, in the
/// shared invoker, which is exactly why it showed up on both destination shapes.
/// Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn an_indexed_map_to_string_values_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_idx_str",
        r#"<?php
function build(): array { return [1, 2, 3]; }
$acc = "";
for ($i = 0; $i < 10; $i++) {
    $r = array_map(function (int $n): string { return strval($n); }, build());
    $acc = $acc . $r[0] . $r[1] . $r[2];
}
var_dump($r);
echo strlen($acc), "\n";
"#,
        "array(3) {\n  [0]=>\n  string(1) \"1\"\n  [1]=>\n  string(1) \"2\"\n  [2]=>\n  string(1) \"3\"\n}\n30\n",
    );
}

/// A DISCARDED string-valued map result leaves no live block.
///
/// Nothing ever reads the mapped strings here, so the release cannot come from any reader — it has
/// to come from the container teardown itself. Reference PHP 8.5.6 prints `done`.
#[test]
fn a_discarded_string_valued_map_result_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_assoc_str_discard",
        r#"<?php
function build(): array { return ["a" => 1, "b" => 2]; }
for ($i = 0; $i < 10; $i++) { array_map(function (int $n): string { return strval($n); }, build()); }
echo "done\n";
"#,
        "done\n",
    );
}

/// A string-valued map result consumed DIRECTLY by another call leaves no live block.
///
/// The mapped container never reaches a named local, so local-slot teardown cannot be what frees
/// the strings. Reference PHP 8.5.6 prints `20`.
#[test]
fn a_string_valued_map_result_passed_straight_into_another_call_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_assoc_str_nested",
        r#"<?php
function build(): array { return ["a" => 1, "b" => 2]; }
$n = 0;
for ($i = 0; $i < 10; $i++) { $n = $n + count(array_map(function (int $v): string { return strval($v); }, build())); }
echo $n, "\n";
"#,
        "20\n",
    );
}

/// USE AFTER FREE PROBE: the mapped strings are read back after every source array is gone.
///
/// This is the test that distinguishes the fix from a premature free. `build()` returns a fresh
/// array that dies at the end of each iteration, and the loop overwrites `$r` ten times, so by the
/// time the reads below run every source AND every earlier result has been torn down. If the
/// mapped strings were freed too early — the failure mode a `live_blocks=0` counter cannot see,
/// because macOS-aarch64 reports a double free as balanced — the surviving `$r` would dump garbage
/// or crash instead of printing its two strings. Reference PHP 8.5.6 prints the block asserted
/// below.
#[test]
fn mapped_strings_survive_after_every_source_array_is_gone() {
    assert_program_output_and_clean_heap(
        "map_heap_assoc_str_uaf",
        r#"<?php
function build(): array { return ["a" => 11, "b" => 22]; }
for ($i = 0; $i < 10; $i++) { $r = array_map(function (int $n): string { return strval($n); }, build()); }
var_dump($r);
echo $r["a"], $r["b"], "\n";
"#,
        "array(2) {\n  [\"a\"]=>\n  string(2) \"11\"\n  [\"b\"]=>\n  string(2) \"22\"\n}\n1122\n",
    );
}

/// The invoker fix is NOT `array_map`-specific, so a bare closure call pins it directly.
///
/// The duplicate `Str`-return persist lives in the uniform callable invoker, which any closure
/// invoked inside a loop goes through — `array_map` was only one victim. Without this test a
/// change that re-introduced the duplicate persist for non-`array_map` call sites would leave every
/// test above green. Reference PHP 8.5.6 prints `0123456789`.
#[test]
fn a_bare_closure_returning_a_string_leaves_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "map_heap_bare_closure_str",
        r#"<?php
$tag = function (int $n): string { return strval($n); };
$acc = "";
for ($i = 0; $i < 10; $i++) { $acc = $acc . $tag($i); }
echo $acc, "\n";
"#,
        "0123456789\n",
    );
}

/// CONTROL: `array_flip()` was already clean and must stay clean.
///
/// `array_flip`'s EIR slot is the container type, which is why the identical program shape stayed
/// clean while `array_map` leaked — it is the contrast that isolated the Mixed slot as the cause.
/// Reference PHP 8.5.6 prints the block asserted below.
#[test]
fn array_flip_was_never_broken_and_stays_clean() {
    assert_program_output_and_clean_heap(
        "map_heap_control_flip",
        r#"<?php
function build(): array { return ["a" => 1, "b" => 2]; }
$acc = "";
for ($i = 0; $i < 10; $i++) {
    $r = array_flip(build());
    $acc = $acc . $r[1] . $r[2];
}
var_dump($r);
echo strlen($acc), "\n";
"#,
        "array(2) {\n  [1]=>\n  string(1) \"a\"\n  [2]=>\n  string(1) \"b\"\n}\n20\n",
    );
}
