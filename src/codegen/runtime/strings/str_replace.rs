use crate::codegen::emit::Emitter;

/// str_replace: replace all occurrences of search with replace in subject.
/// Input: x1/x2=search, x3/x4=replace, x5/x6=subject
/// Output: x1=result_ptr, x2=result_len (in concat_buf)
pub fn emit_str_replace(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_replace ---");
    emitter.label("__rt_str_replace");

    // -- set up stack frame (80 bytes) --
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer

    // -- save input arguments to stack --
    emitter.instruction("stp x1, x2, [sp]");                                    // save search string ptr and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save replacement string ptr and length
    emitter.instruction("stp x5, x6, [sp, #32]");                               // save subject string ptr and length

    // -- get concat_buf destination --
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve exact address of offset variable
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                          // load page address of concat buffer
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                   // resolve exact buffer base address
    emitter.instruction("add x12, x11, x10");                                   // compute destination pointer
    emitter.instruction("str x12, [sp, #48]");                                  // save result start pointer
    emitter.instruction("str x9, [sp, #56]");                                   // save offset variable address

    // -- initialize subject scan index --
    emitter.instruction("mov x13, #0");                                         // subject index = 0

    // -- main loop: scan subject for search string --
    emitter.label("__rt_str_replace_loop");
    emitter.instruction("ldp x5, x6, [sp, #32]");                               // reload subject ptr and length
    emitter.instruction("cmp x13, x6");                                         // check if past end of subject
    emitter.instruction("b.ge __rt_str_replace_done");                          // if done, finalize result

    // -- check if search string matches at current position --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload search ptr and length
    emitter.instruction("cbz x2, __rt_str_replace_copy_byte");                  // empty search = never matches, copy byte
    emitter.instruction("sub x14, x6, x13");                                    // remaining = subject_len - current_pos
    emitter.instruction("cmp x2, x14");                                         // check if search fits in remaining
    emitter.instruction("b.gt __rt_str_replace_copy_byte");                     // search longer than remaining, copy byte

    // -- compare search string at current position --
    emitter.instruction("mov x15, #0");                                         // match comparison index = 0
    emitter.label("__rt_str_replace_match");
    emitter.instruction("cmp x15, x2");                                         // check if all search bytes matched
    emitter.instruction("b.ge __rt_str_replace_found");                         // full match found
    emitter.instruction("add x16, x13, x15");                                   // compute subject index = pos + match_idx
    emitter.instruction("ldrb w17, [x5, x16]");                                 // load subject byte at computed index
    emitter.instruction("ldrb w18, [x1, x15]");                                 // load search byte at match index
    emitter.instruction("cmp w17, w18");                                        // compare subject and search bytes
    emitter.instruction("b.ne __rt_str_replace_copy_byte");                     // mismatch, just copy current byte
    emitter.instruction("add x15, x15, #1");                                    // advance match index
    emitter.instruction("b __rt_str_replace_match");                            // continue matching

    // -- match found: copy replacement string --
    emitter.label("__rt_str_replace_found");
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload replacement ptr and length
    emitter.instruction("mov x15, #0");                                         // replacement copy index = 0
    emitter.label("__rt_str_replace_rep_copy");
    emitter.instruction("cmp x15, x4");                                         // check if all replacement bytes copied
    emitter.instruction("b.ge __rt_str_replace_rep_done");                      // done copying replacement
    emitter.instruction("ldrb w17, [x3, x15]");                                 // load replacement byte at index
    emitter.instruction("strb w17, [x12], #1");                                 // store to dest, advance dest ptr
    emitter.instruction("add x15, x15, #1");                                    // advance replacement index
    emitter.instruction("b __rt_str_replace_rep_copy");                         // continue copying replacement
    emitter.label("__rt_str_replace_rep_done");
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload search ptr and length
    emitter.instruction("add x13, x13, x2");                                    // skip past matched search in subject
    emitter.instruction("b __rt_str_replace_loop");                             // continue scanning subject

    // -- no match: copy single byte from subject --
    emitter.label("__rt_str_replace_copy_byte");
    emitter.instruction("ldp x5, x6, [sp, #32]");                               // reload subject ptr and length
    emitter.instruction("ldrb w17, [x5, x13]");                                 // load subject byte at current position
    emitter.instruction("strb w17, [x12], #1");                                 // store to dest, advance dest ptr
    emitter.instruction("add x13, x13, #1");                                    // advance subject index by 1
    emitter.instruction("b __rt_str_replace_loop");                             // continue scanning

    // -- finalize: compute result length and update concat_off --
    emitter.label("__rt_str_replace_done");
    emitter.instruction("ldr x1, [sp, #48]");                                   // load result start pointer
    emitter.instruction("sub x2, x12, x1");                                     // result length = dest_end - dest_start
    emitter.instruction("ldr x9, [sp, #56]");                                   // load offset variable address
    emitter.instruction("ldr x10, [x9]");                                       // load current concat_off
    emitter.instruction("add x10, x10, x2");                                    // advance offset by result length
    emitter.instruction("str x10, [x9]");                                       // store updated concat_off

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
