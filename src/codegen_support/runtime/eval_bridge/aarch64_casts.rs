//! Purpose:
//! Emits AArch64 value predicates, raw handles, and cast wrappers.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Mixed tag dispatch and raw heap ownership are preserved.

use super::*;

/// Emits AArch64 value predicates, raw handles, and cast wrappers.
pub(super) fn emit_aarch64_casts(emitter: &mut Emitter) {
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

    label_c_global(emitter, "__elephc_eval_value_invoker_ref_cell");
    emitter.instruction("mov x1, x0");                                          // pass the staged Mixed slot address as marker payload
    emitter.instruction(&format!("mov x0, #{}", INVOKER_ARG_REF_CELL_TAG));     // runtime tag 11 marks descriptor-invoker by-reference args
    emitter.instruction(&format!("mov x2, #{}", EVAL_RUNTIME_TAG_MIXED));       // source tag 7 tells invoker fallback paths the slot stores Mixed
    emitter.instruction("b __rt_mixed_from_value");                             // box the marker cell and return it to Rust

    label_c_global(emitter, "__elephc_eval_value_invoker_raw_ref_cell");
    emitter.instruction("mov x2, x1");                                          // pass the staged raw slot source tag as marker metadata
    emitter.instruction("mov x1, x0");                                          // pass the staged raw slot address as marker payload
    emitter.instruction(&format!("mov x0, #{}", INVOKER_ARG_REF_CELL_TAG));     // runtime tag 11 marks descriptor-invoker by-reference args
    emitter.instruction("b __rt_mixed_from_value");                             // box the marker cell and return it to Rust

    label_c_global(emitter, "__elephc_eval_value_raw_word");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while unboxing the scalar cell
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across mixed_unbox
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the wrapper call
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the boxed scalar payload words
    emitter.instruction("mov x0, x1");                                          // return the scalar low payload word to Rust
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore caller frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the wrapper frame
    emitter.instruction("ret");                                                 // return the raw payload word

    label_c_global(emitter, "__elephc_eval_value_raw_high_word");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while unboxing the string cell
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across mixed_unbox
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the wrapper call
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the boxed payload words
    emitter.instruction("mov x0, x2");                                          // return the high payload word to Rust
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore caller frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the wrapper frame
    emitter.instruction("ret");                                                 // return the raw high payload word

    label_c_global(emitter, "__elephc_eval_value_retain_raw_string");
    emitter.instruction("sub sp, sp, #48");                                     // reserve a wrapper frame while persisting the raw string
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across str_persist
    emitter.instruction("add x29, sp, #32");                                    // establish a stable frame pointer
    emitter.instruction("str x2, [sp, #0]");                                    // save the Rust out-len pointer across string persistence
    emitter.instruction("mov x2, x1");                                          // move the raw string length into str_persist input
    emitter.instruction("mov x1, x0");                                          // move the raw string pointer into str_persist input
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the string for staged by-ref ownership
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the Rust out-len pointer
    emitter.instruction("str x2, [x9]");                                        // report the persisted string length to Rust
    emitter.instruction("mov x0, x1");                                          // return the persisted string pointer to Rust
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the string retain wrapper frame
    emitter.instruction("ret");                                                 // return the retained raw string pointer

    label_c_global(emitter, "__elephc_eval_value_from_raw_string");
    emitter.instruction("mov x2, x1");                                          // move the raw string length into the Mixed high word
    emitter.instruction("mov x1, x0");                                          // move the raw string pointer into the Mixed low word
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("b __rt_mixed_from_value");                             // persist and box the raw string payload for eval

    label_c_global(emitter, "__elephc_eval_value_release_raw_string");
    emitter.instruction("b __rt_heap_free_safe");                               // release the staged raw string owner

    label_c_global(emitter, "__elephc_eval_value_retain_raw_heap_word");
    emitter.instruction("sub sp, sp, #32");                                     // reserve a wrapper frame while retaining the raw heap word
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across incref
    emitter.instruction("add x29, sp, #16");                                    // establish a stable frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the raw heap word for the C return value
    emitter.instruction("bl __rt_incref");                                      // retain the heap payload for the staged by-ref slot
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the original raw heap word to Rust
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the retain wrapper frame
    emitter.instruction("ret");                                                 // return the retained raw heap word

    label_c_global(emitter, "__elephc_eval_value_from_raw_word");
    emitter.instruction("mov x2, xzr");                                         // raw one-word scalar payloads have no high payload word
    emitter.instruction("b __rt_mixed_from_value");                             // box the raw scalar payload and return it to Rust

    label_c_global(emitter, "__elephc_eval_value_from_raw_heap_word");
    emitter.instruction("cbz x0, __elephc_eval_value_from_raw_heap_word_miss"); // null raw heap words cannot be boxed
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the uniform heap kind word for tag recovery
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low-byte heap kind tag
    emitter.instruction("cmp x9, #2");                                          // heap kind 2 stores indexed-array payloads
    emitter.instruction("b.eq __elephc_eval_value_from_raw_heap_word_array");   // box indexed arrays with runtime tag 4
    emitter.instruction("cmp x9, #3");                                          // heap kind 3 stores associative hash payloads
    emitter.instruction("b.eq __elephc_eval_value_from_raw_heap_word_hash");    // box hashes with runtime tag 5
    emitter.instruction("cmp x9, #4");                                          // heap kind 4 stores object payloads
    emitter.instruction("b.eq __elephc_eval_value_from_raw_heap_word_object");  // box objects with runtime tag 6
    emitter.instruction("cmp x9, #5");                                          // heap kind 5 stores boxed Mixed payloads
    emitter.instruction("b.eq __elephc_eval_value_from_raw_heap_word_mixed");   // box nested Mixed cells with runtime tag 7
    emitter.label("__elephc_eval_value_from_raw_heap_word_miss");
    emitter.instruction("mov x0, xzr");                                         // report malformed raw heap words as a null Rust handle
    emitter.instruction("ret");                                                 // return the failed boxing sentinel
    emitter.label("__elephc_eval_value_from_raw_heap_word_array");
    emitter.instruction("mov x1, x0");                                          // move the indexed-array payload into the boxing low word
    emitter.instruction("mov x0, #4");                                          // runtime tag 4 = indexed array
    emitter.instruction("b __elephc_eval_value_from_raw_heap_word_box");        // share the one-word heap boxing tail
    emitter.label("__elephc_eval_value_from_raw_heap_word_hash");
    emitter.instruction("mov x1, x0");                                          // move the hash payload into the boxing low word
    emitter.instruction("mov x0, #5");                                          // runtime tag 5 = associative array
    emitter.instruction("b __elephc_eval_value_from_raw_heap_word_box");        // share the one-word heap boxing tail
    emitter.label("__elephc_eval_value_from_raw_heap_word_object");
    emitter.instruction("mov x1, x0");                                          // move the object payload into the boxing low word
    emitter.instruction("mov x0, #6");                                          // runtime tag 6 = object
    emitter.instruction("b __elephc_eval_value_from_raw_heap_word_box");        // share the one-word heap boxing tail
    emitter.label("__elephc_eval_value_from_raw_heap_word_mixed");
    emitter.instruction("mov x1, x0");                                          // move the boxed Mixed payload into the boxing low word
    emitter.instruction("mov x0, #7");                                          // runtime tag 7 = Mixed
    emitter.label("__elephc_eval_value_from_raw_heap_word_box");
    emitter.instruction("mov x2, xzr");                                         // one-word heap payloads do not use a high word
    emitter.instruction("b __rt_mixed_from_value");                             // retain and box the raw heap payload for eval

    label_c_global(emitter, "__elephc_eval_value_release_raw_heap_word");
    emitter.instruction("b __rt_decref_any");                                   // release the staged raw heap slot owner

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

    // `__elephc_eval_value_object_handle` is the PHP OBJECT HANDLE, not the address:
    // the magician's `spl_object_id` / `spl_object_hash` must agree with the AOT
    // engine and with `var_dump`'s `#N`, and all three read the same pool. The
    // address-shaped `object_identity` above stays as it is because destructor
    // bookkeeping keys on the storage, not on the PHP-visible handle.
    label_c_global(emitter, "__elephc_eval_value_object_handle");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame while unboxing the object cell
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across nested calls
    emitter.instruction("mov x29, sp");                                         // establish a stable object-handle wrapper frame
    emitter.instruction("bl __rt_mixed_unbox");                                 // unwrap nested Mixed cells to tag and object payload
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means PHP object
    emitter.instruction("b.ne __elephc_eval_value_object_handle_zero");         // non-object values carry no PHP handle
    emitter.instruction("mov x0, x1");                                          // pass the unboxed object payload to the handle pool
    emitter.instruction("bl __rt_object_handle_of");                            // x0 = this object's PHP handle
    emitter.instruction("b __elephc_eval_value_object_handle_done");            // return the resolved handle
    emitter.label("__elephc_eval_value_object_handle_zero");
    emitter.instruction("mov x0, #0");                                          // report "no handle" for non-object values
    emitter.label("__elephc_eval_value_object_handle_done");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the object-handle wrapper frame
    emitter.instruction("ret");                                                 // return the PHP object handle to Rust

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
    emitter.instruction("cmp x0, #9");                                          // is the eval value a resource?
    emitter.instruction("b.eq __elephc_eval_value_cast_string_resource");       // resources render as PHP's "Resource id #N"
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
    emitter.label("__elephc_eval_value_cast_string_resource");
    emitter.instruction("mov x0, x1");                                          // pass the native resource payload to the display formatter
    emitter.instruction("bl __rt_resource_to_string");                          // format "Resource id #N" into the shared concat scratch (x1/x2)
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string
    emitter.instruction("bl __rt_mixed_from_value");                            // persist and box the borrowed resource display string
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
    emitter.instruction("sub sp, sp, #32");                                     // reserve a payload slot and a saved frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across the context call
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // preserve the eval payload across the context call
    emitter.instruction("bl __rt_stream_default_context_ensure");               // PHP mints id 4 for the request default BEFORE the first stream
    emitter.instruction("ldr x1, [sp, #0]");                                    // move the C resource id into the mixed payload slot
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the wrapper frame before the tail branch
    emitter.instruction("mov x0, #9");                                          // runtime tag 9 = resource
    emitter.instruction("mov x2, xzr");                                         // resource payloads do not use a high word
    emitter.instruction("b __rt_mixed_from_value");                             // box the resource payload and return to Rust

    label_c_global(emitter, "__elephc_eval_value_hash_context");
    emitter.instruction("mov x1, x0");                                          // move the eval hash-context table key into the mixed payload slot
    emitter.instruction("mov x0, #9");                                          // runtime tag 9 = resource, the shape eval's hash builtins read back
    emitter.instruction("mov x2, #5");                                          // resource kind 5 = eval-owned inert handle: no PHP id, no destructor
    emitter.instruction("b __rt_mixed_from_value");                             // box the inert hash-context payload and return to Rust

    label_c_global(emitter, "__elephc_eval_resource_is_closed");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a wrapper frame across the type-name call
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address across the call
    emitter.instruction("mov x29, sp");                                         // establish a stable wrapper frame pointer
    emitter.instruction("bl __rt_resource_type_name");                          // ask the resource registry which name this payload reports
    abi::emit_symbol_address(emitter, "x9", "_resource_type_unknown");
    emitter.instruction("cmp x1, x9");                                          // a closed handle is exactly the one that reports the Unknown literal
    emitter.instruction("cset x0, eq");                                         // report closed as 1 and open as 0
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the wrapper frame
    emitter.instruction("ret");                                                 // return the closed predicate to Rust

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

}
