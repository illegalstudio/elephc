use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_splice_refcounted: remove a portion of a refcounted array and return removed elements.
/// Input:  x0=array_ptr, x1=offset, x2=length
/// Output: x0=new array containing retained removed elements
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
    emitter.instruction("str x1, [sp, #8]");                                    // save offset
    emitter.instruction("str x2, [sp, #16]");                                   // save removal length

    // -- clamp removal length to not exceed array bounds --
    emitter.instruction("ldr x3, [x0]");                                        // load source array length
    emitter.instruction("sub x4, x3, x1");                                      // compute maximum removable length
    emitter.instruction("cmp x2, x4");                                          // compare requested length with maximum removable length
    emitter.instruction("csel x2, x4, x2, gt");                                 // clamp length to the remaining number of elements
    emitter.instruction("str x2, [sp, #16]");                                   // save clamped removal length

    // -- create result array for removed elements --
    emitter.instruction("mov x0, x2");                                          // use removal length as result capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate result array
    emitter.instruction("str x0, [sp, #24]");                                   // save result array pointer

    // -- copy removed elements into the result with retains --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x5, x0, #24");                                     // compute source data base
    emitter.instruction("ldr x6, [sp, #8]");                                    // reload offset
    emitter.instruction("ldr x7, [sp, #16]");                                   // reload removal length
    emitter.instruction("mov x8, #0");                                          // initialize copy-loop index
    emitter.label("__rt_array_splice_ref_copy");
    emitter.instruction("cmp x8, x7");                                          // compare copy index with removal length
    emitter.instruction("b.ge __rt_array_splice_ref_shift");                    // move on to in-place shifting after copying removed elements
    emitter.instruction("add x9, x6, x8");                                      // compute source index = offset + copy index
    emitter.instruction("ldr x1, [x5, x9, lsl #3]");                            // load borrowed removed payload
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload result array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained removed payload into result array
    emitter.instruction("str x0, [sp, #24]");                                   // persist result pointer after possible growth
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
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return result array
}

fn emit_array_splice_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_splice_refcounted ---");
    emitter.label_global("__rt_array_splice_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted splice spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source indexed-array pointer, clamped removal length, and removed-elements result array
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the refcounted splice bookkeeping while keeping helper calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across removal-length clamping and result-array construction
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the requested splice offset across the result-array constructor call and later compaction loop
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source indexed-array logical length before clamping the requested removal length
    emitter.instruction("mov rcx, r10");                                        // seed the remaining-window scratch register from the source indexed-array logical length
    emitter.instruction("sub rcx, rsi");                                        // compute the maximum removable refcounted payload count from the requested splice offset
    emitter.instruction("cmp rdx, -1");                                         // detect the sentinel that means array_splice should remove until the end of the source indexed array
    emitter.instruction("jne __rt_array_splice_ref_known_len_x86");             // keep the explicit requested removal length when the caller did not use the until-end sentinel
    emitter.instruction("mov rdx, rcx");                                        // replace the until-end sentinel with the remaining refcounted payload count in the source indexed array

    emitter.label("__rt_array_splice_ref_known_len_x86");
    emitter.instruction("cmp rdx, rcx");                                        // clamp the requested removal length so it never extends beyond the source indexed-array bounds
    emitter.instruction("jle __rt_array_splice_ref_len_ready_x86");             // keep the explicit requested removal length when it already fits inside the remaining refcounted payload window
    emitter.instruction("mov rdx, rcx");                                        // clamp the requested removal length down to the maximum removable refcounted payload count

    emitter.label("__rt_array_splice_ref_len_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the clamped removal length across the result-array constructor call and later compaction loop
    emitter.instruction("mov rdi, rdx");                                        // pass the clamped removal length as the removed-elements result capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // request 8-byte payload slots for the removed-elements result indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the removed-elements result indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the removed-elements result indexed-array pointer across the refcounted append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the removal-copy loop index to the first removed payload slot
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the requested splice offset before seeding the source removal cursor
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // preserve the current source removal cursor across the refcounted append helper calls

    emitter.label("__rt_array_splice_ref_copy_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the removal-copy index before testing whether every removed payload has been copied out
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // compare the removal-copy index against the clamped removal length
    emitter.instruction("jge __rt_array_splice_ref_shift_x86");                 // start compacting the source indexed array once every removed payload has been copied into the result array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before reading the next removed payload
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the payload base address for the source indexed array
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the current source removal cursor before reading the next removed payload
    emitter.instruction("mov rsi, QWORD PTR [r10 + r11 * 8]");                  // load the next borrowed removed refcounted payload from the source indexed array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the removed-elements result indexed-array pointer before appending the retained payload
    emitter.instruction("call __rt_array_push_refcounted");                     // append the retained removed payload into the result indexed array
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // persist the possibly-grown removed-elements result indexed-array pointer after the append helper returns
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the removal-copy index after helper calls clobbered caller-saved registers
    emitter.instruction("add rcx, 1");                                          // advance the removal-copy index after copying one removed refcounted payload
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // persist the updated removal-copy index across the next append helper call
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the source removal cursor after helper calls clobbered caller-saved registers
    emitter.instruction("add r11, 1");                                          // advance the source removal cursor to the next payload inside the removed splice window
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // persist the updated source removal cursor across the next append helper call
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
    emitter.instruction("add rsp, 48");                                         // release the refcounted splice spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the removed-elements result indexed-array pointer in rax
}
