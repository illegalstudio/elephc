use crate::codegen::emit::Emitter;

/// ucwords: uppercase first letter of each word (after whitespace).
/// Input: x1=ptr, x2=len. Output: x1=new_ptr, x2=len.
pub fn emit_ucwords(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ucwords ---");
    emitter.label("__rt_ucwords");
    emitter.instruction("sub sp, sp, #16");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp]");                                 // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // set frame pointer
    emitter.instruction("bl __rt_strcopy");                                     // copy string to mutable concat_buf
    emitter.instruction("cbz x2, __rt_ucwords_done");                           // empty string → nothing to do
    emitter.instruction("mov x9, x1");                                          // cursor pointer
    emitter.instruction("mov x10, x2");                                         // remaining length
    emitter.instruction("mov x11, #1");                                         // word_start flag (1 = next char starts a word)

    emitter.label("__rt_ucwords_loop");
    emitter.instruction("cbz x10, __rt_ucwords_done");                          // no bytes left → done
    emitter.instruction("ldrb w12, [x9]");                                      // load current byte
    // -- check if current char is whitespace --
    emitter.instruction("cmp w12, #32");                                        // space?
    emitter.instruction("b.eq __rt_ucwords_ws");                                // yes → mark next as word start
    emitter.instruction("cmp w12, #9");                                         // tab?
    emitter.instruction("b.eq __rt_ucwords_ws");                                // yes → mark next as word start
    emitter.instruction("cmp w12, #10");                                        // newline?
    emitter.instruction("b.eq __rt_ucwords_ws");                                // yes → mark next as word start
    // -- not whitespace: uppercase if word_start --
    emitter.instruction("cbz x11, __rt_ucwords_next");                          // not word start → skip uppercasing
    emitter.instruction("cmp w12, #97");                                        // check if char >= 'a'
    emitter.instruction("b.lt __rt_ucwords_clear");                             // not lowercase → just clear flag
    emitter.instruction("cmp w12, #122");                                       // check if char <= 'z'
    emitter.instruction("b.gt __rt_ucwords_clear");                             // not lowercase → just clear flag
    emitter.instruction("sub w12, w12, #32");                                   // convert a-z to A-Z
    emitter.instruction("strb w12, [x9]");                                      // store uppercased byte
    emitter.label("__rt_ucwords_clear");
    emitter.instruction("mov x11, #0");                                         // clear word_start flag
    emitter.instruction("b __rt_ucwords_next");                                 // advance to next char

    emitter.label("__rt_ucwords_ws");
    emitter.instruction("mov x11, #1");                                         // set word_start flag for next char

    emitter.label("__rt_ucwords_next");
    emitter.instruction("add x9, x9, #1");                                      // advance cursor
    emitter.instruction("sub x10, x10, #1");                                    // decrement remaining
    emitter.instruction("b __rt_ucwords_loop");                                 // process next byte

    emitter.label("__rt_ucwords_done");
    emitter.instruction("ldp x29, x30, [sp]");                                 // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x1/x2 from strcopy
}
