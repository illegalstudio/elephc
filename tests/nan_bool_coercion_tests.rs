//! Purpose:
//! End-to-end tests for PHP 8.5's `unexpected NAN value was coerced to bool` E_WARNING — the
//! `__rt_warn_nan_coerced_bool` runtime helper and the inline NAN probe that guards it
//! (`src/codegen_support/runtime/arrays/nan_bool_coercion_warning.rs`), reached from the three
//! float-truthiness lowering sites in `src/codegen/lower_inst` and from the two boxed-Mixed
//! runtime helpers (`__rt_mixed_cast_bool`, `__rt_mixed_is_empty`). Also covers its sibling,
//! `unexpected NAN value was coerced to string`, raised by `__rt_ftoa_coerce` on every float
//! string coercion.
//!
//! The diagnostic is NEW IN PHP 8.5 (RFC `warnings-php-8-5`, "Coercing NAN to other types");
//! 8.2/8.3/8.4 coerce NAN to `true` silently. Before the fix elephc coerced silently on every
//! profile.
//!
//! Called from:
//! - `cargo test --test nan_bool_coercion_tests` through Rust's test harness.
//!
//! Key details:
//! - Harness style mirrors `tests/null_coalesce_merge_tests.rs`: the elephc CLI
//!   (`CARGO_BIN_EXE_elephc`) is invoked as a subprocess in an isolated temp dir, compiled to a
//!   plain executable, run, and its output asserted. Host-target only.
//! - Every expected line was captured from reference PHP 8.5.6 (`php -d xdebug.mode=off`); the
//!   host `php` loads Xdebug, which overloads `var_dump`, so that flag is mandatory when
//!   re-deriving these fixtures.
//! - EVERY NAN reaches its coercion site through a FUNCTION RETURN, a PARAMETER or an array
//!   element, never as a literal. `NAN` in literal position is const-folded, so a literal probe
//!   would measure the constant folder and pass even with the runtime path deleted.
//! - Stdout and stderr are asserted SEPARATELY. The warning is a runtime diagnostic written by
//!   the COMPILED PROGRAM to its own stderr, which is a different stream from the compiler's —
//!   the `elephc_diagnostics` filter used for compile output can therefore never swallow it.
//! - WARNING COUNTS are part of the fixture. php-src warns once per coercion, so a `while`
//!   condition that runs three times warns three times; asserting the whole stderr text (not
//!   just "contains") is what pins that.
//! - NEGATIVE CONTROLS matter more than the positives here. php-src warns for NAN and for
//!   NOTHING ELSE: `INF`, `-INF`, `0.0` and `-0.0` all coerce silently. A fix that warned on
//!   every float-to-bool coercion would be worse than the original silence, and only these
//!   controls can tell the two apart.
//! - elephc does NOT synthesize php-src's ` in <file> on line <n>` message tail. That is the
//!   house convention for every runtime diagnostic — `foreach_non_iterable_warning.rs` and the
//!   `ARRAY_FLIP_SKIPPED_MESSAGES` docblock in `hash_flip.rs` both say so explicitly — so these
//!   tests assert the bare `Warning: <text>` line elephc actually emits.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The exact stderr line elephc emits for one NAN-to-bool coercion.
///
/// php-src appends ` in <file> on line <n>`; elephc deliberately does not (see the module
/// preamble). Keeping the text in one constant makes the message and the counts below impossible
/// to drift apart.
const NAN_WARNING: &str = "Warning: unexpected NAN value was coerced to bool\n";

/// The exact stderr line elephc emits for one NAN-to-string coercion.
const NAN_STRING_WARNING: &str = "Warning: unexpected NAN value was coerced to string\n";

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

/// Keeps only elephc's own diagnostics from a COMPILE's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather than
/// anything elephc emitted: GNU `ld` on Linux reports the static-`getaddrinfo` glibc notes and
/// the `.note.GNU-stack` deprecation, while Apple's linker stays silent. This filter runs on the
/// COMPILER's stderr only; the runtime NAN warning this file asserts on comes from the compiled
/// program's own stderr and never passes through here.
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

/// Compiles `source` under `php_version` (when given) and returns the executable path.
fn compile(dir: &Path, source: &str, stem: &str, php_version: Option<&str>) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    if let Some(version) = php_version {
        cmd.args(["--php-version", version]);
    }
    cmd.arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
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

/// Compiles and runs `source`, asserting its stdout and its stderr independently.
///
/// Stdout carries the PHP program's own output and stderr carries every runtime warning, so an
/// assertion on one can never be satisfied by the other. `expected_stderr` is the COMPLETE
/// stderr, which is what pins the warning COUNT as well as its text.
fn assert_run(prefix: &str, source: &str, expected_stdout: &str, expected_stderr: &str) {
    assert_run_for_version(prefix, source, None, expected_stdout, expected_stderr);
}

/// `assert_run` with an explicit `--php-version` profile.
fn assert_run_for_version(
    prefix: &str,
    source: &str,
    php_version: Option<&str>,
    expected_stdout: &str,
    expected_stderr: &str,
) {
    let dir = make_test_dir(prefix);
    let bin = compile(&dir, source, prefix, php_version);
    let output = Command::new(&bin)
        .output()
        .expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        expected_stdout,
        "program stdout diverged"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        expected_stderr,
        "program stderr diverged"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Returns `NAN_WARNING` repeated `count` times, i.e. the whole stderr of a program that
/// coerced a NAN to bool exactly `count` times.
fn warnings(count: usize) -> String {
    NAN_WARNING.repeat(count)
}

/// Returns `NAN_STRING_WARNING` repeated `count` times, i.e. the whole stderr of a program that
/// coerced a NAN to string exactly `count` times.
fn string_warnings(count: usize) -> String {
    NAN_STRING_WARNING.repeat(count)
}

// ---------------------------------------------------------------------------
// The coercion sites — every syntactic form that turns a float into a bool
// ---------------------------------------------------------------------------

/// The six single-expression coercion sites, all against the same NAN-returning function.
///
/// `empty()`, `(bool)`, `if`, `!`, the ternary condition and `boolval()` are separate lowering
/// paths, and php-src warns at each of them. Reference PHP 8.5.6 prints one warning per line
/// below and the stdout asserted here.
#[test]
fn every_truthiness_site_warns_once_for_a_nan() {
    assert_run(
        "nan_sites",
        r#"<?php
function f(): float { return NAN; }
var_dump(empty(f()));
var_dump((bool)f());
if (f()) { echo "if-true\n"; }
var_dump(!f());
echo f() ? "tern-true\n" : "tern-false\n";
var_dump(boolval(f()));
"#,
        "bool(false)\nbool(true)\nif-true\nbool(false)\ntern-true\nbool(true)\n",
        &warnings(6),
    );
}

/// `while` and the short-circuit operators warn once per EVALUATION, not once per site.
///
/// The loop condition runs three times (`$n` = 0, 1, 2), so php-src warns three times before the
/// loop exits — a per-site "warn once" implementation would print one. The three `&&`/`||`
/// expressions that follow warn once each. Reference PHP 8.5.6 prints six warnings in total and
/// the stdout asserted here.
#[test]
fn loops_and_short_circuit_operators_warn_on_every_evaluation() {
    assert_run(
        "nan_control_flow",
        r#"<?php
function f(): float { return NAN; }
$n = 0;
while (f() && $n < 2) { $n++; }
echo "n=$n\n";
var_dump(f() && true);
var_dump(f() || false);
var_dump(false || f());
"#,
        "n=2\nbool(true)\nbool(true)\nbool(true)\n",
        &warnings(6),
    );
}

/// A BOXED Mixed NAN warns too, through `__rt_mixed_cast_bool` / `__rt_mixed_is_empty`.
///
/// A `mixed`-returning function hands the coercion site a boxed cell rather than a raw `d0`, so
/// the probe lives inside those two runtime helpers instead of in the caller's lowering. They are
/// pinned together because `__rt_mixed_is_empty` is a LEAF helper that needed the separate
/// link-register-preserving probe variant. Reference PHP 8.5.6 prints two warnings and the
/// stdout asserted here.
#[test]
fn a_boxed_mixed_nan_warns_through_the_runtime_helpers() {
    assert_run(
        "nan_boxed_mixed",
        r#"<?php
function m(): mixed { return NAN; }
var_dump((bool)m());
var_dump(empty(m()));
"#,
        "bool(true)\nbool(false)\n",
        &warnings(2),
    );
}

/// A NAN read out of an ARRAY ELEMENT warns at the coercion, not at the read.
///
/// Reference PHP 8.5.6 prints one warning and `bool(true)`.
#[test]
fn a_nan_array_element_warns_when_coerced() {
    assert_run(
        "nan_array_element",
        r#"<?php
function arr(): array { return [NAN]; }
$a = arr();
var_dump((bool)$a[0]);
"#,
        "bool(true)\n",
        &warnings(1),
    );
}

/// A NAN passed as an ARGUMENT warns inside the callee, where the coercion happens.
///
/// The parameter shape matters: it is the one place where the float arrives in a register the
/// caller chose, so it proves the probe is emitted at the coercion site rather than at the value's
/// origin. Reference PHP 8.5.6 prints one warning and `bool(true)`.
#[test]
fn a_nan_argument_warns_inside_the_callee() {
    assert_run(
        "nan_argument",
        r#"<?php
function nanret(): float { return NAN; }
function takes(float $x): bool { return (bool)$x; }
var_dump(takes(nanret()));
"#,
        "bool(true)\n",
        &warnings(1),
    );
}

/// A NAN RETURNED and then coerced by the caller warns exactly once.
///
/// The value crosses a call boundary before reaching the coercion, so a probe wrongly attached to
/// the `return` would warn here as well and produce two lines.
/// Reference PHP 8.5.6 prints one warning and `bool(true)`.
#[test]
fn a_returned_nan_warns_once_at_the_callers_coercion() {
    assert_run(
        "nan_return",
        r#"<?php
function nanret(): float { return NAN; }
function passthrough(float $x): float { return $x; }
var_dump((bool)passthrough(nanret()));
"#,
        "bool(true)\n",
        &warnings(1),
    );
}

// ---------------------------------------------------------------------------
// Negative controls — php warns for NAN and for nothing else
// ---------------------------------------------------------------------------

/// CRITICAL NEGATIVE CONTROL: `INF`, `-INF`, `0.0` and `-0.0` must NOT warn.
///
/// php-src warns only for NAN; every other float-to-bool coercion is silent. This is the test
/// that distinguishes the intended fix from "warn on every float truthiness", which would be a
/// worse regression than the original silence. The values are asserted alongside the empty
/// stderr so a fix that stopped coercing correctly cannot pass by staying quiet.
/// Reference PHP 8.5.6 prints the stdout below with no diagnostics at all.
#[test]
fn inf_and_zero_float_coercions_stay_silent() {
    assert_run(
        "nan_negative_controls",
        r#"<?php
function pinf(): float { return INF; }
function ninf(): float { return -INF; }
function z(): float { return 0.0; }
function nz(): float { return -0.0; }
var_dump((bool)pinf());
var_dump((bool)ninf());
var_dump((bool)z());
var_dump((bool)nz());
var_dump(empty(pinf()), empty(ninf()), empty(z()), empty(nz()));
"#,
        "bool(true)\nbool(true)\nbool(false)\nbool(false)\n\
bool(false)\nbool(false)\nbool(true)\nbool(true)\n",
        "",
    );
}

/// The negative controls hold for BOXED Mixed floats too.
///
/// The probe inside `__rt_mixed_cast_bool` / `__rt_mixed_is_empty` is a separate copy of the
/// self-compare, so an over-eager rewrite of either helper would only show up here.
/// Reference PHP 8.5.6 prints the stdout below with no diagnostics at all.
#[test]
fn boxed_mixed_inf_and_zero_coercions_stay_silent() {
    assert_run(
        "nan_negative_controls_mixed",
        r#"<?php
function mpinf(): mixed { return INF; }
function mninf(): mixed { return -INF; }
function mz(): mixed { return 0.0; }
var_dump((bool)mpinf());
var_dump((bool)mninf());
var_dump((bool)mz());
var_dump(empty(mpinf()), empty(mninf()), empty(mz()));
"#,
        "bool(true)\nbool(true)\nbool(false)\nbool(false)\nbool(false)\nbool(true)\n",
        "",
    );
}

/// A MISSED float array read must not be mistaken for a user NAN.
///
/// elephc's in-band `NULL_SENTINEL` is itself a quiet-NaN bit pattern, so a missed `float` read
/// arrives at a truthiness site looking exactly like `NAN`. `__rt_warn_nan_coerced_bool` filters
/// it by exact bit compare, because php-src reports `Undefined array key` for that read and never
/// the NAN warning. Reference PHP 8.5.6 prints `bool(true)` plus its own undefined-key warning;
/// what is pinned here is that the NAN warning is NOT among elephc's diagnostics.
#[test]
fn a_missed_float_read_reports_the_undefined_key_and_not_the_nan_warning() {
    assert_run(
        "nan_null_sentinel_control",
        r#"<?php
function raw(string $k) { $m = ["a" => 1.5]; return $m[$k]; }
var_dump(empty(raw("zz")));
"#,
        "bool(true)\n",
        "Warning: Undefined array key \"zz\"\n",
    );
}

// ---------------------------------------------------------------------------
// Version gating — the diagnostic exists only from PHP 8.5 on
// ---------------------------------------------------------------------------

/// `--php-version 8.2` coerces NAN silently, exactly as php 8.2 does.
///
/// `nan_bool_coercion_warning_enabled()` gates every call site on `version_id() >= 80500`, so a
/// pre-8.5 profile carries neither the inline probe nor the helper call. The VALUES must be
/// unchanged: 8.2 still coerces NAN to `true`.
#[test]
fn php_82_coerces_nan_silently() {
    assert_run_for_version(
        "nan_version_82",
        r#"<?php
function f(): float { return NAN; }
var_dump((bool)f());
var_dump(empty(f()));
"#,
        Some("8.2"),
        "bool(true)\nbool(false)\n",
        "",
    );
}

/// `--php-version 8.3` coerces NAN silently.
#[test]
fn php_83_coerces_nan_silently() {
    assert_run_for_version(
        "nan_version_83",
        r#"<?php
function f(): float { return NAN; }
var_dump((bool)f());
var_dump(empty(f()));
"#,
        Some("8.3"),
        "bool(true)\nbool(false)\n",
        "",
    );
}

/// `--php-version 8.4` coerces NAN silently — the last profile before the RFC landed.
#[test]
fn php_84_coerces_nan_silently() {
    assert_run_for_version(
        "nan_version_84",
        r#"<?php
function f(): float { return NAN; }
var_dump((bool)f());
var_dump(empty(f()));
"#,
        Some("8.4"),
        "bool(true)\nbool(false)\n",
        "",
    );
}

/// `--php-version 8.5` warns — the same program as the three profiles above.
///
/// Asserted explicitly rather than relying on the default so the gate is pinned from BOTH sides:
/// the three silent tests alone would also pass if the diagnostic had been deleted outright.
#[test]
fn php_85_warns_on_the_same_program_the_older_profiles_accept_silently() {
    assert_run_for_version(
        "nan_version_85",
        r#"<?php
function f(): float { return NAN; }
var_dump((bool)f());
var_dump(empty(f()));
"#,
        Some("8.5"),
        "bool(true)\nbool(false)\n",
        &warnings(2),
    );
}

// ---------------------------------------------------------------------------
// NAN to string — the same RFC's second coercion warning
// ---------------------------------------------------------------------------

/// Every string coercion of a NAN warns once: `echo`, concatenation, interpolation, `(string)`,
/// `strval()`, `.=`, a `string` parameter, a string builtin argument, `printf("%s")`,
/// `print_r()`, `implode()` of a mixed array and `settype(..., "string")`.
///
/// These reach `__rt_ftoa_coerce` through the scalar cast, the builtin argument coercion, the
/// boxed-Mixed cast and the `print_r` walker. Reference PHP 8.5 prints twelve warnings and the
/// stdout asserted here. Regression for #1354.
#[test]
fn every_string_coercion_site_warns_once_for_a_nan() {
    assert_run(
        "nan_string_sites",
        r#"<?php
function f(): float { return NAN; }
function takes(string $s): string { return "[$s]"; }
echo f(), "\n";
echo "a" . f() . "\n";
$n = f();
echo "i=$n\n";
echo (string)f(), "\n";
echo strval(f()), "\n";
$s = "s"; $s .= f(); echo $s, "\n";
echo takes(f()), "\n";
echo str_pad(f(), 5, "-"), "\n";
printf("%s\n", f());
print_r(f()); echo "\n";
echo implode(",", ["a", f()]), "\n";
$t = f(); settype($t, "string"); echo $t, "\n";
"#,
        "NAN\naNAN\ni=NAN\nNAN\nNAN\nsNAN\n[NAN]\nNAN--\nNAN\nNAN\na,NAN\nNAN\n",
        &string_warnings(12),
    );
}

/// Formatting a NAN WITHOUT coercing it to string stays silent, and a boxed Mixed NAN warns.
///
/// php-src warns only on a string conversion: `var_dump()`, `var_export()`, `serialize()`,
/// `json_encode()`, a comparison with a non-numeric string and `number_format()` all format the
/// float without converting it, so they must not warn; neither do `INF` and `-INF`, nor an
/// `@`-suppressed conversion. The last two lines coerce a `mixed` NAN, which goes through
/// `__rt_mixed_cast_string` / `__rt_mixed_write_stdout`. Reference PHP 8.5 prints exactly two
/// warnings and the stdout asserted here.
#[test]
fn nan_formatting_without_string_coercion_stays_silent() {
    assert_run(
        "nan_string_controls",
        r#"<?php
function f(): float { return NAN; }
function m(): mixed { return NAN; }
function inf(): float { return INF; }
var_dump(f());
var_export(f()); echo "\n";
echo serialize(f()), "\n";
var_dump(json_encode(f()));
var_dump(f() == "abc");
echo number_format(f()), "\n";
echo inf(), " ", -inf(), "\n";
echo @strval(f()), "\n";
echo m(), "\n";
echo (string)m(), "\n";
"#,
        "float(NAN)\nNAN\nd:NAN;\nbool(false)\nbool(false)\nnan\nINF -INF\nNAN\nNAN\nNAN\n",
        &string_warnings(2),
    );
}

/// `--php-version 8.4` converts a NAN to string silently, as php 8.4 does.
#[test]
fn php_84_coerces_nan_to_string_silently() {
    assert_run_for_version(
        "nan_string_version_84",
        r#"<?php
function f(): float { return NAN; }
echo f(), "|", (string)f(), "\n";
"#,
        Some("8.4"),
        "NAN|NAN\n",
        "",
    );
}
