use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// mixed_from_value: retain/persist a runtime value and box it into a mixed cell.
/// Input:  x0=value_tag, x1=value_lo, x2=value_hi
/// Output: x0=boxed mixed pointer
pub fn emit_mixed_from_value(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_from_value_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_from_value ---");
    emitter.label_global("__rt_mixed_from_value");

    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame for the incoming payload and boxed result
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the runtime value tag across helper calls
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the incoming payload words across helper calls

    emitter.instruction("cmp x0, #1");                                          // does this mixed payload hold a string?
    emitter.instruction("b.eq __rt_mixed_from_value_string");                   // strings must be persisted for the boxed owner
    emitter.instruction("cmp x0, #4");                                          // does this mixed payload hold an indexed array?
    emitter.instruction("b.eq __rt_mixed_from_value_retain");                   // refcounted child pointers must be retained for the boxed owner
    emitter.instruction("cmp x0, #5");                                          // does this mixed payload hold an associative array?
    emitter.instruction("b.eq __rt_mixed_from_value_retain");                   // refcounted child pointers must be retained for the boxed owner
    emitter.instruction("cmp x0, #6");                                          // does this mixed payload hold an object?
    emitter.instruction("b.eq __rt_mixed_from_value_retain");                   // refcounted child pointers must be retained for the boxed owner
    emitter.instruction("cmp x0, #7");                                          // does this mixed payload hold another mixed cell?
    emitter.instruction("b.eq __rt_mixed_from_value_retain");                   // nested mixed cells must also be retained
    emitter.instruction("b __rt_mixed_from_value_alloc");                       // scalars can be boxed without additional retention

    emitter.label("__rt_mixed_from_value_string");
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the string payload for the boxed owner
    emitter.instruction("stp x1, x2, [sp, #8]");                                // replace the saved payload with the owned string pointer and length
    emitter.instruction("b __rt_mixed_from_value_alloc");                       // continue with allocation after persisting the string

    emitter.label("__rt_mixed_from_value_retain");
    emitter.instruction("mov x0, x1");                                          // move the child heap pointer into the incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain the shared child pointer for the boxed owner

    emitter.label("__rt_mixed_from_value_alloc");
    emitter.instruction("mov x0, #24");                                         // mixed cells store tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the mixed cell storage
    emitter.instruction("mov x9, #5");                                          // low byte 5 = mixed cell heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // install the mixed-cell heap kind in the uniform header
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the saved runtime value tag
    emitter.instruction("str x10, [x0]");                                       // store the runtime value tag at mixed[0]
    emitter.instruction("ldp x11, x12, [sp, #8]");                              // reload the normalized payload words
    emitter.instruction("stp x11, x12, [x0, #8]");                              // store the payload words at mixed[8] and mixed[16]
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the boxed mixed pointer in x0
}

fn emit_mixed_from_value_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_from_value ---");
    emitter.label_global("__rt_mixed_from_value");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before boxing the mixed payload
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the temporary payload spill
    emitter.instruction("sub rsp, 24");                                         // reserve local slots for tag, value_lo, and value_hi
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the runtime value tag across the malloc call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the low payload word across the malloc call
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the high payload word across the malloc call
    emitter.instruction("mov rdi, 24");                                         // mixed cells store tag plus two payload words
    emitter.instruction("call malloc");                                         // allocate the mixed cell storage with libc malloc
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the saved runtime value tag after malloc
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the runtime value tag at mixed[0]
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the saved low payload word after malloc
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the low payload word at mixed[8]
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the saved high payload word after malloc
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store the high payload word at mixed[16]
    emitter.instruction("add rsp, 24");                                         // release the temporary payload spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the boxed mixed pointer in rax
}
