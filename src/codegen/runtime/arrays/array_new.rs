use crate::codegen::emit::Emitter;

/// array_new: create a new array on the heap.
/// Input: x0 = capacity, x1 = element size (8 or 16)
/// Output: x0 = pointer to array header
/// Layout: [length:8][capacity:8][elem_size:8][elements...]
pub fn emit_array_new(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_new ---");
    emitter.label("__rt_array_new");

    // -- set up stack frame, save arguments for use after heap_alloc call --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save capacity to stack (need it after bl)
    emitter.instruction("str x1, [sp, #8]");                                    // save elem_size to stack (need it after bl)

    // -- calculate total bytes needed: 24-byte header + (capacity * elem_size) --
    emitter.instruction("mul x2, x0, x1");                                      // x2 = capacity * elem_size = data region size
    emitter.instruction("add x0, x2, #24");                                     // x0 = data size + 24-byte header
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate memory, x0 = pointer to array
    emitter.instruction("mov x9, #2");                                          // heap kind 2 = indexed array
    emitter.instruction("str x9, [x0, #-8]");                                   // store indexed-array kind in the uniform heap header

    // -- initialize the array header fields --
    emitter.instruction("str xzr, [x0]");                                       // header[0]: length = 0 (array starts empty)
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload capacity from stack
    emitter.instruction("str x9, [x0, #8]");                                    // header[8]: capacity = original x0 arg
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload elem_size from stack
    emitter.instruction("str x9, [x0, #16]");                                   // header[16]: elem_size = original x1 arg

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = array pointer
}
