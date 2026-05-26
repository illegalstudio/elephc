//! Purpose:
//! Emits the `__rt_array_union`, `__rt_array_union_clone_left` runtime helper assembly for array union.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_array_union` runtime helper for PHP's `+` operator on dense indexed arrays.
/// Duplicates from the left operand are preserved; only the right-side suffix whose
/// numeric keys are absent from the left is appended. The result array owns its storage
/// (COW-cloned from the left before appending).
///
/// ABI: `x0` = left array pointer, `x1` = right array pointer; returns result in `x0`.
/// The result must be released by the caller.
pub fn emit_array_union(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_union_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_union ---");
    emitter.label_global("__rt_array_union");

    // -- set up stack frame and preserve both source arrays --
    emitter.instruction("sub sp, sp, #80");                                     // reserve spill slots for the union walk
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish a stable frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the left indexed-array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the right indexed-array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load the number of numeric keys already present on the left
    emitter.instruction("ldr x10, [x1]");                                       // load the number of numeric keys present on the right
    emitter.instruction("str x9, [sp, #24]");                                   // save the first right index that may be missing on the left
    emitter.instruction("str x10, [sp, #32]");                                  // save the right indexed-array length

    // -- empty left arrays are exactly the right operand copied --
    emitter.instruction("cbnz x9, __rt_array_union_clone_left");                // non-empty left arrays need a real suffix merge
    emitter.instruction("mov x0, x1");                                          // clone the right operand when the left has no keys
    emitter.instruction("bl __rt_array_clone_shallow");                         // create an owned result array from the right operand
    emitter.instruction("b __rt_array_union_return");                           // return the cloned right operand

    // -- clone the left operand before appending missing right-side keys --
    emitter.label("__rt_array_union_clone_left");
    emitter.instruction("bl __rt_array_clone_shallow");                         // clone the left operand so the union result owns its storage
    emitter.instruction("str x0, [sp, #16]");                                   // save the evolving result indexed-array pointer
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the right indexed-array pointer
    emitter.instruction("ldr x12, [x11, #-8]");                                 // load the packed right array kind word
    emitter.instruction("lsr x12, x12, #8");                                    // move the right value_type tag into the low bits
    emitter.instruction("and x12, x12, #0x7f");                                 // isolate the right value_type tag
    emitter.instruction("str x12, [sp, #40]");                                  // save the right value_type tag for append dispatch

    // -- append each right entry whose numeric key does not exist on the left --
    emitter.label("__rt_array_union_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the current right index
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the right indexed-array length
    emitter.instruction("cmp x9, x10");                                         // have all missing right keys been copied?
    emitter.instruction("b.ge __rt_array_union_done");                          // finish once the right suffix is exhausted
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the right value_type tag
    emitter.instruction("cmp x12, #1");                                         // is the right array storing string slots?
    emitter.instruction("b.eq __rt_array_union_push_string");                   // strings need pointer+length loads and persistence
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the right indexed-array pointer
    emitter.instruction("add x11, x11, #24");                                   // move to the right array payload base
    emitter.instruction("ldr x1, [x11, x9, lsl #3]");                           // load the right scalar or pointer payload for this numeric key
    emitter.instruction("cmp x12, #4");                                         // is the payload tag in the refcounted range?
    emitter.instruction("b.lo __rt_array_union_push_scalar");                   // scalar payloads can be byte-copied through the int append helper
    emitter.instruction("cmp x12, #7");                                         // is the payload tag still a supported refcounted value?
    emitter.instruction("b.hi __rt_array_union_push_scalar");                   // unknown high tags fall back to scalar copying
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the current result indexed-array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append and retain the borrowed right-side heap payload
    emitter.instruction("str x0, [sp, #16]");                                   // save the possibly grown result indexed-array pointer
    emitter.instruction("b __rt_array_union_next");                             // advance to the next right index

    emitter.label("__rt_array_union_push_scalar");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the current result indexed-array pointer
    emitter.instruction("bl __rt_array_push_int");                              // append the scalar payload bits to the result array
    emitter.instruction("str x0, [sp, #16]");                                   // save the possibly grown result indexed-array pointer
    emitter.instruction("b __rt_array_union_next");                             // advance to the next right index

    emitter.label("__rt_array_union_push_string");
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the right indexed-array pointer
    emitter.instruction("lsl x13, x9, #4");                                     // compute the byte offset for a 16-byte string slot
    emitter.instruction("add x11, x11, x13");                                   // advance to the right string slot
    emitter.instruction("add x11, x11, #24");                                   // skip the indexed-array header
    emitter.instruction("ldp x1, x2, [x11]");                                   // load the right string pointer and length
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the current result indexed-array pointer
    emitter.instruction("bl __rt_array_push_str");                              // persist and append the right string payload
    emitter.instruction("str x0, [sp, #16]");                                   // save the possibly grown result indexed-array pointer

    emitter.label("__rt_array_union_next");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the current right index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next right numeric key
    emitter.instruction("str x9, [sp, #24]");                                   // save the updated right index
    emitter.instruction("b __rt_array_union_loop");                             // continue copying missing right suffix entries

    emitter.label("__rt_array_union_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the completed result indexed-array pointer

    emitter.label("__rt_array_union_return");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the union spill slots
    emitter.instruction("ret");                                                 // return to generated code
}

/// x86_64 Linux implementation of `__rt_array_union`.
/// Identical in behavior to the ARM64 path but uses System V AMD64 ABI conventions:
/// `rdi` = left array pointer, `rsi` = right array pointer; result returned in `rax`.
fn emit_array_union_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_union ---");
    emitter.label_global("__rt_array_union");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving union spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for both source arrays
    emitter.instruction("sub rsp, 48");                                         // reserve local storage for the result pointer, cursor, length, and value tag
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left indexed-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the right indexed-array pointer
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the number of numeric keys already present on the left
    emitter.instruction("mov r11, QWORD PTR [rsi]");                            // load the number of numeric keys present on the right
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the first right index that may be missing on the left
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the right indexed-array length
    emitter.instruction("test r10, r10");                                       // is the left indexed array empty?
    emitter.instruction("jnz __rt_array_union_x86_clone_left");                 // non-empty left arrays need a suffix merge
    emitter.instruction("mov rdi, rsi");                                        // clone the right operand when the left has no keys
    emitter.instruction("call __rt_array_clone_shallow");                       // create an owned result array from the right operand
    emitter.instruction("jmp __rt_array_union_x86_return");                     // return the cloned right operand

    emitter.label("__rt_array_union_x86_clone_left");
    emitter.instruction("call __rt_array_clone_shallow");                       // clone the left operand so the union result owns its storage
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the evolving result indexed-array pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the right indexed-array pointer
    emitter.instruction("mov r11, QWORD PTR [r10 - 8]");                        // load the packed right array kind word
    emitter.instruction("shr r11, 8");                                          // move the right value_type tag into the low bits
    emitter.instruction("and r11, 0x7f");                                       // isolate the right value_type tag
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // save the right value_type tag for append dispatch

    emitter.label("__rt_array_union_x86_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current right index
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the right indexed-array length
    emitter.instruction("cmp r10, r11");                                        // have all missing right keys been copied?
    emitter.instruction("jae __rt_array_union_x86_done");                       // finish once the right suffix is exhausted
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the right value_type tag
    emitter.instruction("cmp r9, 1");                                           // is the right array storing string slots?
    emitter.instruction("je __rt_array_union_x86_push_string");                 // strings need pointer+length loads and persistence
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the right indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + 24 + r10 * 8]");             // load the right scalar or pointer payload for this numeric key
    emitter.instruction("cmp r9, 4");                                           // is the payload tag in the refcounted range?
    emitter.instruction("jb __rt_array_union_x86_push_scalar");                 // scalar payloads can be byte-copied through the int append helper
    emitter.instruction("cmp r9, 7");                                           // is the payload tag still a supported refcounted value?
    emitter.instruction("ja __rt_array_union_x86_push_scalar");                 // unknown high tags fall back to scalar copying
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the current result indexed-array pointer
    emitter.instruction("call __rt_array_push_refcounted");                     // append and retain the borrowed right-side heap payload
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the possibly grown result indexed-array pointer
    emitter.instruction("jmp __rt_array_union_x86_next");                       // advance to the next right index

    emitter.label("__rt_array_union_x86_push_scalar");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the current result indexed-array pointer
    emitter.instruction("call __rt_array_push_int");                            // append the scalar payload bits to the result array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the possibly grown result indexed-array pointer
    emitter.instruction("jmp __rt_array_union_x86_next");                       // advance to the next right index

    emitter.label("__rt_array_union_x86_push_string");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the right indexed-array pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current right index
    emitter.instruction("shl r10, 4");                                          // compute the byte offset for a 16-byte string slot
    emitter.instruction("lea r11, [r11 + r10 + 24]");                           // address the right string slot inside the indexed-array payload
    emitter.instruction("mov rsi, QWORD PTR [r11]");                            // load the right string pointer
    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                        // load the right string length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the current result indexed-array pointer
    emitter.instruction("call __rt_array_push_str");                            // persist and append the right string payload
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the possibly grown result indexed-array pointer

    emitter.label("__rt_array_union_x86_next");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current right index
    emitter.instruction("add r10, 1");                                          // advance to the next right numeric key
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the updated right index
    emitter.instruction("jmp __rt_array_union_x86_loop");                       // continue copying missing right suffix entries

    emitter.label("__rt_array_union_x86_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the completed result indexed-array pointer

    emitter.label("__rt_array_union_x86_return");
    emitter.instruction("add rsp, 48");                                         // release the union spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code
}
