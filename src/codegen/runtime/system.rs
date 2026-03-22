use crate::codegen::emit::Emitter;

/// build_argv: create a PHP $argv array from OS argc/argv.
/// Reads _global_argc and _global_argv, builds a string array.
/// Output: x0 = pointer to array
pub fn emit_build_argv(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: build_argv ---");
    emitter.label("__rt_build_argv");
    emitter.instruction("sub sp, sp, #48");
    emitter.instruction("stp x29, x30, [sp, #32]");
    emitter.instruction("add x29, sp, #32");

    // Load argc
    emitter.instruction("adrp x9, _global_argc@PAGE");
    emitter.instruction("add x9, x9, _global_argc@PAGEOFF");
    emitter.instruction("ldr x19, [x9]"); // x19 = argc (callee-saved)

    // Load argv pointer
    emitter.instruction("adrp x9, _global_argv@PAGE");
    emitter.instruction("add x9, x9, _global_argv@PAGEOFF");
    emitter.instruction("ldr x20, [x9]"); // x20 = argv (callee-saved)

    // Save callee-saved regs
    emitter.instruction("stp x19, x20, [sp, #0]");
    emitter.instruction("str x21, [sp, #16]");

    // Create string array: capacity = argc, elem_size = 16
    emitter.instruction("mov x0, x19");
    emitter.instruction("mov x1, #16");
    emitter.instruction("bl __rt_array_new");
    emitter.instruction("mov x21, x0"); // x21 = array ptr

    // Loop: for i = 0..argc, push each argv[i]
    emitter.instruction("mov x22, #0"); // i = 0 (use a stack slot)
    emitter.instruction("str x22, [sp, #24]");

    emitter.label("__rt_build_argv_loop");
    emitter.instruction("ldr x22, [sp, #24]");
    emitter.instruction("cmp x22, x19");
    emitter.instruction("b.ge __rt_build_argv_done");

    // x1 = argv[i] (C string pointer)
    emitter.instruction("ldr x1, [x20, x22, lsl #3]");

    // Compute string length (scan for \0)
    emitter.instruction("mov x2, #0");
    emitter.label("__rt_build_argv_strlen");
    emitter.instruction("ldrb w3, [x1, x2]");
    emitter.instruction("cbz w3, __rt_build_argv_push");
    emitter.instruction("add x2, x2, #1");
    emitter.instruction("b __rt_build_argv_strlen");

    // Push (x1=ptr, x2=len) to array
    emitter.label("__rt_build_argv_push");
    emitter.instruction("mov x0, x21"); // array ptr
    emitter.instruction("bl __rt_array_push_str");

    // i++
    emitter.instruction("ldr x22, [sp, #24]");
    emitter.instruction("add x22, x22, #1");
    emitter.instruction("str x22, [sp, #24]");
    emitter.instruction("b __rt_build_argv_loop");

    emitter.label("__rt_build_argv_done");
    emitter.instruction("mov x0, x21"); // return array ptr

    // Restore callee-saved
    emitter.instruction("ldp x19, x20, [sp, #0]");
    emitter.instruction("ldr x21, [sp, #16]");
    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");
}
