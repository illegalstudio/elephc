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

use super::value_error;

const ARRAY_FILTER_MODE_MSG_LEN: usize = "array_filter(): Argument #3 ($mode) must be one of ARRAY_FILTER_USE_VALUE, ARRAY_FILTER_USE_KEY, or ARRAY_FILTER_USE_BOTH.".len();

/// Emits the `__rt_array_filter` runtime helper.
/// Dispatches to the x86_64 Linux SysV ABI variant; ARM64 uses the default implementation below.
/// Iterates over `source_array`, calling `callback(element)` for each. Elements where the callback
/// returns non-zero are copied into a newly allocated array. The new array length is set to the
/// count of kept elements. Caller-saved registers are preserved across the callback loop.
/// Input: x0 = callback address, x1 = source array ptr, x2 = optional callback environment, x3 = mode
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
    emitter.instruction("sub sp, sp, #96");                                     // allocate 96 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #64]");                             // save callee-saved x19, x20
    emitter.instruction("str x21, [sp, #56]");                                  // save callee-saved x21
    emitter.instruction("str x2, [sp, #0]");                                    // save optional callback environment pointer to stack
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer to stack
    emitter.instruction("str x3, [sp, #40]");                                   // save array_filter callback mode
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)

    // -- validate mode before allocating or invoking callback --
    emitter.instruction("ldr x9, [sp, #40]");                                   // load array_filter mode
    emitter.instruction("cmp x9, #0");                                          // is mode ARRAY_FILTER_USE_VALUE?
    emitter.instruction("b.eq __rt_array_filter_mode_valid");                   // accept value-only callback mode
    emitter.instruction("cmp x9, #1");                                          // is mode ARRAY_FILTER_USE_BOTH?
    emitter.instruction("b.eq __rt_array_filter_mode_valid");                   // accept value-and-key callback mode
    emitter.instruction("cmp x9, #2");                                          // is mode ARRAY_FILTER_USE_KEY?
    emitter.instruction("b.eq __rt_array_filter_mode_valid");                   // accept key-only callback mode
    emitter.instruction("b __rt_array_filter_invalid_mode");                    // reject any other mode with ValueError
    emitter.label("__rt_array_filter_mode_valid");

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
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload array_filter callback mode
    emitter.instruction("cmp x10, #2");                                         // does callback receive only the key?
    emitter.instruction("b.eq __rt_array_filter_key_args");                     // prepare key-only callback arguments
    emitter.instruction("cmp x10, #1");                                         // does callback receive value and key?
    emitter.instruction("b.eq __rt_array_filter_both_args");                    // prepare value-and-key callback arguments
    emitter.instruction("ldr x9, [sp, #0]");                                    // load optional callback environment pointer
    emitter.instruction("cbz x9, __rt_array_filter_call");                      // keep legacy one-argument callback ABI when no environment is present
    emitter.instruction("mov x1, x9");                                          // pass capture environment as the wrapper's second argument
    emitter.instruction("b __rt_array_filter_call");                            // call value-only callback after argument setup
    emitter.label("__rt_array_filter_both_args");
    emitter.instruction("mov x1, x20");                                         // pass source index as the callback key argument
    emitter.instruction("ldr x9, [sp, #0]");                                    // load optional callback environment pointer
    emitter.instruction("cbz x9, __rt_array_filter_call");                      // call value-and-key callback without environment
    emitter.instruction("mov x2, x9");                                          // pass capture environment after value and key
    emitter.instruction("b __rt_array_filter_call");                            // call value-and-key callback after argument setup
    emitter.label("__rt_array_filter_key_args");
    emitter.instruction("mov x0, x20");                                         // pass source index as the only callback argument
    emitter.instruction("ldr x9, [sp, #0]");                                    // load optional callback environment pointer
    emitter.instruction("cbz x9, __rt_array_filter_call");                      // call key-only callback without environment
    emitter.instruction("mov x1, x9");                                          // pass capture environment after key argument

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
    emitter.instruction("ldr x21, [sp, #56]");                                  // restore callee-saved x21
    emitter.instruction("ldp x19, x20, [sp, #64]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new filtered array

    emitter.label("__rt_array_filter_invalid_mode");
    value_error::emit_throw_value_error_aarch64(
        emitter,
        "_array_filter_mode_msg",
        ARRAY_FILTER_MODE_MSG_LEN,
    );
}

/// Emits the `__rt_array_filter` runtime helper for x86_64 Linux (SysV ABI).
/// Saves frame pointer and callee-saved registers (r12–r14), allocates a destination array sized
/// to the source length, then iterates source indices in r13 while accumulating kept elements in r14.
/// Calls the user callback through r12 with value, key, or both based on rcx mode; optionally
/// passes the capture environment from [rbp-64] after the visible arguments. Copies kept elements to
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
    emitter.instruction("sub rsp, 56");                                         // reserve local slots for filter bookkeeping, mode, and optional callback environment
    emitter.instruction("mov r12, rdi");                                        // keep the callback address in a callee-saved register across the filtering loop
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the source array pointer so the loop can reload it after callback calls
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save optional callback environment pointer for captured-closure wrappers
    emitter.instruction("mov QWORD PTR [rbp - 72], rcx");                       // save array_filter callback mode for validation and loop calls
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                         // is mode ARRAY_FILTER_USE_VALUE?
    emitter.instruction("je __rt_array_filter_mode_valid_x86");                 // accept value-only callback mode
    emitter.instruction("cmp QWORD PTR [rbp - 72], 1");                         // is mode ARRAY_FILTER_USE_BOTH?
    emitter.instruction("je __rt_array_filter_mode_valid_x86");                 // accept value-and-key callback mode
    emitter.instruction("cmp QWORD PTR [rbp - 72], 2");                         // is mode ARRAY_FILTER_USE_KEY?
    emitter.instruction("je __rt_array_filter_mode_valid_x86");                 // accept key-only callback mode
    emitter.instruction("jmp __rt_array_filter_invalid_mode_x86");              // reject any other mode with ValueError
    emitter.label("__rt_array_filter_mode_valid_x86");
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
    emitter.instruction("mov r11, QWORD PTR [rbp - 72]");                       // reload array_filter callback mode
    emitter.instruction("cmp r11, 2");                                          // does callback receive only the key?
    emitter.instruction("je __rt_array_filter_key_args_x86");                   // prepare key-only callback arguments
    emitter.instruction("cmp r11, 1");                                          // does callback receive value and key?
    emitter.instruction("je __rt_array_filter_both_args_x86");                  // prepare value-and-key callback arguments
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // check whether this runtime call carries a callback capture environment
    emitter.instruction("je __rt_array_filter_call");                           // keep legacy one-argument callback ABI when no environment is present
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // pass capture environment as the wrapper's second argument
    emitter.instruction("jmp __rt_array_filter_call");                          // call value-only callback after argument setup
    emitter.label("__rt_array_filter_both_args_x86");
    emitter.instruction("mov rsi, r13");                                        // pass source index as the callback key argument
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // check whether a capture environment follows value and key
    emitter.instruction("je __rt_array_filter_call");                           // call value-and-key callback without environment
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // pass capture environment after value and key
    emitter.instruction("jmp __rt_array_filter_call");                          // call value-and-key callback after argument setup
    emitter.label("__rt_array_filter_key_args_x86");
    emitter.instruction("mov rdi, r13");                                        // pass source index as the only callback argument
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // check whether a capture environment follows the key
    emitter.instruction("je __rt_array_filter_call");                           // call key-only callback without environment
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // pass capture environment after key argument
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
    emitter.instruction("add rsp, 56");                                         // release the filter local bookkeeping slots before restoring callee-saved registers
    emitter.instruction("pop r14");                                             // restore the caller destination-length callee-saved register
    emitter.instruction("pop r13");                                             // restore the caller source-index callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller callback callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the filtered array pointer
    emitter.instruction("ret");                                                 // return the filtered destination array pointer in rax

    emitter.label("__rt_array_filter_invalid_mode_x86");
    value_error::emit_throw_value_error_x86_64(
        emitter,
        "_array_filter_mode_msg",
        ARRAY_FILTER_MODE_MSG_LEN,
    );
}
