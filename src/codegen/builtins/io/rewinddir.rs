//! Purpose:
//! Emits PHP `rewinddir` calls.
//! Rewinds a directory handle opened by `opendir()` back to its first entry.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - The descriptor is unboxed from the stream resource and handed to the
//!   `__rt_rewinddir` runtime helper, which calls libc `rewinddir`.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("rewinddir()");
    emit_stream_fd_arg("rewinddir", &args[0], emitter, ctx, data);
    // -- dispatch: synthetic wrapper fd -> dir_rewinddir, else libc rewinddir --
    let wrapper = ctx.next_label("rewinddir_wrapper");
    let after = ctx.next_label("rewinddir_after");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov w9, #0x4000");                             // high half of USER_WRAPPER_FD_BASE
            emitter.instruction("lsl w9, w9, #16");                             // form 0x40000000
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", wrapper));                  // dispatch into dir_rewinddir
            abi::emit_call_label(emitter, "__rt_rewinddir");                    // libc rewinddir
            emitter.instruction(&format!("b {}", after));                       // skip the wrapper path
            emitter.label(&wrapper);
            abi::emit_call_label(emitter, "__rt_user_wrapper_dir_rewinddir");   // wrapper dir_rewinddir
            emitter.label(&after);
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rax, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", wrapper));                   // dispatch into dir_rewinddir
            emitter.instruction("mov rdi, rax");                                // descriptor into the runtime-helper argument register
            abi::emit_call_label(emitter, "__rt_rewinddir");                    // libc rewinddir
            emitter.instruction(&format!("jmp {}", after));                     // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("mov rdi, rax");                                // descriptor into the runtime-helper argument register
            abi::emit_call_label(emitter, "__rt_user_wrapper_dir_rewinddir");   // wrapper dir_rewinddir
            emitter.label(&after);
        }
    }
    Some(PhpType::Void)
}
