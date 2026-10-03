//! Purpose:
//! Emits the `__rt_feof` runtime helper assembly for feof.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Platform}};
use crate::codegen_support::abi;

/// Emits the `__rt_feof` runtime helper.
/// Dispatches to the target-specific implementation based on `emitter.target`.
/// Input: x0 = file descriptor number
/// Output: x0 = 1 if EOF reached, 0 otherwise
pub fn emit_feof(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_feof_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: feof ---");
    emitter.label_global("__rt_feof");

    // -- user-wrapper synthetic fd path (Phase 10 step 4) --
    emitter.instruction("mov w9, #0x4000");                                     // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
    emitter.instruction("lsl w9, w9, #16");                                     // shift into bits 30..16 to form 0x40000000
    emitter.instruction("cmp x0, x9");                                          // is this a synthetic user-wrapper fd?
    emitter.instruction("b.lt __rt_feof_local_fd");                             // not a wrapper fd → read the local eof-flag table
    emitter.instruction("b __rt_user_wrapper_feof");                            // wrapper fd: tail-call stream_eof (uncond → cross-atom safe)
    emitter.label("__rt_feof_local_fd");

    // -- load eof flag for this fd from _eof_flags array --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("ldrb w0, [x9, x0]");                                   // load _eof_flags[fd] into return register
    emitter.instruction("ret");                                                 // return to caller
}

/// x86_64 Linux implementation of `__rt_feof`.
/// Loads the EOF flag byte from the `_eof_flags` array for the given file descriptor.
/// Input: rdi = file descriptor number
/// Output: eax = 1 if EOF reached, 0 otherwise
fn emit_feof_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: feof ---");
    emitter.label_global("__rt_feof");

    // -- user-wrapper synthetic fd path (Phase 10 step 4) --
    emitter.instruction("mov r9d, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rdi, r9");                                         // is this a synthetic user-wrapper fd?
    emitter.instruction("jge __rt_user_wrapper_feof");                          // dispatch into the wrapper's stream_eof instead of reading the eof-flag table

    if emitter.target.platform == Platform::Windows {
        emitter.instruction("call __rt_win_stream_slot");                       // map opaque SOCKET/CRT descriptor to a bounded stream-state slot
        emitter.instruction("mov rdi, rax");                                    // use only the compact slot for fixed-table addressing
    }
    abi::emit_symbol_address(emitter, "r10", "_eof_flags");                     // materialize the eof-flag table base address for the queried stream slot
    emitter.instruction("movzx eax, BYTE PTR [r10 + rdi]");                     // load the tracked eof flag byte without indexing by a raw Windows handle
    emitter.instruction("ret");                                                 // return the eof flag to the caller using the standard x86_64 integer result register
}
