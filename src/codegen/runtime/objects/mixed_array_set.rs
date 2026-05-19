//! Purpose:
//! Emits the `__rt_mixed_array_set` runtime helper for writes into boxed Mixed arrays.
//! Mutates JSON-decoded indexed-array and hash payloads reached through Mixed cells.
//!
//! Called from:
//! - `crate::codegen::runtime::objects::emit_mixed_array_set()`.
//!
//! Key details:
//! - The key tuple matches `emit_normalized_hash_key`: int keys use `key_hi = -1`.
//! - The helper consumes the boxed Mixed value pointer when the write succeeds.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

pub fn emit_mixed_array_set(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_array_set_x86_64(emitter);
        return;
    }
    emit_mixed_array_set_aarch64(emitter);
}

fn emit_mixed_array_set_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_set ---");
    emitter.label_global("__rt_mixed_array_set");

    // Inputs: x0 = mixed_ptr, x1 = key_lo, x2 = key_hi, x3 = value_mixed_ptr.
    emitter.instruction("sub sp, sp, #80");                                     // reserve frame for inputs, array state, and saved frame registers
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the target Mixed cell
    emitter.instruction("str x1, [sp, #8]");                                    // save key_lo for indexed-array addressing
    emitter.instruction("str x2, [sp, #16]");                                   // save key_hi so integer keys can be distinguished
    emitter.instruction("str x3, [sp, #24]");                                   // save the boxed value consumed by the write

    emitter.instruction("cbz x0, __rt_mixed_array_set_drop");                   // non-existent Mixed targets cannot be mutated
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed payload tag
    emitter.instruction("cmp x9, #4");                                          // is the Mixed payload an indexed array?
    emitter.instruction("b.eq __rt_mixed_array_set_indexed");                   // route indexed arrays to slot-based mutation
    emitter.instruction("cmp x9, #5");                                          // is the Mixed payload an associative array?
    emitter.instruction("b.eq __rt_mixed_array_set_assoc");                     // route hash arrays to key-based mutation
    emitter.instruction("b __rt_mixed_array_set_drop");                         // non-array Mixed payloads cannot be mutated here
    emitter.label("__rt_mixed_array_set_indexed");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the indexed-array pointer from the Mixed payload
    emitter.instruction("cbz x10, __rt_mixed_array_set_drop");                  // null array payloads cannot be mutated
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload key_hi
    emitter.instruction("cmn x11, #1");                                         // does key_hi carry the integer-key sentinel?
    emitter.instruction("b.ne __rt_mixed_array_set_drop");                      // string keys are not valid for indexed-array writes
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the requested integer index
    emitter.instruction("cmp x9, #0");                                          // reject negative indexes before touching storage
    emitter.instruction("b.lt __rt_mixed_array_set_drop");                      // negative indexed writes are ignored by this helper
    emitter.instruction("ldr x11, [x10]");                                      // load the current logical length
    emitter.instruction("str x11, [sp, #48]");                                  // preserve the original length for overwrite and extension checks
    emitter.instruction("ldr x12, [x10, #16]");                                 // load the element size used by the array payload
    emitter.instruction("cmp x12, #8");                                         // Mixed arrays must use pointer-sized slots
    emitter.instruction("b.ne __rt_mixed_array_set_drop");                      // non-pointer layouts cannot safely receive Mixed cells
    emitter.instruction("ldr x12, [x10, #-8]");                                 // load the packed indexed-array metadata
    emitter.instruction("ubfx x13, x12, #8, #7");                               // extract the runtime value_type tag
    emitter.instruction("cmp x11, #0");                                         // is the array currently empty?
    emitter.instruction("b.eq __rt_mixed_array_set_type_ready");                // empty arrays can be stamped as Mixed-valued before the first write
    emitter.instruction("cmp x13, #7");                                         // do existing slots already hold boxed Mixed pointers?
    emitter.instruction("b.ne __rt_mixed_array_set_drop");                      // avoid corrupting typed arrays wrapped in a Mixed cell
    emitter.label("__rt_mixed_array_set_type_ready");

    emitter.instruction("mov x0, x10");                                         // pass the indexed array to the copy-on-write helper
    emitter.instruction("bl __rt_array_ensure_unique");                         // split shared arrays before mutating a boxed payload
    emitter.instruction("str x0, [sp, #32]");                                   // save the unique array pointer
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the target Mixed cell after the helper call
    emitter.instruction("str x0, [x10, #8]");                                   // publish the unique array pointer back into the Mixed cell
    emitter.instruction("ldr x12, [x0, #-8]");                                  // reload the unique array metadata
    emitter.instruction("mov x13, #0x80ff");                                    // preserve indexed-array kind and copy-on-write bits
    emitter.instruction("and x12, x12, x13");                                   // clear stale value_type bits
    emitter.instruction("mov x13, #7");                                         // runtime value_type 7 = boxed Mixed
    emitter.instruction("lsl x13, x13, #8");                                    // move the Mixed tag into the metadata byte lane
    emitter.instruction("orr x12, x12, x13");                                   // combine preserved container bits with the Mixed value type
    emitter.instruction("str x12, [x0, #-8]");                                  // stamp the indexed array as Mixed-valued
    emitter.instruction("mov x12, #8");                                         // boxed Mixed slots are pointer-sized
    emitter.instruction("str x12, [x0, #16]");                                  // persist the pointer-sized slot width
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the requested integer index
    emitter.instruction("str x9, [sp, #40]");                                   // preserve the target index across growth and release helpers

    emitter.label("__rt_mixed_array_set_grow_check");
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the current unique array pointer
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the target index
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the current capacity
    emitter.instruction("cmp x9, x12");                                         // does the target index fit in the current capacity?
    emitter.instruction("b.lo __rt_mixed_array_set_grow_ready");                // skip growth once the destination slot is addressable
    emitter.instruction("mov x0, x10");                                         // pass the current array pointer to the growth helper
    emitter.instruction("bl __rt_array_grow");                                  // grow the unique array until the target slot fits
    emitter.instruction("str x0, [sp, #32]");                                   // save the possibly reallocated array pointer
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the owning Mixed cell
    emitter.instruction("str x0, [x10, #8]");                                   // publish the grown array pointer back into the Mixed cell
    emitter.instruction("b __rt_mixed_array_set_grow_check");                   // continue growing until the target slot fits

    emitter.label("__rt_mixed_array_set_grow_ready");
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the final array pointer
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the target index
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the original logical length
    emitter.instruction("cmp x9, x11");                                         // does this write overwrite an existing slot?
    emitter.instruction("b.hs __rt_mixed_array_set_skip_release");              // writes past the old end do not replace an existing Mixed cell
    emitter.instruction("add x12, x10, #24");                                   // compute the indexed-array data base
    emitter.instruction("ldr x0, [x12, x9, lsl #3]");                           // load the previous boxed Mixed pointer from the slot
    emitter.instruction("bl __rt_decref_mixed");                                // release the overwritten Mixed cell
    emitter.label("__rt_mixed_array_set_skip_release");

    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the array pointer after the release helper
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the target index after the release helper
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the new boxed Mixed value
    emitter.instruction("add x12, x10, #24");                                   // compute the indexed-array data base for the store
    emitter.instruction("str x0, [x12, x9, lsl #3]");                           // store the new boxed Mixed pointer into the target slot

    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the original logical length for extension checks
    emitter.instruction("cmp x9, x11");                                         // did the write extend beyond the old logical length?
    emitter.instruction("b.lo __rt_mixed_array_set_done");                      // overwrites leave the logical length unchanged
    emitter.instruction("mov x12, x11");                                        // start zero-filling at the old logical end
    emitter.label("__rt_mixed_array_set_extend_loop");
    emitter.instruction("cmp x12, x9");                                         // have all gap slots before the target been initialized?
    emitter.instruction("b.ge __rt_mixed_array_set_store_len");                 // stop once the loop reaches the written slot
    emitter.instruction("add x13, x10, #24");                                   // compute the indexed-array data base for the gap slot
    emitter.instruction("str xzr, [x13, x12, lsl #3]");                         // initialize the gap slot to null
    emitter.instruction("add x12, x12, #1");                                    // advance to the next gap slot
    emitter.instruction("b __rt_mixed_array_set_extend_loop");                  // continue zero-filling until the target slot is reached
    emitter.label("__rt_mixed_array_set_store_len");
    emitter.instruction("add x12, x9, #1");                                     // compute the new logical length
    emitter.instruction("str x12, [x10]");                                      // store the extended logical length
    emitter.instruction("b __rt_mixed_array_set_done");                         // finish after extending the array

    emitter.label("__rt_mixed_array_set_assoc");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the associative-array hash pointer from the Mixed payload
    emitter.instruction("cbz x10, __rt_mixed_array_set_drop");                  // null hash payloads cannot be mutated
    emitter.instruction("mov x0, x10");                                         // pass the current hash table to the hash-set helper
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the normalized key low word
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the normalized key high word
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload the boxed Mixed value pointer
    emitter.instruction("mov x4, xzr");                                         // boxed Mixed hash values only use the low payload word
    emitter.instruction("mov x5, #7");                                          // runtime value tag 7 = boxed Mixed
    emitter.instruction("bl __rt_hash_set");                                    // insert or update the hash entry, preserving PHP key semantics
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the owning Mixed cell after hash mutation
    emitter.instruction("str x0, [x10, #8]");                                   // publish the possibly-reallocated hash table back to the Mixed cell
    emitter.instruction("b __rt_mixed_array_set_done");                         // finish after mutating the associative array

    emitter.label("__rt_mixed_array_set_drop");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the unused boxed value
    emitter.instruction("bl __rt_decref_mixed");                                // release the boxed value when the write cannot be applied
    emitter.label("__rt_mixed_array_set_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to generated code
}

fn emit_mixed_array_set_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_set ---");
    emitter.label_global("__rt_mixed_array_set");

    // Inputs (SysV): rdi = mixed_ptr, rsi = key_lo, rdx = key_hi, rcx = value_mixed_ptr.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 64");                                         // reserve slots for inputs, array state, and indexes
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the target Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save key_lo for indexed-array addressing
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save key_hi so integer keys can be distinguished
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the boxed value consumed by the write

    emitter.instruction("test rdi, rdi");                                       // non-existent Mixed targets cannot be mutated
    emitter.instruction("je __rt_mixed_array_set_drop");                        // drop the value when the target is null
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed payload tag
    emitter.instruction("cmp r10, 4");                                          // is the Mixed payload an indexed array?
    emitter.instruction("je __rt_mixed_array_set_indexed");                     // route indexed arrays to slot-based mutation
    emitter.instruction("cmp r10, 5");                                          // is the Mixed payload an associative array?
    emitter.instruction("je __rt_mixed_array_set_assoc");                       // route hash arrays to key-based mutation
    emitter.instruction("jmp __rt_mixed_array_set_drop");                       // non-array Mixed payloads cannot be mutated here
    emitter.label("__rt_mixed_array_set_indexed");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the indexed-array pointer from the Mixed payload
    emitter.instruction("test r10, r10");                                       // null array payloads cannot be mutated
    emitter.instruction("je __rt_mixed_array_set_drop");                        // drop the value when the array payload is absent
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload key_hi
    emitter.instruction("cmp r11, -1");                                         // does key_hi carry the integer-key sentinel?
    emitter.instruction("jne __rt_mixed_array_set_drop");                       // string keys are not valid for indexed-array writes
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the requested integer index
    emitter.instruction("cmp r9, 0");                                           // reject negative indexes before touching storage
    emitter.instruction("jl __rt_mixed_array_set_drop");                        // negative indexed writes are ignored by this helper
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current logical length
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // preserve the original length for overwrite and extension checks
    emitter.instruction("mov r8, QWORD PTR [r10 + 16]");                        // load the element size used by the array payload
    emitter.instruction("cmp r8, 8");                                           // Mixed arrays must use pointer-sized slots
    emitter.instruction("jne __rt_mixed_array_set_drop");                       // non-pointer layouts cannot safely receive Mixed cells
    emitter.instruction("mov r8, QWORD PTR [r10 - 8]");                         // load the packed indexed-array metadata
    emitter.instruction("shr r8, 8");                                           // move the value_type tag into the low byte
    emitter.instruction("and r8, 0x7f");                                        // isolate the runtime value_type tag
    emitter.instruction("cmp r11, 0");                                          // is the array currently empty?
    emitter.instruction("je __rt_mixed_array_set_type_ready");                  // empty arrays can be stamped as Mixed-valued before the first write
    emitter.instruction("cmp r8, 7");                                           // do existing slots already hold boxed Mixed pointers?
    emitter.instruction("jne __rt_mixed_array_set_drop");                       // avoid corrupting typed arrays wrapped in a Mixed cell
    emitter.label("__rt_mixed_array_set_type_ready");

    emitter.instruction("mov rdi, r10");                                        // pass the indexed array to the copy-on-write helper
    emitter.instruction("call __rt_array_ensure_unique");                       // split shared arrays before mutating a boxed payload
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the unique array pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the target Mixed cell after the helper call
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // publish the unique array pointer back into the Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // reload the unique array metadata
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // preserve x86 heap marker, indexed-array kind, and COW bits
    emitter.instruction("and r10, r11");                                        // clear stale value_type bits
    emitter.instruction("or r10, 0x700");                                       // encode runtime value_type 7 = boxed Mixed
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the indexed array as Mixed-valued
    emitter.instruction("mov QWORD PTR [rax + 16], 8");                         // persist the pointer-sized slot width
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the requested integer index
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // preserve the target index across growth and release helpers

    emitter.label("__rt_mixed_array_set_grow_check");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current unique array pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the target index
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load the current capacity
    emitter.instruction("cmp r9, r11");                                         // does the target index fit in the current capacity?
    emitter.instruction("jb __rt_mixed_array_set_grow_ready");                  // skip growth once the destination slot is addressable
    emitter.instruction("mov rdi, r10");                                        // pass the current array pointer to the growth helper
    emitter.instruction("call __rt_array_grow");                                // grow the unique array until the target slot fits
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the possibly reallocated array pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the owning Mixed cell
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // publish the grown array pointer back into the Mixed cell
    emitter.instruction("jmp __rt_mixed_array_set_grow_check");                 // continue growing until the target slot fits

    emitter.label("__rt_mixed_array_set_grow_ready");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the final array pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the target index
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the original logical length
    emitter.instruction("cmp r9, r11");                                         // does this write overwrite an existing slot?
    emitter.instruction("jae __rt_mixed_array_set_skip_release");               // writes past the old end do not replace an existing Mixed cell
    emitter.instruction("mov rax, QWORD PTR [r10 + 24 + r9 * 8]");              // load the previous boxed Mixed pointer from the slot
    emitter.instruction("call __rt_decref_mixed");                              // release the overwritten Mixed cell
    emitter.label("__rt_mixed_array_set_skip_release");

    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the array pointer after the release helper
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the target index after the release helper
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the new boxed Mixed value
    emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], rax");              // store the new boxed Mixed pointer into the target slot

    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the original logical length for extension checks
    emitter.instruction("cmp r9, r11");                                         // did the write extend beyond the old logical length?
    emitter.instruction("jb __rt_mixed_array_set_done");                        // overwrites leave the logical length unchanged
    emitter.instruction("mov r8, r11");                                         // start zero-filling at the old logical end
    emitter.label("__rt_mixed_array_set_extend_loop");
    emitter.instruction("cmp r8, r9");                                          // have all gap slots before the target been initialized?
    emitter.instruction("jae __rt_mixed_array_set_store_len");                  // stop once the loop reaches the written slot
    emitter.instruction("mov QWORD PTR [r10 + 24 + r8 * 8], 0");                // initialize the gap slot to null
    emitter.instruction("add r8, 1");                                           // advance to the next gap slot
    emitter.instruction("jmp __rt_mixed_array_set_extend_loop");                // continue zero-filling until the target slot is reached
    emitter.label("__rt_mixed_array_set_store_len");
    emitter.instruction("lea r8, [r9 + 1]");                                    // compute the new logical length
    emitter.instruction("mov QWORD PTR [r10], r8");                             // store the extended logical length
    emitter.instruction("jmp __rt_mixed_array_set_done");                       // finish after extending the array

    emitter.label("__rt_mixed_array_set_assoc");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the associative-array hash pointer from the Mixed payload
    emitter.instruction("test r10, r10");                                       // null hash payloads cannot be mutated
    emitter.instruction("je __rt_mixed_array_set_drop");                        // drop the value when the hash payload is absent
    emitter.instruction("mov rdi, r10");                                        // pass the current hash table to the hash-set helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the normalized key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the normalized key high word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the boxed Mixed value pointer
    emitter.instruction("xor r8, r8");                                          // boxed Mixed hash values only use the low payload word
    emitter.instruction("mov r9, 7");                                           // runtime value tag 7 = boxed Mixed
    emitter.instruction("call __rt_hash_set");                                  // insert or update the hash entry, preserving PHP key semantics
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the owning Mixed cell after hash mutation
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // publish the possibly-reallocated hash table back to the Mixed cell
    emitter.instruction("jmp __rt_mixed_array_set_done");                       // finish after mutating the associative array

    emitter.label("__rt_mixed_array_set_drop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the unused boxed value
    emitter.instruction("call __rt_decref_mixed");                              // release the boxed value when the write cannot be applied
    emitter.label("__rt_mixed_array_set_done");
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code
}
