use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::emit::Emitter;
use crate::types::PhpType;

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
        || ctx.variables.iter().any(|(name, var)| {
            !ctx.global_vars.contains(name)
                && !ctx.static_vars.contains(name)
                && !ctx.ref_params.contains(name)
                && var.epilogue_cleanup_safe
                && var.ownership == HeapOwnership::Owned
                && (matches!(var.ty, PhpType::Str) || var.ty.is_refcounted())
        })
}

pub(crate) fn emit_owned_local_epilogue_cleanup(emitter: &mut Emitter, ctx: &Context) {
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
        super::super::abi::int_result_reg(emitter),
    );
    emit_owned_local_epilogue_cleanup(emitter, ctx);
    super::super::abi::emit_cleanup_callback_epilogue(emitter);
    emitter.blank();
}
