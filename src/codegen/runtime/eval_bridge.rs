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

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

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
