//! Purpose:
//! End-to-end tests for what `opcache_get_status()` reports about the RUNTIME SCRIPT CACHE —
//! the tier that holds files included at run time through the eval bridge, which the
//! compile-time manifest cannot contain.
//!
//! Called from:
//! - `cargo test --test opcache_runtime_cache_tests` through Rust's test harness.
//!
//! Key details:
//! - A runtime-dynamic include is a COMPILE ERROR at AOT top level, so every probe here
//!   performs its include from inside `eval()`. That is not an artifact of the test: it is
//!   the only way the tier is reachable at all.
//! - The `--ini opcache.enable_cli=1` binaries are the interesting ones. The DEFAULT CLI
//!   binary is pinned separately and must keep reporting `false` from
//!   `opcache_get_status()`, because its cache is disabled.
//! - The pay-for-use pin is the point of `a_binary_without_eval_reports_only_the_manifest`:
//!   a program with no eval bridge has no dynamic tier, and the lowering folds every
//!   bridge call to the empty-cache answer rather than linking the interpreter.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp
//!   dir, the same harness style as `opcache_manifest_tests`. Host-target only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes, CANONICALIZED
/// so the paths the probe builds match the spelling the cache stores.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
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

/// Compiles `<dir>/main.php` with the supplied `--ini` assignments and returns the executable.
fn compile(dir: &Path, ini: &[&str]) -> PathBuf {
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(dir.join("main.php"));
    for assignment in ini {
        cmd.arg("--ini").arg(assignment);
    }
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "compilation failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    dir.join("main")
}

/// Runs a compiled binary, asserting success, and returns its stdout.
fn run_binary(bin: &Path) -> String {
    let output = Command::new(bin).output().expect("failed to run binary");
    assert!(
        output.status.success(),
        "binary failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Returns the value of a `key=value` line the probe printed.
fn field<'a>(output: &'a str, key: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("probe printed no `{key}=` line; output was:\n{output}"))
}

/// Writes the dynamic-include fixture: an entry that includes a sibling from inside `eval()`.
fn write_dynamic_fixture(dir: &Path, probe: &str) {
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();
    fs::write(dir.join("main.php"), probe).unwrap();
}

/// The probe: include a dynamic file from `eval()`, then report what the status array says.
const STATUS_PROBE: &str = r#"<?php
eval('include __DIR__ . "/lib.php";');
$s = opcache_get_status();
if (!is_array($s)) {
    echo "status=false\n";
    return;
}
echo 'status=array', "\n";
echo 'num=', $s['opcache_statistics']['num_cached_scripts'], "\n";
echo 'keys=', $s['opcache_statistics']['num_cached_keys'], "\n";
echo 'hits=', $s['opcache_statistics']['hits'], "\n";
echo 'misses=', $s['opcache_statistics']['misses'], "\n";
echo 'rate=', $s['opcache_statistics']['opcache_hit_rate'], "\n";
echo 'scripts=', count($s['scripts']), "\n";
echo 'cached=', (opcache_is_script_cached(__DIR__ . '/lib.php') ? '1' : '0'), "\n";
$found = 0;
foreach ($s['scripts'] as $key => $entry) {
    if ($key === __DIR__ . '/lib.php') {
        $found = 1;
        echo 'entry_full_path=', $entry['full_path'], "\n";
        echo 'entry_keys=', count($entry), "\n";
        echo 'entry_ts_positive=', ($entry['timestamp'] > 0 ? '1' : '0'), "\n";
    }
}
echo 'found=', $found, "\n";
"#;

/// Verifies a dynamically included file appears in `opcache_get_status()` with live counters.
///
/// This is the whole point of the change: before it, the status array described only the
/// compile-time manifest and reported `hits`/`misses` as a permanent `0`, while the cache
/// behind `eval()`'s includes was actively serving them.
#[test]
fn a_dynamically_included_file_reaches_the_status_array() {
    let dir = make_test_dir("opcache_rt_status");
    write_dynamic_fixture(&dir, STATUS_PROBE);

    let output = run_binary(&compile(&dir, &["opcache.enable_cli=1"]));

    assert_eq!(field(&output, "status"), "array");
    // The entry file is the manifest's single member; the dynamic include adds one more.
    assert_eq!(field(&output, "num"), "2");
    assert_eq!(field(&output, "keys"), "2");
    assert_eq!(field(&output, "scripts"), "2");
    assert_eq!(field(&output, "found"), "1");
    assert_eq!(field(&output, "entry_full_path"), field(&output, "entry_full_path"));
    // The dynamic entry carries the reference 7-key shape on an 8.5 target.
    assert_eq!(field(&output, "entry_keys"), "7");
    assert_eq!(field(&output, "entry_ts_positive"), "1");
}

/// Verifies the miss from the first include is counted, and the hit from the second.
#[test]
fn the_counters_follow_the_actual_includes() {
    let dir = make_test_dir("opcache_rt_counters");
    write_dynamic_fixture(
        &dir,
        r#"<?php
eval('include __DIR__ . "/lib.php"; include __DIR__ . "/lib.php";');
$s = opcache_get_status();
echo 'hits=', $s['opcache_statistics']['hits'], "\n";
echo 'misses=', $s['opcache_statistics']['misses'], "\n";
echo 'rate=', $s['opcache_statistics']['opcache_hit_rate'], "\n";
"#,
    );

    let output = run_binary(&compile(&dir, &["opcache.enable_cli=1"]));

    // One cold fill, then one warm serve.
    assert_eq!(field(&output, "misses"), "1");
    assert_eq!(field(&output, "hits"), "1");
    // php-src reports the rate as a percentage of lookups: 1 hit of 2 lookups is 50.
    assert_eq!(field(&output, "rate"), "50");
}

/// Verifies the hit rate is `0`, not a division by zero, when nothing has been looked up.
#[test]
fn an_untouched_cache_reports_a_zero_hit_rate() {
    let dir = make_test_dir("opcache_rt_rate");
    write_dynamic_fixture(
        &dir,
        r#"<?php
eval('$x = 1;');
$s = opcache_get_status();
echo 'hits=', $s['opcache_statistics']['hits'], "\n";
echo 'misses=', $s['opcache_statistics']['misses'], "\n";
echo 'rate=', $s['opcache_statistics']['opcache_hit_rate'], "\n";
echo 'num=', $s['opcache_statistics']['num_cached_scripts'], "\n";
"#,
    );

    let output = run_binary(&compile(&dir, &["opcache.enable_cli=1"]));

    assert_eq!(field(&output, "hits"), "0");
    assert_eq!(field(&output, "misses"), "0");
    assert_eq!(field(&output, "rate"), "0");
    assert_eq!(field(&output, "num"), "1");
}

/// Verifies a program with no eval bridge reports exactly the manifest, and nothing else.
///
/// This is the pay-for-use pin. Such a binary has no dynamic tier, so every bridge call is
/// folded to the empty-cache answer at lowering time rather than linking the interpreter.
#[test]
fn a_binary_without_eval_reports_only_the_manifest() {
    let dir = make_test_dir("opcache_rt_noeval");
    write_dynamic_fixture(
        &dir,
        r#"<?php
$s = opcache_get_status();
echo 'num=', $s['opcache_statistics']['num_cached_scripts'], "\n";
echo 'hits=', $s['opcache_statistics']['hits'], "\n";
echo 'misses=', $s['opcache_statistics']['misses'], "\n";
echo 'rate=', $s['opcache_statistics']['opcache_hit_rate'], "\n";
echo 'full=', ($s['cache_full'] ? '1' : '0'), "\n";
echo 'scripts=', count($s['scripts']), "\n";
echo 'restarts=', $s['opcache_statistics']['manual_restarts'], "\n";
echo 'last=', $s['opcache_statistics']['last_restart_time'], "\n";
"#,
    );

    let output = run_binary(&compile(&dir, &["opcache.enable_cli=1"]));

    assert_eq!(field(&output, "num"), "1");
    assert_eq!(field(&output, "scripts"), "1");
    assert_eq!(field(&output, "hits"), "0");
    assert_eq!(field(&output, "misses"), "0");
    assert_eq!(field(&output, "rate"), "0");
    assert_eq!(field(&output, "full"), "0");
    assert_eq!(field(&output, "restarts"), "0");
    assert_eq!(field(&output, "last"), "0");
}

/// Verifies a default CLI binary still answers `false`, cache or no cache.
///
/// The whole runtime cache is gated on the cache-enabled state, so a default CLI build must
/// behave exactly as it did before this tier existed.
#[test]
fn a_default_cli_binary_still_reports_no_status() {
    let dir = make_test_dir("opcache_rt_default");
    write_dynamic_fixture(&dir, STATUS_PROBE);

    let output = run_binary(&compile(&dir, &[]));

    assert_eq!(field(&output, "status"), "false");
}

/// Verifies `opcache_reset()` from inside `eval()` shows up as a pending restart natively,
/// and that nothing else moves until the restart is actually performed.
///
/// The native latch and the runtime cache's latch are different objects; reporting only the
/// native one would deny a restart the cache has actually scheduled.
///
/// The other three figures are the DEFERRAL: php-src schedules the restart and performs it
/// at the next request, so within this one the cache keeps its entries and neither
/// `manual_restarts` nor `last_restart_time` moves. VERIFIED on reference PHP 8.5.10, which
/// reports exactly this shape. A CLI program is a single request and so never reaches the
/// boundary that performs it — correctly, since reference would restart at a next request
/// a CLI process does not have.
#[test]
fn a_reset_issued_from_eval_is_reported_natively() {
    let dir = make_test_dir("opcache_rt_reset");
    write_dynamic_fixture(
        &dir,
        r#"<?php
eval('include __DIR__ . "/lib.php"; opcache_reset();');
$s = opcache_get_status();
echo 'pending=', ($s['restart_pending'] ? '1' : '0'), "\n";
echo 'restarts=', $s['opcache_statistics']['manual_restarts'], "\n";
echo 'last_positive=', ($s['opcache_statistics']['last_restart_time'] > 0 ? '1' : '0'), "\n";
echo 'num=', $s['opcache_statistics']['num_cached_scripts'], "\n";
"#,
    );

    let output = run_binary(&compile(&dir, &["opcache.enable_cli=1"]));

    assert_eq!(field(&output, "pending"), "1", "the latch is the only thing that moves");
    assert_eq!(field(&output, "restarts"), "0", "counted at the restart, not the schedule");
    assert_eq!(field(&output, "last_positive"), "0");
    // The dynamic entry SURVIVES the schedule: manifest (1) plus the cached include (1).
    assert_eq!(field(&output, "num"), "2");
}

/// Verifies `opcache_get_status()` answers the same thing natively and from inside `eval()`.
///
/// It did not. The eval interpreter carries its own handler for the name and dispatched it
/// BEFORE consulting the program's own declarations, so the same call in the same binary
/// answered an array natively and `false` from inside `eval()`. The interpreter now prefers
/// the prelude's declaration when the binary carries one — which is the body that knows both
/// the compile-time manifest and the live cache — and only falls back to its handler for a
/// program that has no such declaration.
///
/// The assertion is EQUALITY between the two sides, not a fixed value: what matters is that
/// one binary cannot hold two answers.
#[test]
fn the_status_array_is_the_same_written_natively_and_inside_eval() {
    let dir = make_test_dir("opcache_rt_sides");
    write_dynamic_fixture(
        &dir,
        r#"<?php
eval('include __DIR__ . "/lib.php";');
$native = opcache_get_status();
echo 'native_type=', (is_array($native) ? 'array' : 'false'), "\n";
echo 'native_num=', $native['opcache_statistics']['num_cached_scripts'], "\n";
echo 'native_misses=', $native['opcache_statistics']['misses'], "\n";
echo 'native_scripts=', count($native['scripts']), "\n";
echo 'native_keys=', implode(',', array_keys($native)), "\n";
eval('
$inside = opcache_get_status();
echo "inside_type=", (is_array($inside) ? "array" : "false"), "\n";
echo "inside_num=", $inside["opcache_statistics"]["num_cached_scripts"], "\n";
echo "inside_misses=", $inside["opcache_statistics"]["misses"], "\n";
echo "inside_scripts=", count($inside["scripts"]), "\n";
echo "inside_keys=", implode(",", array_keys($inside)), "\n";
');
"#,
    );

    let output = run_binary(&compile(&dir, &["opcache.enable_cli=1"]));

    for key in ["type", "num", "misses", "scripts", "keys"] {
        assert_eq!(
            field(&output, &format!("native_{key}")),
            field(&output, &format!("inside_{key}")),
            "`{key}` disagrees between the native and the eval-written call"
        );
    }
    // And the shared answer is the live one, not the empty fallback.
    assert_eq!(field(&output, "native_type"), "array");
    assert_eq!(field(&output, "native_num"), "2");
}

/// The probe: include the same freshly written file twice from `eval()`, then report
/// whether the cache kept it. A stored entry makes the second include a HIT.
const FRESH_FILE_PROBE: &str = r#"<?php
eval('include __DIR__ . "/lib.php"; include __DIR__ . "/lib.php";');
$s = opcache_get_status();
echo 'hits=', $s['opcache_statistics']['hits'], "\n";
echo 'misses=', $s['opcache_statistics']['misses'], "\n";
"#;

/// Verifies `opcache.file_update_protection` refuses to CACHE a just-written file, while
/// still running it.
///
/// The window is set ENORMOUS rather than left at the default 2s, and that is what makes
/// this deterministic: with 2s the assertion depends on the binary starting within two
/// seconds of the fixture being written, which loses under parallel test load and fails for
/// a reason that has nothing to do with the directive. The exact boundary is pinned
/// precisely, and without a clock, by
/// `script_cache::config::tests::file_update_protection_refuses_only_younger_files`.
///
/// `lib.php` is still written AFTER the compile: compilation takes seconds, and a fixture
/// written before it would be measurably older than the test intends.
///
/// Both includes must therefore MISS, and the file must still run: refusing to cache is not
/// refusing to execute.
#[test]
fn a_freshly_written_file_is_run_but_not_cached() {
    let dir = make_test_dir("opcache_fup_fresh");
    write_dynamic_fixture(&dir, FRESH_FILE_PROBE);
    let binary = compile(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=100000"],
    );
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();

    let output = run_binary(&binary);

    // `hits`/`misses` are the only usable signal here: `opcache_is_script_cached()` called
    // natively answers from the frozen manifest, so it reports `false` for a dynamically
    // included file whether or not the runtime cache stored it.
    assert_eq!(
        field(&output, "hits"),
        "0",
        "a protected file must never hit; probe said:\n{output}"
    );
    assert_eq!(field(&output, "misses"), "2", "both includes miss");
}

/// Verifies `opcache.file_update_protection=0` disables the guard, so the identically fresh
/// fixture caches immediately and the second include hits.
///
/// This is the differential that proves the refusal above comes from the directive rather
/// than from the file being uncacheable for some other reason.
#[test]
fn zero_file_update_protection_caches_a_fresh_file() {
    let dir = make_test_dir("opcache_fup_off");
    write_dynamic_fixture(&dir, FRESH_FILE_PROBE);
    let binary = compile(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
    );
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();

    let output = run_binary(&binary);

    assert_eq!(
        field(&output, "hits"),
        "1",
        "the second include must hit; probe said:\n{output}"
    );
    assert_eq!(field(&output, "misses"), "1", "only the first include misses");
}
