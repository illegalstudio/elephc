//! Purpose:
//! Pins the runtime script cache's observable behaviour: the disabled gate, warm hits,
//! mtime revalidation and its `revalidate_freq` window, the budget and entry ceilings,
//! and the discard/reset/compile_file operations the OPcache API drives.
//!
//! Called from:
//! - `cargo test` through Rust's test harness, as `store`'s child test module.
//!
//! Key details:
//! - The cache is a process-wide singleton while the configuration is thread-local, so
//!   every test here takes `test_lock()`, which also clears the cache. Without that,
//!   parallel tests would read each other's counters.
//! - Fixtures write real files: mtime validation is the behaviour under test, and a
//!   fake clock would prove nothing about it.

use super::*;
use crate::script_cache::config::set_config;
use crate::script_cache::store::lock_for_test as test_lock;

/// Returns a configuration with the cache on and the given revalidation window.
fn enabled_config(revalidate_freq: u64) -> ScriptCacheConfig {
    ScriptCacheConfig {
        enabled: true,
        revalidate_freq,
        // The freshness guard is OFF for these fixtures. They are written microseconds
        // before the assertion, so php-src's default `file_update_protection = 2` would
        // refuse to cache every one of them and these tests would be measuring that guard
        // instead of what they mean to measure. Reference PHP needs the same
        // `-d opcache.file_update_protection=0` for exactly this reason.
        file_update_protection: 0,
        ..ScriptCacheConfig::disabled()
    }
}

/// Writes a fixture file under a per-test temp directory and returns its path.
fn write_fixture(name: &str, contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "elephc-script-cache-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("fixture directory should be creatable");
    let path = dir.join("fixture.php");
    std::fs::write(&path, contents).expect("fixture should be writable");
    path
}

/// Renders a segment list as a compact shape, for assertions about what was served.
fn shape(segments: &[ScriptSegment]) -> Vec<&'static str> {
    segments
        .iter()
        .map(|segment| match segment {
            ScriptSegment::Output(_) => "out",
            ScriptSegment::Code(_) => "code",
            ScriptSegment::ParseError(_) => "err",
        })
        .collect()
}

/// Verifies a disabled cache stores nothing and counts nothing.
#[test]
fn a_disabled_cache_stores_nothing() {
    let _guard = test_lock();
    set_config(ScriptCacheConfig::disabled());
    let path = write_fixture("disabled", "<?php $x = 1;");

    load_script(&path).expect("fixture should load");
    load_script(&path).expect("fixture should load");

    assert_eq!(stats(), ScriptCacheStats::default());
}

/// Verifies the second load of an unchanged file is served warm.
#[test]
fn a_second_load_is_a_hit() {
    let _guard = test_lock();
    set_config(enabled_config(2));
    let path = write_fixture("hit", "<?php $x = 1;");

    load_script(&path).expect("fixture should load");
    load_script(&path).expect("fixture should load");
    let stats = stats();

    assert_eq!((stats.hits, stats.misses, stats.num_cached_scripts), (1, 1, 1));
}

/// Verifies a warm hit serves the same segment shape a cold fill produced.
#[test]
fn a_warm_hit_serves_the_same_segments() {
    let _guard = test_lock();
    set_config(enabled_config(2));
    let path = write_fixture("shape", "A<?php $x = 1; ?>B");

    let cold = load_script(&path).expect("fixture should load");
    let warm = load_script(&path).expect("fixture should load");

    assert_eq!(shape(&cold), ["out", "code", "out"]);
    assert_eq!(shape(&warm), shape(&cold));
}

/// Verifies a changed file is re-read once its revalidation window has passed.
///
/// `revalidate_freq = 0` makes every load re-`stat`, which is what isolates the mtime
/// comparison from the window that normally suppresses it.
#[test]
fn a_changed_file_is_refilled_when_revalidation_is_due() {
    let _guard = test_lock();
    set_config(enabled_config(0));
    let path = write_fixture("changed", "<?php $x = 1;");
    load_script(&path).expect("fixture should load");

    // A SIZE change is decisive on its own: mtime has one-second resolution here, so a
    // same-second rewrite would otherwise still look unchanged.
    std::fs::write(&path, "<?php $x = 1; $y = 2;").expect("fixture should be rewritable");
    load_script(&path).expect("fixture should load");
    let stats = stats();

    assert_eq!((stats.hits, stats.misses), (0, 2));
    assert_eq!(stats.num_cached_scripts, 1);
}

/// Verifies a changed file inside the revalidation window is still served stale.
///
/// This is php-src's behaviour, not a shortcut: `opcache.revalidate_freq` is exactly
/// the promise that a changed file may serve stale for that many seconds.
#[test]
fn a_changed_file_stays_stale_inside_the_revalidation_window() {
    let _guard = test_lock();
    set_config(enabled_config(3600));
    let path = write_fixture("stale", "<?php $x = 1;");
    load_script(&path).expect("fixture should load");

    std::fs::write(&path, "<?php $x = 1; $y = 2;").expect("fixture should be rewritable");
    load_script(&path).expect("fixture should load");
    let stats = stats();

    assert_eq!((stats.hits, stats.misses), (1, 1));
}

/// Verifies `opcache.validate_timestamps = 0` never re-reads a changed file.
#[test]
fn timestamp_validation_off_never_refills() {
    let _guard = test_lock();
    set_config(ScriptCacheConfig {
        validate_timestamps: false,
        ..enabled_config(0)
    });
    let path = write_fixture("novalidate", "<?php $x = 1;");
    load_script(&path).expect("fixture should load");

    std::fs::write(&path, "<?php $x = 1; $y = 2;").expect("fixture should be rewritable");
    load_script(&path).expect("fixture should load");
    let stats = stats();

    assert_eq!((stats.hits, stats.misses), (1, 1));
}

/// Verifies `opcache.max_file_size` refuses to CACHE a large file but still runs it.
#[test]
fn an_oversized_file_is_served_but_not_cached() {
    let _guard = test_lock();
    set_config(ScriptCacheConfig {
        max_file_size: 4,
        ..enabled_config(2)
    });
    let path = write_fixture("oversized", "<?php $x = 1;");

    let segments = load_script(&path).expect("fixture should load");
    let stats = stats();

    assert_eq!(shape(&segments), ["code"]);
    assert_eq!(stats.num_cached_scripts, 0);
}

/// Verifies the entry ceiling refuses a new script and latches `cache_full`.
#[test]
fn the_entry_ceiling_latches_cache_full() {
    let _guard = test_lock();
    set_config(ScriptCacheConfig {
        max_accelerated_files: 1,
        ..enabled_config(2)
    });
    let first = write_fixture("ceiling-a", "<?php $x = 1;");
    let second = write_fixture("ceiling-b", "<?php $y = 2;");

    load_script(&first).expect("fixture should load");
    load_script(&second).expect("fixture should load");
    let stats = stats();

    assert_eq!(stats.num_cached_scripts, 1);
    assert!(stats.cache_full);
}

/// Verifies the byte budget refuses a new script and latches `cache_full`.
#[test]
fn the_byte_budget_latches_cache_full() {
    let _guard = test_lock();
    set_config(ScriptCacheConfig {
        memory_consumption: 1,
        ..enabled_config(2)
    });
    let path = write_fixture("budget", "<?php $x = 1;");

    load_script(&path).expect("fixture should load");
    let stats = stats();

    assert_eq!(stats.num_cached_scripts, 0);
    assert!(stats.cache_full);
}

/// Verifies a forced discard keeps the slot but stops reporting the file as cached.
#[test]
fn a_discard_keeps_the_slot_and_zeroes_the_timestamp() {
    let _guard = test_lock();
    set_config(enabled_config(2));
    let path = write_fixture("discard", "<?php $x = 1;");
    load_script(&path).expect("fixture should load");

    assert!(is_cached(&path));
    assert!(discard(&path));
    assert!(!is_cached(&path));
    assert_eq!(stats().num_cached_scripts, 1);
    assert_eq!(cached_scripts()[0].timestamp, 0);
}

/// Verifies discarding a path that was never cached reports no entry.
#[test]
fn discarding_an_uncached_path_reports_nothing() {
    let _guard = test_lock();
    set_config(enabled_config(2));
    let path = write_fixture("discard-missing", "<?php $x = 1;");

    assert!(!discard(&path));
}

/// Verifies a discarded entry is refilled rather than served on the next include.
#[test]
fn a_discarded_entry_is_refilled_on_the_next_load() {
    let _guard = test_lock();
    set_config(enabled_config(3600));
    let path = write_fixture("discard-refill", "<?php $x = 1;");
    load_script(&path).expect("fixture should load");
    discard(&path);

    load_script(&path).expect("fixture should load");

    assert!(is_cached(&path));
    assert_eq!(stats().misses, 2);
}

/// Verifies `opcache_compile_file()` caches a file without executing it.
#[test]
fn compile_file_caches_without_running() {
    let _guard = test_lock();
    set_config(enabled_config(2));
    let path = write_fixture("compile", "<?php $x = 1;");

    assert!(compile_file(&path));
    assert!(is_cached(&path));
}

/// Verifies `opcache_compile_file()` reports failure for a file that cannot be read.
#[test]
fn compile_file_reports_a_missing_file() {
    let _guard = test_lock();
    set_config(enabled_config(2));

    assert!(!compile_file(Path::new("/no/such/elephc/fixture.php")));
}

/// Verifies `opcache_compile_file()` re-caches a discarded entry.
#[test]
fn compile_file_reverses_a_discard() {
    let _guard = test_lock();
    set_config(enabled_config(2));
    let path = write_fixture("compile-undiscard", "<?php $x = 1;");
    load_script(&path).expect("fixture should load");
    discard(&path);

    assert!(compile_file(&path));
    assert!(is_cached(&path));
}

/// Verifies `opcache_compile_file()` does nothing while the cache is disabled.
#[test]
fn compile_file_is_inert_while_disabled() {
    let _guard = test_lock();
    set_config(ScriptCacheConfig::disabled());
    let path = write_fixture("compile-disabled", "<?php $x = 1;");

    assert!(!compile_file(&path));
    assert!(!is_cached(&path));
}

/// Verifies a scheduled restart LATCHES and changes nothing else until it is applied.
///
/// php-src defers the restart to the next request, so within the scheduling one the cache
/// keeps answering. VERIFIED on reference PHP 8.5.10: straight after `opcache_reset()`,
/// `opcache_is_script_cached()` is still true, `num_cached_scripts` is unchanged, and both
/// `manual_restarts` and `last_restart_time` are still 0.
#[test]
fn a_scheduled_restart_latches_without_flushing() {
    let _guard = test_lock();
    set_config(enabled_config(2));
    let path = write_fixture("reset", "<?php $x = 1;");
    load_script(&path).expect("fixture should load");

    assert!(schedule_restart());
    let stats = stats();

    assert_eq!(stats.num_cached_scripts, 1, "the entry must survive");
    assert!(stats.used_memory > 0, "and keep its footprint");
    assert_eq!(stats.manual_restarts, 0, "counted at the restart, not the schedule");
    assert_eq!(stats.last_restart_time, 0);
    assert!(stats.restart_pending, "only the latch moves");
}

/// Verifies applying the pending restart is what empties the cache and counts it.
///
/// This runs at a request boundary, which is where php-src performs the restart it
/// scheduled. Clearing the latch is part of it: the next request must be able to schedule
/// its own.
#[test]
fn applying_a_pending_restart_empties_the_cache_and_counts_it() {
    let _guard = test_lock();
    set_config(enabled_config(2));
    let path = write_fixture("reset-apply", "<?php $x = 1;");
    load_script(&path).expect("fixture should load");
    assert!(schedule_restart());

    assert!(apply_pending_restart(), "a pending restart is performed");
    let stats = stats();

    assert_eq!(stats.num_cached_scripts, 0);
    assert_eq!(stats.used_memory, 0);
    assert_eq!(stats.manual_restarts, 1);
    assert!(stats.last_restart_time > 0);
    assert!(!stats.restart_pending, "the latch clears for the next request");
    assert!(!apply_pending_restart(), "and nothing is pending afterwards");
}

/// Verifies a second restart in the same request reports `false` and counts nothing.
///
/// php-src's schedule clears the flag its own guard tests, so only the first call succeeds.
#[test]
fn a_second_restart_reports_false_and_counts_nothing() {
    let _guard = test_lock();
    set_config(enabled_config(2));

    assert!(schedule_restart());
    assert!(!schedule_restart());
    assert_eq!(stats().manual_restarts, 0, "nothing is counted until it is applied");
}

/// Verifies the cache still fills after a restart: the latch reports, it does not disable.
#[test]
fn a_restart_does_not_stop_the_cache_from_filling_again() {
    let _guard = test_lock();
    set_config(enabled_config(2));
    let path = write_fixture("reset-refill", "<?php $x = 1;");
    load_script(&path).expect("fixture should load");
    schedule_restart();

    load_script(&path).expect("fixture should load");

    assert!(is_cached(&path));
}

/// Verifies a missing file still reports the error kind the direct read produced.
#[test]
fn a_missing_file_still_reports_an_io_error() {
    let _guard = test_lock();
    set_config(enabled_config(2));

    let error = load_script(Path::new("/no/such/elephc/fixture.php"))
        .expect_err("a missing include must not succeed");

    assert_eq!(error.kind(), io::ErrorKind::NotFound);
}

/// Verifies the reported script list carries the accounted footprint, hits and path.
#[test]
fn cached_scripts_report_their_path_and_footprint() {
    let _guard = test_lock();
    set_config(enabled_config(2));
    let path = write_fixture("report", "A<?php $x = 1; ?>B");
    load_script(&path).expect("fixture should load");
    load_script(&path).expect("fixture should load");

    let scripts = cached_scripts();

    assert_eq!(scripts.len(), 1);
    assert_eq!(scripts[0].hits, 1);
    assert!(scripts[0].full_path.ends_with("fixture.php"));
    assert!(scripts[0].memory_consumption > 0);
    assert!(scripts[0].timestamp > 0);
}

/// Verifies recompiling a cached file does not double-count its bytes in the budget.
///
/// An earlier revision removed the entry before refilling it, which returned nothing to
/// `used_memory`; three compiles of one file then charged the budget three times and the
/// entry was weighed against `max_accelerated_files` as a new one each time.
#[test]
fn recompiling_a_file_charges_the_budget_once() {
    let _guard = test_lock();
    set_config(enabled_config(2));
    let path = write_fixture("recompile-budget", "A<?php $x = 1; ?>B");

    load_script(&path).expect("fixture should load");
    let after_first = stats().used_memory;
    compile_file(&path);
    compile_file(&path);
    let stats = stats();

    assert!(after_first > 0, "the fixture should have cost something");
    assert_eq!(stats.used_memory, after_first);
    assert_eq!(stats.num_cached_scripts, 1);
}

/// Verifies repeated recompiles of ONE file never exhaust the byte budget.
///
/// The leak's second observable consequence: a `used_memory` that grows on every recompile
/// eventually crosses `opcache.memory_consumption`, latching `cache_full` and refusing to
/// cache a file the cache already held. The budget here fits two entries, so a correct
/// accounting can recompile one file forever.
#[test]
fn repeated_recompiles_never_exhaust_the_budget() {
    let _guard = test_lock();
    let path = write_fixture("budget-recompile", "A<?php $x = 1; ?>B");
    let footprint = std::fs::metadata(&path)
        .expect("fixture should be readable")
        .len() as usize;
    set_config(ScriptCacheConfig {
        memory_consumption: footprint * 2,
        ..enabled_config(2)
    });
    load_script(&path).expect("fixture should load");

    for _ in 0..5 {
        compile_file(&path);
    }
    let stats = stats();

    assert!(!stats.cache_full, "the budget should not have been exhausted");
    assert!(is_cached(&path));
    assert!(stats.used_memory <= footprint);
}
