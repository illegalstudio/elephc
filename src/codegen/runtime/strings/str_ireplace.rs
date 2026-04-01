use crate::codegen::emit::Emitter;

/// str_ireplace: case-insensitive str_replace.
/// Input: x1/x2=search, x3/x4=replace, x5/x6=subject. Output: x1/x2=result.
pub fn emit_str_ireplace(emitter: &mut Emitter) {
    // Same as str_replace but uses case-insensitive comparison.
    emitter.blank();
    emitter.comment("--- runtime: str_ireplace ---");
    emitter.label_global("__rt_str_ireplace");
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save search ptr/len
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save replace ptr/len
    emitter.instruction("stp x5, x6, [sp, #32]");                               // save subject ptr/len

    // -- get concat_buf destination --
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // load concat offset page
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                          // load concat buffer page
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                   // resolve address
    emitter.instruction("add x12, x11, x10");                                   // destination pointer
    emitter.instruction("str x12, [sp, #48]");                                  // save result start
    emitter.instruction("str x9, [sp, #56]");                                   // save offset variable ptr
    emitter.instruction("mov x13, #0");                                         // subject scan index

    emitter.label("__rt_sirepl_loop");
    emitter.instruction("ldp x5, x6, [sp, #32]");                               // reload subject
    emitter.instruction("cmp x13, x6");                                         // check if past end
    emitter.instruction("b.ge __rt_sirepl_done");                               // done scanning

    // -- case-insensitive match check --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload search
    emitter.instruction("cbz x2, __rt_sirepl_copy_byte");                       // empty search → no match
    emitter.instruction("sub x14, x6, x13");                                    // remaining in subject
    emitter.instruction("cmp x2, x14");                                         // search longer than remaining?
    emitter.instruction("b.gt __rt_sirepl_copy_byte");                          // yes → can't match

    emitter.instruction("mov x15, #0");                                         // match index
    emitter.label("__rt_sirepl_cmp");
    emitter.instruction("cmp x15, x2");                                         // compared all search chars?
    emitter.instruction("b.ge __rt_sirepl_found");                              // full match found

    emitter.instruction("add x16, x13, x15");                                   // subject position
    emitter.instruction("ldrb w17, [x5, x16]");                                 // load subject byte
    emitter.instruction("ldrb w18, [x1, x15]");                                 // load search byte
    // -- tolower both for comparison --
    emitter.instruction("cmp w17, #65");                                        // subject byte >= 'A'?
    emitter.instruction("b.lt 1f");                                             // skip if not
    emitter.instruction("cmp w17, #90");                                        // subject byte <= 'Z'?
    emitter.instruction("b.gt 1f");                                             // skip if not
    emitter.instruction("add w17, w17, #32");                                   // tolower subject byte
    emitter.raw("1:");
    emitter.instruction("cmp w18, #65");                                        // search byte >= 'A'?
    emitter.instruction("b.lt 2f");                                             // skip if not
    emitter.instruction("cmp w18, #90");                                        // search byte <= 'Z'?
    emitter.instruction("b.gt 2f");                                             // skip if not
    emitter.instruction("add w18, w18, #32");                                   // tolower search byte
    emitter.raw("2:");
    emitter.instruction("cmp w17, w18");                                        // compare lowered bytes
    emitter.instruction("b.ne __rt_sirepl_copy_byte");                          // mismatch → not a match
    emitter.instruction("add x15, x15, #1");                                    // advance match index
    emitter.instruction("b __rt_sirepl_cmp");                                   // continue matching

    emitter.label("__rt_sirepl_found");
    // -- copy replacement --
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload replace
    emitter.instruction("mov x15, #0");                                         // replace copy index
    emitter.label("__rt_sirepl_rep");
    emitter.instruction("cmp x15, x4");                                         // all replacement bytes copied?
    emitter.instruction("b.ge __rt_sirepl_rep_done");                           // yes → advance past search
    emitter.instruction("ldrb w17, [x3, x15]");                                 // load replacement byte
    emitter.instruction("strb w17, [x12], #1");                                 // store to output, advance dest
    emitter.instruction("add x15, x15, #1");                                    // next replacement byte
    emitter.instruction("b __rt_sirepl_rep");                                   // continue
    emitter.label("__rt_sirepl_rep_done");
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload search length
    emitter.instruction("add x13, x13, x2");                                    // skip past matched search in subject
    emitter.instruction("b __rt_sirepl_loop");                                  // continue scanning

    emitter.label("__rt_sirepl_copy_byte");
    emitter.instruction("ldp x5, x6, [sp, #32]");                               // reload subject
    emitter.instruction("ldrb w17, [x5, x13]");                                 // load subject byte
    emitter.instruction("strb w17, [x12], #1");                                 // copy to output
    emitter.instruction("add x13, x13, #1");                                    // advance subject index
    emitter.instruction("b __rt_sirepl_loop");                                  // continue

    emitter.label("__rt_sirepl_done");
    emitter.instruction("ldr x1, [sp, #48]");                                   // result start
    emitter.instruction("sub x2, x12, x1");                                     // result length
    emitter.instruction("ldr x9, [sp, #56]");                                   // offset variable ptr
    emitter.instruction("ldr x10, [x9]");                                       // current offset
    emitter.instruction("add x10, x10, x2");                                    // advance by result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame
    emitter.instruction("add sp, sp, #80");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}
