//! Purpose:
//! Emits the dependency-free libc `poll()` adapter used by the Async reactor.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::platform::emit_platform_runtime()`.
//!
//! Key details:
//! - Input entries are packed as `[fd, interest, ...]`; the helper builds
//!   transient eight-byte `pollfd` records and returns the first ready index.
//! - Return sentinels are `-1` timeout, `-2` EINTR, `-3` invalid input, and
//!   `-5` another polling failure. Any non-zero `revents`, including hangup or
//!   an invalid descriptor, is delivered as readiness for the task to inspect.

use crate::codegen_support::{
    emit::Emitter,
    platform::{Arch, Platform},
};

/// Emits `__rt_async_fd([source], operation) -> descriptor|status`.
pub fn emit_async_fd(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: async_fd ---");
    emitter.label_global("__rt_async_fd");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("push rbp");                                        // preserve caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish stable helper frame
        emitter.instruction("push r12");                                        // preserve source array across libc calls
        emitter.instruction("push r13");                                        // preserve operation across libc calls
        emitter.instruction("mov r12, rdi");                                    // retain boxed source array
        emitter.instruction("mov r13, rsi");                                    // retain ownership operation
        emitter.instruction("cmp QWORD PTR [r12], 1");                          // source array must contain one entry
        emitter.instruction("jl __rt_async_fd_invalid_x86");                    // reject an empty source array
        emitter.instruction("mov r9, QWORD PTR [r12 - 8]");                     // load indexed-array kind word
        emitter.instruction("shr r9, 8");                                       // move value type into low byte
        emitter.instruction("and r9, 0x7f");                                    // isolate stored value type
        emitter.instruction("mov rdi, QWORD PTR [r12 + 24]");                   // load raw fd or Mixed cell
        emitter.instruction("cmp r9, 7");                                       // does the source need Mixed unboxing?
        emitter.instruction("jne __rt_async_fd_dispatch_x86");                  // raw fd is ready for libc
        emitter.instruction("test rdi, rdi");                                   // validate Mixed source cell
        emitter.instruction("jz __rt_async_fd_invalid_x86");                    // reject a null source cell
        emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                    // extract native descriptor payload
        emitter.label("__rt_async_fd_dispatch_x86");
        emitter.instruction("test r13, r13");                                   // operation zero duplicates the source
        emitter.instruction("jz __rt_async_fd_dup_x86");                        // call dup for registration ownership
        emitter.instruction("cmp r13, 1");                                      // operation one releases a duplicate
        emitter.instruction("jne __rt_async_fd_invalid_x86");                   // reject unknown operations
        emitter.instruction("call close");                                      // close reactor-owned descriptor
        emitter.instruction("cdqe");                                            // sign-extend libc status
        emitter.instruction("jmp __rt_async_fd_return_x86");                    // join common epilogue
        emitter.label("__rt_async_fd_dup_x86");
        emitter.instruction("call dup");                                        // duplicate source before another task can reuse its fd
        emitter.instruction("cdqe");                                            // sign-extend returned descriptor
        emitter.instruction("jmp __rt_async_fd_return_x86");                    // join common epilogue
        emitter.label("__rt_async_fd_invalid_x86");
        emitter.instruction("mov rax, -1");                                     // invalid input or operation sentinel
        emitter.label("__rt_async_fd_return_x86");
        emitter.instruction("pop r13");                                         // restore caller's operation register
        emitter.instruction("pop r12");                                         // restore caller's source register
        emitter.instruction("pop rbp");                                         // restore caller frame pointer
        emitter.instruction("ret");                                             // return descriptor or status
        return;
    }

    emitter.instruction("sub sp, sp, #32");                                     // reserve aligned helper frame
    emitter.instruction("stp x19, x20, [sp, #0]");                              // preserve source and operation registers
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish stable helper frame
    emitter.instruction("mov x19, x0");                                         // retain boxed source array
    emitter.instruction("mov x20, x1");                                         // retain ownership operation
    emitter.instruction("ldr x9, [x19]");                                       // load source array length
    emitter.instruction("cmp x9, #1");                                          // source array must contain one entry
    emitter.instruction("b.lt __rt_async_fd_invalid");                          // reject an empty source array
    emitter.instruction("ldr x10, [x19, #-8]");                                 // load indexed-array kind word
    emitter.instruction("lsr x10, x10, #8");                                    // move value type into low byte
    emitter.instruction("and x10, x10, #0x7f");                                 // isolate stored value type
    emitter.instruction("ldr x0, [x19, #24]");                                  // load raw fd or Mixed cell
    emitter.instruction("cmp x10, #7");                                         // does the source need Mixed unboxing?
    emitter.instruction("b.ne __rt_async_fd_dispatch");                         // raw fd is ready for libc
    emitter.instruction("cbz x0, __rt_async_fd_invalid");                       // reject a null source cell
    emitter.instruction("ldr x0, [x0, #8]");                                    // extract native descriptor payload
    emitter.label("__rt_async_fd_dispatch");
    emitter.instruction("cbz x20, __rt_async_fd_dup");                          // operation zero duplicates the source
    emitter.instruction("cmp x20, #1");                                         // operation one releases a duplicate
    emitter.instruction("b.ne __rt_async_fd_invalid");                          // reject unknown operations
    emitter.bl_c("close");
    emitter.instruction("sxtw x0, w0");                                         // sign-extend libc status
    emitter.instruction("b __rt_async_fd_return");                              // join common epilogue
    emitter.label("__rt_async_fd_dup");
    emitter.bl_c("dup");
    emitter.instruction("sxtw x0, w0");                                         // sign-extend returned descriptor
    emitter.instruction("b __rt_async_fd_return");                              // join common epilogue
    emitter.label("__rt_async_fd_invalid");
    emitter.instruction("mov x0, #-1");                                         // invalid input or operation sentinel
    emitter.label("__rt_async_fd_return");
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore caller source and operation registers
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release helper frame
    emitter.instruction("ret");                                                 // return descriptor or status
}

/// Emits `__rt_async_poll(entries, timeout_ms) -> registration_index|sentinel`.
pub fn emit_async_poll(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_async_poll_x86_64(emitter);
        return;
    }

    let errno_symbol = match emitter.platform {
        Platform::MacOS => "__error",
        Platform::Linux => "__errno_location",
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    };

    emitter.blank();
    emitter.comment("--- runtime: async_poll ---");
    emitter.label_global("__rt_async_poll");
    emitter.instruction("sub sp, sp, #64");                                     // save five callee registers plus frame linkage
    emitter.instruction("stp x19, x20, [sp, #0]");                              // preserve packed entries and timeout holders
    emitter.instruction("stp x21, x22, [sp, #16]");                             // preserve count and allocated pollfd buffer
    emitter.instruction("str x23, [sp, #32]");                                  // preserve result holder
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a stable frame
    emitter.instruction("mov x19, x0");                                         // x19 = packed entries array
    emitter.instruction("mov x20, x1");                                         // x20 = timeout milliseconds
    emitter.instruction("cmp x20, #0");                                         // preserve the negative infinite-wait sentinel
    emitter.instruction("b.lt __rt_async_poll_timeout_in_range");               // negative values are already valid for poll
    emitter.instruction("mov x9, #2147483647");                                 // C poll accepts a signed 32-bit timeout
    emitter.instruction("cmp x20, x9");                                         // does the requested wait exceed INT_MAX milliseconds?
    emitter.instruction("csel x20, x20, x9, le");                               // clamp finite waits before the C ABI narrows them
    emitter.label("__rt_async_poll_timeout_in_range");
    emitter.instruction("ldr x21, [x19]");                                      // x21 = packed entry count
    emitter.instruction("tst x21, #1");                                         // every registration needs fd plus interest
    emitter.instruction("b.ne __rt_async_poll_invalid");                        // reject an odd packed array
    emitter.instruction("lsr x21, x21, #1");                                    // x21 = pollfd count
    emitter.instruction("cbz x21, __rt_async_poll_timeout");                    // no registrations are immediately timed out
    emitter.instruction("lsl x0, x21, #3");                                     // allocate count * sizeof(pollfd)
    emitter.bl_c("malloc");
    emitter.instruction("cbz x0, __rt_async_poll_invalid");                     // allocation failure
    emitter.instruction("mov x22, x0");                                         // x22 = pollfd buffer

    emitter.instruction("ldr x10, [x19, #-8]");                                 // packed indexed-array kind word
    emitter.instruction("lsr x10, x10, #8");                                    // move value type into the low byte
    emitter.instruction("and x10, x10, #0x7f");                                 // x10 = stored value type
    emitter.instruction("add x11, x19, #24");                                   // x11 = packed entry data
    emitter.instruction("mov x9, #0");                                          // x9 = registration index
    emitter.label("__rt_async_poll_build_loop");
    emitter.instruction("cmp x9, x21");                                         // built every pollfd?
    emitter.instruction("b.ge __rt_async_poll_call");                           // poll after every registration has a native record
    emitter.instruction("add x14, x11, x9, lsl #4");                            // source pair = data + index * 16
    emitter.instruction("ldr x12, [x14]");                                      // raw fd or Mixed cell
    emitter.instruction("ldr x13, [x14, #8]");                                  // raw interest or Mixed cell
    emitter.instruction("cmp x10, #7");                                         // mixed-boxed array?
    emitter.instruction("b.ne __rt_async_poll_build_store");                    // raw integer entries need no Mixed unboxing
    emitter.instruction("cbz x12, __rt_async_poll_invalid_free");               // malformed Mixed fd cell
    emitter.instruction("cbz x13, __rt_async_poll_invalid_free");               // malformed Mixed interest cell
    emitter.instruction("ldr x12, [x12, #8]");                                  // unbox fd payload
    emitter.instruction("ldr x13, [x13, #8]");                                  // unbox interest payload
    emitter.label("__rt_async_poll_build_store");
    emitter.instruction("add x15, x22, x9, lsl #3");                            // destination pollfd = buffer + index * 8
    emitter.instruction("str w12, [x15]");                                      // pollfd.fd
    emitter.instruction("strh w13, [x15, #4]");                                 // pollfd.events
    emitter.instruction("strh wzr, [x15, #6]");                                 // clear pollfd.revents
    emitter.instruction("add x9, x9, #1");                                      // next registration
    emitter.instruction("b __rt_async_poll_build_loop");                        // build the next registration record

    emitter.label("__rt_async_poll_call");
    emitter.instruction("mov x0, x22");                                         // pollfd array
    emitter.instruction("mov x1, x21");                                         // nfds
    emitter.instruction("mov x2, x20");                                         // timeout milliseconds
    emitter.bl_c("poll");
    emitter.instruction("cmp w0, #0");                                          // timeout, error, or ready count?
    emitter.instruction("b.eq __rt_async_poll_timeout_free");                   // zero ready descriptors means timeout
    emitter.instruction("b.lt __rt_async_poll_error");                          // negative result requires errno inspection
    emitter.instruction("mov x9, #0");                                          // scan in registration order
    emitter.label("__rt_async_poll_scan_loop");
    emitter.instruction("cmp x9, x21");                                         // scanned every returned record?
    emitter.instruction("b.ge __rt_async_poll_invalid_free");                   // libc reported readiness without revents
    emitter.instruction("add x10, x22, x9, lsl #3");                            // address this registration's pollfd
    emitter.instruction("ldrh w11, [x10, #6]");                                 // pollfd.revents
    emitter.instruction("cbnz w11, __rt_async_poll_ready");                     // deliver the first non-zero readiness mask
    emitter.instruction("add x9, x9, #1");                                      // inspect the next registration
    emitter.instruction("b __rt_async_poll_scan_loop");                         // continue the ordered scan

    emitter.label("__rt_async_poll_ready");
    emitter.instruction("mov x23, x9");                                         // return ready registration index
    emitter.instruction("b __rt_async_poll_free");                              // release the transient pollfd array
    emitter.label("__rt_async_poll_error");
    emitter.bl_c(errno_symbol);
    emitter.instruction("ldr w9, [x0]");                                        // errno from poll
    emitter.instruction("cmp w9, #4");                                          // EINTR is 4 on Darwin and Linux
    emitter.instruction("b.eq __rt_async_poll_interrupted_free");               // expose EINTR so the scheduler can recompute deadlines
    emitter.instruction("b __rt_async_poll_failed_free");                       // preserve a distinct hard-failure sentinel
    emitter.label("__rt_async_poll_timeout_free");
    emitter.instruction("mov x23, #-1");                                        // timeout sentinel
    emitter.instruction("b __rt_async_poll_free");                              // release the transient pollfd array
    emitter.label("__rt_async_poll_interrupted_free");
    emitter.instruction("mov x23, #-2");                                        // interrupted-wait sentinel
    emitter.instruction("b __rt_async_poll_free");                              // release the transient pollfd array
    emitter.label("__rt_async_poll_failed_free");
    emitter.instruction("mov x23, #-5");                                        // non-EINTR poll failure sentinel
    emitter.instruction("b __rt_async_poll_free");                              // release the transient pollfd array
    emitter.label("__rt_async_poll_invalid_free");
    emitter.instruction("mov x23, #-3");                                        // malformed input or allocation failure sentinel
    emitter.label("__rt_async_poll_free");
    emitter.instruction("mov x0, x22");                                         // free the temporary pollfd array
    emitter.bl_c("free");
    emitter.instruction("b __rt_async_poll_return");                            // join the no-allocation return paths
    emitter.label("__rt_async_poll_timeout");
    emitter.instruction("mov x23, #-1");                                        // empty registration set behaves as timeout
    emitter.instruction("b __rt_async_poll_return");                            // no pollfd allocation to release
    emitter.label("__rt_async_poll_invalid");
    emitter.instruction("mov x23, #-3");                                        // reject malformed packed input
    emitter.label("__rt_async_poll_return");
    emitter.instruction("mov x0, x23");                                         // publish index or sentinel result
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore caller-owned entries and timeout registers
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore caller-owned count and buffer registers
    emitter.instruction("ldr x23, [sp, #32]");                                  // restore caller's result-holder register
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return index or sentinel to EIR lowering
}

/// Emits the Linux x86_64 variant of `__rt_async_poll`.
fn emit_async_poll_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: async_poll ---");
    emitter.label_global("__rt_async_poll");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable helper frame
    emitter.instruction("push r12");                                            // packed entries, later result
    emitter.instruction("push r13");                                            // timeout milliseconds
    emitter.instruction("push rbx");                                            // pollfd count without borrowing the reserved ctx register
    emitter.instruction("push r15");                                            // allocated pollfd buffer
    emitter.instruction("mov r12, rdi");                                        // retain packed entries across libc calls
    emitter.instruction("mov r13, rsi");                                        // retain timeout across libc calls
    emitter.instruction("cmp r13, 0");                                          // preserve the negative infinite-wait sentinel
    emitter.instruction("jl __rt_async_poll_timeout_in_range_x86");             // negative values are already valid for poll
    emitter.instruction("mov r10, 2147483647");                                 // C poll accepts a signed 32-bit timeout
    emitter.instruction("cmp r13, r10");                                        // does the requested wait exceed INT_MAX milliseconds?
    emitter.instruction("cmovg r13, r10");                                      // clamp finite waits before the C ABI narrows them
    emitter.label("__rt_async_poll_timeout_in_range_x86");
    emitter.instruction("mov rbx, QWORD PTR [r12]");                            // packed entry count
    emitter.instruction("test rbx, 1");                                         // every registration needs fd plus interest
    emitter.instruction("jnz __rt_async_poll_invalid_x86");                     // reject odd packed input
    emitter.instruction("shr rbx, 1");                                          // pollfd count
    emitter.instruction("jz __rt_async_poll_timeout_x86");                      // empty input behaves as timeout

    emitter.instruction("lea rdi, [rbx * 8]");                                  // count * sizeof(pollfd)
    emitter.instruction("call malloc");                                         // allocate transient native pollfd records
    emitter.instruction("test rax, rax");                                       // did allocation succeed?
    emitter.instruction("jz __rt_async_poll_invalid_x86");                      // report allocation failure
    emitter.instruction("mov r15, rax");                                        // retain pollfd buffer across libc calls
    emitter.instruction("mov r9, QWORD PTR [r12 - 8]");                         // packed indexed-array kind word
    emitter.instruction("shr r9, 8");                                           // shift value type into low byte
    emitter.instruction("and r9, 0x7f");                                        // stored value type
    emitter.instruction("lea r10, [r12 + 24]");                                 // packed entry data
    emitter.instruction("xor r8d, r8d");                                        // registration index
    emitter.label("__rt_async_poll_build_loop_x86");
    emitter.instruction("cmp r8, rbx");                                         // built every pollfd record?
    emitter.instruction("jge __rt_async_poll_call_x86");                        // wait after construction completes
    emitter.instruction("mov rax, r8");                                         // registration index for pair addressing
    emitter.instruction("shl rax, 4");                                          // source pair byte offset
    emitter.instruction("lea rcx, [r10 + rax]");                                // address packed fd/interest pair
    emitter.instruction("mov rdx, QWORD PTR [rcx]");                            // raw fd or Mixed cell
    emitter.instruction("mov rsi, QWORD PTR [rcx + 8]");                        // raw interest or Mixed cell
    emitter.instruction("cmp r9, 7");                                           // mixed-boxed entries need payload extraction
    emitter.instruction("jne __rt_async_poll_build_store_x86");                 // raw integers are already native values
    emitter.instruction("test rdx, rdx");                                       // validate fd Mixed cell
    emitter.instruction("jz __rt_async_poll_invalid_free_x86");                 // reject null fd cell
    emitter.instruction("test rsi, rsi");                                       // validate interest Mixed cell
    emitter.instruction("jz __rt_async_poll_invalid_free_x86");                 // reject null interest cell
    emitter.instruction("mov rdx, QWORD PTR [rdx + 8]");                        // unbox fd payload
    emitter.instruction("mov rsi, QWORD PTR [rsi + 8]");                        // unbox interest payload
    emitter.label("__rt_async_poll_build_store_x86");
    emitter.instruction("lea rcx, [r15 + r8 * 8]");                             // address destination pollfd
    emitter.instruction("mov DWORD PTR [rcx], edx");                            // pollfd.fd
    emitter.instruction("mov WORD PTR [rcx + 4], si");                          // pollfd.events
    emitter.instruction("mov WORD PTR [rcx + 6], 0");                           // clear pollfd.revents
    emitter.instruction("add r8, 1");                                           // advance registration index
    emitter.instruction("jmp __rt_async_poll_build_loop_x86");                  // build the next native record

    emitter.label("__rt_async_poll_call_x86");
    emitter.instruction("mov rdi, r15");                                        // libc pollfd array argument
    emitter.instruction("mov rsi, rbx");                                        // libc nfds argument
    emitter.instruction("mov edx, r13d");                                       // libc timeout-milliseconds argument
    emitter.instruction("call poll");                                           // wait for descriptor readiness
    emitter.instruction("test eax, eax");                                       // timeout, error, or ready count?
    emitter.instruction("jz __rt_async_poll_timeout_free_x86");                 // zero ready descriptors means timeout
    emitter.instruction("js __rt_async_poll_error_x86");                        // inspect errno after a negative result
    emitter.instruction("xor r8d, r8d");                                        // scan returned records in registration order
    emitter.label("__rt_async_poll_scan_loop_x86");
    emitter.instruction("cmp r8, rbx");                                         // scanned every returned record?
    emitter.instruction("jge __rt_async_poll_invalid_free_x86");                // ready count without revents is invalid
    emitter.instruction("movzx eax, WORD PTR [r15 + r8 * 8 + 6]");              // pollfd.revents
    emitter.instruction("test eax, eax");                                       // does this record carry any readiness flag?
    emitter.instruction("jnz __rt_async_poll_ready_x86");                       // deliver the first ready registration
    emitter.instruction("add r8, 1");                                           // inspect the next registration
    emitter.instruction("jmp __rt_async_poll_scan_loop_x86");                   // continue the ordered scan

    emitter.label("__rt_async_poll_ready_x86");
    emitter.instruction("mov r12, r8");                                         // retain ready registration index across free
    emitter.instruction("jmp __rt_async_poll_free_x86");                        // release transient pollfd storage
    emitter.label("__rt_async_poll_error_x86");
    emitter.instruction("call __errno_location");                               // read libc thread-local errno
    emitter.instruction("cmp DWORD PTR [rax], 4");                              // EINTR
    emitter.instruction("je __rt_async_poll_interrupted_free_x86");             // expose EINTR for deadline recomputation
    emitter.instruction("jmp __rt_async_poll_failed_free_x86");                 // preserve hard poll failure separately
    emitter.label("__rt_async_poll_timeout_free_x86");
    emitter.instruction("mov r12, -1");                                         // timeout sentinel
    emitter.instruction("jmp __rt_async_poll_free_x86");                        // release transient pollfd storage
    emitter.label("__rt_async_poll_interrupted_free_x86");
    emitter.instruction("mov r12, -2");                                         // interrupted-wait sentinel
    emitter.instruction("jmp __rt_async_poll_free_x86");                        // release transient pollfd storage
    emitter.label("__rt_async_poll_failed_free_x86");
    emitter.instruction("mov r12, -5");                                         // non-EINTR poll failure sentinel
    emitter.instruction("jmp __rt_async_poll_free_x86");                        // release transient pollfd storage
    emitter.label("__rt_async_poll_invalid_free_x86");
    emitter.instruction("mov r12, -3");                                         // malformed input or allocation failure sentinel
    emitter.label("__rt_async_poll_free_x86");
    emitter.instruction("mov rdi, r15");                                        // free the transient pollfd array
    emitter.instruction("call free");                                           // release native scratch storage
    emitter.instruction("jmp __rt_async_poll_return_x86");                      // join no-allocation return paths
    emitter.label("__rt_async_poll_timeout_x86");
    emitter.instruction("mov r12, -1");                                         // empty registration set behaves as timeout
    emitter.instruction("jmp __rt_async_poll_return_x86");                      // no allocation to release
    emitter.label("__rt_async_poll_invalid_x86");
    emitter.instruction("mov r12, -3");                                         // reject malformed packed input
    emitter.label("__rt_async_poll_return_x86");
    emitter.instruction("mov rax, r12");                                        // publish index or sentinel result
    emitter.instruction("pop r15");                                             // restore caller's buffer register
    emitter.instruction("pop rbx");                                             // restore caller's count register
    emitter.instruction("pop r13");                                             // restore caller's timeout register
    emitter.instruction("pop r12");                                             // restore caller's entries register
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return index or sentinel to EIR lowering
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Checks AArch64 poll dispatch, ordered readiness scanning and signed timeout clamping.
    #[test]
    fn emits_aarch64_poll_adapter_with_ordered_scan() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_async_poll(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains(".globl __rt_async_poll\n"));
        assert!(asm.contains("bl _poll"));
        assert!(asm.contains("__rt_async_poll_scan_loop:"));
        assert!(asm.contains("mov x9, #2147483647"));
        assert!(asm.contains("csel x20, x20, x9, le"));
    }

    /// Checks Linux x86_64 poll dispatch, ordered scanning and its timeout clamp sequence.
    #[test]
    fn emits_linux_x86_64_poll_adapter_with_ordered_scan() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_async_poll(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains(".globl __rt_async_poll\n"));
        assert!(asm.contains("call poll"));
        assert!(asm.contains("__rt_async_poll_scan_loop_x86:"));
        assert!(asm.contains("mov r10, 2147483647"));
        assert!(asm.contains("cmovg r13, r10"));
    }
}
