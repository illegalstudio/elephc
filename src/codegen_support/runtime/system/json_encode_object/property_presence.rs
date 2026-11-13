//! Purpose:
//! Omits uninitialized typed properties before the JSON encoder writes their keys.
//!
//! Called from:
//! - The native JSON object property walker on AArch64 and x86_64.
//!
//! Key details:
//! - Ordinary slots keep their marker in the high word; owned reference cells use the low word.
//! - Skipping an entry advances the descriptor index without adding an output separator.

use super::*;
use crate::codegen_support::sentinels::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;

/// Branches over one absent declared property while retaining the walker frame layout.
pub(super) fn emit_skip_uninitialized(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x17, [sp, #56]");                          // inspect the descriptor before writing a key
            emitter.instruction(&format!("tbz x17, #{}, __rt_json_obj_property_present", JSON_DECLARED_PROPERTY_FLAG.trailing_zeros())); // untyped slots retain their ordinary value representation
            emitter.instruction("ldr x9, [sp]");                                // recover the object whose property is being encoded
            emitter.instruction("ldr x10, [sp, #48]");                          // recover the physical property slot index
            emitter.instruction("add x9, x9, x10, lsl #4");                     // advance over the preceding two-word property slots
            emitter.instruction("add x9, x9, #8");                              // skip the object's class identifier
            emitter.instruction(&format!("tbz x17, #{}, __rt_json_obj_direct_marker", JSON_REFERENCE_PROPERTY_FLAG.trailing_zeros())); // direct properties keep initialization in their high word
            emitter.instruction("ldr x9, [x9]");                                // follow the managed reference-property cell
            emitter.instruction("ldr x10, [x9]");                               // inspect the reference cell's initialization marker
            emitter.instruction("b __rt_json_obj_check_marker");                // share the sentinel comparison
            emitter.label("__rt_json_obj_direct_marker");
            emitter.instruction("ldr x10, [x9, #8]");                           // inspect an ordinary property's high-word marker
            emitter.label("__rt_json_obj_check_marker");
            abi::emit_load_int_immediate(emitter, "x11", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
            emitter.instruction("cmp x10, x11");                                // distinguish an uninitialized property from any initialized value
            emitter.instruction("b.eq __rt_json_obj_next_property");            // omit the property without emitting a comma or a key
            emitter.label("__rt_json_obj_property_present");
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                // inspect the descriptor before writing a key
            emitter.instruction(&format!("test r9, {JSON_DECLARED_PROPERTY_FLAG}")); // untyped slots retain their ordinary value representation
            emitter.instruction("jz __rt_json_obj_property_present_x");         // only declared slots carry a typed initialization marker
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                // recover the object whose property is being encoded
            emitter.instruction("mov r11, QWORD PTR [rbp - 56]");               // recover the physical property slot index
            emitter.instruction("shl r11, 4");                                  // advance over the preceding two-word property slots
            emitter.instruction("lea r10, [r10 + r11 + 8]");                    // skip the object's class identifier
            emitter.instruction(&format!("test r9, {JSON_REFERENCE_PROPERTY_FLAG}")); // reference slots store their marker inside the managed cell
            emitter.instruction("jz __rt_json_obj_direct_marker_x");            // ordinary slots keep initialization in their high word
            emitter.instruction("mov r10, QWORD PTR [r10]");                    // follow the managed reference-property cell
            emitter.instruction("mov rax, QWORD PTR [r10]");                    // inspect the reference cell's initialization marker
            emitter.instruction("jmp __rt_json_obj_check_marker_x");            // share the sentinel comparison
            emitter.label("__rt_json_obj_direct_marker_x");
            emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                // inspect an ordinary property's high-word marker
            emitter.label("__rt_json_obj_check_marker_x");
            abi::emit_load_int_immediate(emitter, "rcx", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
            emitter.instruction("cmp rax, rcx");                                // distinguish an uninitialized property from any initialized value
            emitter.instruction("je __rt_json_obj_next_property_x");            // omit the property without emitting a comma or a key
            emitter.label("__rt_json_obj_property_present_x");
        }
    }
}
