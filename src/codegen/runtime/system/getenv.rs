use crate::codegen::emit::Emitter;

/// __rt_getenv: get environment variable value.
/// Input:  x1=name ptr, x2=name len
/// Output: x1=value ptr, x2=value len (empty string if not found)
pub fn emit_getenv(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: getenv ---");
    emitter.label_global("__rt_getenv");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set new frame pointer

    // -- null-terminate the name string --
    emitter.instruction("bl __rt_cstr");                                        // convert to C string → x0=null-terminated ptr

    // -- call libc getenv --
    emitter.instruction("bl _getenv");                                          // getenv(name) → x0=value ptr or NULL

    // -- check for NULL return --
    emitter.instruction("cbz x0, __rt_getenv_empty");                           // if NULL, return empty string

    // -- scan for null terminator to compute length --
    emitter.instruction("mov x1, x0");                                          // x1 = value ptr (start)
    emitter.instruction("mov x2, #0");                                          // x2 = length counter
    emitter.label("__rt_getenv_len");
    emitter.instruction("ldrb w9, [x0, x2]");                                   // load byte at offset x2
    emitter.instruction("cbz w9, __rt_getenv_done");                            // if null terminator, done counting
    emitter.instruction("add x2, x2, #1");                                      // increment length
    emitter.instruction("b __rt_getenv_len");                                   // continue scanning

    // -- return empty string when env var not found --
    emitter.label("__rt_getenv_empty");
    emitter.instruction("mov x1, #0");                                          // empty string ptr (null)
    emitter.instruction("mov x2, #0");                                          // empty string length = 0

    // -- clean up and return --
    emitter.label("__rt_getenv_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
