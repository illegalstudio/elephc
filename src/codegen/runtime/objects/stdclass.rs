//! Runtime helpers for the built-in `stdClass`.
//!
//! `stdClass` instances are 16-byte heap allocations laid out as
//! `[class_id:8][hash_ptr:8]`. The hash table at offset 8 holds dynamic
//! property name → boxed-`Mixed` mappings (hash value type 7) and is allocated
//! lazily on the first property write so empty instances stay cheap.
//!
//! User code reaches these helpers through the codegen lowering of
//! `$obj->name` and `$obj->name = $val` once the receiver type resolves to
//! `Object("stdClass")`. The decoder produces stdClass instances directly
//! through `__rt_stdclass_from_hash` when `json_decode($json, false)` is the
//! caller-visible default.
//!
//! # Calling convention
//!
//! ARM64 (AArch64) follows the standard PCS register order used elsewhere
//! in this runtime. x86_64 uses the SysV convention so codegen can prepare
//! the same set of `mov` sequences it already uses for `__rt_hash_set` and
//! the other multi-argument array helpers. The codegen lowering for
//! stdClass property access therefore copies the receiver from the result
//! register (`rax`) into `rdi` before issuing the call on x86_64; on ARM64
//! the receiver is already in `x0`.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emit `__rt_stdclass_new() → obj_ptr` for both targets.
///
/// Allocates a fresh stdClass instance: 16-byte heap block, object heap
/// kind, class_id from `_stdclass_class_id`, and an empty 8-slot hash for
/// property storage. The hash is allocated eagerly so subsequent property
/// writes can call `__rt_hash_set` without a null check.
pub fn emit_stdclass_new(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stdclass_new_x86_64(emitter);
        return;
    }
    emit_stdclass_new_aarch64(emitter);
}

/// Emit `__rt_stdclass_from_hash(hash_ptr) → obj_ptr` for both targets.
///
/// Used by the json_decode object path when `$associative` is false (the
/// PHP default): we already built a hash of decoded entries during recursive
/// descent, so wrap that hash in a stdClass instance instead of allocating a
/// fresh one and copying entries.
pub fn emit_stdclass_from_hash(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stdclass_from_hash_x86_64(emitter);
        return;
    }
    emit_stdclass_from_hash_aarch64(emitter);
}

/// Emit `__rt_stdclass_get(obj, name_ptr, name_len) → Mixed*`.
///
/// Looks up the named property in the stdClass's internal hash. If the hash
/// is empty or the property is missing, returns a freshly boxed null Mixed
/// so callers can treat the result uniformly.
pub fn emit_stdclass_get(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stdclass_get_x86_64(emitter);
        return;
    }
    emit_stdclass_get_aarch64(emitter);
}

/// Emit `__rt_json_encode_stdclass(hash_ptr) → string`.
///
/// PHP's `json_encode($obj)` produces `{}` for an empty stdClass even though
/// the underlying hash is empty (the assoc encoder defaults empty hashes to
/// `[]`). This wrapper inspects the hash count: empty → emit the literal
/// `{}` into the concat buffer, non-empty → tail-call the standard assoc
/// encoder, which already handles the string-keyed case as `{...}`.
pub fn emit_json_encode_stdclass(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_stdclass_x86_64(emitter);
        return;
    }
    emit_json_encode_stdclass_aarch64(emitter);
}

/// Emit `__rt_mixed_property_get(mixed_ptr, name_ptr, name_len) → Mixed*`.
///
/// Read a property name on a `Mixed` receiver. The most common entry point is
/// `$obj->name` after `$obj = json_decode(...)`: PHP returns stdClass by
/// default and the result lives in a Mixed cell. This helper unboxes the
/// cell, validates that it carries a stdClass instance, and forwards to
/// `__rt_stdclass_get`. Non-object payloads or unrelated classes return a
/// fresh Mixed(null) so callers can treat the result uniformly.
pub fn emit_mixed_property_get(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_property_get_x86_64(emitter);
        return;
    }
    emit_mixed_property_get_aarch64(emitter);
}

/// Emit `__rt_mixed_property_set(mixed_ptr, name_ptr, name_len, value_mixed_ptr)`.
///
/// Companion to `__rt_mixed_property_get`: writes a value into a stdClass
/// instance reached through a Mixed receiver. Non-stdClass payloads silently
/// drop the write (mirroring PHP's "attempt to assign property on non-object"
/// warning behaviour for the most common idiom).
pub fn emit_mixed_property_set(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_property_set_x86_64(emitter);
        return;
    }
    emit_mixed_property_set_aarch64(emitter);
}

/// Emit `__rt_stdclass_set(obj, name_ptr, name_len, mixed_ptr)`.
///
/// Stores an already-boxed Mixed pointer in the stdClass's hash under the
/// given key. The codegen lowering boxes the user's value into a Mixed cell
/// before the call (via the existing `emit_box_current_value_as_mixed`
/// helper), so this routine only handles lazy hash allocation and the
/// `__rt_hash_set` insertion.
pub fn emit_stdclass_set(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stdclass_set_x86_64(emitter);
        return;
    }
    emit_stdclass_set_aarch64(emitter);
}

// AArch64 -----------------------------------------------------------------

fn emit_json_encode_stdclass_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_stdclass ---");
    emitter.label_global("__rt_json_encode_stdclass");

    // x0 = hash pointer. The hash count lives at offset 0 in the header.
    emitter.instruction("ldr x9, [x0]");                                        // x9 = entry count
    emitter.instruction("cbz x9, __rt_json_encode_stdclass_empty");             // empty hash → emit "{}" directly
    emitter.instruction("b __rt_json_encode_assoc");                            // non-empty stdClass → defer to the assoc encoder, which emits {"k":v,...}

    emitter.label("__rt_json_encode_stdclass_empty");
    // Emit the literal "{}" into _concat_buf at the current offset and
    // return (ptr, len) for the active string-result ABI.
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // x10 = current concat-buffer offset
    abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // x12 = output write pointer for this encoder call
    emitter.instruction("mov w13, #123");                                       // ASCII '{'
    emitter.instruction("strb w13, [x12]");                                     // write '{' at the output position
    emitter.instruction("mov w13, #125");                                       // ASCII '}'
    emitter.instruction("strb w13, [x12, #1]");                                 // write '}' immediately after '{'
    emitter.instruction("add x10, x10, #2");                                    // advance the concat-buffer offset by the two emitted bytes
    emitter.instruction("str x10, [x9]");                                       // persist the new offset for any subsequent encoder
    emitter.instruction("mov x1, x12");                                         // result string pointer
    emitter.instruction("mov x2, #2");                                          // result string length
    emitter.instruction("ret");                                                 // return (ptr, len) to the caller via the ABI string registers
}

fn emit_stdclass_new_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stdclass_new ---");
    emitter.label_global("__rt_stdclass_new");

    emitter.instruction("sub sp, sp, #32");                                     // reserve 32 bytes: obj slot + saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set new frame pointer

    emitter.instruction("mov x0, #16");                                         // payload size = class_id + hash_ptr
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = obj pointer (8-byte aligned)
    emitter.instruction("mov x9, #4");                                          // heap kind 4 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the heap header with the object kind
    emitter.instruction("str x0, [sp, #0]");                                    // park the obj pointer while we initialize fields

    abi::emit_symbol_address(emitter, "x9", "_stdclass_class_id");
    emitter.instruction("ldr x10, [x9]");                                       // load the compile-time stdClass class_id
    emitter.instruction("str x10, [x0]");                                       // store class_id at obj+0

    emitter.instruction("mov x0, #8");                                          // initial capacity = 8 slots
    emitter.instruction("mov x1, #7");                                          // value_type = 7 (boxed Mixed)
    emitter.instruction("bl __rt_hash_new");                                    // x0 = empty hash pointer

    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the saved obj pointer
    emitter.instruction("str x0, [x9, #8]");                                    // store hash_ptr at obj+8
    emitter.instruction("mov x0, x9");                                          // return value = obj pointer

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the local frame
    emitter.instruction("ret");                                                 // return obj pointer in x0
}

fn emit_stdclass_from_hash_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stdclass_from_hash ---");
    emitter.label_global("__rt_stdclass_from_hash");

    emitter.instruction("sub sp, sp, #32");                                     // reserve 32 bytes: hash slot + saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the incoming hash pointer for later

    emitter.instruction("mov x0, #16");                                         // payload size = class_id + hash_ptr
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = obj pointer
    emitter.instruction("mov x9, #4");                                          // heap kind 4 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the heap header with the object kind

    abi::emit_symbol_address(emitter, "x9", "_stdclass_class_id");
    emitter.instruction("ldr x10, [x9]");                                       // load the compile-time stdClass class_id
    emitter.instruction("str x10, [x0]");                                       // store class_id at obj+0

    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the incoming hash pointer
    emitter.instruction("str x9, [x0, #8]");                                    // store hash_ptr at obj+8

    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the local frame
    emitter.instruction("ret");                                                 // return obj pointer in x0
}

fn emit_stdclass_get_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stdclass_get ---");
    emitter.label_global("__rt_stdclass_get");

    emitter.instruction("sub sp, sp, #48");                                     // frame: obj + name_ptr + name_len + saved fp/lr + slack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save obj
    emitter.instruction("str x1, [sp, #8]");                                    // save name_ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save name_len

    emitter.instruction("ldr x9, [x0, #8]");                                    // load hash_ptr from obj+8
    emitter.instruction("cbz x9, __rt_stdclass_get_null");                      // empty hash → return Mixed(null)

    emitter.instruction("mov x0, x9");                                          // x0 = hash pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = name_ptr (key_lo)
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = name_len (key_hi)
    emitter.instruction("bl __rt_hash_get");                                    // x0=found, x1=value_lo, x2=value_hi, x3=value_tag

    emitter.instruction("cbz x0, __rt_stdclass_get_null");                      // not found → Mixed(null)
    emitter.instruction("mov x0, x1");                                          // hit: stored value is the boxed Mixed pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0

    emitter.label("__rt_stdclass_get_null");
    emitter.instruction("mov x0, #8");                                          // tag = 8 (null)
    emitter.instruction("mov x1, #0");                                          // payload lo = 0
    emitter.instruction("mov x2, #0");                                          // payload hi = 0
    emitter.instruction("bl __rt_mixed_from_value");                            // box null into a Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0
}

fn emit_stdclass_set_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stdclass_set ---");
    emitter.label_global("__rt_stdclass_set");

    // Stack:
    //   [sp, #0]  = obj
    //   [sp, #8]  = name_ptr
    //   [sp, #16] = name_len
    //   [sp, #24] = mixed_ptr
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // reserve frame: 4 inputs + saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save obj
    emitter.instruction("str x1, [sp, #8]");                                    // save name_ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save name_len
    emitter.instruction("str x3, [sp, #24]");                                   // save mixed_ptr

    emitter.instruction("ldr x9, [x0, #8]");                                    // load current hash_ptr from obj+8
    emitter.instruction("cbnz x9, __rt_stdclass_set_have_hash");                // already allocated → skip lazy init

    emitter.instruction("mov x0, #8");                                          // initial capacity = 8 slots
    emitter.instruction("mov x1, #7");                                          // value_type = 7 (boxed Mixed)
    emitter.instruction("bl __rt_hash_new");                                    // x0 = empty hash pointer
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload obj
    emitter.instruction("str x0, [x10, #8]");                                   // store the new hash_ptr at obj+8

    emitter.label("__rt_stdclass_set_have_hash");

    emitter.instruction("ldr x10, [sp, #0]");                                   // reload obj
    emitter.instruction("ldr x0, [x10, #8]");                                   // x0 = current hash_ptr
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = name_ptr (key_lo)
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = name_len (key_hi)
    emitter.instruction("ldr x3, [sp, #24]");                                   // x3 = value_lo (Mixed pointer)
    emitter.instruction("mov x4, #0");                                          // value_hi (unused for boxed Mixed)
    emitter.instruction("mov x5, #7");                                          // value_tag = 7 (boxed Mixed)
    emitter.instruction("bl __rt_hash_set");                                    // x0 = updated hash pointer

    emitter.instruction("ldr x10, [sp, #0]");                                   // reload obj
    emitter.instruction("str x0, [x10, #8]");                                   // update obj+8 with the latest hash pointer

    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return (no return value)
}

fn emit_mixed_property_get_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_property_get ---");
    emitter.label_global("__rt_mixed_property_get");

    // Inputs: x0 = mixed_ptr, x1 = name_ptr, x2 = name_len. Output: x0 = Mixed*.
    emitter.instruction("sub sp, sp, #48");                                     // reserve frame: mixed + name_ptr + name_len + saved fp/lr + slack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save mixed_ptr
    emitter.instruction("str x1, [sp, #8]");                                    // save name_ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save name_len

    emitter.instruction("cbz x0, __rt_mixed_property_get_null");                // null Mixed → null result
    emitter.instruction("ldr x9, [x0]");                                        // load tag from mixed[0]
    emitter.instruction("cmp x9, #6");                                          // tag = 6 (object)?
    emitter.instruction("b.ne __rt_mixed_property_get_null");                   // non-object payload → null result
    emitter.instruction("ldr x9, [x0, #8]");                                    // load obj pointer from mixed[8]
    emitter.instruction("cbz x9, __rt_mixed_property_get_null");                // null obj → null result
    emitter.instruction("ldr x10, [x9]");                                       // load class_id from obj[0]
    abi::emit_symbol_address(emitter, "x11", "_stdclass_class_id");
    emitter.instruction("ldr x11, [x11]");                                      // load the compile-time stdClass class_id sentinel
    emitter.instruction("cmp x10, x11");                                        // is the receiver a stdClass instance?
    emitter.instruction("b.ne __rt_mixed_property_get_null");                   // unrelated class → null result

    emitter.instruction("mov x0, x9");                                          // x0 = stdClass obj pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = name_ptr
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = name_len
    emitter.instruction("bl __rt_stdclass_get");                                // delegate to the dynamic-property reader
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0

    emitter.label("__rt_mixed_property_get_null");
    emitter.instruction("mov x0, #8");                                          // tag = 8 (null)
    emitter.instruction("mov x1, #0");                                          // value_lo = 0
    emitter.instruction("mov x2, #0");                                          // value_hi = 0
    emitter.instruction("bl __rt_mixed_from_value");                            // box null into a fresh Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return Mixed* in x0
}

fn emit_mixed_property_set_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_property_set ---");
    emitter.label_global("__rt_mixed_property_set");

    // Inputs: x0 = mixed_ptr, x1 = name_ptr, x2 = name_len, x3 = value_mixed_ptr.
    emitter.instruction("sub sp, sp, #48");                                     // reserve frame: 4 inputs + saved fp/lr
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save mixed_ptr
    emitter.instruction("str x1, [sp, #8]");                                    // save name_ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save name_len
    emitter.instruction("str x3, [sp, #24]");                                   // save value_mixed_ptr

    emitter.instruction("cbz x0, __rt_mixed_property_set_done");                // null Mixed → silently drop the write
    emitter.instruction("ldr x9, [x0]");                                        // load tag from mixed[0]
    emitter.instruction("cmp x9, #6");                                          // tag = 6 (object)?
    emitter.instruction("b.ne __rt_mixed_property_set_done");                   // non-object payload → silently drop the write
    emitter.instruction("ldr x9, [x0, #8]");                                    // load obj pointer from mixed[8]
    emitter.instruction("cbz x9, __rt_mixed_property_set_done");                // null obj → silently drop the write
    emitter.instruction("ldr x10, [x9]");                                       // load class_id from obj[0]
    abi::emit_symbol_address(emitter, "x11", "_stdclass_class_id");
    emitter.instruction("ldr x11, [x11]");                                      // load the compile-time stdClass class_id sentinel
    emitter.instruction("cmp x10, x11");                                        // is the receiver a stdClass instance?
    emitter.instruction("b.ne __rt_mixed_property_set_done");                   // unrelated class → silently drop the write

    emitter.instruction("mov x0, x9");                                          // x0 = stdClass obj pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = name_ptr
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = name_len
    emitter.instruction("ldr x3, [sp, #24]");                                   // x3 = value_mixed_ptr
    emitter.instruction("bl __rt_stdclass_set");                                // delegate to the dynamic-property writer

    emitter.label("__rt_mixed_property_set_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the local frame
    emitter.instruction("ret");                                                 // return (no return value)
}

// x86_64 ------------------------------------------------------------------

fn emit_json_encode_stdclass_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_stdclass ---");
    emitter.label_global("__rt_json_encode_stdclass");

    // rax = hash pointer (string-result ABI on x86_64 in this codebase).
    // The hash count lives at offset 0 in the header.
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // r10 = entry count
    emitter.instruction("test r10, r10");                                       // empty hash?
    emitter.instruction("jne __rt_json_encode_assoc");                          // non-empty → defer to the assoc encoder

    // Empty hash: emit "{}" into _concat_buf and return (rax, rdx).
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // r10 = current concat-buffer offset
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // r11 = base of the concat buffer
    emitter.instruction("add r11, r10");                                        // r11 = output write pointer for this encoder call
    emitter.instruction("mov BYTE PTR [r11], 123");                             // write '{' at the output position
    emitter.instruction("mov BYTE PTR [r11 + 1], 125");                         // write '}' immediately after '{'
    emitter.instruction("add r10, 2");                                          // advance the concat-buffer offset by the two emitted bytes
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r10");              // persist the new offset for any subsequent encoder
    emitter.instruction("mov rax, r11");                                        // result string pointer in the leading x86_64 string register
    emitter.instruction("mov rdx, 2");                                          // result string length in the paired x86_64 string register
    emitter.instruction("ret");                                                 // return (ptr, len) to the caller via the ABI string registers
}

fn emit_stdclass_new_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stdclass_new ---");
    emitter.label_global("__rt_stdclass_new");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 16");                                         // reserve a slot for the obj pointer

    emitter.instruction("mov rax, 16");                                         // payload size = class_id + hash_ptr
    emitter.instruction("call __rt_heap_alloc");                                // rax = obj pointer
    emitter.instruction("mov r10, 0x454C504800000004");                         // x86_64 heap header word: ELPH marker | object kind 4
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the heap header with the object kind
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // park the obj pointer in the local slot

    emitter.instruction("mov r10, QWORD PTR [rip + _stdclass_class_id]");       // load the compile-time stdClass class_id
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store class_id at obj+0

    emitter.instruction("mov rax, 8");                                          // initial capacity = 8 slots (mixed_from_value first arg)
    emitter.instruction("mov rdi, 7");                                          // value_type = 7 (boxed Mixed)
    emitter.instruction("call __rt_hash_new");                                  // rax = empty hash pointer

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the saved obj pointer
    emitter.instruction("mov QWORD PTR [rdi + 8], rax");                        // store hash_ptr at obj+8
    emitter.instruction("mov rax, rdi");                                        // return value = obj pointer

    emitter.instruction("mov rsp, rbp");                                        // restore the stack pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return obj pointer in rax
}

fn emit_stdclass_from_hash_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stdclass_from_hash ---");
    emitter.label_global("__rt_stdclass_from_hash");

    // Inputs: rdi = hash_ptr.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 16");                                         // reserve a slot for the saved hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the incoming hash pointer for later

    emitter.instruction("mov rax, 16");                                         // payload size = class_id + hash_ptr
    emitter.instruction("call __rt_heap_alloc");                                // rax = obj pointer
    emitter.instruction("mov r10, 0x454C504800000004");                         // x86_64 heap header word: ELPH marker | object kind 4
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the heap header with the object kind

    emitter.instruction("mov r10, QWORD PTR [rip + _stdclass_class_id]");       // load the compile-time stdClass class_id
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store class_id at obj+0

    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the saved hash pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store hash_ptr at obj+8

    emitter.instruction("mov rsp, rbp");                                        // restore the stack pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return obj pointer in rax
}

fn emit_stdclass_get_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stdclass_get ---");
    emitter.label_global("__rt_stdclass_get");

    // Inputs (SysV): rdi=obj, rsi=name_ptr, rdx=name_len. Output: rax=Mixed*.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve slots for obj, name_ptr, name_len, scratch
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save obj
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save name_ptr
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save name_len

    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load hash_ptr from obj+8
    emitter.instruction("test r10, r10");                                       // empty hash?
    emitter.instruction("je __rt_stdclass_get_null");                           // yes → return Mixed(null)

    emitter.instruction("mov rdi, r10");                                        // rdi = hash pointer for hash_get
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // rsi = name_ptr (key_lo)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // rdx = name_len (key_hi)
    emitter.instruction("call __rt_hash_get");                                  // rax=found, rdi=value_lo, rsi=value_hi, rcx=value_tag

    emitter.instruction("test rax, rax");                                       // hit?
    emitter.instruction("je __rt_stdclass_get_null");                           // not found → Mixed(null)

    emitter.instruction("mov rax, rdi");                                        // return value = boxed Mixed pointer
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    emitter.label("__rt_stdclass_get_null");
    emitter.instruction("mov rax, 8");                                          // tag = 8 (null) for mixed_from_value
    emitter.instruction("mov rdi, 0");                                          // value_lo = 0
    emitter.instruction("mov rsi, 0");                                          // value_hi = 0
    emitter.instruction("call __rt_mixed_from_value");                          // box null into a Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
}

fn emit_mixed_property_get_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_property_get ---");
    emitter.label_global("__rt_mixed_property_get");

    // Inputs (SysV): rdi = mixed_ptr, rsi = name_ptr, rdx = name_len.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve slots for mixed_ptr, name_ptr, name_len, scratch
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save mixed_ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save name_ptr
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save name_len

    emitter.instruction("test rdi, rdi");                                       // null Mixed → null result
    emitter.instruction("je __rt_mixed_property_get_null");                     // branch on the current JSON object encoder condition
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load tag from mixed[0]
    emitter.instruction("cmp r10, 6");                                          // tag = 6 (object)?
    emitter.instruction("jne __rt_mixed_property_get_null");                    // non-object payload → null result
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load obj pointer from mixed[8]
    emitter.instruction("test r10, r10");                                       // null obj → null result
    emitter.instruction("je __rt_mixed_property_get_null");                     // branch on the current JSON object encoder condition
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load class_id from obj[0]
    emitter.instruction("mov r12, QWORD PTR [rip + _stdclass_class_id]");       // load the compile-time stdClass class_id sentinel
    emitter.instruction("cmp r11, r12");                                        // is the receiver a stdClass instance?
    emitter.instruction("jne __rt_mixed_property_get_null");                    // unrelated class → null result

    emitter.instruction("mov rdi, r10");                                        // rdi = stdClass obj pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // rsi = name_ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // rdx = name_len
    emitter.instruction("call __rt_stdclass_get");                              // delegate to the dynamic-property reader
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax

    emitter.label("__rt_mixed_property_get_null");
    emitter.instruction("mov rax, 8");                                          // tag = 8 (null)
    emitter.instruction("mov rdi, 0");                                          // value_lo = 0
    emitter.instruction("mov rsi, 0");                                          // value_hi = 0
    emitter.instruction("call __rt_mixed_from_value");                          // box null into a fresh Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return Mixed* in rax
}

fn emit_mixed_property_set_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_property_set ---");
    emitter.label_global("__rt_mixed_property_set");

    // Inputs (SysV): rdi = mixed_ptr, rsi = name_ptr, rdx = name_len, rcx = value_mixed_ptr.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve slots for the 4 saved inputs
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save mixed_ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save name_ptr
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save name_len
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save value_mixed_ptr

    emitter.instruction("test rdi, rdi");                                       // null Mixed → drop write
    emitter.instruction("je __rt_mixed_property_set_done");                     // branch on the current JSON object encoder condition
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load tag from mixed[0]
    emitter.instruction("cmp r10, 6");                                          // tag = 6 (object)?
    emitter.instruction("jne __rt_mixed_property_set_done");                    // non-object payload → drop write
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load obj pointer from mixed[8]
    emitter.instruction("test r10, r10");                                       // null obj → drop write
    emitter.instruction("je __rt_mixed_property_set_done");                     // branch on the current JSON object encoder condition
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load class_id from obj[0]
    emitter.instruction("mov r12, QWORD PTR [rip + _stdclass_class_id]");       // load the compile-time stdClass class_id sentinel
    emitter.instruction("cmp r11, r12");                                        // is the receiver a stdClass instance?
    emitter.instruction("jne __rt_mixed_property_set_done");                    // unrelated class → drop write

    emitter.instruction("mov rdi, r10");                                        // rdi = stdClass obj pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // rsi = name_ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // rdx = name_len
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // rcx = value_mixed_ptr
    emitter.instruction("call __rt_stdclass_set");                              // delegate to the dynamic-property writer

    emitter.label("__rt_mixed_property_set_done");
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return (no return value)
}

fn emit_stdclass_set_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stdclass_set ---");
    emitter.label_global("__rt_stdclass_set");

    // Inputs (SysV): rdi=obj, rsi=name_ptr, rdx=name_len, rcx=mixed_ptr.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve slots for the 4 saved inputs (16-byte aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save obj
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save name_ptr
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save name_len
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save mixed_ptr

    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load current hash_ptr from obj+8
    emitter.instruction("test r10, r10");                                       // already allocated?
    emitter.instruction("jne __rt_stdclass_set_have_hash");                     // skip lazy init when present

    emitter.instruction("mov rax, 8");                                          // initial capacity = 8 slots
    emitter.instruction("mov rdi, 7");                                          // value_type = 7 (boxed Mixed)
    emitter.instruction("call __rt_hash_new");                                  // rax = empty hash pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload obj
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // store the new hash_ptr at obj+8

    emitter.label("__rt_stdclass_set_have_hash");

    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload obj
    emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                        // rdi = current hash_ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // rsi = name_ptr (key_lo)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // rdx = name_len (key_hi)
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // rcx = mixed_ptr (value_lo)
    emitter.instruction("xor r8, r8");                                          // value_hi = 0 (unused for boxed Mixed)
    emitter.instruction("mov r9, 7");                                           // value_tag = 7 (boxed Mixed)
    emitter.instruction("call __rt_hash_set");                                  // rax = updated hash pointer

    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload obj
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // update obj+8 with the latest hash pointer

    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return (no return value)
}
