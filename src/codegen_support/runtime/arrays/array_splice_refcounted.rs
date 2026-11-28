//! Purpose:
//! Emits the `__rt_array_splice_refcounted` runtime helper assembly for array splice refcounted.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.
//! - The removal window is normalized by the shared `slice_bounds` prologue, so the removal count is
//!   always non-negative and the compaction loop never reads or writes outside the source payload.
//! - Removed slots transfer their existing owners into the result without extra retains.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::arrays::slice_bounds::emit_slice_bounds;
use elephc_memoir::values::array_splice_remove;

/// Emits the `__rt_array_splice_refcounted` runtime helper for array splice.
///
/// Removes a consecutive slice from a refcounted PHP array (at `offset` for `length`
/// elements), shifts the trailing elements left to close the gap, and returns a newly
/// allocated array containing the removed elements (which the caller owns).
///
/// # Arguments
/// * `emitter` - The assembly emitter (ARM64 or x86_64 based on target).
///
/// # Input registers (ARM64 calling convention)
/// * `x0` - source array pointer
/// * `x1` - `$offset` (starting position of removal, may be negative)
/// * `x2` - `$length` (may be negative)
/// * `x3` - 1 when a `$length` was supplied, 0 when it was omitted or `null`
///
/// # Output registers (ARM64 calling convention)
/// * `x0` - new array containing the removed elements (caller owns)
/// * `x1` - the normalized removal offset, i.e. the index a `$replacement` is inserted at
///
/// # ABI details
/// * `emit_slice_bounds` normalizes the offset/length pair first, so the removal count is always in
///   `[0, array_length - offset]`.
/// * Preserves source array metadata; updates the source array's logical length in-place.
/// * The caller separates the source before mutation; removed slots transfer ownership directly.
/// * Calls `__rt_array_new` once, then moves pointer slots without nested calls.
pub fn emit_array_splice_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_splice_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_splice_refcounted ---");
    emitter.label_global("__rt_array_splice_refcounted");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer

    // -- normalize the requested removal window against PHP's offset/length rules --
    emit_slice_bounds(emitter, "__rt_array_splice_ref");
    array_splice_remove::emit_remove_refcounted(emitter);
}

/// Emits the x86_64 Linux variant of `__rt_array_splice_refcounted`.
///
/// Identical in behavior to the ARM64 variant but emits x86_64 instructions using the
/// System V AMD64 ABI (registers: rdi=array, rsi=`$offset`, rdx=`$length`, rcx=1 when a `$length`
/// was supplied and 0 when it was omitted or `null`; returns the removed-elements array in rax and
/// the normalized removal offset in rdx).
///
/// The implementation mirrors the ARM64 logic: normalize the removal window, copy removed elements
/// into a new result array, shift remaining elements left in-place, and return the result array.
fn emit_array_splice_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_splice_refcounted ---");
    emitter.label_global("__rt_array_splice_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted splice spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source indexed-array pointer, clamped removal length, and removed-elements result array
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the refcounted splice bookkeeping while keeping helper calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across removal-length clamping and result-array construction

    // -- normalize the requested removal window against PHP's offset/length rules --
    emit_slice_bounds(emitter, "__rt_array_splice_ref");
    array_splice_remove::emit_remove_refcounted(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// The transfer loop neither retains removed owners nor clobbers its live loop registers.
    #[test]
    fn splice_transfers_slots_without_nested_calls_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_array_splice_refcounted(&mut emitter);
            let asm = emitter.output();
            assert!(!asm.contains("__rt_array_push_refcounted"), "{name}");
            assert!(!asm.contains("__rt_incref"), "{name}");
            assert!(asm.contains("0x7f00"), "{name}");
            let (copy, shift) = if target.arch == Arch::AArch64 {
                ("__rt_array_splice_ref_copy:", "__rt_array_splice_ref_shift:")
            } else {
                ("__rt_array_splice_ref_copy_x86:", "__rt_array_splice_ref_shift_x86:")
            };
            let body = asm.split_once(copy).unwrap().1.split_once(shift).unwrap().0;
            assert!(!body.lines().any(|line| line.trim_start().starts_with("bl ") || line.trim_start().starts_with("call ")), "{name}: {body}");
        }
    }
}
