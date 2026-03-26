use crate::codegen::emit::Emitter;

/// array_push_str: push a string element (ptr+len) to an array.
/// Input: x0 = array pointer, x1 = str ptr, x2 = str len
pub fn emit_array_push_str(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_str ---");
    emitter.label("__rt_array_push_str");

    // -- check capacity before pushing --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = array capacity
    emitter.instruction("cmp x9, x10");                                         // is the array full?
    emitter.instruction("b.ge __rt_array_push_str_err");                        // fatal error if at capacity

    // -- compute address of the next string slot (16 bytes per element) --
    emitter.instruction("lsl x10, x9, #4");                                     // x10 = length * 16 (byte offset, 16 bytes per string)
    emitter.instruction("add x10, x0, x10");                                    // x10 = array base + byte offset
    emitter.instruction("add x10, x10, #24");                                   // x10 = skip 24-byte header to reach data region

    // -- store the string pointer and length as a pair --
    emitter.instruction("str x1, [x10]");                                       // store string pointer at slot[0..8]
    emitter.instruction("str x2, [x10, #8]");                                   // store string length at slot[8..16]

    // -- increment the array length --
    emitter.instruction("add x9, x9, #1");                                      // length += 1
    emitter.instruction("str x9, [x0]");                                        // write updated length back to header
    emitter.instruction("ret");                                                 // return to caller

    // -- fatal error: array capacity exceeded --
    emitter.label("__rt_array_push_str_err");
    emitter.instruction("mov x0, #2");                                          // fd = stderr
    emitter.instruction("adrp x1, _arr_cap_err_msg@PAGE");                      // load page of error message
    emitter.instruction("add x1, x1, _arr_cap_err_msg@PAGEOFF");                // resolve error message address
    emitter.instruction("mov x2, #38");                                         // message length
    emitter.instruction("mov x16, #4");                                         // syscall 4 = sys_write
    emitter.instruction("svc #0x80");                                           // write error to stderr
    emitter.instruction("mov x0, #1");                                          // exit code 1
    emitter.instruction("mov x16, #1");                                         // syscall 1 = sys_exit
    emitter.instruction("svc #0x80");                                           // terminate process
}
