use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_filter_refcounted: filter a refcounted array using a callback, returning a new array.
/// Input: x0 = callback function address, x1 = source array pointer
/// Output: x0 = pointer to new filtered array
pub fn emit_array_filter_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_filter_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_filter_refcounted ---");
    emitter.label_global("__rt_array_filter_refcounted");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #48]");                             // save callee-saved x19 and x20
    emitter.instruction("str x21, [sp, #40]");                                  // save callee-saved x21
    emitter.instruction("str x0, [sp, #0]");                                    // save callback address
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer
    emitter.instruction("mov x19, x0");                                         // keep callback address in callee-saved register

    // -- read source length and create destination array --
    emitter.instruction("ldr x9, [x1]");                                        // load source array length
    emitter.instruction("str x9, [sp, #16]");                                   // save source array length
    emitter.instruction("mov x0, x9");                                          // use source length as destination capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #24]");                                   // save destination array pointer
    emitter.instruction("mov x20, #0");                                         // initialize source index
    emitter.instruction("mov x21, #0");                                         // initialize destination length tracker

    emitter.label("__rt_array_filter_ref_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload source length
    emitter.instruction("cmp x20, x9");                                         // compare source index with source length
    emitter.instruction("b.ge __rt_array_filter_ref_done");                     // finish once every source element has been examined
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("add x1, x1, #24");                                     // compute source data base
    emitter.instruction("ldr x0, [x1, x20, lsl #3]");                           // load source element for callback
    emitter.instruction("str x0, [sp, #32]");                                   // preserve source element across callback
    emitter.instruction("blr x19");                                             // call callback with source element in x0
    emitter.instruction("cbz x0, __rt_array_filter_ref_skip");                  // skip element when callback returned falsy
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload borrowed source payload
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained payload into destination array
    emitter.instruction("str x0, [sp, #24]");                                   // persist destination pointer after possible growth
    emitter.instruction("add x21, x21, #1");                                    // track number of kept elements

    emitter.label("__rt_array_filter_ref_skip");
    emitter.instruction("add x20, x20, #1");                                    // increment source index
    emitter.instruction("b __rt_array_filter_ref_loop");                        // continue filtering

    emitter.label("__rt_array_filter_ref_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload destination array pointer
    emitter.instruction("str x21, [x0]");                                       // set filtered array length
    emitter.instruction("ldr x21, [sp, #40]");                                  // restore callee-saved x21
    emitter.instruction("ldp x19, x20, [sp, #48]");                             // restore callee-saved x19 and x20
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return filtered array
}

fn emit_array_filter_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_filter_refcounted ---");
    emitter.label_global("__rt_array_filter_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted-filter spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved source array, destination array, and candidate payload
    emitter.instruction("push r12");                                            // preserve the callback address register because the filter loop calls through it repeatedly
    emitter.instruction("push r13");                                            // preserve the source-index register because the loop keeps it live across callback invocations
    emitter.instruction("push r14");                                            // preserve the destination-length register because kept-element count survives callback invocations
    emitter.instruction("sub rsp, 32");                                         // reserve local slots for the source array pointer, source length, destination array pointer, and borrowed candidate payload
    emitter.instruction("mov r12, rdi");                                        // keep the callback address in a callee-saved register across the filtering loop
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the source array pointer so the loop can reload it after callback and append helper calls
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // load the source array length from the first field of the array header
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the source array length across the destination-array allocation call
    emitter.instruction("mov rdi, r10");                                        // pass the source array length as the maximum destination capacity to __rt_array_new
    emitter.instruction("mov rsi, 8");                                          // request 8-byte slots because the filtered payloads are retained heap pointers
    emitter.instruction("call __rt_array_new");                                 // allocate the destination array that will retain the kept refcounted payloads
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the destination array pointer for the filtering loop and final return path
    emitter.instruction("xor r13d, r13d");                                      // start the source index at zero before scanning the source array
    emitter.instruction("xor r14d, r14d");                                      // start the destination kept-element count at zero before the first callback

    emitter.label("__rt_array_filter_ref_loop");
    emitter.instruction("cmp r13, QWORD PTR [rbp - 40]");                       // stop once the source index reaches the saved source-array length
    emitter.instruction("jge __rt_array_filter_ref_done");                      // finish filtering once every source element has been tested
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the source array pointer after the previous callback or append helper call
    emitter.instruction("mov rdi, QWORD PTR [r10 + r13 * 8 + 24]");             // load the current borrowed source payload into the callback argument register
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // preserve the borrowed candidate payload across the callback so truthy matches can retain it
    emitter.instruction("call r12");                                            // invoke the user callback with the current borrowed payload and read the truthy result from rax
    emitter.instruction("test rax, rax");                                       // check whether the callback reported a truthy keep/skip decision
    emitter.instruction("jz __rt_array_filter_ref_skip");                       // skip retaining/copying the payload when the callback returned zero / false
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the destination array pointer as the first append helper argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the borrowed payload to retain and append into the destination array
    emitter.instruction("call __rt_array_push_refcounted");                     // retain the kept payload and append it into the destination array, returning the possibly-grown array pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // persist the destination array pointer after the refcounted append helper may have reallocated storage
    emitter.instruction("add r14, 1");                                          // advance the destination kept-element count after retaining and appending a payload

    emitter.label("__rt_array_filter_ref_skip");
    emitter.instruction("add r13, 1");                                          // advance the source index after examining the current source payload
    emitter.instruction("jmp __rt_array_filter_ref_loop");                      // continue filtering until the whole source array has been examined

    emitter.label("__rt_array_filter_ref_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the destination array pointer for final length publication and return
    emitter.instruction("mov QWORD PTR [rax], r14");                            // publish the number of kept payloads as the destination array logical length
    emitter.instruction("add rsp, 32");                                         // release the refcounted-filter local bookkeeping slots before restoring callee-saved registers
    emitter.instruction("pop r14");                                             // restore the caller destination-length callee-saved register
    emitter.instruction("pop r13");                                             // restore the caller source-index callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller callback callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the filtered array pointer
    emitter.instruction("ret");                                                 // return the filtered destination array pointer in rax
}
