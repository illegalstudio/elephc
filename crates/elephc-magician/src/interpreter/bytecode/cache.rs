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
pub(in crate::interpreter) fn lookup_executable(code: &[u8]) -> Option<Result<Arc<CompiledModule>, CompileGap>> {
    if code.len() > MAX_CACHEABLE_FRAGMENT_BYTES {
        return None;
    }
    lock_cache().lookup(code)
}

/// Stores a compiled module or a gap under the exact source bytes.
pub(super) fn store_executable(code: &[u8], result: Result<Arc<CompiledModule>, CompileGap>) {
    if code.len() > MAX_CACHEABLE_FRAGMENT_BYTES {
        return;
    }
    lock_cache().insert(code.to_vec(), result);
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

/// Bounded FIFO of compiled modules and negative gap entries.
struct ExecutableCache {
    capacity: usize,
    entries: HashMap<Vec<u8>, Result<Arc<CompiledModule>, CompileGap>>,
    order: VecDeque<Vec<u8>>,
}

impl ExecutableCache {
    /// Creates an empty cache with the requested maximum entry count.
    fn new(capacity: usize) -> Self {
        Self { capacity, entries: HashMap::new(), order: VecDeque::new() }
    }

    /// Returns the stored module or gap for these exact bytes.
    fn lookup(&self, code: &[u8]) -> Option<Result<Arc<CompiledModule>, CompileGap>> {
        self.entries.get(code).cloned()
    }

    /// Inserts a result and evicts the oldest distinct source when full.
    fn insert(&mut self, code: Vec<u8>, result: Result<Arc<CompiledModule>, CompileGap>) {
        if self.capacity == 0 {
            return;
        }
        if self.entries.contains_key(code.as_slice()) {
            self.entries.insert(code, result);
            return;
        }
        while self.entries.len() >= self.capacity {
            let Some(oldest) = self.order.pop_front() else { break };
            self.entries.remove(&oldest);
        }
        self.order.push_back(code.clone());
        self.entries.insert(code, result);
    }
}
