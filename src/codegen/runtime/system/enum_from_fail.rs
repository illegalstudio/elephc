use crate::codegen::emit::Emitter;

pub fn emit_enum_from_fail(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: enum_from_fail ---");
    emitter.label_global("__rt_enum_from_fail");
    emitter.adrp("x1", "_enum_from_msg");                        // load the enum-from error message page
    emitter.add_lo12("x1", "x1", "_enum_from_msg");                  // resolve the enum-from error message address
    emitter.instruction("mov x2, #33");                                         // byte length of the enum-from error message
    emitter.instruction("mov x0, #2");                                          // write diagnostics to stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #70");                                         // use EX_SOFTWARE as the process exit status
    emitter.syscall(1);
}
