//! Purpose:
//! Exposes suspended native PHP frames to Magician's combined backtrace builder.
//!
//! Called from:
//! - `super::emit_eval_bridge_runtime()` and the runtime backtrace entry hook.
//!
//! Key details:
//! - Activation offset 24 contains the same reader used by native debug_backtrace.
//! - Readers return owned Mixed frame hashes and honor the original options mask.

use super::*;

/// Walks visible native activations and invokes the indexed reader through its C ABI.
pub(super) fn emit_backtrace_entry_wrapper(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_backtrace_entry");
    let arm = emitter.target.arch == Arch::AArch64;
    let cursor = if arm { "x9" } else { "r10" };
    let reader = if arm { "x10" } else { "r11" };
    let index = abi::int_arg_reg_name(emitter.target, 0);
    let options = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_frame_prologue(emitter, 32);
    abi::emit_store_to_sp(emitter, options, 0);
    abi::emit_load_symbol_to_reg(emitter, cursor, "_exc_call_frame_top", 0);
    emitter.label("__elephc_eval_backtrace_entry_next");
    if arm {
        emitter.instruction("cbz x9, __elephc_eval_backtrace_entry_empty");     // stop after the outermost activation
    } else {
        emitter.instruction("test r10, r10");                                   // test whether another activation exists
        emitter.instruction("jz __elephc_eval_backtrace_entry_empty");          // stop after the outermost activation
    }
    abi::emit_load_from_address(emitter, reader, cursor, 24);
    if arm {
        emitter.instruction("cbz x10, __elephc_eval_backtrace_entry_skip");     // skip non-PHP cleanup activations
        emitter.instruction("cbz x0, __elephc_eval_backtrace_entry_read");      // found the requested visible frame
        emitter.instruction("sub x0, x0, #1");                                  // count only PHP-visible readers
    } else {
        emitter.instruction("test r11, r11");                                   // non-PHP cleanup activations have no reader
        emitter.instruction("jz __elephc_eval_backtrace_entry_skip");           // skip invisible activations
        emitter.instruction("test rdi, rdi");                                   // check whether this is the indexed visible frame
        emitter.instruction("jz __elephc_eval_backtrace_entry_read");           // materialize the selected frame
        emitter.instruction("sub rdi, 1");                                      // count only PHP-visible readers
    }
    emitter.label("__elephc_eval_backtrace_entry_skip");
    abi::emit_load_from_address(emitter, cursor, cursor, 0);
    abi::emit_jump(emitter, "__elephc_eval_backtrace_entry_next");
    emitter.label("__elephc_eval_backtrace_entry_read");
    abi::emit_reg_move(emitter, index, cursor);
    abi::emit_load_temporary_stack_slot(emitter, options, 0);
    let mode = abi::int_arg_reg_name(emitter.target, 2);
    abi::emit_load_int_immediate(emitter, mode, -1);
    abi::emit_call_reg(emitter, reader);
    abi::emit_jump(emitter, "__elephc_eval_backtrace_entry_done");
    emitter.label("__elephc_eval_backtrace_entry_empty");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    emitter.label("__elephc_eval_backtrace_entry_done");
    abi::emit_frame_restore(emitter, 32);
    emitter.instruction("ret");                                                 // return the owned frame or a null end marker
}
