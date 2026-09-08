//! Purpose:
//! End-to-end tests for the CLI `ini_get_all()` surface injected by
//! `src/opcache_prelude.rs`: the `$details` projection (both shapes), the SORTED key
//! order, and the php-src extension-filter dispatch (verbatim lowercase match, known
//! module with no directives → `[]`, unknown module → `E_WARNING` + `false`, `'core'` →
//! the unfiltered surface).
//!
//! Called from:
//! - `cargo test --test opcache_ini_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated
//!   temp dir, compile a plain executable, run it, and assert stdout/stderr — the same
//!   harness style as `extension_loaded_tests`. Host-target only (macOS aarch64 local).
//! - REGRESSION ANCHOR: `ini_get_all(null, false)` used to SIGSEGV (exit 139, no output)
//!   because one loop wrote an array literal on the `$details` branch and a scalar on the
//!   other into the same array slot. `plain_details_false_returns_flat_strings` is that
//!   repro; a regression shows up as a non-zero exit from `run_binary`.
//! - The probe programs narrow with `is_array()` before indexing/counting because
//!   `ini_get_all` is `array|false` (its return hint is deliberately omitted so ordinary
//!   union return inference handles the exits — see `CLI_INI_GET_ALL_TEMPLATE`). Sorted
//!   order is checked with an explicit `strcmp` walk rather than `sort()`, which does not
//!   accept a narrowed union element type on this branch. (`array_keys()` no longer has that
//!   restriction — it accepts a `mixed` argument and dispatches on the runtime tag; see
//!   `tests/array_result_type_tests.rs`. The `strcmp` walk is kept anyway because it pins the
//!   ORDER of the rendered key list, which a key-set comparison would not.)

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// The number of `opcache.*` directives the default (8.5) target registers.
const OPCACHE_DIRECTIVE_COUNT: usize = 54;

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

/// Compiles `source` to a plain executable and returns its path.
fn compile(dir: &Path, source: &str, stem: &str) -> PathBuf {
    compile_with_ini(dir, source, stem, &[]).0
}

/// Compiles `source` with the supplied `--ini KEY=VALUE` overrides, returning the executable
/// path together with the compiler's STDERR — which is where the OPcache quantity diagnostics
/// land (`crate::main::emit_ini_override_warnings`).
fn compile_with_ini(
    dir: &Path,
    source: &str,
    stem: &str,
    ini: &[(&str, &str)],
) -> (PathBuf, String) {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    for (key, value) in ini {
        cmd.arg("--ini").arg(format!("{key}={value}"));
    }
    cmd.arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    let raw_stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "elephc compile failed:\n{raw_stderr}"
    );
    (dir.join(stem), elephc_diagnostics(&raw_stderr))
}

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather than
/// anything elephc emitted: GNU `ld` reports the static-`getaddrinfo`/`gethostbyname` glibc
/// notes and the `.note.GNU-stack` deprecation, while Apple's linker stays silent. Those lines
/// start with `/usr/bin/ld:` or a `(.text.…)` section reference, so anchoring on elephc's own
/// line starts isolates its diagnostics — and still surfaces an UNEXPECTED elephc warning, which
/// an allow-list of known messages would have hidden.
///
/// elephc emits two prefixes: `Warning: …` for the INI-override diagnostics (`src/main.rs`) and
/// `warning: …` / `warning[line:col]: …` for compile warnings (`src/errors/report.rs`).
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("Warning: ")
                || line.starts_with("warning:")
                || line.starts_with("warning[")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A probe that prints `<directive>|<typed value>|<raw ini string>` for each requested key,
/// reading BOTH surfaces the way the reference probes in the docblocks below do.
fn two_surface_probe(keys: &[&str]) -> String {
    let mut src = String::from("<?php $d = opcache_get_configuration()['directives'];");
    for key in keys {
        src.push_str(&format!(
            " echo '{key}', '|', var_export($d['{key}'], true), '|', var_export(ini_get('{key}'), true), \"\\n\";"
        ));
    }
    src
}

/// Runs a compiled executable and returns `(stdout, stderr)`.
///
/// Asserts a clean exit first: the defect this file anchors was a SIGSEGV (exit 139) with
/// no output at all, so a status assertion is the load-bearing check, not the stdout compare.
fn run_binary(bin: &Path) -> (String, String) {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?} — 139 means the ini_get_all SIGSEGV is back):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Verifies CLI `--ini error_reporting=<PHP expression>` initializes the runtime
/// mask before user code executes, including named E_* constants and bitwise operators.
#[test]
fn cli_error_reporting_expression_initializes_runtime_mask() {
    let dir = make_test_dir("error_reporting_ini");
    let (bin, compile_stderr) = compile_with_ini(
        &dir,
        "<?php echo error_reporting();",
        "mask",
        &[("error_reporting", "E_ALL&~E_DEPRECATED")],
    );
    assert!(
        compile_stderr.is_empty(),
        "unexpected compile diagnostic: {compile_stderr}"
    );
    let (stdout, stderr) = run_binary(&bin);
    assert_eq!(stdout, "22527");
    assert_eq!(stderr, "");
}

/// Verifies CLI `--ini date.timezone` applies before user code and invalid values reproduce
/// PHP's startup warning while falling back to UTC.
#[test]
fn cli_date_timezone_initializes_runtime_and_warns_on_invalid_value() {
    let dir = make_test_dir("date_timezone_ini");
    let (valid_bin, valid_compile_stderr) = compile_with_ini(
        &dir,
        "<?php echo date_default_timezone_get();",
        "valid_tz",
        &[("date.timezone", "Europe/Paris")],
    );
    assert!(valid_compile_stderr.is_empty());
    let (valid_stdout, valid_stderr) = run_binary(&valid_bin);
    assert_eq!(valid_stdout, "Europe/Paris");
    assert_eq!(valid_stderr, "");

    let (invalid_bin, invalid_compile_stderr) = compile_with_ini(
        &dir,
        "<?php echo date_default_timezone_get();",
        "invalid_tz",
        &[("date.timezone", " Incorrect/Zone")],
    );
    assert!(invalid_compile_stderr.is_empty());
    let (invalid_stdout, invalid_stderr) = run_binary(&invalid_bin);
    assert_eq!(invalid_stdout, "UTC");
    assert_eq!(
        invalid_stderr,
        "\nWarning: PHP Startup: Invalid date.timezone value ' Incorrect/Zone', using 'UTC' instead in Unknown on line 0\n"
    );
}

/// `date.timezone` is the mutable Core/date directive that shares the CLI `ini_get`/`ini_set`
/// surface with the compile-time OPcache directives. Valid updates return the previous identifier;
/// invalid updates warn at the real call site, return `false`, preserve the previous value, and
/// remain suppressible with `@`.
#[test]
fn date_timezone_round_trip_and_invalid_warning_match_php_src() {
    let dir = make_test_dir("opcache_ini_date_timezone");
    let src = r#"<?php
echo ini_get("date.timezone"), "\n";
echo ini_set("date.timezone", "Europe/London"), "\n";
echo ini_get("date.timezone"), "\n";
var_dump(ini_set("date.timezone", "Mars/Valles_Marineris"));
@ini_set("date.timezone", "Incorrect/Zone");
echo ini_get("date.timezone"), "\n";
"#;
    let bin = compile(&dir, src, "app");
    let source = fs::canonicalize(dir.join("app.php")).unwrap();
    let (out, err) = run_binary(&bin);
    assert_eq!(
        out, "UTC\nUTC\nEurope/London\nbool(false)\nEurope/London\n",
        "date.timezone must expose php-src's mutable INI state and return values"
    );
    assert_eq!(
        err,
        format!(
            "\nWarning: ini_set(): Invalid date.timezone value 'Mars/Valles_Marineris', using 'Europe/London' instead in {} on line 5\n",
            source.display()
        ),
        "the invalid update must emit one suppression-aware warning at the source call site"
    );
}

/// REGRESSION ANCHOR for the `ini_get_all(null, false)` SIGSEGV.
///
/// `$details === false` must yield a FLAT map of raw INI strings (`'opcache.enable' => '1'`),
/// one entry per opcache directive. Before the two-single-shape-helpers split this program
/// exited 139 with no output, because the single projection loop wrote an array literal on the
/// `$details` branch and a scalar on the other into the same array slot.
#[test]
fn plain_details_false_returns_flat_strings() {
    let dir = make_test_dir("opcache_ini_plain");
    let src = "<?php \
        $all = ini_get_all(null, false); \
        if (is_array($all)) { \
            echo count($all), \"\\n\"; \
            echo $all['opcache.enable'], \"\\n\"; \
            echo $all['opcache.jit'], \"\\n\"; \
            echo $all['opcache.memory_consumption'], \"\\n\"; \
            echo $all['opcache.max_wasted_percentage'], \"\\n\"; \
        }";
    let bin = compile(&dir, src, "app");
    let (out, _) = run_binary(&bin);
    assert_eq!(
        out,
        format!("{OPCACHE_DIRECTIVE_COUNT}\n1\ndisable\n128\n5\n"),
        "ini_get_all(null, false) must return flat raw INI strings"
    );
}

/// `$details === true` (the default) yields `['global_value' => v, 'local_value' => v,
/// 'access' => N]` per entry, where `N` is the `PHP_INI_*` bitmask: `7` (PHP_INI_ALL) for
/// `opcache.enable`, `4` (PHP_INI_SYSTEM) for `opcache.memory_consumption`.
#[test]
fn default_details_returns_entry_arrays() {
    let dir = make_test_dir("opcache_ini_details");
    let src = "<?php \
        $all = ini_get_all(); \
        if (is_array($all)) { \
            echo count($all), \"\\n\"; \
            $e = $all['opcache.enable']; \
            echo $e['global_value'], '|', $e['local_value'], '|', $e['access'], \"\\n\"; \
            $m = $all['opcache.memory_consumption']; \
            echo $m['global_value'], '|', $m['local_value'], '|', $m['access'], \"\\n\"; \
        }";
    let bin = compile(&dir, src, "app");
    let (out, _) = run_binary(&bin);
    assert_eq!(
        out,
        format!("{OPCACHE_DIRECTIVE_COUNT}\n1|1|7\n128|128|4\n"),
        "ini_get_all() detail entries must carry global_value/local_value/access"
    );
}

/// `ini_get_all` reports its keys SORTED ASCENDING (reference PHP 8.5.6), unlike
/// `opcache_get_configuration()['directives']`, which uses registration order. The walk
/// compares each key against its predecessor with `strcmp` and also pins the first/last key,
/// so a re-ordering of the rendered key list fails here.
#[test]
fn keys_are_sorted_ascending() {
    let dir = make_test_dir("opcache_ini_sorted");
    let src = "<?php \
        $all = ini_get_all(null, false); \
        if (is_array($all)) { \
            $prev = ''; $ok = 1; $first = ''; $last = ''; \
            foreach ($all as $k => $v) { \
                $ks = (string) $k; \
                if ($first === '') { $first = $ks; } \
                if ($prev !== '' && strcmp($ks, $prev) <= 0) { $ok = 0; } \
                $prev = $ks; $last = $ks; \
            } \
            echo ($ok === 1 ? 'SORTED' : 'UNSORTED'), \"\\n\"; \
            echo $first, \"\\n\"; \
            echo $last, \"\\n\"; \
        }";
    let bin = compile(&dir, src, "app");
    let (out, _) = run_binary(&bin);
    assert_eq!(
        out, "SORTED\nopcache.blacklist_filename\nopcache.validate_timestamps\n",
        "ini_get_all keys must be sorted ascending"
    );
}

/// The extension filter matches VERBATIM against the lowercase module registry, with no case
/// folding (unlike `extension_loaded`, which IS case-insensitive — the two must not share a
/// comparison helper). So `'zend opcache'` selects the 54 opcache directives while
/// `'Zend OPcache'` — the spelling `get_loaded_extensions()` reports — is "not found":
/// `false` plus the verbatim `E_WARNING` text on stderr.
#[test]
fn extension_filter_matches_verbatim_lowercase() {
    let dir = make_test_dir("opcache_ini_verbatim");
    let src = "<?php \
        $hit = ini_get_all('zend opcache'); \
        if (is_array($hit)) { echo 'hit=', count($hit), \"\\n\"; } \
        echo 'cased=', (ini_get_all('Zend OPcache') === false ? 'false' : 'ARRAY'), \"\\n\"; \
        echo 'upper=', (ini_get_all('ZEND OPCACHE') === false ? 'false' : 'ARRAY'), \"\\n\"; \
        echo 'alias=', (ini_get_all('opcache') === false ? 'false' : 'ARRAY'), \"\\n\";";
    let bin = compile(&dir, src, "app");
    let (out, err) = run_binary(&bin);
    assert_eq!(
        out,
        format!("hit={OPCACHE_DIRECTIVE_COUNT}\ncased=false\nupper=false\nalias=false\n"),
        "only the verbatim lowercase module name matches"
    );
    assert!(
        err.contains("Warning: ini_get_all(): Extension \"Zend OPcache\" cannot be found\n"),
        "the not-found warning text must be verbatim: {err:?}"
    );
    assert!(
        err.contains("Warning: ini_get_all(): Extension \"opcache\" cannot be found\n"),
        "'opcache' is not an alias for the 'zend opcache' module: {err:?}"
    );
}

/// A KNOWN module that registers no INI directives yields an EMPTY ARRAY, not `false`
/// (verified on reference PHP 8.5.6 for spl/json/ctype/reflection). Only a module that is not
/// in the registry at all produces the warning + `false`. This is the distinction
/// `__elephc_ini_module_known` exists to draw.
#[test]
fn known_module_without_directives_returns_empty_array() {
    let dir = make_test_dir("opcache_ini_known");
    let src = "<?php \
        foreach (['json', 'spl', 'ctype', 'reflection', 'standard'] as $m) { \
            $r = ini_get_all($m); \
            echo $m, '=', (is_array($r) ? (string) count($r) : 'false'), \"\\n\"; \
        } \
        $missing = ini_get_all('no_such_module'); \
        echo 'no_such_module=', ($missing === false ? 'false' : 'ARRAY'), \"\\n\";";
    let bin = compile(&dir, src, "app");
    let (out, err) = run_binary(&bin);
    assert_eq!(
        out,
        "json=0\nspl=0\nctype=0\nreflection=0\nstandard=0\nno_such_module=false\n",
        "known-but-empty modules yield [], an unknown module yields false"
    );
    assert!(
        err.contains("Warning: ini_get_all(): Extension \"no_such_module\" cannot be found\n"),
        "only the unknown module warns: {err:?}"
    );
    assert!(
        !err.contains("\"json\""),
        "a known module must not warn: {err:?}"
    );
}

/// `'core'` maps to the UNFILTERED surface, reproducing php-src's rule that Core's
/// `module_number` is 0 so the per-module filter is skipped for it (reference PHP returns
/// every registered directive of every module for `ini_get_all('core')`).
///
/// DOCUMENTED DIVERGENCE: reference PHP's unfiltered surface is 403 entries on the reference
/// build because every loaded module contributes; elephc models only the directive blocks it
/// owns, so a CLI binary's unfiltered surface is the 54 opcache directives (87 under `--web`,
/// where the session block joins). The RULE is reproduced; the POPULATION is elephc's.
#[test]
fn core_selects_the_unfiltered_surface() {
    let dir = make_test_dir("opcache_ini_core");
    let src = "<?php \
        $core = ini_get_all('core', false); \
        $none = ini_get_all(null, false); \
        if (is_array($core) && is_array($none)) { \
            echo count($core), '=', count($none), \"\\n\"; \
            echo $core['opcache.enable'], \"\\n\"; \
        } \
        $core_d = ini_get_all('core'); \
        if (is_array($core_d)) { \
            $e = $core_d['opcache.enable']; \
            echo $e['access'], \"\\n\"; \
        }";
    let bin = compile(&dir, src, "app");
    let (out, err) = run_binary(&bin);
    assert_eq!(
        out,
        format!("{OPCACHE_DIRECTIVE_COUNT}={OPCACHE_DIRECTIVE_COUNT}\n1\n7\n"),
        "'core' must yield the same surface as the null (unfiltered) extension"
    );
    assert!(
        err.is_empty(),
        "'core' is a known module and must not warn: {err:?}"
    );
}

/// `zend_ini_parse_bool` HAS NO FAILURE PATH, so an unparseable `--ini` bool is stored as
/// `false` and echoed VERBATIM — it does not leave the compiled default in place.
///
/// Reference PHP 8.5.6, `php -d opcache.enable_cli=1 -d opcache.save_comments=<v> -r '...'`
/// reading `opcache_get_configuration()['directives']` and `ini_get()`:
///
/// | raw       | directives | `ini_get`   |
/// |-----------|------------|-------------|
/// | `garbage` | `false`    | `'garbage'` |
/// | `2`       | `true`     | `'2'`       |
/// | `-1`      | `true`     | `'-1'`      |
/// | `on`      | `true`     | `'1'`       |
/// | `none`    | `false`    | `''`        |
///
/// The `2` / `-1` rows are `zend_ini_parse_bool`'s `atoi` tail; the `on` / `none` rows are the
/// INI scanner's rewrite arriving BEFORE the handler.
#[test]
fn malformed_bool_override_is_stored_as_false_not_ignored() {
    let dir = make_test_dir("opcache_ini_bool_fix");
    let keys = [
        "opcache.save_comments",
        "opcache.use_cwd",
        "opcache.validate_permission",
        "opcache.dups_fix",
        "opcache.record_warnings",
    ];
    let (bin, stderr) = compile_with_ini(
        &dir,
        &two_surface_probe(&keys),
        "app",
        &[
            ("opcache.save_comments", "garbage"),
            ("opcache.use_cwd", "2"),
            ("opcache.validate_permission", "-1"),
            ("opcache.dups_fix", "on"),
            ("opcache.record_warnings", "none"),
        ],
    );
    let (out, _) = run_binary(&bin);
    assert_eq!(
        out,
        "opcache.save_comments|false|'garbage'\n\
         opcache.use_cwd|true|'2'\n\
         opcache.validate_permission|true|'-1'\n\
         opcache.dups_fix|true|'1'\n\
         opcache.record_warnings|false|''\n"
    );
    assert!(
        stderr.is_empty(),
        "a bool value never warns — there is no invalid bool: {stderr:?}"
    );
}

/// `zend_ini_parse_quantity` HAS NO FAILURE PATH EITHER: a malformed `--ini` quantity stores its
/// leading numeric prefix (or `0`), echoes the raw string, and WARNS.
///
/// Reference PHP 8.5.6, `php -d opcache.enable_cli=1 -d opcache.max_file_size=<v> -r '...'`:
///
/// | raw         | directives   | `ini_get`     | startup warning                          |
/// |-------------|--------------|---------------|------------------------------------------|
/// | `12abc`     | `12`         | `'12abc'`     | `unknown multiplier "c"`, as `"12"`       |
/// | `garbage`   | `0`          | `'garbage'`   | `no valid leading digits`, as `"0"`       |
/// | `0b101`     | `5`          | `'0b101'`     | —                                        |
/// | `12  M`     | `12582912`   | `'12  M'`     | —                                        |
/// | `12MM`      | `12582912`   | `'12MM'`      | trailing data, as `"12M"`                |
/// | `on`        | `1`          | `'1'`         | — (scanner rewrite reaches the quantity) |
#[test]
fn malformed_int_override_is_stored_and_warns() {
    let dir = make_test_dir("opcache_ini_int_fix");
    let keys = [
        "opcache.max_file_size",
        "opcache.file_update_protection",
        "opcache.jit_debug",
        "opcache.jit_bisect_limit",
        "opcache.jit_max_exit_counters",
        "opcache.log_verbosity_level",
    ];
    let (bin, stderr) = compile_with_ini(
        &dir,
        &two_surface_probe(&keys),
        "app",
        &[
            ("opcache.max_file_size", "12abc"),
            ("opcache.file_update_protection", "garbage"),
            ("opcache.jit_debug", "0b101"),
            ("opcache.jit_bisect_limit", "12  M"),
            ("opcache.jit_max_exit_counters", "12MM"),
            ("opcache.log_verbosity_level", "on"),
        ],
    );
    let (out, _) = run_binary(&bin);
    assert_eq!(
        out,
        "opcache.max_file_size|12|'12abc'\n\
         opcache.file_update_protection|0|'garbage'\n\
         opcache.jit_debug|5|'0b101'\n\
         opcache.jit_bisect_limit|12582912|'12  M'\n\
         opcache.jit_max_exit_counters|12582912|'12MM'\n\
         opcache.log_verbosity_level|1|'1'\n"
    );
    // The compile-time analogue of reference PHP's startup warnings. Byte-identical to
    // reference's text apart from the severity prefix and the `in Unknown on line 0` tail.
    // Emitted in DIRECTIVE REGISTRATION order, one line per malformed quantity. `0b101` and
    // `12  M` are CLEAN (binary literal; whitespace before a multiplier is legal) and `on` is
    // rewritten to `"1"` before the parser sees it, so none of those three warn.
    let warnings: Vec<&str> = stderr.lines().collect();
    assert_eq!(
        warnings,
        vec![
            "Warning: Invalid \"opcache.max_file_size\" setting. Invalid quantity \"12abc\": \
             unknown multiplier \"c\", interpreting as \"12\" for backwards compatibility",
            "Warning: Invalid \"opcache.file_update_protection\" setting. Invalid quantity \
             \"garbage\": no valid leading digits, interpreting as \"0\" for backwards compatibility",
            "Warning: Invalid \"opcache.jit_max_exit_counters\" setting. Invalid quantity \
             \"12MM\", interpreting as \"12M\" for backwards compatibility",
        ],
        "only the malformed quantities warn; the clean ones stay silent"
    );
}

/// PHP's INI SCANNER rewrites the boolean-alias barewords for EVERY directive, including plain
/// STRING ones that have no boolean handler anywhere on the path, and `ini_get()` reports the
/// REWRITTEN value.
///
/// Reference PHP 8.5.6: `-d opcache.preferred_memory_model=on` → `ini_get` = `'1'`;
/// `=none` → `''`; `-d opcache.jit=on` → `ini_get` = `'1'` while
/// `opcache_get_status()['jit']` still reports the TRACING triple (kind 5, opt_level 4,
/// opt_flags 6). That last pair is the ordering guarantee: scanner first, handler second.
#[test]
fn scanner_rewrites_bool_aliases_for_string_directives() {
    let dir = make_test_dir("opcache_ini_scanner_fix");
    let keys = [
        "opcache.preferred_memory_model",
        "opcache.blacklist_filename",
        "opcache.error_log",
        "opcache.preload_user",
        "opcache.file_cache",
    ];
    let (bin, stderr) = compile_with_ini(
        &dir,
        &two_surface_probe(&keys),
        "app",
        &[
            ("opcache.preferred_memory_model", "on"),
            ("opcache.blacklist_filename", "none"),
            ("opcache.error_log", "TRUE"),
            ("opcache.preload_user", "nOnE"),
            // Not an alias: passed through byte-for-byte, whitespace included.
            ("opcache.file_cache", "/var/cache"),
        ],
    );
    let (out, _) = run_binary(&bin);
    assert_eq!(
        out,
        "opcache.preferred_memory_model|'1'|'1'\n\
         opcache.blacklist_filename|''|''\n\
         opcache.error_log|'1'|'1'\n\
         opcache.preload_user|''|''\n\
         opcache.file_cache|'/var/cache'|'/var/cache'\n"
    );
    assert!(stderr.is_empty(), "string directives never warn: {stderr:?}");
}

/// The scanner rewrite must NOT disturb the `opcache.jit` MODE mapping: `on` reaches the handler
/// as `"1"`, which is one of the TRACING spellings, so `ini_get()` reports `'1'` while
/// `opcache_get_status()['jit']` still reports the tracing triple. VERIFIED on reference PHP
/// 8.5.6 with `-d opcache.jit_buffer_size=64M -d opcache.jit=on`.
#[test]
fn scanner_rewrite_preserves_the_jit_mode_mapping() {
    let dir = make_test_dir("opcache_ini_jit_alias");
    let src = "<?php \
        echo var_export(ini_get('opcache.jit'), true), \"\\n\"; \
        $s = opcache_get_status(); \
        if (is_array($s)) { \
            $j = $s['jit']; \
            echo $j['kind'], ',', $j['opt_level'], ',', $j['opt_flags'], \"\\n\"; \
        }";
    let (bin, _) = compile_with_ini(
        &dir,
        src,
        "app",
        &[
            ("opcache.enable_cli", "1"),
            ("opcache.jit_buffer_size", "64M"),
            ("opcache.jit", "on"),
        ],
    );
    let (out, _) = run_binary(&bin);
    assert_eq!(
        out, "'1'\n5,4,6\n",
        "ini_get reports the rewrite, the jit mode is still tracing"
    );
}

/// `opcache_get_status()['opcache_statistics']['start_time']` is the moment the cache started —
/// a FIXED point, identical on every call for the life of the process. VERIFIED on reference PHP
/// 8.5.6: two calls with `sleep(2)` between them report the SAME value.
///
/// REGRESSION ANCHOR: the template used to bake a literal `time()` into the array, which was
/// re-evaluated per call and made this probe report values two apart.
#[test]
fn start_time_is_stable_across_calls() {
    let dir = make_test_dir("opcache_ini_start_time");
    let src = "<?php \
        $a = opcache_get_status(false); \
        sleep(2); \
        $b = opcache_get_status(false); \
        if (is_array($a) && is_array($b)) { \
            $x = $a['opcache_statistics']['start_time']; \
            $y = $b['opcache_statistics']['start_time']; \
            echo ($x === $y) ? \"stable\\n\" : \"drift\\n\"; \
            echo ($x > 1700000000) ? \"sane\\n\" : \"insane\\n\"; \
        }";
    let (bin, _) = compile_with_ini(&dir, src, "app", &[("opcache.enable_cli", "1")]);
    let (out, _) = run_binary(&bin);
    assert_eq!(out, "stable\nsane\n");
}

/// THE RUST/PHP AGREEMENT PIN. Every override rule has TWO implementations — the compile-time
/// Rust one (`crate::opcache::directives`) and the runtime PHP one baked into the binary
/// (`ENV_OVERRIDE_HELPERS`) — and they must answer identically for the same raw string.
///
/// This drives the SAME value through both paths for the same directive and asserts the two
/// reports match: `--ini opcache.<d>=<v>` (Rust, compile time) against
/// `ELEPHC_INI_opcache__<d>=<v>` (PHP, run time) on a binary compiled with no override at all.
/// A divergence in the scanner rewrite, the bool `atoi` tail or the quantity base detection
/// shows up here as a mismatch rather than as a silent per-surface drift.
#[test]
fn rust_and_php_override_paths_agree() {
    let dir = make_test_dir("opcache_ini_agreement");
    // (directive, raw value) — one bool, one int, one string, covering scanner + both handlers.
    let cases = [
        ("opcache.save_comments", "garbage"),
        ("opcache.save_comments", "2"),
        ("opcache.save_comments", "on"),
        ("opcache.save_comments", "none"),
        ("opcache.max_file_size", "12abc"),
        ("opcache.max_file_size", "0b101"),
        ("opcache.max_file_size", "12MM"),
        ("opcache.max_file_size", "garbage"),
        ("opcache.max_file_size", "on"),
        ("opcache.error_log", "TRUE"),
        ("opcache.error_log", "/tmp/o.log"),
    ];
    // One binary with NO override: the environment path is what moves it.
    let (env_bin, _) = compile_with_ini(
        &dir,
        &two_surface_probe(&["opcache.save_comments", "opcache.max_file_size", "opcache.error_log"]),
        "envapp",
        &[],
    );
    for (index, (directive, raw)) in cases.iter().enumerate() {
        // Rust path: bake the value with `--ini`.
        let (ini_bin, _) = compile_with_ini(
            &dir,
            &two_surface_probe(&[directive]),
            &format!("iniapp{index}"),
            &[(directive, raw)],
        );
        let (ini_out, _) = run_binary(&ini_bin);

        // PHP path: the same value through `ELEPHC_INI_*` on the un-overridden binary.
        let env_name = format!("ELEPHC_INI_{}", directive.replace('.', "__"));
        let output = Command::new(&env_bin)
            .env(&env_name, raw)
            .output()
            .expect("failed to run compiled binary");
        assert!(output.status.success(), "env probe exited non-zero");
        let env_out = String::from_utf8_lossy(&output.stdout).into_owned();
        let env_line = env_out
            .lines()
            .find(|line| line.starts_with(&format!("{directive}|")))
            .unwrap_or_else(|| panic!("missing {directive} in env probe output"));

        assert_eq!(
            ini_out.trim_end(),
            env_line,
            "Rust (--ini) and PHP (ELEPHC_INI_*) disagree for {directive}={raw:?}"
        );
    }
}

/// `ini_get_all()` reports `opcache.file_cache`'s `global_value` / `local_value` as PHP `null`,
/// not `''` — the ONE directive of the 54 for which it does, in BOTH the `$details=true` and
/// `$details=false` surfaces.
///
/// REFERENCE (PHP 8.5.6, `php -d xdebug.mode=off -d opcache.enable=1 -d opcache.enable_cli=1`):
///
/// ```text
/// $a = ini_get_all();
/// $a['opcache.file_cache'] => ['global_value' => NULL, 'local_value' => NULL, 'access' => 4]
/// $a['opcache.error_log']  => ['global_value' => '',   'local_value' => '',   'access' => 4]
/// ini_get_all(null, false)['opcache.file_cache'] => NULL
/// ini_get('opcache.file_cache')                  => string(0) ""
/// ```
///
/// php-src registers it `STD_PHP_INI_ENTRY("opcache.file_cache", NULL, …)`; every other opcache
/// string directive defaults to `""`. `ini_get()` reports `''` in every case, so the NULL is
/// visible through `ini_get_all()` alone.
#[test]
fn file_cache_reports_null_values_in_ini_get_all() {
    let dir = make_test_dir("opcache_ini_null");
    let bin = compile(
        &dir,
        r#"<?php
$a = ini_get_all();
if (is_array($a)) {
    echo 'fc_g=', var_export($a['opcache.file_cache']['global_value'], true), "\n";
    echo 'fc_l=', var_export($a['opcache.file_cache']['local_value'], true), "\n";
    echo 'el_g=', var_export($a['opcache.error_log']['global_value'], true), "\n";
}
$p = ini_get_all(null, false);
if (is_array($p)) {
    echo 'plain_fc=', var_export($p['opcache.file_cache'], true), "\n";
    echo 'plain_el=', var_export($p['opcache.error_log'], true), "\n";
}
echo 'get=', var_export(ini_get('opcache.file_cache'), true), "\n";
"#,
        "nullapp",
    );
    let (stdout, _) = run_binary(&bin);
    assert!(stdout.contains("fc_g=NULL\n"), "global_value must be NULL:\n{stdout}");
    assert!(stdout.contains("fc_l=NULL\n"), "local_value must be NULL:\n{stdout}");
    assert!(stdout.contains("el_g=''\n"), "every other directive stays a string:\n{stdout}");
    assert!(stdout.contains("plain_fc=NULL\n"), "$details=false is NULL too:\n{stdout}");
    assert!(stdout.contains("plain_el=''\n"), "{stdout}");
    assert!(stdout.contains("get=''\n"), "ini_get() still reports '':\n{stdout}");
}

/// ASSIGNING `opcache.file_cache` — at compile time with `--ini`, or at run time through
/// `ELEPHC_INI_*` — makes `ini_get_all()` report the STRING, exactly as `-d` does in reference
/// PHP. The NULL means "never set", not "empty".
///
/// REFERENCE (PHP 8.5.6): `-d opcache.file_cache=/tmp/fcx` reports
/// `['global_value' => '/tmp/fcx', 'local_value' => '/tmp/fcx', 'access' => 4]`, and
/// `-d opcache.file_cache=` (the empty string) reports `''`/`''` — NOT NULL.
#[test]
fn assigning_file_cache_replaces_the_null_with_the_string() {
    let dir = make_test_dir("opcache_ini_null_set");
    let probe = r#"<?php
$a = ini_get_all();
if (is_array($a)) { echo 'fc=', var_export($a['opcache.file_cache']['global_value'], true), "\n"; }
"#;

    let (ini_bin, _) = compile_with_ini(
        &dir,
        probe,
        "nullsetapp",
        &[("opcache.file_cache", "/tmp/fcx")],
    );
    let (ini_out, _) = run_binary(&ini_bin);
    assert!(ini_out.contains("fc='/tmp/fcx'"), "--ini must assign it:\n{ini_out}");

    // The same directive re-pointed at RUN time on an un-overridden binary.
    let env_bin = compile(&dir, probe, "nullenvapp");
    let output = Command::new(&env_bin)
        .env("ELEPHC_INI_opcache__file_cache", "/tmp/fcx")
        .output()
        .expect("failed to run compiled binary");
    assert!(output.status.success(), "env probe exited non-zero");
    let env_out = String::from_utf8_lossy(&output.stdout);
    assert!(env_out.contains("fc='/tmp/fcx'"), "env override must assign it:\n{env_out}");
}

/// An OUT-OF-RANGE integer override is REFUSED, leaving the compiled default in BOTH surfaces,
/// and the ten JIT validators additionally print their verbatim reference warning at compile time.
///
/// REFERENCE (PHP 8.5.6, `-d opcache.enable=1 -d opcache.enable_cli=1 -d opcache.jit=tracing`),
/// reading `opcache_get_configuration()['directives']`:
///
/// | `-d`                                    | reported | stderr                                    |
/// |-----------------------------------------|----------|-------------------------------------------|
/// | `opcache.max_accelerated_files=199`     | `10000`  | (silent at the default verbosity)         |
/// | `opcache.max_accelerated_files=1000001` | `10000`  | (silent)                                  |
/// | `opcache.interned_strings_buffer=-1`    | `8`      | (silent)                                  |
/// | `opcache.interned_strings_buffer=32768` | `8`      | (silent)                                  |
/// | `opcache.jit_hot_func=256`              | `127`    | `… setting; using default value instead. Should be between 0 and 255` |
/// | `opcache.jit_max_loop_unrolls=10`       | `8`      | `… setting. Should be between 1 and 10`   |
/// | `opcache.jit_max_recursive_calls=10`    | `2`      | `… setting. Should be between 1 and 10`   |
/// | `opcache.jit_max_recursive_returns=4`   | `2`      | `… setting. Should be between 0 and 4`    |
/// | `opcache.jit_max_trace_length=3`        | `1024`   | `… setting. Should be between 4 and 1024` |
///
/// The last four rows are php-src's own off-by-one: the warning names a bound the handler then
/// rejects (`val < ZEND_JIT_TRACE_MAX_*`). The `ini_get` string still echoes the user's raw value
/// only when the store SUCCEEDS, so a refused override reports the DEFAULT string here.
#[test]
fn out_of_range_integer_overrides_revert_to_the_default() {
    let dir = make_test_dir("opcache_ini_range");
    let cases: [(&str, &str, &str, &str, Option<&str>); 9] = [
        ("opcache.max_accelerated_files", "199", "10000", "'10000'", None),
        ("opcache.max_accelerated_files", "1000001", "10000", "'10000'", None),
        ("opcache.interned_strings_buffer", "-1", "8", "'8'", None),
        ("opcache.interned_strings_buffer", "32768", "8", "'8'", None),
        (
            "opcache.jit_hot_func",
            "256",
            "127",
            "'127'",
            Some("Invalid \"opcache.jit_hot_func\" setting; using default value instead. Should be between 0 and 255"),
        ),
        (
            "opcache.jit_max_loop_unrolls",
            "10",
            "8",
            "'8'",
            Some("Invalid \"opcache.jit_max_loop_unrolls\" setting. Should be between 1 and 10"),
        ),
        (
            "opcache.jit_max_recursive_calls",
            "10",
            "2",
            "'2'",
            Some("Invalid \"opcache.jit_max_recursive_calls\" setting. Should be between 1 and 10"),
        ),
        (
            "opcache.jit_max_recursive_returns",
            "4",
            "2",
            "'2'",
            Some("Invalid \"opcache.jit_max_recursive_returns\" setting. Should be between 0 and 4"),
        ),
        (
            "opcache.jit_max_trace_length",
            "3",
            "1024",
            "'1024'",
            Some("Invalid \"opcache.jit_max_trace_length\" setting. Should be between 4 and 1024"),
        ),
    ];

    for (index, (directive, raw, typed, ini_string, warning)) in cases.iter().enumerate() {
        let (bin, compile_stderr) = compile_with_ini(
            &dir,
            &two_surface_probe(&[directive]),
            &format!("rangeapp{index}"),
            &[(directive, raw)],
        );
        let (stdout, _) = run_binary(&bin);
        assert_eq!(
            stdout.trim_end(),
            format!("{directive}|{typed}|{ini_string}"),
            "{directive}={raw} must revert to the compiled default on BOTH surfaces"
        );
        match warning {
            Some(body) => assert!(
                compile_stderr.contains(&format!("Warning: {body}")),
                "{directive}={raw} must print `Warning: {body}`, got:\n{compile_stderr}"
            ),
            None => assert!(
                compile_stderr.trim().is_empty(),
                "{directive}={raw} refuses SILENTLY at the default log_verbosity_level, got:\n{compile_stderr}"
            ),
        }
    }
}

/// The BOUNDARY values of every range-validated directive are ACCEPTED and stored, and a
/// malformed quantity on a JIT validator prints BOTH diagnostics in php-src's order.
///
/// REFERENCE (PHP 8.5.6): `-d opcache.max_accelerated_files=200` reports `200`,
/// `=1000000` reports `1000000`; `-d opcache.jit_max_loop_unrolls=9` reports `9`;
/// `-d opcache.jit_max_recursive_returns=3` reports `3`; `-d opcache.jit_max_trace_length=4`
/// reports `4`. And `-d opcache.jit_hot_func=999abc` prints, in this order:
///
/// ```text
/// Warning: Invalid "opcache.jit_hot_func" setting. Invalid quantity "999abc": unknown multiplier "c", interpreting as "999" for backwards compatibility in Unknown on line 0
/// Warning: Invalid "opcache.jit_hot_func" setting; using default value instead. Should be between 0 and 255 in Unknown on line 0
/// ```
///
/// because `zend_ini_parse_quantity_warn` runs BEFORE the range test.
#[test]
fn in_range_boundary_values_are_stored() {
    let dir = make_test_dir("opcache_ini_bounds");
    let cases: [(&str, &str); 6] = [
        ("opcache.max_accelerated_files", "200"),
        ("opcache.max_accelerated_files", "1000000"),
        ("opcache.jit_max_loop_unrolls", "9"),
        ("opcache.jit_max_recursive_returns", "3"),
        ("opcache.jit_max_trace_length", "4"),
        ("opcache.jit_hot_func", "255"),
    ];
    for (index, (directive, raw)) in cases.iter().enumerate() {
        let (bin, compile_stderr) = compile_with_ini(
            &dir,
            &two_surface_probe(&[directive]),
            &format!("boundapp{index}"),
            &[(directive, raw)],
        );
        let (stdout, _) = run_binary(&bin);
        assert_eq!(
            stdout.trim_end(),
            format!("{directive}|{raw}|'{raw}'"),
            "{directive}={raw} is IN range and must be stored"
        );
        assert!(
            compile_stderr.trim().is_empty(),
            "an in-range value warns not at all, got:\n{compile_stderr}"
        );
    }

    // Malformed AND out of range: the quantity diagnostic first, then the range one.
    let (_, both) = compile_with_ini(
        &dir,
        &two_surface_probe(&["opcache.jit_hot_func"]),
        "bothwarnapp",
        &[("opcache.jit_hot_func", "999abc")],
    );
    let quantity = both
        .find("Invalid quantity \"999abc\"")
        .expect("the quantity diagnostic must be printed");
    let range = both
        .find("Should be between 0 and 255")
        .expect("the range diagnostic must be printed");
    assert!(quantity < range, "php-src prints the quantity line first:\n{both}");
}

/// `opcache.max_accelerated_files` is `atoi`-read, not quantity-read: a `K` suffix or an `0x`
/// prefix is IGNORED, so both values fall below the 200 floor and the default stands.
///
/// REFERENCE (PHP 8.5.6): `-d opcache.max_accelerated_files=8K` and `=0x1000` BOTH report
/// `10000`, where the quantity parser would have stored 8192 and 4096. The same flag also prints
/// NO quantity diagnostic (unlike `-d opcache.max_file_size=12abc`, which does), because its
/// handler never reaches `zend_ini_parse_quantity_warn`.
#[test]
fn max_accelerated_files_ignores_quantity_suffixes() {
    let dir = make_test_dir("opcache_ini_maf_atoi");
    for (index, raw) in ["8K", "0x1000", "12abc"].iter().enumerate() {
        let (bin, compile_stderr) = compile_with_ini(
            &dir,
            &two_surface_probe(&["opcache.max_accelerated_files"]),
            &format!("atoiapp{index}"),
            &[("opcache.max_accelerated_files", raw)],
        );
        let (stdout, _) = run_binary(&bin);
        assert_eq!(
            stdout.trim_end(),
            "opcache.max_accelerated_files|10000|'10000'",
            "atoi reads no digits in range for {raw:?}"
        );
        assert!(
            compile_stderr.trim().is_empty(),
            "an atoi-read directive carries no quantity diagnostic, got:\n{compile_stderr}"
        );
    }
}

/// The 8.2 profile reports `opcache.jit_prof_threshold` as an INT (php-src 8.2 uses
/// `add_assoc_long` on a `double` field, which truncates); 8.3+ report it as a FLOAT. The RAW
/// `ini_get` string is `'0.005'` on every version.
///
/// REFERENCE — real PHP 8.2.31 (`/opt/homebrew/opt/php@8.2/bin/php -d xdebug.mode=off
/// -d opcache.enable=1 -d opcache.enable_cli=1`) against PHP 8.5.6:
///
/// | `-d opcache.jit_prof_threshold=` | 8.2.31    | 8.5.6          | `ini_get` (both) |
/// |----------------------------------|-----------|----------------|------------------|
/// | (default)                        | `int(0)`  | `float(0.005)` | `'0.005'`        |
/// | `2.7`                            | `int(2)`  | `float(2.7)`   | `'2.7'`          |
/// | `0.5`                            | `int(0)`  | `float(0.5)`   | `'0.5'`          |
/// | `-1.9`                           | `int(-1)` | `float(-1.9)`  | `'-1.9'`         |
/// | `0x10`                           | `int(0)`  | `float(0)`     | `'0x10'`         |
///
/// The `0x10 → 0` row proves the read is `zend_strtod`, not the quantity parser (which reads 16).
/// 8.3 cannot be probed on this host (`php@8.3` symlinks to 8.5) and is derived from php-src:
/// `PHP-8.3` already uses `add_assoc_double`, so it behaves like 8.4/8.5.
#[test]
fn jit_prof_threshold_is_an_int_in_the_82_profile() {
    let dir = make_test_dir("opcache_ini_prof82");
    let probe = two_surface_probe(&["opcache.jit_prof_threshold"]);

    for (version, expected) in [
        ("8.2", "opcache.jit_prof_threshold|0|'0.005'"),
        ("8.3", "opcache.jit_prof_threshold|0.005|'0.005'"),
        ("8.5", "opcache.jit_prof_threshold|0.005|'0.005'"),
    ] {
        let php = dir.join(format!("prof{}.php", version.replace('.', "")));
        fs::write(&php, &probe).unwrap();
        let output = Command::new(elephc_bin())
            .env("XDG_CACHE_HOME", dir.join("cache-root"))
            .current_dir(&dir)
            .arg("--php-version")
            .arg(version)
            .arg(&php)
            .output()
            .expect("failed to spawn elephc");
        assert!(
            output.status.success(),
            "elephc --php-version {version} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let bin = dir.join(format!("prof{}", version.replace('.', "")));
        let (stdout, _) = run_binary(&bin);
        assert_eq!(stdout.trim_end(), expected, "for --php-version {version}");
    }

    // The 8.2 TRUNCATION of an overridden value.
    for (index, (raw, expected)) in [("2.7", "2"), ("0.5", "0"), ("-1.9", "-1"), ("0x10", "0")]
        .iter()
        .enumerate()
    {
        let php = dir.join(format!("proft{index}.php"));
        fs::write(&php, &probe).unwrap();
        let output = Command::new(elephc_bin())
            .env("XDG_CACHE_HOME", dir.join("cache-root"))
            .current_dir(&dir)
            .arg("--php-version")
            .arg("8.2")
            .arg("--ini")
            .arg(format!("opcache.jit_prof_threshold={raw}"))
            .arg(&php)
            .output()
            .expect("failed to spawn elephc");
        assert!(output.status.success(), "elephc 8.2 --ini {raw} failed");
        let (stdout, _) = run_binary(&dir.join(format!("proft{index}")));
        assert_eq!(
            stdout.trim_end(),
            format!("opcache.jit_prof_threshold|{expected}|'{raw}'"),
            "8.2 truncates {raw} toward zero"
        );
    }
}
