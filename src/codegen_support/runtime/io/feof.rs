//! Purpose:
//! Emits the `__rt_feof` runtime helper assembly for feof.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Native EOF is owned by `StreamState`; userspace wrappers still delegate
//!   to their `stream_eof` callback through the synthetic backend descriptor.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the `__rt_feof` runtime helper.
/// Dispatches to the target-specific implementation based on `emitter.target`.
/// Input: x0 = opaque stream handle
/// Output: x0 = 1 if EOF reached, 0 otherwise
pub fn emit_feof(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_feof_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: feof ---");
    emitter.label_global("__rt_feof");
    emitter.instruction("sub sp, sp, #32");                                     // preserve the stream handle and caller frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a stable helper frame
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the opaque stream handle
    emitter.instruction("bl __rt_stream_fd");                                   // resolve the backend descriptor for wrapper dispatch
    emitter.instruction("mov w9, #0x4000");                                     // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
    emitter.instruction("lsl w9, w9, #16");                                     // shift into bits 30..16 to form 0x40000000
    emitter.instruction("cmp x0, x9");                                          // is the backend below the synthetic wrapper range?
    emitter.instruction("b.lo __rt_feof_stream_state");                         // native descriptors use authoritative StreamState EOF
    // The wrapper fd range ends at the allocated handle capacity, not a
    // fixed 256: a slot beyond the bound would be misread as a native fd.
    super::emit_load_handles_cap(emitter, "x10");
    emitter.instruction("add x10, x9, x10");                                    // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
    emitter.instruction("cmp x0, x10");                                         // is the backend above the synthetic wrapper range?
    emitter.instruction("b.hs __rt_feof_stream_state");                         // non-wrapper synthetic backends use StreamState EOF
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame before wrapper tail dispatch
    emitter.instruction("add sp, sp, #32");                                     // release helper scratch storage
    emitter.instruction("b __rt_user_wrapper_feof");                            // wrapper backend delegates to stream_eof
    emitter.label("__rt_feof_stream_state");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the opaque stream handle
    emitter.instruction("bl __rt_stream_eof_get");                              // read EOF from the stable StreamState
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                     // release helper scratch storage
    emitter.instruction("ret");                                                 // return the state-owned EOF predicate
}

/// x86_64 Linux implementation of `__rt_feof`.
/// Resolves wrappers by backend descriptor and native EOF by opaque handle.
/// Input: rdi = opaque stream handle
/// Output: eax = 1 if EOF reached, 0 otherwise
fn emit_feof_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: feof ---");
    emitter.label_global("__rt_feof");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 16");                                         // reserve aligned storage for the stream handle
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the opaque stream handle
    emitter.instruction("call __rt_stream_fd");                                 // resolve the backend descriptor for wrapper dispatch
    emitter.instruction("mov r9d, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rax, r9");                                         // is the backend below the synthetic wrapper range?
    emitter.instruction("jb __rt_feof_stream_state_x86");                       // native descriptors use authoritative StreamState EOF
    // The wrapper fd range ends at the allocated handle capacity, not a
    // fixed 256: a slot beyond the bound would be misread as a native fd.
    super::emit_load_handles_cap(emitter, "r10");
    emitter.instruction("add r10, r9");                                         // wrapper range end = USER_WRAPPER_FD_BASE + handle capacity
    emitter.instruction("cmp rax, r10");                                        // is the backend above the synthetic wrapper range?
    emitter.instruction("jae __rt_feof_stream_state_x86");                      // non-wrapper synthetic backends use StreamState EOF
    emitter.instruction("mov rdi, rax");                                        // pass the synthetic backend descriptor to stream_eof
    emitter.instruction("add rsp, 16");                                         // release stream-handle storage before tail dispatch
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_user_wrapper_feof");                          // wrapper backend delegates to stream_eof
    emitter.label("__rt_feof_stream_state_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the opaque stream handle
    emitter.instruction("call __rt_stream_eof_get");                            // read EOF from the stable StreamState
    emitter.instruction("add rsp, 16");                                         // release stream-handle storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the state-owned EOF predicate
}
