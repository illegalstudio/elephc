use crate::codegen::emit::Emitter;

/// array_grow: double the capacity of an array, copying all elements.
/// Allocates a new array with 2x capacity, copies header + elements,
/// and returns the new pointer without freeing the old array.
///
/// This intentionally prefers leaking over use-after-free: callers may still
/// hold aliases to the previous storage after a growth-triggering mutation.
/// Input:  x0 = old array pointer
/// Output: x0 = new array pointer (with doubled capacity)
pub fn emit_array_grow(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_grow ---");
    emitter.label("__rt_array_grow");

    // -- set up stack frame --
    // Stack layout:
    //   [sp, #0]  = old array pointer
    //   [sp, #8]  = old length
    //   [sp, #16] = old elem_size
    //   [sp, #24] = new capacity
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save old array pointer

    // -- read old array header --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = old length
    emitter.instruction("str x9, [sp, #8]");                                    // save old length
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = old capacity
    emitter.instruction("ldr x11, [x0, #16]");                                  // x11 = elem_size
    emitter.instruction("str x11, [sp, #16]");                                  // save elem_size

    // -- compute new capacity (2x old, minimum 8) --
    emitter.instruction("lsl x12, x10, #1");                                    // x12 = old_capacity * 2
    emitter.instruction("cmp x12, #8");                                         // at least 8 elements
    emitter.instruction("b.ge __rt_array_grow_alloc");                          // skip if already >= 8
    emitter.instruction("mov x12, #8");                                         // minimum capacity = 8
    emitter.label("__rt_array_grow_alloc");
    emitter.instruction("str x12, [sp, #24]");                                  // save new capacity

    // -- allocate new array: 24 + new_capacity * elem_size --
    emitter.instruction("mul x0, x12, x11");                                    // x0 = new_capacity * elem_size
    emitter.instruction("add x0, x0, #24");                                     // x0 = total bytes (header + data)
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate new array → x0
    emitter.instruction("mov x14, #2");                                         // heap kind 2 = indexed array
    emitter.instruction("str x14, [x0, #-8]");                                  // store indexed-array kind in the uniform heap header

    // -- write new array header --
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload old length
    emitter.instruction("str x9, [x0]");                                        // new_array.length = old length
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload new capacity
    emitter.instruction("str x12, [x0, #8]");                                   // new_array.capacity = new capacity
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload elem_size
    emitter.instruction("str x11, [x0, #16]");                                  // new_array.elem_size = old elem_size

    // -- copy elements from old array to new array --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = old array pointer
    emitter.instruction("add x1, x1, #24");                                     // x1 = old data start (skip header)
    emitter.instruction("add x2, x0, #24");                                     // x2 = new data start (skip header)
    emitter.instruction("mul x3, x9, x11");                                     // x3 = bytes to copy (length * elem_size)
    emitter.instruction("str x0, [sp, #24]");                                   // save new array ptr (reusing slot)

    // -- byte-copy loop --
    emitter.label("__rt_array_grow_copy");
    emitter.instruction("cbz x3, __rt_array_grow_done");                        // all bytes copied
    emitter.instruction("ldrb w4, [x1], #1");                                   // load byte from old, advance
    emitter.instruction("strb w4, [x2], #1");                                   // store byte to new, advance
    emitter.instruction("sub x3, x3, #1");                                      // decrement remaining
    emitter.instruction("b __rt_array_grow_copy");                              // continue copying

    // -- return new array pointer --
    emitter.label("__rt_array_grow_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = new array pointer

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new array
}
