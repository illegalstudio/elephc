//! Purpose:
//! Emits `__rt_str_persist_owned`: persist a string whose source the caller owns.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - `__rt_str_persist` takes over a transient concat temporary but DUPLICATES every other heap
//!   string, which is right for a borrowed source and a leak for an owned one: a callback's
//!   returned string, or the fresh copy `__rt_mixed_cast_string` hands back for a boxed string.
//!   This variant persists, then frees the source when the persisted string is a different
//!   block. `__rt_heap_free_safe` ignores anything that is not a live heap block, so a literal
//!   or `_concat_buf` scratch source is left alone.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the owned-source string persistence helper.
///
/// # ABI
/// - ARM64: in `x1` = pointer, `x2` = length; out `x1`/`x2` = the persisted string.
/// - x86_64: in `rax` = pointer, `rdx` = length; out `rax`/`rdx` = the persisted string.
pub fn emit_str_persist_owned(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_persist_owned ---");
    emitter.label_global("__rt_str_persist_owned");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-32]!");                   // frame with one slot for the source string
            emitter.instruction("mov x29, sp");
            emitter.instruction("stp x1, x2, [sp, #16]");                       // remember the owned source
            emitter.instruction("bl __rt_str_persist");                         // take the source over, or duplicate it
            emitter.instruction("ldr x9, [sp, #16]");                           // the source pointer
            emitter.instruction("cmp x9, x1");                                  // was the source itself kept?
            emitter.instruction("b.eq __rt_str_persist_owned_done");            // yes: nothing is left over
            emitter.instruction("stp x1, x2, [sp, #16]");                       // keep the persisted string across the release
            emitter.instruction("mov x0, x9");
            emitter.instruction("bl __rt_heap_free_safe");                      // release the duplicated source the caller owned
            emitter.instruction("ldp x1, x2, [sp, #16]");                       // the persisted string is the result
            emitter.label("__rt_str_persist_owned_done");
            emitter.instruction("ldp x29, x30, [sp], #32");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // frame with one slot for the source string
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 16");
            emitter.instruction("mov QWORD PTR [rbp - 16], rax");               // remember the owned source
            emitter.instruction("mov QWORD PTR [rbp - 8], rdx");
            emitter.instruction("call __rt_str_persist");                       // take the source over, or duplicate it
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // the source pointer
            emitter.instruction("cmp r10, rax");                                // was the source itself kept?
            emitter.instruction("je __rt_str_persist_owned_done");              // yes: nothing is left over
            emitter.instruction("mov QWORD PTR [rbp - 16], rax");               // keep the persisted string across the release
            emitter.instruction("mov QWORD PTR [rbp - 8], rdx");
            emitter.instruction("mov rax, r10");
            emitter.instruction("call __rt_heap_free_safe");                    // release the duplicated source the caller owned
            emitter.instruction("mov rax, QWORD PTR [rbp - 16]");               // the persisted string is the result
            emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");
            emitter.label("__rt_str_persist_owned_done");
            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}
