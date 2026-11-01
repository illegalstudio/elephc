//! Purpose:
//! Emits ReflectionClass flag queries for both supported architectures.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Abstract, interface, trait, enum, and readonly flags retain their bit layout.

use super::*;

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
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the stored class-name length
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested class-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested class-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_class_flags_skip");      // length mismatch means this row belongs to another class
    emitter.instruction("str x11, [sp, #32]");                                  // save the row index across the class-name compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested class-name length
    emitter.instruction("ldr x3, [x10]");                                       // pass the stored class-name pointer
    emitter.instruction("mov x4, x12");                                         // pass the stored class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #32]");                                  // restore the row index after the class-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested class name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_class_flags_skip");      // class mismatch means scanning must continue
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the matched class-flag metadata row
    emitter.instruction("ldr x0, [x10, #16]");                                  // return the row's ReflectionClass predicate flags
    emitter.instruction("b __elephc_eval_reflection_class_flags_done");         // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_class_flags_skip");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current class-flag metadata row
    emitter.instruction("add x10, x10, #56");                                   // advance to the next 56-byte class metadata row
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

/// Emits the ARM64 eval hook that returns the declaring file of a matched AOT class-like.
pub(super) fn emit_aarch64_eval_reflection_class_source_file(emitter: &mut Emitter) {
    let string_symbol = emitter.target.extern_symbol("__elephc_eval_value_string");
    label_c_global(emitter, "__elephc_eval_reflection_class_source_file");
    emitter.instruction("sub sp, sp, #64");                                     // reserve class-file scan state across string comparisons
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable class-file scan frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    abi::emit_symbol_address(emitter, "x9", "_eval_reflection_class_count");
    emitter.instruction("ldr x9, [x9]");                                        // load the class metadata row count
    emitter.instruction("cbz x9, __elephc_eval_reflection_class_source_file_miss"); // an empty table has no source-file rows
    emitter.instruction("str x9, [sp, #16]");                                   // save the class metadata row count
    abi::emit_symbol_address(emitter, "x10", "_eval_reflection_classes");
    emitter.instruction("str x10, [sp, #24]");                                  // save the current class metadata row
    emitter.instruction("mov x11, #0");                                         // start scanning at class metadata row zero
    emitter.label("__elephc_eval_reflection_class_source_file_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the class metadata row count
    emitter.instruction("cmp x11, x9");                                         // have all class metadata rows been scanned?
    emitter.instruction("b.ge __elephc_eval_reflection_class_source_file_miss"); // no row matched before the end of the table
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current class metadata row
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the stored class-name length
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested class-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested class-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_class_source_file_skip"); // length mismatch means this row belongs to another class
    emitter.instruction("str x11, [sp, #32]");                                  // save the row index across the class-name compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested class-name length
    emitter.instruction("ldr x3, [x10]");                                       // pass the stored canonical class-name pointer
    emitter.instruction("mov x4, x12");                                         // pass the stored canonical class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #32]");                                  // restore the row index after the class-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested class name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_class_source_file_skip"); // class mismatch means scanning must continue
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the matched class metadata row
    emitter.instruction("ldr x1, [x10, #48]");                                  // load the matched declaration path length
    emitter.instruction("cbz x1, __elephc_eval_reflection_class_source_file_miss"); // no path marks an internal class
    emitter.instruction("ldr x0, [x10, #40]");                                  // load the matched declaration path pointer
    emitter.instruction(&format!("bl {string_symbol}"));                        // box the declaring file path for Rust
    emitter.instruction("b __elephc_eval_reflection_class_source_file_done");   // restore the wrapper frame after boxing
    emitter.label("__elephc_eval_reflection_class_source_file_skip");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current class metadata row
    emitter.instruction("add x10, x10, #56");                                   // advance to the next 56-byte class metadata row
    emitter.instruction("str x10, [sp, #24]");                                  // persist the advanced row cursor
    emitter.instruction("add x11, x11, #1");                                    // advance the row index
    emitter.instruction("b __elephc_eval_reflection_class_source_file_loop");   // continue scanning class metadata rows
    emitter.label("__elephc_eval_reflection_class_source_file_miss");
    emitter.instruction("mov x0, xzr");                                         // return null for unknown and internal class-like names
    emitter.label("__elephc_eval_reflection_class_source_file_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the class-file metadata scan frame
    emitter.instruction("ret");                                                 // return the declaring file, or null for a miss
}

/// Emits the ARM64 eval hook that returns the canonical name of a matched AOT class-like.
pub(super) fn emit_aarch64_eval_reflection_class_name(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_reflection_class_name");
    emitter.instruction("sub sp, sp, #64");                                     // reserve class-name scan state across string comparisons
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable class-name scan frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    abi::emit_symbol_address(emitter, "x9", "_eval_reflection_class_count");
    emitter.instruction("ldr x9, [x9]");                                        // load the class metadata row count
    emitter.instruction("cbz x9, __elephc_eval_reflection_class_name_miss");    // an empty table cannot contain a canonical name
    emitter.instruction("str x9, [sp, #16]");                                   // save the class metadata row count
    abi::emit_symbol_address(emitter, "x10", "_eval_reflection_classes");
    emitter.instruction("str x10, [sp, #24]");                                  // save the current class metadata row
    emitter.instruction("mov x11, #0");                                         // start scanning at class metadata row zero
    emitter.label("__elephc_eval_reflection_class_name_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the class metadata row count
    emitter.instruction("cmp x11, x9");                                         // have all class metadata rows been scanned?
    emitter.instruction("b.ge __elephc_eval_reflection_class_name_miss");       // no row matched before the end of the table
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current class metadata row
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the stored canonical class-name length
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested class-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested class-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_class_name_skip");       // length mismatch means this row belongs to another class
    emitter.instruction("str x11, [sp, #32]");                                  // save the row index across the class-name compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested class-name length
    emitter.instruction("ldr x3, [x10]");                                       // pass the stored canonical class-name pointer
    emitter.instruction("mov x4, x12");                                         // pass the stored canonical class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #32]");                                  // restore the row index after the class-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested class name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_class_name_skip");       // class mismatch means scanning must continue
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the matched class metadata row
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("ldr x1, [x10]");                                       // load the canonical class-name pointer
    emitter.instruction("ldr x2, [x10, #8]");                                   // load the canonical class-name length
    emitter.instruction("bl __rt_mixed_from_value");                            // box the canonical class name for Rust
    emitter.instruction("b __elephc_eval_reflection_class_name_done");          // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_class_name_skip");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current class metadata row
    emitter.instruction("add x10, x10, #56");                                   // advance to the next 56-byte class metadata row
    emitter.instruction("str x10, [sp, #24]");                                  // persist the advanced row cursor
    emitter.instruction("add x11, x11, #1");                                    // advance the row index
    emitter.instruction("b __elephc_eval_reflection_class_name_loop");          // continue scanning class metadata rows
    emitter.label("__elephc_eval_reflection_class_name_miss");
    emitter.instruction("mov x0, xzr");                                         // return null when no AOT class metadata matched
    emitter.label("__elephc_eval_reflection_class_name_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the class metadata scan frame
    emitter.instruction("ret");                                                 // return the canonical class name, or null for a miss
}

/// Emits the ARM64 eval hook that returns an AOT class doc comment when present.
pub(super) fn emit_aarch64_eval_reflection_class_doc_comment(emitter: &mut Emitter) {
    let string_symbol = emitter.target.extern_symbol("__elephc_eval_value_string");
    label_c_global(emitter, "__elephc_eval_reflection_class_doc_comment");
    emitter.instruction("sub sp, sp, #64");                                     // reserve class-name scan state across helper calls
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address across runtime calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable class-doc lookup frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    abi::emit_symbol_address(emitter, "x9", "_eval_reflection_class_count");
    emitter.instruction("ldr x9, [x9]");                                        // load the generated class metadata row count
    emitter.instruction("cbz x9, __elephc_eval_reflection_class_doc_comment_miss"); // an empty table has no doc comments
    emitter.instruction("str x9, [sp, #16]");                                   // save the metadata row count across string comparisons
    abi::emit_symbol_address(emitter, "x10", "_eval_reflection_classes");
    emitter.instruction("str x10, [sp, #24]");                                  // save the current class metadata row
    emitter.instruction("mov x11, #0");                                         // start scanning at class metadata row zero
    emitter.label("__elephc_eval_reflection_class_doc_comment_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the metadata row count
    emitter.instruction("cmp x11, x9");                                         // have all class metadata rows been scanned?
    emitter.instruction("b.ge __elephc_eval_reflection_class_doc_comment_miss"); // no matching documented class remains
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current class metadata row
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the stored class-name length
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested class-name length
    emitter.instruction("cmp x12, x2");                                         // compare class-name lengths before bytes
    emitter.instruction("b.ne __elephc_eval_reflection_class_doc_comment_skip"); // a length mismatch cannot match
    emitter.instruction("str x11, [sp, #32]");                                  // preserve the row index across case-insensitive compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested class-name length
    emitter.instruction("ldr x3, [x10]");                                       // pass the stored canonical class-name pointer
    emitter.instruction("mov x4, x12");                                         // pass the stored canonical class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // apply PHP class-name case-insensitive comparison
    emitter.instruction("ldr x11, [sp, #32]");                                  // restore the row index after comparison
    emitter.instruction("cmp x0, #0");                                          // did the requested class name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_class_doc_comment_skip"); // continue scanning after a mismatch
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the matching metadata row
    emitter.instruction("ldr x1, [x10, #32]");                                  // load the optional doc-comment byte length
    emitter.instruction("cbz x1, __elephc_eval_reflection_class_doc_comment_miss"); // an undocumented class reports no metadata
    emitter.instruction("ldr x0, [x10, #24]");                                  // load the retained doc-comment bytes
    emitter.instruction(&format!("bl {string_symbol}"));                        // box the borrowed static bytes for Magician
    emitter.instruction("b __elephc_eval_reflection_class_doc_comment_done");   // restore the wrapper frame after boxing
    emitter.label("__elephc_eval_reflection_class_doc_comment_skip");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current class metadata row
    emitter.instruction("add x10, x10, #56");                                   // advance to the next 56-byte class metadata row
    emitter.instruction("str x10, [sp, #24]");                                  // persist the advanced row cursor
    emitter.instruction("add x11, x11, #1");                                    // advance the row index
    emitter.instruction("b __elephc_eval_reflection_class_doc_comment_loop");   // continue scanning class metadata
    emitter.label("__elephc_eval_reflection_class_doc_comment_miss");
    emitter.instruction("mov x0, xzr");                                         // return null when no AOT doc comment exists
    emitter.label("__elephc_eval_reflection_class_doc_comment_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #64");                                     // release class-doc lookup storage
    emitter.instruction("ret");                                                 // return the boxed doc comment or null
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
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load the stored class-name length
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare stored and requested class-name lengths
    emitter.instruction("jne __elephc_eval_reflection_class_flags_skip_x86");   // length mismatch means this row belongs to another class
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the row index across the class-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested class-name length
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // pass the stored class-name pointer
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // restore the row index after the class-name compare
    emitter.instruction("test rax, rax");                                       // did the requested class name match this row?
    emitter.instruction("jne __elephc_eval_reflection_class_flags_skip_x86");   // class mismatch means scanning must continue
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the matched class-flag metadata row
    emitter.instruction("mov rax, QWORD PTR [r10 + 16]");                       // return the row's ReflectionClass predicate flags
    emitter.instruction("jmp __elephc_eval_reflection_class_flags_done_x86");   // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_class_flags_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current class-flag metadata row
    emitter.instruction("add r10, 56");                                         // advance to the next 56-byte class metadata row
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

/// Emits the x86_64 eval hook that returns the declaring file of a matched AOT class-like.
pub(super) fn emit_x86_64_eval_reflection_class_source_file(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_reflection_class_source_file");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable class-file scan frame
    emitter.instruction("sub rsp, 48");                                         // reserve class-name, count, cursor, and index slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    abi::emit_symbol_address(emitter, "r10", "_eval_reflection_class_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the class metadata row count
    emitter.instruction("test r10, r10");                                       // is the class metadata table empty?
    emitter.instruction("jz __elephc_eval_reflection_class_source_file_miss_x86"); // an empty table has no source-file rows
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the class metadata row count
    abi::emit_symbol_address(emitter, "r11", "_eval_reflection_classes");
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the current class metadata row
    emitter.instruction("xor r11d, r11d");                                      // start scanning at class metadata row zero
    emitter.label("__elephc_eval_reflection_class_source_file_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the class metadata row count
    emitter.instruction("cmp r11, r10");                                        // have all class metadata rows been scanned?
    emitter.instruction("jae __elephc_eval_reflection_class_source_file_miss_x86"); // no row matched before the end of the table
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current class metadata row
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load the stored class-name length
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare stored and requested class-name lengths
    emitter.instruction("jne __elephc_eval_reflection_class_source_file_skip_x86"); // length mismatch means another class
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the row index across the class-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested class-name length
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // pass the stored canonical class-name pointer
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // restore the row index after the class-name compare
    emitter.instruction("test rax, rax");                                       // did the requested class name match this row?
    emitter.instruction("jne __elephc_eval_reflection_class_source_file_skip_x86"); // class mismatch means keep scanning
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the matched class metadata row
    emitter.instruction("mov rsi, QWORD PTR [r10 + 48]");                       // load the matched declaration path length
    emitter.instruction("test rsi, rsi");                                       // does this class-like have a source file?
    emitter.instruction("jz __elephc_eval_reflection_class_source_file_miss_x86"); // internal class-likes have no file
    emitter.instruction("mov rdi, QWORD PTR [r10 + 40]");                       // load the matched declaration path pointer
    emitter.instruction("call __elephc_eval_value_string");                     // box the declaring file path for Rust
    emitter.instruction("jmp __elephc_eval_reflection_class_source_file_done_x86"); // restore the wrapper frame after boxing
    emitter.label("__elephc_eval_reflection_class_source_file_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current class metadata row
    emitter.instruction("add r10, 56");                                         // advance to the next 56-byte class metadata row
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the advanced row cursor
    emitter.instruction("inc r11");                                             // advance the row index
    emitter.instruction("jmp __elephc_eval_reflection_class_source_file_loop_x86"); // continue scanning class metadata rows
    emitter.label("__elephc_eval_reflection_class_source_file_miss_x86");
    emitter.instruction("xor eax, eax");                                        // return null for unknown and internal class-like names
    emitter.label("__elephc_eval_reflection_class_source_file_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the declaring file, or null for a miss
}

/// Emits the x86_64 eval hook that returns the canonical name of a matched AOT class-like.
pub(super) fn emit_x86_64_eval_reflection_class_name(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_reflection_class_name");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable class-name scan frame
    emitter.instruction("sub rsp, 48");                                         // reserve class-name, count, cursor, and index slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    abi::emit_symbol_address(emitter, "r10", "_eval_reflection_class_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the class metadata row count
    emitter.instruction("test r10, r10");                                       // is the class metadata table empty?
    emitter.instruction("jz __elephc_eval_reflection_class_name_miss_x86");     // an empty table cannot contain a canonical name
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the class metadata row count
    abi::emit_symbol_address(emitter, "r11", "_eval_reflection_classes");
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the current class metadata row
    emitter.instruction("xor r11d, r11d");                                      // start scanning at class metadata row zero
    emitter.label("__elephc_eval_reflection_class_name_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the class metadata row count
    emitter.instruction("cmp r11, r10");                                        // have all class metadata rows been scanned?
    emitter.instruction("jae __elephc_eval_reflection_class_name_miss_x86");    // no row matched before the end of the table
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current class metadata row
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load the stored canonical class-name length
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare stored and requested class-name lengths
    emitter.instruction("jne __elephc_eval_reflection_class_name_skip_x86");    // length mismatch means this row belongs to another class
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the row index across the class-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested class-name length
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // pass the stored canonical class-name pointer
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // restore the row index after the class-name compare
    emitter.instruction("test rax, rax");                                       // did the requested class name match this row?
    emitter.instruction("jne __elephc_eval_reflection_class_name_skip_x86");    // class mismatch means scanning must continue
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the matched class metadata row
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // load the canonical class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // load the canonical class-name length
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // box the canonical class name for Rust
    emitter.instruction("jmp __elephc_eval_reflection_class_name_done_x86");    // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_class_name_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current class metadata row
    emitter.instruction("add r10, 56");                                         // advance to the next 56-byte class metadata row
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the advanced row cursor
    emitter.instruction("inc r11");                                             // advance the row index
    emitter.instruction("jmp __elephc_eval_reflection_class_name_loop_x86");    // continue scanning class metadata rows
    emitter.label("__elephc_eval_reflection_class_name_miss_x86");
    emitter.instruction("xor eax, eax");                                        // return null when no AOT class metadata matched
    emitter.label("__elephc_eval_reflection_class_name_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the canonical class name, or null for a miss
}

/// Emits the x86_64 eval hook that returns an AOT class doc comment when present.
pub(super) fn emit_x86_64_eval_reflection_class_doc_comment(emitter: &mut Emitter) {
    let string_symbol = emitter.target.extern_symbol("__elephc_eval_value_string");
    label_c_global(emitter, "__elephc_eval_reflection_class_doc_comment");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable class-doc lookup frame
    emitter.instruction("sub rsp, 48");                                         // reserve class-name, count, cursor, and index slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    abi::emit_symbol_address(emitter, "r10", "_eval_reflection_class_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the generated class metadata row count
    emitter.instruction("test r10, r10");                                       // is the metadata table empty?
    emitter.instruction("jz __elephc_eval_reflection_class_doc_comment_miss_x86"); // an empty table has no doc comments
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the metadata row count across string comparisons
    abi::emit_symbol_address(emitter, "r11", "_eval_reflection_classes");
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the current class metadata row
    emitter.instruction("xor r11d, r11d");                                      // start scanning at class metadata row zero
    emitter.label("__elephc_eval_reflection_class_doc_comment_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the metadata row count
    emitter.instruction("cmp r11, r10");                                        // have all class metadata rows been scanned?
    emitter.instruction("jae __elephc_eval_reflection_class_doc_comment_miss_x86"); // no matching documented class remains
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current class metadata row
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load the stored class-name length
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare class-name lengths before bytes
    emitter.instruction("jne __elephc_eval_reflection_class_doc_comment_skip_x86"); // a length mismatch cannot match
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // preserve the row index across case-insensitive compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested class-name length
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // pass the stored canonical class-name pointer
    emitter.instruction("call __rt_strcasecmp");                                // apply PHP class-name case-insensitive comparison
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // restore the row index after comparison
    emitter.instruction("test rax, rax");                                       // did the requested class name match this row?
    emitter.instruction("jne __elephc_eval_reflection_class_doc_comment_skip_x86"); // continue scanning after a mismatch
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the matching metadata row
    emitter.instruction("mov rsi, QWORD PTR [r10 + 32]");                       // load the optional doc-comment byte length
    emitter.instruction("test rsi, rsi");                                       // does this class retain a doc comment?
    emitter.instruction("jz __elephc_eval_reflection_class_doc_comment_miss_x86"); // undocumented classes report no metadata
    emitter.instruction("mov rdi, QWORD PTR [r10 + 24]");                       // load the retained doc-comment bytes
    emitter.instruction(&format!("call {string_symbol}"));                      // box the borrowed static bytes for Magician
    emitter.instruction("jmp __elephc_eval_reflection_class_doc_comment_done_x86"); // restore the wrapper frame after boxing
    emitter.label("__elephc_eval_reflection_class_doc_comment_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current class metadata row
    emitter.instruction("add r10, 56");                                         // advance to the next 56-byte class metadata row
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the advanced row cursor
    emitter.instruction("inc r11");                                             // advance the row index
    emitter.instruction("jmp __elephc_eval_reflection_class_doc_comment_loop_x86"); // continue scanning class metadata
    emitter.label("__elephc_eval_reflection_class_doc_comment_miss_x86");
    emitter.instruction("xor eax, eax");                                        // return null when no AOT doc comment exists
    emitter.label("__elephc_eval_reflection_class_doc_comment_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // release class-doc lookup storage
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame
    emitter.instruction("ret");                                                 // return the boxed doc comment or null
}
