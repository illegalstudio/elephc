//! Purpose:
//! Emits `__rt_closedir` over a typed `StreamState` directory backend.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Backend ownership is detached before callbacks for exact-once cleanup.
//! - Glob cleanup releases libc matches, the synthetic descriptor, and the
//!   heap-owned glob state in child-before-parent order.

use crate::codegen_support::runtime::resources::layout::{
    STREAM_BACKEND_AUX_OFFSET, STREAM_BACKEND_DIRECTORY, STREAM_BACKEND_GLOB_DIRECTORY,
    STREAM_BACKEND_KIND_OFFSET, STREAM_BACKEND_USER_DIRECTORY, STREAM_FD_OFFSET,
};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits exact-once directory closure from an authoritative `StreamState` pointer.
pub fn emit_closedir(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_closedir_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: closedir through StreamState backend ownership ---");
    emitter.label_global("__rt_closedir");
    emitter.instruction("sub sp, sp, #48");                                     // preserve state, owner, descriptor, and caller frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("cbz x0, __rt_closedir_fail");                          // a missing StreamState owns no directory backend
    emitter.instruction("str x0, [sp, #0]");                                    // preserve StreamState across backend cleanup
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", STREAM_BACKEND_KIND_OFFSET
    ));                                                                         // load the typed directory backend discriminator
    emitter.instruction(&format!(
        "ldr x10, [x0, #{}]", STREAM_BACKEND_AUX_OFFSET
    ));                                                                         // load native or glob backend ownership
    emitter.instruction("str x10, [sp, #8]");                                   // preserve the detached backend owner
    emitter.instruction(&format!("ldr x11, [x0, #{}]", STREAM_FD_OFFSET));      // load the descriptor or synthetic wrapper handle
    emitter.instruction("str x11, [sp, #16]");                                  // preserve the backend descriptor
    emitter.instruction(&format!(
        "str xzr, [x0, #{}]", STREAM_BACKEND_AUX_OFFSET
    ));                                                                         // detach auxiliary ownership before re-entrant callbacks
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_GLOB_DIRECTORY
    ));                                                                         // is this an owned glob iterator?
    emitter.instruction("b.eq __rt_closedir_glob");                             // release every glob-owned child
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_USER_DIRECTORY
    ));                                                                         // is this a userspace directory wrapper?
    emitter.instruction("b.eq __rt_closedir_user");                             // dispatch its close callback
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_DIRECTORY
    ));                                                                         // is this a native libc directory?
    emitter.instruction("b.ne __rt_closedir_fail");                             // reject non-directory backends defensively
    emitter.instruction("mov x0, x10");                                         // pass the detached owning DIR* to libc
    emitter.instruction("cbz x0, __rt_closedir_fail");                          // an already-detached owner cannot close twice
    emitter.bl_c("closedir");
    emitter.instruction("b __rt_closedir_done");                                // return libc's close status

    emitter.label("__rt_closedir_glob");
    emitter.instruction("cbz x10, __rt_closedir_fail");                         // an already-detached glob cannot close twice
    emitter.instruction("add x0, x10, #24");                                    // pass the embedded glob_t to globfree
    emitter.bl_c("globfree");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the synthetic descriptor
    emitter.instruction("cmp x0, #0");                                          // is the descriptor valid?
    emitter.instruction("b.lt __rt_closedir_glob_free");                        // skip close for an absent descriptor
    emitter.syscall(6);                                                         // close the descriptor minted for this glob stream
    emitter.label("__rt_closedir_glob_free");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the detached glob-state allocation
    emitter.instruction("bl __rt_heap_free");                                   // release the owned glob state itself
    emitter.instruction("mov x0, #0");                                          // report successful directory closure
    emitter.instruction("b __rt_closedir_done");                                // join the helper epilogue

    emitter.label("__rt_closedir_user");
    emitter.instruction("mov x0, x11");                                         // pass the synthetic directory handle to wrapper dispatch
    emitter.instruction("bl __rt_user_wrapper_dir_closedir");                   // invoke dir_closedir exactly once
    emitter.instruction("b __rt_closedir_done");                                // preserve the wrapper callback result
    emitter.label("__rt_closedir_fail");
    emitter.instruction("mov x0, #-1");                                         // report invalid or already-detached state
    emitter.label("__rt_closedir_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the backend close result
}

/// Emits Linux x86_64 exact-once typed directory closure.
fn emit_closedir_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: closedir through StreamState backend ownership ---");
    emitter.label_global("__rt_closedir");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // preserve state, owner, and descriptor
    emitter.instruction("test rdi, rdi");                                       // is an authoritative StreamState available?
    emitter.instruction("jz __rt_closedir_fail_x86");                           // a missing state owns no directory backend
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve StreamState across backend cleanup
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rdi + {}]", STREAM_BACKEND_KIND_OFFSET
    ));                                                                         // load the typed directory backend discriminator
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rdi + {}]", STREAM_BACKEND_AUX_OFFSET
    ));                                                                         // load native or glob backend ownership
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the detached backend owner
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rdi + {}]", STREAM_FD_OFFSET
    ));                                                                         // load the descriptor or synthetic wrapper handle
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // preserve the backend descriptor
    emitter.instruction(&format!(
        "mov QWORD PTR [rdi + {}], 0", STREAM_BACKEND_AUX_OFFSET
    ));                                                                         // detach auxiliary ownership before re-entrant callbacks
    emitter.instruction(&format!(
        "cmp r9, {}", STREAM_BACKEND_GLOB_DIRECTORY
    ));                                                                         // is this an owned glob iterator?
    emitter.instruction("je __rt_closedir_glob_x86");                           // release every glob-owned child
    emitter.instruction(&format!(
        "cmp r9, {}", STREAM_BACKEND_USER_DIRECTORY
    ));                                                                         // is this a userspace directory wrapper?
    emitter.instruction("je __rt_closedir_user_x86");                           // dispatch its close callback
    emitter.instruction(&format!("cmp r9, {}", STREAM_BACKEND_DIRECTORY));      // is this a native libc directory?
    emitter.instruction("jne __rt_closedir_fail_x86");                          // reject non-directory backends defensively
    emitter.instruction("mov rdi, r10");                                        // pass the detached owning DIR* to libc
    emitter.instruction("test rdi, rdi");                                       // is native ownership still attached?
    emitter.instruction("jz __rt_closedir_fail_x86");                           // an already-detached owner cannot close twice
    emitter.bl_c("closedir");
    emitter.instruction("jmp __rt_closedir_done_x86");                          // return libc's close status

    emitter.label("__rt_closedir_glob_x86");
    emitter.instruction("test r10, r10");                                       // is glob ownership still attached?
    emitter.instruction("jz __rt_closedir_fail_x86");                           // an already-detached glob cannot close twice
    emitter.instruction("lea rdi, [r10 + 24]");                                 // pass the embedded glob_t to globfree
    emitter.instruction("call globfree");                                       // release libc-owned match strings and vector
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the synthetic descriptor
    emitter.instruction("test rdi, rdi");                                       // is the descriptor valid?
    emitter.instruction("js __rt_closedir_glob_free_x86");                      // skip close for an absent descriptor
    emitter.instruction("call close");                                          // close the descriptor minted for this glob stream
    emitter.label("__rt_closedir_glob_free_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the detached glob-state allocation
    emitter.instruction("call __rt_heap_free");                                 // release the owned glob state itself
    emitter.instruction("xor eax, eax");                                        // report successful directory closure
    emitter.instruction("jmp __rt_closedir_done_x86");                          // join the helper epilogue

    emitter.label("__rt_closedir_user_x86");
    emitter.instruction("mov rdi, r11");                                        // pass the synthetic directory handle to wrapper dispatch
    emitter.instruction("call __rt_user_wrapper_dir_closedir");                 // invoke dir_closedir exactly once
    emitter.instruction("jmp __rt_closedir_done_x86");                          // preserve the wrapper callback result
    emitter.label("__rt_closedir_fail_x86");
    emitter.instruction("mov rax, -1");                                         // report invalid or already-detached state
    emitter.label("__rt_closedir_done_x86");
    emitter.instruction("add rsp, 32");                                         // release helper scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the backend close result
}
