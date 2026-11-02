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
//! - A destination already promoted to hash storage (kind 3, from an earlier sparse/negative
//!   write) is detected up front and inserted through `__rt_hash_set` directly; `__rt_hash_set`
//!   performs its own copy-on-write split, so the indexed `__rt_array_ensure_unique` path is
//!   skipped entirely for that case.
//! - An indexed destination whose target index is negative, or greater than the current
//!   logical length, is promoted to a hash via `__rt_array_to_hash` (which preserves insertion
//!   order; this array's value_type is always stamped 7 = Mixed) before the new entry is
//!   inserted — PHP never pads a gap with nulls. The superseded indexed container is released
//!   with `__rt_decref_array` once its entries have been retained into the new hash. A negative
//!   key used to be silently DROPPED (the value released, the array returned unchanged); PHP
//!   accepts a negative integer key as an ordinary hash key, so it is promoted like any other
//!   out-of-range key instead.
//! - A slot that owns a share of a reference (marker tag 12 or 13) is written THROUGH by a plain
//!   value (`__rt_ref_marker_store`) and keeps its marker; only an incoming reference -- `=&` --
//!   replaces it.

use crate::codegen_support::callable_invoker_args::{
    ARRAY_GLOBAL_REF_CELL_TAG, ARRAY_LOCAL_REF_CELL_TAG, INVOKER_ARG_REF_CELL_TAG,
};
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

    // Stack layout: [sp,#0]=array/hash ptr, [sp,#8]=index, [sp,#16]=consumed boxed Mixed value,
    // [sp,#24]=unique array ptr / promoted-hash scratch, [sp,#32]=index copy, [sp,#40]=original
    // logical length, [sp,#64/#72]=saved fp/lr.
    emitter.instruction("sub sp, sp, #80");                                     // reserve frame for array, index, value, growth state, and saved registers
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish a helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the incoming array/hash pointer for the kind dispatch
    emitter.instruction("str x1, [sp, #8]");                                    // save the target index
    emitter.instruction("str x2, [sp, #16]");                                   // save the consumed boxed Mixed value

    // -- dispatch on the destination's CURRENT runtime storage kind --
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the packed heap-kind/value_type word
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low byte holding the storage kind
    emitter.instruction("cmp x9, #3");                                          // kind 3 = an earlier write already promoted this destination to a hash
    emitter.instruction("b.eq __rt_array_set_mixed_already_hash");              // insert directly; __rt_hash_set performs its own COW split

    // -- indexed destination: split shared storage before mutating --
    emitter.instruction("bl __rt_array_ensure_unique");                        // split shared indexed arrays before mutating boxed Mixed slots
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

    // -- promotion check: PHP never pads a gap, and never drops a negative key --
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload the original logical length
    emitter.instruction("cmp x9, #0");                                         // negative indexes cannot live in packed indexed storage
    emitter.instruction("b.lt __rt_array_set_mixed_promote");                   // promote to a hash so a negative key survives like PHP
    emitter.instruction("cmp x9, x11");                                         // a key past the end would create a sparse gap
    emitter.instruction("b.hi __rt_array_set_mixed_promote");                   // promote to a hash so a sparse key survives like PHP

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
    emitter.instruction("cbz x0, __rt_array_set_mixed_release_old");            // a null gap is replaced like any value
    emitter.instruction("ldr x13, [x0]");                                       // inspect the replaced box's tag
    emitter.instruction(&format!("cmp x13, #{}", ARRAY_GLOBAL_REF_CELL_TAG));   // does the slot own a share of a global reference?
    emitter.instruction("b.eq __rt_array_set_mixed_owned_ref");                 // a plain write goes through it
    emitter.instruction(&format!("cmp x13, #{}", ARRAY_LOCAL_REF_CELL_TAG));    // does the slot own a share of a local reference?
    emitter.instruction("b.ne __rt_array_set_mixed_release_old");               // ordinary boxes are simply replaced
    emitter.label("__rt_array_set_mixed_owned_ref");
    emitter.instruction("ldr x14, [sp, #16]");                                  // the incoming box
    emitter.instruction("cbz x14, __rt_array_set_mixed_store_through");         // a null box is a plain value
    emitter.instruction("ldr x13, [x14]");                                      // inspect the incoming box's tag
    emitter.instruction(&format!("cmp x13, #{}", INVOKER_ARG_REF_CELL_TAG));    // is the incoming value a reference too?
    emitter.instruction("b.eq __rt_array_set_mixed_release_old");               // `=&` rebinds: release this slot's share, store the new one
    emitter.instruction(&format!("cmp x13, #{}", ARRAY_GLOBAL_REF_CELL_TAG));   // is the incoming value a global reference?
    emitter.instruction("b.eq __rt_array_set_mixed_release_old");               // `=&` rebinds: release this slot's share, store the new one
    emitter.instruction(&format!("cmp x13, #{}", ARRAY_LOCAL_REF_CELL_TAG));    // is the incoming value a local reference?
    emitter.instruction("b.eq __rt_array_set_mixed_release_old");               // `=&` rebinds: release this slot's share, store the new one
    emitter.label("__rt_array_set_mixed_store_through");
    emitter.instruction("str x0, [sp, #48]");                                   // keep the slot's marker for the typed-cell fallback
    emitter.instruction("mov x1, x14");                                         // the incoming box is the value to write through
    emitter.instruction("mov x2, #0");                                          // a box has no high payload word
    emitter.instruction("mov x3, #7");                                          // runtime value tag 7 = boxed Mixed
    emitter.instruction("bl __rt_ref_marker_store");                            // write the value into the shared reference cell
    emitter.instruction("cbnz x0, __rt_array_set_mixed_done");                  // the slot keeps its marker; nothing else to store
    emitter.instruction("ldr x0, [sp, #48]");                                   // a typed cell: replace the marker as before
    emitter.label("__rt_array_set_mixed_release_old");
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

    // -- indexed destination + negative/sparse int key: promote to hash then insert --
    emitter.label("__rt_array_set_mixed_promote");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the unique indexed-array pointer
    emitter.instruction("bl __rt_array_to_hash");                               // build an owned hash preserving order (value_type is already Mixed)
    emitter.instruction("str x0, [sp, #48]");                                   // save the promoted hash pointer
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the now-superseded unique indexed array
    emitter.instruction("bl __rt_decref_array");                                // release it once its entries are retained by the promoted hash
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the promoted hash as the insert target
    emitter.instruction("b __rt_array_set_mixed_hash_insert");

    // -- destination already promoted: insert directly (hash_set handles COW) --
    emitter.label("__rt_array_set_mixed_already_hash");

    emitter.label("__rt_array_set_mixed_hash_insert");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the target integer key
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks a scalar integer hash key
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the consumed boxed Mixed value as the hash value low word
    emitter.instruction("mov x4, #0");                                          // boxed Mixed hash payloads leave the high value word empty
    emitter.instruction("mov x5, #7");                                          // runtime value tag 7 = boxed Mixed
    emitter.instruction("bl __rt_hash_set");                                    // insert the entry, growing/COW-splitting the hash as needed
    emitter.instruction("b __rt_array_set_mixed_return");

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

    // Frame layout: [rbp-8]=array/hash ptr, [rbp-16]=index, [rbp-24]=consumed boxed Mixed value,
    // [rbp-32]=unique array ptr / promoted-hash scratch, [rbp-40]=index copy, [rbp-48]=original
    // logical length, [rbp-56]=second promoted-hash scratch.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 64");                                         // reserve slots for inputs, array state, indexes, and value pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the incoming array/hash pointer for the kind dispatch
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the target index
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the consumed boxed Mixed value

    // -- dispatch on the destination's CURRENT runtime storage kind --
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the packed heap-kind/value_type word
    emitter.instruction("and r10, 0xff");                                       // isolate the low byte holding the storage kind
    emitter.instruction("cmp r10, 3");                                          // kind 3 = an earlier write already promoted this destination to a hash
    emitter.instruction("je __rt_array_set_mixed_already_hash");                // insert directly; __rt_hash_set performs its own COW split

    // -- indexed destination: split shared storage before mutating --
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

    // -- promotion check: PHP never pads a gap, and never drops a negative key --
    emitter.instruction("cmp r9, 0");                                           // negative indexes cannot live in packed indexed storage
    emitter.instruction("jl __rt_array_set_mixed_promote");                     // promote to a hash so a negative key survives like PHP
    emitter.instruction("cmp r9, QWORD PTR [rbp - 48]");                        // a key past the end (original logical length) would create a sparse gap
    emitter.instruction("ja __rt_array_set_mixed_promote");                     // promote to a hash so a sparse key survives like PHP

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
    emitter.instruction("test rax, rax");                                       // a null gap is replaced like any value
    emitter.instruction("jz __rt_array_set_mixed_release_old");                 // release nothing for it
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // inspect the replaced box's tag
    emitter.instruction(&format!("cmp r11, {}", ARRAY_GLOBAL_REF_CELL_TAG));    // does the slot own a share of a global reference?
    emitter.instruction("je __rt_array_set_mixed_owned_ref");                   // a plain write goes through it
    emitter.instruction(&format!("cmp r11, {}", ARRAY_LOCAL_REF_CELL_TAG));     // does the slot own a share of a local reference?
    emitter.instruction("jne __rt_array_set_mixed_release_old");                // ordinary boxes are simply replaced
    emitter.label("__rt_array_set_mixed_owned_ref");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // the incoming box
    emitter.instruction("test rsi, rsi");                                       // a null box is a plain value
    emitter.instruction("jz __rt_array_set_mixed_store_through");               // write it through the reference
    emitter.instruction("mov r11, QWORD PTR [rsi]");                            // inspect the incoming box's tag
    emitter.instruction(&format!("cmp r11, {}", INVOKER_ARG_REF_CELL_TAG));     // is the incoming value a reference too?
    emitter.instruction("je __rt_array_set_mixed_release_old");                 // `=&` rebinds: release this slot's share, store the new one
    emitter.instruction(&format!("cmp r11, {}", ARRAY_GLOBAL_REF_CELL_TAG));    // is the incoming value a global reference?
    emitter.instruction("je __rt_array_set_mixed_release_old");                 // `=&` rebinds: release this slot's share, store the new one
    emitter.instruction(&format!("cmp r11, {}", ARRAY_LOCAL_REF_CELL_TAG));     // is the incoming value a local reference?
    emitter.instruction("je __rt_array_set_mixed_release_old");                 // `=&` rebinds: release this slot's share, store the new one
    emitter.label("__rt_array_set_mixed_store_through");
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // keep the slot's marker for the typed-cell fallback
    emitter.instruction("mov rdi, rax");                                        // the slot's marker box
    emitter.instruction("xor edx, edx");                                        // a box has no high payload word
    emitter.instruction("mov ecx, 7");                                          // runtime value tag 7 = boxed Mixed
    emitter.instruction("call __rt_ref_marker_store");                          // write the value into the shared reference cell
    emitter.instruction("test rax, rax");                                       // was the value written through?
    emitter.instruction("jnz __rt_array_set_mixed_done");                       // the slot keeps its marker; nothing else to store
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // a typed cell: replace the marker as before
    emitter.label("__rt_array_set_mixed_release_old");
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

    // -- indexed destination + negative/sparse int key: promote to hash then insert --
    emitter.label("__rt_array_set_mixed_promote");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the unique indexed-array pointer
    emitter.instruction("call __rt_array_to_hash");                            // build an owned hash preserving order (value_type is already Mixed)
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                      // save the promoted hash pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                      // reload the now-superseded unique indexed array
    emitter.instruction("call __rt_decref_array");                             // release it once its entries are retained by the promoted hash
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                      // reload the promoted hash as the insert target
    emitter.instruction("jmp __rt_array_set_mixed_hash_insert");

    // -- destination already promoted: insert directly (hash_set handles COW) --
    emitter.label("__rt_array_set_mixed_already_hash");
    emitter.instruction("mov rax, rdi");                                       // the incoming pointer is already the hash insert target

    emitter.label("__rt_array_set_mixed_hash_insert");
    emitter.instruction("mov rdi, rax");                                       // pass the array/hash pointer as the hash_set target
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                      // reload the target integer key
    emitter.instruction("mov rdx, -1");                                        // key_hi sentinel marks a scalar integer hash key
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                      // reload the consumed boxed Mixed value as the hash value low word
    emitter.instruction("xor r8, r8");                                         // boxed Mixed hash payloads leave the high value word empty
    emitter.instruction("mov r9, 7");                                          // runtime value tag 7 = boxed Mixed
    emitter.instruction("call __rt_hash_set");                                 // insert the entry, growing/COW-splitting the hash as needed
    emitter.instruction("jmp __rt_array_set_mixed_return");

    emitter.label("__rt_array_set_mixed_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                      // return the final indexed-array pointer
    emitter.label("__rt_array_set_mixed_return");
    emitter.instruction("mov rsp, rbp");                                       // restore stack pointer
    emitter.instruction("pop rbp");                                            // restore caller frame pointer
    emitter.instruction("ret");                                                // return to generated code
}
