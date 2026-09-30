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

    // -- a slot holding an invoker ref-cell marker ALIASES the caller's storage --
    //
    // The lowering carries this check inline for a direct `$out[$i] = …`, but a `foreach` key is
    // a boxed Mixed and lowers to `__rt_array_set_mixed_key`, whose integer-key path delegates
    // HERE. Without the check that path replaced the array element and the caller's variable
    // never moved, silently — the same defect the inline check fixes, reached by another road.
    //
    // It runs BEFORE `ensure_unique`: a marker write mutates the CALLER's cell, not this array,
    // so there is nothing to split and the array pointer must come back unchanged.
    emitter.instruction("ldr x9, [x0]");                                        // the array's logical length
    emitter.instruction("cmp x1, x9");                                          // only an existing slot can hold a marker
    emitter.instruction("b.hs __rt_array_set_mixed_no_marker");                 // appends and gap writes are ordinary
    emitter.instruction("add x10, x0, #24");                                    // the boxed-Mixed payload base
    emitter.instruction("ldr x11, [x10, x1, lsl #3]");                          // the existing slot
    emitter.instruction("cbz x11, __rt_array_set_mixed_no_marker");             // a null gap slot is an ordinary write
    emitter.instruction("ldr x12, [x11]");                                      // the slot's Mixed tag
    emitter.instruction(&format!("cmp x12, #{}", crate::codegen_support::callable_invoker_args::INVOKER_ARG_REF_CELL_TAG));
    emitter.instruction("b.ne __rt_array_set_mixed_no_marker");                 // ordinary Mixed slots are replaced below
    emitter.instruction("ldr x12, [x11, #16]");                                 // the caller cell's runtime value tag
    emitter.instruction("ldr x10, [x11, #8]");                                  // the caller cell's address
    emitter.instruction(&format!("cmp x12, #{}", crate::codegen::runtime_value_tag(&crate::types::PhpType::Mixed)));
    emitter.instruction("b.eq __rt_array_set_mixed_marker_cell");               // a Mixed caller cell takes the handle itself
    emitter.instruction("ldr x9, [x2, #8]");                                    // the replacement's low payload word
    emitter.instruction("str x9, [x10]");                                       // write it through the caller cell
    emitter.instruction(&format!("cmp x12, #{}", crate::codegen::runtime_value_tag(&crate::types::PhpType::Str)));
    emitter.instruction("b.ne __rt_array_set_mixed_marker_scalar_done");        // a one-word cell must not lose its NEIGHBOUR
    emitter.instruction("ldr x9, [x2, #16]");                                   // the replacement's high payload word
    emitter.instruction("str x9, [x10, #8]");                                   // a string caller cell is pointer AND length
    emitter.label("__rt_array_set_mixed_marker_scalar_done");
    emitter.instruction("mov x0, x2");                                          // the value this helper was handed to consume
    emitter.instruction("bl __rt_decref_mixed");                                // its payload has been copied out; drop the reference
    emitter.instruction("ldr x0, [sp, #0]");                                    // the array is unchanged by a marker write
    emitter.instruction("b __rt_array_set_mixed_return");
    emitter.label("__rt_array_set_mixed_marker_cell");
    emitter.instruction("str x2, [x10]");                                       // hand the boxed Mixed to the caller cell
    emitter.instruction("ldr x0, [sp, #0]");                                    // the array is unchanged by a marker write
    emitter.instruction("b __rt_array_set_mixed_return");
    emitter.label("__rt_array_set_mixed_no_marker");
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the array pointer the checks clobbered
    emitter.instruction("ldr x1, [sp, #8]");                                     // and the index
    emitter.instruction("ldr x2, [sp, #16]");                                    // and the value

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

    // See the AArch64 counterpart: a slot holding an invoker ref-cell marker aliases the
    // CALLER's storage, and the `foreach` road reaches this helper through
    // `__rt_array_set_mixed_key`.
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // the array's logical length
    emitter.instruction("cmp rsi, r9");                                         // only an existing slot can hold a marker
    emitter.instruction("jae __rt_array_set_mixed_no_marker_x");                // appends and gap writes are ordinary
    emitter.instruction("mov r10, QWORD PTR [rdi + 24 + rsi * 8]");             // the existing slot
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_array_set_mixed_no_marker_x");                 // a null gap slot is an ordinary write
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // the slot's Mixed tag
    emitter.instruction(&format!("cmp r11, {}", crate::codegen_support::callable_invoker_args::INVOKER_ARG_REF_CELL_TAG));
    emitter.instruction("jne __rt_array_set_mixed_no_marker_x");                // ordinary Mixed slots are replaced below
    emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                       // the caller cell's runtime value tag
    emitter.instruction("mov r10, QWORD PTR [r10 + 8]");                        // the caller cell's address
    emitter.instruction(&format!("cmp r11, {}", crate::codegen::runtime_value_tag(&crate::types::PhpType::Mixed)));
    emitter.instruction("je __rt_array_set_mixed_marker_cell_x");               // a Mixed caller cell takes the handle itself
    emitter.instruction("mov r9, QWORD PTR [rdx + 8]");                         // the replacement's low payload word
    emitter.instruction("mov QWORD PTR [r10], r9");                             // write it through the caller cell
    emitter.instruction(&format!("cmp r11, {}", crate::codegen::runtime_value_tag(&crate::types::PhpType::Str)));
    emitter.instruction("jne __rt_array_set_mixed_marker_scalar_done_x");       // a one-word cell must not lose its NEIGHBOUR
    emitter.instruction("mov r9, QWORD PTR [rdx + 16]");                        // the replacement's high payload word
    emitter.instruction("mov QWORD PTR [r10 + 8], r9");                         // a string caller cell is pointer AND length
    emitter.label("__rt_array_set_mixed_marker_scalar_done_x");
    emitter.instruction("mov rdi, rdx");                                        // the value this helper was handed to consume
    emitter.instruction("call __rt_decref_mixed");                              // its payload has been copied out; drop the reference
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the array is unchanged by a marker write
    emitter.instruction("jmp __rt_array_set_mixed_return");
    emitter.label("__rt_array_set_mixed_marker_cell_x");
    emitter.instruction("mov QWORD PTR [r10], rdx");                            // hand the boxed Mixed to the caller cell
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the array is unchanged by a marker write
    emitter.instruction("jmp __rt_array_set_mixed_return");
    emitter.label("__rt_array_set_mixed_no_marker_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the array pointer the checks clobbered
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // and the index
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // and the value

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
