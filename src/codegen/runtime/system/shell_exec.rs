use crate::codegen::emit::Emitter;

/// __rt_shell_exec: execute a shell command and capture its output.
/// Input:  x1=command ptr, x2=command len
/// Output: x1=output ptr (in concat_buf), x2=output len
pub fn emit_shell_exec(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: shell_exec ---");
    emitter.label("__rt_shell_exec");

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
    emitter.instruction("bl _popen");                                           // popen(cmd, "r") → x0=FILE*
    emitter.instruction("str x0, [sp, #0]");                                    // save FILE* on stack

    // -- set up output buffer using concat_buf --
    emitter.instruction("adrp x9, _concat_buf@PAGE");                           // load page of concat buffer
    emitter.instruction("add x9, x9, _concat_buf@PAGEOFF");                     // resolve concat buffer address
    emitter.instruction("str x9, [sp, #8]");                                    // save buffer start ptr
    emitter.instruction("mov x10, #0");                                         // x10 = bytes written counter
    emitter.instruction("str x10, [sp, #16]");                                  // save counter on stack

    // -- read loop: fgetc until EOF --
    emitter.label("__rt_shell_exec_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload FILE* from stack
    emitter.instruction("bl _fgetc");                                           // read one byte → w0=char or -1 (EOF)
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
    emitter.instruction("bl _pclose");                                          // close the pipe

    // -- return output string --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = output buffer start
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = output length

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
