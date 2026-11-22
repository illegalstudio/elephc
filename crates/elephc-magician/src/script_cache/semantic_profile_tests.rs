//! Purpose:
//! Verifies parser and script caches cannot reuse results across semantic profiles.
//!
//! Called from:
//! - The Magician unit test harness.
//!
//! Key details:
//! - One owned fixture directory is removed on both success and panic.
//! - Profile identity is checked even when timestamp validation is disabled.

use super::{config, file_cache, file_store, segments, store};
use elephc_semantics::{SemanticProfile, StrictFeature};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Owns source and disk-cache files for one isolated test.
struct Fixture { dir: PathBuf, source: PathBuf }

impl Fixture {
    /// Creates a unique source which permissive parsing accepts as a float.
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!("elephc-strict-cache-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&dir).unwrap();
        let fixture = Self { source: dir.join("source.php"), dir };
        std::fs::write(&fixture.source, b"<?php return 9223372036854775808;").unwrap();
        fixture
    }

    /// Enables cache admission without timestamp or fresh-file refusals.
    fn configure(&self, enabled: bool) -> config::ScriptCacheConfig {
        let config = config::ScriptCacheConfig { enabled, validate_timestamps: false,
            file_update_protection: 0, ..config::ScriptCacheConfig::disabled() };
        config::set_config(config.clone());
        file_cache::set_file_cache_config(file_cache::FileCacheConfig {
            path: self.dir.join("cache").to_string_lossy().into_owned(), read_only: false,
        });
        config
    }
}

impl Drop for Fixture {
    /// Clears thread-local configuration and removes exactly this fixture's owned directory.
    fn drop(&mut self) {
        config::set_config(config::ScriptCacheConfig::disabled());
        file_cache::set_file_cache_config(file_cache::FileCacheConfig::default());
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Moves `path`'s mtime to an exact whole second for deterministic revalidation.
fn set_mtime(path: &std::path::Path, seconds: u64) {
    let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(when)
        .unwrap();
}

/// Returns the checked-literal policy without requiring runtime activation of a staged flag.
fn checked_literals() -> SemanticProfile {
    SemanticProfile::PERMISSIVE.with(StrictFeature::Overflow, true)
}

/// Pins profile isolation for the byte memo when OPcache is disabled and the path cache when enabled.
#[test]
fn strict_cache_script_profiles_reject_warm_permissive_parses() {
    let _guard = store::lock_for_test();
    let fixture = Fixture::new();
    for enabled in [false, true] {
        fixture.configure(enabled);
        let permissive = store::load_script_with_semantics(&fixture.source, SemanticProfile::PERMISSIVE).unwrap();
        assert!(matches!(permissive.as_ref(), [segments::ScriptSegment::Code(_)]));
        let strict = store::load_script_with_semantics(&fixture.source, checked_literals()).unwrap();
        assert!(matches!(strict.as_ref(), [segments::ScriptSegment::ParseError(crate::errors::EvalParseError::IntegerLiteralOverflow)]));
        assert!(!store::is_cached_with_semantics(&fixture.source, checked_literals()));
        let again = store::load_script_with_semantics(&fixture.source, SemanticProfile::PERMISSIVE).unwrap();
        assert!(matches!(again.as_ref(), [segments::ScriptSegment::Code(_)]));
    }
}

/// A failed parse in one profile cannot discard another profile's timestamp-free script.
#[test]
fn strict_cache_failed_cross_profile_fill_keeps_the_warm_variant() {
    let _guard = store::lock_for_test();
    let fixture = Fixture::new();
    fixture.configure(true);
    std::fs::write(&fixture.source, b"<?php return 1;").unwrap();
    let permissive = store::load_script_with_semantics(
        &fixture.source, SemanticProfile::PERMISSIVE,
    ).unwrap();
    assert!(matches!(permissive.as_ref(), [segments::ScriptSegment::Code(_)]));

    std::fs::write(&fixture.source, b"<?php return 9223372036854775808;").unwrap();
    let strict = store::load_script_with_semantics(&fixture.source, checked_literals()).unwrap();
    assert!(matches!(strict.as_ref(), [segments::ScriptSegment::ParseError(
        crate::errors::EvalParseError::IntegerLiteralOverflow,
    )]));

    let again = store::load_script_with_semantics(
        &fixture.source, SemanticProfile::PERMISSIVE,
    ).unwrap();
    assert!(Arc::ptr_eq(&permissive, &again), "the permissive context must keep its warm script");
}

/// PHP-visible cache status aggregates independently reusable profile variants by path.
#[test]
fn strict_cache_status_aggregates_profile_variants_by_path() {
    let _guard = store::lock_for_test();
    let fixture = Fixture::new();
    fixture.configure(true);
    std::fs::write(&fixture.source, b"<?php return 1;").unwrap();

    for profile in [SemanticProfile::PERMISSIVE, checked_literals()] {
        store::load_script_with_semantics(&fixture.source, profile).unwrap();
        store::load_script_with_semantics(&fixture.source, profile).unwrap();
    }

    let stats = store::stats();
    let scripts = store::cached_scripts();
    assert_eq!(scripts.len(), 1, "OPcache exposes one row for one canonical path");
    assert_eq!(stats.num_cached_scripts, scripts.len(), "the status total matches its rows");
    assert_eq!(scripts[0].hits, stats.hits, "the row sums hits from both profiles");
    assert_eq!(
        scripts[0].memory_consumption, stats.used_memory,
        "the row sums the memory retained by both profiles"
    );
}

/// A stale discarded profile cannot hide or invalidate a fresh live sibling.
#[test]
fn strict_cache_discarded_stale_profile_keeps_fresh_sibling_visible() {
    let _guard = store::lock_for_test();
    let fixture = Fixture::new();
    config::set_config(config::ScriptCacheConfig {
        enabled: true,
        validate_timestamps: true,
        revalidate_freq: 0,
        file_update_protection: 2,
        ..config::ScriptCacheConfig::disabled()
    });
    file_cache::set_file_cache_config(file_cache::FileCacheConfig::default());
    std::fs::write(&fixture.source, b"<?php return 1;").unwrap();
    config::stamp_request_time(1_000_000);
    set_mtime(&fixture.source, 990_000);
    store::load_script_with_semantics(&fixture.source, SemanticProfile::PERMISSIVE).unwrap();

    std::fs::write(&fixture.source, b"<?php return 2;").unwrap();
    set_mtime(&fixture.source, 1_000_000);
    store::load_script_with_semantics(&fixture.source, SemanticProfile::PERMISSIVE).unwrap();

    config::stamp_request_time(1_000_010);
    store::load_script_with_semantics(&fixture.source, checked_literals()).unwrap();
    let scripts = store::cached_scripts();
    assert_eq!(scripts.len(), 1, "discarded and live variants share one visible row");
    assert_eq!(scripts[0].timestamp, 1_000_000, "the live variant supplies freshness");
    assert_eq!(scripts[0].memory_consumption, store::stats().used_memory);

    assert!(store::invalidate(&fixture.source, false));
    assert!(
        store::is_cached_with_semantics(&fixture.source, checked_literals()),
        "the stale discarded variant must not retire its fresh sibling"
    );
}

/// Non-forced invalidation retires a stale live profile without discarding its fresh sibling.
#[test]
fn strict_cache_live_stale_profile_keeps_fresh_sibling_visible() {
    let _guard = store::lock_for_test();
    let fixture = Fixture::new();
    config::set_config(config::ScriptCacheConfig {
        enabled: true,
        validate_timestamps: true,
        revalidate_freq: 0,
        file_update_protection: 0,
        ..config::ScriptCacheConfig::disabled()
    });
    file_cache::set_file_cache_config(file_cache::FileCacheConfig::default());
    config::stamp_request_time(1_000_000);
    std::fs::write(&fixture.source, b"<?php return 1;").unwrap();
    set_mtime(&fixture.source, 990_000);
    store::load_script_with_semantics(
        &fixture.source,
        SemanticProfile::PERMISSIVE,
    )
    .unwrap();

    std::fs::write(&fixture.source, b"<?php return 2;").unwrap();
    set_mtime(&fixture.source, 1_000_000);
    store::load_script_with_semantics(&fixture.source, checked_literals()).unwrap();

    assert!(store::invalidate(&fixture.source, false));
    assert!(
        store::is_cached_with_semantics(&fixture.source, checked_literals()),
        "a stale live profile must not retire its fresh sibling",
    );
    assert!(
        !store::is_cached_with_semantics(&fixture.source, SemanticProfile::PERMISSIVE),
        "the stale live profile must be retired",
    );
}

/// Missing files retire timestamp-free profiles instead of treating two zeroes as fresh.
#[test]
fn strict_cache_deleted_file_discards_unrecorded_profile_sibling() {
    let _guard = store::lock_for_test();
    let fixture = Fixture::new();
    config::set_config(config::ScriptCacheConfig {
        enabled: true,
        validate_timestamps: true,
        revalidate_freq: 0,
        file_update_protection: 0,
        ..config::ScriptCacheConfig::disabled()
    });
    file_cache::set_file_cache_config(file_cache::FileCacheConfig::default());
    config::stamp_request_time(1_000_000);
    std::fs::write(&fixture.source, b"<?php return 1;").unwrap();
    set_mtime(&fixture.source, 990_000);
    store::load_script_with_semantics(
        &fixture.source,
        SemanticProfile::PERMISSIVE,
    )
    .unwrap();

    config::set_config(config::ScriptCacheConfig {
        enabled: true,
        validate_timestamps: false,
        revalidate_freq: 0,
        file_update_protection: 0,
        ..config::ScriptCacheConfig::disabled()
    });
    store::load_script_with_semantics(&fixture.source, checked_literals()).unwrap();
    std::fs::remove_file(&fixture.source).unwrap();
    config::set_config(config::ScriptCacheConfig {
        enabled: true,
        validate_timestamps: true,
        revalidate_freq: 0,
        file_update_protection: 0,
        ..config::ScriptCacheConfig::disabled()
    });

    assert!(store::invalidate(&fixture.source, false));
    assert!(
        !store::is_cached_with_semantics(&fixture.source, checked_literals()),
        "a timestamp-free profile must not survive deletion",
    );
    assert!(
        !store::is_cached_with_semantics(&fixture.source, SemanticProfile::PERMISSIVE),
        "the timestamped sibling is stale after deletion",
    );
}

/// Rejects disk entries from another profile, including ABI mismatches before replay.
#[test]
fn strict_cache_disk_headers_require_matching_semantics() {
    let _guard = store::lock_for_test();
    let fixture = Fixture::new();
    let config = fixture.configure(true);
    let source = std::fs::canonicalize(&fixture.source).unwrap();
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true);
    let segments = segments::segment_script_with_semantics(b"<?php return true;", segments::ParseMode::Fresh, profile);
    file_store::store_entry_with_semantics(&config, &source, None, 0, 0, &segments, profile);
    assert!(file_store::load_entry_with_semantics(&config, &source, None, 0, profile).is_some());
    assert!(file_store::load_entry_with_semantics(&config, &source, None, 0, SemanticProfile::PERMISSIVE).is_none());
    assert!(file_store::contains_with_semantics(&source, profile));
    assert!(!file_store::contains_with_semantics(&source, SemanticProfile::PERMISSIVE));
    // Change only the neutral ABI field, leaving the stored IR decodable.
    let directory = std::fs::read_dir(fixture.dir.join("cache")).unwrap().next().unwrap().unwrap().path();
    let entry = std::fs::read_dir(directory).unwrap().next().unwrap().unwrap().path();
    let mut bytes = std::fs::read(&entry).unwrap();
    bytes[16..24].copy_from_slice(&0_u64.to_le_bytes());
    std::fs::write(&entry, &bytes).unwrap();
    assert!(
        file_store::load_entry_with_semantics(
            &config,
            &source,
            None,
            0,
            profile,
        )
        .is_none(),
        "the requested filename must not bypass a mismatched profile header"
    );
    bytes[16..24].copy_from_slice(&profile.bits().to_le_bytes());
    bytes[12..16].copy_from_slice(&(elephc_semantics::SEMANTICS_ABI_VERSION + 1).to_le_bytes());
    std::fs::write(&entry, bytes).unwrap();
    assert!(file_store::load_entry_with_semantics(&config, &source, None, 0, profile).is_none());
}

/// Disk cache variants keep separate files and path invalidation removes all of them.
#[test]
fn strict_cache_disk_profiles_keep_separate_files_and_invalidate_together() {
    let _guard = store::lock_for_test();
    let fixture = Fixture::new();
    let config = fixture.configure(true);
    std::fs::write(&fixture.source, b"<?php return 1;").unwrap();
    let permissive = segments::segment_script_with_semantics(
        b"<?php return 1;",
        segments::ParseMode::Fresh,
        SemanticProfile::PERMISSIVE,
    );
    let strict = segments::segment_script_with_semantics(
        b"<?php return 1;",
        segments::ParseMode::Fresh,
        checked_literals(),
    );

    file_store::store_entry_with_semantics(
        &config,
        &fixture.source,
        None,
        0,
        0,
        &permissive,
        SemanticProfile::PERMISSIVE,
    );
    file_store::store_entry_with_semantics(
        &config,
        &fixture.source,
        None,
        0,
        0,
        &strict,
        checked_literals(),
    );

    assert!(
        file_store::load_entry_with_semantics(
            &config,
            &fixture.source,
            None,
            0,
            SemanticProfile::PERMISSIVE,
        )
        .is_some()
    );
    assert!(
        file_store::load_entry_with_semantics(
            &config,
            &fixture.source,
            None,
            0,
            checked_literals(),
        )
        .is_some()
    );
    let directory = std::fs::read_dir(fixture.dir.join("cache"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);

    assert!(file_store::invalidate(&config, &fixture.source));
    assert_eq!(std::fs::read_dir(directory).unwrap().count(), 0);
}

/// Checks that native OPcache entry points pass and validate the explicit hidden ABI.
#[test]
fn strict_cache_native_entry_points_validate_profile_identity() {
    use crate::ffi::context::{__elephc_eval_opcache_rt_compile_profiled, __elephc_eval_opcache_rt_is_cached_profiled};
    let _guard = store::lock_for_test();
    let fixture = Fixture::new();
    fixture.configure(true);
    let path = fixture.source.to_str().unwrap().as_bytes();
    let abi = u64::from(elephc_semantics::SEMANTICS_ABI_VERSION);
    unsafe {
        assert_eq!(__elephc_eval_opcache_rt_compile_profiled(path.as_ptr(), path.len() as u64, abi, 0), 1);
        assert_eq!(__elephc_eval_opcache_rt_is_cached_profiled(path.as_ptr(), path.len() as u64, abi, checked_literals().bits()), 0);
        assert_eq!(__elephc_eval_opcache_rt_compile_profiled(path.as_ptr(), path.len() as u64, abi, checked_literals().bits()), 0);
        assert_eq!(__elephc_eval_opcache_rt_compile_profiled(path.as_ptr(), path.len() as u64, abi + 1, 0), 0);
        assert_eq!(__elephc_eval_opcache_rt_compile_profiled(path.as_ptr(), path.len() as u64, abi, 1 << 63), 0);
    }
}
