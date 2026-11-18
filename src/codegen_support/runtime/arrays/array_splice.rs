//! Purpose:
//! Emits the `__rt_array_splice` runtime helper assembly for array splice.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.
//! - The removal window is normalized by the shared `slice_bounds` prologue, so the removal count is
//!   always non-negative and the compaction loop never reads or writes outside the source payload.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::arrays::slice_bounds::emit_slice_bounds;
use elephc_memoir::values::array_splice_remove;

/// Emits the `__rt_array_splice` runtime helper for ARM64 and x86_64.
///
/// Removes a contiguous slice from the source array starting at `offset` containing
/// `length` elements and returns those removed elements in a new array.
///
/// ## ARM64 ABI
/// - **Input**: `x0` = source array pointer, `x1` = `$offset`, `x2` = `$length`, `x3` = 1 when a
///   `$length` was supplied and 0 when it was omitted or `null`
/// - **Output**: `x0` = new array containing the removed elements, `x1` = the normalized removal
///   offset, i.e. the index a `$replacement` is inserted at
/// - **Behavior**: The original array is modified in-place; remaining elements shift left to fill the gap.
/// - **Clamping**: `emit_slice_bounds` normalizes the window first, so the removal count is always in
///   `[0, array_length - offset]` — a negative `$length` stops that many elements before the end and
///   an over-large negative one removes nothing.
/// - **COW contract**: The result array is freshly allocated; callers receive owned storage.
/// - **Stack frame**: 48 bytes allocated; saves x29, x30 and four stack slots for array ptr / offset / length / result ptr.
pub fn emit_array_splice(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_splice_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_splice ---");
    emitter.label_global("__rt_array_splice");

    // -- set up stack frame, save arguments --
    // Stack layout:
    //   [sp, #0]  = source array pointer
    //   [sp, #8]  = offset
    //   [sp, #16] = removal length
    //   [sp, #24] = result array pointer
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer

    // -- normalize the requested removal window against PHP's offset/length rules --
    emit_slice_bounds(emitter, "__rt_array_splice");
    array_splice_remove::emit_remove_scalar(emitter);
}

/// Emits the `__rt_array_splice` runtime helper for the x86_64 Linux ABI.
///
/// Identical in behavior to the ARM64 variant but uses x86_64 calling conventions and register set.
///
/// ## x86_64 ABI
/// - **Input**: `rdi` = source array pointer, `rsi` = `$offset`, `rdx` = `$length`, `rcx` = 1 when a
///   `$length` was supplied and 0 when it was omitted or `null`
/// - **Output**: `rax` = new array containing the removed elements, `rdx` = the normalized removal
///   offset, i.e. the index a `$replacement` is inserted at
/// - **Behavior**: Same semantics as ARM64 — in-place mutation, left-shift to fill gap, clamped length.
/// - **Frame layout**: 32-byte aligned spill area at `[rbp - 8]` through `[rbp - 32]` preserves:
///   source array pointer, offset, clamped length, and result array pointer across constructor calls.
/// - **Preserves**: `rbp` across the helper; all other registers are caller-saved.
fn emit_array_splice_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_splice ---");
    emitter.label_global("__rt_array_splice");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving scalar splice spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source indexed-array pointer, normalized removal length, and result pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the scalar splice bookkeeping while keeping nested constructor calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across removal-length clamping and result-array construction

    // -- normalize the requested removal window against PHP's offset/length rules --
    emit_slice_bounds(emitter, "__rt_array_splice");
    array_splice_remove::emit_remove_scalar(emitter);
}
