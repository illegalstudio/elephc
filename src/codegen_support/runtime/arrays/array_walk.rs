//! Purpose:
//! Emits the `__rt_array_walk` and `__rt_array_walk_keyed` runtime helpers.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - `__rt_array_walk` passes `(element)` and an optional capture environment.
//! - `__rt_array_walk_keyed` passes `(element, key)` and that environment as the third argument.
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits both array-walk helpers for the current target.
///
/// Inputs for each helper: callback address, source array pointer, optional callback environment.
/// The callback return value is discarded. Callee-saved registers are preserved.
pub fn emit_array_walk(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_walk_linux_x86_64(emitter, "__rt_array_walk", false);
        emit_array_walk_linux_x86_64(emitter, "__rt_array_walk_keyed", true);
        return;
    }
    emit_array_walk_aarch64(emitter, "__rt_array_walk", false);
    emit_array_walk_aarch64(emitter, "__rt_array_walk_keyed", true);
}

/// Emits one AArch64 array-walk helper.
///
/// `keyed` selects `(element, key, env)` instead of `(element, env)`.
fn emit_array_walk_aarch64(emitter: &mut Emitter, symbol: &str, keyed: bool) {
    let loop_label = format!("{symbol}_loop");
    let call_label = format!("{symbol}_call");
    let done_label = format!("{symbol}_done");
    emitter.blank();
    emitter.comment(&format!("--- runtime: {symbol} ---"));
    emitter.label_global(symbol);

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #32]");                             // save callee-saved x19, x20
    emitter.instruction("str x2, [sp, #16]");                                   // save optional callback environment pointer to stack
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)
    emitter.instruction("str x1, [sp, #0]");                                    // save source array pointer to stack

    // -- read source array length --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save length to stack

    // -- set up loop counter --
    emitter.instruction("mov x20, #0");                                         // x20 = loop index i = 0

    // -- loop: call callback on each element --
    emitter.label(&loop_label);
    emitter.instruction("ldr x9, [sp, #8]");                                    // load source length
    emitter.instruction("cmp x20, x9");                                         // compare i with length
    emitter.instruction(&format!("b.ge {done_label}"));                         // if i >= length, loop complete

    // -- load element from source array --
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x1, x1, #24");                                     // skip header to data region
    emitter.instruction("ldr x0, [x1, x20, lsl #3]");                           // x0 = source[i]
    if keyed {
        emitter.instruction("mov x1, x20");                                     // x1 = integer key i
    }
    emitter.instruction("ldr x9, [sp, #16]");                                   // load optional callback environment pointer
    emitter.instruction(&format!("cbz x9, {call_label}"));                      // skip the environment when this callback captures nothing
    if keyed {
        emitter.instruction("mov x2, x9");                                      // pass capture environment after the value and key
    } else {
        emitter.instruction("mov x1, x9");                                      // pass capture environment as the wrapper's second argument
    }

    // -- call callback (ignore return value) --
    emitter.label(&call_label);
    if keyed {
        emitter.instruction("blr x19");                                         // call callback(element, key)
    } else {
        emitter.instruction("blr x19");                                         // call callback(element)
    }

    // -- advance loop --
    emitter.instruction("add x20, x20, #1");                                    // i += 1
    emitter.instruction(&format!("b {loop_label}"));                            // continue loop

    // -- done --
    emitter.label(&done_label);

    // -- tear down stack frame and return --
    emitter.instruction("ldp x19, x20, [sp, #32]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("mov x0, #1");                                          // PHP array_walk returns bool(true)
    emitter.instruction("ret");                                                 // return the boolean result
}

/// Emits one Linux x86_64 array-walk helper.
///
/// `keyed` selects `(element, key, env)` instead of `(element, env)`.
fn emit_array_walk_linux_x86_64(emitter: &mut Emitter, symbol: &str, keyed: bool) {
    let loop_label = format!("{symbol}_loop");
    let call_label = format!("{symbol}_call");
    let done_label = format!("{symbol}_done");
    emitter.blank();
    emitter.comment(&format!("--- runtime: {symbol} ---"));
    emitter.label_global(symbol);

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving walk spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved source array and source length
    emitter.instruction("push r12");                                            // preserve the callback address register because the walk loop calls through it repeatedly
    emitter.instruction("push r13");                                            // preserve the source-index register because the loop keeps it live across callback invocations
    emitter.instruction("push r14");                                            // preserve the optional callback environment pointer across callback invocations
    emitter.instruction("sub rsp, 24");                                         // reserve local slots for the source array pointer and source length
    emitter.instruction("mov r12, rdi");                                        // keep the callback address in a callee-saved register across the walk loop
    emitter.instruction("mov r14, rdx");                                        // keep the optional callback environment pointer across every callback invocation
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the source array pointer so the loop can reload it after callback calls
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // load the source array length from the first field of the array header
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the source array length for loop termination checks
    emitter.instruction("xor r13d, r13d");                                      // start the source index at zero before walking the source array

    emitter.label(&loop_label);
    emitter.instruction("cmp r13, QWORD PTR [rbp - 40]");                       // stop once the source index reaches the saved source-array length
    emitter.instruction(&format!("jge {done_label}"));                          // finish walking once every source element has been passed to the callback
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the source array pointer after the previous callback invocation
    emitter.instruction("mov rdi, QWORD PTR [r10 + r13 * 8 + 24]");             // load the current source element into the first SysV integer argument register
    if keyed {
        emitter.instruction("mov rsi, r13");                                    // pass the integer key in the second SysV integer argument register
    }
    emitter.instruction("test r14, r14");                                       // check whether this runtime call carries a callback capture environment
    emitter.instruction(&format!("jz {call_label}"));                           // skip the environment when this callback captures nothing
    if keyed {
        emitter.instruction("mov rdx, r14");                                    // pass capture environment after the value and key
    } else {
        emitter.instruction("mov rsi, r14");                                    // pass capture environment as the wrapper's second argument
    }
    emitter.label(&call_label);
    if keyed {
        emitter.instruction("call r12");                                        // invoke the user callback with the current element and key
    } else {
        emitter.instruction("call r12");                                        // invoke the user callback with the current source element and ignore the scalar return value
    }
    emitter.instruction("add r13, 1");                                          // advance the source index after visiting the current element
    emitter.instruction(&format!("jmp {loop_label}"));                          // continue walking until the whole source array has been visited

    emitter.label(&done_label);
    emitter.instruction("add rsp, 24");                                         // release the walk local bookkeeping slots before restoring callee-saved registers
    emitter.instruction("pop r14");                                             // restore the caller callback-environment callee-saved register
    emitter.instruction("pop r13");                                             // restore the caller source-index callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller callback callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("mov rax, 1");                                          // PHP array_walk returns bool(true)
    emitter.instruction("ret");                                                 // return the boolean result to the caller
}
