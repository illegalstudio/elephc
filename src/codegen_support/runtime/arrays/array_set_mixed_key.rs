//! Purpose:
//! Emits the `__rt_array_set_mixed_key` runtime helper for writes into a
//! statically `Array(Mixed)` indexed local whose key is a boxed `Mixed` cell
//! (notably PHP `foreach` loop keys, which are always `Mixed` in EIR).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
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
//! - OWNERSHIP: the caller passes one OWNED reference to the incoming array,
//!   mirroring `Op::ArrayToHash`. A consuming raw-local store transfers its existing
//!   slot owner; a ref-cell, static, or global store acquires a separate helper owner
//!   because its later storeback retires the owner currently in storage.
//!   The in-place paths hand that `+1` back inside the returned pointer; the promote
//!   paths abandon the source for a freshly built hash and RELEASE it, because
//!   `__rt_array_hash_union` only borrows its operands. The helper therefore always
//!   returns a `+1` the destination owns. Acquiring a second reference for a raw
//!   local would leak the abandoned array, while omitting it for a retiring storeback
//!   would release the same source owner twice.
//! - The alternate entry point skips a float-key diagnostic after the read half
//!   of a compound update has already reported it.
//! - Nullable-integer invoker markers validate and write through before any promotion.
//!   Other markers promote to hash storage for the shared ownership-safe setter.
//! - Owned-refusal entries retire the separate destination owner of a ref-cell/static/global
//!   storeback on shape-12 errors. Mode bit zero still controls only float-key diagnostics.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the boxed-Mixed-key indexed/hash array set helper for the current target.
pub fn emit_array_set_mixed_key(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_set_mixed_key_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_set_mixed_key ---");
    emitter.label_global("__rt_array_set_mixed_key");
    abi::emit_load_int_immediate(emitter, "x3", 0);
    emitter.instruction("b __rt_array_set_mixed_key_entry");                    // enter with float-key diagnostics enabled
    emitter.label_global("__rt_array_set_mixed_key_already_diagnosed");
    abi::emit_load_int_immediate(emitter, "x3", 1);
    emitter.instruction("b __rt_array_set_mixed_key_entry");                    // retain the original raw-local owner when a nullable write refuses
    emitter.label_global("__rt_array_set_mixed_key_owned_refusal");
    abi::emit_load_int_immediate(emitter, "x3", 2);
    emitter.instruction("b __rt_array_set_mixed_key_entry");                    // identify the separate destination owner acquired for a retiring storeback
    emitter.label_global("__rt_array_set_mixed_key_owned_refusal_already_diagnosed");
    abi::emit_load_int_immediate(emitter, "x3", 3);
    emitter.label_shared("__rt_array_set_mixed_key_entry");

    emitter.instruction("sub sp, sp, #96");                                     // reserve frame for array, key, value, promoted key, temp/merged hash
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish a helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the incoming indexed-array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the boxed Mixed key cell
    emitter.instruction("str x2, [sp, #16]");                                   // save the consumed boxed Mixed value
    emitter.instruction("str x3, [sp, #56]");                                   // remember whether this source operation already diagnosed its float key

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
    emitter.instruction("cmp x0, #8");                                          // null mixed keys normalize to the empty string like PHP
    emitter.instruction("b.eq __rt_array_set_mixed_key_null_promote");          // promote the indexed array to a hash with an empty-string key
    emitter.instruction("cmp x0, #2");                                          // float mixed keys are cast to integer keys like PHP
    emitter.instruction("b.ne __rt_array_set_mixed_key_int_ready");             // integer/bool keys are already valid indexed indexes
    emitter.instruction("fmov d0, x1");                                         // load the float key payload into the FP register
    emitter.instruction("ldr x9, [sp, #56]");                                   // check whether the read half already diagnosed this key
    emitter.instruction("and x9, x9, #1");                                      // ownership mode must not suppress a required float-key warning
    emitter.instruction("cbnz x9, __rt_array_set_mixed_key_int_float_silent");  // use the silent PHP cast for a diagnosed compound key
    abi::emit_call_label(emitter, "__rt_float_key_to_int");
    emitter.instruction("b __rt_array_set_mixed_key_int_float_done");           // keep the diagnosed integer result
    emitter.label("__rt_array_set_mixed_key_int_float_silent");
    abi::emit_call_label(emitter, "__rt_php_float_to_int");
    emitter.instruction("mov x0, x9");                                          // recover the silent PHP integer conversion result
    emitter.label("__rt_array_set_mixed_key_int_float_done");
    emitter.instruction("mov x1, x0");                                          // use the diagnosed PHP integer as the index
    emitter.label("__rt_array_set_mixed_key_int_ready");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the indexed-array pointer
    emitter.instruction("cmp x1, #0");                                          // negative int keys cannot live in packed indexed storage
    emitter.instruction("b.lt __rt_array_set_mixed_key_int_promote");           // promote to a hash so a negative key survives like PHP
    emitter.instruction("ldr x9, [x0]");                                        // load the current logical length of the indexed array
    emitter.instruction("cmp x1, x9");                                          // a key past the end would create a sparse gap
    emitter.instruction("b.hi __rt_array_set_mixed_key_int_promote");           // promote to a hash so a sparse key survives like PHP
    emitter.instruction("b.eq __rt_array_set_mixed_key_int_plain");             // a contiguous append has no existing reference slot
    emitter.instruction("ldr x10, [x0, #-8]");                                  // inspect the existing slot representation
    emitter.instruction("ubfx x10, x10, #8, #7");                               // isolate the indexed element tag
    emitter.instruction("cmp x10, #7");                                         // only boxed slots can hold invoker markers
    emitter.instruction("b.ne __rt_array_set_mixed_key_int_plain");             // typed slots use ordinary widening and replacement
    emitter.instruction("add x10, x0, #24");                                    // locate the packed data area
    emitter.instruction("ldr x10, [x10, x1, lsl #3]");                          // inspect the overwritten boxed slot
    emitter.instruction("cbz x10, __rt_array_set_mixed_key_int_plain");         // absent slots have no write-through identity
    emitter.instruction("ldr x11, [x10]");                                      // preserve the marker pointer while inspecting its inner tag
    emitter.instruction("cmp x11, #11");                                        // invoker markers retain the caller storage address
    emitter.instruction("b.ne __rt_array_set_mixed_key_int_plain");             // ordinary values keep the indexed replacement path
    emit_tagged_marker_write_or_promote(emitter);
    emitter.label("__rt_array_set_mixed_key_int_plain");

    // -- widen typed slots to boxed Mixed BEFORE dropping a Mixed cell into one --
    // `__rt_array_set_mixed` re-stamps the destination's value_type to 7 (boxed Mixed) but never
    // converts the slots that are already there. Writing into a still-typed array (`[1, 2, 3]`)
    // therefore left slots 1 and 2 holding raw integers while the header claimed they were Mixed
    // cell pointers — reading either one dereferenced the integer and segfaulted. Only element 0,
    // the one just written, survived. Widen first, exactly as the Mixed-append lowering does.
    emitter.instruction("str x1, [sp, #24]");                                   // save the target index across the widening call
    emitter.instruction("ldr x1, [x0, #-8]");                                   // load the destination's packed metadata
    emitter.instruction("lsr x1, x1, #8");                                      // move the runtime value_type tag into the low bits
    emitter.instruction("and x1, x1, #0x7f");                                   // isolate the destination's value_type tag
    emitter.instruction("cmp x1, #7");                                          // tag 7 = the slots already hold boxed Mixed cells
    emitter.instruction("b.eq __rt_array_set_mixed_key_int_boxed");             // nothing to widen for an already-Mixed destination
    emitter.instruction("bl __rt_array_to_mixed");                              // box every existing typed slot and stamp the array Mixed
    emitter.instruction("str x0, [sp, #0]");                                    // republish the possibly copy-on-write-split array pointer
    emitter.label("__rt_array_set_mixed_key_int_boxed");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the widened indexed-array pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the target index
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the consumed boxed Mixed value
    emitter.instruction("bl __rt_array_set_mixed");                             // store the value into packed indexed storage and return the array
    emitter.instruction("b __rt_array_set_mixed_key_done");                     // finish after an indexed write

    // -- indexed destination + out-of-range int key: promote to hash then set --
    emitter.label("__rt_array_set_mixed_key_int_promote");
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks a scalar integer hash key
    emitter.instruction("str x1, [sp, #24]");                                   // save the integer key low word across helper calls
    emitter.instruction("str x2, [sp, #32]");                                   // save the integer key high-word sentinel
    emitter.instruction("b __rt_array_set_mixed_key_promote_alloc");            // share the indexed-to-hash promotion path

    // -- indexed destination + null key: promote to hash with the empty-string key like PHP --
    emitter.label("__rt_array_set_mixed_key_null_promote");
    abi::emit_symbol_address(emitter, "x1", "_empty_str");                     // null normalizes to the empty string "" key pointer
    emitter.instruction("mov x2, #0");                                          // the empty-string key has zero length (string-key marker)
    emitter.instruction("str x1, [sp, #24]");                                   // save the empty-string key low word across helper calls
    emitter.instruction("str x2, [sp, #32]");                                   // save the string-key high word (0 = string key)
    emitter.instruction("b __rt_array_set_mixed_key_promote_alloc");            // share the indexed-to-hash promotion path

    // -- indexed destination + string key: promote to hash then set --
    emitter.label("__rt_array_set_mixed_key_string_promote");
    emitter.instruction("bl __rt_hash_normalize_key");                          // normalize the string key payload (x1/x2) into a hash key pair
    emitter.instruction("cmn x2, #1");                                          // a numeric string names the same existing indexed slot as its integer key
    emitter.instruction("b.eq __rt_array_set_mixed_key_int_ready");             // validate nullable markers before promotion can consume the indexed owner
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
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source indexed array this promotion abandons
    emitter.instruction("bl __rt_decref_array");                                // release the reference the lowering acquired: hash_union only BORROWED it
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the promoted merged hash for Mixed-box conversion
    emitter.instruction("bl __rt_hash_to_mixed");                               // box union-copied scalar slots as Mixed cells so foreach readback is correct
    emitter.instruction("str x0, [sp, #48]");                                   // save the Mixed-boxed promoted hash pointer (ensure_unique may reallocate)
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the promoted merged hash as the set target
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the normalized key low word
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the normalized key high word
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the consumed boxed Mixed value
    emitter.instruction("mov x4, #0");                                          // boxed Mixed hash payloads leave the high value word empty
    emitter.instruction("mov x5, #7");                                          // value_type 7 marks the slot as a boxed Mixed pointer
    emit_hash_set_with_refusal_owner(emitter, true);
    emitter.instruction("b __rt_array_set_mixed_key_done");                     // finish after a promoted hash write

    // -- already-hash destination: materialize the key and set directly --
    emitter.label("__rt_array_set_mixed_key_hash_path");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed Mixed key cell
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel the key cell to tag in x0 and payload in x1/x2
    emitter.instruction("cmp x0, #1");                                          // string mixed keys need normalization
    emitter.instruction("b.eq __rt_array_set_mixed_key_hash_string");           // route string keys through the hash-key normalizer
    emitter.instruction("cmp x0, #8");                                          // null mixed keys normalize to the empty string like PHP
    emitter.instruction("b.eq __rt_array_set_mixed_key_hash_null");             // route null keys to the empty-string hash key path
    emitter.instruction("cmp x0, #2");                                          // float mixed keys are cast to integer keys like PHP
    emitter.instruction("b.ne __rt_array_set_mixed_key_hash_int");              // integer/bool keys become scalar integer hash keys
    emitter.instruction("fmov d0, x1");                                         // load the float key payload into the FP register
    emitter.instruction("ldr x9, [sp, #56]");                                   // check whether the read half already diagnosed this key
    emitter.instruction("and x9, x9, #1");                                      // inspect only the already-diagnosed flag
    emitter.instruction("cbnz x9, __rt_array_set_mixed_key_hash_float_silent"); // use the silent PHP cast for a diagnosed compound key
    abi::emit_call_label(emitter, "__rt_float_key_to_int");
    emitter.instruction("b __rt_array_set_mixed_key_hash_float_done");          // keep the diagnosed integer result
    emitter.label("__rt_array_set_mixed_key_hash_float_silent");
    abi::emit_call_label(emitter, "__rt_php_float_to_int");
    emitter.instruction("mov x0, x9");                                          // recover the silent PHP integer conversion result
    emitter.label("__rt_array_set_mixed_key_hash_float_done");
    emitter.instruction("mov x1, x0");                                          // use the diagnosed PHP integer as the hash key
    emitter.label("__rt_array_set_mixed_key_hash_int");
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks scalar integer hash keys
    emitter.instruction("b __rt_array_set_mixed_key_hash_set");                 // proceed to the hash insert with an integer key
    emitter.label("__rt_array_set_mixed_key_hash_null");
    abi::emit_symbol_address(emitter, "x1", "_empty_str");                     // null normalizes to the empty string "" key pointer
    emitter.instruction("mov x2, #0");                                          // the empty-string key has zero length (string-key marker)
    emitter.instruction("b __rt_array_set_mixed_key_hash_set");                 // proceed to the hash insert with the empty-string key
    emitter.label("__rt_array_set_mixed_key_hash_string");
    emitter.instruction("bl __rt_hash_normalize_key");                          // normalize the string key payload (x1/x2) into a hash key pair
    emitter.label("__rt_array_set_mixed_key_hash_set");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash pointer as the set target
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload the consumed boxed Mixed value
    emitter.instruction("mov x4, #0");                                          // boxed Mixed hash payloads leave the high value word empty
    emitter.instruction("mov x5, #7");                                          // value_type 7 marks the slot as a boxed Mixed pointer
    emit_hash_set_with_refusal_owner(emitter, false);

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
    abi::emit_load_int_immediate(emitter, "rcx", 0);
    emitter.instruction("jmp __rt_array_set_mixed_key_entry");                  // enter with float-key diagnostics enabled
    emitter.label_global("__rt_array_set_mixed_key_already_diagnosed");
    abi::emit_load_int_immediate(emitter, "rcx", 1);
    emitter.instruction("jmp __rt_array_set_mixed_key_entry");                  // retain raw-local ownership on a refused write
    emitter.label_global("__rt_array_set_mixed_key_owned_refusal");
    abi::emit_load_int_immediate(emitter, "rcx", 2);
    emitter.instruction("jmp __rt_array_set_mixed_key_entry");                  // identify the distinct owner acquired for a retiring storeback
    emitter.label_global("__rt_array_set_mixed_key_owned_refusal_already_diagnosed");
    abi::emit_load_int_immediate(emitter, "rcx", 3);
    emitter.label("__rt_array_set_mixed_key_entry");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 64");                                         // reserve 16-aligned slots for inputs, promoted key, and temp/merged hash
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the incoming indexed-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the boxed Mixed key cell
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the consumed boxed Mixed value
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // remember whether this source operation already diagnosed its float key

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
    emitter.instruction("cmp rax, 8");                                          // null mixed keys normalize to the empty string like PHP
    emitter.instruction("je __rt_array_set_mixed_key_null_promote");            // promote the indexed array to a hash with an empty-string key
    emitter.instruction("cmp rax, 2");                                          // float mixed keys are cast to integer keys like PHP
    emitter.instruction("jne __rt_array_set_mixed_key_int_ready");              // integer/bool keys are already valid indexed indexes
    emitter.instruction("movq xmm0, rdi");                                      // load the float key payload into the FP register
    emitter.instruction("test QWORD PTR [rbp - 64], 1");                        // ownership mode does not suppress float-key diagnostics
    emitter.instruction("jne __rt_array_set_mixed_key_int_float_silent");       // use the silent PHP cast for a diagnosed compound key
    abi::emit_call_label(emitter, "__rt_float_key_to_int");
    emitter.instruction("jmp __rt_array_set_mixed_key_int_float_done");         // keep the diagnosed integer result
    emitter.label("__rt_array_set_mixed_key_int_float_silent");
    abi::emit_call_label(emitter, "__rt_php_float_to_int");
    emitter.instruction("mov rax, r11");                                        // recover the silent PHP integer conversion result
    emitter.label("__rt_array_set_mixed_key_int_float_done");
    emitter.instruction("mov rdi, rax");                                        // use the diagnosed PHP integer as the index
    emitter.label("__rt_array_set_mixed_key_int_ready");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the indexed-array pointer
    emitter.instruction("cmp rdi, 0");                                          // negative int keys cannot live in packed indexed storage
    emitter.instruction("jl __rt_array_set_mixed_key_int_promote");             // promote to a hash so a negative key survives like PHP
    emitter.instruction("mov rcx, QWORD PTR [rax]");                            // load the current logical length of the indexed array
    emitter.instruction("cmp rdi, rcx");                                        // a key past the end would create a sparse gap
    emitter.instruction("ja __rt_array_set_mixed_key_int_promote");             // promote to a hash so a sparse key survives like PHP
    emitter.instruction("je __rt_array_set_mixed_key_int_plain");               // a contiguous append has no existing reference slot
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // inspect the existing slot representation
    emitter.instruction("shr r10, 8");                                          // move the indexed element tag into the low bits
    emitter.instruction("and r10, 0x7f");                                       // isolate the indexed element tag
    emitter.instruction("cmp r10, 7");                                          // only boxed slots can hold invoker markers
    emitter.instruction("jne __rt_array_set_mixed_key_int_plain");              // typed slots use ordinary widening and replacement
    emitter.instruction("mov r10, QWORD PTR [rax + 24 + rdi * 8]");             // inspect the overwritten boxed slot
    emitter.instruction("test r10, r10");                                       // absent slots have no write-through identity
    emitter.instruction("jz __rt_array_set_mixed_key_int_plain");               // avoid inspecting an absent boxed slot
    emitter.instruction("cmp QWORD PTR [r10], 11");                             // invoker markers retain the caller storage address
    emitter.instruction("jne __rt_array_set_mixed_key_int_plain");              // ordinary values keep the indexed replacement path
    emit_tagged_marker_write_or_promote(emitter);
    emitter.label("__rt_array_set_mixed_key_int_plain");

    // -- widen typed slots to boxed Mixed BEFORE dropping a Mixed cell into one --
    // See the AArch64 twin: `__rt_array_set_mixed` re-stamps the destination Mixed but never
    // converts the slots already in it, so a write into `[1, 2, 3]` left the untouched slots
    // holding raw integers behind a header claiming they were cell pointers. Here `rdi` holds the
    // integer key and `rax` the array — the opposite of the AArch64 register split.
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the target index across the widening call
    emitter.instruction("mov rsi, QWORD PTR [rax - 8]");                        // load the destination's packed metadata
    emitter.instruction("shr rsi, 8");                                          // move the runtime value_type tag into the low bits
    emitter.instruction("and rsi, 0x7f");                                       // isolate the destination's value_type tag
    emitter.instruction("cmp rsi, 7");                                          // tag 7 = the slots already hold boxed Mixed cells
    emitter.instruction("je __rt_array_set_mixed_key_int_boxed");               // nothing to widen for an already-Mixed destination
    emitter.instruction("mov rdi, rax");                                        // __rt_array_to_mixed takes the array in rdi and the tag in rsi
    emitter.instruction("call __rt_array_to_mixed");                            // box every existing typed slot and stamp the array Mixed
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // republish the possibly copy-on-write-split array pointer
    emitter.label("__rt_array_set_mixed_key_int_boxed");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the widened indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the target index as the indexed-set index argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the consumed boxed Mixed value
    emitter.instruction("call __rt_array_set_mixed");                           // store the value into packed indexed storage and return the array
    emitter.instruction("jmp __rt_array_set_mixed_key_done");                   // finish after an indexed write

    // -- indexed destination + out-of-range int key: promote to hash then set --
    emitter.label("__rt_array_set_mixed_key_int_promote");
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the integer key low word across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], -1");                        // key_hi sentinel marks a scalar integer hash key
    emitter.instruction("jmp __rt_array_set_mixed_key_promote_alloc");          // share the indexed-to-hash promotion path

    // -- indexed destination + null key: promote to hash with the empty-string key like PHP --
    emitter.label("__rt_array_set_mixed_key_null_promote");
    emitter.instruction("lea rax, [rip + _empty_str]");                         // null normalizes to the empty string "" key pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the empty-string key low word across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // the empty-string key has zero length (string-key marker)
    emitter.instruction("jmp __rt_array_set_mixed_key_promote_alloc");          // share the indexed-to-hash promotion path

    // -- indexed destination + string key: promote to hash then set --
    emitter.label("__rt_array_set_mixed_key_string_promote");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed string pointer into the normalizer input
    emitter.instruction("call __rt_hash_normalize_key");                        // normalize the string key into a hash key pair in rax/rdx
    emitter.instruction("cmp rdx, -1");                                         // detect a numeric string normalized to an integer index
    emitter.instruction("jne __rt_array_set_mixed_key_string_ready");           // retain hash promotion for actual string keys
    emitter.instruction("mov rdi, rax");                                        // publish the normalized integer in the indexed dispatch register
    emitter.instruction("jmp __rt_array_set_mixed_key_int_ready");              // validate the existing marker before consuming any indexed owner
    emitter.label("__rt_array_set_mixed_key_string_ready");
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
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed array this promotion abandons
    emitter.instruction("call __rt_decref_array");                              // release the reference the lowering acquired: hash_union only BORROWED it
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the promoted merged hash for Mixed-box conversion
    emitter.instruction("call __rt_hash_to_mixed");                             // box union-copied scalar slots as Mixed cells so foreach readback is correct
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the Mixed-boxed promoted hash pointer (ensure_unique may reallocate)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the promoted merged hash as the set target
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the normalized key low word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the normalized key high word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the consumed boxed Mixed value
    emitter.instruction("xor r8, r8");                                          // boxed Mixed hash payloads leave the high value word empty
    emitter.instruction("mov r9, 7");                                           // value_type 7 marks the slot as a boxed Mixed pointer
    emit_hash_set_with_refusal_owner(emitter, true);
    emitter.instruction("jmp __rt_array_set_mixed_key_done");                   // finish after a promoted hash write

    // -- already-hash destination: materialize the key and set directly --
    emitter.label("__rt_array_set_mixed_key_hash_path");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed Mixed key cell
    emitter.instruction("call __rt_mixed_unbox");                               // peel the key cell to tag in rax and payload in rdi/rdx
    emitter.instruction("cmp rax, 1");                                          // string mixed keys need normalization
    emitter.instruction("je __rt_array_set_mixed_key_hash_string");             // route string keys through the hash-key normalizer
    emitter.instruction("cmp rax, 8");                                          // null mixed keys normalize to the empty string like PHP
    emitter.instruction("je __rt_array_set_mixed_key_hash_null");               // route null keys to the empty-string hash key path
    emitter.instruction("cmp rax, 2");                                          // float mixed keys are cast to integer keys like PHP
    emitter.instruction("jne __rt_array_set_mixed_key_hash_int");               // integer/bool keys become scalar integer hash keys
    emitter.instruction("movq xmm0, rdi");                                      // load the float key payload into the FP register
    emitter.instruction("test QWORD PTR [rbp - 64], 1");                        // inspect only the already-diagnosed flag
    emitter.instruction("jne __rt_array_set_mixed_key_hash_float_silent");      // use the silent PHP cast for a diagnosed compound key
    abi::emit_call_label(emitter, "__rt_float_key_to_int");
    emitter.instruction("jmp __rt_array_set_mixed_key_hash_float_done");        // keep the diagnosed integer result
    emitter.label("__rt_array_set_mixed_key_hash_float_silent");
    abi::emit_call_label(emitter, "__rt_php_float_to_int");
    emitter.instruction("mov rax, r11");                                        // recover the silent PHP integer conversion result
    emitter.label("__rt_array_set_mixed_key_hash_float_done");
    emitter.instruction("mov rdi, rax");                                        // use the diagnosed PHP integer as the hash key
    emitter.label("__rt_array_set_mixed_key_hash_int");
    emitter.instruction("mov rsi, rdi");                                        // publish the integer key payload as the hash key low word
    emitter.instruction("mov rdx, -1");                                         // key_hi sentinel marks scalar integer hash keys
    emitter.instruction("jmp __rt_array_set_mixed_key_hash_set");               // proceed to the hash insert with an integer key
    emitter.label("__rt_array_set_mixed_key_hash_null");
    emitter.instruction("lea rax, [rip + _empty_str]");                         // null normalizes to the empty string "" key pointer
    emitter.instruction("mov rsi, rax");                                        // publish the empty-string pointer as the hash key low word
    emitter.instruction("xor edx, edx");                                        // the empty-string key has zero length (string-key marker)
    emitter.instruction("jmp __rt_array_set_mixed_key_hash_set");               // proceed to the hash insert with the empty-string key
    emitter.label("__rt_array_set_mixed_key_hash_string");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed string pointer into the normalizer input
    emitter.instruction("call __rt_hash_normalize_key");                        // normalize the string key into a hash key pair in rax/rdx
    emitter.instruction("mov rsi, rax");                                        // publish the normalized key low word as the hash key low word
    emitter.label("__rt_array_set_mixed_key_hash_set");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash pointer as the set target
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the consumed boxed Mixed value
    emitter.instruction("xor r8, r8");                                          // boxed Mixed hash payloads leave the high value word empty
    emitter.instruction("mov r9, 7");                                           // value_type 7 marks the slot as a boxed Mixed pointer
    emit_hash_set_with_refusal_owner(emitter, false);

    emitter.label("__rt_array_set_mixed_key_done");
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the final array/hash pointer in rax
}

/// Forwards the extra owner into the hash setter while retaining ordinary COW semantics.
fn emit_hash_set_with_refusal_owner(emitter: &mut Emitter, promoted: bool) {
    if promoted {
        abi::emit_call_label(emitter, "__rt_hash_set_owned_refusal");
        return;
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp, #56]");                           // forward the mixed-key caller's separate helper-owner contract
            emitter.instruction("tbz x9, #1, 987f");                            // raw-local transfers keep their destination owner on refusal
        }
        Arch::X86_64 => {
            emitter.instruction("test QWORD PTR [rbp - 64], 2");                // preserve the ownership distinction for already-hash destinations
            emitter.instruction("jz 987f");                                     // raw locals use the ordinary non-consuming refusal path
        }
    }
    abi::emit_call_label(emitter, "__rt_hash_set_owned_refusal");
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { "b 988f" }
        else { "jmp 988f" });                                                   // preserve the resulting owned hash pointer
    emitter.label("987");
    abi::emit_call_label(emitter, "__rt_hash_set");
    emitter.label("988");
}

/// Consumes a nullable scalar replacement without consuming or relocating its destination array.
fn emit_tagged_marker_write_or_promote(emitter: &mut Emitter) {
    let valid = "__rt_array_set_mixed_key_tagged_valid";
    let invalid = "__rt_array_set_mixed_key_tagged_invalid";
    let result = abi::int_result_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x11, [x10, #16]");                         // inspect the physical shape before promotion consumes the source array
            emitter.instruction(&format!("cmp x11, #{}", crate::codegen_support::callable_invoker_args::INVOKER_ARG_TAGGED_SCALAR_SOURCE)); // recognize inline nullable-integer storage
            emitter.instruction("b.ne __rt_array_set_mixed_key_int_promote");   // other markers retain the existing hash-based writer
            emitter.instruction("ldr x10, [x10, #8]");                          // address the caller's two-word nullable-integer slot
            emitter.instruction("str x10, [sp, #24]");                          // preserve its address across replacement dereferencing
            emitter.instruction("ldr x0, [sp, #16]");                           // borrow the consumed replacement box while decoding its PHP value
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp QWORD PTR [r10 + 16], {}", crate::codegen_support::callable_invoker_args::INVOKER_ARG_TAGGED_SCALAR_SOURCE)); // recognize inline nullable-integer storage
            emitter.instruction("jne __rt_array_set_mixed_key_int_promote");    // other markers retain the existing hash-based writer
            emitter.instruction("mov r10, QWORD PTR [r10 + 8]");                // address the caller's two-word nullable-integer slot
            emitter.instruction("mov QWORD PTR [rbp - 32], r10");               // preserve its address across replacement dereferencing
            emitter.instruction("mov rax, QWORD PTR [rbp - 24]");               // borrow the consumed replacement box while decoding its PHP value
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    crate::codegen_support::callable_invoker_args::emit_tagged_scalar_ref_assignment_guard(
        emitter, result, valid, invalid,
    );
    emitter.label(invalid);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("ldr x0, [sp, #16]"),              // consume the rejected box while the original array owner is still intact
        Arch::X86_64 => emitter.instruction("mov rax, QWORD PTR [rbp - 24]"),   // consume the rejected box while the original array owner is still intact
    }
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    super::container_refusal_owner::emit_release(emitter);
    abi::emit_call_label(emitter, "__rt_tagged_reference_assignment_error");
    emitter.label(valid);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x10, [sp, #24]");                          // reload the selected caller slot without touching the marker array
            emitter.instruction("str x1, [x10]");                               // publish the decoded scalar payload
            emitter.instruction("str x0, [x10, #8]");                           // publish its actual integer or null tag
            emitter.instruction("ldr x0, [sp, #16]");                           // retire the complete box, including any persistent reference wrapper
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rbp - 32]");               // reload the selected caller slot without touching the marker array
            emitter.instruction("mov QWORD PTR [r10], rdi");                    // publish the decoded scalar payload
            emitter.instruction("mov QWORD PTR [r10 + 8], rax");                // publish its actual integer or null tag
            emitter.instruction("mov rax, QWORD PTR [rbp - 24]");               // retire the complete box, including any persistent reference wrapper
        }
    }
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("ldr x0, [sp]"),                   // transfer the unchanged destination owner back to its caller
        Arch::X86_64 => emitter.instruction("mov rax, QWORD PTR [rbp - 8]"),    // transfer the unchanged destination owner back to its caller
    }
    abi::emit_jump(emitter, "__rt_array_set_mixed_key_done");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Numeric strings reach the indexed marker guard before any promotion can release storage.
    #[test]
    fn normalized_numeric_string_keys_validate_indexed_markers_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_array_set_mixed_key(&mut emitter);
            let assembly = emitter.output();
            let normalized = assembly.split("__rt_array_set_mixed_key_string_promote:").nth(1).unwrap()
                .split("__rt_array_set_mixed_key_promote_alloc:").next().unwrap();
            assert!(normalized.find("__rt_hash_normalize_key").unwrap()
                < normalized.find("__rt_array_set_mixed_key_int_ready").unwrap(), "{name}: {normalized}");
            assert!(!normalized.contains("__rt_decref_array") && !normalized.contains("__rt_hash_new"),
                "{name}: normalization must not consume a destination owner");
        }
    }

    /// Mixed keys validate nullable marker writes before consuming the original array owner.
    #[test]
    fn nullable_marker_mixed_key_writes_precede_array_promotion_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_array_set_mixed_key(&mut emitter);
            let assembly = emitter.output();
            let selection = assembly.split("__rt_array_set_mixed_key_int_ready:").nth(1).unwrap()
                .split("__rt_array_set_mixed_key_int_plain:").next().unwrap();
            let invalid = selection.find("__rt_array_set_mixed_key_tagged_invalid:").unwrap();
            let valid = selection.find("__rt_array_set_mixed_key_tagged_valid:").unwrap();
            let refusal = &selection[invalid..valid];
            assert!(refusal.find("__rt_decref_mixed").unwrap()
                < refusal.find("__rt_tagged_reference_assignment_error").unwrap(), "{name}: {refusal}");
            assert!(!selection.contains("__rt_decref_array") && !selection.contains("__rt_hash_new"),
                "{name}: a refused marker write must leave the destination owner intact");
            assert!(selection.contains("__rt_mixed_unbox"), "{name}: persistent reference replacements need decoding");
            let publication = &selection[valid..];
            let store = match target.arch {
                Arch::AArch64 => "str x0, [x10, #8]",
                Arch::X86_64 => "mov QWORD PTR [r10 + 8], rax",
            };
            assert!(publication.find(store).unwrap() < publication.find("__rt_decref_mixed").unwrap(),
                "{name}: publish both scalar words before retiring the decoded replacement");
            assert!(publication.contains("__rt_array_set_mixed_key_done"), "{name}: return the original array directly");
        }
    }

    /// Every supported target routes packed invoker slots to ownership-safe hash writeback.
    #[test]
    fn append_latest_invoker_promotion_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_array_set_mixed_key(&mut emitter);
            super::super::super::objects::emit_mixed_array_set(&mut emitter);
            super::super::super::system::emit_json_encode_mixed(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_json_encode_mixed_invoker:"), "{name}");
            assert!(asm.contains("__rt_json_encode_mixed_nested:"), "{name}");
            match target.arch {
                Arch::AArch64 => {
                    assert!(asm.contains("cmp x11, #11"), "{name}");
                    assert!(asm.contains("b.ne __rt_array_set_mixed_key_int_promote"), "{name}");
                    assert!(asm.contains("cmp x9, #11\n"), "{name}");
                    assert!(asm.contains("b.eq __rt_mixed_array_set_promote"), "{name}");
                }
                Arch::X86_64 => {
                    assert!(asm.contains("cmp QWORD PTR [r10], 11"), "{name}");
                    assert!(asm.contains("jne __rt_array_set_mixed_key_int_promote"), "{name}");
                    assert!(asm.contains("cmp QWORD PTR [rax], 11"), "{name}");
                    assert!(asm.contains("je __rt_mixed_array_set_promote"), "{name}");
                }
            }
        }
    }
}
