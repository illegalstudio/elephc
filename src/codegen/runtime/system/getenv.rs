//! Purpose:
//! Emits the `__rt_getenv`, `__rt_cstr` runtime helper assembly for getenv.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - The helper converts PHP strings to C strings and returns empty pointer/length pairs for missing environment variables.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_getenv` helper for ARM64 targets (macOS/Linux).
///
/// Converts a PHP string (name ptr in x1, name len in x2) to a C string via
/// `__rt_cstr`, calls libc `getenv`, and returns the value as a PHP string
/// (ptr in x1, len in x2) or an empty string (x1=0, x2=0) when the variable
/// is not found. Preserves the PHP getenv semantics: missing env vars produce
/// an empty string result, not an error.
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

/// Emits `__rt_getenv` helper for x86_64 Linux targets.
///
/// Converts a PHP string (name in rdi via `__rt_cstr`) to a null-terminated C
/// string, calls libc `getenv`, and returns the value as a PHP string (rax=ptr,
/// rdx=len) or an empty string (rax=0, rdx=0) when not found. Uses the System V
/// AMD64 ABI for register conventions and frame layout.
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
