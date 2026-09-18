//! Purpose:
//! Emits the x86_64 `allowed_classes` option decoder and class-membership gate.
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

/// Emits the x86_64 version of the unserialize allowed-class policy helpers.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unserialize_allowed_classes ---");
    emitter.label_global("__rt_unserialize_set_options_mixed");
    emitter.instruction("test rax, rax");                                       // null Mixed cannot carry an options hash
    emitter.instruction("jz __rt_unser_options_type_error_x");                  // null is not a valid options array
    emitter.instruction("cmp QWORD PTR [rax], 4");                              // Mixed(indexed array) is valid default options
    emitter.instruction("jne __rt_unser_options_mixed_assoc_x");                // keep the conditional jump on an assembler-local target
    emitter.instruction("jmp __rt_unserialize_set_options_indexed");            // enter the exported indexed-options helper unconditionally
    emitter.label("__rt_unser_options_mixed_assoc_x");
    emitter.instruction("cmp QWORD PTR [rax], 5");                              // only Mixed(assoc array) can represent the options map
    emitter.instruction("jne __rt_unser_options_type_error_x");                 // reject other tags before payload reads
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // unwrap the validated associative-hash payload
    emitter.instruction("jmp __rt_unserialize_set_options");                    // scan a trusted options hash

    emitter.label_global("__rt_unserialize_set_options_indexed");
    emitter.instruction("ret");                                                 // indexed options have no allowed_classes key

    emitter.label_global("__rt_unserialize_set_options");
    emitter.instruction("push rbp");                                            // preserve the caller frame while options helpers run
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for normalized policy construction
    emitter.instruction("sub rsp, 64");                                         // reserve source, destination, cursor, kind, and conversion ownership spills
    emitter.instruction("mov rdi, rax");                                        // hash_get receiver
    emitter.instruction("lea rsi, [rip + _unser_allowed_classes_key]");         // options key pointer
    emitter.instruction("mov rdx, 15");                                         // options key length
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=payload, rcx=tag
    emitter.instruction("test rax, rax");                                       // did options include allowed_classes?
    emitter.instruction("jz __rt_unser_options_done_x");                        // absent option preserves allow-all policy
    emitter.instruction("cmp rcx, 7");                                          // heterogeneous option hashes store a boxed Mixed value
    emitter.instruction("jne __rt_unser_options_unboxed_x");                    // direct typed payload needs no unwrap
    emitter.instruction("mov rcx, QWORD PTR [rdi]");                            // recover boxed value tag
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // recover boxed value payload
    emitter.label("__rt_unser_options_unboxed_x");
    emitter.instruction("cmp rcx, 3");                                          // boolean option?
    emitter.instruction("jne __rt_unser_options_list_x");                       // arrays are allow-lists
    emitter.instruction("test rdi, rdi");                                       // false blocks all object hydration
    emitter.instruction("jnz __rt_unser_options_done_x");                       // true preserves allow-all policy
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_mode], 1");        // mode 1 = no class hydration
    emitter.instruction("jmp __rt_unser_options_done_x");                       // restore the options helper frame
    emitter.label("__rt_unser_options_list_x");
    emitter.instruction("cmp rcx, 4");                                          // indexed array allow-list?
    emitter.instruction("je __rt_unser_options_indexed_list_setup_x");          // normalize packed values in index order
    emitter.instruction("cmp rcx, 5");                                          // associative array allow-list?
    emitter.instruction("jne __rt_unser_allowed_classes_policy_type_error_x");  // PHP accepts only bool or either PHP array shape
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve associative source hash across allocation
    emitter.instruction("call __rt_hash_count");                                // capacity equals the number of values to normalize
    emitter.instruction("mov rdi, rax");                                        // array_new capacity argument
    emitter.instruction("mov r10, 5");                                          // source kind 5 = associative hash
    emitter.instruction("jmp __rt_unser_options_list_allocate_x");              // share normalized string-array allocation
    emitter.label("__rt_unser_options_indexed_list_setup_x");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve indexed source array across allocation
    emitter.instruction("mov r10, 4");                                          // source kind 4 = indexed array
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // capacity equals packed logical length
    emitter.label("__rt_unser_options_list_allocate_x");
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // remember source shape for iterator dispatch
    emitter.instruction("mov rsi, 16");                                         // normalized entries are direct string pairs
    emitter.instruction("call __rt_array_new");                                 // allocate context-owned normalized allow-list
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // retain normalized owner across validation calls
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // start packed index/hash cursor at zero
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_list_mixed], 0");  // membership scans direct string pairs only
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_mode], 2");        // publish named allow-list mode before validation
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_list], rax");      // errors now release the partial normalized owner through end
    emitter.instruction("cmp QWORD PTR [rbp - 32], 5");                         // associative source?
    emitter.instruction("je __rt_unser_options_hash_list_loop_x");              // hash keys are ignored; values supply class names

    emitter.label("__rt_unser_options_indexed_list_loop_x");
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // indexed source array
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // packed element index
    emitter.instruction("cmp r9, QWORD PTR [r8]");                              // normalized every packed value?
    emitter.instruction("jae __rt_unser_options_done_x");                       // policy is already published
    emitter.instruction("lea r10, [r9 + 1]");                                   // advance index before nested helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // preserve next index across conversion helpers
    emitter.instruction("mov r11, QWORD PTR [r8 - 8]");                         // packed array metadata precedes payload header
    emitter.instruction("shr r11, 8");                                          // move runtime element tag to low bits
    emitter.instruction("and r11, 0x7f");                                       // isolate seven-bit runtime element tag
    emitter.instruction("cmp r11, 1");                                          // direct string payload?
    emitter.instruction("je __rt_unser_options_indexed_string_x");              // direct strings append without conversion
    emitter.instruction("cmp r11, 6");                                          // direct object pointer payload?
    emitter.instruction("je __rt_unser_options_indexed_object_x");              // objects convert through __toString
    emitter.instruction("cmp r11, 7");                                          // heterogeneous packed values use boxed Mixed cells
    emitter.instruction("jne __rt_unser_allowed_classes_entry_metadata_error_x"); // other homogeneous values are invalid class names
    emitter.instruction("mov r10, QWORD PTR [r8 + r9 * 8 + 24]");               // selected boxed Mixed pointer
    emitter.instruction("test r10, r10");                                       // null/malformed cell?
    emitter.instruction("jz __rt_unser_allowed_classes_entry_null_error_x");    // report PHP null
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // boxed runtime tag
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // boxed low payload
    emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                       // boxed high payload
    emitter.instruction("cmp r11, 1");                                          // boxed string entry?
    emitter.instruction("je __rt_unser_options_append_borrowed_x");             // append the boxed string's borrowed bytes
    emitter.instruction("cmp r11, 6");                                          // boxed object entry?
    emitter.instruction("je __rt_unser_options_boxed_object_x");                // unwrap the object for __toString conversion
    emitter.instruction("mov rax, r11");                                        // forward invalid boxed runtime tag
    emitter.instruction("mov rdi, rsi");                                        // forward invalid boxed low payload
    emitter.instruction("jmp __rt_unser_allowed_classes_entry_type_error_x");   // other boxed types are invalid class names
    emitter.label("__rt_unser_options_indexed_string_x");
    emitter.instruction("mov r10, r9");                                         // copy index before scaling
    emitter.instruction("shl r10, 4");                                          // direct string-pair stride is sixteen bytes
    emitter.instruction("lea r10, [r8 + r10 + 24]");                            // select current direct string pair
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // borrow source string pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // borrow source string length
    emitter.instruction("jmp __rt_unser_options_append_borrowed_x");            // append the borrowed string as an allowed name
    emitter.label("__rt_unser_options_indexed_object_x");
    emitter.instruction("mov rax, QWORD PTR [r8 + r9 * 8 + 24]");               // load selected direct object pointer
    emitter.instruction("test rax, rax");                                       // null object slot?
    emitter.instruction("jz __rt_unser_allowed_classes_entry_null_error_x");    // report PHP null
    emitter.instruction("jmp __rt_unser_options_convert_object_x");             // convert the entry object via __toString
    emitter.label("__rt_unser_options_boxed_object_x");
    emitter.instruction("mov rax, rsi");                                        // boxed low payload is object pointer
    emitter.instruction("jmp __rt_unser_options_convert_object_x");             // convert the boxed object via __toString

    emitter.label("__rt_unser_options_hash_list_loop_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // associative source hash
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // insertion-order iterator cursor
    emitter.instruction("call __rt_hash_iter_next_value");                      // return next borrowed value tuple; ignore key registers
    emitter.instruction("cmp rax, -1");                                         // exhausted every associative value?
    emitter.instruction("je __rt_unser_options_done_x");                        // normalized policy is ready
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve next iterator cursor
    emitter.instruction("cmp r9, 7");                                           // boxed Mixed hash value?
    emitter.instruction("jne __rt_unser_options_hash_unboxed_x");               // direct typed values skip the unwrap
    emitter.instruction("test rcx, rcx");                                       // null boxed cell?
    emitter.instruction("jz __rt_unser_allowed_classes_entry_null_error_x");    // report PHP null
    emitter.instruction("mov r10, rcx");                                        // preserve boxed cell pointer for payload reads
    emitter.instruction("mov r9, QWORD PTR [r10]");                             // unwrap actual value tag
    emitter.instruction("mov rcx, QWORD PTR [r10 + 8]");                        // unwrap low payload
    emitter.instruction("mov r8, QWORD PTR [r10 + 16]");                        // unwrap high payload
    emitter.label("__rt_unser_options_hash_unboxed_x");
    emitter.instruction("cmp r9, 1");                                           // string hash value?
    emitter.instruction("je __rt_unser_options_hash_string_x");                 // strings append without conversion
    emitter.instruction("cmp r9, 6");                                           // object hash value?
    emitter.instruction("je __rt_unser_options_hash_object_x");                 // objects convert through __toString
    emitter.instruction("mov rax, r9");                                         // forward invalid value runtime tag
    emitter.instruction("mov rdi, rcx");                                        // forward invalid low payload
    emitter.instruction("jmp __rt_unser_allowed_classes_entry_type_error_x");   // other value types are invalid class names
    emitter.label("__rt_unser_options_hash_string_x");
    emitter.instruction("mov rsi, rcx");                                        // normalize iterator string pointer to push ABI
    emitter.instruction("mov rdx, r8");                                         // normalize iterator string length to push ABI
    emitter.instruction("jmp __rt_unser_options_append_borrowed_x");            // append the borrowed hash-value string
    emitter.label("__rt_unser_options_hash_object_x");
    emitter.instruction("mov rax, rcx");                                        // iterator low payload is object pointer

    emitter.label("__rt_unser_options_convert_object_x");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve offending object for Error and owner cleanup
    emitter.instruction("call __rt_unser_allowed_object_to_string");            // invoke __toString under internal exception boundary
    emitter.instruction("test rax, rax");                                       // did the class expose __toString?
    emitter.instruction("jz __rt_unser_allowed_classes_entry_object_error_x");  // absent method raises PHP conversion Error
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve transient method-return owner across append
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // preserve returned byte length while classifying ownership
    emitter.instruction("call __rt_heap_kind");                                 // kind 7 is transferred in place by str_persist inside push_str
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve original heap kind across normalized append
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload array_push_str string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reload array_push_str string length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // normalized destination array
    emitter.instruction("call __rt_array_push_str");                            // persist and append converted class name
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // retain possibly-grown destination
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_list], rax");      // keep cleanup owner current after possible growth
    emitter.instruction("cmp QWORD PTR [rbp - 56], 7");                         // did push_str take over a concat temporary in place?
    emitter.instruction("je __rt_unser_options_list_continue_x");               // transferred storage is now owned by the normalized array
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // recover transient __toString return owner
    emitter.instruction("call __rt_heap_free_safe");                            // release it after array_push_str copied the bytes
    emitter.instruction("jmp __rt_unser_options_list_continue_x");              // process the next source value

    emitter.label("__rt_unser_options_append_borrowed_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // normalized destination array
    emitter.instruction("call __rt_array_push_str");                            // persist and append borrowed source bytes
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // retain possibly-grown destination
    emitter.instruction("mov QWORD PTR [rip + _unser_allowed_list], rax");      // keep cleanup owner current after possible growth
    emitter.label("__rt_unser_options_list_continue_x");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 5");                         // resume the matching source traversal
    emitter.instruction("je __rt_unser_options_hash_list_loop_x");              // continue associative values
    emitter.instruction("jmp __rt_unser_options_indexed_list_loop_x");          // otherwise continue packed values
    emitter.label("__rt_unser_options_done_x");
    emitter.instruction("leave");                                               // restore the caller frame after policy normalization helpers
    emitter.instruction("ret");                                                 // caller proceeds to parsing

    emitter.label_global("__rt_unserialize_class_allowed");
    emitter.instruction("push rbp");                                            // preserve caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish helper frame
    emitter.instruction("sub rsp, 48");                                         // class name plus list scan state
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // class name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // class name length
    emitter.instruction("mov r8, QWORD PTR [rip + _unser_allowed_mode]");       // current policy mode
    emitter.instruction("test r8, r8");                                         // mode 0 = allow all
    emitter.instruction("jz __rt_unser_class_allowed_yes_x");                   // hydrate unrestricted classes
    emitter.instruction("cmp r8, 1");                                           // mode 1 = block all
    emitter.instruction("je __rt_unser_class_allowed_no_x");                    // never hydrate blocked classes
    emitter.instruction("mov r8, QWORD PTR [rip + _unser_allowed_list]");       // indexed string-array payload
    emitter.instruction("test r8, r8");                                         // malformed list fails closed
    emitter.instruction("jz __rt_unser_class_allowed_no_x");                    // no list cannot grant hydration
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // list length
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // save list base across strcmp
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // save list length
    emitter.instruction("xor r10d, r10d");                                      // list cursor
    emitter.label("__rt_unser_class_allowed_loop_x");
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // exhausted every allowed name?
    emitter.instruction("jae __rt_unser_class_allowed_no_x");                   // no exact match means incomplete object
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // list base
    emitter.instruction("cmp QWORD PTR [rip + _unser_allowed_list_mixed], 0");  // does this list contain boxed Mixed strings?
    emitter.instruction("jne __rt_unser_class_allowed_mixed_cell_x");           // boxed cells need an extra dereference
    emitter.instruction("mov rax, r10");                                        // preserve the list cursor while deriving a byte offset
    emitter.instruction("shl rax, 4");                                          // scale the cursor by the 16-byte direct-string pair stride
    emitter.instruction("add r11, rax");                                        // advance the list base to the selected direct-string pair
    emitter.instruction("add r11, 24");                                         // skip the indexed-array header before reading the string pair
    emitter.instruction("jmp __rt_unser_class_allowed_compare_x");              // compare against the requested class name
    emitter.label("__rt_unser_class_allowed_mixed_cell_x");
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8 + 24]");             // validated boxed string cell
    emitter.instruction("add r11, 8");                                          // string payload pair follows its Mixed tag
    emitter.label("__rt_unser_class_allowed_compare_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // requested class-name length
    emitter.instruction("mov rdx, QWORD PTR [r11]");                            // allow-list string pointer
    emitter.instruction("mov rcx, QWORD PTR [r11 + 8]");                        // allow-list string length
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve cursor across the string helper
    emitter.instruction("call __rt_strcasecmp");                                // PHP class names compare case-insensitively
    emitter.instruction("test rax, rax");                                       // comparison result
    emitter.instruction("jz __rt_unser_class_allowed_yes_x");                   // matching allow-list entry grants hydration
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // restore cursor after helper call
    emitter.instruction("add r10, 1");                                          // advance after mismatch
    emitter.instruction("jmp __rt_unser_class_allowed_loop_x");                 // scan next allowed class
    emitter.label("__rt_unser_class_allowed_yes_x");
    emitter.instruction("mov eax, 1");                                          // true = instantiate and hydrate
    emitter.instruction("jmp __rt_unser_class_allowed_return_x");               // shared frame teardown
    emitter.label("__rt_unser_class_allowed_no_x");
    emitter.instruction("xor eax, eax");                                        // false = incomplete object without hooks
    emitter.label("__rt_unser_class_allowed_return_x");
    emitter.instruction("leave");                                               // restore caller frame and stack
    emitter.instruction("ret");                                                 // return allow decision

    emitter.label("__rt_unser_options_type_error_x");
    emitter.instruction("test rax, rax");                                       // a null Mixed pointer reports null
    emitter.instruction("jz __rt_unser_options_type_null_error_x");             // report the rejected options value as null
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // rejected runtime payload
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // rejected runtime tag
    emitter.instruction("jmp __rt_unser_options_type_dispatch_x");              // compose the options TypeError message
    emitter.label("__rt_unser_options_type_null_error_x");
    emitter.instruction("mov eax, 8");                                          // runtime null tag
    emitter.instruction("xor edi, edi");                                        // null has no payload
    emitter.label("__rt_unser_options_type_dispatch_x");
    emitter.instruction("lea rsi, [rip + _unser_options_type_prefix]");         // options diagnostic prefix
    emitter.instruction(&format!("mov rdx, {}", UNSER_OPTIONS_TYPE_PREFIX.len())); // prefix byte length
    emitter.instruction("jmp __rt_unser_throw_type_error");                     // helper closes context and throws exactly once

    emitter.label("__rt_unser_allowed_classes_entry_null_error_x");
    emitter.instruction("mov eax, 8");                                          // forward the invalid null-entry tag directly
    emitter.instruction("xor edi, edi");                                        // null has no payload
    emitter.instruction("jmp __rt_unser_allowed_classes_entry_type_error_x");   // share the entry TypeError cleanup path
    emitter.label("__rt_unser_allowed_classes_entry_metadata_error_x");
    emitter.instruction("mov rax, r11");                                        // packed element metadata uses runtime tag numbering
    emitter.instruction("xor edi, edi");                                        // homogeneous metadata has no single payload
    emitter.label("__rt_unser_allowed_classes_entry_type_error_x");
    emitter.instruction("leave");                                               // remove options decoder frame before common cleanup
    emitter.instruction("lea rsi, [rip + _unser_allowed_classes_entry_prefix]"); // list-entry diagnostic prefix
    emitter.instruction(&format!("mov rdx, {}", UNSER_ALLOWED_CLASSES_ENTRY_PREFIX.len())); // prefix byte length
    emitter.instruction("jmp __rt_unser_throw_type_error");                     // helper closes context and throws exactly once

    emitter.label("__rt_unser_allowed_classes_entry_object_error_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // recover offending object while source storage remains live
    emitter.instruction("leave");                                               // remove options decoder frame before Error construction
    emitter.instruction("jmp __rt_unser_throw_object_string_error");            // helper resolves class name, closes context once, and throws Error

    emitter.label("__rt_unser_allowed_classes_policy_type_error_x");
    emitter.instruction("mov rax, rcx");                                        // rejected policy runtime tag
    emitter.instruction("leave");                                               // remove options decoder frame before common cleanup
    emitter.instruction("lea rsi, [rip + _unser_allowed_classes_policy_prefix]"); // policy diagnostic prefix
    emitter.instruction(&format!("mov rdx, {}", UNSER_ALLOWED_CLASSES_POLICY_PREFIX.len())); // prefix byte length
    emitter.instruction("jmp __rt_unser_throw_type_error");                     // helper closes context and throws exactly once
}
