//! Purpose:
//! Emits PHP `closedir` calls.
//! Closes a directory handle opened by `opendir()`.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The descriptor is unboxed from the stream resource and handed to the
//!   `__rt_closedir` runtime helper, which calls libc `closedir`.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits codegen for PHP `closedir()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("closedir()");
    emit_stream_fd_arg("closedir", &args[0], emitter, ctx, data);
    // -- dispatch: synthetic wrapper fd -> dir_closedir, else libc closedir --
    let wrapper = ctx.next_label("closedir_wrapper");
    let after = ctx.next_label("closedir_after");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov w9, #0x4000");                             // high half of USER_WRAPPER_FD_BASE
            emitter.instruction("lsl w9, w9, #16");                             // form 0x40000000
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", wrapper));                  // dispatch into dir_closedir
            abi::emit_call_label(emitter, "__rt_closedir");                     // libc closedir
            emitter.instruction(&format!("b {}", after));                       // skip the wrapper path
            emitter.label(&wrapper);
            abi::emit_call_label(emitter, "__rt_user_wrapper_dir_closedir");    // wrapper dir_closedir + free handle
            emitter.label(&after);
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rax, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", wrapper));                   // dispatch into dir_closedir
            emitter.instruction("mov rdi, rax");                                // descriptor into the runtime-helper argument register
            abi::emit_call_label(emitter, "__rt_closedir");                     // libc closedir
            emitter.instruction(&format!("jmp {}", after));                     // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("mov rdi, rax");                                // descriptor into the runtime-helper argument register
            abi::emit_call_label(emitter, "__rt_user_wrapper_dir_closedir");    // wrapper dir_closedir + free handle
            emitter.label(&after);
        }
    }
    Some(PhpType::Void)
}
