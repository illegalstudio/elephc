use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_slice_refcounted: extract a slice of a refcounted array into a new array.
/// Input: x0 = array pointer, x1 = offset, x2 = length (-1 means to end)
/// Output: x0 = pointer to new sliced array
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
    emitter.instruction("ldr x9, [x0]");                                        // load source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save source length

    // -- handle negative offset: convert to positive --
    emitter.instruction("cmp x1, #0");                                          // test whether offset is negative
    emitter.instruction("b.ge __rt_array_slice_ref_pos_off");                   // skip adjustment for non-negative offsets
    emitter.instruction("add x1, x9, x1");                                      // convert negative offset into positive index
    emitter.instruction("cmp x1, #0");                                          // clamp converted offset against zero
    emitter.instruction("csel x1, xzr, x1, lt");                                // use zero when converted offset is still negative

    emitter.label("__rt_array_slice_ref_pos_off");
    emitter.instruction("cmp x1, x9");                                          // compare offset with source length
    emitter.instruction("b.ge __rt_array_slice_ref_empty");                     // return empty array when offset is out of range
    emitter.instruction("sub x3, x9, x1");                                      // compute maximum possible slice length
    emitter.instruction("cmn x2, #1");                                          // check whether requested length is -1
    emitter.instruction("csel x2, x3, x2, eq");                                 // use remaining length when caller requested -1
    emitter.instruction("cmp x2, x3");                                          // compare requested length with remaining length
    emitter.instruction("csel x2, x3, x2, gt");                                 // clamp requested length to remaining length
    emitter.instruction("str x1, [sp, #16]");                                   // save normalized offset
    emitter.instruction("str x2, [sp, #24]");                                   // save normalized slice length

    // -- create destination array --
    emitter.instruction("mov x0, x2");                                          // move slice length into destination capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #32]");                                   // save destination array pointer

    // -- copy the requested range with retains --
    emitter.instruction("mov x6, #0");                                          // initialize loop index
    emitter.label("__rt_array_slice_ref_loop");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload slice length
    emitter.instruction("cmp x6, x4");                                          // compare loop index with slice length
    emitter.instruction("b.ge __rt_array_slice_ref_done");                      // finish after copying every requested element
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x2, x1, #24");                                     // compute source data base
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload normalized offset
    emitter.instruction("add x7, x3, x6");                                      // compute source index = offset + loop index
    emitter.instruction("ldr x1, [x2, x7, lsl #3]");                            // load borrowed source payload
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained payload into destination array
    emitter.instruction("str x0, [sp, #32]");                                   // persist destination pointer after possible growth
    emitter.instruction("add x6, x6, #1");                                      // increment loop index
    emitter.instruction("b __rt_array_slice_ref_loop");                         // continue copying

    emitter.label("__rt_array_slice_ref_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return sliced array

    emitter.label("__rt_array_slice_ref_empty");
    emitter.instruction("mov x0, #0");                                          // request zero-capacity destination array
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate empty destination array
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return empty sliced array
}

fn emit_array_slice_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_slice_refcounted ---");
    emitter.label_global("__rt_array_slice_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted slice spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source indexed-array pointer, normalized offset, clamped length, and destination array
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the refcounted slice bookkeeping while keeping helper calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across slice normalization and destination construction
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source indexed-array logical length before normalizing the requested slice offset and length
    emitter.instruction("cmp rsi, 0");                                          // detect the negative-offset case that counts backward from the end of the source indexed array
    emitter.instruction("jge __rt_array_slice_ref_pos_off_x86");                // skip the backward-from-end adjustment when the requested slice offset is already non-negative
    emitter.instruction("add rsi, r10");                                        // convert a negative slice offset into a source-length-relative positive offset
    emitter.instruction("cmp rsi, 0");                                          // clamp the normalized slice offset so it never points before the start of the source indexed array
    emitter.instruction("jge __rt_array_slice_ref_pos_off_x86");                // keep the normalized slice offset when it no longer points before the start of the source indexed array
    emitter.instruction("xor esi, esi");                                        // clamp the normalized slice offset to zero when it would still point before the source array start

    emitter.label("__rt_array_slice_ref_pos_off_x86");
    emitter.instruction("cmp rsi, r10");                                        // detect the out-of-bounds offset case before allocating the destination indexed array
    emitter.instruction("jge __rt_array_slice_ref_empty_x86");                  // return an empty indexed array when the requested slice offset starts beyond the source length
    emitter.instruction("mov rcx, r10");                                        // seed the remaining-window scratch register from the source indexed-array logical length
    emitter.instruction("sub rcx, rsi");                                        // compute the remaining refcounted payload count from the normalized slice offset to the end of the source indexed array
    emitter.instruction("cmp rdx, -1");                                         // detect the sentinel that means array_slice should run until the end of the source indexed array
    emitter.instruction("jne __rt_array_slice_ref_known_len_x86");              // keep the explicit requested slice length when the caller did not use the until-end sentinel
    emitter.instruction("mov rdx, rcx");                                        // replace the until-end sentinel with the remaining refcounted payload count in the source indexed array

    emitter.label("__rt_array_slice_ref_known_len_x86");
    emitter.instruction("cmp rdx, rcx");                                        // clamp the requested slice length so it cannot extend beyond the source indexed-array bounds
    emitter.instruction("jle __rt_array_slice_ref_len_ready_x86");              // keep the explicit requested slice length when it already fits inside the remaining refcounted payload window
    emitter.instruction("mov rdx, rcx");                                        // clamp the requested slice length down to the remaining refcounted payload count

    emitter.label("__rt_array_slice_ref_len_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the normalized slice offset across the destination indexed-array constructor call
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the clamped slice length across the destination indexed-array constructor call
    emitter.instruction("mov rdi, rdx");                                        // pass the clamped slice length as the destination indexed-array capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // request 8-byte payload slots for the destination refcounted indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the destination indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the destination indexed-array pointer across the refcounted append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the slice-copy loop index to the first destination payload slot
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the normalized source slice offset before seeding the source payload cursor
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // preserve the current source slice cursor across refcounted append helper calls

    emitter.label("__rt_array_slice_ref_copy_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the destination slice index before testing whether every requested payload has been copied
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // compare the destination slice index against the clamped slice length
    emitter.instruction("jge __rt_array_slice_ref_done_x86");                   // finish once every requested refcounted payload has been copied into the destination indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before reading the next refcounted slice payload
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the payload base address for the source indexed array
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the current source slice cursor before reading the next refcounted payload
    emitter.instruction("mov rsi, QWORD PTR [r10 + r11 * 8]");                  // load the next borrowed refcounted payload from the source slice window
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the destination indexed-array pointer before appending the retained slice payload
    emitter.instruction("call __rt_array_push_refcounted");                     // append the retained slice payload into the destination indexed array
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // persist the possibly-grown destination indexed-array pointer after the append helper returns
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the destination slice index after helper calls clobbered caller-saved registers
    emitter.instruction("add rcx, 1");                                          // advance the destination slice index after copying one refcounted payload
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // persist the updated destination slice index across the next append helper call
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the source slice cursor after helper calls clobbered caller-saved registers
    emitter.instruction("add r11, 1");                                          // advance the source slice cursor to the next refcounted payload in the requested slice window
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // persist the updated source slice cursor across the next append helper call
    emitter.instruction("jmp __rt_array_slice_ref_copy_x86");                   // continue copying refcounted slice payloads until the requested slice window is exhausted

    emitter.label("__rt_array_slice_ref_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the destination indexed-array pointer in the standard x86_64 integer result register
    emitter.instruction("add rsp, 48");                                         // release the refcounted slice spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the refcounted sliced indexed-array pointer in rax

    emitter.label("__rt_array_slice_ref_empty_x86");
    emitter.instruction("mov rdi, 0");                                          // request an empty destination indexed-array capacity when the normalized slice offset starts beyond the source length
    emitter.instruction("mov rsi, 8");                                          // request 8-byte payload slots for the empty destination indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the empty destination indexed array through the shared x86_64 constructor
    emitter.instruction("add rsp, 48");                                         // release the refcounted slice spill slots before returning the empty destination indexed array
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the empty-slice constructor path
    emitter.instruction("ret");                                                 // return the empty refcounted sliced indexed-array pointer in rax
}
