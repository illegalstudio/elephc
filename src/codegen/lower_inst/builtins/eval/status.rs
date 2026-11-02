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

/// [`emit_eval_status_check`] for a call that boxed operands for the bridge.
///
/// The bridge BORROWS the cells this frame boxed (receiver, arguments, the argument pack), so a
/// non-zero status -- an uncaught Throwable above all -- has to release them before it unwinds:
/// the normal path releases them right after the check, and the throwing path used to skip that,
/// leaking the receiver's reference for good. Measured: a `ReflectionClass` built from a runtime
/// class name keeps its whole member graph (18 blocks) when `getMethod()` of an unknown method
/// throws, and every later throw leaks another one.
///
/// With the module's cells helper the site only passes the cells' address and a bit mask
/// of their eight-byte slots (`_eir_shared_eval_status_fail_cells` releases them, then enters the
/// module's status helper), so the cost per site is a few instructions, not a release per cell.
pub(super) fn emit_eval_status_check_releasing(ctx: &mut FunctionContext<'_>, boxed: &[usize]) {
    if boxed.is_empty() {
        emit_eval_status_check(ctx);
        return;
    }
    let mask = boxed.iter().try_fold(0u64, |mask, offset| {
        (offset % 8 == 0 && offset / 8 < 64).then(|| mask | (1u64 << (offset / 8)))
    });
    if let (Some(mask), Some(helper)) = (mask, crate::codegen::shared_eval_status::shared_cells_label(ctx)) {
        let ok_label = ctx.next_label("eval_status_ok");
        abi::emit_branch_if_int_result_zero(ctx.emitter, &ok_label);
        let target = ctx.emitter.target;
        abi::emit_load_temporary_stack_slot(
            ctx.emitter,
            abi::int_arg_reg_name(target, 1),
            EVAL_RESULT_ERROR_OFFSET,
        );
        abi::emit_temporary_stack_address(ctx.emitter, abi::int_arg_reg_name(target, 2), 0);
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(target, 3), mask as i64);
        abi::emit_call_label(ctx.emitter, helper);
        ctx.emitter.label(&ok_label);
        return;
    }
    let ok_label = ctx.next_label("eval_status_ok_released");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &ok_label);
    // The status is parked across the releases; the error cell stays in its stack slot.
    emit_release_eval_boxed_operands_keeping_int_answer(ctx, boxed);
    emit_eval_status_check(ctx);
    ctx.emitter.label(&ok_label);
}

/// Body of `_eir_shared_eval_status_fail_cells`: status in the integer result register, the site's
/// error cell, the address of its boxed cells and their slot bit mask in the next three argument
/// registers. Releases every marked cell, then enters the ordinary failure helper; never returns.
pub(in crate::codegen) fn emit_shared_eval_status_fail_cells_body(
    ctx: &mut FunctionContext<'_>,
    fail_label: &str,
) -> Result<()> {
    let lp = ctx.next_label("eval_fail_cells_loop");
    let done = ctx.next_label("eval_fail_cells_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #48");                         // mask, base, status, error cell
            ctx.emitter.instruction("str x3, [sp]");                            // remaining cells
            ctx.emitter.instruction("str x2, [sp, #8]");                        // address of slot 0
            ctx.emitter.instruction("str x0, [sp, #16]");                       // the status
            ctx.emitter.instruction("str x1, [sp, #24]");                       // the site's error cell
            ctx.emitter.label(&lp);
            ctx.emitter.instruction("ldr x3, [sp]");                            // remaining cells
            ctx.emitter.instruction(&format!("cbz x3, {done}"));                // all released
            ctx.emitter.instruction("rbit x4, x3");                             // lowest marked slot, found from the other end
            ctx.emitter.instruction("clz x4, x4");                              // its index
            ctx.emitter.instruction("sub x5, x3, #1");                          // clear that bit
            ctx.emitter.instruction("and x3, x3, x5");                          // remaining cells without it
            ctx.emitter.instruction("str x3, [sp]");                            // keep them across the release
            ctx.emitter.instruction("ldr x9, [sp, #8]");                        // address of slot 0
            ctx.emitter.instruction("ldr x0, [x9, x4, lsl #3]");                // the boxed cell
            ctx.emitter.instruction("bl __rt_decref_mixed");                    // release what the site boxed for the bridge
            ctx.emitter.instruction(&format!("b {lp}"));                        // next cell
            ctx.emitter.label(&done);
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // the status again
            ctx.emitter.instruction("ldr x1, [sp, #24]");                       // the error cell again
            ctx.emitter.instruction("add sp, sp, #48");                         // drop the scratch slots
            ctx.emitter.instruction(&format!("bl {fail_label}"));               // the ordinary failure path; never returns
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 48");                             // mask, base, status, error cell
            ctx.emitter.instruction("mov QWORD PTR [rsp], rcx");                // remaining cells
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");            // address of slot 0
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], rax");           // the status
            ctx.emitter.instruction("mov QWORD PTR [rsp + 24], rsi");           // the site's error cell
            ctx.emitter.label(&lp);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                // remaining cells
            ctx.emitter.instruction("test rax, rax");                           // any left?
            ctx.emitter.instruction(&format!("jz {done}"));                     // all released
            ctx.emitter.instruction("bsf rcx, rax");                            // index of the lowest marked slot
            ctx.emitter.instruction("lea rdx, [rax - 1]");                      // clear that bit
            ctx.emitter.instruction("and rax, rdx");                            // remaining cells without it
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // keep them across the release
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // address of slot 0
            ctx.emitter.instruction("mov rax, QWORD PTR [rdx + rcx * 8]");      // the boxed cell
            ctx.emitter.instruction("call __rt_decref_mixed");                  // release what the site boxed for the bridge
            ctx.emitter.instruction(&format!("jmp {lp}"));                      // next cell
            ctx.emitter.label(&done);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // the status again
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");           // the error cell again
            ctx.emitter.instruction("add rsp, 48");                             // drop the scratch slots
            ctx.emitter.instruction(&format!("call {fail_label}"));             // the ordinary failure path; never returns
        }
    }
    Ok(())
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
    emit_eval_bridge_fatal_exit(ctx, EVAL_STATUS_RUNTIME_FATAL);
    ctx.emitter.label(&parse_error_label);
    emit_eval_parse_error_exit(ctx);
    ctx.emitter.label(&throwable_label);
    emit_shared_eval_throw_current(ctx);
    ctx.emitter.label(&unsupported_label);
    emit_eval_bridge_fatal_exit(ctx, EVAL_STATUS_UNSUPPORTED);
    ctx.emitter.label(&escaping_pcntl_label);
    emit_eval_bridge_fatal_exit(ctx, EVAL_STATUS_ESCAPING_PCNTL_CALLABLE);
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

/// Exits after the bridge has already printed the PHP parse diagnostic.
///
/// The message names the file, the line and the failing token, none of which a constant in the
/// generated assembly can carry, so `libelephc-magician` prints it on standard output — where
/// `php -n` 8.5.6 prints it — before returning the parse-error status. Only the exit is left
/// here, and it uses PHP's own status for a parse error.
pub(super) fn emit_eval_parse_error_exit(ctx: &mut FunctionContext<'_>) {
    abi::emit_exit(ctx.emitter, EVAL_PARSE_ERROR_EXIT_STATUS);
}

/// Reports the fatal for one anonymous status, then exits the process.
///
/// When the interpreter bridge is linked, it writes the message: it names what the interpreter
/// could not do, the file it was asked in and the line, none of which a constant in the
/// generated assembly can carry — the same reason the parse diagnostic moved to
/// `libelephc-magician`.
///
/// WHEN IT IS NOT LINKED, THIS MUST NOT CALL IT. A scope-only program — one whose eval
/// fragments were all compiled ahead of time, so `eval_scope` is set and `eval_bridge` is not —
/// runs them natively against the runtime's own `__elephc_eval_scope_*` helpers and
/// deliberately does not link `libelephc-magician`
/// (`runtime_features::link_requirements_for_runtime_features` asks for that archive only when
/// `eval_bridge` is set). Emitting the call regardless left every such program with an
/// undefined `__elephc_eval_report_runtime_fatal` at link time, from builtin-class emissions
/// that carry these arms whether or not the fragment needed the interpreter. The alternative
/// fix — making the feature scan set `eval_bridge` for these references — would link the whole
/// interpreter into programs that compiled every fragment ahead of time, which is the cost
/// scope-only AOT exists to avoid.
///
/// So without the bridge the constant is written here, exactly as it was before the diagnostic
/// moved: it is the same text the reporter itself falls back to when the interpreter recorded
/// no clause, and with no interpreter there is never a clause. Either way the exit status is
/// the one the bridge has always used.
pub(super) fn emit_eval_bridge_fatal_exit(ctx: &mut FunctionContext<'_>, status: i64) {
    if ctx.module.required_runtime_features.eval_bridge {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter
                    .instruction(&format!("mov x0, #{status}")); // pass the ABI status the bridge should describe
            }
            Arch::X86_64 => {
                ctx.emitter
                    .instruction(&format!("mov edi, {status}")); // pass the ABI status the bridge should describe
            }
        }
        let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_report_runtime_fatal");
        abi::emit_call_label(ctx.emitter, &symbol);
    } else {
        emit_eval_fatal_constant(ctx, status);
    }
    abi::emit_exit(ctx.emitter, EVAL_RUNTIME_FATAL_EXIT_STATUS);
}

/// Writes the constant wording for one anonymous status straight to stderr.
///
/// Mirrors `__elephc_eval_report_runtime_fatal`'s own fallback, including its silence: only the
/// two anonymous fatals have wording, and any other status writes nothing, because a parse
/// error has already printed its own diagnostic and an uncaught Throwable is reported by the
/// runtime unwinder. The caller emits the exit, so this returns exactly as the reporter does.
fn emit_eval_fatal_constant(ctx: &mut FunctionContext<'_>, status: i64) {
    let message = if status == EVAL_STATUS_RUNTIME_FATAL {
        EVAL_RUNTIME_FATAL_MESSAGE
    } else if status == EVAL_STATUS_UNSUPPORTED {
        EVAL_UNSUPPORTED_MESSAGE
    } else if status == EVAL_STATUS_ESCAPING_PCNTL_CALLABLE {
        EVAL_ESCAPING_PCNTL_CALLABLE_MESSAGE
    } else {
        return;
    };
    emit_eval_fatal_message(ctx, message);
}

/// Emits a write of one constant eval diagnostic to stderr, without exiting.
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
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the eval runtime diagnostic to Linux stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter
                .instruction(&format!("mov edx, {}", message_len)); // pass the eval runtime diagnostic byte length
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the eval runtime diagnostic before exiting
        }
    }
}
