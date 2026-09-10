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
    emitter.instruction("str x1, [sp, #8]");                                    // save normalized offset
    emitter.instruction("str x2, [sp, #16]");                                   // save clamped removal length

    // -- create result array for removed elements --
    emitter.instruction("mov x0, x2");                                          // use removal length as result capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate result array
    emitter.instruction("str x0, [sp, #24]");                                   // save result array pointer
    emitter.instruction("ldr x9, [sp, #0]");                                    // load the separated source container
    emitter.instruction("ldur x9, [x9, #-8]");                                  // read its element metadata
    emitter.instruction("and x9, x9, #0x7f00");                                 // exclude the persistent COW flag from the new result
    emitter.instruction("ldur x10, [x0, #-8]");                                 // preserve the destination heap metadata
    emitter.instruction("orr x10, x10, x9");                                    // stamp the transferred element type
    emitter.instruction("stur x10, [x0, #-8]");                                 // publish result element ownership
    emitter.instruction("ldr x9, [sp, #16]");                                   // load the exact removal count
    emitter.instruction("str x9, [x0]");                                        // set result length before the non-calling copy loop

    // -- transfer removed element owners into the preallocated result --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x5, x0, #24");                                     // compute source data base
    emitter.instruction("ldr x6, [sp, #8]");                                    // reload offset
    emitter.instruction("ldr x7, [sp, #16]");                                   // reload removal length
    emitter.instruction("mov x8, #0");                                          // initialize copy-loop index
    emitter.label("__rt_array_splice_ref_copy");
    emitter.instruction("cmp x8, x7");                                          // compare copy index with removal length
    emitter.instruction("b.ge __rt_array_splice_ref_shift");                    // move on to in-place shifting after copying removed elements
    emitter.instruction("add x9, x6, x8");                                      // compute source index = offset + copy index
    emitter.instruction("ldr x1, [x5, x9, lsl #3]");                            // load the source-owned slot for transfer
    emitter.instruction("ldr x10, [sp, #24]");                                  // load the preallocated result container
    emitter.instruction("add x10, x10, #24");                                   // address its pointer slots
    emitter.instruction("str x1, [x10, x8, lsl #3]");                           // transfer the removed owner without incrementing its refcount
    emitter.instruction("add x8, x8, #1");                                      // increment copy-loop index
    emitter.instruction("b __rt_array_splice_ref_copy");                        // continue copying removed elements

    // -- shift remaining elements left inside the source array --
    emitter.label("__rt_array_splice_ref_shift");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // reload original source length
    emitter.instruction("add x5, x0, #24");                                     // compute source data base
    emitter.instruction("ldr x6, [sp, #8]");                                    // reload offset as destination start
    emitter.instruction("ldr x7, [sp, #16]");                                   // reload removal length
    emitter.instruction("add x8, x6, x7");                                      // initialize source read index
    emitter.label("__rt_array_splice_ref_shift_loop");
    emitter.instruction("cmp x8, x3");                                          // compare source read index with original length
    emitter.instruction("b.ge __rt_array_splice_ref_update");                   // stop shifting after exhausting the tail segment
    emitter.instruction("ldr x9, [x5, x8, lsl #3]");                            // load tail payload
    emitter.instruction("str x9, [x5, x6, lsl #3]");                            // move tail payload left in-place
    emitter.instruction("add x6, x6, #1");                                      // increment destination write index
    emitter.instruction("add x8, x8, #1");                                      // increment source read index
    emitter.instruction("b __rt_array_splice_ref_shift_loop");                  // continue shifting

    emitter.label("__rt_array_splice_ref_update");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // reload original source length
    emitter.instruction("ldr x7, [sp, #16]");                                   // reload removal length
    emitter.instruction("sub x3, x3, x7");                                      // compute new source length
    emitter.instruction("str x3, [x0]");                                        // store new source length

    // -- return removed-elements result array --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload result array pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // return the normalized removal offset, the index a $replacement is inserted at
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return result array
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
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the normalized splice offset across the result-array constructor call and later compaction loop
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the clamped removal length across the result-array constructor call and later compaction loop
    emitter.instruction("mov rdi, rdx");                                        // pass the clamped removal length as the removed-elements result capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // request 8-byte payload slots for the removed-elements result indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the removed-elements result indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // keep the preallocated removed-elements array across compaction
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // load the separated source container
    emitter.instruction("mov r10, QWORD PTR [r10 - 8]");                        // load the source element metadata
    emitter.instruction("and r10, 0x7f00");                                     // exclude the persistent flag and heap magic
    emitter.instruction("or QWORD PTR [rax - 8], r10");                         // retain destination magic while stamping transferred owners
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // load the exact removal count
    emitter.instruction("mov QWORD PTR [rax], r10");                            // publish the preallocated result length
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the removal-copy loop index to the first removed payload slot
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the requested splice offset before seeding the source removal cursor
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // initialize the source cursor at the normalized offset

    emitter.label("__rt_array_splice_ref_copy_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the removal-copy index before testing whether every removed payload has been copied out
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // compare the removal-copy index against the clamped removal length
    emitter.instruction("jge __rt_array_splice_ref_shift_x86");                 // start compacting the source indexed array once every removed payload has been copied into the result array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before reading the next removed payload
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the payload base address for the source indexed array
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the current source removal cursor before reading the next removed payload
    emitter.instruction("mov rsi, QWORD PTR [r10 + r11 * 8]");                  // load the next source-owned pointer for transfer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // load the preallocated removed-elements array
    emitter.instruction("mov QWORD PTR [rdi + rcx * 8 + 24], rsi");             // transfer the removed owner without retaining it again
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the next destination slot index
    emitter.instruction("add rcx, 1");                                          // advance the removal-copy index after copying one removed refcounted payload
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the next destination slot index
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the source removal cursor
    emitter.instruction("add r11, 1");                                          // advance the source removal cursor to the next payload inside the removed splice window
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // save the next source slot index
    emitter.instruction("jmp __rt_array_splice_ref_copy_x86");                  // continue copying removed refcounted payloads until the full splice window has been materialized

    emitter.label("__rt_array_splice_ref_shift_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before compacting the remaining refcounted payloads in place
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // reload the original source indexed-array logical length before starting the in-place compaction loop
    emitter.instruction("lea r10, [r10 + 24]");                                 // recompute the payload base address for the source indexed array
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // seed the destination compaction cursor from the requested splice offset
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the clamped removal length before computing the source compaction cursor
    emitter.instruction("add r9, r8");                                          // seed the source compaction cursor from the first payload after the removed splice window

    emitter.label("__rt_array_splice_ref_shift_loop_x86");
    emitter.instruction("cmp r9, r11");                                         // compare the source compaction cursor against the original source indexed-array logical length
    emitter.instruction("jge __rt_array_splice_ref_update_x86");                // stop compacting once every trailing refcounted payload has moved left over the removed splice window
    emitter.instruction("mov rax, QWORD PTR [r10 + r9 * 8]");                   // load the next trailing refcounted payload that must slide left over the removed splice window
    emitter.instruction("mov QWORD PTR [r10 + r8 * 8], rax");                   // store that trailing refcounted payload into the next compacted destination slot in the source indexed array
    emitter.instruction("add r8, 1");                                           // advance the compacted destination cursor after filling one payload slot
    emitter.instruction("add r9, 1");                                           // advance the trailing source cursor to the next payload beyond the removed splice window
    emitter.instruction("jmp __rt_array_splice_ref_shift_loop_x86");            // continue compacting trailing refcounted payloads until the source indexed-array gap is closed

    emitter.label("__rt_array_splice_ref_update_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before publishing the shortened logical length
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // reload the original source indexed-array logical length before subtracting the removed splice window
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the clamped removal length that must be subtracted from the source indexed-array logical length
    emitter.instruction("sub r11, r9");                                         // compute the shortened source indexed-array logical length after removing the splice window
    emitter.instruction("mov QWORD PTR [r10], r11");                            // persist the shortened source indexed-array logical length back into the array header
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the removed-elements result indexed-array pointer before returning it to the caller
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // return the normalized removal offset, the index a $replacement is inserted at
    emitter.instruction("add rsp, 48");                                         // release the refcounted splice spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the removed-elements result indexed-array pointer in rax
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
