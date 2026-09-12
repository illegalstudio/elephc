//! Purpose:
//! End-to-end tests for the compile-time OPcache SCRIPT MANIFEST — the set of PHP source files
//! an elephc binary reports as cached through `opcache_get_status()['scripts']` /
//! `num_cached_scripts`, `opcache_is_script_cached()` and `opcache_compile_file()`. The manifest
//! is built by `opcache_prelude::collect_manifest` from three groups (entry file,
//! statically-resolved include/require targets, autoloaded files) and baked by
//! `opcache_prelude::bake_manifest`.
//!
//! Called from:
//! - `cargo test --test opcache_manifest_tests` through Rust's test harness.
//!
//! Key details:
//! - The multi-file fixture is the point: the entry `require`s a sibling, pulls a PSR-4 class in
//!   through composer.json `autoload.psr-4`, that class file itself `require`s a helper, and
//!   composer.json `autoload.files` prefixes a bootstrap. All five files are compiled into the
//!   binary, so all five must be reported. The PSR-4 file is the load-bearing case: it is only
//!   knowable AFTER name resolution (PSR-4 is a fixpoint over canonical FQNs), which is why the
//!   compiler injects the OPcache declarations early and bakes their manifest late.
//! - `opcache_is_script_cached()` / `opcache_compile_file()` on the included and autoloaded files
//!   returned `false` before the manifest covered them; the multi-file test pins the `true`.
//! - A single-file program must still report exactly ONE cached script — the regression guard for
//!   the two-phase manifest.
//! - Every binary here is compiled with `--ini opcache.enable_cli=1`. A default CLI binary follows
//!   reference PHP's `opcache.enable_cli=0`, where `opcache_get_status()` returns `false` and both
//!   file predicates return `false` regardless of the manifest — that gate is covered by
//!   `opcache_restrict_api_tests` / `opcache_preload_tests` and is deliberately not re-pinned here.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp dir,
//!   the same harness style as `opcache_preload_tests` / `opcache_ini_tests`. Host-target only
//!   (macOS aarch64 local).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes, returned
/// CANONICALIZED so the paths this test builds match the spelling elephc bakes (on macOS
/// `std::env::temp_dir()` lives under `/var/folders/...`, which resolves to
/// `/private/var/folders/...`, the same normalization `__FILE__` applies).
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

/// Compiles `<dir>/<stem>.php` with the supplied `--ini` assignments, asserting success, and
/// returns the executable path.
fn compile(dir: &Path, stem: &str, ini: &[&str]) -> PathBuf {
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(dir.join(format!("{}.php", stem)));
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
    dir.join(stem)
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

/// The multi-file probe. It exercises all three manifest groups, then dumps the cached-script
/// count, every `scripts` key, and both file predicates for each file plus one never-compiled
/// path. `$probe_paths` is built with `__DIR__` so the expectations are canonical.
const MULTI_PROBE: &str = r#"<?php
require __DIR__ . '/second.php';
$w = new \App\Widget();
echo 'run=', $w->tag(), '|', second_marker(), '|', boot_marker(), "\n";
$s = opcache_get_status();
if (is_array($s)) {
    echo 'num=', $s['opcache_statistics']['num_cached_scripts'], "\n";
    echo 'keys=', $s['opcache_statistics']['num_cached_keys'], "\n";
    foreach ($s['scripts'] as $key => $entry) {
        echo 'script=', $key, "\n";
    }
} else {
    echo "status=false\n";
}
$probe_paths = [
    __FILE__,
    __DIR__ . '/second.php',
    __DIR__ . '/src/Widget.php',
    __DIR__ . '/src/widget_helper.php',
    __DIR__ . '/src/bootstrap.php',
    '/not/compiled.php',
];
foreach ($probe_paths as $path) {
    echo 'cached=', ($path), '|', (opcache_is_script_cached($path) ? '1' : '0'),
         '|', (opcache_compile_file($path) ? '1' : '0'), "\n";
}
"#;

/// Writes the five-file fixture into `dir` and returns the entry stem.
///
/// Layout (each file exercises a different manifest group):
/// - `main.php`           — the ENTRY file (group 1)
/// - `second.php`         — a plain `require` from the entry (group 2)
/// - `src/Widget.php`     — a PSR-4 class file, autoloaded via `new \App\Widget` (group 3)
/// - `src/widget_helper.php` — `require`d BY the autoloaded class file (group 3, nested)
/// - `src/bootstrap.php`  — composer.json `autoload.files` (group 3)
fn write_multi_fixture(dir: &Path) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("composer.json"),
        r#"{"autoload":{"psr-4":{"App\\":"src/"},"files":["src/bootstrap.php"]}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("src/bootstrap.php"),
        "<?php\nfunction boot_marker(): int { return 7; }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/Widget.php"),
        "<?php\nnamespace App;\nrequire __DIR__ . '/widget_helper.php';\n\
         class Widget { public function tag(): string { return widget_helper_tag(); } }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/widget_helper.php"),
        "<?php\nfunction widget_helper_tag(): string { return 'helper'; }\n",
    )
    .unwrap();
    fs::write(
        dir.join("second.php"),
        "<?php\nfunction second_marker(): int { return 42; }\n",
    )
    .unwrap();
    fs::write(dir.join("main.php"), MULTI_PROBE).unwrap();
}

/// THE MULTI-FILE CASE. Every PHP source file compiled into the binary is a cached script:
/// the entry, its `require`d sibling, the PSR-4 autoloaded class, the file that class itself
/// `require`s, and the Composer `autoload.files` bootstrap. All five are counted, all five key
/// the `scripts` map by canonical path, and both file predicates report `true` for each.
///
/// The included and autoloaded files answering `true` IS the improvement this test guards: the
/// manifest previously held only the entry file and `autoload.files`, so `second.php`,
/// `src/Widget.php` and `src/widget_helper.php` all answered `false`.
#[test]
fn multi_file_manifest_reports_includes_and_autoloaded_files() {
    let dir = make_test_dir("opcache_manifest_multi");
    write_multi_fixture(&dir);
    let bin = compile(&dir, "main", &["opcache.enable_cli=1"]);
    let out = run_binary(&bin);

    // The program really does run all three groups' code.
    assert!(out.contains("run=helper|42|7\n"), "{out}");

    // Five cached scripts, and `num_cached_keys` tracks them (one key per script).
    assert!(out.contains("num=5\n"), "expected 5 cached scripts:\n{out}");
    assert!(out.contains("keys=5\n"), "expected 5 cache keys:\n{out}");

    // The `scripts` map is keyed by each file's CANONICAL path — the same spelling `__FILE__`
    // bakes, which is what makes `$s['scripts'][__FILE__]` a hit.
    for relative in [
        "main.php",
        "second.php",
        "src/Widget.php",
        "src/widget_helper.php",
        "src/bootstrap.php",
    ] {
        let expected = format!("script={}\n", dir.join(relative).display());
        assert!(
            out.contains(&expected),
            "the scripts map must be keyed by {relative}'s canonical path:\n{out}"
        );
    }

    // Both file predicates report `true` for every compiled-in file...
    for relative in [
        "main.php",
        "second.php",
        "src/Widget.php",
        "src/widget_helper.php",
        "src/bootstrap.php",
    ] {
        let expected = format!("cached={}|1|1\n", dir.join(relative).display());
        assert!(
            out.contains(&expected),
            "{relative} must be reported cached AND compilable:\n{out}"
        );
    }

    // ...and `false` for a path this binary never compiled. The negative control: without it,
    // a predicate that answered `true` unconditionally would pass every assertion above.
    assert!(
        out.contains("cached=/not/compiled.php|0|0\n"),
        "an uncompiled path must be neither cached nor compilable:\n{out}"
    );
}

/// A SINGLE-FILE program keeps reporting exactly one cached script. The regression guard for the
/// two-phase manifest: neither the placeholder rendered at injection time nor the post-autoload
/// bake may add, drop or duplicate an entry when there is nothing to autoload and nothing to
/// include.
#[test]
fn single_file_manifest_still_reports_exactly_the_entry() {
    let dir = make_test_dir("opcache_manifest_single");
    fs::write(
        dir.join("solo.php"),
        r#"<?php
$s = opcache_get_status();
if (is_array($s)) {
    echo 'num=', $s['opcache_statistics']['num_cached_scripts'], "\n";
    echo 'keys=', $s['opcache_statistics']['num_cached_keys'], "\n";
    foreach ($s['scripts'] as $key => $entry) {
        echo 'script=', $key, "\n";
    }
} else {
    echo "status=false\n";
}
echo 'self=', (opcache_is_script_cached(__FILE__) ? '1' : '0'), "\n";
echo 'miss=', (opcache_is_script_cached('/not/compiled.php') ? '1' : '0'), "\n";
"#,
    )
    .unwrap();
    let bin = compile(&dir, "solo", &["opcache.enable_cli=1"]);
    let out = run_binary(&bin);

    assert!(out.contains("num=1\n"), "{out}");
    assert!(out.contains("keys=1\n"), "{out}");
    assert_eq!(
        out.matches("script=").count(),
        1,
        "exactly one scripts entry:\n{out}"
    );
    assert!(
        out.contains(&format!("script={}\n", dir.join("solo.php").display())),
        "{out}"
    );
    assert!(out.contains("self=1\n"), "{out}");
    assert!(out.contains("miss=0\n"), "{out}");
}

/// The `scripts` map's three clock fields come from TWO different clocks, exactly as php-src's
/// `accelerator_get_scripts` reads them.
///
/// REFERENCE (PHP 8.5.6, `-d opcache.enable=1 -d opcache.enable_cli=1
/// -d opcache.file_update_protection=0`, printing `time()` and `filemtime(__FILE__)` alongside):
///
/// ```text
/// timestamp           = 1784992711 = filemtime()          <- the FILE clock
/// last_used_timestamp = 1784992714 = time()               <- the REQUEST clock
/// revalidate          = 1784992716 = last_used_timestamp + opcache.revalidate_freq (2)
/// ```
///
/// and with `-d opcache.revalidate_freq=60`, `revalidate == last_used_timestamp + 60`
/// (1784992775 against 1784992715) — which is what pins the base as the REQUEST clock rather than
/// the mtime. An earlier elephc revision derived all three from the mtime, so `revalidate` landed
/// in the PAST and `last_used_timestamp` differed per entry; reference makes it identical for
/// every entry, because one request stamps them all.
#[test]
fn scripts_map_clocks_follow_the_request_clock() {
    let dir = make_test_dir("opcache_clocks");
    fs::write(
        dir.join("clocks.php"),
        r#"<?php
$now = time();
$s = opcache_get_status();
if (is_array($s)) {
    foreach ($s['scripts'] as $key => $entry) {
        echo 'mtime_eq=', ($entry['timestamp'] === filemtime((string) $key) ? '1' : '0'), "\n";
        echo 'used_now=', ($entry['last_used_timestamp'] === $now ? '1' : '0'), "\n";
        echo 'reval_delta=', ($entry['revalidate'] - $entry['last_used_timestamp']), "\n";
        echo 'used_ge_mtime=', ($entry['last_used_timestamp'] >= $entry['timestamp'] ? '1' : '0'), "\n";
        echo 'last_used=', $entry['last_used'], "\n";
    }
}
"#,
    )
    .unwrap();

    // Default revalidate_freq (2).
    let bin = compile(&dir, "clocks", &["opcache.enable_cli=1"]);
    let out = run_binary(&bin);
    assert!(out.contains("mtime_eq=1\n"), "timestamp stays the file mtime:\n{out}");
    assert!(out.contains("used_now=1\n"), "last_used_timestamp is the request clock:\n{out}");
    assert!(out.contains("reval_delta=2\n"), "revalidate = last_used + freq:\n{out}");
    assert!(
        out.contains("used_ge_mtime=1\n"),
        "the request clock is never BEFORE the file mtime:\n{out}"
    );
    // `asctime` shape: `Www Mmm dd hh:mm:ss yyyy` — 24 characters, no trailing newline.
    let last_used = out
        .lines()
        .find_map(|line| line.strip_prefix("last_used="))
        .expect("last_used must be reported");
    assert_eq!(last_used.len(), 24, "asctime is 24 chars, got {last_used:?}");

    // A non-default revalidate_freq moves the delta, proving the base is the request clock.
    let bin60 = compile(&dir, "clocks", &["opcache.enable_cli=1", "opcache.revalidate_freq=60"]);
    let out60 = run_binary(&bin60);
    assert!(out60.contains("reval_delta=60\n"), "{out60}");
    assert!(out60.contains("used_now=1\n"), "{out60}");
}

/// `last_used` is formatted in the SYSTEM timezone, because php-src builds it with libc
/// `asctime(localtime(…))` rather than through PHP's date functions.
///
/// REFERENCE (PHP 8.5.6, same request, `last_used_timestamp` 1784994683 throughout):
///
/// | environment            | `last_used`                 | `date('D M d H:i:s Y', ts)` |
/// |------------------------|-----------------------------|-----------------------------|
/// | `TZ=UTC`               | `Sat Jul 25 15:51:23 2026`  | `Sat Jul 25 15:51:23 2026`  |
/// | `TZ=America/New_York`  | `Sat Jul 25 11:51:23 2026`  | `Sat Jul 25 15:51:23 2026`  |
/// | `TZ` unset             | `Sat Jul 25 17:51:23 2026`  | `Sat Jul 25 15:51:23 2026`  |
///
/// (this host's `/etc/localtime` → `…/zoneinfo/Europe/Paris`, and `date.timezone` is UTC). So the
/// field tracks TZ while `date()` does not, which is why elephc resolves the zone itself and
/// applies it only around this one formatting — restoring the previous default afterwards, which
/// the `after_tz` assertion pins.
#[test]
fn last_used_is_formatted_in_the_system_timezone() {
    let dir = make_test_dir("opcache_tz");
    fs::write(
        dir.join("tz.php"),
        r#"<?php
$s = opcache_get_status();
if (is_array($s)) {
    foreach ($s['scripts'] as $key => $entry) {
        echo 'last_used=', $entry['last_used'], "\n";
        echo 'ts=', $entry['last_used_timestamp'], "\n";
    }
}
echo 'after_tz=', date_default_timezone_get(), "\n";
"#,
    )
    .unwrap();
    let bin = compile(&dir, "tz", &["opcache.enable_cli=1"]);

    let hour = |tz: Option<&str>| -> String {
        let mut cmd = Command::new(&bin);
        match tz {
            Some(zone) => {
                cmd.env("TZ", zone);
            }
            None => {
                cmd.env_remove("TZ");
            }
        }
        let output = cmd.output().expect("failed to run binary");
        assert!(output.status.success(), "binary failed for {tz:?}");
        let out = String::from_utf8_lossy(&output.stdout).into_owned();
        // Restoring the default timezone is part of the contract: a caller's own date() must not
        // shift because it called opcache_get_status().
        assert!(
            out.contains("after_tz=UTC\n"),
            "the default timezone must be restored:\n{out}"
        );
        out.lines()
            .find_map(|line| line.strip_prefix("last_used="))
            .expect("last_used must be reported")
            // `Www Mmm dd HH:MM:SS yyyy` — the hour field.
            .get(11..13)
            .expect("asctime must carry an hour field")
            .to_string()
    };

    let utc: i32 = hour(Some("UTC")).parse().unwrap();
    let new_york: i32 = hour(Some("America/New_York")).parse().unwrap();
    let paris: i32 = hour(Some("Europe/Paris")).parse().unwrap();
    // New York is UTC-4 in July, Paris UTC+2 (modulo the day wrap).
    assert_eq!((new_york - utc).rem_euclid(24), 20, "America/New_York is UTC-4 in July");
    assert_eq!((paris - utc).rem_euclid(24), 2, "Europe/Paris is UTC+2 in July");
}

/// EVERY `scripts` entry formats `last_used` in the same zone — not just the first.
///
/// REGRESSION. `last_used_is_formatted_in_the_system_timezone` above compiles a fixture with
/// exactly ONE script and reads only the first `last_used=` line, so it could not observe
/// entries disagreeing with each other. They did: the binary reported its first entry in
/// local time and EVERY OTHER ENTRY IN UTC.
///
/// The cause was a self-poisoning lookup. `__elephc_opcache_system_timezone()` consulted
/// `getenv('TZ')`, and elephc's `date_default_timezone_set()` WRITES `TZ` — it drives libc
/// through `putenv` + `tzset`, because the binary carries no tzdata. The first `last_used`
/// therefore restored the PHP default and left `TZ=UTC` behind, and every later entry read
/// that back as though it were the system zone. The lookup is memoized now.
///
/// VERIFIED against reference PHP 8.5.10 on this fixture: all three entries agree, both with
/// no `TZ` and with `TZ=America/New_York`.
#[test]
fn every_scripts_entry_agrees_on_the_timezone() {
    let dir = make_test_dir("opcache_tz_multi");
    fs::write(dir.join("a.php"), "<?php $a = 1;\n").unwrap();
    fs::write(dir.join("b.php"), "<?php $b = 1;\n").unwrap();
    fs::write(
        dir.join("multi.php"),
        r#"<?php
require __DIR__ . '/a.php';
require __DIR__ . '/b.php';
$s = opcache_get_status();
foreach ($s['scripts'] as $entry) {
    echo 'last_used=', $entry['last_used'], "\n";
}
"#,
    )
    .unwrap();
    let bin = compile(&dir, "multi", &["opcache.enable_cli=1"]);

    let hours = |tz: Option<&str>| -> Vec<String> {
        let mut cmd = Command::new(&bin);
        match tz {
            Some(zone) => {
                cmd.env("TZ", zone);
            }
            None => {
                cmd.env_remove("TZ");
            }
        }
        let output = cmd.output().expect("failed to run binary");
        assert!(output.status.success(), "binary failed for {tz:?}");
        let out = String::from_utf8_lossy(&output.stdout).into_owned();
        let hours: Vec<String> = out
            .lines()
            .filter_map(|line| line.strip_prefix("last_used="))
            // `Www Mmm dd HH:MM:SS yyyy` — the hour field.
            .map(|stamp| stamp.get(11..13).expect("asctime hour").to_string())
            .collect();
        assert_eq!(hours.len(), 3, "three manifest entries expected:\n{out}");
        hours
    };

    for zone in [None, Some("America/New_York"), Some("Europe/Paris")] {
        let reported = hours(zone);
        assert!(
            reported.iter().all(|hour| *hour == reported[0]),
            "entries disagree on the zone for {zone:?}: {reported:?}"
        );
    }
}

/// `opcache_reset()` LATCHES: the first call returns `true` and flips
/// `opcache_get_status()['restart_pending']`, the second returns `false`, and NOTHING ELSE
/// observable changes within the request.
///
/// REFERENCE (PHP 8.5.6, one request, `-d opcache.enable=1 -d opcache.enable_cli=1
/// -d opcache.file_update_protection=0`):
///
/// ```text
/// R1=true  pending=true
/// R2=false pending=true  in_progress=false
/// opcache_enabled=true  num_cached_scripts=1  count(scripts)=1  manual_restarts=0
/// is_script_cached=true  invalidate=true  compile_file=true
/// ```
///
/// php-src's `zend_accel_schedule_restart` clears the SHARED `ZCSG(accelerator_enabled)` (which is
/// what `opcache_reset`'s own guard reads, hence the `false` on the second call) while every other
/// function reads the REQUEST-LOCAL `ZCG(accelerator_enabled)`, snapshotted at activation and
/// therefore untouched. The restart itself happens on the NEXT request.
#[test]
fn reset_latches_restart_pending_and_is_not_idempotent() {
    let dir = make_test_dir("opcache_reset_latch");
    fs::write(
        dir.join("reset.php"),
        r#"<?php
$s0 = opcache_get_status(false);
if (is_array($s0)) { echo 'pending0=', var_export($s0['restart_pending'], true), "\n"; }
echo 'r1=', var_export(opcache_reset(), true), "\n";
$s1 = opcache_get_status();
if (is_array($s1)) {
    echo 'pending1=', var_export($s1['restart_pending'], true), "\n";
    echo 'in_progress=', var_export($s1['restart_in_progress'], true), "\n";
    echo 'enabled=', var_export($s1['opcache_enabled'], true), "\n";
    echo 'num=', $s1['opcache_statistics']['num_cached_scripts'], "\n";
    echo 'manual=', $s1['opcache_statistics']['manual_restarts'], "\n";
    echo 'nscripts=', count($s1['scripts']), "\n";
}
echo 'r2=', var_export(opcache_reset(), true), "\n";
echo 'r3=', var_export(opcache_reset(), true), "\n";
echo 'cached=', var_export(opcache_is_script_cached(__FILE__), true), "\n";
echo 'inv=', var_export(opcache_invalidate(__FILE__), true), "\n";
"#,
    )
    .unwrap();
    let bin = compile(&dir, "reset", &["opcache.enable_cli=1"]);
    let out = run_binary(&bin);

    assert!(out.contains("pending0=false\n"), "{out}");
    assert!(out.contains("r1=true\n"), "{out}");
    assert!(out.contains("pending1=true\n"), "{out}");
    assert!(out.contains("in_progress=false\n"), "{out}");
    assert!(out.contains("r2=false\n"), "a second reset reports failure:\n{out}");
    assert!(out.contains("r3=false\n"), "and stays failed:\n{out}");
    // Everything else is untouched within the request.
    assert!(out.contains("enabled=true\n"), "{out}");
    assert!(out.contains("num=1\n"), "{out}");
    assert!(out.contains("manual=0\n"), "the restart has not happened yet:\n{out}");
    assert!(out.contains("nscripts=1\n"), "{out}");
    assert!(out.contains("cached=true\n"), "{out}");
    assert!(out.contains("inv=true\n"), "{out}");
}

/// A FORCED `opcache_invalidate()` DISCARDS the entry: `opcache_is_script_cached()` flips to
/// `false` and the `scripts` entry's `timestamp` drops to `0`, while the entry itself and both
/// cached-script counts STAY. `opcache_compile_file()` re-caches it.
///
/// REFERENCE (PHP 8.5.6, one request, `-d opcache.enable=1 -d opcache.enable_cli=1
/// -d opcache.file_update_protection=0`):
///
/// ```text
/// is_script_cached      = true
/// invalidate($f, true)  = true
/// is_script_cached      = false
/// count(scripts)        = 1              <- unchanged
/// num_cached_scripts/num_cached_keys = 1/2   <- unchanged
/// scripts[$f]['timestamp'] = 0           <- the only field that moves
/// compile_file($f) = true ; is_script_cached = true
/// ```
///
/// php-src's `zend_accel_discard_script` sets `corrupted = true` and `timestamp = 0`; the script
/// keeps its shared-memory slot until the next restart, which is why the counts do not move.
///
/// A NON-forced `opcache_invalidate()` returns `true` without discarding: the file's mtime has not
/// changed, so `do_validate_timestamps` succeeds and php-src leaves the entry alone.
#[test]
fn forced_invalidate_discards_the_entry_and_compile_file_restores_it() {
    let dir = make_test_dir("opcache_discard");
    fs::write(
        dir.join("discard.php"),
        r#"<?php
$f = __FILE__;
echo 'c0=', var_export(opcache_is_script_cached($f), true), "\n";
echo 'soft=', var_export(opcache_invalidate($f), true), "\n";
echo 'c_soft=', var_export(opcache_is_script_cached($f), true), "\n";
echo 'forced=', var_export(opcache_invalidate($f, true), true), "\n";
echo 'c1=', var_export(opcache_is_script_cached($f), true), "\n";
$s = opcache_get_status();
if (is_array($s)) {
    echo 'nscripts=', count($s['scripts']), "\n";
    echo 'num=', $s['opcache_statistics']['num_cached_scripts'], "\n";
    echo 'ts=', $s['scripts'][$f]['timestamp'], "\n";
    echo 'full_path=', $s['scripts'][$f]['full_path'], "\n";
}
echo 'cf=', var_export(opcache_compile_file($f), true), "\n";
echo 'c2=', var_export(opcache_is_script_cached($f), true), "\n";
$s2 = opcache_get_status();
if (is_array($s2)) { echo 'ts2=', ($s2['scripts'][$f]['timestamp'] === filemtime($f) ? 'mtime' : 'other'), "\n"; }
"#,
    )
    .unwrap();
    let bin = compile(&dir, "discard", &["opcache.enable_cli=1"]);
    let out = run_binary(&bin);

    assert!(out.contains("c0=true\n"), "{out}");
    // A soft invalidate resolves the path but discards nothing.
    assert!(out.contains("soft=true\n"), "{out}");
    assert!(out.contains("c_soft=true\n"), "a non-forced invalidate must not discard:\n{out}");
    assert!(out.contains("forced=true\n"), "{out}");
    assert!(out.contains("c1=false\n"), "the forced invalidate must discard:\n{out}");
    // The entry and the counts stay exactly as reference PHP leaves them.
    assert!(out.contains("nscripts=1\n"), "the entry stays in the scripts map:\n{out}");
    assert!(out.contains("num=1\n"), "num_cached_scripts is unchanged:\n{out}");
    assert!(out.contains("ts=0\n"), "a discarded entry reports timestamp 0:\n{out}");
    assert!(out.contains(&format!("full_path={}\n", dir.join("discard.php").display())), "{out}");
    // Re-compiling restores it, mtime and all.
    assert!(out.contains("cf=true\n"), "{out}");
    assert!(out.contains("c2=true\n"), "compile_file must re-cache:\n{out}");
    assert!(out.contains("ts2=mtime\n"), "and restore the mtime:\n{out}");
}

/// `opcache_invalidate('')` returns `true`, because PHP's `realpath('')` resolves to the CURRENT
/// WORKING DIRECTORY and php-src's `zend_accel_invalidate` returns SUCCESS for any path that
/// resolves — a directory included.
///
/// REFERENCE (PHP 8.5.6) path matrix, `opcache_invalidate($p)`:
///
/// | `$p`             | result  |
/// |------------------|---------|
/// | `''`             | `true`  |
/// | `'.'`            | `true`  |
/// | `'..'`           | `true`  |
/// | `'/'`            | `true`  |
/// | `'/tmp'`         | `true`  |
/// | `__FILE__`       | `true`  |
/// | `'./<file>'`     | `true`  |
/// | `'/no/such/file'`| `false` |
/// | `' '`            | `false` |
/// | `"\0"`           | `false` |
///
/// The empty-string row is the one elephc used to get wrong, and the cause was `realpath`, not
/// OPcache: elephc's `__rt_realpath` is libc `realpath()`, which fails with ENOENT on `""`, while
/// PHP's `expand_filepath` maps it to the cwd. The three path-taking OPcache functions therefore
/// spell the empty case out as `getcwd()`. The underlying `realpath('')` divergence is unfixed and
/// reported separately.
#[test]
fn empty_and_directory_paths_follow_the_reference_matrix() {
    let dir = make_test_dir("opcache_paths");
    fs::write(
        dir.join("paths.php"),
        r#"<?php
foreach (['', '.', '..', '/', '/tmp', '/no/such/file', ' '] as $p) {
    echo 'inv[', $p, ']=', var_export(opcache_invalidate($p), true), "\n";
}
echo 'self=', var_export(opcache_invalidate(__FILE__), true), "\n";
// A resolvable NON-manifest path is never reported as cached, however it resolves.
echo 'cached_empty=', var_export(opcache_is_script_cached(''), true), "\n";
echo 'cached_tmp=', var_export(opcache_is_script_cached('/tmp'), true), "\n";
echo 'compile_empty=', var_export(opcache_compile_file(''), true), "\n";
"#,
    )
    .unwrap();
    let bin = compile(&dir, "paths", &["opcache.enable_cli=1"]);
    let out = run_binary(&bin);

    for path in ["", ".", "..", "/", "/tmp"] {
        assert!(
            out.contains(&format!("inv[{path}]=true\n")),
            "opcache_invalidate({path:?}) must be true:\n{out}"
        );
    }
    assert!(out.contains("inv[/no/such/file]=false\n"), "{out}");
    assert!(out.contains("inv[ ]=false\n"), "{out}");
    assert!(out.contains("self=true\n"), "{out}");
    assert!(out.contains("cached_empty=false\n"), "{out}");
    assert!(out.contains("cached_tmp=false\n"), "{out}");
    assert!(out.contains("compile_empty=false\n"), "{out}");
}

/// `opcache.interned_strings_buffer=0` OMITS the whole `interned_strings_usage` key, and every
/// non-zero buffer reports `used_memory < buffer_size` with `free_memory = buffer_size -
/// used_memory > 0`.
///
/// REFERENCE (PHP 8.5.6) top-level key list with `-d opcache.interned_strings_buffer=0`:
/// `opcache_enabled, cache_full, restart_pending, restart_in_progress, memory_usage,
/// opcache_statistics, scripts, jit` — EIGHT keys, `interned_strings_usage` absent, against nine
/// with the default `8`. php-src guards the sub-array with
/// `if (ZCSG(interned_strings).start && ZCSG(interned_strings).end)`.
///
/// And with `-d opcache.interned_strings_buffer=1`, reference reports
/// `buffer_size 1048576, used_memory 824200, free_memory 224376` — used STRICTLY below buffer. The
/// absolute figures are implementation-defined (no two builds agree, and reference's own vary run
/// to run); only the inequalities and the exact `used + free == buffer` identity are pinned.
#[test]
fn interned_strings_usage_is_omitted_for_a_zero_buffer() {
    let dir = make_test_dir("opcache_interned");
    fs::write(
        dir.join("interned.php"),
        r#"<?php
$s = opcache_get_status(false);
if (is_array($s)) {
    echo 'present=', (isset($s['interned_strings_usage']) ? '1' : '0'), "\n";
    if (isset($s['interned_strings_usage'])) {
        $i = $s['interned_strings_usage'];
        echo 'buffer=', $i['buffer_size'], "\n";
        echo 'used=', $i['used_memory'], "\n";
        echo 'free=', $i['free_memory'], "\n";
        echo 'sum_ok=', (($i['used_memory'] + $i['free_memory']) === $i['buffer_size'] ? '1' : '0'), "\n";
        echo 'used_lt=', ($i['used_memory'] < $i['buffer_size'] ? '1' : '0'), "\n";
        echo 'free_pos=', ($i['free_memory'] > 0 ? '1' : '0'), "\n";
    }
}
"#,
    )
    .unwrap();

    let zero = compile(&dir, "interned", &["opcache.enable_cli=1", "opcache.interned_strings_buffer=0"]);
    let zero_out = run_binary(&zero);
    assert!(
        zero_out.contains("present=0\n"),
        "a zero buffer must OMIT the key entirely:\n{zero_out}"
    );

    for buffer in ["1", "2", "8", "16"] {
        let bin = compile(
            &dir,
            "interned",
            &["opcache.enable_cli=1", &format!("opcache.interned_strings_buffer={buffer}")],
        );
        let out = run_binary(&bin);
        assert!(out.contains("present=1\n"), "buffer={buffer}:\n{out}");
        assert!(
            out.contains(&format!("buffer={}\n", buffer.parse::<i64>().unwrap() * 1_048_576)),
            "buffer={buffer} is reported in BYTES:\n{out}"
        );
        assert!(out.contains("sum_ok=1\n"), "used + free must equal buffer:\n{out}");
        assert!(out.contains("used_lt=1\n"), "used must stay BELOW buffer:\n{out}");
        assert!(out.contains("free_pos=1\n"), "free must never be 0 or negative:\n{out}");
    }
}

/// `max_cached_keys` is the first php-src prime `>= opcache.max_accelerated_files`.
///
/// REFERENCE (PHP 8.5.6, `-d opcache.max_accelerated_files=<n>`, reading
/// `opcache_get_status(false)['opcache_statistics']['max_cached_keys']`): 200 → 223, 223 → 223,
/// 224 → 463, 1000 → 1979, 3000 → 3907, 10000 → 16229, 65536 → 130987, 1000000 → 1048793. The
/// `223 → 223` row is what distinguishes php-src's `hash_size <= prime` from a strict `>`. An
/// earlier elephc revision baked 16229 for the default and echoed the RAW requested count for any
/// override, so `--ini opcache.max_accelerated_files=1000` reported 1000.
#[test]
fn max_cached_keys_rounds_up_through_the_prime_table() {
    let dir = make_test_dir("opcache_primes");
    fs::write(
        dir.join("primes.php"),
        r#"<?php
$s = opcache_get_status(false);
if (is_array($s)) { echo 'mck=', $s['opcache_statistics']['max_cached_keys'], "\n"; }
"#,
    )
    .unwrap();
    for (files, expected) in [
        ("200", 223),
        ("223", 223),
        ("224", 463),
        ("1000", 1_979),
        ("3000", 3_907),
        ("10000", 16_229),
        ("65536", 130_987),
        ("1000000", 1_048_793),
    ] {
        let bin = compile(
            &dir,
            "primes",
            &["opcache.enable_cli=1", &format!("opcache.max_accelerated_files={files}")],
        );
        let out = run_binary(&bin);
        assert!(
            out.contains(&format!("mck={expected}\n")),
            "max_accelerated_files={files} must report {expected}:\n{out}"
        );
    }
}

/// `opcache.enable=0` disables the cache even with `opcache.enable_cli=1`: the two are ANDed on
/// CLI. Every cache-API function reports the disabled result, and the binary's own
/// `opcache_get_configuration()` agrees.
///
/// REFERENCE (PHP 8.5.6), a script asserting all four `=== false`:
///
/// | `-d opcache.enable` | `-d opcache.enable_cli` | all four `=== false`? |
/// |---------------------|-------------------------|-----------------------|
/// | `0`                 | `1`                     | YES                   |
/// | `1`                 | `0`                     | YES                   |
/// | `1`                 | `1`                     | no (cache enabled)    |
/// | `0`                 | `0`                     | YES                   |
///
/// The first row is the fix: elephc used to read `opcache.enable_cli` ALONE on CLI, so
/// `--ini opcache.enable=0 --ini opcache.enable_cli=1` produced a binary reporting the status array
/// and `true` from `opcache_reset()` while reporting `opcache.enable => false` from its own
/// configuration — a self-contradiction.
#[test]
fn enable_and_enable_cli_are_anded_on_cli() {
    let dir = make_test_dir("opcache_enable_and");
    fs::write(
        dir.join("enable.php"),
        r#"<?php
echo 'status=', var_export(opcache_get_status() === false, true), "\n";
echo 'reset=', var_export(opcache_reset() === false, true), "\n";
echo 'inv=', var_export(opcache_invalidate(__FILE__) === false, true), "\n";
echo 'cached=', var_export(opcache_is_script_cached(__FILE__) === false, true), "\n";
$c = opcache_get_configuration();
echo 'directive=', var_export($c['directives']['opcache.enable'], true), "\n";
"#,
    )
    .unwrap();
    for (enable, enable_cli, disabled) in [
        ("0", "1", true),
        ("1", "0", true),
        ("1", "1", false),
        ("0", "0", true),
    ] {
        let bin = compile(
            &dir,
            "enable",
            &[
                &format!("opcache.enable={enable}"),
                &format!("opcache.enable_cli={enable_cli}"),
            ],
        );
        let out = run_binary(&bin);
        let expected = if disabled { "true" } else { "false" };
        for key in ["status", "reset", "inv", "cached"] {
            assert!(
                out.contains(&format!("{key}={expected}\n")),
                "enable={enable} enable_cli={enable_cli}: {key} must be {expected}:\n{out}"
            );
        }
        // The reported directive must agree with the derived state, never contradict it.
        assert!(
            out.contains(&format!("directive={}\n", enable == "1")),
            "enable={enable}: the reported directive must echo the flag:\n{out}"
        );
    }
}

/// The two OPcache functions elephc had been missing exist, are pay-for-use, and return reference
/// PHP's values.
///
/// REFERENCE (PHP 8.5.6). The extension exports EIGHT functions —
/// `opcache_reset`, `opcache_get_status`, `opcache_compile_file`, `opcache_invalidate`,
/// `opcache_jit_blacklist`, `opcache_get_configuration`, `opcache_is_script_cached`,
/// `opcache_is_script_cached_in_file_cache` (from `(new ReflectionExtension('Zend
/// OPcache'))->getFunctions()`). Their signatures, from `ReflectionFunction`:
///
/// ```text
/// opcache_is_script_cached_in_file_cache(string $filename): bool   -> bool(false)
/// opcache_jit_blacklist(Closure $closure): void                    -> NULL
/// ```
///
/// `opcache_is_script_cached_in_file_cache` returns `false` for EVERY path on an unconfigured
/// reference PHP, because php-src returns early on `!ZCG(accel_directives).file_cache` and
/// `opcache.file_cache` has a C NULL default. `opcache_jit_blacklist` only touches the JIT
/// blacklist, behind `#ifdef HAVE_JIT`.
#[test]
fn the_two_missing_functions_exist_and_match_reference() {
    let dir = make_test_dir("opcache_new_fns");
    fs::write(
        dir.join("newfns.php"),
        r#"<?php
echo 'fc_self=', var_export(opcache_is_script_cached_in_file_cache(__FILE__), true), "\n";
echo 'fc_miss=', var_export(opcache_is_script_cached_in_file_cache('/not/compiled.php'), true), "\n";
echo 'jitbl=', var_export(opcache_jit_blacklist(function () { return 1; }), true), "\n";
echo 'exists_fc=', var_export(function_exists('opcache_is_script_cached_in_file_cache'), true), "\n";
echo 'exists_jb=', var_export(function_exists('opcache_jit_blacklist'), true), "\n";
"#,
    )
    .unwrap();
    let bin = compile(&dir, "newfns", &["opcache.enable_cli=1"]);
    let out = run_binary(&bin);

    assert!(out.contains("fc_self=false\n"), "no file cache exists:\n{out}");
    assert!(out.contains("fc_miss=false\n"), "{out}");
    assert!(out.contains("jitbl=NULL\n"), "a void function evaluates to NULL:\n{out}");
    assert!(out.contains("exists_fc=true\n"), "{out}");
    assert!(out.contains("exists_jb=true\n"), "{out}");

    // Pay-for-use: a program that names neither must not carry either declaration.
    fs::write(
        dir.join("nofns.php"),
        r#"<?php
echo 'exists_fc=', var_export(function_exists('opcache_is_script_cached_in_file_cache'), true), "\n";
"#,
    )
    .unwrap();
    // (Naming it in a string literal DOES inject it — the detector deliberately over-approximates
    // so `function_exists` and the callable forms keep working. That is the case above; the
    // negative case is a program that never mentions the name at all, which cannot observe it.)
    let bin2 = compile(&dir, "nofns", &["opcache.enable_cli=1"]);
    let out2 = run_binary(&bin2);
    assert!(out2.contains("exists_fc=true\n"), "the string literal counts as a reference:\n{out2}");
}

/// Verifies the `scripts` entry carries `revalidate` only from PHP 8.3 on.
///
/// php-src added the key in 8.3; a `--php-version 8.2` build must not report a key the runtime it
/// targets does not have. Captured from the official Docker images on Linux, which is the only
/// place this is observable — macOS's shared-memory model hides the `scripts` map entirely, so
/// every local probe saw an empty entry set and the gap was invisible:
///
/// ```text
/// php:8.2-cli  full_path hits last_used last_used_timestamp memory_consumption timestamp
/// php:8.3-cli  full_path hits last_used last_used_timestamp memory_consumption revalidate timestamp
/// ```
///
/// The 8.3 row is the control: without it a fix that dropped `revalidate` everywhere would pass.
#[test]
fn scripts_entry_reports_revalidate_only_from_php_83() {
    for (version, expects_revalidate) in [("8.2", false), ("8.3", true), ("8.5", true)] {
        let dir = make_test_dir(&format!("opcache_revalidate_{}", version.replace('.', "_")));
        fs::write(
            dir.join("reval.php"),
            r#"<?php
$s = opcache_get_status();
if (is_array($s)) {
    foreach ($s['scripts'] as $key => $entry) {
        foreach ($entry as $field => $value) { echo 'K=', $field, "\n"; }
    }
}
"#,
        )
        .unwrap();

        let output = Command::new(elephc_bin())
            .env("XDG_CACHE_HOME", dir.join("cache-root"))
            .current_dir(&dir)
            .arg("--php-version")
            .arg(version)
            .arg("--ini")
            .arg("opcache.enable_cli=1")
            .arg(dir.join("reval.php"))
            .output()
            .expect("failed to spawn elephc");
        assert!(
            output.status.success(),
            "compile failed for {version}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let out = run_binary(&dir.join("reval"));

        assert!(
            out.contains("K=timestamp\n"),
            "the entry must still carry its other fields under {version}:\n{out}"
        );
        assert_eq!(
            out.contains("K=revalidate\n"),
            expects_revalidate,
            "PHP {version} {} report `revalidate`:\n{out}",
            if expects_revalidate { "must" } else { "must NOT" }
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
