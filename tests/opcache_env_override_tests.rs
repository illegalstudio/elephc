//! Purpose:
//! End-to-end tests for the RUNTIME `ELEPHC_INI_*` per-directive environment override baked by
//! `src/opcache_prelude.rs` (`ENV_OVERRIDE_HELPERS` / `render_opcache_env_helpers`). Covers both
//! environment-variable spellings and their precedence, the "both surfaces move together"
//! property (`ini_get`/`ini_get_all` AND `opcache_get_configuration()['directives']`), the
//! per-type normalization (bool / int with hex+`K`/`M`/`G` / percent / plain float / string), the
//! invalid-value-is-ignored floor, the compile-time→runtime precedence chain, and — the honesty
//! property — that an EXCLUDED directive's environment variable is ignored on both surfaces.
//!
//! Called from:
//! - `cargo test --test opcache_env_override_tests` through Rust's test harness.
//!
//! Key details:
//! - THIS IS AN ELEPHC EXTENSION, NOT PHP PARITY. Reference PHP has no per-directive environment
//!   override; its only environment mechanisms are file-granularity (`PHPRC`,
//!   `PHP_INI_SCAN_DIR`). VERIFIED on reference PHP 8.5.6: `PHP_INI_opcache_jit=tracing`,
//!   `opcache_jit=tracing` and `opcache.jit=tracing` in the environment all leave
//!   `ini_get('opcache.jit')` reporting the compiled default `'disable'`. What IS pinned from
//!   reference PHP here is the per-type NORMALIZATION (see the per-test notes) and the rule that
//!   `-d`-style overrides move `ini_get` and `opcache_get_configuration()` together.
//! - THE SCOPE RULE is what `excluded_directive_is_ignored_on_both_surfaces` guards: only
//!   directives elephc merely REPORTS are runtime-overridable. `opcache.enable_cli` bakes the
//!   cache-enabled gate at compile time, so honoring it on the reporting surface alone would
//!   produce a binary whose `ini_get('opcache.enable_cli') === '1'` sits next to an
//!   `opcache_get_status()` that still returns `false`. An ignored environment variable is
//!   honest; a self-contradicting report is not.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp dir,
//!   compile a plain executable, then run it WITH ENVIRONMENT VARIABLES SET ON THE CHILD, the
//!   same harness style as `opcache_jit_status_tests` / `opcache_ini_tests`. Host-target only
//!   (macOS aarch64 local).
//! - The probe narrows with `is_array()` before indexing because `opcache_get_status()`'s return
//!   hint is deliberately omitted so ordinary union return inference handles its two exits (see
//!   `GET_STATUS_TEMPLATE`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// The probe program: prints both surfaces (the normalized
/// `opcache_get_configuration()['directives']` value and the raw `ini_get` string) for one
/// directive of every type code, plus the excluded `opcache.enable_cli` and the cache-enabled
/// state derived from it. Every line is `key=<var_export>`.
///
/// Directive selection, one per type code plus the excluded control:
/// - `opcache.save_comments` — `'b'` (bool)
/// - `opcache.jit_debug` — `'i'` (int; accepts decimal, `0x` hex and `K`/`M`/`G`). It stands in
///   for the whole int normalizer, so it must be a directive elephc only REPORTS: the earlier
///   pick, `opcache.max_file_size`, now bakes the runtime cache's admission rule and is excluded
///   from runtime override. VERIFIED on reference PHP 8.5.10 that the two normalize identically
///   (`1M`→`1048576`, `12abc`→`12`, `8K`→`8192`, `nope`→`0`, `0x10`→`16`), so no row moved.
/// - `opcache.optimization_level` — `'i'`, and the one whose DEFAULT raw string is hex
/// - `opcache.max_wasted_percentage` — `'p'` (percent, `atoi` + `1..=50` + `/100`)
/// - `opcache.jit_prof_threshold` — `'f'` (plain float, `strtod` leading prefix)
/// - `opcache.lockfile_path` — `'s'` (string, verbatim). Same substitution, for the same
///   reason: `opcache.error_log` now selects where the `zend_accel_error` channel writes.
/// - `opcache.enable_cli` — EXCLUDED (bakes the cache-enabled gate)
const PROBE: &str = r#"<?php
$d = opcache_get_configuration()['directives'];
echo 'cfg.save_comments=', var_export($d['opcache.save_comments'], true), "\n";
echo 'ini.save_comments=', var_export(ini_get('opcache.save_comments'), true), "\n";
echo 'cfg.jit_debug=', var_export($d['opcache.jit_debug'], true), "\n";
echo 'ini.jit_debug=', var_export(ini_get('opcache.jit_debug'), true), "\n";
echo 'cfg.optimization_level=', var_export($d['opcache.optimization_level'], true), "\n";
echo 'ini.optimization_level=', var_export(ini_get('opcache.optimization_level'), true), "\n";
echo 'cfg.max_wasted_percentage=', var_export($d['opcache.max_wasted_percentage'], true), "\n";
echo 'ini.max_wasted_percentage=', var_export(ini_get('opcache.max_wasted_percentage'), true), "\n";
echo 'cfg.jit_prof_threshold=', var_export($d['opcache.jit_prof_threshold'], true), "\n";
echo 'ini.jit_prof_threshold=', var_export(ini_get('opcache.jit_prof_threshold'), true), "\n";
echo 'cfg.lockfile_path=', var_export($d['opcache.lockfile_path'], true), "\n";
echo 'ini.lockfile_path=', var_export(ini_get('opcache.lockfile_path'), true), "\n";
echo 'cfg.enable_cli=', var_export($d['opcache.enable_cli'], true), "\n";
echo 'ini.enable_cli=', var_export(ini_get('opcache.enable_cli'), true), "\n";
echo 'status_is_array=', var_export(is_array(opcache_get_status()), true), "\n";
"#;

/// The `ini_get_all` probe: the same directive read through the flat and the detailed
/// projections, proving the whole `ini_get_all` surface inherits the override (both projections
/// route through `__elephc_opcache_ini_string`).
const ALL_PROBE: &str = r#"<?php
$plain = ini_get_all(null, false);
if (is_array($plain)) {
    echo 'plain=', var_export($plain['opcache.save_comments'], true), "\n";
}
$details = ini_get_all('zend opcache', true);
if (is_array($details)) {
    $entry = $details['opcache.save_comments'];
    if (is_array($entry)) {
        echo 'global=', var_export($entry['global_value'], true), "\n";
        echo 'local=', var_export($entry['local_value'], true), "\n";
    }
}
"#;

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

/// Compiles `source` in `dir` with the supplied extra CLI arguments and returns the executable.
fn compile(dir: &Path, source: &str, stem: &str, args: &[&str]) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(&php);
    cmd.args(args);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed for {args:?}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Runs `binary` with `env` applied to the CHILD process only and returns stdout.
///
/// The parent's own environment is left untouched, so tests stay independent under the parallel
/// test harness — the override is a property of the run, not of the test process.
fn run_with_env(binary: &Path, env: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(binary);
    for (key, value) in env {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}) for {env:?}:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Compiles `PROBE` once in a fresh dir and returns the executable path.
fn probe_binary(prefix: &str, args: &[&str]) -> (PathBuf, PathBuf) {
    let dir = make_test_dir(prefix);
    let binary = compile(&dir, PROBE, "app", args);
    (dir, binary)
}

/// Extracts the `key=value` line for `key` from probe stdout.
fn line<'a>(stdout: &'a str, key: &str) -> &'a str {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("probe stdout has no `{key}=` line:\n{stdout}"))
}

/// The compile-time report, with no environment variable in sight. This is the REGRESSION ANCHOR:
/// every value below is what the binary printed before the runtime override existed, so an
/// unset `ELEPHC_INI_*` costs nothing observable.
#[test]
fn no_env_reports_the_compile_time_values() {
    let (_dir, binary) = probe_binary("opcache_env_baseline", &[]);
    let out = run_with_env(&binary, &[]);
    assert_eq!(line(&out, "cfg.save_comments"), "true");
    assert_eq!(line(&out, "ini.save_comments"), "'1'");
    assert_eq!(line(&out, "cfg.jit_debug"), "0");
    assert_eq!(line(&out, "ini.jit_debug"), "'0'");
    assert_eq!(line(&out, "cfg.optimization_level"), "2147401727");
    assert_eq!(line(&out, "ini.optimization_level"), "'0x7FFEBFFF'");
    assert_eq!(line(&out, "cfg.max_wasted_percentage"), "0.05");
    assert_eq!(line(&out, "ini.max_wasted_percentage"), "'5'");
    assert_eq!(line(&out, "cfg.jit_prof_threshold"), "0.005");
    assert_eq!(line(&out, "ini.jit_prof_threshold"), "'0.005'");
    assert_eq!(line(&out, "cfg.lockfile_path"), "'/tmp'");
    assert_eq!(line(&out, "ini.lockfile_path"), "'/tmp'");
    assert_eq!(line(&out, "cfg.enable_cli"), "false");
    assert_eq!(line(&out, "ini.enable_cli"), "'0'");
    // A default CLI binary reports the cache disabled (matching reference `php script.php`).
    assert_eq!(line(&out, "status_is_array"), "false");
}

/// An empty environment variable is treated as UNSET by the OPcache override helper, which casts
/// `getenv` to string and uses the dotted spelling as a fallback when the result is empty. This is
/// an override-policy choice; `getenv` itself distinguishes an empty value from a missing name.
#[test]
fn empty_env_value_is_treated_as_unset() {
    let (_dir, binary) = probe_binary("opcache_env_empty", &[]);
    let out = run_with_env(&binary, &[("ELEPHC_INI_opcache__save_comments", "")]);
    assert_eq!(line(&out, "cfg.save_comments"), "true");
    assert_eq!(line(&out, "ini.save_comments"), "'1'");
}

/// THE CORE PROPERTY: an `ELEPHC_INI_*` override moves BOTH surfaces at once — the raw INI string
/// `ini_get()` reports and the normalized typed value in
/// `opcache_get_configuration()['directives']` — exactly as `-d` moves both in reference PHP.
///
/// The normalizations pinned here are byte-verified against reference PHP 8.5.6 with the matching
/// `-d` flag: `save_comments=0` → `false` / `'0'`; `jit_debug=1M` → `1048576` / `'1M'`;
/// `optimization_level=0x10` → `16` / `'0x10'`; `max_wasted_percentage=10` → `0.1` / `'10'`;
/// `jit_prof_threshold=0.5` → `0.5` / `'0.5'`; `lockfile_path=/tmp/o.log` → the path on both.
#[test]
fn underscore_spelling_moves_both_surfaces() {
    let (_dir, binary) = probe_binary("opcache_env_both", &[]);
    let out = run_with_env(
        &binary,
        &[
            ("ELEPHC_INI_opcache__save_comments", "0"),
            ("ELEPHC_INI_opcache__jit_debug", "1M"),
            ("ELEPHC_INI_opcache__optimization_level", "0x10"),
            ("ELEPHC_INI_opcache__max_wasted_percentage", "10"),
            ("ELEPHC_INI_opcache__jit_prof_threshold", "0.5"),
            ("ELEPHC_INI_opcache__lockfile_path", "/tmp/o.log"),
        ],
    );
    assert_eq!(line(&out, "cfg.save_comments"), "false");
    assert_eq!(line(&out, "ini.save_comments"), "'0'");
    assert_eq!(line(&out, "cfg.jit_debug"), "1048576");
    assert_eq!(line(&out, "ini.jit_debug"), "'1M'");
    assert_eq!(line(&out, "cfg.optimization_level"), "16");
    assert_eq!(line(&out, "ini.optimization_level"), "'0x10'");
    assert_eq!(line(&out, "cfg.max_wasted_percentage"), "0.1");
    assert_eq!(line(&out, "ini.max_wasted_percentage"), "'10'");
    assert_eq!(line(&out, "cfg.jit_prof_threshold"), "0.5");
    assert_eq!(line(&out, "ini.jit_prof_threshold"), "'0.5'");
    assert_eq!(line(&out, "cfg.lockfile_path"), "'/tmp/o.log'");
    assert_eq!(line(&out, "ini.lockfile_path"), "'/tmp/o.log'");
}

/// The DOTTED spelling (`ELEPHC_INI_opcache.save_comments`) is the secondary lookup. It exists
/// because the `__` form is the only one a POSIX shell can assign inline (`FOO.BAR=1 cmd` is a
/// syntax error), while the dotted form stays reachable through `env`, `putenv`, Docker `--env`
/// and systemd units. Both surfaces move for it too.
#[test]
fn dotted_spelling_is_the_secondary_lookup() {
    let (_dir, binary) = probe_binary("opcache_env_dotted", &[]);
    let out = run_with_env(&binary, &[("ELEPHC_INI_opcache.save_comments", "off")]);
    assert_eq!(line(&out, "cfg.save_comments"), "false");
    // `off` is rewritten to `''` by the INI scanner before any handler sees it.
    assert_eq!(line(&out, "ini.save_comments"), "''");
}

/// When both spellings are set the `__` form WINS, and the dotted one is consulted only when the
/// `__` form is unset or empty.
#[test]
fn underscore_spelling_wins_over_dotted() {
    let (_dir, binary) = probe_binary("opcache_env_precedence", &[]);

    let underscore_wins = run_with_env(
        &binary,
        &[
            ("ELEPHC_INI_opcache.save_comments", "1"),
            ("ELEPHC_INI_opcache__save_comments", "0"),
        ],
    );
    assert_eq!(line(&underscore_wins, "cfg.save_comments"), "false");
    assert_eq!(line(&underscore_wins, "ini.save_comments"), "'0'");

    // An EMPTY `__` form falls through to the dotted one.
    let falls_back = run_with_env(
        &binary,
        &[
            ("ELEPHC_INI_opcache.save_comments", "0"),
            ("ELEPHC_INI_opcache__save_comments", ""),
        ],
    );
    assert_eq!(line(&falls_back, "cfg.save_comments"), "false");
    assert_eq!(line(&falls_back, "ini.save_comments"), "'0'");
}

/// A malformed value falls back to the compile-time value ONLY where reference PHP's own handler
/// REFUSES the store. That is exactly one of the runtime type codes:
///
/// - `'b'` (bool) and `'i'` (int) CANNOT refuse — `zend_ini_parse_bool` answers `false` for
///   `garbage` and `zend_ini_parse_quantity` reads `12abc` as `12` — so the malformed value is
///   STORED and echoed, matching what `php -d` does.
/// - `'p'` (`opcache.max_wasted_percentage`) DOES refuse: `OnUpdateMaxWastedPercentage` rejects
///   anything outside `1..=50`, leaving the compiled value on both surfaces.
///
/// Keeping the runtime path aligned with the compile-time one is the point: a directive must not
/// report one value when set through `--ini` and another through `ELEPHC_INI_*`.
#[test]
fn invalid_env_value_falls_back_only_where_reference_refuses_the_store() {
    let (_dir, binary) = probe_binary("opcache_env_invalid", &[]);
    let out = run_with_env(
        &binary,
        &[
            ("ELEPHC_INI_opcache__save_comments", "garbage"),
            ("ELEPHC_INI_opcache__jit_debug", "12abc"),
            ("ELEPHC_INI_opcache__max_wasted_percentage", "99"),
        ],
    );
    // Stored, not ignored.
    assert_eq!(line(&out, "cfg.save_comments"), "false");
    assert_eq!(line(&out, "ini.save_comments"), "'garbage'");
    assert_eq!(line(&out, "cfg.jit_debug"), "12");
    assert_eq!(line(&out, "ini.jit_debug"), "'12abc'");
    // Refused: the percent handler is the one that can say no.
    assert_eq!(line(&out, "cfg.max_wasted_percentage"), "0.05");
    assert_eq!(line(&out, "ini.max_wasted_percentage"), "'5'");
}

/// THE HONESTY PROPERTY. `opcache.enable_cli` is EXCLUDED from runtime override because it bakes
/// the cache-enabled gate at compile time (`opcache_get_status`, `opcache_reset`,
/// `opcache_invalidate`, … all carry it as a literal). Setting its environment variable therefore
/// changes NOTHING: the directives array still reports `false`, `ini_get` still reports `'0'`, and
/// `opcache_get_status()` still returns `false` — no self-contradicting binary.
///
/// The same run also pins that an excluded STRING and INT directive stay put (`opcache.jit` and
/// `opcache.memory_consumption` are checked through the unchanged `status_is_array` line, which
/// only an honored `enable_cli` could flip).
#[test]
fn excluded_directive_is_ignored_on_both_surfaces() {
    let (_dir, binary) = probe_binary("opcache_env_excluded", &[]);
    let out = run_with_env(
        &binary,
        &[
            ("ELEPHC_INI_opcache__enable_cli", "1"),
            ("ELEPHC_INI_opcache.enable_cli", "1"),
            ("ELEPHC_INI_opcache__memory_consumption", "512"),
            ("ELEPHC_INI_opcache__jit", "tracing"),
        ],
    );
    assert_eq!(line(&out, "cfg.enable_cli"), "false");
    assert_eq!(line(&out, "ini.enable_cli"), "'0'");
    assert_eq!(
        line(&out, "status_is_array"),
        "false",
        "an ignored enable_cli override must leave opcache_get_status() disabled"
    );
    // A reporting-only directive in the SAME run still moves, so the exclusion is per-directive
    // and not a blanket switch-off of the mechanism.
    let both = run_with_env(
        &binary,
        &[
            ("ELEPHC_INI_opcache__enable_cli", "1"),
            ("ELEPHC_INI_opcache__save_comments", "0"),
        ],
    );
    assert_eq!(line(&both, "cfg.enable_cli"), "false");
    assert_eq!(line(&both, "status_is_array"), "false");
    assert_eq!(line(&both, "cfg.save_comments"), "false");
    assert_eq!(line(&both, "ini.save_comments"), "'0'");
}

/// Runtime env beats compile-time `--ini`, and `--ini` beats the baked default: the full
/// precedence chain baked default → `--ini` → `ELEPHC_INI_*`.
#[test]
fn env_overrides_the_compile_time_ini_flag() {
    let (_dir, binary) = probe_binary(
        "opcache_env_over_ini",
        &["--ini", "opcache.save_comments=1", "--ini", "opcache.jit_debug=4096"],
    );

    // No env: the `--ini` values are what the binary reports.
    let compiled = run_with_env(&binary, &[]);
    assert_eq!(line(&compiled, "cfg.save_comments"), "true");
    assert_eq!(line(&compiled, "cfg.jit_debug"), "4096");
    assert_eq!(line(&compiled, "ini.jit_debug"), "'4096'");

    // Env wins over `--ini` on both surfaces.
    let overridden = run_with_env(
        &binary,
        &[
            ("ELEPHC_INI_opcache__save_comments", "0"),
            ("ELEPHC_INI_opcache__jit_debug", "8K"),
        ],
    );
    assert_eq!(line(&overridden, "cfg.save_comments"), "false");
    assert_eq!(line(&overridden, "ini.save_comments"), "'0'");
    assert_eq!(line(&overridden, "cfg.jit_debug"), "8192");
    assert_eq!(line(&overridden, "ini.jit_debug"), "'8K'");

    // A MALFORMED env value does NOT fall back — `zend_ini_parse_quantity` cannot fail, so the
    // runtime normalizer stores what reference PHP would store (`nope` has no leading digits →
    // `0`) and reports the raw string verbatim, exactly as the compile-time `--ini` path does.
    // Only a directive whose handler can genuinely REFUSE a value (the `'p'` percent code) still
    // falls back; see `invalid_env_value_falls_back_only_where_reference_refuses_the_store`.
    let invalid = run_with_env(&binary, &[("ELEPHC_INI_opcache__jit_debug", "nope")]);
    assert_eq!(line(&invalid, "cfg.jit_debug"), "0");
    assert_eq!(line(&invalid, "ini.jit_debug"), "'nope'");
}

/// The integer normalizer reproduces `parse_ini_int` at runtime: plain decimal, `K`/`M`/`G` byte
/// suffixes, `0x`/`0X` hex, and a leading sign. Each row is what the equivalent
/// `php -d opcache.jit_debug=<v>` reports on reference PHP 8.5.6.
#[test]
fn int_normalizer_covers_every_ini_integer_form() {
    let (_dir, binary) = probe_binary("opcache_env_int_forms", &[]);
    for (raw, expected) in [
        ("100", "100"),
        ("+7", "7"),
        ("-5", "-5"),
        ("1K", "1024"),
        ("1k", "1024"),
        ("2M", "2097152"),
        ("1G", "1073741824"),
        ("0x10", "16"),
        ("0X1f", "31"),
        (" 8 ", "8"),
    ] {
        let out = run_with_env(&binary, &[("ELEPHC_INI_opcache__jit_debug", raw)]);
        assert_eq!(line(&out, "cfg.jit_debug"), expected, "raw {raw:?}");
        // The raw INI string is always the environment value VERBATIM, matching reference PHP,
        // where `-d opcache.jit_debug=1M` makes `ini_get` report `'1M'`.
        assert_eq!(
            line(&out, "ini.jit_debug"),
            format!("'{raw}'"),
            "raw {raw:?}"
        );
    }
    // Malformed integers are STORED, not ignored: `zend_ini_parse_quantity` has no rejection
    // path, so the value is its leading numeric prefix (or 0) and `ini_get` echoes the raw
    // string. Each row matches `php -d opcache.jit_debug=<v>` on reference PHP 8.5.6.
    for (raw, cfg) in [
        ("abc", "0"),
        ("1.9", "1"),
        ("0x", "0"),
        ("0xzz", "0"),
        ("12abc", "12"),
        ("08", "0"),
    ] {
        let out = run_with_env(&binary, &[("ELEPHC_INI_opcache__jit_debug", raw)]);
        assert_eq!(line(&out, "cfg.jit_debug"), cfg, "raw {raw:?}");
        assert_eq!(
            line(&out, "ini.jit_debug"),
            format!("'{raw}'"),
            "raw {raw:?}"
        );
    }
    // The one exception: an EMPTY environment value is indistinguishable from an unset one
    // (see the `EMPTY MEANS UNSET` note on `ENV_OVERRIDE_HELPERS`), so it keeps the baked value.
    let empty = run_with_env(&binary, &[("ELEPHC_INI_opcache__jit_debug", "")]);
    assert_eq!(line(&empty, "cfg.jit_debug"), "0");
    assert_eq!(line(&empty, "ini.jit_debug"), "'0'");
    // The scanner rewrite reaches the integer normalizer too.
    for (raw, cfg, ini) in [("on", "1", "'1'"), ("none", "0", "''")] {
        let out = run_with_env(&binary, &[("ELEPHC_INI_opcache__jit_debug", raw)]);
        assert_eq!(line(&out, "cfg.jit_debug"), cfg, "raw {raw:?}");
        assert_eq!(line(&out, "ini.jit_debug"), ini, "raw {raw:?}");
    }
}

/// The two float type codes behave differently, exactly as reference PHP 8.5.6 does.
///
/// `opcache.max_wasted_percentage` (`'p'`) is read with C `atoi` — an INTEGER truncation — and
/// refused outside `1..=50` (verified: `2.5` → `0.02`, NOT `0.025`; `3e1` → `0.03`; `0.1`, `0`,
/// `60` and `abc` all keep the default). `opcache.jit_prof_threshold` (`'f'`) is read with
/// `zend_strtod` LEADING-PREFIX semantics and NEVER fails (verified: `0.005x` → `0.005`,
/// `abc` → `0.0`, and `ini_get` reports the raw string verbatim in both).
#[test]
fn float_normalizers_follow_reference_semantics() {
    let (_dir, binary) = probe_binary("opcache_env_float_forms", &[]);

    // Percent: atoi truncation inside the range.
    for (raw, expected) in [
        ("1", "0.01"),
        ("2.5", "0.02"),
        ("1.9", "0.01"),
        ("3e1", "0.03"),
        ("2abc", "0.02"),
        ("50", "0.5"),
        ("50.9", "0.5"),
        ("+3", "0.03"),
    ] {
        let out = run_with_env(&binary, &[("ELEPHC_INI_opcache__max_wasted_percentage", raw)]);
        assert_eq!(
            line(&out, "cfg.max_wasted_percentage"),
            expected,
            "percent {raw:?}"
        );
        assert_eq!(
            line(&out, "ini.max_wasted_percentage"),
            format!("'{raw}'"),
            "percent {raw:?}"
        );
    }
    // Percent: out of range / no leading digits ⇒ ignored on both surfaces.
    for raw in ["0.1", "0", "60", "-5", "abc", "0x10"] {
        let out = run_with_env(&binary, &[("ELEPHC_INI_opcache__max_wasted_percentage", raw)]);
        assert_eq!(
            line(&out, "cfg.max_wasted_percentage"),
            "0.05",
            "percent {raw:?}"
        );
        assert_eq!(
            line(&out, "ini.max_wasted_percentage"),
            "'5'",
            "percent {raw:?}"
        );
    }

    // Plain float: leading-prefix, always stores.
    for (raw, expected) in [
        ("0.5", "0.5"),
        ("1e-3", "0.001"),
        ("3", "3.0"),
        ("-1", "-1.0"),
        ("0.005x", "0.005"),
        ("abc", "0.0"),
    ] {
        let out = run_with_env(&binary, &[("ELEPHC_INI_opcache__jit_prof_threshold", raw)]);
        assert_eq!(
            line(&out, "cfg.jit_prof_threshold"),
            expected,
            "float {raw:?}"
        );
        assert_eq!(
            line(&out, "ini.jit_prof_threshold"),
            format!("'{raw}'"),
            "float {raw:?}"
        );
    }
}

/// `ini_get_all` inherits the override through both of its projections (the flat one and the
/// `global_value`/`local_value` detail entries), because both route through the same
/// `__elephc_opcache_ini_string` dispatcher `ini_get` uses.
#[test]
fn ini_get_all_projections_inherit_the_override() {
    let dir = make_test_dir("opcache_env_get_all");
    let binary = compile(&dir, ALL_PROBE, "all", &[]);

    let baseline = run_with_env(&binary, &[]);
    assert_eq!(line(&baseline, "plain"), "'1'");
    assert_eq!(line(&baseline, "global"), "'1'");
    assert_eq!(line(&baseline, "local"), "'1'");

    let overridden = run_with_env(&binary, &[("ELEPHC_INI_opcache__save_comments", "0")]);
    assert_eq!(line(&overridden, "plain"), "'0'");
    assert_eq!(line(&overridden, "global"), "'0'");
    assert_eq!(line(&overridden, "local"), "'0'");
}

/// Bool spellings normalize case-insensitively to the same pair of values on both surfaces, and
/// the RAW string is the one PHP's INI SCANNER would have stored — which is NOT always the
/// spelling that was typed. `on`/`true`/`yes` are rewritten to `'1'` and
/// `off`/`false`/`no`/`none`/`null` to `''` before any handler runs, so those are what `ini_get`
/// reports (VERIFIED on reference PHP 8.5.6: `-d opcache.save_comments=on` → `ini_get` = `'1'`).
/// Everything else is reported verbatim. The compile-time `--ini` path does exactly the same.
#[test]
fn bool_normalizer_accepts_every_ini_spelling() {
    let (_dir, binary) = probe_binary("opcache_env_bool_forms", &[]);
    // (raw, cfg, ini) — `ini` is the scanner-rewritten string, not the raw spelling.
    for (raw, cfg, ini) in [
        ("1", "true", "'1'"),
        ("on", "true", "'1'"),
        ("On", "true", "'1'"),
        ("TRUE", "true", "'1'"),
        ("yes", "true", "'1'"),
        ("Yes", "true", "'1'"),
        // The `atoi` tail: truthy without being a recognized spelling.
        ("2", "true", "'2'"),
        ("-1", "true", "'-1'"),
        ("0", "false", "'0'"),
        ("off", "false", "''"),
        ("Off", "false", "''"),
        ("false", "false", "''"),
        ("no", "false", "''"),
        ("none", "false", "''"),
        ("null", "false", "''"),
        // `zend_ini_parse_bool` cannot fail: an unparseable value is FALSE, and it is STORED.
        ("garbage", "false", "'garbage'"),
    ] {
        let out = run_with_env(&binary, &[("ELEPHC_INI_opcache__save_comments", raw)]);
        assert_eq!(line(&out, "cfg.save_comments"), cfg, "{raw}");
        assert_eq!(line(&out, "ini.save_comments"), ini, "{raw}");
    }
}
