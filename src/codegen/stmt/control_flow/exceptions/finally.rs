use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

use super::{PENDING_BRANCH, PENDING_RETURN};

pub(crate) fn emit_branch_through_finally(emitter: &mut Emitter, ctx: &Context, target_label: &str) {
    emit_set_pending_action(emitter, ctx, PENDING_BRANCH, Some(target_label), false);
    let finally_entry = &ctx
        .finally_stack
        .last()
        .expect("codegen bug: pending branch requested without an active finally")
        .entry_label;
    abi::emit_jump(emitter, finally_entry);                                        // transfer control to the innermost finally before branching onward
}

pub(crate) fn emit_return_through_finally(emitter: &mut Emitter, ctx: &Context) {
    emit_set_pending_action(
        emitter,
        ctx,
        PENDING_RETURN,
        ctx.return_label.as_deref(),
        true,
    );
    let finally_entry = &ctx
        .finally_stack
        .last()
        .expect("codegen bug: pending return requested without an active finally")
        .entry_label;
    abi::emit_jump(emitter, finally_entry);                                        // transfer control to the innermost finally before returning
}

pub(super) fn emit_set_pending_action(
    emitter: &mut Emitter,
    ctx: &Context,
    action: u64,
    target_label: Option<&str>,
    preserve_return: bool,
) {
    let action_offset = ctx
        .pending_action_offset
        .expect("codegen bug: missing pending-action slot");
    let target_offset = ctx
        .pending_target_offset
        .expect("codegen bug: missing pending-target slot");
    let scratch = abi::temp_int_reg(emitter.target);

    abi::emit_load_int_immediate(emitter, scratch, action as i64);                 // materialize the pending control-flow action kind
    abi::store_at_offset(emitter, scratch, action_offset);                         // record the pending action kind for finally dispatch
    if let Some(label) = target_label {
        emit_label_address(emitter, label, scratch);
        abi::store_at_offset(emitter, scratch, target_offset);                     // record the post-finally branch target address
    }
    if preserve_return {
        let return_offset = ctx
            .pending_return_value_offset
            .expect("codegen bug: missing pending return spill slot");
        abi::emit_preserve_return_value(emitter, &ctx.return_type, return_offset);
    }
}

pub(super) fn emit_finally_dispatch(
    emitter: &mut Emitter,
    ctx: &Context,
    normal_resume_label: &str,
    dispatch_return: &str,
    dispatch_branch: &str,
    dispatch_rethrow: &str,
) {
    let action_offset = ctx
        .pending_action_offset
        .expect("codegen bug: missing pending-action slot");
    let target_offset = ctx
        .pending_target_offset
        .expect("codegen bug: missing pending-target slot");
    let parent_finally = ctx.finally_stack.last().map(|info| info.entry_label.clone());
    let result_reg = abi::int_result_reg(emitter);
    let target_reg = abi::temp_int_reg(emitter.target);

    abi::load_at_offset(emitter, result_reg, action_offset);                       // reload the pending control-flow action after finally body execution
    abi::emit_branch_if_int_result_zero(emitter, normal_resume_label);             // ordinary finally completion falls through to the local continuation
    if let Some(parent_label) = parent_finally {
        abi::emit_jump(emitter, &parent_label);                                    // outer finally blocks must observe the same pending action first
        return;
    }

    emitter.instruction(&format!("cmp {}, 1", result_reg));                        // is the pending action a return?
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b.eq {}", dispatch_return)), // restore the return value then continue to the return target
        Arch::X86_64 => emitter.instruction(&format!("je {}", dispatch_return)),   // restore the return value then continue to the return target
    }
    emitter.instruction(&format!("cmp {}, 2", result_reg));                        // is the pending action a branch (break/continue)?
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b.eq {}", dispatch_branch)), // jump to the recorded target after finally
        Arch::X86_64 => emitter.instruction(&format!("je {}", dispatch_branch)),   // jump to the recorded target after finally
    }
    emitter.instruction(&format!("cmp {}, 3", result_reg));                        // is the pending action an exception rethrow?
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b.eq {}", dispatch_rethrow)), // resume propagating the current exception after finally
        Arch::X86_64 => emitter.instruction(&format!("je {}", dispatch_rethrow)),   // resume propagating the current exception after finally
    }
    abi::emit_jump(emitter, normal_resume_label);                                  // unknown action kinds fall back to ordinary completion

    emitter.label(dispatch_return);
    restore_pending_return_value(emitter, ctx);
    abi::emit_store_zero_to_local_slot(emitter, action_offset);                    // clear the pending action before leaving finally
    abi::load_at_offset(emitter, target_reg, target_offset);                       // reload the recorded return target address
    emit_indirect_jump(emitter, target_reg);                                       // branch indirectly to the function epilogue

    emitter.label(dispatch_branch);
    abi::emit_store_zero_to_local_slot(emitter, action_offset);                    // clear the pending action before leaving finally
    abi::load_at_offset(emitter, target_reg, target_offset);                       // reload the recorded branch target address
    emit_indirect_jump(emitter, target_reg);                                       // branch indirectly to the loop target after finally

    emitter.label(dispatch_rethrow);
    abi::emit_store_zero_to_local_slot(emitter, action_offset);                    // clear the pending action before resuming exception propagation
    abi::emit_call_label(emitter, "__rt_rethrow_current");                         // keep unwinding the current exception after finally
}

fn restore_pending_return_value(emitter: &mut Emitter, ctx: &Context) {
    let return_offset = ctx
        .pending_return_value_offset
        .expect("codegen bug: missing pending return spill slot");
    abi::emit_restore_return_value(emitter, &ctx.return_type, return_offset);
}

fn emit_label_address(emitter: &mut Emitter, label: &str, dest_reg: &str) {
    abi::emit_symbol_address(emitter, dest_reg, label);                            // materialize the local control-flow target label address for pending dispatch
}

fn emit_indirect_jump(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("br {}", reg)),              // branch indirectly through the restored finally target register
        Arch::X86_64 => emitter.instruction(&format!("jmp {}", reg)),              // branch indirectly through the restored finally target register
    }
}
