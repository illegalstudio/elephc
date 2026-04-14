use crate::codegen::{emit::Emitter, platform::Arch};

/// fread: read N bytes from a file descriptor.
/// Input:  x0=fd, x1=length to read
/// Output: x1=string pointer (in concat_buf), x2=actual bytes read
pub fn emit_fread(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fread_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fread ---");
    emitter.label_global("__rt_fread");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- save fd and requested length --
    emitter.instruction("str x0, [sp, #0]");                                    // save file descriptor
    emitter.instruction("str x1, [sp, #8]");                                    // save requested read length

    // -- get concat_buf write position --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // compute write pointer: buf + offset
    emitter.instruction("str x12, [sp, #16]");                                  // save start pointer for return value

    // -- perform read syscall --
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd for read syscall
    emitter.instruction("mov x1, x12");                                         // buffer pointer for read
    emitter.instruction("ldr x2, [sp, #8]");                                    // number of bytes to read
    emitter.syscall(3);

    // -- update concat_off by actual bytes read --
    emitter.instruction("str x0, [sp, #24]");                                   // save actual bytes read
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x0");                                    // advance offset by bytes read
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- set eof flag if read returned 0 --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload bytes read
    emitter.instruction("cbnz x0, __rt_fread_done");                            // if bytes > 0, skip eof flag
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("mov w10, #1");                                         // eof marker value
    emitter.instruction("strb w10, [x9, x0]");                                  // set _eof_flags[fd] = 1

    // -- return pointer and length --
    emitter.label("__rt_fread_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return string start pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // return actual bytes read as length

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_fread_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fread ---");
    emitter.label_global("__rt_fread");

    emitter.instruction("cmp rdi, 0");                                          // does fread() have a valid non-negative file descriptor to read from?
    emitter.instruction("jge __rt_fread_fd_ok_x86");                            // continue to the normal read path when the file descriptor is valid
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer immediately when fopen() failed
    emitter.instruction("xor edx, edx");                                        // return an empty string length immediately when fopen() failed
    emitter.instruction("ret");                                                 // skip the stream read path entirely for invalid file descriptors

    emitter.label("__rt_fread_fd_ok_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while fread() uses local spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved file descriptor, length, and concat-buffer start pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned stack space for the fread() read-path temporaries

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the file descriptor across the concat-buffer address computation and libc read() call
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the requested byte count across the concat-buffer address computation and libc read() call
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer absolute offset before appending the fread() result
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base address once for the x86_64 fread() helper
    emitter.instruction("lea rax, [r11 + r10]");                                // compute the start pointer for the bytes that libc read() will append
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the concat-buffer start pointer for the final elephc string result

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the file descriptor as the first libc read() argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the concat-buffer write pointer as the second libc read() argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // pass the requested byte count as the third libc read() argument
    emitter.instruction("call read");                                           // read the requested bytes into the concat-buffer append window through libc read()
    emitter.instruction("cmp rax, 0");                                          // did libc read() append at least one byte into the concat buffer?
    emitter.instruction("jle __rt_fread_eof_x86");                              // treat EOF or read failure as an empty result and mark the stream as exhausted

    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // reload the previous concat-buffer absolute offset before publishing the fread() append
    emitter.instruction("add r10, rax");                                        // advance the concat-buffer offset by the number of bytes libc read() returned
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r10");              // publish the updated concat-buffer offset for later string appenders
    emitter.instruction("mov rdx, rax");                                        // return the successful byte count in the x86_64 elephc string-length result register
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the concat-buffer start pointer in the x86_64 elephc string-pointer result register
    emitter.instruction("add rsp, 32");                                         // release the fread() spill slots before returning the successful string slice
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the successful fread() path
    emitter.instruction("ret");                                                 // return the borrowed concat-buffer string slice to the caller

    emitter.label("__rt_fread_eof_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the file descriptor so the eof-flag table can mark this stream as exhausted
    emitter.instruction("lea r11, [rip + _eof_flags]");                         // materialize the eof-flag table base address for the current stream descriptor
    emitter.instruction("mov BYTE PTR [r11 + r10], 1");                         // mark the current file descriptor as EOF-reached after the zero-byte or failed read
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer when libc read() reports EOF or failure
    emitter.instruction("xor edx, edx");                                        // return an empty string length when libc read() reports EOF or failure
    emitter.instruction("add rsp, 32");                                         // release the fread() spill slots before returning the empty-string result
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the EOF/error fread() path
    emitter.instruction("ret");                                                 // return the empty string result for the exhausted or failed stream read
}
