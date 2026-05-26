//! Purpose:
//! Emits the `__rt_realpath`, `__rt_cstr` runtime helper assembly for realpath.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen::{emit::Emitter, platform::Arch};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// realpath: canonicalize a path through libc realpath().
/// Input:  x1/x2 = path string
/// Output: x1/x2 = canonical path (owned heap string), or x1=0/x2=0 on failure
///
/// On failure (e.g. non-existent path) the helper returns the empty string.
/// The type signature exposes `string|false` and PHP-side `=== false`
/// comparisons rely on the empty-string sentinel as for `file_get_contents`.
pub fn emit_realpath(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_realpath_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment after preceding runtime literals
    emitter.comment("--- runtime: realpath ---");
    emitter.label_global("__rt_realpath");

    // Frame layout (32 bytes):
    //   sp+ 0  : owned heap buffer pointer
    //   sp+ 8  : reserved (alignment)
    //   sp+16  : x29 / x30
    emitter.instruction("sub sp, sp, #32");                                     // reserve frame for the saved buffer pointer plus the saved frame regs
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish new frame pointer

    // -- null-terminate the input path through __rt_cstr --
    emitter.instruction("bl __rt_cstr");                                        // convert path to null-terminated C string, x0 = cstr ptr
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the C path pointer across the heap_alloc() call

    // -- allocate a 4096-byte owned buffer for the canonical path --
    emitter.instruction("mov x0, #4096");                                       // PATH_MAX-style buffer for the canonical result
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate owned heap storage; result in x0
    emitter.instruction("mov x9, #1");                                          // heap-kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // tag the buffer as an owned string in the uniform heap header
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the owned buffer pointer across the libc call

    // -- call libc realpath(c_path, buf) --
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the C path pointer
    emitter.instruction("mov x0, x9");                                          // first argument: path
    emitter.instruction("ldr x1, [sp, #0]");                                    // second argument: owned destination buffer
    emitter.bl_c("realpath");                                                   // libc realpath(path, buf) → x0 = buf on success, NULL on failure
    emitter.instruction("cbz x0, __rt_realpath_fail");                          // libc returned NULL: report failure (empty string)

    // -- scan the owned buffer for the trailing null terminator --
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the owned buffer as the result string pointer
    emitter.instruction("mov x2, #0");                                          // initialize the length counter to 0
    emitter.label("__rt_realpath_len");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // load next byte from the owned buffer
    emitter.instruction("cbz w9, __rt_realpath_done");                          // null terminator → length found
    emitter.instruction("add x2, x2, #1");                                      // count one more non-null byte
    emitter.instruction("b __rt_realpath_len");                                 // continue scanning

    emitter.label("__rt_realpath_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the realpath() frame
    emitter.instruction("ret");                                                 // return canonical path in x1/x2

    emitter.label("__rt_realpath_fail");
    // libc returned NULL: free the unused owned buffer and return the empty string sentinel
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the owned buffer pointer for cleanup
    emitter.instruction("bl __rt_heap_free");                                   // release the unused buffer back to the heap
    emitter.instruction("mov x1, #0");                                          // return empty pointer (string|false sentinel)
    emitter.instruction("mov x2, #0");                                          // return zero length (string|false sentinel)
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address on the failure path
    emitter.instruction("add sp, sp, #32");                                     // deallocate the realpath() frame on the failure path
    emitter.instruction("ret");                                                 // return the empty-string result
}

/// x86_64 Linux-specific realpath emitter.
///
/// Mirrors the generic ARM64 realpath helper: calls `__rt_cstr` to null-terminate
/// the input path, allocates a 4096-byte owned heap buffer, invokes libc `realpath(path, buf)`,
/// scans the result for the null terminator to determine length, and returns the canonical
/// path in `rax/rdx` (pointer/length) or the empty-string sentinel (`rax=0, rdx=0`) on failure.
///
/// Frame layout uses `[rbp-8]` for the C path pointer and `[rbp-16]` for the owned buffer
/// pointer. On failure, the allocated buffer is freed via `__rt_heap_free` before returning.
fn emit_realpath_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: realpath ---");
    emitter.label_global("__rt_realpath");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while realpath uses local spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the C path pointer and the owned buffer pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots: [rbp-8]=cpath, [rbp-16]=heap

    emitter.instruction("call __rt_cstr");                                      // convert path in rax/rdx into a null-terminated C path in rax
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the C path pointer across the heap_alloc() call

    emitter.instruction("mov rax, 4096");                                       // request a 4096-byte owned buffer for the canonical path
    emitter.instruction("call __rt_heap_alloc");                                // allocate owned heap storage; pointer returned in rax
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 1)); // materialize the owned-string heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the allocated buffer as a persisted elephc string in the uniform heap header
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the owned buffer pointer across the libc realpath() call

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // first argument: C path pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // second argument: owned destination buffer
    emitter.instruction("call realpath");                                       // libc realpath(path, buf) → rax = buf on success, NULL on failure
    emitter.instruction("test rax, rax");                                       // detect libc realpath() failure
    emitter.instruction("jz __rt_realpath_fail_x86");                           // jump to the failure path when libc returns NULL

    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the owned buffer to scan for its null terminator
    emitter.instruction("xor rdx, rdx");                                        // initialize result length to zero
    emitter.label("__rt_realpath_len_x86");
    emitter.instruction("mov r9b, BYTE PTR [r8 + rdx]");                        // load next byte from the owned buffer
    emitter.instruction("test r9b, r9b");                                       // null terminator?
    emitter.instruction("jz __rt_realpath_done_x86");                           // length found
    emitter.instruction("add rdx, 1");                                          // count one more non-null byte
    emitter.instruction("jmp __rt_realpath_len_x86");                           // continue scanning

    emitter.label("__rt_realpath_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return owned buffer pointer in the x86_64 string result register
    emitter.instruction("add rsp, 32");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return canonical path in rax/rdx

    emitter.label("__rt_realpath_fail_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the owned buffer for cleanup
    emitter.instruction("call __rt_heap_free");                                 // release the unused buffer back to the heap
    emitter.instruction("xor eax, eax");                                        // return empty-string pointer (string|false sentinel)
    emitter.instruction("xor edx, edx");                                        // return empty-string length (string|false sentinel)
    emitter.instruction("add rsp, 32");                                         // release the spill slots on the failure path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer on the failure path
    emitter.instruction("ret");                                                 // return the empty-string result
}
