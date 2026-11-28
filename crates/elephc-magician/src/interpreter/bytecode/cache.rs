//! Purpose:
//! Caches the compiled executable form of an eval fragment, including gaps.
//! Execution still runs effects, declarations, and scope writes on every call.
//!
//! Called from:
//! - `crate::interpreter::bytecode::try_execute()` when a source key is active.
//! - `crate::ffi::execute` and nested `eval()` which install that key.
//!
//! Key details:
//! - The key is the source bytes, not a program address: stack addresses get reused.
//! - Capacity and the 64 KiB ceiling match the parse cache. This cache stores
//!   modules and named gaps, never runtime cells.
//! - An unset key is distinct from an empty fragment. Only the guarded call reads the pointer.

use super::compile::CompileGap;
use elephc_semantics::SemanticProfile;
use crate::eval_ir::CompiledModule;
use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

const EXECUTABLE_CACHE_CAPACITY: usize = 256;
const MAX_CACHEABLE_FRAGMENT_BYTES: usize = 64 * 1024;

thread_local! {
    /// Source bytes of the eval currently executing, installed by a drop guard.
    static CACHE_KEY: Cell<Option<(*const u8, usize)>> = const { Cell::new(None) };
}

/// Restores the previous source key, including when the guarded call panics.
struct CacheKeyGuard(Option<(*const u8, usize)>);

impl Drop for CacheKeyGuard {
    /// Puts back the key that was current before this guard.
    fn drop(&mut self) {
        let previous = self.0.take();
        CACHE_KEY.with(|key| key.set(previous));
    }
}

/// Runs `body` with `code` as the executable-cache key.
pub(crate) fn with_executable_cache_key<T>(code: &[u8], body: impl FnOnce() -> T) -> T {
    let previous = CACHE_KEY.with(|key| key.replace(Some((code.as_ptr(), code.len()))));
    let _guard = CacheKeyGuard(previous);
    body()
}

/// Copies the active source key when it is small enough to cache.
pub(super) fn copy_current_key() -> Option<Vec<u8>> {
    let (ptr, len) = CACHE_KEY.with(|key| key.get())?;
    if len > MAX_CACHEABLE_FRAGMENT_BYTES {
        return None;
    }
    // The guard in `with_executable_cache_key` keeps these bytes alive for this call.
    let code = unsafe { std::slice::from_raw_parts(ptr, len) };
    Some(code.to_vec())
}

/// Returns a cached module or gap for the exact source bytes.
pub(in crate::interpreter) fn lookup_executable(code: &[u8], profile: SemanticProfile) -> Option<Result<Arc<CompiledModule>, CompileGap>> {
    if code.len() > MAX_CACHEABLE_FRAGMENT_BYTES {
        return None;
    }
    lock_cache().lookup(code, CacheProfile::current(profile))
}

/// Stores a compiled module or a gap under the exact source bytes.
pub(super) fn store_executable(code: &[u8], profile: SemanticProfile, result: Result<Arc<CompiledModule>, CompileGap>) {
    if code.len() > MAX_CACHEABLE_FRAGMENT_BYTES {
        return;
    }
    lock_cache().insert(code.to_vec(), CacheProfile::current(profile), result);
}

static EXECUTABLE_CACHE: OnceLock<Mutex<ExecutableCache>> = OnceLock::new();

/// Returns the process-wide executable cache, creating it on first use.
fn executable_cache() -> &'static Mutex<ExecutableCache> {
    EXECUTABLE_CACHE.get_or_init(|| Mutex::new(ExecutableCache::new(EXECUTABLE_CACHE_CAPACITY)))
}

/// Locks the cache and keeps the map when a previous panic poisoned the mutex.
fn lock_cache() -> MutexGuard<'static, ExecutableCache> {
    executable_cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Every profile dimension that can alter compilation or builtin visibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CacheProfile {
    semantics: SemanticProfile,
    php_only: bool,
    php_version: u32,
}

impl CacheProfile {
    /// Combines explicit activation guarantees with the existing physical-source profile.
    fn current(semantics: SemanticProfile) -> Self {
        Self { semantics, php_only: crate::php_only_mode::php_only_mode(),
            php_version: crate::eval_php_profile::eval_php_version_id() }
    }
}

/// One profile's cached executable modules and negative compilation gaps.
type ProfileEntries = HashMap<Vec<u8>, Result<Arc<CompiledModule>, CompileGap>>;

/// Bounded FIFO shared across profiles, with allocation-free borrowed-source lookup.
struct ExecutableCache {
    capacity: usize,
    len: usize,
    entries: HashMap<CacheProfile, ProfileEntries>,
    order: VecDeque<(CacheProfile, Vec<u8>)>,
}

impl ExecutableCache {
    /// Creates an empty cache with a total entry limit across all profiles.
    fn new(capacity: usize) -> Self {
        Self { capacity, len: 0, entries: HashMap::new(), order: VecDeque::new() }
    }

    /// Returns the stored module or gap for these bytes and effective compilation profile.
    fn lookup(&self, code: &[u8], profile: CacheProfile) -> Option<Result<Arc<CompiledModule>, CompileGap>> {
        self.entries.get(&profile)?.get(code).cloned()
    }

    /// Inserts a result and evicts the oldest distinct source/profile pair when full.
    fn insert(&mut self, code: Vec<u8>, profile: CacheProfile, result: Result<Arc<CompiledModule>, CompileGap>) {
        if self.capacity == 0 { return; }
        if let Some(entry) = self.entries.get_mut(&profile).and_then(|entries| entries.get_mut(code.as_slice())) {
            *entry = result;
            return;
        }
        while self.len >= self.capacity {
            let Some((old_profile, old_code)) = self.order.pop_front() else { break };
            if let Some(entries) = self.entries.get_mut(&old_profile) {
                if entries.remove(&old_code).is_some() { self.len -= 1; }
                if entries.is_empty() { self.entries.remove(&old_profile); }
            }
        }
        self.order.push_back((profile, code.clone()));
        self.entries.entry(profile).or_default().insert(code, result);
        self.len += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elephc_semantics::StrictFeature;

    /// Negative compilation results remain profile-local and share one global capacity limit.
    #[test]
    fn strict_profile_bytecode_cache_partitions_gaps_and_capacity() {
        let profile = CacheProfile::current(SemanticProfile::PERMISSIVE);
        let strict = CacheProfile { semantics: profile.semantics.with(StrictFeature::Bools, true), ..profile };
        let php_only = CacheProfile { php_only: !profile.php_only, ..profile };
        let version = CacheProfile { php_version: profile.php_version + 100, ..profile };
        let mut cache = ExecutableCache::new(2);
        let gap = CompileGap::new("strict-profile-test");
        cache.insert(b"same source".to_vec(), profile, Err(gap));
        assert!(cache.lookup(b"same source", strict).is_none());
        assert!(cache.lookup(b"same source", php_only).is_none());
        assert!(cache.lookup(b"same source", version).is_none());
        cache.insert(b"same source".to_vec(), strict, Err(gap));
        assert!(cache.lookup(b"same source", profile).is_some());
        assert!(cache.lookup(b"same source", strict).is_some());
        cache.insert(b"same source".to_vec(), php_only, Err(gap));
        assert!(cache.lookup(b"same source", profile).is_none());
        assert!(cache.lookup(b"same source", strict).is_some());
        assert_eq!(cache.len, 2);
    }
}
