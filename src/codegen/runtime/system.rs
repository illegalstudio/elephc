use crate::codegen::emit::Emitter;

/// argv: get command-line argument by index.
/// Input:  x0 = argument index
/// Output: x1 = string pointer, x2 = string length
pub fn emit_argv(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: argv ---");
    emitter.label("__rt_argv");
    emitter.instruction("adrp x9, _global_argv@PAGE");
    emitter.instruction("add x9, x9, _global_argv@PAGEOFF");
    emitter.instruction("ldr x9, [x9]");
    emitter.instruction("ldr x1, [x9, x0, lsl #3]");

    emitter.instruction("mov x2, #0");
    emitter.label("__rt_argv_len");
    emitter.instruction("ldrb w3, [x1, x2]");
    emitter.instruction("cbz w3, __rt_argv_done");
    emitter.instruction("add x2, x2, #1");
    emitter.instruction("b __rt_argv_len");

    emitter.label("__rt_argv_done");
    emitter.instruction("ret");
}
