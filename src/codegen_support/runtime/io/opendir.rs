//! Purpose:
//! Emits the `__rt_opendir` runtime helper, which opens a directory stream
//! through libc `opendir` and exposes its underlying descriptor.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - libc `opendir` yields a `DIR*`; `dirfd` recovers the raw descriptor that
//!   becomes the PHP directory stream resource value.
//! - The `DIR*` is recorded in the `_dir_handles` table keyed by descriptor so
//!   `readdir()`, `rewinddir()`, and `closedir()` can hand it back to libc.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// opendir: open a directory stream and return its descriptor.
/// Input:  AArch64 x1/x2 = directory path string
///         x86_64  rax/rdx = directory path string
/// Output: the directory descriptor, or -1 on failure
pub fn emit_opendir(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_opendir_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: opendir ---");
    emitter.label_global("__rt_opendir");

    // -- userspace stream-wrapper probe (registered scheme://) --
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // probe frame, save fp/lr
    emitter.instruction("mov x29, sp");                                         // establish the probe frame pointer
    emitter.instruction("stp x1, x2, [sp, #16]");                               // save path ptr/len for the fall-through
    emitter.instruction("bl __rt_user_wrapper_opendir");                        // path in x1/x2 → fd | -1 | -2
    emitter.instruction("cmn x0, #2");                                          // is the result the "not a wrapper" sentinel (-2)?
    emitter.instruction("b.eq __rt_opendir_uw_fall");                           // no registered scheme matched → fall through to libc
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the probe frame
    emitter.instruction("ret");                                                 // return the synthetic fd or false sentinel
    emitter.label("__rt_opendir_uw_fall");
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // restore the path ptr/len
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the probe frame

    // -- glob:// scheme probe before the libc opendir path --
    emitter.instruction("cmp x2, #7");                                          // "glob://" needs at least seven bytes
    emitter.instruction("b.lt __rt_opendir_no_glob");                           // too short → fall through to libc opendir
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load scheme byte 0
    emitter.instruction("cmp w9, #103");                                        // 'g'?
    emitter.instruction("b.ne __rt_opendir_no_glob");                           // not the glob scheme
    emitter.instruction("ldrb w9, [x1, #1]");                                   // load scheme byte 1
    emitter.instruction("cmp w9, #108");                                        // 'l'?
    emitter.instruction("b.ne __rt_opendir_no_glob");                           // not the glob scheme
    emitter.instruction("ldrb w9, [x1, #2]");                                   // load scheme byte 2
    emitter.instruction("cmp w9, #111");                                        // 'o'?
    emitter.instruction("b.ne __rt_opendir_no_glob");                           // not the glob scheme
    emitter.instruction("ldrb w9, [x1, #3]");                                   // load scheme byte 3
    emitter.instruction("cmp w9, #98");                                         // 'b'?
    emitter.instruction("b.ne __rt_opendir_no_glob");                           // not the glob scheme
    emitter.instruction("ldrb w9, [x1, #4]");                                   // load scheme byte 4
    emitter.instruction("cmp w9, #58");                                         // ':'?
    emitter.instruction("b.ne __rt_opendir_no_glob");                           // not the glob scheme
    emitter.instruction("ldrb w9, [x1, #5]");                                   // load scheme byte 5
    emitter.instruction("cmp w9, #47");                                         // '/'?
    emitter.instruction("b.ne __rt_opendir_no_glob");                           // not the glob scheme
    emitter.instruction("ldrb w9, [x1, #6]");                                   // load scheme byte 6
    emitter.instruction("cmp w9, #47");                                         // '/'?
    emitter.instruction("b.ne __rt_opendir_no_glob");                           // not the glob scheme
    emitter.instruction("b __rt_opendir_glob");                                 // glob:// path: tail-call into the synthetic helper
    emitter.label("__rt_opendir_no_glob");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // frame for the saved registers and the DIR* slot
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- null-terminate the directory path --
    emitter.instruction("bl __rt_cstr");                                        // convert the path to a C string, x0 = C string

    // -- open the directory stream --
    emitter.bl_c("opendir");
    emitter.instruction("cbz x0, __rt_opendir_fail");                           // a NULL DIR* means opendir failed
    emitter.instruction("str x0, [sp, #16]");                                   // save the DIR* across the dirfd call

    // -- recover the underlying descriptor with dirfd --
    emitter.bl_c("dirfd");

    // -- record the DIR* in the fd->DIR* table for readdir/closedir --
    abi::emit_symbol_address(emitter, "x9", "_dir_handles");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the DIR* handle
    emitter.instruction("str x10, [x9, x0, lsl #3]");                           // _dir_handles[fd] = DIR*
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the directory descriptor

    emitter.label("__rt_opendir_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 reports an opendir failure
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for opendir.
fn emit_opendir_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: opendir ---");
    emitter.label_global("__rt_opendir");

    // -- userspace stream-wrapper probe (registered scheme://) --
    emitter.instruction("push rbp");                                            // probe frame: preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the probe frame pointer
    emitter.instruction("sub rsp, 16");                                         // spill slot for the saved path
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save path ptr for the fall-through
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save path len for the fall-through
    emitter.instruction("call __rt_user_wrapper_opendir");                      // path in rax/rdx → fd | -1 | -2
    emitter.instruction("cmp rax, -2");                                         // is the result the "not a wrapper" sentinel (-2)?
    emitter.instruction("je __rt_opendir_uw_fall_x86");                         // no registered scheme matched → fall through to libc
    emitter.instruction("add rsp, 16");                                         // release the probe frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the synthetic fd or false sentinel
    emitter.label("__rt_opendir_uw_fall_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the path ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the path len
    emitter.instruction("add rsp, 16");                                         // release the probe frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer

    // -- glob:// scheme probe before the libc opendir path --
    emitter.instruction("cmp rdx, 7");                                          // "glob://" needs at least seven bytes
    emitter.instruction("jl __rt_opendir_no_glob_x86");                         // too short → fall through to libc opendir
    emitter.instruction("movzx ecx, BYTE PTR [rax + 0]");                       // load scheme byte 0
    emitter.instruction("cmp ecx, 103");                                        // 'g'?
    emitter.instruction("jne __rt_opendir_no_glob_x86");                        // not the glob scheme
    emitter.instruction("movzx ecx, BYTE PTR [rax + 1]");                       // load scheme byte 1
    emitter.instruction("cmp ecx, 108");                                        // 'l'?
    emitter.instruction("jne __rt_opendir_no_glob_x86");                        // not the glob scheme
    emitter.instruction("movzx ecx, BYTE PTR [rax + 2]");                       // load scheme byte 2
    emitter.instruction("cmp ecx, 111");                                        // 'o'?
    emitter.instruction("jne __rt_opendir_no_glob_x86");                        // not the glob scheme
    emitter.instruction("movzx ecx, BYTE PTR [rax + 3]");                       // load scheme byte 3
    emitter.instruction("cmp ecx, 98");                                         // 'b'?
    emitter.instruction("jne __rt_opendir_no_glob_x86");                        // not the glob scheme
    emitter.instruction("movzx ecx, BYTE PTR [rax + 4]");                       // load scheme byte 4
    emitter.instruction("cmp ecx, 58");                                         // ':'?
    emitter.instruction("jne __rt_opendir_no_glob_x86");                        // not the glob scheme
    emitter.instruction("movzx ecx, BYTE PTR [rax + 5]");                       // load scheme byte 5
    emitter.instruction("cmp ecx, 47");                                         // '/'?
    emitter.instruction("jne __rt_opendir_no_glob_x86");                        // not the glob scheme
    emitter.instruction("movzx ecx, BYTE PTR [rax + 6]");                       // load scheme byte 6
    emitter.instruction("cmp ecx, 47");                                         // '/'?
    emitter.instruction("jne __rt_opendir_no_glob_x86");                        // not the glob scheme
    emitter.instruction("jmp __rt_opendir_glob");                               // glob:// path: tail-call into the synthetic helper
    emitter.label("__rt_opendir_no_glob_x86");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve a spill slot for the DIR* handle

    // -- null-terminate the directory path --
    emitter.instruction("call __rt_cstr");                                      // convert the path to a C string, rax = C string

    // -- open the directory stream --
    emitter.instruction("mov rdi, rax");                                        // C-string path argument for opendir
    emitter.bl_c("opendir");
    emitter.instruction("test rax, rax");                                       // a NULL DIR* means opendir failed
    emitter.instruction("jz __rt_opendir_fail_x86");                            // bail out on an opendir failure
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the DIR* across the dirfd call

    // -- recover the underlying descriptor with dirfd --
    emitter.instruction("mov rdi, rax");                                        // DIR* argument for dirfd
    emitter.bl_c("dirfd");

    // -- record the DIR* in the fd->DIR* table for readdir/closedir --
    abi::emit_symbol_address(emitter, "r10", "_dir_handles");                   // base of the fd->DIR* table
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the DIR* handle
    emitter.instruction("mov QWORD PTR [r10 + rax * 8], r9");                   // _dir_handles[fd] = DIR*
    emitter.instruction("add rsp, 16");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the directory descriptor

    emitter.label("__rt_opendir_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 reports an opendir failure
    emitter.instruction("add rsp, 16");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
