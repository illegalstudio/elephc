//! Purpose:
//! Installs the native exception boundary used by userspace stream-wrapper adapters.
//! Restores the outer handler and diagnostic state before an escaped Throwable is rethrown.
//!
//! Called from:
//! - `crate::codegen::user_wrapper_adapters::emit_user_wrapper_adapter()`.
//!
//! Key details:
//! - The handler record uses the shared EIR/native callback layout.
//! - `setjmp` runs before callback temporaries are allocated so `longjmp` restores the stack.

use crate::codegen::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};
use crate::types::PhpType;

/// Returns the complete adapter frame size after reserving one native exception handler.
pub(super) fn frame_size_with_boundary(local_frame_size: usize) -> usize {
    local_frame_size + TRY_HANDLER_SLOT_SIZE + 16
}

/// Returns the frame-pointer-relative base offset of the adapter exception handler.
pub(super) fn boundary_base_offset(frame_size: usize) -> usize {
    frame_size - 16
}

/// Pushes an exception handler that branches to `escape_label` after a callback `longjmp`.
pub(super) fn emit_boundary_push(
    emitter: &mut Emitter,
    handler_base: usize,
    escape_label: &str,
) {
    let scratch = abi::temp_int_reg(emitter.target);
    let unbounded_label = format!("{escape_label}_not_installed");
    emitter.comment("push user stream-wrapper callback exception boundary");
    abi::emit_load_symbol_to_reg(emitter, scratch, "_exc_handler_top", 0);
    abi::store_at_offset(emitter, scratch, handler_base);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_exc_call_frame_top", 0);
    abi::store_at_offset(emitter, scratch, handler_base - 8);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_rt_diag_suppression", 0);
    abi::store_at_offset(
        emitter,
        scratch,
        handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET,
    );
    abi::emit_load(emitter, &PhpType::Int, handler_base);
    abi::emit_branch_if_int_result_zero(emitter, &unbounded_label);
    abi::emit_frame_slot_address(emitter, scratch, handler_base);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_handler_top", 0);
    abi::emit_frame_slot_address(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        handler_base - TRY_HANDLER_JMP_BUF_OFFSET,
    );
    emitter.bl_c("setjmp");
    abi::emit_branch_if_int_result_nonzero(emitter, escape_label);
    emitter.label(&unbounded_label);
}

/// Pops the adapter handler and restores the diagnostic depth captured before `setjmp`.
pub(super) fn emit_boundary_pop(emitter: &mut Emitter, handler_base: usize) {
    let scratch = abi::temp_int_reg(emitter.target);
    emitter.comment("pop user stream-wrapper callback exception boundary");
    abi::load_at_offset(emitter, scratch, handler_base);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_handler_top", 0);
    abi::load_at_offset(
        emitter,
        scratch,
        handler_base - TRY_HANDLER_DIAG_DEPTH_OFFSET,
    );
    abi::emit_store_reg_to_symbol(emitter, scratch, "_rt_diag_suppression", 0);
}
