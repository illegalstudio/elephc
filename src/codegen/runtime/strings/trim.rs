//! Purpose:
//! Emits the `__rt_trim`, `__rt_ltrim` runtime helper assembly for trim.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - Trim helpers scan byte ranges without copying unless the returned pointer/length slice changes.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits the `__rt_trim` runtime helper.
/// Delegates to `__rt_ltrim` then `__rt_rtrim` to strip leading and trailing ASCII
/// whitespace. Returns adjusted pointer/length in registers per target ABI:
/// ARM64: x1=ptr, x2=len; x86_64: rax=ptr, rdx=len. No heap allocation.
pub fn emit_trim(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_trim_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: trim ---");
    // ltrim first, then rtrim
    emitter.label_global("__rt_trim");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- delegate to ltrim then rtrim --
    emitter.instruction("bl __rt_ltrim");                                       // strip leading whitespace (adjusts x1, x2)
    emitter.instruction("bl __rt_rtrim");                                       // strip trailing whitespace (adjusts x2)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_trim`. Preserves frame pointer across the
/// call chain and forwards to `__rt_ltrim` then `__rt_rtrim`. Returns adjusted
/// pointer/length in rax/rdx per the x86_64 ELF ABI. No heap allocation.
fn emit_trim_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: trim ---");
    emitter.label_global("__rt_trim");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while trim() delegates to the x86_64 ltrim/rtrim helpers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base even though trim() only forwards the borrowed string pair
    emitter.instruction("call __rt_ltrim");                                     // strip leading whitespace from the borrowed elephc string slice first
    emitter.instruction("call __rt_rtrim");                                     // strip trailing whitespace from the borrowed elephc string slice after the left trim
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the trim helper chain completes
    emitter.instruction("ret");                                                 // return the adjusted borrowed string slice in rax/rdx
}
