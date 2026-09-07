//! Purpose:
//! Extends native Core inventories with live declarations from the eval context.
//!
//! Called from:
//! - Core constant and function inventory emitters through the eval facade.
//!
//! Key details:
//! - The native string-array and Mixed-hash representations stay unchanged.
//! - Borrowed bridge names are copied and constant cells retained by each snapshot.

use super::*;

/// Appends dynamic constants or function names to the owned container in the result register.
pub(in crate::codegen::lower_inst::builtins) fn append_eval_inventory(ctx: &mut FunctionContext<'_>, constants: bool) -> Result<()> {
    if !has_eval_context(ctx) { return Ok(()); }
    let result = abi::int_result_reg(ctx.emitter);
    let loop_label = ctx.next_label("eval_inventory_next");
    let done = ctx.next_label("eval_inventory_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    abi::emit_store_to_sp(ctx.emitter, result, 32);
    ensure_eval_context(ctx)?;
    abi::emit_load_int_immediate(ctx.emitter, result, 0);
    abi::emit_store_to_sp(ctx.emitter, result, 40);
    ctx.emitter.label(&loop_label);
    load_eval_context_to_arg(ctx, 0);
    let arg1 = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let arg2 = abi::int_arg_reg_name(ctx.emitter.target, 2);
    let arg3 = abi::int_arg_reg_name(ctx.emitter.target, 3);
    abi::emit_load_int_immediate(ctx.emitter, arg1, i64::from(constants));
    abi::emit_load_temporary_stack_slot(ctx.emitter, arg2, 40);
    abi::emit_temporary_stack_address(ctx.emitter, arg3, 48);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_inventory_entry");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &done);
    if constants {
        abi::emit_load_temporary_stack_slot(ctx.emitter, result, 64);
        abi::emit_call_label(ctx.emitter, "__rt_incref");
    }
    for (arg, offset) in [(0, 32), (1, 48), (2, 56)] {
        let reg = abi::int_arg_reg_name(ctx.emitter.target, arg);
        abi::emit_load_temporary_stack_slot(ctx.emitter, reg, offset);
    }
    if constants {
        abi::emit_load_temporary_stack_slot(ctx.emitter, arg3, 64);
        for (arg, value) in [(4, 0), (5, 7)] {
            let reg = abi::int_arg_reg_name(ctx.emitter.target, arg);
            abi::emit_load_int_immediate(ctx.emitter, reg, value);
        }
        abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    } else {
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
    }
    abi::emit_store_to_sp(ctx.emitter, result, 32);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 40);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("add x0, x0, #1"),             // advance the stable declaration index
        Arch::X86_64 => ctx.emitter.instruction("add rax, 1"),                  // advance the stable declaration index
    }
    abi::emit_store_to_sp(ctx.emitter, result, 40);
    abi::emit_jump(ctx.emitter, &loop_label);
    ctx.emitter.label(&done);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 32);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_STACK_BYTES);
    Ok(())
}
