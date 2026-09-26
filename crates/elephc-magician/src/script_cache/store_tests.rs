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
use crate::script_cache::config::{
    clear_directive_overrides, set_config, stamp_request_time, swap_directive,
    DIRECTIVE_REVALIDATE_FREQ, DIRECTIVE_VALIDATE_TIMESTAMPS,
};
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

    // THE TIMESTAMP MUST MOVE, and is moved explicitly. Freshness is the mtime alone, as
    // php-src's `do_validate_timestamps` is; mtime has one-second resolution, so a rewrite
    // landing in the same second is NOT a change to either engine. This test used to rely on
    // the length instead, which pinned a rule reference does not have.
    std::fs::write(&path, "<?php $x = 1; $y = 2;").expect("fixture should be rewritable");
    set_mtime(&path, 900_000);
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

/// Moves `path`'s mtime to `seconds` since the epoch.
///
/// The fixtures here rely on SIZE to detect a rewrite, because mtime has one-second
/// resolution and a same-second rewrite looks unchanged. `is_cached` compares the timestamp
/// ALONE, as php-src's `do_validate_timestamps` does, so a test of it has to move the mtime
/// explicitly — otherwise it would pass on a size change the function no longer looks at.
fn set_mtime(path: &std::path::Path, seconds: u64) {
    let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds);
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("fixture should reopen")
        .set_modified(when)
        .expect("mtime should be settable");
}

/// Verifies `is_cached` stops vouching for a stale entry once revalidation is due, and
/// keeps vouching inside the window.
///
/// php-src's `opcache_is_script_cached()` validates the timestamp, but only once
/// `revalidate_freq` has elapsed. MEASURED on reference PHP 8.5 with the source rewritten
/// under an older mtime: `revalidate_freq=0` reports it uncached, the default `2` still
/// reports it cached. This answered `true` in both.
///
/// THE PAIR IS THE TEST. A version that re-stat'd on every call would satisfy the first
/// assertion and break the second, which is the configuration nearly everyone runs.
#[test]
fn is_cached_revalidates_only_once_the_window_has_passed() {
    let _guard = test_lock();

    set_config(enabled_config(0));
    let due = write_fixture("iscached_due", "<?php $x = 1;");
    set_mtime(&due, 1_000_000);
    load_script(&due).expect("fixture should load");
    assert!(is_cached(&due), "freshly loaded, the entry is cached");
    set_mtime(&due, 900_000);
    assert!(
        !is_cached(&due),
        "revalidate_freq=0: a moved timestamp must make the entry report uncached"
    );

    set_config(enabled_config(3600));
    let within = write_fixture("iscached_within", "<?php $x = 1;");
    set_mtime(&within, 1_000_000);
    load_script(&within).expect("fixture should load");
    set_mtime(&within, 900_000);
    assert!(
        is_cached(&within),
        "inside the revalidation window php-src does not re-stat, so neither may this"
    );
}

/// Verifies `revalidate_freq=0` forces a re-stat whatever deadline the entry carries.
///
/// php-src tests the CURRENT frequency before the stored deadline, so lowering it to `0` with
/// `ini_set()` takes effect at once. The entry's deadline was computed under the old
/// frequency, and reading only that kept vouching for it for up to the OLD window.
/// MEASURED: cache under `60`, move the mtime, `ini_set(...,'0')`; reference reports the
/// script uncached, elephc reported it cached.
#[test]
fn a_zero_frequency_override_forces_revalidation() {
    let _guard = test_lock();
    set_config(enabled_config(3600));
    let path = write_fixture("freq_override", "<?php $x = 1;");
    set_mtime(&path, 1_000_000);
    load_script(&path).expect("fixture should load");
    set_mtime(&path, 900_000);
    assert!(is_cached(&path), "inside the 3600s window the entry is still vouched for");

    swap_directive(DIRECTIVE_REVALIDATE_FREQ, 0, true);
    let answer = is_cached(&path);
    clear_directive_overrides();
    assert!(
        !answer,
        "revalidate_freq lowered to 0 must re-stat at once, not wait out the old deadline"
    );
}

/// Verifies a SUCCESSFUL `is_cached` check renews the revalidation window.
///
/// php-src's query goes through `validate_timestamp_and_record_ex`, which records
/// `request_time + revalidate_freq` on success. MEASURED with `revalidate_freq=2`: once the
/// window has lapsed, a query of the unchanged file renews it, so a change made right after
/// is not yet seen — reference answers `true, true`; elephc answered `true, false`.
///
/// Driven through the REQUEST TIME rather than `sleep`, which is what makes it deterministic:
/// the second query is placed past the first deadline by stamping, not by waiting.
#[test]
fn a_successful_query_renews_the_revalidation_window() {
    let _guard = test_lock();
    set_config(enabled_config(2));
    stamp_request_time(1_000_000);
    let path = write_fixture("renew", "<?php $x = 1;");
    set_mtime(&path, 999_000);
    load_script(&path).expect("fixture should load");

    // Past the first deadline (1_000_002): the check runs, succeeds, and must renew.
    stamp_request_time(1_000_005);
    assert!(is_cached(&path), "unchanged: cached");
    set_mtime(&path, 900_000);
    assert!(
        is_cached(&path),
        "the renewed window (to 1_000_007) must keep vouching for the entry"
    );
}

/// Verifies invalidating a DELETED file still retires, and reports, its entry.
///
/// `invalidate` asked whether the entry was live through `is_cached`, which revalidates —
/// and a deleted file fails revalidation. So it reported nothing live for exactly the entry
/// an invalidate of a deleted file exists to retire. MEASURED, `revalidate_freq=0`:
/// reference answers `true`, elephc answered `false`.
///
/// IT ALSO PINS A SECOND DEFECT, and that is not an accident of the fixture. On macOS the
/// temp directory is `/var/folders/…`, a symlink to `/private/var/folders/…`. Once the file
/// is deleted, `canonicalize` fails on it, and the key fell back to the raw symlinked path —
/// which never matches the canonical key the entry was stored under. This test failed for
/// that reason first, after the presence fix was already in; `cache_key` now resolves the
/// directory that still exists, as php-src does. On a platform whose temp directory is not
/// a symlink it still pins the presence half.
#[test]
fn invalidating_a_deleted_file_reports_the_entry_it_retired() {
    let _guard = test_lock();
    set_config(enabled_config(0));
    let path = write_fixture("deleted", "<?php $x = 1;");
    load_script(&path).expect("fixture should load");
    std::fs::remove_file(&path).expect("fixture should be removable");

    assert!(
        invalidate(&path, true),
        "the entry was present and is retired, even though the file is gone"
    );
}

/// Verifies the warm path treats a size-only rewrite as FRESH, as php-src does.
///
/// Rewrite to a different length, restore the mtime, include again: reference replays the
/// stored script. The warm path compared the length and re-read the file. MEASURED on
/// reference PHP 8.5.
#[test]
fn a_size_only_rewrite_is_served_from_the_cache() {
    let _guard = test_lock();
    set_config(enabled_config(0));
    let path = write_fixture("size_only", "<?php $x = 1;");
    set_mtime(&path, 1_000_000);
    load_script(&path).expect("fixture should load");

    std::fs::write(&path, "<?php $x = 1; $y = 'a much longer body';")
        .expect("fixture should be rewritable");
    set_mtime(&path, 1_000_000);
    load_script(&path).expect("fixture should load");

    assert_eq!(
        (stats().hits, stats().misses),
        (1, 1),
        "the second include must be a HIT on the stored script"
    );
}

/// Verifies admission reads the REQUEST time, not the wall clock.
///
/// php-src measures a file's age against `ZCG(request_time)`, fixed when the request starts.
/// A file whose mtime equals the request time is zero seconds old for the whole request, so
/// the default protection of 2 refuses it however long the request runs. MEASURED: write a
/// file, `sleep(3)`, compile it — reference leaves it uncached, elephc cached it.
///
/// The discriminator is stark: by the WALL clock this fixture is decades old and would be
/// admitted at once; by the request time it is brand new and must be refused.
#[test]
fn admission_measures_age_against_the_request_time() {
    let _guard = test_lock();
    set_config(ScriptCacheConfig {
        file_update_protection: 2,
        ..enabled_config(0)
    });
    let path = write_fixture("request_age", "<?php $x = 1;");
    set_mtime(&path, 1_000_000);
    stamp_request_time(1_000_000);
    load_script(&path).expect("fixture should load");

    assert!(
        !is_cached(&path),
        "zero seconds old by the request clock: the protection window must refuse it"
    );
}

/// Verifies the revalidation deadline itself is still INSIDE the window.
///
/// php-src skips the check while `revalidate >= request_time`, so at exactly `stored + freq`
/// the entry is still vouched for and the stat is due one second later. `>=` re-stat'd at the
/// deadline.
#[test]
fn revalidation_is_due_only_after_the_deadline() {
    let _guard = test_lock();
    set_config(enabled_config(2));
    stamp_request_time(1_000_000);
    let path = write_fixture("deadline_edge", "<?php $x = 1;");
    set_mtime(&path, 999_000);
    load_script(&path).expect("fixture should load");
    set_mtime(&path, 900_000);

    stamp_request_time(1_000_002);
    assert!(is_cached(&path), "at the deadline php-src still skips the check");
    stamp_request_time(1_000_003);
    assert!(!is_cached(&path), "one second past it the moved timestamp is seen");
}

/// Verifies a successful renewal invalidates status snapshots.
///
/// The renewed deadline is what `opcache_get_status()['scripts']` reports as `revalidate`,
/// and the FFI reuses its snapshot until `generation` moves. MEASURED: status, then
/// `ini_set('opcache.revalidate_freq', '0')` and a query, then status again — reference's
/// deadline moved, elephc kept reporting the old one.
#[test]
fn a_query_renewal_moves_the_generation() {
    let _guard = test_lock();
    set_config(enabled_config(60));
    let path = write_fixture("renew_generation", "<?php $x = 1;");
    load_script(&path).expect("fixture should load");
    let before = generation();

    swap_directive(DIRECTIVE_REVALIDATE_FREQ, 0, true);
    let answer = is_cached(&path);
    clear_directive_overrides();

    assert!(answer, "the file is unchanged, so the forced check succeeds");
    assert_ne!(generation(), before, "the renewed deadline must invalidate snapshots");
}

/// Verifies a FAILED revalidation discards the old entry even when the replacement is refused.
///
/// php-src discards the stale script before compiling the new version; when
/// `file_update_protection` then refuses to store that version, nothing is cached — the old
/// script is gone. Leaving it live made the next include serve the OLD version once
/// `validate_timestamps` was turned off. MEASURED: reference printed `B B`, elephc `B A`.
#[test]
fn a_failed_revalidation_discards_even_when_the_refill_is_refused() {
    let _guard = test_lock();
    set_config(ScriptCacheConfig {
        file_update_protection: 2,
        ..enabled_config(0)
    });
    stamp_request_time(1_000_000);
    let path = write_fixture("refused_refill", "<?php $x = 1;");
    set_mtime(&path, 990_000);
    assert_eq!(shape(&load_script(&path).unwrap()), ["code"], "version A is admitted");

    std::fs::write(&path, "B<?php $x = 1;").expect("fixture should be rewritable");
    set_mtime(&path, 1_000_000);
    assert_eq!(shape(&load_script(&path).unwrap()), ["out", "code"], "version B runs");

    swap_directive(DIRECTIVE_VALIDATE_TIMESTAMPS, 0, true);
    let third = shape(&load_script(&path).unwrap());
    clear_directive_overrides();
    assert_eq!(third, ["out", "code"], "A was discarded; it must not come back");
}

/// Verifies an entry admitted with `validate_timestamps=0` carries NO timestamp.
///
/// php-src records the timestamp only under validation. Such an entry stays trusted by
/// `opcache_is_script_cached()` once validation is back on — `validate_timestamp_and_record`
/// answers SUCCESS for `timestamp == 0` — while a non-forced `opcache_invalidate()`, which
/// calls `do_validate_timestamps` directly, compares that `0` with the real mtime and
/// retires it. MEASURED: reference `cached=1` before the invalidate and `cached=0` after;
/// elephc kept the entry, having recorded the mtime regardless.
#[test]
fn an_entry_admitted_without_validation_has_no_timestamp() {
    let _guard = test_lock();
    set_config(ScriptCacheConfig {
        validate_timestamps: false,
        ..enabled_config(0)
    });
    let path = write_fixture("unrecorded", "<?php $x = 1;");
    load_script(&path).expect("fixture should load");

    swap_directive(DIRECTIVE_VALIDATE_TIMESTAMPS, 1, true);
    let trusted = is_cached(&path);
    invalidate(&path, false);
    let after = is_cached(&path);
    clear_directive_overrides();

    assert!(trusted, "an unrecorded timestamp is never checked by the query");
    assert!(!after, "a non-forced invalidate compares 0 against the mtime and retires it");
}

/// Points the file cache at a per-test directory and returns it.
fn with_file_cache(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "elephc-script-cache-fc-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("file-cache directory should be creatable");
    crate::script_cache::file_cache::set_file_cache_config(
        crate::script_cache::file_cache::FileCacheConfig {
            path: dir.to_string_lossy().into_owned(),
            read_only: false,
        },
    );
    dir
}

/// Empties the in-memory cache, leaving the disk alone — a new process over the same directory.
fn forget_memory() {
    *lock_script_cache() = ScriptCache::default();
}

/// Verifies a DISK HIT keeps the timestamp it was stored with.
///
/// php-src's deserialized script keeps its `timestamp`. Deriving it from the reader's
/// configuration recorded none under `validate_timestamps=0`, so once validation was turned
/// back on the entry was never re-checked and a changed source kept running the stored
/// version. MEASURED across two processes: reference printed `A B-longer`, elephc `A A`.
#[test]
fn a_disk_hit_keeps_its_stored_timestamp() {
    let _guard = test_lock();
    with_file_cache("disk_timestamp");
    set_config(enabled_config(0));
    let path = write_fixture("disk_timestamp", "<?php $x = 1;");
    set_mtime(&path, 1_000_000);
    load_script(&path).expect("the writer stores version A on disk");
    forget_memory();

    std::fs::write(&path, "B<?php $x = 1;").expect("fixture should be rewritable");
    set_mtime(&path, 900_000);
    set_config(ScriptCacheConfig {
        validate_timestamps: false,
        ..enabled_config(0)
    });
    assert_eq!(shape(&load_script(&path).unwrap()), ["code"], "unvalidated: disk A");

    swap_directive(DIRECTIVE_VALIDATE_TIMESTAMPS, 1, true);
    let second = shape(&load_script(&path).unwrap());
    clear_directive_overrides();
    assert_eq!(second, ["out", "code"], "validated against A's timestamp: B is read");
}

/// Verifies a disk entry written WITHOUT validation is a miss for a validating reader.
///
/// php-src serializes the recorded timestamp, which is `0` under `validate_timestamps=0`, so
/// a reader that validates finds it matching no mtime and recompiles — even over an unchanged
/// source. MEASURED across two processes: reference's reader missed, elephc's hit, because
/// the disk carried the real mtime.
#[test]
fn an_unvalidated_disk_entry_is_a_miss_for_a_validating_reader() {
    let _guard = test_lock();
    with_file_cache("disk_unvalidated");
    set_config(ScriptCacheConfig {
        validate_timestamps: false,
        ..enabled_config(0)
    });
    let path = write_fixture("disk_unvalidated", "<?php $x = 1;");
    set_mtime(&path, 1_000_000);
    load_script(&path).expect("the writer stores the entry on disk");
    forget_memory();

    set_config(enabled_config(0));
    load_script(&path).expect("the reader loads the unchanged source");

    assert_eq!(
        (stats().hits, stats().misses),
        (0, 1),
        "the stored timestamp is 0, which no real mtime matches"
    );
}

/// Verifies `max_file_size` refuses a COMPILE, not an entry already on disk.
///
/// php-src loads the file cache before it enters the compile path, and the size and
/// blacklist refusals live inside that path. MEASURED across two processes, the reader
/// running `validate_timestamps=0` with `max_file_size=1` over a source changed since it was
/// stored: reference ran the stored version, elephc re-read the new one.
#[test]
fn the_size_refusal_does_not_bypass_a_disk_entry() {
    let _guard = test_lock();
    with_file_cache("disk_size_refusal");
    set_config(enabled_config(0));
    let path = write_fixture("disk_size_refusal", "<?php $x = 1;");
    set_mtime(&path, 1_000_000);
    load_script(&path).expect("the writer stores version A on disk");
    forget_memory();

    std::fs::write(&path, "B<?php $x = 1;").expect("fixture should be rewritable");
    set_mtime(&path, 900_000);
    set_config(ScriptCacheConfig {
        validate_timestamps: false,
        max_file_size: 1,
        ..enabled_config(0)
    });

    assert_eq!(shape(&load_script(&path).unwrap()), ["code"], "the stored A is served");
}

/// Verifies the blacklist refuses a COMPILE, not an entry already on disk. Same measurement
/// as the size refusal above, with the file blacklisted instead.
#[test]
fn the_blacklist_does_not_bypass_a_disk_entry() {
    let _guard = test_lock();
    let cache = with_file_cache("disk_blacklist");
    set_config(enabled_config(0));
    let path = write_fixture("disk_blacklist", "<?php $x = 1;");
    set_mtime(&path, 1_000_000);
    load_script(&path).expect("the writer stores version A on disk");
    forget_memory();

    std::fs::write(&path, "B<?php $x = 1;").expect("fixture should be rewritable");
    set_mtime(&path, 900_000);
    let list = cache.join("blacklist.txt");
    let canonical = std::fs::canonicalize(&path).expect("fixture should resolve");
    std::fs::write(&list, format!("{}\n", canonical.display())).expect("blacklist writable");
    crate::script_cache::blacklist::load(&list.to_string_lossy());
    assert!(
        crate::script_cache::blacklist::blocks(&canonical),
        "PREMISE: the fixture must actually be blacklisted"
    );
    set_config(ScriptCacheConfig {
        validate_timestamps: false,
        ..enabled_config(0)
    });

    assert_eq!(shape(&load_script(&path).unwrap()), ["code"], "the stored A is served");
}

/// Verifies a DISK HIT keeps its writer's revalidation deadline instead of a fresh window.
///
/// php-src serializes `revalidate` and its loader leaves it alone. Granting a new window at
/// load masked a change made right after it. MEASURED: written under `revalidate_freq=0`,
/// read two seconds later under `60`, source changed after the load — reference reports the
/// script uncached. And the other direction, written AND read under `60` at once: reference
/// still vouches for it, the stored window being open. Driven by the request clock, so the
/// "two seconds later" is a stamp rather than a sleep.
#[test]
fn a_disk_hit_keeps_its_writers_deadline() {
    let _guard = test_lock();
    with_file_cache("disk_deadline");
    set_config(enabled_config(0));
    stamp_request_time(1_000_000);
    let path = write_fixture("disk_deadline", "<?php $x = 1;");
    set_mtime(&path, 999_000);
    load_script(&path).expect("the writer stores the entry, deadline 1_000_000");
    forget_memory();

    set_config(enabled_config(60));
    stamp_request_time(1_000_005);
    load_script(&path).expect("the reader loads it from disk");
    set_mtime(&path, 900_000);

    assert!(
        !is_cached(&path),
        "the writer's deadline has passed: the moved timestamp must be seen at once"
    );
}

/// Verifies a DISK HIT counts in the script's own `hits`, not only in the aggregate.
///
/// MEASURED: seed the disk, `opcache_compile_file($p)` in a fresh process — reference reports
/// the script's `hits` as 1; elephc reported 0 while its aggregate said 1.
#[test]
fn a_disk_hit_counts_its_first_hit() {
    let _guard = test_lock();
    with_file_cache("disk_hit_count");
    set_config(enabled_config(0));
    let path = write_fixture("disk_hit_count", "<?php $x = 1;");
    set_mtime(&path, 1_000_000);
    load_script(&path).expect("the writer stores the entry on disk");
    forget_memory();

    load_script(&path).expect("the reader loads it from disk");

    let scripts = cached_scripts();
    assert_eq!(scripts.len(), 1, "the disk hit is admitted");
    assert_eq!(
        (scripts[0].hits, stats().hits),
        (1, 1),
        "the script's own counter and the aggregate both count the disk hit"
    );
}

/// Verifies the AGE refusal outranks the SIZE refusal, and counts a miss.
///
/// php-src applies `file_update_protection` before `max_file_size`; the first is a miss, the
/// second a `blacklist_misses`. MEASURED with `max_file_size=1` under a raised protection:
/// reference counts `misses+1 blacklist_misses+0`; elephc counted the opposite.
#[test]
fn the_age_refusal_outranks_the_size_refusal() {
    let _guard = test_lock();
    set_config(ScriptCacheConfig {
        max_file_size: 1,
        file_update_protection: 2_147_483_647,
        ..enabled_config(0)
    });
    let path = write_fixture("age_before_size", "<?php $x = 1;");
    set_mtime(&path, 1_000_000);

    load_script(&path).expect("the refused script still runs");

    let stats = stats();
    assert_eq!((stats.misses, stats.blacklist_misses), (1, 0), "age refuses first");
}

/// Verifies the mtime-`0` refusal outranks the SIZE refusal too. MEASURED the same way.
#[test]
fn the_zero_timestamp_refusal_outranks_the_size_refusal() {
    let _guard = test_lock();
    set_config(ScriptCacheConfig {
        max_file_size: 1,
        ..enabled_config(0)
    });
    let path = write_fixture("zero_before_size", "<?php $x = 1;");
    set_mtime(&path, 0);

    load_script(&path).expect("the refused script still runs");

    let stats = stats();
    assert_eq!((stats.misses, stats.blacklist_misses), (1, 0), "no timestamp refuses first");
}

/// Verifies the SIZE refusal alone still counts a `blacklist_misses`, not a miss.
///
/// The other half of the two tests above, so reordering cannot silently move this case.
/// MEASURED: with the protection off, both engines report `blacklist_misses+1`.
#[test]
fn the_size_refusal_alone_counts_a_blacklist_miss() {
    let _guard = test_lock();
    set_config(ScriptCacheConfig {
        max_file_size: 1,
        ..enabled_config(0)
    });
    let path = write_fixture("size_alone", "<?php $x = 1;");
    set_mtime(&path, 1_000_000);

    load_script(&path).expect("the refused script still runs");

    let stats = stats();
    assert_eq!((stats.misses, stats.blacklist_misses), (0, 1));
}

/// Verifies a pending restart refuses the MEMORY admission but still writes the DISK.
///
/// php-src's compile falls back to `file_cache_compile_file()` while a restart is pending,
/// which stores on disk. MEASURED: `opcache_reset(); opcache_compile_file($p);` — reference
/// answers `true` from the disk query and `false` from the memory one; elephc answered `false`
/// to both.
#[test]
fn a_pending_restart_still_writes_the_disk_cache() {
    let _guard = test_lock();
    with_file_cache("restart_disk");
    set_config(enabled_config(0));
    let path = write_fixture("restart_disk", "<?php $x = 1;");
    set_mtime(&path, 1_000_000);

    schedule_restart();
    assert!(compile_file(&path), "the file compiles");

    assert!(!is_cached(&path), "memory admission is closed while the restart is pending");
    assert!(
        crate::script_cache::file_store::contains(&path),
        "but the disk cache is still written"
    );
}

/// Verifies `invalidate` reports the MEMORY entry it found, never the disk removal.
///
/// php-src's `zend_accel_invalidate` answers `file_found` — the path resolved, or the memory
/// hash held a live entry — and removes the disk entry without reporting it. MEASURED across two
/// processes: seed the disk, unlink the source, `opcache_invalidate($p, true)` in a fresh
/// process — reference `false`, elephc `true`; both removed the disk entry.
#[test]
fn invalidating_a_disk_only_entry_reports_nothing_found() {
    let _guard = test_lock();
    with_file_cache("disk_only_invalidate");
    set_config(enabled_config(0));
    let path = write_fixture("disk_only_invalidate", "<?php $x = 1;");
    set_mtime(&path, 1_000_000);
    load_script(&path).expect("the writer stores the entry on disk");
    forget_memory();
    let key = cache_key(&path);
    std::fs::remove_file(&path).expect("fixture should be removable");
    let unvalidated = ScriptCacheConfig {
        validate_timestamps: false,
        ..enabled_config(0)
    };
    assert!(
        crate::script_cache::file_store::load(&unvalidated, &key, None, 0).is_some(),
        "PREMISE: the entry is on disk and nothing is in memory"
    );

    assert!(!invalidate(&path, true), "no memory entry, and the path no longer resolves");
    assert!(
        crate::script_cache::file_store::load(&unvalidated, &key, None, 0).is_none(),
        "the disk entry is still removed"
    );
}

/// Verifies `file_update_protection` does not refuse an EXISTING disk entry.
///
/// The guard keeps a part-written file out of the cache; a disk hit's bytes were admitted by
/// an earlier compile. php-src loads the file cache before reaching the age check. MEASURED
/// with the protection at its maximum over an unchanged source: reference reports the script
/// cached, elephc refused it.
#[test]
fn a_disk_hit_is_admitted_whatever_the_protection() {
    let _guard = test_lock();
    with_file_cache("disk_protection");
    set_config(enabled_config(0));
    let path = write_fixture("disk_protection", "<?php $x = 1;");
    set_mtime(&path, 1_000_000);
    load_script(&path).expect("the writer stores the entry on disk");
    forget_memory();

    set_config(ScriptCacheConfig {
        file_update_protection: 2_147_483_647,
        ..enabled_config(0)
    });
    load_script(&path).expect("the reader loads it from disk");

    assert!(is_cached(&path), "a disk hit is not a fresh compile; the guard does not apply");
}

/// Verifies a source whose mtime is `0` is run but never cached.
///
/// php-src reads the timestamp before compiling and does not cache a script without one.
/// Admitting it stored timestamp `0`, which also means "unrecorded" here, so it was never
/// revalidated. MEASURED: `touch($p, 0)`, include, rewrite, include — reference reports it
/// uncached and runs the new source; elephc cached it and ran the old one.
#[test]
fn a_source_with_no_timestamp_is_not_cached() {
    let _guard = test_lock();
    set_config(enabled_config(0));
    let path = write_fixture("zero_mtime", "<?php $x = 1;");
    set_mtime(&path, 0);

    load_script(&path).expect("the script still runs");

    assert!(!is_cached(&path), "a script with no timestamp must not be cached");
    assert_eq!(stats().num_cached_scripts, 0);
}

/// Verifies a source whose mtime is `0` IS cached when nothing reads the timestamp.
///
/// php-src only fetches the timestamp for `validate_timestamps`, `file_update_protection` or
/// `max_file_size`; with all three off there is no `0` to refuse. MEASURED: reference caches
/// the file with all three off, and refuses it with any one of them on.
#[test]
fn a_source_with_no_timestamp_is_cached_when_nothing_reads_it() {
    let _guard = test_lock();
    set_config(ScriptCacheConfig {
        validate_timestamps: false,
        max_file_size: 0,
        ..enabled_config(0)
    });
    let path = write_fixture("zero_mtime_unread", "<?php $x = 1;");
    set_mtime(&path, 0);

    load_script(&path).expect("fixture should load");

    assert!(is_cached(&path), "nothing asked for the timestamp, so nothing refused it");
}

/// Verifies a source dated BEFORE 1970 is cached and revalidated like any other.
///
/// `st_mtime` is signed and php-src records a negative one normally. Reading it as "no
/// timestamp" first made the entry unrecorded, then — once a `0` timestamp refused admission
/// — made the file uncacheable. MEASURED with `touch($p, -60)`: reference reports it cached,
/// then runs the rewritten source once its mtime moves.
#[test]
fn a_source_dated_before_the_epoch_is_cached_and_revalidated() {
    let _guard = test_lock();
    set_config(enabled_config(0));
    let path = write_fixture("pre_epoch", "<?php $x = 1;");
    let set_before_epoch = |seconds: u64| {
        std::fs::File::options()
            .write(true)
            .open(&path)
            .expect("fixture should reopen")
            .set_modified(std::time::UNIX_EPOCH - std::time::Duration::from_secs(seconds))
            .expect("mtime should be settable");
    };
    set_before_epoch(60);
    load_script(&path).expect("fixture should load");
    assert!(is_cached(&path), "a negative timestamp is still a timestamp");

    std::fs::write(&path, "B<?php $x = 1;").expect("fixture should be rewritable");
    set_before_epoch(30);
    assert_eq!(
        shape(&load_script(&path).unwrap()),
        ["out", "code"],
        "the moved timestamp is seen and the new source runs"
    );
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
