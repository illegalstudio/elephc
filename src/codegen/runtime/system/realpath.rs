use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// __rt_realpath: resolve a path to its canonical absolute pathname.
/// Input:  x1=path ptr, x2=path len
/// Output: x1=resolved ptr, x2=resolved len (empty string on error)
pub(crate) fn emit_realpath(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_realpath_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: realpath ---");
    emitter.label_global("__rt_realpath");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set new frame pointer

    // -- null-terminate the path string --
    emitter.instruction("bl __rt_cstr");                                        // convert to C string → x0=null-terminated path

    // -- call libc realpath(path, NULL) --
    emitter.instruction("mov x1, #0");                                          // x1 = NULL (realpath allocates result buffer)
    emitter.bl_c("realpath");                                                    // realpath(path, NULL) → x0=malloc'd result or NULL

    // -- check for NULL return (error) --
    emitter.instruction("cbz x0, __rt_realpath_empty");                         // if NULL, return empty string (maps to false)

    // -- measure the result string length --
    emitter.instruction("mov x1, x0");                                          // x1 = result ptr (start)
    emitter.instruction("mov x2, #0");                                          // x2 = length counter
    emitter.label("__rt_realpath_len");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // load byte at offset x2
    emitter.instruction("cbz w9, __rt_realpath_done");                          // if null terminator, done counting
    emitter.instruction("add x2, x2, #1");                                      // increment length
    emitter.instruction("b __rt_realpath_len");                                 // continue scanning

    // -- return empty string on error --
    emitter.label("__rt_realpath_empty");
    emitter.instruction("mov x1, #0");                                          // empty string ptr (null)
    emitter.instruction("mov x2, #0");                                          // empty string length = 0

    // -- clean up and return --
    emitter.label("__rt_realpath_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_realpath_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: realpath ---");
    emitter.label_global("__rt_realpath");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while the realpath helper performs nested libc calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the x86_64 realpath helper

    abi::emit_call_label(emitter, "__rt_cstr");                                 // convert the path string result regs into a null-terminated C string in the scratch buffer
    emitter.instruction("mov rdi, rax");                                        // pass the null-terminated path in the SysV first-argument register
    emitter.instruction("xor esi, esi");                                        // pass NULL as the resolved_path pointer so realpath malloc()s the result
    emitter.bl_c("realpath");                                                   // realpath(path, NULL) → rax=malloc'd result or NULL

    emitter.instruction("test rax, rax");                                       // did realpath succeed?
    emitter.instruction("je __rt_realpath_empty");                              // failures map to the empty PHP string result

    emitter.instruction("mov r8, rax");                                         // preserve the start of the resolved path for the final PHP string pointer result
    emitter.instruction("mov rdx, 0");                                          // seed the returned PHP string length counter at zero bytes
    emitter.label("__rt_realpath_len");
    emitter.instruction("mov cl, BYTE PTR [r8 + rdx]");                         // load the next byte from the resolved path while measuring its length
    emitter.instruction("test cl, cl");                                         // did we reach the terminating C null byte?
    emitter.instruction("je __rt_realpath_done");                               // stop scanning once the full resolved path length is known
    emitter.instruction("add rdx, 1");                                          // advance the returned PHP string length by one byte
    emitter.instruction("jmp __rt_realpath_len");                               // continue scanning until the C string terminator is found

    emitter.label("__rt_realpath_empty");
    emitter.instruction("mov rax, 0");                                          // return empty string ptr (null) when realpath fails
    emitter.instruction("mov rdx, 0");                                          // return empty string len = 0 when realpath fails
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the empty result
    emitter.instruction("ret");                                                 // return to the caller with the empty PHP string result

    emitter.label("__rt_realpath_done");
    emitter.instruction("mov rax, r8");                                         // return the start of the resolved path as the PHP string pointer result
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the measured string result
    emitter.instruction("ret");                                                 // return to the caller with the resolved path ptr/len
}
