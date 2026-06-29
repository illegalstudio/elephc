//! Purpose:
//! Emits the `__rt_array_set_mixed_key` runtime helper for writes into a
//! statically `Array(Mixed)` indexed local whose key is a boxed `Mixed` cell
//! (notably PHP `foreach` loop keys, which are always `Mixed` in EIR).
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - The key tag is only known at runtime, so the helper tag-dispatches: an
//!   integer/bool/float key keeps the destination on indexed storage (preserving
//!   indexed-only consumers like `implode`), while a string key promotes the
//!   destination to associative hash storage. An already-hash destination
//!   (kind 3, e.g. after an earlier string key in the same rebuild) always goes
//!   through the hash path regardless of key tag, matching PHP array semantics.
//! - The string-key promote path copies the existing indexed entries into a new
//!   Mixed-typed hash via `__rt_array_hash_union`, then runs `__rt_hash_to_mixed`
//!   so any scalar (non-Mixed) slots copied from the indexed source are boxed as
//!   Mixed cells before the new entry is inserted. Without that step a prior
//!   integer push (e.g. `$dst[] = 9`) would read back empty through the Mixed
//!   foreach value loader, since the union leaves scalar slots raw.
//! - The value is a boxed `Mixed` pointer consumed by the write (stored directly
//!   into the slot for the indexed path, or stored as a `Mixed`-tagged hash
//!   payload for the hash path), mirroring `__rt_array_set_mixed` ownership.
//! - The helper does not release the incoming array pointer; the caller owns the
//!   old local release so the promoted hash can replace it cleanly.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the boxed-Mixed-key indexed/hash array set helper for the current target.
pub fn emit_array_set_mixed_key(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_set_mixed_key_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_set_mixed_key ---");
    emitter.label_global("__rt_array_set_mixed_key");

    emitter.instruction("sub sp, sp, #96");                                     // reserve frame for array, key, value, promoted key, temp/merged hash
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish a helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the incoming indexed-array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the boxed Mixed key cell
    emitter.instruction("str x2, [sp, #16]");                                   // save the consumed boxed Mixed value

    // -- materialize an empty indexed array for null/uninitialized destinations --
    emitter.instruction("cbz x0, __rt_array_set_mixed_key_alloc_empty");        // a null destination (e.g. `$dst = []`) needs a real array before writes
    emitter.instruction("b __rt_array_set_mixed_key_kind_dispatch");            // skip allocation for already-allocated destinations
    emitter.label("__rt_array_set_mixed_key_alloc_empty");
    emitter.instruction("mov x0, #0");                                          // request zero initial capacity for the empty indexed array
    emitter.instruction("mov x1, #8");                                          // Mixed-element slots are one pointer wide
    emitter.instruction("bl __rt_array_new");                                   // allocate an empty indexed array to write into
    emitter.instruction("str x0, [sp, #0]");                                    // replace the null destination with the new array
    emitter.label("__rt_array_set_mixed_key_kind_dispatch");

    // -- dispatch on the destination runtime kind --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the (possibly just-allocated) destination pointer
    emitter.instruction("bl __rt_heap_kind");                                   // load the destination heap kind byte into x0
    emitter.instruction("cmp x0, #3");                                          // kind 3 marks already-promoted associative hash storage
    emitter.instruction("b.eq __rt_array_set_mixed_key_hash_path");             // route already-hash destinations through the hash writer

    // -- indexed destination: dispatch on the key tag --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed Mixed key cell
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel the key cell to tag in x0 and payload in x1/x2
    emitter.instruction("cmp x0, #1");                                          // string mixed keys need hash storage
    emitter.instruction("b.eq __rt_array_set_mixed_key_string_promote");        // promote the indexed array to a hash for string keys
    emitter.instruction("cmp x0, #2");                                          // float mixed keys are cast to integer keys like PHP
    emitter.instruction("b.ne __rt_array_set_mixed_key_int_ready");             // integer/bool keys are already valid indexed indexes
    emitter.instruction("fmov d0, x1");                                         // load the float key payload into the FP register
    emitter.instruction("fcvtzs x1, d0");                                       // cast the float key to an integer index like PHP
    emitter.label("__rt_array_set_mixed_key_int_ready");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the indexed-array pointer
    emitter.instruction("cmp x1, #0");                                          // negative int keys cannot live in packed indexed storage
    emitter.instruction("b.lt __rt_array_set_mixed_key_int_promote");           // promote to a hash so a negative key survives like PHP
    emitter.instruction("ldr x9, [x0]");                                        // load the current logical length of the indexed array
    emitter.instruction("cmp x1, x9");                                          // a key past the end would create a sparse gap
    emitter.instruction("b.hi __rt_array_set_mixed_key_int_promote");           // promote to a hash so a sparse key survives like PHP
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the consumed boxed Mixed value
    emitter.instruction("bl __rt_array_set_mixed");                             // store the value into packed indexed storage and return the array
    emitter.instruction("b __rt_array_set_mixed_key_done");                     // finish after an indexed write

    // -- indexed destination + out-of-range int key: promote to hash then set --
    emitter.label("__rt_array_set_mixed_key_int_promote");
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks a scalar integer hash key
    emitter.instruction("str x1, [sp, #24]");                                   // save the integer key low word across helper calls
    emitter.instruction("str x2, [sp, #32]");                                   // save the integer key high-word sentinel
    emitter.instruction("b __rt_array_set_mixed_key_promote_alloc");            // share the indexed-to-hash promotion path

    // -- indexed destination + string key: promote to hash then set --
    emitter.label("__rt_array_set_mixed_key_string_promote");
    emitter.instruction("bl __rt_hash_normalize_key");                          // normalize the string key payload (x1/x2) into a hash key pair
    emitter.instruction("str x1, [sp, #24]");                                   // save the normalized key low word across helper calls
    emitter.instruction("str x2, [sp, #32]");                                   // save the normalized key high word across helper calls
    emitter.label("__rt_array_set_mixed_key_promote_alloc");
    emitter.instruction("mov x0, #16");                                         // initial hash capacity for the promoted Mixed-typed hash
    emitter.instruction("mov x1, #7");                                          // runtime value_type 7 = boxed Mixed slots
    emitter.instruction("bl __rt_hash_new");                                    // allocate an empty temporary hash
    emitter.instruction("str x0, [sp, #40]");                                   // save the temporary hash pointer
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the indexed-array pointer as the union left operand
    emitter.instruction("ldr x1, [sp, #40]");                                   // pass the temporary hash as the union right operand
    emitter.instruction("bl __rt_array_hash_union");                            // copy the indexed entries into a new Mixed-typed hash
    emitter.instruction("str x0, [sp, #48]");                                   // save the promoted merged hash pointer
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the temporary hash for release
    emitter.instruction("bl __rt_decref_hash");                                 // release the empty temporary hash after the union copy
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the promoted merged hash for Mixed-box conversion
    emitter.instruction("bl __rt_hash_to_mixed");                               // box union-copied scalar slots as Mixed cells so foreach readback is correct
    emitter.instruction("str x0, [sp, #48]");                                   // save the Mixed-boxed promoted hash pointer (ensure_unique may reallocate)
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the promoted merged hash as the set target
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the normalized key low word
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the normalized key high word
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the consumed boxed Mixed value
    emitter.instruction("mov x4, #0");                                          // boxed Mixed hash payloads leave the high value word empty
    emitter.instruction("mov x5, #7");                                          // value_type 7 marks the slot as a boxed Mixed pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert the entry into the promoted hash and return it
    emitter.instruction("b __rt_array_set_mixed_key_done");                     // finish after a promoted hash write

    // -- already-hash destination: materialize the key and set directly --
    emitter.label("__rt_array_set_mixed_key_hash_path");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed Mixed key cell
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel the key cell to tag in x0 and payload in x1/x2
    emitter.instruction("cmp x0, #1");                                          // string mixed keys need normalization
    emitter.instruction("b.eq __rt_array_set_mixed_key_hash_string");           // route string keys through the hash-key normalizer
    emitter.instruction("cmp x0, #2");                                          // float mixed keys are cast to integer keys like PHP
    emitter.instruction("b.ne __rt_array_set_mixed_key_hash_int");              // integer/bool keys become scalar integer hash keys
    emitter.instruction("fmov d0, x1");                                         // load the float key payload into the FP register
    emitter.instruction("fcvtzs x1, d0");                                       // cast the float key to an integer hash key like PHP
    emitter.label("__rt_array_set_mixed_key_hash_int");
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks scalar integer hash keys
    emitter.instruction("b __rt_array_set_mixed_key_hash_set");                 // proceed to the hash insert with an integer key
    emitter.label("__rt_array_set_mixed_key_hash_string");
    emitter.instruction("bl __rt_hash_normalize_key");                          // normalize the string key payload (x1/x2) into a hash key pair
    emitter.label("__rt_array_set_mixed_key_hash_set");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash pointer as the set target
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the consumed boxed Mixed value
    emitter.instruction("mov x4, #0");                                          // boxed Mixed hash payloads leave the high value word empty
    emitter.instruction("mov x5, #7");                                          // value_type 7 marks the slot as a boxed Mixed pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert the entry into the hash and return it

    emitter.label("__rt_array_set_mixed_key_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the final array/hash pointer in x0
}

/// Emits the Linux x86_64 boxed-Mixed-key indexed/hash array set helper.
fn emit_array_set_mixed_key_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_set_mixed_key ---");
    emitter.label_global("__rt_array_set_mixed_key");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 64");                                         // reserve 16-aligned slots for inputs, promoted key, and temp/merged hash
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the incoming indexed-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the boxed Mixed key cell
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the consumed boxed Mixed value

    // -- materialize an empty indexed array for null/uninitialized destinations --
    emitter.instruction("test rdi, rdi");                                       // a null destination (e.g. `$dst = []`) needs a real array before writes
    emitter.instruction("je __rt_array_set_mixed_key_alloc_empty");             // allocate an empty indexed array for null destinations
    emitter.instruction("jmp __rt_array_set_mixed_key_kind_dispatch");          // skip allocation for already-allocated destinations
    emitter.label("__rt_array_set_mixed_key_alloc_empty");
    emitter.instruction("mov rdi, 0");                                          // request zero initial capacity for the empty indexed array
    emitter.instruction("mov rsi, 8");                                          // Mixed-element slots are one pointer wide
    emitter.instruction("call __rt_array_new");                                 // allocate an empty indexed array to write into
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // replace the null destination with the new array
    emitter.label("__rt_array_set_mixed_key_kind_dispatch");

    // -- dispatch on the destination runtime kind --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the (possibly just-allocated) destination pointer
    emitter.instruction("call __rt_heap_kind");                                 // load the destination heap kind byte into rax
    emitter.instruction("cmp rax, 3");                                          // kind 3 marks already-promoted associative hash storage
    emitter.instruction("je __rt_array_set_mixed_key_hash_path");               // route already-hash destinations through the hash writer

    // -- indexed destination: dispatch on the key tag --
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed Mixed key cell
    emitter.instruction("call __rt_mixed_unbox");                               // peel the key cell to tag in rax and payload in rdi/rdx
    emitter.instruction("cmp rax, 1");                                          // string mixed keys need hash storage
    emitter.instruction("je __rt_array_set_mixed_key_string_promote");          // promote the indexed array to a hash for string keys
    emitter.instruction("cmp rax, 2");                                          // float mixed keys are cast to integer keys like PHP
    emitter.instruction("jne __rt_array_set_mixed_key_int_ready");              // integer/bool keys are already valid indexed indexes
    emitter.instruction("movq xmm0, rdi");                                      // load the float key payload into the FP register
    emitter.instruction("cvttsd2si rdi, xmm0");                                 // cast the float key to an integer index like PHP
    emitter.label("__rt_array_set_mixed_key_int_ready");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the indexed-array pointer
    emitter.instruction("cmp rdi, 0");                                          // negative int keys cannot live in packed indexed storage
    emitter.instruction("jl __rt_array_set_mixed_key_int_promote");             // promote to a hash so a negative key survives like PHP
    emitter.instruction("mov rcx, QWORD PTR [rax]");                            // load the current logical length of the indexed array
    emitter.instruction("cmp rdi, rcx");                                        // a key past the end would create a sparse gap
    emitter.instruction("ja __rt_array_set_mixed_key_int_promote");             // promote to a hash so a sparse key survives like PHP
    emitter.instruction("mov rsi, rdi");                                        // publish the integer key as the indexed-set index argument
    emitter.instruction("mov rdi, rax");                                        // reload the indexed-array pointer into the set argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the consumed boxed Mixed value
    emitter.instruction("call __rt_array_set_mixed");                           // store the value into packed indexed storage and return the array
    emitter.instruction("jmp __rt_array_set_mixed_key_done");                   // finish after an indexed write

    // -- indexed destination + out-of-range int key: promote to hash then set --
    emitter.label("__rt_array_set_mixed_key_int_promote");
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the integer key low word across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], -1");                        // key_hi sentinel marks a scalar integer hash key
    emitter.instruction("jmp __rt_array_set_mixed_key_promote_alloc");          // share the indexed-to-hash promotion path

    // -- indexed destination + string key: promote to hash then set --
    emitter.label("__rt_array_set_mixed_key_string_promote");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed string pointer into the normalizer input
    emitter.instruction("call __rt_hash_normalize_key");                        // normalize the string key into a hash key pair in rax/rdx
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the normalized key low word across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the normalized key high word across helper calls
    emitter.label("__rt_array_set_mixed_key_promote_alloc");
    emitter.instruction("mov rdi, 16");                                         // initial hash capacity for the promoted Mixed-typed hash
    emitter.instruction("mov rsi, 7");                                          // runtime value_type 7 = boxed Mixed slots
    emitter.instruction("call __rt_hash_new");                                  // allocate an empty temporary hash
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the temporary hash pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the indexed-array pointer as the union left operand
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // pass the temporary hash as the union right operand
    emitter.instruction("call __rt_array_hash_union");                          // copy the indexed entries into a new Mixed-typed hash
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the promoted merged hash pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the temporary hash for release
    emitter.instruction("call __rt_decref_hash");                               // release the empty temporary hash after the union copy
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the promoted merged hash for Mixed-box conversion
    emitter.instruction("call __rt_hash_to_mixed");                             // box union-copied scalar slots as Mixed cells so foreach readback is correct
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the Mixed-boxed promoted hash pointer (ensure_unique may reallocate)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the promoted merged hash as the set target
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the normalized key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the normalized key high word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the consumed boxed Mixed value
    emitter.instruction("xor r8, r8");                                          // boxed Mixed hash payloads leave the high value word empty
    emitter.instruction("mov r9, 7");                                           // value_type 7 marks the slot as a boxed Mixed pointer
    emitter.instruction("call __rt_hash_set");                                  // insert the entry into the promoted hash and return it
    emitter.instruction("jmp __rt_array_set_mixed_key_done");                   // finish after a promoted hash write

    // -- already-hash destination: materialize the key and set directly --
    emitter.label("__rt_array_set_mixed_key_hash_path");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed Mixed key cell
    emitter.instruction("call __rt_mixed_unbox");                               // peel the key cell to tag in rax and payload in rdi/rdx
    emitter.instruction("cmp rax, 1");                                          // string mixed keys need normalization
    emitter.instruction("je __rt_array_set_mixed_key_hash_string");             // route string keys through the hash-key normalizer
    emitter.instruction("cmp rax, 2");                                          // float mixed keys are cast to integer keys like PHP
    emitter.instruction("jne __rt_array_set_mixed_key_hash_int");               // integer/bool keys become scalar integer hash keys
    emitter.instruction("movq xmm0, rdi");                                      // load the float key payload into the FP register
    emitter.instruction("cvttsd2si rdi, xmm0");                                 // cast the float key to an integer hash key like PHP
    emitter.label("__rt_array_set_mixed_key_hash_int");
    emitter.instruction("mov rsi, rdi");                                        // publish the integer key payload as the hash key low word
    emitter.instruction("mov rdx, -1");                                         // key_hi sentinel marks scalar integer hash keys
    emitter.instruction("jmp __rt_array_set_mixed_key_hash_set");               // proceed to the hash insert with an integer key
    emitter.label("__rt_array_set_mixed_key_hash_string");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed string pointer into the normalizer input
    emitter.instruction("call __rt_hash_normalize_key");                        // normalize the string key into a hash key pair in rax/rdx
    emitter.instruction("mov rsi, rax");                                        // publish the normalized key low word as the hash key low word
    emitter.label("__rt_array_set_mixed_key_hash_set");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash pointer as the set target
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the consumed boxed Mixed value
    emitter.instruction("xor r8, r8");                                          // boxed Mixed hash payloads leave the high value word empty
    emitter.instruction("mov r9, 7");                                           // value_type 7 marks the slot as a boxed Mixed pointer
    emitter.instruction("call __rt_hash_set");                                  // insert the entry into the hash and return it

    emitter.label("__rt_array_set_mixed_key_done");
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the final array/hash pointer in rax
}