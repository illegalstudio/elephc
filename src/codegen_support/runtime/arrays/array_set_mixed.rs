//! Purpose:
//! Emits the `__rt_array_set_mixed` runtime helper for indexed-array writes
//! whose slots contain boxed `Mixed` cells.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The helper consumes the incoming boxed `Mixed` value, preserves COW, grows
//!   indexed storage as needed, and releases any overwritten boxed cell.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the boxed-Mixed indexed-array set helper for the current target.
pub fn emit_array_set_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_set_mixed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_set_mixed ---");
    emitter.label_global("__rt_array_set_mixed");

    emitter.instruction("sub sp, sp, #80");                                     // reserve frame for array, index, value, growth state, and saved registers
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish a helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the incoming indexed-array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the target index
    emitter.instruction("str x2, [sp, #16]");                                   // save the consumed boxed Mixed value

    emitter.instruction("cmp x1, #0");                                          // reject negative indexes before mutating indexed-array storage
    emitter.instruction("b.lt __rt_array_set_mixed_drop");                      // release the incoming value and return the original array for ignored writes
    emitter.instruction("bl __rt_array_ensure_unique");                         // split shared indexed arrays before mutating boxed Mixed slots
    emitter.instruction("str x0, [sp, #24]");                                   // save the unique indexed-array pointer

    emitter.instruction("ldr x11, [x0]");                                       // load the original logical length for overwrite and extension checks
    emitter.instruction("str x11, [sp, #40]");                                  // preserve the original logical length across helper calls
    emitter.instruction("ldr x12, [x0, #-8]");                                  // load the packed indexed-array metadata
    emitter.instruction("mov x13, #0x80ff");                                    // preserve indexed-array kind and persistent COW bits
    emitter.instruction("and x12, x12, x13");                                   // clear stale value_type metadata before stamping Mixed slots
    emitter.instruction("mov x13, #7");                                         // runtime value_type 7 = boxed Mixed
    emitter.instruction("lsl x13, x13, #8");                                    // move the Mixed tag into the packed value_type byte
    emitter.instruction("orr x12, x12, x13");                                   // combine stable indexed-array metadata with the Mixed slot tag
    emitter.instruction("str x12, [x0, #-8]");                                  // persist boxed-Mixed indexed-array metadata
    emitter.instruction("mov x12, #8");                                         // boxed Mixed slots store one heap pointer
    emitter.instruction("str x12, [x0, #16]");                                  // persist the pointer-sized slot width
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the target index after metadata stamping
    emitter.instruction("str x9, [sp, #32]");                                   // preserve the target index across growth and release helpers

    emitter.label("__rt_array_set_mixed_grow_check");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current unique indexed-array pointer
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the target index
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the current indexed-array capacity
    emitter.instruction("cmp x9, x12");                                         // does the target index fit in the current allocation?
    emitter.instruction("b.lo __rt_array_set_mixed_grow_ready");                // skip growth once the destination slot is addressable
    emitter.instruction("mov x0, x10");                                         // pass the current indexed array to the growth helper
    emitter.instruction("bl __rt_array_grow");                                  // grow indexed-array storage until the target slot fits
    emitter.instruction("str x0, [sp, #24]");                                   // save the possibly reallocated indexed-array pointer
    emitter.instruction("b __rt_array_set_mixed_grow_check");                   // continue growing until the target slot fits

    emitter.label("__rt_array_set_mixed_grow_ready");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the final indexed-array pointer
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the target index
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload the original logical length
    emitter.instruction("cmp x9, x11");                                         // does this write overwrite an existing slot?
    emitter.instruction("b.hs __rt_array_set_mixed_skip_release");              // writes past the old end do not replace an existing Mixed cell
    emitter.instruction("add x12, x10, #24");                                   // compute the indexed-array data base
    emitter.instruction("ldr x0, [x12, x9, lsl #3]");                           // load the previous boxed Mixed pointer from the slot
    emitter.instruction("bl __rt_decref_mixed");                                // release the overwritten boxed Mixed cell
    emitter.label("__rt_array_set_mixed_skip_release");

    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the indexed-array pointer after old-slot release
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the target index after old-slot release
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the consumed boxed Mixed value
    emitter.instruction("add x12, x10, #24");                                   // compute the indexed-array data base for the store
    emitter.instruction("str x0, [x12, x9, lsl #3]");                           // store the boxed Mixed value into the target slot

    emitter.instruction("ldr x11, [sp, #40]");                                  // reload the original logical length for extension checks
    emitter.instruction("cmp x9, x11");                                         // did the write extend beyond the old logical length?
    emitter.instruction("b.lo __rt_array_set_mixed_done");                      // overwrites leave the logical length unchanged
    emitter.instruction("mov x12, x11");                                        // start zero-filling gaps at the old logical end
    emitter.label("__rt_array_set_mixed_extend_loop");
    emitter.instruction("cmp x12, x9");                                         // have all gap slots before the target been initialized?
    emitter.instruction("b.ge __rt_array_set_mixed_store_len");                 // stop before touching the written slot
    emitter.instruction("add x13, x10, #24");                                   // compute the indexed-array data base for this gap slot
    emitter.instruction("str xzr, [x13, x12, lsl #3]");                         // initialize the gap slot to null
    emitter.instruction("add x12, x12, #1");                                    // advance to the next gap slot
    emitter.instruction("b __rt_array_set_mixed_extend_loop");                  // continue zero-filling until the target slot is reached
    emitter.label("__rt_array_set_mixed_store_len");
    emitter.instruction("add x12, x9, #1");                                     // compute the new logical length
    emitter.instruction("str x12, [x10]");                                      // publish the extended indexed-array length
    emitter.instruction("b __rt_array_set_mixed_done");                         // finish after extending the array

    emitter.label("__rt_array_set_mixed_drop");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the unused boxed Mixed value
    emitter.instruction("bl __rt_decref_mixed");                                // release the value because the write is ignored
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the original indexed-array pointer as the return value
    emitter.instruction("b __rt_array_set_mixed_return");                       // skip the normal return-value reload
    emitter.label("__rt_array_set_mixed_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // return the final indexed-array pointer
    emitter.label("__rt_array_set_mixed_return");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to generated code
}

/// Emits the Linux x86_64 boxed-Mixed indexed-array set helper.
fn emit_array_set_mixed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_set_mixed ---");
    emitter.label_global("__rt_array_set_mixed");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 64");                                         // reserve slots for inputs, array state, indexes, and value pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the incoming indexed-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the target index
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the consumed boxed Mixed value

    emitter.instruction("cmp rsi, 0");                                          // reject negative indexes before mutating indexed-array storage
    emitter.instruction("jl __rt_array_set_mixed_drop");                        // release the incoming value and return the original array for ignored writes
    emitter.instruction("call __rt_array_ensure_unique");                       // split shared indexed arrays before mutating boxed Mixed slots
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the unique indexed-array pointer

    emitter.instruction("mov r11, QWORD PTR [rax]");                            // load the original logical length for overwrite and extension checks
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // preserve the original logical length across helper calls
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the packed indexed-array metadata
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // preserve heap marker, indexed-array kind, and persistent COW bits
    emitter.instruction("and r10, r11");                                        // clear stale value_type metadata before stamping Mixed slots
    emitter.instruction("or r10, 0x700");                                       // encode runtime value_type 7 = boxed Mixed
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // persist boxed-Mixed indexed-array metadata
    emitter.instruction("mov QWORD PTR [rax + 16], 8");                         // boxed Mixed slots store one heap pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the target index after metadata stamping
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // preserve the target index across growth and release helpers

    emitter.label("__rt_array_set_mixed_grow_check");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current unique indexed-array pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the target index
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load the current indexed-array capacity
    emitter.instruction("cmp r9, r11");                                         // does the target index fit in the current allocation?
    emitter.instruction("jb __rt_array_set_mixed_grow_ready");                  // skip growth once the destination slot is addressable
    emitter.instruction("mov rdi, r10");                                        // pass the current indexed array to the growth helper
    emitter.instruction("call __rt_array_grow");                                // grow indexed-array storage until the target slot fits
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the possibly reallocated indexed-array pointer
    emitter.instruction("jmp __rt_array_set_mixed_grow_check");                 // continue growing until the target slot fits

    emitter.label("__rt_array_set_mixed_grow_ready");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the final indexed-array pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the target index
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the original logical length
    emitter.instruction("cmp r9, r11");                                         // does this write overwrite an existing slot?
    emitter.instruction("jae __rt_array_set_mixed_skip_release");               // writes past the old end do not replace an existing Mixed cell
    emitter.instruction("mov rax, QWORD PTR [r10 + 24 + r9 * 8]");              // load the previous boxed Mixed pointer from the slot
    emitter.instruction("call __rt_decref_mixed");                              // release the overwritten boxed Mixed cell
    emitter.label("__rt_array_set_mixed_skip_release");

    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the indexed-array pointer after old-slot release
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the target index after old-slot release
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the consumed boxed Mixed value
    emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], rax");              // store the boxed Mixed value into the target slot

    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the original logical length for extension checks
    emitter.instruction("cmp r9, r11");                                         // did the write extend beyond the old logical length?
    emitter.instruction("jb __rt_array_set_mixed_done");                        // overwrites leave the logical length unchanged
    emitter.instruction("mov r8, r11");                                         // start zero-filling gaps at the old logical end
    emitter.label("__rt_array_set_mixed_extend_loop");
    emitter.instruction("cmp r8, r9");                                          // have all gap slots before the target been initialized?
    emitter.instruction("jae __rt_array_set_mixed_store_len");                  // stop before touching the written slot
    emitter.instruction("mov QWORD PTR [r10 + 24 + r8 * 8], 0");                // initialize the gap slot to null
    emitter.instruction("add r8, 1");                                           // advance to the next gap slot
    emitter.instruction("jmp __rt_array_set_mixed_extend_loop");                // continue zero-filling until the target slot is reached
    emitter.label("__rt_array_set_mixed_store_len");
    emitter.instruction("lea r8, [r9 + 1]");                                    // compute the new logical length
    emitter.instruction("mov QWORD PTR [r10], r8");                             // publish the extended indexed-array length
    emitter.instruction("jmp __rt_array_set_mixed_done");                       // finish after extending the array

    emitter.label("__rt_array_set_mixed_drop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the unused boxed Mixed value
    emitter.instruction("call __rt_decref_mixed");                              // release the value because the write is ignored
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the original indexed-array pointer as the return value
    emitter.instruction("jmp __rt_array_set_mixed_return");                     // skip the normal return-value reload
    emitter.label("__rt_array_set_mixed_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the final indexed-array pointer
    emitter.label("__rt_array_set_mixed_return");
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to generated code
}
