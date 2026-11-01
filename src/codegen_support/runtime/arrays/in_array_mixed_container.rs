//! Purpose:
//! Emits runtime-dispatched membership checks for indexed or associative arrays
//! stored inside a boxed gradual value.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_managed_runtime()`.
//!
//! Key details:
//! - Each source value is read as an owned boxed `Mixed` cell, compared with the
//!   canonical loose or strict helper, and released before the next iteration.
//! - A fifth mode argument selects a boolean membership result or the boxed matching key,
//!   allowing `in_array()` and `array_search()` to share one runtime scan.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the gradual-container `in_array()` helper for the selected target.
pub fn emit_in_array_mixed_container(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 gradual-container membership loop.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: in_array_mixed_container ---");
    emitter.label_global("__rt_in_array_mixed_container");
    // Frame slots: needle=0, source=8, kind=16, strict=24, cursor=32,
    // current value=40, comparison=48, key=56/64, return-key=72, saved fp/lr=80/88.
    emitter.instruction("sub sp, sp, #96");                                     // reserve membership state and an aligned nested-call frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the runtime helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the borrowed boxed needle
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the borrowed raw container pointer
    emitter.instruction("str x2, [sp, #16]");                                   // preserve runtime container kind 4 or 5
    emitter.instruction("str x3, [sp, #24]");                                   // preserve loose-zero or strict-one mode
    emitter.instruction("str x4, [sp, #72]");                                   // preserve boolean-zero or matching-key-one result mode
    emitter.instruction("cbz x1, __rt_in_array_mixed_container_false");         // a defensive null container has no values
    emitter.instruction("str xzr, [sp, #32]");                                  // initialize the indexed position or hash cursor
    emitter.instruction("cmp x2, #4");                                          // tag 4 selects indexed-array iteration
    emitter.instruction("b.eq __rt_in_array_mixed_container_indexed_loop");     // scan packed keys from zero to length minus one
    emitter.instruction("b __rt_in_array_mixed_container_hash_loop");           // tag 5 scans associative insertion order

    emitter.label("__rt_in_array_mixed_container_indexed_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the current indexed key
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the raw indexed-array pointer
    emitter.instruction("ldr x11, [x10]");                                      // load the current indexed-array length
    emitter.instruction("cmp x9, x11");                                         // test whether every indexed value was visited
    emitter.instruction("b.ge __rt_in_array_mixed_container_false");            // report no match after the indexed tail
    emitter.instruction("mov x0, x10");                                         // pass the raw indexed container to the generic reader
    emitter.instruction("mov x1, x9");                                          // pass the current integer key
    emitter.instruction("mov x2, #-1");                                         // mark the lookup key as an integer
    emitter.instruction("mov x3, #0");                                          // known-present indexed reads never warn
    emitter.instruction("bl __rt_array_get_mixed_key");                         // obtain an owned dereferenced boxed value
    emitter.instruction("str x0, [sp, #40]");                                   // preserve the owned value for comparison and release
    emitter.instruction("b __rt_in_array_mixed_container_compare");             // compare through the shared boxed-value path

    emitter.label("__rt_in_array_mixed_container_hash_loop");
    emitter.instruction("ldr x0, [sp, #8]");                                    // pass the raw associative container to the iterator
    emitter.instruction("ldr x1, [sp, #32]");                                   // pass the current insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next key and advance cursor
    emitter.instruction("cmn x0, #1");                                          // did iteration reach the end sentinel?
    emitter.instruction("b.eq __rt_in_array_mixed_container_false");            // report no match after the associative tail
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the next insertion-order cursor
    emitter.instruction("str x1, [sp, #56]");                                   // preserve current key low word
    emitter.instruction("str x2, [sp, #64]");                                   // preserve current key length or integer sentinel
    emitter.instruction("ldr x0, [sp, #8]");                                    // pass the raw associative container to the generic reader
    emitter.instruction("ldr x1, [sp, #56]");                                   // restore current key low word
    emitter.instruction("ldr x2, [sp, #64]");                                   // restore current key length or integer sentinel
    emitter.instruction("mov x3, #0");                                          // known-present associative reads never warn
    emitter.instruction("bl __rt_array_get_mixed_key");                         // obtain an owned dereferenced boxed value
    emitter.instruction("str x0, [sp, #40]");                                   // preserve the owned value for comparison and release

    emitter.label("__rt_in_array_mixed_container_compare");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the borrowed boxed needle as comparison argument one
    emitter.instruction("ldr x1, [sp, #40]");                                   // load the owned boxed source value as argument two
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload strict comparison mode
    emitter.instruction("cbnz x9, __rt_in_array_mixed_container_strict");       // strict mode uses tag-and-payload identity
    emitter.instruction("bl __rt_mixed_loose_eq");                              // apply ordinary PHP loose equality
    emitter.instruction("b __rt_in_array_mixed_container_compared");            // converge on balanced value release
    emitter.label("__rt_in_array_mixed_container_strict");
    emitter.instruction("bl __rt_mixed_strict_eq");                             // apply ordinary PHP strict equality
    emitter.label("__rt_in_array_mixed_container_compared");
    emitter.instruction("str x0, [sp, #48]");                                   // preserve the boolean comparison result
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the owned source value
    emitter.instruction("bl __rt_decref_mixed");                                // release the per-value boxed read
    emitter.instruction("ldr x9, [sp, #48]");                                   // restore the comparison result after release
    emitter.instruction("cbnz x9, __rt_in_array_mixed_container_true");         // stop at the first matching value
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload runtime container kind
    emitter.instruction("cmp x9, #4");                                          // indexed iteration advances its own integer key
    emitter.instruction("b.ne __rt_in_array_mixed_container_hash_loop");        // hash iteration already saved its next cursor
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the current indexed key
    emitter.instruction("add x9, x9, #1");                                      // advance to the next indexed key
    emitter.instruction("str x9, [sp, #32]");                                   // persist the next indexed position
    emitter.instruction("b __rt_in_array_mixed_container_indexed_loop");        // continue scanning indexed values

    emitter.label("__rt_in_array_mixed_container_true");
    emitter.instruction("ldr x9, [sp, #72]");                                   // select boolean membership or boxed matching-key output
    emitter.instruction("cbz x9, __rt_in_array_mixed_container_true_bool");     // ordinary in_array returns an unboxed boolean
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the runtime container kind for key boxing
    emitter.instruction("cmp x9, #4");                                          // indexed arrays always return the current integer position
    emitter.instruction("b.ne __rt_in_array_mixed_container_true_hash_key");    // associative arrays return their preserved integer or string key
    emitter.instruction("ldr x1, [sp, #32]");                                   // load the matching indexed position as the Mixed payload
    emitter.instruction("mov x2, xzr");                                         // integer Mixed payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer key
    emitter.instruction("bl __rt_mixed_from_value");                            // box the indexed matching key
    emitter.instruction("b __rt_in_array_mixed_container_return");              // return the boxed indexed key
    emitter.label("__rt_in_array_mixed_container_true_hash_key");
    emitter.instruction("ldr x1, [sp, #56]");                                   // load the associative matching key low word
    emitter.instruction("ldr x2, [sp, #64]");                                   // load string length or the integer-key sentinel
    emitter.instruction("cmn x2, #1");                                          // is this an integer key?
    emitter.instruction("b.ne __rt_in_array_mixed_container_true_string_key");  // non-sentinel high word denotes a string key
    emitter.instruction("mov x2, xzr");                                         // integer Mixed payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer key
    emitter.instruction("bl __rt_mixed_from_value");                            // box the associative integer key
    emitter.instruction("b __rt_in_array_mixed_container_return");              // return the boxed integer key
    emitter.label("__rt_in_array_mixed_container_true_string_key");
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string key
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the associative string key
    emitter.instruction("b __rt_in_array_mixed_container_return");              // return the boxed string key
    emitter.label("__rt_in_array_mixed_container_true_bool");
    emitter.instruction("mov x0, #1");                                          // return true after finding an equal value
    emitter.instruction("b __rt_in_array_mixed_container_return");              // skip the no-match result
    emitter.label("__rt_in_array_mixed_container_false");
    emitter.instruction("ldr x9, [sp, #72]");                                   // select raw false or boxed false for a miss
    emitter.instruction("cbz x9, __rt_in_array_mixed_container_false_bool");    // ordinary in_array returns an unboxed boolean
    emitter.instruction("mov x1, xzr");                                         // false Mixed payload is zero
    emitter.instruction("mov x2, xzr");                                         // boolean Mixed payloads do not use a high word
    emitter.instruction("mov x0, #3");                                          // runtime tag 3 = bool
    emitter.instruction("bl __rt_mixed_from_value");                            // box array_search's false miss
    emitter.instruction("b __rt_in_array_mixed_container_return");              // return the boxed miss
    emitter.label("__rt_in_array_mixed_container_false_bool");
    emitter.instruction("mov x0, #0");                                          // return false when no value matches
    emitter.label("__rt_in_array_mixed_container_return");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the membership helper frame
    emitter.instruction("ret");                                                 // return the boolean membership result
}

/// Emits the x86_64 gradual-container membership loop.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: in_array_mixed_container ---");
    emitter.label_global("__rt_in_array_mixed_container");
    // Frame slots: needle=-8, source=-16, kind=-24, strict=-32, cursor=-40,
    // current value=-48, comparison=-56, key=-64/-72, return-key=-80.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable runtime helper frame
    emitter.instruction("sub rsp, 80");                                         // reserve aligned membership state for nested calls
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the borrowed boxed needle
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the borrowed raw container pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve runtime container kind 4 or 5
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // preserve loose-zero or strict-one mode
    emitter.instruction("mov QWORD PTR [rbp - 80], r8");                        // preserve boolean-zero or matching-key-one result mode
    emitter.instruction("test rsi, rsi");                                       // a defensive null container has no values
    emitter.instruction("jz __rt_in_array_mixed_container_false_x86");          // return false for a null payload
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize indexed position or hash cursor
    emitter.instruction("cmp rdx, 4");                                          // tag 4 selects indexed-array iteration
    emitter.instruction("je __rt_in_array_mixed_container_indexed_loop_x86");   // scan packed keys from zero to length minus one
    emitter.instruction("jmp __rt_in_array_mixed_container_hash_loop_x86");     // tag 5 scans associative insertion order

    emitter.label("__rt_in_array_mixed_container_indexed_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the current indexed key
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the raw indexed-array pointer
    emitter.instruction("cmp r9, QWORD PTR [r10]");                             // test whether every indexed value was visited
    emitter.instruction("jge __rt_in_array_mixed_container_false_x86");         // report no match after the indexed tail
    emitter.instruction("mov rdi, r10");                                        // pass the raw indexed container to the generic reader
    emitter.instruction("mov rsi, r9");                                         // pass the current integer key
    emitter.instruction("mov rdx, -1");                                         // mark the lookup key as an integer
    emitter.instruction("xor ecx, ecx");                                        // known-present indexed reads never warn
    emitter.instruction("call __rt_array_get_mixed_key");                       // obtain an owned dereferenced boxed value
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the owned value for comparison and release
    emitter.instruction("jmp __rt_in_array_mixed_container_compare_x86");       // compare through the shared boxed-value path

    emitter.label("__rt_in_array_mixed_container_hash_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass the raw associative container to the iterator
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // pass the current insertion-order cursor
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next key and advance cursor
    emitter.instruction("cmp rax, -1");                                         // did iteration reach the end sentinel?
    emitter.instruction("je __rt_in_array_mixed_container_false_x86");          // report no match after the associative tail
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the next insertion-order cursor
    emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                       // preserve current key low word
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // preserve current key length or integer sentinel
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass the raw associative container to the generic reader
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // restore current key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // restore current key length or integer sentinel
    emitter.instruction("xor ecx, ecx");                                        // known-present associative reads never warn
    emitter.instruction("call __rt_array_get_mixed_key");                       // obtain an owned dereferenced boxed value
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // preserve the owned value for comparison and release

    emitter.label("__rt_in_array_mixed_container_compare_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the borrowed boxed needle as comparison argument one
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // load the owned boxed source value as argument two
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // choose loose or strict equality
    emitter.instruction("jne __rt_in_array_mixed_container_strict_x86");        // strict mode uses tag-and-payload identity
    emitter.instruction("call __rt_mixed_loose_eq");                            // apply ordinary PHP loose equality
    emitter.instruction("jmp __rt_in_array_mixed_container_compared_x86");      // converge on balanced value release
    emitter.label("__rt_in_array_mixed_container_strict_x86");
    emitter.instruction("call __rt_mixed_strict_eq");                           // apply ordinary PHP strict equality
    emitter.label("__rt_in_array_mixed_container_compared_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the boolean comparison result
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the owned source value
    emitter.instruction("call __rt_decref_mixed");                              // release the per-value boxed read
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // restore the comparison outcome after release
    emitter.instruction("jne __rt_in_array_mixed_container_true_x86");          // stop at the first matching value
    emitter.instruction("cmp QWORD PTR [rbp - 24], 4");                         // indexed iteration advances its own integer key
    emitter.instruction("jne __rt_in_array_mixed_container_hash_loop_x86");     // hash iteration already saved its next cursor
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // advance to the next indexed key
    emitter.instruction("jmp __rt_in_array_mixed_container_indexed_loop_x86");  // continue scanning indexed values

    emitter.label("__rt_in_array_mixed_container_true_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 80], 0");                         // select boolean membership or boxed matching-key output
    emitter.instruction("je __rt_in_array_mixed_container_true_bool_x86");      // ordinary in_array returns an unboxed boolean
    emitter.instruction("cmp QWORD PTR [rbp - 24], 4");                         // indexed arrays always return the current integer position
    emitter.instruction("jne __rt_in_array_mixed_container_true_hash_key_x86"); // associative arrays return their preserved integer or string key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // load the matching indexed position as the Mixed payload
    emitter.instruction("xor esi, esi");                                        // integer Mixed payloads do not use a high word
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = integer key
    emitter.instruction("call __rt_mixed_from_value");                          // box the indexed matching key
    emitter.instruction("jmp __rt_in_array_mixed_container_return_x86");        // return the boxed indexed key
    emitter.label("__rt_in_array_mixed_container_true_hash_key_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // load the associative matching key low word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // load string length or the integer-key sentinel
    emitter.instruction("cmp rsi, -1");                                         // is this an integer key?
    emitter.instruction("jne __rt_in_array_mixed_container_true_string_key_x86"); // non-sentinel high word denotes a string key
    emitter.instruction("xor esi, esi");                                        // integer Mixed payloads do not use a high word
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = integer key
    emitter.instruction("call __rt_mixed_from_value");                          // box the associative integer key
    emitter.instruction("jmp __rt_in_array_mixed_container_return_x86");        // return the boxed integer key
    emitter.label("__rt_in_array_mixed_container_true_string_key_x86");
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string key
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the associative string key
    emitter.instruction("jmp __rt_in_array_mixed_container_return_x86");        // return the boxed string key
    emitter.label("__rt_in_array_mixed_container_true_bool_x86");
    emitter.instruction("mov eax, 1");                                          // return true after finding an equal value
    emitter.instruction("jmp __rt_in_array_mixed_container_return_x86");        // skip the no-match result
    emitter.label("__rt_in_array_mixed_container_false_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 80], 0");                         // select raw false or boxed false for a miss
    emitter.instruction("je __rt_in_array_mixed_container_false_bool_x86");     // ordinary in_array returns an unboxed boolean
    emitter.instruction("xor edi, edi");                                        // false Mixed payload is zero
    emitter.instruction("xor esi, esi");                                        // boolean Mixed payloads do not use a high word
    emitter.instruction("mov eax, 3");                                          // runtime tag 3 = bool
    emitter.instruction("call __rt_mixed_from_value");                          // box array_search's false miss
    emitter.instruction("jmp __rt_in_array_mixed_container_return_x86");        // return the boxed miss
    emitter.label("__rt_in_array_mixed_container_false_bool_x86");
    emitter.instruction("xor eax, eax");                                        // return false when no value matches
    emitter.label("__rt_in_array_mixed_container_return_x86");
    emitter.instruction("add rsp, 80");                                         // release the membership helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boolean membership result
}
