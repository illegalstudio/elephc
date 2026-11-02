//! Purpose:
//! Handles eval bridge statuses, thrown values, and fatal diagnostics.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Diagnostics and process exit remain target-aware and ABI-stable.

use super::*;

/// Preserves an escaping eval Throwable while allowing committed scope changes to be reloaded.
pub(super) fn prepare_eval_scope_reload(ctx: &mut FunctionContext<'_>) {
    let thrown = ctx.next_label("eval_reload_captured_throwable");
    let ready = ctx.next_label("eval_reload_scope_ready");
    let result = abi::int_result_reg(ctx.emitter);
    let scratch = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, scratch, 0);
    abi::emit_store_to_sp(ctx.emitter, scratch, EVAL_RELOAD_THROWABLE_OFFSET);
    emit_branch_if_eval_status(ctx, EVAL_STATUS_UNCAUGHT_THROWABLE, &thrown);
    emit_eval_status_check(ctx);
    abi::emit_jump(ctx.emitter, &ready);
    ctx.emitter.label(&thrown);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, EVAL_RESULT_ERROR_OFFSET);
    abi::emit_call_label(ctx.emitter, "__rt_throwable_take_boxed");
    abi::emit_store_to_sp(ctx.emitter, result, EVAL_RELOAD_THROWABLE_OFFSET);
    abi::emit_load_int_immediate(ctx.emitter, scratch, 0);
    abi::emit_store_to_sp(ctx.emitter, scratch, EVAL_RESULT_ERROR_OFFSET);
    ctx.emitter.label(&ready);
}

/// Roots both outcome owners if scope conversion itself throws before writeback can finish.
pub(super) fn protect_eval_reload_owners(ctx: &mut FunctionContext<'_>) {
    let address = abi::tertiary_scratch_reg(ctx.emitter);
    for (owner, record) in [
        (EVAL_RELOAD_THROWABLE_OFFSET, EVAL_RELOAD_THROW_OWNER_OFFSET),
        (EVAL_TEMP_CELL_OFFSET, EVAL_RELOAD_VALUE_OWNER_OFFSET),
    ] {
        abi::emit_temporary_stack_address(ctx.emitter, address, owner);
        abi::emit_link_call_operand_owner_at_stack(ctx.emitter, address, false, record);
    }
}

/// Propagates an accumulated Throwable only after all synchronized locals and globals are current.
pub(super) fn finish_eval_scope_reload(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.comment("finish guarded eval scope writeback");
    abi::emit_unlink_call_operand_owner_at_stack(ctx.emitter, EVAL_RELOAD_VALUE_OWNER_OFFSET);
    abi::emit_unlink_call_operand_owner_at_stack(ctx.emitter, EVAL_RELOAD_THROW_OWNER_OFFSET);
    let done = ctx.next_label("eval_reload_without_throwable");
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, EVAL_RELOAD_THROWABLE_OFFSET);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &done);
    // Cleanup can replace a successful eval return, so retire that otherwise-unobserved owner too.
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, EVAL_TEMP_CELL_OFFSET);
    scope_reload::retire_eval_replacement_owner(
        ctx.emitter, &PhpType::Mixed, Some(EVAL_RELOAD_THROWABLE_OFFSET),
    );
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, EVAL_RELOAD_THROWABLE_OFFSET);
    abi::emit_store_reg_to_symbol(ctx.emitter, result, "_exc_value", 0);
    abi::emit_release_temporary_stack(ctx.emitter, EVAL_BRIDGE_STACK_BYTES);
    abi::emit_call_label(ctx.emitter, "__rt_throw_current");
    ctx.emitter.label(&done);
}

/// Dispatches a non-zero eval bridge status to its throw or fatal handler.
///
/// The pending Throwable is passed in ARGUMENT REGISTER 1 (`x1` / `rsi`): it
/// lives in the caller's eval scratch frame, which the helper cannot address,
/// because inside the helper `sp` belongs to the helper.
pub(super) fn emit_eval_status_check(ctx: &mut FunctionContext<'_>) {
    let throwable_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_temporary_stack_slot(ctx.emitter, throwable_arg, EVAL_RESULT_ERROR_OFFSET);
    let label = crate::codegen::eval_status_helper::EVAL_STATUS_LABEL;
    abi::emit_call_label(ctx.emitter, label);
}

/// Raises the Throwable passed as the helper's first argument.
fn emit_shared_eval_throw_current(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let throwable_arg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_reg_move(ctx.emitter, result_reg, throwable_arg);
    abi::emit_call_label(ctx.emitter, "__rt_throwable_take_boxed");
    abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, "_exc_value", 0);
    abi::emit_call_label(ctx.emitter, "__rt_throw_current");
}

/// Emits the body of the shared eval status dispatch.
///
/// The status arrives in the result register, exactly where the inline check
/// expected it, and the pending Throwable arrives in the first argument
/// register. The helper never returns for a non-zero status: every branch
/// either throws or exits.
pub(in crate::codegen) fn emit_eval_status_helper_body(ctx: &mut FunctionContext<'_>) {
    let ok_label = ctx.next_label("eval_status_ok");
    let parse_error_label = ctx.next_label("eval_status_parse_error");
    let throwable_label = ctx.next_label("eval_status_throwable");
    let unsupported_label = ctx.next_label("eval_status_unsupported");
    let escaping_pcntl_label = ctx.next_label("eval_status_escaping_pcntl_callable");
    let user_fatal_label = ctx.next_label("eval_status_user_fatal");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &ok_label);
    emit_branch_if_eval_status(ctx, EVAL_STATUS_PARSE_ERROR, &parse_error_label);
    emit_branch_if_eval_status(ctx, EVAL_STATUS_UNCAUGHT_THROWABLE, &throwable_label);
    emit_branch_if_eval_status(ctx, EVAL_STATUS_UNSUPPORTED, &unsupported_label);
    emit_branch_if_eval_status(
        ctx,
        EVAL_STATUS_ESCAPING_PCNTL_CALLABLE,
        &escaping_pcntl_label,
    );
    emit_branch_if_eval_status(ctx, EVAL_STATUS_USER_FATAL, &user_fatal_label);
    emit_eval_fatal_message(ctx, EVAL_RUNTIME_FATAL_MESSAGE);
    ctx.emitter.label(&parse_error_label);
    emit_eval_fatal_message(ctx, EVAL_PARSE_ERROR_MESSAGE);
    ctx.emitter.label(&throwable_label);
    emit_shared_eval_throw_current(ctx);
    ctx.emitter.label(&unsupported_label);
    emit_eval_fatal_message(ctx, EVAL_UNSUPPORTED_MESSAGE);
    ctx.emitter.label(&escaping_pcntl_label);
    emit_eval_fatal_message(ctx, EVAL_ESCAPING_PCNTL_CALLABLE_MESSAGE);
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
