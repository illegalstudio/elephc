use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_pad_refcounted: pad a refcounted array to a specified size with a borrowed payload.
/// Input: x0 = array pointer, x1 = size (negative = pad left), x2 = borrowed pad payload
/// Output: x0 = pointer to new padded array
pub fn emit_array_pad_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_pad_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_pad_refcounted ---");
    emitter.label_global("__rt_array_pad_refcounted");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save requested size
    emitter.instruction("str x2, [sp, #16]");                                   // save borrowed pad payload
    emitter.instruction("ldr x9, [x0]");                                        // load source array length
    emitter.instruction("str x9, [sp, #24]");                                   // save source array length

    // -- determine absolute target size and padding direction --
    emitter.instruction("cmp x1, #0");                                          // check whether caller requested left-padding
    emitter.instruction("b.ge __rt_array_pad_ref_positive");                    // skip negation for right-padding
    emitter.instruction("neg x3, x1");                                          // compute absolute target size
    emitter.instruction("mov x4, #1");                                          // remember that padding goes on the left
    emitter.instruction("b __rt_array_pad_ref_check");                          // continue with normalized size

    emitter.label("__rt_array_pad_ref_positive");
    emitter.instruction("mov x3, x1");                                          // normalized target size already positive
    emitter.instruction("mov x4, #0");                                          // remember that padding goes on the right

    emitter.label("__rt_array_pad_ref_check");
    emitter.instruction("cmp x3, x9");                                          // compare normalized target size with source length
    emitter.instruction("csel x5, x3, x9, gt");                                 // x5 = max(source_len, target_len)
    emitter.instruction("sub x6, x5, x9");                                      // x6 = number of pad elements to insert
    emitter.instruction("str x4, [sp, #32]");                                   // save pad-left flag
    emitter.instruction("str x6, [sp, #40]");                                   // save pad element count

    // -- create destination array --
    emitter.instruction("mov x0, x5");                                          // use resulting size as destination capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #48]");                                   // save destination array pointer

    // -- pad left when requested --
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload pad-left flag
    emitter.instruction("cbz x4, __rt_array_pad_ref_copy_source");              // skip left padding when caller requested right padding
    emitter.instruction("mov x7, #0");                                          // initialize left-pad loop index
    emitter.label("__rt_array_pad_ref_fill_left");
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload pad element count
    emitter.instruction("cmp x7, x6");                                          // compare loop index with pad count
    emitter.instruction("b.ge __rt_array_pad_ref_copy_source");                 // stop left-padding after inserting every pad element
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload borrowed pad payload
    emitter.instruction("str x7, [sp, #56]");                                   // preserve left-pad loop index across helper calls
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained pad payload to the destination array
    emitter.instruction("str x0, [sp, #48]");                                   // persist destination pointer after possible growth
    emitter.instruction("ldr x7, [sp, #56]");                                   // restore left-pad loop index after helper calls
    emitter.instruction("add x7, x7, #1");                                      // increment left-pad loop index
    emitter.instruction("b __rt_array_pad_ref_fill_left");                      // continue left-padding

    // -- copy source payloads into destination --
    emitter.label("__rt_array_pad_ref_copy_source");
    emitter.instruction("mov x7, #0");                                          // initialize source loop index
    emitter.label("__rt_array_pad_ref_copy_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload source array length
    emitter.instruction("cmp x7, x9");                                          // compare loop index with source length
    emitter.instruction("b.ge __rt_array_pad_ref_fill_right");                  // move on to right-padding after copying every source element
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x2, x1, #24");                                     // compute source data base
    emitter.instruction("ldr x1, [x2, x7, lsl #3]");                            // load borrowed source payload
    emitter.instruction("str x7, [sp, #56]");                                   // preserve source loop index across helper calls
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained source payload into destination array
    emitter.instruction("str x0, [sp, #48]");                                   // persist destination pointer after possible growth
    emitter.instruction("ldr x7, [sp, #56]");                                   // restore source loop index after helper calls
    emitter.instruction("add x7, x7, #1");                                      // increment source loop index
    emitter.instruction("b __rt_array_pad_ref_copy_loop");                      // continue copying source elements

    // -- pad right when requested --
    emitter.label("__rt_array_pad_ref_fill_right");
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload pad-left flag
    emitter.instruction("cbnz x4, __rt_array_pad_ref_done");                    // skip right-padding when caller already padded on the left
    emitter.instruction("mov x7, #0");                                          // initialize right-pad loop index
    emitter.label("__rt_array_pad_ref_fill_right_loop");
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload pad element count
    emitter.instruction("cmp x7, x6");                                          // compare loop index with pad count
    emitter.instruction("b.ge __rt_array_pad_ref_done");                        // finish after inserting every right-pad element
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload borrowed pad payload
    emitter.instruction("str x7, [sp, #56]");                                   // preserve right-pad loop index across helper calls
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained pad payload to the destination array
    emitter.instruction("str x0, [sp, #48]");                                   // persist destination pointer after possible growth
    emitter.instruction("ldr x7, [sp, #56]");                                   // restore right-pad loop index after helper calls
    emitter.instruction("add x7, x7, #1");                                      // increment right-pad loop index
    emitter.instruction("b __rt_array_pad_ref_fill_right_loop");                // continue right-padding

    emitter.label("__rt_array_pad_ref_done");
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return padded array
}

fn emit_array_pad_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_pad_refcounted ---");
    emitter.label_global("__rt_array_pad_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted pad spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source indexed-array pointer, requested size, pad payload, and destination array
    emitter.instruction("sub rsp, 56");                                         // reserve aligned spill slots for the refcounted pad bookkeeping while keeping helper calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across normalization and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the signed requested target size across normalization and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the borrowed pad payload across the repeated refcounted append helper calls
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source indexed-array logical length before deriving the normalized target size and pad count
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the source indexed-array logical length across the destination constructor call
    emitter.instruction("mov r11, rsi");                                        // copy the signed requested target size before normalizing it to an absolute length
    emitter.instruction("xor ecx, ecx");                                        // initialize the pad-left flag to false before checking whether the requested target size is negative
    emitter.instruction("cmp r11, 0");                                          // detect the negative-target-size case that means pad on the left
    emitter.instruction("jge __rt_array_pad_ref_abs_ready_x86");                // skip negation when the requested target size already pads on the right
    emitter.instruction("neg r11");                                             // normalize the requested target size to its absolute magnitude
    emitter.instruction("mov rcx, 1");                                          // remember that the requested target size was negative so padding must happen on the left

    emitter.label("__rt_array_pad_ref_abs_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // preserve the pad-left flag across the destination constructor and append helper calls
    emitter.instruction("mov rax, r11");                                        // seed the normalized target length from the absolute requested target size
    emitter.instruction("cmp rax, r10");                                        // compare the normalized target length with the source indexed-array logical length
    emitter.instruction("jge __rt_array_pad_ref_target_ready_x86");             // keep the normalized target length when it already exceeds the source length
    emitter.instruction("mov rax, r10");                                        // fall back to the source indexed-array logical length when no padding is required

    emitter.label("__rt_array_pad_ref_target_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the final destination length across the destination constructor and append helper calls
    emitter.instruction("mov r11, rax");                                        // seed the pad-count scratch register from the final destination length
    emitter.instruction("sub r11, r10");                                        // compute how many refcounted pad payloads must be inserted to reach the final destination length
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // preserve the pad-count value across the destination constructor and append helper calls
    emitter.instruction("mov rdi, rax");                                        // pass the final destination length as the padded indexed-array capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // request 8-byte payload slots for the padded refcounted indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the padded destination indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // reuse the destination-length spill slot to preserve the padded destination indexed-array pointer across append helper calls
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the pad-left flag before branching between the left-padding and right-padding layouts
    emitter.instruction("test r10, r10");                                       // check whether the requested target size asked for left-padding
    emitter.instruction("jz __rt_array_pad_ref_copy_source_x86");               // skip the leading pad loop when the caller requested right-padding
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // reuse the pad-left flag spill slot as the leading pad-loop index

    emitter.label("__rt_array_pad_ref_fill_left_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the leading pad-loop index before testing whether every pad slot has been filled
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 56]");                       // compare the leading pad-loop index against the computed pad-count value
    emitter.instruction("jge __rt_array_pad_ref_copy_source_x86");              // stop left-padding once every required pad payload has been appended
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the padded destination indexed-array pointer before appending the retained pad payload
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the borrowed pad payload into the second x86_64 append helper argument register
    emitter.instruction("call __rt_array_push_refcounted");                     // append the retained pad payload into the padded destination indexed array
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // persist the possibly-grown padded destination indexed-array pointer after the append helper returns
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the leading pad-loop index after helper calls clobbered caller-saved registers
    emitter.instruction("add rcx, 1");                                          // advance the leading pad-loop index after appending one retained pad payload
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // persist the updated leading pad-loop index across the next append helper call
    emitter.instruction("jmp __rt_array_pad_ref_fill_left_x86");                // continue filling the leading pad prefix until the required pad count is exhausted

    emitter.label("__rt_array_pad_ref_copy_source_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // reset the reused loop-index spill slot before copying source payloads into the padded destination array

    emitter.label("__rt_array_pad_ref_copy_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the source-copy index before testing whether every source payload has been appended
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // compare the source-copy index against the source indexed-array logical length
    emitter.instruction("jge __rt_array_pad_ref_fill_right_x86");               // move on to right-padding after copying every source refcounted payload into the destination array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before reading the next source refcounted payload
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the payload base address for the source indexed array
    emitter.instruction("mov rsi, QWORD PTR [r10 + rcx * 8]");                  // load the next borrowed refcounted payload from the source indexed array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the padded destination indexed-array pointer before appending the retained source payload
    emitter.instruction("call __rt_array_push_refcounted");                     // append the retained source payload into the padded destination indexed array
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // persist the possibly-grown padded destination indexed-array pointer after the append helper returns
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the source-copy index after helper calls clobbered caller-saved registers
    emitter.instruction("add rcx, 1");                                          // advance the source-copy index after appending one retained source payload
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // persist the updated source-copy index across the next append helper call
    emitter.instruction("jmp __rt_array_pad_ref_copy_loop_x86");                // continue copying source refcounted payloads into the padded destination array

    emitter.label("__rt_array_pad_ref_fill_right_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the signed requested target size before deciding whether trailing padding is still required
    emitter.instruction("cmp r10, 0");                                          // detect the negative-target-size case, which already consumed every required pad payload on the left
    emitter.instruction("jl __rt_array_pad_ref_done_x86");                      // skip the trailing pad loop when the caller requested left-padding
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // reset the reused loop-index spill slot before appending trailing pad payloads

    emitter.label("__rt_array_pad_ref_fill_right_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the trailing pad-loop index before testing whether every required pad slot has been filled
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 56]");                       // compare the trailing pad-loop index against the computed pad-count value
    emitter.instruction("jge __rt_array_pad_ref_done_x86");                     // finish once every required trailing pad payload has been appended
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the padded destination indexed-array pointer before appending the retained pad payload
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the borrowed pad payload into the second x86_64 append helper argument register
    emitter.instruction("call __rt_array_push_refcounted");                     // append the retained trailing pad payload into the padded destination indexed array
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // persist the possibly-grown padded destination indexed-array pointer after the append helper returns
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the trailing pad-loop index after helper calls clobbered caller-saved registers
    emitter.instruction("add rcx, 1");                                          // advance the trailing pad-loop index after appending one retained pad payload
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // persist the updated trailing pad-loop index across the next append helper call
    emitter.instruction("jmp __rt_array_pad_ref_fill_right_loop_x86");          // continue appending trailing refcounted pad payloads until the pad-count requirement is exhausted

    emitter.label("__rt_array_pad_ref_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return the padded destination indexed-array pointer in the standard x86_64 integer result register
    emitter.instruction("add rsp, 56");                                         // release the refcounted pad spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the padded refcounted indexed-array pointer in rax
}
