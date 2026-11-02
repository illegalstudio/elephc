//! Purpose:
//! Emits AArch64 reflection name and source-file scanners.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Class-filtered tables retain their dense metadata layout.

use super::*;

/// Emits the ARM64 eval hook that returns AOT ReflectionMethod names.
pub(super) fn emit_aarch64_eval_reflection_method_names(emitter: &mut Emitter) {
    emit_aarch64_eval_reflection_member_names(
        emitter,
        "__elephc_eval_reflection_method_names",
        "_eval_reflection_method_count",
        "_eval_reflection_methods",
        "__elephc_eval_reflection_method_names",
        "method",
        56,
    );
}

/// Emits the ARM64 eval hook that returns AOT ReflectionProperty names.
pub(super) fn emit_aarch64_eval_reflection_property_names(emitter: &mut Emitter) {
    emit_aarch64_eval_reflection_member_names(
        emitter,
        "__elephc_eval_reflection_property_names",
        "_eval_reflection_property_count",
        "_eval_reflection_properties",
        "__elephc_eval_reflection_property_names",
        "property",
        56,
    );
}

/// Emits the ARM64 eval hook that returns AOT ReflectionClass interface names.
pub(super) fn emit_aarch64_eval_reflection_class_interface_names(emitter: &mut Emitter) {
    emit_aarch64_eval_reflection_member_names(
        emitter,
        "__elephc_eval_reflection_class_interface_names",
        "_eval_reflection_class_interface_count",
        "_eval_reflection_class_interfaces",
        "__elephc_eval_reflection_class_interface_names",
        "class interface",
        32,
    );
}

/// Emits the ARM64 eval hook that returns AOT `class_uses()` trait names.
pub(super) fn emit_aarch64_eval_reflection_class_trait_names(emitter: &mut Emitter) {
    emit_aarch64_eval_reflection_member_names(
        emitter,
        "__elephc_eval_reflection_class_trait_names",
        "_eval_reflection_class_trait_count",
        "_eval_reflection_class_traits",
        "__elephc_eval_reflection_class_trait_names",
        "class trait",
        32,
    );
}

/// Emits the ARM64 eval hook that returns AOT ReflectionClass trait alias names.
pub(super) fn emit_aarch64_eval_reflection_class_trait_alias_names(emitter: &mut Emitter) {
    emit_aarch64_eval_reflection_member_names(
        emitter,
        "__elephc_eval_reflection_class_trait_alias_names",
        "_eval_reflection_class_trait_alias_count",
        "_eval_reflection_class_trait_aliases",
        "__elephc_eval_reflection_class_trait_alias_names",
        "class trait alias",
        32,
    );
}

/// Emits the ARM64 eval hook that returns AOT ReflectionClass trait alias sources.
pub(super) fn emit_aarch64_eval_reflection_class_trait_alias_sources(emitter: &mut Emitter) {
    emit_aarch64_eval_reflection_member_names(
        emitter,
        "__elephc_eval_reflection_class_trait_alias_sources",
        "_eval_reflection_class_trait_alias_count",
        "_eval_reflection_class_trait_alias_sources",
        "__elephc_eval_reflection_class_trait_alias_sources",
        "class trait alias source",
        32,
    );
}

/// Emits the ARM64 eval hook that returns the AOT reflection source file.
pub(super) fn emit_aarch64_eval_reflection_source_file(emitter: &mut Emitter) {
    let string_symbol = emitter.target.extern_symbol("__elephc_eval_value_string");
    label_c_global(emitter, "__elephc_eval_reflection_source_file");
    abi::emit_symbol_address(emitter, "x9", "_eval_reflection_source_file_len");
    emitter.instruction("ldr x1, [x9]");                                        // load the generated source-file length
    emitter.instruction("cbz x1, __elephc_eval_reflection_source_file_miss");   // report no source file when EIR metadata is absent
    abi::emit_symbol_address(emitter, "x0", "_eval_reflection_source_file");
    emitter.instruction(&format!("b {string_symbol}"));                         // box the generated source-file path for Rust
    emitter.label("__elephc_eval_reflection_source_file_miss");
    emitter.instruction("mov x0, xzr");                                         // return null when no source file is available
    emitter.instruction("ret");                                                 // finish the source-file metadata lookup
}

/// Emits an ARM64 class-filtered AOT reflection member-name scanner.
pub(super) fn emit_aarch64_eval_reflection_member_names(
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
    emitter.instruction("sub sp, sp, #112");                                    // reserve scan state across allocation and string comparisons
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #96");                                    // establish a stable member-name scan frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    abi::emit_symbol_address(emitter, "x9", count_symbol);
    emitter.instruction("ldr x9, [x9]");                                        // load the AOT reflection member row count
    emitter.instruction("str x9, [sp, #16]");                                   // save the table count across helper calls
    emitter.instruction("mov x0, #4");                                          // start from the wrapper's small minimum and grow only for matching members
    emitter.instruction(&format!("bl {string_array_new_symbol}"));              // allocate the boxed result string array
    emitter.instruction(&format!("cbz x0, {miss_label}"));                      // allocation failure reports a null pointer to Rust
    emitter.instruction("str x0, [sp, #24]");                                   // save the boxed result string array
    abi::emit_symbol_address(emitter, "x10", table_symbol);
    emitter.instruction("str x10, [sp, #32]");                                  // save the current member metadata row
    emitter.instruction("mov x11, #0");                                         // start scanning at member metadata row zero
    emitter.label(&loop_label);
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the member metadata row count
    emitter.instruction("cmp x11, x9");                                         // have all member metadata rows been scanned?
    emitter.instruction(&format!("b.ge {done_label}"));                         // return the accumulated names after the final row
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the current member metadata row
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the stored class-name length
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested class-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested class-name lengths
    emitter.instruction(&format!("b.ne {skip_label}"));                         // length mismatch means this row belongs to another class
    emitter.instruction("str x11, [sp, #40]");                                  // save the row index across the class-name compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested class-name length
    emitter.instruction("ldr x3, [x10]");                                       // pass the stored class-name pointer
    emitter.instruction("mov x4, x12");                                         // pass the stored class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #40]");                                  // restore the row index after the class-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested class name match this row?
    emitter.instruction(&format!("b.ne {skip_label}"));                         // class mismatch means scanning must continue
    emitter.instruction("str x11, [sp, #40]");                                  // save the row index across appending the member name
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed result string array
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the matched member metadata row
    emitter.instruction("ldr x1, [x10, #16]");                                  // pass the stored member-name pointer
    emitter.instruction("ldr x2, [x10, #24]");                                  // pass the stored member-name length
    emitter.instruction(&format!("bl {string_array_push_symbol}"));             // append the matched member name to the result array
    emitter.instruction(&format!("cbz x0, {miss_label}"));                      // malformed append state reports a null pointer to Rust
    emitter.instruction("str x0, [sp, #24]");                                   // save the updated boxed result string array
    emitter.instruction("ldr x11, [sp, #40]");                                  // restore the row index after appending the member name
    emitter.label(&skip_label);
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the current member metadata row
    emitter.instruction(&format!("add x10, x10, #{row_stride}"));               // advance to the next reflection metadata row
    emitter.instruction("str x10, [sp, #32]");                                  // persist the advanced row cursor
    emitter.instruction("add x11, x11, #1");                                    // advance the row index
    emitter.instruction(&format!("b {loop_label}"));                            // continue scanning member metadata rows
    emitter.label(&done_label);
    emitter.instruction("ldr x0, [sp, #24]");                                   // return the boxed result string array
    emitter.instruction(&format!("b {label_prefix}_ret"));                      // share the frame teardown path
    emitter.label(&miss_label);
    emitter.instruction("mov x0, xzr");                                         // return a null pointer when allocation or append failed
    emitter.label(&format!("{label_prefix}_ret"));
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the member-name scan frame
    emitter.instruction("ret");                                                 // return the boxed name array, or null on failure, to Rust
    emitter.comment(&format!("--- end eval reflection {member_kind} names ---"));
}
