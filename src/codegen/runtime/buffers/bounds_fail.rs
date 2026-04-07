use crate::codegen::emit::Emitter;

pub fn emit_buffer_bounds_fail(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_bounds_fail ---");
    emitter.label_global("__rt_buffer_bounds_fail");
    emitter.adrp("x1", "_buffer_bounds_msg");                    // load the error message page
    emitter.add_lo12("x1", "x1", "_buffer_bounds_msg");              // resolve the buffer bounds message address
    emitter.instruction("mov x2, #40");                                         // byte length of the fixed buffer bounds error message
    emitter.instruction("mov x0, #2");                                          // write diagnostics to stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #70");                                         // use EX_SOFTWARE as the process exit status
    emitter.syscall(1);
}
