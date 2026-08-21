//! Purpose:
//! End-to-end tests for PHP 8.5's `unexpected NAN value was coerced to string` E_WARNING — the
//! `__rt_warn_nan_coerced_string` runtime helper and the inline NAN probe that guards it
//! (`src/codegen_support/runtime/strings/nan_string_coercion_warning.rs`), reached from the one
//! place every float-to-string conversion passes through, `__rt_ftoa`.
//!
//! The diagnostic is NEW IN PHP 8.5 (RFC `warnings-php-8-5`, "Coercing NAN to other types");
//! 8.2/8.3/8.4 render `NAN` silently — measured here against php 8.2.31, which is silent, and
//! php 8.5.6, which warns. Before the fix elephc produced the right BYTES on every profile and
//! never warned, so a program that stringified a NAN lost the diagnostic entirely.
//!
//! Called from:
//! - `cargo test --test nan_string_coercion_tests` through Rust's test harness.
//!
//! Key details:
//! - Harness style mirrors `tests/nan_bool_coercion_tests.rs`, the sibling diagnostic from the
//!   same RFC: the elephc CLI (`CARGO_BIN_EXE_elephc`) is invoked as a subprocess in an isolated
//!   temp dir, compiled to a plain executable, run, and its output asserted. Host-target only.
//! - EVERY NAN reaches its coercion site through a FUNCTION RETURN, never as a literal. `NAN` in
//!   literal position is const-folded — php folds it too, and warns at COMPILE time — so a
//!   literal probe would measure the constant folder and pass even with the runtime path deleted.
//! - Stdout and stderr are asserted SEPARATELY, and `expected_stderr` is the COMPLETE stderr,
//!   which is what pins the warning COUNT. php-src warns once per COERCION, not once per site:
//!   `f() . f()` warns twice and a loop body warns once per iteration.
//! - THE NEGATIVE CONTROLS ARE THE POINT. php warns for NAN and for nothing else — `INF`,
//!   `-INF`, `0.0`, `-0.0` and finite values are silent — and it also stays silent for the four
//!   float renderers that are NOT string coercions: `var_dump`, `json_encode`, `number_format`
//!   and `sprintf('%f')`. Those four reach `__rt_ftoa_repr`, `__rt_json_encode_float`, a direct
//!   `snprintf` and the printf engine instead, and the whole fix rests on that separation. A
//!   probe placed one layer lower would warn for all of them.
//! - elephc does NOT synthesize php-src's ` in <file> on line <n>` message tail, and writes its
//!   runtime diagnostics to STDERR where php writes this one to STDOUT. Both are the house
//!   convention for every elephc runtime diagnostic (`foreach_non_iterable_warning.rs`,
//!   `nan_bool_coercion_warning.rs`), so these tests assert the bare `Warning: <text>` line on
//!   stderr.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The exact stderr line elephc emits for one NAN-to-string coercion.
///
/// php-src appends ` in <file> on line <n>`; elephc deliberately does not (see the module
/// preamble). Keeping the text in one constant makes the message and the counts below impossible
/// to drift apart.
const NAN_WARNING: &str = "Warning: unexpected NAN value was coerced to string\n";

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
/// Linking also surfaces the HOST linker's warnings, which are environmental rather than anything
/// elephc emitted. This filter runs on the COMPILER's stderr only; the runtime NAN warning these
/// tests assert on comes from the compiled program's own stderr and never passes through here.
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

/// Returns `NAN_WARNING` repeated `count` times, i.e. the whole stderr of a program that coerced
/// a NAN to a string exactly `count` times.
fn warnings(count: usize) -> String {
    NAN_WARNING.repeat(count)
}

/// The program the version-gate tests share, so the three profiles differ only in their gate.
const VERSION_GATE_SOURCE: &str = r#"<?php
function f(): float { return NAN; }
echo f(), "|";
echo (string)f(), "|";
"#;

// ---------------------------------------------------------------------------
// The coercion sites — every syntactic form that turns a float into a string
// ---------------------------------------------------------------------------

/// The nine string-coercion sites, all against the same NAN-returning function.
///
/// `echo`, `(string)`, `strval()`, `implode()`, concatenation, `sprintf('%s')`, interpolation,
/// `print_r()` and a string builtin taking a float are separate lowering paths, and php warns at
/// each of them. They converge on `__rt_ftoa`, which is why one probe covers all nine.
///
/// Reference PHP 8.5.6 on this exact program prints the stdout asserted here and NINE warnings.
#[test]
fn every_string_coercion_site_warns_once_for_a_nan() {
    assert_run(
        "nan_str_sites",
        r#"<?php
function f(): float { return NAN; }
echo f(), "\n";
echo (string)f(), "\n";
echo strval(f()), "\n";
echo implode(",", [f()]), "\n";
echo "x" . f(), "\n";
echo sprintf('%s', f()), "\n";
$v = f();
echo "in{$v}terp\n";
echo print_r(f(), true), "\n";
echo str_pad(f(), 5, "."), "\n";
"#,
        "NAN\nNAN\nNAN\nNAN\nxNAN\nNAN\ninNANterp\nNAN\nNAN..\n",
        &warnings(9),
    );
}

/// The warning is per COERCION, not per site: two operands warn twice, a loop warns per pass.
///
/// A "warn once per site" implementation would print two warnings for this program instead of
/// five, and would still pass a test that only asserted the text.
///
/// Reference PHP 8.5.6 on this exact program prints `NANNAN` then three `NAN` lines, and FIVE
/// warnings.
#[test]
fn repeated_coercions_warn_once_each() {
    assert_run(
        "nan_str_counts",
        r#"<?php
function f(): float { return NAN; }
echo f() . f(), "\n";
for ($i = 0; $i < 3; $i++) { echo f(), "\n"; }
"#,
        "NANNAN\nNAN\nNAN\nNAN\n",
        &warnings(5),
    );
}

/// `@` silences the coercion it encloses, and only that one.
///
/// The middle line is the control: `@f()` suppresses inside the CALL, but the `echo` that
/// stringifies its result is outside the suppression, so php still warns there. A fix that
/// bypassed the suppression counter would print three warnings; one that swallowed the whole
/// statement would print none.
///
/// Reference PHP 8.5.6 on this exact program prints three `NAN|` lines and ONE warning.
#[test]
fn the_suppression_operator_silences_the_coercion_it_encloses() {
    assert_run(
        "nan_str_suppress",
        r#"<?php
function f(): float { return NAN; }
echo @((string)f()), "|\n";
echo @f(), "|\n";
$s = @implode(",", [f()]);
echo $s, "|\n";
"#,
        "NAN|\nNAN|\nNAN|\n",
        &warnings(1),
    );
}

// ---------------------------------------------------------------------------
// Negative controls — everything php coerces or renders SILENTLY
// ---------------------------------------------------------------------------

/// `INF`, `-INF`, `0.0`, `-0.0` and a finite float all stringify without a warning.
///
/// NAN is the only value that compares unordered with itself, which is exactly what the probe
/// tests. An implementation that warned on every float-to-string conversion — or that keyed off
/// the `N` byte and so caught nothing else, but also nothing about infinities — is separated from
/// a correct one only here.
///
/// Reference PHP 8.5.6 on this exact program prints `INF|-INF|0|-0|1.5|` and NOTHING on stderr.
#[test]
fn inf_and_finite_float_coercions_stay_silent() {
    assert_run(
        "nan_str_inf_control",
        r#"<?php
function inf(): float { return INF; }
function ninf(): float { return -INF; }
function zero(): float { return 0.0; }
function nzero(): float { return -0.0; }
function fin(): float { return 1.5; }
echo inf(), "|", ninf(), "|", zero(), "|", nzero(), "|", fin(), "|\n";
"#,
        "INF|-INF|0|-0|1.5|\n",
        "",
    );
}

/// The four float renderers that are NOT string coercions stay silent for a NAN.
///
/// php warns when a NAN is coerced to a STRING, not whenever a NAN is printed: `var_dump`,
/// `json_encode`, `number_format` and `sprintf('%f')` all render one without complaint. In elephc
/// they reach `__rt_ftoa_repr`, `__rt_json_encode_float`, a direct `snprintf` and the printf
/// engine — never `__rt_ftoa` — and that separation is the whole reason the probe can live at
/// `__rt_ftoa`'s entry.
///
/// The `sprintf('%f')` case is asserted through `strlen(...) > 0` rather than its bytes: what
/// this test owns is the SILENCE, and the spelling of `%f` on a NAN is a separate question.
///
/// Reference PHP 8.5.6 on this exact program prints the stdout asserted here and NOTHING on
/// stderr.
#[test]
fn the_float_renderers_php_leaves_silent_stay_silent() {
    assert_run(
        "nan_str_renderer_control",
        r#"<?php
function f(): float { return NAN; }
var_dump(f());
echo json_encode([1]), "|\n";
echo number_format(f()), "|\n";
var_dump(strlen(sprintf('%f', f())) > 0);
"#,
        "float(NAN)\n[1]|\nnan|\nbool(true)\n",
        "",
    );
}

// ---------------------------------------------------------------------------
// Version gating — the diagnostic exists only from PHP 8.5 on
// ---------------------------------------------------------------------------

/// `--php-version 8.2` stringifies NAN silently, exactly as php 8.2.31 does.
///
/// `nan_string_coercion_warning_enabled()` gates the probe on `version_id() >= 80500`, so a
/// pre-8.5 profile carries neither the inline test nor the helper call. The BYTES must be
/// unchanged: 8.2 still renders `NAN`.
#[test]
fn php_82_coerces_nan_to_string_silently() {
    assert_run_for_version(
        "nan_str_version_82",
        VERSION_GATE_SOURCE,
        Some("8.2"),
        "NAN|NAN|",
        "",
    );
}

/// `--php-version 8.3` stringifies NAN silently.
#[test]
fn php_83_coerces_nan_to_string_silently() {
    assert_run_for_version(
        "nan_str_version_83",
        VERSION_GATE_SOURCE,
        Some("8.3"),
        "NAN|NAN|",
        "",
    );
}

/// `--php-version 8.4` stringifies NAN silently — the last profile before the RFC landed.
#[test]
fn php_84_coerces_nan_to_string_silently() {
    assert_run_for_version(
        "nan_str_version_84",
        VERSION_GATE_SOURCE,
        Some("8.4"),
        "NAN|NAN|",
        "",
    );
}

/// `--php-version 8.5` warns on the same program the older profiles accept silently.
///
/// Asserting the identical source against both sides of the gate is what proves the difference is
/// the PROFILE and not the program.
#[test]
fn php_85_warns_on_the_same_program_the_older_profiles_accept_silently() {
    assert_run_for_version(
        "nan_str_version_85",
        VERSION_GATE_SOURCE,
        Some("8.5"),
        "NAN|NAN|",
        &warnings(2),
    );
}
