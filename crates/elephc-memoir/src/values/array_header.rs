//! Purpose:
//! Writes and copies the packed element tag in an indexed-array header.
//!
//! Called from:
//! - Compiler array emission and native array storage conversions.
//!
//! Key details:
//! - Both operations preserve the storage kind, COW flag and x86_64 heap magic.

use crate::{abi, Arch, Emitter, ValueTag};

/// Stamps a caller-selected element tag without changing the array's ownership metadata.
pub fn emit_array_value_type_stamp(emitter: &mut impl Emitter, array_reg: &str, value_type_tag: ValueTag) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x10, [{}, #-8]", array_reg));     // load the packed array kind word from the heap header
            emitter.instruction("mov x12, #0x80ff");                            // preserve the indexed-array kind and persistent COW flag
            emitter.instruction("and x10, x10, x12");                           // keep only the persistent indexed-array metadata bits
            emitter.instruction(&format!("mov x11, #{}", value_type_tag));      // materialize the runtime array value_type tag
            emitter.instruction("lsl x11, x11, #8");                            // move the value_type tag into the packed kind-word byte lane
            emitter.instruction("orr x10, x10, x11");                           // combine the heap kind with the array value_type tag
            emitter.instruction(&format!("str x10, [{}, #-8]", array_reg));     // persist the packed array kind word in the heap header
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "r12");                                 // preserve the x86_64 nested-call scratch register before reusing it as a temporary array-stamp helper
            emit_value_type_stamp_x86_64(emitter, array_reg, value_type_tag, "r12");
            abi::emit_pop_reg(emitter, "r12");                                  // restore the x86_64 nested-call scratch register after the array value-type stamp is complete
        }
    }
}

/// Stamps the packed header using the caller's available metadata scratch register.
fn emit_value_type_stamp_x86_64(
    emitter: &mut impl Emitter, array_reg: &str, value_type_tag: ValueTag, scratch: &str,
) {
    emitter.instruction(&format!("mov r10, QWORD PTR [{array_reg} - 8]"));      // load the packed header
    emitter.instruction(&format!("mov {scratch}, 0xffffffff000080ff"));         // preserve the magic, kind and COW flag
    emitter.instruction(&format!("and r10, {scratch}"));                        // clear the stale tag lane
    emitter.instruction(&format!("mov {scratch}, {value_type_tag}"));           // stage the new value tag
    emitter.instruction(&format!("shl {scratch}, 8"));                          // place the tag in the correct byte lane
    emitter.instruction(&format!("or r10, {scratch}"));                         // combine the tag and existing metadata
    emitter.instruction(&format!("mov QWORD PTR [{array_reg} - 8], r10"));      // publish the new packed header
}

/// Allocates an owned array of Mixed slots with its count already in the first C argument register.
pub fn emit_allocate_mixed_slots(emitter: &mut impl Emitter) {
    let mixed_tag = ValueTag::new(7);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("mov x1, #8");                                  // allocate one pointer word per boxed argument
            abi::emit_call_label(emitter, "__rt_array_new");
            emit_array_value_type_stamp(emitter, "x0", mixed_tag);
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, 8");                                  // allocate one pointer word per boxed argument
            abi::emit_call_label(emitter, "__rt_array_new");
            emit_value_type_stamp_x86_64(emitter, "rax", mixed_tag, "r11");
        }
    }
}

/// Copies one array's runtime `value_type` tag into another array's packed kind word.
///
/// The source slot is a native memory operand holding the source array pointer.
pub fn emit_array_value_type_inherit(
    emitter: &mut impl Emitter,
    dest_reg: &str,
    source_slot: &str,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, {}", source_slot));           // reload the source array pointer
            emitter.instruction("ldr x9, [x9, #-8]");                           // load the source packed heap-kind word
            emitter.instruction("lsr x9, x9, #8");                              // shift the source value_type into the low bits
            emitter.instruction("and x9, x9, #0x7f");                           // isolate the value_type, dropping the COW bit
            emitter.instruction("lsl x9, x9, #8");                              // move it back into the packed byte lane
            emitter.instruction(&format!("ldr x10, [{}, #-8]", dest_reg));      // load the destination packed heap-kind word
            emitter.instruction("mov x11, #0x80ff");                            // preserve the indexed-array kind and persistent COW flag
            emitter.instruction("and x10, x10, x11");                           // keep only the destination's own metadata bits
            emitter.instruction("orr x10, x10, x9");                            // combine them with the inherited value_type
            emitter.instruction(&format!("str x10, [{}, #-8]", dest_reg));      // persist the destination packed kind word
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "r12");                                 // preserve the nested-call scratch register before reusing it
            emitter.instruction(&format!("mov r10, {}", source_slot));          // reload the source array pointer
            emitter.instruction("mov r10, QWORD PTR [r10 - 8]");                // load the source packed heap-kind word
            emitter.instruction("shr r10, 8");                                  // shift the source value_type into the low bits
            emitter.instruction("and r10, 127");                                // isolate the value_type, dropping the COW bit
            emitter.instruction("shl r10, 8");                                  // move it back into the packed byte lane
            emitter.instruction(&format!("mov r11, QWORD PTR [{} - 8]", dest_reg)); // load the destination packed heap-kind word
            emitter.instruction("mov r12, 0xffffffff000080ff");                 // preserve the heap magic marker, kind and persistent COW flag
            emitter.instruction("and r11, r12");                                // keep only the destination's own metadata bits
            emitter.instruction("or r11, r10");                                 // combine them with the inherited value_type
            emitter.instruction(&format!("mov QWORD PTR [{} - 8], r11", dest_reg)); // persist the destination packed kind word
            abi::emit_pop_reg(emitter, "r12");                                  // restore the nested-call scratch register
        }
    }
}

/// Stamps a freshly returned packed array, using only caller-saved registers.
pub fn emit_result_value_type_stamp(emitter: &mut impl Emitter, tag: ValueTag) {
    match emitter.arch() {
        Arch::AArch64 => emit_array_value_type_stamp(emitter, "x0", tag),
        Arch::X86_64 => emit_value_type_stamp_x86_64(emitter, "rax", tag, "r11"),
    }
}

/// Decodes the packed element tag from an AArch64 metadata word without the COW bit.
pub fn emit_extract_value_tag_aarch64(emitter: &mut impl Emitter, destination: &str, metadata: &str) {
    emitter.instruction(&format!("ubfx {destination}, {metadata}, #8, #7"));    // keep only the seven indexed-element tag bits
}

/// Loads the byte stride from the native packed array's descriptor.
pub fn emit_load_element_stride(emitter: &mut impl Emitter, array: &str, stride: &str) {
    emitter.emit_load_from_address(stride, array, 16);
}

/// Decodes the x86-64 packed element tag from its shifted metadata word.
/// Both register names refer to the same register, at the required operand widths.
pub fn emit_extract_value_tag_x86_64(emitter: &mut impl Emitter, metadata: &str, tag: &str) {
    emitter.instruction(&format!("shr {metadata}, 8"));                        // position the packed tag in the low byte
    emitter.instruction(&format!("and {tag}, 0x7f"));                           // discard ownership flags and heap magic
}
