//! Purpose:
//! Allocates eval-declared classes from constructorless native prelude factories.
//!
//! Called from:
//! - The dynamic constructorless object opcode fallback on every supported target.
//!
//! Key details:
//! - An active interpreter frame supplies context to helpers with no eval local.
//! - Success retires the saved class string; a miss leaves it for the normal diagnostic.

use super::*;

/// Probes the active eval class table without invoking a user constructor.
pub(in crate::codegen::lower_inst::builtins) fn lower_eval_constructorless_fallback(
    ctx: &mut FunctionContext<'_>, miss_label: &str,
) -> Result<()> {
    let missing = ctx.next_label("eval_constructorless_missing");
    let done = ctx.next_label("eval_constructorless_done");
    let name_ptr = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let name_len = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_ptr, 0);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_len, 8);
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_store_to_sp(ctx.emitter, name_ptr, EVAL_CODE_PTR_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, name_len, EVAL_CODE_LEN_OFFSET);
    if has_eval_context(ctx) {
        ensure_eval_context(ctx)?;
        load_eval_context_to_arg(ctx, 0);
    } else {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), 0);
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_ptr, EVAL_CODE_PTR_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, name_len, EVAL_CODE_LEN_OFFSET);
    abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 3), 0);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_try_allocate_object_v1");
    abi::emit_call_label(ctx.emitter, &symbol);
    emit_branch_if_eval_c_int_negative(ctx, &missing);
    emit_eval_status_check(ctx);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), EVAL_RESULT_VALUE_CELL_OFFSET);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES + 16);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&missing);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_jump(ctx.emitter, miss_label);
    ctx.emitter.label(&done);
    Ok(())
}
