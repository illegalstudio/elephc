//! Purpose:
//! Complements boxed PHP integers, floats and byte strings and rejects other values.
//!
//! Called from:
//! - The typed EIR MixedBitNot operation and Magician's scalar bit-not callback.
//!
//! Key details:
//! - The input is borrowed; no input owner survives a float diagnostic callback.
//! - Float conversion shares PHP array-key precision and range diagnostics.
//! - String results are independently boxed and scratch reservations are retired.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Prefix used by the shared target-aware TypeError constructor.
pub(crate) const BIT_NOT_PREFIX: &str = "Cannot perform bitwise not on ";
/// Persistent diagnostic rows, with ordinary objects using their actual class-name row.
pub(crate) const BIT_NOT_TYPE_ROWS: [(&str, usize); 8] = [
    ("_unser_type_true", 4), ("_unser_type_false", 5),
    ("_unser_type_array", 5), ("_unser_type_null", 4),
    ("_unser_type_resource", 8), ("_sprintf_closure_class_name", 7),
    ("_unser_type_object", 6), ("_unser_type_unknown", 7),
];

/// Borrows a Mixed cell in the integer result register and returns a fresh Mixed owner.
pub(crate) fn emit_mixed_bit_not(e: &mut Emitter) {
    if e.target.arch == Arch::X86_64 { emit_x86_64(e); } else { emit_aarch64(e); }
}

/// Emits the AArch64 borrowed-value dispatch and independent result construction.
fn emit_aarch64(e: &mut Emitter) {
    e.blank();
    e.label_global("__rt_mixed_bit_not");
    e.instruction("sub sp, sp, #64");                                           // reserve source, reservation, result and linkage slots
    e.instruction("stp x29, x30, [sp, #48]");                                   // preserve the caller linkage
    e.instruction("add x29, sp, #48");                                          // establish the helper frame
    abi::emit_call_label(e, "__rt_mixed_unbox");
    e.instruction("cmp x0, #0");                                                // select integer complement
    e.instruction("b.eq __rt_mixed_bit_not_int");                               // integers need no conversion
    e.instruction("cmp x0, #1");                                                // select byte-string complement
    e.instruction("b.eq __rt_mixed_bit_not_string");                            // preserve string byte semantics
    e.instruction("cmp x0, #2");                                                // select implicit float conversion
    e.instruction("b.eq __rt_mixed_bit_not_float");                             // diagnostics precede allocation of the result
    e.instruction("cmp x0, #3");                                                // bool errors name the literal value
    e.instruction("b.eq __rt_mixed_bit_not_bool");                              // distinguish true and false
    e.instruction("cmp x0, #4");                                                // indexed arrays are unsupported operands
    e.instruction("b.eq __rt_mixed_bit_not_array");                             // select the array diagnostic
    e.instruction("cmp x0, #5");                                                // associative arrays have the same PHP type
    e.instruction("b.eq __rt_mixed_bit_not_array");                             // select the array diagnostic
    e.instruction("cmp x0, #6");                                                // objects name their concrete class
    e.instruction("b.eq __rt_mixed_bit_not_object");                            // consult dense class-name metadata
    e.instruction("cmp x0, #8");                                                // null is an unsupported operand
    e.instruction("b.eq __rt_mixed_bit_not_null");                              // select the null diagnostic
    e.instruction("cmp x0, #9");                                                // resources are unsupported operands
    e.instruction("b.eq __rt_mixed_bit_not_resource");                          // select the resource diagnostic
    e.instruction("cmp x0, #10");                                               // callable descriptors represent Closure objects
    e.instruction("b.eq __rt_mixed_bit_not_closure");                           // select the Closure diagnostic
    e.instruction("mov x10, #7");                                               // malformed tags use the unknown type
    e.instruction("b __rt_mixed_bit_not_row");                                  // select a persistent diagnostic row
    e.label("__rt_mixed_bit_not_float");
    e.instruction("fmov d0, x1");                                               // pass the unboxed float to the shared conversion
    abi::emit_call_label(e, "__rt_float_key_to_int");
    e.instruction("mov x1, x0");                                                // reuse the converted integer after diagnostics
    e.label("__rt_mixed_bit_not_int");
    e.instruction("mvn x1, x1");                                                // complement the integer payload
    e.instruction("mov x0, #0");                                                // runtime tag zero identifies an integer
    e.instruction("mov x2, #0");                                                // integer payloads have no high word
    abi::emit_call_label(e, "__rt_mixed_from_value");
    e.instruction("b __rt_mixed_bit_not_done");                                 // return the fresh integer box
    e.label("__rt_mixed_bit_not_string");
    e.instruction("stp x1, x2, [sp]");                                          // preserve the borrowed source slice
    e.instruction("mov x0, x2");                                                // reserve the original string length
    abi::emit_call_label(e, "__rt_concat_reserve");
    e.instruction("str x0, [sp, #16]");                                         // remember the reservation for conditional retirement
    e.instruction("ldp x9, x10, [sp]");                                         // reload source pointer and length
    e.instruction("mov x11, x0");                                               // initialize the destination cursor
    e.label("__rt_mixed_bit_not_bytes");
    e.instruction("cbz x10, __rt_mixed_bit_not_bytes_done");                    // finish after every byte is complemented
    e.instruction("ldrb w12, [x9], #1");                                        // read and advance the source cursor
    e.instruction("mvn w12, w12");                                              // complement the byte without integer string coercion
    e.instruction("strb w12, [x11], #1");                                       // write and advance the destination cursor
    e.instruction("sub x10, x10, #1");                                          // count the remaining bytes
    e.instruction("b __rt_mixed_bit_not_bytes");                                // complement the next source byte
    e.label("__rt_mixed_bit_not_bytes_done");
    e.instruction("ldr x1, [sp, #16]");                                         // return the reserved destination pointer
    e.instruction("ldr x2, [sp, #8]");                                          // return the unchanged byte length
    abi::emit_call_label(e, "__rt_concat_publish");
    e.instruction("mov x0, #1");                                                // runtime tag one identifies a byte string
    abi::emit_call_label(e, "__rt_mixed_from_value");
    e.instruction("str x0, [sp, #24]");                                         // preserve the independent boxed result
    e.instruction("ldr x0, [sp, #16]");                                         // retire only the reservation's producer reference
    abi::emit_call_label(e, "__rt_heap_free");
    e.instruction("ldr x0, [sp, #24]");                                         // restore the fresh boxed string owner
    e.instruction("b __rt_mixed_bit_not_done");                                 // leave through the common epilogue
    e.label("__rt_mixed_bit_not_bool");
    e.instruction("cmp x1, #0");                                                // inspect the boolean payload
    e.instruction("cset x10, eq");                                              // true uses row zero and false row one
    e.instruction("b __rt_mixed_bit_not_row");                                  // select the literal boolean spelling
    e.label("__rt_mixed_bit_not_array");
    e.instruction("mov x10, #2");                                               // select the array type row
    e.instruction("b __rt_mixed_bit_not_row");                                  // build the catchable TypeError
    e.label("__rt_mixed_bit_not_null");
    e.instruction("mov x10, #3");                                               // select the null type row
    e.instruction("b __rt_mixed_bit_not_row");                                  // build the catchable TypeError
    e.label("__rt_mixed_bit_not_resource");
    e.instruction("mov x10, #4");                                               // select the resource type row
    e.instruction("b __rt_mixed_bit_not_row");                                  // build the catchable TypeError
    e.label("__rt_mixed_bit_not_closure");
    e.instruction("mov x10, #5");                                               // select the Closure class row
    e.instruction("b __rt_mixed_bit_not_row");                                  // build the catchable TypeError
    e.label("__rt_mixed_bit_not_object");
    e.instruction("ldr x13, [x1]");                                             // read the concrete runtime class id
    abi::emit_load_symbol_to_reg(e, "x10", "_class_name_count", 0);
    e.instruction("cmp x13, x10");                                              // reject ids outside the metadata table
    e.instruction("b.hs __rt_mixed_bit_not_object_fallback");                   // malformed classes use the object spelling
    abi::emit_symbol_address(e, "x11", "_class_name_entries");
    e.instruction("add x2, x11, x13, lsl #4");                                  // select the persistent concrete class-name row
    e.instruction("ldr x12, [x2, #8]");                                         // inspect the concrete name length
    e.instruction("cbnz x12, __rt_mixed_bit_not_error");                        // use the declared class name when available
    e.label("__rt_mixed_bit_not_object_fallback");
    e.instruction("mov x10, #6");                                               // select the generic object spelling
    e.label("__rt_mixed_bit_not_row");
    abi::emit_symbol_address(e, "x11", "_bit_not_type_rows");
    e.instruction("add x2, x11, x10, lsl #4");                                  // select the persistent diagnostic name row
    e.label("__rt_mixed_bit_not_error");
    abi::emit_symbol_address(e, "x0", "_bit_not_prefix");
    abi::emit_load_int_immediate(e, "x1", BIT_NOT_PREFIX.len() as i64);
    e.instruction("mov x3, #0");                                                // the TypeError has no suffix pointer
    e.instruction("mov x4, #0");                                                // the TypeError has no suffix bytes
    e.instruction("ldp x29, x30, [sp, #48]");                                   // restore caller linkage before the tail throw
    e.instruction("add sp, sp, #64");                                           // retire the borrowed-input helper frame
    e.instruction("b __rt_spl_throw_offset_type");                              // construct and throw the standard TypeError
    e.label("__rt_mixed_bit_not_done");
    e.instruction("ldp x29, x30, [sp, #48]");                                   // restore caller linkage
    e.instruction("add sp, sp, #64");                                           // release the helper frame
    e.instruction("ret");                                                       // return the fresh Mixed owner
}

/// Emits the System V dispatch with the same diagnostics, byte transform and ownership.
fn emit_x86_64(e: &mut Emitter) {
    e.blank();
    e.label_global("__rt_mixed_bit_not");
    e.instruction("push rbp");                                                  // preserve and align the caller frame
    e.instruction("mov rbp, rsp");                                              // establish the helper frame
    e.instruction("sub rsp, 32");                                               // reserve source, reservation and boxed-result slots
    abi::emit_call_label(e, "__rt_mixed_unbox");
    e.instruction("cmp rax, 0");                                                // select integer complement
    e.instruction("je __rt_mixed_bit_not_int");                                 // integers need no conversion
    e.instruction("cmp rax, 1");                                                // select byte-string complement
    e.instruction("je __rt_mixed_bit_not_string");                              // preserve string byte semantics
    e.instruction("cmp rax, 2");                                                // select implicit float conversion
    e.instruction("je __rt_mixed_bit_not_float");                               // diagnostics precede result allocation
    e.instruction("cmp rax, 3");                                                // bool errors name the literal value
    e.instruction("je __rt_mixed_bit_not_bool");                                // distinguish true and false
    e.instruction("cmp rax, 4");                                                // indexed arrays are unsupported operands
    e.instruction("je __rt_mixed_bit_not_array");                               // select the array diagnostic
    e.instruction("cmp rax, 5");                                                // associative arrays have the same PHP type
    e.instruction("je __rt_mixed_bit_not_array");                               // select the array diagnostic
    e.instruction("cmp rax, 6");                                                // objects name their concrete class
    e.instruction("je __rt_mixed_bit_not_object");                              // consult dense class-name metadata
    e.instruction("cmp rax, 8");                                                // null is an unsupported operand
    e.instruction("je __rt_mixed_bit_not_null");                                // select the null diagnostic
    e.instruction("cmp rax, 9");                                                // resources are unsupported operands
    e.instruction("je __rt_mixed_bit_not_resource");                            // select the resource diagnostic
    e.instruction("cmp rax, 10");                                               // callable descriptors represent Closure objects
    e.instruction("je __rt_mixed_bit_not_closure");                             // select the Closure diagnostic
    e.instruction("mov r10, 7");                                                // malformed tags use the unknown type
    e.instruction("jmp __rt_mixed_bit_not_row");                                // select a persistent diagnostic row
    e.label("__rt_mixed_bit_not_float");
    e.instruction("movq xmm0, rdi");                                            // pass the unboxed float to the shared conversion
    abi::emit_call_label(e, "__rt_float_key_to_int");
    e.instruction("mov rdi, rax");                                              // reuse the converted integer after diagnostics
    e.label("__rt_mixed_bit_not_int");
    e.instruction("not rdi");                                                   // complement the integer payload
    e.instruction("xor eax, eax");                                              // runtime tag zero identifies an integer
    e.instruction("xor esi, esi");                                              // integer payloads have no high word
    abi::emit_call_label(e, "__rt_mixed_from_value");
    e.instruction("jmp __rt_mixed_bit_not_done");                               // return the fresh integer box
    e.label("__rt_mixed_bit_not_string");
    e.instruction("mov QWORD PTR [rbp - 8], rdi");                              // preserve the borrowed source pointer
    e.instruction("mov QWORD PTR [rbp - 16], rdx");                             // preserve the source byte length
    e.instruction("mov rax, rdx");                                              // reserve the original string length
    abi::emit_call_label(e, "__rt_concat_reserve");
    e.instruction("mov QWORD PTR [rbp - 24], rax");                             // remember the reservation for conditional retirement
    e.instruction("mov r8, QWORD PTR [rbp - 8]");                               // reload the source cursor
    e.instruction("mov rcx, QWORD PTR [rbp - 16]");                             // reload the remaining byte count
    e.instruction("mov r11, rax");                                              // initialize the destination cursor
    e.label("__rt_mixed_bit_not_bytes");
    e.instruction("test rcx, rcx");                                             // check whether every byte was complemented
    e.instruction("jz __rt_mixed_bit_not_bytes_done");                          // finish the bytewise transform
    e.instruction("mov dl, BYTE PTR [r8]");                                     // read the next source byte
    e.instruction("not dl");                                                    // complement the byte without integer string coercion
    e.instruction("mov BYTE PTR [r11], dl");                                    // store the complemented byte
    e.instruction("add r8, 1");                                                 // advance the source cursor
    e.instruction("add r11, 1");                                                // advance the destination cursor
    e.instruction("sub rcx, 1");                                                // count the remaining bytes
    e.instruction("jmp __rt_mixed_bit_not_bytes");                              // complement the next source byte
    e.label("__rt_mixed_bit_not_bytes_done");
    e.instruction("mov rax, QWORD PTR [rbp - 24]");                             // publish the reserved destination pointer
    e.instruction("mov rdx, QWORD PTR [rbp - 16]");                             // publish the original byte length
    abi::emit_call_label(e, "__rt_concat_publish");
    e.instruction("mov rdi, rax");                                              // pass the new string as the low payload word
    e.instruction("mov rsi, rdx");                                              // pass the new string length as the high payload word
    e.instruction("mov eax, 1");                                                // runtime tag one identifies a byte string
    abi::emit_call_label(e, "__rt_mixed_from_value");
    e.instruction("mov QWORD PTR [rbp - 32], rax");                             // preserve the independent boxed result
    e.instruction("mov rax, QWORD PTR [rbp - 24]");                             // retire only the reservation's producer reference
    abi::emit_call_label(e, "__rt_heap_free");
    e.instruction("mov rax, QWORD PTR [rbp - 32]");                             // restore the fresh boxed string owner
    e.instruction("jmp __rt_mixed_bit_not_done");                               // leave through the common epilogue
    e.label("__rt_mixed_bit_not_bool");
    e.instruction("xor r10d, r10d");                                            // true uses row zero
    e.instruction("test rdi, rdi");                                             // inspect the boolean payload
    e.instruction("setz r10b");                                                 // false uses row one
    e.instruction("jmp __rt_mixed_bit_not_row");                                // select the literal boolean spelling
    e.label("__rt_mixed_bit_not_array");
    e.instruction("mov r10, 2");                                                // select the array type row
    e.instruction("jmp __rt_mixed_bit_not_row");                                // build the catchable TypeError
    e.label("__rt_mixed_bit_not_null");
    e.instruction("mov r10, 3");                                                // select the null type row
    e.instruction("jmp __rt_mixed_bit_not_row");                                // build the catchable TypeError
    e.label("__rt_mixed_bit_not_resource");
    e.instruction("mov r10, 4");                                                // select the resource type row
    e.instruction("jmp __rt_mixed_bit_not_row");                                // build the catchable TypeError
    e.label("__rt_mixed_bit_not_closure");
    e.instruction("mov r10, 5");                                                // select the Closure class row
    e.instruction("jmp __rt_mixed_bit_not_row");                                // build the catchable TypeError
    e.label("__rt_mixed_bit_not_object");
    e.instruction("mov r9, QWORD PTR [rdi]");                                   // read the concrete runtime class id
    abi::emit_load_symbol_to_reg(e, "r10", "_class_name_count", 0);
    e.instruction("cmp r9, r10");                                               // reject ids outside the metadata table
    e.instruction("jae __rt_mixed_bit_not_object_fallback");                    // malformed classes use the object spelling
    abi::emit_symbol_address(e, "r11", "_class_name_entries");
    e.instruction("shl r9, 4");                                                 // compute the class-name row byte offset
    e.instruction("lea rdx, [r11 + r9]");                                       // select the persistent concrete class-name row
    e.instruction("cmp QWORD PTR [rdx + 8], 0");                                // inspect the concrete name length
    e.instruction("jne __rt_mixed_bit_not_error");                              // use the declared class name when available
    e.label("__rt_mixed_bit_not_object_fallback");
    e.instruction("mov r10, 6");                                                // select the generic object spelling
    e.label("__rt_mixed_bit_not_row");
    abi::emit_symbol_address(e, "r11", "_bit_not_type_rows");
    e.instruction("shl r10, 4");                                                // compute the diagnostic row byte offset
    e.instruction("lea rdx, [r11 + r10]");                                      // select the persistent diagnostic name row
    e.label("__rt_mixed_bit_not_error");
    abi::emit_symbol_address(e, "rdi", "_bit_not_prefix");
    abi::emit_load_int_immediate(e, "rsi", BIT_NOT_PREFIX.len() as i64);
    e.instruction("xor ecx, ecx");                                              // the TypeError has no suffix pointer
    e.instruction("xor r8d, r8d");                                              // the TypeError has no suffix bytes
    e.instruction("mov rsp, rbp");                                              // retire the borrowed-input helper frame
    e.instruction("pop rbp");                                                   // restore the caller frame before the tail throw
    e.instruction("jmp __rt_spl_throw_offset_type");                            // construct and throw the standard TypeError
    e.label("__rt_mixed_bit_not_done");
    e.instruction("mov rsp, rbp");                                              // release the helper frame
    e.instruction("pop rbp");                                                   // restore the caller frame
    e.instruction("ret");                                                       // return the fresh Mixed owner
}
