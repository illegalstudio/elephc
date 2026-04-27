use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

pub(crate) fn emit_diagnostics(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_diagnostics_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: diagnostics ---");

    emitter.label_global("__rt_diag_push_suppression");
    emitter.adrp("x9", "_rt_diag_suppression");
    emitter.add_lo12("x9", "x9", "_rt_diag_suppression");
    emitter.instruction("ldr x10, [x9]");                                       // load the current nested diagnostic-suppression depth
    emitter.instruction("add x10, x10, #1");                                    // enter one additional diagnostic-suppression scope
    emitter.instruction("str x10, [x9]");                                       // publish the incremented diagnostic-suppression depth
    emitter.instruction("ret");                                                 // return to the suppressed expression wrapper

    emitter.label_global("__rt_diag_pop_suppression");
    emitter.adrp("x9", "_rt_diag_suppression");
    emitter.add_lo12("x9", "x9", "_rt_diag_suppression");
    emitter.instruction("ldr x10, [x9]");                                       // load the current nested diagnostic-suppression depth
    emitter.instruction("cbz x10, __rt_diag_pop_done");                         // avoid underflow if suppression scopes are already balanced
    emitter.instruction("sub x10, x10, #1");                                    // leave one diagnostic-suppression scope
    emitter.instruction("str x10, [x9]");                                       // publish the decremented diagnostic-suppression depth
    emitter.label("__rt_diag_pop_done");
    emitter.instruction("ret");                                                 // return to the expression wrapper after restoring suppression state

    emitter.label_global("__rt_diag_warning");
    emitter.adrp("x9", "_rt_diag_suppression");
    emitter.add_lo12("x9", "x9", "_rt_diag_suppression");
    emitter.instruction("ldr x10, [x9]");                                       // load suppression depth before deciding whether to emit the warning
    emitter.instruction("cbnz x10, __rt_diag_warning_done");                    // suppress the warning while inside an active @ scope
    emitter.instruction("mov x0, #2");                                          // fd = stderr for runtime warning diagnostics
    emitter.syscall(4);
    emitter.label("__rt_diag_warning_done");
    emitter.instruction("ret");                                                 // return after either writing or suppressing the warning
}

fn emit_diagnostics_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: diagnostics ---");

    emitter.label_global("__rt_diag_push_suppression");
    emitter.instruction("mov r10, QWORD PTR [rip + _rt_diag_suppression]");     // load the current nested diagnostic-suppression depth
    emitter.instruction("add r10, 1");                                          // enter one additional diagnostic-suppression scope
    emitter.instruction("mov QWORD PTR [rip + _rt_diag_suppression], r10");     // publish the incremented diagnostic-suppression depth
    emitter.instruction("ret");                                                 // return to the suppressed expression wrapper

    emitter.label_global("__rt_diag_pop_suppression");
    emitter.instruction("mov r10, QWORD PTR [rip + _rt_diag_suppression]");     // load the current nested diagnostic-suppression depth
    emitter.instruction("test r10, r10");                                       // check whether a suppression scope is active before decrementing
    emitter.instruction("jz __rt_diag_pop_done_linux_x86_64");                  // avoid underflow if suppression scopes are already balanced
    emitter.instruction("sub r10, 1");                                          // leave one diagnostic-suppression scope
    emitter.instruction("mov QWORD PTR [rip + _rt_diag_suppression], r10");     // publish the decremented diagnostic-suppression depth
    emitter.label("__rt_diag_pop_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return to the expression wrapper after restoring suppression state

    emitter.label_global("__rt_diag_warning");
    emitter.instruction("mov r10, QWORD PTR [rip + _rt_diag_suppression]");     // load suppression depth before deciding whether to emit the warning
    emitter.instruction("test r10, r10");                                       // is runtime warning output currently suppressed?
    emitter.instruction("jnz __rt_diag_warning_done_linux_x86_64");             // suppress the warning while inside an active @ scope
    emitter.instruction("mov rdx, rsi");                                        // move warning length into the Linux write length register
    emitter.instruction("mov rsi, rdi");                                        // move warning pointer into the Linux write buffer register
    emitter.instruction("mov edi, 2");                                          // fd = stderr for runtime warning diagnostics
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the runtime warning diagnostic to stderr
    emitter.label("__rt_diag_warning_done_linux_x86_64");
    emitter.instruction("ret");                                                 // return after either writing or suppressing the warning
}
