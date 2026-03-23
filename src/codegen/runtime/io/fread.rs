use crate::codegen::emit::Emitter;

/// fread: read N bytes from a file descriptor.
/// Input:  x0=fd, x1=length to read
/// Output: x1=string pointer (in concat_buf), x2=actual bytes read
pub fn emit_fread(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fread ---");
    emitter.label("__rt_fread");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer

    // -- save fd and requested length --
    emitter.instruction("str x0, [sp, #0]");                                    // save file descriptor
    emitter.instruction("str x1, [sp, #8]");                                    // save requested read length

    // -- get concat_buf write position --
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve exact address
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                          // load page address of concat buffer
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                   // resolve exact buffer base address
    emitter.instruction("add x12, x11, x10");                                   // compute write pointer: buf + offset
    emitter.instruction("str x12, [sp, #16]");                                  // save start pointer for return value

    // -- perform read syscall --
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd for read syscall
    emitter.instruction("mov x1, x12");                                         // buffer pointer for read
    emitter.instruction("ldr x2, [sp, #8]");                                    // number of bytes to read
    emitter.instruction("mov x16, #3");                                         // syscall 3 = read
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- update concat_off by actual bytes read --
    emitter.instruction("str x0, [sp, #24]");                                   // save actual bytes read
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // reload concat_off address
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve exact address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x0");                                    // advance offset by bytes read
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- set eof flag if read returned 0 --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload bytes read
    emitter.instruction("cbnz x0, __rt_fread_done");                            // if bytes > 0, skip eof flag
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload fd
    emitter.instruction("adrp x9, _eof_flags@PAGE");                            // load page address of eof flags
    emitter.instruction("add x9, x9, _eof_flags@PAGEOFF");                      // resolve exact address
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
