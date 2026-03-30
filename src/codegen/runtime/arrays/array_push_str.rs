use crate::codegen::emit::Emitter;

/// array_push_str: push a string element (ptr+len) to an array, growing if needed.
/// Always persists the string to heap first (ensures safety even when the
/// string points to the volatile concat_buf).
/// Input:  x0 = array pointer, x1 = str ptr, x2 = str len
/// Output: x0 = array pointer (may differ if array was reallocated)
pub fn emit_array_push_str(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_str ---");
    emitter.label("__rt_array_push_str");

    // -- set up stack frame (needed for str_persist and potential growth) --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the incoming string ptr/len across ensure_unique
    emitter.instruction("bl __rt_array_ensure_unique");                         // split shared arrays before persisting/appending a new string slot
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique array pointer

    // -- persist string to heap before pushing --
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // restore the incoming string ptr/len after ensure_unique
    emitter.instruction("bl __rt_str_persist");                                 // copy string to heap, x1=heap_ptr, x2=len
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save persisted string ptr and len

    // -- check capacity before pushing --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array pointer
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = array capacity
    emitter.instruction("cmp x9, x10");                                         // is the array full?
    emitter.instruction("b.ge __rt_array_push_str_grow");                       // grow array if at capacity

    // -- push directly --
    emitter.label("__rt_array_push_str_push");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload array pointer
    emitter.instruction("ldr x9, [x0]");                                        // reload length
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // reload persisted string ptr and len
    emitter.instruction("lsl x10, x9, #4");                                     // x10 = length * 16 (byte offset)
    emitter.instruction("add x10, x0, x10");                                    // x10 = array base + byte offset
    emitter.instruction("add x10, x10, #24");                                   // x10 = skip header to data region
    emitter.instruction("str x1, [x10]");                                       // store string pointer at slot[0..8]
    emitter.instruction("str x2, [x10, #8]");                                   // store string length at slot[8..16]
    emitter.instruction("add x9, x9, #1");                                      // length += 1
    emitter.instruction("str x9, [x0]");                                        // write updated length back to header

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return (x0 = array pointer, unchanged)

    // -- slow path: grow array then push --
    emitter.label("__rt_array_push_str_grow");
    emitter.instruction("bl __rt_array_grow");                                  // double array capacity → x0 = new array
    emitter.instruction("str x0, [sp, #0]");                                    // update saved array pointer
    emitter.instruction("b __rt_array_push_str_push");                          // go push into the grown array
}
