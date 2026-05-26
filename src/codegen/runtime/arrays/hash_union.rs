//! Purpose:
//! Emits the `__rt_hash_union`, `__rt_hash_clone_shallow` runtime helper assembly for hash union.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Hash helpers must normalize PHP keys and preserve bucket layout, ownership, and iteration conventions.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_hash_union` runtime helper for PHP associative-array union.
/// Clones the left operand, then walks the right operand in insertion order,
/// copying only keys that are absent from the result. Duplicate keys keep the
/// left value. Handles ownership transfer for strings and increments refcounts
/// for refcounted payloads before insertion.
///
/// Input:  x0=left_hash_ptr, x1=right_hash_ptr
/// Output: x0=result_hash_ptr
/// Calls: `__rt_hash_clone_shallow`, `__rt_hash_iter_next`, `__rt_hash_get`, `__rt_str_persist`, `__rt_incref`, `__rt_hash_set`
pub fn emit_hash_union(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_union_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_union ---");
    emitter.label_global("__rt_hash_union");

    // -- set up stack frame and clone the left operand --
    emitter.instruction("sub sp, sp, #96");                                     // reserve spill slots for the hash union walk
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish a stable frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right associative-array pointer
    emitter.instruction("bl __rt_hash_clone_shallow");                          // clone the left hash so the union result owns its entries
    emitter.instruction("str x0, [sp, #8]");                                    // save the evolving result associative-array pointer
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize the insertion-order iterator cursor

    // -- walk right entries and copy only missing keys --
    emitter.label("__rt_hash_union_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right associative-array pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the insertion-order iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next right entry in insertion order
    emitter.instruction("cmn x0, #1");                                          // did the iterator report the terminal sentinel?
    emitter.instruction("b.eq __rt_hash_union_done");                           // finish once every right entry has been considered
    emitter.instruction("str x0, [sp, #16]");                                   // save the next insertion-order iterator cursor
    emitter.instruction("str x1, [sp, #24]");                                   // save the borrowed right key pointer
    emitter.instruction("str x2, [sp, #32]");                                   // save the borrowed right key length
    emitter.instruction("str x3, [sp, #40]");                                   // save the borrowed right value low word
    emitter.instruction("str x4, [sp, #48]");                                   // save the borrowed right value high word
    emitter.instruction("str x5, [sp, #56]");                                   // save the right value runtime tag
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the current result associative-array pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the candidate key pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the candidate key length
    emitter.instruction("bl __rt_hash_get");                                    // check whether the left/result side already has this key
    emitter.instruction("cbnz x0, __rt_hash_union_loop");                       // duplicate keys keep the left value and skip insertion

    // -- make the right value owned by the result before insertion --
    emitter.instruction("ldr x5, [sp, #56]");                                   // reload the right value runtime tag
    emitter.instruction("cmp x5, #1");                                          // is the right value a string payload?
    emitter.instruction("b.eq __rt_hash_union_value_string");                   // strings must be persisted for the result hash owner
    emitter.instruction("cmp x5, #4");                                          // is the right value in the refcounted payload range?
    emitter.instruction("b.lo __rt_hash_union_value_scalar");                   // scalar payloads can be copied directly
    emitter.instruction("cmp x5, #7");                                          // is the right value still a supported refcounted payload?
    emitter.instruction("b.hi __rt_hash_union_value_scalar");                   // unknown high tags fall back to scalar copying
    emitter.instruction("ldr x0, [sp, #40]");                                   // load the borrowed refcounted right payload
    emitter.instruction("bl __rt_incref");                                      // retain the right payload for the result hash
    emitter.instruction("ldr x3, [sp, #40]");                                   // reload the retained right value low word
    emitter.instruction("ldr x4, [sp, #48]");                                   // reload the right value high word
    emitter.instruction("ldr x5, [sp, #56]");                                   // reload the right value runtime tag
    emitter.instruction("b __rt_hash_union_insert");                            // insert the retained payload

    emitter.label("__rt_hash_union_value_string");
    emitter.instruction("ldr x1, [sp, #40]");                                   // load the borrowed right string pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // load the borrowed right string length
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the string payload for the result hash
    emitter.instruction("mov x3, x1");                                          // move the owned string pointer into the hash-set value low word
    emitter.instruction("mov x4, x2");                                          // move the owned string length into the hash-set value high word
    emitter.instruction("ldr x5, [sp, #56]");                                   // reload the string runtime tag
    emitter.instruction("b __rt_hash_union_insert");                            // insert the owned string payload

    emitter.label("__rt_hash_union_value_scalar");
    emitter.instruction("ldr x3, [sp, #40]");                                   // reload the scalar right value low word
    emitter.instruction("ldr x4, [sp, #48]");                                   // reload the scalar right value high word
    emitter.instruction("ldr x5, [sp, #56]");                                   // reload the scalar runtime tag

    // -- insert a right entry that was absent from the left/result hash --
    emitter.label("__rt_hash_union_insert");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the current result associative-array pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the borrowed right key pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the borrowed right key length
    emitter.instruction("bl __rt_hash_set");                                    // append the missing key/value pair to the result hash
    emitter.instruction("str x0, [sp, #8]");                                    // save the possibly grown result associative-array pointer
    emitter.instruction("b __rt_hash_union_loop");                              // continue scanning right-side entries

    emitter.label("__rt_hash_union_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the completed result associative-array pointer
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the hash union spill slots
    emitter.instruction("ret");                                                 // return to generated code
}

/// x86_64 variant of `emit_hash_union`. Mirrors the ARM64 logic but uses
/// the System V AMD64 ABI: parameters arrive in rdi, rsi, return value in rax.
fn emit_hash_union_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_union ---");
    emitter.label_global("__rt_hash_union");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving hash-union spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the union walk
    emitter.instruction("sub rsp, 80");                                         // reserve local storage while keeping nested calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right associative-array pointer
    emitter.instruction("call __rt_hash_clone_shallow");                        // clone the left hash so the union result owns its entries
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the evolving result associative-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the insertion-order iterator cursor

    emitter.label("__rt_hash_union_x86_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the right associative-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the insertion-order iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next right entry in insertion order
    emitter.instruction("cmp rax, -1");                                         // did the iterator report the terminal sentinel?
    emitter.instruction("je __rt_hash_union_x86_done");                         // finish once every right entry has been considered
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the next insertion-order iterator cursor
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the borrowed right key pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the borrowed right key length
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save the borrowed right value low word
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // save the borrowed right value high word
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // save the right value runtime tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the current result associative-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the candidate key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the candidate key length
    emitter.instruction("call __rt_hash_get");                                  // check whether the left/result side already has this key
    emitter.instruction("test rax, rax");                                       // did the result hash already contain the right key?
    emitter.instruction("jnz __rt_hash_union_x86_loop");                        // duplicate keys keep the left value and skip insertion

    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // reload the right value runtime tag
    emitter.instruction("cmp r10, 1");                                          // is the right value a string payload?
    emitter.instruction("je __rt_hash_union_x86_value_string");                 // strings must be persisted for the result hash owner
    emitter.instruction("cmp r10, 4");                                          // is the right value in the refcounted payload range?
    emitter.instruction("jb __rt_hash_union_x86_value_scalar");                 // scalar payloads can be copied directly
    emitter.instruction("cmp r10, 7");                                          // is the right value still a supported refcounted payload?
    emitter.instruction("ja __rt_hash_union_x86_value_scalar");                 // unknown high tags fall back to scalar copying
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // load the borrowed refcounted right payload
    emitter.instruction("call __rt_incref");                                    // retain the right payload for the result hash
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the retained right value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // reload the right value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload the right value runtime tag
    emitter.instruction("jmp __rt_hash_union_x86_insert");                      // insert the retained payload

    emitter.label("__rt_hash_union_x86_value_string");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // load the borrowed right string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // load the borrowed right string length
    emitter.instruction("call __rt_str_persist");                               // duplicate the string payload for the result hash
    emitter.instruction("mov rcx, rax");                                        // move the owned string pointer into the hash-set value low word
    emitter.instruction("mov r8, rdx");                                         // move the owned string length into the hash-set value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload the string runtime tag
    emitter.instruction("jmp __rt_hash_union_x86_insert");                      // insert the owned string payload

    emitter.label("__rt_hash_union_x86_value_scalar");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the scalar right value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // reload the scalar right value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload the scalar runtime tag

    emitter.label("__rt_hash_union_x86_insert");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the current result associative-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the borrowed right key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the borrowed right key length
    emitter.instruction("call __rt_hash_set");                                  // append the missing key/value pair to the result hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the possibly grown result associative-array pointer
    emitter.instruction("jmp __rt_hash_union_x86_loop");                        // continue scanning right-side entries

    emitter.label("__rt_hash_union_x86_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the completed result associative-array pointer
    emitter.instruction("add rsp, 80");                                         // release the hash-union spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code
}
