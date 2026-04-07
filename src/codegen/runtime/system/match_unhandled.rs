use crate::codegen::emit::Emitter;

pub fn emit_match_unhandled(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: match_unhandled ---");
    emitter.label_global("__rt_match_unhandled");
    emitter.adrp("x1", "_match_unhandled_msg");                  // load the unhandled-match error message page
    emitter.add_lo12("x1", "x1", "_match_unhandled_msg");            // resolve the unhandled-match error message address
    emitter.instruction("mov x2, #34");                                         // byte length of the unhandled-match error message
    emitter.instruction("mov x0, #2");                                          // write diagnostics to stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #70");                                         // use EX_SOFTWARE as the process exit status
    emitter.syscall(1);
}
