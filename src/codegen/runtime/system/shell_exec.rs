use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// __rt_shell_exec: execute a shell command and capture its output.
/// Input:  x1=command ptr, x2=command len
/// Output: x1=output ptr (in concat_buf), x2=output len
pub fn emit_shell_exec(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_shell_exec_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: shell_exec ---");
    emitter.label_global("__rt_shell_exec");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set new frame pointer

    // -- null-terminate the command string --
    emitter.instruction("bl __rt_cstr");                                        // convert to C string → x0=null-terminated cmd

    // -- build "r\0" mode string on the stack --
    emitter.instruction("mov w9, #0x72");                                       // w9 = 'r' (0x72)
    emitter.instruction("strb w9, [sp, #32]");                                  // store 'r' at sp+32
    emitter.instruction("strb wzr, [sp, #33]");                                 // store null terminator at sp+33

    // -- open pipe with popen("cmd", "r") --
    emitter.instruction("add x1, sp, #32");                                     // x1 = pointer to "r\0" on stack
    emitter.bl_c("popen");                                           // popen(cmd, "r") → x0=FILE*
    emitter.instruction("str x0, [sp, #0]");                                    // save FILE* on stack

    // -- set up output buffer using concat_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_buf");
    emitter.instruction("str x9, [sp, #8]");                                    // save buffer start ptr
    emitter.instruction("mov x10, #0");                                         // x10 = bytes written counter
    emitter.instruction("str x10, [sp, #16]");                                  // save counter on stack

    // -- read loop: fgetc until EOF --
    emitter.label("__rt_shell_exec_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload FILE* from stack
    emitter.bl_c("fgetc");                                           // read one byte → w0=char or -1 (EOF)
    emitter.instruction("cmn w0, #1");                                          // compare 32-bit return with -1 (EOF)
    emitter.instruction("b.eq __rt_shell_exec_close");                          // if EOF, stop reading

    // -- store byte in concat_buf --
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload buffer start
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload bytes written count
    emitter.instruction("strb w0, [x9, x10]");                                  // store byte at buffer[count]
    emitter.instruction("add x10, x10, #1");                                    // increment byte count
    emitter.instruction("str x10, [sp, #16]");                                  // save updated count
    emitter.instruction("b __rt_shell_exec_loop");                              // continue reading

    // -- close the pipe --
    emitter.label("__rt_shell_exec_close");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload FILE* from stack
    emitter.bl_c("pclose");                                          // close the pipe

    // -- return output string --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = output buffer start
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = output length

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_shell_exec_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: shell_exec ---");
    emitter.label_global("__rt_shell_exec");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while the shell helper performs nested libc calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the x86_64 shell helper
    emitter.instruction("sub rsp, 32");                                         // reserve aligned local space for FILE*, output length, output buffer ptr, and the popen mode string

    abi::emit_call_label(emitter, "__rt_cstr");                                 // convert the command string result regs into a null-terminated C string in the scratch buffer
    emitter.instruction("mov rdi, rax");                                        // pass the null-terminated command pointer in the SysV first-argument register
    emitter.instruction("mov BYTE PTR [rbp - 32], 0x72");                       // store 'r' as the popen mode string's first byte
    emitter.instruction("mov BYTE PTR [rbp - 31], 0");                          // store the popen mode string's trailing C null terminator
    emitter.instruction("lea rsi, [rbp - 32]");                                 // pass the address of the local \"r\\0\" mode string in the SysV second-argument register
    emitter.bl_c("popen");                                                      // popen(cmd, \"r\") → rax=FILE* or NULL on failure
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the FILE* so the read loop and close path can reload it later
    emitter.instruction("test rax, rax");                                       // did popen succeed and return a readable pipe?
    emitter.instruction("je __rt_shell_exec_empty");                            // failed pipes map to the empty PHP string result

    abi::emit_symbol_address(emitter, "r8", "_concat_buf");
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // save the concat scratch buffer pointer so the read loop can append output bytes
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // initialize the shell output length counter at zero bytes

    emitter.label("__rt_shell_exec_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the FILE* for the next fgetc() probe
    emitter.bl_c("fgetc");                                                      // read one byte from the pipe → eax=byte or -1 (EOF)
    emitter.instruction("cmp eax, -1");                                         // did the pipe reach EOF?
    emitter.instruction("je __rt_shell_exec_close");                            // stop reading once the full shell output has been consumed

    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the concat scratch buffer pointer for the next output byte store
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the current shell output length before appending another byte
    emitter.instruction("mov BYTE PTR [r8 + r9], al");                          // append the freshly read shell-output byte into the concat scratch buffer
    emitter.instruction("add r9, 1");                                           // advance the shell output length after appending one more byte
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // save the updated shell output length for the next read iteration
    emitter.instruction("jmp __rt_shell_exec_loop");                            // continue reading until the pipe reports EOF

    emitter.label("__rt_shell_exec_close");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the FILE* before closing the popen pipe
    emitter.bl_c("pclose");                                                     // close the pipe after the full command output has been read
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the concat scratch buffer pointer as the PHP string pointer result
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // return the measured command output length as the PHP string length result
    emitter.instruction("add rsp, 32");                                         // release the x86_64 shell helper's aligned local frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the captured shell output
    emitter.instruction("ret");                                                 // return to the caller with the captured shell output ptr/len

    emitter.label("__rt_shell_exec_empty");
    emitter.instruction("mov rax, 0");                                          // return empty string ptr (null) when popen fails
    emitter.instruction("mov rdx, 0");                                          // return empty string len = 0 when popen fails
    emitter.instruction("add rsp, 32");                                         // release the x86_64 shell helper's aligned local frame before returning the empty result
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the empty shell result
    emitter.instruction("ret");                                                 // return to the caller with the empty PHP string result
}
