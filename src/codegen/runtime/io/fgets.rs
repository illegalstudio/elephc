use crate::codegen::{emit::Emitter, platform::Arch};

/// fgets: read one line from a file descriptor.
/// Input:  x0=fd
/// Output: x1=string pointer (in concat_buf), x2=string length (including \n if present)
/// Side effect: sets _eof_flags[fd] if EOF reached
pub fn emit_fgets(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fgets_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fgets ---");
    emitter.label_global("__rt_fgets");

    // -- check for invalid fd (negative = fopen failed) --
    emitter.instruction("cmp x0, #0");                                          // check if fd is negative
    emitter.instruction("b.ge __rt_fgets_fd_ok");                               // if fd >= 0, proceed normally
    emitter.instruction("mov x1, #0");                                          // return empty string: null pointer
    emitter.instruction("mov x2, #0");                                          // return empty string: zero length
    emitter.instruction("ret");                                                 // return immediately to caller

    // -- set up stack frame --
    emitter.label("__rt_fgets_fd_ok");
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- save fd and record starting position in concat_buf --
    emitter.instruction("str x0, [sp, #0]");                                    // save file descriptor on stack
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    emitter.instruction("str x10, [sp, #8]");                                   // save start offset for calculating length later
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // compute write pointer: buf + offset
    emitter.instruction("str x12, [sp, #16]");                                  // save start pointer for return value

    // -- read loop: one byte at a time until \n or EOF --
    emitter.label("__rt_fgets_loop");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x1, x11, x10");                                    // buf pointer for read syscall

    // -- read 1 byte via syscall --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd for read syscall
    emitter.instruction("mov x2, #1");                                          // read exactly 1 byte
    emitter.syscall(3);

    // -- check if read failed or returned 0 (EOF) --
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: check return value
        emitter.instruction("b.le __rt_fgets_eof");                             // if <= 0, error or EOF
    } else {
        emitter.instruction("b.cs __rt_fgets_eof");                             // macOS: if carry set, read syscall failed
        emitter.instruction("cbz x0, __rt_fgets_eof");                          // if 0 bytes read, we hit EOF
    }

    // -- advance concat_off by 1 --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, #1");                                    // advance by 1 byte
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- check if the byte we just read is \n --
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("sub x13, x10, #1");                                    // offset of byte just read
    emitter.instruction("ldrb w14, [x11, x13]");                                // load the byte we just read
    emitter.instruction("cmp w14, #0x0A");                                      // compare with newline character
    emitter.instruction("b.eq __rt_fgets_done");                                // if newline, line is complete
    emitter.instruction("b __rt_fgets_loop");                                   // otherwise continue reading

    // -- EOF reached: set eof flag for this fd --
    emitter.label("__rt_fgets_eof");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("mov w10, #1");                                         // eof marker value
    emitter.instruction("strb w10, [x9, x0]");                                  // set _eof_flags[fd] = 1

    // -- return result string --
    emitter.label("__rt_fgets_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return string start pointer
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset (end position)
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload start offset
    emitter.instruction("sub x2, x10, x11");                                    // length = current offset - start offset

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_fgets_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fgets ---");
    emitter.label_global("__rt_fgets");

    emitter.instruction("cmp rdi, 0");                                          // does fgets() have a valid non-negative file descriptor to read from?
    emitter.instruction("jge __rt_fgets_fd_ok_x86");                            // continue to the normal line-read path when the file descriptor is valid
    emitter.instruction("xor eax, eax");                                        // return an empty string pointer immediately when fopen() failed
    emitter.instruction("xor edx, edx");                                        // return an empty string length immediately when fopen() failed
    emitter.instruction("ret");                                                 // skip the line-read loop entirely for invalid file descriptors

    emitter.label("__rt_fgets_fd_ok_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while fgets() uses local spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved file descriptor and concat-buffer start metadata
    emitter.instruction("sub rsp, 32");                                         // reserve aligned stack space for the stream read loop temporaries

    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the file descriptor across the repeated libc read() calls in the fgets() loop
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer absolute offset before appending the line bytes
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the starting concat-buffer offset so the final line length can be reconstructed
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base address once for the x86_64 fgets() helper
    emitter.instruction("lea r10, [r11 + r10]");                                // compute the start pointer for the borrowed line slice that fgets() will return
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // preserve the line start pointer for the final elephc string result

    emitter.label("__rt_fgets_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // reload the current concat-buffer absolute offset before reading one more byte
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // rematerialize the concat-buffer base address for the current one-byte read destination
    emitter.instruction("lea rsi, [r11 + r10]");                                // compute the address where libc read() should append the next byte
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the tracked file descriptor as the first libc read() argument
    emitter.instruction("mov edx, 1");                                          // request exactly one byte so fgets() can stop on the first newline
    emitter.instruction("call read");                                           // read one byte from the stream through libc read() into the concat buffer
    emitter.instruction("cmp rax, 0");                                          // did libc read() append a byte into the concat buffer?
    emitter.instruction("jle __rt_fgets_eof_x86");                              // treat EOF or read failure as the end of the current line and mark the stream as exhausted

    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // reload the previous concat-buffer absolute offset before publishing the appended byte
    emitter.instruction("add r10, 1");                                          // advance the concat-buffer offset by the one byte that libc read() appended
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r10");              // publish the updated concat-buffer offset for later string appenders
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // rematerialize the concat-buffer base address so the newly appended byte can be inspected
    emitter.instruction("movzx ecx, BYTE PTR [r11 + r10 - 1]");                 // load the byte that was just appended at the new concat-buffer tail
    emitter.instruction("cmp cl, 0x0A");                                        // did the newly appended byte terminate the line with a newline?
    emitter.instruction("jne __rt_fgets_loop_x86");                             // keep reading until fgets() hits newline, EOF, or read failure

    emitter.label("__rt_fgets_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the concat-buffer start pointer for the borrowed fgets() line slice
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // reload the concat-buffer absolute end offset after the line-read loop finishes
    emitter.instruction("sub r10, QWORD PTR [rbp - 16]");                       // compute the borrowed line length from the difference between end and start offsets
    emitter.instruction("mov rdx, r10");                                        // return the borrowed line length in the x86_64 elephc string-length result register
    emitter.instruction("add rsp, 32");                                         // release the fgets() spill slots before returning the borrowed line slice
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the x86_64 fgets() helper completes
    emitter.instruction("ret");                                                 // return the borrowed concat-buffer line slice to the caller

    emitter.label("__rt_fgets_eof_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the file descriptor so the eof-flag table can mark this stream as exhausted
    emitter.instruction("lea r11, [rip + _eof_flags]");                         // materialize the eof-flag table base address for the current stream descriptor
    emitter.instruction("mov BYTE PTR [r11 + r10], 1");                         // mark the current file descriptor as EOF-reached after the zero-byte or failed read
    emitter.instruction("jmp __rt_fgets_done_x86");                             // return the possibly empty borrowed slice accumulated before EOF or read failure
}
