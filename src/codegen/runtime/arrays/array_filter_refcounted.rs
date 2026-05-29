//! Purpose:
//! Emits the `__rt_array_filter_refcounted`, `__rt_array_new` runtime helper assembly for array filter refcounted.
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

/// Emits `__rt_array_filter_refcounted` for ARM64: filters a refcounted array using a callback, producing a new array.
/// ABI (ARM64): x0 = callback fn ptr, x1 = source array ptr, x2 = optional callback environment ptr, x3 = mode. Returns array ptr in x0.
/// Each source element is passed to the callback; truthy return keeps the element in the destination array.
/// String elements use a ptr+len (16-byte) ABI; refcounted scalars use a single pointer register.
/// The destination array grows automatically via `__rt_array_push_refcounted` or `__rt_array_push_str` as elements are kept.
/// Preserves all callee-saved registers (x19-x21, x29-x30).
/// Does not increment refcounts of source elements; destination array takes ownership of kept payloads.
pub fn emit_array_filter_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_filter_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_filter_refcounted ---");
    emitter.label_global("__rt_array_filter_refcounted");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #112");                                    // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #80]");                             // save callee-saved x19 and x20
    emitter.instruction("str x21, [sp, #72]");                                  // save callee-saved x21
    emitter.instruction("str x2, [sp, #0]");                                    // save optional callback environment pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer
    emitter.instruction("str x3, [sp, #56]");                                   // save array_filter callback mode
    emitter.instruction("mov x19, x0");                                         // keep callback address in callee-saved register

    // -- validate mode before allocating or invoking callback --
    emitter.instruction("ldr x9, [sp, #56]");                                   // load array_filter mode
    emitter.instruction("cmp x9, #0");                                          // is mode ARRAY_FILTER_USE_VALUE?
    emitter.instruction("b.eq __rt_array_filter_ref_mode_valid");               // accept value-only callback mode
    emitter.instruction("cmp x9, #1");                                          // is mode ARRAY_FILTER_USE_BOTH?
    emitter.instruction("b.eq __rt_array_filter_ref_mode_valid");               // accept value-and-key callback mode
    emitter.instruction("cmp x9, #2");                                          // is mode ARRAY_FILTER_USE_KEY?
    emitter.instruction("b.eq __rt_array_filter_ref_mode_valid");               // accept key-only callback mode
    emitter.instruction("b __rt_array_filter_ref_invalid_mode");                // reject any other mode with ValueError
    emitter.label("__rt_array_filter_ref_mode_valid");

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
    emitter.instruction("ldr x10, [sp, #56]");                                  // reload array_filter callback mode
    emitter.instruction("cmp x10, #2");                                         // does callback receive only the key?
    emitter.instruction("b.eq __rt_array_filter_ref_key_args");                 // prepare key-only callback arguments
    emitter.instruction("cmp x10, #1");                                         // does callback receive value and key?
    emitter.instruction("b.eq __rt_array_filter_ref_both_args");                // prepare value-and-key callback arguments
    emitter.instruction("ldr x9, [sp, #0]");                                    // load optional callback environment pointer
    emitter.instruction("cbz x9, __rt_array_filter_ref_call");                  // keep legacy one-argument callback ABI when no environment is present
    emitter.instruction("mov x1, x9");                                          // pass capture environment as the wrapper's second argument
    emitter.instruction("b __rt_array_filter_ref_call");                        // call through the shared callback branch
    emitter.label("__rt_array_filter_ref_both_args");
    emitter.instruction("mov x1, x20");                                         // pass source index as the callback key argument
    emitter.instruction("ldr x9, [sp, #0]");                                    // load optional callback environment pointer
    emitter.instruction("cbz x9, __rt_array_filter_ref_call");                  // call value-and-key callback without environment
    emitter.instruction("mov x2, x9");                                          // pass capture environment after value and key
    emitter.instruction("b __rt_array_filter_ref_call");                        // call through the shared callback branch
    emitter.label("__rt_array_filter_ref_key_args");
    emitter.instruction("mov x0, x20");                                         // pass source index as the only callback argument
    emitter.instruction("ldr x9, [sp, #0]");                                    // load optional callback environment pointer
    emitter.instruction("cbz x9, __rt_array_filter_ref_call");                  // call key-only callback without environment
    emitter.instruction("mov x1, x9");                                          // pass capture environment after key argument
    emitter.instruction("b __rt_array_filter_ref_call");                        // call through the shared callback branch
    emitter.label("__rt_array_filter_ref_load_str");
    emitter.instruction("lsl x11, x20, #4");                                    // compute source string byte offset from the logical index
    emitter.instruction("add x11, x1, x11");                                    // compute address of the current source string slot
    emitter.instruction("ldr x0, [x11]");                                       // load source string pointer for callback
    emitter.instruction("ldr x1, [x11, #8]");                                   // load source string length for callback
    emitter.instruction("str x0, [sp, #40]");                                   // preserve source string pointer across callback
    emitter.instruction("str x1, [sp, #48]");                                   // preserve source string length across callback
    emitter.instruction("ldr x10, [sp, #56]");                                  // reload array_filter callback mode
    emitter.instruction("cmp x10, #2");                                         // does callback receive only the key?
    emitter.instruction("b.eq __rt_array_filter_ref_str_key_args");             // prepare string-array key-only callback arguments
    emitter.instruction("cmp x10, #1");                                         // does callback receive value and key?
    emitter.instruction("b.eq __rt_array_filter_ref_str_both_args");            // prepare string-array value-and-key callback arguments
    emitter.instruction("ldr x9, [sp, #0]");                                    // load optional callback environment pointer
    emitter.instruction("cbz x9, __rt_array_filter_ref_call");                  // keep legacy string callback ABI when no environment is present
    emitter.instruction("mov x2, x9");                                          // pass capture environment after the string ptr/len pair
    emitter.instruction("b __rt_array_filter_ref_call");                        // call value-only string callback after argument setup
    emitter.label("__rt_array_filter_ref_str_both_args");
    emitter.instruction("mov x2, x20");                                         // pass source index after the string ptr/len pair
    emitter.instruction("ldr x9, [sp, #0]");                                    // load optional callback environment pointer
    emitter.instruction("cbz x9, __rt_array_filter_ref_call");                  // call string value-and-key callback without environment
    emitter.instruction("mov x3, x9");                                          // pass capture environment after string value and key
    emitter.instruction("b __rt_array_filter_ref_call");                        // call string value-and-key callback after argument setup
    emitter.label("__rt_array_filter_ref_str_key_args");
    emitter.instruction("mov x0, x20");                                         // pass source index as the only callback argument
    emitter.instruction("ldr x9, [sp, #0]");                                    // load optional callback environment pointer
    emitter.instruction("cbz x9, __rt_array_filter_ref_call");                  // call key-only callback without environment
    emitter.instruction("mov x1, x9");                                          // pass capture environment after key argument
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
    emitter.instruction("ldr x21, [sp, #72]");                                  // restore callee-saved x21
    emitter.instruction("ldp x19, x20, [sp, #80]");                             // restore callee-saved x19 and x20
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return filtered array

    emitter.label("__rt_array_filter_ref_invalid_mode");
    value_error::emit_throw_value_error_aarch64(
        emitter,
        "_array_filter_mode_msg",
        ARRAY_FILTER_MODE_MSG_LEN,
    );
}

/// Emits `__rt_array_filter_refcounted` for x86_64 Linux: filters a refcounted array using a callback, producing a new array.
/// ABI (x86_64 SysV): rdi = callback fn ptr, rsi = source array ptr, rdx = optional callback environment ptr, rcx = mode. Returns array ptr in rax.
/// Each source element is passed to the callback; truthy return keeps the element in the destination array.
/// String elements use a ptr+len (16-byte) ABI; refcounted scalars use a single pointer register.
/// The destination array grows automatically via `__rt_array_push_refcounted` or `__rt_array_push_str` as elements are kept.
/// Preserves all callee-saved registers (r12-r14, rbp).
/// Does not increment refcounts of source elements; destination array takes ownership of kept payloads.
fn emit_array_filter_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_filter_refcounted ---");
    emitter.label_global("__rt_array_filter_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted-filter spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved source array, destination array, and candidate payload
    emitter.instruction("push r12");                                            // preserve the callback address register because the filter loop calls through it repeatedly
    emitter.instruction("push r13");                                            // preserve the source-index register because the loop keeps it live across callback invocations
    emitter.instruction("push r14");                                            // preserve the destination-length register because kept-element count survives callback invocations
    emitter.instruction("sub rsp, 72");                                         // reserve local slots for refcounted-filter bookkeeping, mode, and optional callback environment
    emitter.instruction("mov r12, rdi");                                        // keep the callback address in a callee-saved register across the filtering loop
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the source array pointer so the loop can reload it after callback and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save optional callback environment pointer for captured-closure wrappers
    emitter.instruction("mov QWORD PTR [rbp - 88], rcx");                       // save array_filter callback mode for validation and loop calls
    emitter.instruction("cmp QWORD PTR [rbp - 88], 0");                         // is mode ARRAY_FILTER_USE_VALUE?
    emitter.instruction("je __rt_array_filter_ref_mode_valid_x86");             // accept value-only callback mode
    emitter.instruction("cmp QWORD PTR [rbp - 88], 1");                         // is mode ARRAY_FILTER_USE_BOTH?
    emitter.instruction("je __rt_array_filter_ref_mode_valid_x86");             // accept value-and-key callback mode
    emitter.instruction("cmp QWORD PTR [rbp - 88], 2");                         // is mode ARRAY_FILTER_USE_KEY?
    emitter.instruction("je __rt_array_filter_ref_mode_valid_x86");             // accept key-only callback mode
    emitter.instruction("jmp __rt_array_filter_ref_invalid_mode_x86");          // reject any other mode with ValueError
    emitter.label("__rt_array_filter_ref_mode_valid_x86");
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
    emitter.instruction("mov r11, QWORD PTR [rbp - 88]");                       // reload array_filter callback mode
    emitter.instruction("cmp r11, 2");                                          // does callback receive only the key?
    emitter.instruction("je __rt_array_filter_ref_key_args_x86");               // prepare key-only callback arguments
    emitter.instruction("cmp r11, 1");                                          // does callback receive value and key?
    emitter.instruction("je __rt_array_filter_ref_both_args_x86");              // prepare value-and-key callback arguments
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // check whether this runtime call carries a callback capture environment
    emitter.instruction("je __rt_array_filter_ref_call");                       // keep legacy one-argument callback ABI when no environment is present
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // pass capture environment as the wrapper's second argument
    emitter.instruction("jmp __rt_array_filter_ref_call");                      // call through the shared callback branch
    emitter.label("__rt_array_filter_ref_both_args_x86");
    emitter.instruction("mov rsi, r13");                                        // pass source index as the callback key argument
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // check whether a capture environment follows value and key
    emitter.instruction("je __rt_array_filter_ref_call");                       // call value-and-key callback without environment
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // pass capture environment after value and key
    emitter.instruction("jmp __rt_array_filter_ref_call");                      // call through the shared callback branch
    emitter.label("__rt_array_filter_ref_key_args_x86");
    emitter.instruction("mov rdi, r13");                                        // pass source index as the only callback argument
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // check whether a capture environment follows the key
    emitter.instruction("je __rt_array_filter_ref_call");                       // call key-only callback without environment
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // pass capture environment after key argument
    emitter.instruction("jmp __rt_array_filter_ref_call");                      // call through the shared callback branch
    emitter.label("__rt_array_filter_ref_load_str");
    emitter.instruction("mov rcx, r13");                                        // copy logical source index before scaling to a string slot offset
    emitter.instruction("shl rcx, 4");                                          // convert source index into a 16-byte string-slot offset
    emitter.instruction("lea rcx, [r10 + rcx + 24]");                           // compute address of the current source string slot
    emitter.instruction("mov rdi, QWORD PTR [rcx]");                            // load source string pointer for callback
    emitter.instruction("mov rsi, QWORD PTR [rcx + 8]");                        // load source string length for callback
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // preserve source string pointer across callback
    emitter.instruction("mov QWORD PTR [rbp - 80], rsi");                       // preserve source string length across callback
    emitter.instruction("mov r11, QWORD PTR [rbp - 88]");                       // reload array_filter callback mode
    emitter.instruction("cmp r11, 2");                                          // does callback receive only the key?
    emitter.instruction("je __rt_array_filter_ref_str_key_args_x86");           // prepare string-array key-only callback arguments
    emitter.instruction("cmp r11, 1");                                          // does callback receive value and key?
    emitter.instruction("je __rt_array_filter_ref_str_both_args_x86");          // prepare string-array value-and-key callback arguments
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // check whether this runtime call carries a callback capture environment
    emitter.instruction("je __rt_array_filter_ref_call");                       // keep legacy string callback ABI when no environment is present
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // pass capture environment after the string ptr/len pair
    emitter.instruction("jmp __rt_array_filter_ref_call");                      // call value-only string callback after argument setup
    emitter.label("__rt_array_filter_ref_str_both_args_x86");
    emitter.instruction("mov rdx, r13");                                        // pass source index after the string ptr/len pair
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // check whether a capture environment follows string value and key
    emitter.instruction("je __rt_array_filter_ref_call");                       // call string value-and-key callback without environment
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // pass capture environment after string value and key
    emitter.instruction("jmp __rt_array_filter_ref_call");                      // call string value-and-key callback after argument setup
    emitter.label("__rt_array_filter_ref_str_key_args_x86");
    emitter.instruction("mov rdi, r13");                                        // pass source index as the only callback argument
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // check whether a capture environment follows the key
    emitter.instruction("je __rt_array_filter_ref_call");                       // call key-only callback without environment
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // pass capture environment after key argument
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
    emitter.instruction("add rsp, 72");                                         // release the refcounted-filter local bookkeeping slots before restoring callee-saved registers
    emitter.instruction("pop r14");                                             // restore the caller destination-length callee-saved register
    emitter.instruction("pop r13");                                             // restore the caller source-index callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller callback callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the filtered array pointer
    emitter.instruction("ret");                                                 // return the filtered destination array pointer in rax

    emitter.label("__rt_array_filter_ref_invalid_mode_x86");
    value_error::emit_throw_value_error_x86_64(
        emitter,
        "_array_filter_mode_msg",
        ARRAY_FILTER_MODE_MSG_LEN,
    );
}
