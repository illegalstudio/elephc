//! Purpose:
//! The on-disk half of `opcache.file_cache`: stores a script's parsed segments under the
//! configured directory so a process that starts cold can skip the read, the `<?php` scan
//! and the parse. php-src's file cache does the same for its opcodes.
//!
//! Called from:
//! - `crate::script_cache::store` on a fill (write) and on a miss (read).
//! - `crate::script_cache::file_cache` for the `opcache_is_script_cached_in_file_cache()`
//!   answer.
//!
//! Key details:
//! - WHY IT PAYS, measured rather than assumed (`super::format_bench`): bincode decoding is
//!   3.5–5x faster than re-parsing at ~1.8x the source size. `serde_json`, the format this
//!   crate already had, decodes a small script SLOWER than parsing it — which is why the
//!   dependency exists.
//! - ENTRIES ARE NEVER TRUSTED BLIND. Three independent checks must all pass before one is
//!   used: the [`SYSTEM_ID`] directory (so a different build never reads another's IR), the
//!   header's magic and format version, and the source's own mtime, size and canonical path.
//!   Any mismatch — or any deserialization error — is treated as a miss, never as an error:
//!   a cache that cannot be read is a cache that is not there.
//! - The canonical path lives in the HEADER, not only in the file name. The name is a
//!   64-bit hash, so two paths can collide; verifying the path on read makes a collision a
//!   miss instead of a wrong script.
//! - Every filesystem failure is swallowed. A cache is an optimisation, so a full disk or a
//!   read-only mount must degrade to "no cache", never to a failed include.

use super::config::ScriptCacheConfig;
use super::segments::ScriptSegment;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Bumped whenever the stored shape changes in a way an older file could not be read as.
///
/// THIS IS A CORRECTNESS SWITCH, not a label. bincode is not self-describing: a payload
/// written by a different eval-IR shape can decode into plausible-looking garbage rather
/// than failing, and the result would be executed. `super::format_guard` fails the build's
/// tests when the IR changes without this moving, so the bump is not left to memory.
pub(crate) const FORMAT_VERSION: u32 = 1;

/// Identifies the writer, so one build never reads another's entries.
///
/// php-src calls this the `system_id` and derives it from the PHP version, extension set
/// and build flags. The analogue here is the crate version plus the format version: the
/// eval IR travels inside this crate, so a released change to it moves the former and a
/// development change moves the latter.
fn system_id() -> String {
    format!("{}-{}", env!("CARGO_PKG_VERSION"), FORMAT_VERSION)
}

/// What one cache file holds. The header fields are all validated before the segments are
/// used, so this struct is the contract rather than a convenience wrapper.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct CacheFile {
    /// Guards against reading a file that is not one of ours at all.
    magic: u64,
    /// Guards against an older or newer stored shape. See [`FORMAT_VERSION`].
    format_version: u32,
    /// The canonical source path, so a file-name hash collision is a miss, not a mix-up.
    path: String,
    /// The source's mtime when the entry was written.
    mtime: Option<i64>,
    /// The source's length when the entry was written.
    size: u64,
    /// The parsed script.
    segments: Vec<ScriptSegment>,
}

/// `elephc opcache file cache`, as a little-endian tag. Any other leading bytes are foreign.
const MAGIC: u64 = 0x454C_5048_435F_4F46;

/// Returns the directory this build's entries live in, or `None` when caching is off.
///
/// Answers `None` for an unconfigured `opcache.file_cache`, which is php-src's default and
/// the state in which the whole feature is inert.
fn cache_dir(config: &ScriptCacheConfig) -> Option<PathBuf> {
    let file_cache = super::file_cache::file_cache_config();
    if !config.enabled || file_cache.path.is_empty() {
        return None;
    }
    Some(Path::new(&file_cache.path).join(system_id()))
}

/// Returns the file one script's entry is stored in.
///
/// The name is a 64-bit hash of the canonical path rather than the path itself: a path can
/// be longer than a file name may be, and can contain separators. The full path is carried
/// in the header, so the hash only has to spread entries out, not identify them.
fn entry_path(dir: &Path, canonical: &Path) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonical.hash(&mut hasher);
    dir.join(format!("{:016x}.bin", hasher.finish()))
}

/// Reads a script's segments from the file cache, if a valid entry is there.
///
/// Returns `None` for every failure mode — absent, foreign, stale, unreadable or corrupt —
/// because each of them means the same thing to the caller: parse the file instead.
pub(crate) fn load(
    config: &ScriptCacheConfig,
    canonical: &Path,
    mtime: Option<i64>,
    size: u64,
) -> Option<Vec<ScriptSegment>> {
    let dir = cache_dir(config)?;
    let bytes = std::fs::read(entry_path(&dir, canonical)).ok()?;
    let cached: CacheFile = bincode::deserialize(&bytes).ok()?;
    if cached.magic != MAGIC || cached.format_version != FORMAT_VERSION {
        return None;
    }
    // The source must be EXACTLY what was stored. `opcache.validate_timestamps` governs how
    // often an in-memory entry is re-checked; it does not license running a stale file from
    // disk, so this comparison is unconditional.
    if cached.path != canonical.to_string_lossy() || cached.mtime != mtime || cached.size != size {
        return None;
    }
    Some(cached.segments)
}

/// Writes a script's segments to the file cache, doing nothing when it cannot.
///
/// Silent by design: `opcache.file_cache_read_only` forbids writing, a missing directory is
/// created if possible, and any I/O failure leaves the caller with a working — merely
/// uncached — include.
pub(crate) fn store(
    config: &ScriptCacheConfig,
    canonical: &Path,
    mtime: Option<i64>,
    size: u64,
    segments: &[ScriptSegment],
) {
    if super::file_cache::file_cache_config().read_only {
        return;
    }
    let Some(dir) = cache_dir(config) else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let payload = CacheFile {
        magic: MAGIC,
        format_version: FORMAT_VERSION,
        path: canonical.to_string_lossy().into_owned(),
        mtime,
        size,
        segments: segments.to_vec(),
    };
    let Ok(bytes) = bincode::serialize(&payload) else {
        return;
    };
    // Written through a temporary and renamed, so a reader never sees a half-written entry.
    // A crashed write leaves a stray temp file rather than a corrupt cache hit.
    let target = entry_path(&dir, canonical);
    let temp = target.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::write(&temp, &bytes).is_err() {
        let _ = std::fs::remove_file(&temp);
        return;
    }
    if std::fs::rename(&temp, &target).is_err() {
        let _ = std::fs::remove_file(&temp);
    }
}

/// Returns whether a usable entry for `canonical` is on disk right now.
///
/// This is what `opcache_is_script_cached_in_file_cache()` answers, so it applies the same
/// validation a read does: reporting an entry that a read would reject would be a lie.
pub(crate) fn contains(canonical: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(canonical) else {
        return false;
    };
    let canonical = std::fs::canonicalize(canonical).unwrap_or_else(|_| canonical.to_path_buf());
    let mtime = super::store::mtime_seconds(&metadata);
    load(
        &super::config::config(),
        &canonical,
        mtime,
        metadata.len(),
    )
    .is_some()
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the three independent checks an entry must pass, because each of them exists to
    //! stop a WRONG script being executed rather than merely to avoid a miss.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Each test uses its own directory, so they stay independent under the parallel
    //!   harness even though the configuration they read is thread-local.

    use super::super::config::ScriptCacheConfig;
    use super::super::file_cache::{set_file_cache_config, FileCacheConfig};
    use super::super::segments::{segment_script, ParseMode};
    use super::*;

    /// Points the thread's file cache at a fresh directory and returns it.
    fn configure(name: &str, read_only: bool) -> (ScriptCacheConfig, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "elephc-file-store-{}-{}-{:?}",
            name,
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("cache dir");
        set_file_cache_config(FileCacheConfig {
            path: dir.to_string_lossy().into_owned(),
            read_only,
        });
        let config = ScriptCacheConfig {
            enabled: true,
            ..ScriptCacheConfig::disabled()
        };
        (config, dir)
    }

    /// Verifies a stored script comes back with the same segment shape.
    #[test]
    fn a_stored_script_round_trips() {
        let (config, _dir) = configure("roundtrip", false);
        let source = b"<?php $a = 1; ?>tail<?php $b = 2;";
        let segments = segment_script(source, ParseMode::Fresh);
        let path = Path::new("/tmp/elephc-file-store-roundtrip.php");

        store(&config, path, Some(42), source.len() as u64, &segments);
        let loaded = load(&config, path, Some(42), source.len() as u64);

        assert_eq!(
            loaded.map(|s| s.len()),
            Some(segments.len()),
            "the entry must come back with every segment"
        );
    }

    /// Verifies a source whose mtime or size moved is REFUSED rather than served stale.
    ///
    /// This is the check that keeps a changed file from running as its old self, so it is
    /// unconditional — `opcache.validate_timestamps` governs re-`stat` frequency in memory,
    /// not whether a disk entry may be trusted.
    #[test]
    fn a_moved_source_is_refused() {
        let (config, _dir) = configure("stale", false);
        let source = b"<?php $a = 1;";
        let segments = segment_script(source, ParseMode::Fresh);
        let path = Path::new("/tmp/elephc-file-store-stale.php");
        store(&config, path, Some(42), source.len() as u64, &segments);

        assert!(
            load(&config, path, Some(43), source.len() as u64).is_none(),
            "mtime moved"
        );
        assert!(load(&config, path, Some(42), 999).is_none(), "size moved");
        assert!(
            load(&config, path, Some(42), source.len() as u64).is_some(),
            "unchanged hits"
        );
    }

    /// Verifies another path never reads this one's entry, even on a name-hash collision.
    ///
    /// The file name is a 64-bit hash, so a collision is possible; the canonical path in the
    /// header is what makes one a miss instead of the wrong script.
    #[test]
    fn another_path_never_reads_this_entry() {
        let (config, _dir) = configure("path", false);
        let source = b"<?php $a = 1;";
        let segments = segment_script(source, ParseMode::Fresh);
        let mine = Path::new("/tmp/elephc-file-store-mine.php");
        store(&config, mine, Some(42), source.len() as u64, &segments);

        let theirs = Path::new("/tmp/elephc-file-store-theirs.php");
        assert!(load(&config, theirs, Some(42), source.len() as u64).is_none());
    }

    /// Verifies `opcache.file_cache_read_only` writes nothing.
    #[test]
    fn read_only_stores_nothing() {
        let (config, dir) = configure("readonly", true);
        let source = b"<?php $a = 1;";
        let segments = segment_script(source, ParseMode::Fresh);
        let path = Path::new("/tmp/elephc-file-store-readonly.php");

        store(&config, path, Some(42), source.len() as u64, &segments);

        assert!(load(&config, path, Some(42), source.len() as u64).is_none());
        assert!(
            !dir.join(system_id()).exists(),
            "not even the directory is created"
        );
    }

    /// Verifies an unconfigured `opcache.file_cache` is completely inert.
    #[test]
    fn an_unconfigured_directory_stores_nothing() {
        set_file_cache_config(FileCacheConfig::new());
        let config = ScriptCacheConfig {
            enabled: true,
            ..ScriptCacheConfig::disabled()
        };
        let segments = segment_script(b"<?php $a = 1;", ParseMode::Fresh);
        let path = Path::new("/tmp/elephc-file-store-unset.php");

        store(&config, path, Some(42), 13, &segments);

        assert!(load(&config, path, Some(42), 13).is_none());
    }
}
