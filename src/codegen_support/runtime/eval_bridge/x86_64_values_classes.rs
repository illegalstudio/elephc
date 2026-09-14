//! Purpose:
//! Emits x86_64 scalar, object, and class-query eval wrappers.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Wrapper order and SysV C ABI labels match the bridge contract.

use super::*;

/// Emits x86_64 scalar, object, and class-query eval wrappers.
pub(super) fn emit_x86_64_values_classes(emitter: &mut Emitter) {
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
    emitter.instruction("sub rsp, 16");                                         // reserve slots for the raw object and boxed result
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
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the raw object owner before boxing it for eval
    emitter.instruction("mov rdi, rax");                                        // move the allocated object pointer into the Mixed payload
    emitter.instruction("mov eax, 6");                                          // runtime tag 6 = object
    emitter.instruction("xor esi, esi");                                        // object payloads do not use a high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the allocated object for Rust
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the boxed Mixed while consuming the raw object owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the raw object owner created by the allocator
    emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                      // load the raw object refcount after Mixed boxing retained it
    emitter.instruction("sub r10d, 1");                                         // consume the allocator-owned object reference locally
    emitter.instruction("mov DWORD PTR [rax - 12], r10d");                      // leave the boxed Mixed as the sole object owner
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // restore the boxed object Mixed as the Rust return value
    emitter.instruction("jmp __elephc_eval_value_new_object_done_x86");         // skip the null boxing path after successful allocation
    emitter.label("__elephc_eval_value_new_object_null_x86");
    emitter.instruction("mov eax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor edi, edi");                                        // null has no low payload word
    emitter.instruction("xor esi, esi");                                        // null has no high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box null for unknown eval class names
    emitter.label("__elephc_eval_value_new_object_done_x86");
    emitter.instruction("mov rsp, rbp");                                        // release the dynamic-object wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed object or null Mixed cell to Rust

    emit_x86_64_object_from_raw_wrapper(emitter);
    emit_x86_64_install_dynamic_object_destructor_hook(emitter);
    emit_x86_64_object_clone_shallow_wrapper(emitter);

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

    emit_x86_64_eval_reflection_method_names(emitter);
    emit_x86_64_eval_reflection_property_names(emitter);
    emit_x86_64_eval_reflection_class_interface_names(emitter);
    emit_x86_64_eval_reflection_class_trait_names(emitter);
    emit_x86_64_eval_reflection_class_trait_alias_names(emitter);
    emit_x86_64_eval_reflection_class_trait_alias_sources(emitter);
    emit_x86_64_eval_reflection_source_file(emitter);
    emit_x86_64_eval_reflection_class_flags(emitter);
    emit_x86_64_eval_reflection_method_flags(emitter);
    emit_x86_64_eval_reflection_method_declaring_class(emitter);
    emit_x86_64_eval_reflection_property_declaring_class(emitter);
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
    emitter.instruction("push rbp");                                            // align the helper call and preserve caller linkage
    emitter.instruction("mov rax, rdi");                                        // pass the possibly referenced object
    emitter.instruction("call __rt_mixed_deref");                               // resolve the concrete receiver before class-name lookup
    emitter.instruction("pop rbp");                                             // restore linkage for the leaf metadata lookup
    emitter.instruction("mov rdi, rax");                                        // inspect the concrete object cell
    emitter.instruction("test rdi, rdi");                                       // reject null boxed handles before reading their tag
    emitter.instruction("jz __elephc_eval_value_object_class_name_miss_x86");   // null handles cannot provide a class name
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed eval value runtime tag
    emitter.instruction("cmp r10, 6");                                          // tag 6 is an object payload
    emitter.instruction("jne __elephc_eval_value_object_class_name_miss_x86");  // non-objects cannot provide a class name
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the object payload pointer
    emit_branch_if_null_container(
        emitter,
        "r10",
        "r11",
        "__elephc_eval_value_object_class_name_miss_x86",
    );
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

}
