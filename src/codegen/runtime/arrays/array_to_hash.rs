//! Purpose:
//! Emits the `__rt_array_to_hash` runtime helper that converts an indexed array to an owned hash.
//! Lets the hash-based array builtins accept indexed-array inputs (keys 0..n-1).
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - String values are persisted (independent copies) and heap values retained, so the result owns its payloads.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_to_hash: build an owned hash {0: e0, 1: e1, ...} from an indexed array.
/// Input:  x0 = indexed array pointer
/// Output: x0 = new owned hash table with integer keys 0..length-1
///
/// Reads the indexed value_type to extract each element: string elements (16-byte slots)
/// are persisted into independent heap copies; heap-backed elements are retained; scalar
/// elements are copied by value. Used to accept indexed inputs in the hash-based builtins.
pub fn emit_array_to_hash(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_to_hash_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_to_hash ---");
    emitter.label_global("__rt_array_to_hash");
    emitter.instruction("sub sp, sp, #80");                                     // allocate the conversion stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the indexed array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load the indexed array length
    emitter.instruction("str x9, [sp, #24]");                                   // save the length
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the uniform heap-kind header word
    emitter.instruction("lsr x10, x10, #8");                                    // shift the packed value_type into the low bits
    emitter.instruction("and x10, x10, #0x7f");                                 // isolate the indexed-array value_type (also the Mixed tag)
    emitter.instruction("str x10, [sp, #32]");                                  // save the value_type / runtime tag
    emitter.instruction("ldr x11, [x0, #16]");                                  // load the element size (stride) from the header
    emitter.instruction("str x11, [sp, #40]");                                  // save the element stride
    emitter.instruction("mov x1, x10");                                         // value_type for the new hash header
    emitter.instruction("cmp x9, #8");                                          // is the length below the minimum hash capacity?
    emitter.instruction("b.ge __rt_array_to_hash_cap_ok");                      // use the length as the capacity hint
    emitter.instruction("mov x9, #8");                                          // clamp the capacity hint to a small minimum
    emitter.label("__rt_array_to_hash_cap_ok");
    emitter.instruction("mov x0, x9");                                          // capacity hint for the new hash
    emitter.instruction("bl __rt_hash_new");                                    // allocate the result hash, x0 = result
    emitter.instruction("str x0, [sp, #8]");                                    // save the result hash pointer
    emitter.instruction("str xzr, [sp, #16]");                                  // index i = 0
    emitter.label("__rt_array_to_hash_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the length
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the index
    emitter.instruction("cmp x10, x9");                                         // has the index reached the length?
    emitter.instruction("b.ge __rt_array_to_hash_done");                        // all elements converted
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the indexed array pointer
    emitter.instruction("add x11, x11, #24");                                   // skip the 24-byte indexed-array header
    emitter.instruction("ldr x12, [sp, #40]");                                  // reload the element stride
    emitter.instruction("mul x13, x10, x12");                                   // byte offset of element[i]
    emitter.instruction("add x11, x11, x13");                                   // x11 = address of element[i]
    emitter.instruction("ldr x3, [x11]");                                       // load the element low word
    emitter.instruction("str x3, [sp, #48]");                                   // save the element low word
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the value_type
    emitter.instruction("cmp x9, #1");                                          // is the element a string?
    emitter.instruction("b.eq __rt_array_to_hash_string");                      // strings need persistence
    emitter.instruction("mov x9, #0");                                          // non-string elements have no high word
    emitter.instruction("str x9, [sp, #56]");                                   // save a zero high word
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the value_type
    emitter.instruction("cmp x9, #4");                                          // is the element below the heap-backed tag range?
    emitter.instruction("b.lt __rt_array_to_hash_set");                         // scalar elements need no retain
    emitter.instruction("cmp x9, #7");                                          // is the element above the heap-backed tag range?
    emitter.instruction("b.gt __rt_array_to_hash_set");                         // non-heap tags need no retain
    emitter.instruction("ldr x0, [sp, #48]");                                   // load the heap-backed element pointer
    emitter.instruction("bl __rt_incref");                                      // retain the heap-backed element for the result hash
    emitter.instruction("b __rt_array_to_hash_set");                            // continue to insertion
    emitter.label("__rt_array_to_hash_string");
    emitter.instruction("ldr x2, [x11, #8]");                                   // load the string length from the 16-byte slot
    emitter.instruction("ldr x1, [sp, #48]");                                   // load the string pointer
    emitter.instruction("bl __rt_str_persist");                                 // copy the string into an independent heap block, x1 = new pointer
    emitter.instruction("str x1, [sp, #48]");                                   // save the persisted string pointer
    emitter.instruction("str x2, [sp, #56]");                                   // save the string length
    emitter.label("__rt_array_to_hash_set");
    emitter.instruction("ldr x0, [sp, #8]");                                    // result hash pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // integer key = index i
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("ldr x3, [sp, #48]");                                   // value low word
    emitter.instruction("ldr x4, [sp, #56]");                                   // value high word
    emitter.instruction("ldr x5, [sp, #32]");                                   // value runtime tag (= value_type)
    emitter.instruction("bl __rt_hash_set");                                    // insert element[i] at integer key i
    emitter.instruction("str x0, [sp, #8]");                                    // update the result pointer after possible reallocation
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the index
    emitter.instruction("add x10, x10, #1");                                    // advance to the next element
    emitter.instruction("str x10, [sp, #16]");                                  // save the advanced index
    emitter.instruction("b __rt_array_to_hash_loop");                           // continue converting elements
    emitter.label("__rt_array_to_hash_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return the result hash in x0
}

/// x86_64 Linux implementation of `__rt_array_to_hash`.
/// Input:  rdi = indexed array pointer
/// Output: rax = new owned hash with integer keys 0..length-1
fn emit_array_to_hash_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_to_hash ---");
    emitter.label_global("__rt_array_to_hash");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                         // reserve local slots for the conversion loop state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the indexed array pointer
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the indexed array length
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the length
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the uniform heap-kind header word
    emitter.instruction("shr r10, 8");                                          // shift the packed value_type into the low bits
    emitter.instruction("and r10, 127");                                        // isolate the indexed-array value_type (also the Mixed tag)
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the value_type / runtime tag
    emitter.instruction("mov r11, QWORD PTR [rdi + 16]");                       // load the element size (stride) from the header
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the element stride
    emitter.instruction("mov rsi, r10");                                        // value_type for the new hash header
    emitter.instruction("mov rdi, rax");                                        // capacity hint = length
    emitter.instruction("cmp rdi, 8");                                          // is the length below the minimum hash capacity?
    emitter.instruction("jge __rt_array_to_hash_cap_ok");                       // use the length as the capacity hint
    emitter.instruction("mov rdi, 8");                                          // clamp the capacity hint to a small minimum
    emitter.label("__rt_array_to_hash_cap_ok");
    emitter.instruction("call __rt_hash_new");                                  // allocate the result hash, rax = result
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the result hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // index i = 0
    emitter.label("__rt_array_to_hash_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the index
    emitter.instruction("cmp rax, QWORD PTR [rbp - 16]");                       // has the index reached the length?
    emitter.instruction("jge __rt_array_to_hash_done");                         // all elements converted
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the indexed array pointer
    emitter.instruction("add r10, 24");                                         // skip the 24-byte indexed-array header
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the element stride
    emitter.instruction("imul r11, rax");                                       // byte offset of element[i]
    emitter.instruction("add r10, r11");                                        // r10 = address of element[i]
    emitter.instruction("mov rcx, QWORD PTR [r10]");                            // load the element low word
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save the element low word
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the value_type
    emitter.instruction("cmp r9, 1");                                           // is the element a string?
    emitter.instruction("je __rt_array_to_hash_string");                        // strings need persistence
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // non-string elements have no high word
    emitter.instruction("cmp r9, 4");                                           // is the element below the heap-backed tag range?
    emitter.instruction("jl __rt_array_to_hash_set");                           // scalar elements need no retain
    emitter.instruction("cmp r9, 7");                                           // is the element above the heap-backed tag range?
    emitter.instruction("jg __rt_array_to_hash_set");                           // non-heap tags need no retain
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // load the heap-backed element pointer
    emitter.instruction("call __rt_incref");                                    // retain the heap-backed element for the result hash
    emitter.instruction("jmp __rt_array_to_hash_set");                          // continue to insertion
    emitter.label("__rt_array_to_hash_string");
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the string length from the 16-byte slot
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // load the string pointer
    emitter.instruction("call __rt_str_persist");                               // copy the string into an independent heap block, rax = new pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the persisted string pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save the string length
    emitter.label("__rt_array_to_hash_set");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // result hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // integer key = index i
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // value high word
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // value runtime tag (= value_type)
    emitter.instruction("call __rt_hash_set");                                  // insert element[i] at integer key i
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // update the result pointer after possible reallocation
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the index
    emitter.instruction("add rax, 1");                                          // advance to the next element
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the advanced index
    emitter.instruction("jmp __rt_array_to_hash_loop");                         // continue converting elements
    emitter.label("__rt_array_to_hash_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // rax = result hash pointer
    emitter.instruction("add rsp, 64");                                         // release the local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the result hash in rax
}

