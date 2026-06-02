//! Purpose:
//! Emits the `__rt_stream_copy_to_stream` runtime helper assembly for stream_copy_to_stream.
//! Copies every remaining byte from a source descriptor to a destination descriptor.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - Copies through a 2048-byte stack scratch buffer in a read/write loop until
//!   the source reports EOF; returns the total number of bytes copied.

use crate::codegen::{emit::Emitter, platform::Arch};

/// stream_copy_to_stream: copy all remaining bytes between two descriptors.
/// Input:  x0=source fd, x1=destination fd
/// Output: x0=total bytes copied
pub fn emit_stream_copy_to_stream(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_copy_to_stream_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_copy_to_stream ---");
    emitter.label_global("__rt_stream_copy_to_stream");

    // -- set up the frame: 32 bytes of locals, 16 saved registers, 2048-byte buffer --
    emitter.instruction("sub sp, sp, #2096");                                   // reserve locals, saved registers, and the copy buffer
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source file descriptor
    emitter.instruction("str x1, [sp, #8]");                                    // save the destination file descriptor
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize the copied-byte total to zero

    // -- copy 2048-byte chunks until the source reports EOF --
    emitter.label("__rt_stream_copy_to_stream_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source file descriptor
    emitter.instruction("add x1, sp, #48");                                     // point at the stack copy buffer
    emitter.instruction("mov x2, #2048");                                       // read up to 2048 bytes per chunk
    emitter.syscall(3);
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative read result means failure
    }
    emitter.instruction(
        &emitter
            .platform
            .branch_on_syscall_success("__rt_stream_copy_to_stream_read_ok"),
    ); // continue only when the read syscall succeeded
    emitter.instruction("b __rt_stream_copy_to_stream_done");                   // stop copying after a read failure
    emitter.label("__rt_stream_copy_to_stream_read_ok");
    emitter.instruction("cbz x0, __rt_stream_copy_to_stream_done");             // a zero-byte read means EOF
    emitter.instruction("str x0, [sp, #24]");                                   // save the number of bytes read this chunk

    // -- write the chunk to the destination descriptor --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the destination file descriptor
    emitter.instruction("add x1, sp, #48");                                     // point at the chunk just read
    emitter.instruction("ldr x2, [sp, #24]");                                   // write exactly the bytes that were read
    emitter.syscall(4);

    // -- accumulate the chunk size into the running total --
    emitter.instruction("ldr x10, [sp, #16]");                                  // load the running copied-byte total
    emitter.instruction("ldr x11, [sp, #24]");                                  // load this chunk's byte count
    emitter.instruction("add x10, x10, x11");                                   // add the chunk to the running total
    emitter.instruction("str x10, [sp, #16]");                                  // store the updated copied-byte total
    emitter.instruction("b __rt_stream_copy_to_stream_loop");                   // copy the next chunk

    // -- return the total number of bytes copied --
    emitter.label("__rt_stream_copy_to_stream_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the total copied-byte count
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #2096");                                   // release the stack frame
    emitter.instruction("ret");                                                 // return the copied-byte count to the caller
}

fn emit_stream_copy_to_stream_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_copy_to_stream ---");
    emitter.label_global("__rt_stream_copy_to_stream");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 2080");                                       // reserve locals and the 2048-byte copy buffer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source file descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the destination file descriptor
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the copied-byte total to zero

    emitter.label("__rt_stream_copy_to_stream_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the source file descriptor
    emitter.instruction("lea rsi, [rbp - 2080]");                               // point at the stack copy buffer
    emitter.instruction("mov rdx, 2048");                                       // read up to 2048 bytes per chunk
    emitter.instruction("call read");                                           // read one chunk through libc read()
    emitter.instruction("cmp rax, 0");                                          // did read() return any bytes?
    emitter.instruction("jle __rt_stream_copy_to_stream_done_x86");             // stop on EOF (0) or failure (negative)
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the number of bytes read this chunk

    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the destination file descriptor
    emitter.instruction("lea rsi, [rbp - 2080]");                               // point at the chunk just read
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // write exactly the bytes that were read
    emitter.instruction("call write");                                          // write the chunk through libc write()

    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // load this chunk's byte count
    emitter.instruction("add QWORD PTR [rbp - 24], r10");                       // add the chunk to the running total
    emitter.instruction("jmp __rt_stream_copy_to_stream_loop_x86");             // copy the next chunk

    emitter.label("__rt_stream_copy_to_stream_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the total copied-byte count
    emitter.instruction("add rsp, 2080");                                       // release the stack frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the copied-byte count to the caller
}
