//! Purpose:
//! End-to-end tests for the `jit` sub-array of `opcache_get_status()`, baked by
//! `src/opcache_prelude.rs::render_jit_status` from the `opcache.jit` mode parser in
//! `src/opcache/directives.rs`. Covers the default (all-zero) shape, every accepted spelling
//! family (`tracing`/`on`, `function`, the CRTO 4-digit forms, the switched-off forms), the
//! invalid-spelling behavior, and the 8.2 per-version default.
//!
//! Called from:
//! - `cargo test --test opcache_jit_status_tests` through Rust's test harness.
//!
//! Key details:
//! - Every expectation here is PINNED FROM REFERENCE PHP, reproduced on this host with
//!   `php -d opcache.enable=1 -d opcache.enable_cli=1 -d opcache.jit_buffer_size=64M
//!   -d opcache.jit=<spelling> -r 'var_export(opcache_get_status()["jit"]);'` on Homebrew
//!   PHP 8.5.6 (Xdebug 3.5.0 loaded, which overrides `zend_execute_ex` and therefore puts the
//!   JIT in reference PHP's own "configured but unavailable" state) and on Homebrew PHP 8.2.31.
//! - THAT STATE IS THE POINT. An AOT-compiled elephc binary has no runtime JIT engine and no JIT
//!   buffer, so it is permanently in exactly that state: reference PHP keeps reporting the
//!   CONFIGURED `kind`/`opt_level`/`opt_flags` while reporting `enabled`/`on` false and both
//!   buffer figures 0. The same shape was reproduced WITHOUT Xdebug via
//!   `php -n -d opcache.jit=tracing -d opcache.jit_buffer_size=0`, confirming it is the generic
//!   unavailable shape and not an Xdebug artifact. See `render_jit_status` for the full write-up.
//! - `tracing` is the documented alias of the CRTO form `1254` and `function` of `1205`; the
//!   tests assert that identity directly rather than trusting two independent constants.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp dir,
//!   the same harness style as `opcache_ini_tests` / `opcache_restrict_api_tests`. Host-target
//!   only (macOS aarch64 local).
//! - The probe narrows with `is_array()` twice (once for the `array|false` status, once for the
//!   `jit` sub-array) because `opcache_get_status()`'s return hint is deliberately omitted so
//!   ordinary union return inference handles its two exits — see `GET_STATUS_TEMPLATE`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// The probe program: prints the seven `jit` keys, one per line, in reference key order.
const PROBE: &str = r#"<?php
$s = opcache_get_status();
if (is_array($s)) {
    $j = $s['jit'];
    if (is_array($j)) {
        echo 'enabled=', var_export($j['enabled'], true), "\n";
        echo 'on=', var_export($j['on'], true), "\n";
        echo 'kind=', $j['kind'], "\n";
        echo 'opt_level=', $j['opt_level'], "\n";
        echo 'opt_flags=', $j['opt_flags'], "\n";
        echo 'buffer_size=', $j['buffer_size'], "\n";
        echo 'buffer_free=', $j['buffer_free'], "\n";
    }
}
"#;

/// Builds the expected probe stdout for a `(kind, opt_level, opt_flags)` triple.
///
/// The other four keys are NOT parameters: `enabled`/`on`/`buffer_size`/`buffer_free` are the
/// documented always-unavailable clamp, so pinning them inside this helper is what makes every
/// test below assert the clamp as well as the mapping.
fn expected(kind: i64, opt_level: i64, opt_flags: i64) -> String {
    format!(
        "enabled=false\non=false\nkind={kind}\nopt_level={opt_level}\nopt_flags={opt_flags}\n\
         buffer_size=0\nbuffer_free=0\n"
    )
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

/// Compiles `PROBE` in `dir` with the supplied extra CLI arguments and returns the executable.
fn compile(dir: &Path, stem: &str, args: &[&str]) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, PROBE).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.arg("build");
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

/// Compiles and runs the probe with `--ini opcache.enable_cli=1` plus `extra`, returning stdout.
///
/// `opcache.enable_cli=1` is required for every case: a default CLI binary reports the cache
/// disabled and `opcache_get_status()` returns `false` before any array exists (matching
/// reference `php script.php`), so the `jit` sub-array would never be reached.
fn jit_status(dir_prefix: &str, extra: &[&str]) -> String {
    let dir = make_test_dir(dir_prefix);
    let mut args = vec!["--ini", "opcache.enable_cli=1"];
    args.extend_from_slice(extra);
    let bin = compile(&dir, "app", &args);
    let output = Command::new(&bin).output().expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// BASELINE: the default target (`opcache.jit = disable` on 8.5) reports the all-zero/false
/// array — byte-identical to reference PHP 8.5.6's own default and to what elephc emitted
/// before the mode parser existed. This is the regression anchor for "the default is unchanged".
#[test]
fn default_jit_status_is_all_zero() {
    assert_eq!(jit_status("opcache_jit_default", &[]), expected(0, 0, 0));
}

/// `--ini opcache.jit=tracing` reports the tracing triple (kind 5 / opt_level 4 / opt_flags 6)
/// with the unavailable clamp. Reference PHP 8.5.6 with the JIT unavailable reports exactly this
/// array for `-d opcache.jit=tracing`, including when a 64M buffer is configured.
#[test]
fn tracing_reports_reference_triple() {
    assert_eq!(
        jit_status("opcache_jit_tracing", &["--ini", "opcache.jit=tracing"]),
        expected(5, 4, 6)
    );
}

/// `on` is an alias of `tracing`, and `tracing` is an alias of the CRTO form `1254` — asserted
/// as an identity so the three cannot drift apart.
#[test]
fn tracing_aliases_agree() {
    let tracing = jit_status("opcache_jit_alias_tracing", &["--ini", "opcache.jit=tracing"]);
    let on = jit_status("opcache_jit_alias_on", &["--ini", "opcache.jit=on"]);
    let crto = jit_status("opcache_jit_alias_1254", &["--ini", "opcache.jit=1254"]);
    assert_eq!(on, tracing, "`on` must alias `tracing`");
    assert_eq!(crto, tracing, "`tracing` must alias the CRTO form 1254");
}

/// `--ini opcache.jit=function` reports kind 0 (`ZEND_JIT_ON_SCRIPT_LOAD`) / opt_level 5 /
/// opt_flags 6, and is the alias of the CRTO form `1205`.
#[test]
fn function_reports_reference_triple() {
    let function = jit_status("opcache_jit_function", &["--ini", "opcache.jit=function"]);
    assert_eq!(function, expected(0, 5, 6));
    assert_eq!(
        jit_status("opcache_jit_1205", &["--ini", "opcache.jit=1205"]),
        function,
        "`function` must alias the CRTO form 1205"
    );
}

/// A hand-written CRTO form decodes digit by digit: `T` → `kind`, `O` → `opt_level`, and
/// `opt_flags` = the `R` register-allocation bits OR'd with 4 when `C` is 1. `1111` exercises a
/// different value in every position (kind 1, opt_level 1, opt_flags LOCAL(1) | CPU(4) = 5) and
/// `1235` a third combination (kind 3, opt_level 5, opt_flags GLOBAL(2) | CPU(4) = 6).
#[test]
fn crto_forms_decode_per_digit() {
    assert_eq!(
        jit_status("opcache_jit_1111", &["--ini", "opcache.jit=1111"]),
        expected(1, 1, 5)
    );
    assert_eq!(
        jit_status("opcache_jit_1235", &["--ini", "opcache.jit=1235"]),
        expected(3, 5, 6)
    );
}

/// The switched-off spellings (`off`, `0`) report the all-zero array. In reference PHP they
/// differ from `disable` only in `enabled` (true vs false) — a distinction the unavailable clamp
/// erases, which is precisely what reference PHP itself does once the JIT is unavailable.
#[test]
fn switched_off_spellings_report_zero() {
    assert_eq!(
        jit_status("opcache_jit_off", &["--ini", "opcache.jit=off"]),
        expected(0, 0, 0)
    );
    assert_eq!(
        jit_status("opcache_jit_zero", &["--ini", "opcache.jit=0"]),
        expected(0, 0, 0)
    );
    assert_eq!(
        jit_status("opcache_jit_disable", &["--ini", "opcache.jit=disable"]),
        expected(0, 0, 0)
    );
}

/// An INVALID spelling falls back to the compiled default, exactly as reference PHP does (its
/// INI handler refuses the store and is re-invoked with the default). A non-numeric body assigns
/// nothing, so 8.5's `disable` default leaves the all-zero array.
///
/// A rejected NUMERIC body is different: reference PHP validates and assigns one digit at a time,
/// so the digits that passed are still visible afterwards. `2254` (rejected on `C = 2`) leaves
/// kind 5 / opt_level 4 / opt_flags 2, and `1355` (rejected on `R = 3`) leaves kind 5 /
/// opt_level 5 / opt_flags 0 — both pinned to reference PHP 8.5.6.
#[test]
fn invalid_spellings_match_reference_fallback() {
    assert_eq!(
        jit_status("opcache_jit_garbage", &["--ini", "opcache.jit=garbage"]),
        expected(0, 0, 0)
    );
    assert_eq!(
        jit_status("opcache_jit_9999", &["--ini", "opcache.jit=9999"]),
        expected(0, 0, 0)
    );
    assert_eq!(
        jit_status("opcache_jit_2254", &["--ini", "opcache.jit=2254"]),
        expected(5, 4, 2),
        "a numeric rejection keeps the digits that already passed"
    );
    assert_eq!(
        jit_status("opcache_jit_1355", &["--ini", "opcache.jit=1355"]),
        expected(5, 5, 0)
    );
}

/// An invalid `opcache.jit` is NOT stored, so `ini_get('opcache.jit')` and
/// `opcache_get_configuration()` both keep reporting the compiled default — `'disable'` on 8.5
/// and `'tracing'` on 8.2 (both verified on the matching reference build). A valid one is
/// reported verbatim.
#[test]
fn invalid_spelling_is_not_stored_in_the_ini_surface() {
    const INI_PROBE: &str = r#"<?php
echo 'ini=', var_export(ini_get('opcache.jit'), true), "\n";
$c = opcache_get_configuration();
if (is_array($c)) {
    $d = $c['directives'];
    if (is_array($d)) { echo 'cfg=', var_export($d['opcache.jit'], true), "\n"; }
}
"#;
    let probe = |prefix: &str, args: &[&str]| {
        let dir = make_test_dir(prefix);
        let php = dir.join("app.php");
        fs::write(&php, INI_PROBE).unwrap();
        let mut cmd = Command::new(elephc_bin());
        cmd.arg("build");
        cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
        cmd.current_dir(&dir);
        cmd.arg(&php);
        cmd.arg("--ini").arg("opcache.enable_cli=1");
        cmd.args(args);
        let out = cmd.output().expect("failed to spawn elephc");
        assert!(
            out.status.success(),
            "compile failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let run = Command::new(dir.join("app")).output().expect("failed to run binary");
        assert!(run.status.success());
        String::from_utf8_lossy(&run.stdout).into_owned()
    };

    assert_eq!(
        probe("opcache_jit_ini_bad", &["--ini", "opcache.jit=garbage"]),
        "ini='disable'\ncfg='disable'\n"
    );
    assert_eq!(
        probe("opcache_jit_ini_bad_num", &["--ini", "opcache.jit=1244"]),
        "ini='disable'\ncfg='disable'\n",
        "T=4 is the retired doc-comment trigger and is rejected"
    );
    assert_eq!(
        probe("opcache_jit_ini_good", &["--ini", "opcache.jit=1254"]),
        "ini='1254'\ncfg='1254'\n"
    );
    assert_eq!(
        probe(
            "opcache_jit_ini_82_bad",
            &["--php-version", "8.2", "--ini", "opcache.jit=garbage"]
        ),
        "ini='tracing'\ncfg='tracing'\n",
        "8.2's compiled default is `tracing`, not `disable`"
    );
}

/// PER-VERSION DEFAULT: 8.2/8.3 default `opcache.jit` to `tracing`, so their DEFAULT jit
/// sub-array carries the tracing triple under the clamp, while 8.4/8.5 default to `disable` and
/// stay all-zero. Pinned to reference PHP 8.2.31 with Xdebug loaded and its stock configuration,
/// which reports exactly `enabled=false, on=false, kind=5, opt_level=4, opt_flags=6,
/// buffer_size=0, buffer_free=0`.
#[test]
fn php82_default_reports_tracing_triple() {
    assert_eq!(
        jit_status("opcache_jit_v82", &["--php-version", "8.2"]),
        expected(5, 4, 6)
    );
    assert_eq!(
        jit_status("opcache_jit_v84", &["--php-version", "8.4"]),
        expected(0, 0, 0)
    );
}
