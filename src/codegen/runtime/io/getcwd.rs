use crate::codegen::emit::Emitter;

/// getcwd: get the current working directory.
/// Input:  none
/// Output: x1=string pointer, x2=string length
pub fn emit_getcwd(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: getcwd ---");
    emitter.label_global("__rt_getcwd");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish new frame pointer

    // -- allocate heap buffer for path --
    emitter.instruction("mov x0, #1024");                                       // request 1024 bytes for path buffer
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate, x0=buffer pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save buffer pointer on stack

    // -- call libc getcwd --
    emitter.instruction("mov x1, #1024");                                       // buffer size
    emitter.instruction("bl _getcwd");                                          // getcwd(buf, size), x0=buf on success

    // -- calculate string length by scanning for null --
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload buffer pointer as string start
    emitter.instruction("mov x2, #0");                                          // initialize length counter
    emitter.label("__rt_getcwd_len");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // load byte at current position
    emitter.instruction("cbz w9, __rt_getcwd_done");                            // if null terminator, length is complete
    emitter.instruction("add x2, x2, #1");                                      // increment length counter
    emitter.instruction("b __rt_getcwd_len");                                   // continue scanning

    // -- return string pointer and length --
    emitter.label("__rt_getcwd_done");

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
