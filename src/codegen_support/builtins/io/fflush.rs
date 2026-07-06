//! Purpose:
//! Emits PHP `fflush` stream builtin calls over runtime file handles.
//! Uses shared stream unboxing before invoking file descriptor runtime helpers.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Stream resources must be validated and failure results must follow PHP false/null conventions.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits the `fflush` builtin call, flushing the output buffer of an open file handle.
///
/// # Arguments
/// - `_name`: Unused name for dispatch; the builtin is identified by this module.
/// - `args`: Must contain at least one `Expr` identifying the stream resource.
/// - `emitter`: Target assembly emitter.
/// - `ctx`: Codegen context carrying variable layout and stream metadata.
/// - `data`: Data section for constants and relocations.
///
/// # Behavior
/// Unboxes the stream resource via `emit_stream_fd_arg` to extract the raw file descriptor,
/// then calls `__rt_fflush` (a libc `fsync` wrapper with PHP-side fflush semantics).
///
/// # Return
/// Always returns `Some(PhpType::Bool)` — `true` on success, `false` on error (e.g., invalid stream).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fflush()");
    emit_stream_fd_arg("fflush", &args[0], emitter, ctx, data);
    let user_wrapper_label = ctx.next_label("fflush_user_wrapper");
    let after_dispatch = ctx.next_label("fflush_after_dispatch");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- user-wrapper synthetic fd path (Phase 10 step 4) --
            emitter.instruction("mov w9, #0x4000");                             // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
            emitter.instruction("lsl w9, w9, #16");                             // shift into bits 30..16 to form 0x40000000
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", user_wrapper_label));       // dispatch into the wrapper's stream_flush instead of fsync
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rax, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", user_wrapper_label));        // dispatch into the wrapper's stream_flush instead of fsync
        }
    }
    abi::emit_call_label(emitter, "__rt_fflush");                               // libc fsync(fd) wrapper (PHP-side fflush semantics)
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {}", after_dispatch)), // skip the user-wrapper path on the normal-fd result
        Arch::X86_64 => emitter.instruction(&format!("jmp {}", after_dispatch)), // skip the user-wrapper path on the normal-fd result
    }
    emitter.label(&user_wrapper_label);
    if matches!(emitter.target.arch, Arch::X86_64) {
        emitter.instruction("mov rdi, rax");                                    // move the synthetic fd into the first SysV arg register for the wrapper helper
    }
    abi::emit_call_label(emitter, "__rt_user_wrapper_fflush");                  // dispatch into the wrapper's stream_flush
    emitter.label(&after_dispatch);
    Some(PhpType::Bool)
}
