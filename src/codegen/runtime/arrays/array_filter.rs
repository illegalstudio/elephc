//! Purpose:
//! Emits the `__rt_array_filter`, `__rt_array_new` runtime helper assembly for array filter.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_array_filter` runtime helper.
/// Dispatches to the x86_64 Linux SysV ABI variant; ARM64 uses the default implementation below.
/// Iterates over `source_array`, calling `callback(element)` for each. Elements where the callback
/// returns non-zero are copied into a newly allocated array. The new array length is set to the
/// count of kept elements. Caller-saved registers are preserved across the callback loop.
/// Input: x0 = callback address, x1 = source array ptr, x2 = optional callback environment
/// Output: x0 = new array containing only elements where callback returned truthy
pub fn emit_array_filter(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_filter_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_filter ---");
    emitter.label_global("__rt_array_filter");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #48]");                             // save callee-saved x19, x20
    emitter.instruction("str x21, [sp, #40]");                                  // save callee-saved x21
    emitter.instruction("str x2, [sp, #0]");                                    // save optional callback environment pointer to stack
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer to stack
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)

    // -- read source array length and create new array --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #16]");                                   // save length to stack
    emitter.instruction("mov x0, x9");                                          // x0 = capacity for new array (same size max)
    emitter.instruction("mov x1, #8");                                          // x1 = element size (8 bytes for int)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array → x0=new array ptr
    emitter.instruction("str x0, [sp, #24]");                                   // save new array pointer to stack

    // -- set up loop counters --
    emitter.instruction("mov x20, #0");                                         // x20 = source index i = 0
    emitter.instruction("mov x21, #0");                                         // x21 = dest index j = 0

    // -- loop: test each element with callback --
    emitter.label("__rt_array_filter_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // load source length
    emitter.instruction("cmp x20, x9");                                         // compare i with length
    emitter.instruction("b.ge __rt_array_filter_done");                         // if i >= length, loop complete

    // -- load element from source array --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("add x1, x1, #24");                                     // skip header to data region
    emitter.instruction("ldr x0, [x1, x20, lsl #3]");                           // x0 = source[i]

    // -- save current element for potential copy --
    emitter.instruction("str x0, [sp, #32]");                                   // save element value to stack
    emitter.instruction("ldr x9, [sp, #0]");                                    // load optional callback environment pointer
    emitter.instruction("cbz x9, __rt_array_filter_call");                      // keep legacy one-argument callback ABI when no environment is present
    emitter.instruction("mov x1, x9");                                          // pass capture environment as the wrapper's second argument

    // -- call callback with element as argument --
    emitter.label("__rt_array_filter_call");
    emitter.instruction("blr x19");                                             // call callback(element) → result in x0

    // -- check if callback returned truthy --
    emitter.instruction("cbz x0, __rt_array_filter_skip");                      // if callback returned 0, skip element

    // -- callback returned truthy: copy element to new array --
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload saved element value
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload new array pointer
    emitter.instruction("add x2, x1, #24");                                     // skip header to data region
    emitter.instruction("str x9, [x2, x21, lsl #3]");                           // new_array[j] = element
    emitter.instruction("add x21, x21, #1");                                    // j += 1 (advance dest index)

    // -- advance source index --
    emitter.label("__rt_array_filter_skip");
    emitter.instruction("add x20, x20, #1");                                    // i += 1
    emitter.instruction("b __rt_array_filter_loop");                            // continue loop

    // -- set length on new array and return --
    emitter.label("__rt_array_filter_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = new array pointer
    emitter.instruction("str x21, [x0]");                                       // set new array length = number of kept elements

    // -- tear down stack frame and return --
    emitter.instruction("ldr x21, [sp, #40]");                                  // restore callee-saved x21
    emitter.instruction("ldp x19, x20, [sp, #48]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new filtered array
}

/// Emits the `__rt_array_filter` runtime helper for x86_64 Linux (SysV ABI).
/// Saves frame pointer and callee-saved registers (r12–r14), allocates a destination array sized
/// to the source length, then iterates source indices in r13 while accumulating kept elements in r14.
/// Calls the user callback through r12 with the current element in rdi; optionally passes the
/// capture environment from [rbp-64] as rsi when present. Copies kept elements to
/// `destination_array[kept_count * 8 + 24]`. Stores the final kept count as the destination array length.
/// Clobbers: rax, r10, r11 (caller-saved); preserves: r12, r13, r14, rbp across the loop.
fn emit_array_filter_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_filter ---");
    emitter.label_global("__rt_array_filter");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving callback-filter spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved source array, destination array, and kept element state
    emitter.instruction("push r12");                                            // preserve the callback address register because the filter loop calls through it repeatedly
    emitter.instruction("push r13");                                            // preserve the source-index register because the loop keeps it live across callback invocations
    emitter.instruction("push r14");                                            // preserve the destination-length register because kept-element count survives callback invocations
    emitter.instruction("sub rsp, 40");                                         // reserve local slots for filter bookkeeping and optional callback environment
    emitter.instruction("mov r12, rdi");                                        // keep the callback address in a callee-saved register across the filtering loop
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the source array pointer so the loop can reload it after callback calls
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save optional callback environment pointer for captured-closure wrappers
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // load the source array length from the first field of the array header
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the source array length across the destination-array allocation call
    emitter.instruction("mov rdi, r10");                                        // pass the source array length as the maximum destination capacity to __rt_array_new
    emitter.instruction("mov rsi, 8");                                          // request 8-byte element slots for the scalar filter runtime
    emitter.instruction("call __rt_array_new");                                 // allocate the destination array that will store the kept scalar elements
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the destination array pointer for the loop body and final return path
    emitter.instruction("xor r13d, r13d");                                      // start the source index at zero before scanning the source array
    emitter.instruction("xor r14d, r14d");                                      // start the destination kept-element count at zero before the first callback

    emitter.label("__rt_array_filter_loop");
    emitter.instruction("cmp r13, QWORD PTR [rbp - 40]");                       // stop once the source index reaches the saved source-array length
    emitter.instruction("jge __rt_array_filter_done");                          // finish filtering once every source element has been tested
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the source array pointer after the previous callback invocation
    emitter.instruction("mov rdi, QWORD PTR [r10 + r13 * 8 + 24]");             // load the current source element into the first SysV integer argument register
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // preserve the candidate source element across the callback so truthy matches can copy it
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // check whether this runtime call carries a callback capture environment
    emitter.instruction("je __rt_array_filter_call");                           // keep legacy one-argument callback ABI when no environment is present
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // pass capture environment as the wrapper's second argument
    emitter.label("__rt_array_filter_call");
    emitter.instruction("call r12");                                            // invoke the user callback with the current source element and read the truthy result from rax
    emitter.instruction("test rax, rax");                                       // check whether the callback reported a truthy keep/skip decision
    emitter.instruction("jz __rt_array_filter_skip");                           // skip copying the element when the callback returned zero / false
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the destination array pointer after the callback clobbered caller-saved registers
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the kept source element saved before the callback invocation
    emitter.instruction("mov QWORD PTR [r10 + r14 * 8 + 24], r11");             // copy the kept scalar element into the next destination-array slot
    emitter.instruction("add r14, 1");                                          // advance the destination kept-element count after storing a kept value

    emitter.label("__rt_array_filter_skip");
    emitter.instruction("add r13, 1");                                          // advance the source index after examining the current source element
    emitter.instruction("jmp __rt_array_filter_loop");                          // continue filtering until the whole source array has been examined

    emitter.label("__rt_array_filter_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the destination array pointer for final length publication and return
    emitter.instruction("mov QWORD PTR [rax], r14");                            // publish the number of kept elements as the destination array logical length
    emitter.instruction("add rsp, 40");                                         // release the filter local bookkeeping slots before restoring callee-saved registers
    emitter.instruction("pop r14");                                             // restore the caller destination-length callee-saved register
    emitter.instruction("pop r13");                                             // restore the caller source-index callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller callback callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the filtered array pointer
    emitter.instruction("ret");                                                 // return the filtered destination array pointer in rax
}
