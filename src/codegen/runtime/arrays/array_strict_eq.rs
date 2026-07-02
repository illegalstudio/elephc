//! Purpose:
//! Emits the `__rt_array_strict_eq` runtime helper for indexed-array strict
//! equality (`===`) comparisons. Operates on the packed indexed-array layout
//! `[length:8][capacity:8][elem_size:8][elements...]` produced by
//! `__rt_array_new` / `__rt_array_push_*` / `__rt_array_set_*`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Compares two indexed arrays for PHP strict equality: identical length,
//!   identical `elem_size`, and element-wise value equality. Element value
//!   comparison dispatches on `elem_size`: 8-byte slots compare the full word
//!   (int/bool/float bit pattern); 16-byte slots hold a string `(ptr, len)`
//!   pair and compare by value through `__rt_str_eq`.
//! - A pointer-identity short-circuit handles aliases and cycles (`left == right`).
//! - The 8-byte float bit comparison treats `NaN == NaN` as true, which differs
//!   from PHP's `NaN === NaN === false`. This is a documented first-cut limitation;
//!   the runtime does not currently distinguish float slots by a separate tag.
//!   Mixed-valued indexed arrays (boxed `Mixed` slots, `elem_size == 8`) compare
//!   the boxed cell pointers through `__rt_mixed_strict_eq` so heterogeneous
//!   element types compare by runtime tag and payload.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the indexed-array strict equality helper for the current target.
///
/// Dispatches to the x86_64 Linux variant when targeting that architecture;
/// otherwise emits the portable ARM64 implementation.
///
/// # Inputs (ARM64)
/// - `x0`: left indexed-array pointer
/// - `x1`: right indexed-array pointer
///
/// # Output
/// - `x0` (ARM64) / `rax` (x86_64): `1` when strictly equal, `0` otherwise.
pub fn emit_array_strict_eq(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_strict_eq_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_strict_eq ---");
    emitter.label_global("__rt_array_strict_eq");

    // -- pointer-identity short-circuit (aliases and cycles) --
    emitter.instruction("cmp x0, x1");                                          // compare the left and right array pointers for identity
    emitter.instruction("b.eq __rt_array_strict_eq_true");                      // identical pointers are strictly equal

    // -- set up stack frame and save inputs --
    // [sp,#0]  = left array pointer
    // [sp,#8]  = right array pointer
    // [sp,#16] = loop index
    // [sp,#24] = length (shared once verified)
    // [sp,#32] = elem_size (shared once verified)
    // [sp,#40] = saved x29
    // [sp,#48] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // reserve the helper frame for inputs, loop state, and saved registers
    emitter.instruction("stp x29, x30, [sp, #40]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #40");                                    // establish the helper frame pointer
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save the left and right array pointers

    // -- compare lengths --
    emitter.instruction("ldr x2, [x0]");                                        // load the left indexed-array length
    emitter.instruction("ldr x3, [x1]");                                        // load the right indexed-array length
    emitter.instruction("cmp x2, x3");                                          // compare left and right lengths
    emitter.instruction("b.ne __rt_array_strict_eq_false_restore");             // different lengths are never strictly equal

    // -- compare elem_sizes --
    emitter.instruction("ldr x4, [x0, #16]");                                   // load the left element size
    emitter.instruction("ldr x5, [x1, #16]");                                   // load the right element size
    emitter.instruction("cmp x4, x5");                                          // compare left and right element sizes
    emitter.instruction("b.ne __rt_array_strict_eq_false_restore");             // different element sizes are never strictly equal

    // -- length zero short-circuits to true --
    emitter.instruction("cbz x2, __rt_array_strict_eq_true_restore");           // empty arrays with matching element size are strictly equal

    // -- save loop state and dispatch on element size --
    emitter.instruction("str x2, [sp, #24]");                                   // save the shared length for the element loop
    emitter.instruction("str x4, [sp, #32]");                                   // save the shared element size for the element loop
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize the loop index to zero

    // -- dispatch on element size: 8-byte scalar/Mixed slots vs 16-byte string slots --
    emitter.instruction("cmp x4, #16");                                         // are the slots 16-byte string pairs?
    emitter.instruction("b.eq __rt_array_strict_eq_str_loop");                  // route 16-byte string slots through the string-equality loop

    // -- 8-byte element loop: int/bool/float bit pattern or boxed Mixed pointer --
    emitter.label("__rt_array_strict_eq_word_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left indexed-array pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the right indexed-array pointer
    emitter.instruction("ldr x6, [sp, #16]");                                   // reload the current loop index
    emitter.instruction("ldr x7, [sp, #32]");                                   // reload the element size (8 for word slots)

    // -- detect boxed Mixed slots (value_type tag 7) and delegate to __rt_mixed_strict_eq --
    emitter.instruction("ldr x8, [x0, #-8]");                                   // load the left packed indexed-array metadata
    emitter.instruction("lsr x8, x8, #8");                                      // shift the value_type byte into the low byte
    emitter.instruction("and x8, x8, #0xff");                                   // isolate the value_type tag
    emitter.instruction("cmp x8, #7");                                          // value_type 7 marks boxed Mixed slots
    emitter.instruction("b.ne __rt_array_strict_eq_word_cmp");                  // non-Mixed slots compare the raw word directly

    // -- boxed Mixed slot comparison via __rt_mixed_strict_eq --
    emitter.instruction("add x9, x0, #24");                                     // compute the left data region base
    emitter.instruction("ldr x0, [x9, x6, lsl #3]");                            // load the left boxed Mixed pointer
    emitter.instruction("add x9, x1, #24");                                     // compute the right data region base
    emitter.instruction("ldr x1, [x9, x6, lsl #3]");                            // load the right boxed Mixed pointer
    emitter.instruction("stp x29, x30, [sp, #40]");                             // re-save frame registers around the nested helper call
    emitter.instruction("bl __rt_mixed_strict_eq");                             // compare the boxed Mixed cells by tag and payload
    emitter.instruction("ldp x29, x30, [sp, #40]");                             // restore frame registers after the nested helper call
    emitter.instruction("cbz x0, __rt_array_strict_eq_false_restore");          // a mismatched Mixed cell makes the arrays unequal
    emitter.instruction("b __rt_array_strict_eq_word_next");                    // advance to the next word slot

    // -- raw word comparison for int/bool/float slots --
    emitter.label("__rt_array_strict_eq_word_cmp");
    emitter.instruction("add x9, x0, #24");                                     // compute the left data region base
    emitter.instruction("ldr x10, [x9, x6, lsl #3]");                           // load the left element word
    emitter.instruction("add x9, x1, #24");                                     // compute the right data region base
    emitter.instruction("ldr x11, [x9, x6, lsl #3]");                           // load the right element word
    emitter.instruction("cmp x10, x11");                                        // compare the two element words
    emitter.instruction("b.ne __rt_array_strict_eq_false_restore");             // a mismatched word makes the arrays unequal

    emitter.label("__rt_array_strict_eq_word_next");
    emitter.instruction("ldr x6, [sp, #16]");                                   // reload the loop index
    emitter.instruction("add x6, x6, #1");                                      // advance to the next element
    emitter.instruction("str x6, [sp, #16]");                                   // store the updated loop index
    emitter.instruction("ldr x7, [sp, #24]");                                   // reload the shared length
    emitter.instruction("cmp x6, x7");                                          // have all elements been compared?
    emitter.instruction("b.lo __rt_array_strict_eq_word_loop");                 // continue while the index remains below the length
    emitter.instruction("b __rt_array_strict_eq_true_restore");                 // all words matched

    // -- 16-byte string element loop: compare (ptr, len) pairs by value via __rt_str_eq --
    emitter.label("__rt_array_strict_eq_str_loop");
    emitter.instruction("ldr x6, [sp, #16]");                                   // reload the current loop index
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left indexed-array pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the right indexed-array pointer

    // -- compute left element address: base + 24 + index * 16 --
    emitter.instruction("add x9, x0, #24");                                     // compute the left data region base
    emitter.instruction("add x9, x9, x6, lsl #4");                              // offset to the left slot address
    emitter.instruction("ldr x1, [x9]");                                        // load the left string pointer
    emitter.instruction("ldr x2, [x9, #8]");                                    // load the left string length

    // -- compute right element address: base + 24 + index * 16 --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right indexed-array pointer into a temporary
    emitter.instruction("add x9, x0, #24");                                     // compute the right data region base
    emitter.instruction("add x9, x9, x6, lsl #4");                              // offset to the right slot address
    emitter.instruction("ldr x3, [x9]");                                        // load the right string pointer
    emitter.instruction("ldr x4, [x9, #8]");                                    // load the right string length

    // -- call __rt_str_eq(ptr_a, len_a, ptr_b, len_b) --
    emitter.instruction("stp x29, x30, [sp, #40]");                             // re-save frame registers around the nested helper call
    emitter.instruction("bl __rt_str_eq");                                      // compare the two string payloads byte-by-byte
    emitter.instruction("ldp x29, x30, [sp, #40]");                             // restore frame registers after the nested helper call
    emitter.instruction("cbz x0, __rt_array_strict_eq_false_restore");          // a mismatched string makes the arrays unequal

    // -- advance the string loop --
    emitter.instruction("ldr x6, [sp, #16]");                                   // reload the loop index
    emitter.instruction("add x6, x6, #1");                                      // advance to the next string slot
    emitter.instruction("str x6, [sp, #16]");                                   // store the updated loop index
    emitter.instruction("ldr x7, [sp, #24]");                                   // reload the shared length
    emitter.instruction("cmp x6, x7");                                          // have all string elements been compared?
    emitter.instruction("b.lo __rt_array_strict_eq_str_loop");                  // continue while the index remains below the length
    emitter.instruction("b __rt_array_strict_eq_true_restore");                 // all strings matched

    // -- result paths --
    emitter.label("__rt_array_strict_eq_true_restore");
    emitter.instruction("mov x0, #1");                                          // materialize the strict-equality true result
    emitter.instruction("b __rt_array_strict_eq_epilogue");                     // skip the false path

    emitter.label("__rt_array_strict_eq_false_restore");
    emitter.instruction("mov x0, #0");                                          // materialize the strict-equality false result

    emitter.label("__rt_array_strict_eq_epilogue");
    emitter.instruction("ldp x29, x30, [sp, #40]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the strict-equality boolean in x0

    // -- no-frame fast paths (identity and early length checks jump here directly) --
    emitter.label("__rt_array_strict_eq_true");
    emitter.instruction("mov x0, #1");                                          // materialize true for identical pointers
    emitter.instruction("ret");                                                 // return true without allocating a frame
}

/// Emits the x86_64 Linux variant of the indexed-array strict equality helper.
///
/// Mirrors the ARM64 algorithm using the System V AMD64 ABI: `rdi` for the left
/// array pointer and `rsi` for the right, with the boolean result in `rax`.
fn emit_array_strict_eq_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_strict_eq ---");
    emitter.label_global("__rt_array_strict_eq");

    // -- pointer-identity short-circuit (aliases and cycles) --
    emitter.instruction("cmp rdi, rsi");                                        // compare the left and right array pointers for identity
    emitter.instruction("je __rt_array_strict_eq_true");                        // identical pointers are strictly equal

    // -- set up stack frame and save inputs --
    // [rbp - 8]   = left array pointer
    // [rbp - 16]  = right array pointer
    // [rbp - 24]  = loop index
    // [rbp - 32]  = length (shared once verified)
    // [rbp - 40]  = elem_size (shared once verified)
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame base
    emitter.instruction("sub rsp, 48");                                         // reserve spill slots for inputs and loop state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left indexed-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the right indexed-array pointer

    // -- compare lengths --
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the left indexed-array length
    emitter.instruction("mov r11, QWORD PTR [rsi]");                            // load the right indexed-array length
    emitter.instruction("cmp r10, r11");                                        // compare left and right lengths
    emitter.instruction("jne __rt_array_strict_eq_false_restore");              // different lengths are never strictly equal

    // -- compare elem_sizes --
    emitter.instruction("mov r10, QWORD PTR [rdi + 16]");                       // load the left element size
    emitter.instruction("mov r11, QWORD PTR [rsi + 16]");                       // load the right element size
    emitter.instruction("cmp r10, r11");                                        // compare left and right element sizes
    emitter.instruction("jne __rt_array_strict_eq_false_restore");              // different element sizes are never strictly equal

    // -- length zero short-circuits to true --
    emitter.instruction("test r10, r10");                                       // is the shared length zero?
    emitter.instruction("jz __rt_array_strict_eq_true_restore");                // empty arrays with matching element size are strictly equal

    // -- save loop state and dispatch on element size --
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // reload the shared left indexed-array length
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the shared length for the element loop
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the shared element size for the element loop
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the loop index to zero

    // -- dispatch on element size: 8-byte scalar/Mixed slots vs 16-byte string slots --
    emitter.instruction("cmp r10, 16");                                         // are the slots 16-byte string pairs?
    emitter.instruction("je __rt_array_strict_eq_str_loop");                    // route 16-byte string slots through the string-equality loop

    // -- 8-byte element loop: int/bool/float bit pattern or boxed Mixed pointer --
    emitter.label("__rt_array_strict_eq_word_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the left indexed-array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the right indexed-array pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the current loop index
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the element size (8 for word slots)

    // -- detect boxed Mixed slots (value_type tag 7) and delegate to __rt_mixed_strict_eq --
    emitter.instruction("mov r8, QWORD PTR [r10 - 8]");                         // load the left packed indexed-array metadata
    emitter.instruction("shr r8, 8");                                           // shift the value_type byte into the low byte
    emitter.instruction("and r8, 0xff");                                        // isolate the value_type tag
    emitter.instruction("cmp r8, 7");                                           // value_type 7 marks boxed Mixed slots
    emitter.instruction("jne __rt_array_strict_eq_word_cmp");                   // non-Mixed slots compare the raw word directly

    // -- boxed Mixed slot comparison via __rt_mixed_strict_eq --
    emitter.instruction("mov rdi, QWORD PTR [r10 + 24 + rax * 8]");             // load the left boxed Mixed pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + 24 + rax * 8]");             // load the right boxed Mixed pointer
    emitter.instruction("call __rt_mixed_strict_eq");                           // compare the boxed Mixed cells by tag and payload
    emitter.instruction("test rax, rax");                                       // check the strict-equality helper result
    emitter.instruction("jz __rt_array_strict_eq_false_restore");               // a mismatched Mixed cell makes the arrays unequal
    emitter.instruction("jmp __rt_array_strict_eq_word_next");                  // advance to the next word slot

    // -- raw word comparison for int/bool/float slots --
    emitter.label("__rt_array_strict_eq_word_cmp");
    emitter.instruction("mov r8, QWORD PTR [r10 + 24 + rax * 8]");              // load the left element word
    emitter.instruction("mov r9, QWORD PTR [r11 + 24 + rax * 8]");              // load the right element word
    emitter.instruction("cmp r8, r9");                                          // compare the two element words
    emitter.instruction("jne __rt_array_strict_eq_false_restore");              // a mismatched word makes the arrays unequal

    emitter.label("__rt_array_strict_eq_word_next");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the loop index
    emitter.instruction("add rax, 1");                                          // advance to the next element
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // store the updated loop index
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the shared length
    emitter.instruction("cmp rax, rcx");                                        // have all elements been compared?
    emitter.instruction("jb __rt_array_strict_eq_word_loop");                   // continue while the index remains below the length
    emitter.instruction("jmp __rt_array_strict_eq_true_restore");               // all words matched

    // -- 16-byte string element loop: compare (ptr, len) pairs by value via __rt_str_eq --
    emitter.label("__rt_array_strict_eq_str_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the left indexed-array pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the right indexed-array pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the current loop index

    // -- load left string (ptr, len): address = base + 24 + index * 16 --
    emitter.instruction("lea r8, [r10 + rax * 2]");                             // scale the index by 16 bytes (rax * 2 * 8)
    emitter.instruction("lea r8, [r8 + 24]");                                   // offset past the fixed header to the left slot
    emitter.instruction("mov rdi, QWORD PTR [r8]");                             // load the left string pointer into the first str_eq argument
    emitter.instruction("mov rsi, QWORD PTR [r8 + 8]");                         // load the left string length into the second str_eq argument

    // -- load right string (ptr, len): address = base + 24 + index * 16 --
    emitter.instruction("lea r8, [r11 + rax * 2]");                             // scale the index by 16 bytes (rax * 2 * 8)
    emitter.instruction("lea r8, [r8 + 24]");                                   // offset past the fixed header to the right slot
    emitter.instruction("mov rdx, QWORD PTR [r8]");                             // load the right string pointer into the third str_eq argument
    emitter.instruction("mov rcx, QWORD PTR [r8 + 8]");                         // load the right string length into the fourth str_eq argument

    // -- call __rt_str_eq(ptr_a, len_a, ptr_b, len_b) --
    emitter.instruction("call __rt_str_eq");                                    // compare the two string payloads byte-by-byte
    emitter.instruction("test rax, rax");                                       // check the string-equality helper result
    emitter.instruction("jz __rt_array_strict_eq_false_restore");               // a mismatched string makes the arrays unequal

    // -- advance the string loop --
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the loop index
    emitter.instruction("add rax, 1");                                          // advance to the next string slot
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // store the updated loop index
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the shared length
    emitter.instruction("cmp rax, rcx");                                        // have all string elements been compared?
    emitter.instruction("jb __rt_array_strict_eq_str_loop");                    // continue while the index remains below the length
    emitter.instruction("jmp __rt_array_strict_eq_true_restore");               // all strings matched

    // -- result paths --
    emitter.label("__rt_array_strict_eq_true_restore");
    emitter.instruction("mov rax, 1");                                          // materialize the strict-equality true result
    emitter.instruction("jmp __rt_array_strict_eq_epilogue");                   // skip the false path

    emitter.label("__rt_array_strict_eq_false_restore");
    emitter.instruction("xor rax, rax");                                        // materialize the strict-equality false result

    emitter.label("__rt_array_strict_eq_epilogue");
    emitter.instruction("add rsp, 48");                                         // release the helper spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the strict-equality boolean in rax

    // -- no-frame fast path for identical pointers --
    emitter.label("__rt_array_strict_eq_true");
    emitter.instruction("mov rax, 1");                                          // materialize true for identical pointers
    emitter.instruction("ret");                                                 // return true without allocating a frame
}