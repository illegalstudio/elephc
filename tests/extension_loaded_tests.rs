//! Purpose:
//! End-to-end tests that `extension_loaded()` / `get_loaded_extensions()` report the
//! bridge extensions actually linked into a given compilation: forced via
//! `--with-<flag>`, auto-detected from feature usage (e.g. `hash()` / `bcadd()`), or absent.
//!
//! Called from:
//! - `cargo test --test extension_loaded_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated
//!   temp dir, compile a plain executable, run it, and assert stdout — mirroring the
//!   web_tests / cdylib_tests harness. Host-target only (macOS aarch64 local).
//! - The forced-bridge paths use `--with-pdo` (PDO) and `--with-pcntl` (pcntl); the auto-detected path uses
//!   `hash()` (links elephc_crypto -> the `hash` extension); the negative case links no
//!   bridge. Core extensions (json) always report loaded; a bridge with no linked
//!   staticlib (curl) never does.
//! - THE CURL ASSERTIONS BELOW ARE THE NEGATIVE HALF, and they stay `false` on purpose:
//!   none of these programs mentions a `curl_*` name or passes `--with-curl`, so the
//!   bridge is not linked and `extension_loaded('curl')` must say so. The positive
//!   half — a program that calls `curl_init()` and reports curl LOADED — lives in
//!   `tests/codegen/curl/easy_handle.rs` rather than here, because compiling a curl
//!   program through this file's CLI subprocess would additionally require a managed
//!   native `curl` project (manifest + lock + installed artifacts) inside each test's
//!   isolated `XDG_CACHE_HOME`, which these tests deliberately keep empty. The codegen
//!   harness links the same bridge and seeds the same extension list
//!   (`support::runner::test_linked_extensions`, mirroring
//!   `pipeline::backend`'s use of `linker::php_extension_for_lib`), so the positive case
//!   is still exercised end to end on a machine with the packages installed.

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

/// Compiles `source` to a plain executable with extra compiler flags and returns its path.
fn compile_with_flags(dir: &Path, source: &str, stem: &str, flags: &[&str]) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.args(flags).arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Runs a compiled executable and returns its stdout as a string.
fn run_binary(bin: &Path) -> String {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// `ext/random` is always compiled in, so both introspection APIs report it without any
/// bridge flag or feature use.
#[test]
fn random_extension_is_always_loaded() {
    let dir = make_test_dir("ext_random");
    let src = "<?php \
        var_dump(extension_loaded('random')); \
        var_dump(in_array('random', get_loaded_extensions()));";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    let out = run_binary(&bin);
    assert_eq!(out, "bool(true)\nbool(true)\n");
}

/// Verifies a program compiled `--with-pdo` reports the PDO extension as loaded through
/// both `extension_loaded('PDO')` and `get_loaded_extensions()`, while an always-present
/// core extension (json) stays loaded and an unlinked bridge (curl) stays unloaded.
#[test]
fn with_pdo_reports_pdo_extension_loaded() {
    let dir = make_test_dir("ext_with_pdo");
    let src = "<?php \
        var_dump(extension_loaded('PDO')); \
        var_dump(in_array('PDO', get_loaded_extensions())); \
        var_dump(extension_loaded('json')); \
        var_dump(extension_loaded('curl'));";
    let bin = compile_with_flags(&dir, src, "app", &["--with-pdo"]);
    let out = run_binary(&bin);
    assert_eq!(
        out, "bool(true)\nbool(true)\nbool(true)\nbool(false)\n",
        "with --with-pdo: PDO loaded (both APIs), json loaded, curl not loaded"
    );
}

/// Verifies the same program compiled WITHOUT `--with-pdo` (and not using PDO) reports the
/// PDO extension as not loaded, confirming reporting tracks the actual link set.
#[test]
fn without_pdo_reports_pdo_extension_not_loaded() {
    let dir = make_test_dir("ext_no_pdo");
    let src = "<?php \
        var_dump(extension_loaded('PDO')); \
        var_dump(in_array('PDO', get_loaded_extensions())); \
        var_dump(extension_loaded('json')); \
        var_dump(extension_loaded('curl'));";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    let out = run_binary(&bin);
    assert_eq!(
        out, "bool(false)\nbool(false)\nbool(true)\nbool(false)\n",
        "without pdo: PDO not loaded (both APIs), json loaded, curl not loaded"
    );
}

/// Verifies a program that uses `hash()` auto-links the crypto bridge and reports its
/// canonical PHP extension name (`hash`) as loaded, while PDO (not linked here) stays
/// unloaded — exercising the feature-auto-detected `required_libraries` path.
#[test]
fn hash_usage_reports_hash_extension_loaded() {
    let dir = make_test_dir("ext_hash");
    let src = "<?php \
        echo hash('sha256', 'abc'), \"\\n\"; \
        var_dump(extension_loaded('hash')); \
        var_dump(in_array('hash', get_loaded_extensions())); \
        var_dump(extension_loaded('PDO'));";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    let out = run_binary(&bin);
    assert_eq!(
        out,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n\
         bool(true)\nbool(true)\nbool(false)\n",
        "using hash(): hash loaded (both APIs), PDO not loaded"
    );
}

/// Verifies a bridge-free program STILL reports BCMath, because elephc can provide every one of
/// php's 14 `bc*` functions to any program that asks for one.
///
/// This assertion was `no` until `extension_loaded()` was re-derived from the function surface.
/// The old answer read the LINK SET, which is circular for the question callers actually ask:
/// a program that gates `bcadd()` on `extension_loaded('bcmath')` never calls it, so the bridge
/// never links, so the gate stays false and the working implementation is unreachable. The answer
/// now describes what elephc CAN provide, which is also what `function_exists('bcadd')` has always
/// answered here — the two could not agree while this said `no`.
#[test]
fn unused_bcmath_still_reports_extension_loaded() {
    let dir = make_test_dir("ext_no_bcmath");
    let src = "<?php echo extension_loaded('bcmath') ? 'yes' : 'no', '|', \
        function_exists('bcadd') ? 'yes' : 'no';";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    assert_eq!(run_binary(&bin), "yes|yes");
}

/// Verifies using `bcadd()` auto-links the bridge and exposes its canonical extension name.
#[test]
fn bcadd_usage_reports_bcmath_extension_loaded() {
    let dir = make_test_dir("ext_bcmath_auto");
    let src = "<?php echo bcadd('1', '2'), '|', extension_loaded('bcmath') ? 'yes' : 'no';";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    assert_eq!(run_binary(&bin), "3|yes");
}

/// Verifies `--with-bcmath` reports the extension even when no BCMath function is called.
#[test]
fn with_bcmath_reports_extension_loaded() {
    let dir = make_test_dir("ext_bcmath_forced");
    let src = "<?php echo extension_loaded('bcmath') ? 'yes' : 'no';";
    let bin = compile_with_flags(&dir, src, "app", &["--with-bcmath"]);
    assert_eq!(run_binary(&bin), "yes");
}

/// Verifies a program that auto-detects PDO usage (no `--with-pdo`) reports the PDO
/// extension from the injected PHP surface — and does NOT report mysqli, even though
/// both surfaces link the same `elephc_pdo` archive. Guards the surface-based
/// reporting split: the linked archive alone must not imply any PHP extension.
#[test]
fn pdo_usage_still_reports_pdo_without_mysqli() {
    let dir = make_test_dir("ext_pdo_not_mysqli");
    let src = "<?php new PDO('sqlite::memory:'); \
        var_dump(extension_loaded('PDO')); \
        var_dump(extension_loaded('mysqli'));";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    let out = run_binary(&bin);
    assert_eq!(
        out, "bool(true)\nbool(false)\n",
        "PDO usage: PDO loaded from the injected surface, mysqli not loaded"
    );
}

/// Verifies a mysqli-only program reports mysqli — and not PDO — even though both
/// surfaces link the same `elephc_pdo` archive: reporting tracks the injected PHP
/// surface, not the staticlib.
#[test]
fn mysqli_usage_reports_mysqli_not_pdo() {
    let dir = make_test_dir("ext_mysqli_not_pdo");
    let src = "<?php new mysqli(); \
        var_dump(extension_loaded('mysqli')); \
        var_dump(extension_loaded('PDO')); \
        var_dump(extension_loaded('mysqlnd'));";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    let out = run_binary(&bin);
    assert_eq!(
        out, "bool(true)\nbool(false)\nbool(false)\n",
        "mysqli usage: mysqli loaded, PDO not loaded, mysqlnd never reported"
    );
}

/// Verifies `--with-mysqli` force-injects the mysqli surface for a program with no
/// static mysqli reference, without dragging the PDO classes or extension along.
#[test]
fn with_mysqli_force_injects_without_static_new() {
    let dir = make_test_dir("ext_with_mysqli");
    let src = "<?php var_dump(class_exists('mysqli')); \
        var_dump(extension_loaded('mysqli')); \
        var_dump(extension_loaded('PDO')); \
        var_dump(class_exists('PDO'));";
    let bin = compile_with_flags(&dir, src, "app", &["--with-mysqli"]);
    let out = run_binary(&bin);
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(false)\nbool(false)\n",
        "--with-mysqli: mysqli class + extension present, PDO absent"
    );
}

/// Verifies `--with-mysqli` roots the injected surface for reachability like
/// `--with-pdo` (forced prelude group): with only an `extension_loaded` probe in
/// the source — no static class/function reference and no dynamic hazard — the
/// mysqli classes and methods must survive the compiler's dead-code
/// elimination into the emitted code. (The LINKER may still strip functions
/// nothing in this particular binary references — that is per-binary and
/// reference-driven; the compiler-level keep is what `--with-mysqli`
/// guarantees, and what a program with any dynamic-lookup hazard relies on.)
#[test]
fn with_mysqli_forces_surface_past_reachability() {
    let dir = make_test_dir("ext_with_mysqli_dce");
    let src = "<?php var_dump(extension_loaded('mysqli'));";
    let bin = compile_with_flags(&dir, src, "app", &["--with-mysqli"]);
    let out = run_binary(&bin);
    assert_eq!(out, "bool(true)\n");
    let php = dir.join("asm.php");
    fs::write(&php, src).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(&dir);
    cmd.args(["--with-mysqli", "--emit-asm"]).arg(&php);
    let output = cmd.output().expect("failed to spawn elephc --emit-asm");
    assert!(
        output.status.success(),
        "elephc --emit-asm failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let asm = fs::read_to_string(dir.join("asm.s")).expect("emitted assembly missing");
    assert!(
        asm.contains("_method_mysqli_query"),
        "--with-mysqli: forced mysqli surface was dead-code-eliminated (mysqli::query missing from emitted code)"
    );
}

/// Verifies a program using BOTH surfaces reports both extensions (they share one
/// archive; the shared externs must be declared exactly once for this to compile).
#[test]
fn pdo_and_mysqli_usage_reports_both() {
    let dir = make_test_dir("ext_pdo_and_mysqli");
    let src = "<?php new PDO('sqlite::memory:'); new mysqli(); \
        var_dump(extension_loaded('PDO')); \
        var_dump(extension_loaded('mysqli'));";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    let out = run_binary(&bin);
    assert_eq!(
        out, "bool(true)\nbool(true)\n",
        "both surfaces: PDO and mysqli both loaded"
    );
}

/// Verifies a bridge-free program STILL reports PCNTL, and that the seven Linux-only names php
/// itself does not ship on macOS are not counted against it.
///
/// MEASURED on php 8.5.10 / macOS aarch64: php answers `extension_loaded('pcntl')` true while
/// `function_exists('pcntl_unshare')` is false, and elephc's macOS surface is missing exactly the
/// same seven names (`pcntl_unshare`, `pcntl_setns`, `pcntl_rfork`, `pcntl_sigtimedwait`,
/// `pcntl_sigwaitinfo`, `pcntl_getcpuaffinity`, `pcntl_setcpuaffinity`). Judging completeness
/// against php's LINUX function list would report the extension absent on the one target where the
/// two surfaces are identical, which is why `builtin_absent_by_php_platform_rule` exists.
#[test]
fn unused_pcntl_still_reports_extension_loaded() {
    let dir = make_test_dir("ext_no_pcntl");
    // `pcntl_unshare` is deliberately NOT asserted: it exists on Linux and not on macOS, in php
    // and in elephc alike, so pinning it here would make the test platform-specific for no gain.
    let src = "<?php echo extension_loaded('pcntl') ? 'yes' : 'no', '|', \
        function_exists('pcntl_fork') ? 'yes' : 'no';";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    assert_eq!(run_binary(&bin), "yes|yes");
}

/// Verifies `--with-pcntl` links the bridge and reports the canonical extension name.
#[test]
fn with_pcntl_reports_extension_loaded() {
    let dir = make_test_dir("ext_pcntl_forced");
    let src = "<?php echo extension_loaded('PCNTL') ? 'yes' : 'no';";
    let bin = compile_with_flags(&dir, src, "app", &["--with-pcntl"]);
    assert_eq!(run_binary(&bin), "yes");
}

/// Verifies extension-name matching is case-insensitive (PHP semantics): `--with-tls`
/// makes `extension_loaded('openssl')` and `extension_loaded('OpenSSL')` both report true.
#[test]
fn extension_name_matching_is_case_insensitive() {
    let dir = make_test_dir("ext_tls_case");
    let src = "<?php \
        var_dump(extension_loaded('openssl')); \
        var_dump(extension_loaded('OpenSSL')); \
        var_dump(in_array('openssl', get_loaded_extensions()));";
    let bin = compile_with_flags(&dir, src, "app", &["--with-tls"]);
    let out = run_binary(&bin);
    assert_eq!(
        out, "bool(true)\nbool(true)\nbool(true)\n",
        "with --with-tls: openssl loaded case-insensitively via both APIs"
    );
}

/// Verifies a bridge-free CLI program reports its built-in json/session modules while leaving
/// bridge-backed extensions (PDO/hash/openssl/curl) unloaded.
#[test]
fn bridge_free_program_reports_only_core_extensions() {
    let dir = make_test_dir("ext_core_only");
    let src = "<?php \
        var_dump(extension_loaded('json')); \
        var_dump(extension_loaded('session')); \
        var_dump(extension_loaded('PDO')); \
        var_dump(extension_loaded('hash')); \
        var_dump(extension_loaded('openssl')); \
        var_dump(extension_loaded('curl'));";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    let out = run_binary(&bin);
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\n",
        "CLI core json/session loaded; no bridge extensions reported"
    );
}

/// Verifies the PHP CLI profile exposes the built-in session extension even without HTTP mode.
/// PHP's session extension is present in this host's ordinary CLI and `php -n` profiles, so
/// `extension_loaded()`, `function_exists()`, and `get_loaded_extensions()` must agree.
#[test]
fn cli_profile_reports_session_extension_and_api_loaded() {
    let dir = make_test_dir("ext_session_cli");
    let src = "<?php \
        echo extension_loaded('session') ? 'loaded' : 'missing', '|', \
             function_exists('session_start') ? 'available' : 'missing', '|', \
             in_array('session', get_loaded_extensions()) ? 'listed' : 'missing';";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    assert_eq!(run_binary(&bin), "loaded|available|listed");
}

/// Verifies the real CLI session implementation starts in PHP_SESSION_NONE and persists data
/// through the file handler when restarted in the same process.
#[test]
fn cli_session_start_reads_and_writes_the_file_handler() {
    let dir = make_test_dir("session_cli_roundtrip");
    let src = r#"<?php
session_save_path(__DIR__);
session_id('elephc-cli-session');
echo session_status(), ':';
session_start();
$_SESSION['answer'] = 42;
session_write_close();
session_id('elephc-cli-session');
session_start();
echo session_status(), ':', $_SESSION['answer'];
session_destroy();
"#;
    let bin = compile_with_flags(&dir, src, "app", &[]);
    assert_eq!(run_binary(&bin), "1:2:42");
}

/// Verifies a NON-LITERAL argument resolves against the same effective extension set as a
/// literal, case-insensitively: the runtime string is compared to each baked candidate.
/// The empty string and an unlinked name must report false — negative controls for the
/// dynamic membership test, whose emitter shape had never executed from PHP before.
#[test]
fn dynamic_argument_matches_core_extensions() {
    let dir = make_test_dir("ext_dyn_core");
    let src = "<?php \
        $a = 'json'; $b = 'JSON'; $c = 'curl'; $d = ''; \
        $e = 'Zend OPcache'; $f = 'zend opcache'; \
        var_dump(extension_loaded($a)); \
        var_dump(extension_loaded($b)); \
        var_dump(extension_loaded($c)); \
        var_dump(extension_loaded($d)); \
        var_dump(extension_loaded($e)); \
        var_dump(extension_loaded($f));";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    let out = run_binary(&bin);
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(false)\nbool(false)\nbool(true)\nbool(true)\n",
        "dynamic arg: json/JSON true, curl false, empty false, both spellings of the \
         multi-word 'Zend OPcache' true"
    );
}

/// The probe shared by the two linked-bridge tests below: the SAME loop over variable names
/// must report PDO only when `--with-pdo` linked it.
const DYNAMIC_LINKED_BRIDGE_PROBE: &str = "<?php \
        foreach (['json', 'curl', 'PDO', 'pcre'] as $x) { \
            echo $x, '=', extension_loaded($x) ? 'T' : 'F', ' '; \
        }";

/// Verifies the dynamic path consults the per-compilation linked-bridge set, not just the
/// core list: without `--with-pdo`, PDO is not loaded.
///
/// One compile per test: the `--with-pdo` half links the whole PDO archive, and the two
/// compiles together ran past nextest's per-test budget on the macOS runners.
#[test]
fn dynamic_argument_reports_unlinked_pdo() {
    let without = make_test_dir("ext_dyn_nopdo");
    let bin = compile_with_flags(&without, DYNAMIC_LINKED_BRIDGE_PROBE, "app", &[]);
    assert_eq!(
        run_binary(&bin),
        "json=T curl=F PDO=F pcre=T ",
        "dynamic loop without --with-pdo: PDO not linked"
    );
}

/// Verifies the same dynamic loop flips PDO to loaded once `--with-pdo` links it, with every
/// other answer unchanged.
#[test]
fn dynamic_argument_tracks_linked_bridges() {
    let with = make_test_dir("ext_dyn_pdo");
    let compile_started = std::time::Instant::now();
    eprintln!("dynamic linked-bridge PDO compile started");
    let bin = compile_with_flags(&with, DYNAMIC_LINKED_BRIDGE_PROBE, "app", &["--with-pdo"]);
    eprintln!(
        "dynamic linked-bridge PDO compile completed in {:?}",
        compile_started.elapsed()
    );
    let run_started = std::time::Instant::now();
    let output = run_binary(&bin);
    eprintln!(
        "dynamic linked-bridge PDO run completed in {:?}",
        run_started.elapsed()
    );
    assert_eq!(
        output,
        "json=T curl=F PDO=T pcre=T ",
        "dynamic loop with --with-pdo: PDO flips to loaded, others unchanged"
    );
}

/// Verifies `get_loaded_extensions($flag)` accepts a NON-LITERAL flag: both candidate lists are
/// compile-time constants, so the emitted code selects between two baked arrays at runtime instead
/// of failing the compile with "argument must be a literal bool or int".
///
/// Reference PHP 8.5.6 with OPcache loaded prints exactly this sequence for the same probe:
/// `get_loaded_extensions(false)` lists both `json` and `Zend OPcache` (a Zend extension is also a
/// regular module), while `get_loaded_extensions(true)` lists `Zend OPcache` but not `json`. The
/// literal column proves the dynamic branch selects the same lists the fold does.
#[test]
fn get_loaded_extensions_accepts_a_dynamic_flag() {
    let dir = make_test_dir("ext_dyn_flag");
    let src = "<?php \
        $regular = false; $zend = true; \
        var_dump(in_array('json', get_loaded_extensions($regular))); \
        var_dump(in_array('Zend OPcache', get_loaded_extensions($regular))); \
        var_dump(in_array('json', get_loaded_extensions($zend))); \
        var_dump(in_array('Zend OPcache', get_loaded_extensions($zend))); \
        var_dump(in_array('json', get_loaded_extensions(false))); \
        var_dump(in_array('Zend OPcache', get_loaded_extensions(true)));";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    assert_eq!(
        run_binary(&bin),
        "bool(true)\nbool(true)\nbool(false)\nbool(true)\nbool(true)\nbool(true)\n",
        "a dynamic flag must select the same regular/Zend lists a literal flag folds to"
    );
}

/// Verifies the dynamic-flag branch still reports this compilation's linked bridges: a value that
/// only reaches the call site through a function parameter cannot be const-folded, so both arms
/// must be materialized and the false arm must still contain the `--with-pdo` bridge.
#[test]
fn get_loaded_extensions_dynamic_flag_tracks_linked_bridges() {
    let src = "<?php \
        function names(bool $zend): string { return implode(',', get_loaded_extensions($zend)); } \
        echo str_contains(names(false), 'PDO') ? 'T' : 'F'; \
        echo str_contains(names(true), 'PDO') ? 'T' : 'F'; \
        echo str_contains(names(true), 'Zend OPcache') ? 'T' : 'F';";

    let without = make_test_dir("ext_dyn_flag_nopdo");
    let bin = compile_with_flags(&without, src, "app", &[]);
    assert_eq!(
        run_binary(&bin),
        "FFT",
        "without --with-pdo neither list mentions PDO; the Zend list still has Zend OPcache"
    );

    let with = make_test_dir("ext_dyn_flag_pdo");
    let bin = compile_with_flags(&with, src, "app", &["--with-pdo"]);
    assert_eq!(
        run_binary(&bin),
        "TFT",
        "with --with-pdo the regular list gains PDO while the Zend list is unaffected"
    );
}

/// Verifies a bridge-free program STILL reports iconv: elephc declares all 10 of php's iconv
/// functions, so it can serve any caller that asks.
///
/// The measured defect this replaced: `extension_loaded('iconv')` answered `false` in the very
/// same program where `function_exists('iconv')` answered `true`. A library reading the first
/// ships its own slower replacement for a function elephc already has.
#[test]
fn unused_iconv_still_reports_extension_loaded() {
    let dir = make_test_dir("ext_no_iconv");
    let src = "<?php echo extension_loaded('iconv') ? 'yes' : 'no', '|', \
        function_exists('iconv') ? 'yes' : 'no';";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    assert_eq!(run_binary(&bin), "yes|yes");
}

/// Verifies calling `iconv_strlen()` auto-links the bridge and reports its extension name.
#[test]
fn iconv_usage_reports_iconv_extension_loaded() {
    let dir = make_test_dir("ext_iconv_auto");
    let src = "<?php echo iconv_strlen('abc'), '|', extension_loaded('iconv') ? 'yes' : 'no';";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    assert_eq!(run_binary(&bin), "3|yes");
}

/// Verifies `--with-iconv` reports the extension even when no iconv function is called.
#[test]
fn with_iconv_reports_extension_loaded() {
    let dir = make_test_dir("ext_iconv_forced");
    let src = "<?php echo extension_loaded('iconv') ? 'yes' : 'no';";
    let bin = compile_with_flags(&dir, src, "app", &["--with-iconv"]);
    assert_eq!(run_binary(&bin), "yes");
}

/// Recomputes [`elephc_builtin_contract::PhpModule::covers_php_function_surface`] from the shared
/// catalog and the vendored php-src baseline, and fails with the exact per-module diff when the
/// two disagree.
///
/// THIS IS THE TEETH ON `extension_loaded()`. The reported set is derived at compile time from
/// this one fact per module; the fact itself is a `const fn` because codegen needs it without a
/// JSON parser. That leaves exactly one way for the answer to rot: someone adds (or removes) a
/// catalog function and the completeness of its module changes without the `const fn` moving. So
/// this test rebuilds the verdict from the two sources that cannot lie — every `BuiltinContract`
/// in the catalog, and php 8.5.10's own snapshot in `scripts/docs/php_baseline.json` — and
/// compares module by module.
///
/// MUTATION-CHECKED: adding `Mbstring` to the `const fn`, or dropping `Iconv` from it, must fail
/// here. The message names the module and the missing functions so the fix is mechanical.
#[test]
fn extension_surface_completeness_matches_php_baseline() {
    use elephc_builtin_contract::PhpModule;
    use std::collections::{BTreeMap, BTreeSet};

    let baseline_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/docs/php_baseline.json");
    let raw = std::fs::read_to_string(&baseline_path)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", baseline_path.display()));
    let baseline: serde_json::Value =
        serde_json::from_str(&raw).expect("php_baseline.json must be valid JSON");
    let functions = baseline["functions"]
        .as_object()
        .expect("php_baseline.json must carry a flat functions map");

    // php's exported function names, keyed by the LOWERCASE module name `PhpModule::php_name`
    // uses, which is the key the snapshot itself is generated with.
    let mut php_by_module: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (function, module) in functions {
        let module = module.as_str().expect("baseline module names are strings");
        php_by_module
            .entry(module.to_ascii_lowercase())
            .or_default()
            .insert(function.to_ascii_lowercase());
    }

    let mut wrong: Vec<String> = Vec::new();
    for module in PhpModule::ALL.iter().copied() {
        if !module.is_php() {
            continue;
        }
        let php: BTreeSet<String> = php_by_module
            .get(module.php_name())
            .cloned()
            .unwrap_or_default();
        let elephc: BTreeSet<String> = elephc_builtin_contract::module_php_functions(module)
            .map(|contract| contract.name.to_ascii_lowercase())
            .collect();
        let missing: Vec<&String> = php.difference(&elephc).collect();

        // php exports no procedural functions for `Reflection`; elephc provides it as classes, so
        // the function comparison is vacuous and the module is complete by fiat. Every other
        // module needs php to actually export something AND elephc to declare all of it — the
        // second condition is what stops a module elephc has nothing for from counting as
        // "complete" because nothing is missing from nothing.
        let expected = if module == PhpModule::Reflection {
            true
        } else {
            !php.is_empty() && missing.is_empty()
        };
        if expected != module.covers_php_function_surface() {
            let detail = if missing.is_empty() {
                format!("elephc declares {} of php's {}", elephc.len(), php.len())
            } else {
                let shown: Vec<&str> = missing.iter().take(6).map(|name| name.as_str()).collect();
                format!(
                    "elephc declares {} of php's {}; missing {} incl. {:?}",
                    php.len() - missing.len(),
                    php.len(),
                    missing.len(),
                    shown
                )
            };
            wrong.push(format!(
                "{}: covers_php_function_surface() says {} but the catalog says {expected} ({detail})",
                module.php_name(),
                module.covers_php_function_surface()
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "PhpModule::covers_php_function_surface() disagrees with the catalog + php {} baseline:\n  {}",
        baseline["php_version"].as_str().unwrap_or("?"),
        wrong.join("\n  ")
    );

    // Anchors, so a future refactor that makes the loop vacuous still fails here.
    assert!(!PhpModule::Mbstring.covers_php_function_surface());
    assert!(!PhpModule::Ctype.covers_php_function_surface());
    assert!(PhpModule::Iconv.covers_php_function_surface());
    assert!(PhpModule::Json.covers_php_function_surface());
}

/// Verifies the two answers about one extension cannot disagree, on a program that names both.
///
/// `extension_loaded('mbstring')` used to be `true` while `mb_strtoupper` did not exist, which is
/// the shape every `symfony/polyfill-*`-style package reads to decide whether to declare its
/// replacements: told the extension is there, it declines, and the first call fatals. The pairing
/// below is the framework-free version of that — for each extension, a function elephc HAS and one
/// it does NOT — so the over-claim is visible without any library.
#[test]
fn partially_covered_extensions_report_not_loaded() {
    let dir = make_test_dir("ext_partial");
    let src = "<?php \
        var_dump(extension_loaded('mbstring'), function_exists('mb_strtoupper'), function_exists('mb_substr')); \
        var_dump(extension_loaded('ctype'), function_exists('ctype_alnum'), function_exists('ctype_upper')); \
        var_dump(extension_loaded('iconv'), function_exists('iconv'), function_exists('iconv_strlen'));";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    let out = run_binary(&bin);
    assert_eq!(
        out,
        // mbstring: 4 of php's 65, so not loaded even though `mb_strtoupper` is real.
        // ctype: 4 of php's 11, same shape.
        // iconv: 10 of php's 10, so loaded, with no gap behind it.
        "bool(false)\nbool(true)\nbool(false)\n\
         bool(false)\nbool(true)\nbool(false)\n\
         bool(true)\nbool(true)\nbool(true)\n",
        "a partially covered extension must report not-loaded; a complete one must report loaded"
    );
}

/// Verifies `extension_loaded()` and `get_loaded_extensions()` answer from one set.
///
/// They did not for `Zend OPcache` at one point in this change: its `opcache_*` contracts are
/// prelude declarations the pipeline injects on demand, so the catalog walk found them absent and
/// answered `false` while `get_loaded_extensions(true)` still listed the extension. Every name the
/// array reports must satisfy the membership test, both ways.
#[test]
fn listing_and_membership_agree_on_every_name() {
    let dir = make_test_dir("ext_agree");
    let src = "<?php \
        foreach (get_loaded_extensions() as $e) { if (!extension_loaded($e)) { echo 'listed-but-absent:', $e, \"\\n\"; } } \
        foreach (get_loaded_extensions(true) as $e) { if (!extension_loaded($e)) { echo 'zend-listed-but-absent:', $e, \"\\n\"; } } \
        echo 'ok', \"\\n\";";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    assert_eq!(run_binary(&bin), "ok\n");
}
