//! Purpose:
//! Emits AArch64 scalar, object, and class-query eval wrappers.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Wrapper order and C ABI labels match the bridge registration contract.

use super::*;

/// Emits AArch64 scalar, object, and class-query eval wrappers.
pub(super) fn emit_aarch64_values_classes(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, xzr");                                         // null has no low payload word
    emitter.instruction("mov x2, xzr");                                         // null has no high payload word
    emitter.instruction("b __rt_mixed_from_value");                             // box the null payload and return to Rust

    label_c_global(emitter, "__elephc_eval_value_bool");
    emitter.instruction("cmp x0, #0");                                          // normalize any non-zero C bool payload to PHP true
    emitter.instruction("cset x1, ne");                                         // bool payload is 1 for true and 0 for false
    emitter.instruction("mov x0, #3");                                          // runtime tag 3 = bool
    emitter.instruction("mov x2, xzr");                                         // bool payloads do not use a high word
    emitter.instruction("b __rt_mixed_from_value");                             // box the bool payload and return to Rust

    label_c_global(emitter, "__elephc_eval_value_new_object");
    emitter.instruction("sub sp, sp, #32");                                     // allocate a wrapper frame with object and boxed-result slots
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across runtime calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("cmp x1, #8");                                          // stdClass has an 8-byte class name
    emitter.instruction("b.ne __elephc_eval_value_new_object_generic");         // use the generic factory for non-stdClass lengths
    emitter.instruction("ldrb w9, [x0]");                                       // load candidate byte 0 for stdClass comparison
    emitter.instruction("cmp w9, #115");                                        // byte 0 must be 's'
    emitter.instruction("b.ne __elephc_eval_value_new_object_generic");         // fall back when byte 0 differs
    emitter.instruction("ldrb w9, [x0, #1]");                                   // load candidate byte 1 for stdClass comparison
    emitter.instruction("cmp w9, #116");                                        // byte 1 must be 't'
    emitter.instruction("b.ne __elephc_eval_value_new_object_generic");         // fall back when byte 1 differs
    emitter.instruction("ldrb w9, [x0, #2]");                                   // load candidate byte 2 for stdClass comparison
    emitter.instruction("cmp w9, #100");                                        // byte 2 must be 'd'
    emitter.instruction("b.ne __elephc_eval_value_new_object_generic");         // fall back when byte 2 differs
    emitter.instruction("ldrb w9, [x0, #3]");                                   // load candidate byte 3 for stdClass comparison
    emitter.instruction("cmp w9, #67");                                         // byte 3 must be 'C'
    emitter.instruction("b.ne __elephc_eval_value_new_object_generic");         // fall back when byte 3 differs
    emitter.instruction("ldrb w9, [x0, #4]");                                   // load candidate byte 4 for stdClass comparison
    emitter.instruction("cmp w9, #108");                                        // byte 4 must be 'l'
    emitter.instruction("b.ne __elephc_eval_value_new_object_generic");         // fall back when byte 4 differs
    emitter.instruction("ldrb w9, [x0, #5]");                                   // load candidate byte 5 for stdClass comparison
    emitter.instruction("cmp w9, #97");                                         // byte 5 must be 'a'
    emitter.instruction("b.ne __elephc_eval_value_new_object_generic");         // fall back when byte 5 differs
    emitter.instruction("ldrb w9, [x0, #6]");                                   // load candidate byte 6 for stdClass comparison
    emitter.instruction("cmp w9, #115");                                        // byte 6 must be 's'
    emitter.instruction("b.ne __elephc_eval_value_new_object_generic");         // fall back when byte 6 differs
    emitter.instruction("ldrb w9, [x0, #7]");                                   // load candidate byte 7 for stdClass comparison
    emitter.instruction("cmp w9, #115");                                        // byte 7 must be 's'
    emitter.instruction("b.ne __elephc_eval_value_new_object_generic");         // fall back when byte 7 differs
    emitter.instruction("bl __rt_stdclass_new");                                // allocate stdClass with its dynamic-property hash
    emitter.instruction("b __elephc_eval_value_new_object_box");                // box the stdClass object for Rust
    emitter.label("__elephc_eval_value_new_object_generic");
    emitter.instruction("mov x2, x1");                                          // move the C class-name length into new_by_name's string ABI
    emitter.instruction("mov x1, x0");                                          // move the C class-name pointer into new_by_name's string ABI
    emitter.instruction("bl __rt_new_by_name");                                 // allocate the named AOT class object, or return null on miss
    emitter.instruction("cbz x0, __elephc_eval_value_new_object_null");         // box PHP null when no runtime class matched the eval name
    emitter.label("__elephc_eval_value_new_object_box");
    emitter.instruction("str x0, [sp, #0]");                                    // save the raw object owner before boxing it for eval
    emitter.instruction("mov x1, x0");                                          // move the allocated object pointer into the Mixed payload
    emitter.instruction("mov x0, #6");                                          // runtime tag 6 = object
    emitter.instruction("mov x2, xzr");                                         // object payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the allocated object for Rust
    emitter.instruction("str x0, [sp, #8]");                                    // save the boxed Mixed while consuming the raw object owner
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the raw object owner created by the allocator
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load the raw object refcount after Mixed boxing retained it
    emitter.instruction("sub w9, w9, #1");                                      // consume the allocator-owned object reference locally
    emitter.instruction("str w9, [x0, #-12]");                                  // leave the boxed Mixed as the sole object owner
    emitter.instruction("ldr x0, [sp, #8]");                                    // restore the boxed object Mixed as the Rust return value
    emitter.instruction("b __elephc_eval_value_new_object_done");               // skip the null boxing path after successful allocation
    emitter.label("__elephc_eval_value_new_object_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, xzr");                                         // null has no low payload word
    emitter.instruction("mov x2, xzr");                                         // null has no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box null for unknown eval class names
    emitter.label("__elephc_eval_value_new_object_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the dynamic-object wrapper frame
    emitter.instruction("ret");                                                 // return the boxed object or null Mixed cell to Rust

    emit_aarch64_object_from_raw_wrapper(emitter);
    emit_aarch64_install_dynamic_object_destructor_hook(emitter);
    emit_aarch64_object_clone_shallow_wrapper(emitter);

    label_c_global(emitter, "__elephc_eval_class_exists");
    emitter.instruction("sub sp, sp, #64");                                     // reserve helper frame for class-name lookup state
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address across string compares
    emitter.instruction("add x29, sp, #48");                                    // establish a stable class-exists frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    abi::emit_symbol_address(emitter, "x9", "_classes_by_name_count");
    emitter.instruction("ldr x9, [x9]");                                        // load the registered class-name count
    emitter.instruction("cbz x9, __elephc_eval_class_exists_miss");             // an empty table cannot contain the requested class
    emitter.instruction("str x9, [sp, #16]");                                   // save the table count across string compares
    abi::emit_symbol_address(emitter, "x10", "_classes_by_name");
    emitter.instruction("str x10, [sp, #24]");                                  // save the current class-name table cursor
    emitter.instruction("mov x11, #0");                                         // start scanning at table index zero
    emitter.label("__elephc_eval_class_exists_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the class-name table count
    emitter.instruction("cmp x11, x9");                                         // have all class-name entries been scanned?
    emitter.instruction("b.ge __elephc_eval_class_exists_miss");                // no class matched before the end of the table
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current class-name table entry
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the stored class-name length
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested class-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested class-name lengths
    emitter.instruction("b.ne __elephc_eval_class_exists_skip");                // length mismatch means this entry cannot match
    emitter.instruction("str x11, [sp, #32]");                                  // save the table index across the string compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested class-name length
    emitter.instruction("ldr x3, [x10]");                                       // pass the stored class-name pointer
    emitter.instruction("mov x4, x12");                                         // pass the stored class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #32]");                                  // restore the table index after the string compare
    emitter.instruction("cmp x0, #0");                                          // did the requested class name match this entry?
    emitter.instruction("b.eq __elephc_eval_class_exists_hit");                 // report true on a class-name match
    emitter.label("__elephc_eval_class_exists_skip");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current class-name table entry
    emitter.instruction("add x10, x10, #32");                                   // advance to the next class-name table entry
    emitter.instruction("str x10, [sp, #24]");                                  // persist the advanced table cursor
    emitter.instruction("add x11, x11, #1");                                    // advance the table index
    emitter.instruction("b __elephc_eval_class_exists_loop");                   // continue scanning the class-name table
    emitter.label("__elephc_eval_class_exists_hit");
    emitter.instruction("mov x0, #1");                                          // return true for a matched class name
    emitter.instruction("b __elephc_eval_class_exists_done");                   // skip the false result after a match
    emitter.label("__elephc_eval_class_exists_miss");
    emitter.instruction("mov x0, #0");                                          // return false when no class-name entry matched
    emitter.label("__elephc_eval_class_exists_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the class-exists helper frame
    emitter.instruction("ret");                                                 // return the class-exists flag to Rust

    emit_aarch64_eval_name_table_exists(
        emitter,
        "__elephc_eval_interface_exists",
        "_interface_names_count",
        "_interface_names",
        "__elephc_eval_interface_exists",
    );

    emit_aarch64_eval_name_table_exists(
        emitter,
        "__elephc_eval_trait_exists",
        "_trait_names_count",
        "_trait_names",
        "__elephc_eval_trait_exists",
    );
    emit_aarch64_eval_name_table_exists(
        emitter,
        "__elephc_eval_enum_exists",
        "_enum_names_count",
        "_enum_names",
        "__elephc_eval_enum_exists",
    );

    emit_aarch64_eval_reflection_method_names(emitter);
    emit_aarch64_eval_reflection_property_names(emitter);
    emit_aarch64_eval_reflection_class_interface_names(emitter);
    emit_aarch64_eval_reflection_class_trait_names(emitter);
    emit_aarch64_eval_reflection_class_trait_alias_names(emitter);
    emit_aarch64_eval_reflection_class_trait_alias_sources(emitter);
    emit_aarch64_eval_reflection_source_file(emitter);
    emit_aarch64_eval_reflection_class_flags(emitter);
    emit_aarch64_eval_reflection_method_flags(emitter);
    emit_aarch64_eval_reflection_method_declaring_class(emitter);
    emit_aarch64_eval_reflection_property_declaring_class(emitter);
    emit_aarch64_eval_reflection_property_flags(emitter);

    label_c_global(emitter, "__elephc_eval_value_is_a");
    emitter.instruction("sub sp, sp, #64");                                     // reserve relation lookup state and preserve the Rust return address
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address across runtime match helpers
    emitter.instruction("add x29, sp, #48");                                    // establish a stable is-a relation frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed eval object-or-class cell
    emitter.instruction("str x3, [sp, #8]");                                    // save whether exact class matches should be rejected
    emitter.instruction("bl __rt_instanceof_lookup");                           // resolve the target class/interface string to matcher metadata
    emitter.instruction("cmp x0, #0");                                          // did the target string resolve to emitted metadata?
    emitter.instruction("b.eq __elephc_eval_value_is_a_false");                 // unresolved targets cannot match eval object values
    emitter.instruction("str x1, [sp, #16]");                                   // save the target class/interface id
    emitter.instruction("str x2, [sp, #24]");                                   // save the target kind: 0 class, 1 interface
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed eval value for unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // unwrap nested Mixed cells to tag and payload words
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means the eval value is an object
    emitter.instruction("b.eq __elephc_eval_value_is_a_object");                // object values can use their concrete runtime class id
    emitter.instruction("cmp x0, #1");                                          // runtime tag 1 means the eval value is a class string
    emitter.instruction("b.eq __elephc_eval_value_is_a_string");                // class-string values need source metadata lookup
    emitter.instruction("b __elephc_eval_value_is_a_false");                    // other runtime tags cannot satisfy class relations
    emitter.label("__elephc_eval_value_is_a_string");
    emitter.instruction("bl __rt_instanceof_lookup");                           // resolve the source class string to matcher metadata
    emitter.instruction("cmp x0, #0");                                          // did the source string resolve to emitted metadata?
    emitter.instruction("b.eq __elephc_eval_value_is_a_false");                 // unresolved source strings cannot match relation metadata
    emitter.instruction("cmp x2, #0");                                          // source strings must resolve to concrete classes for this matcher
    emitter.instruction("b.ne __elephc_eval_value_is_a_false");                 // interface-source strings need a dedicated interface-parent matcher
    emitter.instruction("str x1, [sp, #32]");                                   // build a fake object header containing the source class id
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the exact-self exclusion flag
    emitter.instruction("cbz x10, __elephc_eval_value_is_a_string_match");      // is_a() allows exact class-string matches
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload target kind before exact-class filtering
    emitter.instruction("cbnz x11, __elephc_eval_value_is_a_string_match");     // interface targets cannot be exact concrete-class self matches
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the target concrete class id
    emitter.instruction("cmp x1, x13");                                         // compare source and target class ids for subclass self exclusion
    emitter.instruction("b.eq __elephc_eval_value_is_a_false");                 // is_subclass_of() excludes the exact class string
    emitter.label("__elephc_eval_value_is_a_string_match");
    emitter.instruction("add x0, sp, #32");                                     // pass the fake object header to the metadata matcher
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the target class/interface id
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass the target kind: 0 class, 1 interface
    emitter.instruction("bl __rt_exception_matches");                           // test class-string inheritance or implemented interfaces
    emitter.instruction("b __elephc_eval_value_is_a_done");                     // keep the matcher result and restore the wrapper frame
    emitter.label("__elephc_eval_value_is_a_object");
    emitter.instruction("mov x9, x1");                                          // keep the unboxed object pointer for matcher input
    emitter.instruction("cbz x9, __elephc_eval_value_is_a_false");              // malformed object payloads cannot match class metadata
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the exact-self exclusion flag
    emitter.instruction("cbz x10, __elephc_eval_value_is_a_match");             // is_a() allows exact class matches
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload target kind before exact-class filtering
    emitter.instruction("cbnz x11, __elephc_eval_value_is_a_match");            // interface targets cannot be exact concrete-class self matches
    emitter.instruction("ldr x12, [x9]");                                       // load the object's concrete runtime class id
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the target concrete class id
    emitter.instruction("cmp x12, x13");                                        // compare object and target class ids for subclass self exclusion
    emitter.instruction("b.eq __elephc_eval_value_is_a_false");                 // is_subclass_of() excludes the object's exact class
    emitter.label("__elephc_eval_value_is_a_match");
    emitter.instruction("mov x0, x9");                                          // pass the unboxed object pointer to the metadata matcher
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the target class/interface id
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass the target kind: 0 class, 1 interface
    emitter.instruction("bl __rt_exception_matches");                           // test inheritance or implemented-interface metadata
    emitter.instruction("b __elephc_eval_value_is_a_done");                     // keep the matcher result and restore the wrapper frame
    emitter.label("__elephc_eval_value_is_a_false");
    emitter.instruction("mov x0, #0");                                          // return false for unresolved, scalar, or exact-self subclass cases
    emitter.label("__elephc_eval_value_is_a_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the relation lookup frame
    emitter.instruction("ret");                                                 // return the boolean class-relation result to Rust

    label_c_global(emitter, "__elephc_eval_value_object_class_name");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve linkage while following the referenced object
    emitter.instruction("bl __rt_mixed_deref");                                 // resolve the concrete receiver before class-name lookup
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore linkage for the leaf metadata lookup
    emitter.instruction("cbz x0, __elephc_eval_value_object_class_name_miss");  // reject null boxed handles before reading their tag
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed eval value runtime tag
    emitter.instruction("cmp x9, #6");                                          // tag 6 is an object payload
    emitter.instruction("b.ne __elephc_eval_value_object_class_name_miss");     // non-objects cannot provide a class name
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the object payload pointer
    emit_branch_if_null_container(
        emitter,
        "x9",
        "x10",
        "__elephc_eval_value_object_class_name_miss",
    );
    emitter.instruction("ldr x10, [x9]");                                       // load the object's runtime class id
    abi::emit_symbol_address(emitter, "x11", "_class_name_count");
    emitter.instruction("ldr x11, [x11]");                                      // load the dense class-name table length
    emitter.instruction("cmp x10, x11");                                        // check whether the class id is in table bounds
    emitter.instruction("b.hs __elephc_eval_value_object_class_name_miss");     // reject missing or out-of-range class ids
    abi::emit_symbol_address(emitter, "x11", "_class_name_entries");
    emitter.instruction("lsl x12, x10, #4");                                    // convert class id to a 16-byte table-entry offset
    emitter.instruction("add x11, x11, x12");                                   // address the class-name entry for this class id
    emitter.instruction("ldr x1, [x11]");                                       // load the class-name string pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // load the class-name string length
    emitter.instruction("cbz x2, __elephc_eval_value_object_class_name_miss");  // reject table holes with empty names
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("b __rt_mixed_from_value");                             // persist and box the class-name string for Rust
    emitter.label("__elephc_eval_value_object_class_name_miss");
    emitter.instruction("mov x0, xzr");                                         // report failure as a null C pointer to Rust
    emitter.instruction("ret");                                                 // return the failure sentinel

    label_c_global(emitter, "__elephc_eval_value_parent_class_name");
    emitter.instruction("sub sp, sp, #80");                                     // reserve lookup state and a call-preserving wrapper frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #64");                                    // establish a stable parent-class lookup frame pointer
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the eval value tag and payload words
    emitter.instruction("cmp x0, #6");                                          // tag 6 is an object payload
    emitter.instruction("b.eq __elephc_eval_value_parent_class_name_object");   // derive the parent from the object's runtime class id
    emitter.instruction("cmp x0, #1");                                          // tag 1 is a class-name string payload
    emitter.instruction("b.eq __elephc_eval_value_parent_class_name_string");   // resolve a class string through generated metadata
    emitter.instruction("b __elephc_eval_value_parent_class_name_empty");       // unsupported input types have no parent class name
    emitter.label("__elephc_eval_value_parent_class_name_object");
    emitter.instruction("cbz x1, __elephc_eval_value_parent_class_name_empty"); // malformed object payloads have no parent class
    emitter.instruction("ldr x9, [x1]");                                        // load the object's runtime class id
    emitter.instruction("b __elephc_eval_value_parent_class_name_from_id");     // convert the class id to its parent class name
    emitter.label("__elephc_eval_value_parent_class_name_string");
    emitter.instruction("str x1, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the requested class-name length
    abi::emit_symbol_address(emitter, "x9", "_classes_by_name_count");
    emitter.instruction("ldr x9, [x9]");                                        // load the registered class-name count
    emitter.instruction("cbz x9, __elephc_eval_value_parent_class_name_empty"); // an empty class table cannot resolve a parent name
    emitter.instruction("str x9, [sp, #16]");                                   // save the table count across string compares
    abi::emit_symbol_address(emitter, "x10", "_classes_by_name");
    emitter.instruction("str x10, [sp, #24]");                                  // save the current class-name table cursor
    emitter.instruction("mov x11, #0");                                         // start scanning generated class-name entries at index zero
    emitter.label("__elephc_eval_value_parent_class_name_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the class-name table count
    emitter.instruction("cmp x11, x9");                                         // have all generated class names been checked?
    emitter.instruction("b.ge __elephc_eval_value_parent_class_name_empty");    // no generated class matched the requested string
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current class-name metadata entry
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the stored class-name length
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested class-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested name lengths first
    emitter.instruction("b.ne __elephc_eval_value_parent_class_name_skip");     // length mismatch means this class entry cannot match
    emitter.instruction("str x11, [sp, #32]");                                  // preserve the scan index across the string compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested class-name length
    emitter.instruction("ldr x3, [x10]");                                       // pass the generated class-name pointer
    emitter.instruction("mov x4, x12");                                         // pass the generated class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #32]");                                  // restore the scan index after the string compare
    emitter.instruction("cmp x0, #0");                                          // did the requested class name match this entry?
    emitter.instruction("b.eq __elephc_eval_value_parent_class_name_hit");      // resolve the matched class entry to its parent id
    emitter.label("__elephc_eval_value_parent_class_name_skip");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current class-name table entry
    emitter.instruction("add x10, x10, #32");                                   // advance to the next class-name table entry
    emitter.instruction("str x10, [sp, #24]");                                  // persist the advanced table cursor
    emitter.instruction("add x11, x11, #1");                                    // advance the class-name scan index
    emitter.instruction("b __elephc_eval_value_parent_class_name_loop");        // continue scanning generated class names
    emitter.label("__elephc_eval_value_parent_class_name_hit");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the matched class-name table entry
    emitter.instruction("ldr x9, [x10, #16]");                                  // load the matched runtime class id
    emitter.label("__elephc_eval_value_parent_class_name_from_id");
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");
    emitter.instruction("ldr x10, [x10]");                                      // load the dense class-name table length
    emitter.instruction("cmp x9, x10");                                         // check that the class id can index parent metadata
    emitter.instruction("b.hs __elephc_eval_value_parent_class_name_empty");    // unknown class ids have no parent class name
    abi::emit_symbol_address(emitter, "x11", "_class_parent_ids");
    emitter.instruction("lsl x12, x9, #3");                                     // convert class id to a parent-id table byte offset
    emitter.instruction("ldr x9, [x11, x12]");                                  // load the parent runtime class id
    emitter.instruction("mov x13, #-1");                                        // materialize the parentless class sentinel
    emitter.instruction("cmp x9, x13");                                         // check whether the runtime class has no parent
    emitter.instruction("b.eq __elephc_eval_value_parent_class_name_empty");    // parentless runtime classes produce an empty string
    emitter.instruction("cmp x9, x10");                                         // check that the parent class id can index name metadata
    emitter.instruction("b.hs __elephc_eval_value_parent_class_name_empty");    // invalid parent ids produce an empty string
    abi::emit_symbol_address(emitter, "x11", "_class_name_entries");
    emitter.instruction("lsl x12, x9, #4");                                     // convert parent id to a 16-byte name-entry offset
    emitter.instruction("add x11, x11, x12");                                   // address the parent class-name metadata row
    emitter.instruction("ldr x1, [x11]");                                       // load the parent class-name string pointer
    emitter.instruction("ldr x2, [x11, #8]");                                   // load the parent class-name string length
    emitter.instruction("cbz x2, __elephc_eval_value_parent_class_name_empty"); // table holes represent missing parent names
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the parent class-name string
    emitter.instruction("b __elephc_eval_value_parent_class_name_done");        // restore the wrapper frame before returning to Rust
    emitter.label("__elephc_eval_value_parent_class_name_empty");
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("mov x1, xzr");                                         // missing parent names use an empty string pointer
    emitter.instruction("mov x2, xzr");                                         // missing parent names use an empty string length
    emitter.instruction("bl __rt_mixed_from_value");                            // box the empty parent class-name string
    emitter.label("__elephc_eval_value_parent_class_name_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the parent-class lookup wrapper frame
    emitter.instruction("ret");                                                 // return the boxed parent class-name string to Rust

}
