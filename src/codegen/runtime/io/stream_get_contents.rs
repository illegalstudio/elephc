//! Purpose:
//! Emits the `__rt_stream_get_contents` runtime helper assembly for stream_get_contents.
//! Reads every remaining byte from a file descriptor into the shared concat buffer.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - The result string is a borrowed slice of `_concat_buf`, matching `__rt_fread`;
//!   the loop reads in 4096-byte chunks until the descriptor reports EOF.

use crate::codegen::abi::emit_symbol_address;
use crate::codegen::{emit::Emitter, platform::Arch};

/// stream_get_contents: read all remaining bytes from a file descriptor.
/// Input:  x0=fd
/// Output: x1=string pointer (in concat_buf), x2=total bytes read
pub fn emit_stream_get_contents(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_get_contents_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_get_contents ---");
    emitter.label_global("__rt_stream_get_contents");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate frame for fd, start pointer, and running total
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source file descriptor

    // -- record the start of the result inside the concat buffer --
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat-buffer offset
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // compute the result start pointer
    emitter.instruction("str x12, [sp, #8]");                                   // save the result start pointer
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize the running byte total to zero

    // -- read 4096-byte chunks until EOF --
    emitter.label("__rt_stream_get_contents_loop");
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat-buffer offset
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x1, x11, x10");                                    // compute the chunk write pointer
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source file descriptor
    emitter.instruction("mov x2, #4096");                                       // read up to 4096 bytes per chunk
    emitter.syscall(3);
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative read result means failure
    }
    emitter.instruction(
        &emitter
            .platform
            .branch_on_syscall_success("__rt_stream_get_contents_read_ok"),
    ); // continue only when the read syscall succeeded
    emitter.instruction("b __rt_stream_get_contents_done");                     // stop reading after a read failure
    emitter.label("__rt_stream_get_contents_read_ok");
    emitter.instruction("cbz x0, __rt_stream_get_contents_done");               // a zero-byte read means EOF
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the concat-buffer offset
    emitter.instruction("add x10, x10, x0");                                    // advance it past the bytes just read
    emitter.instruction("str x10, [x9]");                                       // publish the updated concat-buffer offset
    emitter.instruction("ldr x10, [sp, #16]");                                  // load the running byte total
    emitter.instruction("add x10, x10, x0");                                    // add the bytes from this chunk
    emitter.instruction("str x10, [sp, #16]");                                  // store the running byte total
    emitter.instruction("b __rt_stream_get_contents_loop");                     // read the next chunk

    // -- mark EOF and return the accumulated string --
    emitter.label("__rt_stream_get_contents_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source file descriptor
    emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("mov w10, #1");                                         // EOF marker value
    emitter.instruction("strb w10, [x9, x0]");                                  // set _eof_flags[fd] = 1
    emitter.instruction("ldr x1, [sp, #8]");                                    // return the result start pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // return the total bytes read as the length
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the accumulated string slice
}

fn emit_stream_get_contents_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_get_contents ---");
    emitter.label_global("__rt_stream_get_contents");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve aligned locals for fd, start pointer, and total
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source file descriptor
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer offset
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base address
    emitter.instruction("lea rax, [r11 + r10]");                                // compute the result start pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the result start pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the running byte total to zero

    emitter.label("__rt_stream_get_contents_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer offset
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base address
    emitter.instruction("lea rsi, [r11 + r10]");                                // compute the chunk write pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the source file descriptor
    emitter.instruction("mov rdx, 4096");                                       // read up to 4096 bytes per chunk
    emitter.instruction("call read");                                           // read one chunk through libc read()
    emitter.instruction("cmp rax, 0");                                          // did read() return any bytes?
    emitter.instruction("jle __rt_stream_get_contents_done_x86");               // stop on EOF (0) or failure (negative)
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the concat-buffer offset
    emitter.instruction("add r10, rax");                                        // advance it past the bytes just read
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r10");              // publish the updated concat-buffer offset
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                       // add this chunk to the running byte total
    emitter.instruction("jmp __rt_stream_get_contents_loop_x86");               // read the next chunk

    emitter.label("__rt_stream_get_contents_done_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source file descriptor
    emitter.instruction("lea r11, [rip + _eof_flags]");                         // materialize the eof-flag table base address
    emitter.instruction("mov BYTE PTR [r11 + r10], 1");                         // mark the descriptor as EOF-reached
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the result start pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // return the total bytes read as the length
    emitter.instruction("add rsp, 32");                                         // release the local stack space
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the accumulated string slice
}
