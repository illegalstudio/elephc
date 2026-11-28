//! Purpose:
//! Contains unserialize callbacks beneath the interpreter's Rust stack.
//!
//! Called from:
//! - Shared eval value wrapper emission and the runtime builtin dispatcher.
//!
//! Key details:
//! - Borrowed inputs survive callbacks; only successful results transfer through result-out.
//! - Shared protection converts native throws to the existing PendingThrowable status.

use super::{abi, Arch, Emitter};
use crate::codegen_support::{runtime::exceptions::emit_protected_status, try_handlers::TRY_HANDLER_SLOT_SIZE};
use elephc_builtin_contract::RuntimeBuiltinStatus;

const ENTRY: &str = "__rt_eval_unserialize_call";

/// Emits a protected two-argument adapter around the boxed parser wrapper.
pub(super) fn emit(emitter: &mut Emitter) {
    emit_protected_status(emitter, ENTRY, body);
}

/// Publishes the owned result only after parsing and its context cleanup finish normally.
fn body(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::symbol_scratch_reg(emitter);
    let failed = "__rt_eval_unserialize_call_body_failed";
    let done = "__rt_eval_unserialize_call_body_done";
    abi::emit_load_temporary_stack_slot(emitter, scratch, TRY_HANDLER_SLOT_SIZE + 8);
    abi::emit_branch_if_zero_wide(emitter, scratch, failed);
    abi::emit_load_temporary_stack_slot(emitter,
        abi::int_arg_reg_name(emitter.target, 0), TRY_HANDLER_SLOT_SIZE);
    emitter.bl_c("__elephc_eval_value_unserialize");
    abi::emit_branch_if_zero_wide(emitter, result, failed);
    abi::emit_load_temporary_stack_slot(emitter, scratch, TRY_HANDLER_SLOT_SIZE + 8);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::emit_load_int_immediate(emitter, result, RuntimeBuiltinStatus::Success as i64);
    abi::emit_jump(emitter, done);
    emitter.label(failed);
    abi::emit_load_int_immediate(emitter, result, RuntimeBuiltinStatus::RuntimeFatal as i64);
    emitter.label(done);
}

/// Routes the unary builtin through protection while preserving the dispatcher's result slot.
pub(super) fn emit_dispatch(emitter: &mut Emitter) {
    let (label, count, args, out, unsupported, done) = match emitter.target.arch {
        Arch::AArch64 => ("__elephc_runtime_builtin_v1_unserialize", "x21", "x20", "x22",
            "__elephc_runtime_builtin_v1_unsupported", "__elephc_runtime_builtin_v1_done"),
        Arch::X86_64 => ("__elephc_runtime_builtin_v1_unserialize_x86", "r13", "r12", "r14",
            "__elephc_runtime_builtin_v1_unsupported_x86", "__elephc_runtime_builtin_v1_done_x86"),
    };
    emitter.label(label);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cmp {count}, #1")),      // require exactly one borrowed data argument
        Arch::X86_64 => emitter.instruction(&format!("cmp {count}, 1")),        // require exactly one borrowed data argument
    }
    abi::emit_branch_if_not_equal_wide(emitter, unsupported);
    abi::emit_load_from_address(emitter, abi::int_arg_reg_name(emitter.target, 0), args, 0);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 1), out);
    abi::emit_call_label(emitter, ENTRY);
    abi::emit_jump(emitter, done);
}
