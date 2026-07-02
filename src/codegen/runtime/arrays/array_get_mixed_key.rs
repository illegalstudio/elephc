//! Purpose:
//! Emits the `__rt_array_get_mixed_key` runtime helper for reads from a
//! statically `Array(Mixed)` indexed local whose key is a boxed `Mixed` cell
//! (notably PHP `foreach` loop keys, which are always `Mixed` in EIR).
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - The key tag is only known at runtime, so the helper dispatches on the
//!   destination runtime kind first. An already-promoted hash (kind 3, produced
//!   by an earlier string-key write in the same foreach rebuild) reads through
//!   `__rt_hash_get`; a hit whose slot tag is 7 (boxed Mixed pointer) is retained
//!   before returning, and any other slot tag is re-boxed via
//!   `__rt_mixed_from_value`. On a miss the helper returns boxed `Mixed(null)`.
//! - An indexed destination (kind 1 or 2) unboxes the key: a string key returns
//!   boxed `Mixed(null)` (undefined index, matching PHP quiet access), while an
//!   integer key is bounds-checked and the element is boxed via
//!   `__rt_mixed_from_value` (or retained when the slot already holds a boxed
//!   Mixed pointer, tag 7).
//! - The helper does not release or mutate the incoming array pointer; it is a
//!   pure read returning an owned boxed `Mixed` cell (borrowed slots are
//!   retained first), mirroring `__rt_mixed_array_get` ownership.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the boxed-Mixed-key indexed/hash array get helper for the current target.
pub fn emit_array_get_mixed_key(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_get_mixed_key_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_get_mixed_key ---");
    emitter.label_global("__rt_array_get_mixed_key");

    // Stack:
    //   [sp, #0]  = array/hash pointer
    //   [sp, #8]  = boxed Mixed key cell
    //   [sp, #16] = saved x29
    //   [sp, #24] = saved x30
    emitter.instruction("sub sp, sp, #32");                                     // reserve frame: 2 inputs + saved fp/lr (16-byte aligned)
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish a helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the incoming array/hash pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the boxed Mixed key cell

    // -- dispatch on the destination runtime kind --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the destination pointer
    emitter.instruction("bl __rt_heap_kind");                                   // load the destination heap kind byte into x0
    emitter.instruction("cmp x0, #3");                                          // kind 3 marks already-promoted associative hash storage
    emitter.instruction("b.eq __rt_array_get_mixed_key_hash_path");             // route already-hash destinations through the hash reader

    // -- indexed destination: dispatch on the key tag --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed Mixed key cell
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel the key cell to tag in x0 and payload in x1/x2
    emitter.instruction("cmp x0, #1");                                          // string mixed keys are undefined on indexed storage
    emitter.instruction("b.eq __rt_array_get_mixed_key_missing");               // return boxed Mixed(null) for a string key on indexed storage
    emitter.instruction("cmp x0, #2");                                          // float mixed keys are cast to integer keys like PHP
    emitter.instruction("b.ne __rt_array_get_mixed_key_int_ready");             // integer/bool keys are already valid indexed indexes
    emitter.instruction("fmov d0, x1");                                         // load the float key payload into the FP register
    emitter.instruction("fcvtzs x1, d0");                                       // cast the float key to an integer index like PHP
    emitter.label("__rt_array_get_mixed_key_int_ready");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the indexed-array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load the current logical length of the indexed array
    emitter.instruction("cmp x1, #0");                                          // negative int keys are undefined on indexed storage
    emitter.instruction("b.lt __rt_array_get_mixed_key_missing");               // return boxed Mixed(null) for a negative indexed-array key
    emitter.instruction("cmp x1, x9");                                          // an index past the end is undefined
    emitter.instruction("b.ge __rt_array_get_mixed_key_missing");               // return boxed Mixed(null) for an out-of-bounds indexed-array key
    emitter.instruction("ldr x13, [x0, #-8]");                                  // load packed indexed-array kind metadata
    emitter.instruction("ubfx x13, x13, #8, #7");                               // extract the runtime element value_type tag
    emitter.instruction("add x10, x0, #24");                                    // skip the 24-byte array header to reach the contiguous payload
    emitter.instruction("cmp x13, #7");                                         // are indexed slots already boxed Mixed pointers?
    emitter.instruction("b.eq __rt_array_get_mixed_key_indexed_boxed");         // boxed slots must be retained before returning
    emitter.instruction("cmp x13, #1");                                         // do indexed slots contain string pointer/length pairs?
    emitter.instruction("b.eq __rt_array_get_mixed_key_indexed_string");        // string slots need a 16-byte load before boxing
    emitter.instruction("cmp x13, #8");                                         // do indexed slots represent null payloads?
    emitter.instruction("b.eq __rt_array_get_mixed_key_null");                  // null slots have no payload to read
    emitter.instruction("ldr x1, [x10, x1, lsl #3]");                           // load scalar or pointer payload from the typed indexed slot
    emitter.instruction("mov x2, #0");                                          // typed indexed slots use one payload word except strings
    emitter.instruction("mov x0, x13");                                         // x0 = runtime value_type tag for the boxed result
    emitter.instruction("bl __rt_mixed_from_value");                            // box the typed indexed-array element into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned Mixed* in x0

    emitter.label("__rt_array_get_mixed_key_indexed_boxed");
    emitter.instruction("ldr x0, [x10, x1, lsl #3]");                           // load the boxed Mixed pointer from the indexed slot
    emitter.instruction("cbz x0, __rt_array_get_mixed_key_null_ret");           // empty slot → Mixed(null)
    emitter.instruction("bl __rt_incref");                                      // retain the stored Mixed cell so the caller owns the returned result
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned Mixed* in x0

    emitter.label("__rt_array_get_mixed_key_indexed_string");
    emitter.instruction("lsl x1, x1, #4");                                      // convert the element index to a 16-byte string slot offset
    emitter.instruction("add x10, x10, x1");                                    // x10 = address of the selected string slot
    emitter.instruction("ldr x1, [x10]");                                       // load string pointer from the selected slot
    emitter.instruction("ldr x2, [x10, #8]");                                   // load string length from the selected slot
    emitter.instruction("mov x0, #1");                                          // x0 = string runtime value_type tag
    emitter.instruction("bl __rt_mixed_from_value");                            // box the string indexed-array element into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned Mixed* in x0

    emitter.label("__rt_array_get_mixed_key_null");
    emitter.instruction("mov x0, #8");                                          // x0 = null runtime value_type tag
    emitter.instruction("mov x1, #0");                                          // value_lo = 0 for null
    emitter.instruction("mov x2, #0");                                          // value_hi = 0 for null
    emitter.instruction("bl __rt_mixed_from_value");                            // box the null value into a fresh Mixed cell
    emitter.label("__rt_array_get_mixed_key_null_ret");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned Mixed* in x0

    emitter.label("__rt_array_get_mixed_key_missing");
    emitter.instruction("mov x0, #8");                                          // x0 = null runtime value_type tag
    emitter.instruction("mov x1, #0");                                          // value_lo = 0 for null
    emitter.instruction("mov x2, #0");                                          // value_hi = 0 for null
    emitter.instruction("bl __rt_mixed_from_value");                            // box the null value into a fresh Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned Mixed* in x0

    // -- already-hash destination: materialize the key and read directly --
    emitter.label("__rt_array_get_mixed_key_hash_path");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed Mixed key cell
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel the key cell to tag in x0 and payload in x1/x2
    emitter.instruction("cmp x0, #1");                                          // string mixed keys need normalization
    emitter.instruction("b.eq __rt_array_get_mixed_key_hash_string");           // route string keys through the hash-key normalizer
    emitter.instruction("cmp x0, #2");                                          // float mixed keys are cast to integer keys like PHP
    emitter.instruction("b.ne __rt_array_get_mixed_key_hash_int");              // integer/bool keys become scalar integer hash keys
    emitter.instruction("fmov d0, x1");                                         // load the float key payload into the FP register
    emitter.instruction("fcvtzs x1, d0");                                       // cast the float key to an integer hash key like PHP
    emitter.label("__rt_array_get_mixed_key_hash_int");
    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel marks scalar integer hash keys
    emitter.instruction("b __rt_array_get_mixed_key_hash_get");                 // proceed to the hash read with an integer key
    emitter.label("__rt_array_get_mixed_key_hash_string");
    emitter.instruction("bl __rt_hash_normalize_key");                          // normalize the string key payload (x1/x2) into a hash key pair
    emitter.label("__rt_array_get_mixed_key_hash_get");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash pointer as the hash_get target
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=value_lo, x2=value_hi, x3=value_tag
    emitter.instruction("cbz x0, __rt_array_get_mixed_key_missing");            // miss → boxed Mixed(null)
    emitter.instruction("cmp x3, #7");                                          // is the hash entry already a boxed Mixed pointer?
    emitter.instruction("b.ne __rt_array_get_mixed_key_hash_box");              // no → box (lo, hi, tag) into a fresh Mixed cell
    emitter.instruction("mov x0, x1");                                          // yes → move the stored Mixed cell into the return register
    emitter.instruction("bl __rt_incref");                                      // retain the stored Mixed cell so the caller owns the returned result
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned Mixed* in x0
    emitter.label("__rt_array_get_mixed_key_hash_box");
    emitter.instruction("mov x0, x3");                                          // x0 = value_tag (mixed_from_value first arg)
    emitter.instruction("bl __rt_mixed_from_value");                            // box the typed entry into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned Mixed* in x0
}

/// Emits the Linux x86_64 boxed-Mixed-key indexed/hash array get helper.
fn emit_array_get_mixed_key_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_get_mixed_key ---");
    emitter.label_global("__rt_array_get_mixed_key");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 32");                                         // reserve 16-aligned slots for the 2 saved inputs
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the incoming array/hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the boxed Mixed key cell

    // -- dispatch on the destination runtime kind --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the destination pointer
    emitter.instruction("call __rt_heap_kind");                                 // load the destination heap kind byte into rax
    emitter.instruction("cmp rax, 3");                                          // kind 3 marks already-promoted associative hash storage
    emitter.instruction("je __rt_array_get_mixed_key_hash_path");               // route already-hash destinations through the hash reader

    // -- indexed destination: dispatch on the key tag --
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed Mixed key cell
    emitter.instruction("call __rt_mixed_unbox");                               // peel the key cell to tag in rax and payload in rdi/rdx
    emitter.instruction("cmp rax, 1");                                          // string mixed keys are undefined on indexed storage
    emitter.instruction("je __rt_array_get_mixed_key_missing");                 // return boxed Mixed(null) for a string key on indexed storage
    emitter.instruction("cmp rax, 2");                                          // float mixed keys are cast to integer keys like PHP
    emitter.instruction("jne __rt_array_get_mixed_key_int_ready");              // integer/bool keys are already valid indexed indexes
    emitter.instruction("movq xmm0, rdi");                                      // load the float key payload into the FP register
    emitter.instruction("cvttsd2si rdi, xmm0");                                 // cast the float key to an integer index like PHP
    emitter.label("__rt_array_get_mixed_key_int_ready");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the indexed-array pointer
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // load the current logical length of the indexed array
    emitter.instruction("cmp rdi, 0");                                          // negative int keys are undefined on indexed storage
    emitter.instruction("jl __rt_array_get_mixed_key_missing");                 // return boxed Mixed(null) for a negative indexed-array key
    emitter.instruction("cmp rdi, r9");                                         // an index past the end is undefined
    emitter.instruction("jge __rt_array_get_mixed_key_missing");                // return boxed Mixed(null) for an out-of-bounds indexed-array key
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load packed indexed-array kind metadata
    emitter.instruction("shr r10, 8");                                          // shift the runtime element value_type tag into the low bits
    emitter.instruction("and r10, 0x7f");                                       // remove the persistent COW flag from the extracted tag
    emitter.instruction("lea r11, [rax + 24]");                                 // skip the 24-byte array header to reach the contiguous payload
    emitter.instruction("cmp r10, 7");                                          // are indexed slots already boxed Mixed pointers?
    emitter.instruction("je __rt_array_get_mixed_key_indexed_boxed");           // boxed slots must be retained before returning
    emitter.instruction("cmp r10, 1");                                          // do indexed slots contain string pointer/length pairs?
    emitter.instruction("je __rt_array_get_mixed_key_indexed_string");          // string slots need a 16-byte load before boxing
    emitter.instruction("cmp r10, 8");                                          // do indexed slots represent null payloads?
    emitter.instruction("je __rt_array_get_mixed_key_null");                    // null slots have no payload to read
    emitter.instruction("mov rax, r10");                                        // rax = runtime value_type tag for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [r11 + rdi * 8]");                  // load scalar or pointer payload from the typed indexed slot
    emitter.instruction("xor esi, esi");                                        // typed indexed slots use one payload word except strings
    emitter.instruction("call __rt_mixed_from_value");                          // box the typed indexed-array element into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the owned Mixed* in rax

    emitter.label("__rt_array_get_mixed_key_indexed_boxed");
    emitter.instruction("mov rax, QWORD PTR [r11 + rdi * 8]");                  // load the boxed Mixed pointer from the indexed slot
    emitter.instruction("test rax, rax");                                       // empty slot → Mixed(null)
    emitter.instruction("je __rt_array_get_mixed_key_null_ret");                // return boxed Mixed(null) for an empty indexed slot
    emitter.instruction("call __rt_incref");                                    // retain the stored Mixed cell so the caller owns the returned result
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the owned Mixed* in rax

    emitter.label("__rt_array_get_mixed_key_indexed_string");
    emitter.instruction("shl rdi, 4");                                          // convert the element index to a 16-byte string slot offset
    emitter.instruction("add r11, rdi");                                        // r11 = address of the selected string slot
    emitter.instruction("mov rax, 1");                                          // rax = string runtime value_type tag
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // load string pointer from the selected slot
    emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                        // load string length from the selected slot
    emitter.instruction("call __rt_mixed_from_value");                          // box the string indexed-array element into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the owned Mixed* in rax

    emitter.label("__rt_array_get_mixed_key_null");
    emitter.instruction("mov rax, 8");                                          // rax = null runtime value_type tag
    emitter.instruction("mov rdi, 0");                                          // value_lo = 0 for null
    emitter.instruction("mov rsi, 0");                                          // value_hi = 0 for null
    emitter.instruction("call __rt_mixed_from_value");                          // box the null value into a fresh Mixed cell
    emitter.label("__rt_array_get_mixed_key_null_ret");
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the owned Mixed* in rax

    emitter.label("__rt_array_get_mixed_key_missing");
    emitter.instruction("mov rax, 8");                                          // rax = null runtime value_type tag
    emitter.instruction("mov rdi, 0");                                          // value_lo = 0 for null
    emitter.instruction("mov rsi, 0");                                          // value_hi = 0 for null
    emitter.instruction("call __rt_mixed_from_value");                          // box the null value into a fresh Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the owned Mixed* in rax

    // -- already-hash destination: materialize the key and read directly --
    emitter.label("__rt_array_get_mixed_key_hash_path");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed Mixed key cell
    emitter.instruction("call __rt_mixed_unbox");                               // peel the key cell to tag in rax and payload in rdi/rdx
    emitter.instruction("cmp rax, 1");                                          // string mixed keys need normalization
    emitter.instruction("je __rt_array_get_mixed_key_hash_string");             // route string keys through the hash-key normalizer
    emitter.instruction("cmp rax, 2");                                          // float mixed keys are cast to integer keys like PHP
    emitter.instruction("jne __rt_array_get_mixed_key_hash_int");               // integer/bool keys become scalar integer hash keys
    emitter.instruction("movq xmm0, rdi");                                      // load the float key payload into the FP register
    emitter.instruction("cvttsd2si rdi, xmm0");                                 // cast the float key to an integer hash key like PHP
    emitter.label("__rt_array_get_mixed_key_hash_int");
    emitter.instruction("mov rsi, rdi");                                        // publish the integer key payload as the hash key low word
    emitter.instruction("mov rdx, -1");                                         // key_hi sentinel marks scalar integer hash keys
    emitter.instruction("jmp __rt_array_get_mixed_key_hash_get");               // proceed to the hash read with an integer key
    emitter.label("__rt_array_get_mixed_key_hash_string");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed string pointer into the normalizer input
    emitter.instruction("call __rt_hash_normalize_key");                        // normalize the string key into a hash key pair in rax/rdx
    emitter.instruction("mov rsi, rax");                                        // publish the normalized key low word as the hash key low word
    emitter.label("__rt_array_get_mixed_key_hash_get");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash pointer as the hash_get target
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=value_lo, rsi=value_hi, rcx=value_tag
    emitter.instruction("test rax, rax");                                       // miss → boxed Mixed(null)
    emitter.instruction("je __rt_array_get_mixed_key_missing");                 // return boxed Mixed(null) after a hash miss
    emitter.instruction("cmp rcx, 7");                                          // is the hash entry already a boxed Mixed pointer?
    emitter.instruction("jne __rt_array_get_mixed_key_hash_box");               // no → box (lo, hi, tag) into a fresh Mixed cell
    emitter.instruction("mov rax, rdi");                                        // yes → move the stored Mixed cell into the return register
    emitter.instruction("call __rt_incref");                                    // retain the stored Mixed cell so the caller owns the returned result
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the owned Mixed* in rax
    emitter.label("__rt_array_get_mixed_key_hash_box");
    emitter.instruction("mov rax, rcx");                                        // rax = value_tag (mixed_from_value first arg)
    emitter.instruction("call __rt_mixed_from_value");                          // box the typed entry into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the owned Mixed* in rax
}