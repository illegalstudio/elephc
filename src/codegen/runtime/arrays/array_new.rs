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
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save capacity to stack (need it after bl)
    emitter.instruction("str x1, [sp, #8]");                                    // save elem_size to stack (need it after bl)
    emitter.instruction("str xzr, [sp, #16]");                                  // keep a reserved scratch slot for future array metadata helpers

    // -- calculate total bytes needed: 24-byte header + (capacity * elem_size) --
    emitter.instruction("mul x2, x0, x1");                                      // x2 = capacity * elem_size = data region size
    emitter.instruction("add x0, x2, #24");                                     // x0 = data size + 24-byte header
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate memory, x0 = pointer to array
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload elem_size for the default packed metadata choice
    emitter.instruction("cmp x9, #16");                                         // does the array store 16-byte string payloads?
    emitter.instruction("cset x9, eq");                                         // 16-byte arrays default to string value_type tag 1, others to 0
    emitter.instruction("lsl x9, x9, #8");                                      // move the value_type tag into the packed kind-word byte lane
    emitter.instruction("mov x10, #0x8000");                                    // bit 15 marks heap containers that participate in copy-on-write
    emitter.instruction("orr x9, x9, x10");                                     // preserve the persistent copy-on-write container flag in the kind word
    emitter.instruction("add x9, x9, #2");                                      // low byte 2 = indexed array heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // store the packed indexed-array kind word in the heap header

    // -- initialize the array header fields --
    emitter.instruction("str xzr, [x0]");                                       // header[0]: length = 0 (array starts empty)
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload capacity from stack
    emitter.instruction("str x9, [x0, #8]");                                    // header[8]: capacity = original x0 arg
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload elem_size from stack
    emitter.instruction("str x9, [x0, #16]");                                   // header[16]: elem_size = original x1 arg

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = array pointer
}
