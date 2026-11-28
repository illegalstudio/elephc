//! Purpose:
//! Emits ReflectionClass flag queries for both supported architectures.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Abstract, interface, trait, enum, and readonly flags retain their bit layout.

use super::*;
use elephc_memoir::values::reflection_metadata::{self, ClassField};

/// Emits the ARM64 eval hook that returns AOT ReflectionClass flags.
pub(super) fn emit_aarch64_eval_reflection_class_flags(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_reflection_class_flags");
    emitter.instruction("sub sp, sp, #64");                                     // reserve class-flag scan state across string comparisons
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable class-flag scan frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    abi::emit_symbol_address(emitter, "x9", "_eval_reflection_class_count");
    emitter.instruction("ldr x9, [x9]");                                        // load the class-flag metadata row count
    emitter.instruction("cbz x9, __elephc_eval_reflection_class_flags_miss");   // an empty table cannot contain class flags
    emitter.instruction("str x9, [sp, #16]");                                   // save the class-flag row count
    abi::emit_symbol_address(emitter, "x10", "_eval_reflection_classes");
    emitter.instruction("str x10, [sp, #24]");                                  // save the current class-flag metadata row
    emitter.instruction("mov x11, #0");                                         // start scanning at class-flag row zero
    emitter.label("__elephc_eval_reflection_class_flags_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the class-flag metadata row count
    emitter.instruction("cmp x11, x9");                                         // have all class-flag rows been scanned?
    emitter.instruction("b.ge __elephc_eval_reflection_class_flags_miss");      // no row matched before the end of the table
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current class-flag metadata row
    reflection_metadata::emit_load_class_field(emitter, "x12", "x10", ClassField::ClassNameLength);
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested class-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested class-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_class_flags_skip");      // length mismatch means this row belongs to another class
    emitter.instruction("str x11, [sp, #32]");                                  // save the row index across the class-name compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested class-name length
    reflection_metadata::emit_load_class_field(emitter, "x3", "x10", ClassField::ClassName);
    emitter.instruction("mov x4, x12");                                         // pass the stored class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #32]");                                  // restore the row index after the class-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested class name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_class_flags_skip");      // class mismatch means scanning must continue
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the matched class-flag metadata row
    reflection_metadata::emit_load_class_field(emitter, "x0", "x10", ClassField::Flags);
    emitter.instruction("b __elephc_eval_reflection_class_flags_done");         // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_class_flags_skip");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current class-flag metadata row
    reflection_metadata::emit_advance_class_row(emitter, "x10");
    emitter.instruction("str x10, [sp, #24]");                                  // persist the advanced row cursor
    emitter.instruction("add x11, x11, #1");                                    // advance the row index
    emitter.instruction("b __elephc_eval_reflection_class_flags_loop");         // continue scanning class-flag metadata rows
    emitter.label("__elephc_eval_reflection_class_flags_miss");
    emitter.instruction("mov x0, #0");                                          // return zero when no AOT class flags matched
    emitter.label("__elephc_eval_reflection_class_flags_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the class-flag metadata scan frame
    emitter.instruction("ret");                                                 // return flags, or zero for a miss, to Rust
}

/// Emits the x86_64 eval hook that returns AOT ReflectionClass flags.
pub(super) fn emit_x86_64_eval_reflection_class_flags(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_reflection_class_flags");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable class-flag scan frame
    emitter.instruction("sub rsp, 48");                                         // reserve class-name, count, cursor, and index slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    abi::emit_symbol_address(emitter, "r10", "_eval_reflection_class_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the class-flag metadata row count
    emitter.instruction("test r10, r10");                                       // is the class-flag metadata table empty?
    emitter.instruction("jz __elephc_eval_reflection_class_flags_miss_x86");    // an empty table cannot contain class flags
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the class-flag row count
    abi::emit_symbol_address(emitter, "r11", "_eval_reflection_classes");
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the current class-flag metadata row
    emitter.instruction("xor r11d, r11d");                                      // start scanning at class-flag row zero
    emitter.label("__elephc_eval_reflection_class_flags_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the class-flag metadata row count
    emitter.instruction("cmp r11, r10");                                        // have all class-flag rows been scanned?
    emitter.instruction("jae __elephc_eval_reflection_class_flags_miss_x86");   // no row matched before the end of the table
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current class-flag metadata row
    reflection_metadata::emit_load_class_field(emitter, "rcx", "r10", ClassField::ClassNameLength);
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare stored and requested class-name lengths
    emitter.instruction("jne __elephc_eval_reflection_class_flags_skip_x86");   // length mismatch means this row belongs to another class
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the row index across the class-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested class-name length
    reflection_metadata::emit_load_class_field(emitter, "rdx", "r10", ClassField::ClassName);
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // restore the row index after the class-name compare
    emitter.instruction("test rax, rax");                                       // did the requested class name match this row?
    emitter.instruction("jne __elephc_eval_reflection_class_flags_skip_x86");   // class mismatch means scanning must continue
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the matched class-flag metadata row
    reflection_metadata::emit_load_class_field(emitter, "rax", "r10", ClassField::Flags);
    emitter.instruction("jmp __elephc_eval_reflection_class_flags_done_x86");   // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_class_flags_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current class-flag metadata row
    reflection_metadata::emit_advance_class_row(emitter, "r10");
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the advanced row cursor
    emitter.instruction("inc r11");                                             // advance the row index
    emitter.instruction("jmp __elephc_eval_reflection_class_flags_loop_x86");   // continue scanning class-flag metadata rows
    emitter.label("__elephc_eval_reflection_class_flags_miss_x86");
    emitter.instruction("xor eax, eax");                                        // return zero when no AOT class flags matched
    emitter.label("__elephc_eval_reflection_class_flags_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return flags, or zero for a miss, to Rust
}
