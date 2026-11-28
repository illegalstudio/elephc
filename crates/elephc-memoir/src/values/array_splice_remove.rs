//! Purpose:
//! Transfers removed packed array slots to a new array and compacts the source.
//!
//! Called from:
//! - The compiler's packed array splice runtime emitters.
//!
//! Key details:
//! - The caller provides the normalized slice bounds and sets up the target stack frame.

use crate::{Arch, Emitter};

/// Copies scalar slots into the result, compacts the source, and returns from the splice frame.
pub fn emit_remove_scalar(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
        emitter.instruction("str x1, [sp, #8]");                                // save normalized offset
        emitter.instruction("str x2, [sp, #16]");                               // save clamped removal length

        // -- create result array for removed elements --
        emitter.instruction("mov x0, x2");                                      // x0 = capacity = removal length
        emitter.instruction("mov x1, #8");                                      // x1 = elem_size (8 bytes per int)
        emitter.instruction("bl __rt_array_new");                               // create result array, x0 = result ptr
        emitter.instruction("str x0, [sp, #24]");                               // save result array pointer

        // -- copy removed elements to result array --
        emitter.instruction("ldr x0, [sp, #0]");                                // reload source array pointer
        emitter.instruction("add x5, x0, #24");                                 // x5 = source data base
        emitter.instruction("ldr x6, [sp, #8]");                                // x6 = offset
        emitter.instruction("ldr x7, [sp, #16]");                               // x7 = removal length
        emitter.instruction("mov x8, #0");                                      // x8 = j = 0

        emitter.label("__rt_array_splice_copy");
        emitter.instruction("cmp x8, x7");                                      // compare j with removal length
        emitter.instruction("b.ge __rt_array_splice_shift");                    // if j >= length, start shifting

        emitter.instruction("add x9, x6, x8");                                  // x9 = offset + j (source index)
        emitter.instruction("ldr x1, [x5, x9, lsl #3]");                        // x1 = source[offset + j]
        emitter.instruction("ldr x0, [sp, #24]");                               // x0 = result array pointer
        emitter.instruction("bl __rt_array_push_int");                          // push to result array
        emitter.instruction("str x0, [sp, #24]");                               // persist the result pointer in case the append reallocated it

        emitter.instruction("add x8, x8, #1");                                  // j += 1
        emitter.instruction("b __rt_array_splice_copy");                        // continue copying

        // -- shift remaining elements left to fill the gap --
        emitter.label("__rt_array_splice_shift");
        emitter.instruction("ldr x0, [sp, #0]");                                // reload source array pointer
        emitter.instruction("ldr x3, [x0]");                                    // x3 = original source length
        emitter.instruction("add x5, x0, #24");                                 // x5 = source data base
        emitter.instruction("ldr x6, [sp, #8]");                                // x6 = offset (destination start)
        emitter.instruction("ldr x7, [sp, #16]");                               // x7 = removal length
        emitter.instruction("add x8, x6, x7");                                  // x8 = offset + removal_length (source start)

        emitter.label("__rt_array_splice_shift_loop");
        emitter.instruction("cmp x8, x3");                                      // compare source index with array length
        emitter.instruction("b.ge __rt_array_splice_update");                   // if past end, update length

        emitter.instruction("ldr x9, [x5, x8, lsl #3]");                        // x9 = source[source_idx]
        emitter.instruction("str x9, [x5, x6, lsl #3]");                        // source[dest_idx] = source[source_idx]
        emitter.instruction("add x6, x6, #1");                                  // dest_idx += 1
        emitter.instruction("add x8, x8, #1");                                  // source_idx += 1
        emitter.instruction("b __rt_array_splice_shift_loop");                  // continue shifting

        // -- update source array length --
        emitter.label("__rt_array_splice_update");
        emitter.instruction("ldr x0, [sp, #0]");                                // reload source array pointer
        emitter.instruction("ldr x3, [x0]");                                    // x3 = original length
        emitter.instruction("ldr x7, [sp, #16]");                               // x7 = removal length
        emitter.instruction("sub x3, x3, x7");                                  // x3 = new length
        emitter.instruction("str x3, [x0]");                                    // store new length in header

        // -- return result array --
        emitter.instruction("ldr x0, [sp, #24]");                               // x0 = result array pointer
        emitter.instruction("ldr x1, [sp, #8]");                                // x1 = normalized offset, the index a $replacement is inserted at
        emitter.instruction("ldp x29, x30, [sp, #32]");                         // restore frame pointer and return address
        emitter.instruction("add sp, sp, #48");                                 // deallocate stack frame
        emitter.instruction("ret");                                             // return with x0 = removed elements array
        }
        Arch::X86_64 => {
        emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                   // preserve the normalized splice offset across the result-array constructor call
        emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                   // preserve the clamped removal length across the result-array constructor call
        emitter.instruction("mov rdi, rdx");                                    // pass the clamped removal length as the result indexed-array capacity to the shared constructor
        emitter.instruction("mov rsi, 8");                                      // request 8-byte scalar payload slots for the result indexed array
        emitter.instruction("call __rt_array_new");                             // allocate the result indexed array that will hold the removed scalar payloads
        emitter.instruction("mov QWORD PTR [rbp - 32], rax");                   // preserve the result indexed-array pointer across the scalar copy and source-shift loops
        emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                    // reload the source indexed-array pointer after the constructor clobbered caller-saved registers
        emitter.instruction("lea r10, [r10 + 24]");                             // compute the first scalar payload slot address in the source indexed array
        emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                   // reload the result indexed-array pointer before seeding the removal-copy loop
        emitter.instruction("lea r11, [r11 + 24]");                             // compute the first scalar payload slot address in the result indexed array
        emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                    // reload the requested splice offset before seeding the source removal cursor
        emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                    // reload the clamped removal length before testing whether any scalar payloads must be copied out
        emitter.instruction("xor ecx, ecx");                                    // initialize the removal-copy index to the first scalar payload slot in the result indexed array

        emitter.label("__rt_array_splice_copy_x86");
        emitter.instruction("cmp rcx, r9");                                     // compare the removal-copy index against the clamped removal length
        emitter.instruction("jge __rt_array_splice_shift_x86");                 // start compacting the source indexed array once every removed scalar payload has been copied out
        emitter.instruction("mov rax, QWORD PTR [r10 + r8 * 8]");               // load the current scalar payload that belongs to the removed splice window
        emitter.instruction("mov QWORD PTR [r11 + rcx * 8], rax");              // store that scalar payload into the next result indexed-array slot
        emitter.instruction("add r8, 1");                                       // advance the source removal cursor to the next scalar payload inside the removed splice window
        emitter.instruction("add rcx, 1");                                      // advance the result indexed-array cursor after copying one removed scalar payload
        emitter.instruction("jmp __rt_array_splice_copy_x86");                  // continue copying until the removed splice window has been materialized into the result indexed array

        emitter.label("__rt_array_splice_shift_x86");
        emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                    // reload the source indexed-array pointer before compacting the remaining scalar payloads in place
        emitter.instruction("mov r11, QWORD PTR [r10]");                        // reload the original source indexed-array logical length before starting the in-place compaction loop
        emitter.instruction("lea r10, [r10 + 24]");                             // recompute the first scalar payload slot address in the source indexed array
        emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                    // seed the destination compaction cursor from the requested splice offset
        emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                    // reload the clamped removal length before computing the source compaction cursor
        emitter.instruction("add r9, r8");                                      // seed the source compaction cursor from the first scalar payload after the removed splice window

        emitter.label("__rt_array_splice_shift_loop_x86");
        emitter.instruction("cmp r9, r11");                                     // compare the source compaction cursor against the original source indexed-array logical length
        emitter.instruction("jge __rt_array_splice_update_x86");                // stop compacting once every trailing scalar payload has moved left over the removed splice window
        emitter.instruction("mov rax, QWORD PTR [r10 + r9 * 8]");               // load the next trailing scalar payload that must slide left over the removed splice window
        emitter.instruction("mov QWORD PTR [r10 + r8 * 8], rax");               // store that trailing scalar payload into the next compacted destination slot in the source indexed array
        emitter.instruction("add r8, 1");                                       // advance the compacted destination cursor after filling one scalar payload slot
        emitter.instruction("add r9, 1");                                       // advance the trailing source cursor to the next scalar payload beyond the removed splice window
        emitter.instruction("jmp __rt_array_splice_shift_loop_x86");            // continue compacting trailing scalar payloads until the source indexed array gap is closed

        emitter.label("__rt_array_splice_update_x86");
        emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                    // reload the source indexed-array pointer before publishing the shortened logical length
        emitter.instruction("mov r11, QWORD PTR [r10]");                        // reload the original source indexed-array logical length before subtracting the removed splice window
        emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                    // reload the clamped removal length that must be subtracted from the source indexed-array logical length
        emitter.instruction("sub r11, r9");                                     // compute the shortened source indexed-array logical length after removing the splice window
        emitter.instruction("mov QWORD PTR [r10], r11");                        // persist the shortened source indexed-array logical length back into the array header
        emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                   // reload the result indexed-array pointer before publishing its logical length
        emitter.instruction("mov QWORD PTR [rax], r9");                         // store the clamped removal length as the result indexed-array logical length
        emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                   // return the normalized removal offset, the index a $replacement is inserted at
        emitter.instruction("add rsp, 32");                                     // release the scalar splice spill slots before returning to the caller
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer after the scalar splice helper completes
        emitter.instruction("ret");                                             // return the result indexed-array pointer in rax
        }
    }
}

/// Moves owned pointer slots to the result, compacts the source, and returns from the splice frame.
pub fn emit_remove_refcounted(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
        emitter.instruction("str x1, [sp, #8]");                                // save normalized offset
        emitter.instruction("str x2, [sp, #16]");                               // save clamped removal length

        // -- create result array for removed elements --
        emitter.instruction("mov x0, x2");                                      // use removal length as result capacity
        emitter.instruction("mov x1, #8");                                      // use 8-byte slots for heap pointers
        emitter.instruction("bl __rt_array_new");                               // allocate result array
        emitter.instruction("str x0, [sp, #24]");                               // save result array pointer
        emitter.instruction("ldr x9, [sp, #0]");                                // load the separated source container
        emitter.instruction("ldur x9, [x9, #-8]");                              // read its element metadata
        emitter.instruction("and x9, x9, #0x7f00");                             // exclude the persistent COW flag from the new result
        emitter.instruction("ldur x10, [x0, #-8]");                             // preserve the destination heap metadata
        emitter.instruction("orr x10, x10, x9");                                // stamp the transferred element type
        emitter.instruction("stur x10, [x0, #-8]");                             // publish result element ownership
        emitter.instruction("ldr x9, [sp, #16]");                               // load the exact removal count
        emitter.instruction("str x9, [x0]");                                    // set result length before the non-calling copy loop

        // -- transfer removed element owners into the preallocated result --
        emitter.instruction("ldr x0, [sp, #0]");                                // reload source array pointer
        emitter.instruction("add x5, x0, #24");                                 // compute source data base
        emitter.instruction("ldr x6, [sp, #8]");                                // reload offset
        emitter.instruction("ldr x7, [sp, #16]");                               // reload removal length
        emitter.instruction("mov x8, #0");                                      // initialize copy-loop index
        emitter.label("__rt_array_splice_ref_copy");
        emitter.instruction("cmp x8, x7");                                      // compare copy index with removal length
        emitter.instruction("b.ge __rt_array_splice_ref_shift");                // move on to in-place shifting after copying removed elements
        emitter.instruction("add x9, x6, x8");                                  // compute source index = offset + copy index
        emitter.instruction("ldr x1, [x5, x9, lsl #3]");                        // load the source-owned slot for transfer
        emitter.instruction("ldr x10, [sp, #24]");                              // load the preallocated result container
        emitter.instruction("add x10, x10, #24");                               // address its pointer slots
        emitter.instruction("str x1, [x10, x8, lsl #3]");                       // transfer the removed owner without incrementing its refcount
        emitter.instruction("add x8, x8, #1");                                  // increment copy-loop index
        emitter.instruction("b __rt_array_splice_ref_copy");                    // continue copying removed elements

        // -- shift remaining elements left inside the source array --
        emitter.label("__rt_array_splice_ref_shift");
        emitter.instruction("ldr x0, [sp, #0]");                                // reload source array pointer
        emitter.instruction("ldr x3, [x0]");                                    // reload original source length
        emitter.instruction("add x5, x0, #24");                                 // compute source data base
        emitter.instruction("ldr x6, [sp, #8]");                                // reload offset as destination start
        emitter.instruction("ldr x7, [sp, #16]");                               // reload removal length
        emitter.instruction("add x8, x6, x7");                                  // initialize source read index
        emitter.label("__rt_array_splice_ref_shift_loop");
        emitter.instruction("cmp x8, x3");                                      // compare source read index with original length
        emitter.instruction("b.ge __rt_array_splice_ref_update");               // stop shifting after exhausting the tail segment
        emitter.instruction("ldr x9, [x5, x8, lsl #3]");                        // load tail payload
        emitter.instruction("str x9, [x5, x6, lsl #3]");                        // move tail payload left in-place
        emitter.instruction("add x6, x6, #1");                                  // increment destination write index
        emitter.instruction("add x8, x8, #1");                                  // increment source read index
        emitter.instruction("b __rt_array_splice_ref_shift_loop");              // continue shifting

        emitter.label("__rt_array_splice_ref_update");
        emitter.instruction("ldr x0, [sp, #0]");                                // reload source array pointer
        emitter.instruction("ldr x3, [x0]");                                    // reload original source length
        emitter.instruction("ldr x7, [sp, #16]");                               // reload removal length
        emitter.instruction("sub x3, x3, x7");                                  // compute new source length
        emitter.instruction("str x3, [x0]");                                    // store new source length

        // -- return removed-elements result array --
        emitter.instruction("ldr x0, [sp, #24]");                               // reload result array pointer
        emitter.instruction("ldr x1, [sp, #8]");                                // return the normalized removal offset, the index a $replacement is inserted at
        emitter.instruction("ldp x29, x30, [sp, #32]");                         // restore frame pointer and return address
        emitter.instruction("add sp, sp, #48");                                 // deallocate stack frame
        emitter.instruction("ret");                                             // return result array
        }
        Arch::X86_64 => {
        emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                   // preserve the normalized splice offset across the result-array constructor call and later compaction loop
        emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                   // preserve the clamped removal length across the result-array constructor call and later compaction loop
        emitter.instruction("mov rdi, rdx");                                    // pass the clamped removal length as the removed-elements result capacity to the shared constructor
        emitter.instruction("mov rsi, 8");                                      // request 8-byte payload slots for the removed-elements result indexed array
        emitter.instruction("call __rt_array_new");                             // allocate the removed-elements result indexed array through the shared x86_64 constructor
        emitter.instruction("mov QWORD PTR [rbp - 32], rax");                   // keep the preallocated removed-elements array across compaction
        emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                    // load the separated source container
        emitter.instruction("mov r10, QWORD PTR [r10 - 8]");                    // load the source element metadata
        emitter.instruction("and r10, 0x7f00");                                 // exclude the persistent flag and heap magic
        emitter.instruction("or QWORD PTR [rax - 8], r10");                     // retain destination magic while stamping transferred owners
        emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                   // load the exact removal count
        emitter.instruction("mov QWORD PTR [rax], r10");                        // publish the preallocated result length
        emitter.instruction("mov QWORD PTR [rbp - 40], 0");                     // initialize the removal-copy loop index to the first removed payload slot
        emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                   // reload the requested splice offset before seeding the source removal cursor
        emitter.instruction("mov QWORD PTR [rbp - 48], r10");                   // initialize the source cursor at the normalized offset

        emitter.label("__rt_array_splice_ref_copy_x86");
        emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                   // reload the removal-copy index before testing whether every removed payload has been copied out
        emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                   // compare the removal-copy index against the clamped removal length
        emitter.instruction("jge __rt_array_splice_ref_shift_x86");             // start compacting the source indexed array once every removed payload has been copied into the result array
        emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                    // reload the source indexed-array pointer before reading the next removed payload
        emitter.instruction("lea r10, [r10 + 24]");                             // compute the payload base address for the source indexed array
        emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                   // reload the current source removal cursor before reading the next removed payload
        emitter.instruction("mov rsi, QWORD PTR [r10 + r11 * 8]");              // load the next source-owned pointer for transfer
        emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                   // load the preallocated removed-elements array
        emitter.instruction("mov QWORD PTR [rdi + rcx * 8 + 24], rsi");         // transfer the removed owner without retaining it again
        emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                   // reload the next destination slot index
        emitter.instruction("add rcx, 1");                                      // advance the removal-copy index after copying one removed refcounted payload
        emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                   // save the next destination slot index
        emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                   // reload the source removal cursor
        emitter.instruction("add r11, 1");                                      // advance the source removal cursor to the next payload inside the removed splice window
        emitter.instruction("mov QWORD PTR [rbp - 48], r11");                   // save the next source slot index
        emitter.instruction("jmp __rt_array_splice_ref_copy_x86");              // continue copying removed refcounted payloads until the full splice window has been materialized

        emitter.label("__rt_array_splice_ref_shift_x86");
        emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                    // reload the source indexed-array pointer before compacting the remaining refcounted payloads in place
        emitter.instruction("mov r11, QWORD PTR [r10]");                        // reload the original source indexed-array logical length before starting the in-place compaction loop
        emitter.instruction("lea r10, [r10 + 24]");                             // recompute the payload base address for the source indexed array
        emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                    // seed the destination compaction cursor from the requested splice offset
        emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                    // reload the clamped removal length before computing the source compaction cursor
        emitter.instruction("add r9, r8");                                      // seed the source compaction cursor from the first payload after the removed splice window

        emitter.label("__rt_array_splice_ref_shift_loop_x86");
        emitter.instruction("cmp r9, r11");                                     // compare the source compaction cursor against the original source indexed-array logical length
        emitter.instruction("jge __rt_array_splice_ref_update_x86");            // stop compacting once every trailing refcounted payload has moved left over the removed splice window
        emitter.instruction("mov rax, QWORD PTR [r10 + r9 * 8]");               // load the next trailing refcounted payload that must slide left over the removed splice window
        emitter.instruction("mov QWORD PTR [r10 + r8 * 8], rax");               // store that trailing refcounted payload into the next compacted destination slot in the source indexed array
        emitter.instruction("add r8, 1");                                       // advance the compacted destination cursor after filling one payload slot
        emitter.instruction("add r9, 1");                                       // advance the trailing source cursor to the next payload beyond the removed splice window
        emitter.instruction("jmp __rt_array_splice_ref_shift_loop_x86");        // continue compacting trailing refcounted payloads until the source indexed-array gap is closed

        emitter.label("__rt_array_splice_ref_update_x86");
        emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                    // reload the source indexed-array pointer before publishing the shortened logical length
        emitter.instruction("mov r11, QWORD PTR [r10]");                        // reload the original source indexed-array logical length before subtracting the removed splice window
        emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                    // reload the clamped removal length that must be subtracted from the source indexed-array logical length
        emitter.instruction("sub r11, r9");                                     // compute the shortened source indexed-array logical length after removing the splice window
        emitter.instruction("mov QWORD PTR [r10], r11");                        // persist the shortened source indexed-array logical length back into the array header
        emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                   // reload the removed-elements result indexed-array pointer before returning it to the caller
        emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                   // return the normalized removal offset, the index a $replacement is inserted at
        emitter.instruction("add rsp, 48");                                     // release the refcounted splice spill slots before returning
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer before returning to the caller
        emitter.instruction("ret");                                             // return the removed-elements result indexed-array pointer in rax
        }
    }
}

/// Moves unique string slots to the result, compacts the source, and returns from the splice frame.
pub fn emit_remove_strings(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
        emitter.instruction("str x1, [sp, #8]");                                // save the normalized removal offset
        emitter.instruction("str x2, [sp, #16]");                               // save the clamped removal count

        // -- allocate the removed-elements array with string-shaped 16-byte payload slots --
        emitter.instruction("mov x0, x2");                                      // capacity = removal count
        emitter.instruction("mov x1, #16");                                     // string payload slots carry a pointer and a length
        emitter.instruction("bl __rt_array_new");                               // allocate the removed-elements array, already stamped as a string array
        emitter.instruction("str x0, [sp, #24]");                               // preserve the removed-elements array pointer across the move loops

        // -- move the removed string slots out of the source and into the result --
        emitter.instruction("ldr x3, [sp, #0]");                                // reload the source indexed-array pointer
        emitter.instruction("add x3, x3, #24");                                 // compute the source string payload base address
        emitter.instruction("add x4, x0, #24");                                 // compute the result string payload base address
        emitter.instruction("ldr x5, [sp, #8]");                                // reload the normalized removal offset
        emitter.instruction("ldr x6, [sp, #16]");                               // reload the clamped removal count
        emitter.instruction("mov x7, #0");                                      // start the move loop at the first removed string slot

        emitter.label("__rt_array_splice_str_copy");
        emitter.instruction("cmp x7, x6");                                      // has every removed string slot been moved out?
        emitter.instruction("b.ge __rt_array_splice_str_copy_done");            // finish once the removed window is materialized
        emitter.instruction("add x8, x5, x7");                                  // compute the source slot index inside the removed window
        emitter.instruction("lsl x8, x8, #4");                                  // scale it by the 16-byte string slot size
        emitter.instruction("add x8, x3, x8");                                  // compute the source string slot address
        emitter.instruction("ldp x9, x10, [x8]");                               // load the removed string pointer and length
        emitter.instruction("lsl x11, x7, #4");                                 // scale the result cursor by the 16-byte string slot size
        emitter.instruction("add x11, x4, x11");                                // compute the destination string slot address
        emitter.instruction("stp x9, x10, [x11]");                              // hand the owned string payload over to the result array
        emitter.instruction("add x7, x7, #1");                                  // advance to the next removed string slot
        emitter.instruction("b __rt_array_splice_str_copy");                    // keep moving removed string slots

        emitter.label("__rt_array_splice_str_copy_done");
        emitter.instruction("ldr x0, [sp, #24]");                               // reload the removed-elements array pointer
        emitter.instruction("ldr x6, [sp, #16]");                               // reload the clamped removal count
        emitter.instruction("str x6, [x0]");                                    // publish the removed-elements array logical length

        // -- compact the surviving source string slots over the removed window --
        emitter.instruction("ldr x0, [sp, #0]");                                // reload the source indexed-array pointer
        emitter.instruction("ldr x12, [x0]");                                   // load the original source logical length
        emitter.instruction("add x3, x0, #24");                                 // recompute the source string payload base address
        emitter.instruction("ldr x5, [sp, #8]");                                // seed the destination compaction cursor from the removal offset
        emitter.instruction("ldr x6, [sp, #16]");                               // reload the clamped removal count
        emitter.instruction("add x7, x5, x6");                                  // seed the source compaction cursor past the removed window

        emitter.label("__rt_array_splice_str_shift");
        emitter.instruction("cmp x7, x12");                                     // has every trailing string slot slid left?
        emitter.instruction("b.ge __rt_array_splice_str_update");               // stop once the source gap is closed
        emitter.instruction("lsl x8, x7, #4");                                  // scale the trailing source cursor by the string slot size
        emitter.instruction("add x8, x3, x8");                                  // compute the trailing source string slot address
        emitter.instruction("ldp x9, x10, [x8]");                               // load the trailing string pointer and length
        emitter.instruction("lsl x11, x5, #4");                                 // scale the compacted destination cursor by the string slot size
        emitter.instruction("add x11, x3, x11");                                // compute the compacted destination string slot address
        emitter.instruction("stp x9, x10, [x11]");                              // slide the trailing string payload left over the removed window
        emitter.instruction("add x5, x5, #1");                                  // advance the compacted destination cursor
        emitter.instruction("add x7, x7, #1");                                  // advance the trailing source cursor
        emitter.instruction("b __rt_array_splice_str_shift");                   // keep compacting trailing string slots

        emitter.label("__rt_array_splice_str_update");
        emitter.instruction("sub x12, x12, x6");                                // compute the shortened source logical length
        emitter.instruction("str x12, [x0]");                                   // persist the shortened source logical length
        emitter.instruction("ldr x0, [sp, #24]");                               // return the removed-elements array pointer
        emitter.instruction("ldr x1, [sp, #8]");                                // return the normalized removal offset, the index a $replacement is inserted at
        emitter.instruction("ldp x29, x30, [sp, #32]");                         // restore frame pointer and return address
        emitter.instruction("add sp, sp, #48");                                 // release the string-splice bookkeeping slots
        emitter.instruction("ret");                                             // return to the caller
        }
        Arch::X86_64 => {
        emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                   // save the normalized removal offset
        emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                   // save the clamped removal count
        emitter.instruction("mov rdi, rdx");                                    // pass the removal count as the removed-elements array capacity
        emitter.instruction("mov rsi, 16");                                     // request string-shaped 16-byte payload slots
        emitter.instruction("call __rt_array_new");                             // allocate the removed-elements array, already stamped as a string array
        emitter.instruction("mov QWORD PTR [rbp - 32], rax");                   // preserve the removed-elements array pointer across the move loops

        // -- move the removed string slots out of the source and into the result --
        emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                    // reload the source indexed-array pointer after the constructor
        emitter.instruction("lea r10, [r10 + 24]");                             // compute the source string payload base address
        emitter.instruction("lea r11, [rax + 24]");                             // compute the result string payload base address
        emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                    // seed the source cursor from the normalized removal offset
        emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                    // reload the clamped removal count
        emitter.instruction("xor ecx, ecx");                                    // start the move loop at the first removed string slot

        emitter.label("__rt_array_splice_str_copy_x86");
        emitter.instruction("cmp rcx, r9");                                     // has every removed string slot been moved out?
        emitter.instruction("jge __rt_array_splice_str_copy_done_x86");         // finish once the removed window is materialized
        emitter.instruction("mov rax, r8");                                     // copy the source cursor before scaling it
        emitter.instruction("shl rax, 4");                                      // scale the source cursor by the 16-byte string slot size
        emitter.instruction("mov rdx, QWORD PTR [r10 + rax]");                  // load the removed string pointer
        emitter.instruction("mov rsi, QWORD PTR [r10 + rax + 8]");              // load the removed string length
        emitter.instruction("mov rax, rcx");                                    // copy the result cursor before scaling it
        emitter.instruction("shl rax, 4");                                      // scale the result cursor by the 16-byte string slot size
        emitter.instruction("mov QWORD PTR [r11 + rax], rdx");                  // hand the owned string pointer over to the result array
        emitter.instruction("mov QWORD PTR [r11 + rax + 8], rsi");              // store the matching string length in the result slot
        emitter.instruction("add r8, 1");                                       // advance to the next removed string slot
        emitter.instruction("add rcx, 1");                                      // advance the result cursor
        emitter.instruction("jmp __rt_array_splice_str_copy_x86");              // keep moving removed string slots

        emitter.label("__rt_array_splice_str_copy_done_x86");
        emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                   // reload the removed-elements array pointer
        emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                    // reload the clamped removal count
        emitter.instruction("mov QWORD PTR [rax], r9");                         // publish the removed-elements array logical length

        // -- compact the surviving source string slots over the removed window --
        emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                    // reload the source indexed-array pointer
        emitter.instruction("mov rdi, QWORD PTR [r10]");                        // load the original source logical length
        emitter.instruction("lea r10, [r10 + 24]");                             // recompute the source string payload base address
        emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                    // seed the destination compaction cursor from the removal offset
        emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                    // reload the clamped removal count
        emitter.instruction("add r9, r8");                                      // seed the source compaction cursor past the removed window

        emitter.label("__rt_array_splice_str_shift_x86");
        emitter.instruction("cmp r9, rdi");                                     // has every trailing string slot slid left?
        emitter.instruction("jge __rt_array_splice_str_update_x86");            // stop once the source gap is closed
        emitter.instruction("mov rax, r9");                                     // copy the trailing source cursor before scaling it
        emitter.instruction("shl rax, 4");                                      // scale it by the 16-byte string slot size
        emitter.instruction("mov rdx, QWORD PTR [r10 + rax]");                  // load the trailing string pointer
        emitter.instruction("mov rsi, QWORD PTR [r10 + rax + 8]");              // load the trailing string length
        emitter.instruction("mov rax, r8");                                     // copy the compacted destination cursor before scaling it
        emitter.instruction("shl rax, 4");                                      // scale it by the 16-byte string slot size
        emitter.instruction("mov QWORD PTR [r10 + rax], rdx");                  // slide the trailing string pointer left over the removed window
        emitter.instruction("mov QWORD PTR [r10 + rax + 8], rsi");              // slide the matching string length left as well
        emitter.instruction("add r8, 1");                                       // advance the compacted destination cursor
        emitter.instruction("add r9, 1");                                       // advance the trailing source cursor
        emitter.instruction("jmp __rt_array_splice_str_shift_x86");             // keep compacting trailing string slots

        emitter.label("__rt_array_splice_str_update_x86");
        emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                    // reload the source indexed-array pointer
        emitter.instruction("mov r11, QWORD PTR [r10]");                        // reload the original source logical length
        emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                    // reload the clamped removal count
        emitter.instruction("sub r11, r9");                                     // compute the shortened source logical length
        emitter.instruction("mov QWORD PTR [r10], r11");                        // persist the shortened source logical length
        emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                   // return the removed-elements array pointer
        emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                   // return the normalized removal offset, the index a $replacement is inserted at
        emitter.instruction("add rsp, 32");                                     // release the string-splice spill slots
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer
        emitter.instruction("ret");                                             // return to the caller
        }
    }
}
