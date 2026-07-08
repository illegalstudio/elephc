//! Purpose:
//! Emits the `__rt_array_get_mixed_key` runtime helper for reads from a
//! statically `Array(Mixed)` indexed local whose key is a boxed `Mixed` cell
//! or a string — the read-side mirror of `__rt_array_set_mixed_key`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The key tag is only known at runtime. The helper tag-dispatches: an
//!   integer/bool key reads from indexed storage (fast path), while a string
//!   key normalizes and routes through `__rt_hash_get` if the array has already
//!   been promoted to hash storage (kind 3). A string key on pure indexed
//!   storage returns `Mixed(null)` with an undefined-key warning, matching PHP.
//! - Inputs are array pointer, normalized key pair, and a warning flag. The
//!   result is always a boxed `Mixed` pointer in x0 (caller owns it).

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the mixed-key indexed/hash array read helper for the current target.
pub fn emit_array_get_mixed_key(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_get_mixed_key_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_get_mixed_key ---");
    emitter.label_global("__rt_array_get_mixed_key");

    // Stack:
    //   [sp, #0]  = array_ptr
    //   [sp, #8]  = key_lo
    //   [sp, #16] = key_hi
    //   [sp, #24] = warn_on_missing
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // reserve frame: 4 inputs + saved fp/lr (16-byte aligned)
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the incoming array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the key low word
    emitter.instruction("str x2, [sp, #16]");                                   // save the key high word (sentinel)
    emitter.instruction("str x3, [sp, #24]");                                   // save whether missing keys should emit PHP warnings

    emitter.instruction("cbz x0, __rt_array_get_mixed_key_null");               // null array → Mixed(null)

    // -- dispatch on array storage kind --
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load packed kind metadata from the array header
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low byte (kind tag)
    emitter.instruction("cmp x9, #3");                                           // kind 3 = hash storage?
    emitter.instruction("b.eq __rt_array_get_mixed_key_hash");                   // route hash-storage arrays through hash_get
    emitter.instruction("cmp x9, #2");                                          // kind 2 = indexed storage?
    emitter.instruction("b.ne __rt_array_get_mixed_key_null");                   // unknown kind → null

    // -- indexed storage: dispatch on key tag --
    emitter.label("__rt_array_get_mixed_key_indexed");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload key_hi
    emitter.instruction("cmn x11, #1");                                          // compare with -1 (int-key sentinel)
    emitter.instruction("b.ne __rt_array_get_mixed_key_string_on_indexed");      // string key on indexed storage → warn + null

    // -- integer key on indexed storage: inline bounds-checked read --
    emitter.instruction("ldr x12, [sp, #8]");                                   // x12 = key_lo (int index)
    emitter.instruction("ldr x9, [x0]");                                         // x9 = array length (header offset 0)
    emitter.instruction("cmp x12, #0");                                          // negative index → null
    emitter.instruction("b.lt __rt_array_get_mixed_key_int_missing");            // warn and return null for a negative indexed-array key
    emitter.instruction("cmp x12, x9");                                         // index >= length → null
    emitter.instruction("b.ge __rt_array_get_mixed_key_int_missing");           // warn and return null for an out-of-bounds indexed-array key
    emitter.instruction("ldr x13, [x0, #-8]");                                   // reload kind metadata for element type tag
    emitter.instruction("ubfx x13, x13, #8, #7");                               // extract the runtime element value_type tag
    emitter.instruction("add x10, x0, #24");                                    // skip the 24-byte array header to reach the contiguous payload
    emitter.instruction("cmp x13, #7");                                         // are indexed slots already boxed Mixed pointers?
    emitter.instruction("b.eq __rt_array_get_mixed_key_indexed_boxed");          // boxed slots must be retained before returning
    emitter.instruction("cmp x13, #1");                                         // do indexed slots contain string pointer/length pairs?
    emitter.instruction("b.eq __rt_array_get_mixed_key_indexed_string");         // string slots need a 16-byte load before boxing
    emitter.instruction("cmp x13, #8");                                         // do indexed slots represent null payloads?
    emitter.instruction("b.eq __rt_array_get_mixed_key_indexed_null");            // null slots have no payload to read
    emitter.instruction("ldr x1, [x10, x12, lsl #3]");                           // load scalar or pointer payload from the typed indexed slot
    emitter.instruction("mov x2, #0");                                          // typed indexed slots use one payload word except strings
    emitter.instruction("mov x0, x13");                                         // x0 = runtime value_type tag for the boxed result
    emitter.instruction("bl __rt_mixed_from_value");                             // box the typed indexed-array element into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #32]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                      // release the local frame
    emitter.instruction("ret");                                                  // return Mixed* in x0

    emitter.label("__rt_array_get_mixed_key_indexed_boxed");
    emitter.instruction("ldr x0, [x10, x12, lsl #3]");                            // load the boxed Mixed pointer from the indexed slot
    emitter.instruction("cbz x0, __rt_array_get_mixed_key_null");                // empty slot → null Mixed
    emitter.instruction("bl __rt_incref");                                       // retain the stored Mixed cell so the caller owns the returned result
    emitter.instruction("ldp x29, x30, [sp, #32]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                      // release the local frame
    emitter.instruction("ret");                                                  // return Mixed* in x0

    emitter.label("__rt_array_get_mixed_key_indexed_string");
    emitter.instruction("lsl x12, x12, #4");                                    // convert the element index to a 16-byte string slot offset
    emitter.instruction("add x10, x10, x12");                                   // x10 = address of the selected string slot
    emitter.instruction("ldr x1, [x10]");                                       // load string pointer from the selected slot
    emitter.instruction("ldr x2, [x10, #8]");                                   // load string length from the selected slot
    emitter.instruction("mov x0, #1");                                          // x0 = string runtime value_type tag
    emitter.instruction("bl __rt_mixed_from_value");                             // box the string indexed-array element into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #32]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                      // release the local frame
    emitter.instruction("ret");                                                  // return Mixed* in x0

    emitter.label("__rt_array_get_mixed_key_indexed_null");
    emitter.instruction("mov x0, #8");                                          // x0 = null runtime value_type tag
    emitter.instruction("mov x1, #0");                                          // value_lo = 0 for null
    emitter.instruction("mov x2, #0");                                          // value_hi = 0 for null
    emitter.instruction("bl __rt_mixed_from_value");                             // box the null indexed-array element into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #32]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                      // release the local frame
    emitter.instruction("ret");                                                  // return Mixed* in x0

    emitter.label("__rt_array_get_mixed_key_int_missing");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the warn-on-missing flag
    emitter.instruction("cbz x9, __rt_array_get_mixed_key_null");               // silent reads skip undefined-key warnings
    emitter.instruction("ldr x0, [sp, #8]");                                     // reload the missing integer key for the PHP warning
    emitter.instruction("bl __rt_warn_undefined_array_key_int");                 // emit or suppress the undefined-array-key warning
    emitter.instruction("b __rt_array_get_mixed_key_null");                      // return boxed Mixed(null) after the warning

    // -- string key on indexed storage: PHP returns null and may warn --
    emitter.label("__rt_array_get_mixed_key_string_on_indexed");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the warn-on-missing flag
    emitter.instruction("cbz x9, __rt_array_get_mixed_key_null");               // silent reads skip undefined-key warnings
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the missing string key pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the missing string key length
    emitter.instruction("bl __rt_warn_undefined_array_key_str");                // emit or suppress the undefined string-key warning
    emitter.instruction("b __rt_array_get_mixed_key_null");                     // return boxed Mixed(null) for a string key on indexed storage

    // -- hash storage: delegate to __rt_hash_get ---
    emitter.label("__rt_array_get_mixed_key_hash");
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = key_lo
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = key_hi
    emitter.instruction("bl __rt_hash_get");                                     // x0=found, x1=value_lo, x2=value_hi, x3=value_tag
    emitter.instruction("cbz x0, __rt_array_get_mixed_key_hash_missing");        // miss → optional warning + null
    emitter.instruction("cmp x3, #7");                                           // is the hash entry already a boxed Mixed?
    emitter.instruction("b.ne __rt_array_get_mixed_key_hash_box");               // no → box (lo, hi, tag) into a fresh Mixed cell
    emitter.instruction("mov x0, x1");                                           // yes → move the stored Mixed cell into the return register
    emitter.instruction("bl __rt_incref");                                       // retain the stored Mixed cell so the caller owns the returned result
    emitter.instruction("ldp x29, x30, [sp, #32]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                      // release the local frame
    emitter.instruction("ret");                                                  // return Mixed* in x0
    emitter.label("__rt_array_get_mixed_key_hash_box");
    emitter.instruction("mov x0, x3");                                          // x0 = value_tag (mixed_from_value first arg)
    emitter.instruction("mov x1, x1");                                          // x1 = value_lo (already in place)
    emitter.instruction("mov x2, x2");                                          // x2 = value_hi (already in place)
    emitter.instruction("bl __rt_mixed_from_value");                             // box the hash entry into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #32]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                      // release the local frame
    emitter.instruction("ret");                                                  // return Mixed* in x0

    emitter.label("__rt_array_get_mixed_key_hash_missing");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the warn-on-missing flag
    emitter.instruction("cbz x9, __rt_array_get_mixed_key_null");               // silent reads skip undefined-key warnings
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload key_hi to distinguish integer and string keys
    emitter.instruction("cmn x11, #1");                                         // check whether the missing hash key is integer-keyed
    emitter.instruction("b.eq __rt_array_get_mixed_key_hash_missing_int");       // integer keys use the decimal warning helper
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the missing string key pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the missing string key length
    emitter.instruction("bl __rt_warn_undefined_array_key_str");                // emit or suppress the undefined string-key warning
    emitter.instruction("b __rt_array_get_mixed_key_null");                     // return boxed Mixed(null) after the string-key warning
    emitter.label("__rt_array_get_mixed_key_hash_missing_int");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the missing integer key
    emitter.instruction("bl __rt_warn_undefined_array_key_int");                // emit or suppress the undefined integer-key warning
    emitter.instruction("b __rt_array_get_mixed_key_null");                     // return boxed Mixed(null) after the integer-key warning

    // -- return Mixed(null) ---
    emitter.label("__rt_array_get_mixed_key_null");
    emitter.instruction("mov x0, #8");                                          // x0 = null runtime value_type tag
    emitter.instruction("mov x1, #0");                                          // value_lo = 0 for null
    emitter.instruction("mov x2, #0");                                          // value_hi = 0 for null
    emitter.instruction("bl __rt_mixed_from_value");                             // box null into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #32]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                      // release the local frame
    emitter.instruction("ret");                                                  // return Mixed* in x0
}

/// Emits the x86_64 variant of `__rt_array_get_mixed_key`.
fn emit_array_get_mixed_key_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_get_mixed_key ---");
    emitter.label_global("__rt_array_get_mixed_key");

    // Stack layout (16-byte aligned):
    //   [rbp - 8]  = array_ptr
    //   [rbp - 16] = key_lo
    //   [rbp - 24] = key_hi
    //   [rbp - 32] = warn_on_missing
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve 32 bytes for locals (16-byte aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the incoming array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the key low word
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the key high word (sentinel)
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save whether missing keys should emit PHP warnings

    emitter.instruction("test rdi, rdi");                                       // null array check
    emitter.instruction("je __rt_array_get_mixed_key_null");                    // null array → Mixed(null)

    // -- dispatch on array storage kind --
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // load packed kind metadata from the array header
    emitter.instruction("and r9, 0xff");                                        // isolate the low byte (kind tag)
    emitter.instruction("cmp r9, 3");                                           // kind 3 = hash storage?
    emitter.instruction("je __rt_array_get_mixed_key_hash");                    // route hash-storage arrays through hash_get
    emitter.instruction("cmp r9, 2");                                           // kind 2 = indexed storage?
    emitter.instruction("jne __rt_array_get_mixed_key_null");                   // unknown kind → null

    // -- indexed storage: dispatch on key tag --
    emitter.label("__rt_array_get_mixed_key_indexed");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload key_hi
    emitter.instruction("cmp r11, -1");                                         // compare with -1 (int-key sentinel)
    emitter.instruction("jne __rt_array_get_mixed_key_string_on_indexed");      // string key on indexed storage → warn + null

    // -- integer key on indexed storage: inline bounds-checked read --
    emitter.instruction("mov r12, QWORD PTR [rbp - 16]");                       // r12 = key_lo (int index)
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // r9 = array length (header offset 0)
    emitter.instruction("test r12, r12");                                       // negative index → null
    emitter.instruction("js __rt_array_get_mixed_key_int_missing");             // warn and return null for a negative indexed-array key
    emitter.instruction("cmp r12, r9");                                         // index >= length → null
    emitter.instruction("jge __rt_array_get_mixed_key_int_missing");            // warn and return null for an out-of-bounds indexed-array key
    emitter.instruction("mov r13, QWORD PTR [rdi - 8]");                        // reload kind metadata for element type tag
    emitter.instruction("shr r13, 8");                                          // shift the element type tag into the low 7 bits
    emitter.instruction("and r13, 0x7f");                                       // mask the element type tag
    emitter.instruction("lea r10, [rdi + 24]");                                 // skip the 24-byte array header to reach the contiguous payload
    emitter.instruction("cmp r13, 7");                                          // are indexed slots already boxed Mixed pointers?
    emitter.instruction("je __rt_array_get_mixed_key_indexed_boxed");            // boxed slots must be retained before returning
    emitter.instruction("cmp r13, 1");                                          // do indexed slots contain string pointer/length pairs?
    emitter.instruction("je __rt_array_get_mixed_key_indexed_string");           // string slots need a 16-byte load before boxing
    emitter.instruction("cmp r13, 8");                                          // do indexed slots represent null payloads?
    emitter.instruction("je __rt_array_get_mixed_key_indexed_null");             // null slots have no payload to read
    emitter.instruction("mov rdi, QWORD PTR [r10 + r12 * 8]");                  // load scalar or pointer payload from the typed indexed slot
    emitter.instruction("xor rsi, rsi");                                        // typed indexed slots use one payload word except strings
    emitter.instruction("mov rax, r13");                                        // rax = runtime value_type tag for the boxed result
    emitter.instruction("call __rt_mixed_from_value");                          // box the typed indexed-array element into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    emitter.label("__rt_array_get_mixed_key_indexed_boxed");
    emitter.instruction("mov rax, QWORD PTR [r10 + r12 * 8]");                  // load the boxed Mixed pointer from the indexed slot
    emitter.instruction("test rax, rax");                                       // empty slot → null Mixed
    emitter.instruction("je __rt_array_get_mixed_key_null");                    // return null for an empty boxed slot
    emitter.instruction("call __rt_incref");                                     // retain the stored Mixed cell so the caller owns the returned result
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    emitter.label("__rt_array_get_mixed_key_indexed_string");
    emitter.instruction("shl r12, 4");                                          // convert the element index to a 16-byte string slot offset
    emitter.instruction("add r10, r12");                                        // r10 = address of the selected string slot
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // load string pointer from the selected slot
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // load string length from the selected slot
    emitter.instruction("mov rax, 1");                                          // rax = string runtime value_type tag
    emitter.instruction("call __rt_mixed_from_value");                          // box the string indexed-array element into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    emitter.label("__rt_array_get_mixed_key_indexed_null");
    emitter.instruction("mov rax, 8");                                          // rax = null runtime value_type tag
    emitter.instruction("xor rdi, rdi");                                        // value_lo = 0 for null
    emitter.instruction("xor rsi, rsi");                                        // value_hi = 0 for null
    emitter.instruction("call __rt_mixed_from_value");                          // box the null indexed-array element into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    emitter.label("__rt_array_get_mixed_key_int_missing");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the warn-on-missing flag
    emitter.instruction("test r10, r10");                                       // should this read emit undefined-key warnings?
    emitter.instruction("jz __rt_array_get_mixed_key_null");                    // silent reads skip undefined-key warnings
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the missing integer key for the PHP warning
    emitter.instruction("call __rt_warn_undefined_array_key_int");              // emit or suppress the undefined-array-key warning
    emitter.instruction("jmp __rt_array_get_mixed_key_null");                   // return boxed Mixed(null) after the warning

    // -- string key on indexed storage: PHP returns null and may warn --
    emitter.label("__rt_array_get_mixed_key_string_on_indexed");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the warn-on-missing flag
    emitter.instruction("test r10, r10");                                       // should this read emit undefined-key warnings?
    emitter.instruction("jz __rt_array_get_mixed_key_null");                    // silent reads skip undefined-key warnings
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the missing string key pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the missing string key length
    emitter.instruction("call __rt_warn_undefined_array_key_str");              // emit or suppress the undefined string-key warning
    emitter.instruction("jmp __rt_array_get_mixed_key_null");                   // return boxed Mixed(null) for a string key on indexed storage

    // -- hash storage: delegate to __rt_hash_get ---
    emitter.label("__rt_array_get_mixed_key_hash");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // rsi = key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // rdx = key_hi
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rsi=value_lo, rdx=value_hi, rcx=value_tag
    emitter.instruction("test rax, rax");                                       // miss → null
    emitter.instruction("je __rt_array_get_mixed_key_hash_missing");            // miss → optional warning + null
    emitter.instruction("cmp rcx, 7");                                          // is the hash entry already a boxed Mixed?
    emitter.instruction("jne __rt_array_get_mixed_key_hash_box");               // no → box (lo, hi, tag) into a fresh Mixed cell
    emitter.instruction("mov rax, rsi");                                        // yes → move the stored Mixed cell into the return register
    emitter.instruction("call __rt_incref");                                     // retain the stored Mixed cell so the caller owns the returned result
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
    emitter.label("__rt_array_get_mixed_key_hash_box");
    emitter.instruction("mov rax, rcx");                                        // rax = value_tag (mixed_from_value first arg)
    emitter.instruction("mov rdi, rsi");                                        // rdi = value_lo from hash_get
    emitter.instruction("mov rsi, rdx");                                        // rsi = value_hi from hash_get
    emitter.instruction("call __rt_mixed_from_value");                          // box the hash entry into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    emitter.label("__rt_array_get_mixed_key_hash_missing");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the warn-on-missing flag
    emitter.instruction("test r10, r10");                                       // should this read emit undefined-key warnings?
    emitter.instruction("jz __rt_array_get_mixed_key_null");                    // silent reads skip undefined-key warnings
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload key_hi to distinguish integer and string keys
    emitter.instruction("cmp r11, -1");                                         // check whether the missing hash key is integer-keyed
    emitter.instruction("je __rt_array_get_mixed_key_hash_missing_int");        // integer keys use the decimal warning helper
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the missing string key pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the missing string key length
    emitter.instruction("call __rt_warn_undefined_array_key_str");              // emit or suppress the undefined string-key warning
    emitter.instruction("jmp __rt_array_get_mixed_key_null");                   // return boxed Mixed(null) after the string-key warning
    emitter.label("__rt_array_get_mixed_key_hash_missing_int");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the missing integer key
    emitter.instruction("call __rt_warn_undefined_array_key_int");              // emit or suppress the undefined integer-key warning
    emitter.instruction("jmp __rt_array_get_mixed_key_null");                   // return boxed Mixed(null) after the integer-key warning

    // -- return Mixed(null) ---
    emitter.label("__rt_array_get_mixed_key_null");
    emitter.instruction("mov rax, 8");                                          // rax = null runtime value_type tag
    emitter.instruction("xor rdi, rdi");                                        // value_lo = 0 for null
    emitter.instruction("xor rsi, rsi");                                        // value_hi = 0 for null
    emitter.instruction("call __rt_mixed_from_value");                          // box null into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
}
