//! Purpose:
//! End-to-end tests for three codegen defects where a PHP value was silently CORRUPTED or
//! silently replaced by another type's zero value. All three were hit in the field, worked
//! around, and shipped undiagnosed, so each is pinned here in BOTH the broken spelling and the
//! workaround spelling that was used to dodge it.
//!
//! - BUG A — `$arr[$key] ?? $default` on an ABSENT key returned the *value* branch instead of
//!   the default when the array's element type is `bool`/`false`. The miss handed back the raw
//!   in-band null sentinel, so `(["a" => true])["zz"] ?? false` yielded `true`, and in integer
//!   context the sentinel leaked verbatim as `9223372036854775806`.
//! - BUG B — storing a heap string into a `static` local was a USE-AFTER-FREE. The store took
//!   no reference but the source temp was still released afterwards, so the static was left
//!   pointing at freed memory and rendered whatever the allocator handed out next.
//! - BUG C — a hint-less function returning a string on one path and `null` on another
//!   inferred `string`, so `return null` was lowered as a null-to-string coercion and the
//!   caller saw `""` instead of `NULL`.
//!
//! Called from:
//! - `cargo test --test null_value_codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp dir,
//!   compile a plain executable, run it, and assert stdout — the same harness style as
//!   `array_result_type_tests` / `function_exists_tests`. Host-target only (macOS aarch64 local).
//! - Every expected value in this file was taken from reference PHP 8.5.6
//!   (`php -d xdebug.mode=off`).
//! - REGRESSION ANCHOR (BUG A): the root cause was `emit_is_null_result` in
//!   `src/codegen/lower_inst/predicates.rs` answering a CONSTANT "not null" for `Bool` values
//!   whenever `--null-repr=tagged` is active (the default). That shortcut is only sound for
//!   `Int`, because `array_access_element_result_type` (`src/ir_lower/expr/mod.rs`) widens only
//!   *Int* element reads to the null-capable `TaggedScalar`. A `bool`/`false` element read
//!   still returns the in-band `NULL_SENTINEL` word on a miss, so `is_null` has to compare
//!   against it. A genuine bool is only ever 0 or 1, so that comparison is exact.
//! - REGRESSION ANCHOR (BUG B): `store_local` in `src/ir_lower/context.rs` gated its retain on
//!   `uses_global || previous_kind == LocalKind::PhpLocal`, excluding `LocalKind::StaticLocal`,
//!   while `release_source_after_store` had no such exclusion. The emitted EIR was
//!   `store_static_local v9` immediately followed by `release v9` with no `acquire` — a plain
//!   local emits `acquire` / `store_local` / `release`. The fix retains for static locals too
//!   and releases the PREVIOUS occupant so a reassigned static does not leak instead.
//! - BUG B is why `strpos()` false-positived on a mutated `static` haystack: `strpos` was the
//!   victim, not the culprit — the haystack itself was stale recycled memory. `strpos` is
//!   binary-safe and was never the problem, which `strpos_over_mutated_static_haystack_*`
//!   pins directly.
//! - REGRESSION ANCHOR (BUG C): `Checker::wider_type` in
//!   `src/types/checker/functions/returns.rs` resolved `Void` (elephc's spelling of PHP `null`)
//!   to the OTHER type. `Union([Str, Void])` was already the shape a declared `?string` hint
//!   and a ternary/match join produced, so the fold now agrees with both.
//! - BUG A is NOT specific to `static` locals — it was first seen through one, but a plain
//!   array PARAMETER reproduces it identically. Both spellings are pinned so a regression is
//!   visible whichever way the array reaches the read.
//! - The `?? false` / `?? true` pairs are deliberate: with a `bool`-element array, a default
//!   that happens to EQUAL the stored value cannot distinguish a correct miss from a wrong
//!   value-branch fallthrough. Each test pairs answers that DIFFER.
//! - These tests deliberately keep the `??` default the SAME PHP type as the array element.
//!   A cross-type default (`bool` element with a `string` default) additionally trips an
//!   UNFIXED merge-type collapse in which `wider_type_syntactic` lets `Str` absorb the other
//!   arm, so `(["a" => true])["a"] ?? "MISS"` still renders the hit as `string(1) "1"`. That is
//!   a separate defect and is intentionally NOT pinned here.
//! - Compile-failure assertions filter stderr through `elephc_diagnostics` because the system
//!   linker (GNU `ld` on Linux) emits warnings macOS does not.

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

/// Runs the compiler on `source` with extra flags and returns its raw output.
fn compile_raw(dir: &Path, source: &str, stem: &str, flags: &[&str]) -> std::process::Output {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.arg("build");
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.args(flags).arg(&php);
    cmd.output().expect("failed to spawn elephc")
}

/// Compiles `source` to a plain executable with extra compiler flags and returns its path.
fn compile_with_flags(dir: &Path, source: &str, stem: &str, flags: &[&str]) -> PathBuf {
    let output = compile_raw(dir, source, stem, flags);
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Compiles `source` to a plain executable with no extra flags.
fn compile(dir: &Path, source: &str, stem: &str) -> PathBuf {
    compile_with_flags(dir, source, stem, &[])
}

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather than
/// anything elephc emitted: GNU `ld` on Linux reports the static-`getaddrinfo` glibc notes and
/// the `.note.GNU-stack` deprecation, while Apple's linker stays silent. elephc's own lines
/// start with `error`/`warning`, so anchoring on those prefixes isolates its diagnostics — and
/// still surfaces an UNEXPECTED elephc diagnostic, which an allow-list would have hidden.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("error")
                || line.starts_with("Error")
                || line.starts_with("warning")
                || line.starts_with("Warning: ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Runs a compiled executable and returns its stdout, asserting a clean exit.
fn run_binary(bin: &Path) -> String {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
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
    let bin = compile(&dir, source, prefix);
    assert_eq!(run_binary(&bin), expected);
    let _ = fs::remove_dir_all(&dir);
}

/// Compiles `source` with `--heap-debug`, runs it, and asserts a clean leak summary.
///
/// `--gc-stats` is known to under-report, so allocation accounting is checked through
/// `--heap-debug`, whose trailer reports live blocks and a leak verdict.
fn assert_heap_clean(prefix: &str, source: &str) {
    let dir = make_test_dir(prefix);
    let bin = compile_with_flags(&dir, source, prefix, &["--heap-debug"]);
    let output = Command::new(&bin).output().expect("failed to run compiled binary");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("leak summary: clean"),
        "expected a clean heap-debug leak summary, got:\n{combined}"
    );
    assert!(
        combined.contains("live_blocks=0"),
        "expected zero live heap blocks at exit, got:\n{combined}"
    );
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// BUG A — null-coalescing miss on a bool-element array
// ---------------------------------------------------------------------------

/// BUG A: a missed read of a `bool`-element array must yield the `??` DEFAULT, not the value.
///
/// Reference PHP 8.5.6 prints `bool(true)` then `bool(false)`. Before the fix the miss printed
/// `bool(true)`, because `is_null` on a `Bool` slot was compiled to a constant "not null" and
/// the `??` took its value branch over the raw null sentinel.
#[test]
fn bool_element_array_miss_yields_the_coalesce_default() {
    assert_program_output(
        "coalesce_bool_miss",
        r#"<?php
function look(array $m, string $k) { return $m[$k] ?? false; }
var_dump(look(["a" => true], "a"));
var_dump(look(["a" => true], "zz"));
"#,
        "bool(true)\nbool(false)\n",
    );
}

/// BUG A, `static`-local spelling: the shape the defect was ORIGINALLY reported through.
///
/// The bug is not caused by the `static` storage class, but the campaign hit it this way, so
/// both spellings are pinned: a regression that only affects static locals stays visible here.
/// Reference PHP 8.5.6 prints `bool(true)` then `bool(false)`.
#[test]
fn bool_element_static_local_array_miss_yields_the_coalesce_default() {
    assert_program_output(
        "coalesce_bool_miss_static",
        r#"<?php
function look(string $k) {
    static $m = ["a" => true];
    return $m[$k] ?? false;
}
var_dump(look("a"));
var_dump(look("zz"));
"#,
        "bool(true)\nbool(false)\n",
    );
}

/// BUG A: the miss must not leak the in-band null sentinel as a numeric payload.
///
/// This is the loudest spelling of the same defect: before the fix the missed read carried
/// `0x7fff_ffff_ffff_fffe` (elephc's `NULL_SENTINEL`), so an integer cast printed
/// `9223372036854775806`. Reference PHP 8.5.6 prints `bool(true)` then `int(0)`.
#[test]
fn bool_element_array_miss_does_not_leak_the_null_sentinel() {
    assert_program_output(
        "coalesce_bool_sentinel",
        r#"<?php
function look(array $m, string $k) { return $m[$k] ?? false; }
$v = look(["a" => true], "zz");
var_dump($v === false);
var_dump((int)$v);
"#,
        "bool(true)\nint(0)\n",
    );
}

/// BUG A, `false`-element spelling: an array whose element type is the `false` SINGLETON.
///
/// `["a" => false]` infers element type `false`, not `bool`, and took a different `is_null`
/// arm — the catch-all, which also answers a constant "not null". A hit of `false` and a miss
/// defaulting to `true` are chosen so the two answers DIFFER.
/// Reference PHP 8.5.6 prints `bool(false)` then `bool(true)`.
#[test]
fn false_singleton_element_array_miss_yields_the_coalesce_default() {
    assert_program_output(
        "coalesce_false_singleton",
        r#"<?php
function look(array $m, string $k) { return $m[$k] ?? true; }
var_dump(look(["a" => false], "a"));
var_dump(look(["a" => false], "zz"));
"#,
        "bool(false)\nbool(true)\n",
    );
}

/// NEGATIVE CONTROL for BUG A: teaching `is_null` to recognise the sentinel for `Bool` must not
/// make a REAL stored `false` look absent.
///
/// A present `false` is a HIT — `??` only falls through on absent-or-null — so `?? true` must
/// return `false`, not the default. If the sentinel comparison had been widened into a
/// truthiness test, both rows would print `bool(true)` instead.
/// Reference PHP 8.5.6 prints `bool(false)` for both rows.
#[test]
fn present_false_is_a_hit_not_a_coalesce_miss() {
    assert_program_output(
        "coalesce_present_false",
        r#"<?php
function look(array $m, string $k) { return $m[$k] ?? true; }
var_dump(look(["a" => false], "a"));
var_dump(look(["a" => false, "b" => false], "b"));
"#,
        "bool(false)\nbool(false)\n",
    );
}

/// NEGATIVE CONTROL for BUG A: an ordinary bool local must still never be null.
///
/// `is_null($someBool)` is now a sentinel COMPARISON rather than a constant zero, so this pins
/// that widening the predicate did not make real bools nullable, and that `isset()` still
/// answers presence correctly on both a hit and a miss.
/// Reference PHP 8.5.6 prints `bool(false)`, `bool(false)`, `bool(true)`, `bool(false)`.
#[test]
fn ordinary_bool_locals_are_never_null() {
    assert_program_output(
        "coalesce_bool_locals",
        r#"<?php
$t = true;
$f = false;
var_dump(is_null($t));
var_dump(is_null($f));
var_dump(isset(["a" => true]["a"]));
var_dump(isset(["a" => true]["zz"]));
"#,
        "bool(false)\nbool(false)\nbool(true)\nbool(false)\n",
    );
}

// ---------------------------------------------------------------------------
// BUG B — static-local string use-after-free
// ---------------------------------------------------------------------------

/// BUG B: a user function's string result stored into a `static` local must stay valid.
///
/// The static kept the callee's buffer without taking a reference, then the caller released
/// it — so the next allocation recycled those bytes. `$junk` between the two calls is what
/// makes the corruption observable: before the fix the second `keep("")` printed
/// `string(11) "ZZZZZZZZZZZ"`, i.e. `$junk`'s payload.
/// Reference PHP 8.5.6 prints `string(11) "/tmp/ab.log"` twice.
#[test]
fn static_local_keeps_a_user_call_string_result_alive() {
    assert_program_output(
        "static_local_call_result",
        r#"<?php
function tag(string $x): string { return strtolower($x); }
function keep(string $x): string {
    static $last = "";
    if ($x !== "") { $last = tag($x); }
    return $last;
}
var_dump(keep("/tmp/AB.log"));
$junk = strtoupper("zzzzzzzzzzz");
var_dump(keep(""));
"#,
        "string(11) \"/tmp/ab.log\"\nstring(11) \"/tmp/ab.log\"\n",
    );
}

/// BUG B, workaround spelling: binding the call result to a plain local FIRST.
///
/// This is the shape the field campaign switched to in order to dodge the corruption. It was
/// correct before the fix and must stay correct after it, so a future regression is visible in
/// both spellings rather than silently hiding behind the workaround.
/// Reference PHP 8.5.6 prints `string(11) "/tmp/ab.log"` twice.
#[test]
fn static_local_call_result_bound_via_a_local_stays_correct() {
    assert_program_output(
        "static_local_call_result_workaround",
        r#"<?php
function tag(string $x): string { return strtolower($x); }
function keep(string $x): string {
    static $last = "";
    if ($x !== "") { $t = tag($x); $last = $t; }
    return $last;
}
var_dump(keep("/tmp/AB.log"));
$junk = strtoupper("zzzzzzzzzzz");
var_dump(keep(""));
"#,
        "string(11) \"/tmp/ab.log\"\nstring(11) \"/tmp/ab.log\"\n",
    );
}

/// BUG B: a `static` accumulator appended to across calls must not lose its history.
///
/// Appending through an intermediate heap local is the shape that corrupts: the static's own
/// buffer was freed on every return, so the HEAD of the string (the old content) came back as
/// recycled memory while the freshly appended TAIL stayed intact — before the fix this printed
/// `"<b<<b>"` and `"<c<c>><c>"`.
/// Reference PHP 8.5.6 prints `"<a>"`, `"<a><b>"`, `"<a><b><c>"`.
#[test]
fn static_local_string_accumulator_keeps_its_history() {
    assert_program_output(
        "static_local_accumulator",
        r#"<?php
function acc(string $k): void {
    static $hay = "";
    $q = "<" . $k . ">";
    $hay .= $q;
    var_dump($hay);
}
acc("a"); acc("b"); acc("c");
"#,
        "string(3) \"<a>\"\nstring(6) \"<a><b>\"\nstring(9) \"<a><b><c>\"\n",
    );
}

/// BUG B: the `strpos()` seen-set idiom over a mutated `static` haystack.
///
/// This is defect 2 as reported from the field — `strpos` false-positived, returning `0` where
/// PHP returns `false`. `strpos` is binary-safe and was never at fault: it was reading a
/// haystack whose bytes had already been freed and recycled. Before the fix every lookup after
/// the first answered `true`.
/// Reference PHP 8.5.6 prints `false, false, true, false, true`.
#[test]
fn strpos_over_mutated_static_haystack_matches_php() {
    assert_program_output(
        "static_local_strpos_seen_set",
        r#"<?php
function seen(string $k): bool {
    static $hay = "";
    $q = "\x01" . $k . "\x01";
    $r = strpos($hay, $q);
    $hay .= $q;
    return $r !== false;
}
foreach (["a", "b", "a", "c", "b"] as $k) { var_dump(seen($k)); }
"#,
        "bool(false)\nbool(false)\nbool(true)\nbool(false)\nbool(true)\n",
    );
}

/// NEGATIVE CONTROL for BUG B: `strpos` is binary-safe, independently of any static local.
///
/// The field report suspected `strpos` treated NUL-containing needles as C strings. It does
/// not — these all match reference PHP 8.5.6 exactly, which is what isolates the seen-set
/// false-positive above to the static-local corruption rather than to `strpos`.
/// Reference PHP 8.5.6 prints `int(3)`, `bool(false)`, `int(1)`, `bool(false)`.
#[test]
fn strpos_is_binary_safe_for_nul_containing_needles() {
    assert_program_output(
        "strpos_nul_safety",
        r#"<?php
$h1 = "abc\x00def";
$h2 = "abcdef";
$h3 = "a\x00b";
$h4 = "\x01/a\x01";
var_dump(strpos($h1, "\x00d"));
var_dump(strpos($h2, "c\x00zzz"));
var_dump(strpos($h3, "\x00b"));
var_dump(strpos($h4, "\x01/b\x01"));
"#,
        "int(3)\nbool(false)\nint(1)\nbool(false)\n",
    );
}

/// NEGATIVE CONTROL for BUG B: retaining static-local stores must not LEAK.
///
/// The fix adds a reference on every store into a static local, so it also has to release the
/// previous occupant — otherwise a reassigned static grows the heap by one buffer per call.
/// 200 reassignments of a growing accumulator must still end with zero live blocks.
#[test]
fn reassigned_static_local_string_does_not_leak() {
    assert_heap_clean(
        "static_local_no_leak",
        r#"<?php
function acc(string $k): string {
    static $h = "";
    $q = strtoupper($k);
    $h .= $q;
    return $h;
}
for ($i = 0; $i < 200; $i++) { acc("x"); }
echo strlen(acc("y")), "\n";
"#,
    );
}

// ---------------------------------------------------------------------------
// BUG C — hint-less union return dropping the null arm
// ---------------------------------------------------------------------------

/// BUG C: a hint-less function returning a string on one path and `null` on another.
///
/// Before the fix the inferred return type collapsed to `string`, so `return null` was lowered
/// as a null-to-string coercion and the null path printed `string(0) ""`.
/// Reference PHP 8.5.6 prints `string(3) "yes"` then `NULL`.
#[test]
fn hintless_string_or_null_return_keeps_the_null() {
    assert_program_output(
        "union_return_null",
        r#"<?php
function pick(int $x) {
    if ($x > 0) { return "yes"; }
    return null;
}
var_dump(pick(1));
var_dump(pick(0));
"#,
        "string(3) \"yes\"\nNULL\n",
    );
}

/// BUG C, workaround spelling: the SAME function with an explicit `?string` hint.
///
/// Writing the hint was the field workaround and was already correct; it is the model the
/// hint-less path now matches, so both spellings are pinned side by side.
/// Reference PHP 8.5.6 prints `string(3) "yes"` then `NULL`.
#[test]
fn hinted_nullable_string_return_keeps_the_null() {
    assert_program_output(
        "union_return_null_hinted",
        r#"<?php
function pick(int $x): ?string {
    if ($x > 0) { return "yes"; }
    return null;
}
var_dump(pick(1));
var_dump(pick(0));
"#,
        "string(3) \"yes\"\nNULL\n",
    );
}

/// BUG C: the recovered null must be a REAL null, not an empty string that merely prints as one.
///
/// `var_dump` alone could be satisfied by a nullable-looking rendering; the identity
/// comparisons pin that the value participates in `===` the way PHP's null does.
/// Reference PHP 8.5.6 prints `bool(true)` then `bool(false)`.
#[test]
fn hintless_union_return_null_is_identical_to_null() {
    assert_program_output(
        "union_return_null_identity",
        r#"<?php
function pick(int $x) {
    if ($x > 0) { return "yes"; }
    return null;
}
var_dump(pick(0) === null);
var_dump(pick(0) === "");
"#,
        "bool(true)\nbool(false)\n",
    );
}

/// NEGATIVE CONTROL for BUG C: a function whose every path returns a string stays a plain
/// string, and one whose every path returns null stays null.
///
/// Making `Void` widen into a nullable union must not make UNRELATED returns nullable: a
/// two-exit all-string function must not become `string|null`, and an all-null function must
/// not become `null|null`.
/// Reference PHP 8.5.6 prints `string(1) "a"`, `string(1) "b"`, `NULL`.
#[test]
fn returns_without_a_null_path_are_not_widened() {
    assert_program_output(
        "union_return_negative",
        r#"<?php
function two(int $x) {
    if ($x > 0) { return "a"; }
    return "b";
}
function allNull(int $x) {
    if ($x > 0) { return null; }
    return null;
}
var_dump(two(1));
var_dump(two(0));
var_dump(allNull(1));
"#,
        "string(1) \"a\"\nstring(1) \"b\"\nNULL\n",
    );
}

/// NEGATIVE CONTROL: the checker must still REJECT a statically wrong program.
///
/// Widening a runtime null predicate and a return-type fold must not soften the checker:
/// passing an array where a `string` is declared is still a compile error.
#[test]
fn checker_still_rejects_mistyped_argument() {
    let dir = make_test_dir("null_value_negative_checker");
    let output = compile_raw(
        &dir,
        r#"<?php
function want(string $s): string { return $s; }
echo want(["a" => true]);
"#,
        "null_value_negative_checker",
        &[],
    );
    let raw = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "elephc compile unexpectedly SUCCEEDED — the checker over-accepted:\n{raw}"
    );
    let diagnostics = elephc_diagnostics(&raw);
    assert!(
        diagnostics.contains("want"),
        "expected a checker diagnostic naming the callee, got:\n{diagnostics}"
    );
    let _ = fs::remove_dir_all(&dir);
}
