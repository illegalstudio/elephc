use crate::codegen::emit::Emitter;

/// str_split: split string into array of chunks.
/// Input: x1/x2=string, x3=chunk_length. Output: x0=array pointer.
pub fn emit_str_split(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_split ---");
    emitter.label_global("__rt_str_split");
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save string ptr/len
    emitter.instruction("str x3, [sp, #16]");                                   // save chunk length

    // -- create array --
    emitter.instruction("mov x0, #16");                                         // initial capacity
    emitter.instruction("mov x1, #16");                                         // elem_size = 16 (str ptr+len)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #24]");                                   // save array pointer
    emitter.instruction("str xzr, [sp, #32]");                                  // current position = 0

    emitter.label("__rt_str_split_loop");
    emitter.instruction("ldr x4, [sp, #32]");                                   // load current position
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload string ptr/len
    emitter.instruction("cmp x4, x2");                                          // past end of string?
    emitter.instruction("b.ge __rt_str_split_done");                            // yes → done

    // -- compute this chunk's actual length --
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload chunk length
    emitter.instruction("sub x5, x2, x4");                                      // remaining = len - pos
    emitter.instruction("cmp x5, x3");                                          // remaining vs chunk_length
    emitter.instruction("csel x5, x3, x5, gt");                                 // chunk = min(remaining, chunk_length)

    // -- push chunk as string element --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload array pointer
    emitter.instruction("add x1, x1, x4");                                      // x1 = base + current position
    emitter.instruction("mov x2, x5");                                          // x2 = chunk length
    emitter.instruction("bl __rt_array_push_str");                              // push chunk onto array
    emitter.instruction("str x0, [sp, #24]");                                   // update array pointer after possible realloc

    // -- advance position by chunk length --
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload position
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload chunk length
    emitter.instruction("add x4, x4, x3");                                      // position += chunk_length
    emitter.instruction("str x4, [sp, #32]");                                   // save updated position
    emitter.instruction("b __rt_str_split_loop");                               // continue

    emitter.label("__rt_str_split_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // return array pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame
    emitter.instruction("add sp, sp, #64");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}
