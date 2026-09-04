//! Purpose:
//! Lowers globals, extern globals, constants, Mixed boxing, and invoker ref markers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers a global storage load into the result register and SSA destination slot.
pub(super) fn lower_load_global(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?;
    let symbol = ir_global_symbol(name);
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("load_global missing result value"))?;
    let ty = ctx.value_php_type(result)?;
    ctx.data
        .add_comm(symbol.clone(), ty.codegen_repr().stack_size().max(8));
    abi::emit_load_symbol_to_result(ctx.emitter, &symbol, &ty);
    store_if_result(ctx, inst)
}

/// Lowers a global storage store from one SSA operand.
pub(super) fn lower_store_global(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?.to_string();
    let symbol = ir_global_symbol(&name);
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    let store_ty = if ctx.module.web && crate::superglobals::is_superglobal(&name) {
        ty.codegen_repr()
    } else {
        let source_ty = ty.codegen_repr();
        if source_ty != PhpType::Mixed {
            if ctx.value_can_transfer_ownership_to_consumer(value)? {
                emit_box_current_owned_value_as_mixed(ctx.emitter, &source_ty);
            } else {
                emit_box_current_value_as_mixed(ctx.emitter, &source_ty);
            }
        }
        PhpType::Mixed
    };
    ctx.data
        .add_comm(symbol.clone(), store_ty.codegen_repr().stack_size().max(8));
    abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &store_ty, true);
    Ok(())
}

/// Lowers a C extern global load into the EIR result slot.
pub(super) fn lower_extern_global_load(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("extern_global_load missing result value"))?;
    let ty = ctx.value_php_type(result)?;
    let symbol = ctx.emitter.target.extern_symbol(name);
    match ty.codegen_repr() {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Resource(_)
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Callable => {
            abi::emit_load_extern_symbol_to_reg(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Float => {
            abi::emit_load_extern_symbol_to_reg(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Str => {
            abi::emit_load_extern_symbol_to_reg(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
            abi::emit_call_label(ctx.emitter, "__rt_cstr_to_str");
        }
        other => {
            ctx.emitter.comment(&format!(
                "WARNING: unsupported extern global load for ${} with PHP type {:?}",
                name, other
            ));
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers a C extern global store from one SSA operand.
pub(super) fn lower_extern_global_store(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?.to_string();
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?.codegen_repr();
    let symbol = ctx.emitter.target.extern_symbol(&name);
    match ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Resource(_)
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Callable => {
            abi::emit_store_reg_to_extern_symbol(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Float => {
            abi::emit_store_reg_to_extern_symbol(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_to_cstr");
            abi::emit_store_reg_to_extern_symbol(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        other => {
            ctx.emitter.comment(&format!(
                "WARNING: unsupported extern global store for ${} with PHP type {:?}",
                name, other
            ));
        }
    }
    Ok(())
}

/// Lowers an integer constant into the canonical integer result register and slot.
pub(super) fn lower_const_i64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_i64(inst)?;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    store_if_result(ctx, inst)
}

/// Lowers a boolean constant into the canonical integer result register and slot.
pub(super) fn lower_const_bool(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = i64::from(expect_bool(inst)?);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    store_if_result(ctx, inst)
}

/// Lowers a null constant to the selected one-word or tagged-scalar representation.
/// Lowers a read of a local no store definitely reached: PHP's warning, then `null`.
///
/// The message arrived finished from EIR — both halves are compile-time constants — so this only
/// has to hand it to the shared diagnostic funnel. The ` in FILE on line N` suffix is NOT appended
/// here: the instruction carries `MAY_WARN`, so `publish_diagnostic_location` has already stamped
/// the line, exactly as it does for every other warning.
///
/// The value produced is the same `null` `Op::ConstNull` produces, and by the same helper, so a
/// consumer expecting a tagged scalar gets one; PHP's `zval_undefined_cv` likewise answers with
/// `&EG(uninitialized_zval)` rather than a value of its own.
pub(super) fn lower_warned_null(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let data = expect_data(inst)?;
    let (label, len) = ctx.intern_string_data(data)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
    lower_const_null(ctx, inst)
}

pub(super) fn lower_const_null(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.result_php_type.codegen_repr() == PhpType::TaggedScalar {
        crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
    } else {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            0x7fff_ffff_ffff_fffe,
        );
    }
    store_if_result(ctx, inst)
}

/// Lowers explicit Mixed boxing for scalar, string, object, and existing Mixed operands.
pub(super) fn lower_mixed_box(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let source_ty = ctx.load_value_to_result(value)?;
    let raw_source_ty = ctx.raw_value_php_type(value)?;
    let box_ty = if matches!(raw_source_ty, PhpType::Resource(_)) {
        raw_source_ty
    } else {
        source_ty
    };
    emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
    store_if_result(ctx, inst)
}

/// Clones a boxed Mixed zval cell so later mutation cannot rewrite an aliased source cell.
pub(super) fn lower_mixed_clone(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    load_value_to_first_int_arg(ctx, value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    store_if_result(ctx, inst)
}

/// Lowers an invoker-only by-reference argument marker for descriptor calls.
pub(super) fn lower_invoker_ref_arg(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let source_ty = ctx.local_php_type(slot)?.codegen_repr();
    let ref_cell_reg = abi::secondary_scratch_reg(ctx.emitter);
    let marker_tag_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let source_tag_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.materialize_local_storage_address(slot, ref_cell_reg)?;
    abi::emit_load_int_immediate(
        ctx.emitter,
        marker_tag_reg,
        callable_invoker_args::INVOKER_ARG_REF_CELL_TAG,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        source_tag_reg,
        crate::codegen::runtime_value_tag(&source_ty) as i64,
    );
    ctx.emitter.comment("cufa_invoker_ref_cell");
    emit_box_runtime_payload_as_mixed(ctx.emitter, marker_tag_reg, ref_cell_reg, source_tag_reg);
    store_if_result(ctx, inst)
}
