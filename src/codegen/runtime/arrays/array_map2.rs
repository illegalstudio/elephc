//! Purpose:
//! Emits the `__rt_array_map2` runtime helper for the two-input-array form of PHP `array_map`.
//! Zips two integer arrays element-wise through a callback into a new integer-keyed list.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Result length is `max(len(a), len(b))`; the shorter array is padded with 0 (PHP passes null,
//!   which coerces to 0 in integer context).
//! - The callback is always invoked as `cb(elem0, elem1, env)`; for a non-capturing callback the
//!   environment argument is 0 and ignored, matching the direct-call path emitted by codegen.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_array_map2` runtime helper.
///
/// Iterates `i` from 0 to `max(len0, len1)`, loading `arr0[i]` and `arr1[i]` (or 0 when the
/// index is past the end of that array), invoking the callback with both elements plus the
/// optional capture environment, and storing each result into a freshly allocated array.
///
/// # Input registers
/// - ARM64: `x0` = callback/wrapper entry, `x1` = first array, `x2` = second array, `x3` = env (0 = none)
/// - x86_64: `rdi` = callback/wrapper entry, `rsi` = first array, `rdx` = second array, `rcx` = env (0 = none)
///
/// # Output registers
/// - ARM64: `x0` = new mapped array pointer; x86_64: `rax` = new mapped array pointer
pub fn emit_array_map2(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_map2_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_map2 (two input arrays) ---");
    emitter.label_global("__rt_array_map2");

    // -- set up stack frame (frame layout documented inline) --
    emitter.instruction("sub sp, sp, #96");                                     // allocate 96 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish new frame pointer
    emitter.instruction("stp x19, x20, [sp, #64]");                             // save callee-saved x19, x20

    // -- save inputs: x19=callback entry, x20=loop index; stack holds env/arrays/lengths/dest --
    emitter.instruction("mov x19, x0");                                         // x19 = callback entry point (callee-saved across calls)
    emitter.instruction("str x3, [sp, #0]");                                    // save the optional capture environment pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the first source array pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the second source array pointer
    emitter.instruction("ldr x9, [x1]");                                        // x9 = length of the first array
    emitter.instruction("str x9, [sp, #32]");                                   // save the first array length
    emitter.instruction("ldr x10, [x2]");                                       // x10 = length of the second array
    emitter.instruction("str x10, [sp, #40]");                                  // save the second array length

    // -- compute result length = max(len0, len1) --
    emitter.instruction("cmp x9, x10");                                         // compare the two array lengths
    emitter.instruction("csel x11, x9, x10, ge");                               // x11 = max(len0, len1)
    emitter.instruction("str x11, [sp, #48]");                                  // save the result length

    // -- allocate the destination array with 8-byte integer slots --
    emitter.instruction("mov x0, x11");                                         // x0 = capacity for the new array
    emitter.instruction("mov x1, #8");                                          // x1 = element size (8 bytes for int)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array → x0 = new array pointer
    emitter.instruction("str x0, [sp, #24]");                                   // save the destination array pointer
    emitter.instruction("mov x20, #0");                                         // x20 = loop index i = 0

    // -- loop: zip arr0[i] and arr1[i] through the callback --
    emitter.label("__rt_array_map2_loop");
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the result length
    emitter.instruction("cmp x20, x11");                                        // compare i with the result length
    emitter.instruction("b.ge __rt_array_map2_done");                           // if i >= result length, loop complete

    // -- load arr0[i] into x0, or 0 when i is past the first array's end --
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the first array length
    emitter.instruction("cmp x20, x9");                                         // is i within the first array's bounds?
    emitter.instruction("b.ge __rt_array_map2_elem0_zero");                     // out of bounds → pad with 0
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the first array pointer
    emitter.instruction("add x1, x1, #24");                                     // skip the array header to the data region
    emitter.instruction("ldr x0, [x1, x20, lsl #3]");                           // x0 = arr0[i]
    emitter.instruction("b __rt_array_map2_elem0_ready");                       // first element is ready
    emitter.label("__rt_array_map2_elem0_zero");
    emitter.instruction("mov x0, #0");                                          // pad the first element with 0 (PHP null in int context)
    emitter.label("__rt_array_map2_elem0_ready");

    // -- load arr1[i] into x1, or 0 when i is past the second array's end (must not touch x0) --
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the second array length
    emitter.instruction("cmp x20, x10");                                        // is i within the second array's bounds?
    emitter.instruction("b.ge __rt_array_map2_elem1_zero");                     // out of bounds → pad with 0
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the second array pointer
    emitter.instruction("add x2, x2, #24");                                     // skip the array header to the data region
    emitter.instruction("ldr x1, [x2, x20, lsl #3]");                           // x1 = arr1[i]
    emitter.instruction("b __rt_array_map2_elem1_ready");                       // second element is ready
    emitter.label("__rt_array_map2_elem1_zero");
    emitter.instruction("mov x1, #0");                                          // pad the second element with 0 (PHP null in int context)
    emitter.label("__rt_array_map2_elem1_ready");

    // -- pass the capture environment and call the callback --
    emitter.instruction("ldr x2, [sp, #0]");                                    // x2 = capture environment (0 for non-capturing callbacks)
    emitter.instruction("blr x19");                                             // call cb(elem0, elem1, env) → result in x0

    // -- store the result into the destination array --
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the destination array pointer
    emitter.instruction("add x2, x1, #24");                                     // skip the array header to the data region
    emitter.instruction("str x0, [x2, x20, lsl #3]");                           // dest[i] = callback result
    emitter.instruction("add x20, x20, #1");                                    // i += 1
    emitter.instruction("b __rt_array_map2_loop");                              // continue the zip loop

    // -- publish the destination length and return --
    emitter.label("__rt_array_map2_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = destination array pointer
    emitter.instruction("ldr x9, [sp, #48]");                                   // x9 = result length
    emitter.instruction("str x9, [x0]");                                        // publish the destination array length
    emitter.instruction("ldp x19, x20, [sp, #64]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new mapped array
}

/// Emits the x86_64 (System V) implementation of `__rt_array_map2`.
///
/// Same behavior as the ARM64 variant. The callback receives the two elements in `rdi`/`rsi`
/// and the capture environment in `rdx`; the transformed result is read from `rax`.
fn emit_array_map2_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_map2 (two input arrays) ---");
    emitter.label_global("__rt_array_map2");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving array-map2 spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the callback, source arrays, and destination slots
    emitter.instruction("push r12");                                            // preserve the callback scratch register across every callback invocation
    emitter.instruction("push r13");                                            // preserve the loop-index scratch register across callback invocations
    emitter.instruction("sub rsp, 80");                                         // reserve local slots; 80 keeps rsp 16-byte aligned before the SysV calls below
    emitter.instruction("mov r12, rdi");                                        // keep the callback entry point in a callee-saved register across the zip loop
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the optional capture environment pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rsi");                       // save the first source array pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the second source array pointer
    emitter.instruction("mov rax, QWORD PTR [rsi]");                            // load the length of the first array
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the first array length
    emitter.instruction("mov r10, QWORD PTR [rdx]");                            // load the length of the second array
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // save the second array length
    emitter.instruction("cmp rax, r10");                                        // compare the two array lengths
    emitter.instruction("cmovl rax, r10");                                      // rax = max(len0, len1)
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the result length
    emitter.instruction("mov rdi, rax");                                        // pass the result length as the destination capacity
    emitter.instruction("mov rsi, 8");                                          // request 8-byte integer element slots
    emitter.instruction("call __rt_array_new");                                 // allocate the destination array → rax = new array pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the destination array pointer
    emitter.instruction("xor r13d, r13d");                                      // start the zip loop at logical index zero

    emitter.label("__rt_array_map2_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // reload the result length
    emitter.instruction("cmp r13, rax");                                        // compare i with the result length
    emitter.instruction("jge __rt_array_map2_done");                            // exit once every result slot has been produced

    // -- load arr0[i] into rdi, or 0 when i is past the first array's end --
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the first array length
    emitter.instruction("cmp r13, rax");                                        // is i within the first array's bounds?
    emitter.instruction("jge __rt_array_map2_elem0_zero");                      // out of bounds → pad with 0
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the first array pointer
    emitter.instruction("mov rdi, QWORD PTR [r10 + r13 * 8 + 24]");             // rdi = arr0[i]
    emitter.instruction("jmp __rt_array_map2_elem0_ready");                     // first element is ready
    emitter.label("__rt_array_map2_elem0_zero");
    emitter.instruction("xor edi, edi");                                        // pad the first element with 0 (PHP null in int context)
    emitter.label("__rt_array_map2_elem0_ready");

    // -- load arr1[i] into rsi, or 0 when i is past the second array's end (must not touch rdi) --
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the second array length
    emitter.instruction("cmp r13, rax");                                        // is i within the second array's bounds?
    emitter.instruction("jge __rt_array_map2_elem1_zero");                      // out of bounds → pad with 0
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the second array pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + r13 * 8 + 24]");             // rsi = arr1[i]
    emitter.instruction("jmp __rt_array_map2_elem1_ready");                     // second element is ready
    emitter.label("__rt_array_map2_elem1_zero");
    emitter.instruction("xor esi, esi");                                        // pad the second element with 0 (PHP null in int context)
    emitter.label("__rt_array_map2_elem1_ready");

    // -- pass the capture environment and call the callback --
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // rdx = capture environment (0 for non-capturing callbacks)
    emitter.instruction("call r12");                                            // call cb(elem0, elem1, env) → result in rax
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the destination array pointer after the callback
    emitter.instruction("mov QWORD PTR [r10 + r13 * 8 + 24], rax");             // dest[i] = callback result
    emitter.instruction("add r13, 1");                                          // i += 1
    emitter.instruction("jmp __rt_array_map2_loop");                            // continue the zip loop

    emitter.label("__rt_array_map2_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the destination array pointer for the return value
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the result length
    emitter.instruction("mov QWORD PTR [rax], r10");                            // publish the destination array length
    emitter.instruction("add rsp, 80");                                         // release the local bookkeeping slots
    emitter.instruction("pop r13");                                             // restore the caller's loop-index callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller's callback callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return rax = new mapped array pointer
}
