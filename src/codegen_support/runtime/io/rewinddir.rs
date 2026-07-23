//! Purpose:
//! Emits the `__rt_rewinddir` runtime helper, which rewinds a directory stream
//! opened by `opendir()` back to its first entry.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - The `DIR*` recorded by `__rt_opendir` in `_dir_handles` is handed to libc
//!   `rewinddir`; the handle stays registered so later `readdir()` calls reuse it.
//! - A descriptor with no recorded `DIR*` is a no-op.
//! - The x86_64 `rewinddir` call site routes through `Emitter::emit_call_c`. On
//!   Windows, `__rt_sys_rewinddir` closes and reopens the `FindFirstFileExW`
//!   search against its retained pattern. Other targets continue to use libc.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// rewinddir: rewind a directory stream to its first entry.
/// Input:  AArch64 x0 = directory descriptor / x86_64 rdi = directory descriptor
pub fn emit_rewinddir(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_rewinddir_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: rewinddir ---");
    emitter.label_global("__rt_rewinddir");

    emitter.instruction("sub sp, sp, #16");                                     // minimal frame for the libc call
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- glob:// path takes precedence: probe _glob_handles[fd] first --
    abi::emit_symbol_address(emitter, "x9", "_glob_handles");
    emitter.instruction("ldr x10, [x9, x0, lsl #3]");                           // glob_state* = _glob_handles[fd]
    emitter.instruction("cbz x10, __rt_rewinddir_libc");                        // no glob handle: fall through to libc rewinddir
    emitter.instruction("str xzr, [x10, #16]");                                 // reset the glob iteration index to 0
    emitter.instruction("b __rt_rewinddir_done");                               // skip the libc path

    emitter.label("__rt_rewinddir_libc");
    // -- look up the DIR* recorded for this descriptor --
    abi::emit_symbol_address(emitter, "x9", "_dir_handles");
    emitter.instruction("ldr x10, [x9, x0, lsl #3]");                           // DIR* = _dir_handles[fd]
    emitter.instruction("cbz x10, __rt_rewinddir_done");                        // nothing recorded: nothing to rewind
    emitter.instruction("mov x0, x10");                                         // DIR* argument for rewinddir
    emitter.bl_c("rewinddir");

    emitter.label("__rt_rewinddir_done");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 stream runtime helper for rewinddir.
fn emit_rewinddir_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: rewinddir ---");
    emitter.label_global("__rt_rewinddir");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    // -- glob:// path takes precedence: probe _glob_handles[fd] first --
    abi::emit_symbol_address(emitter, "r9", "_glob_handles");                   // base of the fd → glob_state pointer table
    emitter.instruction("mov r10, QWORD PTR [r9 + rdi * 8]");                   // glob_state* = _glob_handles[fd]
    emitter.instruction("test r10, r10");                                       // glob handle present?
    emitter.instruction("jz __rt_rewinddir_libc_x86");                          // no glob handle: fall through to libc rewinddir
    emitter.instruction("mov QWORD PTR [r10 + 16], 0");                         // reset the glob iteration index to 0
    emitter.instruction("jmp __rt_rewinddir_done_x86");                         // skip the libc path

    emitter.label("__rt_rewinddir_libc_x86");
    // -- look up the DIR* recorded for this descriptor --
    abi::emit_symbol_address(emitter, "r9", "_dir_handles");                    // base of the fd->DIR* table
    emitter.instruction("mov r10, QWORD PTR [r9 + rdi * 8]");                   // DIR* = _dir_handles[fd]
    emitter.instruction("test r10, r10");                                       // was a DIR* recorded for this descriptor?
    emitter.instruction("jz __rt_rewinddir_done_x86");                          // nothing recorded: nothing to rewind
    emitter.instruction("mov rdi, r10");                                        // DIR* argument for rewinddir
    emitter.emit_call_c("rewinddir");                                           // Windows: close and reopen the retained FindFirstFileExW search

    emitter.label("__rt_rewinddir_done_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
