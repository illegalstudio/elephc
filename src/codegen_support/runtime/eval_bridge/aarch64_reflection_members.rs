//! Purpose:
//! Emits AArch64 reflection flags and declaring-class lookups.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Method and property metadata use the existing table strides.

use super::*;
use elephc_memoir::values::reflection_metadata::{self, MemberField};

/// Emits the ARM64 eval hook that returns AOT ReflectionMethod predicate flags.
pub(super) fn emit_aarch64_eval_reflection_method_flags(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_reflection_method_flags");
    emitter.instruction("sub sp, sp, #96");                                     // reserve scan state across runtime string comparisons
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #80");                                    // establish a stable scan frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    emitter.instruction("str x2, [sp, #16]");                                   // save the requested method-name pointer
    emitter.instruction("str x3, [sp, #24]");                                   // save the requested method-name length
    abi::emit_symbol_address(emitter, "x9", "_eval_reflection_method_count");
    emitter.instruction("ldr x9, [x9]");                                        // load the AOT reflection-method row count
    emitter.instruction("cbz x9, __elephc_eval_reflection_method_flags_miss");  // an empty table cannot contain the requested method
    emitter.instruction("str x9, [sp, #32]");                                   // save the table count across string comparisons
    abi::emit_symbol_address(emitter, "x10", "_eval_reflection_methods");
    emitter.instruction("str x10, [sp, #40]");                                  // save the current method metadata row
    emitter.instruction("mov x11, #0");                                         // start scanning at method metadata row zero
    emitter.label("__elephc_eval_reflection_method_flags_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the method metadata row count
    emitter.instruction("cmp x11, x9");                                         // have all method metadata rows been scanned?
    emitter.instruction("b.ge __elephc_eval_reflection_method_flags_miss");     // no row matched before the end of the table
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current method metadata row
    reflection_metadata::emit_load_member_field(emitter, "x12", "x10", MemberField::ClassNameLength);
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested class-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested class-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_method_flags_skip");     // length mismatch means the class cannot match
    emitter.instruction("str x11, [sp, #48]");                                  // save the row index across the class-name compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested class-name length
    reflection_metadata::emit_load_member_field(emitter, "x3", "x10", MemberField::ClassName);
    emitter.instruction("mov x4, x12");                                         // pass the stored class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #48]");                                  // restore the row index after the class-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested class name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_method_flags_skip");     // class mismatch means the row cannot match
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current row for the method-name compare
    reflection_metadata::emit_load_member_field(emitter, "x12", "x10", MemberField::MemberNameLength);
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload the requested method-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested method-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_method_flags_skip");     // length mismatch means the method cannot match
    emitter.instruction("str x11, [sp, #48]");                                  // save the row index across the method-name compare
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the requested method-name pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass the requested method-name length
    reflection_metadata::emit_load_member_field(emitter, "x3", "x10", MemberField::MemberName);
    emitter.instruction("mov x4, x12");                                         // pass the stored method-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare method names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #48]");                                  // restore the row index after the method-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested method name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_method_flags_skip");     // method mismatch means scanning must continue
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the matched method metadata row
    reflection_metadata::emit_load_member_field(emitter, "x0", "x10", MemberField::Flags);
    emitter.instruction("b __elephc_eval_reflection_method_flags_done");        // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_method_flags_skip");
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current method metadata row
    reflection_metadata::emit_advance_member_row(emitter, "x10");
    emitter.instruction("str x10, [sp, #40]");                                  // persist the advanced row cursor
    emitter.instruction("add x11, x11, #1");                                    // advance the row index
    emitter.instruction("b __elephc_eval_reflection_method_flags_loop");        // continue scanning method metadata rows
    emitter.label("__elephc_eval_reflection_method_flags_miss");
    emitter.instruction("mov x0, #0");                                          // return zero when no AOT method metadata matched
    emitter.label("__elephc_eval_reflection_method_flags_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the method metadata scan frame
    emitter.instruction("ret");                                                 // return flags, or zero for a miss, to Rust
}

/// Emits the ARM64 eval hook that returns a matched AOT ReflectionMethod declaring class.
pub(super) fn emit_aarch64_eval_reflection_method_declaring_class(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_reflection_method_declaring_class");
    emitter.instruction("sub sp, sp, #96");                                     // reserve scan state across runtime string comparisons
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #80");                                    // establish a stable scan frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    emitter.instruction("str x2, [sp, #16]");                                   // save the requested method-name pointer
    emitter.instruction("str x3, [sp, #24]");                                   // save the requested method-name length
    abi::emit_symbol_address(emitter, "x9", "_eval_reflection_method_count");
    emitter.instruction("ldr x9, [x9]");                                        // load the AOT reflection-method row count
    emitter.instruction("cbz x9, __elephc_eval_reflection_method_declaring_class_miss"); // an empty table cannot contain the requested method
    emitter.instruction("str x9, [sp, #32]");                                   // save the table count across string comparisons
    abi::emit_symbol_address(emitter, "x10", "_eval_reflection_methods");
    emitter.instruction("str x10, [sp, #40]");                                  // save the current method metadata row
    emitter.instruction("mov x11, #0");                                         // start scanning at method metadata row zero
    emitter.label("__elephc_eval_reflection_method_declaring_class_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the method metadata row count
    emitter.instruction("cmp x11, x9");                                         // have all method metadata rows been scanned?
    emitter.instruction("b.ge __elephc_eval_reflection_method_declaring_class_miss"); // no row matched before the end of the table
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current method metadata row
    reflection_metadata::emit_load_member_field(emitter, "x12", "x10", MemberField::ClassNameLength);
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested class-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested class-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_method_declaring_class_skip"); // length mismatch means the class cannot match
    emitter.instruction("str x11, [sp, #48]");                                  // save the row index across the class-name compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested class-name length
    reflection_metadata::emit_load_member_field(emitter, "x3", "x10", MemberField::ClassName);
    emitter.instruction("mov x4, x12");                                         // pass the stored class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #48]");                                  // restore the row index after the class-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested class name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_method_declaring_class_skip"); // class mismatch means the row cannot match
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current row for the method-name compare
    reflection_metadata::emit_load_member_field(emitter, "x12", "x10", MemberField::MemberNameLength);
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload the requested method-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested method-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_method_declaring_class_skip"); // length mismatch means the method cannot match
    emitter.instruction("str x11, [sp, #48]");                                  // save the row index across the method-name compare
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the requested method-name pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass the requested method-name length
    reflection_metadata::emit_load_member_field(emitter, "x3", "x10", MemberField::MemberName);
    emitter.instruction("mov x4, x12");                                         // pass the stored method-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare method names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #48]");                                  // restore the row index after the method-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested method name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_method_declaring_class_skip"); // method mismatch means scanning must continue
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the matched method metadata row
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    reflection_metadata::emit_load_member_field(emitter, "x1", "x10", MemberField::DeclaringClass);
    reflection_metadata::emit_load_member_field(emitter, "x2", "x10", MemberField::DeclaringClassLength);
    emitter.instruction("bl __rt_mixed_from_value");                            // box the declaring class name for Rust
    emitter.instruction("b __elephc_eval_reflection_method_declaring_class_done"); // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_method_declaring_class_skip");
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current method metadata row
    reflection_metadata::emit_advance_member_row(emitter, "x10");
    emitter.instruction("str x10, [sp, #40]");                                  // persist the advanced row cursor
    emitter.instruction("add x11, x11, #1");                                    // advance the row index
    emitter.instruction("b __elephc_eval_reflection_method_declaring_class_loop"); // continue scanning method metadata rows
    emitter.label("__elephc_eval_reflection_method_declaring_class_miss");
    emitter.instruction("mov x0, xzr");                                         // return null when no AOT method metadata matched
    emitter.label("__elephc_eval_reflection_method_declaring_class_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the method metadata scan frame
    emitter.instruction("ret");                                                 // return the declaring class string, or null for a miss, to Rust
}

/// Emits the ARM64 eval hook that returns a matched AOT ReflectionProperty declaring class.
pub(super) fn emit_aarch64_eval_reflection_property_declaring_class(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_reflection_property_declaring_class");
    emitter.instruction("sub sp, sp, #96");                                     // reserve scan state across runtime string comparisons
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #80");                                    // establish a stable scan frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    emitter.instruction("str x2, [sp, #16]");                                   // save the requested property-name pointer
    emitter.instruction("str x3, [sp, #24]");                                   // save the requested property-name length
    abi::emit_symbol_address(emitter, "x9", "_eval_reflection_property_count");
    emitter.instruction("ldr x9, [x9]");                                        // load the AOT reflection-property row count
    emitter.instruction("cbz x9, __elephc_eval_reflection_property_declaring_class_miss"); // an empty table cannot contain the requested property
    emitter.instruction("str x9, [sp, #32]");                                   // save the table count across string comparisons
    abi::emit_symbol_address(emitter, "x10", "_eval_reflection_properties");
    emitter.instruction("str x10, [sp, #40]");                                  // save the current property metadata row
    emitter.instruction("mov x11, #0");                                         // start scanning at property metadata row zero
    emitter.label("__elephc_eval_reflection_property_declaring_class_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the property metadata row count
    emitter.instruction("cmp x11, x9");                                         // have all property metadata rows been scanned?
    emitter.instruction("b.ge __elephc_eval_reflection_property_declaring_class_miss"); // no row matched before the end of the table
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current property metadata row
    reflection_metadata::emit_load_member_field(emitter, "x12", "x10", MemberField::ClassNameLength);
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested class-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested class-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_property_declaring_class_skip"); // length mismatch means the class cannot match
    emitter.instruction("str x11, [sp, #48]");                                  // save the row index across the class-name compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested class-name length
    reflection_metadata::emit_load_member_field(emitter, "x3", "x10", MemberField::ClassName);
    emitter.instruction("mov x4, x12");                                         // pass the stored class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #48]");                                  // restore the row index after the class-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested class name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_property_declaring_class_skip"); // class mismatch means the row cannot match
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current row for the property-name compare
    reflection_metadata::emit_load_member_field(emitter, "x12", "x10", MemberField::MemberNameLength);
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload the requested property-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested property-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_property_declaring_class_skip"); // length mismatch means the property cannot match
    emitter.instruction("str x11, [sp, #48]");                                  // save the row index across the property-name compare
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the requested property-name pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass the requested property-name length
    reflection_metadata::emit_load_member_field(emitter, "x3", "x10", MemberField::MemberName);
    emitter.instruction("mov x4, x12");                                         // pass the stored property-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare property names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #48]");                                  // restore the row index after the property-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested property name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_property_declaring_class_skip"); // property mismatch means scanning must continue
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the matched property metadata row
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    reflection_metadata::emit_load_member_field(emitter, "x1", "x10", MemberField::DeclaringClass);
    reflection_metadata::emit_load_member_field(emitter, "x2", "x10", MemberField::DeclaringClassLength);
    emitter.instruction("bl __rt_mixed_from_value");                            // box the declaring class name for Rust
    emitter.instruction("b __elephc_eval_reflection_property_declaring_class_done"); // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_property_declaring_class_skip");
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current property metadata row
    reflection_metadata::emit_advance_member_row(emitter, "x10");
    emitter.instruction("str x10, [sp, #40]");                                  // persist the advanced row cursor
    emitter.instruction("add x11, x11, #1");                                    // advance the row index
    emitter.instruction("b __elephc_eval_reflection_property_declaring_class_loop"); // continue scanning property metadata rows
    emitter.label("__elephc_eval_reflection_property_declaring_class_miss");
    emitter.instruction("mov x0, xzr");                                         // return null when no AOT property metadata matched
    emitter.label("__elephc_eval_reflection_property_declaring_class_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the property metadata scan frame
    emitter.instruction("ret");                                                 // return the declaring class string, or null for a miss, to Rust
}

/// Emits the ARM64 eval hook that returns AOT ReflectionProperty predicate flags.
pub(super) fn emit_aarch64_eval_reflection_property_flags(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_reflection_property_flags");
    emitter.instruction("sub sp, sp, #96");                                     // reserve scan state across runtime string comparisons
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #80");                                    // establish a stable scan frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    emitter.instruction("str x2, [sp, #16]");                                   // save the requested property-name pointer
    emitter.instruction("str x3, [sp, #24]");                                   // save the requested property-name length
    abi::emit_symbol_address(emitter, "x9", "_eval_reflection_property_count");
    emitter.instruction("ldr x9, [x9]");                                        // load the AOT reflection-property row count
    emitter.instruction("cbz x9, __elephc_eval_reflection_property_flags_miss"); // an empty table cannot contain the requested property
    emitter.instruction("str x9, [sp, #32]");                                   // save the table count across string comparisons
    abi::emit_symbol_address(emitter, "x10", "_eval_reflection_properties");
    emitter.instruction("str x10, [sp, #40]");                                  // save the current property metadata row
    emitter.instruction("mov x11, #0");                                         // start scanning at property metadata row zero
    emitter.label("__elephc_eval_reflection_property_flags_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the property metadata row count
    emitter.instruction("cmp x11, x9");                                         // have all property metadata rows been scanned?
    emitter.instruction("b.ge __elephc_eval_reflection_property_flags_miss");   // no row matched before the end of the table
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current property metadata row
    reflection_metadata::emit_load_member_field(emitter, "x12", "x10", MemberField::ClassNameLength);
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested class-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested class-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_property_flags_skip");   // length mismatch means the class cannot match
    emitter.instruction("str x11, [sp, #48]");                                  // save the row index across the class-name compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested class-name length
    reflection_metadata::emit_load_member_field(emitter, "x3", "x10", MemberField::ClassName);
    emitter.instruction("mov x4, x12");                                         // pass the stored class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #48]");                                  // restore the row index after the class-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested class name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_property_flags_skip");   // class mismatch means the row cannot match
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current row for the property-name compare
    reflection_metadata::emit_load_member_field(emitter, "x12", "x10", MemberField::MemberNameLength);
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload the requested property-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested property-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_property_flags_skip");   // length mismatch means the property cannot match
    emitter.instruction("str x11, [sp, #48]");                                  // save the row index across the property-name compare
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the requested property-name pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass the requested property-name length
    reflection_metadata::emit_load_member_field(emitter, "x3", "x10", MemberField::MemberName);
    emitter.instruction("mov x4, x12");                                         // pass the stored property-name length
    emitter.instruction("bl __rt_str_eq");                                      // compare property names with PHP case-sensitive rules
    emitter.instruction("ldr x11, [sp, #48]");                                  // restore the row index after the property-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested property name match this row?
    emitter.instruction("b.eq __elephc_eval_reflection_property_flags_skip");   // property mismatch means scanning must continue
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the matched property metadata row
    reflection_metadata::emit_load_member_field(emitter, "x0", "x10", MemberField::Flags);
    emitter.instruction("b __elephc_eval_reflection_property_flags_done");      // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_property_flags_skip");
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current property metadata row
    reflection_metadata::emit_advance_member_row(emitter, "x10");
    emitter.instruction("str x10, [sp, #40]");                                  // persist the advanced row cursor
    emitter.instruction("add x11, x11, #1");                                    // advance the row index
    emitter.instruction("b __elephc_eval_reflection_property_flags_loop");      // continue scanning property metadata rows
    emitter.label("__elephc_eval_reflection_property_flags_miss");
    emitter.instruction("mov x0, #0");                                          // return zero when no AOT property metadata matched
    emitter.label("__elephc_eval_reflection_property_flags_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the property metadata scan frame
    emitter.instruction("ret");                                                 // return flags, or zero for a miss, to Rust
}
