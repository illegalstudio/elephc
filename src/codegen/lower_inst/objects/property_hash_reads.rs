//! Purpose:
//! Shares presence-aware boxed property reads for dynamic tables and stdClass.
//!
//! Called from:
//! - Runtime-named property resolution and ordinary checked stdClass reads.
//!
//! Key details:
//! - Hash presence is independent of the boxed value tag, including initialized null.
//! - The caller's saved receiver and key are consumed once before any missing-read Error.

use super::*;

/// Reads stdClass from prepared ABI arguments through its property-hash slot.
pub(super) fn emit_initialized_stdclass_get(ctx: &mut FunctionContext<'_>, property: Option<&str>) -> Result<()> {
    let target = ctx.emitter.target;
    abi::emit_push_reg(ctx.emitter, abi::int_arg_reg_name(target, 0));
    abi::emit_push_reg_pair(ctx.emitter, abi::int_arg_reg_name(target, 1), abi::int_arg_reg_name(target, 2));
    let message = property.map(elephc_semantics::initialization::property_message)
        .unwrap_or_else(|| elephc_semantics::initialization::DYNAMIC_PROPERTY_MESSAGE.to_string());
    emit_stacked_property_hash_get(ctx, 8, 16, 0, 32, &message)
}

/// Retains a present boxed entry or applies the original instruction's missing-read policy.
pub(super) fn emit_stacked_property_hash_get(
    ctx: &mut FunctionContext<'_>, hash_offset: usize, receiver_offset: usize,
    name_offset: usize, frame_bytes: usize, missing_message: &str,
) -> Result<()> {
    let target = ctx.emitter.target;
    let miss_label = ctx.next_label("runtime_dynamic_prop_hash_miss");
    let done_label = ctx.next_label("runtime_dynamic_prop_hash_done");
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, receiver_offset);
    abi::emit_load_from_address(
        ctx.emitter,
        abi::int_arg_reg_name(target, 0),
        object_reg,
        hash_offset,
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_arg_reg_name(target, 1), name_offset);
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_arg_reg_name(target, 2),
        name_offset + 8,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_get");
    // Released before the found test so the branch reads freshly computed flags on every target.
    abi::emit_release_temporary_stack(ctx.emitter, frame_bytes);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &miss_label);
    match target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // return the boxed Mixed cell stored in the hash entry
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // return the boxed Mixed cell stored in the hash entry
        }
    }
    abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Mixed);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&miss_label);
    if ctx.property_read_requires_initialization() {
        super::super::exceptions::emit_error(ctx, missing_message);
    }
    emit_boxed_null(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}
