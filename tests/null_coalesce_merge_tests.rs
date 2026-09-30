//! Purpose:
//! End-to-end tests for two null-coalescing defects, both of which handed the caller a value
//! reference PHP never produces. They are pinned together because a single `$m[$k] ?? "MISS"`
//! tripped both at once.
//!
//! - BUG A — `??` on a `float`-element array took the VALUE branch on a MISS. A float slot had
//!   no miss marker at all: `emit_hash_get_miss` / `emit_array_get_null_fallback` materialized
//!   a plain `0.0`, and `emit_is_null_result` answered a constant "not null" for `Float`, so
//!   `(["a" => 1.5])["zz"] ?? "MISS"` silently evaluated to `0` instead of `"MISS"`. The fix
//!   marks a SILENT float miss with the in-band `NULL_SENTINEL` word reinterpreted as a quiet
//!   NaN and teaches `is_null`/`empty()` to recognize it.
//! - BUG B — `??` COLLAPSED the type of a hit. The checker merged the two arms with
//!   `wider_type_syntactic`, whose `Str` arm absorbs the other one (that is PHP's *coercion*
//!   order, not `??` semantics), so the whole expression typed as `string` and a `true` hit
//!   came back as `'1'`, an `int` hit as `'1'`, an `array` hit as `''`. The fix routes all
//!   three `??` inference sites through `null_coalesce_merge_type`, which answers `mixed` for
//!   mismatched arms — the same answer the IR-level `wider_type_for_merge` already gave.
//!
//! Called from:
//! - `cargo test --test null_coalesce_merge_tests` through Rust's test harness.
//!
//! Key details:
//! - Harness style mirrors `tests/null_value_codegen_tests.rs`: the elephc CLI
//!   (`CARGO_BIN_EXE_elephc`) is invoked as a subprocess in an isolated temp dir, compiled to a
//!   plain executable, run, and its stdout asserted. Host-target only.
//! - Every expected value was taken from reference PHP 8.5.6 (`php -d xdebug.mode=off`).
//! - The `??` default is deliberately a DIFFERENT PHP type from the array element in most
//!   tests. That is the shape BUG B needs, and it is exactly the shape
//!   `null_value_codegen_tests` avoided while the merge collapse was still unfixed.
//! - Negative controls pair answers that DIFFER between the hit and the miss, so a wrong
//!   value-branch fallthrough can never be mistaken for a correct default.
//! - The float miss marker is a quiet NaN whose payload differs from the `NAN` a PHP program
//!   can store, so `["a" => NAN]["a"] ?? "MISS"` must still report the stored `NAN` as a HIT.
//!   Reference PHP prints `float(NAN)` for that read.
//! - The marker is only emitted for the SILENT read variants (`??`, `isset()`, `empty()`).
//!   A warned read (`$m[$missing]` in plain value position) keeps materializing `0.0`, which
//!   `warned_float_miss_keeps_its_zero_value_shape` pins so the containment cannot drift.
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

/// Compiles `source`, runs it, and asserts its stdout equals `expected`, warnings included.
///
/// Used where the PHP program is expected to emit a runtime warning: `??` must SUPPRESS the
/// undefined-key warning while a plain read must still raise it.
///
/// ONE stream, INTERLEAVED, because that is what php does — `display_errors` is `STDOUT` by CLI
/// default, so a diagnostic lands in the program's own output where it happened rather than in a
/// separate pile after it. Stderr is asserted EMPTY for the same reason.
///
/// php's ` in <file> on line <n>` tail is folded out. It is real and elephc emits it, but the
/// path is a per-run temp directory, so a pin that kept it could not be written down. The
/// location has its own mechanism and its own tests; what these pins are about is the VALUE.
fn assert_program_output_with_warnings(prefix: &str, source: &str, expected: &str) {
    let dir = make_test_dir(prefix);
    let bin = compile(&dir, source, prefix);
    let output = Command::new(&bin)
        .output()
        .expect("failed to run compiled binary");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(stderr.is_empty(), "php writes its diagnostics to stdout: {stderr:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let located = regex_free_strip_location(&stdout);
    assert_eq!(located, expected);
    let _ = fs::remove_dir_all(&dir);
}

/// Removes php's ` in <file> on line <n>` tail from every line that carries one.
///
/// Hand-rolled rather than a regex: this crate's test binaries carry no regex dependency, and the
/// shape is fixed enough to match by hand — the LAST ` in ` on a line that also ends in
/// ` on line <digits>`.
fn regex_free_strip_location(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let stripped = line
            .rfind(" in ")
            .filter(|_| {
                line.rsplit_once(" on line ")
                    .is_some_and(|(_, n)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            })
            .map_or(line, |at| &line[..at]);
        out.push_str(stripped);
    }
    out
}

// ---------------------------------------------------------------------------
// BUG A — a float-element miss took the value branch
// ---------------------------------------------------------------------------

/// BUG A, headline shape: a missed read of a `float`-element array must take the `??` DEFAULT.
///
/// Reference PHP 8.5.6 (`php -d xdebug.mode=off`) prints `1.5 'MISS'`. Before the fix elephc
/// printed `'1.5' '0'`: the miss materialized a plain `0.0` that `is_null` could not tell from
/// a stored value, so `??` took the VALUE branch and the caller silently got `0`.
#[test]
fn float_element_array_miss_yields_the_coalesce_default() {
    assert_program_output(
        "coalesce_float_miss",
        r#"<?php
function fco(string $k) { $m = ["a" => 1.5]; return $m[$k] ?? "MISS"; }
echo var_export(fco("a"), true), " ", var_export(fco("zz"), true), "\n";
"#,
        "1.5 'MISS'\n",
    );
}

/// BUG A with a float default, so the hit and the miss are both floats.
///
/// This is the negative control that no type collapse can fake: both branches render as
/// floats, so the ONLY way to print `-1` on the miss is to have genuinely taken the default.
/// Reference PHP 8.5.6 prints `float(1.5)` then `float(-1)`.
#[test]
fn float_element_array_miss_with_a_float_default_takes_the_default_branch() {
    assert_program_output(
        "coalesce_float_float_default",
        r#"<?php
function fco(string $k) { $m = ["a" => 1.5]; return $m[$k] ?? -1.0; }
var_dump(fco("a"));
var_dump(fco("zz"));
"#,
        "float(1.5)\nfloat(-1)\n",
    );
}

/// BUG A on an INDEXED float array: the out-of-bounds fallback shares the defect and the fix.
///
/// `emit_array_get_null_fallback` (indexed) and `emit_hash_get_miss` (associative) are separate
/// emitters, so a fix applied to only one of them stays visible here.
/// Reference PHP 8.5.6 prints `float(2.5)` then `float(-1)`.
#[test]
fn indexed_float_array_out_of_bounds_yields_the_coalesce_default() {
    assert_program_output(
        "coalesce_float_indexed",
        r#"<?php
function fco(int $i) { $m = [1.5, 2.5]; return $m[$i] ?? -1.0; }
var_dump(fco(1));
var_dump(fco(7));
"#,
        "float(2.5)\nfloat(-1)\n",
    );
}

/// BUG A, `static`-local spelling: the miss marker must survive static storage.
///
/// Reference PHP 8.5.6 prints `float(1.5)` then `float(-1)`.
#[test]
fn float_element_static_local_array_miss_yields_the_coalesce_default() {
    assert_program_output(
        "coalesce_float_miss_static",
        r#"<?php
function look(string $k) {
    static $m = ["a" => 1.5];
    return $m[$k] ?? -1.0;
}
var_dump(look("a"));
var_dump(look("zz"));
"#,
        "float(1.5)\nfloat(-1)\n",
    );
}

/// BUG A negative control: a genuinely stored `NAN` is a HIT, never a miss.
///
/// The float miss marker is the `NULL_SENTINEL` word read as a double — a quiet NaN whose
/// payload differs from the canonical `NAN` (`0x7ff8_0000_0000_0000`) a PHP program can store,
/// and `is_null` compares raw BITS rather than using a float compare (which reports every NaN
/// as unordered). Reference PHP 8.5.6 prints `float(NAN)` then `string(4) "MISS"`.
#[test]
fn a_stored_nan_element_is_a_hit_and_not_a_miss() {
    assert_program_output(
        "coalesce_float_stored_nan",
        r#"<?php
function fnan(string $k) { $m = ["a" => NAN]; return $m[$k] ?? "MISS"; }
var_dump(fnan("a"));
var_dump(fnan("zz"));
"#,
        "float(NAN)\nstring(4) \"MISS\"\n",
    );
}

/// BUG A negative control: a stored `0.0` is a HIT, so `??` must return it, not the default.
///
/// `0.0` was the pre-fix miss value, so this pins that the fix did not simply relabel zero as
/// "missing". Reference PHP 8.5.6 prints `float(0)` then `float(-1)`.
#[test]
fn a_stored_zero_float_element_is_a_hit_and_not_a_miss() {
    assert_program_output(
        "coalesce_float_stored_zero",
        r#"<?php
function fzero(string $k) { $m = ["a" => 0.0]; return $m[$k] ?? -1.0; }
var_dump(fzero("a"));
var_dump(fzero("zz"));
"#,
        "float(0)\nfloat(-1)\n",
    );
}

/// BUG A companion: `empty()` on a missing float element is `true` (PHP: `empty(null)`).
///
/// `empty()` lowers through the same silent read as `??`, so the float miss marker reaches it
/// too. Testing the payload against `0.0` alone would answer `false` for the marker NaN. The
/// `bool` case is pinned alongside because it had the same hole for a different reason: a
/// missing `bool` element carries the in-band sentinel, a NON-ZERO integer, so `empty()`
/// answered `false` there before this change.
/// Reference PHP 8.5.6 prints `false false` then `true true`.
#[test]
fn empty_on_a_missing_float_or_bool_element_matches_php() {
    assert_program_output(
        "coalesce_empty_miss",
        r#"<?php
function probe(string $k) {
    $mf = ["a" => 1.5];
    $mb = ["a" => true];
    echo var_export(empty($mf[$k]), true), " ", var_export(empty($mb[$k]), true), "\n";
}
probe("a");
probe("zz");
"#,
        "false false\ntrue true\n",
    );
}

/// BUG A containment: a WARNED read keeps its historical `0.0` value shape.
///
/// The float null marker is only emitted for the silent read variants. A plain `$m[$missing]`
/// in value position still warns and still evaluates to `0.0`, so this change cannot make a
/// float read start rendering as `NAN` anywhere a program was not already asking about null.
/// (Reference PHP evaluates the read to `NULL`; matching that is a separate, untouched gap —
/// what is pinned here is that this fix did not move the needle in the WRONG direction.)
///
/// MEASURED on `php -n`: `\nWarning: Undefined array key "zz" in <file> on line 2\nNULL\n`. The
/// warning comes FIRST — `raw("zz")` is evaluated before `var_dump` prints — and shares stdout
/// with the dump. elephc now agrees on all of that; `float(0)` for php's `NULL` is the whole of
/// the remaining difference, which is what this pins.
#[test]
fn warned_float_miss_keeps_its_zero_value_shape() {
    assert_program_output_with_warnings(
        "coalesce_float_warned_read",
        r#"<?php
function raw(string $k) { $m = ["a" => 1.5]; return $m[$k]; }
var_dump(raw("zz"));
"#,
        "\nWarning: Undefined array key \"zz\"\nfloat(0)\n",
    );
}

// ---------------------------------------------------------------------------
// BUG B — the merge collapsed the hit's type
// ---------------------------------------------------------------------------

/// BUG B, headline shape: a `bool` hit against a `string` default stays a `bool`.
///
/// Reference PHP 8.5.6 prints `true 'MISS'`. Before the fix elephc printed `'1' 'MISS'`: the
/// miss was already right, but `wider_type_syntactic` let the `Str` default absorb the `Bool`
/// value arm, so the whole `??` typed as `string` and the hit was stringified.
#[test]
fn bool_hit_against_a_string_default_keeps_its_type() {
    assert_program_output(
        "coalesce_type_bool",
        r#"<?php
function coal(string $k) { $m = ["a" => true]; return $m[$k] ?? "MISS"; }
echo var_export(coal("a"), true), " ", var_export(coal("zz"), true), "\n";
"#,
        "true 'MISS'\n",
    );
}

/// BUG B across the element-type matrix — the defect was a CLASS, not one instance.
///
/// `int`, `bool`, `float` and `array` hits were all stringified (`'1'`, `'1'`, `'1.5'`, `''`).
/// `string` and `null` elements were already correct: a `string` element matches the default's
/// type so no merge happened, and a `null` element correctly takes the default branch.
/// Reference PHP 8.5.6 output is reproduced verbatim below.
#[test]
fn every_element_type_keeps_its_hit_type_through_the_merge() {
    assert_program_output(
        "coalesce_type_matrix",
        r#"<?php
function f_int(string $k) { $m = ["a" => 1]; return $m[$k] ?? "MISS"; }
function f_str(string $k) { $m = ["a" => "s"]; return $m[$k] ?? "MISS"; }
function f_bool(string $k) { $m = ["a" => true]; return $m[$k] ?? "MISS"; }
function f_float(string $k) { $m = ["a" => 1.5]; return $m[$k] ?? "MISS"; }
function f_null(string $k) { $m = ["a" => null]; return $m[$k] ?? "MISS"; }
function f_arr(string $k) { $m = ["a" => [1, 2]]; return $m[$k] ?? "MISS"; }
foreach (["int", "str", "bool", "float", "null", "arr"] as $t) {
    $fn = "f_$t";
    echo $t, ": ", var_export($fn("a"), true), " | ", var_export($fn("zz"), true), "\n";
}
"#,
        "int: 1 | 'MISS'\n\
str: 's' | 'MISS'\n\
bool: true | 'MISS'\n\
float: 1.5 | 'MISS'\n\
null: 'MISS' | 'MISS'\n\
arr: array (\n  0 => 1,\n  1 => 2,\n) | 'MISS'\n",
    );
}

/// BUG B on a HETEROGENEOUS array, where the element type is genuinely `mixed`.
///
/// `mixed ?? string` also collapsed to `string`, so both a `bool` and an `int` hit came back
/// stringified. Reference PHP 8.5.6 prints `bool(true)`, `int(3)`, `string(4) "MISS"`.
#[test]
fn a_mixed_element_hit_against_a_string_default_keeps_its_runtime_type() {
    assert_program_output(
        "coalesce_type_mixed_element",
        r#"<?php
function coal(string $k) { $m = ["a" => true, "b" => 3]; return $m[$k] ?? "MISS"; }
var_dump(coal("a"));
var_dump(coal("b"));
var_dump(coal("zz"));
"#,
        "bool(true)\nint(3)\nstring(4) \"MISS\"\n",
    );
}

/// BUG B where the merge feeds a LOCAL rather than a direct `return`.
///
/// The collapse lives in the checker's expression inference, so it reaches the local's type
/// just as it reached the function's inferred return type.
/// Reference PHP 8.5.6 prints `bool(true)` then `string(4) "MISS"`.
#[test]
fn the_merge_type_survives_assignment_to_a_local() {
    assert_program_output(
        "coalesce_type_via_local",
        r#"<?php
function coal(string $k) { $m = ["a" => true]; $r = $m[$k] ?? "MISS"; return $r; }
var_dump(coal("a"));
var_dump(coal("zz"));
"#,
        "bool(true)\nstring(4) \"MISS\"\n",
    );
}

/// BUG A + BUG B together in a CHAINED `?? ... ??`, the shape that needs both fixes.
///
/// The first arm must detect its float miss (BUG A) to reach the second arm at all, and each
/// surviving hit must keep its float type through two merges against a string (BUG B). Before
/// the fixes this printed `'1.5' '0' '0'`.
/// Reference PHP 8.5.6 prints `1.5 | 2.5 | 'MISS'`.
#[test]
fn a_chained_coalesce_walks_every_arm_and_keeps_each_type() {
    assert_program_output(
        "coalesce_chain",
        r#"<?php
function chain(string $k, string $j) {
    $m = ["a" => 1.5];
    $n = ["b" => 2.5];
    return $m[$k] ?? $n[$j] ?? "MISS";
}
echo var_export(chain("a", "zz"), true), " | ",
     var_export(chain("zz", "b"), true), " | ",
     var_export(chain("zz", "zz"), true), "\n";
"#,
        "1.5 | 2.5 | 'MISS'\n",
    );
}

/// The `??=` spelling of both fixes: a float miss must ASSIGN the default, a hit must not.
///
/// `??=` shares the merge helper with `??`, and its right-hand side is only evaluated when the
/// target reads as null — which for a float element is exactly the marker BUG A introduces.
/// Reference PHP 8.5.6 prints `float(1.5)` then `string(4) "MISS"`.
#[test]
fn null_coalescing_assignment_over_a_float_element_matches_php() {
    assert_program_output(
        "coalesce_assign_float",
        r#"<?php
function seed(string $k) {
    $m = ["a" => 1.5];
    $r = $m[$k] ?? "MISS";
    $r ??= "UNREACHED";
    return $r;
}
var_dump(seed("a"));
var_dump(seed("zz"));
"#,
        "float(1.5)\nstring(4) \"MISS\"\n",
    );
}

/// `??` on an array-typed arm keeps its ELEMENT type instead of collapsing to `mixed`.
///
/// The merge folds two containers element-wise, so the pervasive `$arr ?? []` idiom (an empty
/// literal is `array<never>`) still yields the left arm's element type and stays iterable.
/// Reference PHP 8.5.6 prints `3` then `0`.
#[test]
fn coalescing_with_an_empty_array_default_keeps_the_element_type() {
    assert_program_output(
        "coalesce_array_default",
        r#"<?php
function total(array $rows, string $k) {
    $picked = $rows[$k] ?? [];
    $sum = 0;
    foreach ($picked as $n) { $sum += $n; }
    return $sum;
}
echo total(["a" => [1, 2]], "a"), "\n";
echo total(["a" => [1, 2]], "zz"), "\n";
"#,
        "3\n0\n",
    );
}

/// `??` against a `null` default must NOT widen: the value arm's type is preserved verbatim.
///
/// `Void` is the identity of the merge, so `$m[$k] ?? null` keeps the value arm's concrete
/// type instead of becoming `mixed`, and a HIT renders exactly as the stored element.
/// Reference PHP 8.5.6 prints `float(1.5)`.
///
/// NOT pinned here, deliberately: the MISS half of `$m[$k] ?? null`. elephc returns the value
/// arm's zero (`float(0)`, `int(0)`, `""`) where reference PHP returns `NULL`, for EVERY
/// element type. That is a third, pre-existing defect — a hint-less function whose `??` merge
/// resolves to a concrete scalar has nowhere to put PHP null — and it is untouched by this
/// change. Asserting the miss here would pin the wrong answer.
#[test]
fn coalescing_with_a_null_default_preserves_the_value_arm_type() {
    assert_program_output(
        "coalesce_null_default",
        r#"<?php
function pick(string $k) { $m = ["a" => 1.5]; return $m[$k] ?? null; }
var_dump(pick("a"));
"#,
        "float(1.5)\n",
    );
}

/// `??` suppresses the undefined-key warning on BOTH the hit and the miss.
///
/// The silent read variant is what carries the float miss marker, so this pins that the fix
/// did not accidentally route a `??` element read back through the warning emitter.
#[test]
fn coalesce_over_a_missing_float_element_stays_warning_free() {
    assert_program_output_with_warnings(
        "coalesce_float_no_warning",
        r#"<?php
function fco(string $k) { $m = ["a" => 1.5]; return $m[$k] ?? -1.0; }
var_dump(fco("zz"));
"#,
        "float(-1)\n",
    );
}
