//! Purpose:
//! Emits C-ABI wrappers used by the optional `elephc-eval` bridge crate.
//! Adapts Rust staticlib calls to elephc's internal runtime value helper ABI.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` when `RuntimeFeatures.eval` is enabled.
//!
//! Key details:
//! - Exported wrapper labels use platform C-symbol mangling because they are
//!   referenced from Rust object files, while internal `__rt_*` calls keep the
//!   existing assembly ABI.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Builds the x86_64 instruction that installs the Mixed heap-kind marker.
fn x86_64_mixed_heap_kind_instruction() -> String {
    format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 5)
}

/// Emits every eval value wrapper required by `libelephc-eval`.
pub(crate) fn emit_eval_bridge_runtime(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: eval bridge value wrappers ---");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64_wrappers(emitter),
        Arch::X86_64 => emit_x86_64_wrappers(emitter),
    }
}

/// Emits ARM64 C-ABI wrappers around the internal mixed value helpers.
fn emit_aarch64_wrappers(emitter: &mut Emitter) {
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
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame across dynamic object lookup
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across runtime calls
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
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
    emitter.instruction("mov x1, x0");                                          // move the allocated object pointer into the Mixed payload
    emitter.instruction("mov x0, #6");                                          // runtime tag 6 = object
    emitter.instruction("mov x2, xzr");                                         // object payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the allocated object for Rust
    emitter.instruction("b __elephc_eval_value_new_object_done");               // skip the null boxing path after successful allocation
    emitter.label("__elephc_eval_value_new_object_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, xzr");                                         // null has no low payload word
    emitter.instruction("mov x2, xzr");                                         // null has no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box null for unknown eval class names
    emitter.label("__elephc_eval_value_new_object_done");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the dynamic-object wrapper frame
    emitter.instruction("ret");                                                 // return the boxed object or null Mixed cell to Rust

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

    emit_aarch64_eval_reflection_method_flags(emitter);
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
    emitter.instruction("cbz x0, __elephc_eval_value_object_class_name_miss");  // reject null boxed handles before reading their tag
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed eval value runtime tag
    emitter.instruction("cmp x9, #6");                                          // tag 6 is an object payload
    emitter.instruction("b.ne __elephc_eval_value_object_class_name_miss");     // non-objects cannot provide a class name
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the object payload pointer
    emitter.instruction("cbz x9, __elephc_eval_value_object_class_name_miss");  // reject malformed object payloads
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

    label_c_global(emitter, "__elephc_eval_value_array_new");
    emitter.instruction("sub sp, sp, #48");                                     // allocate a wrapper frame for array allocation and boxing
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across runtime calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable wrapper frame pointer
    emitter.instruction("mov x9, #4");                                          // minimum indexed-array capacity for eval literals
    emitter.instruction("cmp x0, x9");                                          // compare requested capacity with the minimum capacity
    emitter.instruction("csel x0, x0, x9, hs");                                 // use max(requested, 4) as the runtime allocation capacity
    emitter.instruction("mov x1, #8");                                          // Mixed indexed arrays store boxed-cell pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate indexed-array storage for boxed Mixed slots
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the packed indexed-array heap kind word
    emitter.instruction("mov x12, #0x80ff");                                    // preserve indexed-array kind and persistent COW metadata
    emitter.instruction("and x10, x10, x12");                                   // clear the default scalar value_type bits
    emitter.instruction("mov x11, #7");                                         // runtime value_type 7 = boxed Mixed
    emitter.instruction("lsl x11, x11, #8");                                    // move the value_type tag into the packed kind word
    emitter.instruction("orr x10, x10, x11");                                   // stamp the array as carrying boxed Mixed slots
    emitter.instruction("str x10, [x0, #-8]");                                  // persist the updated indexed-array metadata
    emitter.instruction("str x0, [sp, #0]");                                    // save the owned array pointer while allocating the Mixed box
    emitter.instruction("mov x0, #24");                                         // Mixed cells store tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate a boxed Mixed cell without retaining the new array
    emitter.instruction("mov x9, #5");                                          // low byte 5 = mixed cell heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // install the mixed-cell heap kind in the uniform header
    emitter.instruction("mov x10, #4");                                         // runtime tag 4 = indexed array
    emitter.instruction("str x10, [x0]");                                       // store the indexed-array tag in the Mixed cell
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the owned indexed-array pointer
    emitter.instruction("str x11, [x0, #8]");                                   // store the array pointer as the Mixed low payload word
    emitter.instruction("str xzr, [x0, #16]");                                  // indexed arrays do not use the high payload word
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the array-new wrapper frame
    emitter.instruction("ret");                                                 // return the boxed array Mixed cell to Rust

    label_c_global(emitter, "__elephc_eval_value_string_array_new");
    emitter.instruction("sub sp, sp, #48");                                     // allocate a wrapper frame for string-array allocation and boxing
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across runtime calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable wrapper frame pointer
    emitter.instruction("mov x9, #4");                                          // minimum indexed-array capacity for eval metadata lists
    emitter.instruction("cmp x0, x9");                                          // compare requested capacity with the minimum capacity
    emitter.instruction("csel x0, x0, x9, hs");                                 // use max(requested, 4) as the runtime allocation capacity
    emitter.instruction("mov x1, #16");                                         // direct string arrays store pointer/length pairs
    emitter.instruction("bl __rt_array_new");                                   // allocate indexed-array storage for direct string slots
    emitter.instruction("str x0, [sp, #0]");                                    // save the owned string-array pointer while boxing it
    emitter.instruction("mov x0, #24");                                         // Mixed cells store tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate a boxed Mixed cell without retaining the new array
    emitter.instruction("mov x9, #5");                                          // low byte 5 = mixed cell heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // install the mixed-cell heap kind in the uniform header
    emitter.instruction("mov x10, #4");                                         // runtime tag 4 = indexed array
    emitter.instruction("str x10, [x0]");                                       // store the indexed-array tag in the Mixed cell
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the owned direct-string array pointer
    emitter.instruction("str x11, [x0, #8]");                                   // store the string-array pointer as the Mixed low payload word
    emitter.instruction("str xzr, [x0, #16]");                                  // indexed arrays do not use the high payload word
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the string-array-new wrapper frame
    emitter.instruction("ret");                                                 // return the boxed direct-string array Mixed cell to Rust

    label_c_global(emitter, "__elephc_eval_value_string_array_push");
    emitter.instruction("sub sp, sp, #48");                                     // allocate a wrapper frame while appending one metadata string
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across runtime calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed string-array owner
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the incoming string pointer and length
    emitter.instruction("cbz x0, __elephc_eval_value_string_array_push_fail");  // reject malformed null string-array handles
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the indexed-array tag and payload pointer
    emitter.instruction("cmp x0, #4");                                          // runtime tag 4 means indexed array
    emitter.instruction("b.ne __elephc_eval_value_string_array_push_fail");     // reject non-array metadata containers
    emitter.instruction("mov x0, x1");                                          // pass the unboxed array payload to the string append helper
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // reload the string payload to append
    emitter.instruction("bl __rt_array_push_str");                              // persist and append the string, returning the updated array payload
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the boxed string-array owner
    emitter.instruction("str x0, [x9, #8]");                                    // update the boxed payload in case the array grew
    emitter.instruction("mov x0, x9");                                          // return the boxed string-array owner to Rust
    emitter.instruction("b __elephc_eval_value_string_array_push_done");        // skip the malformed-input null result
    emitter.label("__elephc_eval_value_string_array_push_fail");
    emitter.instruction("mov x0, xzr");                                         // report a null pointer so Rust converts it to RuntimeFatal
    emitter.label("__elephc_eval_value_string_array_push_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the string-array-push wrapper frame
    emitter.instruction("ret");                                                 // return the updated boxed string-array handle to Rust

    label_c_global(emitter, "__elephc_eval_value_assoc_new");
    emitter.instruction("sub sp, sp, #48");                                     // allocate a wrapper frame for hash allocation and boxing
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across runtime calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable wrapper frame pointer
    emitter.instruction("mov x9, #16");                                         // minimum hash capacity for eval associative literals
    emitter.instruction("cmp x0, x9");                                          // compare requested capacity with the minimum hash capacity
    emitter.instruction("csel x0, x0, x9, hs");                                 // use max(requested, 16) as the hash allocation capacity
    emitter.instruction("mov x1, #7");                                          // runtime value_type 7 = boxed Mixed hash values
    emitter.instruction("bl __rt_hash_new");                                    // allocate associative-array storage for boxed Mixed entries
    emitter.instruction("str x0, [sp, #0]");                                    // save the owned hash pointer while allocating the Mixed box
    emitter.instruction("mov x0, #24");                                         // Mixed cells store tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate a boxed Mixed cell without retaining the new hash
    emitter.instruction("mov x9, #5");                                          // low byte 5 = mixed cell heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // install the mixed-cell heap kind in the uniform header
    emitter.instruction("mov x10, #5");                                         // runtime tag 5 = associative array
    emitter.instruction("str x10, [x0]");                                       // store the associative-array tag in the Mixed cell
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the owned hash pointer
    emitter.instruction("str x11, [x0, #8]");                                   // store the hash pointer as the Mixed low payload word
    emitter.instruction("str xzr, [x0, #16]");                                  // associative arrays do not use the high payload word
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the assoc-new wrapper frame
    emitter.instruction("ret");                                                 // return the boxed associative-array Mixed cell to Rust

    label_c_global(emitter, "__elephc_eval_value_array_get");
    emitter.instruction("sub sp, sp, #32");                                     // allocate a wrapper frame for key coercion
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed array receiver while coercing the key
    emitter.instruction("mov x0, x1");                                          // pass the boxed key to the eval key normalizer
    emitter.instruction("bl __elephc_eval_key_normalize");                      // normalize eval array key to key_lo/key_hi
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed array receiver
    emitter.instruction("bl __rt_mixed_array_get");                             // read the boxed Mixed element or Mixed(null)
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the array-get wrapper frame
    emitter.instruction("ret");                                                 // return the boxed element to Rust

    label_c_global(emitter, "__elephc_eval_value_array_key_exists");
    emitter.instruction("sub sp, sp, #48");                                     // allocate a wrapper frame for key existence probing
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the boxed array receiver while normalizing the key
    emitter.instruction("bl __elephc_eval_key_normalize");                      // normalize eval array key to key_lo/key_hi
    emitter.instruction("str x1, [sp, #8]");                                    // save the normalized key low word
    emitter.instruction("str x2, [sp, #16]");                                   // save the normalized key high word
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed array receiver for tag dispatch
    emitter.instruction("cbz x0, __elephc_eval_value_array_key_exists_false");  // null handles do not contain array keys
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed Mixed runtime tag
    emitter.instruction("cmp x9, #4");                                          // tag 4 = indexed array
    emitter.instruction("b.eq __elephc_eval_value_array_key_exists_indexed");   // indexed arrays use bounds-based key existence
    emitter.instruction("cmp x9, #5");                                          // tag 5 = associative array
    emitter.instruction("b.eq __elephc_eval_value_array_key_exists_assoc");     // associative arrays use hash existence
    emitter.instruction("b __elephc_eval_value_array_key_exists_false");        // scalar values do not contain array keys
    emitter.label("__elephc_eval_value_array_key_exists_indexed");
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload normalized key_hi for integer-key checking
    emitter.instruction("cmn x2, #1");                                          // integer keys carry key_hi = -1
    emitter.instruction("b.ne __elephc_eval_value_array_key_exists_false");     // non-integer keys never exist in indexed arrays
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed indexed-array receiver
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the indexed-array payload pointer
    emitter.instruction("cbz x0, __elephc_eval_value_array_key_exists_false");  // missing payload cannot contain a key
    emitter.instruction("ldr x1, [sp, #8]");                                    // pass normalized integer key to the bounds helper
    emitter.instruction("bl __rt_array_key_exists");                            // return whether the integer key is in bounds
    emitter.instruction("b __elephc_eval_value_array_key_exists_box");          // box the existence flag for Rust
    emitter.label("__elephc_eval_value_array_key_exists_assoc");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed associative-array receiver
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the hash payload pointer
    emitter.instruction("cbz x0, __elephc_eval_value_array_key_exists_false");  // missing hash payload cannot contain a key
    emitter.instruction("ldr x1, [sp, #8]");                                    // pass normalized key_lo to the hash lookup
    emitter.instruction("ldr x2, [sp, #16]");                                   // pass normalized key_hi to the hash lookup
    emitter.instruction("bl __rt_hash_get");                                    // return hash found flag in x0
    emitter.instruction("b __elephc_eval_value_array_key_exists_box");          // box the hash existence flag for Rust
    emitter.label("__elephc_eval_value_array_key_exists_false");
    emitter.instruction("mov x0, #0");                                          // report false for misses and unsupported receivers
    emitter.label("__elephc_eval_value_array_key_exists_box");
    emitter.instruction("mov x1, x0");                                          // move the C bool result into mixed value_lo
    emitter.instruction("mov x0, #3");                                          // runtime tag 3 = boolean
    emitter.instruction("mov x2, xzr");                                         // boolean payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the bool result for Rust
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the key-exists wrapper frame
    emitter.instruction("ret");                                                 // return the boxed bool result to Rust

    label_c_global(emitter, "__elephc_eval_value_array_iter_key");
    emitter.instruction("sub sp, sp, #48");                                     // allocate a wrapper frame for insertion-order key iteration
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable iterator-key frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed array receiver while walking the container
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested zero-based foreach position
    emitter.instruction("cbz x0, __elephc_eval_value_array_iter_key_null");     // null handles produce a null key
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed Mixed runtime tag
    emitter.instruction("cmp x9, #4");                                          // tag 4 = indexed array
    emitter.instruction("b.eq __elephc_eval_value_array_iter_key_indexed");     // indexed arrays expose integer positions as foreach keys
    emitter.instruction("cmp x9, #5");                                          // tag 5 = associative array
    emitter.instruction("b.eq __elephc_eval_value_array_iter_key_assoc");       // associative arrays expose insertion-order hash keys
    emitter.instruction("b __elephc_eval_value_array_iter_key_null");           // scalar values have no foreach-visible key
    emitter.label("__elephc_eval_value_array_iter_key_indexed");
    emitter.instruction("ldr x1, [sp, #8]");                                    // use the requested foreach position as the integer key payload
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer key
    emitter.instruction("mov x2, xzr");                                         // integer keys do not use a high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the indexed foreach key as an owned Mixed cell
    emitter.instruction("b __elephc_eval_value_array_iter_key_done");           // return the boxed key to Rust
    emitter.label("__elephc_eval_value_array_iter_key_assoc");
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the hash payload pointer from the Mixed cell
    emitter.instruction("cbz x9, __elephc_eval_value_array_iter_key_null");     // null hash payloads produce a null key
    emitter.instruction("str x9, [sp, #16]");                                   // save the hash pointer for repeated iterator helper calls
    emitter.instruction("str xzr, [sp, #24]");                                  // start the insertion-order position counter at zero
    emitter.instruction("mov x1, xzr");                                         // cursor 0 starts at the hash head entry
    emitter.label("__elephc_eval_value_array_iter_key_assoc_loop");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the hash pointer before advancing the hash iterator
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next insertion-order hash key
    emitter.instruction("cmn x0, #1");                                          // did the iterator report the done sentinel?
    emitter.instruction("b.eq __elephc_eval_value_array_iter_key_null");        // out-of-range positions produce a null key
    emitter.instruction("ldr x10, [sp, #24]");                                  // load the current insertion-order position
    emitter.instruction("ldr x11, [sp, #8]");                                   // load the requested foreach position
    emitter.instruction("cmp x10, x11");                                        // is this the requested hash entry?
    emitter.instruction("b.eq __elephc_eval_value_array_iter_key_assoc_box");   // box the current hash key when the position matches
    emitter.instruction("add x10, x10, #1");                                    // advance the insertion-order position counter
    emitter.instruction("str x10, [sp, #24]");                                  // persist the updated position counter for the next probe
    emitter.instruction("mov x1, x0");                                          // use the returned cursor for the next hash iterator call
    emitter.instruction("b __elephc_eval_value_array_iter_key_assoc_loop");     // continue walking until the requested position is reached
    emitter.label("__elephc_eval_value_array_iter_key_assoc_box");
    emitter.instruction("cmn x2, #1");                                          // integer hash keys carry key_hi = -1
    emitter.instruction("b.ne __elephc_eval_value_array_iter_key_assoc_string"); // string hash keys need string-tag boxing
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer key
    emitter.instruction("mov x2, xzr");                                         // integer keys do not use a high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the associative integer key as Mixed
    emitter.instruction("b __elephc_eval_value_array_iter_key_done");           // return the boxed key to Rust
    emitter.label("__elephc_eval_value_array_iter_key_assoc_string");
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string key
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the associative string key as Mixed
    emitter.instruction("b __elephc_eval_value_array_iter_key_done");           // return the boxed key to Rust
    emitter.label("__elephc_eval_value_array_iter_key_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, xzr");                                         // null keys do not use a low payload word
    emitter.instruction("mov x2, xzr");                                         // null keys do not use a high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box null for invalid foreach-key requests
    emitter.label("__elephc_eval_value_array_iter_key_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the iterator-key wrapper frame
    emitter.instruction("ret");                                                 // return the boxed foreach key to Rust

    label_c_global(emitter, "__elephc_eval_value_array_set");
    emitter.instruction("sub sp, sp, #48");                                     // allocate a wrapper frame for key coercion and value retention
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed array receiver
    emitter.instruction("str x2, [sp, #8]");                                    // save the boxed value being written
    emitter.instruction("mov x0, x1");                                          // pass the boxed key to the eval key normalizer
    emitter.instruction("bl __elephc_eval_key_normalize");                      // normalize eval array key to key_lo/key_hi
    emitter.instruction("str x1, [sp, #16]");                                   // save the normalized key low word
    emitter.instruction("str x2, [sp, #24]");                                   // save the normalized key high word
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the value so the array consumes a retained owner
    emitter.instruction("bl __rt_incref");                                      // retain the boxed value for Mixed array storage
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass the boxed array receiver to the Mixed array setter
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the normalized key low word to the setter
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass the normalized key high word to the setter
    emitter.instruction("ldr x3, [sp, #8]");                                    // pass the retained boxed value to be consumed by the setter
    emitter.instruction("bl __rt_mixed_array_set");                             // mutate the boxed Mixed array through the shared runtime helper
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the target boxed array receiver to Rust
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the array-set wrapper frame
    emitter.instruction("ret");                                                 // return the boxed array Mixed cell to Rust

    label_c_global(emitter, "__elephc_eval_value_array_len");
    emitter.instruction("cbz x0, __elephc_eval_value_array_len_zero");          // null handles have no iterable eval elements
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed Mixed runtime tag
    emitter.instruction("cmp x9, #4");                                          // tag 4 = indexed array
    emitter.instruction("b.eq __elephc_eval_value_array_len_load");             // indexed arrays expose their header element count
    emitter.instruction("cmp x9, #5");                                          // tag 5 = associative array
    emitter.instruction("b.eq __elephc_eval_value_array_len_load");             // associative arrays expose their header entry count
    emitter.label("__elephc_eval_value_array_len_zero");
    emitter.instruction("mov x0, #0");                                          // scalar values have zero foreach-visible elements in this subset
    emitter.instruction("ret");                                                 // return the empty length to Rust
    emitter.label("__elephc_eval_value_array_len_load");
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the array/hash payload pointer from the Mixed cell
    emitter.instruction("cbz x9, __elephc_eval_value_array_len_zero");          // null payloads are treated as empty containers
    emitter.instruction("ldr x0, [x9]");                                        // load the runtime container element count
    emitter.instruction("ret");                                                 // return the element count to Rust

    label_c_global(emitter, "__elephc_eval_value_object_property_len");
    emitter.instruction("cbz x0, __elephc_eval_value_object_property_len_zero"); // null handles have no JSON-visible object properties
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed Mixed runtime tag
    emitter.instruction("cmp x9, #6");                                          // tag 6 = object
    emitter.instruction("b.ne __elephc_eval_value_object_property_len_zero");   // non-objects expose no JSON-visible properties here
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the object payload pointer
    emitter.instruction("cbz x9, __elephc_eval_value_object_property_len_zero"); // null object payloads have no visible properties
    abi::emit_symbol_address(emitter, "x10", "_stdclass_class_id");
    emitter.instruction("ldr x10, [x10]");                                      // load the compile-time stdClass class id
    emitter.instruction("ldr x11, [x9]");                                       // load the object's runtime class id
    emitter.instruction("cmp x11, x10");                                        // check whether the object is stdClass
    emitter.instruction("b.ne __elephc_eval_value_object_property_len_zero");   // non-stdClass objects expose no bridge-visible properties
    emitter.instruction("ldr x9, [x9, #8]");                                    // load stdClass dynamic-property hash pointer
    emitter.instruction("cbz x9, __elephc_eval_value_object_property_len_zero"); // null property hashes are treated as empty objects
    emitter.instruction("ldr x0, [x9]");                                        // load the hash entry count
    emitter.instruction("ret");                                                 // return the public property count to Rust
    emitter.label("__elephc_eval_value_object_property_len_zero");
    emitter.instruction("mov x0, #0");                                          // report zero JSON-visible object properties
    emitter.instruction("ret");                                                 // return the empty property count to Rust

    label_c_global(emitter, "__elephc_eval_value_object_property_iter_key");
    emitter.instruction("sub sp, sp, #48");                                     // allocate a wrapper frame for insertion-order property iteration
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable property-iterator frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the boxed object receiver while walking properties
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested zero-based property position
    emitter.instruction("cbz x0, __elephc_eval_value_object_property_iter_key_null"); // null handles produce a null property key
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed Mixed runtime tag
    emitter.instruction("cmp x9, #6");                                          // tag 6 = object
    emitter.instruction("b.ne __elephc_eval_value_object_property_iter_key_null"); // non-objects have no JSON-visible property key
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the object payload pointer
    emitter.instruction("cbz x9, __elephc_eval_value_object_property_iter_key_null"); // null object payloads produce a null key
    abi::emit_symbol_address(emitter, "x10", "_stdclass_class_id");
    emitter.instruction("ldr x10, [x10]");                                      // load the compile-time stdClass class id
    emitter.instruction("ldr x11, [x9]");                                       // load the object's runtime class id
    emitter.instruction("cmp x11, x10");                                        // check whether the object is stdClass
    emitter.instruction("b.ne __elephc_eval_value_object_property_iter_key_null"); // non-stdClass objects have no bridge-visible key
    emitter.instruction("ldr x9, [x9, #8]");                                    // load stdClass dynamic-property hash pointer
    emitter.instruction("cbz x9, __elephc_eval_value_object_property_iter_key_null"); // null property hashes produce a null key
    emitter.instruction("str x9, [sp, #16]");                                   // save the hash pointer for repeated iterator helper calls
    emitter.instruction("str xzr, [sp, #24]");                                  // start the insertion-order property counter at zero
    emitter.instruction("mov x1, xzr");                                         // cursor 0 starts at the property hash head entry
    emitter.label("__elephc_eval_value_object_property_iter_key_loop");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the hash pointer before advancing the iterator
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next insertion-order property key
    emitter.instruction("cmn x0, #1");                                          // did the iterator report the done sentinel?
    emitter.instruction("b.eq __elephc_eval_value_object_property_iter_key_null"); // out-of-range positions produce a null key
    emitter.instruction("ldr x10, [sp, #24]");                                  // load the current insertion-order property position
    emitter.instruction("ldr x11, [sp, #8]");                                   // load the requested property position
    emitter.instruction("cmp x10, x11");                                        // is this the requested property entry?
    emitter.instruction("b.eq __elephc_eval_value_object_property_iter_key_box"); // box the current property key when the position matches
    emitter.instruction("add x10, x10, #1");                                    // advance the insertion-order property counter
    emitter.instruction("str x10, [sp, #24]");                                  // persist the updated property counter
    emitter.instruction("mov x1, x0");                                          // use the returned cursor for the next iterator call
    emitter.instruction("b __elephc_eval_value_object_property_iter_key_loop"); // continue walking until the requested position is reached
    emitter.label("__elephc_eval_value_object_property_iter_key_box");
    emitter.instruction("cmn x2, #1");                                          // integer hash keys carry key_hi = -1
    emitter.instruction("b.ne __elephc_eval_value_object_property_iter_key_string"); // string property keys need string-tag boxing
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer key fallback
    emitter.instruction("mov x2, xzr");                                         // integer keys do not use a high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the integer property key as Mixed
    emitter.instruction("b __elephc_eval_value_object_property_iter_key_done"); // return the boxed key to Rust
    emitter.label("__elephc_eval_value_object_property_iter_key_string");
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string property key
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the string property key as Mixed
    emitter.instruction("b __elephc_eval_value_object_property_iter_key_done"); // return the boxed key to Rust
    emitter.label("__elephc_eval_value_object_property_iter_key_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, xzr");                                         // null keys do not use a low payload word
    emitter.instruction("mov x2, xzr");                                         // null keys do not use a high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box null for invalid property-key requests
    emitter.label("__elephc_eval_value_object_property_iter_key_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the property-iterator wrapper frame
    emitter.instruction("ret");                                                 // return the boxed property key to Rust

    emitter.label("__elephc_eval_key_normalize");
    emitter.instruction("sub sp, sp, #32");                                     // allocate a helper frame while classifying the boxed eval key
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across runtime calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the original boxed key for fallback integer casts
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose key tag plus payload words
    emitter.instruction("cmp x0, #1");                                          // is the eval key a string?
    emitter.instruction("b.eq __elephc_eval_key_normalize_string");             // normalize PHP string array keys through hash rules
    emitter.instruction("cmp x0, #0");                                          // is the eval key already an integer?
    emitter.instruction("b.eq __elephc_eval_key_normalize_int");                // integer keys only need the sentinel high word
    emitter.instruction("cmp x0, #8");                                          // is the eval key null?
    emitter.instruction("b.eq __elephc_eval_key_normalize_null");               // PHP treats null array keys as the empty string
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the original boxed key for PHP integer coercion
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce non-string keys to the current integer-key fallback
    emitter.instruction("mov x1, x0");                                          // publish the coerced integer key low word
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer array key
    emitter.instruction("b __elephc_eval_key_normalize_done");                  // return the fallback integer key tuple
    emitter.label("__elephc_eval_key_normalize_string");
    emitter.instruction("bl __rt_hash_normalize_key");                          // normalize numeric strings while preserving true string keys
    emitter.instruction("b __elephc_eval_key_normalize_done");                  // return the normalized string/int key tuple
    emitter.label("__elephc_eval_key_normalize_int");
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer array key
    emitter.instruction("b __elephc_eval_key_normalize_done");                  // finish integer key normalization
    emitter.label("__elephc_eval_key_normalize_null");
    emitter.instruction("mov x1, xzr");                                         // null array keys use the empty-string pointer
    emitter.instruction("mov x2, xzr");                                         // null array keys use the empty-string length
    emitter.label("__elephc_eval_key_normalize_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the key-normalizer helper frame
    emitter.instruction("ret");                                                 // return key_lo/key_hi in x1/x2

    label_c_global(emitter, "__elephc_eval_value_is_array_like");
    emitter.instruction("cbz x0, __elephc_eval_value_is_array_like_false");     // null handles cannot be indexed as arrays
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed Mixed runtime tag
    emitter.instruction("cmp x9, #4");                                          // tag 4 = indexed array
    emitter.instruction("b.eq __elephc_eval_value_is_array_like_true");         // indexed arrays are valid eval array-write receivers
    emitter.instruction("cmp x9, #5");                                          // tag 5 = associative array
    emitter.instruction("b.eq __elephc_eval_value_is_array_like_true");         // associative arrays are valid eval array-write receivers
    emitter.instruction("cmp x9, #6");                                          // tag 6 = object
    emitter.instruction("b.eq __elephc_eval_value_is_array_like_true");         // ArrayAccess-capable objects are delegated to runtime set helpers
    emitter.label("__elephc_eval_value_is_array_like_false");
    emitter.instruction("mov x0, #0");                                          // report false for scalar and null values
    emitter.instruction("ret");                                                 // return the boolean result to Rust
    emitter.label("__elephc_eval_value_is_array_like_true");
    emitter.instruction("mov x0, #1");                                          // report true for array-like values
    emitter.instruction("ret");                                                 // return the boolean result to Rust

    label_c_global(emitter, "__elephc_eval_value_is_null");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while unboxing the Mixed cell
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across mixed_unbox
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_unbox");                                 // unwrap nested Mixed cells to a concrete runtime tag
    emitter.instruction("cmp x0, #8");                                          // runtime tag 8 means PHP null
    emitter.instruction("cset x0, eq");                                         // return true when the unboxed tag is null
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the null-check wrapper frame
    emitter.instruction("ret");                                                 // return the boolean result to Rust

    label_c_global(emitter, "__elephc_eval_value_type_tag");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while unboxing the Mixed cell
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across mixed_unbox
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_unbox");                                 // unwrap nested Mixed cells and return the concrete runtime tag
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the type-tag wrapper frame
    emitter.instruction("ret");                                                 // return the unboxed runtime tag to Rust

    label_c_global(emitter, "__elephc_eval_value_object_identity");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while unboxing the object cell
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across mixed_unbox
    emitter.instruction("mov x29, sp");                                         // establish a stable object-identity wrapper frame
    emitter.instruction("bl __rt_mixed_unbox");                                 // unwrap nested Mixed cells to tag and object payload
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means PHP object
    emitter.instruction("csel x0, x1, xzr, eq");                                // return the object payload pointer or zero on mismatch
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the object-identity wrapper frame
    emitter.instruction("ret");                                                 // return the object identity pointer to Rust

    label_c_global(emitter, "__elephc_eval_value_cast_int");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while casting and boxing the value
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across helper calls
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_cast_int");                              // cast the boxed eval value to a PHP integer payload
    emitter.instruction("mov x1, x0");                                          // move the integer cast result into mixed value_lo
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the cast integer result for Rust
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the cast wrapper frame
    emitter.instruction("ret");                                                 // return the boxed integer cast result to Rust

    label_c_global(emitter, "__elephc_eval_value_cast_float");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while casting and boxing the value
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across helper calls
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the boxed eval value to a PHP double payload
    emitter.instruction("fmov x1, d0");                                         // move the double cast bits into mixed value_lo
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the cast double result for Rust
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the cast wrapper frame
    emitter.instruction("ret");                                                 // return the boxed double cast result to Rust

    label_c_global(emitter, "__elephc_eval_value_cast_string");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while unboxing and boxing the string result
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across helper calls
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the concrete payload tag and value words
    emitter.instruction("cmp x0, #0");                                          // is the eval value an integer?
    emitter.instruction("b.eq __elephc_eval_value_cast_string_int");            // integers cast through decimal formatting
    emitter.instruction("cmp x0, #1");                                          // is the eval value already a string?
    emitter.instruction("b.eq __elephc_eval_value_cast_string_box");            // strings can be boxed through the normal ownership path
    emitter.instruction("cmp x0, #2");                                          // is the eval value a double?
    emitter.instruction("b.eq __elephc_eval_value_cast_string_float");          // doubles cast through decimal formatting
    emitter.instruction("cmp x0, #3");                                          // is the eval value a boolean?
    emitter.instruction("b.eq __elephc_eval_value_cast_string_bool");           // booleans cast to "1" or the empty string
    emitter.label("__elephc_eval_value_cast_string_empty");
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("mov x1, xzr");                                         // unsupported and falsey payloads use an empty string pointer
    emitter.instruction("mov x2, xzr");                                         // unsupported and falsey payloads use an empty string length
    emitter.instruction("bl __rt_mixed_from_value");                            // box the empty string result for Rust
    emitter.instruction("b __elephc_eval_value_cast_string_done");              // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_cast_string_int");
    emitter.instruction("mov x0, x1");                                          // pass the integer payload to decimal formatting
    emitter.instruction("bl __rt_itoa");                                        // format the integer cast result as a string pair
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the formatted integer string
    emitter.instruction("b __elephc_eval_value_cast_string_done");              // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_cast_string_box");
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the existing string payload once
    emitter.instruction("b __elephc_eval_value_cast_string_done");              // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_cast_string_float");
    emitter.instruction("fmov d0, x1");                                         // move the double payload bits into the FP argument register
    emitter.instruction("bl __rt_ftoa");                                        // format the double cast result as a string pair
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the formatted double string
    emitter.instruction("b __elephc_eval_value_cast_string_done");              // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_cast_string_bool");
    emitter.instruction("cbz x1, __elephc_eval_value_cast_string_empty");       // false casts to the empty string
    emitter.instruction("mov x0, x1");                                          // pass the true payload to decimal formatting
    emitter.instruction("bl __rt_itoa");                                        // format true as the string "1"
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the true string result
    emitter.label("__elephc_eval_value_cast_string_done");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the string-cast wrapper frame
    emitter.instruction("ret");                                                 // return the boxed string cast result to Rust

    label_c_global(emitter, "__elephc_eval_value_cast_bool");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while casting and boxing the value
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across helper calls
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_cast_bool");                             // cast the boxed eval value to PHP truthiness
    emitter.instruction("mov x1, x0");                                          // move the boolean cast result into mixed value_lo
    emitter.instruction("mov x0, #3");                                          // runtime tag 3 = boolean
    emitter.instruction("mov x2, xzr");                                         // boolean payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the cast boolean result for Rust
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the cast wrapper frame
    emitter.instruction("ret");                                                 // return the boxed boolean cast result to Rust

    label_c_global(emitter, "__elephc_eval_value_int");
    emitter.instruction("mov x1, x0");                                          // move the C integer argument into the mixed payload slot
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("b __rt_mixed_from_value");                             // box the integer payload and return to Rust

    label_c_global(emitter, "__elephc_eval_value_resource");
    emitter.instruction("mov x1, x0");                                          // move the C resource id into the mixed payload slot
    emitter.instruction("mov x0, #9");                                          // runtime tag 9 = resource
    emitter.instruction("mov x2, xzr");                                         // resource payloads do not use a high word
    emitter.instruction("b __rt_mixed_from_value");                             // box the resource payload and return to Rust

    label_c_global(emitter, "__elephc_eval_value_float");
    emitter.instruction("fmov x1, d0");                                         // move the C double bits into the mixed payload slot
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("b __rt_mixed_from_value");                             // box the double payload and return to Rust

    label_c_global(emitter, "__elephc_eval_value_string");
    emitter.instruction("mov x2, x1");                                          // move the C string length into mixed value_hi
    emitter.instruction("mov x1, x0");                                          // move the C string pointer into mixed value_lo
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("b __rt_mixed_from_value");                             // persist and box the string payload for eval

    label_c_global(emitter, "__elephc_eval_value_abs");
    emitter.instruction("b __rt_abs_mixed");                                    // compute PHP abs() for one boxed eval value

    label_c_global(emitter, "__elephc_eval_value_ceil");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while casting and boxing ceil
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across helper calls
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the boxed eval argument to a PHP double for ceil
    emitter.bl_c("ceil");
    emitter.instruction("fmov x1, d0");                                         // move the ceil result bits into mixed value_lo
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the ceil result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the ceil wrapper frame
    emitter.instruction("ret");                                                 // return the boxed ceil result to Rust

    label_c_global(emitter, "__elephc_eval_value_floor");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while casting and boxing floor
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across helper calls
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the boxed eval argument to a PHP double for floor
    emitter.bl_c("floor");
    emitter.instruction("fmov x1, d0");                                         // move the floor result bits into mixed value_lo
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the floor result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the floor wrapper frame
    emitter.instruction("ret");                                                 // return the boxed floor result to Rust

    label_c_global(emitter, "__elephc_eval_value_sqrt");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while casting and boxing sqrt
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across helper calls
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the boxed eval argument to a PHP double for sqrt
    emitter.bl_c("sqrt");
    emitter.instruction("fmov x1, d0");                                         // move the sqrt result bits into mixed value_lo
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the sqrt result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the sqrt wrapper frame
    emitter.instruction("ret");                                                 // return the boxed sqrt result to Rust

    label_c_global(emitter, "__elephc_eval_value_strrev");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while casting and reversing
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across helper calls
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_cast_string");                           // cast the boxed eval argument to a PHP string pair
    emitter.instruction("bl __rt_strrev");                                      // reverse the PHP byte string into concat storage
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the reversed string for Rust
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the strrev wrapper frame
    emitter.instruction("ret");                                                 // return the boxed reversed string to Rust

    label_c_global(emitter, "__elephc_eval_value_fdiv");
    emitter.instruction("sub sp, sp, #32");                                     // allocate wrapper slots for the right operand and left double
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the left boxed operand to a PHP numeric double
    emitter.instruction("str d0, [sp, #8]");                                    // save the left double across the right cast
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for numeric casting
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the right boxed operand to a PHP numeric double
    emitter.instruction("fmov d1, d0");                                         // keep the right divisor in d1
    emitter.instruction("ldr d0, [sp, #8]");                                    // reload the left dividend into d0
    emitter.instruction("fdiv d0, d0, d1");                                     // compute fdiv() with IEEE zero handling
    emitter.instruction("fcmp d0, d0");                                         // detect NaN so PHP echo prints NAN without a sign
    emitter.instruction("b.vs __elephc_eval_value_fdiv_nan");                   // normalize unordered fdiv results before boxing
    emitter.instruction("fmov x1, d0");                                         // move the fdiv result bits into mixed value_lo
    emitter.instruction("b __elephc_eval_value_fdiv_box");                      // skip the canonical NaN payload path
    emitter.label("__elephc_eval_value_fdiv_nan");
    emitter.instruction("mov x1, xzr");                                         // start the canonical quiet NaN payload from zero bits
    emitter.instruction("movk x1, #0x7ff8, lsl #48");                           // install the positive quiet NaN exponent/significand
    emitter.label("__elephc_eval_value_fdiv_box");
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the fdiv result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the fdiv wrapper frame
    emitter.instruction("ret");                                                 // return the boxed fdiv result to Rust

    label_c_global(emitter, "__elephc_eval_value_fmod");
    emitter.instruction("sub sp, sp, #32");                                     // allocate wrapper slots for the right operand and left double
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the left boxed operand to a PHP numeric double
    emitter.instruction("str d0, [sp, #8]");                                    // save the left double across the right cast
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for numeric casting
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the right boxed operand to a PHP numeric double
    emitter.instruction("fmov d1, d0");                                         // keep the right divisor in d1
    emitter.instruction("ldr d0, [sp, #8]");                                    // reload the left dividend into d0
    emitter.instruction("fdiv d2, d0, d1");                                     // compute the fmod quotient before truncation
    emitter.instruction("frintz d2, d2");                                       // truncate the quotient toward zero
    emitter.instruction("fmsub d0, d2, d1, d0");                                // compute dividend minus truncated quotient times divisor
    emitter.instruction("fcmp d0, d0");                                         // detect NaN so PHP echo prints NAN without a sign
    emitter.instruction("b.vs __elephc_eval_value_fmod_nan");                   // normalize unordered fmod results before boxing
    emitter.instruction("fmov x1, d0");                                         // move the fmod result bits into mixed value_lo
    emitter.instruction("b __elephc_eval_value_fmod_box");                      // skip the canonical NaN payload path
    emitter.label("__elephc_eval_value_fmod_nan");
    emitter.instruction("mov x1, xzr");                                         // start the canonical quiet NaN payload from zero bits
    emitter.instruction("movk x1, #0x7ff8, lsl #48");                           // install the positive quiet NaN exponent/significand
    emitter.label("__elephc_eval_value_fmod_box");
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the fmod result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the fmod wrapper frame
    emitter.instruction("ret");                                                 // return the boxed fmod result to Rust

    label_c_global(emitter, "__elephc_eval_value_add");
    emitter.instruction("b __rt_mixed_numeric_add");                            // add two boxed mixed values and return the boxed result

    label_c_global(emitter, "__elephc_eval_value_sub");
    emitter.instruction("b __rt_mixed_numeric_sub");                            // subtract two boxed mixed values and return the boxed result

    label_c_global(emitter, "__elephc_eval_value_mul");
    emitter.instruction("b __rt_mixed_numeric_mul");                            // multiply two boxed mixed values and return the boxed result

    label_c_global(emitter, "__elephc_eval_value_div");
    emitter.instruction("sub sp, sp, #32");                                     // allocate wrapper slots for the right operand and left double
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the left boxed operand to a PHP numeric double
    emitter.instruction("str d0, [sp, #8]");                                    // save the left double across the right cast
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for numeric casting
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the right boxed operand to a PHP numeric double
    emitter.instruction("fcmp d0, #0.0");                                       // detect division by zero before the hardware divide
    emitter.instruction("b.eq __elephc_eval_value_div_null");                   // return null until eval has throwable propagation
    emitter.instruction("fmov d1, d0");                                         // keep the right divisor in d1
    emitter.instruction("ldr d0, [sp, #8]");                                    // reload the left dividend into d0
    emitter.instruction("fdiv d0, d0, d1");                                     // compute PHP division as a double result
    emitter.instruction("fmov x1, d0");                                         // move the double bits into mixed value_lo
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the division result into a Mixed cell
    emitter.instruction("b __elephc_eval_value_div_done");                      // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_div_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null fallback for division by zero
    emitter.instruction("mov x1, xzr");                                         // null has no low payload word
    emitter.instruction("mov x2, xzr");                                         // null has no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box null for unsupported division-by-zero propagation
    emitter.label("__elephc_eval_value_div_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the division wrapper frame
    emitter.instruction("ret");                                                 // return the boxed division result to Rust

    label_c_global(emitter, "__elephc_eval_value_mod");
    emitter.instruction("sub sp, sp, #32");                                     // allocate wrapper slots for the right operand and left integer
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("bl __rt_mixed_cast_int");                              // cast the left boxed operand to a PHP integer
    emitter.instruction("str x0, [sp, #8]");                                    // save the left integer across the right cast
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for integer casting
    emitter.instruction("bl __rt_mixed_cast_int");                              // cast the right boxed operand to a PHP integer
    emitter.instruction("cbz x0, __elephc_eval_value_mod_null");                // return null until eval has throwable propagation
    emitter.instruction("mov x2, x0");                                          // keep the integer divisor in x2
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the integer dividend into x1
    emitter.instruction("sdiv x3, x1, x2");                                     // compute the signed integer quotient
    emitter.instruction("msub x1, x3, x2, x1");                                 // compute dividend - quotient * divisor
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("bl __rt_mixed_from_value");                            // box the modulo result into a Mixed cell
    emitter.instruction("b __elephc_eval_value_mod_done");                      // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_mod_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null fallback for modulo by zero
    emitter.instruction("mov x1, xzr");                                         // null has no low payload word
    emitter.instruction("mov x2, xzr");                                         // null has no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box null for unsupported modulo-by-zero propagation
    emitter.label("__elephc_eval_value_mod_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the modulo wrapper frame
    emitter.instruction("ret");                                                 // return the boxed modulo result to Rust

    label_c_global(emitter, "__elephc_eval_value_pow");
    emitter.instruction("sub sp, sp, #32");                                     // allocate wrapper slots for the right operand and left double
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the left boxed operand to a PHP numeric double
    emitter.instruction("str d0, [sp, #8]");                                    // save the exponentiation base across the right cast
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for numeric casting
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the right boxed operand to a PHP numeric double
    emitter.instruction("fmov d1, d0");                                         // move the exponent into libc pow's second argument
    emitter.instruction("ldr d0, [sp, #8]");                                    // reload the base into libc pow's first argument
    emitter.bl_c("pow");
    emitter.instruction("fmov x1, d0");                                         // move the pow result bits into mixed value_lo
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the exponentiation result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the exponentiation wrapper frame
    emitter.instruction("ret");                                                 // return the boxed exponentiation result to Rust

    label_c_global(emitter, "__elephc_eval_value_round");
    emitter.instruction("sub sp, sp, #48");                                     // allocate wrapper slots for precision state and saved doubles
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the optional precision cell while casting the value
    emitter.instruction("str x2, [sp, #8]");                                    // save whether the caller supplied a precision argument
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the boxed eval value to a PHP numeric double
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the precision-presence flag after the value cast
    emitter.instruction("cbnz x2, __elephc_eval_value_round_precision");        // use the precision path when a second argument is present
    emitter.bl_c("round");
    emitter.instruction("b __elephc_eval_value_round_box");                     // box the default-precision round result
    emitter.label("__elephc_eval_value_round_precision");
    emitter.instruction("str d0, [sp, #16]");                                   // save the original value while casting the precision
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the precision cell for integer casting
    emitter.instruction("bl __rt_mixed_cast_int");                              // cast the optional precision to a PHP integer
    emitter.instruction("scvtf d1, x0");                                        // convert the precision to a floating exponent for pow
    emitter.instruction("fmov d0, #10.0");                                      // materialize 10.0 as the precision multiplier base
    emitter.bl_c("pow");
    emitter.instruction("ldr d1, [sp, #16]");                                   // reload the original value after pow returns the multiplier
    emitter.instruction("fmul d1, d1, d0");                                     // scale the value by the precision multiplier
    emitter.instruction("str d0, [sp, #24]");                                   // save the multiplier for rescaling after round
    emitter.instruction("fmov d0, d1");                                         // move the scaled value into the round argument
    emitter.bl_c("round");
    emitter.instruction("ldr d1, [sp, #24]");                                   // reload the precision multiplier for rescaling
    emitter.instruction("fdiv d0, d0, d1");                                     // scale the rounded value back to requested precision
    emitter.label("__elephc_eval_value_round_box");
    emitter.instruction("fmov x1, d0");                                         // move the round result bits into mixed value_lo
    emitter.instruction("mov x2, xzr");                                         // double payloads do not use a high word
    emitter.instruction("mov x0, #2");                                          // runtime tag 2 = double
    emitter.instruction("bl __rt_mixed_from_value");                            // box the round result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the round wrapper frame
    emitter.instruction("ret");                                                 // return the boxed round result to Rust

    label_c_global(emitter, "__elephc_eval_value_bit_not");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame for the cast helper call
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across the cast
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_mixed_cast_int");                              // cast the boxed operand to a PHP integer
    emitter.instruction("mvn x1, x0");                                          // compute bitwise complement of the integer payload
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("bl __rt_mixed_from_value");                            // box the bitwise NOT result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the bitwise NOT wrapper frame
    emitter.instruction("ret");                                                 // return the boxed bitwise NOT result to Rust

    label_c_global(emitter, "__elephc_eval_value_bitwise");
    emitter.instruction("sub sp, sp, #48");                                     // allocate wrapper slots for right operand, opcode, and left integer
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("str x2, [sp, #8]");                                    // save the eval bitwise opcode across helper calls
    emitter.instruction("bl __rt_mixed_cast_int");                              // cast the left boxed operand to a PHP integer
    emitter.instruction("str x0, [sp, #16]");                                   // save the left integer across the right cast
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for integer casting
    emitter.instruction("bl __rt_mixed_cast_int");                              // cast the right boxed operand to a PHP integer
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the left integer into the payload register
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the eval bitwise opcode for dispatch
    emitter.instruction("cmp x2, #0");                                          // is this integer bitwise AND?
    emitter.instruction("b.eq __elephc_eval_value_bitwise_and");                // route opcode 0 to integer AND
    emitter.instruction("cmp x2, #1");                                          // is this integer bitwise OR?
    emitter.instruction("b.eq __elephc_eval_value_bitwise_or");                 // route opcode 1 to integer OR
    emitter.instruction("cmp x2, #2");                                          // is this integer bitwise XOR?
    emitter.instruction("b.eq __elephc_eval_value_bitwise_xor");                // route opcode 2 to integer XOR
    emitter.instruction("cmp x2, #3");                                          // is this integer left shift?
    emitter.instruction("b.eq __elephc_eval_value_bitwise_shl");                // route opcode 3 to integer left shift
    emitter.instruction("cmp x2, #4");                                          // is this integer right shift?
    emitter.instruction("b.eq __elephc_eval_value_bitwise_shr");                // route opcode 4 to integer right shift
    emitter.instruction("b __elephc_eval_value_bitwise_null");                  // fail closed for unknown bitwise opcodes
    emitter.label("__elephc_eval_value_bitwise_and");
    emitter.instruction("and x1, x1, x0");                                      // compute integer bitwise AND
    emitter.instruction("b __elephc_eval_value_bitwise_box");                   // box the integer bitwise result
    emitter.label("__elephc_eval_value_bitwise_or");
    emitter.instruction("orr x1, x1, x0");                                      // compute integer bitwise OR
    emitter.instruction("b __elephc_eval_value_bitwise_box");                   // box the integer bitwise result
    emitter.label("__elephc_eval_value_bitwise_xor");
    emitter.instruction("eor x1, x1, x0");                                      // compute integer bitwise XOR
    emitter.instruction("b __elephc_eval_value_bitwise_box");                   // box the integer bitwise result
    emitter.label("__elephc_eval_value_bitwise_shl");
    emitter.instruction("cmp x0, #0");                                          // negative shift counts are runtime errors in PHP
    emitter.instruction("b.lt __elephc_eval_value_bitwise_null");               // return null until eval has throwable propagation
    emitter.instruction("lsl x1, x1, x0");                                      // shift the integer payload left
    emitter.instruction("b __elephc_eval_value_bitwise_box");                   // box the integer shift result
    emitter.label("__elephc_eval_value_bitwise_shr");
    emitter.instruction("cmp x0, #0");                                          // negative shift counts are runtime errors in PHP
    emitter.instruction("b.lt __elephc_eval_value_bitwise_null");               // return null until eval has throwable propagation
    emitter.instruction("asr x1, x1, x0");                                      // shift the integer payload right arithmetically
    emitter.instruction("b __elephc_eval_value_bitwise_box");                   // box the integer shift result
    emitter.label("__elephc_eval_value_bitwise_box");
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("bl __rt_mixed_from_value");                            // box the bitwise result into a Mixed cell
    emitter.instruction("b __elephc_eval_value_bitwise_done");                  // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_bitwise_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null fallback for unsupported bitwise errors
    emitter.instruction("mov x1, xzr");                                         // null has no low payload word
    emitter.instruction("mov x2, xzr");                                         // null has no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box null for unsupported bitwise error propagation
    emitter.label("__elephc_eval_value_bitwise_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the bitwise wrapper frame
    emitter.instruction("ret");                                                 // return the boxed bitwise result to Rust

    label_c_global(emitter, "__elephc_eval_value_concat");
    emitter.instruction("sub sp, sp, #64");                                     // allocate wrapper frame for the right operand and string pairs
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("bl __rt_mixed_cast_string");                           // cast the left boxed operand to a PHP string pair
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the left string pointer and length
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for string casting
    emitter.instruction("bl __rt_mixed_cast_string");                           // cast the right boxed operand to a PHP string pair
    emitter.instruction("mov x3, x1");                                          // move the right string pointer into concat's right pointer register
    emitter.instruction("mov x4, x2");                                          // move the right string length into concat's right length register
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // reload the left string pair for concat
    emitter.instruction("bl __rt_concat");                                      // concatenate the two PHP string pairs
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string for boxing the concat result
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the concatenated string
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the concat wrapper frame
    emitter.instruction("ret");                                                 // return the boxed concat result to Rust

    label_c_global(emitter, "__elephc_eval_value_compare");
    emitter.instruction("sub sp, sp, #64");                                     // allocate a wrapper frame for comparison operands and opcode
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address across comparison helpers
    emitter.instruction("add x29, sp, #48");                                    // establish a stable comparison wrapper frame
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand for later casts
    emitter.instruction("str x2, [sp, #8]");                                    // save the eval comparison opcode
    emitter.instruction("str x0, [sp, #16]");                                   // save the left boxed operand for equality helper calls
    emitter.instruction("cmp x2, #0");                                          // is this loose equality?
    emitter.instruction("b.eq __elephc_eval_value_compare_eq");                 // route == through the mixed loose-equality helper
    emitter.instruction("cmp x2, #1");                                          // is this loose inequality?
    emitter.instruction("b.eq __elephc_eval_value_compare_ne");                 // route != through the mixed loose-equality helper
    emitter.instruction("cmp x2, #6");                                          // is this strict equality?
    emitter.instruction("b.eq __elephc_eval_value_compare_strict_eq");          // route === through the mixed strict-equality helper
    emitter.instruction("cmp x2, #7");                                          // is this strict inequality?
    emitter.instruction("b.eq __elephc_eval_value_compare_strict_ne");          // route !== through the mixed strict-equality helper
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the left boxed operand to a numeric comparison double
    emitter.instruction("str d0, [sp, #24]");                                   // save the normalized left numeric operand
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for numeric casting
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the right boxed operand to a numeric comparison double
    emitter.instruction("ldr d1, [sp, #24]");                                   // reload the left numeric operand for the float comparison
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the eval comparison opcode for dispatch
    emitter.instruction("cmp x9, #2");                                          // is this a less-than comparison?
    emitter.instruction("b.eq __elephc_eval_value_compare_lt");                 // materialize left < right from float comparison flags
    emitter.instruction("cmp x9, #3");                                          // is this a less-than-or-equal comparison?
    emitter.instruction("b.eq __elephc_eval_value_compare_lte");                // materialize left <= right from float comparison flags
    emitter.instruction("cmp x9, #4");                                          // is this a greater-than comparison?
    emitter.instruction("b.eq __elephc_eval_value_compare_gt");                 // materialize left > right from float comparison flags
    emitter.instruction("cmp x9, #5");                                          // is this a greater-than-or-equal comparison?
    emitter.instruction("b.eq __elephc_eval_value_compare_gte");                // materialize left >= right from float comparison flags
    emitter.instruction("mov x1, #0");                                          // unknown comparison opcodes fail closed as false
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the fallback false result
    emitter.label("__elephc_eval_value_compare_eq");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the left operand for loose equality
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the right operand for loose equality
    emitter.instruction("bl __elephc_eval_mixed_loose_eq");                     // compute scalar PHP loose equality
    emitter.instruction("mov x1, x0");                                          // move equality into the bool payload register
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the equality result
    emitter.label("__elephc_eval_value_compare_ne");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the left operand for loose inequality
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the right operand for loose inequality
    emitter.instruction("bl __elephc_eval_mixed_loose_eq");                     // compute scalar PHP loose equality before inversion
    emitter.instruction("eor x1, x0, #1");                                      // invert equality for the != operator
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the inequality result
    emitter.label("__elephc_eval_value_compare_strict_eq");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the left operand for strict equality
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the right operand for strict equality
    emitter.instruction("bl __rt_mixed_strict_eq");                             // compute PHP strict equality
    emitter.instruction("mov x1, x0");                                          // move strict equality into the bool payload register
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the strict-equality result
    emitter.label("__elephc_eval_value_compare_strict_ne");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the left operand for strict inequality
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the right operand for strict inequality
    emitter.instruction("bl __rt_mixed_strict_eq");                             // compute PHP strict equality before inversion
    emitter.instruction("eor x1, x0, #1");                                      // invert equality for the !== operator
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the strict-inequality result
    emitter.label("__elephc_eval_value_compare_lt");
    emitter.instruction("fcmp d1, d0");                                         // compare numeric eval operands for <
    emitter.instruction("cset x1, mi");                                         // ordered less-than becomes boolean true
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the less-than result
    emitter.label("__elephc_eval_value_compare_lte");
    emitter.instruction("fcmp d1, d0");                                         // compare numeric eval operands for <=
    emitter.instruction("cset x1, ls");                                         // ordered less-than-or-equal becomes boolean true
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the less-than-or-equal result
    emitter.label("__elephc_eval_value_compare_gt");
    emitter.instruction("fcmp d1, d0");                                         // compare numeric eval operands for >
    emitter.instruction("cset x1, gt");                                         // ordered greater-than becomes boolean true
    emitter.instruction("b __elephc_eval_value_compare_box");                   // box the greater-than result
    emitter.label("__elephc_eval_value_compare_gte");
    emitter.instruction("fcmp d1, d0");                                         // compare numeric eval operands for >=
    emitter.instruction("cset x1, ge");                                         // ordered greater-than-or-equal becomes boolean true
    emitter.label("__elephc_eval_value_compare_box");
    emitter.instruction("mov x0, #3");                                          // runtime tag 3 = bool
    emitter.instruction("mov x2, xzr");                                         // bool payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the comparison result as a Mixed bool
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the comparison wrapper frame
    emitter.instruction("ret");                                                 // return the boxed comparison result to Rust

    emitter.label("__elephc_eval_mixed_loose_eq");
    emitter.instruction("sub sp, sp, #96");                                     // allocate helper slots for unboxed tags, payloads, and casts
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address across mixed helper calls
    emitter.instruction("add x29, sp, #80");                                    // establish a stable loose-equality helper frame
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save incoming boxed operands for later casts
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox the left eval operand into tag and payload words
    emitter.instruction("str x0, [sp, #16]");                                   // save the left runtime tag
    emitter.instruction("stp x1, x2, [sp, #24]");                               // save the left payload words
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed operand for unboxing
    emitter.instruction("bl __rt_mixed_unbox");                                 // unbox the right eval operand into tag and payload words
    emitter.instruction("str x0, [sp, #40]");                                   // save the right runtime tag
    emitter.instruction("stp x1, x2, [sp, #48]");                               // save the right payload words
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the left runtime tag for equality dispatch
    emitter.instruction("cmp x9, #3");                                          // does the left operand have PHP bool semantics?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_bool");              // bool comparisons use truthiness on both operands
    emitter.instruction("cmp x0, #3");                                          // does the right operand have PHP bool semantics?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_bool");              // bool comparisons use truthiness on both operands
    emitter.instruction("cmp x9, x0");                                          // do the operands have the same runtime tag?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_same_tag");          // same-tag scalars use focused payload comparisons
    emitter.instruction("cmp x9, #8");                                          // is the left operand null?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_left_null");         // null compares equal only to empty strings before numeric fallback
    emitter.instruction("cmp x0, #8");                                          // is the right operand null?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_right_null");        // null compares equal only to empty strings before numeric fallback
    emitter.instruction("cmp x9, #1");                                          // is a non-matching left operand a string?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_left_string");       // compare numeric strings against numeric scalars
    emitter.instruction("cmp x0, #1");                                          // is a non-matching right operand a string?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_right_string");      // compare numeric strings against numeric scalars
    emitter.instruction("b __elephc_eval_mixed_loose_eq_numeric");              // remaining scalar mismatches compare numerically
    emitter.label("__elephc_eval_mixed_loose_eq_same_tag");
    emitter.instruction("cmp x9, #8");                                          // are both operands null?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_true");              // null loosely equals null
    emitter.instruction("cmp x9, #1");                                          // are both operands strings?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_strings");           // strings use PHP loose string equality
    emitter.instruction("cmp x9, #2");                                          // are both operands floats?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_floats");            // floats compare with native floating equality
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the left low payload word
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload the right low payload word
    emitter.instruction("cmp x10, x11");                                        // compare low payload words for int and pointer-like scalars
    emitter.instruction("b.ne __elephc_eval_mixed_loose_eq_false");             // mismatched low payloads are not equal
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the left high payload word
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload the right high payload word
    emitter.instruction("cmp x10, x11");                                        // compare high payload words for pointer-like scalars
    emitter.instruction("cset x0, eq");                                         // materialize same-tag payload equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the payload equality result
    emitter.label("__elephc_eval_mixed_loose_eq_strings");
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // reload the left string pointer and length
    emitter.instruction("ldp x3, x4, [sp, #48]");                               // reload the right string pointer and length
    emitter.instruction("bl __rt_str_loose_eq");                                // compare strings with PHP loose numeric-string rules
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the string loose-equality result
    emitter.label("__elephc_eval_mixed_loose_eq_floats");
    emitter.instruction("ldr d1, [sp, #24]");                                   // reload the left float payload
    emitter.instruction("ldr d0, [sp, #48]");                                   // reload the right float payload
    emitter.instruction("fcmp d1, d0");                                         // compare same-tag float payloads
    emitter.instruction("cset x0, eq");                                         // floats loosely equal only when ordered equal
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the float equality result
    emitter.label("__elephc_eval_mixed_loose_eq_left_null");
    emitter.instruction("cmp x0, #1");                                          // is null being compared with a string?
    emitter.instruction("b.ne __elephc_eval_mixed_loose_eq_numeric");           // non-string null comparisons fall back to numeric zero
    emitter.instruction("ldr x10, [sp, #56]");                                  // load the right string length
    emitter.instruction("cmp x10, #0");                                         // null loosely equals only the empty string
    emitter.instruction("cset x0, eq");                                         // materialize the null/string equality result
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the null/string equality result
    emitter.label("__elephc_eval_mixed_loose_eq_right_null");
    emitter.instruction("cmp x9, #1");                                          // is null being compared with a string?
    emitter.instruction("b.ne __elephc_eval_mixed_loose_eq_numeric");           // non-string null comparisons fall back to numeric zero
    emitter.instruction("ldr x10, [sp, #32]");                                  // load the left string length
    emitter.instruction("cmp x10, #0");                                         // null loosely equals only the empty string
    emitter.instruction("cset x0, eq");                                         // materialize the string/null equality result
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the string/null equality result
    emitter.label("__elephc_eval_mixed_loose_eq_left_string");
    emitter.instruction("cmp x0, #0");                                          // can the right operand be compared numerically as an int?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_left_string_numeric"); // parse the left string for numeric equality
    emitter.instruction("cmp x0, #2");                                          // can the right operand be compared numerically as a float?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_left_string_numeric"); // parse the left string for numeric equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_false");                // non-numeric string mismatches are not loosely equal here
    emitter.label("__elephc_eval_mixed_loose_eq_left_string_numeric");
    emitter.instruction("ldp x1, x2, [sp, #24]");                               // reload the left string pointer and length for numeric parsing
    emitter.instruction("bl __rt_str_to_number");                               // parse the left string under PHP numeric-string rules
    emitter.instruction("cbz x0, __elephc_eval_mixed_loose_eq_false");          // non-numeric strings do not equal numeric scalars
    emitter.instruction("str d0, [sp, #64]");                                   // save the parsed left numeric-string value
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed operand for numeric casting
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the right operand to a comparison double
    emitter.instruction("ldr d1, [sp, #64]");                                   // reload the parsed left numeric-string value
    emitter.instruction("fcmp d1, d0");                                         // compare parsed string and numeric scalar values
    emitter.instruction("cset x0, eq");                                         // materialize string/numeric loose equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the string/numeric equality result
    emitter.label("__elephc_eval_mixed_loose_eq_right_string");
    emitter.instruction("cmp x9, #0");                                          // can the left operand be compared numerically as an int?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_right_string_numeric"); // parse the right string for numeric equality
    emitter.instruction("cmp x9, #2");                                          // can the left operand be compared numerically as a float?
    emitter.instruction("b.eq __elephc_eval_mixed_loose_eq_right_string_numeric"); // parse the right string for numeric equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_false");                // non-numeric string mismatches are not loosely equal here
    emitter.label("__elephc_eval_mixed_loose_eq_right_string_numeric");
    emitter.instruction("ldp x1, x2, [sp, #48]");                               // reload the right string pointer and length for numeric parsing
    emitter.instruction("bl __rt_str_to_number");                               // parse the right string under PHP numeric-string rules
    emitter.instruction("cbz x0, __elephc_eval_mixed_loose_eq_false");          // non-numeric strings do not equal numeric scalars
    emitter.instruction("str d0, [sp, #64]");                                   // save the parsed right numeric-string value
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left boxed operand for numeric casting
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the left operand to a comparison double
    emitter.instruction("ldr d1, [sp, #64]");                                   // reload the parsed right numeric-string value
    emitter.instruction("fcmp d0, d1");                                         // compare numeric scalar and parsed string values
    emitter.instruction("cset x0, eq");                                         // materialize numeric/string loose equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the numeric/string equality result
    emitter.label("__elephc_eval_mixed_loose_eq_bool");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left boxed operand for truthiness
    emitter.instruction("bl __rt_mixed_cast_bool");                             // cast the left operand to PHP truthiness
    emitter.instruction("str x0, [sp, #64]");                                   // save the left truthiness result
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed operand for truthiness
    emitter.instruction("bl __rt_mixed_cast_bool");                             // cast the right operand to PHP truthiness
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the left truthiness result
    emitter.instruction("cmp x9, x0");                                          // compare boolean truthiness for loose equality
    emitter.instruction("cset x0, eq");                                         // materialize bool loose equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the bool loose-equality result
    emitter.label("__elephc_eval_mixed_loose_eq_numeric");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the left boxed operand for numeric equality
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the left operand to a comparison double
    emitter.instruction("str d0, [sp, #64]");                                   // save the left numeric equality operand
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the right boxed operand for numeric equality
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the right operand to a comparison double
    emitter.instruction("ldr d1, [sp, #64]");                                   // reload the left numeric equality operand
    emitter.instruction("fcmp d1, d0");                                         // compare numeric operands for loose equality
    emitter.instruction("cset x0, eq");                                         // materialize numeric loose equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the numeric loose-equality result
    emitter.label("__elephc_eval_mixed_loose_eq_true");
    emitter.instruction("mov x0, #1");                                          // materialize true for loose equality
    emitter.instruction("b __elephc_eval_mixed_loose_eq_done");                 // return the true result
    emitter.label("__elephc_eval_mixed_loose_eq_false");
    emitter.instruction("mov x0, #0");                                          // materialize false for loose equality
    emitter.label("__elephc_eval_mixed_loose_eq_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the loose-equality helper frame
    emitter.instruction("ret");                                                 // return the loose-equality boolean in x0

    label_c_global(emitter, "__elephc_eval_value_spaceship");
    emitter.instruction("sub sp, sp, #32");                                     // allocate wrapper slots for the right operand and left double
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across helper calls
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the right boxed operand while casting the left operand
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the left boxed operand to a PHP numeric double
    emitter.instruction("str d0, [sp, #8]");                                    // save the left numeric spaceship operand
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the right boxed operand for numeric casting
    emitter.instruction("bl __rt_mixed_cast_float");                            // cast the right boxed operand to a PHP numeric double
    emitter.instruction("ldr d1, [sp, #8]");                                    // reload the left numeric spaceship operand
    emitter.instruction("fcmp d1, d0");                                         // compare left and right numeric operands for spaceship
    emitter.instruction("b.vs __elephc_eval_value_spaceship_gt");               // PHP treats unordered NaN spaceship comparisons as greater
    emitter.instruction("cset x1, gt");                                         // set result to 1 when left is greater than right
    emitter.instruction("csinv x1, x1, xzr, ge");                               // keep 1/0 for greater/equal, or produce -1 for less
    emitter.instruction("b __elephc_eval_value_spaceship_box");                 // box the ordered spaceship result
    emitter.label("__elephc_eval_value_spaceship_gt");
    emitter.instruction("mov x1, #1");                                          // greater or unordered comparisons produce result 1
    emitter.label("__elephc_eval_value_spaceship_box");
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("bl __rt_mixed_from_value");                            // box the spaceship result into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the spaceship wrapper frame
    emitter.instruction("ret");                                                 // return the boxed spaceship result to Rust

    label_c_global(emitter, "__elephc_eval_value_echo");
    emitter.instruction("b __rt_mixed_write_stdout");                           // echo one boxed mixed value and return to Rust

    label_c_global(emitter, "__elephc_eval_value_string_bytes");
    emitter.instruction("sub sp, sp, #48");                                     // allocate a wrapper frame for output pointers
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across string casting
    emitter.instruction("add x29, sp, #32");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the caller's out_ptr storage address
    emitter.instruction("str x2, [sp, #8]");                                    // save the caller's out_len storage address
    emitter.instruction("bl __rt_mixed_cast_string");                           // cast the boxed eval value to a PHP string pair
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the optional out_ptr storage address
    emitter.instruction("cbz x9, __elephc_eval_value_string_bytes_len");        // skip pointer storage when the caller passed null
    emitter.instruction("str x1, [x9]");                                        // store the string pointer for Rust to copy immediately
    emitter.label("__elephc_eval_value_string_bytes_len");
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the optional out_len storage address
    emitter.instruction("cbz x10, __elephc_eval_value_string_bytes_done");      // skip length storage when the caller passed null
    emitter.instruction("str x2, [x10]");                                       // store the string byte length for Rust
    emitter.label("__elephc_eval_value_string_bytes_done");
    emitter.instruction("mov x0, #1");                                          // report successful string conversion to Rust
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the string-bytes wrapper frame
    emitter.instruction("ret");                                                 // return the success flag to Rust

    label_c_global(emitter, "__elephc_eval_value_truthy");
    emitter.instruction("b __rt_mixed_cast_bool");                              // cast one boxed mixed value to PHP truthiness for eval

    label_c_global(emitter, "__elephc_eval_value_retain");
    emitter.instruction("b __rt_incref");                                       // retain one eval-owned boxed Mixed cell

    label_c_global(emitter, "__elephc_eval_warning");
    emitter.instruction("mov x2, x1");                                          // move warning length into the runtime diagnostic length register
    emitter.instruction("mov x1, x0");                                          // move warning pointer into the runtime diagnostic buffer register
    emitter.instruction("b __rt_diag_warning");                                 // emit or suppress one eval runtime warning

    label_c_global(emitter, "__elephc_eval_value_release");
    emitter.instruction("b __rt_decref_mixed");                                 // release one eval-owned boxed Mixed cell
}

/// Emits Linux x86_64 C-ABI wrappers around the internal mixed value helpers.
fn emit_x86_64_wrappers(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_null");
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null has no low payload word
    emitter.instruction("xor esi, esi");                                        // null has no high payload word
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the null payload and return to Rust

    label_c_global(emitter, "__elephc_eval_value_bool");
    emitter.instruction("xor r10d, r10d");                                      // prepare the normalized PHP bool payload
    emitter.instruction("test rdi, rdi");                                       // treat any non-zero C bool argument as true
    emitter.instruction("setne r10b");                                          // bool payload is 1 for true and 0 for false
    emitter.instruction("mov rdi, r10");                                        // move the normalized bool into mixed value_lo
    emitter.instruction("mov eax, 3");                                          // runtime tag 3 = bool
    emitter.instruction("xor esi, esi");                                        // bool payloads do not use a high word
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the bool payload and return to Rust

    label_c_global(emitter, "__elephc_eval_value_new_object");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable dynamic-object wrapper frame
    emitter.instruction("cmp rsi, 8");                                          // stdClass has an 8-byte class name
    emitter.instruction("jne __elephc_eval_value_new_object_generic_x86");      // use the generic factory for non-stdClass lengths
    emitter.instruction("cmp BYTE PTR [rdi], 115");                             // byte 0 must be 's'
    emitter.instruction("jne __elephc_eval_value_new_object_generic_x86");      // fall back when byte 0 differs
    emitter.instruction("cmp BYTE PTR [rdi + 1], 116");                         // byte 1 must be 't'
    emitter.instruction("jne __elephc_eval_value_new_object_generic_x86");      // fall back when byte 1 differs
    emitter.instruction("cmp BYTE PTR [rdi + 2], 100");                         // byte 2 must be 'd'
    emitter.instruction("jne __elephc_eval_value_new_object_generic_x86");      // fall back when byte 2 differs
    emitter.instruction("cmp BYTE PTR [rdi + 3], 67");                          // byte 3 must be 'C'
    emitter.instruction("jne __elephc_eval_value_new_object_generic_x86");      // fall back when byte 3 differs
    emitter.instruction("cmp BYTE PTR [rdi + 4], 108");                         // byte 4 must be 'l'
    emitter.instruction("jne __elephc_eval_value_new_object_generic_x86");      // fall back when byte 4 differs
    emitter.instruction("cmp BYTE PTR [rdi + 5], 97");                          // byte 5 must be 'a'
    emitter.instruction("jne __elephc_eval_value_new_object_generic_x86");      // fall back when byte 5 differs
    emitter.instruction("cmp BYTE PTR [rdi + 6], 115");                         // byte 6 must be 's'
    emitter.instruction("jne __elephc_eval_value_new_object_generic_x86");      // fall back when byte 6 differs
    emitter.instruction("cmp BYTE PTR [rdi + 7], 115");                         // byte 7 must be 's'
    emitter.instruction("jne __elephc_eval_value_new_object_generic_x86");      // fall back when byte 7 differs
    emitter.instruction("call __rt_stdclass_new");                              // allocate stdClass with its dynamic-property hash
    emitter.instruction("jmp __elephc_eval_value_new_object_box_x86");          // box the stdClass object for Rust
    emitter.label("__elephc_eval_value_new_object_generic_x86");
    emitter.instruction("mov rax, rdi");                                        // move the C class-name pointer into new_by_name's string ABI
    emitter.instruction("mov rdx, rsi");                                        // move the C class-name length into new_by_name's string ABI
    emitter.instruction("call __rt_new_by_name");                               // allocate the named AOT class object, or return null on miss
    emitter.instruction("test rax, rax");                                       // did the runtime class-name lookup allocate an object?
    emitter.instruction("jz __elephc_eval_value_new_object_null_x86");          // box PHP null when no runtime class matched the eval name
    emitter.label("__elephc_eval_value_new_object_box_x86");
    emitter.instruction("mov rdi, rax");                                        // move the allocated object pointer into the Mixed payload
    emitter.instruction("mov eax, 6");                                          // runtime tag 6 = object
    emitter.instruction("xor esi, esi");                                        // object payloads do not use a high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the allocated object for Rust
    emitter.instruction("jmp __elephc_eval_value_new_object_done_x86");         // skip the null boxing path after successful allocation
    emitter.label("__elephc_eval_value_new_object_null_x86");
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null has no low payload word
    emitter.instruction("xor esi, esi");                                        // null has no high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box null for unknown eval class names
    emitter.label("__elephc_eval_value_new_object_done_x86");
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed object or null Mixed cell to Rust

    label_c_global(emitter, "__elephc_eval_class_exists");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable class-exists frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve slots for name, count, cursor, and index
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    abi::emit_symbol_address(emitter, "r10", "_classes_by_name_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the registered class-name count
    emitter.instruction("test r10, r10");                                       // is the class-name table empty?
    emitter.instruction("jz __elephc_eval_class_exists_miss_x86");              // an empty table cannot contain the requested class
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the table count across string compares
    abi::emit_symbol_address(emitter, "r11", "_classes_by_name");
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the current class-name table cursor
    emitter.instruction("xor r11d, r11d");                                      // start scanning at table index zero
    emitter.label("__elephc_eval_class_exists_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the class-name table count
    emitter.instruction("cmp r11, r10");                                        // have all class-name entries been scanned?
    emitter.instruction("jae __elephc_eval_class_exists_miss_x86");             // no class matched before the end of the table
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current class-name table entry
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load the stored class-name length
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare stored and requested class-name lengths
    emitter.instruction("jne __elephc_eval_class_exists_skip_x86");             // length mismatch means this entry cannot match
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the table index across the string compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested class-name length
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // pass the stored class-name pointer
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // restore the table index after the string compare
    emitter.instruction("test rax, rax");                                       // did the requested class name match this entry?
    emitter.instruction("je __elephc_eval_class_exists_hit_x86");               // report true on a class-name match
    emitter.label("__elephc_eval_class_exists_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current class-name table entry
    emitter.instruction("add r10, 32");                                         // advance to the next class-name table entry
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the advanced table cursor
    emitter.instruction("inc r11");                                             // advance the table index
    emitter.instruction("jmp __elephc_eval_class_exists_loop_x86");             // continue scanning the class-name table
    emitter.label("__elephc_eval_class_exists_hit_x86");
    emitter.instruction("mov eax, 1");                                          // return true for a matched class name
    emitter.instruction("jmp __elephc_eval_class_exists_done_x86");             // skip the false result after a match
    emitter.label("__elephc_eval_class_exists_miss_x86");
    emitter.instruction("xor eax, eax");                                        // return false when no class-name entry matched
    emitter.label("__elephc_eval_class_exists_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the class-exists flag to Rust

    emit_x86_64_eval_name_table_exists(
        emitter,
        "__elephc_eval_interface_exists",
        "_interface_names_count",
        "_interface_names",
        "__elephc_eval_interface_exists_x86",
    );

    emit_x86_64_eval_name_table_exists(
        emitter,
        "__elephc_eval_trait_exists",
        "_trait_names_count",
        "_trait_names",
        "__elephc_eval_trait_exists_x86",
    );
    emit_x86_64_eval_name_table_exists(
        emitter,
        "__elephc_eval_enum_exists",
        "_enum_names_count",
        "_enum_names",
        "__elephc_eval_enum_exists_x86",
    );

    emit_x86_64_eval_reflection_method_flags(emitter);
    emit_x86_64_eval_reflection_property_flags(emitter);

    label_c_global(emitter, "__elephc_eval_value_is_a");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across runtime match helpers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable is-a relation frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve slots for value pointer, flags, and target metadata
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the boxed eval object-or-class cell
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // save whether exact class matches should be rejected
    emitter.instruction("mov rax, rsi");                                        // move the target string pointer into the lookup ABI register
    emitter.instruction("call __rt_instanceof_lookup");                         // resolve the target class/interface string to matcher metadata
    emitter.instruction("test rax, rax");                                       // did the target string resolve to emitted metadata?
    emitter.instruction("je __elephc_eval_value_is_a_false_x86");               // unresolved targets cannot match eval object values
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the target class/interface id
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the target kind: 0 class, 1 interface
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the boxed eval value for unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // unwrap nested Mixed cells to tag and payload words
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means the eval value is an object
    emitter.instruction("je __elephc_eval_value_is_a_object_x86");              // object values can use their concrete runtime class id
    emitter.instruction("cmp rax, 1");                                          // runtime tag 1 means the eval value is a class string
    emitter.instruction("je __elephc_eval_value_is_a_string_x86");              // class-string values need source metadata lookup
    emitter.instruction("jmp __elephc_eval_value_is_a_false_x86");              // other runtime tags cannot satisfy class relations
    emitter.label("__elephc_eval_value_is_a_string_x86");
    emitter.instruction("mov rax, rdi");                                        // pass the source class-string pointer to the metadata lookup
    emitter.instruction("call __rt_instanceof_lookup");                         // resolve the source class string to matcher metadata
    emitter.instruction("test rax, rax");                                       // did the source string resolve to emitted metadata?
    emitter.instruction("je __elephc_eval_value_is_a_false_x86");               // unresolved source strings cannot match relation metadata
    emitter.instruction("test rdx, rdx");                                       // source strings must resolve to concrete classes for this matcher
    emitter.instruction("jne __elephc_eval_value_is_a_false_x86");              // interface-source strings need a dedicated interface-parent matcher
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // build a fake object header containing the source class id
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // does this call reject exact concrete-class matches?
    emitter.instruction("je __elephc_eval_value_is_a_string_match_x86");        // is_a() allows exact class-string matches
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // is the target a concrete class rather than an interface?
    emitter.instruction("jne __elephc_eval_value_is_a_string_match_x86");       // interface targets cannot be exact concrete-class self matches
    emitter.instruction("cmp rdi, QWORD PTR [rbp - 24]");                       // compare source and target class ids for subclass self exclusion
    emitter.instruction("je __elephc_eval_value_is_a_false_x86");               // is_subclass_of() excludes the exact class string
    emitter.label("__elephc_eval_value_is_a_string_match_x86");
    emitter.instruction("lea rdi, [rbp - 40]");                                 // pass the fake object header to the metadata matcher
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the target class/interface id
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // pass the target kind: 0 class, 1 interface
    emitter.instruction("call __rt_exception_matches");                         // test class-string inheritance or implemented interfaces
    emitter.instruction("jmp __elephc_eval_value_is_a_done_x86");               // keep the matcher result and restore the wrapper frame
    emitter.label("__elephc_eval_value_is_a_object_x86");
    emitter.instruction("test rdi, rdi");                                       // check the unboxed object pointer before reading its header
    emitter.instruction("je __elephc_eval_value_is_a_false_x86");               // malformed object payloads cannot match class metadata
    emitter.instruction("mov r8, rdi");                                         // keep the unboxed object pointer for matcher input
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // does this call reject exact concrete-class matches?
    emitter.instruction("je __elephc_eval_value_is_a_match_x86");               // is_a() allows exact class matches
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // is the target a concrete class rather than an interface?
    emitter.instruction("jne __elephc_eval_value_is_a_match_x86");              // interface targets cannot be exact concrete-class self matches
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the object's concrete runtime class id
    emitter.instruction("cmp r9, QWORD PTR [rbp - 24]");                        // compare object and target class ids for subclass self exclusion
    emitter.instruction("je __elephc_eval_value_is_a_false_x86");               // is_subclass_of() excludes the object's exact class
    emitter.label("__elephc_eval_value_is_a_match_x86");
    emitter.instruction("mov rdi, r8");                                         // pass the unboxed object pointer to the metadata matcher
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the target class/interface id
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // pass the target kind: 0 class, 1 interface
    emitter.instruction("call __rt_exception_matches");                         // test inheritance or implemented-interface metadata
    emitter.instruction("jmp __elephc_eval_value_is_a_done_x86");               // keep the matcher result and restore the wrapper frame
    emitter.label("__elephc_eval_value_is_a_false_x86");
    emitter.instruction("xor eax, eax");                                        // return false for unresolved, scalar, or exact-self subclass cases
    emitter.label("__elephc_eval_value_is_a_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // discard relation lookup spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boolean class-relation result to Rust

    label_c_global(emitter, "__elephc_eval_value_object_class_name");
    emitter.instruction("test rdi, rdi");                                       // reject null boxed handles before reading their tag
    emitter.instruction("jz __elephc_eval_value_object_class_name_miss_x86");   // null handles cannot provide a class name
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed eval value runtime tag
    emitter.instruction("cmp r10, 6");                                          // tag 6 is an object payload
    emitter.instruction("jne __elephc_eval_value_object_class_name_miss_x86");  // non-objects cannot provide a class name
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the object payload pointer
    emitter.instruction("test r10, r10");                                       // check the unboxed object pointer before dereferencing it
    emitter.instruction("jz __elephc_eval_value_object_class_name_miss_x86");   // reject malformed object payloads
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the object's runtime class id
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_class_name_count", 0);
    emitter.instruction("cmp r11, rdx");                                        // check whether the class id is in table bounds
    emitter.instruction("jae __elephc_eval_value_object_class_name_miss_x86");  // reject missing or out-of-range class ids
    abi::emit_symbol_address(emitter, "rdx", "_class_name_entries");
    emitter.instruction("shl r11, 4");                                          // convert class id to a 16-byte table-entry offset
    emitter.instruction("add rdx, r11");                                        // address the class-name entry for this class id
    emitter.instruction("mov rdi, QWORD PTR [rdx]");                            // load the class-name string pointer
    emitter.instruction("mov rsi, QWORD PTR [rdx + 8]");                        // load the class-name string length
    emitter.instruction("test rsi, rsi");                                       // table holes use a zero-length name
    emitter.instruction("jz __elephc_eval_value_object_class_name_miss_x86");   // reject table holes with empty names
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("jmp __rt_mixed_from_value");                           // persist and box the class-name string for Rust
    emitter.label("__elephc_eval_value_object_class_name_miss_x86");
    emitter.instruction("xor eax, eax");                                        // report failure as a null C pointer to Rust
    emitter.instruction("ret");                                                 // return the failure sentinel

    label_c_global(emitter, "__elephc_eval_value_parent_class_name");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable parent-class lookup frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve lookup state while keeping the stack call-aligned
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_unbox input
    emitter.instruction("call __rt_mixed_unbox");                               // expose the eval value tag and payload words
    emitter.instruction("cmp rax, 6");                                          // tag 6 is an object payload
    emitter.instruction("je __elephc_eval_value_parent_class_name_object_x86"); // derive the parent from the object's runtime class id
    emitter.instruction("cmp rax, 1");                                          // tag 1 is a class-name string payload
    emitter.instruction("je __elephc_eval_value_parent_class_name_string_x86"); // resolve a class string through generated metadata
    emitter.instruction("jmp __elephc_eval_value_parent_class_name_empty_x86"); // unsupported input types have no parent class name
    emitter.label("__elephc_eval_value_parent_class_name_object_x86");
    emitter.instruction("test rdi, rdi");                                       // check the unboxed object pointer before reading its header
    emitter.instruction("jz __elephc_eval_value_parent_class_name_empty_x86");  // malformed object payloads have no parent class
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // load the object's runtime class id
    emitter.instruction("jmp __elephc_eval_value_parent_class_name_from_id_x86"); // convert the class id to its parent class name
    emitter.label("__elephc_eval_value_parent_class_name_string_x86");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the requested class-name length
    abi::emit_symbol_address(emitter, "r10", "_classes_by_name_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the registered class-name count
    emitter.instruction("test r10, r10");                                       // is the generated class-name table empty?
    emitter.instruction("jz __elephc_eval_value_parent_class_name_empty_x86");  // an empty class table cannot resolve a parent name
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the table count across string compares
    abi::emit_symbol_address(emitter, "r11", "_classes_by_name");
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the current class-name table cursor
    emitter.instruction("xor r11d, r11d");                                      // start scanning generated class-name entries at index zero
    emitter.label("__elephc_eval_value_parent_class_name_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the class-name table count
    emitter.instruction("cmp r11, r10");                                        // have all generated class names been checked?
    emitter.instruction("jae __elephc_eval_value_parent_class_name_empty_x86"); // no generated class matched the requested string
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current class-name metadata entry
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load the stored class-name length
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare stored and requested name lengths first
    emitter.instruction("jne __elephc_eval_value_parent_class_name_skip_x86");  // length mismatch means this class entry cannot match
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // preserve the scan index across the string compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested class-name length
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // pass the generated class-name pointer
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // restore the scan index after the string compare
    emitter.instruction("test rax, rax");                                       // did the requested class name match this entry?
    emitter.instruction("je __elephc_eval_value_parent_class_name_hit_x86");    // resolve the matched class entry to its parent id
    emitter.label("__elephc_eval_value_parent_class_name_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current class-name table entry
    emitter.instruction("add r10, 32");                                         // advance to the next class-name table entry
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the advanced table cursor
    emitter.instruction("inc r11");                                             // advance the class-name scan index
    emitter.instruction("jmp __elephc_eval_value_parent_class_name_loop_x86");  // continue scanning generated class names
    emitter.label("__elephc_eval_value_parent_class_name_hit_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the matched class-name table entry
    emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                       // load the matched runtime class id
    emitter.label("__elephc_eval_value_parent_class_name_from_id_x86");
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_class_name_count", 0);
    emitter.instruction("cmp r11, rdx");                                        // check that the class id can index parent metadata
    emitter.instruction("jae __elephc_eval_value_parent_class_name_empty_x86"); // unknown class ids have no parent class name
    abi::emit_symbol_address(emitter, "rdx", "_class_parent_ids");
    emitter.instruction("mov r11, QWORD PTR [rdx + r11 * 8]");                  // load the parent runtime class id
    emitter.instruction("cmp r11, -1");                                         // check whether the runtime class has no parent
    emitter.instruction("je __elephc_eval_value_parent_class_name_empty_x86");  // parentless runtime classes produce an empty string
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_class_name_count", 0);
    emitter.instruction("cmp r11, rdx");                                        // check that the parent class id can index name metadata
    emitter.instruction("jae __elephc_eval_value_parent_class_name_empty_x86"); // invalid parent ids produce an empty string
    abi::emit_symbol_address(emitter, "rdx", "_class_name_entries");
    emitter.instruction("shl r11, 4");                                          // convert parent id to a 16-byte name-entry offset
    emitter.instruction("add rdx, r11");                                        // address the parent class-name metadata row
    emitter.instruction("mov rdi, QWORD PTR [rdx]");                            // load the parent class-name string pointer
    emitter.instruction("mov rsi, QWORD PTR [rdx + 8]");                        // load the parent class-name string length
    emitter.instruction("test rsi, rsi");                                       // table holes represent missing parent names
    emitter.instruction("jz __elephc_eval_value_parent_class_name_empty_x86");  // missing parent names produce an empty string
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the parent class-name string
    emitter.instruction("jmp __elephc_eval_value_parent_class_name_done_x86");  // restore the wrapper frame before returning to Rust
    emitter.label("__elephc_eval_value_parent_class_name_empty_x86");
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("xor edi, edi");                                        // missing parent names use an empty string pointer
    emitter.instruction("xor esi, esi");                                        // missing parent names use an empty string length
    emitter.instruction("call __rt_mixed_from_value");                          // box the empty parent class-name string
    emitter.label("__elephc_eval_value_parent_class_name_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed parent class-name string to Rust

    label_c_global(emitter, "__elephc_eval_value_array_new");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve local slots for the array pointer
    emitter.instruction("cmp rdi, 4");                                          // compare requested capacity with the minimum capacity
    emitter.instruction("mov r10, 4");                                          // minimum indexed-array capacity for eval literals
    emitter.instruction("cmovb rdi, r10");                                      // use max(requested, 4) as the runtime allocation capacity
    emitter.instruction("mov rsi, 8");                                          // Mixed indexed arrays store boxed-cell pointers
    emitter.instruction("call __rt_array_new");                                 // allocate indexed-array storage for boxed Mixed slots
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the packed indexed-array heap kind word
    emitter.instruction("mov r11, 0xffffffff000080ff");                         // preserve heap magic, indexed-array kind, and COW metadata
    emitter.instruction("and r10, r11");                                        // clear the default scalar value_type bits
    emitter.instruction("mov r11, 7");                                          // runtime value_type 7 = boxed Mixed
    emitter.instruction("shl r11, 8");                                          // move the value_type tag into the packed kind word
    emitter.instruction("or r10, r11");                                         // stamp the array as carrying boxed Mixed slots
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // persist the updated indexed-array metadata
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the owned array pointer while allocating the Mixed box
    emitter.instruction("mov rax, 24");                                         // Mixed cells store tag plus two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate a boxed Mixed cell without retaining the new array
    emitter.instruction(&x86_64_mixed_heap_kind_instruction());                 // materialize the mixed-cell heap kind with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // install the mixed-cell heap kind in the uniform header
    emitter.instruction("mov QWORD PTR [rax], 4");                              // runtime tag 4 = indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the owned indexed-array pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the array pointer as the Mixed low payload word
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // indexed arrays do not use the high payload word
    emitter.instruction("add rsp, 16");                                         // release the array-new wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed array Mixed cell to Rust

    label_c_global(emitter, "__elephc_eval_value_string_array_new");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve local slots for the string-array pointer
    emitter.instruction("cmp rdi, 4");                                          // compare requested capacity with the minimum capacity
    emitter.instruction("mov r10, 4");                                          // minimum indexed-array capacity for eval metadata lists
    emitter.instruction("cmovb rdi, r10");                                      // use max(requested, 4) as the runtime allocation capacity
    emitter.instruction("mov rsi, 16");                                         // direct string arrays store pointer/length pairs
    emitter.instruction("call __rt_array_new");                                 // allocate indexed-array storage for direct string slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the owned direct-string array pointer while boxing it
    emitter.instruction("mov rax, 24");                                         // Mixed cells store tag plus two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate a boxed Mixed cell without retaining the new array
    emitter.instruction(&x86_64_mixed_heap_kind_instruction());                 // materialize the mixed-cell heap kind with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // install the mixed-cell heap kind in the uniform header
    emitter.instruction("mov QWORD PTR [rax], 4");                              // runtime tag 4 = indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the owned direct-string array pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the string-array pointer as the Mixed low payload word
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // indexed arrays do not use the high payload word
    emitter.instruction("add rsp, 16");                                         // release the string-array-new wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed direct-string array Mixed cell to Rust

    label_c_global(emitter, "__elephc_eval_value_string_array_push");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve local slots for boxed owner and incoming string payload
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the boxed string-array owner
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the incoming string pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the incoming string length
    emitter.instruction("test rdi, rdi");                                       // check whether the boxed string-array handle is null
    emitter.instruction("jz __elephc_eval_value_string_array_push_fail_x86");   // reject malformed null string-array handles
    emitter.instruction("mov rax, rdi");                                        // move the boxed owner into mixed_unbox's input register
    emitter.instruction("call __rt_mixed_unbox");                               // expose the indexed-array tag and payload pointer
    emitter.instruction("cmp rax, 4");                                          // runtime tag 4 means indexed array
    emitter.instruction("jne __elephc_eval_value_string_array_push_fail_x86");  // reject non-array metadata containers
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the string pointer to append
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the string length to append
    emitter.instruction("call __rt_array_push_str");                            // persist and append the string, returning the updated array payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the boxed string-array owner
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // update the boxed payload in case the array grew
    emitter.instruction("mov rax, r10");                                        // return the boxed string-array owner to Rust
    emitter.instruction("jmp __elephc_eval_value_string_array_push_done_x86");  // skip the malformed-input null result
    emitter.label("__elephc_eval_value_string_array_push_fail_x86");
    emitter.instruction("xor eax, eax");                                        // report a null pointer so Rust converts it to RuntimeFatal
    emitter.label("__elephc_eval_value_string_array_push_done_x86");
    emitter.instruction("add rsp, 32");                                         // release the string-array-push wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the updated boxed string-array handle to Rust

    label_c_global(emitter, "__elephc_eval_value_assoc_new");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across runtime calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve local slots for the hash pointer
    emitter.instruction("cmp rdi, 16");                                         // compare requested capacity with the minimum hash capacity
    emitter.instruction("mov r10, 16");                                         // minimum hash capacity for eval associative literals
    emitter.instruction("cmovb rdi, r10");                                      // use max(requested, 16) as the hash allocation capacity
    emitter.instruction("mov rsi, 7");                                          // runtime value_type 7 = boxed Mixed hash values
    emitter.instruction("call __rt_hash_new");                                  // allocate associative-array storage for boxed Mixed entries
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the owned hash pointer while allocating the Mixed box
    emitter.instruction("mov rax, 24");                                         // Mixed cells store tag plus two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate a boxed Mixed cell without retaining the new hash
    emitter.instruction(&x86_64_mixed_heap_kind_instruction());                 // materialize the mixed-cell heap kind with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // install the mixed-cell heap kind in the uniform header
    emitter.instruction("mov QWORD PTR [rax], 5");                              // runtime tag 5 = associative array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the owned hash pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the hash pointer as the Mixed low payload word
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // associative arrays do not use the high payload word
    emitter.instruction("add rsp, 16");                                         // release the assoc-new wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed associative-array Mixed cell to Rust

    label_c_global(emitter, "__elephc_eval_value_array_get");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve local slots for the boxed array receiver
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the boxed array receiver while coercing the key
    emitter.instruction("mov rdi, rsi");                                        // pass the boxed key to the eval key normalizer
    emitter.instruction("call __elephc_eval_key_normalize");                    // normalize eval array key to key_lo/key_hi
    emitter.instruction("mov rsi, rax");                                        // pass normalized key_lo to the reader
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the boxed array receiver
    emitter.instruction("call __rt_mixed_array_get");                           // read the boxed Mixed element or Mixed(null)
    emitter.instruction("add rsp, 16");                                         // release the array-get wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed element to Rust

    label_c_global(emitter, "__elephc_eval_value_array_key_exists");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve slots for receiver and normalized key words
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the boxed array receiver while normalizing the key
    emitter.instruction("call __elephc_eval_key_normalize");                    // normalize eval array key to key_lo/key_hi
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the normalized key low word
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the normalized key high word
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the boxed array receiver for tag dispatch
    emitter.instruction("test rdi, rdi");                                       // null handles do not contain array keys
    emitter.instruction("jz __elephc_eval_value_array_key_exists_false");       // report false for null runtime cells
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed Mixed runtime tag
    emitter.instruction("cmp r10, 4");                                          // tag 4 = indexed array
    emitter.instruction("je __elephc_eval_value_array_key_exists_indexed");     // indexed arrays use bounds-based key existence
    emitter.instruction("cmp r10, 5");                                          // tag 5 = associative array
    emitter.instruction("je __elephc_eval_value_array_key_exists_assoc");       // associative arrays use hash existence
    emitter.instruction("jmp __elephc_eval_value_array_key_exists_false");      // scalar values do not contain array keys
    emitter.label("__elephc_eval_value_array_key_exists_indexed");
    emitter.instruction("cmp QWORD PTR [rbp - 24], -1");                        // integer keys carry key_hi = -1
    emitter.instruction("jne __elephc_eval_value_array_key_exists_false");      // non-integer keys never exist in indexed arrays
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the boxed indexed-array receiver
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // load the indexed-array payload pointer
    emitter.instruction("test rdi, rdi");                                       // missing payload cannot contain a key
    emitter.instruction("jz __elephc_eval_value_array_key_exists_false");       // report false for missing indexed-array payloads
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass normalized integer key to the bounds helper
    emitter.instruction("call __rt_array_key_exists");                          // return whether the integer key is in bounds
    emitter.instruction("jmp __elephc_eval_value_array_key_exists_box");        // box the existence flag for Rust
    emitter.label("__elephc_eval_value_array_key_exists_assoc");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the boxed associative-array receiver
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // load the hash payload pointer
    emitter.instruction("test rdi, rdi");                                       // missing hash payload cannot contain a key
    emitter.instruction("jz __elephc_eval_value_array_key_exists_false");       // report false for missing associative-array payloads
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass normalized key_lo to the hash lookup
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // pass normalized key_hi to the hash lookup
    emitter.instruction("call __rt_hash_get");                                  // return hash found flag in rax
    emitter.instruction("jmp __elephc_eval_value_array_key_exists_box");        // box the hash existence flag for Rust
    emitter.label("__elephc_eval_value_array_key_exists_false");
    emitter.instruction("xor eax, eax");                                        // report false for misses and unsupported receivers
    emitter.label("__elephc_eval_value_array_key_exists_box");
    emitter.instruction("mov rdi, rax");                                        // move the C bool result into mixed value_lo
    emitter.instruction("mov eax, 3");                                          // runtime tag 3 = boolean
    emitter.instruction("xor esi, esi");                                        // boolean payloads do not use a high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the bool result for Rust
    emitter.instruction("add rsp, 32");                                         // release the key-exists wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed bool result to Rust

    label_c_global(emitter, "__elephc_eval_value_array_iter_key");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable iterator-key wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve slots for receiver, target position, hash pointer, and counter
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the boxed array receiver while walking the container
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested zero-based foreach position
    emitter.instruction("test rdi, rdi");                                       // null handles produce a null key
    emitter.instruction("jz __elephc_eval_value_array_iter_key_null");          // branch to boxed null for null runtime cells
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed Mixed runtime tag
    emitter.instruction("cmp r10, 4");                                          // tag 4 = indexed array
    emitter.instruction("je __elephc_eval_value_array_iter_key_indexed");       // indexed arrays expose integer positions as foreach keys
    emitter.instruction("cmp r10, 5");                                          // tag 5 = associative array
    emitter.instruction("je __elephc_eval_value_array_iter_key_assoc");         // associative arrays expose insertion-order hash keys
    emitter.instruction("jmp __elephc_eval_value_array_iter_key_null");         // scalar values have no foreach-visible key
    emitter.label("__elephc_eval_value_array_iter_key_indexed");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // use the requested foreach position as the integer key payload
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer key
    emitter.instruction("xor esi, esi");                                        // integer keys do not use a high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box the indexed foreach key as an owned Mixed cell
    emitter.instruction("jmp __elephc_eval_value_array_iter_key_done");         // return the boxed key to Rust
    emitter.label("__elephc_eval_value_array_iter_key_assoc");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the hash payload pointer from the Mixed cell
    emitter.instruction("test r10, r10");                                       // null hash payloads produce a null key
    emitter.instruction("jz __elephc_eval_value_array_iter_key_null");          // branch to boxed null for missing hash payloads
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the hash pointer for repeated iterator helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the insertion-order position counter at zero
    emitter.instruction("xor esi, esi");                                        // cursor 0 starts at the hash head entry
    emitter.label("__elephc_eval_value_array_iter_key_assoc_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the hash pointer before advancing the hash iterator
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next insertion-order hash key
    emitter.instruction("cmp rax, -1");                                         // did the iterator report the done sentinel?
    emitter.instruction("je __elephc_eval_value_array_iter_key_null");          // out-of-range positions produce a null key
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // load the current insertion-order position
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // load the requested foreach position
    emitter.instruction("cmp r10, r11");                                        // is this the requested hash entry?
    emitter.instruction("je __elephc_eval_value_array_iter_key_assoc_box");     // box the current hash key when the position matches
    emitter.instruction("add r10, 1");                                          // advance the insertion-order position counter
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the updated position counter for the next probe
    emitter.instruction("mov rsi, rax");                                        // use the returned cursor for the next hash iterator call
    emitter.instruction("jmp __elephc_eval_value_array_iter_key_assoc_loop");   // continue walking until the requested position is reached
    emitter.label("__elephc_eval_value_array_iter_key_assoc_box");
    emitter.instruction("cmp rdx, -1");                                         // integer hash keys carry key_hi = -1
    emitter.instruction("jne __elephc_eval_value_array_iter_key_assoc_string"); // string hash keys need string-tag boxing
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer key
    emitter.instruction("xor esi, esi");                                        // integer keys do not use a high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box the associative integer key as Mixed
    emitter.instruction("jmp __elephc_eval_value_array_iter_key_done");         // return the boxed key to Rust
    emitter.label("__elephc_eval_value_array_iter_key_assoc_string");
    emitter.instruction("mov rsi, rdx");                                        // move the string key length into the boxing high word
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string key
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the associative string key as Mixed
    emitter.instruction("jmp __elephc_eval_value_array_iter_key_done");         // return the boxed key to Rust
    emitter.label("__elephc_eval_value_array_iter_key_null");
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null keys do not use a low payload word
    emitter.instruction("xor esi, esi");                                        // null keys do not use a high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box null for invalid foreach-key requests
    emitter.label("__elephc_eval_value_array_iter_key_done");
    emitter.instruction("add rsp, 32");                                         // release the iterator-key wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed foreach key to Rust

    label_c_global(emitter, "__elephc_eval_value_array_set");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve local slots for receiver, value, and key
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the boxed array receiver
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the boxed value being written
    emitter.instruction("mov rdi, rsi");                                        // pass the boxed key to the eval key normalizer
    emitter.instruction("call __elephc_eval_key_normalize");                    // normalize eval array key to key_lo/key_hi
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the normalized key low word
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the normalized key high word
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the value so the array consumes a retained owner
    emitter.instruction("call __rt_incref");                                    // retain the boxed value for Mixed array storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the boxed array receiver to the Mixed array setter
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the normalized key low word to the setter
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // pass the normalized key high word to the setter
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // pass the retained boxed value to be consumed by the setter
    emitter.instruction("call __rt_mixed_array_set");                           // mutate the boxed Mixed array through the shared runtime helper
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the target boxed array receiver to Rust
    emitter.instruction("add rsp, 32");                                         // release the array-set wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed array Mixed cell to Rust

    label_c_global(emitter, "__elephc_eval_value_array_len");
    emitter.instruction("test rdi, rdi");                                       // null handles have no iterable eval elements
    emitter.instruction("jz __elephc_eval_value_array_len_zero");               // report empty length for null runtime cells
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed Mixed runtime tag
    emitter.instruction("cmp r10, 4");                                          // tag 4 = indexed array
    emitter.instruction("je __elephc_eval_value_array_len_load");               // indexed arrays expose their header element count
    emitter.instruction("cmp r10, 5");                                          // tag 5 = associative array
    emitter.instruction("je __elephc_eval_value_array_len_load");               // associative arrays expose their header entry count
    emitter.label("__elephc_eval_value_array_len_zero");
    emitter.instruction("xor eax, eax");                                        // scalar values have zero foreach-visible elements in this subset
    emitter.instruction("ret");                                                 // return the empty length to Rust
    emitter.label("__elephc_eval_value_array_len_load");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the array/hash payload pointer from the Mixed cell
    emitter.instruction("test r10, r10");                                       // is the container payload null?
    emitter.instruction("jz __elephc_eval_value_array_len_zero");               // null payloads are treated as empty containers
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the runtime container element count
    emitter.instruction("ret");                                                 // return the element count to Rust

    label_c_global(emitter, "__elephc_eval_value_object_property_len");
    emitter.instruction("test rdi, rdi");                                       // null handles have no JSON-visible object properties
    emitter.instruction("jz __elephc_eval_value_object_property_len_zero");     // report zero properties for null runtime cells
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed Mixed runtime tag
    emitter.instruction("cmp r10, 6");                                          // tag 6 = object
    emitter.instruction("jne __elephc_eval_value_object_property_len_zero");    // non-objects expose no JSON-visible properties here
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the object payload pointer
    emitter.instruction("test r10, r10");                                       // is the object payload null?
    emitter.instruction("jz __elephc_eval_value_object_property_len_zero");     // null object payloads have no visible properties
    abi::emit_load_symbol_to_reg(emitter, "r11", "_stdclass_class_id", 0);
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the object's runtime class id
    emitter.instruction("cmp rax, r11");                                        // check whether the object is stdClass
    emitter.instruction("jne __elephc_eval_value_object_property_len_zero");    // non-stdClass objects expose no bridge-visible properties
    emitter.instruction("mov r10, QWORD PTR [r10 + 8]");                        // load stdClass dynamic-property hash pointer
    emitter.instruction("test r10, r10");                                       // is the property hash null?
    emitter.instruction("jz __elephc_eval_value_object_property_len_zero");     // null property hashes are treated as empty objects
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the hash entry count
    emitter.instruction("ret");                                                 // return the public property count to Rust
    emitter.label("__elephc_eval_value_object_property_len_zero");
    emitter.instruction("xor eax, eax");                                        // report zero JSON-visible object properties
    emitter.instruction("ret");                                                 // return the empty property count to Rust

    label_c_global(emitter, "__elephc_eval_value_object_property_iter_key");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable property-iterator wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve slots for receiver, target position, hash pointer, and counter
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the boxed object receiver while walking properties
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested zero-based property position
    emitter.instruction("test rdi, rdi");                                       // null handles produce a null property key
    emitter.instruction("jz __elephc_eval_value_object_property_iter_key_null"); // branch to boxed null for null runtime cells
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed Mixed runtime tag
    emitter.instruction("cmp r10, 6");                                          // tag 6 = object
    emitter.instruction("jne __elephc_eval_value_object_property_iter_key_null"); // non-objects have no JSON-visible property key
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the object payload pointer
    emitter.instruction("test r10, r10");                                       // is the object payload null?
    emitter.instruction("jz __elephc_eval_value_object_property_iter_key_null"); // null object payloads produce a null key
    abi::emit_load_symbol_to_reg(emitter, "r11", "_stdclass_class_id", 0);
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the object's runtime class id
    emitter.instruction("cmp rax, r11");                                        // check whether the object is stdClass
    emitter.instruction("jne __elephc_eval_value_object_property_iter_key_null"); // non-stdClass objects have no bridge-visible key
    emitter.instruction("mov r10, QWORD PTR [r10 + 8]");                        // load stdClass dynamic-property hash pointer
    emitter.instruction("test r10, r10");                                       // is the property hash null?
    emitter.instruction("jz __elephc_eval_value_object_property_iter_key_null"); // null property hashes produce a null key
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the hash pointer for repeated iterator helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // start the insertion-order property counter at zero
    emitter.instruction("xor esi, esi");                                        // cursor 0 starts at the property hash head entry
    emitter.label("__elephc_eval_value_object_property_iter_key_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the hash pointer before advancing the iterator
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next insertion-order property key
    emitter.instruction("cmp rax, -1");                                         // did the iterator report the done sentinel?
    emitter.instruction("je __elephc_eval_value_object_property_iter_key_null"); // out-of-range positions produce a null key
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // load the current insertion-order property position
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // load the requested property position
    emitter.instruction("cmp r10, r11");                                        // is this the requested property entry?
    emitter.instruction("je __elephc_eval_value_object_property_iter_key_box"); // box the current property key when the position matches
    emitter.instruction("add r10, 1");                                          // advance the insertion-order property counter
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the updated property counter
    emitter.instruction("mov rsi, rax");                                        // use the returned cursor for the next iterator call
    emitter.instruction("jmp __elephc_eval_value_object_property_iter_key_loop"); // continue walking until the requested position is reached
    emitter.label("__elephc_eval_value_object_property_iter_key_box");
    emitter.instruction("cmp rdx, -1");                                         // integer hash keys carry key_hi = -1
    emitter.instruction("jne __elephc_eval_value_object_property_iter_key_string"); // string property keys need string-tag boxing
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer key fallback
    emitter.instruction("xor esi, esi");                                        // integer keys do not use a high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box the integer property key as Mixed
    emitter.instruction("jmp __elephc_eval_value_object_property_iter_key_done"); // return the boxed key to Rust
    emitter.label("__elephc_eval_value_object_property_iter_key_string");
    emitter.instruction("mov rsi, rdx");                                        // move the string key length into the boxing high word
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string property key
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the string property key as Mixed
    emitter.instruction("jmp __elephc_eval_value_object_property_iter_key_done"); // return the boxed key to Rust
    emitter.label("__elephc_eval_value_object_property_iter_key_null");
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null keys do not use a low payload word
    emitter.instruction("xor esi, esi");                                        // null keys do not use a high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box null for invalid property-key requests
    emitter.label("__elephc_eval_value_object_property_iter_key_done");
    emitter.instruction("add rsp, 32");                                         // release the property-iterator wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed property key to Rust

    emitter.label("__elephc_eval_key_normalize");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while classifying the eval key
    emitter.instruction("mov rbp, rsp");                                        // establish a stable key-normalizer frame
    emitter.instruction("sub rsp, 16");                                         // reserve an aligned slot for the original boxed key
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the original boxed key for fallback integer casts
    emitter.instruction("mov rax, rdi");                                        // pass the boxed key to mixed_unbox's internal input register
    emitter.instruction("call __rt_mixed_unbox");                               // expose key tag plus payload words
    emitter.instruction("cmp rax, 1");                                          // is the eval key a string?
    emitter.instruction("je __elephc_eval_key_normalize_string");               // normalize PHP string array keys through hash rules
    emitter.instruction("test rax, rax");                                       // is the eval key already an integer?
    emitter.instruction("jz __elephc_eval_key_normalize_int");                  // integer keys only need the sentinel high word
    emitter.instruction("cmp rax, 8");                                          // is the eval key null?
    emitter.instruction("je __elephc_eval_key_normalize_null");                 // PHP treats null array keys as the empty string
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the original boxed key for PHP integer coercion
    emitter.instruction("mov rax, rdi");                                        // satisfy mixed_cast_int's mixed_unbox input convention
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce non-string keys to the current integer-key fallback
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer array key
    emitter.instruction("jmp __elephc_eval_key_normalize_done");                // return the fallback integer key tuple
    emitter.label("__elephc_eval_key_normalize_string");
    emitter.instruction("mov rax, rdi");                                        // pass the string key pointer to hash normalization
    emitter.instruction("call __rt_hash_normalize_key");                        // normalize numeric strings while preserving true string keys
    emitter.instruction("jmp __elephc_eval_key_normalize_done");                // return the normalized string/int key tuple
    emitter.label("__elephc_eval_key_normalize_int");
    emitter.instruction("mov rax, rdi");                                        // publish the unboxed integer key low word
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer array key
    emitter.instruction("jmp __elephc_eval_key_normalize_done");                // finish integer key normalization
    emitter.label("__elephc_eval_key_normalize_null");
    emitter.instruction("xor eax, eax");                                        // null array keys use the empty-string pointer
    emitter.instruction("xor edx, edx");                                        // null array keys use the empty-string length
    emitter.label("__elephc_eval_key_normalize_done");
    emitter.instruction("add rsp, 16");                                         // release the key-normalizer spill slot
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return key_lo/key_hi in rax/rdx

    label_c_global(emitter, "__elephc_eval_value_is_array_like");
    emitter.instruction("test rdi, rdi");                                       // null handles cannot be indexed as arrays
    emitter.instruction("jz __elephc_eval_value_is_array_like_false");          // report false for null runtime cells
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed Mixed runtime tag
    emitter.instruction("cmp r10, 4");                                          // tag 4 = indexed array
    emitter.instruction("je __elephc_eval_value_is_array_like_true");           // indexed arrays are valid eval array-write receivers
    emitter.instruction("cmp r10, 5");                                          // tag 5 = associative array
    emitter.instruction("je __elephc_eval_value_is_array_like_true");           // associative arrays are valid eval array-write receivers
    emitter.instruction("cmp r10, 6");                                          // tag 6 = object
    emitter.instruction("je __elephc_eval_value_is_array_like_true");           // ArrayAccess-capable objects are delegated to runtime set helpers
    emitter.label("__elephc_eval_value_is_array_like_false");
    emitter.instruction("mov rax, 0");                                          // report false for scalar and null values
    emitter.instruction("ret");                                                 // return the boolean result to Rust
    emitter.label("__elephc_eval_value_is_array_like_true");
    emitter.instruction("mov rax, 1");                                          // report true for array-like values
    emitter.instruction("ret");                                                 // return the boolean result to Rust

    label_c_global(emitter, "__elephc_eval_value_is_null");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // pass the boxed Mixed argument to mixed_unbox
    emitter.instruction("call __rt_mixed_unbox");                               // unwrap nested Mixed cells to a concrete runtime tag
    emitter.instruction("cmp rax, 8");                                          // runtime tag 8 means PHP null
    emitter.instruction("sete al");                                             // set the low byte when the tag is null
    emitter.instruction("movzx eax, al");                                       // widen the C boolean result for Rust
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boolean result to Rust

    label_c_global(emitter, "__elephc_eval_value_type_tag");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // pass the boxed Mixed argument to mixed_unbox
    emitter.instruction("call __rt_mixed_unbox");                               // unwrap nested Mixed cells and return the concrete runtime tag
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the unboxed runtime tag to Rust

    label_c_global(emitter, "__elephc_eval_value_object_identity");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable object-identity wrapper frame
    emitter.instruction("mov rax, rdi");                                        // pass the boxed Mixed argument to mixed_unbox
    emitter.instruction("call __rt_mixed_unbox");                               // unwrap nested Mixed cells to tag and object payload
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means PHP object
    emitter.instruction("je __elephc_eval_value_object_identity_object_x86");   // return the payload pointer for object values
    emitter.instruction("xor eax, eax");                                        // return zero for non-object values
    emitter.instruction("jmp __elephc_eval_value_object_identity_done_x86");    // skip the object-payload result
    emitter.label("__elephc_eval_value_object_identity_object_x86");
    emitter.instruction("mov rax, rdi");                                        // return the unboxed object payload pointer
    emitter.label("__elephc_eval_value_object_identity_done_x86");
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the object identity pointer to Rust

    label_c_global(emitter, "__elephc_eval_value_cast_int");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_cast_int input
    emitter.instruction("call __rt_mixed_cast_int");                            // cast the boxed eval value to a PHP integer payload
    emitter.instruction("mov rdi, rax");                                        // move the integer cast result into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a high word
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the cast integer result for Rust
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed integer cast result to Rust

    label_c_global(emitter, "__elephc_eval_value_cast_float");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the boxed eval value to a PHP double payload
    emitter.instruction("movq rdi, xmm0");                                      // move the double cast bits into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the cast double result for Rust
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed double cast result to Rust

    label_c_global(emitter, "__elephc_eval_value_cast_string");
    emitter.instruction("push rbp");                                            // align the stack while unboxing and boxing the string result
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_unbox input
    emitter.instruction("call __rt_mixed_unbox");                               // expose the concrete payload tag and value words
    emitter.instruction("cmp rax, 0");                                          // is the eval value an integer?
    emitter.instruction("je __elephc_eval_value_cast_string_int_x86");          // integers cast through decimal formatting
    emitter.instruction("cmp rax, 1");                                          // is the eval value already a string?
    emitter.instruction("je __elephc_eval_value_cast_string_box_x86");          // strings can be boxed through the normal ownership path
    emitter.instruction("cmp rax, 2");                                          // is the eval value a double?
    emitter.instruction("je __elephc_eval_value_cast_string_float_x86");        // doubles cast through decimal formatting
    emitter.instruction("cmp rax, 3");                                          // is the eval value a boolean?
    emitter.instruction("je __elephc_eval_value_cast_string_bool_x86");         // booleans cast to \"1\" or the empty string
    emitter.label("__elephc_eval_value_cast_string_empty_x86");
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("xor edi, edi");                                        // unsupported and falsey payloads use an empty string pointer
    emitter.instruction("xor esi, esi");                                        // unsupported and falsey payloads use an empty string length
    emitter.instruction("call __rt_mixed_from_value");                          // box the empty string result for Rust
    emitter.instruction("jmp __elephc_eval_value_cast_string_done_x86");        // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_cast_string_int_x86");
    emitter.instruction("mov rax, rdi");                                        // pass the integer payload to decimal formatting
    emitter.instruction("call __rt_itoa");                                      // format the integer cast result as a string pair
    emitter.instruction("mov rdi, rax");                                        // move the formatted string pointer into mixed value_lo
    emitter.instruction("mov rsi, rdx");                                        // move the formatted string length into mixed value_hi
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the formatted integer string
    emitter.instruction("jmp __elephc_eval_value_cast_string_done_x86");        // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_cast_string_box_x86");
    emitter.instruction("mov rsi, rdx");                                        // move the existing string length into mixed value_hi
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the existing string payload once
    emitter.instruction("jmp __elephc_eval_value_cast_string_done_x86");        // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_cast_string_float_x86");
    emitter.instruction("movq xmm0, rdi");                                      // move the double payload bits into the FP argument register
    emitter.instruction("call __rt_ftoa");                                      // format the double cast result as a string pair
    emitter.instruction("mov rdi, rax");                                        // move the formatted string pointer into mixed value_lo
    emitter.instruction("mov rsi, rdx");                                        // move the formatted string length into mixed value_hi
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the formatted double string
    emitter.instruction("jmp __elephc_eval_value_cast_string_done_x86");        // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_cast_string_bool_x86");
    emitter.instruction("test rdi, rdi");                                       // false casts to the empty string
    emitter.instruction("je __elephc_eval_value_cast_string_empty_x86");        // route false to the empty string boxer
    emitter.instruction("mov rax, rdi");                                        // pass the true payload to decimal formatting
    emitter.instruction("call __rt_itoa");                                      // format true as the string \"1\"
    emitter.instruction("mov rdi, rax");                                        // move the formatted string pointer into mixed value_lo
    emitter.instruction("mov rsi, rdx");                                        // move the formatted string length into mixed value_hi
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the true string result
    emitter.label("__elephc_eval_value_cast_string_done_x86");
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed string cast result to Rust

    label_c_global(emitter, "__elephc_eval_value_cast_bool");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_cast_bool input
    emitter.instruction("call __rt_mixed_cast_bool");                           // cast the boxed eval value to PHP truthiness
    emitter.instruction("mov rdi, rax");                                        // move the boolean cast result into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // boolean payloads do not use a high word
    emitter.instruction("mov eax, 3");                                          // runtime tag 3 = boolean
    emitter.instruction("call __rt_mixed_from_value");                          // box the cast boolean result for Rust
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed boolean cast result to Rust

    label_c_global(emitter, "__elephc_eval_value_int");
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a high word
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the C integer payload in rdi and return

    label_c_global(emitter, "__elephc_eval_value_resource");
    emitter.instruction("mov eax, 9");                                          // runtime tag 9 = resource, with C id already in rdi
    emitter.instruction("xor esi, esi");                                        // resource payloads do not use a high word
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the resource payload and return to Rust

    label_c_global(emitter, "__elephc_eval_value_float");
    emitter.instruction("movq rdi, xmm0");                                      // move the C double bits into mixed value_lo
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the double payload and return to Rust

    label_c_global(emitter, "__elephc_eval_value_string");
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string, with C ptr/len already in rdi/rsi
    emitter.instruction("jmp __rt_mixed_from_value");                           // persist and box the string payload for eval

    label_c_global(emitter, "__elephc_eval_value_abs");
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into abs_mixed input
    emitter.instruction("jmp __rt_abs_mixed");                                  // compute PHP abs() for one boxed eval value

    label_c_global(emitter, "__elephc_eval_value_ceil");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the boxed eval argument to a PHP double for ceil
    emitter.bl_c("ceil");
    emitter.instruction("movq rdi, xmm0");                                      // move the ceil result bits into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the ceil result into a Mixed cell
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed ceil result to Rust

    label_c_global(emitter, "__elephc_eval_value_floor");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the boxed eval argument to a PHP double for floor
    emitter.bl_c("floor");
    emitter.instruction("movq rdi, xmm0");                                      // move the floor result bits into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the floor result into a Mixed cell
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed floor result to Rust

    label_c_global(emitter, "__elephc_eval_value_sqrt");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the boxed eval argument to a PHP double for sqrt
    emitter.bl_c("sqrt");
    emitter.instruction("movq rdi, xmm0");                                      // move the sqrt result bits into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the sqrt result into a Mixed cell
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed sqrt result to Rust

    label_c_global(emitter, "__elephc_eval_value_strrev");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_cast_string input
    emitter.instruction("call __rt_mixed_cast_string");                         // cast the boxed eval argument to a PHP string pair
    emitter.instruction("call __rt_strrev");                                    // reverse the PHP byte string into concat storage
    emitter.instruction("mov rdi, rax");                                        // move the reversed string pointer into mixed value_lo
    emitter.instruction("mov rsi, rdx");                                        // move the reversed string length into mixed value_hi
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the reversed string for Rust
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed reversed string to Rust

    label_c_global(emitter, "__elephc_eval_value_fdiv");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the right operand and left double
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the left boxed operand to a PHP numeric double
    emitter.instruction("movsd QWORD PTR [rbp - 16], xmm0");                    // save the left double across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for numeric casting
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the right boxed operand to a PHP numeric double
    emitter.instruction("movapd xmm1, xmm0");                                   // keep the right divisor in xmm1
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 16]");                    // reload the left dividend into xmm0
    emitter.instruction("divsd xmm0, xmm1");                                    // compute fdiv() with IEEE zero handling
    emitter.instruction("ucomisd xmm0, xmm0");                                  // detect NaN so PHP echo prints NAN without a sign
    emitter.instruction("jp __elephc_eval_value_fdiv_nan_x86");                 // normalize unordered fdiv results before boxing
    emitter.instruction("movq rdi, xmm0");                                      // move the fdiv result bits into mixed value_lo
    emitter.instruction("jmp __elephc_eval_value_fdiv_box_x86");                // skip the canonical NaN payload path
    emitter.label("__elephc_eval_value_fdiv_nan_x86");
    emitter.instruction("movabs rdi, 0x7ff8000000000000");                      // use a positive quiet NaN payload for PHP output
    emitter.label("__elephc_eval_value_fdiv_box_x86");
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the fdiv result into a Mixed cell
    emitter.instruction("add rsp, 32");                                         // release the fdiv wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed fdiv result to Rust

    label_c_global(emitter, "__elephc_eval_value_fmod");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the right operand and left double
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the left boxed operand to a PHP numeric double
    emitter.instruction("movsd QWORD PTR [rbp - 16], xmm0");                    // save the left double across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for numeric casting
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the right boxed operand to a PHP numeric double
    emitter.instruction("movapd xmm1, xmm0");                                   // move the right divisor into the second fmod argument
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 16]");                    // move the left dividend into the first fmod argument
    emitter.bl_c("fmod");
    emitter.instruction("ucomisd xmm0, xmm0");                                  // detect NaN so PHP echo prints NAN without a sign
    emitter.instruction("jp __elephc_eval_value_fmod_nan_x86");                 // normalize unordered fmod results before boxing
    emitter.instruction("movq rdi, xmm0");                                      // move the fmod result bits into mixed value_lo
    emitter.instruction("jmp __elephc_eval_value_fmod_box_x86");                // skip the canonical NaN payload path
    emitter.label("__elephc_eval_value_fmod_nan_x86");
    emitter.instruction("movabs rdi, 0x7ff8000000000000");                      // use a positive quiet NaN payload for PHP output
    emitter.label("__elephc_eval_value_fmod_box_x86");
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the fmod result into a Mixed cell
    emitter.instruction("add rsp, 32");                                         // release the fmod wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed fmod result to Rust

    label_c_global(emitter, "__elephc_eval_value_add");
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into the internal result register
    emitter.instruction("mov rdi, rsi");                                        // move the right boxed operand into the internal argument register
    emitter.instruction("jmp __rt_mixed_numeric_add");                          // add two boxed mixed values and return the boxed result

    label_c_global(emitter, "__elephc_eval_value_sub");
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into the internal result register
    emitter.instruction("mov rdi, rsi");                                        // move the right boxed operand into the internal argument register
    emitter.instruction("jmp __rt_mixed_numeric_sub");                          // subtract two boxed mixed values and return the boxed result

    label_c_global(emitter, "__elephc_eval_value_mul");
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into the internal result register
    emitter.instruction("mov rdi, rsi");                                        // move the right boxed operand into the internal argument register
    emitter.instruction("jmp __rt_mixed_numeric_mul");                          // multiply two boxed mixed values and return the boxed result

    label_c_global(emitter, "__elephc_eval_value_div");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the right operand and left double
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the left boxed operand to a PHP numeric double
    emitter.instruction("movsd QWORD PTR [rbp - 16], xmm0");                    // save the left double across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for numeric casting
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the right boxed operand to a PHP numeric double
    emitter.instruction("pxor xmm1, xmm1");                                     // materialize a zero double for divisor checking
    emitter.instruction("ucomisd xmm0, xmm1");                                  // detect division by zero before the hardware divide
    emitter.instruction("je __elephc_eval_value_div_null_x86");                 // return null until eval has throwable propagation
    emitter.instruction("movapd xmm1, xmm0");                                   // keep the right divisor in xmm1
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 16]");                    // reload the left dividend into xmm0
    emitter.instruction("divsd xmm0, xmm1");                                    // compute PHP division as a double result
    emitter.instruction("movq rdi, xmm0");                                      // move the double bits into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the division result into a Mixed cell
    emitter.instruction("jmp __elephc_eval_value_div_done_x86");                // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_div_null_x86");
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = null fallback for division by zero
    emitter.instruction("xor edi, edi");                                        // null has no low payload word
    emitter.instruction("xor esi, esi");                                        // null has no high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box null for unsupported division-by-zero propagation
    emitter.label("__elephc_eval_value_div_done_x86");
    emitter.instruction("add rsp, 32");                                         // release the division wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed division result to Rust

    label_c_global(emitter, "__elephc_eval_value_mod");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the right operand and left integer
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_int input
    emitter.instruction("call __rt_mixed_cast_int");                            // cast the left boxed operand to a PHP integer
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the left integer across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for integer casting
    emitter.instruction("call __rt_mixed_cast_int");                            // cast the right boxed operand to a PHP integer
    emitter.instruction("test rax, rax");                                       // detect modulo by zero before the hardware divide
    emitter.instruction("jz __elephc_eval_value_mod_null_x86");                 // return null until eval has throwable propagation
    emitter.instruction("mov rdi, rax");                                        // keep the integer divisor in rdi
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the integer dividend into rax
    emitter.instruction("cqo");                                                 // sign-extend the dividend for signed division
    emitter.instruction("idiv rdi");                                            // compute quotient in rax and remainder in rdx
    emitter.instruction("mov rdi, rdx");                                        // move the integer remainder into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a high word
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the modulo result into a Mixed cell
    emitter.instruction("jmp __elephc_eval_value_mod_done_x86");                // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_mod_null_x86");
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = null fallback for modulo by zero
    emitter.instruction("xor edi, edi");                                        // null has no low payload word
    emitter.instruction("xor esi, esi");                                        // null has no high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box null for unsupported modulo-by-zero propagation
    emitter.label("__elephc_eval_value_mod_done_x86");
    emitter.instruction("add rsp, 32");                                         // release the modulo wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed modulo result to Rust

    label_c_global(emitter, "__elephc_eval_value_pow");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the right operand and left double
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the left boxed operand to a PHP numeric double
    emitter.instruction("movsd QWORD PTR [rbp - 16], xmm0");                    // save the exponentiation base across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for numeric casting
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the right boxed operand to a PHP numeric double
    emitter.instruction("movapd xmm1, xmm0");                                   // move the exponent into libc pow's second argument
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 16]");                    // reload the base into libc pow's first argument
    emitter.bl_c("pow");
    emitter.instruction("movq rdi, xmm0");                                      // move the pow result bits into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the exponentiation result into a Mixed cell
    emitter.instruction("add rsp, 32");                                         // release the exponentiation wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed exponentiation result to Rust

    label_c_global(emitter, "__elephc_eval_value_round");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for precision state and saved doubles
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the optional precision cell while casting the value
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save whether the caller supplied a precision argument
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the boxed eval value to a PHP numeric double
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // check whether a precision argument was supplied
    emitter.instruction("jne __elephc_eval_value_round_precision_x86");         // use the precision path when a second argument is present
    emitter.bl_c("round");
    emitter.instruction("jmp __elephc_eval_value_round_box_x86");               // box the default-precision round result
    emitter.label("__elephc_eval_value_round_precision_x86");
    emitter.instruction("movsd QWORD PTR [rbp - 24], xmm0");                    // save the original value while casting the precision
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the precision cell for integer casting
    emitter.instruction("call __rt_mixed_cast_int");                            // cast the optional precision to a PHP integer
    emitter.instruction("cvtsi2sd xmm1, rax");                                  // convert the precision to a floating exponent for pow
    emitter.instruction("mov rax, 0x4024000000000000");                         // materialize the IEEE-754 payload for 10.0
    emitter.instruction("movq xmm0, rax");                                      // move 10.0 into the pow base argument
    emitter.bl_c("pow");
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 24]");                    // reload the original value after pow returns the multiplier
    emitter.instruction("mulsd xmm1, xmm0");                                    // scale the value by the precision multiplier
    emitter.instruction("movsd QWORD PTR [rbp - 32], xmm0");                    // save the multiplier for rescaling after round
    emitter.instruction("movsd xmm0, xmm1");                                    // move the scaled value into the round argument
    emitter.bl_c("round");
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 32]");                    // reload the precision multiplier for rescaling
    emitter.instruction("divsd xmm0, xmm1");                                    // scale the rounded value back to requested precision
    emitter.label("__elephc_eval_value_round_box_x86");
    emitter.instruction("movq rdi, xmm0");                                      // move the round result bits into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("call __rt_mixed_from_value");                          // box the round result into a Mixed cell
    emitter.instruction("add rsp, 48");                                         // release the round wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed round result to Rust

    label_c_global(emitter, "__elephc_eval_value_bit_not");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // keep stack alignment for the cast and boxing calls
    emitter.instruction("mov rax, rdi");                                        // move the boxed operand into mixed_cast_int input
    emitter.instruction("call __rt_mixed_cast_int");                            // cast the boxed operand to a PHP integer
    emitter.instruction("not rax");                                             // compute bitwise complement of the integer payload
    emitter.instruction("mov rdi, rax");                                        // move the complement into mixed value_lo
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a high word
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the bitwise NOT result into a Mixed cell
    emitter.instruction("add rsp, 16");                                         // release the bitwise NOT wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed bitwise NOT result to Rust

    label_c_global(emitter, "__elephc_eval_value_bitwise");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve slots for right operand, opcode, and left integer
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the eval bitwise opcode across helper calls
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_int input
    emitter.instruction("call __rt_mixed_cast_int");                            // cast the left boxed operand to a PHP integer
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the left integer across the right cast
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for integer casting
    emitter.instruction("call __rt_mixed_cast_int");                            // cast the right boxed operand to a PHP integer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the left integer into the payload register
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the eval bitwise opcode for dispatch
    emitter.instruction("cmp r10, 0");                                          // is this integer bitwise AND?
    emitter.instruction("je __elephc_eval_value_bitwise_and_x86");              // route opcode 0 to integer AND
    emitter.instruction("cmp r10, 1");                                          // is this integer bitwise OR?
    emitter.instruction("je __elephc_eval_value_bitwise_or_x86");               // route opcode 1 to integer OR
    emitter.instruction("cmp r10, 2");                                          // is this integer bitwise XOR?
    emitter.instruction("je __elephc_eval_value_bitwise_xor_x86");              // route opcode 2 to integer XOR
    emitter.instruction("cmp r10, 3");                                          // is this integer left shift?
    emitter.instruction("je __elephc_eval_value_bitwise_shl_x86");              // route opcode 3 to integer left shift
    emitter.instruction("cmp r10, 4");                                          // is this integer right shift?
    emitter.instruction("je __elephc_eval_value_bitwise_shr_x86");              // route opcode 4 to integer right shift
    emitter.instruction("jmp __elephc_eval_value_bitwise_null_x86");            // fail closed for unknown bitwise opcodes
    emitter.label("__elephc_eval_value_bitwise_and_x86");
    emitter.instruction("and rdi, rax");                                        // compute integer bitwise AND
    emitter.instruction("jmp __elephc_eval_value_bitwise_box_x86");             // box the integer bitwise result
    emitter.label("__elephc_eval_value_bitwise_or_x86");
    emitter.instruction("or rdi, rax");                                         // compute integer bitwise OR
    emitter.instruction("jmp __elephc_eval_value_bitwise_box_x86");             // box the integer bitwise result
    emitter.label("__elephc_eval_value_bitwise_xor_x86");
    emitter.instruction("xor rdi, rax");                                        // compute integer bitwise XOR
    emitter.instruction("jmp __elephc_eval_value_bitwise_box_x86");             // box the integer bitwise result
    emitter.label("__elephc_eval_value_bitwise_shl_x86");
    emitter.instruction("test rax, rax");                                       // negative shift counts are runtime errors in PHP
    emitter.instruction("js __elephc_eval_value_bitwise_null_x86");             // return null until eval has throwable propagation
    emitter.instruction("mov rcx, rax");                                        // move the shift count into the x86 shift-count register
    emitter.instruction("shl rdi, cl");                                         // shift the integer payload left
    emitter.instruction("jmp __elephc_eval_value_bitwise_box_x86");             // box the integer shift result
    emitter.label("__elephc_eval_value_bitwise_shr_x86");
    emitter.instruction("test rax, rax");                                       // negative shift counts are runtime errors in PHP
    emitter.instruction("js __elephc_eval_value_bitwise_null_x86");             // return null until eval has throwable propagation
    emitter.instruction("mov rcx, rax");                                        // move the shift count into the x86 shift-count register
    emitter.instruction("sar rdi, cl");                                         // shift the integer payload right arithmetically
    emitter.instruction("jmp __elephc_eval_value_bitwise_box_x86");             // box the integer shift result
    emitter.label("__elephc_eval_value_bitwise_box_x86");
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a high word
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the bitwise result into a Mixed cell
    emitter.instruction("jmp __elephc_eval_value_bitwise_done_x86");            // restore the wrapper frame and return
    emitter.label("__elephc_eval_value_bitwise_null_x86");
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = null fallback for unsupported bitwise errors
    emitter.instruction("xor edi, edi");                                        // null has no low payload word
    emitter.instruction("xor esi, esi");                                        // null has no high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box null for unsupported bitwise error propagation
    emitter.label("__elephc_eval_value_bitwise_done_x86");
    emitter.instruction("add rsp, 32");                                         // release the bitwise wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed bitwise result to Rust

    label_c_global(emitter, "__elephc_eval_value_concat");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for right operand and left string pair
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_string input
    emitter.instruction("call __rt_mixed_cast_string");                         // cast the left boxed operand to a PHP string pair
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the left string pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the left string length
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for string casting
    emitter.instruction("call __rt_mixed_cast_string");                         // cast the right boxed operand to a PHP string pair
    emitter.instruction("mov rdi, rax");                                        // move the right string pointer into concat's right pointer register
    emitter.instruction("mov rsi, rdx");                                        // move the right string length into concat's right length register
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the left string pointer for concat
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the left string length for concat
    emitter.instruction("call __rt_concat");                                    // concatenate the two PHP string pairs
    emitter.instruction("mov rdi, rax");                                        // move the concat string pointer into mixed value_lo
    emitter.instruction("mov rsi, rdx");                                        // move the concat string length into mixed value_hi
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string for boxing the concat result
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the concatenated string
    emitter.instruction("add rsp, 32");                                         // release the concat wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed concat result to Rust

    label_c_global(emitter, "__elephc_eval_value_compare");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across comparison helpers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable comparison wrapper frame
    emitter.instruction("sub rsp, 32");                                         // reserve slots for operands, opcode, and numeric casts
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left boxed operand
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the right boxed operand
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the eval comparison opcode
    emitter.instruction("cmp rdx, 0");                                          // is this loose equality?
    emitter.instruction("je __elephc_eval_value_compare_eq");                   // route == through the mixed loose-equality helper
    emitter.instruction("cmp rdx, 1");                                          // is this loose inequality?
    emitter.instruction("je __elephc_eval_value_compare_ne");                   // route != through the mixed loose-equality helper
    emitter.instruction("cmp rdx, 6");                                          // is this strict equality?
    emitter.instruction("je __elephc_eval_value_compare_strict_eq");            // route === through the mixed strict-equality helper
    emitter.instruction("cmp rdx, 7");                                          // is this strict inequality?
    emitter.instruction("je __elephc_eval_value_compare_strict_ne");            // route !== through the mixed strict-equality helper
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the left boxed operand to a numeric comparison double
    emitter.instruction("movsd QWORD PTR [rbp - 32], xmm0");                    // save the normalized left numeric operand
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the right boxed operand for numeric casting
    emitter.instruction("mov rax, rdi");                                        // move the right boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the right boxed operand to a numeric comparison double
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 32]");                    // reload the left numeric operand for the float comparison
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the eval comparison opcode for dispatch
    emitter.instruction("cmp r10, 2");                                          // is this a less-than comparison?
    emitter.instruction("je __elephc_eval_value_compare_lt");                   // materialize left < right from float comparison flags
    emitter.instruction("cmp r10, 3");                                          // is this a less-than-or-equal comparison?
    emitter.instruction("je __elephc_eval_value_compare_lte");                  // materialize left <= right from float comparison flags
    emitter.instruction("cmp r10, 4");                                          // is this a greater-than comparison?
    emitter.instruction("je __elephc_eval_value_compare_gt");                   // materialize left > right from float comparison flags
    emitter.instruction("cmp r10, 5");                                          // is this a greater-than-or-equal comparison?
    emitter.instruction("je __elephc_eval_value_compare_gte");                  // materialize left >= right from float comparison flags
    emitter.instruction("xor eax, eax");                                        // unknown comparison opcodes fail closed as false
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the fallback false result
    emitter.label("__elephc_eval_value_compare_eq");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left operand for loose equality
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the right operand for loose equality
    emitter.instruction("call __elephc_eval_mixed_loose_eq");                   // compute scalar PHP loose equality
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the equality result
    emitter.label("__elephc_eval_value_compare_ne");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left operand for loose inequality
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the right operand for loose inequality
    emitter.instruction("call __elephc_eval_mixed_loose_eq");                   // compute scalar PHP loose equality before inversion
    emitter.instruction("xor rax, 1");                                          // invert equality for the != operator
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the inequality result
    emitter.label("__elephc_eval_value_compare_strict_eq");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left operand for strict equality
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the right operand for strict equality
    emitter.instruction("call __rt_mixed_strict_eq");                           // compute PHP strict equality
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the strict-equality result
    emitter.label("__elephc_eval_value_compare_strict_ne");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left operand for strict inequality
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the right operand for strict inequality
    emitter.instruction("call __rt_mixed_strict_eq");                           // compute PHP strict equality before inversion
    emitter.instruction("xor rax, 1");                                          // invert equality for the !== operator
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the strict-inequality result
    emitter.label("__elephc_eval_value_compare_lt");
    emitter.instruction("ucomisd xmm1, xmm0");                                  // compare numeric eval operands for <
    emitter.instruction("setb al");                                             // set true when left is below right
    emitter.instruction("setnp r10b");                                          // require an ordered comparison
    emitter.instruction("and al, r10b");                                        // clear unordered NaN less-than results
    emitter.instruction("movzx rax, al");                                       // widen the less-than boolean result
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the less-than result
    emitter.label("__elephc_eval_value_compare_lte");
    emitter.instruction("ucomisd xmm1, xmm0");                                  // compare numeric eval operands for <=
    emitter.instruction("setbe al");                                            // set true when left is below or equal to right
    emitter.instruction("setnp r10b");                                          // require an ordered comparison
    emitter.instruction("and al, r10b");                                        // clear unordered NaN less-than-or-equal results
    emitter.instruction("movzx rax, al");                                       // widen the less-than-or-equal boolean result
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the less-than-or-equal result
    emitter.label("__elephc_eval_value_compare_gt");
    emitter.instruction("ucomisd xmm1, xmm0");                                  // compare numeric eval operands for >
    emitter.instruction("seta al");                                             // set true when left is above right
    emitter.instruction("setnp r10b");                                          // require an ordered comparison
    emitter.instruction("and al, r10b");                                        // clear unordered NaN greater-than results
    emitter.instruction("movzx rax, al");                                       // widen the greater-than boolean result
    emitter.instruction("jmp __elephc_eval_value_compare_box");                 // box the greater-than result
    emitter.label("__elephc_eval_value_compare_gte");
    emitter.instruction("ucomisd xmm1, xmm0");                                  // compare numeric eval operands for >=
    emitter.instruction("setae al");                                            // set true when left is above or equal to right
    emitter.instruction("setnp r10b");                                          // require an ordered comparison
    emitter.instruction("and al, r10b");                                        // clear unordered NaN greater-than-or-equal results
    emitter.instruction("movzx rax, al");                                       // widen the greater-than-or-equal boolean result
    emitter.label("__elephc_eval_value_compare_box");
    emitter.instruction("mov rdi, rax");                                        // move the comparison boolean into the Mixed payload register
    emitter.instruction("mov eax, 3");                                          // runtime tag 3 = bool
    emitter.instruction("xor esi, esi");                                        // bool payloads do not use a high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the comparison result as a Mixed bool
    emitter.instruction("add rsp, 32");                                         // release the comparison wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed comparison result to Rust

    emitter.label("__elephc_eval_mixed_loose_eq");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across mixed helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable loose-equality helper frame
    emitter.instruction("sub rsp, 96");                                         // allocate helper slots for unboxed tags, payloads, and casts
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the left boxed operand for later casts
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the right boxed operand for later casts
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_unbox input
    emitter.instruction("call __rt_mixed_unbox");                               // unbox the left eval operand into tag and payload words
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the left runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the left low payload word
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the left high payload word
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the right boxed operand for unboxing
    emitter.instruction("call __rt_mixed_unbox");                               // unbox the right eval operand into tag and payload words
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the right runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // save the right low payload word
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save the right high payload word
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the left runtime tag for equality dispatch
    emitter.instruction("cmp r10, 3");                                          // does the left operand have PHP bool semantics?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_bool");                // bool comparisons use truthiness on both operands
    emitter.instruction("cmp rax, 3");                                          // does the right operand have PHP bool semantics?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_bool");                // bool comparisons use truthiness on both operands
    emitter.instruction("cmp r10, rax");                                        // do the operands have the same runtime tag?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_same_tag");            // same-tag scalars use focused payload comparisons
    emitter.instruction("cmp r10, 8");                                          // is the left operand null?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_left_null");           // null compares equal only to empty strings before numeric fallback
    emitter.instruction("cmp rax, 8");                                          // is the right operand null?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_right_null");          // null compares equal only to empty strings before numeric fallback
    emitter.instruction("cmp r10, 1");                                          // is a non-matching left operand a string?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_left_string");         // compare numeric strings against numeric scalars
    emitter.instruction("cmp rax, 1");                                          // is a non-matching right operand a string?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_right_string");        // compare numeric strings against numeric scalars
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_numeric");            // remaining scalar mismatches compare numerically
    emitter.label("__elephc_eval_mixed_loose_eq_same_tag");
    emitter.instruction("cmp r10, 8");                                          // are both operands null?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_true");                // null loosely equals null
    emitter.instruction("cmp r10, 1");                                          // are both operands strings?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_strings");             // strings use PHP loose string equality
    emitter.instruction("cmp r10, 2");                                          // are both operands floats?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_floats");              // floats compare with native floating equality
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the left low payload word
    emitter.instruction("cmp r11, QWORD PTR [rbp - 56]");                       // compare low payload words for int and pointer-like scalars
    emitter.instruction("jne __elephc_eval_mixed_loose_eq_false");              // mismatched low payloads are not equal
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the left high payload word
    emitter.instruction("cmp r11, QWORD PTR [rbp - 64]");                       // compare high payload words for pointer-like scalars
    emitter.instruction("sete al");                                             // materialize same-tag payload equality
    emitter.instruction("movzx rax, al");                                       // widen the payload equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the payload equality result
    emitter.label("__elephc_eval_mixed_loose_eq_strings");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the left string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload the left string length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // reload the right string pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // reload the right string length
    emitter.instruction("call __rt_str_loose_eq");                              // compare strings with PHP loose numeric-string rules
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the string loose-equality result
    emitter.label("__elephc_eval_mixed_loose_eq_floats");
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 32]");                    // reload the left float payload
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 56]");                    // reload the right float payload
    emitter.instruction("ucomisd xmm1, xmm0");                                  // compare same-tag float payloads
    emitter.instruction("sete al");                                             // set true for ordered float equality
    emitter.instruction("setnp r10b");                                          // require an ordered comparison
    emitter.instruction("and al, r10b");                                        // clear unordered NaN equality
    emitter.instruction("movzx rax, al");                                       // widen the float equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the float equality result
    emitter.label("__elephc_eval_mixed_loose_eq_left_null");
    emitter.instruction("cmp rax, 1");                                          // is null being compared with a string?
    emitter.instruction("jne __elephc_eval_mixed_loose_eq_numeric");            // non-string null comparisons fall back to numeric zero
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // null loosely equals only the empty string
    emitter.instruction("sete al");                                             // materialize the null/string equality result
    emitter.instruction("movzx rax, al");                                       // widen the null/string equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the null/string equality result
    emitter.label("__elephc_eval_mixed_loose_eq_right_null");
    emitter.instruction("cmp r10, 1");                                          // is null being compared with a string?
    emitter.instruction("jne __elephc_eval_mixed_loose_eq_numeric");            // non-string null comparisons fall back to numeric zero
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // null loosely equals only the empty string
    emitter.instruction("sete al");                                             // materialize the string/null equality result
    emitter.instruction("movzx rax, al");                                       // widen the string/null equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the string/null equality result
    emitter.label("__elephc_eval_mixed_loose_eq_left_string");
    emitter.instruction("cmp rax, 0");                                          // can the right operand be compared numerically as an int?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_left_string_numeric"); // parse the left string for numeric equality
    emitter.instruction("cmp rax, 2");                                          // can the right operand be compared numerically as a float?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_left_string_numeric"); // parse the left string for numeric equality
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_false");              // non-numeric string mismatches are not loosely equal here
    emitter.label("__elephc_eval_mixed_loose_eq_left_string_numeric");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the left string pointer for numeric parsing
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the left string length for numeric parsing
    emitter.instruction("call __rt_str_to_number");                             // parse the left string under PHP numeric-string rules
    emitter.instruction("test rax, rax");                                       // did the left string parse as numeric?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_false");               // non-numeric strings do not equal numeric scalars
    emitter.instruction("movsd QWORD PTR [rbp - 72], xmm0");                    // save the parsed left numeric-string value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the right boxed operand for numeric casting
    emitter.instruction("mov rax, rdi");                                        // move the right boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the right operand to a comparison double
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 72]");                    // reload the parsed left numeric-string value
    emitter.instruction("ucomisd xmm1, xmm0");                                  // compare parsed string and numeric scalar values
    emitter.instruction("sete al");                                             // set true for ordered string/numeric equality
    emitter.instruction("setnp r10b");                                          // require an ordered comparison
    emitter.instruction("and al, r10b");                                        // clear unordered NaN equality
    emitter.instruction("movzx rax, al");                                       // widen the string/numeric equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the string/numeric equality result
    emitter.label("__elephc_eval_mixed_loose_eq_right_string");
    emitter.instruction("cmp r10, 0");                                          // can the left operand be compared numerically as an int?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_right_string_numeric"); // parse the right string for numeric equality
    emitter.instruction("cmp r10, 2");                                          // can the left operand be compared numerically as a float?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_right_string_numeric"); // parse the right string for numeric equality
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_false");              // non-numeric string mismatches are not loosely equal here
    emitter.label("__elephc_eval_mixed_loose_eq_right_string_numeric");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the right string pointer for numeric parsing
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // reload the right string length for numeric parsing
    emitter.instruction("call __rt_str_to_number");                             // parse the right string under PHP numeric-string rules
    emitter.instruction("test rax, rax");                                       // did the right string parse as numeric?
    emitter.instruction("je __elephc_eval_mixed_loose_eq_false");               // non-numeric strings do not equal numeric scalars
    emitter.instruction("movsd QWORD PTR [rbp - 72], xmm0");                    // save the parsed right numeric-string value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left boxed operand for numeric casting
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the left operand to a comparison double
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 72]");                    // reload the parsed right numeric-string value
    emitter.instruction("ucomisd xmm0, xmm1");                                  // compare numeric scalar and parsed string values
    emitter.instruction("sete al");                                             // set true for ordered numeric/string equality
    emitter.instruction("setnp r10b");                                          // require an ordered comparison
    emitter.instruction("and al, r10b");                                        // clear unordered NaN equality
    emitter.instruction("movzx rax, al");                                       // widen the numeric/string equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the numeric/string equality result
    emitter.label("__elephc_eval_mixed_loose_eq_bool");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left boxed operand for truthiness
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_bool input
    emitter.instruction("call __rt_mixed_cast_bool");                           // cast the left operand to PHP truthiness
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the left truthiness result
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the right boxed operand for truthiness
    emitter.instruction("mov rax, rdi");                                        // move the right boxed operand into mixed_cast_bool input
    emitter.instruction("call __rt_mixed_cast_bool");                           // cast the right operand to PHP truthiness
    emitter.instruction("cmp QWORD PTR [rbp - 72], rax");                       // compare boolean truthiness for loose equality
    emitter.instruction("sete al");                                             // materialize bool loose equality
    emitter.instruction("movzx rax, al");                                       // widen the bool equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the bool loose-equality result
    emitter.label("__elephc_eval_mixed_loose_eq_numeric");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the left boxed operand for numeric equality
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the left operand to a comparison double
    emitter.instruction("movsd QWORD PTR [rbp - 72], xmm0");                    // save the left numeric equality operand
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the right boxed operand for numeric equality
    emitter.instruction("mov rax, rdi");                                        // move the right boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the right operand to a comparison double
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 72]");                    // reload the left numeric equality operand
    emitter.instruction("ucomisd xmm1, xmm0");                                  // compare numeric operands for loose equality
    emitter.instruction("sete al");                                             // set true for ordered numeric equality
    emitter.instruction("setnp r10b");                                          // require an ordered comparison
    emitter.instruction("and al, r10b");                                        // clear unordered NaN equality
    emitter.instruction("movzx rax, al");                                       // widen the numeric equality result
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the numeric loose-equality result
    emitter.label("__elephc_eval_mixed_loose_eq_true");
    emitter.instruction("mov rax, 1");                                          // materialize true for loose equality
    emitter.instruction("jmp __elephc_eval_mixed_loose_eq_done");               // return the true result
    emitter.label("__elephc_eval_mixed_loose_eq_false");
    emitter.instruction("xor eax, eax");                                        // materialize false for loose equality
    emitter.label("__elephc_eval_mixed_loose_eq_done");
    emitter.instruction("add rsp, 96");                                         // release the loose-equality helper slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the loose-equality boolean in rax

    label_c_global(emitter, "__elephc_eval_value_spaceship");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the right operand and left double
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the right boxed operand while casting the left operand
    emitter.instruction("mov rax, rdi");                                        // move the left boxed operand into mixed_cast_float input
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the left boxed operand to a PHP numeric double
    emitter.instruction("movsd QWORD PTR [rbp - 16], xmm0");                    // save the left numeric spaceship operand
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the right boxed operand for numeric casting
    emitter.instruction("call __rt_mixed_cast_float");                          // cast the right boxed operand to a PHP numeric double
    emitter.instruction("movsd xmm1, QWORD PTR [rbp - 16]");                    // reload the left numeric spaceship operand
    emitter.instruction("ucomisd xmm1, xmm0");                                  // compare left and right numeric operands for spaceship
    emitter.instruction("jp __elephc_eval_value_spaceship_gt_x86");             // PHP treats unordered NaN spaceship comparisons as greater
    emitter.instruction("ja __elephc_eval_value_spaceship_gt_x86");             // route left > right to result 1
    emitter.instruction("jb __elephc_eval_value_spaceship_lt_x86");             // route left < right to result -1
    emitter.instruction("xor edi, edi");                                        // equal operands produce spaceship result 0
    emitter.instruction("jmp __elephc_eval_value_spaceship_box_x86");           // box the equal spaceship result
    emitter.label("__elephc_eval_value_spaceship_gt_x86");
    emitter.instruction("mov rdi, 1");                                          // greater or unordered comparisons produce result 1
    emitter.instruction("jmp __elephc_eval_value_spaceship_box_x86");           // box the greater spaceship result
    emitter.label("__elephc_eval_value_spaceship_lt_x86");
    emitter.instruction("mov rdi, -1");                                         // lesser comparisons produce result -1
    emitter.label("__elephc_eval_value_spaceship_box_x86");
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a high word
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("call __rt_mixed_from_value");                          // box the spaceship result into a Mixed cell
    emitter.instruction("add rsp, 32");                                         // release the spaceship wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed spaceship result to Rust

    label_c_global(emitter, "__elephc_eval_value_echo");
    emitter.instruction("mov rax, rdi");                                        // move the C boxed value argument into mixed echo input
    emitter.instruction("jmp __rt_mixed_write_stdout");                         // echo one boxed mixed value and return to Rust

    label_c_global(emitter, "__elephc_eval_value_string_bytes");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across string casting
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve slots for the caller's output pointers
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the caller's out_ptr storage address
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the caller's out_len storage address
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_cast_string input
    emitter.instruction("call __rt_mixed_cast_string");                         // cast the boxed eval value to a PHP string pair
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the optional out_ptr storage address
    emitter.instruction("test r10, r10");                                       // did the caller request the string pointer?
    emitter.instruction("jz __elephc_eval_value_string_bytes_len");             // skip pointer storage when the caller passed null
    emitter.instruction("mov QWORD PTR [r10], rax");                            // store the string pointer for Rust to copy immediately
    emitter.label("__elephc_eval_value_string_bytes_len");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the optional out_len storage address
    emitter.instruction("test r10, r10");                                       // did the caller request the string length?
    emitter.instruction("jz __elephc_eval_value_string_bytes_done");            // skip length storage when the caller passed null
    emitter.instruction("mov QWORD PTR [r10], rdx");                            // store the string byte length for Rust
    emitter.label("__elephc_eval_value_string_bytes_done");
    emitter.instruction("mov rax, 1");                                          // report successful string conversion to Rust
    emitter.instruction("add rsp, 16");                                         // release the string-bytes wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the success flag to Rust

    label_c_global(emitter, "__elephc_eval_value_truthy");
    emitter.instruction("mov rax, rdi");                                        // move the C boxed value argument into mixed truthiness input
    emitter.instruction("jmp __rt_mixed_cast_bool");                            // cast one boxed mixed value to PHP truthiness for eval

    label_c_global(emitter, "__elephc_eval_value_retain");
    emitter.instruction("mov rax, rdi");                                        // move the C boxed Mixed argument into the internal retain register
    emitter.instruction("jmp __rt_incref");                                     // retain one eval-owned boxed Mixed cell

    label_c_global(emitter, "__elephc_eval_warning");
    emitter.instruction("jmp __rt_diag_warning");                               // emit or suppress one eval runtime warning

    label_c_global(emitter, "__elephc_eval_value_release");
    emitter.instruction("mov rax, rdi");                                        // move the C boxed Mixed argument into the internal release register
    emitter.instruction("jmp __rt_decref_mixed");                               // release one eval-owned boxed Mixed cell
}

/// Emits the ARM64 eval hook that returns AOT ReflectionMethod predicate flags.
fn emit_aarch64_eval_reflection_method_flags(emitter: &mut Emitter) {
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
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the stored class-name length
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested class-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested class-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_method_flags_skip");     // length mismatch means the class cannot match
    emitter.instruction("str x11, [sp, #48]");                                  // save the row index across the class-name compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested class-name length
    emitter.instruction("ldr x3, [x10]");                                       // pass the stored class-name pointer
    emitter.instruction("mov x4, x12");                                         // pass the stored class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #48]");                                  // restore the row index after the class-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested class name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_method_flags_skip");     // class mismatch means the row cannot match
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current row for the method-name compare
    emitter.instruction("ldr x12, [x10, #24]");                                 // load the stored method-name length
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload the requested method-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested method-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_method_flags_skip");     // length mismatch means the method cannot match
    emitter.instruction("str x11, [sp, #48]");                                  // save the row index across the method-name compare
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the requested method-name pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass the requested method-name length
    emitter.instruction("ldr x3, [x10, #16]");                                  // pass the stored method-name pointer
    emitter.instruction("mov x4, x12");                                         // pass the stored method-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare method names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #48]");                                  // restore the row index after the method-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested method name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_method_flags_skip");     // method mismatch means scanning must continue
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the matched method metadata row
    emitter.instruction("ldr x0, [x10, #32]");                                  // return the row's ReflectionMethod predicate flags
    emitter.instruction("b __elephc_eval_reflection_method_flags_done");        // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_method_flags_skip");
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current method metadata row
    emitter.instruction("add x10, x10, #40");                                   // advance to the next 40-byte metadata row
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

/// Emits the ARM64 eval hook that returns AOT ReflectionProperty predicate flags.
fn emit_aarch64_eval_reflection_property_flags(emitter: &mut Emitter) {
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
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the stored class-name length
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested class-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested class-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_property_flags_skip");   // length mismatch means the class cannot match
    emitter.instruction("str x11, [sp, #48]");                                  // save the row index across the class-name compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested class-name length
    emitter.instruction("ldr x3, [x10]");                                       // pass the stored class-name pointer
    emitter.instruction("mov x4, x12");                                         // pass the stored class-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #48]");                                  // restore the row index after the class-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested class name match this row?
    emitter.instruction("b.ne __elephc_eval_reflection_property_flags_skip");   // class mismatch means the row cannot match
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current row for the property-name compare
    emitter.instruction("ldr x12, [x10, #24]");                                 // load the stored property-name length
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload the requested property-name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested property-name lengths
    emitter.instruction("b.ne __elephc_eval_reflection_property_flags_skip");   // length mismatch means the property cannot match
    emitter.instruction("str x11, [sp, #48]");                                  // save the row index across the property-name compare
    emitter.instruction("ldr x1, [sp, #16]");                                   // pass the requested property-name pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // pass the requested property-name length
    emitter.instruction("ldr x3, [x10, #16]");                                  // pass the stored property-name pointer
    emitter.instruction("mov x4, x12");                                         // pass the stored property-name length
    emitter.instruction("bl __rt_str_eq");                                      // compare property names with PHP case-sensitive rules
    emitter.instruction("ldr x11, [sp, #48]");                                  // restore the row index after the property-name compare
    emitter.instruction("cmp x0, #0");                                          // did the requested property name match this row?
    emitter.instruction("b.eq __elephc_eval_reflection_property_flags_skip");   // property mismatch means scanning must continue
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the matched property metadata row
    emitter.instruction("ldr x0, [x10, #32]");                                  // return the row's ReflectionProperty predicate flags
    emitter.instruction("b __elephc_eval_reflection_property_flags_done");      // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_property_flags_skip");
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the current property metadata row
    emitter.instruction("add x10, x10, #40");                                   // advance to the next 40-byte metadata row
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

/// Emits the x86_64 eval hook that returns AOT ReflectionMethod predicate flags.
fn emit_x86_64_eval_reflection_method_flags(emitter: &mut Emitter) {
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
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load the stored class-name length
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare stored and requested class-name lengths
    emitter.instruction("jne __elephc_eval_reflection_method_flags_skip_x86");  // length mismatch means the class cannot match
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save the row index across the class-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested class-name length
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // pass the stored class-name pointer
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // restore the row index after the class-name compare
    emitter.instruction("test rax, rax");                                       // did the requested class name match this row?
    emitter.instruction("jne __elephc_eval_reflection_method_flags_skip_x86");  // class mismatch means the row cannot match
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current row for the method-name compare
    emitter.instruction("mov rcx, QWORD PTR [r10 + 24]");                       // load the stored method-name length
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // compare stored and requested method-name lengths
    emitter.instruction("jne __elephc_eval_reflection_method_flags_skip_x86");  // length mismatch means the method cannot match
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save the row index across the method-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the requested method-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the requested method-name length
    emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                       // pass the stored method-name pointer
    emitter.instruction("call __rt_strcasecmp");                                // compare method names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // restore the row index after the method-name compare
    emitter.instruction("test rax, rax");                                       // did the requested method name match this row?
    emitter.instruction("jne __elephc_eval_reflection_method_flags_skip_x86");  // method mismatch means scanning must continue
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the matched method metadata row
    emitter.instruction("mov rax, QWORD PTR [r10 + 32]");                       // return the row's ReflectionMethod predicate flags
    emitter.instruction("jmp __elephc_eval_reflection_method_flags_done_x86");  // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_method_flags_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current method metadata row
    emitter.instruction("add r10, 40");                                         // advance to the next 40-byte metadata row
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

/// Emits the x86_64 eval hook that returns AOT ReflectionProperty predicate flags.
fn emit_x86_64_eval_reflection_property_flags(emitter: &mut Emitter) {
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
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load the stored class-name length
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare stored and requested class-name lengths
    emitter.instruction("jne __elephc_eval_reflection_property_flags_skip_x86"); // length mismatch means the class cannot match
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save the row index across the class-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested class-name length
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // pass the stored class-name pointer
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // restore the row index after the class-name compare
    emitter.instruction("test rax, rax");                                       // did the requested class name match this row?
    emitter.instruction("jne __elephc_eval_reflection_property_flags_skip_x86"); // class mismatch means the row cannot match
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current row for the property-name compare
    emitter.instruction("mov rcx, QWORD PTR [r10 + 24]");                       // load the stored property-name length
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // compare stored and requested property-name lengths
    emitter.instruction("jne __elephc_eval_reflection_property_flags_skip_x86"); // length mismatch means the property cannot match
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // save the row index across the property-name compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the requested property-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass the requested property-name length
    emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                       // pass the stored property-name pointer
    emitter.instruction("call __rt_str_eq");                                    // compare property names with PHP case-sensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // restore the row index after the property-name compare
    emitter.instruction("test rax, rax");                                       // did the requested property name match this row?
    emitter.instruction("jz __elephc_eval_reflection_property_flags_skip_x86"); // property mismatch means scanning must continue
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the matched property metadata row
    emitter.instruction("mov rax, QWORD PTR [r10 + 32]");                       // return the row's ReflectionProperty predicate flags
    emitter.instruction("jmp __elephc_eval_reflection_property_flags_done_x86"); // restore the wrapper frame after a match
    emitter.label("__elephc_eval_reflection_property_flags_skip_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current property metadata row
    emitter.instruction("add r10, 40");                                         // advance to the next 40-byte metadata row
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

/// Emits a class-like name membership scanner for ARM64 eval bridge hooks.
fn emit_aarch64_eval_name_table_exists(
    emitter: &mut Emitter,
    exported_label: &str,
    count_symbol: &str,
    table_symbol: &str,
    local_stem: &str,
) {
    label_c_global(emitter, exported_label);
    emitter.instruction("sub sp, sp, #64");                                     // reserve lookup state while comparing metadata names
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address across string compares
    emitter.instruction("add x29, sp, #48");                                    // establish a stable name-table lookup frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested name length
    abi::emit_symbol_address(emitter, "x9", count_symbol);
    emitter.instruction("ldr x9, [x9]");                                        // load the metadata-name table count
    emitter.instruction(&format!("cbz x9, {local_stem}_miss"));                 // an empty table cannot contain the requested name
    emitter.instruction("str x9, [sp, #16]");                                   // save the table count across string compares
    abi::emit_symbol_address(emitter, "x10", table_symbol);
    emitter.instruction("str x10, [sp, #24]");                                  // save the current metadata-name table cursor
    emitter.instruction("mov x11, #0");                                         // start scanning at table index zero
    emitter.label(&format!("{local_stem}_loop"));
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the metadata-name table count
    emitter.instruction("cmp x11, x9");                                         // have all metadata-name entries been scanned?
    emitter.instruction(&format!("b.ge {local_stem}_miss"));                    // no metadata name matched before the end
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current metadata-name table entry
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the stored metadata-name length
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the requested name length
    emitter.instruction("cmp x12, x2");                                         // compare stored and requested name lengths
    emitter.instruction(&format!("b.ne {local_stem}_skip"));                    // length mismatch means this entry cannot match
    emitter.instruction("str x11, [sp, #32]");                                  // save the table index across the string compare
    emitter.instruction("ldr x1, [sp, #0]");                                    // pass the requested name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the requested name length
    emitter.instruction("ldr x3, [x10]");                                       // pass the stored metadata-name pointer
    emitter.instruction("mov x4, x12");                                         // pass the stored metadata-name length
    emitter.instruction("bl __rt_strcasecmp");                                  // compare names with PHP case-insensitive rules
    emitter.instruction("ldr x11, [sp, #32]");                                  // restore the table index after the string compare
    emitter.instruction("cmp x0, #0");                                          // did the requested name match this entry?
    emitter.instruction(&format!("b.eq {local_stem}_hit"));                     // report true on a metadata-name match
    emitter.label(&format!("{local_stem}_skip"));
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the current metadata-name table entry
    emitter.instruction("add x10, x10, #16");                                   // advance to the next metadata-name table entry
    emitter.instruction("str x10, [sp, #24]");                                  // persist the advanced table cursor
    emitter.instruction("add x11, x11, #1");                                    // advance the table index
    emitter.instruction(&format!("b {local_stem}_loop"));                       // continue scanning the metadata-name table
    emitter.label(&format!("{local_stem}_hit"));
    emitter.instruction("mov x0, #1");                                          // return true for a matched metadata name
    emitter.instruction(&format!("b {local_stem}_done"));                       // skip the false result after a match
    emitter.label(&format!("{local_stem}_miss"));
    emitter.instruction("mov x0, #0");                                          // return false when no metadata name matched
    emitter.label(&format!("{local_stem}_done"));
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the name-table lookup frame
    emitter.instruction("ret");                                                 // return the metadata-name existence flag to Rust
}

/// Emits an x86_64 eval wrapper that scans one `(name_ptr, name_len)` metadata table.
fn emit_x86_64_eval_name_table_exists(
    emitter: &mut Emitter,
    exported_label: &str,
    count_symbol: &str,
    table_symbol: &str,
    local_stem: &str,
) {
    label_c_global(emitter, exported_label);
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable name-table lookup frame
    emitter.instruction("sub rsp, 48");                                         // reserve slots for name, count, cursor, and index
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested name length
    abi::emit_symbol_address(emitter, "r10", count_symbol);
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the metadata-name table count
    emitter.instruction("test r10, r10");                                       // is the metadata-name table empty?
    emitter.instruction(&format!("jz {local_stem}_miss"));                      // an empty table cannot contain the requested name
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the table count across string compares
    abi::emit_symbol_address(emitter, "r11", table_symbol);
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the current metadata-name table cursor
    emitter.instruction("xor r11d, r11d");                                      // start scanning at table index zero
    emitter.label(&format!("{local_stem}_loop"));
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the metadata-name table count
    emitter.instruction("cmp r11, r10");                                        // have all metadata-name entries been scanned?
    emitter.instruction(&format!("jae {local_stem}_miss"));                     // no metadata name matched before the end
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current metadata-name table entry
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // load the stored metadata-name length
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare stored and requested name lengths
    emitter.instruction(&format!("jne {local_stem}_skip"));                     // length mismatch means this entry cannot match
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the table index across the string compare
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the requested name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested name length
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // pass the stored metadata-name pointer
    emitter.instruction("call __rt_strcasecmp");                                // compare names with PHP case-insensitive rules
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // restore the table index after the string compare
    emitter.instruction("test rax, rax");                                       // did the requested name match this entry?
    emitter.instruction(&format!("je {local_stem}_hit"));                       // report true on a metadata-name match
    emitter.label(&format!("{local_stem}_skip"));
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current metadata-name table entry
    emitter.instruction("add r10, 16");                                         // advance to the next metadata-name table entry
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the advanced table cursor
    emitter.instruction("inc r11");                                             // advance the table index
    emitter.instruction(&format!("jmp {local_stem}_loop"));                     // continue scanning the metadata-name table
    emitter.label(&format!("{local_stem}_hit"));
    emitter.instruction("mov eax, 1");                                          // return true for a matched metadata name
    emitter.instruction(&format!("jmp {local_stem}_done"));                     // skip the false result after a match
    emitter.label(&format!("{local_stem}_miss"));
    emitter.instruction("xor eax, eax");                                        // return false when no metadata name matched
    emitter.label(&format!("{local_stem}_done"));
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the metadata-name existence flag to Rust
}

/// Emits a global label with platform C-symbol mangling.
fn label_c_global(emitter: &mut Emitter, name: &str) {
    let symbol = emitter.target.extern_symbol(name);
    emitter.label_global(&symbol);
}
