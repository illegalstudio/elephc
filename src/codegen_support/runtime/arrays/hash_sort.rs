//! Purpose:
//! Coordinates target-specific hash-table link sort emitters used by PHP's stable
//! key- and value-preserving array sorts.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` through
//!   `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Sorting relinks the insertion-order chain without moving hash buckets or payloads.
//! - Both targets use the same stable bottom-up merge-sort contract and `O(n log n)`
//!   comparison bound.

mod aarch64;
mod x86_64;

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Mode word selecting an ascending key sort (`ksort`).
pub(super) const MODE_KEY_ASCENDING: i64 = 0;

/// Mode word selecting a descending key sort (`krsort`).
pub(super) const MODE_KEY_DESCENDING: i64 = 1;

/// Mode word selecting an ascending value sort (`asort`).
pub(super) const MODE_VALUE_ASCENDING: i64 = 2;

/// Mode word selecting a descending value sort (`arsort`).
pub(super) const MODE_VALUE_DESCENDING: i64 = 3;

/// Bit position at which a resolved key-comparator selector rides in the mode word.
///
/// Bits 0 and 1 already carry the direction and the key/value choice, so the selector starts
/// clear of both. It is a resolved selector rather than PHP's raw `$flags`: the entry stub
/// collapses every accepted spelling (including combinations PHP ignores, such as `999`) into
/// one small number, so the comparison step never has to reason about the raw word again.
pub(super) const KEY_COMPARATOR_SHIFT: i64 = 8;

/// Selector for PHP's default `SORT_REGULAR` key ordering.
pub(super) const KEY_COMPARATOR_REGULAR: i64 = 0;

/// Selector for `SORT_NUMERIC` key ordering.
pub(super) const KEY_COMPARATOR_NUMERIC: i64 = 1;

/// Selector for `SORT_STRING` key ordering.
pub(super) const KEY_COMPARATOR_STRING: i64 = 2;

/// Selector for `SORT_STRING | SORT_FLAG_CASE` key ordering.
pub(super) const KEY_COMPARATOR_STRING_CI: i64 = 3;

/// Selector for `SORT_LOCALE_STRING` key ordering.
pub(super) const KEY_COMPARATOR_LOCALE: i64 = 4;

/// Selector for `SORT_NATURAL` key ordering.
pub(super) const KEY_COMPARATOR_NATURAL: i64 = 5;

/// Selector for `SORT_NATURAL | SORT_FLAG_CASE` key ordering.
pub(super) const KEY_COMPARATOR_NATURAL_CI: i64 = 6;

/// The case-folded selectors sit directly above their base. That is what lets the key-sort
/// entry stub add `SORT_FLAG_CASE` into the selector instead of branching on it.
const _: () = assert!(KEY_COMPARATOR_STRING_CI == KEY_COMPARATOR_STRING + 1);
const _: () = assert!(KEY_COMPARATOR_NATURAL_CI == KEY_COMPARATOR_NATURAL + 1);

/// Emits every hash link-order sort helper for the active target.
pub fn emit_hash_sort(emitter: &mut Emitter) {
    super::hash_key_compare::emit_hash_key_compare(emitter);
    super::key_compare_flags::emit_key_compare_flags(emitter);
    match emitter.target.arch {
        Arch::X86_64 => x86_64::emit(emitter),
        Arch::AArch64 => aarch64::emit(emitter),
    }
}

/// Returns the value-sort entry points, whose mode word is fully known at emit time.
pub(super) fn value_entry_points() -> [(&'static str, i64, &'static str); 2] {
    [
        ("__rt_hash_asort", MODE_VALUE_ASCENDING, "sort a hash by value ascending"),
        ("__rt_hash_arsort", MODE_VALUE_DESCENDING, "sort a hash by value descending"),
    ]
}

/// Returns the key-sort entry points, which fold a runtime PHP `$flags` word into their mode.
pub(super) fn key_entry_points() -> [(&'static str, i64, &'static str); 2] {
    [
        ("__rt_hash_ksort", MODE_KEY_ASCENDING, "sort a hash by key ascending"),
        ("__rt_hash_krsort", MODE_KEY_DESCENDING, "sort a hash by key descending"),
    ]
}
