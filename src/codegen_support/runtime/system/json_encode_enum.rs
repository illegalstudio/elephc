//! Purpose:
//! Dispatches JSON enum encoding to custom serialization, backing scalars, or the unit error.
//!
//! Called from:
//! - `super::json_encode_object` before entering the ordinary object encoder frame.
//!
//! Key details:
//! - Class enum kinds, checked value offsets, and JSON descriptors serve every target.
//! - Partial output uses PHP's zero placeholder and takes precedence over throwing.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Encodes default enum values before entering the ordinary object recursion frame.
pub(super) fn emit_dispatch(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }

    // -- recognize enums before entering object recursion depth --
    emitter.instruction("ldr x12, [x0]");                                       // load the receiver class id
    abi::emit_load_symbol_to_reg(emitter, "x10", "_class_gc_desc_count", 0);
    emitter.instruction("cmp x12, x10");                                        // bounds-check the class id
    emitter.instruction("b.hs __rt_json_enum_ordinary");                        // unknown classes use the ordinary object encoder
    abi::emit_symbol_address(emitter, "x10", "_class_enum_kinds");
    emitter.instruction("ldr x13, [x10, x12, lsl #3]");                         // preserve the enum kind across descriptor lookup
    emitter.instruction("cbz x13, __rt_json_enum_ordinary");                    // ordinary classes retain their existing encoder
    abi::emit_symbol_address(emitter, "x10", "_class_json_desc_ptrs");
    emitter.instruction("ldr x10, [x10, x12, lsl #3]");                         // load the class JSON descriptor
    emitter.instruction("ldr x10, [x10]");                                      // load the descriptor flags
    emitter.instruction("tbnz x10, #0, __rt_json_enum_ordinary");               // JsonSerializable overrides the default enum behavior
    emitter.instruction("cmp x13, #1");                                         // unit enums have no default scalar representation
    emitter.instruction("b.eq __rt_json_enum_unit");                            // report the unsupported unit enum
    abi::emit_symbol_address(emitter, "x10", "_class_enum_value_offsets");
    emitter.instruction("ldr x10, [x10, x12, lsl #3]");                         // load the checked backing property offset
    emitter.instruction("add x10, x0, x10");                                    // address the backing scalar inside the enum instance
    emitter.instruction("cmp x13, #2");                                         // distinguish integer and string backing values
    emitter.instruction("b.ne __rt_json_enum_string");                          // strings use the ordinary escaping and flags
    emitter.instruction("ldr x0, [x10]");                                       // load the integer backing value
    emitter.instruction("b __rt_itoa");                                         // encode the backing integer without entering object depth
    emitter.label("__rt_json_enum_string");
    emitter.instruction("ldp x1, x2, [x10]");                                   // load the backing string pointer and length
    emitter.instruction("b __rt_json_encode_str");                              // encode the backing string with the active flags
    emitter.label("__rt_json_enum_unit");
    abi::emit_load_symbol_to_reg(emitter, "x9", "_json_active_flags", 0);
    emitter.instruction("mov x0, #11");                                         // JSON_ERROR_NON_BACKED_ENUM
    emitter.instruction("tbnz x9, #9, __rt_json_enum_partial");                 // partial output suppresses throw-on-error
    emitter.instruction("sub sp, sp, #16");                                     // reserve a helper call frame
    emitter.instruction("stp x29, x30, [sp]");                                  // preserve the caller frame and return address
    emitter.instruction("bl __rt_json_throw_error");                            // record the error and optionally throw JsonException
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore the caller frame and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper call frame
    emitter.label("__rt_json_enum_partial");
    emitter.instruction("mov x0, #11");                                         // record the unit enum error even for partial output
    abi::emit_store_reg_to_symbol(emitter, "x0", "_json_last_error", 0);
    emitter.instruction("mov x0, #0");                                          // PHP substitutes zero for an unsupported unit enum
    emitter.instruction("b __rt_itoa");                                         // return the placeholder through the integer encoder
    emitter.label("__rt_json_enum_ordinary");
}

/// Emits equivalent enum dispatch using the x86_64 internal runtime ABI.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the receiver class id
    abi::emit_load_symbol_to_reg(emitter, "r11", "_class_gc_desc_count", 0);
    emitter.instruction("cmp r10, r11");                                        // bounds-check the class id
    emitter.instruction("jae __rt_json_enum_ordinary_x");                       // unknown classes use the ordinary object encoder
    abi::emit_symbol_address(emitter, "r11", "_class_enum_kinds");
    emitter.instruction("mov r9, QWORD PTR [r11 + r10*8]");                     // preserve the enum kind across descriptor lookup
    emitter.instruction("test r9, r9");                                         // ordinary classes have no enum kind
    emitter.instruction("je __rt_json_enum_ordinary_x");                        // ordinary classes retain their existing encoder
    abi::emit_symbol_address(emitter, "r11", "_class_json_desc_ptrs");
    emitter.instruction("mov r11, QWORD PTR [r11 + r10*8]");                    // load the class JSON descriptor
    emitter.instruction("test QWORD PTR [r11], 1");                             // does the enum implement JsonSerializable?
    emitter.instruction("jne __rt_json_enum_ordinary_x");                       // custom serialization overrides the default behavior
    emitter.instruction("cmp r9, 1");                                           // unit enums have no default scalar representation
    emitter.instruction("je __rt_json_enum_unit_x");                            // report the unsupported unit enum
    abi::emit_symbol_address(emitter, "r11", "_class_enum_value_offsets");
    emitter.instruction("add rax, QWORD PTR [r11 + r10*8]");                    // address the backing property using its checked offset
    emitter.instruction("cmp r9, 2");                                           // distinguish integer and string backing values
    emitter.instruction("jne __rt_json_enum_string_x");                         // strings use the ordinary escaping and flags
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // load the integer backing value
    emitter.instruction("jmp __rt_itoa");                                       // encode the backing integer without entering object depth
    emitter.label("__rt_json_enum_string_x");
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // load the backing string length
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // load the backing string pointer
    emitter.instruction("jmp __rt_json_encode_str");                            // encode the backing string with the active flags
    emitter.label("__rt_json_enum_unit_x");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_json_active_flags", 0);
    emitter.instruction("mov rax, 11");                                         // JSON_ERROR_NON_BACKED_ENUM
    emitter.instruction("test r10, 512");                                       // does partial output suppress throwing?
    emitter.instruction("jne __rt_json_enum_partial_x");                        // emit a placeholder without throwing
    emitter.instruction("push rbp");                                            // preserve the caller frame and align the helper call
    emitter.instruction("mov rbp, rsp");                                        // establish a helper call frame
    emitter.instruction("call __rt_json_throw_error");                          // record the error and optionally throw JsonException
    emitter.instruction("pop rbp");                                             // restore the caller frame
    emitter.label("__rt_json_enum_partial_x");
    emitter.instruction("mov rax, 11");                                         // record the unit enum error even for partial output
    abi::emit_store_reg_to_symbol(emitter, "rax", "_json_last_error", 0);
    emitter.instruction("xor eax, eax");                                        // PHP substitutes zero for an unsupported unit enum
    emitter.instruction("jmp __rt_itoa");                                       // return the placeholder through the integer encoder
    emitter.label("__rt_json_enum_ordinary_x");
}
