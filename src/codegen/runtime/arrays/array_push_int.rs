use crate::codegen::emit::Emitter;

/// array_push_int: push an integer element to an array.
/// Input: x0 = array pointer, x1 = value
pub fn emit_array_push_int(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_int ---");
    emitter.label("__rt_array_push_int");

    // -- check capacity before pushing --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = array capacity
    emitter.instruction("cmp x9, x10");                                         // is the array full?
    emitter.instruction("b.ge __rt_array_push_int_err");                        // fatal error if at capacity

    // -- store the integer at the next available slot --
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region (skip 24-byte header)
    emitter.instruction("str x1, [x10, x9, lsl #3]");                           // store value at data[length * 8] (8 bytes per int)

    // -- increment the array length --
    emitter.instruction("add x9, x9, #1");                                      // length += 1
    emitter.instruction("str x9, [x0]");                                        // write updated length back to header
    emitter.instruction("ret");                                                 // return to caller

    // -- fatal error: array capacity exceeded --
    emitter.label("__rt_array_push_int_err");
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
