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
    emitter.instruction("str x1, [sp, #16]");                                   // save normalized offset
    emitter.instruction("str x2, [sp, #24]");                                   // save normalized slice length

    // -- create destination array with 16-byte string slots --
    emitter.instruction("mov x0, x2");                                          // move slice length into destination capacity
    emitter.instruction("mov x1, #16");                                         // request 16-byte slots for {pointer, length} pairs
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #32]");                                   // save destination array pointer

    // -- copy the requested range, duplicating each string --
    //
    // The index is SPILLED rather than parked in a register: `__rt_array_push_str` persists the
    // payload and may grow the destination, so it is a full call and every caller-saved register
    // is fair game across it.
    emitter.instruction("mov x6, #0");                                          // initialize loop index
    emitter.instruction("str x6, [sp, #40]");                                   // seed the spilled loop index
    emitter.label("__rt_array_slice_str_loop");
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload the loop index for this iteration
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload slice length
    emitter.instruction("cmp x6, x4");                                          // compare loop index with slice length
    emitter.instruction("b.ge __rt_array_slice_str_done");                      // finish after copying every requested element
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x2, x1, #24");                                     // compute source data base past the 24-byte header
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload normalized offset
    emitter.instruction("add x7, x3, x6");                                      // compute source index = offset + loop index
    emitter.instruction("lsl x7, x7, #4");                                      // scale the source index by the 16-byte string slot
    emitter.instruction("add x7, x2, x7");                                      // compute the address of the source {pointer, length} pair
    emitter.instruction("ldp x1, x2, [x7]");                                    // load the borrowed source pointer and length together
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_str");                              // append a PERSISTED copy of the pair into the destination
    emitter.instruction("str x0, [sp, #32]");                                   // persist destination pointer after possible growth
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload loop index across the append helper call
    emitter.instruction("add x6, x6, #1");                                      // increment loop index
    emitter.instruction("str x6, [sp, #40]");                                   // save the updated loop index for the next iteration
    emitter.instruction("b __rt_array_slice_str_loop");                         // continue copying

    emitter.label("__rt_array_slice_str_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
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
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the normalized slice offset across the destination constructor call
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the clamped slice length across the destination constructor call
    emitter.instruction("mov rdi, rdx");                                        // pass the clamped slice length as the destination capacity
    emitter.instruction("mov rsi, 16");                                         // request 16-byte payload slots for the destination string array
    emitter.instruction("call __rt_array_new");                                 // allocate the destination indexed array
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the destination pointer across the string append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the slice-copy loop index
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the normalized offset before seeding the source cursor
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // preserve the current source slice cursor across append helper calls

    emitter.label("__rt_array_slice_str_copy_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the destination slice index before the bound test
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // compare the destination index against the clamped slice length
    emitter.instruction("jge __rt_array_slice_str_done_x86");                   // finish once every requested string pair has been copied
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the payload base past the 24-byte header
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the current source slice cursor
    emitter.instruction("shl r11, 4");                                          // scale the source cursor by the 16-byte string slot
    emitter.instruction("add r10, r11");                                        // compute the address of the source {pointer, length} pair
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the borrowed source string pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the borrowed source string length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the destination indexed-array pointer
    emitter.instruction("call __rt_array_push_str");                            // append a PERSISTED copy of the pair into the destination
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // persist the possibly-grown destination pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the destination index after the call clobbered caller-saved registers
    emitter.instruction("add rcx, 1");                                          // advance the destination index
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // persist the updated destination index
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the source cursor after the call
    emitter.instruction("add r11, 1");                                          // advance the source cursor to the next string pair
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // persist the updated source cursor
    emitter.instruction("jmp __rt_array_slice_str_copy_x86");                   // continue copying until the window is exhausted

    emitter.label("__rt_array_slice_str_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the destination pointer in the standard result register
    emitter.instruction("add rsp, 48");                                         // release the string-slice spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the sliced string array
}
