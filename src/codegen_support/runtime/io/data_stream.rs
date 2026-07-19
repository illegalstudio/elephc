//! Purpose:
//! Emits the `__rt_data_stream` runtime helper, which materializes a `data://`
//! URI payload as a readable stream descriptor.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - The decoded payload is produced at compile time; this helper only copies
//!   those bytes into an anonymous temp file (via `__rt_tmpfile`) and rewinds
//!   it, so the result behaves like any other read-positioned stream fd.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// data_stream: build a readable stream over a decoded `data://` payload.
/// Input:  AArch64 x0 = payload pointer, x1 = payload length
///         x86_64  rdi = payload pointer, rsi = payload length
/// Output: file descriptor positioned at offset 0, or -1 on failure.
pub fn emit_data_stream(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_data_stream_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: data_stream ---");
    emitter.label_global("__rt_data_stream");

    // Frame (48 bytes): [0]=payload ptr [8]=payload len [16]=fd [32]=x29 [40]=x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the decoded payload pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the decoded payload length

    // -- create the anonymous backing descriptor --
    emitter.instruction("bl __rt_tmpfile");                                     // create an unlinked temp file, x0 = fd
    emitter.instruction("cmp x0, #0");                                          // did tmpfile fail to provide a descriptor?
    emitter.instruction("b.lt __rt_data_stream_fail");                          // propagate the failure sentinel
    emitter.instruction("str x0, [sp, #16]");                                   // save the backing descriptor

    // -- write(fd, payload, length) --
    emitter.instruction("ldr x1, [sp, #0]");                                    // payload pointer for the write
    emitter.instruction("ldr x2, [sp, #8]");                                    // payload length for the write
    emitter.syscall(4);

    // -- lseek(fd, 0, SEEK_SET): rewind so the stream reads from the start --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the backing descriptor
    emitter.instruction("mov x1, #0");                                          // offset = 0
    emitter.instruction("mov x2, #0");                                          // whence = SEEK_SET
    emitter.syscall(199);

    emitter.instruction("ldr x0, [sp, #16]");                                   // return the rewound descriptor
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the data:// stream descriptor

    emitter.label("__rt_data_stream_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 signals a failed data:// stream
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for data stream.
fn emit_data_stream_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: data_stream ---");
    emitter.label_global("__rt_data_stream");

    // Frame (rbp-relative): [-8]=payload ptr [-16]=payload len [-24]=fd
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve the payload and descriptor spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the decoded payload pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the decoded payload length

    // -- create the anonymous backing descriptor --
    emitter.instruction("call __rt_tmpfile");                                   // create an unlinked temp file, rax = fd
    emitter.instruction("cmp rax, 0");                                          // did tmpfile fail to provide a descriptor?
    emitter.instruction("jl __rt_data_stream_fail_x86");                        // propagate the failure sentinel
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the backing descriptor

    // -- write(fd, payload, length) --
    emitter.instruction("mov rdi, rax");                                        // descriptor for the write
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // payload pointer for the write
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // payload length for the write
    emitter.instruction("call write");                                          // copy the payload into the backing file

    // -- lseek(fd, 0, SEEK_SET): rewind so the stream reads from the start --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the backing descriptor
    emitter.instruction("xor esi, esi");                                        // offset = 0
    emitter.instruction("xor edx, edx");                                        // whence = SEEK_SET
    emitter.instruction("call lseek");                                          // rewind the backing file

    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the rewound descriptor
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the data:// stream descriptor

    emitter.label("__rt_data_stream_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 signals a failed data:// stream
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
