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
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

const EVAL_PARSE_CACHE_CAPACITY: usize = 256;
const MAX_CACHEABLE_FRAGMENT_BYTES: usize = 64 * 1024;

type CachedParseResult = Result<Arc<EvalProgram>, EvalParseError>;

static EVAL_PARSE_CACHE: OnceLock<Mutex<EvalParseCache>> = OnceLock::new();

/// Parses an eval fragment, reusing a cached immutable EvalIR program when available.
pub(crate) fn parse_fragment_cached(code: &[u8]) -> CachedParseResult {
    if !is_cacheable_fragment(code) {
        return parser::parse_fragment(code).map(Arc::new);
    }
    if let Some(result) = lock_eval_parse_cache().lookup(code) {
        return result;
    }
    let result = parser::parse_fragment(code).map(Arc::new);
    lock_eval_parse_cache().insert(code.to_vec(), result.clone());
    result
}

/// Returns true when a fragment is small enough to retain in the parse cache.
fn is_cacheable_fragment(code: &[u8]) -> bool {
    code.len() <= MAX_CACHEABLE_FRAGMENT_BYTES
}

/// Returns the process-wide eval parse cache singleton.
fn eval_parse_cache() -> &'static Mutex<EvalParseCache> {
    EVAL_PARSE_CACHE.get_or_init(|| Mutex::new(EvalParseCache::new(EVAL_PARSE_CACHE_CAPACITY)))
}

/// Locks the parse cache and recovers the inner cache if a previous panic poisoned it.
fn lock_eval_parse_cache() -> MutexGuard<'static, EvalParseCache> {
    eval_parse_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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

        cache.insert(source.to_vec(), Err(EvalParseError::PhpOpenTag));

        assert_eq!(
            cache.lookup(source),
            Some(Err(EvalParseError::PhpOpenTag))
        );
    }

    /// Verifies the cache evicts the oldest distinct fragment when capacity is reached.
    #[test]
    fn cache_evicts_oldest_fragment() {
        let mut cache = EvalParseCache::new(2);

        cache.insert(b"return 1;".to_vec(), Err(EvalParseError::UnexpectedToken));
        cache.insert(b"return 2;".to_vec(), Err(EvalParseError::UnexpectedEof));
        cache.insert(b"return 3;".to_vec(), Err(EvalParseError::InvalidNumber));

        assert!(cache.lookup(b"return 1;").is_none());
        assert!(cache.lookup(b"return 2;").is_some());
        assert!(cache.lookup(b"return 3;").is_some());
    }

    /// Verifies a zero-capacity cache stores no entries.
    #[test]
    fn zero_capacity_cache_stores_nothing() {
        let mut cache = EvalParseCache::new(0);

        cache.insert(b"return 1;".to_vec(), Err(EvalParseError::UnexpectedToken));

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
