//! Purpose:
//! Emits the AArch64 `allowed_classes` option decoder and class-membership gate.
//!
//! Called from:
//! - `super::emit_unserialize()` before the shared diagnostics and parser helpers.
//!
//! Key details:
//! - Option values are normalized into owned direct-string arrays before object hydration.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::runtime::data::{
    UNSER_ALLOWED_CLASSES_ENTRY_PREFIX, UNSER_ALLOWED_CLASSES_POLICY_PREFIX,
    UNSER_OPTIONS_TYPE_PREFIX,
};

/// Emits the per-call `allowed_classes` option parser and class-membership gate.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize_allowed_classes ---");
    emitter.label_global("__rt_unserialize_set_options_mixed");
    emitter.instruction("cbnz x0, __rt_unser_options_mixed_nonnull");           // keep the conditional branch inside the mixed-options entry atom
    emitter.instruction("b __rt_unser_options_type_error");                     // route null through the shared TypeError path unconditionally
    emitter.label("__rt_unser_options_mixed_nonnull");
    emitter.instruction("ldr x9, [x0]");                                        // inspect the Mixed runtime tag before any payload dereference
    emitter.instruction("cmp x9, #4");                                          // Mixed(indexed array) is valid default options
    emitter.instruction("b.ne __rt_unser_options_mixed_assoc");                 // keep the conditional branch on an assembler-local target
    emitter.instruction("b __rt_unserialize_set_options_indexed");              // enter the exported indexed-options helper unconditionally
    emitter.label("__rt_unser_options_mixed_assoc");
    emitter.instruction("cmp x9, #5");                                          // Mixed(assoc array) can represent the options map
    emitter.instruction("b.eq __rt_unser_options_mixed_assoc_valid");           // keep the conditional branch inside the mixed-options entry atom
    emitter.instruction("b __rt_unser_options_type_error");                     // route scalar/object shapes through the shared TypeError path
    emitter.label("__rt_unser_options_mixed_assoc_valid");
    emitter.instruction("ldr x0, [x0, #8]");                                    // unwrap the validated associative-hash payload
    emitter.instruction("b __rt_unserialize_set_options");                      // scan a trusted options hash

    emitter.label_global("__rt_unserialize_set_options_indexed");
    emitter.instruction("ret");                                                 // indexed options have no string keys; preserve defaults

    emitter.label_global("__rt_unserialize_set_options");
    emitter.instruction("sub sp, sp, #80");                                     // reserve source, normalized-list, cursor, and conversion spills
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve caller state across allocation, iteration, and user conversion
    emitter.instruction("add x29, sp, #64");                                    // retain a conventional aligned options-decoder frame
    emitter.instruction("mov x9, x0");                                          // preserve raw associative options hash
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_unser_allowed_classes_key");
    emitter.instruction("mov x2, #15");                                         // byte length of allowed_classes
    emitter.instruction("mov x0, x9");                                          // hash_get receiver
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=payload, x3=tag
    emitter.instruction("cbz x0, __rt_unser_options_done");                     // absent option preserves default allow-all policy
    emitter.instruction("cmp x3, #7");                                          // heterogeneous option hashes store a boxed Mixed value
    emitter.instruction("b.ne __rt_unser_options_unboxed");                     // direct typed payload needs no unwrap
    emitter.instruction("ldr x3, [x1]");                                        // recover boxed value tag
    emitter.instruction("ldr x1, [x1, #8]");                                    // recover boxed value payload
    emitter.label("__rt_unser_options_unboxed");
    emitter.instruction("cmp x3, #3");                                          // boolean option?
    emitter.instruction("b.ne __rt_unser_options_list");                        // arrays are allow-lists
    emitter.instruction("cbnz x1, __rt_unser_options_done");                    // true preserves allow-all policy
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_allowed_mode");
    emitter.instruction("mov x10, #1");                                         // mode 1 = no class hydration
    emitter.instruction("str x10, [x9]");                                       // block every serialized object
    emitter.instruction("b __rt_unser_options_done");                           // restore the helper frame through the shared return path
    emitter.label("__rt_unser_options_list");
    emitter.instruction("cmp x3, #4");                                          // indexed array allow-list?
    emitter.instruction("b.eq __rt_unser_options_indexed_list_setup");          // normalize packed values in index order
    emitter.instruction("cmp x3, #5");                                          // associative array allow-list?
    emitter.instruction("b.eq __rt_unser_options_assoc_list");                  // keep the conditional branch inside the options-decoder atom
    emitter.instruction("b __rt_unser_allowed_classes_policy_type_error");      // reject non-array policies through the shared TypeError path
    emitter.label("__rt_unser_options_assoc_list");
    emitter.instruction("str x1, [sp]");                                        // preserve associative source hash across allocation
    emitter.instruction("mov x0, x1");                                          // hash_count receiver
    emitter.instruction("bl __rt_hash_count");                                  // capacity equals the number of values to normalize
    emitter.instruction("mov x9, #5");                                          // source kind 5 = associative hash
    emitter.instruction("b __rt_unser_options_list_allocate");                  // share normalized string-array allocation
    emitter.label("__rt_unser_options_indexed_list_setup");
    emitter.instruction("str x1, [sp]");                                        // preserve indexed source array across allocation
    emitter.instruction("ldr x0, [x1]");                                        // capacity equals the packed logical length
    emitter.instruction("mov x9, #4");                                          // source kind 4 = indexed array
    emitter.label("__rt_unser_options_list_allocate");
    emitter.instruction("str x9, [sp, #24]");                                   // remember source shape for the iterator dispatch
    emitter.instruction("mov x1, #16");                                         // normalized entries are direct string pointer/length pairs
    emitter.instruction("bl __rt_array_new");                                   // allocate the context-owned normalized allow-list
    emitter.instruction("str x0, [sp, #8]");                                    // retain normalized owner across validation calls
    emitter.instruction("str xzr, [sp, #16]");                                  // start packed index/hash cursor at zero
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_allowed_list_mixed");
    emitter.instruction("str xzr, [x9]");                                       // direct string pointer/length cells
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_allowed_mode");
    emitter.instruction("mov x10, #2");                                         // mode 2 = named allow-list
    emitter.instruction("str x10, [x9]");                                       // publish policy before validation so error cleanup owns the partial list
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_allowed_list");
    emitter.instruction("str x0, [x9]");                                        // publish the fresh normalized array owner
    emitter.instruction("ldr x9, [sp, #24]");                                   // select packed or associative value traversal
    emitter.instruction("cmp x9, #5");                                          // associative source?
    emitter.instruction("b.eq __rt_unser_options_hash_list_loop");              // hash keys are ignored; values supply class names

    emitter.label("__rt_unser_options_indexed_list_loop");
    emitter.instruction("ldr x9, [sp]");                                        // indexed source array
    emitter.instruction("ldr x10, [sp, #16]");                                  // packed element index
    emitter.instruction("ldr x11, [x9]");                                       // packed logical length
    emitter.instruction("cmp x10, x11");                                        // normalized every packed value?
    emitter.instruction("b.hs __rt_unser_options_done");                        // publish is already complete
    emitter.instruction("add x11, x10, #1");                                    // advance index before any nested helper call
    emitter.instruction("str x11, [sp, #16]");                                  // preserve next index across conversion helpers
    emitter.instruction("ldr x11, [x9, #-8]");                                  // packed array metadata precedes the payload header
    emitter.instruction("ubfx x11, x11, #8, #7");                               // extract the runtime element tag
    emitter.instruction("cmp x11, #1");                                         // direct string payload?
    emitter.instruction("b.eq __rt_unser_options_indexed_string");              // direct strings append without conversion
    emitter.instruction("cmp x11, #6");                                         // direct object pointer payload?
    emitter.instruction("b.eq __rt_unser_options_indexed_object");              // objects convert through __toString
    emitter.instruction("cmp x11, #7");                                         // heterogeneous packed values use boxed Mixed cells
    emitter.instruction("b.eq __rt_unser_options_indexed_mixed");               // keep the conditional branch inside the options-decoder atom
    emitter.instruction("b __rt_unser_allowed_classes_entry_metadata_error");   // reject other homogeneous types through the shared error path
    emitter.label("__rt_unser_options_indexed_mixed");
    emitter.instruction("add x12, x9, #24");                                    // skip indexed header to the packed payload base
    emitter.instruction("ldr x12, [x12, x10, lsl #3]");                         // read the selected boxed Mixed pointer
    emitter.instruction("cbnz x12, __rt_unser_options_indexed_cell_ready");     // keep the conditional branch inside the options-decoder atom
    emitter.instruction("b __rt_unser_allowed_classes_entry_null_error");       // report a malformed/null cell through the shared error path
    emitter.label("__rt_unser_options_indexed_cell_ready");
    emitter.instruction("ldr x13, [x12]");                                      // boxed runtime tag
    emitter.instruction("ldr x1, [x12, #8]");                                   // boxed low payload
    emitter.instruction("ldr x2, [x12, #16]");                                  // boxed high payload
    emitter.instruction("cmp x13, #1");                                         // boxed string entry?
    emitter.instruction("b.eq __rt_unser_options_append_borrowed");             // append the boxed string's borrowed bytes
    emitter.instruction("cmp x13, #6");                                         // boxed object entry?
    emitter.instruction("b.eq __rt_unser_options_boxed_object");                // unwrap the object for __toString conversion
    emitter.instruction("mov x3, x13");                                         // forward invalid boxed runtime tag
    emitter.instruction("b __rt_unser_allowed_classes_entry_type_error");       // other boxed types are invalid class names
    emitter.label("__rt_unser_options_indexed_string");
    emitter.instruction("add x9, x9, #24");                                     // direct payload begins after indexed header
    emitter.instruction("lsl x10, x10, #4");                                    // string pair stride is sixteen bytes
    emitter.instruction("add x9, x9, x10");                                     // select the current direct string pair
    emitter.instruction("ldp x1, x2, [x9]");                                    // borrow source string pointer and length
    emitter.instruction("b __rt_unser_options_append_borrowed");                // append the borrowed string as an allowed name
    emitter.label("__rt_unser_options_indexed_object");
    emitter.instruction("add x12, x9, #24");                                    // skip indexed header to direct object slots
    emitter.instruction("ldr x0, [x12, x10, lsl #3]");                          // load selected direct object pointer
    emitter.instruction("cbnz x0, __rt_unser_options_indexed_object_ready");    // keep the conditional branch inside the options-decoder atom
    emitter.instruction("b __rt_unser_allowed_classes_entry_null_error");       // report a null object slot through the shared error path
    emitter.label("__rt_unser_options_indexed_object_ready");
    emitter.instruction("b __rt_unser_options_convert_object");                 // convert the entry object via __toString
    emitter.label("__rt_unser_options_boxed_object");
    emitter.instruction("mov x0, x1");                                          // boxed low payload is the object pointer
    emitter.instruction("b __rt_unser_options_convert_object");                 // convert the boxed object via __toString

    emitter.label("__rt_unser_options_hash_list_loop");
    emitter.instruction("ldr x0, [sp]");                                        // associative source hash
    emitter.instruction("ldr x1, [sp, #16]");                                   // insertion-order iterator cursor
    emitter.instruction("bl __rt_hash_iter_next_value");                        // return next borrowed value tuple; keys are intentionally ignored
    emitter.instruction("cmp x0, #-1");                                         // exhausted every associative value?
    emitter.instruction("b.eq __rt_unser_options_done");                        // normalized policy is ready
    emitter.instruction("str x0, [sp, #16]");                                   // preserve next iterator cursor
    emitter.instruction("cmp x5, #7");                                          // boxed Mixed hash value?
    emitter.instruction("b.ne __rt_unser_options_hash_unboxed");                // direct typed values skip the unwrap
    emitter.instruction("cbnz x3, __rt_unser_options_hash_cell_ready");         // keep the conditional branch inside the options-decoder atom
    emitter.instruction("b __rt_unser_allowed_classes_entry_null_error");       // reject a null boxed hash cell through the shared error path
    emitter.label("__rt_unser_options_hash_cell_ready");
    emitter.instruction("mov x12, x3");                                         // preserve boxed cell pointer for payload reads
    emitter.instruction("ldr x5, [x12]");                                       // unwrap actual value tag
    emitter.instruction("ldr x3, [x12, #8]");                                   // unwrap low payload
    emitter.instruction("ldr x4, [x12, #16]");                                  // unwrap high payload
    emitter.label("__rt_unser_options_hash_unboxed");
    emitter.instruction("cmp x5, #1");                                          // string hash value?
    emitter.instruction("b.eq __rt_unser_options_hash_string");                 // strings append without conversion
    emitter.instruction("cmp x5, #6");                                          // object hash value?
    emitter.instruction("b.eq __rt_unser_options_hash_object");                 // objects convert through __toString
    emitter.instruction("mov x1, x3");                                          // forward invalid value payload for dynamic type naming
    emitter.instruction("mov x3, x5");                                          // forward invalid value runtime tag
    emitter.instruction("b __rt_unser_allowed_classes_entry_type_error");       // other value types are invalid class names
    emitter.label("__rt_unser_options_hash_string");
    emitter.instruction("mov x1, x3");                                          // normalize iterator string pointer to push ABI
    emitter.instruction("mov x2, x4");                                          // normalize iterator string length to push ABI
    emitter.instruction("b __rt_unser_options_append_borrowed");                // append the borrowed hash-value string
    emitter.label("__rt_unser_options_hash_object");
    emitter.instruction("mov x0, x3");                                          // iterator low payload is the object pointer

    emitter.label("__rt_unser_options_convert_object");
    emitter.instruction("str x0, [sp, #32]");                                   // preserve offending object for Error and result-owner cleanup
    emitter.instruction("bl __rt_unser_allowed_object_to_string");              // invoke __toString under an internal exception boundary
    emitter.instruction("cbnz x1, __rt_unser_options_object_string_ready");     // keep the conditional branch inside the options-decoder atom
    emitter.instruction("b __rt_unser_allowed_classes_entry_object_error");     // absent conversion enters the shared object Error path
    emitter.label("__rt_unser_options_object_string_ready");
    emitter.instruction("stp x1, x2, [sp, #40]");                               // preserve method-return pair across heap-kind classification
    emitter.instruction("mov x0, x1");                                          // classify ownership of the returned string payload
    emitter.instruction("bl __rt_heap_kind");                                   // kind 7 is transferred in place by str_persist inside push_str
    emitter.instruction("str x0, [sp, #56]");                                   // preserve original heap kind across normalized append
    emitter.instruction("ldp x1, x2, [sp, #40]");                               // reload method-return string pair for array_push_str
    emitter.instruction("ldr x0, [sp, #8]");                                    // normalized destination array
    emitter.instruction("bl __rt_array_push_str");                              // persist and append the converted class name
    emitter.instruction("str x0, [sp, #8]");                                    // retain possibly-grown destination
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_allowed_list");
    emitter.instruction("str x0, [x9]");                                        // keep cleanup owner current after a possible growth
    emitter.instruction("ldr x9, [sp, #56]");                                   // recover original method-return heap kind
    emitter.instruction("cmp x9, #7");                                          // did push_str take over a concat temporary in place?
    emitter.instruction("b.eq __rt_unser_options_list_continue");               // transferred storage is now owned by the normalized array
    emitter.instruction("ldr x0, [sp, #40]");                                   // recover transient __toString return owner
    emitter.instruction("bl __rt_heap_free_safe");                              // release it after array_push_str copied the bytes
    emitter.instruction("b __rt_unser_options_list_continue");                  // process the next source value

    emitter.label("__rt_unser_options_append_borrowed");
    emitter.instruction("ldr x0, [sp, #8]");                                    // normalized destination array
    emitter.instruction("bl __rt_array_push_str");                              // persist and append borrowed source bytes
    emitter.instruction("str x0, [sp, #8]");                                    // retain possibly-grown destination
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_allowed_list");
    emitter.instruction("str x0, [x9]");                                        // keep cleanup owner current after a possible growth
    emitter.label("__rt_unser_options_list_continue");
    emitter.instruction("ldr x9, [sp, #24]");                                   // resume the matching source traversal
    emitter.instruction("cmp x9, #5");                                          // associative source?
    emitter.instruction("b.eq __rt_unser_options_hash_list_loop");              // continue associative values
    emitter.instruction("b __rt_unser_options_indexed_list_loop");              // otherwise continue packed traversal
    emitter.label("__rt_unser_options_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore caller frame and link register after nested helpers
    emitter.instruction("add sp, sp, #80");                                     // release normalized-list decoder state
    emitter.instruction("ret");                                                 // caller proceeds to parsing

    emitter.label_global("__rt_unserialize_class_allowed");
    emitter.instruction("sub sp, sp, #64");                                     // save class name across list scans and comparisons
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve helper frame/link
    emitter.instruction("add x29, sp, #48");                                    // establish stable frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // class name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // class name byte length
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_allowed_mode");
    emitter.instruction("ldr x9, [x9]");                                        // current policy mode
    emitter.instruction("cbz x9, __rt_unser_class_allowed_yes");                // mode 0 = allow all
    emitter.instruction("cmp x9, #1");                                          // mode 1 = block all
    emitter.instruction("b.eq __rt_unser_class_allowed_no");                    // never hydrate blocked classes
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_allowed_list");
    emitter.instruction("ldr x9, [x9]");                                        // indexed string-array payload
    emitter.instruction("cbz x9, __rt_unser_class_allowed_no");                 // malformed list fails closed
    emitter.instruction("ldr x10, [x9]");                                       // list length
    emitter.instruction("str x9, [sp, #16]");                                   // save list base across strcasecmp
    emitter.instruction("str x10, [sp, #24]");                                  // save list length
    emitter.instruction("mov x11, #0");                                         // list cursor
    emitter.label("__rt_unser_class_allowed_loop");
    emitter.instruction("ldr x10, [sp, #24]");                                  // list length
    emitter.instruction("cmp x11, x10");                                        // exhausted every allowed name?
    emitter.instruction("b.ge __rt_unser_class_allowed_no");                    // no exact match means incomplete object
    emitter.instruction("ldr x9, [sp, #16]");                                   // list base
    crate::codegen_support::abi::emit_symbol_address(emitter, "x12", "_unser_allowed_list_mixed");
    emitter.instruction("ldr x12, [x12]");                                      // heterogeneous allow-list representation?
    emitter.instruction("cbnz x12, __rt_unser_class_allowed_mixed_cell");       // boxed cells need an extra dereference
    emitter.instruction("add x9, x9, #24");                                     // skip indexed-array header
    emitter.instruction("lsl x10, x11, #4");                                    // direct string element stride = pointer + length
    emitter.instruction("add x9, x9, x10");                                     // select the current direct-string pair
    emitter.instruction("b __rt_unser_class_allowed_compare");                  // compare against the requested class name
    emitter.label("__rt_unser_class_allowed_mixed_cell");
    emitter.instruction("add x9, x9, #24");                                     // skip the indexed-array header to the cell slots
    emitter.instruction("lsl x10, x11, #3");                                    // boxed Mixed elements are pointers
    emitter.instruction("ldr x9, [x9, x10]");                                   // load the validated boxed string cell
    emitter.instruction("add x9, x9, #8");                                      // string payload pair follows Mixed tag
    emitter.label("__rt_unser_class_allowed_compare");
    emitter.instruction("ldr x1, [sp, #0]");                                    // requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // requested class-name length
    emitter.instruction("ldr x3, [x9]");                                        // allow-list string pointer
    emitter.instruction("ldr x4, [x9, #8]");                                    // allow-list string length
    emitter.instruction("str x11, [sp, #32]");                                  // preserve cursor across the string helper
    emitter.instruction("bl __rt_strcasecmp");                                  // PHP class names compare case-insensitively
    emitter.instruction("cbz x0, __rt_unser_class_allowed_yes");                // matching allow-list entry grants hydration
    emitter.instruction("ldr x11, [sp, #32]");                                  // restore cursor after helper call
    emitter.instruction("add x11, x11, #1");                                    // advance after mismatch
    emitter.instruction("b __rt_unser_class_allowed_loop");                     // scan next allowed class
    emitter.label("__rt_unser_class_allowed_yes");
    emitter.instruction("mov x0, #1");                                          // true = instantiate and hydrate
    emitter.instruction("b __rt_unser_class_allowed_return");                   // shared frame teardown
    emitter.label("__rt_unser_class_allowed_no");
    emitter.instruction("mov x0, #0");                                          // false = incomplete object without hooks
    emitter.label("__rt_unser_class_allowed_return");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore helper frame/link
    emitter.instruction("add sp, sp, #64");                                     // release saved inputs
    emitter.instruction("ret");                                                 // return allow decision

    emitter.label_shared("__rt_unser_options_type_error");
    emitter.instruction("cbz x0, __rt_unser_options_type_null_error");          // a null Mixed pointer reports null
    emitter.instruction("ldr x1, [x0, #8]");                                    // rejected runtime payload
    emitter.instruction("ldr x0, [x0]");                                        // rejected runtime tag
    emitter.instruction("b __rt_unser_options_type_dispatch");                  // compose the options TypeError message
    emitter.label("__rt_unser_options_type_null_error");
    emitter.instruction("mov x0, #8");                                          // runtime null tag
    emitter.instruction("mov x1, #0");                                          // null has no payload
    emitter.label("__rt_unser_options_type_dispatch");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x2", "_unser_options_type_prefix");
    emitter.instruction(&format!("mov x3, #{}", UNSER_OPTIONS_TYPE_PREFIX.len())); // diagnostic prefix byte length
    emitter.instruction("b __rt_unser_throw_type_error");                       // helper closes context and throws exactly once

    emitter.label_shared("__rt_unser_allowed_classes_entry_null_error");
    emitter.instruction("mov x3, #8");                                          // invalid list entry is null
    emitter.instruction("mov x1, #0");                                          // null has no payload
    emitter.instruction("b __rt_unser_allowed_classes_entry_type_error");       // share the entry TypeError cleanup path
    emitter.label_shared("__rt_unser_allowed_classes_entry_metadata_error");
    emitter.instruction("mov x3, x11");                                         // packed element metadata uses the runtime tag numbering
    emitter.instruction("mov x1, #0");                                          // homogeneous metadata has no single payload
    emitter.label_shared("__rt_unser_allowed_classes_entry_type_error");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // remove the options decoder frame before unwinding
    emitter.instruction("add sp, sp, #80");                                     // restore the caller stack before context cleanup
    emitter.instruction("mov x0, x3");                                          // rejected entry runtime tag
    crate::codegen_support::abi::emit_symbol_address(emitter, "x2", "_unser_allowed_classes_entry_prefix");
    emitter.instruction(&format!("mov x3, #{}", UNSER_ALLOWED_CLASSES_ENTRY_PREFIX.len())); // diagnostic prefix byte length
    emitter.instruction("b __rt_unser_throw_type_error");                       // helper closes context and throws exactly once

    emitter.label_shared("__rt_unser_allowed_classes_entry_object_error");
    emitter.instruction("ldr x0, [sp, #32]");                                   // recover offending object while source storage remains live
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // remove options decoder frame before unwinding
    emitter.instruction("add sp, sp, #80");                                     // restore caller stack before Error construction
    emitter.instruction("b __rt_unser_throw_object_string_error");              // helper resolves class name, closes context once, and throws Error

    emitter.label_shared("__rt_unser_allowed_classes_policy_type_error");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // remove the options decoder frame before unwinding
    emitter.instruction("add sp, sp, #80");                                     // restore caller stack before the common cleanup helper
    emitter.instruction("mov x0, x3");                                          // rejected policy runtime tag
    crate::codegen_support::abi::emit_symbol_address(emitter, "x2", "_unser_allowed_classes_policy_prefix");
    emitter.instruction(&format!("mov x3, #{}", UNSER_ALLOWED_CLASSES_POLICY_PREFIX.len())); // diagnostic prefix byte length
    emitter.instruction("b __rt_unser_throw_type_error");                       // helper closes context and throws exactly once
}
