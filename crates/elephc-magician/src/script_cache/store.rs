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
    /// php-src's `persistent_script->timestamp`: the source's mtime when the entry was
    /// admitted, or `0` when it was admitted under `validate_timestamps=0` — which records
    /// no timestamp at all. See `is_unrecorded`.
    timestamp: i64,
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
    /// The entry's own `revalidate_at`, not a figure derived from `last_used`.
    pub revalidate_at: i64,
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

/// Fixes the request time at the current wall clock. See `config::REQUEST_TIME`.
pub fn stamp_request_time_now() {
    super::config::stamp_request_time(now_seconds());
}

/// The clock every FRESHNESS decision reads: the request time when one is stamped, the
/// wall clock otherwise (a configuration installed directly, as the unit tests do).
fn request_now() -> i64 {
    super::config::request_time().unwrap_or_else(now_seconds)
}

/// The cache key for `path`: its canonical form, resolving the EXISTING PREFIX when the file
/// itself is gone.
///
/// Entries are stored under the canonical path, so a lookup must canonicalize the same way.
/// For a file that exists that is `canonicalize`. For a DELETED file it fails — and the
/// fallback used to be the raw path, which misses the stored key whenever that path runs
/// through a symlink. That is not exotic: macOS's `sys_get_temp_dir()` is `/var/folders/…`,
/// a symlink to `/private/var/folders/…`, and so is any deploy that includes through a
/// `current -> releases/42` link.
///
/// php-src resolves the directory that still exists and joins the file name, so it finds the
/// entry. MEASURED: include a file through a symlinked directory, `unlink()` it, then
/// `opcache_invalidate($p, true)` through the same path — reference answers `true`, elephc
/// answered `false`. `invalidate` of a deleted file is exactly when this matters.
fn cache_key(path: &Path) -> std::path::PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => std::fs::canonicalize(parent)
            .map(|dir| dir.join(name))
            .unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
}

/// A file's timestamp as php-src's `do_validate_timestamps` reads it: its mtime, or `0` when
/// it cannot be stated — which never matches a recorded timestamp, so a vanished file fails.
fn file_timestamp(path: &Path) -> i64 {
    std::fs::metadata(path)
        .ok()
        .and_then(|metadata| mtime_seconds(&metadata))
        .unwrap_or(0)
}

/// Whether an entry carries NO recorded timestamp, and so is never revalidated.
///
/// php-src records `timestamp` and `revalidate` only when `validate_timestamps` is on at
/// admission, and `validate_timestamp_and_record` answers SUCCESS for `timestamp == 0`
/// without looking at the file. So an entry admitted with validation off stays trusted by
/// includes and `opcache_is_script_cached()` after validation is turned back on — MEASURED,
/// reference reports it cached — while a non-forced `opcache_invalidate()`, which calls
/// `do_validate_timestamps` directly, compares that `0` against the real mtime and retires
/// it. MEASURED: reference answers `cached=0` after that invalidate; elephc, which recorded
/// the mtime regardless, kept the entry.
fn is_unrecorded(timestamp: i64) -> bool {
    timestamp == 0
}

/// Whether an entry checked at `revalidate_at` must be re-stat'd now.
///
/// `revalidate_freq=0` FORCES the check whatever deadline the entry carries. php-src tests
/// the CURRENT frequency before the stored deadline, so an `ini_set('opcache.revalidate_freq',
/// '0')` takes effect at once; reading only the deadline — computed under the frequency in
/// force when the entry was stored — kept serving an entry for up to the OLD window.
/// MEASURED: cache under `60`, move the mtime, `ini_set(...,'0')`; reference reports the
/// script uncached, elephc reported it cached.
///
/// THE DEADLINE ITSELF IS STILL INSIDE THE WINDOW. php-src skips the check while
/// `revalidate >= request_time`, so the stat is due only once the request time has passed
/// the deadline — `>=` here re-stat'd one second early, at exactly `stored + freq`.
fn revalidation_due(config: &ScriptCacheConfig, revalidate_at: i64, now: i64) -> bool {
    config.validate_timestamps && (config.revalidate_freq == 0 || now > revalidate_at)
}

/// Returns a file's mtime in whole seconds since the Unix epoch, if it has one.
///
/// SIGNED, as `st_mtime` is. A file dated before 1970 has a NEGATIVE timestamp, and php-src
/// records and compares it like any other. Answering `None` there made the entry's timestamp
/// `0`, which here also means "unrecorded" (see `is_unrecorded`), so such a file was never
/// revalidated — and since `0` now refuses admission outright, it would not have been cached
/// at all. Floored like `st_mtime`: half a second before the epoch is `-1`, not `0`.
pub(super) fn mtime_seconds(metadata: &std::fs::Metadata) -> Option<i64> {
    let modified = metadata.modified().ok()?;
    match modified.duration_since(UNIX_EPOCH) {
        Ok(elapsed) => i64::try_from(elapsed.as_secs()).ok(),
        Err(before) => {
            let before = before.duration();
            let whole = i64::try_from(before.as_secs()).ok()?;
            Some(-whole - i64::from(before.subsec_nanos() > 0))
        }
    }
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
    let key = cache_key(path);
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
    let now = request_now();
    let mut cache = lock_script_cache();
    let Some(entry) = cache.entries.get(key) else {
        return Ok(None);
    };
    if entry.discarded {
        return Ok(None);
    }
    if !is_unrecorded(entry.timestamp) && revalidation_due(&config, entry.revalidate_at, now) {
        // THE TIMESTAMP ALONE, as php-src's `do_validate_timestamps` is (it passes a NULL
        // size output). Comparing the length too re-read a file whose content changed but
        // whose mtime was restored — MEASURED, reference keeps serving the stored script.
        // A file that is gone fails too, and the fill reproduces the caller's
        // missing-include diagnostics.
        if file_timestamp(key) != entry.timestamp {
            // A FAILED REVALIDATION DISCARDS THE ENTRY, before the fill decides whether
            // the new version may take its place. php-src calls
            // `zend_accel_lock_discard_script` right there, so when the fill then REFUSES
            // the replacement — `file_update_protection` catching a fresh rewrite — the old
            // script is not left live behind it. Leaving it made the next include serve the
            // OLD version once `validate_timestamps` was turned off. MEASURED: reference
            // printed `B B`, elephc `B A`.
            let entry = cache.entries.get_mut(key).expect("entry was just observed");
            entry.discarded = true;
            cache.generation = cache.generation.wrapping_add(1);
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
    // ONE HANDLE FOR BOTH the bytes and the metadata that will be stored beside them.
    //
    // Reading the file and then stat'ing the PATH is two lookups of a name that can change
    // in between: a rewrite landing in that window records the NEW mtime and size over the
    // OLD bytes, and the same pair is handed to `file_store::store`, so the mismatched entry
    // survives the process and every later validation agrees with it. php-src takes the
    // timestamp from the handle it compiles, for exactly this reason.
    //
    // `opcache.file_update_protection` narrows the window rather than closing it — a file
    // whose mtime is too young is not stored — so the default of 2 seconds hides this, and
    // the documented way to turn that guard off reopens it.
    let mut file = std::fs::File::open(path)?;
    let metadata = file.metadata().ok();
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut bytes)?;
    let mtime = metadata.as_ref().and_then(mtime_seconds);
    let file_size = metadata.as_ref().map_or(bytes.len() as u64, |meta| meta.len());
    // The FILE CACHE is consulted FIRST, before the parser and before the two refusals below.
    // It holds this script already parsed, and only hands it back when the source's canonical
    // path matches and, under validation, its timestamp — so a hit is the same segments a parse
    // would produce, for roughly a quarter of the cost (see `file_store`). A miss, a stale
    // entry or any I/O failure all fall through to the compile path.
    //
    // FIRST, because php-src's `persistent_compile_file` loads the second-level cache before
    // it enters the compile path, and the blacklist and `max_file_size` refusals live INSIDE
    // that path: they decide whether a script may be COMPILED into the cache, not whether an
    // entry already stored may be served. MEASURED across two processes, the reader running
    // `validate_timestamps=0` with `max_file_size=1` — and again with the file blacklisted —
    // over a source changed since it was stored: reference ran the stored version, elephc
    // re-read the new one.
    let from_disk = super::file_store::load_entry(config, key, mtime, file_size);
    let parsed_here = from_disk.is_none();
    if parsed_here {
        // `opcache.blacklist_filename` is decided before any compile accounting, and the
        // ordering is the contract, not a preference. php-src hands a blacklisted file
        // straight back to the original compiler, so such a script: runs normally, is stored
        // NOWHERE — not in the memory cache and not in `opcache.file_cache` either — and
        // counts as a `blacklist_misses` INSTEAD of a `misses`. VERIFIED against reference
        // PHP 8.5.10, where including a blacklisted file left `misses` untouched and moved
        // only `blacklist_misses`, and where including it twice counted TWO refusals: the
        // counter is of refusals, not of distinct files.
        if super::blacklist::blocks(key) {
            let segments: Arc<[ScriptSegment]> =
                Arc::from(segment_script(&bytes, ParseMode::Fresh));
            lock_script_cache().blacklist_misses += 1;
            return Ok(segments);
        }
        // THE TIMESTAMP AND AGE REFUSALS COME NEXT, and BEFORE the size refusal. php-src's
        // compile path reads the timestamp, refuses a `0` one, then applies
        // `file_update_protection`, and only then `max_file_size`. The first two are counted as
        // a MISS; the size refusal as a `blacklist_misses`. Checking size first put a file both
        // rules refuse in the wrong counter. MEASURED with `max_file_size=1`: under a raised
        // protection, reference counts `misses+1 blacklist_misses+0`, elephc counted the
        // opposite; the same for an mtime-`0` file; and with the protection off, both agree on
        // `blacklist_misses+1` — the size refusal alone.
        //
        // The refused script still RUNS: its segments are returned, as for every refusal here.
        let reads_timestamp = config.validate_timestamps
            || config.file_update_protection != 0
            || config.max_file_size != 0;
        let unrecordable = reads_timestamp && mtime.unwrap_or(0) == 0;
        if unrecordable || !config.admits_age(mtime, request_now()) {
            let segments: Arc<[ScriptSegment]> =
                Arc::from(segment_script(&bytes, ParseMode::Fresh));
            lock_script_cache().misses += 1;
            return Ok(segments);
        }
        // The SIZE refusal comes before any accounting, exactly like the blacklist one above
        // and for the same reason: php-src counts an oversized file as a `blacklist_misses`
        // and NOT as a miss. VERIFIED on reference PHP 8.5.10 — `-d opcache.max_file_size=50`
        // over two oversized scripts reports `misses=0 blacklist_misses=2`. The counter's name
        // is php-src's; what it means is "compiled but deliberately not stored", which a size
        // refusal is. Placing it after `misses += 1` made elephc report BOTH.
        if !config.admits_size(file_size) {
            let segments: Arc<[ScriptSegment]> =
                Arc::from(segment_script(&bytes, ParseMode::Fresh));
            lock_script_cache().blacklist_misses += 1;
            return Ok(segments);
        }
    }
    let (segments, stored_mtime, stored_revalidate): (Arc<[ScriptSegment]>, Option<i64>, i64) =
        match from_disk {
            Some(entry) => (Arc::from(entry.segments), entry.mtime, entry.revalidate),
            None => (Arc::from(segment_script(&bytes, ParseMode::Fresh)), None, 0),
        };
    let now = request_now();
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
    // A SCRIPT THAT DID NOT PARSE IS NOT CACHED, in memory or on disk. php-src stores
    // nothing on a compile error; the next request compiles the file again and sees whatever
    // is there now. Caching the failure instead makes a FIXED file keep raising the old error
    // — for up to `opcache.revalidate_freq` seconds (2 by default), and with
    // `opcache.validate_timestamps=0` until the process restarts. That turns an ordinary
    // edit-and-reload into a stale syntax error the developer cannot clear, and the on-disk
    // `opcache.file_cache` copy survives the process entirely.
    //
    // The miss has already been counted above, which is also what php-src does: the compile
    // was attempted and did not come from the cache.
    if segments
        .iter()
        .any(|segment| matches!(segment, ScriptSegment::ParseError(_)))
    {
        return Ok(segments);
    }
    // A file younger than `opcache.file_update_protection` is RUN but not STORED, so a file
    // caught part-written never becomes a cached entry that outlives the write. Unlike the
    // size refusal this one DOES count a miss — VERIFIED: with the guard raised, including a
    // fresh file moves `misses` 1 -> 2 and leaves `blacklist_misses` at 0, while
    // `num_cached_scripts` stays put.
    //
    // ONLY FOR A SCRIPT PARSED HERE. The guard protects against caching a file caught
    // part-written, which a disk hit cannot be: its bytes were admitted by an earlier
    // compile. php-src loads the file cache BEFORE it reaches the age check, so an existing
    // disk entry is admitted to memory however young the source looks. MEASURED with the
    // protection raised to its maximum over an unchanged source: reference reports the
    // script cached, elephc refused it.
    //
    // The mtime-`0` and `file_update_protection` refusals for a script parsed here were taken
    // above, before the size refusal, in php-src's order — see there. A disk hit meets
    // neither: its bytes were admitted by an earlier compile, and php-src loads the file
    // cache before it reaches either check.
    if parsed_here {
        // Only a script this process actually parsed is written back, and only once the
        // refusals above have passed. Writing before them persisted files those very rules
        // exist to keep out — an age refusal would still have left a part-written file in
        // the on-disk cache, which is precisely what `file_update_protection` is for.
        // Re-writing one that came FROM the cache would be pure I/O for a byte-identical file.
        //
        // THE DISK STORES THE RECORDED TIMESTAMP, NOT THE MTIME: `0` under
        // `validate_timestamps=0`, exactly as the memory entry below does. php-src serializes
        // the persistent script's own `timestamp`, so a validating reader compares that `0`
        // with the real mtime and recompiles. MEASURED across two processes over an unchanged
        // source: reference's reader missed on the library, elephc's hit.
        let recorded = if config.validate_timestamps { mtime } else { Some(0) };
        super::file_store::store_entry(
            config,
            key,
            recorded,
            file_size,
            fresh_revalidate_at(config, now),
            &segments,
        );
    }
    // A PENDING RESTART CLOSES ADMISSION. `opcache_reset()` schedules rather than flushes,
    // and php-src's accelerator refuses to admit anything new to shared memory from the
    // moment the flag is set — the entries already there keep answering until the restart
    // lands, but nothing joins them. Without this the script was cached DURING the window
    // the reset opened, and survived the flush that followed.
    //
    // MEASURED, `opcache_reset()` then `opcache_compile_file($p)`: reference reports the
    // file uncached, elephc reported it cached.
    //
    // The script still RUNS — the segments are returned exactly as the refusals above
    // return them.
    //
    // THE DISK IS STILL WRITTEN, which is why this now sits AFTER the disk write. With a
    // restart pending, php-src's compile falls back to `file_cache_compile_file()`, which
    // stores the script on disk; only the shared-memory admission is closed. Skipping the
    // disk write too — this used to say doing so kept the problem out of the next process —
    // made the file-cache query answer `false` where reference answers `true`. MEASURED:
    // `opcache_reset(); opcache_compile_file($p);` then the disk query — reference `true`,
    // elephc `false`; both report the script uncached in memory.
    if cache.restart_pending {
        return Ok(segments);
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
            // Recorded only under validation, as php-src's `cache_script_in_shared_memory`
            // does — see `is_unrecorded` — except that a DISK HIT keeps the timestamp it was
            // stored with, as the deserialized script does in php-src. See
            // `file_store::load_entry`.
            timestamp: if !parsed_here {
                stored_mtime.unwrap_or(0)
            } else if config.validate_timestamps {
                mtime.unwrap_or(0)
            } else {
                0
            },
            footprint,
            // A DISK HIT IS THE SCRIPT'S FIRST HIT. php-src counts it in the aggregate AND in
            // the script's own `hits`; this counted only the aggregate. MEASURED: seed the disk,
            // `opcache_compile_file($p)` in a fresh process — reference reports the script's
            // `hits` as 1, elephc reported 0.
            hits: u64::from(!parsed_here),
            last_used: now,
            // A DISK HIT KEEPS ITS WRITER'S DEADLINE rather than opening a new window. php-src
            // serializes `revalidate` and its loader leaves it alone, so once the writer's window
            // has passed, the reader re-stats on its next check. Granting a fresh window masked
            // a change made right after the load. MEASURED: written under `revalidate_freq=0`,
            // read two seconds later under `60`, source changed after the load — reference
            // reports it uncached, elephc still vouched for it.
            revalidate_at: if parsed_here {
                fresh_revalidate_at(config, now)
            } else {
                stored_revalidate
            },
            discarded: false,
        },
    );
    Ok(segments)
}

/// The revalidation deadline a freshly compiled entry gets: `now + revalidate_freq` under
/// validation, `0` otherwise — php-src records `revalidate` only when it validates.
fn fresh_revalidate_at(config: &ScriptCacheConfig, now: i64) -> i64 {
    if config.validate_timestamps {
        now.saturating_add(config.revalidate_freq as i64)
    } else {
        0
    }
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
/// THERE IS NO DIVERGENCE HERE ANY MORE. This used to flush immediately, because the request
/// boundary a deferred flush needs lives in `elephc-web`, which does not depend on the
/// interpreter crate. `__elephc_eval_opcache_apply_restart` closed that gap: generated code
/// emits it at the top of the `--web` handler, so the flush now lands where php-src's does.
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
    let key = cache_key(path);
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

/// `opcache_invalidate()`: evicts `path` when php-src's predicate says to.
///
/// php-src's `accel_invalidate` is
/// `force || !validate_timestamps || do_validate_timestamps(...) == FAILURE`, and only the
/// FIRST of those three was implemented. The other two are not corner cases:
///
/// - `!validate_timestamps` is the DEPLOYMENT configuration. With
///   `opcache.validate_timestamps=0` nothing is ever re-stated, so an explicit
///   `opcache_invalidate()` is the ONLY way to retire a script — and it did nothing at all
///   unless the caller also passed `force`. MEASURED: reference reports the script
///   uncached after a plain `opcache_invalidate($p)`; elephc reported it still cached.
/// - the timestamp check is the ordinary `validate_timestamps=1` case, where reference
///   evicts a script whose source has moved on and keeps one that has not. MEASURED, both
///   directions: an untouched file survives a non-forced invalidate in reference too.
///
/// THE STALENESS TEST IS THE TIMESTAMP ALONE — and so is every other one in this crate now.
/// `do_validate_timestamps` passes `NULL` for the size output and compares only the
/// timestamp, so a rewrite that changes the LENGTH while preserving the mtime is fresh to
/// reference. MEASURED on three surfaces: a non-forced `opcache_invalidate()` keeps such an
/// entry, a later include replays the stored script, and a second process replays it from
/// the file cache.
///
/// An earlier version of this docblock defended keeping the size in the warm-hit path, as
/// the right question for deciding whether to SERVE an entry. The include measurement
/// settled that too: reference serves the stored script there, so there is one definition
/// of "changed" in this crate, and it is php-src's.
///
/// RETURNS THE CACHE HALF OF THE PHP ANSWER. `opcache_invalidate()` reports "the path
/// resolves, OR a live memory entry was found"; the resolution half is a filesystem question
/// and stays with the builtin. This answers the other half — whether a live MEMORY entry was
/// present when the call began — and never counts the disk removal, which php-src performs
/// but does not report.
pub fn invalidate(path: &Path, force: bool) -> bool {
    let key = cache_key(path);
    // THE ON-DISK ENTRY GOES FIRST, AND UNCONDITIONALLY. php-src calls
    // `zend_file_cache_invalidate` outside the `force || !validate_timestamps || stale`
    // test, so an invalidate that deliberately KEEPS the in-memory entry still drops the
    // disk copy. MEASURED: reference reports `mem_after=1 disk_after=0` for an unchanged
    // file under `validate_timestamps=1`; elephc reported `disk_after=1`, because this
    // returned before reaching the removal.
    //
    // That ordering is the point rather than an accident. The in-memory entry dies with the
    // process; the disk entry outlives it, so leaving one behind is how an invalidated
    // script comes back in the next process — the failure the call exists to prevent.
    //
    // THE ANSWER IS THE MEMORY ENTRY, read BEFORE anything is removed. php-src's
    // `zend_accel_invalidate` reports `file_found`: the path resolved, or the shared-memory
    // hash held a live entry. The disk removal never counts — reference deletes the disk entry
    // of a deleted, uncached file and still answers `false`. This returned `was_live ||
    // removed`, so a disk-only deletion read as success. MEASURED across two processes: seed
    // the disk, unlink the source, `opcache_invalidate($p, true)` in a fresh process —
    // reference `false`, elephc `true`; both removed the disk entry.
    let was_live = is_present(&key);
    super::file_store::invalidate(&config(), &key);
    if !force && config().validate_timestamps && !entry_is_stale(&key) {
        return was_live;
    }
    // A SECOND INVALIDATE RETIRES NOTHING. `discard` answers whether an entry is PRESENT,
    // which stays true once the latch is set, so chaining it here reported success for a
    // call that did no work — reference answers `false` for the second one. The liveness is
    // read before the discard rather than inferred from it, because the latch is idempotent
    // by design and cannot distinguish the two on its own.
    //
    // PRESENCE, NOT `is_cached`. The query revalidates, and a deleted file fails that — so
    // asking it reported "nothing was live" for exactly the entry an invalidate of a deleted
    // file exists to retire. MEASURED, `validate_timestamps=1, revalidate_freq=0`:
    // `opcache_compile_file($p); unlink($p); opcache_invalidate($p, true)` answers `true` in
    // reference and answered `false` here.
    discard(&key);
    was_live
}

/// Returns whether `key`'s entry no longer matches the file on disk — php-src's
/// `do_validate_timestamps(...) == FAILURE`.
///
/// THE TIMESTAMP ONLY. php-src hands that function a `NULL` size output and compares the
/// mtime, so a rewrite that changes the length while preserving the timestamp leaves the
/// entry valid. Comparing the size here too made such a file stale and retired an entry
/// reference keeps.
///
/// A path with NO entry is not stale; there is nothing to be stale about, and reporting it
/// so would make the predicate above take a branch that has no work to do. A file that can
/// no longer be stated IS stale: it was cached and is now unreadable, which is the strongest
/// possible reason not to keep serving it.
///
/// NO `is_unrecorded` SHORT-CIRCUIT HERE, unlike the include and query paths: php-src's
/// `accel_invalidate` calls `do_validate_timestamps` directly, so an entry admitted without
/// a timestamp compares `0` against the real mtime and is stale.
fn entry_is_stale(key: &Path) -> bool {
    let cache = lock_script_cache();
    let Some(entry) = cache.entries.get(key) else {
        return false;
    };
    let timestamp = entry.timestamp;
    drop(cache);
    file_timestamp(key) != timestamp
}

/// Returns whether a path has a live cache entry — present, not discarded, and not known to
/// be stale.
///
/// A PRESENT ENTRY IS NOT NECESSARILY A CACHED SCRIPT. php-src's `opcache_is_script_cached()`
/// validates the timestamp before answering, so once the source has moved on it reports the
/// script uncached rather than vouching for bytes it would not serve. This checked presence
/// only, and answered `true` for an entry the next include was about to replace.
///
/// THE REVALIDATION WINDOW IS RESPECTED, and that is what makes this safe rather than merely
/// stricter. php-src re-stats only once `revalidate_freq` has elapsed since the entry's last
/// check. MEASURED on reference PHP 8.5 with the source rewritten under an older mtime:
///
/// ```text
/// revalidate_freq=0    reference: uncached   elephc was: cached
/// revalidate_freq=2    reference: cached     elephc was: cached   (default — unchanged)
/// revalidate_freq=60   reference: cached     elephc was: cached
/// ```
///
/// A version that re-stat'd on every call would have fixed the first row and broken the
/// second, which is the configuration nearly everyone runs.
///
/// The test is the timestamp alone, as in `entry_is_stale` and php-src's
/// `do_validate_timestamps`.
///
/// A SUCCESSFUL CHECK RENEWS THE WINDOW, as it does in php-src: the query goes through
/// `validate_timestamp_and_record_ex`, which records `request_time + revalidate_freq` on
/// success. This docblock used to say the opposite — that a query must not move the
/// deadline — and that made a query answer differently from reference once the window
/// had lapsed: the first check succeeded without renewing, so a change right after it was
/// seen at once where reference, having just renewed, keeps vouching for the entry until
/// the renewed window lapses.
pub fn is_cached(path: &Path) -> bool {
    let key = cache_key(path);
    let (timestamp, revalidate_at) = {
        let cache = lock_script_cache();
        match cache.entries.get(&key) {
            Some(entry) if !entry.discarded => (entry.timestamp, entry.revalidate_at),
            _ => return false,
        }
    };
    let config = config();
    let now = request_now();
    if is_unrecorded(timestamp) || !revalidation_due(&config, revalidate_at, now) {
        return true;
    }
    let fresh = file_timestamp(&key) == timestamp;
    if fresh {
        let mut cache = lock_script_cache();
        if let Some(entry) = cache.entries.get_mut(&key) {
            entry.revalidate_at = now.saturating_add(config.revalidate_freq as i64);
            // The renewed deadline is what `opcache_get_status()['scripts']` reports as
            // `revalidate`, so a snapshot taken before it is stale. MEASURED: status, then
            // `ini_set('opcache.revalidate_freq', '0')` and a query, then status again —
            // reference's deadline moved back by the old window, elephc's did not.
            cache.generation = cache.generation.wrapping_add(1);
        }
    }
    fresh
}

/// Returns whether `key` has a present, non-discarded entry — no freshness question asked.
fn is_present(key: &Path) -> bool {
    lock_script_cache()
        .entries
        .get(key)
        .is_some_and(|entry| !entry.discarded)
}

/// Reads and caches a script without executing it, as `opcache_compile_file()` does.
///
/// Returns whether the file could be read AND PARSED. A file that parses but the budget
/// refuses still reports success: php-src reports the COMPILE, not the store.
///
/// THE PARSE RESULT HAS TO BE INSPECTED, not inferred from `fill_entry` succeeding.
/// `fill_entry` returns `Ok` for a file that did not parse — deliberately, because the
/// segments it hands back carry the error so a later `include` can RAISE it at the right
/// moment. Reading `is_ok()` as "compiled" therefore reported success for a file with a
/// syntax error, which is the one answer `opcache_compile_file()` must never give: the
/// caller is told the file is ready and nothing is cached.
///
/// DIVERGENCE, stated rather than hidden: reference PHP 8.5 THROWS a `ParseError` here,
/// where this answers `false`. Throwing needs the message and line carried across the
/// bridge, which this signature cannot do; `false` is the honest half of the answer and no
/// longer the wrong one.
///
/// A discarded entry is re-admitted by the fill itself, which inserts a fresh entry with
/// the latch clear. Removing it first would be worse than redundant: the removal does not
/// return the entry's footprint to the budget, so the refill would count the same bytes
/// twice and the entry would be weighed against `max_accelerated_files` as a NEW one.
pub fn compile_file(path: &Path) -> bool {
    compile_file_inner(path)
}

/// Why `opcache_compile_file()` could not OPEN `path`, as the `strerror` text php-src's stream
/// layer prints — or `None` when it opens, which makes any `false` a parse failure instead.
///
/// `compile_file` answers `false` for both, and only an open failure warns: a file that opens
/// but does not parse is the pinned divergence where reference throws. `io::Error`'s
/// `Display` is the `strerror` text plus ` (os error N)`, which php-src never prints.
pub(crate) fn compile_open_failure(path: &Path) -> Option<String> {
    let error = std::fs::File::open(path).err()?;
    let text = error.to_string();
    Some(match text.rfind(" (os error ") {
        Some(cut) if error.raw_os_error().is_some() => text[..cut].to_string(),
        _ => text,
    })
}

/// The two warnings `opcache_compile_file()` prints for a file it cannot open, worded as
/// reference words them, with `display` as the caller spelled the path.
pub(crate) fn compile_open_failure_warnings(display: &str, reason: &str) -> [String; 2] {
    [
        format!("Warning: opcache_compile_file({display}): Failed to open stream: {reason}\n"),
        format!("Warning: opcache_compile_file(): Failed opening '{display}' for inclusion\n"),
    ]
}

/// Compiles one file into the cache for `opcache_compile_file()`, serving a warm entry as a hit;
/// answers whether the file is cached without a parse error.
fn compile_file_inner(path: &Path) -> bool {
    let config = config();
    if !config.enabled {
        return false;
    }
    let key = cache_key(path);
    // A SECOND CALL ON A CACHED FILE IS A HIT, not another compile. php-src's
    // `opcache_compile_file()` goes through the same cache lookup as an include, so calling
    // it twice moves `hits`, not `misses`. Going straight to `fill_entry` re-read and
    // re-parsed the file every time, counted a miss for each, and rebuilt the entry with
    // `hits: 0` and a reset `last_used` — so repeated calls moved every figure the wrong way
    // and dragged `opcache_hit_rate` down with them. MEASURED on reference PHP 8.5.10: three
    // calls give `hits=2 misses=0` beyond the first, against `hits=0 misses=2` here.
    match serve_warm_entry(&key, &config) {
        Ok(Some(_)) => return true,
        Ok(None) => {}
        Err(_) => return false,
    }
    match fill_entry(&key, path, &config) {
        Ok(segments) => !segments
            .iter()
            .any(|segment| matches!(segment, ScriptSegment::ParseError(_))),
        Err(_) => false,
    }
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
            timestamp: if entry.discarded { 0 } else { entry.timestamp },
            revalidate_at: entry.revalidate_at,
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
