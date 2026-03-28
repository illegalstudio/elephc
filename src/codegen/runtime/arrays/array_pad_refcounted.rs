use crate::codegen::emit::Emitter;

/// array_pad_refcounted: pad a refcounted array to a specified size with a borrowed payload.
/// Input: x0 = array pointer, x1 = size (negative = pad left), x2 = borrowed pad payload
/// Output: x0 = pointer to new padded array
pub fn emit_array_pad_refcounted(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_pad_refcounted ---");
    emitter.label("__rt_array_pad_refcounted");

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
    emitter.instruction("b.ge __rt_array_pad_ref_positive");                     // skip negation for right-padding
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
    emitter.instruction("cbz x4, __rt_array_pad_ref_copy_source");               // skip left padding when caller requested right padding
    emitter.instruction("mov x7, #0");                                          // initialize left-pad loop index
    emitter.label("__rt_array_pad_ref_fill_left");
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload pad element count
    emitter.instruction("cmp x7, x6");                                          // compare loop index with pad count
    emitter.instruction("b.ge __rt_array_pad_ref_copy_source");                  // stop left-padding after inserting every pad element
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
    emitter.instruction("b.ge __rt_array_pad_ref_fill_right");                   // move on to right-padding after copying every source element
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
    emitter.instruction("cbnz x4, __rt_array_pad_ref_done");                     // skip right-padding when caller already padded on the left
    emitter.instruction("mov x7, #0");                                          // initialize right-pad loop index
    emitter.label("__rt_array_pad_ref_fill_right_loop");
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload pad element count
    emitter.instruction("cmp x7, x6");                                          // compare loop index with pad count
    emitter.instruction("b.ge __rt_array_pad_ref_done");                         // finish after inserting every right-pad element
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
