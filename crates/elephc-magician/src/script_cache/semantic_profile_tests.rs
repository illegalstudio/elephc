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

/// Rejects disk entries from another profile, including ABI mismatches before replay.
#[test]
fn strict_cache_disk_headers_require_matching_semantics() {
    let _guard = store::lock_for_test();
    let fixture = Fixture::new();
    let config = fixture.configure(true);
    let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true);
    let segments = segments::segment_script_with_semantics(b"<?php return true;", segments::ParseMode::Fresh, profile);
    file_store::store_entry_with_semantics(&config, &fixture.source, None, 0, 0, &segments, profile);
    assert!(file_store::load_entry_with_semantics(&config, &fixture.source, None, 0, profile).is_some());
    assert!(file_store::load_entry_with_semantics(&config, &fixture.source, None, 0, SemanticProfile::PERMISSIVE).is_none());
    assert!(file_store::contains_with_semantics(&fixture.source, profile));
    assert!(!file_store::contains_with_semantics(&fixture.source, SemanticProfile::PERMISSIVE));
    // Change only the neutral ABI field, leaving the stored IR decodable.
    let directory = std::fs::read_dir(fixture.dir.join("cache")).unwrap().next().unwrap().unwrap().path();
    let entry = std::fs::read_dir(directory).unwrap().next().unwrap().unwrap().path();
    let mut bytes = std::fs::read(&entry).unwrap();
    bytes[12..16].copy_from_slice(&(elephc_semantics::SEMANTICS_ABI_VERSION + 1).to_le_bytes());
    std::fs::write(&entry, bytes).unwrap();
    assert!(file_store::load_entry_with_semantics(&config, &fixture.source, None, 0, profile).is_none());
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
