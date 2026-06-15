//! Purpose:
//! Emits C-ABI wrappers used by the optional `elephc-eval` bridge crate.
//! Adapts Rust staticlib calls to elephc's internal runtime value helper ABI.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` when `RuntimeFeatures.eval` is enabled.
//!
//! Key details:
//! - Exported wrapper labels use platform C-symbol mangling because they are
//!   referenced from Rust object files, while internal `__rt_*` calls keep the
//!   existing assembly ABI.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

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

    label_c_global(emitter, "__elephc_eval_value_int");
    emitter.instruction("mov x1, x0");                                          // move the C integer argument into the mixed payload slot
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer
    emitter.instruction("mov x2, xzr");                                         // integer payloads do not use a high word
    emitter.instruction("b __rt_mixed_from_value");                             // box the integer payload and return to Rust

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
    emitter.instruction(&format!(
        "mov r10, 0x{:x}",
        (X86_64_HEAP_MAGIC_HI32 << 32) | 5
    )); // materialize the mixed-cell heap kind with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // install the mixed-cell heap kind in the uniform header
    emitter.instruction("mov QWORD PTR [rax], 4");                              // runtime tag 4 = indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the owned indexed-array pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the array pointer as the Mixed low payload word
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // indexed arrays do not use the high payload word
    emitter.instruction("add rsp, 16");                                         // release the array-new wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed array Mixed cell to Rust

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
    emitter.instruction(&format!(
        "mov r10, 0x{:x}",
        (X86_64_HEAP_MAGIC_HI32 << 32) | 5
    )); // materialize the mixed-cell heap kind with the x86_64 heap marker
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

    label_c_global(emitter, "__elephc_eval_value_int");
    emitter.instruction("mov eax, 0");                                          // runtime tag 0 = integer
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a high word
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the C integer payload in rdi and return

    label_c_global(emitter, "__elephc_eval_value_float");
    emitter.instruction("movq rdi, xmm0");                                      // move the C double bits into mixed value_lo
    emitter.instruction("mov eax, 2");                                          // runtime tag 2 = double
    emitter.instruction("xor esi, esi");                                        // double payloads do not use a high word
    emitter.instruction("jmp __rt_mixed_from_value");                           // box the double payload and return to Rust

    label_c_global(emitter, "__elephc_eval_value_string");
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string, with C ptr/len already in rdi/rsi
    emitter.instruction("jmp __rt_mixed_from_value");                           // persist and box the string payload for eval

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

    label_c_global(emitter, "__elephc_eval_value_release");
    emitter.instruction("mov rax, rdi");                                        // move the C boxed Mixed argument into the internal release register
    emitter.instruction("jmp __rt_decref_mixed");                               // release one eval-owned boxed Mixed cell
}

/// Emits a global label with platform C-symbol mangling.
fn label_c_global(emitter: &mut Emitter, name: &str) {
    let symbol = emitter.target.extern_symbol(name);
    emitter.label_global(&symbol);
}
