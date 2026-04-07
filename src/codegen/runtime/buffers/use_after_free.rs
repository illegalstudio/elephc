use crate::codegen::emit::Emitter;

pub fn emit_buffer_use_after_free(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_use_after_free ---");
    emitter.label_global("__rt_buffer_use_after_free");
    emitter.adrp("x1", "_buffer_uaf_msg");                       // load the error message page
    emitter.add_lo12("x1", "x1", "_buffer_uaf_msg");                 // resolve the use-after-free message address
    emitter.instruction("mov x2, #47");                                         // byte length of the use-after-free error message
    emitter.instruction("mov x0, #2");                                          // write diagnostics to stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #70");                                         // use EX_SOFTWARE as the process exit status
    emitter.syscall(1);
}
