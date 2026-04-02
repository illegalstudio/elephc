use crate::codegen::emit::Emitter;

pub fn emit_buffer_bounds_fail(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_bounds_fail ---");
    emitter.label_global("__rt_buffer_bounds_fail");
    emitter.instruction("adrp x1, _buffer_bounds_msg@PAGE");                    // load the error message page
    emitter.instruction("add x1, x1, _buffer_bounds_msg@PAGEOFF");              // resolve the buffer bounds message address
    emitter.instruction("mov x2, #40");                                         // byte length of the fixed buffer bounds error message
    emitter.instruction("mov x0, #2");                                          // write diagnostics to stderr
    emitter.instruction("mov x16, #4");                                         // syscall 4 = write
    emitter.instruction("svc #0x80");                                           // print the fatal buffer bounds message
    emitter.instruction("mov x0, #70");                                         // use EX_SOFTWARE as the process exit status
    emitter.instruction("mov x16, #1");                                         // syscall 1 = exit
    emitter.instruction("svc #0x80");                                           // terminate immediately after the fatal error
}
