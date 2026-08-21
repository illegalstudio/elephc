//! Purpose:
//! Set, range, fill, combine, and result-shape validation.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Returns the element type accepted by indexed-array value set-operation helpers.
pub(super) fn set_op_indexed_array_element_type(
    ty: PhpType,
    name: &str,
    allow_strings: bool,
) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(elem, PhpType::Mixed | PhpType::Union(_)) {
                return Err(CodegenIrError::unsupported(format!(
                    "{} compares boxed elements by identity, not by value, for indexed-array \
                     element PHP type {:?}",
                    name, elem
                )));
            }
            if matches!(
                elem,
                PhpType::Int
                    | PhpType::Bool
                    | PhpType::Float
                    | PhpType::Callable
                    | PhpType::Void
                    | PhpType::Never
            ) || elem.is_refcounted()
                // 16-byte (ptr, len) slots need a string-aware helper; only the operations
                // that provide one may accept them, or the 8-byte comparison would read a
                // descriptor as two unrelated words.
                || (allow_strings && elem == PhpType::Str)
            {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "{} indexed-array element PHP type {:?}",
                name, elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Verifies two set-operation operands can share one raw slot comparison helper.
pub(super) fn require_set_op_compatible_element_types(
    name: &str,
    first: &PhpType,
    second: &PhpType,
) -> Result<()> {
    if first == second
        || matches!(first, PhpType::Never | PhpType::Void)
        || matches!(second, PhpType::Never | PhpType::Void)
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for incompatible indexed-array element PHP types {:?} and {:?}",
        name, first, second
    )))
}

/// Verifies the EIR result is the key-preserving hash the checker types a value set operation as.
///
/// The survivors keep their source integer keys, so the result of `array_diff`, `array_intersect`
/// and `array_unique` over an indexed array is an `AssocArray` keyed by `Int` whose value type is
/// the first operand's element type. A dense `Array` result here would mean the checker and the
/// backend disagree about whether the operation reindexes.
///
/// The value comparison is the one the dense form already used, so the accepted element layouts
/// are unchanged. It still rejects an operand whose checker element type and EIR element type
/// disagree — `[true, false, true]` infers `array<mixed>` from the checker (`Bool` and `False` do
/// not merge, so the literal falls back to `Mixed`) while the EIR builds `array<bool>` — because
/// the runtime stamps the hash with the SOURCE array's value_type, and a consumer reading it
/// through the checker's `Mixed` would misread every entry. `array_unique` did not run this check
/// before and so accepted that shape; the three operations now agree on it.
pub(super) fn require_set_op_result_type(
    name: &str,
    first_elem_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    match result_ty {
        PhpType::AssocArray { key, value }
            if key.codegen_repr() == PhpType::Int
                && value.codegen_repr() == first_elem_ty.codegen_repr() =>
        {
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} result PHP type {:?} for first element PHP type {:?}",
            name, other, first_elem_ty
        ))),
    }
}

/// Returns the hash operand type accepted by key set-operation helpers.
pub(super) fn assoc_array_key_set_operand_type(ty: PhpType, name: &str, position: &str) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::AssocArray { key, value } => Ok(PhpType::AssocArray { key, value }),
        other => Err(CodegenIrError::unsupported(format!(
            "{} {} argument PHP type {:?}",
            name, position, other
        ))),
    }
}

/// Verifies a key set-operation result preserves the first operand's hash metadata.
pub(super) fn require_assoc_array_key_set_result_type(
    name: &str,
    first_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    if result_ty == first_ty {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} result PHP type {:?} for first argument PHP type {:?}",
        name, result_ty, first_ty
    )))
}

/// Verifies that a `range()` endpoint can be passed to the integer runtime helper.
///
/// `Mixed`/`Union` endpoints are accepted here and unboxed to a plain integer by `lower_range`
/// (via `resolve_int_operand_to_result`); the `__rt_range` helper only consumes integer endpoints.
pub(super) fn require_range_endpoint(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::Mixed | PhpType::Union(_) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "range {} PHP type {:?}",
            name, other
        ))),
    }
}

/// Verifies `range()` is represented as an indexed integer array.
pub(super) fn require_range_result_type(result_ty: &PhpType) -> Result<()> {
    match result_ty {
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Int => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "range result PHP type {:?}",
            other
        ))),
    }
}

/// Returns the shared element type for two compatible 8-byte indexed arrays.
pub(super) fn compatible_eight_byte_indexed_array_element_type(
    first: PhpType,
    second: PhpType,
    name: &str,
) -> Result<PhpType> {
    let first = eight_byte_indexed_array_element_type(first, name)?;
    let second = eight_byte_indexed_array_element_type(second, name)?;
    if first == second
        || matches!(first, PhpType::Never | PhpType::Void)
        || matches!(second, PhpType::Never | PhpType::Void)
    {
        if matches!(first, PhpType::Never | PhpType::Void) {
            return Ok(second);
        }
        return Ok(first);
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for incompatible indexed-array element PHP types {:?} and {:?}",
        name, first, second
    )))
}

/// Verifies that a builtin call has a lowered operand count within an inclusive range.
pub(super) fn ensure_arg_count_between(inst: &Instruction, name: &str, min: usize, max: usize) -> Result<()> {
    let actual = inst.operands.len();
    if (min..=max).contains(&actual) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {}..={} args, got {}",
        name, min, max, actual
    )))
}

/// Verifies that the indexed `array_fill()` helper can store the fill value.
///
/// `Str` is accepted here and routed to `__rt_array_fill_str`, which materializes 16-byte
/// (pointer + length) string slots; the single-word scalar/refcounted helpers cannot carry a
/// string payload.
pub(super) fn require_array_fill_indexed_value_type(value_ty: &PhpType) -> Result<()> {
    if matches!(
        value_ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Void
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    ) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_fill indexed value PHP type {:?}",
        value_ty
    )))
}

/// Verifies that the assoc `array_fill()` helper can box the fill value.
pub(super) fn require_array_fill_assoc_value_type(value_ty: &PhpType) -> Result<()> {
    if matches!(
        value_ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Void
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    ) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_fill assoc value PHP type {:?}",
        value_ty
    )))
}

/// Returns the key element type accepted by `array_fill_keys()`.
pub(super) fn array_fill_keys_key_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => Ok(elem.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_fill_keys keys PHP type {:?}",
            other
        ))),
    }
}

/// Returns the key element type accepted by `array_combine()`.
pub(super) fn array_combine_key_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => Ok(elem.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_combine keys PHP type {:?}",
            other
        ))),
    }
}

/// Returns the value element type accepted by `array_combine()`.
pub(super) fn array_combine_value_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => Ok(elem.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_combine values PHP type {:?}",
            other
        ))),
    }
}

/// Verifies the key array uses the string-slot layout expected by the runtime helper.
pub(super) fn require_array_fill_keys_key_layout(key_elem_ty: &PhpType) -> Result<()> {
    if matches!(key_elem_ty, PhpType::Str | PhpType::Void | PhpType::Never) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_fill_keys key element PHP type {:?}",
        key_elem_ty
    )))
}

/// Verifies the fill payload can be passed through the current runtime helper ABI.
///
/// String values are deliberately excluded because the helper accepts only one value word;
/// preserving string payloads requires a value_hi register/slot path.
pub(super) fn require_array_fill_keys_value_type(value_ty: &PhpType) -> Result<()> {
    if matches!(
        value_ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Void
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
    ) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_fill_keys value PHP type {:?}",
        value_ty
    )))
}

/// Verifies the key array uses the string-slot layout expected by the runtime helper.
pub(super) fn require_array_combine_key_layout(key_elem_ty: &PhpType) -> Result<()> {
    if matches!(key_elem_ty, PhpType::Str | PhpType::Void | PhpType::Never) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_combine key element PHP type {:?}",
        key_elem_ty
    )))
}

/// Verifies the values array uses a slot layout the runtime helper can copy.
///
/// String values are deliberately excluded because indexed string arrays store 16-byte
/// inline slots, while the existing `array_combine` runtime helper reads 8-byte value slots.
pub(super) fn require_array_combine_value_layout(value_elem_ty: &PhpType) -> Result<()> {
    if matches!(
        value_elem_ty,
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Callable
            | PhpType::Void
            | PhpType::Never
    ) || value_elem_ty.is_refcounted()
    {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_combine value element PHP type {:?}",
        value_elem_ty
    )))
}

/// Verifies `array_fill_keys()` produces a hash matching the selected key/value metadata.
pub(super) fn require_array_fill_keys_result_type(
    key_elem_ty: &PhpType,
    value_ty: &PhpType,
    result_ty: &PhpType,
) -> Result<()> {
    let expected_key_ty = array_key_type_from_value_type(key_elem_ty.clone()).codegen_repr();
    match result_ty {
        PhpType::AssocArray { key, value }
            if key.codegen_repr() == expected_key_ty && value.codegen_repr() == *value_ty =>
        {
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_fill_keys result PHP type {:?} for key element PHP type {:?} and value PHP type {:?}",
            other,
            key_elem_ty,
            value_ty
        ))),
    }
}

/// Verifies `array_combine()` produces a hash with the selected value element metadata.
pub(super) fn require_array_combine_result_type(value_elem_ty: &PhpType, result_ty: &PhpType) -> Result<()> {
    match result_ty {
        PhpType::AssocArray { value, .. } if value.codegen_repr() == *value_elem_ty => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_combine result PHP type {:?} for value element PHP type {:?}",
            other, value_elem_ty
        ))),
    }
}

/// Verifies `array_flip()` produces a hash with normalized keys and integer source indexes.
pub(super) fn require_array_flip_result_type(value_elem_ty: &PhpType, result_ty: &PhpType) -> Result<()> {
    let expected_key_ty = array_key_type_from_value_type(value_elem_ty.clone()).codegen_repr();
    match result_ty {
        PhpType::AssocArray { key, value }
            if key.codegen_repr() == expected_key_ty && value.codegen_repr() == PhpType::Int =>
        {
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_flip result PHP type {:?} for value element PHP type {:?}",
            other, value_elem_ty
        ))),
    }
}

/// Verifies the destination element type matches the fill layout or is a Mixed widening.
pub(super) fn require_array_fill_result_type(value_ty: &PhpType, result_elem_ty: &PhpType) -> Result<()> {
    if value_ty == result_elem_ty || result_elem_ty == &PhpType::Mixed {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "array_fill result element PHP type {:?} for value PHP type {:?}",
        result_elem_ty, value_ty
    )))
}

/// Returns true when `array_fill()` is expected to build a keyed hash result.
pub(super) fn array_fill_result_is_assoc(result_ty: &PhpType) -> bool {
    matches!(result_ty.codegen_repr(), PhpType::AssocArray { .. })
}

/// Verifies the assoc `array_fill()` result shape expected by the runtime helper.
pub(super) fn require_array_fill_assoc_result_type(result_ty: &PhpType) -> Result<()> {
    match result_ty.codegen_repr() {
        PhpType::AssocArray { key, value }
            if key.codegen_repr() == PhpType::Int && value.codegen_repr() == PhpType::Mixed =>
        {
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_fill assoc result PHP type {:?}",
            other
        ))),
    }
}

