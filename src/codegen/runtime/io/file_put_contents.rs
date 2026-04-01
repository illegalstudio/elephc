use crate::codegen::emit::Emitter;

/// file_put_contents: write data to a file (creating/truncating).
/// Input:  x1/x2=filename string, x3/x4=data string
/// Output: x0=bytes written
pub fn emit_file_put_contents(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: file_put_contents ---");
    emitter.label_global("__rt_file_put_contents");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- save data string for after cstr call --
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save data ptr and len on stack

    // -- null-terminate the filename --
    emitter.instruction("bl __rt_cstr");                                        // convert filename to C string, x0=cstr path
    emitter.instruction("str x0, [sp, #0]");                                    // save null-terminated path pointer

    // -- open file with write+create+truncate --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload null-terminated path
    emitter.instruction("mov x1, #0x601");                                      // O_WRONLY|O_CREAT|O_TRUNC
    emitter.instruction("mov x2, #0x1A4");                                      // file mode 0644 (octal)
    emitter.instruction("mov x16, #5");                                         // syscall 5 = open
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel
    emitter.instruction("str x0, [sp, #8]");                                    // save fd on stack

    // -- write data to file --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload fd
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload data pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload data length
    emitter.instruction("mov x16, #4");                                         // syscall 4 = write
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel
    emitter.instruction("str x0, [sp, #32]");                                   // save bytes written

    // -- close the file --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload fd
    emitter.instruction("mov x16, #6");                                         // syscall 6 = close
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- return bytes written --
    emitter.instruction("ldr x0, [sp, #32]");                                   // return bytes written

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
