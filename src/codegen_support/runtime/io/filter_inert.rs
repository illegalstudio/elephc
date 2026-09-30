//! Purpose:
//! Emits `__rt_filter_mark_inert`, which stamps a chain node as existing for its resource identity
//! alone.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`, for the filters compiled as an inline shape.
//!
//! Key details:
//! - `zlib.*`, `bzip2.*` and `convert.iconv.*` filter through code emitted over the descriptor, not
//!   through a chain node, so they had no resource at all to hand back. The node minted for them
//!   carries no built-in id and no `php_user_filter`, which is already enough for the chain applier
//!   to pass it by; the flag is what tells `stream_filter_remove()` that removing it must also
//!   clear the per-descriptor tables where the real filtering lives.

use crate::codegen_support::runtime::resources::layout::{FILTER_FLAGS_OFFSET, FILTER_FLAG_INERT};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_filter_mark_inert(handle) -> handle`.
pub fn emit_filter_mark_inert(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mark a filter node inert ---");
    emitter.label_global("__rt_filter_mark_inert");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // frame for the handle and the saved linkage
            emitter.instruction("stp x29, x30, [sp, #16]");                     // save frame pointer and return address
            emitter.instruction("add x29, sp, #16");                            // establish the helper frame pointer
            emitter.instruction("str x0, [sp, #0]");                            // the handle is also the answer
            emitter.instruction("bl __rt_filter_state");                        // resolve the node
            emitter.instruction("cbz x0, __rt_fmi_done");                       // a stale handle stamps nothing
            emitter.instruction(&format!("ldr x9, [x0, #{FILTER_FLAGS_OFFSET}]"));
            emitter.instruction(&format!("orr x9, x9, #{FILTER_FLAG_INERT}"));
            emitter.instruction(&format!("str x9, [x0, #{FILTER_FLAGS_OFFSET}]"));
            emitter.label("__rt_fmi_done");
            emitter.instruction("ldr x0, [sp, #0]");                            // hand the handle back unchanged
            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address
            emitter.instruction("add sp, sp, #32");                             // release the helper frame
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the helper frame
            emitter.instruction("sub rsp, 16");                                 // reserve the handle slot
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // the handle is also the answer
            emitter.instruction("call __rt_filter_state");                      // resolve the node
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_fmi_done_x86");                        // a stale handle stamps nothing
            emitter.instruction(&format!(
                "or QWORD PTR [rax + {FILTER_FLAGS_OFFSET}], {FILTER_FLAG_INERT}"
            ));
            emitter.label("__rt_fmi_done_x86");
            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                // hand the handle back unchanged
            emitter.instruction("add rsp, 16");                                 // release the helper frame
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");
        }
    }
}
