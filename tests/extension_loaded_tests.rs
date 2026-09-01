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
//! - The forced-bridge path uses `--with-pdo` (PDO); the auto-detected path uses
//!   `hash()` (links elephc_crypto -> the `hash` extension); the negative case links no
//!   bridge. Core extensions (json) always report loaded; a bridge with no linked
//!   staticlib (curl) never does.

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

/// Verifies a bridge-free program does not report BCMath merely because its names are known.
#[test]
fn unused_bcmath_reports_extension_not_loaded() {
    let dir = make_test_dir("ext_no_bcmath");
    let src = "<?php echo extension_loaded('bcmath') ? 'yes' : 'no';";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    assert_eq!(run_binary(&bin), "no");
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

/// Verifies a bridge-free program reports only the always-present core set: json loaded,
/// every bridge extension (PDO/hash/openssl) and the never-linked curl not loaded.
#[test]
fn bridge_free_program_reports_only_core_extensions() {
    let dir = make_test_dir("ext_core_only");
    let src = "<?php \
        var_dump(extension_loaded('json')); \
        var_dump(extension_loaded('PDO')); \
        var_dump(extension_loaded('hash')); \
        var_dump(extension_loaded('openssl')); \
        var_dump(extension_loaded('curl'));";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    let out = run_binary(&bin);
    assert_eq!(
        out,
        "bool(true)\nbool(false)\nbool(false)\nbool(false)\nbool(false)\n",
        "bridge-free: only core json loaded; no bridge extensions reported"
    );
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

/// Verifies the dynamic path consults the per-compilation linked-bridge set, not just the
/// core list: the SAME loop over variable names reports PDO only when `--with-pdo` linked it.
#[test]
fn dynamic_argument_tracks_linked_bridges() {
    let src = "<?php \
        foreach (['json', 'curl', 'PDO', 'pcre'] as $x) { \
            echo $x, '=', extension_loaded($x) ? 'T' : 'F', ' '; \
        }";

    let without = make_test_dir("ext_dyn_nopdo");
    let bin = compile_with_flags(&without, src, "app", &[]);
    assert_eq!(
        run_binary(&bin),
        "json=T curl=F PDO=F pcre=T ",
        "dynamic loop without --with-pdo: PDO not linked"
    );

    let with = make_test_dir("ext_dyn_pdo");
    let bin = compile_with_flags(&with, src, "app", &["--with-pdo"]);
    assert_eq!(
        run_binary(&bin),
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

/// Verifies a bridge-free program does not report iconv merely because its names are known.
#[test]
fn unused_iconv_reports_extension_not_loaded() {
    let dir = make_test_dir("ext_no_iconv");
    let src = "<?php echo extension_loaded('iconv') ? 'yes' : 'no';";
    let bin = compile_with_flags(&dir, src, "app", &[]);
    assert_eq!(run_binary(&bin), "no");
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

/// Verifies `get_extension_funcs("date")` preserves php-src's declaration order,
/// accepts case-insensitive names, and returns `false` for an unknown extension.
#[test]
fn get_extension_funcs_reports_exact_date_inventory() {
    let dir = make_test_dir("ext_date_functions");
    let src = r#"<?php
function extension_functions(string $name): array|false {
    return \GeT_ExTeNsIoN_FuNcS($name);
}
$functions = get_extension_funcs("date");
echo implode(",", $functions), "|";
echo get_extension_funcs("DATE") === $functions ? "L" : "l";
$dynamic = "Date";
echo extension_functions($dynamic) === $functions ? "D" : "d";
echo get_extension_funcs("elephc_missing_extension") === false ? "F" : "f";
$reflection = new ReflectionFunction("get_extension_funcs");
echo "|", $reflection->getParameters()[0]->getName(), ":", $reflection->getParameters()[0]->getType(), ":", $reflection->getReturnType();
"#;
    let bin = compile_with_flags(&dir, src, "app", &[]);
    assert_eq!(
        run_binary(&bin),
        "strtotime,date,idate,gmdate,mktime,gmmktime,checkdate,strftime,gmstrftime,time,localtime,getdate,\
date_create,date_create_immutable,date_create_from_format,date_create_immutable_from_format,date_parse,\
date_parse_from_format,date_get_last_errors,date_format,date_modify,date_add,date_sub,date_timezone_get,\
date_timezone_set,date_offset_get,date_diff,date_time_set,date_date_set,date_isodate_set,\
date_timestamp_set,date_timestamp_get,timezone_open,timezone_name_get,timezone_name_from_abbr,\
timezone_offset_get,timezone_transitions_get,timezone_location_get,timezone_identifiers_list,\
timezone_abbreviations_list,timezone_version_get,date_interval_create_from_date_string,\
date_interval_format,date_default_timezone_set,date_default_timezone_get,date_sunrise,date_sunset,\
date_sun_info|LDF|extension:string:array|false"
    );
}
