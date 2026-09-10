//! Purpose:
//! Adapts both array layouts to descriptor-driven comparator set selection.
//!
//! Called from:
//! - The ArrayUdiff and ArrayUintersect runtime-function dispatchers.
//!
//! Key details:
//! - Stack triples borrow EIR operands; the runtime consumes only the acquired descriptor.
//! - Both arrays are validated before callback ownership is acquired.

use super::*;

const BORROWED_BYTES: usize = 96;

/// Selects first-array entries that compare unequal to every second-array value.
pub(crate) fn lower_array_udiff(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_comparator(ctx, inst, "array_udiff", 0)
}

/// Selects first-array entries with an equal second-array value, preserving original keys.
pub(crate) fn lower_array_uintersect(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_comparator(ctx, inst, "array_uintersect", 1)
}

/// Normalizes the comparator once and returns the runtime's independently owned Mixed result.
fn lower_comparator(ctx: &mut FunctionContext<'_>, inst: &Instruction, name: &str, mode: i64) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 3)?;
    let first = expect_operand(inst, 0)?;
    let second = expect_operand(inst, 1)?;
    let callback = expect_operand(inst, 2)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, BORROWED_BYTES);
    super::boxed_membership::store_borrowed_cell(ctx, first, 0)?;
    super::boxed_membership::store_borrowed_cell(ctx, callback, 32)?;
    super::boxed_membership::store_borrowed_cell(ctx, second, 64)?;
    for (index, offset) in [(1, 0), (2, 64)] {
        super::boxed_predicates::validate_source_at(ctx,
            &format!("{name}(): Argument #{index} ($array{index}) must be of type array"),
            BORROWED_BYTES, offset);
    }
    let error = if mode == 0 {
        "array_udiff(): Argument #3 ($callback) must be a valid callback"
    } else {
        "array_uintersect(): Argument #3 ($callback) must be a valid callback"
    };
    super::boxed_predicates::acquire_callback_descriptor(ctx, inst, callback, name, error)?;
    abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), abi::int_result_reg(ctx.emitter));
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 0);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), 64);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 3), mode);
    abi::emit_call_label(ctx.emitter, "__rt_array_udiff_uintersect");
    abi::emit_release_temporary_stack(ctx.emitter, BORROWED_BYTES);
    store_if_result(ctx, inst)
}
