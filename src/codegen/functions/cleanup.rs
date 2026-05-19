//! Purpose:
//! Emits function-scope cleanup for owned locals and structured exit paths.
//! Balances refcounted values before normal returns and exceptional control transfers leave a frame.
//!
//! Called from:
//! - `crate::codegen::functions` and return/throw statement lowering
//!
//! Key details:
//! - Cleanup must follow ownership metadata and avoid releasing borrowed aliases or persistent values.

use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::types::PhpType;

use super::super::abi;

pub(super) fn preserve_return_registers(emitter: &mut Emitter, ctx: &Context, return_ty: &PhpType) {
    let return_offset = ctx
        .pending_return_value_offset
        .expect("codegen bug: missing pending return spill slot");
    super::super::abi::emit_preserve_return_value(emitter, return_ty, return_offset);
}

pub(super) fn restore_return_registers(emitter: &mut Emitter, ctx: &Context, return_ty: &PhpType) {
    let return_offset = ctx
        .pending_return_value_offset
        .expect("codegen bug: missing pending return spill slot");
    super::super::abi::emit_restore_return_value(emitter, return_ty, return_offset);
}

pub(super) fn epilogue_has_side_effects(ctx: &Context) -> bool {
    !ctx.static_vars.is_empty()
        || !ctx.local_ref_cell_flags.is_empty()
        || ctx.variables.iter().any(|(name, var)| {
            !ctx.global_vars.contains(name)
                && !ctx.static_vars.contains(name)
                && !ctx.ref_params.contains(name)
                && var.epilogue_cleanup_safe
                && var.ownership == HeapOwnership::Owned
                && (matches!(var.ty, PhpType::Str) || var.ty.is_refcounted())
        })
}

pub(crate) fn emit_local_ref_cell_flag_zero_init(emitter: &mut Emitter, ctx: &Context) {
    let mut offsets: Vec<_> = ctx
        .local_ref_cell_flags
        .values()
        .map(|flag| flag.offset)
        .collect();
    offsets.sort_unstable();
    for offset in offsets {
        abi::emit_store_zero_to_local_slot(emitter, offset);                   // clear the owned local reference-cell flag at function entry
    }
}

pub(crate) fn emit_owned_local_epilogue_cleanup(
    emitter: &mut Emitter,
    ctx: &Context,
    label_scope: &str,
) {
    let mut cleanup_vars: Vec<_> = ctx
        .variables
        .iter()
        .filter(|(name, var)| {
            !ctx.global_vars.contains(*name)
                && !ctx.static_vars.contains(*name)
                && !ctx.ref_params.contains(*name)
                && var.epilogue_cleanup_safe
                && var.ownership == HeapOwnership::Owned
        })
        .collect();
    cleanup_vars.sort_by_key(|(_, var)| var.stack_offset);

    for (name, var) in cleanup_vars {
        match &var.ty {
            PhpType::Str => {
                if emitter.target.arch == Arch::X86_64 {
                    continue;
                }
                emitter.comment(&format!("epilogue cleanup ${}", name));
                super::super::abi::load_at_offset(
                    emitter,
                    super::super::abi::int_result_reg(emitter),
                    var.stack_offset,
                ); // load owned string pointer from the local slot into the target integer result register
                super::super::abi::emit_call_label(emitter, "__rt_heap_free_safe"); // release owned string storage before returning
            }
            ty if ty.is_refcounted() => {
                emitter.comment(&format!("epilogue cleanup ${}", name));
                super::super::abi::load_at_offset(
                    emitter,
                    super::super::abi::int_result_reg(emitter),
                    var.stack_offset,
                ); // load owned heap pointer from the local slot into the target integer result register
                super::super::abi::emit_decref_if_refcounted(emitter, ty);
            }
            _ => {}
        }
    }
    emit_local_ref_cell_epilogue_cleanup(emitter, ctx, label_scope);
}

fn emit_local_ref_cell_epilogue_cleanup(
    emitter: &mut Emitter,
    ctx: &Context,
    label_scope: &str,
) {
    let mut cleanup_cells: Vec<_> = ctx
        .local_ref_cell_flags
        .values()
        .filter_map(|flag| {
            ctx.variables
                .get(&flag.variable)
                .map(|var| {
                    (
                        flag.variable.as_str(),
                        flag.offset,
                        var.stack_offset,
                        flag.value_ty.clone().unwrap_or_else(|| var.ty.clone()),
                    )
                })
        })
        .collect();
    cleanup_cells.sort_by_key(|(_, flag_offset, _, _)| *flag_offset);

    for (idx, (name, flag_offset, slot_offset, value_ty)) in cleanup_cells.into_iter().enumerate()
    {
        let done = format!("{}_local_ref_cell_cleanup_done_{}", label_scope, idx);
        emitter.comment(&format!("epilogue cleanup local ref cell ${}", name));
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::load_at_offset_scratch(emitter, "x10", flag_offset, "x11");
                emitter.instruction(&format!("cbz x10, {}", done));             // skip cleanup when the reference variable is bound to borrowed storage
                abi::load_at_offset_scratch(emitter, "x9", slot_offset, "x11");
                abi::emit_release_local_ref_cell(emitter, "x9", &value_ty);
                abi::emit_store_zero_to_local_slot(emitter, flag_offset);       // mark the owned reference cell as released
            }
            Arch::X86_64 => {
                abi::load_at_offset_scratch(emitter, "r10", flag_offset, "r11");
                emitter.instruction(&format!("test r10, r10"));                 // check whether this reference variable owns a local cell
                emitter.instruction(&format!("je {}", done));                   // skip cleanup when the reference variable is bound to borrowed storage
                abi::load_at_offset_scratch(emitter, "r11", slot_offset, "r10");
                abi::emit_release_local_ref_cell(emitter, "r11", &value_ty);
                abi::emit_store_zero_to_local_slot(emitter, flag_offset);       // mark the owned reference cell as released
            }
        }
        emitter.label(&done);
    }
}

pub(super) fn emit_activation_record_push(emitter: &mut Emitter, ctx: &Context, cleanup_label: &str) {
    let prev_offset = ctx
        .activation_prev_offset
        .expect("codegen bug: missing activation prev slot");
    let cleanup_offset = ctx
        .activation_cleanup_offset
        .expect("codegen bug: missing activation cleanup slot");
    let frame_base_offset = ctx
        .activation_frame_base_offset
        .expect("codegen bug: missing activation frame-base slot");

    emitter.comment("register exception cleanup frame");
    let scratch = super::super::abi::temp_int_reg(emitter.target);
    super::super::abi::emit_load_symbol_to_reg(emitter, scratch, "_exc_call_frame_top", 0);
    super::super::abi::store_at_offset(emitter, scratch, prev_offset);                 // save the previous call-frame pointer in this frame record
    super::super::abi::emit_symbol_address(emitter, scratch, cleanup_label);
    super::super::abi::store_at_offset(emitter, scratch, cleanup_offset);              // save the cleanup callback address in this frame record
    super::super::abi::emit_copy_frame_pointer(emitter, scratch);
    super::super::abi::store_at_offset(emitter, scratch, frame_base_offset);           // save the current frame pointer in this frame record
    super::super::abi::emit_store_zero_to_local_slot(
        emitter,
        ctx.pending_action_offset
            .expect("codegen bug: missing pending-action slot"),
    ); // clear pending finally action for this activation
    super::super::abi::emit_frame_slot_address(emitter, scratch, prev_offset);         // compute the address of this activation record's first slot
    super::super::abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
}

pub(super) fn emit_activation_record_pop(emitter: &mut Emitter, ctx: &Context) {
    let prev_offset = ctx
        .activation_prev_offset
        .expect("codegen bug: missing activation prev slot");

    emitter.comment("unregister exception cleanup frame");
    let scratch = super::super::abi::temp_int_reg(emitter.target);
    super::super::abi::load_at_offset(emitter, scratch, prev_offset);                  // reload the previous call-frame pointer from this activation
    super::super::abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
}

pub(super) fn emit_frame_cleanup_callback(emitter: &mut Emitter, ctx: &Context, cleanup_label: &str) {
    emitter.label(cleanup_label);
    super::super::abi::emit_cleanup_callback_prologue(
        emitter,
        super::super::abi::int_arg_reg_name(emitter.target, 0),
    );
    emit_owned_local_epilogue_cleanup(emitter, ctx, cleanup_label);
    super::super::abi::emit_cleanup_callback_epilogue(emitter);
    emitter.blank();
}
