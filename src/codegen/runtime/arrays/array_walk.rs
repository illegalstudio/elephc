use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_walk: call a callback on each element of an integer array (no return value).
/// Input: x0 = callback function address, x1 = source array pointer
/// Output: none (void)
pub fn emit_array_walk(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_walk_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_walk ---");
    emitter.label_global("__rt_array_walk");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #16]");                             // save callee-saved x19, x20
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)
    emitter.instruction("str x1, [sp, #0]");                                    // save source array pointer to stack

    // -- read source array length --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save length to stack

    // -- set up loop counter --
    emitter.instruction("mov x20, #0");                                         // x20 = loop index i = 0

    // -- loop: call callback on each element --
    emitter.label("__rt_array_walk_loop");
    emitter.instruction("ldr x9, [sp, #8]");                                    // load source length
    emitter.instruction("cmp x20, x9");                                         // compare i with length
    emitter.instruction("b.ge __rt_array_walk_done");                           // if i >= length, loop complete

    // -- load element from source array --
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x1, x1, #24");                                     // skip header to data region
    emitter.instruction("ldr x0, [x1, x20, lsl #3]");                           // x0 = source[i]

    // -- call callback with element (ignore return value) --
    emitter.instruction("blr x19");                                             // call callback(element)

    // -- advance loop --
    emitter.instruction("add x20, x20, #1");                                    // i += 1
    emitter.instruction("b __rt_array_walk_loop");                              // continue loop

    // -- done --
    emitter.label("__rt_array_walk_done");

    // -- tear down stack frame and return --
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return (void)
}

fn emit_array_walk_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_walk ---");
    emitter.label_global("__rt_array_walk");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving walk spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved source array and source length
    emitter.instruction("push r12");                                            // preserve the callback address register because the walk loop calls through it repeatedly
    emitter.instruction("push r13");                                            // preserve the source-index register because the loop keeps it live across callback invocations
    emitter.instruction("sub rsp, 16");                                         // reserve local slots for the source array pointer and source length
    emitter.instruction("mov r12, rdi");                                        // keep the callback address in a callee-saved register across the walk loop
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the source array pointer so the loop can reload it after callback calls
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // load the source array length from the first field of the array header
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the source array length for loop termination checks
    emitter.instruction("xor r13d, r13d");                                      // start the source index at zero before walking the source array

    emitter.label("__rt_array_walk_loop");
    emitter.instruction("cmp r13, QWORD PTR [rbp - 32]");                       // stop once the source index reaches the saved source-array length
    emitter.instruction("jge __rt_array_walk_done");                            // finish walking once every source element has been passed to the callback
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the source array pointer after the previous callback invocation
    emitter.instruction("mov rdi, QWORD PTR [r10 + r13 * 8 + 24]");             // load the current source element into the first SysV integer argument register
    emitter.instruction("call r12");                                            // invoke the user callback with the current source element and ignore the scalar return value
    emitter.instruction("add r13, 1");                                          // advance the source index after visiting the current element
    emitter.instruction("jmp __rt_array_walk_loop");                            // continue walking until the whole source array has been visited

    emitter.label("__rt_array_walk_done");
    emitter.instruction("add rsp, 16");                                         // release the walk local bookkeeping slots before restoring callee-saved registers
    emitter.instruction("pop r13");                                             // restore the caller source-index callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller callback callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning void
    emitter.instruction("ret");                                                 // return to the caller after walking the whole source array
}
