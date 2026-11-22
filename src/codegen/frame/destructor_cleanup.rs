//! Purpose:
//! Releases every abandoned PHP-frame owner through protected, resumable cleanup steps.
//!
//! Called from:
//! - Native destructor return epilogues and PHP-frame exception activation callbacks.
//!
//! Key details:
//! - Each step borrows the original PHP frame and clears its owner slot before releasing it.
//! - Pending child exceptions replace or chain the active exception without skipping later owners.
//! - Raw reference-cell storage is freed after its payload cleanup, including when that cleanup throws.
//! - Tracked Mixed references release their managed wrapper, preserving any other identity owners.

use super::*;
use elephc_memoir::FrameOffset;
use elephc_memoir::frame_owners::{
    emit_raw_ref_payload_callback, emit_release_borrowed_raw_ref_cell, emit_take_reference_owner,
};
pub(super) use elephc_memoir::frame_owners::{enter_borrowed_frame, leave_borrowed_frame};

/// One owner whose cleanup can execute PHP code while the abandoned frame remains readable.
enum Owner {
    Local(LocalSlotId, PhpType, usize),
    Reference(PhpType, usize),
    EvalScope(usize),
    EvalContext(usize),
}

/// Recognizes destructor bodies using the same case-insensitive method identity as PHP.
pub(in crate::codegen) fn is_destructor(function: &Function) -> bool {
    function.name.rsplit_once("::")
        .is_some_and(|(_, method)| method.eq_ignore_ascii_case("__destruct"))
}

/// Calls the complete cleanup walker on normal return and propagates throws after removing its activation.
pub(super) fn emit_call(ctx: &mut FunctionContext<'_>) {
    let entry = ctx.epilogue_label.as_deref().unwrap().strip_suffix("_epilogue").unwrap();
    let callback = format!("{entry}__cdylib_exception_cleanup");
    let arm = ctx.emitter.target.arch == Arch::AArch64;
    ctx.emitter.instruction(if arm { "mov x0, x29" } else { "mov rdi, rbp" });  // borrow this PHP frame until all owned locals have been released
    abi::emit_call_label(ctx.emitter, &callback);
    let done = ctx.next_label("destructor_cleanup_complete");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &done);
    super::emit_exception_activation_pop(ctx);
    abi::emit_call_label(ctx.emitter, "__rt_throw_current");
    ctx.emitter.label(&done);
}

/// Emits the shared normal/exceptional cleanup walker and its individual protected owner steps.
pub(super) fn emit_callback(ctx: &mut FunctionContext<'_>, entry: &str) {
    let callback = format!("{entry}__cdylib_exception_cleanup");
    let mut owners = super::ref_cell_owner_locals(ctx).into_iter()
        .map(|(_, _, ty, offset)| Owner::Reference(ty, offset)).collect::<Vec<_>>();
    owners.extend(super::function_cleanup_locals(ctx, None).into_iter()
        .map(|(_, slot, ty, offset)| Owner::Local(slot, ty, offset)));
    owners.extend(super::eval_scope_locals(ctx).into_iter().map(|(_, offset)| Owner::EvalScope(offset)));
    owners.extend(super::eval_context_locals(ctx).into_iter().map(|(_, offset)| Owner::EvalContext(offset)));
    ctx.emitter.label_global(&callback);
    enter_borrowed_frame(ctx.emitter);
    super::emit_instr_exit(ctx);
    elephc_memoir::frame_owners::emit_cleanup_steps(ctx.emitter, &callback, owners.len());
    leave_borrowed_frame(ctx.emitter);
    for (index, owner) in owners.iter().enumerate() {
        let step = format!("{callback}_owner_{index}");
        ctx.emitter.label(&step);
        enter_borrowed_frame(ctx.emitter);
        match owner {
            Owner::Local(slot, ty, offset) => super::emit_owned_local_cleanup(ctx, *slot, *offset, ty),
            Owner::Reference(ty, offset) => emit_reference_cleanup(ctx, ty, *offset, &step),
            Owner::EvalScope(offset) => super::emit_eval_scope_cleanup(ctx, *offset),
            Owner::EvalContext(offset) => super::emit_eval_context_cleanup(ctx, *offset),
        }
        leave_borrowed_frame(ctx.emitter);
        if let Owner::Reference(ty, _) = owner {
            if ty.codegen_repr() != PhpType::Mixed {
                emit_reference_payload(ctx.emitter, ty, &format!("{step}_payload"));
            }
        }
    }
}

/// Releases a managed reference or frees raw storage after protected payload destruction.
fn emit_reference_cleanup(ctx: &mut FunctionContext<'_>, ty: &PhpType, offset: usize, step: &str) {
    let done = ctx.next_label("destructor_reference_cleanup_done");
    let result = abi::int_result_reg(ctx.emitter);
    emit_take_reference_owner(ctx.emitter, FrameOffset::new(offset), &done);
    if ty.codegen_repr() == PhpType::Mixed {
        abi::emit_release_local_ref_cell(ctx.emitter, result, ty);
        ctx.emitter.label(&done);
        return;
    }
    let payload_entry = (matches!(ty.codegen_repr(), PhpType::Str | PhpType::Callable)
        || ty.codegen_repr().is_refcounted()).then(|| format!("{step}_payload"));
    emit_release_borrowed_raw_ref_cell(ctx.emitter, payload_entry.as_deref(), &done);
    ctx.emitter.label(&done);
}

/// Releases one borrowed raw reference cell's payload using the authoritative target-aware value ABI.
fn emit_reference_payload(emitter: &mut Emitter, ty: &PhpType, symbol: &str) {
    emit_raw_ref_payload_callback(emitter, symbol, |emitter| match ty.codegen_repr() {
        PhpType::Str => abi::emit_call_label(emitter, "__rt_heap_free_safe"),
        other => abi::emit_decref_if_refcounted(emitter, &other),
    });
}
