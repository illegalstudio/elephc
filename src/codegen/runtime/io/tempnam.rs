use crate::codegen::emit::Emitter;

/// tempnam: create a temporary filename.
/// Input:  x1/x2=dir string, x3/x4=prefix string
/// Output: x1=temp filename pointer, x2=temp filename length
pub fn emit_tempnam(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: tempnam ---");
    emitter.label_global("__rt_tempnam");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- save inputs --
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save dir ptr and len
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save prefix ptr and len

    // -- build template path: dir + "/" + prefix + "XXXXXX" in _cstr_buf --
    emitter.instruction("adrp x9, _cstr_buf@PAGE");                             // load page address of cstr buffer
    emitter.instruction("add x9, x9, _cstr_buf@PAGEOFF");                       // resolve exact buffer address
    emitter.instruction("mov x10, x9");                                         // save buffer start

    // -- copy dir bytes --
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload dir ptr and len
    emitter.label("__rt_tempnam_dir");
    emitter.instruction("cbz x2, __rt_tempnam_slash");                          // if no bytes remain, add slash
    emitter.instruction("ldrb w11, [x1], #1");                                  // load byte from dir, advance ptr
    emitter.instruction("strb w11, [x9], #1");                                  // store byte to buffer, advance ptr
    emitter.instruction("sub x2, x2, #1");                                      // decrement counter
    emitter.instruction("b __rt_tempnam_dir");                                  // continue copying

    // -- append '/' separator --
    emitter.label("__rt_tempnam_slash");
    emitter.instruction("mov w11, #0x2F");                                      // '/' character
    emitter.instruction("strb w11, [x9], #1");                                  // append slash to buffer

    // -- copy prefix bytes --
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // reload prefix ptr and len
    emitter.label("__rt_tempnam_pfx");
    emitter.instruction("cbz x2, __rt_tempnam_xx");                             // if no bytes remain, add XXXXXX
    emitter.instruction("ldrb w11, [x1], #1");                                  // load byte from prefix, advance ptr
    emitter.instruction("strb w11, [x9], #1");                                  // store byte to buffer, advance ptr
    emitter.instruction("sub x2, x2, #1");                                      // decrement counter
    emitter.instruction("b __rt_tempnam_pfx");                                  // continue copying

    // -- append "XXXXXX" template suffix --
    emitter.label("__rt_tempnam_xx");
    emitter.instruction("mov w11, #0x58");                                      // 'X' character
    emitter.instruction("strb w11, [x9], #1");                                  // append X #1
    emitter.instruction("strb w11, [x9], #1");                                  // append X #2
    emitter.instruction("strb w11, [x9], #1");                                  // append X #3
    emitter.instruction("strb w11, [x9], #1");                                  // append X #4
    emitter.instruction("strb w11, [x9], #1");                                  // append X #5
    emitter.instruction("strb w11, [x9], #1");                                  // append X #6
    emitter.instruction("strb wzr, [x9]");                                      // null-terminate the template

    // -- call mkstemp to create the temp file (modifies XXXXXX in-place) --
    emitter.instruction("str x10, [sp, #32]");                                  // save buffer start (clobbered by mkstemp)
    emitter.instruction("mov x0, x10");                                         // pass template buffer to mkstemp
    emitter.instruction("bl _mkstemp");                                         // mkstemp(template), x0=fd

    // -- close the temp file (we only need the name) --
    emitter.instruction("mov x16, #6");                                         // syscall 6 = close
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- calculate length of resulting path --
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload buffer start (was clobbered by mkstemp)
    emitter.instruction("mov x2, #0");                                          // initialize length counter
    emitter.label("__rt_tempnam_len");
    emitter.instruction("ldrb w11, [x1, x2]");                                  // load byte at current position
    emitter.instruction("cbz w11, __rt_tempnam_copy");                          // if null, done counting
    emitter.instruction("add x2, x2, #1");                                      // increment length
    emitter.instruction("b __rt_tempnam_len");                                  // continue scanning

    // -- copy result to concat_buf for safe return --
    emitter.label("__rt_tempnam_copy");
    emitter.instruction("bl __rt_str_persist");                                 // copy to heap, x1=new ptr, x2=len

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
