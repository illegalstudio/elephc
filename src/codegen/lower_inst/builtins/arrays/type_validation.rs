//! Purpose:
//! Array filter/map type validation and Mixed result boxing.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - Preserves callback ABI, target parity, array storage, and ownership contracts.

use super::*;

/// Verifies the aggregate can use the current raw integer-slot runtime helper.
pub(super) fn require_supported_indexed_array(ty: PhpType, name: &str) -> Result<()> {
    match ty.codegen_repr() {
        PhpType::Array(elem) if matches!(*elem, PhpType::Int | PhpType::Bool | PhpType::Never) => {
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}

/// Returns the indexed-array element type supported by the current filter runtime helpers.
pub(super) fn array_filter_source_element_type(ty: PhpType) -> Result<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => {
            let elem = elem.codegen_repr();
            if matches!(
                elem,
                PhpType::Int | PhpType::Bool | PhpType::Str | PhpType::Void | PhpType::Never
            ) || elem.is_refcounted()
            {
                return Ok(elem);
            }
            Err(CodegenIrError::unsupported(format!(
                "array_filter indexed-array element PHP type {:?}",
                elem
            )))
        }
        // An associative source filters through the same runtime; only its keys already exist.
        PhpType::AssocArray { value, .. } => {
            let value = value.codegen_repr();
            if matches!(
                value,
                PhpType::Int | PhpType::Bool | PhpType::Str | PhpType::Void | PhpType::Never
            ) || value.is_refcounted()
            {
                return Ok(value);
            }
            Err(CodegenIrError::unsupported(format!(
                "array_filter associative-array value PHP type {:?}",
                value
            )))
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_filter for PHP type {:?}",
            other
        ))),
    }
}

/// Verifies the filtered result preserves the source element type metadata.
pub(super) fn require_array_filter_result_type(source_elem_ty: &PhpType, result_ty: &PhpType) -> Result<()> {
    match result_ty {
        // php's `array_filter()` preserves keys, so the result is a keyed table whatever the
        // source was — an indexed array cannot express `[1 => 1, 2 => 2]`.
        PhpType::AssocArray { value, .. }
            if value.codegen_repr() == source_elem_ty.codegen_repr()
                || matches!(source_elem_ty, PhpType::Never | PhpType::Void) =>
        {
            Ok(())
        }
        PhpType::Array(elem)
            if elem.codegen_repr() == source_elem_ty.codegen_repr()
                || matches!(source_elem_ty, PhpType::Never | PhpType::Void) =>
        {
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_filter result PHP type {:?} for source element PHP type {:?}",
            other, source_elem_ty
        ))),
    }
}

/// Loads the optional `array_filter()` mode operand into the runtime helper register.
pub(super) fn load_array_filter_mode(
    ctx: &mut FunctionContext<'_>,
    mode: Option<ValueId>,
    reg: &str,
) -> Result<()> {
    if let Some(mode) = mode {
        ctx.load_value_to_reg(mode, reg)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, reg, 0);
    }
    Ok(())
}

/// Returns the visible callback argument types for `array_filter()` mode.
pub(super) fn array_filter_callback_arg_types(
    ctx: &FunctionContext<'_>,
    mode: Option<ValueId>,
    elem_ty: &PhpType,
) -> Result<Option<Vec<PhpType>>> {
    match static_array_filter_mode(ctx, mode)? {
        Some(1) => Ok(Some(vec![elem_ty.codegen_repr(), PhpType::Int])),
        Some(2) => Ok(Some(vec![PhpType::Int])),
        Some(_) => Ok(Some(vec![elem_ty.codegen_repr()])),
        None => Ok(None),
    }
}

/// Returns a compile-time `array_filter()` mode when it is visible in EIR.
pub(super) fn static_array_filter_mode(
    ctx: &FunctionContext<'_>,
    mode: Option<ValueId>,
) -> Result<Option<i64>> {
    let Some(mode) = mode else {
        return Ok(Some(0));
    };
    array_filter_mode_const_i64(ctx, mode)
}

/// Returns a visible integer mode from a direct constant or same-block local load.
pub(super) fn array_filter_mode_const_i64(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<i64>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { block, index, inst } = value_ref.def else {
        return Ok(None);
    };
    let Some(inst_ref) = ctx.function.instruction(inst) else {
        return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
    };
    let inst_ref = if inst_ref.op == Op::LoadLocal {
        let Some(inst_ref) =
            array_filter_local_mode_source_instruction(ctx, block, index, inst_ref)?
        else {
            return Ok(None);
        };
        inst_ref
    } else {
        inst_ref
    };
    if inst_ref.op != Op::ConstI64 {
        return Ok(None);
    }
    let Some(Immediate::I64(value)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "array_filter mode const_i64 has no immediate",
        ));
    };
    Ok(Some(value))
}

/// Resolves an `array_filter()` mode local load to the last same-block store before it.
pub(super) fn array_filter_local_mode_source_instruction<'a>(
    ctx: &'a FunctionContext<'_>,
    block: BlockId,
    load_index: u32,
    load_inst: &Instruction,
) -> Result<Option<&'a Instruction>> {
    let Some(Immediate::LocalSlot(slot)) = load_inst.immediate else {
        return Err(CodegenIrError::invalid_module(
            "array_filter mode load_local has no local slot",
        ));
    };
    let block_ref = ctx
        .function
        .block(block)
        .ok_or_else(|| CodegenIrError::missing_entry("block", block.as_raw()))?;
    let mut stored = None;
    for (index, inst_id) in block_ref.instructions.iter().enumerate() {
        if index as u32 >= load_index {
            break;
        }
        let inst_ref = ctx
            .function
            .instruction(*inst_id)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
        if inst_ref.op == Op::StoreLocal
            && matches!(inst_ref.immediate, Some(Immediate::LocalSlot(candidate)) if candidate == slot)
        {
            stored = inst_ref.operands.first().copied();
        }
    }
    let Some(stored) = stored else {
        return Ok(None);
    };
    let Some(value_ref) = ctx.function.value(stored) else {
        return Err(CodegenIrError::missing_entry("value", stored.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    ctx.function
        .instruction(inst)
        .map(Some)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))
}

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

/// Boxes the integer runtime result when the EIR builtin result slot is Mixed-like.
pub(super) fn box_int_result_for_mixed_builtin(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    if inst.result.is_some()
        && matches!(
            inst.result_php_type.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    {
        emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
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
    if matches!(return_ty, PhpType::Int | PhpType::Bool | PhpType::Str) {
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

