//! Purpose:
//! Emits x86_64 reflection name and source-file scanners.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Class-filtered tables retain their dense metadata layout.

use super::*;

/// Emits the x86_64 eval hook that returns AOT ReflectionMethod names.
pub(super) fn emit_x86_64_eval_reflection_method_names(emitter: &mut Emitter) {
    emit_x86_64_eval_reflection_member_names(
        emitter,
        "__elephc_eval_reflection_method_names",
        "_eval_reflection_method_count",
        "_eval_reflection_methods",
        "__elephc_eval_reflection_method_names_x86",
        "method",
        56,
    );
}

/// Emits the x86_64 eval hook that returns AOT ReflectionProperty names.
pub(super) fn emit_x86_64_eval_reflection_property_names(emitter: &mut Emitter) {
    emit_x86_64_eval_reflection_member_names(
        emitter,
        "__elephc_eval_reflection_property_names",
        "_eval_reflection_property_count",
        "_eval_reflection_properties",
        "__elephc_eval_reflection_property_names_x86",
        "property",
        56,
    );
}

/// Emits the x86_64 eval hook that returns AOT ReflectionClass interface names.
pub(super) fn emit_x86_64_eval_reflection_class_interface_names(emitter: &mut Emitter) {
    emit_x86_64_eval_reflection_member_names(
        emitter,
        "__elephc_eval_reflection_class_interface_names",
        "_eval_reflection_class_interface_count",
        "_eval_reflection_class_interfaces",
        "__elephc_eval_reflection_class_interface_names_x86",
        "class interface",
        32,
    );
}

/// Emits the x86_64 eval hook that returns AOT `class_uses()` trait names.
pub(super) fn emit_x86_64_eval_reflection_class_trait_names(emitter: &mut Emitter) {
    emit_x86_64_eval_reflection_member_names(
        emitter,
        "__elephc_eval_reflection_class_trait_names",
        "_eval_reflection_class_trait_count",
        "_eval_reflection_class_traits",
        "__elephc_eval_reflection_class_trait_names_x86",
        "class trait",
        32,
    );
}

/// Emits the x86_64 eval hook that returns AOT ReflectionClass trait alias names.
pub(super) fn emit_x86_64_eval_reflection_class_trait_alias_names(emitter: &mut Emitter) {
    emit_x86_64_eval_reflection_member_names(
        emitter,
        "__elephc_eval_reflection_class_trait_alias_names",
        "_eval_reflection_class_trait_alias_count",
        "_eval_reflection_class_trait_aliases",
        "__elephc_eval_reflection_class_trait_alias_names_x86",
        "class trait alias",
        32,
    );
}

/// Emits the x86_64 eval hook that returns AOT ReflectionClass trait alias sources.
pub(super) fn emit_x86_64_eval_reflection_class_trait_alias_sources(emitter: &mut Emitter) {
    emit_x86_64_eval_reflection_member_names(
        emitter,
        "__elephc_eval_reflection_class_trait_alias_sources",
        "_eval_reflection_class_trait_alias_count",
        "_eval_reflection_class_trait_alias_sources",
        "__elephc_eval_reflection_class_trait_alias_sources_x86",
        "class trait alias source",
        32,
    );
}

/// Emits the x86_64 eval hook that returns the AOT reflection source file.
pub(super) fn emit_x86_64_eval_reflection_source_file(emitter: &mut Emitter) {
    let string_symbol = emitter.target.extern_symbol("__elephc_eval_value_string");
    label_c_global(emitter, "__elephc_eval_reflection_source_file");
    abi::emit_symbol_address(emitter, "r10", "_eval_reflection_source_file_len");
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the generated source-file length
    emitter.instruction("test rsi, rsi");                                       // is source-file metadata available?
    emitter.instruction("jz __elephc_eval_reflection_source_file_miss_x86");    // report no source file when EIR metadata is absent
    abi::emit_symbol_address(emitter, "rdi", "_eval_reflection_source_file");
    emitter.instruction(&format!("jmp {string_symbol}"));                       // box the generated source-file path for Rust
    emitter.label("__elephc_eval_reflection_source_file_miss_x86");
    emitter.instruction("xor eax, eax");                                        // return null when no source file is available
    emitter.instruction("ret");                                                 // finish the source-file metadata lookup
}

/// Emits an x86_64 class-filtered AOT reflection member-name scanner.
pub(super) fn emit_x86_64_eval_reflection_member_names(
    emitter: &mut Emitter,
    symbol: &str,
    count_symbol: &str,
    table_symbol: &str,
    label_prefix: &str,
    member_kind: &str,
    row_stride: u64,
) {
    let loop_label = format!("{label_prefix}_loop");
    let skip_label = format!("{label_prefix}_skip");
    let miss_label = format!("{label_prefix}_miss");
    let done_label = format!("{label_prefix}_done");
    let string_array_new_symbol = emitter
        .target
        .extern_symbol("__elephc_eval_value_string_array_new");
    let string_array_push_symbol = emitter
        .target
        .extern_symbol("__elephc_eval_value_string_array_push");
    label_c_global(emitter, symbol);
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable member-name scan frame pointer
    emitter.instruction("sub rsp, 80");                                         // reserve scan state across allocation and string comparisons
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    abi::emit_symbol_address(emitter, "r10", count_symbol);
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the AOT reflection member row count
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the table count across helper calls
    emitter.instruction("mov rdi, 4");                                          // start from the wrapper's small minimum and grow only for matching members
    emitter.instruction(&format!("call {string_array_new_symbol}"));            // allocate the boxed result string array
    emitter.instruction(&format!("test rax, rax"));                             // did allocation return a usable boxed array?
    emitter.instruction(&format!("jz {miss_label}"));                           // allocation failure reports a null pointer to Rust
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the boxed result string array
    abi::emit_symbol_address(emitter, "r11", table_symbol);
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the current member metadata row
    emitter.instruction("xor r11d, r11d");                                      // start scanning at member metadata row zero
    emitter.label(&loop_label);
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the member metadata row count
    emitter.instruction("cmp r11, r10");                                        // have all member metadata rows been scanned?
    emitter.instruction(&format!("jae {done_label}"));                          // return the accumulated names after the final row
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current member metadata row
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load the stored class-name length
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare stored and requested class-name lengths
    emitter.instruction(&format!("jne {skip_label}"));                          // length mismatch means this row belongs to another class
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // save the row index across the class-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested class-name length
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // pass the stored class-name pointer
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // restore the row index after the class-name compare
    emitter.instruction("test rax, rax");                                       // did the requested class name match this row?
    emitter.instruction(&format!("jne {skip_label}"));                          // class mismatch means scanning must continue
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // save the row index across appending the member name
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the matched member metadata row
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the boxed result string array
    emitter.instruction("mov rsi, QWORD PTR [r10 + 16]");                       // pass the stored member-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 24]");                       // pass the stored member-name length
    emitter.instruction(&format!("call {string_array_push_symbol}"));           // append the matched member name to the result array
    emitter.instruction("test rax, rax");                                       // did append return a usable boxed array?
    emitter.instruction(&format!("jz {miss_label}"));                           // malformed append state reports a null pointer to Rust
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the updated boxed result string array
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // restore the row index after appending the member name
    emitter.label(&skip_label);
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current member metadata row
    emitter.instruction(&format!("add r10, {row_stride}"));                     // advance to the next reflection metadata row
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // persist the advanced row cursor
    emitter.instruction("inc r11");                                             // advance the row index
    emitter.instruction(&format!("jmp {loop_label}"));                          // continue scanning member metadata rows
    emitter.label(&done_label);
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the boxed result string array
    emitter.instruction(&format!("jmp {label_prefix}_ret"));                    // share the frame teardown path
    emitter.label(&miss_label);
    emitter.instruction("xor eax, eax");                                        // return a null pointer when allocation or append failed
    emitter.label(&format!("{label_prefix}_ret"));
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed name array, or null on failure, to Rust
    emitter.comment(&format!("--- end eval reflection {member_kind} names ---"));
}
