//! Purpose:
//! Joins prepared dense arrays while protecting normalization and result owners.
//!
//! Called from:
//! - The implode and strip_tags native lowerers after materializing join arguments.
//!
//! Key details:
//! - Normalization owners remain visible to same-frame exception cleanup.
//! - The owned string result survives destructors invoked by array retirement.

use super::*;

/// Produces one owned join result and retires an optional normalized array owner.
pub(super) fn emit_owned_implode(
    ctx: &mut FunctionContext<'_>,
    runtime_label: &str,
    normalized_copy: Option<PhpType>,
) {
    let Some(value_ty) = normalized_copy else {
        abi::emit_call_label(ctx.emitter, runtime_label);
        abi::emit_call_label(ctx.emitter, "__rt_str_persist");
        return;
    };
    let array_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x3",
        Arch::X86_64 => "rdx",
    };
    abi::emit_push_reg(ctx.emitter, array_reg);
    let owner_address = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_temporary_stack_address(ctx.emitter, owner_address, 0);
    abi::emit_push_call_operand_owner(ctx.emitter, owner_address, false);
    abi::emit_call_label(ctx.emitter, runtime_label);
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    abi::emit_pop_call_operand_owner(ctx.emitter);
    abi::emit_push_result_value(ctx.emitter, &PhpType::Str);
    abi::emit_temporary_stack_address(ctx.emitter, owner_address, 0);
    abi::emit_push_call_operand_owner(ctx.emitter, owner_address, false);
    let array_offset = abi::CALL_OPERAND_OWNER_RECORD_BYTES + 16;
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), array_offset);
    abi::emit_decref_if_refcounted(ctx.emitter, &PhpType::Array(Box::new(value_ty)));
    abi::emit_pop_call_operand_owner(ctx.emitter);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}
