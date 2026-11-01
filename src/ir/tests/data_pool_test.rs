//! Purpose:
//! Verifies the data pool's interning across the truncation speculative lowering rolls back with.
//!
//! Called from:
//! - `crate::ir::tests`.
//!
//! Key details:
//! - The interners answer from a hash index that a truncation leaves stale on purpose; an entry is
//!   only believed when the vector still holds that value at that position.

use crate::ir::{DataId, DataPool};

/// Interning the same value twice gives one entry and one id.
#[test]
fn interning_is_idempotent() {
    let mut pool = DataPool::default();
    let first = pool.intern_string("alpha");
    let second = pool.intern_string("beta");
    assert_eq!(pool.intern_string("alpha"), first);
    assert_eq!(pool.intern_string("beta"), second);
    assert_eq!(pool.strings, vec!["alpha".to_string(), "beta".to_string()]);
}

/// A value removed by a rollback is interned again at its new position, never at the stale one.
#[test]
fn a_truncated_value_is_not_answered_from_the_stale_index() {
    let mut pool = DataPool::default();
    pool.intern_string("kept");
    pool.intern_string("dropped");
    pool.strings.truncate(1);
    // The slot "dropped" held now belongs to another value.
    assert_eq!(pool.intern_string("other"), DataId::from_raw(1));
    assert_eq!(pool.intern_string("dropped"), DataId::from_raw(2));
    assert_eq!(
        pool.strings,
        vec!["kept".to_string(), "other".to_string(), "dropped".to_string()]
    );
    assert_eq!(pool.intern_string("kept"), DataId::from_raw(0));
}

/// A lookup that must not intern agrees with interning, including after a rollback.
#[test]
fn function_name_lookup_follows_the_vector() {
    let mut pool = DataPool::default();
    pool.intern_function_name("f");
    let g = pool.intern_function_name("g");
    assert_eq!(pool.function_name_id("g"), Some(g));
    pool.function_names.truncate(1);
    assert_eq!(pool.function_name_id("g"), None);
    assert_eq!(pool.function_name_id("h"), None);
}
