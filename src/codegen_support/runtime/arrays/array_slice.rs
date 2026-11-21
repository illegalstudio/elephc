//! Purpose:
//! Emits the `__rt_array_slice` runtime helper assembly for array slice.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.
//! - The slice window is normalized by the shared `slice_bounds` prologue, so the copy loop always
//!   runs over a non-negative element count that lies inside the source payload.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::arrays::slice_bounds::emit_slice_bounds;
use elephc_memoir::values::packed_slice;

/// Emits the `__rt_array_slice` runtime helper for ARM64.
/// Extracts a contiguous slice from an integer array, returning a new array.
///
/// # ABI (ARM64)
/// - Input: x0 = source array pointer, x1 = `$offset`, x2 = `$length`,
///   x3 = 1 when `$length` was supplied, 0 when it was omitted or `null`
/// - Output: x0 = pointer to newly allocated sliced array
///
/// # Behavior
/// - Offset/length normalization is delegated to `emit_slice_bounds`, which applies PHP's rules:
///   negative offsets count from the end, an omitted length runs to the end, and a negative length
///   stops that many elements before the end (clamped to an empty result).
/// - Calls `__rt_array_new` to allocate the result array
pub fn emit_array_slice(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_slice_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_slice ---");
    emitter.label_global("__rt_array_slice");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer

    // -- normalize the requested slice window against PHP's offset/length rules --
    emit_slice_bounds(emitter, "__rt_array_slice");
    packed_slice::emit_copy_scalar(emitter);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = sliced array
}

/// Emits the `__rt_array_slice` runtime helper for x86_64 Linux.
///
/// Same slice semantics as the ARM64 variant; only the System V register encoding differs:
/// `rdi` = source array pointer, `rsi` = `$offset`, `rdx` = `$length`, `rcx` = 1 when a `$length`
/// was supplied and 0 when it was omitted or `null`; the sliced array is returned in `rax`.
fn emit_array_slice_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_slice ---");
    emitter.label_global("__rt_array_slice");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving scalar slice spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source indexed-array pointer, computed offset, slice length, and result pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the scalar slice bookkeeping while keeping nested constructor calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across slice-length normalization and result-array construction

    // -- normalize the requested slice window against PHP's offset/length rules --
    emit_slice_bounds(emitter, "__rt_array_slice");
    packed_slice::emit_copy_scalar(emitter);
    emitter.instruction("add rsp, 32");                                         // release the scalar slice spill slots before returning to the caller
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the scalar slice helper completes
    emitter.instruction("ret");                                                 // return the destination indexed-array pointer in rax
}
