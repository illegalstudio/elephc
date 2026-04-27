use crate::codegen::abi;
use crate::codegen::context::{Context, TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET};
use crate::codegen::emit::Emitter;
use crate::parser::ast::CatchClause;
use crate::types::PhpType;

pub(super) fn emit_try_handler_push(emitter: &mut Emitter, ctx: &Context, handler_offset: usize) {
    let activation_prev_offset = ctx
        .activation_prev_offset
        .expect("codegen bug: missing activation prev slot");
    let scratch = abi::temp_int_reg(emitter.target);

    emitter.comment("push exception handler");
    abi::emit_load_symbol_to_reg(emitter, scratch, "_exc_handler_top", 0);
    abi::store_at_offset(emitter, scratch, handler_offset);                         // save the previous handler pointer in this try slot
    abi::emit_frame_slot_address(emitter, scratch, activation_prev_offset);         // compute the address of the current activation record
    abi::store_at_offset(emitter, scratch, handler_offset - 8);                     // remember which activation frame should survive this catch
    abi::emit_load_symbol_to_reg(emitter, scratch, "_rt_diag_suppression", 0);
    abi::store_at_offset(emitter, scratch, handler_offset - TRY_HANDLER_DIAG_DEPTH_OFFSET); // snapshot runtime warning-suppression depth for exception unwinds
    abi::emit_frame_slot_address(emitter, scratch, handler_offset);                 // compute the address of this try slot's handler header
    abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_handler_top", 0);
}

pub(super) fn emit_try_handler_pop(emitter: &mut Emitter, handler_offset: usize) {
    let scratch = abi::temp_int_reg(emitter.target);
    emitter.comment("pop exception handler");
    abi::load_at_offset(emitter, scratch, handler_offset);                          // reload the previous handler pointer from this try slot
    abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_handler_top", 0);
}

pub(super) fn emit_handler_jmpbuf_address(emitter: &mut Emitter, handler_offset: usize, dest_reg: &str) {
    abi::emit_frame_slot_address(emitter, dest_reg, handler_offset - TRY_HANDLER_JMP_BUF_OFFSET); // compute the jmp_buf base address inside this try slot
}

pub(super) fn emit_restore_diagnostic_suppression(
    emitter: &mut Emitter,
    handler_offset: usize,
) {
    let scratch = abi::temp_int_reg(emitter.target);
    abi::load_at_offset(
        emitter,
        scratch,
        handler_offset - TRY_HANDLER_DIAG_DEPTH_OFFSET,
    ); // reload the suppression depth captured before entering the try body
    abi::emit_store_reg_to_symbol(emitter, scratch, "_rt_diag_suppression", 0);
}

pub(super) fn bind_catch_variable(catch_clause: &CatchClause, emitter: &mut Emitter, ctx: &Context) {
    let Some(variable) = &catch_clause.variable else {
        return;
    };
    let var = ctx
        .variables
        .get(variable)
        .expect("codegen bug: catch variable was not pre-allocated");

    emitter.comment(&format!("bind catch ${}", variable));
    if matches!(var.ty, PhpType::Str) {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), var.stack_offset); // load the previous string pointer before overwriting the catch variable
        abi::emit_call_label(emitter, "__rt_heap_free_safe");                       // release the previous owned string value in the catch slot
    } else if var.ty.is_refcounted() {
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), var.stack_offset); // load the previous heap-backed catch-slot value before overwriting it
        abi::emit_decref_if_refcounted(emitter, &var.ty);                           // release the previous owned heap value in the catch slot
    }
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_store(emitter, &var.ty, var.stack_offset);                            // move the caught exception into the catch variable slot
}

pub(super) fn resolve_catch_match_target(ctx: &Context, raw_name: &str) -> (u64, u64) {
    let resolved_name = match raw_name {
        "self" => ctx.current_class.as_deref().unwrap_or(raw_name),
        "parent" => ctx
            .current_class
            .as_ref()
            .and_then(|class_name| ctx.classes.get(class_name))
            .and_then(|class_info| class_info.parent.as_deref())
            .unwrap_or(raw_name),
        _ => raw_name,
    };
    if let Some(class_info) = ctx.classes.get(resolved_name) {
        (class_info.class_id, 0)
    } else if let Some(interface_info) = ctx.interfaces.get(resolved_name) {
        (interface_info.interface_id, 1)
    } else {
        panic!(
            "codegen bug: unresolved catch target after type checking: {}",
            resolved_name
        )
    }
}
