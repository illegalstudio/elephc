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

use crate::errors::EvalParseError;
use crate::eval_ir::EvalProgram;
use crate::parser;
use elephc_semantics::SemanticProfile;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

const EVAL_PARSE_CACHE_CAPACITY: usize = 256;
const MAX_CACHEABLE_FRAGMENT_BYTES: usize = 64 * 1024;

type CachedParseResult = Result<Arc<EvalProgram>, EvalParseError>;

static EVAL_PARSE_CACHE: OnceLock<Mutex<EvalParseCache>> = OnceLock::new();

/// Parses an eval fragment, reusing a cached immutable EvalIR program when available.
pub(crate) fn parse_fragment_cached(code: &[u8], profile: SemanticProfile) -> CachedParseResult {
    if !is_cacheable_fragment(code) {
        return parser::parse_fragment_with_semantics(code, profile).map(Arc::new);
    }
    if let Some(result) = lock_eval_parse_cache().lookup(code, profile) {
        return result;
    }
    let result = parser::parse_fragment_with_semantics(code, profile).map(Arc::new);
    lock_eval_parse_cache().insert(code.to_vec(), profile, result.clone());
    result
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
pub(crate) fn parse_script_cached(bytes: &[u8], profile: SemanticProfile) -> CachedParseResult {
    let parse = || crate::script_cache::segments::parse_script_with_semantics(bytes, profile).map(Arc::new);
    if !is_cacheable_fragment(bytes) {
        return parse();
    }
    let cache = SCRIPT_PARSE_CACHE
        .get_or_init(|| Mutex::new(EvalParseCache::new(SCRIPT_PARSE_CACHE_CAPACITY)));
    if let Some(result) = lock_cache(cache).lookup(bytes, profile) {
        return result;
    }
    let result = parse();
    lock_cache(cache).insert(bytes.to_vec(), profile, result.clone());
    result
}

/// Bounded FIFO cache for immutable eval parse results.
struct EvalParseCache {
    capacity: usize,
    entries: HashMap<SemanticProfile, HashMap<Vec<u8>, CachedParseResult>>,
    order: VecDeque<(SemanticProfile, Vec<u8>)>,
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
    fn lookup(&self, code: &[u8], profile: SemanticProfile) -> Option<CachedParseResult> {
        self.entries.get(&profile)?.get(code).cloned()
    }

    /// Inserts a parse result and evicts the oldest distinct source when full.
    fn insert(&mut self, code: Vec<u8>, profile: SemanticProfile, result: CachedParseResult) {
        if self.capacity == 0 { return; }
        if let Some(existing) = self.entries.get_mut(&profile).and_then(|entries| entries.get_mut(&code)) {
            *existing = result;
            return;
        }
        while self.order.len() >= self.capacity {
            let Some((old_profile, oldest)) = self.order.pop_front() else { break; };
            if let Some(entries) = self.entries.get_mut(&old_profile) {
                entries.remove(&oldest);
                if entries.is_empty() { self.entries.remove(&old_profile); }
            }
        }
        self.order.push_back((profile, code.clone()));
        self.entries.entry(profile).or_default().insert(code, result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies repeated successful parses reuse the stored EvalIR allocation.
    #[test]
    fn cache_reuses_successful_parse_result() {
        let mut cache = EvalParseCache::new(4);
        let source = b"return 1;";
        let parsed = Arc::new(parser::parse_fragment(source).expect("fragment should parse"));

        cache.insert(source.to_vec(), SemanticProfile::PERMISSIVE, Ok(parsed.clone()));
        let hit = cache
            .lookup(source, SemanticProfile::PERMISSIVE)
            .expect("source should be cached")
            .expect("cached source should be successful");

        assert!(Arc::ptr_eq(&parsed, &hit));
    }

    /// Verifies parse errors are cached too so repeated invalid fragments avoid reparsing.
    #[test]
    fn cache_reuses_parse_errors() {
        let mut cache = EvalParseCache::new(4);
        let source = b"<?php echo 1;";

        cache.insert(source.to_vec(), SemanticProfile::PERMISSIVE, Err(EvalParseError::PhpOpenTag));

        assert_eq!(
            cache.lookup(source, SemanticProfile::PERMISSIVE),
            Some(Err(EvalParseError::PhpOpenTag))
        );
    }

    /// Verifies the cache evicts the oldest distinct fragment when capacity is reached.
    #[test]
    fn cache_evicts_oldest_fragment() {
        let mut cache = EvalParseCache::new(2);

        cache.insert(b"return 1;".to_vec(), SemanticProfile::PERMISSIVE, Err(EvalParseError::UnexpectedToken));
        cache.insert(b"return 2;".to_vec(), SemanticProfile::PERMISSIVE, Err(EvalParseError::UnexpectedEof));
        cache.insert(b"return 3;".to_vec(), SemanticProfile::PERMISSIVE, Err(EvalParseError::InvalidNumber));

        assert!(cache.lookup(b"return 1;", SemanticProfile::PERMISSIVE).is_none());
        assert!(cache.lookup(b"return 2;", SemanticProfile::PERMISSIVE).is_some());
        assert!(cache.lookup(b"return 3;", SemanticProfile::PERMISSIVE).is_some());
    }

    /// Verifies a zero-capacity cache stores no entries.
    #[test]
    fn zero_capacity_cache_stores_nothing() {
        let mut cache = EvalParseCache::new(0);

        cache.insert(b"return 1;".to_vec(), SemanticProfile::PERMISSIVE, Err(EvalParseError::UnexpectedToken));

        assert!(cache.lookup(b"return 1;", SemanticProfile::PERMISSIVE).is_none());
    }

    /// Separates positive and negative parses while sharing one capacity across profiles.
    #[test]
    fn strict_profiles_partition_parse_results_and_share_capacity() {
        let strict = SemanticProfile::PERMISSIVE.with(elephc_semantics::StrictFeature::Overflow, true);
        let source = b"return 9223372036854775808;";
        let permissive = parse_fragment_cached(source, SemanticProfile::PERMISSIVE).unwrap();
        assert_eq!(parse_fragment_cached(source, strict), Err(EvalParseError::IntegerLiteralOverflow));
        assert!(Arc::ptr_eq(&permissive, &parse_fragment_cached(source, SemanticProfile::PERMISSIVE).unwrap()));
        let mut cache = EvalParseCache::new(1);
        cache.insert(source.to_vec(), SemanticProfile::PERMISSIVE, Ok(permissive));
        cache.insert(source.to_vec(), strict, Err(EvalParseError::IntegerLiteralOverflow));
        assert!(cache.lookup(source, SemanticProfile::PERMISSIVE).is_none());
        assert!(cache.lookup(source, strict).is_some());
        assert_eq!(cache.entries.len(), 1);
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
