//! Purpose:
//! Emits the `__rt_array_slice_refcounted` runtime helper assembly for array slice refcounted.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.
//! - The slice window is normalized by the shared `slice_bounds` prologue, so the retain loop always
//!   runs over a non-negative element count that lies inside the source payload.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::arrays::slice_bounds::emit_slice_bounds;
use elephc_memoir::values::packed_slice;

/// Emits the `__rt_array_slice_refcounted` runtime helper.
///
/// Extracts a slice of a refcounted PHP array into a new array. The source array
/// is never modified; the returned array owns its elements independently.
///
/// ## ABI
/// - **ARM64**: `x0` = source array pointer, `x1` = `$offset`, `x2` = `$length`, `x3` = 1 when a
///   `$length` was supplied and 0 when it was omitted or `null`. Result returned in `x0`.
/// - **x86_64 Linux**: `rdi`, `rsi`, `rdx`, `rcx` with the same meaning.
///   Result returned in `rax`.
///
/// ## Slice semantics
/// Delegated to `emit_slice_bounds`: negative offsets count backward from the end and clamp to the
/// start, an omitted `$length` slices to the end, a negative `$length` stops that many elements
/// before the end (clamped to an empty result), and a positive `$length` is clamped to the elements
/// actually available. An offset past the end yields an empty array.
pub fn emit_array_slice_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_slice_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_slice_refcounted ---");
    emitter.label_global("__rt_array_slice_refcounted");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer

    // -- normalize the requested slice window against PHP's offset/length rules --
    emit_slice_bounds(emitter, "__rt_array_slice_ref");
    packed_slice::emit_copy_refcounted(emitter);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return sliced array
}

/// Emits the x86_64 Linux variant of `__rt_array_slice_refcounted`.
/// Identical slice semantics to the ARM64 variant; only the ABI and register
/// encoding differ. Uses the System V AMD64 ABI: `rdi`, `rsi`, `rdx` for arguments,
/// `rax` for return value.
fn emit_array_slice_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_slice_refcounted ---");
    emitter.label_global("__rt_array_slice_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted slice spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source indexed-array pointer, normalized offset, clamped length, and destination array
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the refcounted slice bookkeeping while keeping helper calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across slice normalization and destination construction

    // -- normalize the requested slice window against PHP's offset/length rules --
    emit_slice_bounds(emitter, "__rt_array_slice_ref");
    packed_slice::emit_copy_refcounted(emitter);
    emitter.instruction("add rsp, 48");                                         // release the refcounted slice spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the refcounted sliced indexed-array pointer in rax
}
