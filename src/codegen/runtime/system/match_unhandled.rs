use crate::codegen::emit::Emitter;

pub fn emit_match_unhandled(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: match_unhandled ---");
    emitter.label_global("__rt_match_unhandled");
    emitter.instruction("adrp x1, _match_unhandled_msg@PAGE");                    // load the unhandled-match error message page
    emitter.instruction("add x1, x1, _match_unhandled_msg@PAGEOFF");              // resolve the unhandled-match error message address
    emitter.instruction("mov x2, #34");                                           // byte length of the unhandled-match error message
    emitter.instruction("mov x0, #2");                                            // write diagnostics to stderr
    emitter.instruction("mov x16, #4");                                           // syscall 4 = write
    emitter.instruction("svc #0x80");                                             // print the fatal unhandled-match message
    emitter.instruction("mov x0, #70");                                           // use EX_SOFTWARE as the process exit status
    emitter.instruction("mov x16, #1");                                           // syscall 1 = exit
    emitter.instruction("svc #0x80");                                             // terminate immediately after the fatal error
}
