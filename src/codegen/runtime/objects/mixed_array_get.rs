//! Purpose:
//! Emits the `__rt_mixed_array_get` runtime helper for `$mixed[$key]` access.
//! Routes boxed JSON-style values to indexed-array, hash, or stdClass lookup paths.
//!
//! Called from:
//! - `crate::codegen::runtime::objects::emit_mixed_array_get()`.
//!
//! Key details:
//! - The key tuple matches `emit_normalized_hash_key`: int keys use `key_hi = -1`.
//! - Unsupported payloads and missing keys return boxed `Mixed(null)` for PHP-like quiet access.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Dispatches to the target-specific `__rt_mixed_array_get` emitter.
///
/// Checks `emitter.target.arch` and routes to either `emit_mixed_array_get_x86_64`
/// (SysV ABI) or `emit_mixed_array_get_aarch64` (AAPCS64). The helper is emitted
/// once into the runtime object and is called by generated code for `$mixed[$key]`
/// access on a boxed `Mixed` value.
pub fn emit_mixed_array_get(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_array_get_x86_64(emitter);
        return;
    }
    emit_mixed_array_get_aarch64(emitter);
}

/// Emits `__rt_mixed_array_get` for ARM64 (AAPCS64 ABI).
///
/// Inputs arrive in `x0` = mixed_ptr, `x1` = key_lo, `x2` = key_hi.
/// Returns a pointer to a boxed `Mixed` cell in `x0`.
///
/// The function dispatches on the mixed value's tag:
/// - Tag 4 → indexed array path
/// - Tag 5 → associative array path
/// - Tag 6 → stdClass object path
/// - All others → null (boxed `Mixed(null)`)
///
/// For indexed arrays the key must be integer (`key_hi == -1` sentinel); string keys
/// return null. For objects only `stdClass` with a string key is supported; int keys
/// and non-stdClass objects return null. Missing keys return null. All paths that
/// produce a value box it through `__rt_mixed_from_value` except when the hash entry
/// already holds a boxed `Mixed` pointer (tag 7), which is returned directly.
fn emit_mixed_array_get_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_get ---");
    emitter.label_global("__rt_mixed_array_get");

    // Stack:
    //   [sp, #0]  = mixed_ptr
    //   [sp, #8]  = key_lo
    //   [sp, #16] = key_hi
    //   [sp, #24] = saved x29
    //   [sp, #32] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // reserve frame: 3 inputs + saved fp/lr (16-byte aligned)
    emitter.instruction("stp x29, x30, [sp, #24]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #24");                                    // set new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save mixed_ptr
    emitter.instruction("str x1, [sp, #8]");                                    // save key_lo
    emitter.instruction("str x2, [sp, #16]");                                   // save key_hi

    emitter.instruction("cbz x0, __rt_mixed_array_get_null");                   // null Mixed → Mixed(null)
    emitter.instruction("ldr x9, [x0]");                                        // load tag from mixed[0]
    emitter.instruction("cmp x9, #4");                                          // tag = 4 (indexed array)?
    emitter.instruction("b.eq __rt_mixed_array_get_indexed");                   // branch on the current JSON decoder condition
    emitter.instruction("cmp x9, #5");                                          // tag = 5 (associative array)?
    emitter.instruction("b.eq __rt_mixed_array_get_assoc");                     // branch on the current JSON decoder condition
    emitter.instruction("cmp x9, #6");                                          // tag = 6 (object)?
    emitter.instruction("b.eq __rt_mixed_array_get_object");                    // branch on the current JSON decoder condition
    emitter.instruction("b __rt_mixed_array_get_null");                         // any other payload → null

    // Indexed array: integer key only. key_hi == -1 marks int keys.
    emitter.label("__rt_mixed_array_get_indexed");
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = array pointer
    emitter.instruction("cbz x10, __rt_mixed_array_get_null");                  // defensive null guard
    emitter.instruction("ldr x11, [sp, #16]");                                  // load key_hi
    emitter.instruction("cmn x11, #1");                                         // compare with -1 (int-key sentinel)
    emitter.instruction("b.ne __rt_mixed_array_get_null");                      // string keys on indexed arrays → null
    emitter.instruction("ldr x12, [sp, #8]");                                   // x12 = key_lo (int index)
    emitter.instruction("ldr x9, [x10]");                                       // x9 = array length (header offset 0)
    emitter.instruction("cmp x12, #0");                                         // negative index → null
    emitter.instruction("b.lt __rt_mixed_array_get_indexed_missing");           // warn and return null for a negative indexed-array key
    emitter.instruction("cmp x12, x9");                                         // index >= length → null
    emitter.instruction("b.ge __rt_mixed_array_get_indexed_missing");           // warn and return null for an out-of-bounds indexed-array key
    emitter.instruction("ldr x13, [x10, #-8]");                                 // load packed indexed-array kind metadata
    emitter.instruction("ubfx x13, x13, #8, #7");                               // extract the runtime element value_type tag
    emitter.instruction("add x10, x10, #24");                                   // skip the 24-byte array header to reach the contiguous payload
    emitter.instruction("cmp x13, #7");                                         // are indexed slots already boxed Mixed pointers?
    emitter.instruction("b.eq __rt_mixed_array_get_indexed_boxed");             // boxed slots can be returned directly
    emitter.instruction("cmp x13, #1");                                         // do indexed slots contain string pointer/length pairs?
    emitter.instruction("b.eq __rt_mixed_array_get_indexed_string");            // string slots need a 16-byte load before boxing
    emitter.instruction("cmp x13, #8");                                         // do indexed slots represent null payloads?
    emitter.instruction("b.eq __rt_mixed_array_get_indexed_null");              // null slots have no payload to read
    emitter.instruction("ldr x1, [x10, x12, lsl #3]");                          // load scalar or pointer payload from the typed indexed slot
    emitter.instruction("mov x2, #0");                                          // typed indexed slots use one payload word except strings
    emitter.instruction("mov x0, x13");                                         // x0 = runtime value_type tag for the boxed result
    emitter.instruction("bl __rt_mixed_from_value");                            // box the typed indexed-array element into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0
    emitter.label("__rt_mixed_array_get_indexed_boxed");
    emitter.instruction("ldr x0, [x10, x12, lsl #3]");                          // load the boxed Mixed pointer from the indexed slot
    emitter.instruction("cbz x0, __rt_mixed_array_get_null");                   // empty slot → null Mixed
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0
    emitter.label("__rt_mixed_array_get_indexed_string");
    emitter.instruction("lsl x12, x12, #4");                                    // convert the element index to a 16-byte string slot offset
    emitter.instruction("add x10, x10, x12");                                   // x10 = address of the selected string slot
    emitter.instruction("ldr x1, [x10]");                                       // load string pointer from the selected slot
    emitter.instruction("ldr x2, [x10, #8]");                                   // load string length from the selected slot
    emitter.instruction("mov x0, #1");                                          // x0 = string runtime value_type tag
    emitter.instruction("bl __rt_mixed_from_value");                            // box the string indexed-array element into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0
    emitter.label("__rt_mixed_array_get_indexed_null");
    emitter.instruction("mov x0, #8");                                          // x0 = null runtime value_type tag
    emitter.instruction("mov x1, #0");                                          // value_lo = 0 for null
    emitter.instruction("mov x2, #0");                                          // value_hi = 0 for null
    emitter.instruction("bl __rt_mixed_from_value");                            // box the null indexed-array element into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0
    emitter.label("__rt_mixed_array_get_indexed_missing");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the missing integer key for the PHP warning
    emitter.instruction("bl __rt_warn_undefined_array_key_int");                // emit or suppress the undefined-array-key warning
    emitter.instruction("b __rt_mixed_array_get_null");                         // return boxed Mixed(null) after the warning

    // Associative array: hash_get with normalized key.
    emitter.label("__rt_mixed_array_get_assoc");
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = hash pointer
    emitter.instruction("cbz x10, __rt_mixed_array_get_null");                  // defensive null guard
    emitter.instruction("mov x0, x10");                                         // x0 = hash pointer for hash_get
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = key_lo
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = key_hi
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=value_lo, x2=value_hi, x3=value_tag
    emitter.instruction("cbz x0, __rt_mixed_array_get_null");                   // miss → null
    // For value_tag == 7 the entry already holds a boxed Mixed pointer
    // (json_decode and stdClass populate hashes this way). Anything else
    // (typed string/int/array entries from non-Mixed assoc arrays passing
    // through a Mixed receiver) needs to be re-boxed via mixed_from_value
    // so callers always see a uniform Mixed cell.
    emitter.instruction("cmp x3, #7");                                          // is the hash entry already a boxed Mixed?
    emitter.instruction("b.ne __rt_mixed_array_get_assoc_box");                 // no → box (lo, hi, tag) into a fresh Mixed cell
    emitter.instruction("mov x0, x1");                                          // yes → return the borrowed Mixed* directly
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0
    emitter.label("__rt_mixed_array_get_assoc_box");
    // mixed_from_value(tag, lo, hi). Move (x1, x2, x3) into (x1, x2, x0).
    emitter.instruction("mov x0, x3");                                          // x0 = value_tag (mixed_from_value first arg)
    // x1 already holds value_lo; x2 already holds value_hi.
    emitter.instruction("bl __rt_mixed_from_value");                            // box the typed entry into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0

    // Object: only stdClass with string key.
    emitter.label("__rt_mixed_array_get_object");
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = obj pointer
    emitter.instruction("cbz x10, __rt_mixed_array_get_null");                  // defensive null guard
    emitter.instruction("ldr x11, [x10]");                                      // x11 = class_id
    abi::emit_symbol_address(emitter, "x12", "_stdclass_class_id");
    emitter.instruction("ldr x12, [x12]");                                      // x12 = compile-time stdClass class_id
    emitter.instruction("cmp x11, x12");                                        // is the receiver a stdClass instance?
    emitter.instruction("b.ne __rt_mixed_array_get_null");                      // unrelated class → null
    emitter.instruction("ldr x11, [sp, #16]");                                  // load key_hi
    emitter.instruction("cmn x11, #1");                                         // compare with -1 (int-key sentinel)
    emitter.instruction("b.eq __rt_mixed_array_get_null");                      // int keys on objects → null
    emitter.instruction("mov x0, x10");                                         // x0 = stdClass pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = key_lo (str ptr)
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = key_hi (str len)
    emitter.instruction("bl __rt_stdclass_get");                                // delegate to the dynamic-property reader
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0

    emitter.label("__rt_mixed_array_get_null");
    emitter.instruction("mov x0, #8");                                          // tag = 8 (null)
    emitter.instruction("mov x1, #0");                                          // value_lo = 0
    emitter.instruction("mov x2, #0");                                          // value_hi = 0
    emitter.instruction("bl __rt_mixed_from_value");                            // box null into a fresh Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #24]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0
}

/// Emits `__rt_mixed_array_get` for x86_64 (SysV ABI).
///
/// Inputs arrive in `rdi` = mixed_ptr, `rsi` = key_lo, `rdx` = key_hi.
/// Returns a pointer to a boxed `Mixed` cell in `rax`.
///
/// Same dispatch and return semantics as `emit_mixed_array_get_aarch64`:
/// - Tag 4 → indexed array, tag 5 → associative array, tag 6 → stdClass object
/// - Integer keys on indexed arrays required (`key_hi == -1`); string keys return null
/// - Objects: only `stdClass` with string key supported; int keys return null
/// - Missing keys and unsupported payloads return boxed `Mixed(null)`
/// - Hash entries already holding a boxed `Mixed` (tag 7) are returned directly;
///   all other values are boxed through `__rt_mixed_from_value`
fn emit_mixed_array_get_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_get ---");
    emitter.label_global("__rt_mixed_array_get");

    // Inputs (SysV): rdi = mixed_ptr, rsi = key_lo, rdx = key_hi.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve slots for the 3 saved inputs (16-byte aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save mixed_ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save key_lo
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save key_hi

    emitter.instruction("test rdi, rdi");                                       // null Mixed → null
    emitter.instruction("je __rt_mixed_array_get_null");                        // branch on the current JSON decoder condition
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load tag from mixed[0]
    emitter.instruction("cmp r10, 4");                                          // tag = 4 (indexed array)?
    emitter.instruction("je __rt_mixed_array_get_indexed");                     // branch on the current JSON decoder condition
    emitter.instruction("cmp r10, 5");                                          // tag = 5 (associative array)?
    emitter.instruction("je __rt_mixed_array_get_assoc");                       // branch on the current JSON decoder condition
    emitter.instruction("cmp r10, 6");                                          // tag = 6 (object)?
    emitter.instruction("je __rt_mixed_array_get_object");                      // branch on the current JSON decoder condition
    emitter.instruction("jmp __rt_mixed_array_get_null");                       // any other payload → null

    emitter.label("__rt_mixed_array_get_indexed");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // r10 = array pointer
    emitter.instruction("test r10, r10");                                       // defensive null guard
    emitter.instruction("je __rt_mixed_array_get_null");                        // branch on the current JSON decoder condition
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // load key_hi
    emitter.instruction("cmp r11, -1");                                         // int-key sentinel?
    emitter.instruction("jne __rt_mixed_array_get_null");                       // string key on indexed array → null
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // r8 = key_lo (int index)
    emitter.instruction("mov r9, QWORD PTR [r10]");                             // r9 = array length
    emitter.instruction("cmp r8, 0");                                           // negative index → null
    emitter.instruction("jl __rt_mixed_array_get_indexed_missing");             // warn and return null for a negative indexed-array key
    emitter.instruction("cmp r8, r9");                                          // index >= length → null
    emitter.instruction("jge __rt_mixed_array_get_indexed_missing");            // warn and return null for an out-of-bounds indexed-array key
    emitter.instruction("mov r9, QWORD PTR [r10 - 8]");                         // load packed indexed-array kind metadata
    emitter.instruction("shr r9, 8");                                           // shift the runtime element value_type tag into the low bits
    emitter.instruction("and r9, 0x7f");                                        // remove the persistent COW flag from the extracted tag
    emitter.instruction("lea r10, [r10 + 24]");                                 // skip the 24-byte array header to reach the contiguous payload
    emitter.instruction("cmp r9, 7");                                           // are indexed slots already boxed Mixed pointers?
    emitter.instruction("je __rt_mixed_array_get_indexed_boxed");               // boxed slots can be returned directly
    emitter.instruction("cmp r9, 1");                                           // do indexed slots contain string pointer/length pairs?
    emitter.instruction("je __rt_mixed_array_get_indexed_string");              // string slots need a 16-byte load before boxing
    emitter.instruction("cmp r9, 8");                                           // do indexed slots represent null payloads?
    emitter.instruction("je __rt_mixed_array_get_indexed_null");                // null slots have no payload to read
    emitter.instruction("mov rax, r9");                                         // rax = runtime value_type tag for mixed_from_value
    emitter.instruction("mov rdi, QWORD PTR [r10 + r8 * 8]");                   // rdi = scalar or pointer payload from the typed indexed slot
    emitter.instruction("xor esi, esi");                                        // typed indexed slots use one payload word except strings
    emitter.instruction("call __rt_mixed_from_value");                          // box the typed indexed-array element into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
    emitter.label("__rt_mixed_array_get_indexed_boxed");
    emitter.instruction("mov rax, QWORD PTR [r10 + r8 * 8]");                   // load the boxed Mixed pointer from the indexed slot
    emitter.instruction("test rax, rax");                                       // empty slot → null
    emitter.instruction("je __rt_mixed_array_get_null");                        // branch on the current JSON decoder condition
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
    emitter.label("__rt_mixed_array_get_indexed_string");
    emitter.instruction("shl r8, 4");                                           // convert the element index to a 16-byte string slot offset
    emitter.instruction("add r10, r8");                                         // r10 = address of the selected string slot
    emitter.instruction("mov rax, 1");                                          // rax = string runtime value_type tag
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // rdi = selected string pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // rsi = selected string length
    emitter.instruction("call __rt_mixed_from_value");                          // box the string indexed-array element into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
    emitter.label("__rt_mixed_array_get_indexed_null");
    emitter.instruction("mov rax, 8");                                          // rax = null runtime value_type tag
    emitter.instruction("mov rdi, 0");                                          // value_lo = 0 for null
    emitter.instruction("mov rsi, 0");                                          // value_hi = 0 for null
    emitter.instruction("call __rt_mixed_from_value");                          // box the null indexed-array element into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
    emitter.label("__rt_mixed_array_get_indexed_missing");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the missing integer key for the PHP warning
    emitter.instruction("call __rt_warn_undefined_array_key_int");              // emit or suppress the undefined-array-key warning
    emitter.instruction("jmp __rt_mixed_array_get_null");                       // return boxed Mixed(null) after the warning

    emitter.label("__rt_mixed_array_get_assoc");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // r10 = hash pointer
    emitter.instruction("test r10, r10");                                       // defensive null guard
    emitter.instruction("je __rt_mixed_array_get_null");                        // branch on the current JSON decoder condition
    emitter.instruction("mov rdi, r10");                                        // rdi = hash pointer for hash_get
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // rsi = key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // rdx = key_hi
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=value_lo, rsi=value_hi, rcx=value_tag
    emitter.instruction("test rax, rax");                                       // miss → null
    emitter.instruction("je __rt_mixed_array_get_null");                        // branch on the current JSON decoder condition
    // For value_tag == 7 the entry is already a boxed Mixed pointer; for
    // any other tag (typed string/int/array entries from non-Mixed assoc
    // arrays passing through a Mixed receiver) re-box (lo, hi, tag) so
    // callers always see a uniform Mixed cell.
    emitter.instruction("cmp rcx, 7");                                          // is the hash entry already a boxed Mixed?
    emitter.instruction("jne __rt_mixed_array_get_assoc_box");                  // no → box (lo, hi, tag) into a fresh Mixed cell
    emitter.instruction("mov rax, rdi");                                        // yes → return the borrowed Mixed* directly
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
    emitter.label("__rt_mixed_array_get_assoc_box");
    // mixed_from_value(tag, lo, hi). Helper expects rax=tag, rdi=lo, rsi=hi.
    emitter.instruction("mov rax, rcx");                                        // rax = value_tag
    // rdi and rsi already hold value_lo and value_hi.
    emitter.instruction("call __rt_mixed_from_value");                          // box the typed entry into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    emitter.label("__rt_mixed_array_get_object");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // r10 = obj pointer
    emitter.instruction("test r10, r10");                                       // defensive null guard
    emitter.instruction("je __rt_mixed_array_get_null");                        // branch on the current JSON decoder condition
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // r11 = class_id
    emitter.instruction("mov r12, QWORD PTR [rip + _stdclass_class_id]");       // r12 = compile-time stdClass class_id
    emitter.instruction("cmp r11, r12");                                        // is the receiver a stdClass instance?
    emitter.instruction("jne __rt_mixed_array_get_null");                       // unrelated class → null
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // load key_hi
    emitter.instruction("cmp r11, -1");                                         // int-key sentinel?
    emitter.instruction("je __rt_mixed_array_get_null");                        // int key on object → null
    emitter.instruction("mov rdi, r10");                                        // rdi = stdClass pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // rsi = key_lo (str ptr)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // rdx = key_hi (str len)
    emitter.instruction("call __rt_stdclass_get");                              // delegate to the dynamic-property reader
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    emitter.label("__rt_mixed_array_get_null");
    emitter.instruction("mov rax, 8");                                          // tag = 8 (null) for mixed_from_value
    emitter.instruction("mov rdi, 0");                                          // value_lo = 0
    emitter.instruction("mov rsi, 0");                                          // value_hi = 0
    emitter.instruction("call __rt_mixed_from_value");                          // box null into a fresh Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
}
