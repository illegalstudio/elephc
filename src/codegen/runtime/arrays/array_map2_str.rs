//! Purpose:
//! Emits the `__rt_array_map2_str` runtime helper for the two-input-array form of PHP `array_map`
//! over string arrays. Zips two string arrays element-wise through a string-returning callback.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Source elements are 16-byte (pointer/length) string slots; each element is passed to the
//!   callback as a string argument (`x0`/`x1` and `x2`/`x3` on AArch64; `rdi`/`rsi` and `rdx`/`rcx`
//!   on x86_64). The capture environment is the callback's fifth integer argument (`x4`/`r8`).
//! - Result length is `max(len(a), len(b))`; the shorter array is padded with the empty string
//!   (pointer 0, length 0 — PHP passes null, which is "" in string context).
//! - Each callback result is persisted and appended through `__rt_array_push_str`, which owns the
//!   copy and grows the destination as needed.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_array_map2_str` runtime helper.
///
/// # Input registers
/// - ARM64: `x0` = callback entry, `x1` = first array, `x2` = second array, `x3` = env (0 = none)
/// - x86_64: `rdi` = callback entry, `rsi` = first array, `rdx` = second array, `rcx` = env (0 = none)
///
/// # Output registers
/// - ARM64: `x0` = new mapped string array pointer; x86_64: `rax` = new mapped string array pointer
pub fn emit_array_map2_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_map2_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_map2_str (two string input arrays) ---");
    emitter.label_global("__rt_array_map2_str");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #96");                                     // allocate 96 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish new frame pointer
    emitter.instruction("stp x19, x20, [sp, #64]");                             // save callee-saved x19, x20

    // -- save inputs --
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

    // -- allocate the destination array with 16-byte string slots (length 0, push appends) --
    emitter.instruction("mov x0, x11");                                         // x0 = capacity for the new array
    emitter.instruction("mov x1, #16");                                         // x1 = element size (16 bytes for a string slot)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array → x0 = new array pointer
    emitter.instruction("str x0, [sp, #24]");                                   // save the destination array pointer
    emitter.instruction("mov x20, #0");                                         // x20 = loop index i = 0

    // -- loop: zip arr0[i] and arr1[i] through the callback --
    emitter.label("__rt_array_map2_str_loop");
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the result length
    emitter.instruction("cmp x20, x11");                                        // compare i with the result length
    emitter.instruction("b.ge __rt_array_map2_str_done");                       // if i >= result length, loop complete

    // -- load arr0[i] into x0/x1 (ptr/len), or the empty string when past the first array's end --
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the first array length
    emitter.instruction("cmp x20, x9");                                         // is i within the first array's bounds?
    emitter.instruction("b.ge __rt_array_map2_str_elem0_empty");                // out of bounds → pad with the empty string
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the first array pointer
    emitter.instruction("add x10, x10, #24");                                   // skip the array header to the data region
    emitter.instruction("lsl x11, x20, #4");                                    // compute the 16-byte string-slot offset for index i
    emitter.instruction("add x10, x10, x11");                                   // compute the address of the current first-array string slot
    emitter.instruction("ldr x0, [x10]");                                       // x0 = arr0[i] string pointer
    emitter.instruction("ldr x1, [x10, #8]");                                   // x1 = arr0[i] string length
    emitter.instruction("b __rt_array_map2_str_elem0_ready");                   // first element is ready
    emitter.label("__rt_array_map2_str_elem0_empty");
    emitter.instruction("mov x0, #0");                                          // empty-string pointer for the missing first element
    emitter.instruction("mov x1, #0");                                          // empty-string length for the missing first element
    emitter.label("__rt_array_map2_str_elem0_ready");

    // -- load arr1[i] into x2/x3, or the empty string (must not touch x0/x1) --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the second array length
    emitter.instruction("cmp x20, x9");                                         // is i within the second array's bounds?
    emitter.instruction("b.ge __rt_array_map2_str_elem1_empty");                // out of bounds → pad with the empty string
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the second array pointer
    emitter.instruction("add x10, x10, #24");                                   // skip the array header to the data region
    emitter.instruction("lsl x11, x20, #4");                                    // compute the 16-byte string-slot offset for index i
    emitter.instruction("add x10, x10, x11");                                   // compute the address of the current second-array string slot
    emitter.instruction("ldr x2, [x10]");                                       // x2 = arr1[i] string pointer
    emitter.instruction("ldr x3, [x10, #8]");                                   // x3 = arr1[i] string length
    emitter.instruction("b __rt_array_map2_str_elem1_ready");                   // second element is ready
    emitter.label("__rt_array_map2_str_elem1_empty");
    emitter.instruction("mov x2, #0");                                          // empty-string pointer for the missing second element
    emitter.instruction("mov x3, #0");                                          // empty-string length for the missing second element
    emitter.label("__rt_array_map2_str_elem1_ready");

    // -- pass the capture environment and call the callback --
    emitter.instruction("ldr x4, [sp, #0]");                                    // x4 = capture environment (0 for non-capturing callbacks)
    emitter.instruction("blr x19");                                             // call cb(s0, s1, env) → string result in x1=ptr, x2=len

    // -- persist and append the result string into the destination array --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the destination array pointer for the append
    emitter.instruction("bl __rt_array_push_str");                              // persist + append the result string, returning the possibly-grown array in x0
    emitter.instruction("str x0, [sp, #24]");                                   // store the possibly-grown destination array pointer back
    emitter.instruction("add x20, x20, #1");                                    // i += 1
    emitter.instruction("b __rt_array_map2_str_loop");                          // continue the zip loop

    // -- return the destination array (its length was published by __rt_array_push_str) --
    emitter.label("__rt_array_map2_str_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = destination array pointer
    emitter.instruction("ldp x19, x20, [sp, #64]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new mapped string array
}

/// Emits the x86_64 (System V) implementation of `__rt_array_map2_str`.
///
/// Same behavior as the ARM64 variant. The two string elements are passed in `rdi`/`rsi` and
/// `rdx`/`rcx`, the capture environment in `r8`; the callback returns the result string in
/// `rax`/`rdx`, which is appended via `__rt_array_push_str`.
fn emit_array_map2_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_map2_str (two string input arrays) ---");
    emitter.label_global("__rt_array_map2_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving spill slots
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
    emitter.instruction("mov rsi, 16");                                         // request 16-byte string element slots
    emitter.instruction("call __rt_array_new");                                 // allocate the destination array (length 0; push appends) → rax = new array pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the destination array pointer
    emitter.instruction("xor r13d, r13d");                                      // start the zip loop at logical index zero

    emitter.label("__rt_array_map2_str_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // reload the result length
    emitter.instruction("cmp r13, rax");                                        // compare i with the result length
    emitter.instruction("jge __rt_array_map2_str_done");                        // exit once every result slot has been produced

    // -- load arr0[i] into rdi/rsi, or the empty string when past the first array's end --
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the first array length
    emitter.instruction("cmp r13, rax");                                        // is i within the first array's bounds?
    emitter.instruction("jge __rt_array_map2_str_elem0_empty");                 // out of bounds → pad with the empty string
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the first array pointer
    emitter.instruction("mov rax, r13");                                        // copy the logical index before scaling to a 16-byte string-slot offset
    emitter.instruction("shl rax, 4");                                          // rax = i * 16 (x86 address scales max out at 8, so scale manually)
    emitter.instruction("mov rdi, QWORD PTR [r10 + rax + 24]");                 // rdi = arr0[i] string pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + rax + 32]");                 // rsi = arr0[i] string length
    emitter.instruction("jmp __rt_array_map2_str_elem0_ready");                 // first element is ready
    emitter.label("__rt_array_map2_str_elem0_empty");
    emitter.instruction("xor edi, edi");                                        // empty-string pointer for the missing first element
    emitter.instruction("xor esi, esi");                                        // empty-string length for the missing first element
    emitter.label("__rt_array_map2_str_elem0_ready");

    // -- load arr1[i] into rdx/rcx, or the empty string (must not touch rdi/rsi) --
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the second array length
    emitter.instruction("cmp r13, rax");                                        // is i within the second array's bounds?
    emitter.instruction("jge __rt_array_map2_str_elem1_empty");                 // out of bounds → pad with the empty string
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the second array pointer
    emitter.instruction("mov rax, r13");                                        // copy the logical index before scaling to a 16-byte string-slot offset
    emitter.instruction("shl rax, 4");                                          // rax = i * 16 (x86 address scales max out at 8, so scale manually)
    emitter.instruction("mov rdx, QWORD PTR [r10 + rax + 24]");                 // rdx = arr1[i] string pointer
    emitter.instruction("mov rcx, QWORD PTR [r10 + rax + 32]");                 // rcx = arr1[i] string length
    emitter.instruction("jmp __rt_array_map2_str_elem1_ready");                 // second element is ready
    emitter.label("__rt_array_map2_str_elem1_empty");
    emitter.instruction("xor edx, edx");                                        // empty-string pointer for the missing second element
    emitter.instruction("xor ecx, ecx");                                        // empty-string length for the missing second element
    emitter.label("__rt_array_map2_str_elem1_ready");

    // -- pass the capture environment and call the callback --
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // r8 = capture environment (0 for non-capturing callbacks)
    emitter.instruction("call r12");                                            // call cb(s0, s1, env) → string result in rax=ptr, rdx=len

    // -- persist and append the result string into the destination array --
    emitter.instruction("mov rsi, rax");                                        // move the result string pointer into the array-push payload register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the destination array pointer into the array-push receiver register
    emitter.instruction("call __rt_array_push_str");                            // persist + append the result string, returning the possibly-grown array in rax
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store the possibly-grown destination array pointer back
    emitter.instruction("add r13, 1");                                          // i += 1
    emitter.instruction("jmp __rt_array_map2_str_loop");                        // continue the zip loop

    emitter.label("__rt_array_map2_str_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the destination array pointer for the return value
    emitter.instruction("add rsp, 80");                                         // release the local bookkeeping slots
    emitter.instruction("pop r13");                                             // restore the caller's loop-index callee-saved register
    emitter.instruction("pop r12");                                             // restore the caller's callback callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return rax = new mapped string array pointer
}
