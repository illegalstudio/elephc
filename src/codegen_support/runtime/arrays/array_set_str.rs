//! Purpose:
//! Emits the `__rt_array_set_str` runtime helper for indexed-array string writes.
//! Keeps 16-byte string slots, COW, growth, old-slot release, and length extension together.
//! - Widening an empty array's slot from 8 to 16 bytes must RESTATE the capacity in the new unit.
//!   `__rt_array_new` sizes an `array<never>` for 8-byte slots, so leaving the old count behind
//!   claims twice the buffer and the grow check never fires in time.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The helper persists incoming string payloads before storing them in long-lived array storage.
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
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    #[test]
    fn string_set_recounts_empty_capacity_before_publishing_stride() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_array_set_str(&mut emitter);
            let asm = emitter.output();
            let (count, capacity, stride) = match target.arch {
                Arch::AArch64 => ("mul x11, x11, x10", "str x11, [x0, #8]", "str x10, [x0, #16]"),
                Arch::X86_64 => ("imul r11, r10", "mov QWORD PTR [rax + 8], r11", "mov QWORD PTR [rax + 16], 16"),
            };
            let count_at = asm.find(count).expect("recount allocated bytes as string slots");
            let capacity_at = asm.find(capacity).expect("publish the adjusted slot capacity");
            let stride_at = asm.find(stride).expect("publish the sixteen-byte stride");
            assert!(count_at < capacity_at && capacity_at < stride_at, "{name}: {asm}");
        }
    }
}

/// Emits the string indexed-array set helper for the current target.
pub fn emit_array_set_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_set_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_set_str ---");
    emitter.label_global("__rt_array_set_str");

    // Stack layout: [sp,#0]=array/hash ptr, [sp,#8]=index, [sp,#16]=string ptr, [sp,#24]=string
    // len, [sp,#32]=promoted-hash scratch, [sp,#48/#56]=saved fp/lr.
    emitter.instruction("sub sp, sp, #64");                                     // reserve spill space for array, index, string payload, promotion scratch, and frame state
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the caller frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a frame pointer for nested runtime helper calls
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the incoming array/hash pointer for the kind dispatch
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the target index across copy-on-write splitting
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the incoming string pointer across copy-on-write splitting
    emitter.instruction("str x3, [sp, #24]");                                   // preserve the incoming string length across copy-on-write splitting

    // -- persist the incoming string BEFORE the kind dispatch: both the indexed and the
    //    already-hash insert paths need an owned (not transient) string payload --
    emitter.instruction("ldr x1, [sp, #16]");                                   // move the incoming string pointer into the persist helper argument register
    emitter.instruction("ldr x2, [sp, #24]");                                   // move the incoming string length into the persist helper argument register
    emitter.instruction("bl __rt_str_persist");                                 // duplicate transient strings into owned heap storage for the array slot
    emitter.instruction("str x1, [sp, #16]");                                   // preserve the owned string pointer after persistence
    emitter.instruction("str x2, [sp, #24]");                                   // preserve the owned string length after persistence

    // -- dispatch on the destination's CURRENT runtime storage kind --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the incoming array/hash pointer after the persist call
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the packed heap-kind/value_type word
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low byte holding the storage kind
    emitter.instruction("cmp x9, #3");                                          // kind 3 = an earlier write already promoted this destination to a hash
    emitter.instruction("b.eq __rt_array_set_str_already_hash");                // insert directly; __rt_hash_set performs its own COW split

    // -- indexed destination: split shared storage before mutating --
    emitter.instruction("bl __rt_array_ensure_unique");                        // split shared indexed arrays before mutating the payload storage
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the unique indexed-array pointer across persistence and growth

    emitter.instruction("ldr x9, [x0]");                                        // load logical length before first-write string layout normalization
    emitter.instruction("cbnz x9, __rt_array_set_str_shape_ready");             // non-empty indexed arrays already have a stable element layout
    super::array_push_str::emit_empty_string_capacity(emitter);
    emitter.instruction("mov x10, #16");                                        // string indexed arrays use pointer-plus-length payload slots
    emitter.instruction("str x10, [x0, #16]");                                  // publish the string slot width before any later growth copies payload bytes
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load packed indexed-array metadata from the heap header
    emitter.instruction("mov x11, #0x80ff");                                    // keep only indexed-array kind and persistent copy-on-write bits
    emitter.instruction("and x10, x10, x11");                                   // clear stale value_type metadata on first string write
    emitter.instruction("mov x11, #1");                                         // materialize the runtime value_type tag for string payload slots
    emitter.instruction("lsl x11, x11, #8");                                    // move the string tag into the packed kind-word value_type lane
    emitter.instruction("orr x10, x10, x11");                                   // combine stable indexed-array metadata with the string value_type tag
    emitter.instruction("str x10, [x0, #-8]");                                  // persist string-oriented indexed-array metadata
    emitter.label("__rt_array_set_str_shape_ready");

    // -- promotion check: PHP never pads a gap, it converts to a hash --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the current indexed-array pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the target index after helper calls may have clobbered registers
    emitter.instruction("ldr x9, [x0]");                                        // reload the current logical length
    emitter.instruction("cmp x1, #0");                                          // negative indexes cannot live in packed indexed storage
    emitter.instruction("b.lt __rt_array_set_str_promote");                     // promote to a hash so a negative key survives like PHP
    emitter.instruction("cmp x1, x9");                                          // a key past the end would create a sparse gap
    emitter.instruction("b.hi __rt_array_set_str_promote");                     // promote to a hash so a sparse key survives like PHP

    emitter.label("__rt_array_set_str_grow_check");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the current indexed-array pointer before checking capacity
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the target index after helper calls may have clobbered registers
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the current indexed-array capacity
    emitter.instruction("cmp x1, x10");                                         // does the target offset fit in the current allocation?
    emitter.instruction("b.lo __rt_array_set_str_store");                       // write directly once the slot is addressable
    emitter.instruction("bl __rt_array_grow");                                  // grow the indexed array so the target slot can be materialized
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the possibly reallocated indexed-array pointer
    emitter.instruction("b __rt_array_set_str_grow_check");                     // keep growing until the target offset fits within capacity

    emitter.label("__rt_array_set_str_store");
    emitter.instruction("ldr x9, [x0]");                                        // reload logical length before deciding whether an old slot must be released
    emitter.instruction("cmp x1, x9");                                          // does the target index overwrite an existing string slot?
    emitter.instruction("b.hs __rt_array_set_str_skip_release");                // writes beyond current length do not own an old slot
    emitter.instruction("lsl x10, x1, #4");                                     // scale the target index to the 16-byte string slot width
    emitter.instruction("add x10, x0, x10");                                    // offset from the indexed-array base toward the target string slot
    emitter.instruction("add x10, x10, #24");                                   // skip the fixed indexed-array header
    emitter.instruction("ldr x0, [x10]");                                       // load the previous string pointer before overwriting the slot
    emitter.instruction("bl __rt_heap_free_safe");                              // release the overwritten owned string when it lives in the heap
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the indexed-array pointer after old-slot release
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore the target index after old-slot release
    emitter.label("__rt_array_set_str_skip_release");
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the owned string pointer that should be stored
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload the owned string length that should be stored
    emitter.instruction("lsl x10, x1, #4");                                     // scale the target index to the 16-byte string slot width
    emitter.instruction("add x10, x0, x10");                                    // offset from the indexed-array base toward the destination slot
    emitter.instruction("add x10, x10, #24");                                   // skip the fixed indexed-array header
    emitter.instruction("str x2, [x10]");                                       // store the owned string pointer into the destination slot
    emitter.instruction("str x3, [x10, #8]");                                   // store the owned string length into the destination slot
    emitter.instruction("ldr x9, [x0]");                                        // reload logical length to decide whether this write extends the array
    emitter.instruction("cmp x1, x9");                                          // does the target index overwrite an existing slot?
    emitter.instruction("b.lo __rt_array_set_str_done");                        // keep the current logical length for in-bounds overwrites
    emitter.instruction("mov x11, x9");                                         // start zero-filling gaps at the previous logical length
    emitter.label("__rt_array_set_str_fill_loop");
    emitter.instruction("cmp x11, x1");                                         // have all slots before the target index been initialized?
    emitter.instruction("b.ge __rt_array_set_str_store_len");                   // stop gap filling before touching the target slot
    emitter.instruction("lsl x10, x11, #4");                                    // scale the gap index to the 16-byte string slot width
    emitter.instruction("add x10, x0, x10");                                    // offset from the indexed-array base toward the gap slot
    emitter.instruction("add x10, x10, #24");                                   // skip the fixed indexed-array header
    emitter.instruction("str xzr, [x10]");                                      // initialize the gap string pointer to null
    emitter.instruction("str xzr, [x10, #8]");                                  // initialize the gap string length to zero
    emitter.instruction("add x11, x11, #1");                                    // advance to the next gap slot
    emitter.instruction("b __rt_array_set_str_fill_loop");                      // continue zero-filling until the target slot is reached
    emitter.label("__rt_array_set_str_store_len");
    emitter.instruction("add x11, x1, #1");                                     // compute the new logical length as target index plus one
    emitter.instruction("str x11, [x0]");                                       // publish the extended indexed-array length
    emitter.instruction("b __rt_array_set_str_done");                          // finish after an in-place indexed write

    // -- indexed destination + negative/sparse int key: promote to hash then insert --
    emitter.label("__rt_array_set_str_promote");
    emitter.instruction("bl __rt_array_to_hash");                               // build an owned hash preserving order and the existing value_type
    emitter.instruction("str x0, [sp, #32]");                                   // save the promoted hash pointer
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the now-superseded unique indexed array
    emitter.instruction("bl __rt_decref_array");                                // release it once its entries are retained by the promoted hash
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the promoted hash as the insert target
    emitter.instruction("b __rt_array_set_str_hash_insert");

    // -- destination already promoted: insert directly (hash_set handles COW) --
    emitter.label("__rt_array_set_str_already_hash");

    emitter.label("__rt_array_set_str_hash_insert");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the target integer key
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks a scalar integer hash key
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the owned string pointer as the hash value low word
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the owned string length as the hash value high word
    emitter.instruction("mov x5, #1");                                         // runtime value tag 1 = string payload
    emitter.instruction("bl __rt_hash_set");                                    // insert the entry, growing/COW-splitting the hash as needed
    emitter.label("__rt_array_set_str_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release string set helper spill space
    emitter.instruction("ret");                                                 // return with x0 holding the current array/hash pointer
}

/// Emits the Linux x86_64 string indexed-array set helper.
fn emit_array_set_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_set_str ---");
    emitter.label_global("__rt_array_set_str");

    // Frame layout: [rbp-8]=array/hash ptr, [rbp-16]=index, [rbp-24]=string ptr, [rbp-32]=string
    // length, [rbp-40]=promoted-hash scratch.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving helper spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for array, index, and string payload spills
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for array, index, string payload, and promotion scratch
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the incoming array/hash pointer for the kind dispatch
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the target index across copy-on-write and growth helpers
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the incoming string pointer across copy-on-write splitting
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // preserve the incoming string length across copy-on-write splitting

    // -- persist the incoming string BEFORE the kind dispatch: both the indexed and the
    //    already-hash insert paths need an owned (not transient) string payload --
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // move the incoming string pointer into the persist helper argument register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // move the incoming string length into the persist helper argument register
    emitter.instruction("call __rt_str_persist");                               // duplicate transient strings into owned heap storage for the array slot
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the owned string pointer after persistence
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // preserve the owned string length after persistence

    // -- dispatch on the destination's CURRENT runtime storage kind --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the incoming array/hash pointer after the persist call
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the packed heap-kind/value_type word
    emitter.instruction("and r10, 0xff");                                       // isolate the low byte holding the storage kind
    emitter.instruction("cmp r10, 3");                                          // kind 3 = an earlier write already promoted this destination to a hash
    emitter.instruction("je __rt_array_set_str_already_hash");                  // insert directly; __rt_hash_set performs its own COW split

    // -- indexed destination: split shared storage before mutating --
    emitter.instruction("call __rt_array_ensure_unique");                       // split shared indexed arrays before mutating payload storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the unique indexed-array pointer across persistence and growth

    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load logical length before first-write string layout normalization
    emitter.instruction("test r10, r10");                                       // is this the first write into the indexed array?
    emitter.instruction("jnz __rt_array_set_str_shape_ready");                  // non-empty indexed arrays already have a stable element layout
    super::array_push_str::emit_empty_string_capacity(emitter);
    emitter.instruction("mov QWORD PTR [rax + 16], 16");                        // string indexed arrays use pointer-plus-length payload slots
    emitter.instruction("mov r11, QWORD PTR [rax - 8]");                        // load packed indexed-array metadata from the heap header
    emitter.instruction("mov r8, 0xffffffff000080ff");                          // preserve heap marker, indexed-array kind, and copy-on-write metadata
    emitter.instruction("and r11, r8");                                         // clear stale value_type metadata on first string write
    emitter.instruction("or r11, 0x100");                                       // add runtime value_type tag 1 for string payload slots
    emitter.instruction("mov QWORD PTR [rax - 8], r11");                        // persist string-oriented indexed-array metadata
    emitter.label("__rt_array_set_str_shape_ready");

    // -- promotion check: PHP never pads a gap, it converts to a hash --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the current indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the target index after helper calls may have clobbered registers
    emitter.instruction("cmp rsi, 0");                                          // negative indexes cannot live in packed indexed storage
    emitter.instruction("jl __rt_array_set_str_promote");                       // promote to a hash so a negative key survives like PHP
    emitter.instruction("cmp rsi, QWORD PTR [rax]");                            // a key past the end would create a sparse gap
    emitter.instruction("ja __rt_array_set_str_promote");                       // promote to a hash so a sparse key survives like PHP

    emitter.label("__rt_array_set_str_grow_check");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the current indexed-array pointer before checking capacity
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the target index after helper calls may have clobbered registers
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the current indexed-array capacity
    emitter.instruction("cmp rsi, r10");                                        // does the target offset fit in the current allocation?
    emitter.instruction("jb __rt_array_set_str_store");                         // write directly once the slot is addressable
    emitter.instruction("mov rdi, rax");                                        // pass the current indexed-array pointer to the growth helper
    emitter.instruction("call __rt_array_grow");                                // grow the indexed array so the target slot can be materialized
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the possibly reallocated indexed-array pointer
    emitter.instruction("jmp __rt_array_set_str_grow_check");                   // keep growing until the target offset fits within capacity

    emitter.label("__rt_array_set_str_store");
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload logical length before deciding whether an old slot must be released
    emitter.instruction("cmp rsi, r9");                                         // does the target index overwrite an existing string slot?
    emitter.instruction("jae __rt_array_set_str_skip_release");                 // writes beyond current length do not own an old slot
    emitter.instruction("mov r10, rsi");                                        // copy the target index before scaling it to a string-slot offset
    emitter.instruction("shl r10, 4");                                          // scale the target index to the 16-byte string slot width
    emitter.instruction("lea r10, [rax + r10 + 24]");                           // compute the address of the overwritten string slot
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the previous string pointer before overwriting the slot
    emitter.instruction("call __rt_heap_free_safe");                            // release the overwritten owned string when it lives in the heap
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the indexed-array pointer after old-slot release
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // restore the target index after old-slot release
    emitter.label("__rt_array_set_str_skip_release");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the owned string pointer that should be stored
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the owned string length that should be stored
    emitter.instruction("mov r10, rsi");                                        // copy the target index before scaling it to a string-slot offset
    emitter.instruction("shl r10, 4");                                          // scale the target index to the 16-byte string slot width
    emitter.instruction("lea r10, [rax + r10 + 24]");                           // compute the address of the destination string slot
    emitter.instruction("mov QWORD PTR [r10], rdx");                           // store the owned string pointer into the destination slot
    emitter.instruction("mov QWORD PTR [r10 + 8], rcx");                       // store the owned string length into the destination slot
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // reload logical length to decide whether this write extends the array
    emitter.instruction("cmp rsi, r9");                                         // does the target index overwrite an existing slot?
    emitter.instruction("jb __rt_array_set_str_done");                          // keep the current logical length for in-bounds overwrites
    emitter.instruction("mov r11, r9");                                         // start zero-filling gaps at the previous logical length
    emitter.label("__rt_array_set_str_fill_loop");
    emitter.instruction("cmp r11, rsi");                                        // have all slots before the target index been initialized?
    emitter.instruction("jae __rt_array_set_str_store_len");                    // stop gap filling before touching the target slot
    emitter.instruction("mov r10, r11");                                        // copy the gap index before scaling it to a string-slot offset
    emitter.instruction("shl r10, 4");                                          // scale the gap index to the 16-byte string slot width
    emitter.instruction("lea r10, [rax + r10 + 24]");                           // compute the address of the gap string slot
    emitter.instruction("mov QWORD PTR [r10], 0");                             // initialize the gap string pointer to null
    emitter.instruction("mov QWORD PTR [r10 + 8], 0");                         // initialize the gap string length to zero
    emitter.instruction("add r11, 1");                                          // advance to the next gap slot
    emitter.instruction("jmp __rt_array_set_str_fill_loop");                    // continue zero-filling until the target slot is reached
    emitter.label("__rt_array_set_str_store_len");
    emitter.instruction("lea r11, [rsi + 1]");                                  // compute the new logical length as target index plus one
    emitter.instruction("mov QWORD PTR [rax], r11");                            // publish the extended indexed-array length
    emitter.instruction("jmp __rt_array_set_str_done");                         // finish after an in-place indexed write

    // -- indexed destination + negative/sparse int key: promote to hash then insert --
    emitter.label("__rt_array_set_str_promote");
    emitter.instruction("call __rt_array_to_hash");                            // build an owned hash preserving order and the existing value_type
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                      // save the promoted hash pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                       // reload the now-superseded unique indexed array
    emitter.instruction("call __rt_decref_array");                            // release it once its entries are retained by the promoted hash
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                      // reload the promoted hash as the insert target
    emitter.instruction("jmp __rt_array_set_str_hash_insert");

    // -- destination already promoted: insert directly (hash_set handles COW) --
    emitter.label("__rt_array_set_str_already_hash");
    emitter.instruction("mov rax, rdi");                                       // the incoming pointer is already the hash insert target

    emitter.label("__rt_array_set_str_hash_insert");
    emitter.instruction("mov rdi, rax");                                       // pass the array/hash pointer as the hash_set target
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                      // reload the target integer key
    emitter.instruction("mov rdx, -1");                                        // key_hi sentinel marks a scalar integer hash key
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                      // reload the owned string pointer as the hash value low word
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                       // reload the owned string length as the hash value high word
    emitter.instruction("mov r9, 1");                                          // runtime value tag 1 = string payload
    emitter.instruction("call __rt_hash_set");                                 // insert the entry, growing/COW-splitting the hash as needed
    emitter.label("__rt_array_set_str_done");
    emitter.instruction("add rsp, 48");                                        // release string set helper spill space
    emitter.instruction("pop rbp");                                            // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                // return with rax holding the current array/hash pointer
}
