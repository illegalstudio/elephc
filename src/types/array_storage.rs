//! Purpose:
//! Defines the storage-representation conversions a PHP array local can undergo, as one
//! predicate shared by the type checker and the IR lowering.
//!
//! Called from:
//! - `crate::types::checker::inference::expr::effects` (the env fact a conditional arm leaves behind)
//! - `crate::ir_lower::context` and `crate::ir_lower::stmt::repr_fixpoint` (the op to emit)
//! - Call validation and call-argument lowering (binding a packed list to a hash parameter)
//!
//! Key details:
//! - The checker's parameter specialization compiles a callee for the element type it sees at the
//!   call site, so the checker and the lowering MUST agree on when an array's representation
//!   changes: if they disagree, a boxed array is passed to a body compiled for raw scalar slots and
//!   read back as a pointer. One predicate, used by both, is what keeps them in step.

use super::PhpType;

/// Returns the storage representation a local's type transition converts its array to, when the
/// transition rewrites the array's storage at runtime.
///
/// - `Array(T)` -> `Array(Mixed)` (`Op::ArrayToMixed`): every element slot is replaced by a pointer
///   to a boxed Mixed cell, so an op compiled against raw slots reads a pointer as a scalar.
/// - `Array(_)` -> `AssocArray` (`Op::ArrayToHash`): the packed element vector is replaced by a hash
///   table, so an op compiled against the packed layout reads the wrong memory entirely and,
///   because a hash lookup of a live key simply misses instead of faulting, that one loses data
///   silently.
/// - A concrete array -> PHP `array`: a boxed cell can hold either packed or keyed storage after
///   a declared by-reference parameter writes through it.
///
/// A local with no previous type is not converted: there was no earlier representation for the code
/// above it to have been compiled against. A hash-to-packed transition rebinds the local to a
/// different array rather than converting the one already there.
pub(crate) fn array_storage_conversion(
    previous: Option<&PhpType>,
    next: &PhpType,
) -> Option<PhpType> {
    let previous = previous?;
    if next.is_php_array() && matches!(previous, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Some(next.clone());
    }
    let PhpType::Array(previous_elem) = previous.codegen_repr() else {
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
    if previous.is_php_array() || next.is_php_array() {
        return PhpType::php_array();
    }
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

/// Returns the hash storage a packed `array<element>` argument is rebuilt into when it is bound
/// to the declared hash parameter `expected`, or `None` when the two cannot be bound.
///
/// A list is a valid `array<int, V>` value, but its storage is a packed element vector while the
/// callee is compiled for a hash table. `Op::ArrayToHash` rebuilds one into the other and can
/// box every entry on the way, so three element shapes are convertible: an empty literal (no
/// elements to disagree), elements already in the parameter's value representation, and any
/// boxable element into `mixed` values. Everything else (an `int` list for `float` values, a
/// nested list where the parameter nests a hash, pointer words into boxed cells) has no
/// conversion, and binding it anyway would make the callee read one layout as another.
///
/// The answer is the parameter's own type: it is what the callee reads and, for a by-reference
/// binding, what the caller's variable holds afterwards.
pub(crate) fn packed_argument_hash_storage(
    expected: &PhpType,
    actual: &PhpType,
) -> Option<PhpType> {
    let PhpType::AssocArray { value, .. } = expected.codegen_repr() else {
        return None;
    };
    let PhpType::Array(element) = actual.codegen_repr() else {
        return None;
    };
    let fits = match (element.codegen_repr(), value.codegen_repr()) {
        (PhpType::Never | PhpType::Void, _) => true,
        (PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_), value) => {
            value == element.codegen_repr()
        }
        (_, PhpType::Mixed) => true,
        (PhpType::Object(_), PhpType::Object(_)) => true,
        (element, value) => element == value,
    };
    fits.then(|| expected.clone())
}

/// Returns whether binding `actual` to the declared parameter `expected` crosses from packed
/// list storage to hash storage, convertible or not.
pub(crate) fn binds_packed_argument_to_hash(expected: &PhpType, actual: &PhpType) -> bool {
    matches!(expected.codegen_repr(), PhpType::AssocArray { .. })
        && matches!(actual.codegen_repr(), PhpType::Array(_))
}

/// Returns the storage a packed list variable is converted to, in place, before it is bound to
/// the declared by-REFERENCE container parameter `expected`, or `None` when no in-place
/// conversion exists (or none is needed).
///
/// A by-reference callee reads and writes the caller's own storage with the layout its
/// parameter declares. Two declared layouts differ from a typed list and can be reached from
/// one: a hash (`array<K, V>`, see [`packed_argument_hash_storage`]) and a list of boxed values
/// (a written `array<mixed>`), which every boxable element type widens into. Raw pointer,
/// buffer and packed-record words are not boxable: boxing would turn each into an integer.
pub(crate) fn packed_reference_argument_storage(
    expected: &PhpType,
    actual: &PhpType,
) -> Option<PhpType> {
    if let Some(hash) = packed_argument_hash_storage(expected, actual) {
        return Some(hash);
    }
    if !binds_packed_argument_to_boxed_list(expected, actual) {
        return None;
    }
    let PhpType::Array(element) = actual.codegen_repr() else {
        return None;
    };
    (!matches!(
        element.codegen_repr(),
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_)
    ))
    .then(|| PhpType::Array(Box::new(PhpType::Mixed)))
}

/// Returns whether binding the list `actual` by reference to the declared parameter `expected`
/// needs the caller's storage in another layout, convertible or not.
pub(crate) fn binds_packed_reference_argument(expected: &PhpType, actual: &PhpType) -> bool {
    binds_packed_argument_to_hash(expected, actual)
        || binds_packed_argument_to_boxed_list(expected, actual)
}

/// Returns whether `actual` is a list of unboxed elements bound to a list of boxed values.
fn binds_packed_argument_to_boxed_list(expected: &PhpType, actual: &PhpType) -> bool {
    let (PhpType::Array(expected_element), PhpType::Array(element)) =
        (expected.codegen_repr(), actual.codegen_repr())
    else {
        return false;
    };
    expected_element.codegen_repr() == PhpType::Mixed && element.codegen_repr() != PhpType::Mixed
}
