//! Purpose:
//! Array map type validation and Mixed result boxing.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Returns an indexed-array element type compatible with callback runtime helpers.
pub(super) fn eight_byte_callback_array_element_type(ty: PhpType, name: &str) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => eight_byte_callback_value_type(*elem, name),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Returns the indexed-array element type accepted by `array_map()` callback runtimes.
pub(super) fn array_map_callback_array_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(
                elem,
                PhpType::Int
                    | PhpType::Bool
                    | PhpType::Str
                    | PhpType::Void
                    | PhpType::Never
                    | PhpType::Mixed
            ) {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "array_map indexed-array element PHP type {:?}",
                elem
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_map for PHP type {:?}",
            other
        ))),
    }
}

/// Returns a scalar callback value type that fits in one integer ABI register.
pub(super) fn eight_byte_callback_value_type(ty: PhpType, name: &str) -> Result<PhpType> {
    let ty = ty.codegen_repr();
    if matches!(
        ty,
        PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never
    ) {
        Ok(ty)
    } else {
        Err(CodegenIrError::unsupported(format!(
            "{} PHP type {:?}",
            name, ty
        )))
    }
}

/// Stores the void sentinel, boxing it when the EIR builtin result slot is Mixed-like.
pub(super) fn store_void_builtin_result(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    if inst.result.is_some()
        && matches!(
            inst.result_php_type.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
    }
    store_if_result(ctx, inst)
}

/// Returns the indexed-array slot type produced by the selected `array_map()` runtime helper.
pub(super) fn array_map_callback_result_element_type(return_ty: &PhpType) -> Result<PhpType> {
    let return_ty = return_ty.codegen_repr();
    if matches!(return_ty, PhpType::Int | PhpType::Bool | PhpType::Str | PhpType::Mixed) {
        Ok(return_ty)
    } else {
        Err(CodegenIrError::unsupported(format!(
            "array_map callback return PHP type {:?}",
            return_ty
        )))
    }
}

/// Returns the mapped-VALUE view of the `array_map()` EIR result slot.
///
/// The slot is an indexed array for an indexed source and an associative array for an
/// associative one, and only the mapped value type differs between them — the callback dispatch
/// never looks at the key. `None` means the slot is not an array shape at all.
pub(super) fn array_map_result_slot_element_type(inst: &Instruction) -> Option<PhpType> {
    match inst.result_php_type.codegen_repr() {
        PhpType::Array(elem) => Some(elem.codegen_repr()),
        PhpType::AssocArray { value, .. } => Some(value.codegen_repr()),
        _ => None,
    }
}

/// Returns the descriptor callback result element type from the EIR result slot metadata.
pub(super) fn array_map_descriptor_callback_result_element_type(inst: &Instruction) -> Result<PhpType> {
    if let Some(elem) = array_map_result_slot_element_type(inst) {
        if matches!(
            elem,
            PhpType::Int | PhpType::Bool | PhpType::Str | PhpType::Mixed
        ) {
            return Ok(elem);
        }
        return Err(CodegenIrError::unsupported(format!(
            "array_map descriptor callback result element PHP type {:?}",
            elem
        )));
    }
    match inst.result_php_type.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => Ok(PhpType::Mixed),
        other => Err(CodegenIrError::unsupported(format!(
            "array_map descriptor callback result PHP type {:?}",
            other
        ))),
    }
}

/// Returns the element type expected by the EIR `array_map()` result slot.
pub(super) fn array_map_result_element_type(
    inst: &Instruction,
    callback_elem_ty: &PhpType,
) -> Result<PhpType> {
    if let Some(result_elem_ty) = array_map_result_slot_element_type(inst) {
        if &result_elem_ty == callback_elem_ty || result_elem_ty == PhpType::Mixed {
            return Ok(result_elem_ty);
        }
        return Err(CodegenIrError::unsupported(format!(
            "array_map result element PHP type {:?} for callback result PHP type {:?}",
            result_elem_ty, callback_elem_ty
        )));
    }
    match inst.result_php_type.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => Ok(callback_elem_ty.clone()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_map result PHP type {:?}",
            other
        ))),
    }
}

/// Boxes an indexed-array result when the EIR builtin result slot is Mixed-like.
///
/// OWNERSHIP — the container reference is TRANSFERRED into the Mixed cell, it is not shared.
/// Every caller reaches here holding a container the builtin's own runtime helper just
/// allocated (`__rt_array_map*` / `__rt_array_column` call `__rt_array_new`), so the pointer in
/// the result register carries refcount 1 that NOBODY else tracks: once the box happens, the EIR
/// value for this instruction is the MIXED CELL, not the container, so every downstream
/// EIR-emitted retain/release — including the one `emit_store_result_to_symbol` performs — acts
/// on the Mixed cell and can never reach the container's original reference.
///
/// `__rt_mixed_from_value` INCREFs a container-tagged payload
/// (`src/codegen_support/runtime/arrays/mixed_from_value.rs`, the `_retain` arm), which is the
/// correct contract for a BORROWED payload but leaves a fresh one at refcount 2 with only one
/// release ever scheduled — the container and its element buffer then outlive the program.
/// `emit_box_current_owned_value_as_mixed` boxes and then releases that original reference, so
/// the Mixed cell ends up the container's single owner.
///
/// The transfer belongs HERE and only here: when the result slot is NOT Mixed-like no box is
/// emitted, the container itself stays the EIR value, and the EIR's own release owns it (the
/// `array_flip` shape). Releasing outside this `if` would duplicate that release.
pub(super) fn box_array_result_for_mixed_builtin(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    elem_ty: &PhpType,
) {
    if inst.result.is_some()
        && matches!(
            inst.result_php_type.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    {
        emit_box_current_owned_value_as_mixed(
            ctx.emitter,
            &PhpType::Array(Box::new(elem_ty.clone())),
        );
    }
}
