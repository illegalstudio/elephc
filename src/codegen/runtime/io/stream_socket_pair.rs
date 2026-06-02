//! Purpose:
//! Emits the `__rt_stream_socket_pair` runtime helper, which creates a pair of
//! connected sockets through the `socketpair` system call.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - The two descriptors are returned as a two-element indexed array built
//!   with `__rt_array_new` / `__rt_array_push_int`. The builtin emitter
//!   then widens those int slots into boxed Mixed(resource) cells via
//!   `__rt_array_to_mixed` so callers see a uniform Mixed-of-Mixed array.
//! - A `socketpair` failure yields a null pointer that the builtin boxes
//!   as PHP `false`, matching PHP's `array|false` contract for the
//!   domains the kernel refuses (typically `STREAM_PF_INET`).

use crate::codegen::{emit::Emitter, platform::Arch};

/// stream_socket_pair: create a connected pair of socket descriptors.
/// Input:  AArch64 x0 = domain, x1 = type, x2 = protocol
///         x86_64  rdi = domain, rsi = type, rdx = protocol
/// Output: pointer to a two-element array of socket descriptors
pub fn emit_stream_socket_pair(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_socket_pair_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_pair ---");
    emitter.label_global("__rt_stream_socket_pair");

    // Frame: [0..16) saved regs, [16) the two-descriptor sv[2] output.
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- socketpair(domain, type, protocol, &sv) --
    emitter.instruction("add x3, sp, #16");                                     // pointer to the sv[2] descriptor pair
    emitter.syscall(135);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_ssp_ok"));        // continue when socketpair succeeded

    // -- failure: return a null pointer that the builtin boxes as PHP false --
    emitter.instruction("mov x0, #0");                                          // null pointer signals socketpair failure
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the failure sentinel

    // -- success: build a two-element descriptor array --
    emitter.label("__rt_ssp_ok");
    emitter.instruction("mov x0, #2");                                          // result array capacity
    emitter.instruction("mov x1, #8");                                          // element size = 8 bytes
    emitter.instruction("bl __rt_array_new");                                   // allocate the result array, x0 = pointer
    emitter.instruction("ldr w1, [sp, #16]");                                   // sv[0] = first socket descriptor
    emitter.instruction("bl __rt_array_push_int");                              // push the first descriptor, x0 = array
    emitter.instruction("ldr w1, [sp, #20]");                                   // sv[1] = second socket descriptor
    emitter.instruction("bl __rt_array_push_int");                              // push the second descriptor, x0 = array
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the descriptor-pair array
}

fn emit_stream_socket_pair_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_pair ---");
    emitter.label_global("__rt_stream_socket_pair");

    // Frame: [rbp-16) the two-descriptor sv[2] output.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve the sv[2] descriptor pair

    // -- socketpair(domain, type, protocol, &sv) --
    emitter.instruction("lea r10, [rbp - 16]");                                 // pointer to the sv[2] descriptor pair
    emitter.instruction("mov eax, 53");                                         // Linux x86_64 syscall 53 = socketpair
    emitter.instruction("syscall");                                             // create the connected socket pair
    emitter.instruction("cmp rax, 0");                                          // did socketpair fail?
    emitter.instruction("jl __rt_ssp_fail_x86");                                // a negative result means failure

    // -- success: build a two-element descriptor array --
    emitter.instruction("mov edi, 2");                                          // result array capacity
    emitter.instruction("mov esi, 8");                                          // element size = 8 bytes
    emitter.instruction("call __rt_array_new");                                 // allocate the result array, rax = pointer
    emitter.instruction("mov rdi, rax");                                        // array pointer argument
    emitter.instruction("mov esi, DWORD PTR [rbp - 16]");                       // sv[0] = first socket descriptor
    emitter.instruction("call __rt_array_push_int");                            // push the first descriptor, rax = array
    emitter.instruction("mov rdi, rax");                                        // array pointer argument
    emitter.instruction("mov esi, DWORD PTR [rbp - 12]");                       // sv[1] = second socket descriptor
    emitter.instruction("call __rt_array_push_int");                            // push the second descriptor, rax = array
    emitter.instruction("add rsp, 16");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the descriptor-pair array

    // -- failure: return a null pointer that the builtin boxes as PHP false --
    emitter.label("__rt_ssp_fail_x86");
    emitter.instruction("xor eax, eax");                                        // null pointer signals socketpair failure
    emitter.instruction("add rsp, 16");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure sentinel
}
