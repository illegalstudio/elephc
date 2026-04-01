use crate::codegen::emit::Emitter;

/// __rt_str_to_cstr: copy an elephc string into a freshly allocated C string.
/// Input:  x1 = pointer to string bytes, x2 = length
/// Output: x0 = pointer to heap-allocated null-terminated string
pub fn emit_str_to_cstr(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2"); // ensure 4-byte alignment for ARM64 instructions
    emitter.comment("--- runtime: str_to_cstr ---");
    emitter.label_global("__rt_str_to_cstr");

    // -- save return state before calling heap allocator --
    emitter.instruction("sub sp, sp, #32");                                     // allocate stack space for frame and saved arguments
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a frame pointer for this helper

    // -- allocate len + 1 bytes for copied C string --
    emitter.instruction("str x1, [sp, #0]");                                    // preserve source pointer across heap allocation
    emitter.instruction("str x2, [sp, #8]");                                    // preserve source length across heap allocation
    emitter.instruction("add x0, x2, #1");                                      // requested allocation size = payload + trailing null
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate writable buffer on elephc heap
    emitter.instruction("ldr x1, [sp, #0]");                                    // restore source pointer after allocation
    emitter.instruction("ldr x2, [sp, #8]");                                    // restore source length after allocation

    // -- preserve destination and source state across the copy loop --
    emitter.instruction("mov x9, x0");                                          // keep destination pointer for the final return value
    emitter.instruction("mov x10, x1");                                         // copy source pointer into a scratch register
    emitter.instruction("mov x11, x2");                                         // copy remaining byte count into loop counter

    emitter.label("__rt_str_to_cstr_loop");
    emitter.instruction("cbz x11, __rt_str_to_cstr_done");                      // finish once every source byte has been copied
    emitter.instruction("ldrb w12, [x10], #1");                                 // load one source byte and advance the source pointer
    emitter.instruction("strb w12, [x0], #1");                                  // store one byte into the destination and advance it
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_str_to_cstr_loop");                             // continue copying bytes

    emitter.label("__rt_str_to_cstr_done");
    emitter.instruction("strb wzr, [x0]");                                      // append the trailing null terminator
    emitter.instruction("mov x0, x9");                                          // return the original destination pointer
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and caller return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate local stack frame
    emitter.instruction("ret");                                                 // return to caller
}
