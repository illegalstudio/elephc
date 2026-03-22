use crate::codegen::emit::Emitter;

/// chr: convert int to single-character string.
/// Input: x0 = char code
/// Output: x1 = ptr, x2 = 1
pub fn emit_chr(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: chr ---");
    emitter.label("__rt_chr");

    // -- get concat_buf write position --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve exact address of offset variable
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load page address of concat buffer
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve exact buffer base address

    // -- store single character --
    emitter.instruction("add x1, x7, x8");                                      // compute write position, set as return ptr
    emitter.instruction("strb w0, [x1]");                                       // store the character byte at that position
    emitter.instruction("add x8, x8, #1");                                      // advance offset by 1 byte
    emitter.instruction("str x8, [x6]");                                        // store updated offset to _concat_off
    emitter.instruction("mov x2, #1");                                          // return length = 1 (single character)
    emitter.instruction("ret");                                                 // return to caller
}
