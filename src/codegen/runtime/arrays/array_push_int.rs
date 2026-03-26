use crate::codegen::emit::Emitter;

/// array_push_int: push an integer element to an array, growing if needed.
/// Input:  x0 = array pointer, x1 = value
/// Output: x0 = array pointer (may differ if array was reallocated)
pub fn emit_array_push_int(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_int ---");
    emitter.label("__rt_array_push_int");

    // -- check capacity before pushing --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = array capacity
    emitter.instruction("cmp x9, x10");                                         // is the array full?
    emitter.instruction("b.ge __rt_array_push_int_grow");                       // grow array if at capacity

    // -- fast path: push directly --
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region (skip 24-byte header)
    emitter.instruction("str x1, [x10, x9, lsl #3]");                           // store value at data[length * 8] (8 bytes per int)
    emitter.instruction("add x9, x9, #1");                                      // length += 1
    emitter.instruction("str x9, [x0]");                                        // write updated length back to header
    emitter.instruction("ret");                                                 // return to caller (x0 unchanged)

    // -- slow path: grow array then push --
    emitter.label("__rt_array_push_int_grow");
    emitter.instruction("sub sp, sp, #32");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save value to push

    emitter.instruction("bl __rt_array_grow");                                  // double array capacity → x0 = new array

    emitter.instruction("ldr x1, [sp, #0]");                                    // restore value to push
    emitter.instruction("ldr x9, [x0]");                                        // reload length from new array
    emitter.instruction("add x10, x0, #24");                                    // x10 = data region of new array
    emitter.instruction("str x1, [x10, x9, lsl #3]");                           // store value at data[length * 8]
    emitter.instruction("add x9, x9, #1");                                      // length += 1
    emitter.instruction("str x9, [x0]");                                        // update length in new array

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new array
}
