//! Purpose:
//! End-to-end tests for PHP 8.5's NaN-to-bool and NaN-to-string coercion warnings.
//! The runtime helpers and inline probes live in
//! `src/codegen_support/runtime/arrays/nan_bool_coercion_warning.rs` and are reached from
//! native coercion lowering, boxed-Mixed helpers, runtime callables and eval casts.
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
//! - Native NAN coercions use function returns, parameters or array elements to avoid AOT
//!   constant folding. The eval fixture instead supplies its source at runtime, so its NaN
//!   literal reaches the interpreter's native cast bridge.
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

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Removes a fixture's directory even when an output assertion panics.
struct TestDirectory(PathBuf);

impl Drop for TestDirectory {
    /// Releases only the isolated directory created for this fixture.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

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
    let expected_diagnostics = if stem.starts_with("nan_string_eval") {
        "warning: dynamic eval was compiled without optional regex support"
    } else {
        ""
    };
    assert_eq!(diagnostics, expected_diagnostics, "unexpected elephc diagnostic");
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
    let _cleanup = TestDirectory(dir.clone());
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
        normalize_runtime_stderr(&String::from_utf8_lossy(&output.stderr)),
        expected_stderr,
        "program stderr diverged"
    );
}


/// Removes the runtime diagnostic's leading framing and ` in FILE on line N` suffix.
///
/// The compiler now prints PHP's caller location for every displayed diagnostic. These
/// fixtures pin the message body and warning count, not the temp-directory path.
fn normalize_runtime_stderr(stderr: &str) -> String {
    fn strip_location(line: &str) -> &str {
        let Some(on_line) = line.rfind(" on line ") else { return line; };
        let suffix = &line[on_line + " on line ".len()..];
        if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            return line;
        }
        line[..on_line].rfind(" in ").map_or(line, |in_pos| &line[..in_pos])
    }
    let mut normalized = String::new();
    for line in stderr.lines() {
        let line = strip_location(line);
        if line.is_empty() {
            continue;
        }
        normalized.push_str(line);
        normalized.push('\n');
    }
    normalized
}

/// Returns `NAN_WARNING` repeated `count` times, i.e. the whole stderr of a program that
/// coerced a NAN to bool exactly `count` times.
fn warnings(count: usize) -> String {
    NAN_WARNING.repeat(count)
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
// NAN coerced to string (AIC #219, illegalstudio/elephc#1354)
// ---------------------------------------------------------------------------

/// The exact stderr line elephc emits for one NAN-to-string coercion.
const NAN_STRING_WARNING: &str = "Warning: unexpected NAN value was coerced to string\n";

/// Every string coercion of a NAN warns once: `(string)`, `echo`, concatenation, interpolation,
/// `strval()`, a builtin's string parameter, `implode()`, `settype()`, `printf("%s")`, and a boxed
/// `mixed` cast and `echo`. Reference PHP 8.5.10 prints eleven warnings and the stdout asserted
/// here.
#[test]
fn every_string_coercion_site_warns_once_for_a_nan() {
    assert_run(
        "nan_string_sites",
        r#"<?php
function f(): float { return NAN; }
function m(): mixed { return json_decode('[1]', true) ? NAN : 0; }
echo (string) f(), "\n";
echo f(), "\n";
echo "i:" . f() . "\n";
$v = f(); echo "s:$v\n";
echo strval(f()), "\n";
echo str_pad(f(), 4, "*"), "\n";
echo implode(",", [f()]), "\n";
$t = f(); settype($t, "string"); echo $t, "\n";
printf("%s\n", f());
echo (string) m(), "\n";
echo m(), "\n";
"#,
        "NAN\nNAN\ni:NAN\ns:NAN\nNAN\nNAN*\nNAN\nNAN\nNAN\nNAN\nNAN\n",
        &NAN_STRING_WARNING.repeat(11),
    );
}

/// Only a NAN warns: `INF`, `-INF`, `0.0`, `-0.0` and an ordinary float coerce silently, and
/// `var_dump()`, `print_r()` and `var_export()` render a NAN without coercing it.
#[test]
fn string_coercion_of_other_floats_and_nan_dumps_stay_silent() {
    assert_run(
        "nan_string_controls",
        r#"<?php
function g(float $x): float { return $x; }
echo (string) g(INF), " ", g(-INF), " ", g(0.0), " ", g(-0.0), " ", g(1.5), "\n";
function f(): float { return NAN; }
var_dump(f());
print_r(f()); echo "\n";
echo var_export(f(), true), "\n";
"#,
        "INF -INF 0 -0 1.5\nfloat(NAN)\nNAN\nNAN\n",
        "",
    );
}

/// PHP 8.4 coerces a NAN to string silently, so a `--php-version 8.4` build emits no probe.
#[test]
fn string_coercion_of_a_nan_is_silent_before_php_8_5() {
    assert_run_for_version(
        "nan_string_84",
        r#"<?php
function f(): float { return NAN; }
echo (string) f(), " ", f(), "\n";
"#,
        Some("8.4"),
        "NAN NAN\n",
        "",
    );
}

/// Exercises the IO loader, JSON input loader and typed runtime callable adapter.
const NAN_STRING_ADDITIONAL_SITES: &str = r#"<?php
function f(): float { return NAN; }
echo basename(f()), "\n";
var_dump(json_decode(f()));
function accept_nan_string(string $value): string { return $value; }
$callback = $argc > 0 ? 'accept_nan_string' : 'strrev';
echo call_user_func_array($callback, [f()]), "\n";
"#;

/// Each additional AOT coercion warns once, matching PHP 8.5's string parameter handling.
#[test]
fn nan_string_additional_aot_sites_warn_once() {
    assert_run(
        "nan_string_additional",
        NAN_STRING_ADDITIONAL_SITES,
        "NAN\nNULL\nNAN\n",
        &NAN_STRING_WARNING.repeat(3),
    );
}

/// JSON validation coerces its source, while encoding and loose NaN comparisons stay silent.
#[test]
fn nan_string_json_validation_and_noncoercing_controls() {
    assert_run(
        "nan_string_json",
        r#"<?php
function f(): float { return NAN; }
var_dump(json_validate(f()));
var_dump(json_encode(f()));
function comparison_nan(): mixed { return json_decode('[1]', true) ? NAN : 0; }
var_dump(comparison_nan() == "text", comparison_nan() < "text");
"#,
        "bool(false)\nbool(false)\nbool(false)\nbool(false)\n",
        NAN_STRING_WARNING,
    );
}

/// Dynamic eval reaches the boxed cast bridge rather than the AOT float cast lowering.
#[test]
fn nan_string_dynamic_eval_cast_warns_once() {
    let source = r#"<?php
$code = $argc > 0 ? 'echo (string) NAN, "\n";' : '';
eval($code);
"#;
    assert_run("nan_string_eval", source, "NAN\n", NAN_STRING_WARNING);
    for version in ["8.2", "8.3", "8.4"] {
        assert_run_for_version("nan_string_eval_old", source, Some(version), "NAN\n", "");
    }
}

/// Both argument and environment float elements use pcntl_exec's scalar string conversion.
#[test]
fn nan_string_pcntl_exec_arguments_and_environment_warn_once() {
    let source = r#"<?php
function f(): float { return NAN; }
var_dump(pcntl_exec('/definitely/missing/elephc-nan', [f()], ['VALUE' => f()]));
"#;
    let exec_warning = "Warning: pcntl_exec(): Error has occurred: (errno 2) No such file or directory\n";
    assert_run(
        "nan_string_exec",
        source,
        "bool(false)\n",
        &(NAN_STRING_WARNING.repeat(2) + exec_warning),
    );
    for version in ["8.2", "8.3", "8.4"] {
        assert_run_for_version("nan_string_exec_old", source, Some(version), "bool(false)\n", exec_warning);
    }
}

/// A concatenating warning handler must preserve mixed and raw-float implode results.
#[test]
fn nan_string_implode_preserves_output_across_concatenating_handler() {
    assert_run(
        "nan_string_implode_handler",
        r#"<?php
set_error_handler(function($n, $m) { echo "w:" . $m . "\n"; return $n === E_WARNING; });
function f(): float { return NAN; }
echo implode(",", ["a", f(), "b"]), "\n";
echo implode(",", [f(), f()]), "\n";
"#,
        "w:unexpected NAN value was coerced to string\na,NAN,b\n\
w:unexpected NAN value was coerced to string\n\
w:unexpected NAN value was coerced to string\nNAN,NAN\n",
        "",
    );
}

/// The newly covered AOT sites retain their values without NaN diagnostics before PHP 8.5.
#[test]
fn nan_string_additional_aot_sites_stay_silent_before_php_8_5() {
    for version in ["8.2", "8.3", "8.4"] {
        assert_run_for_version(
            "nan_string_additional_old",
            NAN_STRING_ADDITIONAL_SITES,
            Some(version),
            "NAN\nNULL\nNAN\n",
            "",
        );
    }
}

/// Unused casts still warn and preserve handler calls, including exceptions across a callee.
#[test]
fn nan_string_discarded_casts_keep_warnings_and_handler_throws() {
    assert_run(
        "nan_string_discarded",
        r#"<?php
(string) NAN;
function nan_for_discard(): float { return NAN; }
$value = nan_for_discard();
(string) $value;
set_error_handler(function($n, $m) { echo "handled:" . $m . "\n"; return $n === E_WARNING; });
(string) nan_for_discard();
restore_error_handler();
set_error_handler(function($n, $m) { throw new RuntimeException($m . ":" . $n); });
function discard_nan_string(): void { (string) NAN; }
try { discard_nan_string(); echo "missed\n"; }
catch (RuntimeException $e) { echo "caught:" . $e->getMessage() . "\n"; }
function discard_nan_sum(float $value): void { (string) ($value + 0.0); }
try { discard_nan_sum(NAN); echo "missed-sum\n"; }
catch (RuntimeException $e) { echo "caught-sum:" . $e->getMessage() . "\n"; }
function discard_nan_negation(float $value): void { (string) -$value; }
try { discard_nan_negation(NAN); echo "missed-negation\n"; }
catch (RuntimeException $e) { echo "caught-negation:" . $e->getMessage() . "\n"; }
function discard_nan_float_cast(mixed $value): void { (string) (float) $value; }
try { discard_nan_float_cast(NAN); echo "missed-float-cast\n"; }
catch (RuntimeException $e) { echo "caught-float-cast:" . $e->getMessage() . "\n"; }
const STORED_NAN_STRING_VALUE = NAN;
function discard_nan_constant(): void { (string) STORED_NAN_STRING_VALUE; }
try { discard_nan_constant(); echo "missed-constant\n"; }
catch (RuntimeException $e) { echo "caught-constant:" . $e->getMessage() . "\n"; }
class NanStringConstant { const VALUE = NAN; }
function discard_nan_class_constant(): void { (string) NanStringConstant::VALUE; }
try { discard_nan_class_constant(); echo "missed-class-constant\n"; }
catch (RuntimeException $e) { echo "caught-class-constant:" . $e->getMessage() . "\n"; }
"#,
        "handled:unexpected NAN value was coerced to string\n\
caught:unexpected NAN value was coerced to string:2\n\
caught-sum:unexpected NAN value was coerced to string:2\n\
caught-negation:unexpected NAN value was coerced to string:2\n\
caught-float-cast:unexpected NAN value was coerced to string:2\n\
caught-constant:unexpected NAN value was coerced to string:2\n\
caught-class-constant:unexpected NAN value was coerced to string:2\n",
        &NAN_STRING_WARNING.repeat(2),
    );
}
