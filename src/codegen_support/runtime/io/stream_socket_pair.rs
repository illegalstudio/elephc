//! Purpose:
//! Emits the `__rt_stream_socket_pair` runtime helper, which creates a pair of
//! connected sockets through the `socketpair` system call.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Both descriptors are adopted into the opaque resource registry before
//!   their handles are returned in a two-element indexed array.
//! - The builtin emitter widens those owned handles into boxed Mixed(resource)
//!   cells via `__rt_array_to_mixed`, transferring each registry reference.
//! - A `socketpair` failure yields a null pointer that the builtin boxes
//!   as PHP `false`, matching PHP's `array|false` contract for the
//!   domains the kernel refuses (typically `STREAM_PF_INET`).

use crate::codegen_support::runtime::resources::layout::STREAM_TRANSPORT_GENERIC;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Creates a connected pair of opaque stream handles.
/// Input:  AArch64 x0 = domain, x1 = type, x2 = protocol
///         x86_64  rdi = domain, rsi = type, rdx = protocol
/// Output: pointer to a two-element array of owned registry handles, or null
pub fn emit_stream_socket_pair(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_socket_pair_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_pair ---");
    emitter.label_global("__rt_stream_socket_pair");

    // Frame: [0..8) sv[2], [16) first handle, [24) second handle, [48..64) saved regs.
    emitter.instruction("sub sp, sp, #64");                                     // reserve descriptor, handle, and saved-frame storage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer

    // -- socketpair(domain, type, protocol, &sv) --
    emitter.instruction("mov x3, sp");                                          // pointer to the sv[2] descriptor pair
    emitter.syscall(135);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_ssp_ok"));        // continue when socketpair succeeded

    // -- failure: return a null pointer that the builtin boxes as PHP false --
    emitter.instruction("mov x0, #0");                                          // null pointer signals socketpair failure
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release descriptor and handle scratch storage
    emitter.instruction("ret");                                                 // return the failure sentinel

    // -- adopt both acquired descriptors before exposing PHP resources --
    emitter.label("__rt_ssp_ok");
    emitter.instruction("ldr w0, [sp, #0]");                                    // load the first acquired descriptor
    emitter.instruction("mov x1, #1");                                          // backend kind 1 closes a native descriptor
    emitter.instruction("mov x2, #1");                                          // ownership flag 1 transfers descriptor ownership
    emitter.instruction("mov x3, #0");                                          // direct sockets have no auxiliary backend owner
    emitter.instruction("bl __rt_stream_adopt_fd");                             // publish the first opaque stream handle
    emitter.instruction("cbz x0, __rt_ssp_first_adopt_fail");                   // close the unadopted peer if publication failed
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the first owned registry handle
    emitter.instruction("ldr w0, [sp, #4]");                                    // load the second acquired descriptor
    emitter.instruction("mov x1, #1");                                          // backend kind 1 closes a native descriptor
    emitter.instruction("mov x2, #1");                                          // ownership flag 1 transfers descriptor ownership
    emitter.instruction("mov x3, #0");                                          // direct sockets have no auxiliary backend owner
    emitter.instruction("bl __rt_stream_adopt_fd");                             // publish the second opaque stream handle
    emitter.instruction("cbz x0, __rt_ssp_second_adopt_fail");                  // release the first handle when its peer failed
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the second owned registry handle

    // -- name the transport: php-src calls a socket pair `generic_socket`, and nothing about the
    //    descriptors says so — both ends look like any other non-seekable socket --
    emitter.instruction("ldr x0, [sp, #16]");                                   // the first end
    emitter.instruction("mov x1, #0");                                          // a pair has no address of its own
    emitter.instruction("mov x2, #0");
    emitter.instruction(&format!("mov x3, #{}", STREAM_TRANSPORT_GENERIC));
    emitter.instruction("bl __rt_stream_record_transport");
    emitter.instruction("ldr x0, [sp, #24]");                                   // and the second
    emitter.instruction("mov x1, #0");
    emitter.instruction("mov x2, #0");
    emitter.instruction(&format!("mov x3, #{}", STREAM_TRANSPORT_GENERIC));
    emitter.instruction("bl __rt_stream_record_transport");

    // -- success: transfer both owned handles into the result array --
    emitter.instruction("mov x0, #2");                                          // result array capacity
    emitter.instruction("mov x1, #8");                                          // element size = 8 bytes
    emitter.instruction("bl __rt_array_new");                                   // allocate the result array, x0 = pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // load the first opaque handle
    emitter.instruction("bl __rt_array_push_int");                              // transfer the first owned handle into the raw array
    emitter.instruction("ldr x1, [sp, #24]");                                   // load the second opaque handle
    emitter.instruction("bl __rt_array_push_int");                              // transfer the second owned handle into the raw array
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release descriptor and handle scratch storage
    emitter.instruction("ret");                                                 // return the opaque-handle pair array

    // -- rollback after registry adoption failures --
    emitter.label("__rt_ssp_first_adopt_fail");
    emitter.instruction("ldr w0, [sp, #4]");                                    // load the peer descriptor not consumed by failed adoption
    emitter.syscall(6);
    emitter.instruction("b __rt_ssp_adopt_fail");                               // return PHP false after closing both descriptors
    emitter.label("__rt_ssp_second_adopt_fail");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the first published opaque handle
    emitter.instruction("bl __rt_resource_release");                            // close and retire the first stream after peer failure
    emitter.label("__rt_ssp_adopt_fail");
    emitter.instruction("mov x0, #0");                                          // null pointer signals registry adoption failure
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release descriptor and handle scratch storage
    emitter.instruction("ret");                                                 // return the failure sentinel
}

/// Emits the Linux x86_64 stream runtime helper for stream socket pair.
fn emit_stream_socket_pair_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_pair ---");
    emitter.label_global("__rt_stream_socket_pair");

    // Frame: [rbp-8) sv[2], [rbp-16] first handle, [rbp-24] second handle.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve descriptor and handle scratch storage

    // -- socketpair(domain, type, protocol, &sv) --
    emitter.instruction("lea r10, [rbp - 8]");                                  // pointer to the sv[2] descriptor pair
    emitter.instruction("mov eax, 53");                                         // Linux x86_64 syscall 53 = socketpair
    emitter.instruction("syscall");                                             // create the connected socket pair
    emitter.instruction("cmp rax, 0");                                          // did socketpair fail?
    emitter.instruction("jl __rt_ssp_fail_x86");                                // a negative result means failure

    // -- adopt both acquired descriptors before exposing PHP resources --
    emitter.instruction("mov edi, DWORD PTR [rbp - 8]");                        // load the first acquired descriptor
    emitter.instruction("mov esi, 1");                                          // backend kind 1 closes a native descriptor
    emitter.instruction("mov edx, 1");                                          // ownership flag 1 transfers descriptor ownership
    emitter.instruction("xor ecx, ecx");                                        // direct sockets have no auxiliary backend owner
    emitter.instruction("call __rt_stream_adopt_fd");                           // publish the first opaque stream handle
    emitter.instruction("test rax, rax");                                       // did registry publication succeed?
    emitter.instruction("jz __rt_ssp_first_adopt_fail_x86");                    // close the unadopted peer after failure
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the first owned registry handle
    emitter.instruction("mov edi, DWORD PTR [rbp - 4]");                        // load the second acquired descriptor
    emitter.instruction("mov esi, 1");                                          // backend kind 1 closes a native descriptor
    emitter.instruction("mov edx, 1");                                          // ownership flag 1 transfers descriptor ownership
    emitter.instruction("xor ecx, ecx");                                        // direct sockets have no auxiliary backend owner
    emitter.instruction("call __rt_stream_adopt_fd");                           // publish the second opaque stream handle
    emitter.instruction("test rax, rax");                                       // did peer registry publication succeed?
    emitter.instruction("jz __rt_ssp_second_adopt_fail_x86");                   // release the first handle after peer failure
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the second owned registry handle

    // See the AArch64 counterpart: php-src calls a socket pair `generic_socket`, and nothing about
    // the descriptors says so.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the first end
    emitter.instruction("xor esi, esi");                                        // a pair has no address of its own
    emitter.instruction("xor edx, edx");
    emitter.instruction(&format!("mov rcx, {}", STREAM_TRANSPORT_GENERIC));
    emitter.instruction("call __rt_stream_record_transport");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // and the second
    emitter.instruction("xor esi, esi");
    emitter.instruction("xor edx, edx");
    emitter.instruction(&format!("mov rcx, {}", STREAM_TRANSPORT_GENERIC));
    emitter.instruction("call __rt_stream_record_transport");

    // -- success: transfer both owned handles into the result array --
    emitter.instruction("mov edi, 2");                                          // result array capacity
    emitter.instruction("mov esi, 8");                                          // element size = 8 bytes
    emitter.instruction("call __rt_array_new");                                 // allocate the result array, rax = pointer
    emitter.instruction("mov rdi, rax");                                        // array pointer argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // load the first opaque handle
    emitter.instruction("call __rt_array_push_int");                            // transfer the first owned handle into the raw array
    emitter.instruction("mov rdi, rax");                                        // array pointer argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // load the second opaque handle
    emitter.instruction("call __rt_array_push_int");                            // transfer the second owned handle into the raw array
    emitter.instruction("add rsp, 32");                                         // release descriptor and handle scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the opaque-handle pair array

    // -- rollback after registry adoption failures --
    emitter.label("__rt_ssp_first_adopt_fail_x86");
    emitter.instruction("mov edi, DWORD PTR [rbp - 4]");                        // pass the peer descriptor not consumed by failed adoption
    emitter.instruction("call close");                                          // close the remaining unadopted peer descriptor
    emitter.instruction("jmp __rt_ssp_adopt_fail_x86");                         // return PHP false after closing both descriptors
    emitter.label("__rt_ssp_second_adopt_fail_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the first published opaque handle
    emitter.instruction("call __rt_resource_release");                          // close and retire the first stream after peer failure
    emitter.label("__rt_ssp_adopt_fail_x86");
    emitter.instruction("xor eax, eax");                                        // null pointer signals registry adoption failure
    emitter.instruction("add rsp, 32");                                         // release descriptor and handle scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure sentinel

    // -- failure: return a null pointer that the builtin boxes as PHP false --
    emitter.label("__rt_ssp_fail_x86");
    emitter.instruction("xor eax, eax");                                        // null pointer signals socketpair failure
    emitter.instruction("add rsp, 32");                                         // release descriptor and handle scratch storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure sentinel
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Verifies every supported target adopts both socket descriptors before returning them.
    #[test]
    fn socket_pair_returns_two_opaque_registry_handles() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_stream_socket_pair(&mut emitter);
            let asm = emitter.output();

            assert_eq!(
                asm.matches("__rt_stream_adopt_fd").count(),
                2,
                "{target:?} did not adopt both socket descriptors"
            );
            assert!(
                asm.contains("__rt_resource_release"),
                "{target:?} omitted partial-adoption rollback"
            );
            assert!(
                asm.matches("__rt_array_push_int").count() >= 2,
                "{target:?} omitted one opaque result handle"
            );
        }
    }
}
