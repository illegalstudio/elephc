use crate::codegen::emit::Emitter;

/// __rt_cstr_to_str: convert a null-terminated C string to an elephc string.
/// Input:  x0 = pointer to null-terminated C string
/// Output: x1 = pointer (same as input), x2 = computed length
pub fn emit_cstr_to_str(emitter: &mut Emitter) {
    emitter.blank();
    emitter.raw("    .p2align 2");                                              // ensure 4-byte alignment for ARM64 instructions
    emitter.comment("--- runtime: cstr_to_str ---");
    emitter.label("__rt_cstr_to_str");

    // -- handle null pointer --
    emitter.instruction("cbz x0, __rt_cstr_to_str_null");                      // null pointer → empty string

    // -- scan for null terminator --
    emitter.instruction("mov x1, x0");                                          // output pointer = input pointer
    emitter.instruction("mov x2, #0");                                          // length counter = 0

    emitter.label("__rt_cstr_to_str_loop");
    emitter.instruction("ldrb w3, [x1, x2]");                                  // load byte at offset x2
    emitter.instruction("cbz w3, __rt_cstr_to_str_done");                      // null terminator found
    emitter.instruction("add x2, x2, #1");                                      // increment length
    emitter.instruction("b __rt_cstr_to_str_loop");                             // continue scanning

    emitter.label("__rt_cstr_to_str_null");
    emitter.instruction("mov x1, #0");                                          // null pointer → empty string pointer
    emitter.instruction("mov x2, #0");                                          // null pointer → zero length

    emitter.label("__rt_cstr_to_str_done");
    emitter.instruction("ret");                                                 // return x1=ptr, x2=len
}
