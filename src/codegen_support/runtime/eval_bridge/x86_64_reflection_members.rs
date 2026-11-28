//! Purpose:
//! Emits x86_64 reflection flags and declaring-class lookups.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Method and property metadata use the existing table strides.

use super::*;
use elephc_memoir::values::reflection_metadata::{self, MemberField};

/// Emits the x86_64 eval hook that returns AOT ReflectionMethod predicate flags.
pub(super) fn emit_x86_64_eval_reflection_method_flags(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_reflection_method_flags");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable scan frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve scan state across runtime string comparisons
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested method-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the requested method-name length
    abi::emit_symbol_address(emitter, "r10", "_eval_reflection_method_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the AOT reflection-method row count
    emitter.instruction("test r10, r10");                                       // is the method metadata table empty?
    emitter.instruction("jz __elephc_eval_reflection_method_flags_miss_x86");   // an empty table cannot contain the requested method
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the table count across string comparisons
    abi::emit_symbol_address(emitter, "r11", "_eval_reflection_methods");
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // save the current method metadata row
    emitter.instruction("xor r11d, r11d");                                      // start scanning at method metadata row zero
    emitter.label("__elephc_eval_reflection_method_flags_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the method metadata row count
    emitter.instruction("cmp r11, r10");                                        // have all method metadata rows been scanned?
    emitter.instruction("jae __elephc_eval_reflection_method_flags_miss_x86");  // no row matched before the end of the table
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current method metadata row
    reflection_metadata::emit_load_member_field(emitter, "rcx", "r10", MemberField::ClassNameLength);
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare stored and requested class-name lengths
    emitter.instruction("jne __elephc_eval_reflection_method_flags_skip_x86");  // length mismatch means the class cannot match
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save the row index across the class-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested class-name length
    reflection_metadata::emit_load_member_field(emitter, "rdx", "r10", MemberField::ClassName);
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // restore the row index after the class-name compare
    emitter.instruction("test rax, rax");                                       // did the requested class name match this row?
    emitter.instruction("jne __elephc_eval_reflection_method_flags_skip_x86");  // class mismatch means the row cannot match
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current row for the method-name compare
    reflection_metadata::emit_load_member_field(emitter, "rcx", "r10", MemberField::MemberNameLength);
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // compare stored and requested method-name lengths
    emitter.instruction("jne __elephc_eval_reflection_method_flags_skip_x86");  // length mismatch means the method cannot match
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save the row index across the method-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the requested method-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the requested method-name length
    reflection_metadata::emit_load_member_field(emitter, "rdx", "r10", MemberField::MemberName);
    emitter.instruction("call __rt_strcasecmp");                                // compare method names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // restore the row index after the method-name compare
    emitter.instruction("test rax, rax");                                       // did the requested method name match this row?
    emitter.instruction("jne __elephc_eval_reflection_method_flags_skip_x86");  // method mismatch means scanning must continue
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the matched method metadata row
    reflection_metadata::emit_load_member_field(emitter, "rax", "r10", MemberField::Flags);
    emitter.instruction("jmp __elephc_eval_reflection_method_flags_done_x86");  // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_method_flags_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current method metadata row
    reflection_metadata::emit_advance_member_row(emitter, "r10");
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // persist the advanced row cursor
    emitter.instruction("inc r11");                                             // advance the row index
    emitter.instruction("jmp __elephc_eval_reflection_method_flags_loop_x86");  // continue scanning method metadata rows
    emitter.label("__elephc_eval_reflection_method_flags_miss_x86");
    emitter.instruction("xor eax, eax");                                        // return zero when no AOT method metadata matched
    emitter.label("__elephc_eval_reflection_method_flags_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return flags, or zero for a miss, to Rust
}

/// Emits the x86_64 eval hook that returns a matched AOT ReflectionMethod declaring class.
pub(super) fn emit_x86_64_eval_reflection_method_declaring_class(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_reflection_method_declaring_class");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable scan frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve scan state across runtime string comparisons
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested method-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the requested method-name length
    abi::emit_symbol_address(emitter, "r10", "_eval_reflection_method_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the AOT reflection-method row count
    emitter.instruction("test r10, r10");                                       // is the method metadata table empty?
    emitter.instruction("jz __elephc_eval_reflection_method_declaring_class_miss_x86"); // an empty table cannot contain the requested method
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the table count across string comparisons
    abi::emit_symbol_address(emitter, "r11", "_eval_reflection_methods");
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // save the current method metadata row
    emitter.instruction("xor r11d, r11d");                                      // start scanning at method metadata row zero
    emitter.label("__elephc_eval_reflection_method_declaring_class_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the method metadata row count
    emitter.instruction("cmp r11, r10");                                        // have all method metadata rows been scanned?
    emitter.instruction("jae __elephc_eval_reflection_method_declaring_class_miss_x86"); // no row matched before the end of the table
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current method metadata row
    reflection_metadata::emit_load_member_field(emitter, "rcx", "r10", MemberField::ClassNameLength);
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare stored and requested class-name lengths
    emitter.instruction("jne __elephc_eval_reflection_method_declaring_class_skip_x86"); // length mismatch means the class cannot match
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save the row index across the class-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested class-name length
    reflection_metadata::emit_load_member_field(emitter, "rdx", "r10", MemberField::ClassName);
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // restore the row index after the class-name compare
    emitter.instruction("test rax, rax");                                       // did the requested class name match this row?
    emitter.instruction("jne __elephc_eval_reflection_method_declaring_class_skip_x86"); // class mismatch means the row cannot match
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current row for the method-name compare
    reflection_metadata::emit_load_member_field(emitter, "rcx", "r10", MemberField::MemberNameLength);
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // compare stored and requested method-name lengths
    emitter.instruction("jne __elephc_eval_reflection_method_declaring_class_skip_x86"); // length mismatch means the method cannot match
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save the row index across the method-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the requested method-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the requested method-name length
    reflection_metadata::emit_load_member_field(emitter, "rdx", "r10", MemberField::MemberName);
    emitter.instruction("call __rt_strcasecmp");                                // compare method names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // restore the row index after the method-name compare
    emitter.instruction("test rax, rax");                                       // did the requested method name match this row?
    emitter.instruction("jne __elephc_eval_reflection_method_declaring_class_skip_x86"); // method mismatch means scanning must continue
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the matched method metadata row
    reflection_metadata::emit_load_member_field(emitter, "rdi", "r10", MemberField::DeclaringClass);
    reflection_metadata::emit_load_member_field(emitter, "rsi", "r10", MemberField::DeclaringClassLength);
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // box the declaring class name for Rust
    emitter.instruction("jmp __elephc_eval_reflection_method_declaring_class_done_x86"); // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_method_declaring_class_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current method metadata row
    reflection_metadata::emit_advance_member_row(emitter, "r10");
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // persist the advanced row cursor
    emitter.instruction("inc r11");                                             // advance the row index
    emitter.instruction("jmp __elephc_eval_reflection_method_declaring_class_loop_x86"); // continue scanning method metadata rows
    emitter.label("__elephc_eval_reflection_method_declaring_class_miss_x86");
    emitter.instruction("xor eax, eax");                                        // return null when no AOT method metadata matched
    emitter.label("__elephc_eval_reflection_method_declaring_class_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the declaring class string, or null for a miss, to Rust
}

/// Emits the x86_64 eval hook that returns a matched AOT ReflectionProperty declaring class.
pub(super) fn emit_x86_64_eval_reflection_property_declaring_class(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_reflection_property_declaring_class");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable scan frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve scan state across runtime string comparisons
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested property-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the requested property-name length
    abi::emit_symbol_address(emitter, "r10", "_eval_reflection_property_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the AOT reflection-property row count
    emitter.instruction("test r10, r10");                                       // is the property metadata table empty?
    emitter.instruction("jz __elephc_eval_reflection_property_declaring_class_miss_x86"); // an empty table cannot contain the requested property
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the table count across string comparisons
    abi::emit_symbol_address(emitter, "r11", "_eval_reflection_properties");
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // save the current property metadata row
    emitter.instruction("xor r11d, r11d");                                      // start scanning at property metadata row zero
    emitter.label("__elephc_eval_reflection_property_declaring_class_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the property metadata row count
    emitter.instruction("cmp r11, r10");                                        // have all property metadata rows been scanned?
    emitter.instruction("jae __elephc_eval_reflection_property_declaring_class_miss_x86"); // no row matched before the end of the table
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current property metadata row
    reflection_metadata::emit_load_member_field(emitter, "rcx", "r10", MemberField::ClassNameLength);
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare stored and requested class-name lengths
    emitter.instruction("jne __elephc_eval_reflection_property_declaring_class_skip_x86"); // length mismatch means the class cannot match
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save the row index across the class-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested class-name length
    reflection_metadata::emit_load_member_field(emitter, "rdx", "r10", MemberField::ClassName);
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // restore the row index after the class-name compare
    emitter.instruction("test rax, rax");                                       // did the requested class name match this row?
    emitter.instruction("jne __elephc_eval_reflection_property_declaring_class_skip_x86"); // class mismatch means the row cannot match
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current row for the property-name compare
    reflection_metadata::emit_load_member_field(emitter, "rcx", "r10", MemberField::MemberNameLength);
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // compare stored and requested property-name lengths
    emitter.instruction("jne __elephc_eval_reflection_property_declaring_class_skip_x86"); // length mismatch means the property cannot match
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save the row index across the property-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the requested property-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the requested property-name length
    reflection_metadata::emit_load_member_field(emitter, "rdx", "r10", MemberField::MemberName);
    emitter.instruction("call __rt_strcasecmp");                                // compare property names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // restore the row index after the property-name compare
    emitter.instruction("test rax, rax");                                       // did the requested property name match this row?
    emitter.instruction("jne __elephc_eval_reflection_property_declaring_class_skip_x86"); // property mismatch means scanning must continue
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the matched property metadata row
    reflection_metadata::emit_load_member_field(emitter, "rdi", "r10", MemberField::DeclaringClass);
    reflection_metadata::emit_load_member_field(emitter, "rsi", "r10", MemberField::DeclaringClassLength);
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // box the declaring class name for Rust
    emitter.instruction("jmp __elephc_eval_reflection_property_declaring_class_done_x86"); // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_property_declaring_class_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current property metadata row
    reflection_metadata::emit_advance_member_row(emitter, "r10");
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // persist the advanced row cursor
    emitter.instruction("inc r11");                                             // advance the row index
    emitter.instruction("jmp __elephc_eval_reflection_property_declaring_class_loop_x86"); // continue scanning property metadata rows
    emitter.label("__elephc_eval_reflection_property_declaring_class_miss_x86");
    emitter.instruction("xor eax, eax");                                        // return null when no AOT property metadata matched
    emitter.label("__elephc_eval_reflection_property_declaring_class_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the declaring class string, or null for a miss, to Rust
}

/// Emits the x86_64 eval hook that returns AOT ReflectionProperty predicate flags.
pub(super) fn emit_x86_64_eval_reflection_property_flags(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_reflection_property_flags");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable scan frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve scan state across runtime string comparisons
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested property-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the requested property-name length
    abi::emit_symbol_address(emitter, "r10", "_eval_reflection_property_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the AOT reflection-property row count
    emitter.instruction("test r10, r10");                                       // is the property metadata table empty?
    emitter.instruction("jz __elephc_eval_reflection_property_flags_miss_x86"); // an empty table cannot contain the requested property
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the table count across string comparisons
    abi::emit_symbol_address(emitter, "r11", "_eval_reflection_properties");
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // save the current property metadata row
    emitter.instruction("xor r11d, r11d");                                      // start scanning at property metadata row zero
    emitter.label("__elephc_eval_reflection_property_flags_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the property metadata row count
    emitter.instruction("cmp r11, r10");                                        // have all property metadata rows been scanned?
    emitter.instruction("jae __elephc_eval_reflection_property_flags_miss_x86"); // no row matched before the end of the table
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current property metadata row
    reflection_metadata::emit_load_member_field(emitter, "rcx", "r10", MemberField::ClassNameLength);
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare stored and requested class-name lengths
    emitter.instruction("jne __elephc_eval_reflection_property_flags_skip_x86"); // length mismatch means the class cannot match
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save the row index across the class-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested class-name length
    reflection_metadata::emit_load_member_field(emitter, "rdx", "r10", MemberField::ClassName);
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // restore the row index after the class-name compare
    emitter.instruction("test rax, rax");                                       // did the requested class name match this row?
    emitter.instruction("jne __elephc_eval_reflection_property_flags_skip_x86"); // class mismatch means the row cannot match
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current row for the property-name compare
    reflection_metadata::emit_load_member_field(emitter, "rcx", "r10", MemberField::MemberNameLength);
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // compare stored and requested property-name lengths
    emitter.instruction("jne __elephc_eval_reflection_property_flags_skip_x86"); // length mismatch means the property cannot match
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save the row index across the property-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the requested property-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the requested property-name length
    reflection_metadata::emit_load_member_field(emitter, "rdx", "r10", MemberField::MemberName);
    emitter.instruction("call __rt_str_eq");                                    // compare property names with PHP case-sensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // restore the row index after the property-name compare
    emitter.instruction("test rax, rax");                                       // did the requested property name match this row?
    emitter.instruction("jz __elephc_eval_reflection_property_flags_skip_x86"); // property mismatch means scanning must continue
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the matched property metadata row
    reflection_metadata::emit_load_member_field(emitter, "rax", "r10", MemberField::Flags);
    emitter.instruction("jmp __elephc_eval_reflection_property_flags_done_x86"); // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_property_flags_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current property metadata row
    reflection_metadata::emit_advance_member_row(emitter, "r10");
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // persist the advanced row cursor
    emitter.instruction("inc r11");                                             // advance the row index
    emitter.instruction("jmp __elephc_eval_reflection_property_flags_loop_x86"); // continue scanning property metadata rows
    emitter.label("__elephc_eval_reflection_property_flags_miss_x86");
    emitter.instruction("xor eax, eax");                                        // return zero when no AOT property metadata matched
    emitter.label("__elephc_eval_reflection_property_flags_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return flags, or zero for a miss, to Rust
}
