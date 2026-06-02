//! Purpose:
//! Emits the `__rt_closedir` runtime helper, which closes a directory stream
//! opened by `opendir()`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - The `DIR*` recorded by `__rt_opendir` in `_dir_handles` is cleared and
//!   handed back to libc `closedir`, which closes the stream and its descriptor.
//! - A descriptor with no recorded `DIR*` is a no-op.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// closedir: close a directory stream opened by `opendir()`.
/// Input:  AArch64 x0 = directory descriptor / x86_64 rdi = directory descriptor
pub fn emit_closedir(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_closedir_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: closedir ---");
    emitter.label_global("__rt_closedir");

    emitter.instruction("sub sp, sp, #32");                                     // frame for the libc call + glob fd stash
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- glob:// path takes precedence: probe _glob_handles[fd] first --
    abi::emit_symbol_address(emitter, "x9", "_glob_handles");
    emitter.instruction("ldr x10, [x9, x0, lsl #3]");                           // glob_state* = _glob_handles[fd]
    emitter.instruction("cbz x10, __rt_closedir_libc");                         // no glob handle: fall through to libc closedir
    emitter.instruction("str xzr, [x9, x0, lsl #3]");                           // clear the fd → glob_state entry
    emitter.instruction("str x0, [sp, #16]");                                   // stash the fd across the libc calls
    emitter.instruction("add x0, x10, #24");                                    // &struct.glob_t lives at offset 24
    emitter.bl_c("globfree");                                                   // free the kernel/libc-allocated gl_pathv entries
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the dup-minted fd
    emitter.bl_c("close");                                                      // close the synthetic fd that opendir_glob handed out
    emitter.instruction("b __rt_closedir_done");                                // skip the libc DIR* path

    emitter.label("__rt_closedir_libc");
    // -- look up the DIR* recorded for this descriptor --
    abi::emit_symbol_address(emitter, "x9", "_dir_handles");
    emitter.instruction("ldr x10, [x9, x0, lsl #3]");                           // DIR* = _dir_handles[fd]
    emitter.instruction("cbz x10, __rt_closedir_done");                         // nothing recorded: nothing to close
    emitter.instruction("str xzr, [x9, x0, lsl #3]");                           // clear the fd->DIR* table entry
    emitter.instruction("mov x0, x10");                                         // DIR* argument for closedir
    emitter.bl_c("closedir");

    emitter.label("__rt_closedir_done");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return to the caller
}

fn emit_closedir_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: closedir ---");
    emitter.label_global("__rt_closedir");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // scratch slot for the glob fd

    // -- glob:// path takes precedence: probe _glob_handles[fd] first --
    emitter.instruction("lea r9, [rip + _glob_handles]");                       // base of the fd → glob_state pointer table
    emitter.instruction("mov r10, QWORD PTR [r9 + rdi * 8]");                   // glob_state* = _glob_handles[fd]
    emitter.instruction("test r10, r10");                                       // glob handle present?
    emitter.instruction("jz __rt_closedir_libc_x86");                           // no glob handle: fall through to libc closedir
    emitter.instruction("mov QWORD PTR [r9 + rdi * 8], 0");                     // clear the fd → glob_state entry
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // stash the fd across the libc calls
    emitter.instruction("lea rdi, [r10 + 24]");                                 // &struct.glob_t lives at offset 24
    emitter.instruction("call globfree");                                       // free the kernel/libc-allocated gl_pathv entries
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the dup-minted fd
    emitter.instruction("call close");                                          // close the synthetic fd that opendir_glob handed out
    emitter.instruction("jmp __rt_closedir_done_x86");                          // skip the libc DIR* path

    emitter.label("__rt_closedir_libc_x86");
    // -- look up the DIR* recorded for this descriptor --
    emitter.instruction("lea r9, [rip + _dir_handles]");                        // base of the fd->DIR* table
    emitter.instruction("mov r10, QWORD PTR [r9 + rdi * 8]");                   // DIR* = _dir_handles[fd]
    emitter.instruction("test r10, r10");                                       // was a DIR* recorded for this descriptor?
    emitter.instruction("jz __rt_closedir_done_x86");                           // nothing recorded: nothing to close
    emitter.instruction("mov QWORD PTR [r9 + rdi * 8], 0");                     // clear the fd->DIR* table entry
    emitter.instruction("mov rdi, r10");                                        // DIR* argument for closedir
    emitter.bl_c("closedir");

    emitter.label("__rt_closedir_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the scratch slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
