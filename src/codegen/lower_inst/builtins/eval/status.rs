//! Purpose:
//! Handles eval bridge statuses, thrown values, and fatal diagnostics.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Diagnostics and process exit remain target-aware and ABI-stable.

use super::*;

/// Emits a fatal diagnostic when the eval bridge reports any non-zero status.
pub(super) fn emit_eval_status_check(ctx: &mut FunctionContext<'_>) {
    let ok_label = ctx.next_label("eval_status_ok");
    let parse_error_label = ctx.next_label("eval_status_parse_error");
    let throwable_label = ctx.next_label("eval_status_throwable");
    let unsupported_label = ctx.next_label("eval_status_unsupported");
    let user_fatal_label = ctx.next_label("eval_status_user_fatal");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &ok_label);
    emit_branch_if_eval_status(ctx, EVAL_STATUS_PARSE_ERROR, &parse_error_label);
    emit_branch_if_eval_status(ctx, EVAL_STATUS_UNCAUGHT_THROWABLE, &throwable_label);
    emit_branch_if_eval_status(ctx, EVAL_STATUS_UNSUPPORTED, &unsupported_label);
    emit_branch_if_eval_status(ctx, EVAL_STATUS_USER_FATAL, &user_fatal_label);
    emit_eval_fatal_message(ctx, EVAL_RUNTIME_FATAL_MESSAGE);
    ctx.emitter.label(&parse_error_label);
    emit_eval_fatal_message(ctx, EVAL_PARSE_ERROR_MESSAGE);
    ctx.emitter.label(&throwable_label);
    emit_eval_throw_current(ctx);
    ctx.emitter.label(&unsupported_label);
    emit_eval_fatal_message(ctx, EVAL_UNSUPPORTED_MESSAGE);
    ctx.emitter.label(&user_fatal_label);
    abi::emit_exit(ctx.emitter, 255);
    ctx.emitter.label(&ok_label);
}

/// Branches to a label when the eval bridge returned a specific status code.
pub(super) fn emit_branch_if_eval_status(ctx: &mut FunctionContext<'_>, status: i64, label: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, #{}", result_reg, status)); // compare the eval bridge status against the handled code
            ctx.emitter.instruction(&format!("b.eq {}", label));                // branch to the matching eval status handler
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("cmp {}, {}", result_reg, status)); // compare the eval bridge status against the handled code
            ctx.emitter.instruction(&format!("je {}", label));                  // branch to the matching eval status handler
        }
    }
}

/// Publishes an eval-thrown Throwable and enters the normal runtime unwinder.
pub(super) fn emit_eval_throw_current(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result_reg, EVAL_RESULT_ERROR_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let object_reg = eval_mixed_unbox_low_payload_reg(ctx);
    abi::emit_store_reg_to_symbol(ctx.emitter, object_reg, "_exc_value", 0);
    abi::emit_call_label(ctx.emitter, "__rt_throw_current");
}

/// Returns the low payload register produced by `__rt_mixed_unbox` for eval status handling.
pub(super) fn eval_mixed_unbox_low_payload_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    }
}

/// Emits an eval diagnostic message and exits the process.
pub(super) fn emit_eval_fatal_message(ctx: &mut FunctionContext<'_>, message: &str) {
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the eval runtime diagnostic to stderr
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter
                .instruction(&format!("mov x2, #{}", message_len)); // pass the eval runtime diagnostic byte length
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the eval runtime diagnostic to Linux stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter
                .instruction(&format!("mov edx, {}", message_len)); // pass the eval runtime diagnostic byte length
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the eval runtime diagnostic before exiting
            abi::emit_exit(ctx.emitter, 1);
        }
    }
}
