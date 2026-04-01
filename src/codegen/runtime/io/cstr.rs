use crate::codegen::emit::Emitter;

/// cstr: convert an elephc string (x1=ptr, x2=len) to a null-terminated C string.
/// Uses _cstr_buf (4096 bytes) as scratch space.
/// Input:  x1=ptr, x2=len
/// Output: x0=pointer to null-terminated string in _cstr_buf
///
/// cstr2: same but uses _cstr_buf2 for a second path (needed by rename/copy).
/// Input:  x1=ptr, x2=len
/// Output: x0=pointer to null-terminated string in _cstr_buf2
pub fn emit_cstr(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: cstr ---");
    emitter.label_global("__rt_cstr");

    // -- load destination buffer address --
    emitter.instruction("adrp x9, _cstr_buf@PAGE");                             // load page address of cstr scratch buffer
    emitter.instruction("add x9, x9, _cstr_buf@PAGEOFF");                       // resolve exact address of cstr buffer

    // -- copy bytes from source to buffer --
    emitter.instruction("mov x10, x9");                                         // save buffer start for return value
    emitter.instruction("mov x11, x2");                                         // copy length as loop counter
    emitter.label("__rt_cstr_loop");
    emitter.instruction("cbz x11, __rt_cstr_null");                             // if no bytes remain, append null terminator
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance source ptr
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to buffer, advance buffer ptr
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_cstr_loop");                                    // continue copying

    // -- append null terminator and return --
    emitter.label("__rt_cstr_null");
    emitter.instruction("strb wzr, [x9]");                                      // write null terminator after last byte
    emitter.instruction("mov x0, x10");                                         // return pointer to null-terminated string
    emitter.instruction("ret");                                                 // return to caller

    emitter.blank();
    emitter.comment("--- runtime: cstr2 ---");
    emitter.label_global("__rt_cstr2");

    // -- load second buffer address --
    emitter.instruction("adrp x9, _cstr_buf2@PAGE");                            // load page address of second cstr buffer
    emitter.instruction("add x9, x9, _cstr_buf2@PAGEOFF");                      // resolve exact address of second buffer

    // -- copy bytes from source to buffer --
    emitter.instruction("mov x10, x9");                                         // save buffer start for return value
    emitter.instruction("mov x11, x2");                                         // copy length as loop counter
    emitter.label("__rt_cstr2_loop");
    emitter.instruction("cbz x11, __rt_cstr2_null");                            // if no bytes remain, append null terminator
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance source ptr
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to buffer, advance buffer ptr
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_cstr2_loop");                                   // continue copying

    // -- append null terminator and return --
    emitter.label("__rt_cstr2_null");
    emitter.instruction("strb wzr, [x9]");                                      // write null terminator after last byte
    emitter.instruction("mov x0, x10");                                         // return pointer to null-terminated string
    emitter.instruction("ret");                                                 // return to caller
}
