//! Purpose:
//! Lowers finally execution for pending returns, branches, and rethrows.
//! Participates in the exception-control pipeline around protected statement bodies.
//!
//! Called from:
//! - `crate::codegen::stmt::control_flow::exceptions`
//!
//! Key details:
//! - Pending control-flow state must survive handler transitions and be replayed after finally blocks.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

use super::{PENDING_BRANCH, PENDING_RETURN};

/// Records a pending branch (break/continue) and transfers control to the
/// innermost active finally block. The target label is stored in the pending
/// target slot so that finally dispatch can resume branching after the finally
/// body executes. Panics if no finally is currently active.
pub(crate) fn emit_branch_through_finally(emitter: &mut Emitter, ctx: &Context, target_label: &str) {
    emit_set_pending_action(emitter, ctx, PENDING_BRANCH, Some(target_label), false);
    let finally_entry = &ctx
        .finally_stack
        .last()
        .expect("codegen bug: pending branch requested without an active finally")
        .entry_label;
    abi::emit_jump(emitter, finally_entry);                                        // transfer control to the innermost finally before branching onward
}

/// Records a pending return and transfers control to the innermost active
/// finally block. The return value is spilled to the pending return slot before
/// branching to finally so that the value survives the handler transition.
/// Panics if no finally is currently active.
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

/// Stores the pending control-flow action (return, branch, or rethrow) and an
/// optional target label into the try handler's spill slots. If `preserve_return`
/// is true the current return value is also spilled to the pending return slot.
/// The action constant must be one of `PENDING_RETURN`, `PENDING_BRANCH`, or
/// `PENDING_RETHROW`. Panics if the pending-action or pending-target slots are
/// not allocated.
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

/// Emits the finally dispatch logic that runs after the finally body completes.
/// Reads the pending action from the spill slot; if zero the control flows to
/// `normal_resume_label`. If an outer finally exists, it chains to that first.
/// Otherwise the action is compared against 1 (return), 2 (branch), and 3 (rethrow),
/// dispatching to the corresponding label. Clears the pending action before
/// resuming a return or branch. Re-throws for action 3. Panics if the
/// pending-action or pending-target slots are not allocated.
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

    emitter.instruction(&format!("cmp {}, 1", result_reg));                     // is the pending action a return?
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b.eq {}", dispatch_return)), // restore the return value then continue to the return target
        Arch::X86_64 => emitter.instruction(&format!("je {}", dispatch_return)), // restore the return value then continue to the return target
    }
    emitter.instruction(&format!("cmp {}, 2", result_reg));                     // is the pending action a branch (break/continue)?
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b.eq {}", dispatch_branch)), // jump to the recorded target after finally
        Arch::X86_64 => emitter.instruction(&format!("je {}", dispatch_branch)), // jump to the recorded target after finally
    }
    emitter.instruction(&format!("cmp {}, 3", result_reg));                     // is the pending action an exception rethrow?
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b.eq {}", dispatch_rethrow)), // resume propagating the current exception after finally
        Arch::X86_64 => emitter.instruction(&format!("je {}", dispatch_rethrow)), // resume propagating the current exception after finally
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

/// Restores the pending return value from the spill slot back into the
/// appropriate return register, using `ctx.return_type` to determine the slot
/// width and encoding. Panics if the pending return spill slot is not allocated.
fn restore_pending_return_value(emitter: &mut Emitter, ctx: &Context) {
    let return_offset = ctx
        .pending_return_value_offset
        .expect("codegen bug: missing pending return spill slot");
    abi::emit_restore_return_value(emitter, &ctx.return_type, return_offset);
}

/// Emits the address of a local label into `dest_reg` using the target ABI's
/// symbol-address mechanism. The address is used as a pending target for
/// branch dispatch after a finally block.
fn emit_label_address(emitter: &mut Emitter, label: &str, dest_reg: &str) {
    abi::emit_symbol_address(emitter, dest_reg, label);                            // materialize the local control-flow target label address for pending dispatch
}

/// Emits an indirect branch (`br` on ARM64, `jmp` on x86_64) through `reg`,
/// used to resume branching to a finally target after the finally body completes.
fn emit_indirect_jump(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("br {}", reg)),           // branch indirectly through the restored finally target register
        Arch::X86_64 => emitter.instruction(&format!("jmp {}", reg)),           // branch indirectly through the restored finally target register
    }
}
