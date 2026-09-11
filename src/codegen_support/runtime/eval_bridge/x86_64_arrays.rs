//! Purpose:
//! Emits x86_64 array and object-property iteration eval wrappers.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Key normalization and boxed Mixed ownership remain runtime-compatible.

use super::*;

/// Emits x86_64 array and object-property iteration eval wrappers.
pub(super) fn emit_x86_64_arrays(emitter: &mut Emitter) {
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
    emitter.instruction("xor ecx, ecx");                                        // eval bridge lookup reports misses through its own result contract
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
    emitter.instruction("mov rax, rdi");                                        // pass the potentially referenced array receiver
    emitter.instruction("call __rt_mixed_deref");                               // inspect the current array held by a persistent reference
    emitter.instruction("mov rdi, rax");                                        // restore the concrete receiver argument
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // retain the concrete receiver for the remaining lookup
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
    emit_branch_if_null_container(
        emitter,
        "rdi",
        "r10",
        "__elephc_eval_value_array_key_exists_false",
    );
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass normalized integer key to the bounds helper
    emitter.instruction("call __rt_array_key_exists");                          // return whether the integer key is in bounds
    emitter.instruction("jmp __elephc_eval_value_array_key_exists_box");        // box the existence flag for Rust
    emitter.label("__elephc_eval_value_array_key_exists_assoc");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the boxed associative-array receiver
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // load the hash payload pointer
    emit_branch_if_null_container(
        emitter,
        "rdi",
        "r10",
        "__elephc_eval_value_array_key_exists_false",
    );
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
    emitter.instruction("mov rax, rdi");                                        // pass the potentially referenced array receiver
    emitter.instruction("call __rt_mixed_deref");                               // inspect the current array held by a persistent reference
    emitter.instruction("mov rdi, rax");                                        // restore the concrete receiver argument
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // retain the concrete receiver for the remaining lookup
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
    emit_branch_if_null_container(
        emitter,
        "r10",
        "r11",
        "__elephc_eval_value_array_iter_key_null",
    );
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
    emitter.instruction("push rbp");                                            // preserve linkage and align borrowed dereferencing
    emitter.instruction("mov rax, rdi");                                        // pass the potentially referenced array
    emitter.instruction("call __rt_mixed_deref");                               // count elements in the current referenced array
    emitter.instruction("pop rbp");                                             // restore caller linkage before leaf count branches
    emitter.instruction("mov rdi, rax");                                        // inspect the concrete array cell
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
    emit_branch_if_null_container(
        emitter,
        "r10",
        "r11",
        "__elephc_eval_value_array_len_zero",
    );
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the runtime container element count
    emitter.instruction("ret");                                                 // return the element count to Rust

    label_c_global(emitter, "__elephc_eval_value_object_property_len");
    emitter.instruction("test rdi, rdi");                                       // null handles have no JSON-visible object properties
    emitter.instruction("jz __elephc_eval_value_object_property_len_zero");     // report zero properties for null runtime cells
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the boxed Mixed runtime tag
    emitter.instruction("cmp r10, 6");                                          // tag 6 = object
    emitter.instruction("jne __elephc_eval_value_object_property_len_zero");    // non-objects expose no JSON-visible properties here
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the object payload pointer
    emit_branch_if_null_container(
        emitter,
        "r10",
        "r11",
        "__elephc_eval_value_object_property_len_zero",
    );
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
    emit_branch_if_null_container(
        emitter,
        "r10",
        "r11",
        "__elephc_eval_value_object_property_iter_key_null",
    );
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

}
