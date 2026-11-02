//! Purpose:
//! Emits pure suspended-frame ownership visitors from the ordinary cleanup inventory.
//!
//! Called from:
//! - Per-function exceptional cleanup emission, alongside its activation callback.
//!
//! Key details:
//! - Borrowed, moved and uninitialized owners follow the same predicates as cleanup.
//! - Visitor calls read slots without retaining values or executing PHP.
//! - Raw reference cells expose their owned payload; managed cells remain independent nodes.

use super::*;

/// Enumerates current graph owners while borrowing the original function's frame pointer.
pub(super) fn emit(ctx: &mut FunctionContext<'_>, entry: &str) {
    ctx.emitter.label_global(&format!("{entry}__gc_frame_owners"));
    destructor_cleanup::enter_borrowed_frame(ctx.emitter);
    let arm = ctx.emitter.target.arch == Arch::AArch64;
    ctx.emitter.instruction(if arm { "str x1, [sp, #8]" } else { "mov QWORD PTR [rsp + 8], rsi" }); // retain this traversal's independent native context
    for (_, slot, ty, offset) in function_cleanup_locals(ctx, None) {
        let ty = if ctx.numeric_local_slots.contains(&slot) { PhpType::Mixed } else { ty };
        if !graph_type(&ty) { continue; }
        let skip = ctx.next_label("gc_frame_raw_skip");
        if let Some(state) = ctx.ref_cell_state_offset(slot) {
            abi::load_at_offset(ctx.emitter, abi::int_result_reg(ctx.emitter), state);
            abi::emit_branch_if_int_result_nonzero(ctx.emitter, &skip);
        }
        visit(ctx, offset, 0);
        ctx.emitter.label(&skip);
    }
    for (_, _, ty, offset) in ref_cell_owner_locals(ctx) {
        visit(ctx, offset, 128 | crate::codegen_support::runtime::reference_cells::payload_tag(&ty));
    }
    destructor_cleanup::leave_borrowed_frame(ctx.emitter);
}

/// Distinguishes graph storage from scalar, string and foreign-handle representations.
fn graph_type(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. } |
        PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_) | PhpType::Callable | PhpType::Iterable)
}

/// Passes one authoritative owner-slot address and its reference normalization descriptor.
fn visit(ctx: &mut FunctionContext<'_>, offset: usize, tag: i64) {
    abi::emit_frame_slot_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), offset);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), tag);
    ctx.emitter.instruction(if ctx.emitter.target.arch == Arch::AArch64 {
        "ldr x2, [sp, #8]"
    } else { "mov rdx, QWORD PTR [rsp + 8]" });                                 // reuse the caller's slot-identity set and traversal mode
    abi::emit_call_label(ctx.emitter, "__rt_gc_frame_owner_slot");
}
