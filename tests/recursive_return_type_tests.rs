//! Purpose:
//! End-to-end tests for the recursive-call return-type defect: a call to a free function from
//! inside its own body was typed `Int` regardless of the function's declared return type, so
//!
//! ```php
//! function r(string $x): string {
//!     if (strlen($x) > 2) { return $x; }
//!     return r($x . "a");
//! }
//! ```
//!
//! failed to compile with `Function 'r' return type expects Str, got Int`.
//!
//! ROOT CAUSE — `src/types/checker/functions/resolution/signature.rs`. Before walking a free
//! function's body, `resolve_function_signature` publishes a *provisional* `FunctionSig` into
//! `self.functions` so that a self-call has something to resolve against. That provisional
//! signature hard-coded `return_type: PhpType::Int`, even though the declared hint was already
//! available on the `FnDecl`. The body walk then typed `r(...)` from that placeholder, and
//! `collect_return_infos` compared the resulting `Int` against the real `Str`. The fix seeds the
//! provisional return type from the declared hint (`provisional_return_type`).
//!
//! Methods were never affected because the class-schema pass already seeds a method's declared
//! return type up front (`src/types/checker/schema/validation.rs`); only free functions had the
//! placeholder. That asymmetry is what made the bug look type-specific.
//!
//! WHY IT LOOKED LIKE "NON-`int` RETURN TYPES": the placeholder was ALWAYS `Int`, for every
//! return type. It only became a diagnostic when `Int` was not acceptable for the declared type.
//! A recursive `int`, `float`, `bool` or `string|int` function accidentally agreed with the
//! placeholder and compiled, which is why the defect survived so long. Those cells are pinned
//! here too, as no-regression coverage rather than as fixes.
//!
//! Called from:
//! - `cargo test --test recursive_return_type_tests` through Rust's test harness.
//!
//! Key details:
//! - Harness style mirrors `tests/null_coalesce_merge_tests.rs`: the elephc CLI
//!   (`CARGO_BIN_EXE_elephc`) is invoked as a subprocess in an isolated temp dir, compiled to a
//!   plain executable, run, and its stdout asserted. Host-target only.
//! - Every expected value was taken from reference PHP 8.5.6 (`php -d xdebug.mode=off`).
//! - Compile-failure assertions filter stderr through `elephc_diagnostics` because the system
//!   linker (GNU `ld` on Linux) emits warnings macOS does not.
//! - Every recursive program here has a REACHABLE base case. Unbounded recursion currently
//!   exhausts the native stack and dies on `SIGSEGV`, so a missing base case would abort the
//!   test process rather than fail an assertion.
//! - The bogus `Int` was not confined to the return check: it was the type of the self-call
//!   EXPRESSION, so it also broke argument checking and leaked out to callers through the
//!   function's recorded return type. `recursive_result_feeds_typed_parameter` and
//!   `class_type_recursion_is_usable_by_an_outside_caller` pin those two escape routes.

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

/// Runs the compiler on `source` and returns its raw output.
fn compile_raw(dir: &Path, source: &str, stem: &str) -> std::process::Output {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(&php);
    cmd.output().expect("failed to spawn elephc")
}

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather than
/// anything elephc emitted: GNU `ld` on Linux reports the static-`getaddrinfo` glibc notes and
/// the `.note.GNU-stack` deprecation, while Apple's linker stays silent. elephc's own lines
/// start with `error`/`warning`, so anchoring on those prefixes isolates its diagnostics.
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

/// Compiles `source` to a plain executable, asserting elephc reported no diagnostic.
fn compile(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let output = compile_raw(dir, source, stem);
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
    let bin = compile(&dir, source, prefix);
    assert_eq!(run_binary(&bin), expected);
    let _ = fs::remove_dir_all(&dir);
}

/// Compiles `source` expecting FAILURE, asserting elephc's own diagnostics contain `needle`.
///
/// Only elephc's lines are inspected, so a host linker warning can never satisfy — or spoil —
/// the assertion. The compile must also actually fail: a program that compiles clean is a
/// regression even if the expected text never appears.
fn assert_compile_error(prefix: &str, source: &str, needle: &str) {
    let dir = make_test_dir(prefix);
    let output = compile_raw(&dir, source, prefix);
    let diagnostics = elephc_diagnostics(&String::from_utf8_lossy(&output.stderr));
    assert!(
        !output.status.success(),
        "expected a compile error but elephc succeeded; diagnostics were:\n{diagnostics}"
    );
    assert!(
        diagnostics.contains(needle),
        "expected a diagnostic containing {needle:?}, got:\n{diagnostics}"
    );
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// The reported failures — declared return types the `Int` placeholder rejected
// ---------------------------------------------------------------------------

/// Headline shape from the bug report: a `: string` function that calls itself.
///
/// Reference PHP 8.5.6 prints `zaa`. Before the fix elephc refused to compile this with
/// `error[2:1]: Function 'r' return type expects Str, got Int`.
#[test]
fn string_recursion_returns_the_declared_type() {
    assert_program_output(
        "recret_string",
        r#"<?php
function r(string $x): string {
    if (strlen($x) > 2) { return $x; }
    return r($x . "a");
}
echo r("z"), "\n";
"#,
        "zaa\n",
    );
}

/// A `: array` function that calls itself.
///
/// Reference PHP prints `3|4`. Before the fix: `return type expects Array(Mixed), got Int`.
#[test]
fn array_recursion_returns_the_declared_type() {
    assert_program_output(
        "recret_array",
        r#"<?php
function r(int $x): array {
    if ($x > 2) { return [$x, $x + 1]; }
    return r($x + 1);
}
$a = r(0);
echo $a[0], "|", $a[1], "\n";
"#,
        "3|4\n",
    );
}

/// A nullable `: ?string` function that calls itself.
///
/// The declared type resolves to `Union([Str, Void])`, which the `Int` placeholder did not
/// satisfy. Reference PHP prints `string(4) "done"`. Before the fix:
/// `return type expects Union([Str, Void]), got Int`.
#[test]
fn nullable_string_recursion_returns_the_declared_type() {
    assert_program_output(
        "recret_nullable",
        r#"<?php
function r(int $x): ?string {
    if ($x > 2) { return "done"; }
    return r($x + 1);
}
var_dump(r(0));
"#,
        "string(4) \"done\"\n",
    );
}

/// A function returning a CLASS type that calls itself, consumed by an outside caller.
///
/// This pins the second escape route: the placeholder did not merely upset the return check,
/// it became the function's RECORDED return type, so `r(0)->v` at top level also failed with
/// `Property access requires an object or typed pointer` — a caller-side error for a callee-side
/// bug. Reference PHP prints `3`.
#[test]
fn class_type_recursion_is_usable_by_an_outside_caller() {
    assert_program_output(
        "recret_class",
        r#"<?php
class Box { public int $v = 0; }
function r(int $x): Box {
    if ($x > 2) { $b = new Box(); $b->v = $x; return $b; }
    return r($x + 1);
}
$o = r(0);
echo $o->v, "\n";
"#,
        "3\n",
    );
}

/// A recursive function's result passed to a DIFFERENT function's typed parameter.
///
/// The self-call's bogus type was the type of the expression, not just of the `return`, so this
/// failed in argument position with `Function 'take' parameter $s expects Str, got Int` — a
/// diagnostic pointing at an entirely innocent callee. Reference PHP prints `[zaa]`.
#[test]
fn recursive_result_feeds_typed_parameter() {
    assert_program_output(
        "recret_param",
        r#"<?php
function take(string $s): string { return "[" . $s . "]"; }
function r(string $x): string {
    if (strlen($x) > 2) { return $x; }
    return take(r($x . "a"));
}
echo r("z"), "\n";
"#,
        "[[zaa]]\n",
    );
}

/// A self-call in plain value position (assigned to a local) rather than directly returned.
///
/// Reference PHP prints `zaa`. Before the fix this failed the same way as the direct form,
/// confirming the defect was in expression typing rather than in the `return` statement.
#[test]
fn self_call_outside_return_position() {
    assert_program_output(
        "recret_local",
        r#"<?php
function r(string $x): string {
    if (strlen($x) > 2) { return $x; }
    $y = r($x . "a");
    return $y;
}
echo r("z"), "\n";
"#,
        "zaa\n",
    );
}

// ---------------------------------------------------------------------------
// Mutual recursion — a cycle re-entering a function already on the stack
// ---------------------------------------------------------------------------

/// `a()` calls `b()` calls `a()`, with `a` declared first.
///
/// The cycle re-enters a function whose provisional signature is still published, so the second
/// leg saw the placeholder. Reference PHP prints `zaba`. Before the fix:
/// `error[3:1]: Function 'b' return type expects Str, got Int`.
#[test]
fn mutual_recursion_first_declared_calls_second() {
    assert_program_output(
        "recret_mutual_ab",
        r#"<?php
function a(string $x): string { if (strlen($x) > 3) { return $x; } return b($x . "a"); }
function b(string $x): string { if (strlen($x) > 3) { return $x; } return a($x . "b"); }
echo a("z"), "\n";
"#,
        "zaba\n",
    );
}

/// The same cycle with the DECLARATION ORDER reversed.
///
/// Pinned separately because resolution order — not source order — decides which leg of the
/// cycle sees the provisional signature, and the pre-fix diagnostic moved with it. Reference
/// PHP prints `zaba`.
#[test]
fn mutual_recursion_second_declared_calls_first() {
    assert_program_output(
        "recret_mutual_ba",
        r#"<?php
function b2(string $x): string { if (strlen($x) > 3) { return $x; } return a2($x . "b"); }
function a2(string $x): string { if (strlen($x) > 3) { return $x; } return b2($x . "a"); }
echo a2("z"), "\n";
"#,
        "zaba\n",
    );
}

// ---------------------------------------------------------------------------
// Cells that already worked — pinned so the fix cannot regress them
// ---------------------------------------------------------------------------

/// Recursive `: int`, `: float`, `: bool` and `: string|int` — the cells that already compiled.
///
/// These are exactly the return types the `Int` placeholder happened to satisfy, which is why
/// the defect was mis-reported as affecting only non-`int` functions. Reference PHP prints
/// `3`, `3`, `bool(true)`, `string(1) "s"`.
#[test]
fn return_types_that_tolerated_the_placeholder_still_work() {
    assert_program_output(
        "recret_tolerant",
        r#"<?php
function ri(int $x): int { if ($x > 2) { return $x; } return ri($x + 1); }
function rf(float $x): float { if ($x > 2.0) { return $x; } return rf($x + 1.0); }
function rb(int $x): bool { if ($x > 2) { return true; } return rb($x + 1); }
function ru(int $x): string|int { if ($x > 2) { return "s"; } return ru($x + 1); }
echo ri(0), "\n";
echo rf(0.0), "\n";
var_dump(rb(0));
var_dump(ru(0));
"#,
        "3\n3\nbool(true)\nstring(1) \"s\"\n",
    );
}

/// Instance and static METHODS doing the same self-call, plus a by-ref closure.
///
/// Methods take their declared return type from the class-schema pass, and a closure resolves
/// through its own signature machinery, so none of these ever saw the free-function placeholder.
/// Pinned to keep the asymmetry documented and to prove the fix did not disturb them. Reference
/// PHP prints `zaa` three times.
#[test]
fn methods_and_closures_are_unaffected() {
    assert_program_output(
        "recret_methods",
        r#"<?php
class C {
    public function inst(string $x): string { if (strlen($x) > 2) { return $x; } return $this->inst($x . "a"); }
    public static function stat(string $x): string { if (strlen($x) > 2) { return $x; } return self::stat($x . "a"); }
}
$c = new C();
echo $c->inst("z"), "\n";
echo C::stat("z"), "\n";
$f = function (string $x) use (&$f): string { if (strlen($x) > 2) { return $x; } return $f($x . "a"); };
echo $f("z"), "\n";
"#,
        "zaa\nzaa\nzaa\n",
    );
}

/// A plain forward reference with no recursion at all.
///
/// `a()` calls `b()` before `b` is declared. This always worked — the declaration pre-pass
/// registers every `FnDecl` up front — and it is pinned to keep the fix from being mistaken for
/// a change to forward resolution. Reference PHP prints `z!`.
#[test]
fn forward_reference_without_recursion_still_resolves() {
    assert_program_output(
        "recret_forward",
        r#"<?php
function fwd_a(string $x): string { return fwd_b($x); }
function fwd_b(string $x): string { return $x . "!"; }
echo fwd_a("z"), "\n";
"#,
        "z!\n",
    );
}

// ---------------------------------------------------------------------------
// Unhinted recursive functions
// ---------------------------------------------------------------------------

/// A recursive function with NO return hint infers its type from the base case.
///
/// elephc does NOT require a hint here, and the placeholder is not observable: the unhinted
/// return type is the `wider_type` merge of every `return`, and the base case's real type
/// absorbs the recursive call's placeholder. The four probes below are the ones most likely to
/// expose a leak, because a surviving `Int` would change what `var_dump` prints — `int(1)`
/// instead of `bool(true)`, `int(1)` instead of `float(1.5)`, `int(0)` instead of `NULL`.
///
/// Reference PHP prints `ZAA|3`, `bool(true)`, `float(1.5)`, `NULL`.
#[test]
fn unhinted_recursion_infers_from_the_base_case() {
    assert_program_output(
        "recret_unhinted",
        r#"<?php
function us(string $x) { if (strlen($x) > 2) { return $x; } return us($x . "a"); }
function ub(int $x) { if ($x > 2) { return true; } return ub($x + 1); }
function uf(int $x) { if ($x > 2) { return 1.5; } return uf($x + 1); }
function un(int $x) { if ($x > 2) { return null; } return un($x + 1); }
$s = us("z");
echo strtoupper($s), "|", strlen($s), "\n";
var_dump(ub(0));
var_dump(uf(0));
var_dump(un(0));
"#,
        "ZAA|3\nbool(true)\nfloat(1.5)\nNULL\n",
    );
}

/// An unhinted recursive function returning an ARRAY, and one returning an OBJECT.
///
/// The two shapes whose declared-hint counterparts were hard compile errors, confirming the
/// unhinted path reaches them through inference alone. Reference PHP prints `2|3` and `7`.
#[test]
fn unhinted_recursion_infers_array_and_object_base_cases() {
    assert_program_output(
        "recret_unhinted_agg",
        r#"<?php
class Bx { public int $v = 7; }
function ua(int $x) { if ($x > 2) { return [$x, $x + 1]; } return ua($x + 1); }
function uo(int $x) { if ($x > 2) { return new Bx(); } return uo($x + 1); }
$a = ua(0);
echo count($a), "|", $a[0], "\n";
echo uo(0)->v, "\n";
"#,
        "2|3\n7\n",
    );
}

// ---------------------------------------------------------------------------
// Recursive generators — `yield` overrides the declared hint
// ---------------------------------------------------------------------------

/// A recursive generator, both with and without a `: Generator` hint.
///
/// A body containing `yield` produces a `Generator` whatever the annotation says, so the
/// provisional signature has to agree with that rule rather than with the raw hint — otherwise
/// an unhinted recursive generator would seed the `Int` placeholder and a hinted one would seed
/// a hint that the real pass overrides. Reference PHP prints `3,2,1,0,` for both.
#[test]
fn recursive_generators_resolve_to_generator() {
    assert_program_output(
        "recret_generator",
        r#"<?php
function hinted(int $n): Generator {
    yield $n;
    if ($n > 0) { foreach (hinted($n - 1) as $v) { yield $v; } }
}
function unhinted(int $n) {
    yield $n;
    if ($n > 0) { foreach (unhinted($n - 1) as $v) { yield $v; } }
}
foreach (hinted(3) as $v) { echo $v, ","; }
echo "\n";
foreach (unhinted(3) as $v) { echo $v, ","; }
echo "\n";
"#,
        "3,2,1,0,\n3,2,1,0,\n",
    );
}

// ---------------------------------------------------------------------------
// Negative controls — genuine mismatches must STILL be rejected
// ---------------------------------------------------------------------------

/// A plain, NON-recursive `: string` function returning an int must still fail.
///
/// The baseline control: the fix must not have loosened return-type checking in general.
#[test]
fn non_recursive_return_type_mismatch_is_still_rejected() {
    assert_compile_error(
        "recret_neg_plain",
        r#"<?php
function f(int $x): string { return $x; }
echo f(1), "\n";
"#,
        "Function 'f' return type expects Str, got Int",
    );
}

/// A RECURSIVE `: string` function whose BASE CASE returns an int must still fail.
///
/// The sharpest control for this fix. The pre-fix compiler produced exactly this message for the
/// *correct* program in `string_recursion_returns_the_declared_type`; here the same message must
/// still appear, and now it means what it says. Trusting the declared hint for the self-call must
/// not turn into trusting it for the real returns.
#[test]
fn recursive_function_with_wrong_base_case_type_is_still_rejected() {
    assert_compile_error(
        "recret_neg_base",
        r#"<?php
function r(int $x): string {
    if ($x > 2) { return $x; }
    return r($x + 1);
}
echo r(0), "\n";
"#,
        "Function 'r' return type expects Str, got Int",
    );
}

/// A recursive `: array` function whose base case returns a string must still fail.
#[test]
fn recursive_array_function_with_string_base_case_is_still_rejected() {
    assert_compile_error(
        "recret_neg_array",
        r#"<?php
function r(int $x): array {
    if ($x > 2) { return "nope"; }
    return r($x + 1);
}
print_r(r(0));
"#,
        "Function 'r' return type expects Union([Array(Mixed), AssocArray { key: Mixed, value: Mixed }]), got Str",
    );
}

/// A recursive `: int` function whose base case returns a string must still fail.
///
/// The mirror image of the headline bug: `int` is the type the placeholder used to impersonate,
/// so this proves the seeded value is the DECLARED type rather than a blanket "accept anything".
#[test]
fn recursive_int_function_with_string_base_case_is_still_rejected() {
    assert_compile_error(
        "recret_neg_int",
        r#"<?php
function r(int $x): int {
    if ($x > 2) { return "nope"; }
    return r($x + 1);
}
echo r(0), "\n";
"#,
        "Function 'r' return type expects Int, got Str",
    );
}

/// A mutual-recursion cycle with a bad return in ONE leg must still fail.
///
/// Both legs are declared `: string`; only `b` returns an int. Seeding the cycle with declared
/// types must not let a genuine mismatch hide behind the other leg.
#[test]
fn mutual_recursion_with_a_bad_leg_is_still_rejected() {
    assert_compile_error(
        "recret_neg_mutual",
        r#"<?php
function ma(int $x): string { if ($x > 3) { return "a"; } return mb($x + 1); }
function mb(int $x): string { if ($x > 3) { return 42; } return ma($x + 1); }
echo ma(0), "\n";
"#,
        "Function 'mb' return type expects Str, got Int",
    );
}
