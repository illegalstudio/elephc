//! Purpose:
//! The process-wide runtime script cache: one entry per canonical path holding the
//! segmented, parsed form of a dynamically included PHP file, plus the statistics
//! the OPcache API reports about it.
//!
//! Called from:
//! - `crate::interpreter::include_exec` for every runtime include/require.
//! - `crate::script_cache` re-exports, which the OPcache bridge symbols read.
//!
//! Key details:
//! - The cache is a no-op unless `ScriptCacheConfig::enabled`, which mirrors
//!   `opcache_cache_enabled`. A default CLI binary therefore behaves exactly as it
//!   did before this module existed.
//! - Freshness follows php-src, not "always re-read": on fill an entry records
//!   `revalidate_at = now + opcache.revalidate_freq` and is only re-`stat`ed once
//!   that instant has passed, so a changed file may serve stale for up to
//!   `revalidate_freq` seconds. `opcache.validate_timestamps = 0` never re-stats.
//! - The cache NEVER evicts. php-src refuses new entries once the budget or the
//!   entry ceiling is reached and latches `cache_full`; reproducing that is both
//!   simpler and more faithful than an LRU.
//! - A forced `opcache_invalidate()` marks an entry discarded rather than removing
//!   it, matching php-src keeping the shared-memory slot until the next restart —
//!   and matching what the compile-time manifest emulation already does.

use super::config::{config, ScriptCacheConfig};
use super::segments::{segment_script, ParseMode, ScriptSegment};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// One cached script: its replayable segments plus the freshness and accounting state.
#[derive(Debug)]
struct Entry {
    segments: Arc<[ScriptSegment]>,
    mtime: Option<i64>,
    size: u64,
    footprint: usize,
    hits: u64,
    last_used: i64,
    revalidate_at: i64,
    discarded: bool,
}

/// A read-only view of one cached script, for the `opcache_get_status()` surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedScriptInfo {
    pub full_path: String,
    pub hits: u64,
    pub memory_consumption: usize,
    pub last_used_timestamp: i64,
    pub timestamp: i64,
}

/// The counters `opcache_get_status()` reports for the dynamic tier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub num_cached_scripts: usize,
    pub used_memory: usize,
    pub cache_full: bool,
    pub oom_restarts: u64,
    pub manual_restarts: u64,
    pub last_restart_time: i64,
    pub restart_pending: bool,
    /// `opcache.blacklist_filename` refusals — one per script run but not stored.
    pub blacklist_misses: u64,
}

/// The process-wide cache state.
#[derive(Debug, Default)]
struct ScriptCache {
    entries: HashMap<PathBuf, Entry>,
    hits: u64,
    misses: u64,
    used_memory: usize,
    cache_full: bool,
    oom_restarts: u64,
    manual_restarts: u64,
    blacklist_misses: u64,
    /// Bumped on every mutation, so a reader can tell whether a snapshot it took is still
    /// current. See `generation`.
    generation: u64,
    last_restart_time: i64,
    restart_pending: bool,
}

static SCRIPT_CACHE: OnceLock<Mutex<ScriptCache>> = OnceLock::new();

/// Returns the process-wide script cache singleton.
fn script_cache() -> &'static Mutex<ScriptCache> {
    SCRIPT_CACHE.get_or_init(|| Mutex::new(ScriptCache::default()))
}

/// Locks the cache, recovering the inner state if a previous panic poisoned it.
fn lock_script_cache() -> MutexGuard<'static, ScriptCache> {
    script_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Returns the current wall clock in whole seconds since the Unix epoch.
fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// Returns a file's mtime in whole seconds since the Unix epoch, if it has one.
pub(super) fn mtime_seconds(metadata: &std::fs::Metadata) -> Option<i64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|elapsed| elapsed.as_secs() as i64)
}

/// Loads a script's replayable segments, serving the cache when it is warm.
///
/// Returns the same `io::Error` `std::fs::read` would have produced for a file that
/// cannot be opened, so the caller's missing-include diagnostics are unchanged. When
/// the cache is disabled this reads and segments the file exactly as the uncached
/// path always did, storing nothing.
pub(crate) fn load_script(path: &Path) -> io::Result<Arc<[ScriptSegment]>> {
    let config = config();
    if !config.enabled {
        // No entry will hold this result, so the byte-keyed parse memo is what keeps a
        // repeated include off the parser — exactly as the inline loop did before.
        let bytes = std::fs::read(path)?;
        return Ok(Arc::from(segment_script(&bytes, ParseMode::Memoized)));
    }
    let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(segments) = serve_warm_entry(&key, &config)? {
        return Ok(segments);
    }
    fill_entry(&key, path, &config)
}

/// Returns a warm entry's segments when one is present and still considered fresh.
///
/// Answers `None` when the entry is absent, discarded, or has failed revalidation —
/// each of which must fall through to a fill.
fn serve_warm_entry(
    key: &Path,
    config: &ScriptCacheConfig,
) -> io::Result<Option<Arc<[ScriptSegment]>>> {
    let now = now_seconds();
    let mut cache = lock_script_cache();
    let Some(entry) = cache.entries.get(key) else {
        return Ok(None);
    };
    if entry.discarded {
        return Ok(None);
    }
    if config.validate_timestamps && now >= entry.revalidate_at {
        let Some(metadata) = std::fs::metadata(key).ok() else {
            // The file is gone. Fall through to the fill, which reproduces the
            // caller's missing-include diagnostics rather than serving stale code.
            return Ok(None);
        };
        if mtime_seconds(&metadata) != entry.mtime || metadata.len() != entry.size {
            return Ok(None);
        }
        let freq = config.revalidate_freq as i64;
        let entry = cache.entries.get_mut(key).expect("entry was just observed");
        entry.revalidate_at = now.saturating_add(freq);
    }
    let entry = cache.entries.get_mut(key).expect("entry was just observed");
    entry.hits += 1;
    entry.last_used = now;
    let segments = Arc::clone(&entry.segments);
    cache.hits += 1;
    // A WARM HIT IS A MUTATION TOO. It moves this entry's `hits` and `last_used`, both of
    // which `opcache_get_status()['scripts']` reports, so a snapshot taken before it is now
    // stale. Missing this bump made the status surface report a cached script's hit count as
    // whatever it was at the FIRST status call of the process, while the aggregate `hits`
    // beside it — read from `stats()` rather than the snapshot — kept counting.
    cache.generation = cache.generation.wrapping_add(1);
    Ok(Some(segments))
}

/// Reads, segments and admits a script, returning its segments either way.
///
/// A file the budget or the entry ceiling refuses is still returned to the caller:
/// refusing to CACHE is not refusing to RUN, which is php-src's behaviour once the
/// cache is full.
fn fill_entry(
    key: &Path,
    path: &Path,
    config: &ScriptCacheConfig,
) -> io::Result<Arc<[ScriptSegment]>> {
    let bytes = std::fs::read(path)?;
    let metadata = std::fs::metadata(path).ok();
    let mtime = metadata.as_ref().and_then(mtime_seconds);
    let file_size = metadata.as_ref().map_or(bytes.len() as u64, |meta| meta.len());
    // `opcache.blacklist_filename` is decided FIRST, and the ordering is the contract, not
    // a preference. php-src hands a blacklisted file straight back to the original compiler
    // before any cache accounting, so such a script: runs normally, is stored NOWHERE — not
    // in the memory cache and not in `opcache.file_cache` either — and counts as a
    // `blacklist_misses` INSTEAD of a `misses`. VERIFIED against reference PHP 8.5.10, where
    // including a blacklisted file left `misses` untouched and moved only `blacklist_misses`,
    // and where including it twice counted TWO refusals: the counter is of refusals, not of
    // distinct files.
    if super::blacklist::blocks(key) {
        let segments: Arc<[ScriptSegment]> = Arc::from(segment_script(&bytes, ParseMode::Fresh));
        lock_script_cache().blacklist_misses += 1;
        return Ok(segments);
    }
    let size = file_size;
    // The SIZE refusal comes before any accounting, exactly like the blacklist one above and
    // for the same reason: php-src counts an oversized file as a `blacklist_misses` and NOT
    // as a miss. VERIFIED on reference PHP 8.5.10 — `-d opcache.max_file_size=50` over two
    // oversized scripts reports `misses=0 blacklist_misses=2`. The counter's name is
    // php-src's; what it means is "compiled but deliberately not stored", which a size
    // refusal is. Placing it after `misses += 1` made elephc report BOTH.
    if !config.admits_size(size) {
        let segments: Arc<[ScriptSegment]> = Arc::from(segment_script(&bytes, ParseMode::Fresh));
        lock_script_cache().blacklist_misses += 1;
        return Ok(segments);
    }
    // The FILE CACHE is consulted before the parser. It holds this script already parsed,
    // and only hands it back when the source's mtime, size and canonical path still match
    // what was stored — so a hit is the same segments a parse would produce, for roughly a
    // quarter of the cost (see `file_store`). A miss, a stale entry or any I/O failure all
    // fall through to the parse below.
    let from_disk = super::file_store::load(config, key, mtime, file_size);
    let parsed_here = from_disk.is_none();
    let segments: Arc<[ScriptSegment]> = match from_disk {
        Some(cached) => Arc::from(cached),
        None => Arc::from(segment_script(&bytes, ParseMode::Fresh)),
    };
    let now = now_seconds();
    let mut cache = lock_script_cache();
    // A SECOND-LEVEL HIT IS A HIT. php-src's `persistent_compile_file` only reaches
    // `ZCSG(misses)++` when the file cache did NOT produce a script; a load from it falls
    // into the same branch as a shared-memory hit and bumps `hits`. VERIFIED on reference PHP
    // 8.5.10: two runs against one `opcache.file_cache` directory report `hits=0 misses=2`
    // cold and `hits=2 misses=0` warm. Counting it as a miss reported the exact opposite of
    // reference in the recycled-worker case the file cache exists for.
    if parsed_here {
        cache.misses += 1;
    } else {
        cache.hits += 1;
    }
    // A file younger than `opcache.file_update_protection` is RUN but not STORED, so a file
    // caught part-written never becomes a cached entry that outlives the write. Unlike the
    // size refusal this one DOES count a miss — VERIFIED: with the guard raised, including a
    // fresh file moves `misses` 1 -> 2 and leaves `blacklist_misses` at 0, while
    // `num_cached_scripts` stays put.
    if !config.admits_age(mtime, now) {
        return Ok(segments);
    }
    if parsed_here {
        // Only a script this process actually parsed is written back, and only once the
        // refusals above have passed. Writing before them persisted files those very rules
        // exist to keep out — an age refusal would still have left a part-written file in
        // the on-disk cache, which is precisely what `file_update_protection` is for.
        // Re-writing one that came FROM the cache would be pure I/O for a byte-identical file.
        super::file_store::store(config, key, mtime, file_size, &segments);
    }
    let footprint: usize = segments
        .iter()
        .map(ScriptSegment::memory_footprint)
        .sum::<usize>();
    let replacing = cache.entries.get(key).map_or(0, |entry| entry.footprint);
    if !cache.admits_entry(key, footprint, replacing, config) {
        return Ok(segments);
    }
    cache.used_memory = cache.used_memory - replacing + footprint;
    cache.generation = cache.generation.wrapping_add(1);
    cache.entries.insert(
        key.to_path_buf(),
        Entry {
            segments: Arc::clone(&segments),
            mtime,
            size,
            footprint,
            hits: 0,
            last_used: now,
            revalidate_at: now.saturating_add(config.revalidate_freq as i64),
            discarded: false,
        },
    );
    Ok(segments)
}

impl ScriptCache {
    /// Returns whether one more entry fits, latching `cache_full` when it does not.
    ///
    /// php-src stops storing new scripts once either the byte budget or the entry
    /// ceiling is reached and never evicts, so this refuses rather than making room.
    /// Replacing an existing entry is always allowed: it frees its own footprint.
    fn admits_entry(
        &mut self,
        key: &Path,
        footprint: usize,
        replacing: usize,
        config: &ScriptCacheConfig,
    ) -> bool {
        let replacement = self.entries.contains_key(key);
        if !replacement && self.entries.len() >= config.max_accelerated_files {
            self.cache_full = true;
            return false;
        }
        if self.used_memory - replacing + footprint > config.memory_consumption {
            self.cache_full = true;
            return false;
        }
        true
    }
}

/// Schedules the restart `opcache_reset()` asks for, and returns what it should report.
///
/// `true` on the FIRST call, `false` on every call after it, because php-src's
/// `zend_accel_schedule_restart()` sets `ZCSG(restart_pending)` AND clears the shared
/// `accelerator_enabled` flag that `opcache_reset()`'s own guard tests — so a second call
/// in the same request takes the `false` exit.
///
/// DIVERGENCE, deliberate and narrow: php-src defers the actual flush to the start of the
/// next request, so within one request its cache keeps answering. elephc flushes HERE.
/// The request boundary that would carry a deferred flush lives in `elephc-web`, which does
/// not (and should not) depend on the interpreter crate, so deferring would mean the flush
/// never happening at all — an `opcache_reset()` that cannot reset. Flushing now honours the
/// call; the only observable difference is `opcache_is_script_cached()` immediately after a
/// reset in the same request.
///
/// The byte budget and the `cache_full` latch are released with the entries: a restart is
/// exactly what clears them in php-src too.
pub fn schedule_restart() -> bool {
    let mut cache = lock_script_cache();
    if cache.restart_pending {
        return false;
    }
    // SCHEDULES, and does nothing else. php-src's `zend_accel_schedule_restart` sets the
    // flag and defers the restart itself to the next request, so within THIS one the cache
    // keeps answering and every figure stays put. VERIFIED on reference PHP 8.5.10: right
    // after `opcache_reset()`, `opcache_is_script_cached()` is still true,
    // `num_cached_scripts` is unchanged, `manual_restarts` is still 0 and
    // `last_restart_time` is still 0. [`apply_pending_restart`] is what moves all four.
    cache.restart_pending = true;
    true
}

/// Performs a scheduled restart, if one is pending. Returns whether anything was flushed.
///
/// This is the deferred half of `opcache_reset()`, and it runs at a REQUEST BOUNDARY —
/// generated code calls it at the top of each `--web` request. A CLI program is one
/// request, so it never runs there, which is exactly right: reference PHP would restart at
/// the next request, and a CLI process has none.
///
/// Clearing the latch is what lets a LATER request schedule its own restart again, matching
/// php-src, where the second `opcache_reset()` of one request fails but the next request's
/// succeeds.
pub fn apply_pending_restart() -> bool {
    let mut cache = lock_script_cache();
    if !cache.restart_pending {
        return false;
    }
    cache.generation = cache.generation.wrapping_add(1);
    // php-src's restart runs `zend_reset_cache_vars()`, which ZEROES the lookup counters
    // along with flushing the entries — the restart starts a fresh accounting period, not
    // just a fresh cache. VERIFIED on reference PHP 8.5.10 through `php -S`, which runs many
    // requests in one process so the deferred restart is actually performed: a request
    // reporting `hits=4 misses=2` before the reset is followed, after the restart, by
    // `hits=0` plus only the misses that request itself incurred. Carrying them over left
    // every later request — and both ratios — inflated for the life of the worker.
    cache.hits = 0;
    cache.misses = 0;
    cache.blacklist_misses = 0;
    cache.entries.clear();
    cache.used_memory = 0;
    cache.cache_full = false;
    cache.manual_restarts += 1;
    cache.last_restart_time = now_seconds();
    cache.restart_pending = false;
    true
}

/// Marks a cached script discarded, as a forced `opcache_invalidate()` does.
///
/// Returns whether an entry was present to discard. The entry keeps its slot and its
/// accounted footprint, matching php-src holding the shared-memory block until the
/// next restart, so `num_cached_scripts` does not move.
pub fn discard(path: &Path) -> bool {
    let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut cache = lock_script_cache();
    // The mutable borrow of `entries` has to end before `generation` can be touched, so the
    // flag is read back rather than bumped inside the match arm.
    let discarded = match cache.entries.get_mut(&key) {
        Some(entry) => {
            entry.discarded = true;
            true
        }
        None => false,
    };
    if discarded {
        cache.generation = cache.generation.wrapping_add(1);
    }
    discarded
}

/// Returns whether a path has a live (present, non-discarded) cache entry.
pub fn is_cached(path: &Path) -> bool {
    let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    lock_script_cache()
        .entries
        .get(&key)
        .is_some_and(|entry| !entry.discarded)
}

/// Reads and caches a script without executing it, as `opcache_compile_file()` does.
///
/// Returns whether the file could be read and parsed. A file that parses but the budget
/// refuses still reports success: php-src reports the COMPILE, not the store.
///
/// A discarded entry is re-admitted by the fill itself, which inserts a fresh entry with
/// the latch clear. Removing it first would be worse than redundant: the removal does not
/// return the entry's footprint to the budget, so the refill would count the same bytes
/// twice and the entry would be weighed against `max_accelerated_files` as a NEW one.
pub fn compile_file(path: &Path) -> bool {
    let config = config();
    if !config.enabled {
        return false;
    }
    let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    fill_entry(&key, path, &config).is_ok()
}

/// Returns a counter that changes whenever the cache's entries change.
///
/// The per-script readers snapshot the cache to answer one index at a time; comparing this
/// tells them whether a snapshot they already hold is still good, which is what keeps
/// `opcache_get_status()` from rebuilding it once per field.
pub fn generation() -> u64 {
    lock_script_cache().generation
}

/// Returns the aggregate counters for the `opcache_get_status()` surface.
pub fn stats() -> ScriptCacheStats {
    let cache = lock_script_cache();
    ScriptCacheStats {
        hits: cache.hits,
        misses: cache.misses,
        num_cached_scripts: cache.entries.len(),
        used_memory: cache.used_memory,
        cache_full: cache.cache_full,
        oom_restarts: cache.oom_restarts,
        manual_restarts: cache.manual_restarts,
        last_restart_time: cache.last_restart_time,
        restart_pending: cache.restart_pending,
        blacklist_misses: cache.blacklist_misses,
    }
}

/// Returns one entry per cached script, sorted by path for a deterministic surface.
///
/// A discarded entry reports `timestamp = 0`, which is the single field php-src moves
/// on a discard and the one the compile-time manifest emulation already moves.
pub fn cached_scripts() -> Vec<CachedScriptInfo> {
    let cache = lock_script_cache();
    let mut scripts: Vec<CachedScriptInfo> = cache
        .entries
        .iter()
        .map(|(path, entry)| CachedScriptInfo {
            full_path: path.to_string_lossy().into_owned(),
            hits: entry.hits,
            memory_consumption: entry.footprint,
            last_used_timestamp: entry.last_used,
            timestamp: if entry.discarded { 0 } else { entry.mtime.unwrap_or(0) },
        })
        .collect();
    scripts.sort_by(|left, right| left.full_path.cmp(&right.full_path));
    scripts
}

/// Serializes tests against the process-wide cache singleton.
#[cfg(test)]
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Takes the cache test lock and resets the cache and its counters to an empty state.
///
/// The cache is a process-wide singleton while its configuration is thread-local, so any
/// test that asserts a counter must both serialize against other tests and start from a
/// known state. Shared with the interpreter's OPcache tests, which drive the same cache
/// through the eval surface.
#[cfg(test)]
pub(crate) fn lock_for_test() -> MutexGuard<'static, ()> {
    let guard = TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut cache = lock_script_cache();
    *cache = ScriptCache::default();
    guard
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
