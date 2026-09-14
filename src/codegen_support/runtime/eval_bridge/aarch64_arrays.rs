//! Purpose:
//! Emits AArch64 array and object-property iteration eval wrappers.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Key normalization and boxed Mixed ownership remain runtime-compatible.

use super::*;

/// Emits AArch64 array and object-property iteration eval wrappers.
pub(super) fn emit_aarch64_arrays(emitter: &mut Emitter) {
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
    emitter.instruction("mov x3, xzr");                                         // eval bridge lookup reports misses through its own result contract
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
    emitter.instruction("bl __rt_mixed_deref");                                 // inspect the current array held by a persistent reference
    emitter.instruction("str x0, [sp, #0]");                                    // retain the concrete receiver for the remaining lookup
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
    emit_branch_if_null_container(
        emitter,
        "x0",
        "x9",
        "__elephc_eval_value_array_key_exists_false",
    );
    emitter.instruction("ldr x1, [sp, #8]");                                    // pass normalized integer key to the bounds helper
    emitter.instruction("bl __rt_array_key_exists");                            // return whether the integer key is in bounds
    emitter.instruction("b __elephc_eval_value_array_key_exists_box");          // box the existence flag for Rust
    emitter.label("__elephc_eval_value_array_key_exists_assoc");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the boxed associative-array receiver
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the hash payload pointer
    emit_branch_if_null_container(
        emitter,
        "x0",
        "x9",
        "__elephc_eval_value_array_key_exists_false",
    );
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
    emitter.instruction("bl __rt_mixed_deref");                                 // inspect the current array held by a persistent reference
    emitter.instruction("str x0, [sp, #0]");                                    // retain the concrete receiver for the remaining lookup
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
    emit_branch_if_null_container(
        emitter,
        "x9",
        "x10",
        "__elephc_eval_value_array_iter_key_null",
    );
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
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve caller linkage for borrowed dereferencing
    emitter.instruction("bl __rt_mixed_deref");                                 // count elements in the current referenced array
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore caller linkage before the leaf count branches
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
    emit_branch_if_null_container(
        emitter,
        "x9",
        "x10",
        "__elephc_eval_value_array_len_zero",
    );
    emitter.instruction("ldr x0, [x9]");                                        // load the runtime container element count
    emitter.instruction("ret");                                                 // return the element count to Rust

    label_c_global(emitter, "__elephc_eval_value_object_property_len");
    emitter.instruction("cbz x0, __elephc_eval_value_object_property_len_zero"); // null handles have no JSON-visible object properties
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed Mixed runtime tag
    emitter.instruction("cmp x9, #6");                                          // tag 6 = object
    emitter.instruction("b.ne __elephc_eval_value_object_property_len_zero");   // non-objects expose no JSON-visible properties here
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the object payload pointer
    emit_branch_if_null_container(
        emitter,
        "x9",
        "x10",
        "__elephc_eval_value_object_property_len_zero",
    );
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
    emit_branch_if_null_container(
        emitter,
        "x9",
        "x10",
        "__elephc_eval_value_object_property_iter_key_null",
    );
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

}
