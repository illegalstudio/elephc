//! Purpose:
//! Lowers throw expressions and exception-handler stack instructions.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET,
};

/// Lowers expression-form `throw` through the same runtime path as throw terminators.
pub(super) fn lower_throw_exception(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    super::super::lower_term::lower_throw_value(ctx, value)
}

/// Lowers a static-message catchable PHP `Error` without evaluating later operands.
pub(super) fn lower_throw_error(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if !inst.operands.is_empty() {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expects no operands",
            inst.op.name()
        )));
    }
    let data = expect_data(inst)?;
    let message = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?
        .clone();
    exceptions::emit_error(ctx, &message);
    Ok(())
}

/// Lowers a runtime-string catchable PHP `Error` without evaluating later operands.
pub(super) fn lower_throw_error_value(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let message = expect_operand(inst, 0)?;
    exceptions::emit_error_value(ctx, message)
}

/// Pushes an EIR exception handler and branches to the handler block after `longjmp`.
pub(super) fn lower_try_push_handler(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let token = expect_i64(inst)?;
    let handler_offset = ctx.try_handler_offset(token)?;
    let handler_block = BlockId::from_raw(token as u32);
    let handler_label = ctx.block_label_for_id(handler_block)?;
    let scratch = abi::temp_int_reg(ctx.emitter.target);

    ctx.emitter.comment("push EIR exception handler");
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch, "_exc_handler_top", 0);
    abi::store_at_offset(ctx.emitter, scratch, handler_offset);
    abi::emit_load_int_immediate(ctx.emitter, scratch, 0);
    abi::store_at_offset(ctx.emitter, scratch, handler_offset - 8);
    abi::emit_load_symbol_to_reg(ctx.emitter, scratch, "_rt_diag_suppression", 0);
    abi::store_at_offset(
        ctx.emitter,
        scratch,
        handler_offset - TRY_HANDLER_DIAG_DEPTH_OFFSET,
    );
    abi::emit_frame_slot_address(ctx.emitter, scratch, handler_offset);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_exc_handler_top", 0);
    abi::emit_frame_slot_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        handler_offset - TRY_HANDLER_JMP_BUF_OFFSET,
    );
    ctx.emitter.bl_c("setjmp");
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &handler_label);
    Ok(())
}

/// Pops an EIR exception handler and restores the saved diagnostic-suppression depth.
pub(super) fn lower_try_pop_handler(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let token = expect_i64(inst)?;
    let handler_offset = ctx.try_handler_offset(token)?;
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    ctx.emitter.comment("pop EIR exception handler");
    abi::load_at_offset(ctx.emitter, scratch, handler_offset);
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_exc_handler_top", 0);
    abi::load_at_offset(
        ctx.emitter,
        scratch,
        handler_offset - TRY_HANDLER_DIAG_DEPTH_OFFSET,
    );
    abi::emit_store_reg_to_symbol(ctx.emitter, scratch, "_rt_diag_suppression", 0);
    Ok(())
}

/// Loads the currently active exception object from the runtime exception slot.
pub(super) fn lower_catch_current(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "_exc_value",
        0,
    );
    store_if_result(ctx, inst)
}

/// Takes the active exception into an owned SSA result and clears catch-scoped runtime state.
pub(super) fn lower_catch_bind(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("catch_bind missing owned result"))?;
    let result_ty = ctx.value_php_type(result)?;
    abi::emit_load_symbol_to_result(ctx.emitter, "_exc_value", &result_ty);
    ctx.store_result_value(result)?;
    abi::emit_store_zero_to_symbol(ctx.emitter, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_unser_trace_active", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_unser_trace_exception_ptr", 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, "_dateperiod_foreach_trace_active", 0);
    abi::emit_store_zero_to_symbol(
        ctx.emitter,
        "_dateperiod_foreach_trace_exception_ptr",
        0,
    );
    Ok(())
}
