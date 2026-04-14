use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_splice: remove a portion of an array and return removed elements.
/// Input:  x0=array_ptr, x1=offset, x2=length (number of elements to remove)
/// Output: x0=new array containing removed elements
/// The original array is modified in-place (remaining elements shifted left).
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
    emitter.instruction("str x1, [sp, #8]");                                    // save offset
    emitter.instruction("str x2, [sp, #16]");                                   // save removal length

    // -- clamp removal length to not exceed array bounds --
    emitter.instruction("ldr x3, [x0]");                                        // x3 = source array length
    emitter.instruction("sub x4, x3, x1");                                      // x4 = length - offset (max removable)
    emitter.instruction("cmp x2, x4");                                          // compare requested length with max
    emitter.instruction("csel x2, x4, x2, gt");                                 // clamp to max if too large
    emitter.instruction("str x2, [sp, #16]");                                   // save clamped removal length

    // -- create result array for removed elements --
    emitter.instruction("mov x0, x2");                                          // x0 = capacity = removal length
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size (8 bytes per int)
    emitter.instruction("bl __rt_array_new");                                   // create result array, x0 = result ptr
    emitter.instruction("str x0, [sp, #24]");                                   // save result array pointer

    // -- copy removed elements to result array --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x5, x0, #24");                                     // x5 = source data base
    emitter.instruction("ldr x6, [sp, #8]");                                    // x6 = offset
    emitter.instruction("ldr x7, [sp, #16]");                                   // x7 = removal length
    emitter.instruction("mov x8, #0");                                          // x8 = j = 0

    emitter.label("__rt_array_splice_copy");
    emitter.instruction("cmp x8, x7");                                          // compare j with removal length
    emitter.instruction("b.ge __rt_array_splice_shift");                        // if j >= length, start shifting

    emitter.instruction("add x9, x6, x8");                                      // x9 = offset + j (source index)
    emitter.instruction("ldr x1, [x5, x9, lsl #3]");                            // x1 = source[offset + j]
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = result array pointer
    emitter.instruction("bl __rt_array_push_int");                              // push to result array

    emitter.instruction("add x8, x8, #1");                                      // j += 1
    emitter.instruction("b __rt_array_splice_copy");                            // continue copying

    // -- shift remaining elements left to fill the gap --
    emitter.label("__rt_array_splice_shift");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // x3 = original source length
    emitter.instruction("add x5, x0, #24");                                     // x5 = source data base
    emitter.instruction("ldr x6, [sp, #8]");                                    // x6 = offset (destination start)
    emitter.instruction("ldr x7, [sp, #16]");                                   // x7 = removal length
    emitter.instruction("add x8, x6, x7");                                      // x8 = offset + removal_length (source start)

    emitter.label("__rt_array_splice_shift_loop");
    emitter.instruction("cmp x8, x3");                                          // compare source index with array length
    emitter.instruction("b.ge __rt_array_splice_update");                       // if past end, update length

    emitter.instruction("ldr x9, [x5, x8, lsl #3]");                            // x9 = source[source_idx]
    emitter.instruction("str x9, [x5, x6, lsl #3]");                            // source[dest_idx] = source[source_idx]
    emitter.instruction("add x6, x6, #1");                                      // dest_idx += 1
    emitter.instruction("add x8, x8, #1");                                      // source_idx += 1
    emitter.instruction("b __rt_array_splice_shift_loop");                      // continue shifting

    // -- update source array length --
    emitter.label("__rt_array_splice_update");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // x3 = original length
    emitter.instruction("ldr x7, [sp, #16]");                                   // x7 = removal length
    emitter.instruction("sub x3, x3, x7");                                      // x3 = new length
    emitter.instruction("str x3, [x0]");                                        // store new length in header

    // -- return result array --
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = result array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = removed elements array
}

fn emit_array_splice_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_splice ---");
    emitter.label_global("__rt_array_splice");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving scalar splice spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source indexed-array pointer, normalized removal length, and result pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the scalar splice bookkeeping while keeping nested constructor calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across removal-length clamping and result-array construction
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the requested splice offset across the result-array constructor call
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source indexed-array logical length before clamping the requested removal length
    emitter.instruction("mov rcx, r10");                                        // seed the remaining-window scratch register from the source indexed-array logical length
    emitter.instruction("sub rcx, rsi");                                        // compute the maximum removable scalar payload count from the requested splice offset
    emitter.instruction("cmp rdx, -1");                                         // detect the sentinel that means array_splice should remove until the end of the source indexed array
    emitter.instruction("jne __rt_array_splice_known_len_x86");                 // keep the explicit requested removal length when the caller did not use the until-end sentinel
    emitter.instruction("mov rdx, rcx");                                        // replace the until-end sentinel with the remaining scalar payload count in the source indexed array

    emitter.label("__rt_array_splice_known_len_x86");
    emitter.instruction("cmp rdx, rcx");                                        // clamp the requested removal length so it never extends beyond the source indexed-array bounds
    emitter.instruction("jle __rt_array_splice_len_ready_x86");                 // keep the explicit requested removal length when it already fits inside the remaining scalar payload window
    emitter.instruction("mov rdx, rcx");                                        // clamp the requested removal length down to the maximum removable scalar payload count

    emitter.label("__rt_array_splice_len_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the clamped removal length across the result-array constructor call
    emitter.instruction("mov rdi, rdx");                                        // pass the clamped removal length as the result indexed-array capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // request 8-byte scalar payload slots for the result indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the result indexed array that will hold the removed scalar payloads
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the result indexed-array pointer across the scalar copy and source-shift loops
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer after the constructor clobbered caller-saved registers
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the first scalar payload slot address in the source indexed array
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the result indexed-array pointer before seeding the removal-copy loop
    emitter.instruction("lea r11, [r11 + 24]");                                 // compute the first scalar payload slot address in the result indexed array
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the requested splice offset before seeding the source removal cursor
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the clamped removal length before testing whether any scalar payloads must be copied out
    emitter.instruction("xor ecx, ecx");                                        // initialize the removal-copy index to the first scalar payload slot in the result indexed array

    emitter.label("__rt_array_splice_copy_x86");
    emitter.instruction("cmp rcx, r9");                                         // compare the removal-copy index against the clamped removal length
    emitter.instruction("jge __rt_array_splice_shift_x86");                     // start compacting the source indexed array once every removed scalar payload has been copied out
    emitter.instruction("mov rax, QWORD PTR [r10 + r8 * 8]");                   // load the current scalar payload that belongs to the removed splice window
    emitter.instruction("mov QWORD PTR [r11 + rcx * 8], rax");                  // store that scalar payload into the next result indexed-array slot
    emitter.instruction("add r8, 1");                                           // advance the source removal cursor to the next scalar payload inside the removed splice window
    emitter.instruction("add rcx, 1");                                          // advance the result indexed-array cursor after copying one removed scalar payload
    emitter.instruction("jmp __rt_array_splice_copy_x86");                      // continue copying until the removed splice window has been materialized into the result indexed array

    emitter.label("__rt_array_splice_shift_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before compacting the remaining scalar payloads in place
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // reload the original source indexed-array logical length before starting the in-place compaction loop
    emitter.instruction("lea r10, [r10 + 24]");                                 // recompute the first scalar payload slot address in the source indexed array
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // seed the destination compaction cursor from the requested splice offset
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the clamped removal length before computing the source compaction cursor
    emitter.instruction("add r9, r8");                                          // seed the source compaction cursor from the first scalar payload after the removed splice window

    emitter.label("__rt_array_splice_shift_loop_x86");
    emitter.instruction("cmp r9, r11");                                         // compare the source compaction cursor against the original source indexed-array logical length
    emitter.instruction("jge __rt_array_splice_update_x86");                    // stop compacting once every trailing scalar payload has moved left over the removed splice window
    emitter.instruction("mov rax, QWORD PTR [r10 + r9 * 8]");                   // load the next trailing scalar payload that must slide left over the removed splice window
    emitter.instruction("mov QWORD PTR [r10 + r8 * 8], rax");                   // store that trailing scalar payload into the next compacted destination slot in the source indexed array
    emitter.instruction("add r8, 1");                                           // advance the compacted destination cursor after filling one scalar payload slot
    emitter.instruction("add r9, 1");                                           // advance the trailing source cursor to the next scalar payload beyond the removed splice window
    emitter.instruction("jmp __rt_array_splice_shift_loop_x86");                // continue compacting trailing scalar payloads until the source indexed array gap is closed

    emitter.label("__rt_array_splice_update_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before publishing the shortened logical length
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // reload the original source indexed-array logical length before subtracting the removed splice window
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the clamped removal length that must be subtracted from the source indexed-array logical length
    emitter.instruction("sub r11, r9");                                         // compute the shortened source indexed-array logical length after removing the splice window
    emitter.instruction("mov QWORD PTR [r10], r11");                            // persist the shortened source indexed-array logical length back into the array header
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the result indexed-array pointer before publishing its logical length
    emitter.instruction("mov QWORD PTR [rax], r9");                             // store the clamped removal length as the result indexed-array logical length
    emitter.instruction("add rsp, 32");                                         // release the scalar splice spill slots before returning to the caller
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the scalar splice helper completes
    emitter.instruction("ret");                                                 // return the result indexed-array pointer in rax
}
