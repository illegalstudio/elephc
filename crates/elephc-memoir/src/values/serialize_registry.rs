//! Purpose:
//! Stores the serializer's bounded set of borrowed native object identities.
//!
//! Called from:
//! - Serializer traversal and nested serialize context emission.
//!
//! Key details:
//! - The serializer supplies PHP indices and decides what to output.
//! - The occupied prefix is visible only after both words have been written.

use crate::{Arch, Emitter, abi::emit_symbol_address, data::{DataTarget, StaticStorage}};

const CAPACITY: usize = 65_536;
/// Native symbol for the number of registered object identities.
pub const COUNT: &str = "_ser_obj_count";
/// Storage for the borrowed object addresses.
pub const POINTERS: StaticStorage = StaticStorage { symbol: "_ser_obj_ptrs", bytes: CAPACITY * 8 };
/// Storage for the corresponding serialization indices.
pub const INDICES: StaticStorage = StaticStorage { symbol: "_ser_obj_idxs", bytes: CAPACITY * 8 };

/// Reserves the count and both arrays in the fixed runtime data.
pub fn append_data(out: &mut String, target: &impl DataTarget) {
    out.push_str(&target.common(COUNT, 8));
    POINTERS.append(out, target);
    INDICES.append(out, target);
}

/// Clears the occupied prefix for a new serialization context.
pub fn emit_reset(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emit_symbol_address(emitter, "x9", COUNT);
            emitter.instruction("str xzr, [x9]");                               // reset the occupied count
        }
        Arch::X86_64 => {
            emit_symbol_address(emitter, "r9", COUNT);
            emitter.instruction("mov QWORD PTR [r9], 0");                       // reset the occupied count
        }
    }
}

/// Finds a previously published native object on aarch64, leaving the index in the result.
pub fn emit_lookup_aarch64(emitter: &mut impl Emitter) {
    emit_symbol_address(emitter, "x9", COUNT);
    emitter.instruction("ldr x9, [x9]");                                        // number of registered objects
    emitter.instruction("mov x10, #0");                                         // scan index = 0
    emitter.label("__rt_serialize_obj_ref_scan");
    emitter.instruction("cmp x10, x9");                                         // scanned every registered object?
    emitter.instruction("b.ge __rt_serialize_obj_ref_new");                     // not seen → register a new index
    emit_symbol_address(emitter, "x11", POINTERS.symbol);
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // registered object pointer
    emitter.instruction("ldr x13, [sp, #0]");                                   // this object pointer
    emitter.instruction("cmp x12, x13");                                        // same identity?
    emitter.instruction("b.eq __rt_serialize_obj_ref_found");                   // already serialized → emit a back-reference
    emitter.instruction("add x10, x10, #1");                                    // next registered object
    emitter.instruction("b __rt_serialize_obj_ref_scan");                       // continue scanning
    emitter.label("__rt_serialize_obj_ref_found");
    emit_symbol_address(emitter, "x11", INDICES.symbol);
    emitter.instruction("ldr x0, [x11, x10, lsl #3]");                          // the recorded reference index
}

/// Publishes the selected PHP index under the prepared object identity on aarch64.
pub fn emit_publish_aarch64(emitter: &mut impl Emitter) {
    emit_symbol_address(emitter, "x11", COUNT);
    emitter.instruction("ldr x12, [x11]");                                      // current registered-object count
    emitter.instruction(&format!("mov x13, #{CAPACITY}"));                      // object-map capacity
    emitter.instruction("cmp x12, x13");                                        // is the map full?
    emitter.instruction("b.ge __rt_serialize_obj_ref_done");                    // full → keep counting but stop deduping
    emit_symbol_address(emitter, "x13", POINTERS.symbol);
    emitter.instruction("ldr x14, [sp, #0]");                                   // object pointer
    emitter.instruction("str x14, [x13, x12, lsl #3]");                         // record the object pointer
    emit_symbol_address(emitter, "x13", INDICES.symbol);
    emitter.instruction("str x10, [x13, x12, lsl #3]");                         // record its reference index
    emitter.instruction("add x12, x12, #1");                                    // grow the map
    emitter.instruction("str x12, [x11]");                                      // publish the new count
    emitter.label("__rt_serialize_obj_ref_done");
}

/// Finds a previously published native object on x86_64, leaving the index in the result.
pub fn emit_lookup_x86_64(emitter: &mut impl Emitter) {
    emit_symbol_address(emitter, "r9", COUNT);
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // number of registered objects
    emitter.instruction("xor r10, r10");                                        // scan index = 0
    emitter.label("__rt_serialize_obj_ref_scan");
    emitter.instruction("cmp r10, r9");                                         // scanned every registered object?
    emitter.instruction("jge __rt_serialize_obj_ref_new");                      // not seen → register a new index
    emit_symbol_address(emitter, "r11", POINTERS.symbol);
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // registered object pointer
    emitter.instruction("cmp rax, QWORD PTR [rbp - 8]");                        // same identity?
    emitter.instruction("je __rt_serialize_obj_ref_found");                     // already serialized → emit a back-reference
    emitter.instruction("add r10, 1");                                          // next registered object
    emitter.instruction("jmp __rt_serialize_obj_ref_scan");                     // continue scanning
    emitter.label("__rt_serialize_obj_ref_found");
    emit_symbol_address(emitter, "r11", INDICES.symbol);
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // the recorded reference index
}

/// Publishes the selected PHP index under the prepared object identity on x86_64.
pub fn emit_publish_x86_64(emitter: &mut impl Emitter) {
    emit_symbol_address(emitter, "r9", COUNT);
    emitter.instruction("mov rcx, QWORD PTR [r9]");                             // current registered-object count
    emitter.instruction(&format!("cmp rcx, {CAPACITY}"));                       // is the object map full?
    emitter.instruction("jge __rt_serialize_obj_ref_done");                     // full → keep counting but stop deduping
    emit_symbol_address(emitter, "r11", POINTERS.symbol);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // object pointer
    emitter.instruction("mov QWORD PTR [r11 + rcx*8], rax");                    // record the object pointer
    emit_symbol_address(emitter, "r11", INDICES.symbol);
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // assigned index
    emitter.instruction("mov QWORD PTR [r11 + rcx*8], rax");                    // record its reference index
    emit_symbol_address(emitter, "r9", COUNT);
    emitter.instruction("add rcx, 1");                                          // grow the map
    emitter.instruction("mov QWORD PTR [r9], rcx");                             // publish the new count
    emitter.label("__rt_serialize_obj_ref_done");
}
