//! Purpose:
//! Lowers `array_pad()` and `array_chunk()` through `__rt_array_pad_boxed` /
//! `__rt_array_chunk_boxed` for the shapes their typed helpers cannot read.
//!
//! Called from:
//! - `super::basic::lower_array_pad()` / `lower_array_chunk()` when the checker typed the call as
//!   the boxed PHP array.
//!
//! Key details:
//! - Stack cells borrow the EIR operands; the runtime copies what it keeps.
//! - The length guards are the typed paths' own: they inspect the length while it sits in the
//!   second ABI argument register, before the helper runs.

use super::*;

const BORROWED_BYTES: usize = 64;

/// Lowers `array_pad($array, $length, $value)` through the boxed builder.
pub(super) fn lower_boxed_array_pad(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let length = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, BORROWED_BYTES);
    super::boxed_membership::store_borrowed_cell(ctx, array, 0)?;
    super::boxed_membership::store_borrowed_cell(ctx, value, 32)?;
    ctx.load_value_to_reg(length, abi::int_arg_reg_name(ctx.emitter.target, 1))?;
    super::slice_splice::emit_array_pad_length_guard(ctx);
    for (index, offset) in [(0, 0), (2, 32)] {
        abi::emit_temporary_stack_address(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, index),
            offset,
        );
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_pad_boxed");
    abi::emit_release_temporary_stack(ctx.emitter, BORROWED_BYTES);
    raise_unless_array(ctx, "array_pad(): Argument #1 ($array) must be of type array");
    box_hash_result_for_mixed_builtin(ctx, inst, &PhpType::Mixed);
    store_if_result(ctx, inst)
}

/// Lowers `array_chunk($array, $length[, $preserve_keys])` through the boxed builder.
pub(super) fn lower_boxed_array_chunk(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let array = expect_operand(inst, 0)?;
    let length = expect_operand(inst, 1)?;
    let preserve = inst.operands.get(2).copied();
    abi::emit_reserve_temporary_stack(ctx.emitter, BORROWED_BYTES);
    super::boxed_membership::store_borrowed_cell(ctx, array, 0)?;
    let flag = abi::int_arg_reg_name(ctx.emitter.target, 2);
    match preserve {
        Some(preserve) => {
            ctx.load_value_to_reg(preserve, flag)?;
            abi::emit_store_to_sp(ctx.emitter, flag, 32);
        }
        None => {
            abi::emit_load_int_immediate(ctx.emitter, flag, 0);
            abi::emit_store_to_sp(ctx.emitter, flag, 32);
        }
    }
    ctx.load_value_to_reg(length, abi::int_arg_reg_name(ctx.emitter.target, 1))?;
    super::emit_array_chunk_length_guard(ctx);
    abi::emit_load_temporary_stack_slot(ctx.emitter, flag, 32);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    abi::emit_call_label(ctx.emitter, "__rt_array_chunk_boxed");
    abi::emit_release_temporary_stack(ctx.emitter, BORROWED_BYTES);
    raise_unless_array(ctx, "array_chunk(): Argument #1 ($array) must be of type array");
    box_hash_result_for_mixed_builtin(ctx, inst, &PhpType::Mixed);
    store_if_result(ctx, inst)
}

/// Turns the builders' zero result into php's `TypeError` for a non-array first operand.
fn raise_unless_array(ctx: &mut FunctionContext<'_>, message: &str) {
    let valid = ctx.next_label("array_builder_valid");
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &valid);
    crate::codegen::lower_inst::exceptions::emit_type_error(ctx, message);
    ctx.emitter.label(&valid);
}
