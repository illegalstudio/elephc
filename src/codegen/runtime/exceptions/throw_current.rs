use crate::codegen::{abi, emit::Emitter};
use crate::codegen::platform::Arch;

pub fn emit_throw_current(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_throw_current_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: throw_current ---");
    emitter.label_global("__rt_throw_current");

    // -- save callee-saved state while the throw helper inspects handler stacks --
    emitter.instruction("sub sp, sp, #48");                                     // reserve stack space for handler state and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address for the throw helper
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve callee-saved registers that hold handler metadata
    emitter.instruction("add x29, sp, #32");                                    // install the throw helper's frame pointer
    abi::emit_load_symbol_to_reg(emitter, "x19", "_exc_handler_top", 0);
    emitter.instruction("cbz x19, __rt_throw_current_uncaught");                // fall back to a fatal uncaught-exception path when no handler exists
    emitter.instruction("ldr x0, [x19, #8]");                                   // x0 = activation record that should survive this catch
    emitter.instruction("bl __rt_exception_cleanup_frames");                    // run cleanup callbacks for every unwound activation frame
    abi::emit_store_reg_to_symbol(emitter, "xzr", "_concat_off", 0);
    emitter.instruction("add x0, x19, #16");                                    // x0 = jmp_buf base stored inside the active handler record
    emitter.instruction("mov x1, #1");                                          // longjmp return value = 1 to indicate exceptional control flow
    emitter.bl_c("longjmp");                                         // transfer control directly back to the saved catch resume point

    // -- uncaught exceptions terminate the process with a fatal message --
    emitter.label("__rt_throw_current_uncaught");
    emitter.adrp("x1", "_uncaught_exc_msg");                     // load page of the uncaught-exception error message
    emitter.add_lo12("x1", "x1", "_uncaught_exc_msg");               // resolve the uncaught-exception error message address
    emitter.instruction("mov x2, #32");                                         // uncaught exception message length in bytes
    emitter.instruction("mov x0, #2");                                          // fd = stderr for fatal runtime diagnostics
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit status 1 indicates abnormal termination
    emitter.syscall(1);
}

fn emit_throw_current_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: throw_current ---");
    emitter.label_global("__rt_throw_current");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while the throw helper inspects handler stacks
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the x86_64 throw helper
    emitter.instruction("push r12");                                            // preserve the active handler record pointer across helper calls
    emitter.instruction("push r13");                                            // preserve the scratch callee-saved register used for the fatal path
    abi::emit_load_symbol_to_reg(emitter, "r12", "_exc_handler_top", 0);
    emitter.instruction("test r12, r12");                                       // is there an active exception handler to receive this throw?
    emitter.instruction("jz __rt_throw_current_uncaught");                       // fall back to a fatal uncaught-exception path when no handler exists
    emitter.instruction("mov rdi, QWORD PTR [r12 + 8]");                        // rdi = activation record that should survive this catch
    emitter.instruction("call __rt_exception_cleanup_frames");                  // run cleanup callbacks for every unwound activation frame
    abi::emit_store_zero_to_symbol(emitter, "_concat_off", 0);
    emitter.instruction("lea rdi, [r12 + 16]");                                 // rdi = jmp_buf base stored inside the active handler record
    emitter.instruction("mov esi, 1");                                          // longjmp return value = 1 to indicate exceptional control flow
    emitter.bl_c("longjmp");                                                    // transfer control directly back to the saved catch resume point

    emitter.label("__rt_throw_current_uncaught");
    abi::emit_symbol_address(emitter, "rsi", "_uncaught_exc_msg");
    emitter.instruction("mov edx, 32");                                         // uncaught exception message length in bytes
    emitter.instruction("mov edi, 2");                                          // fd = stderr for fatal runtime diagnostics
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write the fatal uncaught-exception message to stderr
    emitter.instruction("mov edi, 1");                                          // exit status 1 indicates abnormal termination
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process after reporting the uncaught exception
}
