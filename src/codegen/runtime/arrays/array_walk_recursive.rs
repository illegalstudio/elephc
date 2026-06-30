//! Purpose:
//! Emits the `__rt_array_walk_recursive` runtime helper for array_walk_recursive.
//! Recursively walks nested indexed/associative arrays, invoking the callback on each non-array leaf value.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Mirrors `__rt_array_walk` (scalar leaf passed in one word, env optional) but descends into array-valued elements.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_walk_recursive: invoke the callback on every non-array leaf of a nested array.
/// Input:  x0 = callback address, x1 = array pointer, x2 = optional callback environment
/// Output: none (void); the callback return value is discarded
///
/// Indexed arrays whose element type is itself an array recurse element-by-element; hashes
/// recurse into array-tagged values and call the callback on scalar leaves. The callback is
/// invoked as `(leaf, env)` when env is non-null, else `(leaf)`. x19 (callback) and x20 (env)
/// are callee-saved, so they survive both the callback calls and the recursive descent.
pub fn emit_array_walk_recursive(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_walk_recursive_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_walk_recursive ---");
    emitter.label_global("__rt_array_walk_recursive");
    emitter.instruction("sub sp, sp, #64");                                     // allocate the recursive-walk stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up the new frame pointer
    emitter.instruction("stp x19, x20, [sp, #32]");                             // save callee-saved callback address and environment
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved across recursion and calls)
    emitter.instruction("mov x20, x2");                                         // x20 = optional callback environment (callee-saved)
    emitter.instruction("str x1, [sp, #0]");                                    // save the current array pointer
    emitter.instruction("ldr x9, [x1, #-8]");                                   // load the uniform heap-kind header word
    emitter.instruction("and x10, x9, #0xff");                                  // isolate the low-byte heap kind
    emitter.instruction("cmp x10, #3");                                         // is the container an associative hash?
    emitter.instruction("b.eq __rt_array_walk_recursive_hash");                 // hashes iterate in insertion order
    emitter.instruction("lsr x11, x9, #8");                                     // shift the packed indexed-array value_type into the low bits
    emitter.instruction("and x11, x11, #0x7f");                                 // isolate the indexed-array value_type tag
    emitter.instruction("ldr x12, [x1]");                                       // load the indexed-array length
    emitter.instruction("str x12, [sp, #8]");                                   // save the length for the loop bound
    emitter.instruction("str xzr, [sp, #16]");                                  // index i = 0
    emitter.instruction("cmp x11, #4");                                         // are the elements indexed sub-arrays?
    emitter.instruction("b.eq __rt_array_walk_recursive_idx_rec");              // recurse into indexed sub-array elements
    emitter.instruction("cmp x11, #5");                                         // are the elements associative sub-arrays?
    emitter.instruction("b.eq __rt_array_walk_recursive_idx_rec");              // recurse into associative sub-array elements
    emitter.label("__rt_array_walk_recursive_idx_leaf");
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload the length
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the index
    emitter.instruction("cmp x13, x12");                                        // has the index reached the length?
    emitter.instruction("b.ge __rt_array_walk_recursive_done");                 // finish once every element is visited
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the array pointer
    emitter.instruction("add x1, x1, #24");                                     // skip the 24-byte indexed-array header
    emitter.instruction("ldr x0, [x1, x13, lsl #3]");                           // load the scalar leaf at element[i]
    emitter.instruction("cbz x20, __rt_array_walk_recursive_idx_call");         // no environment keeps the one-argument callback ABI
    emitter.instruction("mov x1, x20");                                         // pass the callback environment as the second argument
    emitter.label("__rt_array_walk_recursive_idx_call");
    emitter.instruction("blr x19");                                             // invoke callback(leaf [, env]); return value discarded
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the index after the callback call
    emitter.instruction("add x13, x13, #1");                                    // advance to the next element
    emitter.instruction("str x13, [sp, #16]");                                  // save the advanced index
    emitter.instruction("b __rt_array_walk_recursive_idx_leaf");                // continue the scalar-leaf loop
    emitter.label("__rt_array_walk_recursive_idx_rec");
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload the length
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the index
    emitter.instruction("cmp x13, x12");                                        // has the index reached the length?
    emitter.instruction("b.ge __rt_array_walk_recursive_done");                 // finish once every sub-array is visited
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the array pointer
    emitter.instruction("add x1, x1, #24");                                     // skip the 24-byte indexed-array header
    emitter.instruction("ldr x1, [x1, x13, lsl #3]");                           // load the sub-array pointer at element[i]
    emitter.instruction("mov x0, x19");                                         // pass the callback address to the recursive call
    emitter.instruction("mov x2, x20");                                         // pass the callback environment to the recursive call
    emitter.instruction("bl __rt_array_walk_recursive");                        // recurse into the sub-array
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the index after the recursive call
    emitter.instruction("add x13, x13, #1");                                    // advance to the next sub-array
    emitter.instruction("str x13, [sp, #16]");                                  // save the advanced index
    emitter.instruction("b __rt_array_walk_recursive_idx_rec");                 // continue the recursive descent loop
    emitter.label("__rt_array_walk_recursive_hash");
    emitter.instruction("str xzr, [sp, #16]");                                  // iterator cursor = 0
    emitter.label("__rt_array_walk_recursive_hash_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // next entry: x0=cursor,x1=kptr,x2=klen,x3=vlo,x4=vhi,x5=vtag
    emitter.instruction("cmn x0, #1");                                          // has iteration reached the end (cursor == -1)?
    emitter.instruction("b.eq __rt_array_walk_recursive_done");                 // finish once every entry is visited
    emitter.instruction("str x0, [sp, #16]");                                   // save the next iterator cursor
    emitter.instruction("cmp x5, #4");                                          // is the value an indexed sub-array?
    emitter.instruction("b.eq __rt_array_walk_recursive_hash_rec");             // recurse into indexed sub-array values
    emitter.instruction("cmp x5, #5");                                          // is the value an associative sub-array?
    emitter.instruction("b.eq __rt_array_walk_recursive_hash_rec");             // recurse into associative sub-array values
    emitter.instruction("mov x0, x3");                                          // scalar leaf value goes in the first callback argument
    emitter.instruction("cbz x20, __rt_array_walk_recursive_hash_call");        // no environment keeps the one-argument callback ABI
    emitter.instruction("mov x1, x20");                                         // pass the callback environment as the second argument
    emitter.label("__rt_array_walk_recursive_hash_call");
    emitter.instruction("blr x19");                                             // invoke callback(leaf [, env]); return value discarded
    emitter.instruction("b __rt_array_walk_recursive_hash_loop");               // continue iterating the hash entries
    emitter.label("__rt_array_walk_recursive_hash_rec");
    emitter.instruction("mov x0, x19");                                         // pass the callback address to the recursive call
    emitter.instruction("mov x1, x3");                                          // pass the sub-array value pointer to the recursive call
    emitter.instruction("mov x2, x20");                                         // pass the callback environment to the recursive call
    emitter.instruction("bl __rt_array_walk_recursive");                        // recurse into the sub-array value
    emitter.instruction("b __rt_array_walk_recursive_hash_loop");               // continue iterating the hash entries
    emitter.label("__rt_array_walk_recursive_done");
    emitter.instruction("ldp x19, x20, [sp, #32]");                             // restore callee-saved callback address and environment
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return (void)
}

/// x86_64 Linux implementation of `__rt_array_walk_recursive`.
/// Input:  rdi = callback address, rsi = array pointer, rdx = optional callback environment
/// Output: none (void). Callee-saved r12 (callback) and r14 (environment) survive recursion.
fn emit_array_walk_recursive_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_walk_recursive ---");
    emitter.label_global("__rt_array_walk_recursive");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("push r12");                                            // preserve the callback address across recursion and calls
    emitter.instruction("push r13");                                            // preserve a scratch callee-saved register
    emitter.instruction("push r14");                                            // preserve the optional callback environment across calls
    emitter.instruction("sub rsp, 24");                                         // reserve local slots for the array pointer, length, and index/cursor
    emitter.instruction("mov r12, rdi");                                        // r12 = callback address (callee-saved)
    emitter.instruction("mov r14, rdx");                                        // r14 = optional callback environment (callee-saved)
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the current array pointer
    emitter.instruction("mov r10, QWORD PTR [rsi - 8]");                        // load the uniform heap-kind header word
    emitter.instruction("mov r11, r10");                                        // copy the header word before masking the heap kind
    emitter.instruction("and r11, 255");                                        // isolate the low-byte heap kind
    emitter.instruction("cmp r11, 3");                                          // is the container an associative hash?
    emitter.instruction("je __rt_array_walk_recursive_hash");                   // hashes iterate in insertion order
    emitter.instruction("shr r10, 8");                                          // shift the packed indexed-array value_type into the low bits
    emitter.instruction("and r10, 127");                                        // isolate the indexed-array value_type tag
    emitter.instruction("mov rax, QWORD PTR [rsi]");                            // load the indexed-array length
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the length for the loop bound
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // index i = 0
    emitter.instruction("cmp r10, 4");                                          // are the elements indexed sub-arrays?
    emitter.instruction("je __rt_array_walk_recursive_idx_rec");                // recurse into indexed sub-array elements
    emitter.instruction("cmp r10, 5");                                          // are the elements associative sub-arrays?
    emitter.instruction("je __rt_array_walk_recursive_idx_rec");                // recurse into associative sub-array elements
    emitter.label("__rt_array_walk_recursive_idx_leaf");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the index
    emitter.instruction("cmp rax, QWORD PTR [rbp - 40]");                       // has the index reached the length?
    emitter.instruction("jge __rt_array_walk_recursive_done");                  // finish once every element is visited
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the array pointer
    emitter.instruction("mov rdi, QWORD PTR [r10 + rax * 8 + 24]");             // load the scalar leaf at element[i]
    emitter.instruction("test r14, r14");                                       // is a callback environment present?
    emitter.instruction("jz __rt_array_walk_recursive_idx_call");               // no environment keeps the one-argument callback ABI
    emitter.instruction("mov rsi, r14");                                        // pass the callback environment as the second argument
    emitter.label("__rt_array_walk_recursive_idx_call");
    emitter.instruction("call r12");                                            // invoke callback(leaf [, env]); return value discarded
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the index after the callback call
    emitter.instruction("add rax, 1");                                          // advance to the next element
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the advanced index
    emitter.instruction("jmp __rt_array_walk_recursive_idx_leaf");              // continue the scalar-leaf loop
    emitter.label("__rt_array_walk_recursive_idx_rec");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the index
    emitter.instruction("cmp rax, QWORD PTR [rbp - 40]");                       // has the index reached the length?
    emitter.instruction("jge __rt_array_walk_recursive_done");                  // finish once every sub-array is visited
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the array pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + rax * 8 + 24]");             // load the sub-array pointer at element[i]
    emitter.instruction("mov rdi, r12");                                        // pass the callback address to the recursive call
    emitter.instruction("mov rdx, r14");                                        // pass the callback environment to the recursive call
    emitter.instruction("call __rt_array_walk_recursive");                      // recurse into the sub-array
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the index after the recursive call
    emitter.instruction("add rax, 1");                                          // advance to the next sub-array
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the advanced index
    emitter.instruction("jmp __rt_array_walk_recursive_idx_rec");               // continue the recursive descent loop
    emitter.label("__rt_array_walk_recursive_hash");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // iterator cursor = 0
    emitter.label("__rt_array_walk_recursive_hash_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // reload the iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // next entry: rax=cursor,rdi=kptr,rdx=klen,rcx=vlo,r8=vhi,r9=vtag
    emitter.instruction("cmp rax, -1");                                         // has iteration reached the end?
    emitter.instruction("je __rt_array_walk_recursive_done");                   // finish once every entry is visited
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the next iterator cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // stash the value low word in the hash-path-unused length slot
    emitter.instruction("cmp r9, 4");                                           // is the value an indexed sub-array?
    emitter.instruction("je __rt_array_walk_recursive_hash_rec");               // recurse into indexed sub-array values
    emitter.instruction("cmp r9, 5");                                           // is the value an associative sub-array?
    emitter.instruction("je __rt_array_walk_recursive_hash_rec");               // recurse into associative sub-array values
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // scalar leaf value goes in the first callback argument
    emitter.instruction("test r14, r14");                                       // is a callback environment present?
    emitter.instruction("jz __rt_array_walk_recursive_hash_call");              // no environment keeps the one-argument callback ABI
    emitter.instruction("mov rsi, r14");                                        // pass the callback environment as the second argument
    emitter.label("__rt_array_walk_recursive_hash_call");
    emitter.instruction("call r12");                                            // invoke callback(leaf [, env]); return value discarded
    emitter.instruction("jmp __rt_array_walk_recursive_hash_loop");             // continue iterating the hash entries
    emitter.label("__rt_array_walk_recursive_hash_rec");
    emitter.instruction("mov rdi, r12");                                        // pass the callback address to the recursive call
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // pass the sub-array value pointer to the recursive call
    emitter.instruction("mov rdx, r14");                                        // pass the callback environment to the recursive call
    emitter.instruction("call __rt_array_walk_recursive");                      // recurse into the sub-array value
    emitter.instruction("jmp __rt_array_walk_recursive_hash_loop");             // continue iterating the hash entries
    emitter.label("__rt_array_walk_recursive_done");
    emitter.instruction("add rsp, 24");                                         // release the local bookkeeping slots
    emitter.instruction("pop r14");                                             // restore the caller environment register
    emitter.instruction("pop r13");                                             // restore the caller scratch register
    emitter.instruction("pop r12");                                             // restore the caller callback register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return (void)
}

