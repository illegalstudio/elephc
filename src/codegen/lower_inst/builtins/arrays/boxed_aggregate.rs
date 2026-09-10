//! Purpose:
//! Lowers sum and product through a uniform boxed numeric aggregate ABI.
//!
//! Called from:
//! - The typed ArraySum and ArrayProduct builtin lowerers.
//!
//! Key details:
//! - Borrows source cells without treating a Mixed pointer as an array header.
//! - Both direct and callable routes return owned numeric Mixed cells.
//! - PHP 8.3 introduced warnings for unsupported aggregate values.

use super::*;

/// Preserves runtime int-or-float results and raises a catchable error for non-array inputs.
pub(super) fn lower_aggregate(ctx: &mut FunctionContext<'_>, inst: &Instruction, product: bool) -> Result<()> {
    let name = if product { "array_product" } else { "array_sum" };
    super::super::ensure_arg_count(inst, name, 1)?;
    let source = expect_operand(inst, 0)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    super::boxed_membership::store_borrowed_cell(ctx, source, 0)?;
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1),
        i64::from(crate::codegen::compile_php_version().version_id() >= 80300));
    abi::emit_call_label(ctx.emitter, if product { "__rt_array_product_boxed" } else { "__rt_array_sum_boxed" });
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    let valid = ctx.next_label("array_aggregate_valid");
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &valid);
    crate::codegen::lower_inst::exceptions::emit_type_error(ctx,
        &format!("{name}(): Argument #1 ($array) must be of type array"));
    ctx.emitter.label(&valid);
    store_if_result(ctx, inst)
}
