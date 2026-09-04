//! Purpose:
//! Defines the two storage-representation conversions a PHP array local can undergo, as ONE
//! predicate shared by the type checker and the IR lowering.
//!
//! Called from:
//! - `crate::types::checker::inference::expr::effects` (the env fact a conditional arm leaves behind)
//! - `crate::ir_lower::context` and `crate::ir_lower::stmt::repr_fixpoint` (the op to emit)
//! - `crate::ir_lower::expr::array_builtin_args` (the key-preserving sort receiver promotion)
//!
//! Key details:
//! - The checker's parameter specialization compiles a callee for the element type it sees at the
//!   call site, so the checker and the lowering MUST agree on when an array's representation
//!   changes: if they disagree, a boxed array is passed to a body compiled for raw scalar slots and
//!   read back as a pointer. One predicate, used by both, is what keeps them in step.

use super::PhpType;

/// Reports whether a key-preserving sort promotes a PACKED receiver whose elements are `elem_ty`
/// to int-keyed hash storage.
///
/// php sorts with `zend_array_sort(Z_ARRVAL_P(array), <comparator>, renumber)`
/// (ext/standard/array.c). `sort()`/`rsort()`/`usort()` pass `renumber = 1` and renumber the keys
/// from zero; `asort()`, `arsort()`, `natsort()` and `natcasesort()` pass `0`, so the keys are
/// PERMUTED, not rebuilt. A packed array holds its keys implicitly as slot positions `0..n-1` and
/// cannot express a permutation, so honouring `renumber = 0` means converting the receiver first.
///
/// Only the NATURAL sorts over STRING values promote. Two things have to hold at once, and this is
/// the only pairing where both do:
///
/// - the relinking sorter has to be exact for the payloads the table holds.
///   `natsort`/`natcasesort` relink through `__rt_natcmp`/`__rt_natcasecmp`, and php reaches every
///   value as a STRING first (`zval_get_tmp_string()` then `strnatcmp_ex`), which is exact for
///   string payloads and only for those: measured, `natsort` puts `-5` before `-10` (comparing
///   `"-5"` against `"-10"`) where `asort` puts `-10` first, so borrowing the numeric comparator
///   would silently produce asort's order under natsort's name.
/// - the promoted receiver's LATER consumers have to be swept. A promoted local is a hash for the
///   rest of its life, so every builtin with no associative form refuses it from then on. The
///   string surface was swept — 50 consumer expressions run against `php -n`, 28 byte-identical
///   including every reader a natsort caller reaches.
///
/// `asort()`/`arsort()` on a packed receiver share the divergence exactly (`$a=[3,1,2];
/// asort($a); echo $a[0];` is `3` in php, `1` here) and the mechanism extends to them by adding one
/// match arm — `__rt_hash_asort`/`__rt_hash_arsort` relink through `__rt_php_compare` and were
/// measured identical to php for int, float, string and bool payloads on a hash receiver.
///
/// It is deliberately NOT enabled, and the blocker list is LARGER than the two builtins an
/// earlier note named. `array_sum()`/`array_product()` over an int-valued hash — the two it did
/// name — now compile, so the arm was added and the promoted surface re-measured: a
/// 45-expression sweep over `$a=[3,1,2]; asort($a);` run against `php -n` went from 23
/// disagreements to 9: 20 expressions moved from silently WRONG to byte-identical, three more
/// (`array_filter`, `array_pop`, `array_unique`) traded a wrong answer for a refusal, and SIX
/// that compile today became compile errors:
///
/// ```text
/// array_merge($a,[9])   [1,2,3,9]   -> unsupported: array_merge for PHP type AssocArray { key: Int, value: Int }
/// array_reverse($a)     [3,2,1]     -> unsupported: array_reverse for PHP type AssocArray { … }
/// array_slice($a,1)     [2,3]       -> unsupported: array_slice for PHP type AssocArray { … }
/// array_shift($a)       1|[2,3]     -> unsupported: array_shift for PHP type AssocArray { … }
/// sort($a)              [1,2,3]     -> unsupported: sort for PHP type AssocArray { … }
/// rsort($a)             [3,2,1]     -> unsupported: rsort for PHP type AssocArray { … }
/// ```
///
/// Those six are common enough that trading them for the 20 is not obviously right, so they need
/// their hash-receiver forms first — as their own change, the way `array_sum`/`array_product`
/// got theirs. Each reads only VALUES and returns a RENUMBERED result, so one shape covers them:
/// materialize the hash's values with `emit_loaded_assoc_array_values`, run the existing packed
/// lowering, and rebuild the result with `__rt_array_to_hash`, whose keys `0..n-1` are
/// indistinguishable from a packed list to every consumer measured (`json_encode` of a promoted
/// single-entry hash prints `[7]`, not `{"0":7}`).
///
/// `ksort()`/`krsort()` are absent for a different reason: a packed array's keys are already
/// `0..n-1` ascending, so php's `ksort()` returns the SAME list — measured, `$a=[3,1,2];
/// ksort($a);` prints `[3,1,2]`, which elephc already matches byte for byte — and promoting it
/// would allocate a hash to reproduce an answer it already had. `krsort()` has no packed form for
/// descending key order and keeps its explanatory refusal.
pub(crate) fn key_preserving_sort_promotes(builtin_name: &str, elem_ty: &PhpType) -> bool {
    match crate::names::php_symbol_key(builtin_name.trim_start_matches('\\')).as_str() {
        "natsort" | "natcasesort" => elem_ty.codegen_repr() == PhpType::Str,
        _ => false,
    }
}

/// Returns the storage representation a local's type transition converts its array to, when the
/// transition is one of the two that REWRITE the array's storage at runtime.
///
/// - `Array(T)` -> `Array(Mixed)` (`Op::ArrayToMixed`): every element slot is replaced by a pointer
///   to a boxed Mixed cell, so an op compiled against raw slots reads a pointer as a scalar.
/// - `Array(_)` -> `AssocArray` (`Op::ArrayToHash`): the packed element vector is replaced by a hash
///   table, so an op compiled against the packed layout reads the wrong memory entirely — and,
///   because a hash lookup of a live key simply misses instead of faulting, that one loses data
///   silently.
///
/// A local with no previous type is not converted: there was no earlier representation for the code
/// above it to have been compiled against. A local leaving `AssocArray` is not either — no op
/// converts a hash back to packed storage, so such a transition REBINDS the local to a different
/// array rather than converting the one already there.
pub(crate) fn array_storage_conversion(
    previous: Option<&PhpType>,
    next: &PhpType,
) -> Option<PhpType> {
    let PhpType::Array(previous_elem) = previous?.codegen_repr() else {
        return None;
    };
    match next.codegen_repr() {
        PhpType::Array(next_elem)
            if previous_elem.codegen_repr() != PhpType::Mixed
                && next_elem.codegen_repr() == PhpType::Mixed =>
        {
            Some(PhpType::Array(Box::new(PhpType::Mixed)))
        }
        assoc @ PhpType::AssocArray { .. } => Some(assoc),
        _ => None,
    }
}
/// Joins two conversion targets recorded for the SAME local into the one representation that
/// satisfies both.
///
/// A region can convert one local along both axes on different paths (`if ($c) { $m[0] = "s"; }
/// else { $m["k"] = 1; }`). Entering it with the array merely boxed would leave the hash arm — now
/// lowered against packed storage it no longer has — writing through the wrong layout, so the join
/// of an indexed target with a hash target is the HASH. Two hash targets that disagree on the value
/// type join to a Mixed-valued hash, because the arm the other value type came from would otherwise
/// insert entries tagged differently from what the merge reads back.
pub(crate) fn join_array_storage_conversion(previous: &PhpType, next: &PhpType) -> PhpType {
    match (previous.codegen_repr(), next.codegen_repr()) {
        (PhpType::Array(_), PhpType::Array(_)) => PhpType::Array(Box::new(PhpType::Mixed)),
        (
            PhpType::AssocArray { value: previous_value, .. },
            PhpType::AssocArray { value: next_value, .. },
        ) if previous_value.codegen_repr() == next_value.codegen_repr() => PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: previous_value,
        },
        _ => PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The natural sorts promote a string-valued packed receiver, which is the only pairing whose
    /// comparator is exact (php compares every value as a string) and whose promoted consumer
    /// surface has been swept against `php -n`.
    #[test]
    fn natural_sorts_promote_only_string_receivers() {
        assert!(key_preserving_sort_promotes("natsort", &PhpType::Str));
        assert!(key_preserving_sort_promotes("natcasesort", &PhpType::Str));
        assert!(key_preserving_sort_promotes("\\NatSort", &PhpType::Str));
        for elem in [PhpType::Int, PhpType::Float, PhpType::Bool, PhpType::Mixed] {
            assert!(
                !key_preserving_sort_promotes("natsort", &elem),
                "natsort must not promote a {:?}-valued receiver: the natural comparator reads \
                 every value as a string, which is exact for string payloads only",
                elem
            );
        }
    }

    /// `asort`/`arsort` permute keys in php too, but promoting their receiver is blocked on the
    /// hash-receiver forms of the SIX builtins that compile today and would stop: `array_merge`,
    /// `array_reverse`, `array_slice`, `array_shift`, `sort` and `rsort`, measured by enabling
    /// the arm and re-running a 45-expression sweep against `php -n`. Pinned so enabling it is a
    /// deliberate edit, not a silent one.
    #[test]
    fn value_sorts_and_renumbering_sorts_do_not_promote() {
        for name in ["asort", "arsort", "sort", "rsort", "usort", "ksort", "krsort", "shuffle"] {
            for elem in [PhpType::Str, PhpType::Int, PhpType::Float] {
                assert!(
                    !key_preserving_sort_promotes(name, &elem),
                    "{} must not promote a packed receiver",
                    name
                );
            }
        }
    }
}
