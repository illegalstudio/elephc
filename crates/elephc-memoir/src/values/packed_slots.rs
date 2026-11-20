//! Purpose:
//! Publishes initialized slots and transfers existing owners in packed native arrays.
//!
//! Called from:
//! - Preallocated values builders and indexed Mixed overwrite emitters.
//!
//! Key details:
//! - The array owner remains live while its slot is updated before retirement can reenter.

use crate::Emitter;

/// Appends a 16-byte string payload into the AArch64 result values array.
pub fn emit_append_string_value_aarch64(emitter: &mut impl Emitter, ptr_reg: &str, len_reg: &str) {
    emitter.instruction("ldr x9, [sp, #16]");                                   // load the result values array pointer from the fixed stack layout
    emitter.instruction("ldr x10, [x9]");                                       // load the current result values array length before appending
    emitter.instruction("lsl x11, x10, #4");                                    // convert the result length into a 16-byte string-slot offset
    emitter.instruction("add x11, x9, x11");                                    // advance from the array base to the selected string slot
    emitter.instruction("add x11, x11, #24");                                   // skip the fixed indexed-array header to reach the payload region
    emitter.instruction(&format!("str {}, [x11]", ptr_reg));                    // store the owned string pointer into the next result values slot
    emitter.instruction(&format!("str {}, [x11, #8]", len_reg));                // store the owned string length into the next result values slot
    emitter.instruction("add x10, x10, #1");                                    // increment the result values array length after the append
    emitter.instruction("str x10, [x9]");                                       // persist the updated result values array length in the header
}

/// Appends an 8-byte payload into the AArch64 result values array.
pub fn emit_append_word_value_aarch64(emitter: &mut impl Emitter, value_reg: &str) {
    emitter.instruction("ldr x9, [sp, #16]");                                   // load the result values array pointer from the fixed stack layout
    emitter.instruction("ldr x10, [x9]");                                       // load the current result values array length before appending
    emitter.instruction("add x11, x9, #24");                                    // point at the result values array payload region after the fixed header
    emitter.instruction(&format!("str {}, [x11, x10, lsl #3]", value_reg));     // store the value payload into the next result values slot
    emitter.instruction("add x10, x10, #1");                                    // increment the result values array length after the append
    emitter.instruction("str x10, [x9]");                                       // persist the updated result values array length in the header
}

/// Appends a 16-byte string payload into the x86_64 result values array.
pub fn emit_append_string_value_x86_64(emitter: &mut impl Emitter, ptr_reg: &str, len_reg: &str) {
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // load the result values array pointer from the fixed stack layout
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current result values array length before appending
    emitter.instruction("mov rcx, r11");                                        // copy the result length before scaling it into a string-slot offset
    emitter.instruction("shl rcx, 4");                                          // convert the result length into a 16-byte string-slot offset
    emitter.instruction("add rcx, r10");                                        // advance from the array base to the selected string slot
    emitter.instruction("add rcx, 24");                                         // skip the fixed indexed-array header to reach the payload region
    emitter.instruction(&format!("mov QWORD PTR [rcx], {}", ptr_reg));          // store the owned string pointer into the next result values slot
    emitter.instruction(&format!("mov QWORD PTR [rcx + 8], {}", len_reg));      // store the owned string length into the next result values slot
    emitter.instruction("add r11, 1");                                          // increment the result values array length after the append
    emitter.instruction("mov QWORD PTR [r10], r11");                            // persist the updated result values array length in the header
}

/// Appends an 8-byte payload into the x86_64 result values array.
pub fn emit_append_word_value_x86_64(emitter: &mut impl Emitter, value_reg: &str) {
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // load the result values array pointer from the fixed stack layout
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current result values array length before appending
    emitter.instruction(                                                        // store the value payload into the next result values slot
        &format!("mov QWORD PTR [r10 + r11 * 8 + 24], {}", value_reg)
    );
    emitter.instruction("add r11, 1");                                          // increment the result values array length after the append
    emitter.instruction("mov QWORD PTR [r10], r11");                            // persist the updated result values array length in the header
}

/// Publishes an overwritten Mixed array slot and retires its displaced owner.
pub fn emit_publish_mixed_overwrite_aarch64(emitter: &mut impl Emitter) {
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the attached array before any destructor can reenter
    emitter.instruction("ldr x9, [sp, #40]");                                   // recover the index after marker inspection
    emitter.instruction("ldr x11, [sp, #24]");                                  // transfer the consumed replacement into the attached slot
    emitter.instruction("add x12, x10, #24");                                   // locate the existing slot while its array remains valid
    emitter.instruction("str x11, [x12, x9, lsl #3]");                          // publish before retiring the previous owner's final reference
    crate::cleanup_preserve::emit_unary_cleanup_preserving_exception(emitter, "__rt_decref_mixed", "x0");
}

/// Publishes an overwritten Mixed array slot and retires its displaced owner.
pub fn emit_publish_mixed_overwrite_x86_64(emitter: &mut impl Emitter) {
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the attached array before any destructor can reenter
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // recover the index after marker inspection
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // transfer the consumed replacement into the attached slot
    emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], r11");              // publish before retiring the previous owner's final reference
    crate::cleanup_preserve::emit_unary_cleanup_preserving_exception(emitter, "__rt_decref_mixed", "rax");
}
