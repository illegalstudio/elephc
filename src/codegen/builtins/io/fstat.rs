//! Purpose:
//! Emits PHP `fstat` stream builtin calls over runtime file handles.
//! Uses shared stream unboxing before invoking file descriptor runtime helpers.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Stream resources must be validated and failure results must follow PHP false/null conventions.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;
use super::stat_result::box_stat_array_or_false_result;
use super::stream_arg::emit_stream_fd_arg;

/// Emits the `fstat` builtin call.
///
/// Unboxes the stream resource in `args[0]` to extract the raw file descriptor.
/// A synthetic user-wrapper fd (`>= 0x40000000`) dispatches into
/// `__rt_user_wrapper_fstat`, which invokes the wrapper's `stream_stat()` and
/// returns its boxed Mixed stat array (or boxed `false`) directly. A normal fd
/// calls `__rt_fstat_array` to build a PHP-compatible fstat array and boxes the
/// result. Either path leaves a boxed Mixed in the int-result register, so the
/// builtin returns `PhpType::Mixed` (an array or `false`).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fstat()");
    emit_stream_fd_arg("fstat", &args[0], emitter, ctx, data);
    let wrapper_label = ctx.next_label("fstat_user_wrapper");
    let after_dispatch = ctx.next_label("fstat_after_dispatch");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- user-wrapper synthetic fd path --
            emitter.instruction("mov w9, #0x4000");                             // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
            emitter.instruction("lsl w9, w9, #16");                             // shift into bits 30..16 to form 0x40000000
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", wrapper_label));            // dispatch into the wrapper's stream_stat instead of fstat
            abi::emit_call_label(emitter, "__rt_fstat_array");                  // normal fd: build the PHP-compatible fstat array from the platform stat
            box_stat_array_or_false_result(emitter, ctx);                       // box the raw stat array (or false) into a Mixed cell
            emitter.instruction(&format!("b {}", after_dispatch));              // skip the user-wrapper path for normal fds
            emitter.label(&wrapper_label);
            abi::emit_call_label(emitter, "__rt_user_wrapper_fstat");           // wrapper fd: dispatch stream_stat, result already a boxed Mixed
            emitter.label(&after_dispatch);
        }
        Arch::X86_64 => {
            // -- user-wrapper synthetic fd path --
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rax, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", wrapper_label));             // dispatch into the wrapper's stream_stat instead of fstat
            abi::emit_call_label(emitter, "__rt_fstat_array");                  // normal fd: build the PHP-compatible fstat array from the platform stat
            box_stat_array_or_false_result(emitter, ctx);                       // box the raw stat array (or false) into a Mixed cell
            emitter.instruction(&format!("jmp {}", after_dispatch));            // skip the user-wrapper path for normal fds
            emitter.label(&wrapper_label);
            emitter.instruction("mov rdi, rax");                                // the wrapper helper's handle lookup expects the synthetic fd in rdi
            abi::emit_call_label(emitter, "__rt_user_wrapper_fstat");           // wrapper fd: dispatch stream_stat, result already a boxed Mixed
            emitter.label(&after_dispatch);
        }
    }
    Some(PhpType::Mixed)
}
