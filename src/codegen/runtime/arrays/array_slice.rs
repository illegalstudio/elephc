use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_slice: extract a slice of an integer array into a new array.
/// Input: x0 = array pointer, x1 = offset, x2 = length (-1 means to end)
/// Output: x0 = pointer to new sliced array
/// Handles negative offset (counts from end of array).
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
    emitter.instruction("ldr x9, [x0]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save source length

    // -- handle negative offset: convert to positive --
    emitter.instruction("cmp x1, #0");                                          // check if offset is negative
    emitter.instruction("b.ge __rt_array_slice_pos_off");                       // if non-negative, skip adjustment
    emitter.instruction("add x1, x9, x1");                                      // offset = length + offset (e.g., -2 → length-2)
    emitter.instruction("cmp x1, #0");                                          // clamp to 0 if still negative
    emitter.instruction("csel x1, xzr, x1, lt");                                // if offset < 0, set to 0

    // -- compute actual slice length --
    emitter.label("__rt_array_slice_pos_off");
    emitter.instruction("cmp x1, x9");                                          // check if offset >= array length
    emitter.instruction("b.ge __rt_array_slice_empty");                         // if so, result is empty array
    emitter.instruction("sub x3, x9, x1");                                      // x3 = max possible length = array_len - offset
    emitter.instruction("cmn x2, #1");                                          // check if length == -1 (to end)
    emitter.instruction("csel x2, x3, x2, eq");                                 // if length == -1, use remaining length
    emitter.instruction("cmp x2, x3");                                          // clamp length to max possible
    emitter.instruction("csel x2, x3, x2, gt");                                 // if length > remaining, use remaining
    emitter.instruction("str x1, [sp, #16]");                                   // save computed offset
    emitter.instruction("str x2, [sp, #24]");                                   // save computed slice length

    // -- create new array --
    emitter.instruction("mov x0, x2");                                          // x0 = capacity = slice length
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #32]");                                   // save new array pointer

    // -- copy slice elements --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source array pointer
    emitter.instruction("add x2, x1, #24");                                     // x2 = source data base
    emitter.instruction("ldr x3, [sp, #16]");                                   // x3 = offset
    emitter.instruction("ldr x4, [sp, #24]");                                   // x4 = slice length
    emitter.instruction("add x5, x0, #24");                                     // x5 = dest data base
    emitter.instruction("mov x6, #0");                                          // x6 = i = 0

    emitter.label("__rt_array_slice_copy");
    emitter.instruction("cmp x6, x4");                                          // compare i with slice length
    emitter.instruction("b.ge __rt_array_slice_done");                          // if done, finish up
    emitter.instruction("add x7, x3, x6");                                      // x7 = offset + i (source index)
    emitter.instruction("ldr x8, [x2, x7, lsl #3]");                            // x8 = source[offset + i]
    emitter.instruction("str x8, [x5, x6, lsl #3]");                            // dest[i] = source[offset + i]
    emitter.instruction("add x6, x6, #1");                                      // i += 1
    emitter.instruction("b __rt_array_slice_copy");                             // continue loop

    // -- set length and return --
    emitter.label("__rt_array_slice_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #24]");                                   // x9 = slice length
    emitter.instruction("str x9, [x0]");                                        // set new array length
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = sliced array

    // -- empty result: offset was beyond array bounds --
    emitter.label("__rt_array_slice_empty");
    emitter.instruction("mov x0, #0");                                          // x0 = capacity = 0
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8
    emitter.instruction("bl __rt_array_new");                                   // allocate empty array
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = empty array
}

fn emit_array_slice_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_slice ---");
    emitter.label_global("__rt_array_slice");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving scalar slice spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source indexed-array pointer, computed offset, slice length, and result pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the scalar slice bookkeeping while keeping nested constructor calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across slice-length normalization and result-array construction
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source indexed-array logical length before normalizing the slice offset and requested length
    emitter.instruction("cmp rsi, 0");                                          // detect the negative-offset case that counts backward from the end of the source indexed array
    emitter.instruction("jge __rt_array_slice_pos_off_x86");                    // skip the backward-from-end adjustment when the requested slice offset is already non-negative
    emitter.instruction("add rsi, r10");                                        // convert a negative slice offset into a source-length-relative positive offset
    emitter.instruction("cmp rsi, 0");                                          // clamp the normalized slice offset so it never points before the start of the source indexed array
    emitter.instruction("jge __rt_array_slice_pos_off_x86");                    // keep the normalized slice offset when it no longer points before the start of the source indexed array
    emitter.instruction("xor esi, esi");                                        // clamp the normalized slice offset to zero when it would still point before the source array start

    emitter.label("__rt_array_slice_pos_off_x86");
    emitter.instruction("cmp rsi, r10");                                        // detect the out-of-bounds offset case before allocating the destination indexed array
    emitter.instruction("jge __rt_array_slice_empty_x86");                      // return an empty indexed array when the requested slice offset starts beyond the source length
    emitter.instruction("mov rcx, r10");                                        // seed the maximum removable-length scratch register from the source indexed-array logical length
    emitter.instruction("sub rcx, rsi");                                        // compute the remaining scalar payload count from the normalized slice offset to the end of the source indexed array
    emitter.instruction("cmp rdx, -1");                                         // detect the sentinel that means array_slice should run until the end of the source indexed array
    emitter.instruction("jne __rt_array_slice_known_len_x86");                  // keep the explicit requested slice length when the caller did not use the until-end sentinel
    emitter.instruction("mov rdx, rcx");                                        // replace the until-end sentinel with the remaining scalar payload count in the source indexed array

    emitter.label("__rt_array_slice_known_len_x86");
    emitter.instruction("cmp rdx, rcx");                                        // clamp the requested slice length so it cannot extend beyond the source indexed-array bounds
    emitter.instruction("jle __rt_array_slice_len_ready_x86");                  // keep the explicit requested slice length when it already fits inside the remaining scalar payload window
    emitter.instruction("mov rdx, rcx");                                        // clamp the requested slice length down to the remaining scalar payload count

    emitter.label("__rt_array_slice_len_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the normalized slice offset across the destination indexed-array constructor call
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the clamped slice length across the destination indexed-array constructor call
    emitter.instruction("mov rdi, rdx");                                        // pass the clamped slice length as the destination indexed-array capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // request 8-byte scalar payload slots for the destination indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the destination indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the destination indexed-array pointer across the scalar slice copy loop
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer after the constructor clobbered caller-saved registers
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the first scalar payload slot address in the source indexed array
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the destination indexed-array pointer before seeding the slice-copy loop
    emitter.instruction("lea r11, [r11 + 24]");                                 // compute the first scalar payload slot address in the destination indexed array
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the normalized slice offset before seeding the source index for the copy loop
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the clamped slice length before testing whether there is any scalar payload to copy
    emitter.instruction("xor ecx, ecx");                                        // initialize the destination slice index to the first scalar payload slot in the destination indexed array

    emitter.label("__rt_array_slice_copy_x86");
    emitter.instruction("cmp rcx, r9");                                         // compare the destination slice index against the clamped slice length
    emitter.instruction("jge __rt_array_slice_done_x86");                       // finish once every requested scalar payload has been copied into the destination indexed array
    emitter.instruction("mov rax, QWORD PTR [r10 + r8 * 8]");                   // load the current scalar payload from the normalized source slice position
    emitter.instruction("mov QWORD PTR [r11 + rcx * 8], rax");                  // store that scalar payload into the next destination indexed-array slot
    emitter.instruction("add r8, 1");                                           // advance the normalized source slice index to the next scalar payload slot
    emitter.instruction("add rcx, 1");                                          // advance the destination slice index after copying one scalar payload
    emitter.instruction("jmp __rt_array_slice_copy_x86");                       // continue copying until the destination indexed array holds the full scalar slice

    emitter.label("__rt_array_slice_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the destination indexed-array pointer before publishing its logical length
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the clamped slice length so the destination indexed-array header can report the copied payload count
    emitter.instruction("mov QWORD PTR [rax], r9");                             // store the clamped slice length as the destination indexed-array logical length
    emitter.instruction("add rsp, 32");                                         // release the scalar slice spill slots before returning to the caller
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the scalar slice helper completes
    emitter.instruction("ret");                                                 // return the destination indexed-array pointer in rax

    emitter.label("__rt_array_slice_empty_x86");
    emitter.instruction("mov rdi, 0");                                          // request an empty destination indexed-array capacity when the normalized slice offset starts beyond the source length
    emitter.instruction("mov rsi, 8");                                          // request 8-byte scalar payload slots for the empty destination indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the empty destination indexed array through the shared x86_64 constructor
    emitter.instruction("add rsp, 32");                                         // release the scalar slice spill slots before returning the empty destination indexed array
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the empty-slice constructor path
    emitter.instruction("ret");                                                 // return the empty destination indexed-array pointer in rax
}
