//! Purpose:
//! Adapts callback return storage to the checked integer comparator ABI.
//!
//! Called from:
//! - User-sort callback dispatch once the original EIR invocation selects checked comparisons.
//!
//! Key details:
//! - Static and descriptor callbacks share one consuming runtime guard.
//! - Declared integer returns already satisfy the contract without an extra wrapper.

use super::*;

/// Preserves a descriptor's original return cell until the selected invocation validates it.
pub(super) fn descriptor_label(
    ctx: &mut FunctionContext<'_>, inst: &Instruction, inner_label: &str,
) -> String {
    if inst.strict_comparisons() { checked_return(ctx, inner_label, &PhpType::Mixed) }
    else { inner_label.to_string() }
}

/// Checks a selected callback result without invoking a hidden PHP integer conversion.
pub(super) fn checked_return(
    ctx: &mut FunctionContext<'_>, inner_label: &str, return_ty: &PhpType,
) -> String {
    let return_ty = return_ty.codegen_repr();
    if matches!(return_ty, PhpType::Int | PhpType::Never) { return inner_label.to_string(); }
    let wrapper = ctx.next_label("checked_comparator_return");
    let done = ctx.next_label("after_checked_comparator_return");
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&wrapper);
    abi::emit_frame_prologue(ctx.emitter, 32);
    abi::emit_call_label(ctx.emitter, inner_label);
    if !matches!(return_ty, PhpType::Mixed | PhpType::Union(_)) {
        crate::codegen_support::emit_box_current_owned_value_as_mixed(ctx.emitter, &return_ty);
    }
    abi::emit_call_label(ctx.emitter, "__rt_strict_comparator_owned");
    abi::emit_frame_restore(ctx.emitter, 32);
    abi::emit_return(ctx.emitter);
    ctx.emitter.label(&done);
    wrapper
}
