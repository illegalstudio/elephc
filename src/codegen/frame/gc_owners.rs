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
    for (_, offset) in eval_scope_locals(ctx) {
        let skip = ctx.next_label("gc_frame_eval_scope_skip");
        let scope = abi::int_arg_reg_name(ctx.emitter.target, 0);
        abi::load_at_offset(ctx.emitter, scope, offset);
        if scope != abi::int_result_reg(ctx.emitter) {
            abi::emit_reg_move(ctx.emitter, abi::int_result_reg(ctx.emitter), scope);
        }
        abi::emit_branch_if_int_result_zero(ctx.emitter, &skip);
        abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1),
            "__rt_gc_frame_owner_slot");
        ctx.emitter.instruction(if arm { "ldr x2, [sp, #8]" } else { "mov rdx, QWORD PTR [rsp + 8]" }); // retain this saved-frame traversal's slot identities and mode
        let entry = ctx.emitter.target.extern_symbol("__elephc_eval_scope_visit_owners_v1");
        abi::emit_call_label(ctx.emitter, &entry);
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


#[cfg(test)]
mod tests {
    use crate::ir::{IrType, LocalKind};
    use crate::types::PhpType;
    use crate::codegen_support::platform::Target;

    /// Every saved native frame can enumerate its owned Rust eval scope without treating it as PHP.
    #[test]
    fn suspended_frame_visitors_include_eval_scope_owners_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut module = crate::ir_lower::tests::lower_source_at_for_target(
                "<?php function traceFrozenScope(): void { echo 1; } traceFrozenScope();",
                std::path::Path::new("main.php"), std::path::Path::new("."), target,
            );
            module.required_runtime_features.eval_bridge = true;
            module.required_runtime_features.fiber = true;
            let function = module.functions.iter_mut().find(|function|
                function.name == "traceFrozenScope").unwrap();
            function.add_local(Some("__frozen_scope".into()),
                IrType::I64, PhpType::Int, LocalKind::EvalScope);
            crate::ir::validate_module(&module).unwrap();
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            let entry = target.extern_symbol("__elephc_eval_scope_visit_owners_v1");
            assert!(assembly.contains(&entry), "{name}: expose foreign-owned PHP cells to the saved-frame graph");
            assert!(assembly.contains("gc_frame_eval_scope_skip"), "{name}: uninitialized scopes own nothing");
            assert!(assembly.contains("__rt_gc_frame_owner_slot"), "{name}: retain exact owner-slot identity");
        }
    }
}
