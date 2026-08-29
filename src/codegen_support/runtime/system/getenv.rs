//! Purpose:
//! Emits the `__rt_getenv`, `__rt_cstr` runtime helper assembly for getenv.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - The helper converts PHP strings to C strings. A MISSING variable returns a
//!   null pointer and a PRESENT one returns an owned heap copy of its value —
//!   the two are different answers, and only a null pointer means "not set".
//!   libc hands back a pointer into the environment block, which the caller must
//!   not own, so the found path persists before returning.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_getenv` helper for ARM64 targets (macOS/Linux).
///
/// Converts a PHP string (name ptr in x1, name len in x2) to a C string via
/// `__rt_cstr`, calls libc `getenv`, and returns the value as an owned PHP
/// string (ptr in x1, len in x2), or a null pointer (x1=0, x2=0) when the
/// variable is not set.
///
/// The null pointer is the whole point: PHP's `getenv` answers `false` for a
/// variable that is not set and `""` for one set to the empty string, and those
/// are the two cases the caller has to tell apart. libc already distinguishes
/// them — a missing name gives NULL, an empty value gives a valid pointer to a
/// zero-length string — so the information is here; it is `__rt_str_persist`
/// that carries it, since it gives a zero-length string an owned block rather
/// than a null one.
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
    emitter.instruction("cbz x0, __rt_getenv_unset");                           // libc says NULL only for a name that is not set

    // -- scan for null terminator to compute length --
    emitter.instruction("mov x1, x0");                                          // x1 = value ptr (start)
    emitter.instruction("mov x2, #0");                                          // x2 = length counter
    emitter.label("__rt_getenv_len");
    emitter.instruction("ldrb w9, [x0, x2]");                                   // load byte at offset x2
    emitter.instruction("cbz w9, __rt_getenv_persist");                         // if null terminator, done counting
    emitter.instruction("add x2, x2, #1");                                      // increment length
    emitter.instruction("b __rt_getenv_len");                                   // continue scanning

    // -- copy the value out of the environment block, which the caller must not own --
    //
    // The copy is required by the CONTRACT, not by an observed crash: the result
    // is boxed as an OWNED string, and anything that later classifies that
    // payload reads its heap header at `[ptr - 8]` — eight bytes before a
    // string the allocator never handed out. Removing this does not fail any
    // test on a host where a foreign free is range-rejected, which is exactly
    // why the reason is written down here instead.
    emitter.label("__rt_getenv_persist");
    emitter.instruction("bl __rt_str_persist");                                 // x1/x2 = owned heap copy, non-null even at length zero
    emitter.instruction("b __rt_getenv_done");                                  // skip the not-found path after persisting a real value

    // -- a null pointer, not an empty string: the variable is NOT SET --
    emitter.label("__rt_getenv_unset");
    emitter.instruction("mov x1, #0");                                          // null pointer: the caller boxes this as PHP false
    emitter.instruction("mov x2, #0");                                          // no length to report for a variable that is not set

    // -- clean up and return --
    emitter.label("__rt_getenv_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_getenv` helper for x86_64 Linux targets.
///
/// Converts a PHP string (name in rdi via `__rt_cstr`) to a null-terminated C
/// string, calls libc `getenv`, and returns the value as an owned PHP string
/// (rax=ptr, rdx=len), or a null pointer (rax=0, rdx=0) when the variable is
/// not set — the same two answers as the ARM64 helper above, and for the same
/// reason. Uses the System V AMD64 ABI for register conventions and frame
/// layout.
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
    emitter.instruction("je __rt_getenv_unset");                                // a name that is not set is not a name set to ""

    emitter.instruction("mov r8, rax");                                         // preserve the start of the returned environment string for the final PHP string pointer result
    emitter.instruction("mov rdx, 0");                                          // seed the returned PHP string length counter at zero bytes
    emitter.label("__rt_getenv_len");
    emitter.instruction("mov cl, BYTE PTR [r8 + rdx]");                         // load the next byte from the returned C string while measuring its length
    emitter.instruction("test cl, cl");                                         // did we reach the terminating C null byte?
    emitter.instruction("je __rt_getenv_done");                                 // stop scanning once the full environment string length is known; the done path persists
    emitter.instruction("add rdx, 1");                                          // advance the returned PHP string length by one byte
    emitter.instruction("jmp __rt_getenv_len");                                 // continue scanning until the C string terminator is found

    emitter.label("__rt_getenv_unset");
    emitter.instruction("mov rax, 0");                                          // null pointer: the caller boxes this as PHP false
    emitter.instruction("mov rdx, 0");                                          // no length to report for a variable that is not set
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the not-set answer
    emitter.instruction("ret");                                                 // return the null pointer the caller boxes as PHP false

    emitter.label("__rt_getenv_done");
    emitter.instruction("mov rax, r8");                                         // move the environment-block pointer into str_persist's source register
    abi::emit_call_label(emitter, "__rt_str_persist");                          // rax/rdx = owned heap copy, non-null even at length zero
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the measured string result
    emitter.instruction("ret");                                                 // return to the caller with the owned string ptr/len in the x86_64 result regs
}
