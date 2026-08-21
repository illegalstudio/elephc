//! Purpose:
//! Emits x86_64 value predicates, raw handles, and cast wrappers.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Mixed tag dispatch and raw heap ownership are preserved.

use super::*;

/// Emits x86_64 value predicates, raw handles, and cast wrappers.
pub(super) fn emit_x86_64_casts(emitter: &mut Emitter) {
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

    label_c_global(emitter, "__elephc_eval_value_invoker_ref_cell");
    emitter.instruction(&format!("mov rax, {}", INVOKER_ARG_REF_CELL_TAG));     // runtime tag 11 marks descriptor-invoker by-reference args
    emitter.instruction(&format!("mov rsi, {}", EVAL_RUNTIME_TAG_MIXED));       // source tag 7 tells invoker fallback paths the slot stores Mixed
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the marker cell and return it to Rust

    label_c_global(emitter, "__elephc_eval_value_invoker_raw_ref_cell");
    emitter.instruction(&format!("mov rax, {}", INVOKER_ARG_REF_CELL_TAG));     // runtime tag 11 marks descriptor-invoker by-reference args
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the marker cell and return it to Rust

    label_c_global(emitter, "__elephc_eval_value_raw_word");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // move the boxed scalar cell into mixed_unbox input
    emitter.instruction("call __rt_mixed_unbox");                               // expose the boxed scalar payload words
    emitter.instruction("mov rax, rdi");                                        // return the scalar low payload word to Rust
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the raw payload word

    label_c_global(emitter, "__elephc_eval_value_raw_high_word");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // move the boxed cell into mixed_unbox input
    emitter.instruction("call __rt_mixed_unbox");                               // expose the boxed payload words
    emitter.instruction("mov rax, rdx");                                        // return the high payload word to Rust
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the raw high payload word

    label_c_global(emitter, "__elephc_eval_value_retain_raw_string");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer while persisting a raw string
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve space for the Rust out-len pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // save the Rust out-len pointer across str_persist
    emitter.instruction("mov rax, rdi");                                        // move the raw string pointer into str_persist input
    emitter.instruction("mov rdx, rsi");                                        // move the raw string length into str_persist input
    emitter.instruction("call __rt_str_persist");                               // duplicate the string for staged by-ref ownership
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the Rust out-len pointer
    emitter.instruction("mov QWORD PTR [r10], rdx");                            // report the persisted string length to Rust
    emitter.instruction("add rsp, 16");                                         // release the string retain wrapper spill slot
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the retained raw string pointer

    label_c_global(emitter, "__elephc_eval_value_from_raw_string");
    emitter.instruction("mov rax, 1");                                          // runtime tag 1 = string
    emitter.instruction("mov rdx, rsi");                                        // move the raw string length into the Mixed high word
    emitter.instruction("jmp __rt_mixed_from_value");                           // persist and box the raw string payload for eval

    label_c_global(emitter, "__elephc_eval_value_release_raw_string");
    emitter.instruction("mov rax, rdi");                                        // move the raw string pointer into the runtime release input register
    emitter.instruction("jmp __rt_heap_free_safe");                             // release the staged raw string owner

    label_c_global(emitter, "__elephc_eval_value_retain_raw_heap_word");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer while retaining a raw heap word
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve space for the raw heap word return value
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the raw heap word across incref
    emitter.instruction("mov rax, rdi");                                        // move the raw heap word into the runtime incref input register
    emitter.instruction("call __rt_incref");                                    // retain the heap payload for the staged by-ref slot
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the original raw heap word to Rust
    emitter.instruction("add rsp, 16");                                         // release the retain wrapper spill slot
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the retained raw heap word

    label_c_global(emitter, "__elephc_eval_value_from_raw_word");
    emitter.instruction("mov rax, rdi");                                        // move the runtime tag into the mixed boxing tag register
    emitter.instruction("mov rdi, rsi");                                        // move the raw scalar word into the low payload register
    emitter.instruction("xor esi, esi");                                        // raw one-word scalar payloads have no high payload word
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the raw scalar payload and return it to Rust

    label_c_global(emitter, "__elephc_eval_value_from_raw_heap_word");
    emitter.instruction("test rdi, rdi");                                       // reject null raw heap words before reading their header
    emitter.instruction("jz __elephc_eval_value_from_raw_heap_word_miss_x86");  // null raw heap words cannot be boxed
    emitter.instruction("mov r10, QWORD PTR [rdi - 8]");                        // load the uniform x86_64 heap kind word for tag recovery
    emitter.instruction("and r10, 0xff");                                       // isolate the low-byte heap kind tag
    emitter.instruction("cmp r10, 2");                                          // heap kind 2 stores indexed-array payloads
    emitter.instruction("je __elephc_eval_value_from_raw_heap_word_array_x86"); // box indexed arrays with runtime tag 4
    emitter.instruction("cmp r10, 3");                                          // heap kind 3 stores associative hash payloads
    emitter.instruction("je __elephc_eval_value_from_raw_heap_word_hash_x86");  // box hashes with runtime tag 5
    emitter.instruction("cmp r10, 4");                                          // heap kind 4 stores object payloads
    emitter.instruction("je __elephc_eval_value_from_raw_heap_word_object_x86"); // box objects with runtime tag 6
    emitter.instruction("cmp r10, 5");                                          // heap kind 5 stores boxed Mixed payloads
    emitter.instruction("je __elephc_eval_value_from_raw_heap_word_mixed_x86"); // box nested Mixed cells with runtime tag 7
    emitter.label("__elephc_eval_value_from_raw_heap_word_miss_x86");
    emitter.instruction("xor eax, eax");                                        // report malformed raw heap words as a null Rust handle
    emitter.instruction("ret");                                                 // return the failed boxing sentinel
    emitter.label("__elephc_eval_value_from_raw_heap_word_array_x86");
    emitter.instruction("mov rax, 4");                                          // runtime tag 4 = indexed array
    emitter.instruction("jmp __elephc_eval_value_from_raw_heap_word_box_x86");  // share the one-word heap boxing tail
    emitter.label("__elephc_eval_value_from_raw_heap_word_hash_x86");
    emitter.instruction("mov rax, 5");                                          // runtime tag 5 = associative array
    emitter.instruction("jmp __elephc_eval_value_from_raw_heap_word_box_x86");  // share the one-word heap boxing tail
    emitter.label("__elephc_eval_value_from_raw_heap_word_object_x86");
    emitter.instruction("mov rax, 6");                                          // runtime tag 6 = object
    emitter.instruction("jmp __elephc_eval_value_from_raw_heap_word_box_x86");  // share the one-word heap boxing tail
    emitter.label("__elephc_eval_value_from_raw_heap_word_mixed_x86");
    emitter.instruction("mov rax, 7");                                          // runtime tag 7 = Mixed
    emitter.label("__elephc_eval_value_from_raw_heap_word_box_x86");
    emitter.instruction("xor esi, esi");                                        // one-word heap payloads do not use a high word
    emitter.instruction("jmp __rt_mixed_from_value");                           // retain and box the raw heap payload for eval

    label_c_global(emitter, "__elephc_eval_value_release_raw_heap_word");
    emitter.instruction("mov rax, rdi");                                        // move the raw heap word into the runtime release input register
    emitter.instruction("jmp __rt_decref_any");                                 // release the staged raw heap slot owner

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

    // The PHP OBJECT HANDLE bridge — see the AArch64 arm for why it is separate
    // from the address-shaped `object_identity` above.
    label_c_global(emitter, "__elephc_eval_value_object_handle");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable object-handle wrapper frame
    emitter.instruction("mov rax, rdi");                                        // pass the boxed Mixed argument to mixed_unbox
    emitter.instruction("call __rt_mixed_unbox");                               // unwrap nested Mixed cells to tag and object payload
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means PHP object
    emitter.instruction("jne __elephc_eval_value_object_handle_zero_x86");      // non-object values carry no PHP handle
    emitter.instruction("mov rax, rdi");                                        // pass the unboxed object payload to the handle pool
    emitter.instruction("call __rt_object_handle_of");                          // rax = this object's PHP handle
    emitter.instruction("jmp __elephc_eval_value_object_handle_done_x86");      // return the resolved handle
    emitter.label("__elephc_eval_value_object_handle_zero_x86");
    emitter.instruction("xor eax, eax");                                        // report "no handle" for non-object values
    emitter.label("__elephc_eval_value_object_handle_done_x86");
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the PHP object handle to Rust

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
    emitter.instruction("cmp rax, 9");                                          // is the eval value a resource?
    emitter.instruction("je __elephc_eval_value_cast_string_resource_x86");     // resources render as PHP's \"Resource id #N\"
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
    emitter.label("__elephc_eval_value_cast_string_resource_x86");
    emitter.instruction("mov rax, rdi");                                        // pass the native resource payload to the display formatter
    emitter.instruction("call __rt_resource_to_string");                        // format \"Resource id #N\" into the shared concat scratch
    emitter.instruction("mov rdi, rax");                                        // move the formatted string pointer into mixed value_lo
    emitter.instruction("mov rsi, rdx");                                        // move the formatted string length into mixed value_hi
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string
    emitter.instruction("call __rt_mixed_from_value");                          // persist and box the borrowed resource display string
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
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("push rdi");                                            // preserve the eval payload across the context call
    emitter.instruction("sub rsp, 8");                                          // realign the stack for the context call
    emitter.instruction("call __rt_stream_default_context_ensure");             // PHP mints id 4 for the request default BEFORE the first stream
    emitter.instruction("add rsp, 8");                                          // release the alignment padding
    emitter.instruction("pop rdi");                                             // restore the eval payload into mixed value_lo
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("mov eax, 9");                                          // runtime tag 9 = resource, with C id already in rdi
    emitter.instruction("xor esi, esi");                                        // resource payloads do not use a high word
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the resource payload and return to Rust

    label_c_global(emitter, "__elephc_eval_value_hash_context");
    emitter.instruction("mov eax, 9");                                          // runtime tag 9 = resource, with the eval table key already in rdi
    emitter.instruction("mov esi, 5");                                          // resource kind 5 = eval-owned inert handle: no PHP id, no destructor
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the inert hash-context payload and return to Rust

    label_c_global(emitter, "__elephc_eval_resource_is_closed");
    emitter.instruction("push rbp");                                            // align the stack and preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("mov rax, rdi");                                        // internal runtime helpers take their payload in rax
    emitter.instruction("call __rt_resource_type_name");                        // ask the resource registry which name this payload reports
    abi::emit_symbol_address(emitter, "rcx", "_resource_type_unknown");
    emitter.instruction("cmp rax, rcx");                                        // a closed handle is exactly the one that reports the Unknown literal
    emitter.instruction("mov eax, 0");                                          // clear the result register without disturbing the compare flags
    emitter.instruction("sete al");                                             // report closed as 1 and open as 0
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the closed predicate to Rust

    label_c_global(emitter, "__elephc_eval_value_float");
    emitter.instruction("movq rdi, xmm0");                                      // move the C double bits into mixed value_lo
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the double payload and return to Rust

    label_c_global(emitter, "__elephc_eval_value_string");
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string, with C ptr/len already in rdi/rsi
    emitter.instruction("jmp __rt_mixed_from_value");                           // persist and box the string payload for eval

}
