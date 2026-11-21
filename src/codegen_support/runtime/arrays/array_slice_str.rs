//! Purpose:
//! Emits the `__rt_array_slice_str` runtime helper assembly for slicing an indexed string array.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - An indexed `array<string>` stores 16-byte `{pointer, length}` slots. The shared
//!   `__rt_array_slice` / `__rt_array_slice_refcounted` helpers copy 8 bytes per element, so they
//!   cannot carry a string pair — they were refused at compile time rather than run (issue #675).
//! - Ownership follows `__rt_array_splice_str`: a string array owns its bytes exclusively, so the
//!   copy DUPLICATES through `__rt_array_push_str` (which persists via `__rt_str_persist`) instead
//!   of aliasing the source. `array_slice()` leaves its argument untouched, so both arrays end up
//!   owning independent copies and freeing either one is safe.
//! - The window is normalized by the shared `slice_bounds` prologue, so the copy loop always runs
//!   over a non-negative count that lies inside the source payload.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::arrays::slice_bounds::emit_slice_bounds;
use elephc_memoir::values::packed_slice;

/// Emits the `__rt_array_slice_str` runtime helper.
///
/// Extracts a slice of an indexed string array into a new array. The source is never modified and
/// the result owns its own copy of every selected string.
///
/// ## ABI
/// - **ARM64**: `x0` = source array pointer, `x1` = `$offset`, `x2` = `$length`, `x3` = 1 when a
///   `$length` was supplied and 0 when it was omitted or `null`. Result returned in `x0`.
/// - **x86_64 Linux**: `rdi`, `rsi`, `rdx`, `rcx` with the same meaning. Result in `rax`.
///
/// ## Slice semantics
/// Delegated to `emit_slice_bounds`, identical to every other `array_slice` variant: negative
/// offsets count backward from the end and clamp to the start, an omitted `$length` slices to the
/// end, a negative `$length` stops that many elements before the end (clamped to empty), and a
/// positive `$length` is clamped to what is available. An offset past the end yields an empty
/// array.
pub fn emit_array_slice_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_slice_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_slice_str ---");
    emitter.label_global("__rt_array_slice_str");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer

    // -- normalize the requested slice window against PHP's offset/length rules --
    emit_slice_bounds(emitter, "__rt_array_slice_str");
    packed_slice::emit_copy_strings(emitter);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return sliced array
}

/// Emits the x86_64 Linux variant of `__rt_array_slice_str`.
///
/// Identical slice and ownership semantics to the ARM64 variant; only the ABI and register
/// encoding differ. Uses the System V AMD64 ABI: `rdi`, `rsi`, `rdx`, `rcx` for arguments and
/// `rax` for the result.
fn emit_array_slice_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_slice_str ---");
    emitter.label_global("__rt_array_slice_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving string-slice spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source array, normalized window, and destination
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots while keeping helper calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across normalization and construction

    // -- normalize the requested slice window against PHP's offset/length rules --
    emit_slice_bounds(emitter, "__rt_array_slice_str");
    packed_slice::emit_copy_strings(emitter);
    emitter.instruction("add rsp, 48");                                         // release the string-slice spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the sliced string array
}
