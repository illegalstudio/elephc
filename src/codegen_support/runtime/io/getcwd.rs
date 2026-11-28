//! Purpose:
//! Emits the `__rt_getcwd`, `__rt_heap_alloc` runtime helper assembly for getcwd.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen_support::{emit::Emitter, platform::Arch};


/// Emits the `__rt_getcwd` runtime helper for retrieving the current working directory.
///
/// Dispatches to `emit_getcwd_linux_x86_64` on x86_64; emits a portable ARM64 implementation
/// on other targets. Both paths allocate a 1024-byte heap buffer via `__rt_heap_alloc`, call
/// the libc `getcwd()` function, scan for the null terminator to determine the string length,
/// and return the result as a pointer/length pair in the platform ABI registers.
///
/// # Arguments
/// * `emitter` - The assembly emitter used to write runtime helper instructions.
///
/// # Output (ARM64)
/// * `x1` = string pointer (heap-allocated, owned by caller)
/// * `x2` = string length
///
/// # Output (x86_64)
/// * `rax` = string pointer (heap-allocated, owned-string heap header stamped)
/// * `rdx` = string length
///
/// # Failure behavior
/// If `getcwd()` fails (returns null), the heap buffer is freed and an empty
/// string (null pointer, zero length) is returned on every target.
pub fn emit_getcwd(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_getcwd_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: getcwd ---");
    emitter.label_global("__rt_getcwd");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish new frame pointer

    // -- allocate heap buffer for path --
    elephc_memoir::interop::c_strings::emit_allocate_path_buffer(emitter, 1024);
    emitter.instruction("str x0, [sp, #0]");                                    // save buffer pointer on stack

    // -- call libc getcwd --
    emitter.instruction("mov x1, #1024");                                       // buffer size
    emitter.bl_c("getcwd");                                                    // getcwd(buf, size), x0=buf on success
    emitter.instruction("cbz x0, __rt_getcwd_fail");                            // release the buffer when libc cannot resolve the directory

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

    emitter.label("__rt_getcwd_fail");
    elephc_memoir::interop::c_strings::emit_release_failed_path_buffer(emitter, 0, 8);
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address after cleanup
    emitter.instruction("add sp, sp, #32");                                     // deallocate the failed helper frame
    emitter.instruction("ret");                                                 // return the empty string sentinel
}

/// Emits the x86_64 Linux implementation of the `__rt_getcwd` runtime helper.
///
/// Allocates a 1024-byte heap buffer, stamps it with an owned-string heap header, calls
/// the libc `getcwd()` function, scans for the null terminator to measure the string length,
/// and returns the result via `rax` (pointer) and `rdx` (length).
///
/// On failure (null return from libc `getcwd()`), the allocated buffer is freed and an
/// empty string (null pointer, zero length) is returned.
///
/// # Arguments
/// * `emitter` - The assembly emitter used to write runtime helper instructions.
///
/// # Output on success
/// * `rax` = string pointer (owned, heap-allocated with owned-string heap header)
/// * `rdx` = string length
///
/// # Output on failure
/// * `rax` = null pointer
/// * `rdx` = 0
fn emit_getcwd_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: getcwd ---");
    emitter.label_global("__rt_getcwd");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while getcwd uses local spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the owned buffer pointer and recovered length
    emitter.instruction("sub rsp, 16");                                         // reserve aligned spill slots for the owned buffer pointer and the scanned string length

    elephc_memoir::interop::c_strings::emit_allocate_path_buffer(emitter, 1024);
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the owned buffer pointer across the libc getcwd() and length-scan calls

    emitter.instruction("mov rdi, rax");                                        // pass the owned buffer pointer as the first libc getcwd() argument
    emitter.instruction("mov rsi, 1024");                                       // pass the fixed buffer capacity as the second libc getcwd() argument
    emitter.instruction("call getcwd");                                         // fill the owned buffer with the current working directory through libc getcwd()
    emitter.instruction("test rax, rax");                                       // detect libc getcwd() failure before scanning for the trailing null terminator
    emitter.instruction("jz __rt_getcwd_fail");                                 // return the empty string when libc getcwd() fails

    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the owned buffer pointer before scanning for the trailing C null terminator
    emitter.instruction("xor rdx, rdx");                                        // start the returned string length at zero before scanning the owned buffer
    emitter.label("__rt_getcwd_len");
    emitter.instruction("mov r9b, BYTE PTR [r8 + rdx]");                        // load the next byte from the owned getcwd() buffer while measuring its elephc string length
    emitter.instruction("test r9b, r9b");                                       // stop scanning once the trailing C null terminator is reached
    emitter.instruction("jz __rt_getcwd_done");                                 // finish once the owned buffer has been measured through the first null byte
    emitter.instruction("add rdx, 1");                                          // advance the elephc string length after consuming one non-null path byte
    emitter.instruction("jmp __rt_getcwd_len");                                 // continue scanning the owned buffer until the path length is fully measured

    emitter.label("__rt_getcwd_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the owned buffer pointer in the x86_64 string result register
    emitter.instruction("add rsp, 16");                                         // release the temporary spill slots used by the getcwd() helper
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the owned path string
    emitter.instruction("ret");                                                 // return the current working directory as an owned elephc string

    emitter.label("__rt_getcwd_fail");
    elephc_memoir::interop::c_strings::emit_release_failed_path_buffer(emitter, 0, 8);
    emitter.instruction("add rsp, 16");                                         // release the temporary spill slots used by the failed getcwd() path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the empty string
    emitter.instruction("ret");                                                 // return the empty string result for the failed getcwd() query
}

#[cfg(test)]
mod owner_tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target releases the owned path buffer before returning failure.
    #[test]
    fn memoir_transfer_getcwd_failure_cleans_path_buffer_on_every_target() {
        for target_name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(target_name).unwrap());
            emit_getcwd(&mut emitter);
            let asm = emitter.output();
            let failed = asm.find("__rt_getcwd_fail:").unwrap();
            let released = asm[failed..].find("__rt_heap_free_safe").unwrap() + failed;
            let returned = asm[released..].find("ret").unwrap() + released;
            assert!(failed < released && released < returned, "{target_name}: {asm}");
        }
    }
}
