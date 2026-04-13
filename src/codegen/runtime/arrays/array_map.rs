use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_map: apply a callback to each element of an integer array, returning a new array.
/// Input: x0 = callback function address, x1 = source array pointer
/// Output: x0 = pointer to new array with transformed elements
pub fn emit_array_map(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_map_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_map ---");
    emitter.label_global("__rt_array_map");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #32]");                             // save callee-saved x19, x20
    emitter.instruction("str x0, [sp, #0]");                                    // save callback address to stack
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer to stack
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)

    // -- read source array length and create new array --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #16]");                                   // save length to stack
    emitter.instruction("mov x0, x9");                                          // x0 = capacity for new array
    emitter.instruction("mov x1, #8");                                          // x1 = element size (8 bytes for int)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array → x0=new array ptr
    emitter.instruction("str x0, [sp, #24]");                                   // save new array pointer to stack

    // -- set up loop counter --
    emitter.instruction("mov x20, #0");                                         // x20 = loop index i = 0

    // -- loop: apply callback to each element --
    emitter.label("__rt_array_map_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // load source length
    emitter.instruction("cmp x20, x9");                                         // compare i with length
    emitter.instruction("b.ge __rt_array_map_done");                            // if i >= length, loop complete

    // -- load element from source array --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("add x1, x1, #24");                                     // skip header to data region
    emitter.instruction("ldr x0, [x1, x20, lsl #3]");                           // x0 = source[i]

    // -- call callback with element as argument --
    emitter.instruction("blr x19");                                             // call callback(element) → result in x0

    // -- store result in new array --
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload new array pointer
    emitter.instruction("add x2, x1, #24");                                     // skip header to data region
    emitter.instruction("str x0, [x2, x20, lsl #3]");                           // new_array[i] = callback result

    // -- advance loop --
    emitter.instruction("add x20, x20, #1");                                    // i += 1
    emitter.instruction("b __rt_array_map_loop");                               // continue loop

    // -- set length on new array and return --
    emitter.label("__rt_array_map_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = length
    emitter.instruction("str x9, [x0]");                                        // set new array length

    // -- tear down stack frame and return --
    emitter.instruction("ldp x19, x20, [sp, #32]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new mapped array
}

fn emit_array_map_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_map ---");
    emitter.label_global("__rt_array_map");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving array-map spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the callback, source array, and destination array slots
    emitter.instruction("push r12");                                            // preserve the callback scratch register because the runtime uses it across every callback invocation
    emitter.instruction("push r13");                                            // preserve the loop-index scratch register because the runtime keeps it live across callback calls
    emitter.instruction("sub rsp, 24");                                         // reserve local slots for the source array pointer, source length, and destination array pointer
    emitter.instruction("mov r12, rdi");                                        // keep the callback address in a callee-saved register across the mapping loop
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the source array pointer so the loop can reload it after callback calls
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // load the source array length from the first field of the array header
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the source array length across the destination-array allocation call
    emitter.instruction("mov rdi, r10");                                        // pass the source array length as the destination capacity to __rt_array_new
    emitter.instruction("mov rsi, 8");                                          // request 8-byte element slots for the integer-returning array_map runtime
    emitter.instruction("call __rt_array_new");                                 // allocate the destination array with the same logical capacity as the source array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the destination array pointer for the loop body and final return path
    emitter.instruction("xor r13d, r13d");                                      // start the mapping loop at logical index zero

    emitter.label("__rt_array_map_loop");
    emitter.instruction("cmp r13, QWORD PTR [rbp - 32]");                       // stop once the loop index reaches the saved source array length
    emitter.instruction("jge __rt_array_map_done");                             // exit the mapping loop when every source element has been transformed
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the source array pointer after the previous callback invocation
    emitter.instruction("mov rdi, QWORD PTR [r10 + r13 * 8 + 24]");             // load the current source element into the first SysV integer argument register
    emitter.instruction("call r12");                                            // invoke the user callback with the current element and read the transformed value from rax
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the destination array pointer after the callback clobbered caller-saved registers
    emitter.instruction("mov QWORD PTR [r10 + r13 * 8 + 24], rax");             // store the transformed value into the matching destination-array element slot
    emitter.instruction("add r13, 1");                                          // advance the loop index after storing the transformed destination element
    emitter.instruction("jmp __rt_array_map_loop");                             // continue mapping until the source array has been fully consumed

    emitter.label("__rt_array_map_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the destination array pointer for final length publication and return
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the saved source length so the destination logical length matches the mapped input size
    emitter.instruction("mov QWORD PTR [rax], r10");                            // publish the mapped destination length in the destination array header
    emitter.instruction("add rsp, 24");                                         // release the local source/destination bookkeeping slots before restoring callee-saved registers
    emitter.instruction("pop r13");                                             // restore the caller's loop-index callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller's callback scratch callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the mapped array pointer
    emitter.instruction("ret");                                                 // return the mapped destination array pointer in rax
}
