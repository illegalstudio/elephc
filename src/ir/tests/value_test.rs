//! Purpose:
//! Verifies value IDs and ownership lattice helpers.
//!
//! Called from:
//! - `crate::ir::tests`.
//!
//! Key details:
//! - Ownership is attached to SSA values, so merge behavior must stay stable.

use crate::ir::{Ownership, ValueId};
use crate::types::PhpType;

/// Identical ownership states remain unchanged at a merge.
#[test]
fn ownership_merge_same_state() {
    assert_eq!(Ownership::Owned.merge(Ownership::Owned), Ownership::Owned);
    assert_eq!(
        Ownership::Borrowed.merge(Ownership::Borrowed),
        Ownership::Borrowed
    );
}

/// Distinct heap ownership states merge to maybe-owned.
#[test]
fn ownership_merge_distinct_states_yields_maybe_owned() {
    assert_eq!(
        Ownership::Owned.merge(Ownership::Borrowed),
        Ownership::MaybeOwned
    );
}

/// Scalar integers do not need cleanup tracking.
#[test]
fn ownership_for_php_type_int_is_nonheap() {
    assert_eq!(Ownership::for_php_type(&PhpType::Int), Ownership::NonHeap);
}

/// Strings participate in ownership even though their storage type is not Heap.
#[test]
fn ownership_for_php_type_string_starts_maybe_owned() {
    assert_eq!(
        Ownership::for_php_type(&PhpType::Str),
        Ownership::MaybeOwned
    );
}

/// Packed values are borrowed pointers into buffer-owned storage.
#[test]
fn ownership_for_php_type_packed_is_borrowed() {
    assert_eq!(
        Ownership::for_php_type(&PhpType::Packed("Point".to_string())),
        Ownership::Borrowed
    );
}

/// Value IDs are zero-based function-local table indexes.
#[test]
fn value_id_is_zero_indexed() {
    let value = ValueId::from_raw(0);
    assert_eq!(value.as_raw(), 0);
}
