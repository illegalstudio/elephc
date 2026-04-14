use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_reduce: reduce an integer array to a single value using a callback.
/// Input: x0 = callback function address, x1 = source array pointer, x2 = initial value
/// Output: x0 = accumulated result
/// The callback receives (accumulator, element) and returns the new accumulator.
pub fn emit_array_reduce(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_reduce_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_reduce ---");
    emitter.label_global("__rt_array_reduce");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #32]");                             // save callee-saved x19, x20
    emitter.instruction("str x21, [sp, #24]");                                  // save callee-saved x21
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)
    emitter.instruction("str x1, [sp, #0]");                                    // save source array pointer to stack
    emitter.instruction("mov x21, x2");                                         // x21 = accumulator = initial value

    // -- read source array length --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save length to stack

    // -- set up loop counter --
    emitter.instruction("mov x20, #0");                                         // x20 = loop index i = 0

    // -- loop: apply callback to accumulator and each element --
    emitter.label("__rt_array_reduce_loop");
    emitter.instruction("ldr x9, [sp, #8]");                                    // load source length
    emitter.instruction("cmp x20, x9");                                         // compare i with length
    emitter.instruction("b.ge __rt_array_reduce_done");                         // if i >= length, loop complete

    // -- load element from source array --
    emitter.instruction("ldr x2, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x2, x2, #24");                                     // skip header to data region
    emitter.instruction("ldr x1, [x2, x20, lsl #3]");                           // x1 = source[i] (element)
    emitter.instruction("mov x0, x21");                                         // x0 = accumulator

    // -- call callback(accumulator, element) --
    emitter.instruction("blr x19");                                             // call callback → result in x0
    emitter.instruction("mov x21, x0");                                         // accumulator = callback result

    // -- advance loop --
    emitter.instruction("add x20, x20, #1");                                    // i += 1
    emitter.instruction("b __rt_array_reduce_loop");                            // continue loop

    // -- return accumulated result --
    emitter.label("__rt_array_reduce_done");
    emitter.instruction("mov x0, x21");                                         // x0 = final accumulated result

    // -- tear down stack frame and return --
    emitter.instruction("ldr x21, [sp, #24]");                                  // restore callee-saved x21
    emitter.instruction("ldp x19, x20, [sp, #32]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = accumulated value
}

fn emit_array_reduce_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_reduce ---");
    emitter.label_global("__rt_array_reduce");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving reduce spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved source array and source length
    emitter.instruction("push r12");                                            // preserve the callback address register because the reduce loop calls through it repeatedly
    emitter.instruction("push r13");                                            // preserve the source-index register because the loop keeps it live across callback invocations
    emitter.instruction("push r14");                                            // preserve the accumulator register because the loop keeps it live across callback invocations
    emitter.instruction("sub rsp, 16");                                         // reserve local slots for the source array pointer and source length
    emitter.instruction("mov r12, rdi");                                        // keep the callback address in a callee-saved register across the reduce loop
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the source array pointer so the loop can reload it after callback calls
    emitter.instruction("mov r14, rdx");                                        // keep the current accumulator in a callee-saved register across every callback invocation
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // load the source array length from the first field of the array header
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the source array length for loop termination checks
    emitter.instruction("xor r13d, r13d");                                      // start the source index at zero before reducing the source array

    emitter.label("__rt_array_reduce_loop");
    emitter.instruction("cmp r13, QWORD PTR [rbp - 40]");                       // stop once the source index reaches the saved source-array length
    emitter.instruction("jge __rt_array_reduce_done");                          // finish reduction once every source element has been folded into the accumulator
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the source array pointer after the previous callback invocation
    emitter.instruction("mov rsi, QWORD PTR [r10 + r13 * 8 + 24]");             // load the current source element into the second SysV integer argument register
    emitter.instruction("mov rdi, r14");                                        // move the current accumulator into the first SysV integer argument register
    emitter.instruction("call r12");                                            // invoke the user callback with (accumulator, element) and read the new accumulator from rax
    emitter.instruction("mov r14, rax");                                        // update the live accumulator with the callback result before the next iteration
    emitter.instruction("add r13, 1");                                          // advance the source index after folding the current element
    emitter.instruction("jmp __rt_array_reduce_loop");                          // continue reducing until the whole source array has been consumed

    emitter.label("__rt_array_reduce_done");
    emitter.instruction("mov rax, r14");                                        // move the final accumulator into the x86_64 integer return register
    emitter.instruction("add rsp, 16");                                         // release the reduce local bookkeeping slots before restoring callee-saved registers
    emitter.instruction("pop r14");                                             // restore the caller accumulator callee-saved register
    emitter.instruction("pop r13");                                             // restore the caller source-index callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller callback callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the reduced accumulator
    emitter.instruction("ret");                                                 // return the reduced accumulator in rax
}
