use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// __rt_getenv: get environment variable value.
/// Input:  x1=name ptr, x2=name len
/// Output: x1=value ptr, x2=value len (empty string if not found)
pub fn emit_getenv(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_getenv_linux_x86_64(emitter);
        return;
    }

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
    emitter.bl_c("getenv");                                          // getenv(name) → x0=value ptr or NULL

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

fn emit_getenv_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: getenv ---");
    emitter.label_global("__rt_getenv");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while the getenv helper performs nested libc calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the x86_64 getenv helper

    abi::emit_call_label(emitter, "__rt_cstr");                                 // convert the elephc string result regs into a null-terminated C string in the scratch buffer
    emitter.instruction("mov rdi, rax");                                        // pass the null-terminated environment variable name in the SysV first-argument register
    emitter.bl_c("getenv");                                                     // getenv(name) → rax=value ptr or NULL

    emitter.instruction("test rax, rax");                                       // did libc return a real environment-value pointer?
    emitter.instruction("je __rt_getenv_empty");                                // missing environment variables map to the empty PHP string

    emitter.instruction("mov r8, rax");                                         // preserve the start of the returned environment string for the final PHP string pointer result
    emitter.instruction("mov rdx, 0");                                          // seed the returned PHP string length counter at zero bytes
    emitter.label("__rt_getenv_len");
    emitter.instruction("mov cl, BYTE PTR [r8 + rdx]");                         // load the next byte from the returned C string while measuring its length
    emitter.instruction("test cl, cl");                                         // did we reach the terminating C null byte?
    emitter.instruction("je __rt_getenv_done");                                 // stop scanning once the full environment string length is known
    emitter.instruction("add rdx, 1");                                          // advance the returned PHP string length by one byte
    emitter.instruction("jmp __rt_getenv_len");                                 // continue scanning until the C string terminator is found

    emitter.label("__rt_getenv_empty");
    emitter.instruction("mov rax, 0");                                          // return empty string ptr (null) when the environment variable is missing
    emitter.instruction("mov rdx, 0");                                          // return empty string len = 0 when the environment variable is missing
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the empty-string result
    emitter.instruction("ret");                                                 // return to the caller with the empty PHP string result

    emitter.label("__rt_getenv_done");
    emitter.instruction("mov rax, r8");                                         // return the start of the environment string as the PHP string pointer result
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the measured string result
    emitter.instruction("ret");                                                 // return to the caller with the environment string ptr/len in the x86_64 result regs
}
