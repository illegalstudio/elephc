//! Purpose:
//! Caches parsed eval fragments before interpreter execution.
//! This removes repeated tokenization and parsing for identical runtime source
//! bytes while keeping execution context and scope fully dynamic.
//!
//! Called from:
//! - `crate::ffi::execute::__elephc_eval_execute()`
//! - `crate::interpreter::include_exec` for nested eval/include parsing.
//!
//! Key details:
//! - The cache stores immutable EvalIR parse results only, never runtime cells,
//!   declarations, scope entries, or context-derived magic-constant values.
//! - Large fragments bypass the cache to avoid pinning one-off source strings.

use crate::errors::EvalParseDiagnostic;
use crate::eval_ir::EvalProgram;
use crate::parser;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

const EVAL_PARSE_CACHE_CAPACITY: usize = 256;
const MAX_CACHEABLE_FRAGMENT_BYTES: usize = 64 * 1024;

type CachedParseResult = Result<Arc<EvalProgram>, EvalParseDiagnostic>;

static EVAL_PARSE_CACHE: OnceLock<Mutex<EvalParseCache>> = OnceLock::new();

/// Parses an eval fragment for execution (with source-line markers), reusing a cached
/// immutable EvalIR program when available.
pub(crate) fn parse_fragment_cached(code: &[u8]) -> CachedParseResult {
    if !is_cacheable_fragment(code) {
        return parser::parse_eval_fragment(code).map(Arc::new);
    }
    if let Some(result) = lock_eval_parse_cache().lookup(code) {
        return result;
    }
    let result = parser::parse_eval_fragment(code).map(Arc::new);
    lock_eval_parse_cache().insert(code.to_vec(), result.clone());
    result
}

/// Parses a whole PHP source file, reusing a cached immutable EvalIR program when available.
///
/// A source file and an eval fragment are different grammars — a file carries inline HTML and its
/// own `<?php` tags — so the two share the cache only through distinct keys: a file is keyed by its
/// bytes with a marker no fragment can produce, since the same bytes could legally be both.
pub(crate) fn parse_source_file_cached(code: &[u8]) -> CachedParseResult {
    let key = source_file_cache_key(code);
    if let Some(result) = lock_eval_parse_cache().lookup(&key) {
        return result;
    }
    let result = parser::parse_source_file(code).map(Arc::new);
    lock_eval_parse_cache().insert(key, result.clone());
    result
}

/// Parses a PHP source FILE identified by its path, reusing a cached program without reading it.
///
/// Keyed by path plus the file's modification time and length, so a repeat include costs one
/// `stat` instead of a full read, a key copy of the whole file, and — for anything past
/// `MAX_CACHEABLE_FRAGMENT_BYTES` — a complete re-parse. A long-lived web worker includes the same
/// vendor tree on every request: Symfony re-read and re-hashed 76 files per request, and the
/// generated DI container is far past the fragment cap, so it was re-parsed every single time.
///
/// Identity is (path, mtime, len). That is what php's own opcache validates by default, and it
/// catches every edit an ordinary deploy makes. A file rewritten in place within the same
/// mtime granularity AND to the identical length would be missed, which is why the caller keeps
/// the read-and-parse path for anything it cannot stat.
pub(crate) fn parse_source_file_at_path(path: &std::path::Path) -> Option<CachedParseResult> {
    let metadata = std::fs::metadata(path).ok()?;
    let key = source_path_cache_key(path, &metadata)?;
    if let Some(result) = lock_eval_parse_cache().lookup(&key) {
        return Some(result);
    }
    let code = std::fs::read(path).ok()?;
    let result = parser::parse_source_file(&code).map(Arc::new);
    lock_eval_parse_cache().insert(key, result.clone());
    Some(result)
}

/// Builds the identity key for one source file on disk.
///
/// The marker byte keeps these keys disjoint from both fragment keys (raw bytes) and whole-file
/// content keys (leading `0`), so three different lookups can share one cache.
fn source_path_cache_key(path: &std::path::Path, metadata: &std::fs::Metadata) -> Option<Vec<u8>> {
    use std::time::UNIX_EPOCH;

    let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    let mut key = Vec::with_capacity(path.as_os_str().len() + 32);
    key.push(1);
    key.extend_from_slice(&metadata.len().to_le_bytes());
    key.extend_from_slice(&modified.as_secs().to_le_bytes());
    key.extend_from_slice(&modified.subsec_nanos().to_le_bytes());
    key.extend_from_slice(path.to_string_lossy().as_bytes());
    Some(key)
}

/// Builds the cache key that separates a whole source file from an eval fragment.
fn source_file_cache_key(code: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(code.len() + 1);
    key.push(0);
    key.extend_from_slice(code);
    key
}

/// Returns true when a fragment is small enough to retain in the parse cache.
pub(crate) fn is_cacheable_fragment(code: &[u8]) -> bool {
    code.len() <= MAX_CACHEABLE_FRAGMENT_BYTES
}

/// Returns the process-wide eval parse cache singleton.
fn eval_parse_cache() -> &'static Mutex<EvalParseCache> {
    EVAL_PARSE_CACHE.get_or_init(|| Mutex::new(EvalParseCache::new(EVAL_PARSE_CACHE_CAPACITY)))
}

/// Locks the parse cache and recovers the inner cache if a previous panic poisoned it.
fn lock_eval_parse_cache() -> MutexGuard<'static, EvalParseCache> {
    lock_cache(eval_parse_cache())
}

/// Locks one parse cache and recovers the inner cache if a previous panic poisoned it.
fn lock_cache(cache: &'static Mutex<EvalParseCache>) -> MutexGuard<'static, EvalParseCache> {
    cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Included files memoized by their bytes, for includes made while the script cache is off.
///
/// A SEPARATE CACHE, not more entries in the fragment one: a file starts in HTML mode and a
/// fragment in PHP mode, so the same bytes parse to different programs, and one key space
/// would hand an include an eval fragment's parse (or the reverse).
static SCRIPT_PARSE_CACHE: OnceLock<Mutex<EvalParseCache>> = OnceLock::new();
const SCRIPT_PARSE_CACHE_CAPACITY: usize = 64;

/// Parses an included file, reusing a memoized program for the exact same bytes.
///
/// Files are memoized under the same 64 KiB cap as fragments, so the costs
/// `docs/php/opcache.md` measures for an include without the script cache still hold: a larger
/// file is parsed again on every include, which is what the runtime script cache is for.
pub(crate) fn parse_script_cached(bytes: &[u8]) -> CachedParseResult {
    // The script cache keeps the bare error (it is part of the persisted segment format);
    // this memo shares the fragment cache's located-diagnostic type, with no location.
    let parse = || {
        crate::script_cache::segments::parse_script(bytes)
            .map(Arc::new)
            .map_err(|error| crate::errors::EvalParseDiagnostic::new(error, 0, ""))
    };
    if !is_cacheable_fragment(bytes) {
        return parse();
    }
    let cache = SCRIPT_PARSE_CACHE
        .get_or_init(|| Mutex::new(EvalParseCache::new(SCRIPT_PARSE_CACHE_CAPACITY)));
    if let Some(result) = lock_cache(cache).lookup(bytes) {
        return result;
    }
    let result = parse();
    lock_cache(cache).insert(bytes.to_vec(), result.clone());
    result
}

/// Bounded FIFO cache for immutable eval parse results.
struct EvalParseCache {
    capacity: usize,
    entries: HashMap<Vec<u8>, CachedParseResult>,
    order: VecDeque<Vec<u8>>,
}

impl EvalParseCache {
    /// Creates an empty cache with the requested maximum entry count.
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// Returns a cloned cached parse result for the exact source bytes.
    fn lookup(&self, code: &[u8]) -> Option<CachedParseResult> {
        self.entries.get(code).cloned()
    }

    /// Inserts a parse result and evicts the oldest distinct source when full.
    fn insert(&mut self, code: Vec<u8>, result: CachedParseResult) {
        if self.capacity == 0 {
            return;
        }
        if self.entries.contains_key(code.as_slice()) {
            self.entries.insert(code, result);
            return;
        }
        while self.entries.len() >= self.capacity {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
        self.order.push_back(code.clone());
        self.entries.insert(code, result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::EvalParseError;

    /// Builds one positioned parse diagnostic for a cache test.
    fn diagnostic(error: EvalParseError) -> EvalParseDiagnostic {
        EvalParseDiagnostic::new(error, 1, "end of file")
    }

    /// Verifies repeated successful parses reuse the stored EvalIR allocation.
    #[test]
    fn cache_reuses_successful_parse_result() {
        let mut cache = EvalParseCache::new(4);
        let source = b"return 1;";
        let parsed = Arc::new(parser::parse_fragment(source).expect("fragment should parse"));

        cache.insert(source.to_vec(), Ok(parsed.clone()));
        let hit = cache
            .lookup(source)
            .expect("source should be cached")
            .expect("cached source should be successful");

        assert!(Arc::ptr_eq(&parsed, &hit));
    }

    /// Verifies parse errors are cached too so repeated invalid fragments avoid reparsing.
    #[test]
    fn cache_reuses_parse_errors() {
        let mut cache = EvalParseCache::new(4);
        let source = b"<?php echo 1;";

        cache.insert(source.to_vec(), Err(diagnostic(EvalParseError::PhpOpenTag)));

        assert_eq!(
            cache.lookup(source),
            Some(Err(diagnostic(EvalParseError::PhpOpenTag)))
        );
    }

    /// Verifies the cache evicts the oldest distinct fragment when capacity is reached.
    #[test]
    fn cache_evicts_oldest_fragment() {
        let mut cache = EvalParseCache::new(2);

        cache.insert(b"return 1;".to_vec(), Err(diagnostic(EvalParseError::UnexpectedToken)));
        cache.insert(b"return 2;".to_vec(), Err(diagnostic(EvalParseError::UnexpectedEof)));
        cache.insert(b"return 3;".to_vec(), Err(diagnostic(EvalParseError::InvalidNumber)));

        assert!(cache.lookup(b"return 1;").is_none());
        assert!(cache.lookup(b"return 2;").is_some());
        assert!(cache.lookup(b"return 3;").is_some());
    }

    /// Verifies a zero-capacity cache stores no entries.
    #[test]
    fn zero_capacity_cache_stores_nothing() {
        let mut cache = EvalParseCache::new(0);

        cache.insert(b"return 1;".to_vec(), Err(diagnostic(EvalParseError::UnexpectedToken)));

        assert!(cache.lookup(b"return 1;").is_none());
    }

    /// Verifies very large one-off fragments are kept out of the global cache.
    #[test]
    fn oversized_fragments_are_not_cacheable() {
        assert!(is_cacheable_fragment(&vec![b'a'; MAX_CACHEABLE_FRAGMENT_BYTES]));
        assert!(!is_cacheable_fragment(&vec![
            b'a';
            MAX_CACHEABLE_FRAGMENT_BYTES + 1
        ]));
    }
}
