use crate::codegen::emit::Emitter;

/// substr_replace: replace portion of string.
/// Input: x1/x2=subject, x3/x4=replacement, x0=offset, x7=length (-1=to end).
/// Output: x1/x2=result.
pub fn emit_substr_replace(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: substr_replace ---");
    emitter.label("__rt_substr_replace");
    emitter.instruction("sub sp, sp, #16");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp]");                                 // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // set frame pointer

    // -- clamp offset --
    emitter.instruction("cmp x0, #0");                                          // check if offset is negative
    emitter.instruction("b.ge 1f");                                             // skip if non-negative
    emitter.instruction("add x0, x2, x0");                                      // offset = len + offset
    emitter.instruction("cmp x0, #0");                                          // clamp to 0
    emitter.instruction("csel x0, xzr, x0, lt");                               // if still negative, use 0
    emitter.raw("1:");
    emitter.instruction("cmp x0, x2");                                          // clamp offset to string length
    emitter.instruction("csel x0, x2, x0, gt");                                // min(offset, len)

    // -- compute replace length --
    emitter.instruction("cmn x7, #1");                                          // check if length == -1 (sentinel)
    emitter.instruction("b.ne 2f");                                             // if not sentinel, use given length
    emitter.instruction("sub x7, x2, x0");                                      // length = remaining from offset
    emitter.raw("2:");
    emitter.instruction("cmp x7, #0");                                          // clamp negative length to 0
    emitter.instruction("csel x7, xzr, x7, lt");                               // max(0, length)
    emitter.instruction("add x8, x0, x7");                                      // end = offset + length
    emitter.instruction("cmp x8, x2");                                          // clamp end to string length
    emitter.instruction("csel x8, x2, x8, gt");                                // min(end, len)

    // -- build result: prefix + replacement + suffix --
    emitter.instruction("adrp x9, _concat_off@PAGE");                          // load concat offset page
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                         // load concat buffer page
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                  // resolve address
    emitter.instruction("add x12, x11, x10");                                  // destination pointer
    emitter.instruction("mov x13, x12");                                        // save result start

    // -- copy prefix: subject[0..offset] --
    emitter.instruction("mov x14, #0");                                         // copy index
    emitter.label("__rt_subrepl_pre");
    emitter.instruction("cmp x14, x0");                                         // copied offset bytes?
    emitter.instruction("b.ge __rt_subrepl_mid");                               // yes → copy replacement
    emitter.instruction("ldrb w15, [x1, x14]");                                // load prefix byte
    emitter.instruction("strb w15, [x12], #1");                                // store and advance
    emitter.instruction("add x14, x14, #1");                                   // next byte
    emitter.instruction("b __rt_subrepl_pre");                                  // continue

    // -- copy replacement --
    emitter.label("__rt_subrepl_mid");
    emitter.instruction("mov x14, #0");                                         // replacement copy index
    emitter.label("__rt_subrepl_rep");
    emitter.instruction("cmp x14, x4");                                         // all replacement bytes copied?
    emitter.instruction("b.ge __rt_subrepl_suf");                               // yes → copy suffix
    emitter.instruction("ldrb w15, [x3, x14]");                                // load replacement byte
    emitter.instruction("strb w15, [x12], #1");                                // store and advance
    emitter.instruction("add x14, x14, #1");                                   // next byte
    emitter.instruction("b __rt_subrepl_rep");                                  // continue

    // -- copy suffix: subject[end..len] --
    emitter.label("__rt_subrepl_suf");
    emitter.instruction("mov x14, x8");                                         // start from end position
    emitter.label("__rt_subrepl_suf_loop");
    emitter.instruction("cmp x14, x2");                                         // past end of subject?
    emitter.instruction("b.ge __rt_subrepl_done");                              // yes → done
    emitter.instruction("ldrb w15, [x1, x14]");                                // load suffix byte
    emitter.instruction("strb w15, [x12], #1");                                // store and advance
    emitter.instruction("add x14, x14, #1");                                   // next byte
    emitter.instruction("b __rt_subrepl_suf_loop");                             // continue

    emitter.label("__rt_subrepl_done");
    emitter.instruction("mov x1, x13");                                         // result pointer
    emitter.instruction("sub x2, x12, x13");                                    // result length
    emitter.instruction("ldr x10, [x9]");                                       // reload current offset
    emitter.instruction("add x10, x10, x2");                                    // advance by result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset
    emitter.instruction("ldp x29, x30, [sp]");                                 // restore frame
    emitter.instruction("add sp, sp, #16");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}
