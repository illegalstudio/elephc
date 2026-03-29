use crate::codegen::emit::Emitter;

/// array_merge_into: append all elements from source array to dest array (in-place).
/// Input: x0 = dest array pointer, x1 = source array pointer
/// Both arrays must have 8-byte elements.
pub fn emit_array_merge_into(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_merge_into ---");
    emitter.label("__rt_array_merge_into");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save dest array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer

    // -- check if source is empty --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("cbz x9, __rt_ami_done");                               // if source is empty, nothing to do

    // -- ensure dest has enough capacity --
    emitter.instruction("ldr x10, [x0]");                                       // x10 = dest array length
    emitter.instruction("ldr x11, [x0, #8]");                                   // x11 = dest array capacity
    emitter.instruction("add x12, x10, x9");                                    // x12 = needed capacity (dest_len + src_len)
    emitter.label("__rt_ami_grow_check");
    emitter.instruction("cmp x12, x11");                                        // check if we need to grow
    emitter.instruction("b.le __rt_ami_copy");                                  // skip resize if capacity is enough
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload current dest array pointer before growth
    emitter.instruction("bl __rt_array_grow");                                  // grow dest array storage until it can hold the merge result
    emitter.instruction("str x0, [sp, #0]");                                    // persist the possibly-moved dest array pointer
    emitter.instruction("ldr x11, [x0, #8]");                                   // reload dest capacity after growth
    emitter.instruction("b __rt_ami_grow_check");                               // keep growing until the required capacity fits

    emitter.label("__rt_ami_copy");
    // -- copy elements from source to dest --
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = dest array pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = source array pointer
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source length
    emitter.instruction("ldr x10, [x0]");                                       // x10 = dest current length
    emitter.instruction("add x2, x1, #24");                                     // x2 = source data base
    emitter.instruction("add x3, x0, #24");                                     // x3 = dest data base
    emitter.instruction("mov x4, #0");                                          // x4 = loop index i = 0

    emitter.label("__rt_ami_loop");
    emitter.instruction("cmp x4, x9");                                          // compare i with source length
    emitter.instruction("b.ge __rt_ami_set_len");                               // if done, set new length
    emitter.instruction("ldr x5, [x2, x4, lsl #3]");                            // x5 = source[i]
    emitter.instruction("add x6, x10, x4");                                     // x6 = dest_len + i (target index)
    emitter.instruction("str x5, [x3, x6, lsl #3]");                            // dest[dest_len + i] = source[i]
    emitter.instruction("add x4, x4, #1");                                      // i += 1
    emitter.instruction("b __rt_ami_loop");                                     // continue loop

    emitter.label("__rt_ami_set_len");
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = dest array pointer
    emitter.instruction("ldr x10, [x0]");                                       // x10 = dest old length
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = source pointer
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source length
    emitter.instruction("add x10, x10, x9");                                    // x10 = new total length
    emitter.instruction("str x10, [x0]");                                       // update dest length

    emitter.label("__rt_ami_done");
    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
