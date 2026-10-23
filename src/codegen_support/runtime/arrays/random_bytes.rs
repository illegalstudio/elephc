//! Purpose:
//! Emits the `__rt_random_bytes` runtime helper assembly for PHP's `random_bytes`: an owned
//! string of bytes from the operating system's cryptographically secure generator.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - macOS fills the buffer with `arc4random_buf()`, which never fails.
//! - Linux uses the `getrandom` syscall without flags (it blocks until the pool is
//!   initialized), loops over short reads, and retries `EINTR`. Any other failure frees the
//!   buffer and is reported to the call site, which throws PHP's exception instead of falling
//!   back to a weak source.
//! - The caller has already rejected a length below 1 with PHP's `ValueError`.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Platform}};

/// Linux `EINTR`: a signal interrupted `getrandom`, which is retried.
const EINTR: i64 = 4;

/// Emits the `__rt_random_bytes` runtime helper.
///
/// ABI (AArch64): input `x0` = byte count (at least 1); output `x1`/`x2` = owned string,
/// `x0` = 0 on success or 1 when the operating system refused to provide randomness.
///
/// ABI (x86_64): input `rax` = byte count (at least 1); output `rax`/`rdx` = owned string,
/// `rcx` = 0 on success or 1 when the operating system refused to provide randomness.
pub fn emit_random_bytes(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_random_bytes_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: random_bytes ---");
    emitter.label_global("__rt_random_bytes");

    // -- frame setup and result allocation --
    emitter.instruction("sub sp, sp, #48");                                     // allocate spill space for the length, buffer and fill cursor
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save the frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the random_bytes helper frame pointer
    emitter.instruction("str x0, [sp]");                                        // save the requested byte count
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the owned result buffer, x0 = pointer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the buffer as a string in the uniform heap header
    emitter.instruction("str x0, [sp, #8]");                                    // save the result buffer pointer

    if emitter.platform == Platform::MacOS {
        // -- fill the buffer from arc4random_buf --
        emitter.instruction("ldr x1, [sp]");                                    // pass the byte count to arc4random_buf()
        emitter.bl_c("arc4random_buf");                                         // fill the buffer from the system CSPRNG (never fails)
    } else {
        // -- fill the buffer from getrandom, looping over short reads --
        emitter.instruction("str xzr, [sp, #16]");                              // no byte has been filled yet

        emitter.label("__rt_random_bytes_fill");
        emitter.instruction("ldr x10, [sp, #16]");                              // bytes filled so far
        emitter.instruction("ldr x11, [sp]");                                   // bytes requested
        emitter.instruction("cmp x10, x11");                                    // is the buffer full?
        emitter.instruction("b.ge __rt_random_bytes_done");                     // every requested byte came from the kernel
        emitter.instruction("ldr x0, [sp, #8]");                                // reload the buffer start
        emitter.instruction("add x0, x0, x10");                                 // point at the first unfilled byte
        emitter.instruction("sub x1, x11, x10");                                // ask for the bytes still missing
        emitter.instruction("mov x2, #0");                                      // no flags: block until the entropy pool is initialized
        emitter.instruction("mov x8, #278");                                    // Linux aarch64 getrandom syscall number
        emitter.instruction("svc #0");                                          // x0 = bytes written, or -errno
        emitter.instruction("cmp x0, #0");                                      // did getrandom fail?
        emitter.instruction("b.lt __rt_random_bytes_error");                    // inspect the errno
        emitter.instruction("ldr x10, [sp, #16]");                              // reload the fill cursor
        emitter.instruction("add x10, x10, x0");                                // account for a possibly short read
        emitter.instruction("str x10, [sp, #16]");                              // save the advanced fill cursor
        emitter.instruction("b __rt_random_bytes_fill");                        // request the rest

        // -- error return: retry EINTR, otherwise free the buffer and report failure --
        emitter.label("__rt_random_bytes_error");
        emitter.instruction(&format!("cmn x0, #{EINTR}"));                      // was getrandom interrupted by a signal?
        emitter.instruction("b.eq __rt_random_bytes_fill");                     // retry an interrupted request
        emitter.instruction("ldr x0, [sp, #8]");                                // reload the unfilled result buffer
        emitter.instruction("bl __rt_heap_free");                               // release it: the caller throws and never owns it
        emitter.instruction("mov x1, #0");                                      // no string result on failure
        emitter.instruction("mov x2, #0");                                      // no string result on failure
        emitter.instruction("mov x0, #1");                                      // report the failure so the caller throws
        emitter.instruction("ldp x29, x30, [sp, #32]");                         // restore the frame pointer and return address
        emitter.instruction("add sp, sp, #48");                                 // release the helper frame
        emitter.instruction("ret");                                             // return the failure flag
    }

    // -- success return: hand the filled buffer to the caller --
    emitter.label("__rt_random_bytes_done");
    emitter.instruction("ldr x1, [sp, #8]");                                    // return the filled buffer pointer
    emitter.instruction("ldr x2, [sp]");                                        // return the requested byte count as the length
    emitter.instruction("mov x0, #0");                                          // report success
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the random string
}

/// Emits `__rt_random_bytes` for x86_64 Linux.
fn emit_random_bytes_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: random_bytes ---");
    emitter.label_global("__rt_random_bytes");

    // -- frame setup and result allocation --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the spill slots
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the length, buffer and fill cursor
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the requested byte count
    emitter.instruction("call __rt_heap_alloc");                                // allocate the owned result buffer, rax = pointer
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer as a string in the uniform heap header
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the result buffer pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // no byte has been filled yet

    // -- fill the buffer from getrandom, looping over short reads --
    emitter.label("__rt_random_bytes_fill_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // bytes filled so far
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 8]");                        // is the buffer full?
    emitter.instruction("jge __rt_random_bytes_done_x86");                      // every requested byte came from the kernel
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the buffer start
    emitter.instruction("add rdi, rcx");                                        // point at the first unfilled byte
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the requested byte count
    emitter.instruction("sub rsi, rcx");                                        // ask for the bytes still missing
    emitter.instruction("xor edx, edx");                                        // no flags: block until the entropy pool is initialized
    emitter.instruction("mov eax, 318");                                        // Linux x86_64 syscall 318 = getrandom
    emitter.instruction("syscall");                                             // rax = bytes written, or -errno
    emitter.instruction("test rax, rax");                                       // did getrandom fail?
    emitter.instruction("js __rt_random_bytes_error_x86");                      // inspect the errno
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                       // account for a possibly short read
    emitter.instruction("jmp __rt_random_bytes_fill_x86");                      // request the rest

    // -- error return: retry EINTR, otherwise free the buffer and report failure --
    emitter.label("__rt_random_bytes_error_x86");
    emitter.instruction(&format!("cmp rax, -{EINTR}"));                         // was getrandom interrupted by a signal?
    emitter.instruction("je __rt_random_bytes_fill_x86");                       // retry an interrupted request
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the unfilled result buffer
    emitter.instruction("call __rt_heap_free");                                 // release it: the caller throws and never owns it
    emitter.instruction("xor eax, eax");                                        // no string result on failure
    emitter.instruction("xor edx, edx");                                        // no string result on failure
    emitter.instruction("mov ecx, 1");                                          // report the failure so the caller throws
    emitter.instruction("add rsp, 32");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure flag

    // -- success return: hand the filled buffer to the caller --
    emitter.label("__rt_random_bytes_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the filled buffer pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // return the requested byte count as the length
    emitter.instruction("xor ecx, ecx");                                        // report success
    emitter.instruction("add rsp, 32");                                         // release the spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the random string
}
