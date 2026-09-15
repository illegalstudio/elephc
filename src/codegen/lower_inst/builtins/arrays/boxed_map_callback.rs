//! Purpose:
//! Resolves boxed array_map callbacks and owns their temporary descriptor environments.
//!
//! Called from:
//! - `super::map_dispatch::lower_array_map()` for Mixed callback operands.
//!
//! Key details:
//! - Retaining the resolved descriptor protects captured state if a callback replaces its source.
//! - Exception records cover descriptor invocation and result cleanup across every native ABI.

use super::*;

/// Maps through a runtime-selected boxed callback without losing captures or temporary owners.
pub(super) fn lower_array_map_mixed_callback(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    callback: ValueId,
    array: ValueId,
    elem_ty: &PhpType,
    target: ArrayMapTarget,
) -> Result<()> {
    let identity = ctx.next_label("array_map_null_callback");
    let done = ctx.next_label("array_map_mixed_callback_done");
    ctx.load_value_to_result(callback)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #8");                              // a null callback returns the single source array unchanged
            ctx.emitter.instruction(&format!("b.eq {identity}"));               // bypass callable resolution for the identity form
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 8");                              // a null callback returns the single source array unchanged
            ctx.emitter.instruction(&format!("je {identity}"));                 // bypass callable resolution for the identity form
        }
    }
    let callback_elem_ty = PhpType::Mixed;
    let result_elem_ty = array_map_result_element_type(inst, &callback_elem_ty)?;
    let wrapper = emit_descriptor_callback_wrapper(
        ctx, vec![elem_ty.clone()], callback_elem_ty.clone(),
    );
    crate::codegen::lower_inst::callables::emit_runtime_mixed_callable_descriptor_value_with_type_error(
        ctx, callback, "array_map",
        "array_map(): Argument #1 ($callback) must be a valid callback or null",
    )?;
    let result = abi::int_result_reg(ctx.emitter);
    let owner = abi::tertiary_scratch_reg(ctx.emitter);
    let env_bytes = reserve_descriptor_callback_env_from_reg(ctx, result);
    abi::emit_temporary_stack_address(ctx.emitter, owner, 0);
    abi::emit_push_call_operand_owner(ctx.emitter, owner, true);
    let callback_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let array_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    let env_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, callback_arg, &wrapper);
    ctx.load_value_to_reg(array, array_arg)?;
    abi::emit_temporary_stack_address(ctx.emitter, env_arg, 48);
    emit_array_map_runtime_call(ctx, &callback_elem_ty, env_bytes, target)?;
    finish_array_map_result(ctx, inst, target, &callback_elem_ty, &result_elem_ty)?;
    abi::emit_pop_call_operand_owner(ctx.emitter);

    // Retiring a captured object can invoke PHP. Protect the completed result
    // until the descriptor has been released, including a throwing destructor.
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_temporary_stack_address(ctx.emitter, owner, 0);
    abi::emit_push_call_operand_owner(ctx.emitter, owner, false);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 64);
    callable_descriptor::emit_release_current_descriptor(ctx.emitter);
    abi::emit_pop_call_operand_owner(ctx.emitter);
    abi::emit_pop_reg(ctx.emitter, result);
    abi::emit_release_temporary_stack(ctx.emitter, env_bytes);
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&identity);
    lower_array_map_identity(ctx, inst, array)?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Returns a separate boxed array owner for the single-array, null-callback identity form.
pub(super) fn lower_array_map_identity(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    let source_ty = ctx.load_value_to_result(array)?.codegen_repr();
    if source_ty == PhpType::Mixed {
        abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, &source_ty);
    }
    store_if_result(ctx, inst)
}
