//! Purpose:
//! Exposes borrowed indexed and associative slots without sharing native allocation offsets.
//!
//! Called from:
//! - The mbstring native variable and packed argument bridge.
//!
//! Key details:
//! - Queries return success, end, or invalid without retaining or releasing owners.

use crate::{Arch, Emitter};

/// Emits the count and slot queries used when traversing a live native array.
pub(super) fn emit(emitter: &mut impl Emitter) {
    emitter.label_global(&emitter.extern_symbol("elephc_mbstring_variable_array_count_v1"));
    emitter.emit_load_from_address(emitter.int_result_reg(), emitter.int_arg_reg_name(0), 0);
    emitter.emit_return();
    emit_slot(emitter, false);
    emit_slot(emitter, true);
}

/// Emits a slot query with a three-word output record: payload address, tag, and flags.
fn emit_slot(emitter: &mut impl Emitter, hash: bool) {
    let kind = if hash { "hash" } else { "indexed" };
    let name = format!("elephc_mbstring_variable_{kind}_slot_v1");
    let end = format!("__rt_mbstring_variable_{kind}_end");
    let bad = format!("__rt_mbstring_variable_{kind}_bad");
    emitter.label_global(&emitter.extern_symbol(&name));
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0]");                                // read the logical entry count
            emitter.instruction("cmp x1, x9");                                  // test the requested ordinal against that count
            emitter.instruction(&format!("b.hs {end}"));                        // an exhausted cursor is not an invalid array
            if hash { emit_hash_aarch64(emitter, &bad); }
            else { emit_indexed_aarch64(emitter, &bad); }
            emitter.instruction("stp x9, x10, [x2]");                           // return the borrowed address and tag
            emitter.instruction("str x11, [x2, #16]");                          // return the slot flags
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rsi, QWORD PTR [rdi]");                    // test the requested ordinal against the logical count
            emitter.instruction(&format!("jae {end}"));                         // an exhausted cursor is not an invalid array
            if hash { emit_hash_x86_64(emitter, &bad); }
            else { emit_indexed_x86_64(emitter, &bad); }
            emitter.instruction("mov QWORD PTR [rdx], r8");                     // return the borrowed slot address
            emitter.instruction("mov QWORD PTR [rdx + 8], r9");                 // return the value tag
            emitter.instruction("mov QWORD PTR [rdx + 16], r10");               // return the slot flags
        }
    }
    let result = emitter.int_result_reg();
    emitter.emit_load_int_immediate(result, 0);
    emitter.emit_return();
    emitter.label(&end);
    emitter.emit_load_int_immediate(result, 1);
    emitter.emit_return();
    emitter.label(&bad);
    emitter.emit_load_int_immediate(result, 2);
    emitter.emit_return();
}

/// Locates a packed slot using the element tag and checked stride on AArch64.
fn emit_indexed_aarch64(emitter: &mut impl Emitter, bad: &str) {
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load indexed allocation metadata
    emitter.instruction("ubfx x10, x10, #8, #7");                               // decode the packed element tag
    emitter.instruction("cmp x10, #11");                                        // tagged scalar slots carry a per-slot tag
    emitter.instruction("cset x11, eq");                                        // flag a tagged scalar as read-only to this bridge
    emitter.instruction("cmp x10, #1");                                         // strings also use two words
    emitter.instruction("cset x12, eq");                                        // detect a string slot without clobbering its tag
    emitter.instruction("orr x12, x12, x11");                                   // combine the two wide slot shapes
    emitter.instruction("add x12, x12, #1");                                    // convert the shape into the number of words
    emitter.instruction("lsl x12, x12, #3");                                    // convert the word count to a byte stride
    emitter.instruction("ldr x13, [x0, #16]");                                  // read the stored physical stride
    emitter.instruction("cmp x12, x13");                                        // require a stride consistent with the value tag
    emitter.instruction(&format!("b.ne {bad}"));                                // reject inconsistent native storage
    emitter.instruction("umulh x13, x1, x12");                                  // check the high half of the byte offset
    emitter.instruction(&format!("cbnz x13, {bad}"));                           // reject an overflowing slot offset
    emitter.instruction("mul x9, x1, x12");                                     // compute the slot's byte offset in the payload
    emitter.instruction("adds x9, x9, #24");                                    // advance past the indexed-array header
    emitter.instruction(&format!("b.cs {bad}"));                                // reject a wrapped header-adjusted offset
    emitter.instruction("add x9, x0, x9");                                      // form the borrowed payload address
    emitter.instruction("cbz x11, __rt_mbstring_variable_indexed_tag");         // homogeneous values keep the header tag
    emitter.instruction("ldr x10, [x9, #8]");                                   // tagged scalars carry their own tag word
    emitter.label("__rt_mbstring_variable_indexed_tag");
}

/// Locates a packed slot using the element tag and checked stride on x86_64.
fn emit_indexed_x86_64(emitter: &mut impl Emitter, bad: &str) {
    emitter.instruction("mov r11, rdx");                                        // keep the output record across wide multiplication
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // load indexed allocation metadata
    emitter.instruction("shr r9, 8");                                           // position the packed element tag
    emitter.instruction("and r9d, 127");                                        // isolate the element tag
    emitter.instruction("xor r10d, r10d");                                      // default to a writable homogeneous slot
    emitter.instruction("cmp r9, 11");                                          // tagged scalars carry a per-slot tag
    emitter.instruction("sete r10b");                                           // flag a tagged scalar as read-only to this bridge
    emitter.instruction("xor ecx, ecx");                                        // default to a one-word stride
    emitter.instruction("cmp r9, 1");                                           // strings also use two words
    emitter.instruction("sete cl");                                             // detect string slots without clobbering their tag
    emitter.instruction("or rcx, r10");                                         // combine the two wide slot shapes
    emitter.instruction("lea rcx, [8 + rcx * 8]");                              // convert the shape into a byte stride
    emitter.instruction("cmp rcx, QWORD PTR [rdi + 16]");                       // require a stride consistent with the value tag
    emitter.instruction(&format!("jne {bad}"));                                 // reject inconsistent native storage
    emitter.instruction("mov rax, rsi");                                        // multiply the ordinal by the byte stride
    emitter.instruction("mul rcx");                                             // compute the checked physical offset
    emitter.instruction(&format!("jc {bad}"));                                  // reject an overflowing slot offset
    emitter.instruction("add rax, 24");                                         // advance past the indexed-array header
    emitter.instruction(&format!("jc {bad}"));                                  // reject a wrapped header-adjusted offset
    emitter.instruction("lea r8, [rdi + rax]");                                 // form the borrowed payload address
    emitter.instruction("mov rdx, r11");                                        // restore the result record address
    emitter.instruction("test r10, r10");                                       // does the slot override the homogeneous tag?
    emitter.instruction("jz __rt_mbstring_variable_indexed_tag");               // homogeneous values keep the header tag
    emitter.instruction("mov r9, QWORD PTR [r8 + 8]");                          // tagged scalars carry their own tag word
    emitter.label("__rt_mbstring_variable_indexed_tag");
}

/// Walks live buckets in insertion order on AArch64.
fn emit_hash_aarch64(emitter: &mut impl Emitter, bad: &str) {
    let next = "__rt_mbstring_variable_hash_next";
    let found = "__rt_mbstring_variable_hash_found";
    emitter.instruction("ldr x12, [x0, #8]");                                   // load the maximum valid bucket index
    emitter.instruction("ldr x13, [x0, #40]");                                  // load the bucket allocation
    emitter.instruction(&format!("cbz x13, {bad}"));                            // reject a missing bucket allocation
    emitter.instruction("ldr x14, [x0, #24]");                                  // start at the first inserted bucket
    emitter.label(next);
    emitter.instruction("cmp x14, x12");                                        // check the list index before dereferencing it
    emitter.instruction(&format!("b.hs {bad}"));                                // reject an out-of-range link
    emitter.instruction("lsr x15, x14, #58");                                   // check whether scaling the index would overflow
    emitter.instruction(&format!("cbnz x15, {bad}"));                           // reject overflowing byte offsets
    emitter.instruction("add x15, x13, x14, lsl #6");                           // address the fixed-size bucket
    emitter.instruction("ldr x9, [x15]");                                       // inspect bucket occupancy
    emitter.instruction("cmp x9, #1");                                          // only occupied buckets may be in the live chain
    emitter.instruction(&format!("b.ne {bad}"));                                // reject a tombstone or empty slot
    emitter.instruction(&format!("cbz x1, {found}"));                           // select the requested live bucket
    emitter.instruction("sub x1, x1, #1");                                      // consume one step in insertion order
    emitter.instruction("ldr x14, [x15, #56]");                                 // advance through the next-link field
    emitter.emit_jump(next);
    emitter.label(found);
    emitter.instruction("add x9, x15, #24");                                    // return the bucket's payload slot
    emitter.instruction("ldr x10, [x15, #40]");                                 // return its per-entry value tag
    emitter.instruction("add x11, x15, #40");                                   // preserve the original tag-field identity
}

/// Walks live buckets in insertion order on x86_64.
fn emit_hash_x86_64(emitter: &mut impl Emitter, bad: &str) {
    let next = "__rt_mbstring_variable_hash_next";
    let found = "__rt_mbstring_variable_hash_found";
    emitter.instruction("mov r8, QWORD PTR [rdi + 8]");                         // load the maximum valid bucket index
    emitter.instruction("mov r9, QWORD PTR [rdi + 40]");                        // load the bucket allocation
    emitter.instruction("test r9, r9");                                         // is the bucket allocation present?
    emitter.instruction(&format!("jz {bad}"));                                  // reject a missing bucket allocation
    emitter.instruction("mov r10, QWORD PTR [rdi + 24]");                       // start at the first inserted bucket
    emitter.label(next);
    emitter.instruction("cmp r10, r8");                                         // check the list index before dereferencing it
    emitter.instruction(&format!("jae {bad}"));                                 // reject an out-of-range link
    emitter.instruction("mov r11, r10");                                        // preserve the index across overflow checking
    emitter.instruction("shr r11, 58");                                         // check whether scaling the index would overflow
    emitter.instruction(&format!("jnz {bad}"));                                 // reject overflowing byte offsets
    emitter.instruction("shl r10, 6");                                          // scale the index by the fixed bucket width
    emitter.instruction("lea r11, [r9 + r10]");                                 // address the bucket
    emitter.instruction("cmp QWORD PTR [r11], 1");                              // only occupied buckets may be in the live chain
    emitter.instruction(&format!("jne {bad}"));                                 // reject a tombstone or empty slot
    emitter.instruction("test rsi, rsi");                                       // have we reached the requested ordinal?
    emitter.instruction(&format!("jz {found}"));                                // select the requested live bucket
    emitter.instruction("dec rsi");                                             // consume one step in insertion order
    emitter.instruction("mov r10, QWORD PTR [r11 + 56]");                       // advance through the next-link field
    emitter.emit_jump(next);
    emitter.label(found);
    emitter.instruction("lea r8, [r11 + 24]");                                  // return the bucket's payload slot
    emitter.instruction("mov r9, QWORD PTR [r11 + 40]");                        // return its per-entry value tag
    emitter.instruction("lea r10, [r11 + 40]");                                 // preserve the original tag-field identity
}
