//! Purpose:
//! Emits `__rt_rewinddir` over a typed `StreamState` directory backend.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Native `DIR*`, glob index, and userspace callbacks are selected by backend
//!   kind; no operation depends on descriptor-indexed ownership tables.

use crate::codegen_support::runtime::resources::layout::{
    STREAM_BACKEND_AUX_OFFSET, STREAM_BACKEND_DIRECTORY, STREAM_BACKEND_GLOB_DIRECTORY,
    STREAM_BACKEND_KIND_OFFSET, STREAM_BACKEND_USER_DIRECTORY, STREAM_FD_OFFSET,
};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits typed directory rewind from an authoritative `StreamState` pointer.
pub fn emit_rewinddir(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_rewinddir_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: rewinddir through StreamState backend ownership ---");
    emitter.label_global("__rt_rewinddir");
    emitter.instruction("sub sp, sp, #16");                                     // preserve the caller frame around nested callbacks
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("cbz x0, __rt_rewinddir_done");                         // a missing StreamState has nothing to rewind
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", STREAM_BACKEND_KIND_OFFSET
    ));                                                                         // load the typed directory backend discriminator
    emitter.instruction(&format!(
        "ldr x10, [x0, #{}]", STREAM_BACKEND_AUX_OFFSET
    ));                                                                         // load native or glob backend ownership
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_GLOB_DIRECTORY
    ));                                                                         // is this an owned glob iterator?
    emitter.instruction("b.eq __rt_rewinddir_glob");                            // reset its state-owned index
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_USER_DIRECTORY
    ));                                                                         // is this a userspace directory wrapper?
    emitter.instruction("b.eq __rt_rewinddir_user");                            // dispatch its rewind callback
    emitter.instruction(&format!(
        "cmp x9, #{}", STREAM_BACKEND_DIRECTORY
    ));                                                                         // is this a native libc directory?
    emitter.instruction("b.ne __rt_rewinddir_done");                            // ignore non-directory streams defensively
    emitter.instruction("mov x0, x10");                                         // pass the owning DIR* to libc rewinddir
    emitter.instruction("cbz x0, __rt_rewinddir_done");                         // detached native state is already closed
    emitter.bl_c("rewinddir");
    emitter.instruction("b __rt_rewinddir_done");                               // join the helper epilogue
    emitter.label("__rt_rewinddir_glob");
    emitter.instruction("cbz x10, __rt_rewinddir_done");                        // detached glob state is already closed
    emitter.instruction("str xzr, [x10, #16]");                                 // reset the state-owned glob iteration index
    emitter.instruction("b __rt_rewinddir_done");                               // join the helper epilogue
    emitter.label("__rt_rewinddir_user");
    emitter.instruction(&format!("ldr x0, [x0, #{}]", STREAM_FD_OFFSET));       // load the wrapper's synthetic directory handle
    emitter.instruction("bl __rt_user_wrapper_dir_rewinddir");                  // invoke the userspace dir_rewinddir callback
    emitter.label("__rt_rewinddir_done");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return after backend-specific rewind
}

/// Emits Linux x86_64 typed directory rewind.
fn emit_rewinddir_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: rewinddir through StreamState backend ownership ---");
    emitter.label_global("__rt_rewinddir");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("test rdi, rdi");                                       // is an authoritative StreamState available?
    emitter.instruction("jz __rt_rewinddir_done_x86");                          // a missing state has nothing to rewind
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rdi + {}]", STREAM_BACKEND_KIND_OFFSET
    ));                                                                         // load the typed directory backend discriminator
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rdi + {}]", STREAM_BACKEND_AUX_OFFSET
    ));                                                                         // load native or glob backend ownership
    emitter.instruction(&format!(
        "cmp r9, {}", STREAM_BACKEND_GLOB_DIRECTORY
    ));                                                                         // is this an owned glob iterator?
    emitter.instruction("je __rt_rewinddir_glob_x86");                          // reset its state-owned index
    emitter.instruction(&format!(
        "cmp r9, {}", STREAM_BACKEND_USER_DIRECTORY
    ));                                                                         // is this a userspace directory wrapper?
    emitter.instruction("je __rt_rewinddir_user_x86");                          // dispatch its rewind callback
    emitter.instruction(&format!("cmp r9, {}", STREAM_BACKEND_DIRECTORY));      // is this a native libc directory?
    emitter.instruction("jne __rt_rewinddir_done_x86");                         // ignore non-directory streams defensively
    emitter.instruction("mov rdi, r10");                                        // pass the owning DIR* to libc rewinddir
    emitter.instruction("test rdi, rdi");                                       // is native ownership still attached?
    emitter.instruction("jz __rt_rewinddir_done_x86");                          // detached native state is already closed
    emitter.bl_c("rewinddir");
    emitter.instruction("jmp __rt_rewinddir_done_x86");                         // join the helper epilogue
    emitter.label("__rt_rewinddir_glob_x86");
    emitter.instruction("test r10, r10");                                       // is glob ownership still attached?
    emitter.instruction("jz __rt_rewinddir_done_x86");                          // detached glob state is already closed
    emitter.instruction("mov QWORD PTR [r10 + 16], 0");                         // reset the state-owned glob iteration index
    emitter.instruction("jmp __rt_rewinddir_done_x86");                         // join the helper epilogue
    emitter.label("__rt_rewinddir_user_x86");
    emitter.instruction(&format!(
        "mov rdi, QWORD PTR [rdi + {}]", STREAM_FD_OFFSET
    ));                                                                         // load the wrapper's synthetic directory handle
    emitter.instruction("call __rt_user_wrapper_dir_rewinddir");                // invoke the userspace dir_rewinddir callback
    emitter.label("__rt_rewinddir_done_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return after backend-specific rewind
}
