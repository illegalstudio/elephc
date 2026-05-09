use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_filter_refcounted: filter a refcounted array using a callback, returning a new array.
/// Input: x0 = callback function address, x1 = source array pointer, x2 = optional callback environment pointer
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
    emitter.instruction("sub sp, sp, #96");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #64]");                             // save callee-saved x19 and x20
    emitter.instruction("str x21, [sp, #56]");                                  // save callee-saved x21
    emitter.instruction("str x2, [sp, #0]");                                    // save optional callback environment pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer
    emitter.instruction("mov x19, x0");                                         // keep callback address in callee-saved register

    // -- read source length and create destination array --
    emitter.instruction("ldr x9, [x1]");                                        // load source array length
    emitter.instruction("str x9, [sp, #16]");                                   // save source array length
    emitter.instruction("ldr x10, [x1, #16]");                                  // load source element width for pointer/string dispatch
    emitter.instruction("str x10, [sp, #32]");                                  // save source element width across callback calls
    emitter.instruction("mov x0, x9");                                          // use source length as destination capacity
    emitter.instruction("mov x1, x10");                                         // use the same element width as the source array
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
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload source element width
    emitter.instruction("cmp x10, #16");                                        // does this refcounted array contain string ptr/len slots?
    emitter.instruction("b.eq __rt_array_filter_ref_load_str");                 // use the string callback ABI for 16-byte elements
    emitter.instruction("ldr x0, [x1, x20, lsl #3]");                           // load source element for callback
    emitter.instruction("str x0, [sp, #40]");                                   // preserve source element across callback
    emitter.instruction("ldr x9, [sp, #0]");                                    // load optional callback environment pointer
    emitter.instruction("cbz x9, __rt_array_filter_ref_call");                  // keep legacy one-argument callback ABI when no environment is present
    emitter.instruction("mov x1, x9");                                          // pass capture environment as the wrapper's second argument
    emitter.instruction("b __rt_array_filter_ref_call");                        // call through the shared callback branch
    emitter.label("__rt_array_filter_ref_load_str");
    emitter.instruction("lsl x11, x20, #4");                                    // compute source string byte offset from the logical index
    emitter.instruction("add x11, x1, x11");                                    // compute address of the current source string slot
    emitter.instruction("ldr x0, [x11]");                                       // load source string pointer for callback
    emitter.instruction("ldr x1, [x11, #8]");                                   // load source string length for callback
    emitter.instruction("str x0, [sp, #40]");                                   // preserve source string pointer across callback
    emitter.instruction("str x1, [sp, #48]");                                   // preserve source string length across callback
    emitter.instruction("ldr x9, [sp, #0]");                                    // load optional callback environment pointer
    emitter.instruction("cbz x9, __rt_array_filter_ref_call");                  // keep legacy string callback ABI when no environment is present
    emitter.instruction("mov x2, x9");                                          // pass capture environment after the string ptr/len pair
    emitter.label("__rt_array_filter_ref_call");
    emitter.instruction("blr x19");                                             // call callback with source element in x0
    emitter.instruction("cbz x0, __rt_array_filter_ref_skip");                  // skip element when callback returned falsy
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload source element width for kept-element append
    emitter.instruction("cmp x10, #16");                                        // is the kept element a string slot?
    emitter.instruction("b.eq __rt_array_filter_ref_keep_str");                 // append kept string payloads with the string helper
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload borrowed source payload
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained payload into destination array
    emitter.instruction("str x0, [sp, #24]");                                   // persist destination pointer after possible growth
    emitter.instruction("add x21, x21, #1");                                    // track number of kept elements
    emitter.instruction("b __rt_array_filter_ref_skip");                        // advance to the next source element
    emitter.label("__rt_array_filter_ref_keep_str");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload destination array pointer for string append
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload kept source string pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // reload kept source string length
    emitter.instruction("bl __rt_array_push_str");                              // copy and append the kept string into the destination array
    emitter.instruction("str x0, [sp, #24]");                                   // persist destination pointer after possible growth
    emitter.instruction("add x21, x21, #1");                                    // track number of kept string elements

    emitter.label("__rt_array_filter_ref_skip");
    emitter.instruction("add x20, x20, #1");                                    // increment source index
    emitter.instruction("b __rt_array_filter_ref_loop");                        // continue filtering

    emitter.label("__rt_array_filter_ref_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload destination array pointer
    emitter.instruction("str x21, [x0]");                                       // set filtered array length
    emitter.instruction("ldr x21, [sp, #56]");                                  // restore callee-saved x21
    emitter.instruction("ldp x19, x20, [sp, #64]");                             // restore callee-saved x19 and x20
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate stack frame
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
    emitter.instruction("sub rsp, 56");                                         // reserve local slots for refcounted-filter bookkeeping and optional callback environment
    emitter.instruction("mov r12, rdi");                                        // keep the callback address in a callee-saved register across the filtering loop
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the source array pointer so the loop can reload it after callback and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save optional callback environment pointer for captured-closure wrappers
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // load the source array length from the first field of the array header
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the source array length across the destination-array allocation call
    emitter.instruction("mov r11, QWORD PTR [rsi + 16]");                       // load source element width for pointer/string dispatch
    emitter.instruction("mov QWORD PTR [rbp - 72], r11");                       // save source element width across callback calls
    emitter.instruction("mov rdi, r10");                                        // pass the source array length as the maximum destination capacity to __rt_array_new
    emitter.instruction("mov rsi, r11");                                        // request destination slots with the same width as the source array
    emitter.instruction("call __rt_array_new");                                 // allocate the destination array that will retain the kept refcounted payloads
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the destination array pointer for the filtering loop and final return path
    emitter.instruction("xor r13d, r13d");                                      // start the source index at zero before scanning the source array
    emitter.instruction("xor r14d, r14d");                                      // start the destination kept-element count at zero before the first callback

    emitter.label("__rt_array_filter_ref_loop");
    emitter.instruction("cmp r13, QWORD PTR [rbp - 40]");                       // stop once the source index reaches the saved source-array length
    emitter.instruction("jge __rt_array_filter_ref_done");                      // finish filtering once every source element has been tested
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the source array pointer after the previous callback or append helper call
    emitter.instruction("mov r11, QWORD PTR [rbp - 72]");                       // reload source element width
    emitter.instruction("cmp r11, 16");                                         // does this refcounted array contain string ptr/len slots?
    emitter.instruction("je __rt_array_filter_ref_load_str");                   // use the string callback ABI for 16-byte elements
    emitter.instruction("mov rdi, QWORD PTR [r10 + r13 * 8 + 24]");             // load the current borrowed source payload into the callback argument register
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // preserve the borrowed candidate payload across the callback so truthy matches can retain it
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // check whether this runtime call carries a callback capture environment
    emitter.instruction("je __rt_array_filter_ref_call");                       // keep legacy one-argument callback ABI when no environment is present
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // pass capture environment as the wrapper's second argument
    emitter.instruction("jmp __rt_array_filter_ref_call");                      // call through the shared callback branch
    emitter.label("__rt_array_filter_ref_load_str");
    emitter.instruction("mov rcx, r13");                                        // copy logical source index before scaling to a string slot offset
    emitter.instruction("shl rcx, 4");                                          // convert source index into a 16-byte string-slot offset
    emitter.instruction("lea rcx, [r10 + rcx + 24]");                           // compute address of the current source string slot
    emitter.instruction("mov rdi, QWORD PTR [rcx]");                            // load source string pointer for callback
    emitter.instruction("mov rsi, QWORD PTR [rcx + 8]");                        // load source string length for callback
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // preserve source string pointer across callback
    emitter.instruction("mov QWORD PTR [rbp - 80], rsi");                       // preserve source string length across callback
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // check whether this runtime call carries a callback capture environment
    emitter.instruction("je __rt_array_filter_ref_call");                       // keep legacy string callback ABI when no environment is present
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // pass capture environment after the string ptr/len pair
    emitter.label("__rt_array_filter_ref_call");
    emitter.instruction("call r12");                                            // invoke the user callback with the current borrowed payload and read the truthy result from rax
    emitter.instruction("test rax, rax");                                       // check whether the callback reported a truthy keep/skip decision
    emitter.instruction("jz __rt_array_filter_ref_skip");                       // skip retaining/copying the payload when the callback returned zero / false
    emitter.instruction("cmp QWORD PTR [rbp - 72], 16");                        // is the kept element a string slot?
    emitter.instruction("je __rt_array_filter_ref_keep_str");                   // append kept string payloads with the string helper
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the destination array pointer as the first append helper argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the borrowed payload to retain and append into the destination array
    emitter.instruction("call __rt_array_push_refcounted");                     // retain the kept payload and append it into the destination array, returning the possibly-grown array pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // persist the destination array pointer after the refcounted append helper may have reallocated storage
    emitter.instruction("add r14, 1");                                          // advance the destination kept-element count after retaining and appending a payload
    emitter.instruction("jmp __rt_array_filter_ref_skip");                      // advance to the next source element
    emitter.label("__rt_array_filter_ref_keep_str");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload destination array pointer for string append
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload kept source string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // reload kept source string length
    emitter.instruction("call __rt_array_push_str");                            // copy and append the kept string into the destination array
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // persist destination pointer after the string append helper may grow it
    emitter.instruction("add r14, 1");                                          // advance the destination kept-element count after appending a string

    emitter.label("__rt_array_filter_ref_skip");
    emitter.instruction("add r13, 1");                                          // advance the source index after examining the current source payload
    emitter.instruction("jmp __rt_array_filter_ref_loop");                      // continue filtering until the whole source array has been examined

    emitter.label("__rt_array_filter_ref_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the destination array pointer for final length publication and return
    emitter.instruction("mov QWORD PTR [rax], r14");                            // publish the number of kept payloads as the destination array logical length
    emitter.instruction("add rsp, 56");                                         // release the refcounted-filter local bookkeeping slots before restoring callee-saved registers
    emitter.instruction("pop r14");                                             // restore the caller destination-length callee-saved register
    emitter.instruction("pop r13");                                             // restore the caller source-index callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller callback callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the filtered array pointer
    emitter.instruction("ret");                                                 // return the filtered destination array pointer in rax
}
