//! Purpose:
//! Proves that one typed indexed-array release owns no PHP-visible child cleanup.
//!
//! Called from:
//! - super::emit_cleanup_invoke() after scalar callback authentication rejects an entry.
//!
//! Key details:
//! - Only the exact __rt_decref_array callback and a live managed array qualify.
//! - Logical length zero excludes child releases regardless of reserved capacity.
//! - Hash-promoted storage, nonempty arrays and other callbacks retain protection.
//! - The shared direct stub invokes the original callback and preserves GC semantics.

pub(super) use elephc_memoir::cleanup_fast_paths::empty_array::emit_candidate;
#[cfg(test)]
pub(super) use elephc_memoir::cleanup_fast_paths::empty_array::CANDIDATE;
