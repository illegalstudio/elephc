use crate::codegen::emit::Emitter;

pub fn emit_enum_from_fail(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: enum_from_fail ---");
    emitter.label_global("__rt_enum_from_fail");
    emitter.instruction("adrp x1, _enum_from_msg@PAGE");                        // load the enum-from error message page
    emitter.instruction("add x1, x1, _enum_from_msg@PAGEOFF");                  // resolve the enum-from error message address
    emitter.instruction("mov x2, #33");                                         // byte length of the enum-from error message
    emitter.instruction("mov x0, #2");                                          // write diagnostics to stderr
    emitter.instruction("mov x16, #4");                                         // syscall 4 = write
    emitter.instruction("svc #0x80");                                           // print the fatal enum-from message
    emitter.instruction("mov x0, #70");                                         // use EX_SOFTWARE as the process exit status
    emitter.instruction("mov x16, #1");                                         // syscall 1 = exit
    emitter.instruction("svc #0x80");                                           // terminate immediately after the fatal error
}
