//! Purpose:
//! Emits the `__rt_array_set_int` runtime helper for indexed-array scalar writes.
//! Keeps direct slot assignment target-aware while sharing COW, grow, and length extension.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The helper stores one 8-byte payload slot and returns the possibly reallocated array pointer.
//! - A destination already promoted to hash storage (kind 3, from an earlier sparse/negative
//!   write) is detected up front and inserted through `__rt_hash_set` directly; `__rt_hash_set`
//!   performs its own copy-on-write split, so the indexed `__rt_array_ensure_unique` path is
//!   skipped entirely for that case.
//! - An indexed destination whose target index is negative, or greater than the current
//!   logical length, is promoted to a hash via `__rt_array_to_hash` (which preserves insertion
//!   order and the existing homogeneous value_type) before the new entry is inserted — PHP
//!   never pads a gap with nulls. The superseded indexed container is released with
//!   `__rt_decref_array` once its entries have been retained into the new hash.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

#[cfg(test)]
mod tests;

/// Emits the scalar indexed-array set helper for the current target.
pub fn emit_array_set_int(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_set_int_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_set_int ---");
    emitter.label_global("__rt_array_set_int");

    // -- overwrite a unique in-bounds scalar slot without entering the slow path --
    emitter.comment("-- overwrite a unique in-bounds scalar slot without entering the slow path --");
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the packed heap-kind/value_type word
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low byte holding the storage kind
    emitter.instruction("cmp x9, #3");                                          // kind 3 = a promoted hash, which has no scalar payload region
    emitter.instruction("b.eq __rt_array_set_int_slow");                        // hashes go through the kind dispatch below
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load the current array refcount from the uniform heap header
    emitter.instruction("cmp w9, #1");                                          // can this owner mutate the current payload without a COW split?
    emitter.instruction("b.hi __rt_array_set_int_slow");                        // shared arrays still need the cloning path below
    emitter.instruction("ldr x9, [x0]");                                        // load the current logical length for the overwrite bounds check
    emitter.instruction("cmp x1, x9");                                          // is the requested offset already materialized? (a negative index compares as huge)
    emitter.instruction("b.hs __rt_array_set_int_slow");                        // first writes, extensions and negative keys still need shape, growth or promotion handling
    emitter.instruction("add x9, x0, #24");                                     // compute the base address of the scalar payload region
    emitter.instruction("str x2, [x9, x1, lsl #3]");                            // overwrite the existing unique scalar slot directly
    emitter.instruction("ret");                                                 // return the unchanged unique array pointer

    emitter.label("__rt_array_set_int_slow");
    // Stack layout: [sp,#0]=array/hash ptr, [sp,#8]=index, [sp,#16]=scalar payload,
    // [sp,#24]=promoted-hash scratch, [sp,#48/#56]=saved fp/lr.
    emitter.instruction("sub sp, sp, #64");                                     // reserve spill space for array, index, payload, promotion scratch, and saved frame state
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a frame pointer for nested runtime helper calls
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the incoming array/hash pointer for the kind dispatch
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the target index across copy-on-write splitting
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the scalar payload across copy-on-write splitting

    // -- dispatch on the destination's CURRENT runtime storage kind --
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the packed heap-kind/value_type word
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low byte holding the storage kind
    emitter.instruction("cmp x9, #3");                                          // kind 3 = an earlier write already promoted this destination to a hash
    emitter.instruction("b.eq __rt_array_set_int_already_hash");                // insert directly; __rt_hash_set performs its own COW split

    // -- indexed destination: split shared storage before mutating --
    emitter.instruction("bl __rt_array_ensure_unique");                        // split shared indexed arrays before mutating the payload storage
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the unique indexed-array pointer across shape and growth work

    emitter.instruction("ldr x9, [x0]");                                        // load the current logical length before first-write shape normalization
    emitter.instruction("cbnz x9, __rt_array_set_int_shape_ready");             // non-empty indexed arrays already have a stable element layout
    emitter.instruction("mov x10, #8");                                         // scalar indexed arrays use pointer-sized payload slots
    emitter.instruction("str x10, [x0, #16]");                                  // publish the scalar slot width before any later growth copies payload bytes
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load packed indexed-array metadata from the heap header
    emitter.instruction("mov x11, #0x80ff");                                    // keep only indexed-array kind and persistent copy-on-write bits
    emitter.instruction("and x10, x10, x11");                                   // clear stale string value_type metadata on first scalar write
    emitter.instruction("str x10, [x0, #-8]");                                  // persist scalar-oriented indexed-array metadata
    emitter.label("__rt_array_set_int_shape_ready");

    // -- promotion check: PHP never pads a gap, it converts to a hash --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the target index (helper calls above may have clobbered it)
    emitter.instruction("ldr x9, [x0]");                                        // reload the current logical length
    emitter.instruction("cmp x1, #0");                                          // negative indexes cannot live in packed indexed storage
    emitter.instruction("b.lt __rt_array_set_int_promote");                     // promote to a hash so a negative key survives like PHP
    emitter.instruction("cmp x1, x9");                                          // a key past the end would create a sparse gap
    emitter.instruction("b.hi __rt_array_set_int_promote");                     // promote to a hash so a sparse key survives like PHP

    emitter.label("__rt_array_set_int_grow_check");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the current indexed-array pointer before checking capacity
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the target index after helper calls may have clobbered registers
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the current indexed-array capacity
    emitter.instruction("cmp x1, x10");                                         // does the target offset fit in the current allocation?
    emitter.instruction("b.lo __rt_array_set_int_store");                       // write directly once the slot is addressable
    emitter.instruction("bl __rt_array_grow");                                  // grow the indexed array so the target slot can be materialized
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the possibly reallocated indexed-array pointer
    emitter.instruction("b __rt_array_set_int_grow_check");                     // keep growing until the target offset fits within capacity

    emitter.label("__rt_array_set_int_store");
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the scalar payload that should be stored
    emitter.instruction("add x10, x0, #24");                                    // compute the base address of the scalar payload region
    emitter.instruction("str x2, [x10, x1, lsl #3]");                           // write the scalar payload into the addressed indexed-array slot
    emitter.instruction("ldr x9, [x0]");                                        // reload logical length to decide whether this write extends the array
    emitter.instruction("cmp x1, x9");                                          // does the target index overwrite an existing slot?
    emitter.instruction("b.lo __rt_array_set_int_done");                        // keep the current logical length for in-bounds overwrites
    emitter.instruction("mov x11, x9");                                         // start zero-filling gaps at the previous logical length
    emitter.label("__rt_array_set_int_fill_loop");
    emitter.instruction("cmp x11, x1");                                         // have all slots before the target index been initialized?
    emitter.instruction("b.ge __rt_array_set_int_store_len");                   // stop gap filling before touching the target slot
    emitter.instruction("str xzr, [x10, x11, lsl #3]");                         // initialize the scalar gap slot to zero/null
    emitter.instruction("add x11, x11, #1");                                    // advance to the next gap slot
    emitter.instruction("b __rt_array_set_int_fill_loop");                      // continue zero-filling until the target slot is reached
    emitter.label("__rt_array_set_int_store_len");
    emitter.instruction("add x11, x1, #1");                                     // compute the new logical length as target index plus one
    emitter.instruction("str x11, [x0]");                                       // publish the extended indexed-array length
    emitter.instruction("b __rt_array_set_int_done");                          // finish after an in-place indexed write

    // -- indexed destination + negative/sparse int key: promote to hash then insert --
    emitter.label("__rt_array_set_int_promote");
    emitter.instruction("bl __rt_array_to_hash");                               // build an owned hash preserving order and the existing value_type
    emitter.instruction("str x0, [sp, #24]");                                   // save the promoted hash pointer
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the now-superseded unique indexed array
    emitter.instruction("bl __rt_decref_array");                                // release it once its entries are retained by the promoted hash
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the promoted hash as the insert target
    emitter.instruction("b __rt_array_set_int_hash_insert");

    // -- destination already promoted: insert directly (hash_set handles COW) --
    emitter.label("__rt_array_set_int_already_hash");

    emitter.label("__rt_array_set_int_hash_insert");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the target integer key
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks a scalar integer hash key
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the scalar payload as the hash value low word
    emitter.instruction("mov x4, #0");                                          // scalar hash payloads leave the high value word empty
    emitter.instruction("mov x5, #0");                                         // homogeneous scalar runtime tag (matches the cleared indexed metadata)
    emitter.instruction("bl __rt_hash_set");                                    // insert the entry, growing/COW-splitting the hash as needed
    emitter.label("__rt_array_set_int_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release scalar set helper spill space
    emitter.instruction("ret");                                                 // return with x0 holding the current array/hash pointer
}

/// Emits the Linux x86_64 scalar indexed-array set helper.
fn emit_array_set_int_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_set_int ---");
    emitter.label_global("__rt_array_set_int");

    emitter.instruction("mov rax, rdi");                                        // default the return value to the incoming indexed-array pointer
    // -- overwrite a unique in-bounds scalar slot without entering the slow path --
    emitter.comment("-- overwrite a unique in-bounds scalar slot without entering the slow path --");
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the packed heap-kind/value_type word
    emitter.instruction("and r10, 0xff");                                       // isolate the low byte holding the storage kind
    emitter.instruction("cmp r10, 3");                                          // kind 3 = a promoted hash, which has no scalar payload region
    emitter.instruction("je __rt_array_set_int_slow");                          // hashes go through the kind dispatch below
    emitter.instruction("mov r10d, DWORD PTR [rdi - 12]");                      // load the current indexed-array refcount from the uniform heap header
    emitter.instruction("cmp r10d, 1");                                         // can this owner mutate the current payload without a COW split?
    emitter.instruction("ja __rt_array_set_int_slow");                          // shared arrays still need the cloning path below
    emitter.instruction("cmp rsi, QWORD PTR [rdi]");                            // is the requested offset already materialized? (a negative index compares as huge)
    emitter.instruction("jae __rt_array_set_int_slow");                         // first writes, extensions and negative keys still need shape, growth or promotion handling
    emitter.instruction("mov QWORD PTR [rdi + 24 + rsi * 8], rdx");             // overwrite the existing unique scalar slot directly
    emitter.instruction("ret");                                                 // return the unchanged unique indexed-array pointer

    emitter.label("__rt_array_set_int_slow");
    // Frame layout: [rbp-8]=array/hash ptr, [rbp-16]=index, [rbp-24]=scalar payload,
    // [rbp-32]=promoted-hash scratch.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving helper spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for array, index, and payload spills
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for array pointer, index, payload, and promotion scratch
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the incoming array/hash pointer for the kind dispatch
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the target index across copy-on-write and growth helpers
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the scalar payload across copy-on-write and growth helpers

    // -- dispatch on the destination's CURRENT runtime storage kind --
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the packed heap-kind/value_type word
    emitter.instruction("and r10, 0xff");                                       // isolate the low byte holding the storage kind
    emitter.instruction("cmp r10, 3");                                          // kind 3 = an earlier write already promoted this destination to a hash
    emitter.instruction("je __rt_array_set_int_already_hash");                  // insert directly; __rt_hash_set performs its own COW split

    // -- indexed destination: split shared storage before mutating --
    emitter.instruction("call __rt_array_ensure_unique");                       // split shared indexed arrays before mutating payload storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the unique indexed-array pointer across shape and growth work

    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load logical length before first-write shape normalization
    emitter.instruction("test r10, r10");                                       // is this the first write into the indexed array?
    emitter.instruction("jnz __rt_array_set_int_shape_ready");                  // non-empty indexed arrays already have a stable element layout
    emitter.instruction("mov QWORD PTR [rax + 16], 8");                         // scalar indexed arrays use pointer-sized payload slots
    emitter.instruction("mov r11, QWORD PTR [rax - 8]");                        // load packed indexed-array metadata from the heap header
    emitter.instruction("mov r8, 0xffffffff000080ff");                          // preserve heap marker, indexed-array kind, and copy-on-write metadata
    emitter.instruction("and r11, r8");                                        // clear stale string value_type metadata on first scalar write
    emitter.instruction("mov QWORD PTR [rax - 8], r11");                        // persist scalar-oriented indexed-array metadata
    emitter.label("__rt_array_set_int_shape_ready");

    // -- promotion check: PHP never pads a gap, it converts to a hash --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the unique indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the target index (helper calls above may have clobbered it)
    emitter.instruction("cmp rsi, 0");                                          // negative indexes cannot live in packed indexed storage
    emitter.instruction("jl __rt_array_set_int_promote");                       // promote to a hash so a negative key survives like PHP
    emitter.instruction("cmp rsi, QWORD PTR [rax]");                            // a key past the end would create a sparse gap
    emitter.instruction("ja __rt_array_set_int_promote");                       // promote to a hash so a sparse key survives like PHP

    emitter.label("__rt_array_set_int_grow_check");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the current indexed-array pointer before checking capacity
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the target index after helper calls may have clobbered registers
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the current indexed-array capacity
    emitter.instruction("cmp rsi, r10");                                        // does the target offset fit in the current allocation?
    emitter.instruction("jb __rt_array_set_int_store");                         // write directly once the slot is addressable
    emitter.instruction("mov rdi, rax");                                        // pass the current indexed-array pointer to the growth helper
    emitter.instruction("call __rt_array_grow");                                // grow the indexed array so the target slot can be materialized
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the possibly reallocated indexed-array pointer
    emitter.instruction("jmp __rt_array_set_int_grow_check");                   // keep growing until the target offset fits within capacity

    emitter.label("__rt_array_set_int_store");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the scalar payload that should be stored
    emitter.instruction("mov QWORD PTR [rax + 24 + rsi * 8], rdx");             // write the scalar payload into the addressed indexed-array slot
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload logical length to decide whether this write extends the array
    emitter.instruction("cmp rsi, r9");                                         // does the target index overwrite an existing slot?
    emitter.instruction("jb __rt_array_set_int_done");                          // keep the current logical length for in-bounds overwrites
    emitter.instruction("mov r11, r9");                                         // start zero-filling gaps at the previous logical length
    emitter.label("__rt_array_set_int_fill_loop");
    emitter.instruction("cmp r11, rsi");                                        // have all slots before the target index been initialized?
    emitter.instruction("jae __rt_array_set_int_store_len");                    // stop gap filling before touching the target slot
    emitter.instruction("mov QWORD PTR [rax + 24 + r11 * 8], 0");               // initialize the scalar gap slot to zero/null
    emitter.instruction("add r11, 1");                                          // advance to the next gap slot
    emitter.instruction("jmp __rt_array_set_int_fill_loop");                    // continue zero-filling until the target slot is reached
    emitter.label("__rt_array_set_int_store_len");
    emitter.instruction("lea r11, [rsi + 1]");                                  // compute the new logical length as target index plus one
    emitter.instruction("mov QWORD PTR [rax], r11");                            // publish the extended indexed-array length
    emitter.instruction("jmp __rt_array_set_int_done");                         // finish after an in-place indexed write

    // -- indexed destination + negative/sparse int key: promote to hash then insert --
    emitter.label("__rt_array_set_int_promote");
    emitter.instruction("call __rt_array_to_hash");                            // build an owned hash preserving order and the existing value_type
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                      // save the promoted hash pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                       // reload the now-superseded unique indexed array
    emitter.instruction("call __rt_decref_array");                             // release it once its entries are retained by the promoted hash
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                      // reload the promoted hash as the insert target
    emitter.instruction("jmp __rt_array_set_int_hash_insert");

    // -- destination already promoted: insert directly (hash_set handles COW) --
    emitter.label("__rt_array_set_int_already_hash");
    emitter.instruction("mov rax, rdi");                                       // the incoming pointer is already the hash insert target

    emitter.label("__rt_array_set_int_hash_insert");
    emitter.instruction("mov rdi, rax");                                       // pass the array/hash pointer as the hash_set target
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                      // reload the target integer key
    emitter.instruction("mov rdx, -1");                                        // key_hi sentinel marks a scalar integer hash key
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                      // reload the scalar payload as the hash value low word
    emitter.instruction("xor r8, r8");                                         // scalar hash payloads leave the high value word empty
    emitter.instruction("xor r9, r9");                                         // homogeneous scalar runtime tag (matches the cleared indexed metadata)
    emitter.instruction("call __rt_hash_set");                                 // insert the entry, growing/COW-splitting the hash as needed
    emitter.label("__rt_array_set_int_done");
    emitter.instruction("add rsp, 32");                                        // release scalar set helper spill space
    emitter.instruction("pop rbp");                                            // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                // return with rax holding the current array/hash pointer
}
