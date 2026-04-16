use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_map_str: apply a callback to each element of an array, returning a new string array.
/// Handles both int and string source arrays (detects elem_size from header).
/// Input: x0 = callback function address, x1 = source array pointer
/// Output: x0 = pointer to new array with string elements (elem_size=16)
pub fn emit_array_map_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_map_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_map_str ---");
    emitter.label_global("__rt_array_map_str");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #32]");                             // save callee-saved x19, x20
    emitter.instruction("str x0, [sp, #0]");                                    // save callback address to stack
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer to stack
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)

    // -- read source array metadata --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #16]");                                   // save length to stack
    emitter.instruction("ldr x10, [x1, #16]");                                  // x10 = source elem_size (8=int, 16=str)
    emitter.instruction("str x10, [sp, #24]");                                  // save source elem_size to stack

    // -- create new result array with elem_size=16 (string output) --
    emitter.instruction("mov x0, x9");                                          // x0 = capacity for new array
    emitter.instruction("mov x1, #16");                                         // x1 = element size (16 bytes for string)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array → x0
    emitter.instruction("mov x20, x0");                                         // x20 = new array pointer (callee-saved)

    // -- set up loop counter --
    emitter.instruction("mov x0, #0");                                          // x0 = loop index i = 0
    emitter.instruction("str x0, [sp, #0]");                                    // reuse sp+0 for loop index (callback addr in x19)

    // -- loop: apply callback to each element --
    emitter.label("__rt_array_map_str_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // load loop index
    emitter.instruction("ldr x9, [sp, #16]");                                   // load source length
    emitter.instruction("cmp x0, x9");                                          // compare i with length
    emitter.instruction("b.ge __rt_array_map_str_done");                        // if i >= length, loop complete

    // -- load element from source array based on elem_size --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload source elem_size
    emitter.instruction("add x1, x1, #24");                                     // skip header to data region
    emitter.instruction("mul x11, x0, x10");                                    // x11 = i * elem_size
    emitter.instruction("add x11, x1, x11");                                    // x11 = &source_data[i]

    emitter.instruction("cmp x10, #16");                                        // is source a string array?
    emitter.instruction("b.eq __rt_array_map_str_load_str");                    // yes — load ptr+len

    // -- int source: pass element in x0 (first int param) --
    emitter.instruction("ldr x0, [x11]");                                       // x0 = int element
    emitter.instruction("b __rt_array_map_str_call");                           // proceed to call

    // -- string source: pass element in x0/x1 (first string param = 2 int regs) --
    emitter.label("__rt_array_map_str_load_str");
    emitter.instruction("ldr x0, [x11]");                                       // x0 = string pointer (first half)
    emitter.instruction("ldr x1, [x11, #8]");                                   // x1 = string length (second half)

    // -- call callback --
    emitter.label("__rt_array_map_str_call");
    emitter.instruction("blr x19");                                             // call callback → string result in x1=ptr, x2=len

    // -- persist string result to heap --
    emitter.instruction("bl __rt_str_persist");                                 // copy string to heap, x1=heap_ptr, x2=len

    // -- store string result in new array --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload loop index
    emitter.instruction("add x9, x20, #24");                                    // new array data region
    emitter.instruction("lsl x10, x0, #4");                                     // x10 = i * 16 (string stride)
    emitter.instruction("str x1, [x9, x10]");                                   // store string pointer
    emitter.instruction("add x10, x10, #8");                                    // advance to length slot
    emitter.instruction("str x2, [x9, x10]");                                   // store string length

    // -- advance loop --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload loop index
    emitter.instruction("add x0, x0, #1");                                      // i += 1
    emitter.instruction("str x0, [sp, #0]");                                    // save updated index
    emitter.instruction("b __rt_array_map_str_loop");                           // continue loop

    // -- set length on new array and return --
    emitter.label("__rt_array_map_str_done");
    emitter.instruction("mov x0, x20");                                         // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = length
    emitter.instruction("str x9, [x0]");                                        // set new array length

    // -- tear down stack frame and return --
    emitter.instruction("ldp x19, x20, [sp, #32]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new mapped string array
}

fn emit_array_map_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_map_str ---");
    emitter.label_global("__rt_array_map_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving string-map spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the callback, source array metadata, and destination array pointer
    emitter.instruction("push r12");                                            // preserve the callback address register because the mapping loop calls through it repeatedly
    emitter.instruction("push r13");                                            // preserve the loop-index register because the mapping loop keeps it live across callback invocations
    emitter.instruction("sub rsp, 32");                                         // reserve local slots for the source array pointer, source length, source elem_size, and destination array pointer
    emitter.instruction("mov r12, rdi");                                        // keep the callback address in a callee-saved register across the string-mapping loop
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the source array pointer so the loop can reload it after callback and persist helper calls
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // load the source array length from the first field of the indexed-array header
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the source array length across the destination-array allocation call
    emitter.instruction("mov r11, QWORD PTR [rsi + 16]");                       // load the source element stride so the loop can distinguish scalar and string inputs
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the source element stride across the destination-array allocation call
    emitter.instruction("mov rdi, r10");                                        // pass the source array length as the destination capacity to __rt_array_new
    emitter.instruction("mov rsi, 16");                                         // request 16-byte destination slots because array_map_str always returns strings
    emitter.instruction("call __rt_array_new");                                 // allocate the destination string array with the same logical capacity as the source array
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the destination array pointer for the loop body and final return path
    emitter.instruction("xor r13d, r13d");                                      // start the string-mapping loop at logical index zero

    emitter.label("__rt_array_map_str_loop");
    emitter.instruction("cmp r13, QWORD PTR [rbp - 32]");                       // stop once the loop index reaches the saved source-array length
    emitter.instruction("jge __rt_array_map_str_done");                         // exit the mapping loop when every source element has been transformed into a string
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the source array pointer after the previous callback/persist helper calls
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the source element stride so the loop can decode the current source slot
    emitter.instruction("mov rcx, r13");                                        // copy the logical source index before scaling it by the source element stride
    emitter.instruction("imul rcx, r11");                                       // convert the logical index into the byte offset of the current source slot
    emitter.instruction("lea rcx, [r10 + rcx + 24]");                           // compute the address of the current source slot inside the indexed-array payload region
    emitter.instruction("cmp r11, 16");                                         // does the source array already contain string ptr/len pairs?
    emitter.instruction("je __rt_array_map_str_load_str");                      // branch to the string-input path when the current source slot is a 16-byte string pair
    emitter.instruction("mov rdi, QWORD PTR [rcx]");                            // load the scalar source element into the first SysV integer argument register for the callback
    emitter.instruction("jmp __rt_array_map_str_call");                         // continue into the shared callback invocation path

    emitter.label("__rt_array_map_str_load_str");
    emitter.instruction("mov rdi, QWORD PTR [rcx]");                            // load the source string pointer into the first SysV integer argument register for the callback
    emitter.instruction("mov rsi, QWORD PTR [rcx + 8]");                        // load the source string length into the second SysV integer argument register for the callback

    emitter.label("__rt_array_map_str_call");
    emitter.instruction("call r12");                                            // invoke the user callback and read the produced string result from rax=ptr, rdx=len
    emitter.instruction("mov rsi, rax");                                        // move the callback-produced string pointer into the x86_64 array-push string payload register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the destination array pointer into the x86_64 array-push receiver register
    emitter.instruction("call __rt_array_push_str");                            // persist and append the callback-produced string into the destination array, returning the possibly-grown array pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // persist the destination array pointer after the string-append helper may have reallocated storage
    emitter.instruction("add r13, 1");                                          // advance the loop index after materializing the mapped destination string slot
    emitter.instruction("jmp __rt_array_map_str_loop");                         // continue mapping until the source array has been fully consumed

    emitter.label("__rt_array_map_str_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the destination array pointer for final length publication and return
    emitter.instruction("add rsp, 32");                                         // release the string-map local bookkeeping slots before restoring callee-saved registers
    emitter.instruction("pop r13");                                             // restore the caller loop-index callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller callback callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the mapped string array pointer
    emitter.instruction("ret");                                                 // return the mapped destination string array pointer in rax
}
