use crate::codegen::emit::Emitter;

/// __rt_cstr_to_str: convert a null-terminated C string to an owned elephc string.
/// Input:  x0 = pointer to null-terminated C string
/// Output: x1 = heap-allocated string pointer, x2 = computed length
pub fn emit_cstr_to_str(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2"); // ensure 4-byte alignment for ARM64 instructions
    emitter.comment("--- runtime: cstr_to_str ---");
    emitter.label_global("__rt_cstr_to_str");

    // -- handle null pointer --
    emitter.instruction("cbz x0, __rt_cstr_to_str_null");                       // null pointer → empty string

    // -- preserve source pointer and scan for null terminator --
    emitter.instruction("mov x9, x0");                                          // preserve original C string pointer for the copy pass
    emitter.instruction("mov x2, #0");                                          // length counter = 0

    emitter.label("__rt_cstr_to_str_loop");
    emitter.instruction("ldrb w3, [x9, x2]");                                   // load byte at offset x2 from the C string
    emitter.instruction("cbz w3, __rt_cstr_to_str_done");                       // null terminator found
    emitter.instruction("add x2, x2, #1");                                      // increment length
    emitter.instruction("b __rt_cstr_to_str_loop");                             // continue scanning

    emitter.label("__rt_cstr_to_str_null");
    emitter.instruction("mov x1, #0");                                          // null pointer → empty string pointer
    emitter.instruction("mov x2, #0");                                          // null pointer → zero length
    emitter.instruction("ret");                                                 // return empty string

    emitter.label("__rt_cstr_to_str_done");
    // -- allocate and copy owned elephc string bytes --
    emitter.instruction("sub sp, sp, #32");                                     // allocate stack space for frame and saved metadata
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address before nested call
    emitter.instruction("add x29, sp, #16");                                    // establish a frame pointer for this helper
    emitter.instruction("str x9, [sp, #0]");                                    // preserve source pointer across heap allocation
    emitter.instruction("str x2, [sp, #8]");                                    // preserve computed length across heap allocation
    emitter.instruction("mov x0, x2");                                          // allocation size = computed string length
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate owned elephc string storage
    emitter.instruction("mov x3, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x3, [x0, #-8]");                                   // store string kind in the uniform heap header
    emitter.instruction("ldr x9, [sp, #0]");                                    // restore source pointer after allocation
    emitter.instruction("ldr x2, [sp, #8]");                                    // restore computed length after allocation
    emitter.instruction("mov x1, x0");                                          // x1 = result pointer for elephc string
    emitter.instruction("mov x10, x1");                                         // keep destination pointer for the byte copy loop
    emitter.instruction("mov x11, x9");                                         // keep source pointer for the byte copy loop
    emitter.instruction("mov x12, x2");                                         // copy remaining length into loop counter

    emitter.label("__rt_cstr_to_str_copy_loop");
    emitter.instruction("cbz x12, __rt_cstr_to_str_copy_done");                 // stop copying once all bytes are moved
    emitter.instruction("ldrb w3, [x11], #1");                                  // load one byte from the C string and advance source
    emitter.instruction("strb w3, [x10], #1");                                  // store one byte into the owned elephc buffer
    emitter.instruction("sub x12, x12, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_cstr_to_str_copy_loop");                        // continue copying

    emitter.label("__rt_cstr_to_str_copy_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate helper stack frame
    emitter.instruction("ret");                                                 // return x1=ptr, x2=len
}
