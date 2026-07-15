//! Purpose:
//! Defines the two storage-representation conversions a PHP array local can undergo, as ONE
//! predicate shared by the type checker and the IR lowering.
//!
//! Called from:
//! - `crate::types::checker::inference::expr::effects` (the env fact a conditional arm leaves behind)
//! - `crate::ir_lower::context` and `crate::ir_lower::stmt::repr_fixpoint` (the op to emit)
//!
//! Key details:
//! - The checker's parameter specialization compiles a callee for the element type it sees at the
//!   call site, so the checker and the lowering MUST agree on when an array's representation
//!   changes: if they disagree, a boxed array is passed to a body compiled for raw scalar slots and
//!   read back as a pointer. One predicate, used by both, is what keeps them in step.

use super::PhpType;

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
