use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// mixed_instanceof: unwrap a boxed mixed value and run an object metadata check.
/// Input:  x0=mixed cell, x1=target id, x2=0 class / 1 interface
/// Output: x0=1 when the boxed payload is an object matching the target, else 0
pub fn emit_mixed_instanceof(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_instanceof_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_instanceof ---");
    emitter.label_global("__rt_mixed_instanceof");

    // -- preserve target metadata while the mixed unbox helper rewrites x0/x1/x2 --
    emitter.instruction("sub sp, sp, #32");                                     // allocate a small frame for target metadata and return state
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address around nested helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable frame pointer for this helper
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save target id and target kind before unboxing the mixed payload
    emitter.instruction("bl __rt_mixed_unbox");                                 // unwrap nested mixed cells to a concrete runtime tag and payload
    emitter.instruction("cmp x0, #6");                                          // check whether the boxed payload is an object
    emitter.instruction("b.ne __rt_mixed_instanceof_no");                       // non-object payloads never satisfy instanceof
    emitter.instruction("mov x0, x1");                                          // pass the unboxed object pointer to the metadata matcher
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // restore target id and target kind for the metadata matcher
    emitter.instruction("bl __rt_exception_matches");                           // test the object against class inheritance or interface metadata
    emitter.instruction("b __rt_mixed_instanceof_done");                        // keep the matcher result and restore this helper's frame

    emitter.label("__rt_mixed_instanceof_no");
    emitter.instruction("mov x0, #0");                                          // return false for scalar, array, null, and unknown mixed payloads

    emitter.label("__rt_mixed_instanceof_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address after nested helper calls
    emitter.instruction("add sp, sp, #32");                                     // release this helper's stack frame
    emitter.instruction("ret");                                                 // return the boolean instanceof result in x0
}

fn emit_mixed_instanceof_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_instanceof ---");
    emitter.label_global("__rt_mixed_instanceof");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer for this helper
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for saved target metadata
    emitter.instruction("sub rsp, 16");                                         // reserve slots for target id and target kind
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save target id before mixed_unbox rewrites argument registers
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save target kind before mixed_unbox rewrites argument registers
    emitter.instruction("mov rax, rdi");                                        // move the boxed mixed pointer into the mixed_unbox input register
    emitter.instruction("call __rt_mixed_unbox");                               // unwrap nested mixed cells to a concrete runtime tag and payload
    emitter.instruction("cmp rax, 6");                                          // check whether the boxed payload is an object
    emitter.instruction("jne __rt_mixed_instanceof_no");                        // non-object payloads never satisfy instanceof
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // restore target class/interface id for the metadata matcher
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore target kind for the metadata matcher
    emitter.instruction("call __rt_exception_matches");                         // test the object in rdi against class/interface metadata
    emitter.instruction("jmp __rt_mixed_instanceof_done");                      // keep the matcher result and restore this helper's frame

    emitter.label("__rt_mixed_instanceof_no");
    emitter.instruction("xor eax, eax");                                        // return false for scalar, array, null, and unknown mixed payloads

    emitter.label("__rt_mixed_instanceof_done");
    emitter.instruction("add rsp, 16");                                         // release saved target metadata slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the boolean instanceof result in rax
}
