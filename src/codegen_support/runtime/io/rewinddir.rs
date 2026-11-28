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

use crate::codegen_support::{emit::Emitter, platform::Arch};
use elephc_memoir::{buffers::stream_tables, interop::glob_storage};

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
    stream_tables::GLOB_HANDLES.emit_load_indexed_word(emitter, "x10", "x0", "x9");
    emitter.instruction("cbz x10, __rt_rewinddir_libc");                        // no glob handle: fall through to libc rewinddir
    glob_storage::emit_rewind(emitter, "x10");
    emitter.instruction("b __rt_rewinddir_done");                               // skip the libc path

    emitter.label("__rt_rewinddir_libc");
    // -- look up the DIR* recorded for this descriptor --
    elephc_memoir::buffers::stream_tables::DIR_HANDLES.emit_load_indexed_word(emitter, "x10", "x0", "x9");
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
    stream_tables::GLOB_HANDLES.emit_load_indexed_word(emitter, "r10", "rdi", "r9");
    emitter.instruction("test r10, r10");                                       // glob handle present?
    emitter.instruction("jz __rt_rewinddir_libc_x86");                          // no glob handle: fall through to libc rewinddir
    glob_storage::emit_rewind(emitter, "r10");
    emitter.instruction("jmp __rt_rewinddir_done_x86");                         // skip the libc path

    emitter.label("__rt_rewinddir_libc_x86");
    // -- look up the DIR* recorded for this descriptor --
    elephc_memoir::buffers::stream_tables::DIR_HANDLES.emit_load_indexed_word(emitter, "r10", "rdi", "r9");
    emitter.instruction("test r10, r10");                                       // was a DIR* recorded for this descriptor?
    emitter.instruction("jz __rt_rewinddir_done_x86");                          // nothing recorded: nothing to rewind
    emitter.instruction("mov rdi, r10");                                        // DIR* argument for rewinddir
    emitter.bl_c("rewinddir");

    emitter.label("__rt_rewinddir_done_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
