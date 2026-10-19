//! Purpose:
//! Emits the canonical runtime helper for `$mixed[] = $value`.
//! Handles indexed arrays, associative arrays, and PHP null autovivification.
//!
//! Called from:
//! - `Op::MixedArrayAppend` lowering in `crate::codegen::lower_inst::arrays`.
//!
//! Key details:
//! - The helper consumes the owned boxed Mixed value on every path.
//! - Indexed arrays derive the append key once and delegate COW/slot conversion
//!   to `__rt_mixed_array_set`; associative arrays use `__rt_hash_append`.
//! - A PHP `ArrayAccess` object receives `offsetSet(null, $value)`; another non-SPL object
//!   throws PHP's `Cannot use object of type X as array`.
//! - Tag 8 and legacy tag-4/tag-5 null-container payloads autovivify through
//!   `__rt_mixed_cell_autovivify_array` before the append, and so does `false` (PHP also prints a
//!   deprecation there). A string throws `[] operator not supported for strings`; an int, a float
//!   or `true` throws `Cannot use a scalar value as an array`, as PHP does.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::emit_branch_if_null_container;
use crate::codegen_support::NULL_SENTINEL;

/// Class-id symbols of the runtime's own SPL containers, whose `$x[] = $v` keeps the previous
/// handling instead of the PHP `ArrayAccess::offsetSet` dispatch.
const SPL_CONTAINER_CLASS_IDS: [&str; 4] = [
    "_spl_fixed_array_class_id",
    "_spl_dll_class_id",
    "_spl_stack_class_id",
    "_spl_queue_class_id",
];

/// Emits `__rt_mixed_array_append` for the active target.
pub(crate) fn emit_mixed_array_append(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_array_append_x86_64(emitter);
        return;
    }
    emit_mixed_array_append_aarch64(emitter);
}

/// Emits the AArch64 Mixed-array append helper.
fn emit_mixed_array_append_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_append ---");
    emitter.label_global("__rt_mixed_array_append");

    emitter.instruction("sub sp, sp, #48");                                     // reserve cell/value/receiver/offset slots and saved frame registers
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the target Mixed cell
    emitter.instruction("str x1, [sp, #8]");                                    // save the owned boxed value consumed by the append
    emitter.instruction("cbz x0, __rt_mixed_array_append_drop");                // absent receivers cannot be mutated
    emitter.instruction("ldr x9, [x0]");                                        // load the receiver's runtime payload tag
    emitter.instruction("cmp x9, #4");                                          // is the receiver an indexed array?
    emitter.instruction("b.eq __rt_mixed_array_append_indexed");                // derive the dense-array append index
    emitter.instruction("cmp x9, #5");                                          // is the receiver an associative array?
    emitter.instruction("b.eq __rt_mixed_array_append_assoc");                  // append using the hash's next automatic integer key
    emitter.instruction("cmp x9, #8");                                          // is the receiver PHP null?
    emitter.instruction("b.eq __rt_mixed_array_append_autovivify");             // PHP `$x[]` autovivifies null to an indexed array
    emitter.instruction("cmp x9, #6");                                          // is the receiver an object?
    emitter.instruction("b.eq __rt_mixed_array_append_object");                 // a PHP ArrayAccess receives offsetSet(null, $value)
    emitter.instruction("cmp x9, #1");                                          // is the receiver a string?
    emitter.instruction("b.eq __rt_mixed_array_append_string");                 // PHP refuses `[]` on strings
    emitter.instruction("cmp x9, #3");                                          // is the receiver a bool?
    emitter.instruction("b.eq __rt_mixed_array_append_bool");                   // false autovivifies, true is a scalar
    emitter.instruction("cmp x9, #0");                                          // is the receiver an int?
    emitter.instruction("b.eq __rt_mixed_array_append_scalar");                 // PHP refuses `[]` on a scalar
    emitter.instruction("cmp x9, #2");                                          // is the receiver a float?
    emitter.instruction("b.eq __rt_mixed_array_append_scalar");                 // PHP refuses `[]` on a scalar
    emitter.instruction("b __rt_mixed_array_append_drop");                      // any other receiver shape drops this write

    // PHP autovivifies `false` (with a deprecation) and refuses `true` like any scalar.
    emitter.label("__rt_mixed_array_append_bool");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the bool payload
    emitter.instruction("cbz x10, __rt_mixed_array_append_autovivify");         // false becomes a fresh array

    emitter.label("__rt_mixed_array_append_scalar");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed value
    emitter.instruction("bl __rt_decref_mixed");                                // release it before leaving through the Error
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame before the tail-call
    emitter.instruction("b __rt_throw_scalar_as_array");                        // never returns

    emitter.label("__rt_mixed_array_append_string");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed value
    emitter.instruction("bl __rt_decref_mixed");                                // release it before leaving through the Error
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame before the tail-call
    emitter.instruction("b __rt_throw_string_append");                          // never returns

    // `$mixed[] = $v` on a PHP `ArrayAccess` object is `$mixed->offsetSet(null, $v)`, resolved
    // through the same dense method table `__rt_mixed_array_set` uses for keyed writes. The
    // runtime's own SPL containers keep their previous handling (the write is dropped).
    emitter.label("__rt_mixed_array_append_object");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the object payload from the Mixed cell
    emit_branch_if_null_container(emitter, "x10", "x9", "__rt_mixed_array_append_autovivify");
    emitter.instruction("str x10, [sp, #16]");                                  // save the object receiver across the offset boxing
    emitter.instruction("ldr x11, [x10]");                                      // load the object's class id
    for symbol in SPL_CONTAINER_CLASS_IDS {
        abi::emit_symbol_address(emitter, "x12", symbol);
        emitter.instruction("ldr x12, [x12]");                                  // load the runtime container's class id
        emitter.instruction("cmp x11, x12");                                    // is this one of the runtime's own SPL containers?
        emitter.instruction("b.eq __rt_mixed_array_append_drop");               // SPL containers keep their previous append handling
    }
    emitter.instruction("tbnz x11, #63, __rt_mixed_array_append_not_indexable"); // synthetic negative ids cannot index metadata
    abi::emit_load_symbol_to_reg(emitter, "x12", "_class_iface_method_count", 0);
    emitter.instruction("cmp x11, x12");                                        // is the id within the dense table?
    emitter.instruction("b.hs __rt_mixed_array_append_not_indexable");          // out-of-range ids have no entry
    abi::emit_symbol_address(emitter, "x12", "_class_offsetset_ptrs");
    emitter.instruction("ldr x12, [x12, x11, lsl #3]");                         // resolve the concrete or inherited offsetSet
    emitter.instruction("cbz x12, __rt_mixed_array_append_not_indexable");      // 0 means the class is not ArrayAccess at all
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = PHP null, the offset of an append
    emitter.instruction("mov x1, #0");                                          // null has no low payload word
    emitter.instruction("mov x2, #0");                                          // null has no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the null offset passed to offsetSet
    emitter.instruction("str x0, [sp, #24]");                                   // keep the boxed offset for its release
    emitter.instruction("mov x1, x0");                                          // pass the boxed null offset as argument 1
    emitter.instruction("ldr x0, [sp, #16]");                                   // pass the unboxed receiver as argument 0
    emitter.instruction("ldr x2, [sp, #8]");                                    // pass the boxed value as argument 2
    emitter.instruction("ldr x11, [x0]");                                       // reload the class id, clobbered across the boxing call
    abi::emit_symbol_address(emitter, "x12", "_class_offsetset_ptrs");
    emitter.instruction("ldr x12, [x12, x11, lsl #3]");                         // re-resolve offsetSet for the same class
    emitter.instruction("blr x12");                                             // write through PHP's ArrayAccess::offsetSet
    // The PHP method BORROWS both arguments; this helper consumes the value, so it releases the
    // value and the offset it boxed.
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed offset
    emitter.instruction("bl __rt_decref_mixed");                                // release it
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed value
    emitter.instruction("bl __rt_decref_mixed");                                // release it too
    emitter.instruction("b __rt_mixed_array_append_done");                      // leave through the shared epilogue

    // PHP refuses `$obj[] = $v` on an object that is not ArrayAccess.
    emitter.label("__rt_mixed_array_append_not_indexable");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the boxed value
    emitter.instruction("bl __rt_decref_mixed");                                // release it before leaving through the Error
    emitter.instruction("ldr x0, [sp, #16]");                                   // pass the receiver to name its class
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame before the tail-call
    emitter.instruction("b __rt_throw_object_not_array");                       // never returns

    emitter.label("__rt_mixed_array_append_indexed");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the indexed-array payload pointer
    emitter.instruction("cbz x10, __rt_mixed_array_append_autovivify");         // legacy null payloads autovivify before any header read
    abi::emit_load_int_immediate(emitter, "x11", NULL_SENTINEL);
    emitter.instruction("cmp x10, x11");                                        // does the payload carry the in-band null-container sentinel?
    emitter.instruction("b.eq __rt_mixed_array_append_autovivify");             // sentinels are PHP null, never array pointers
    emitter.instruction("ldr x1, [x10]");                                       // use the current logical length as the append key
    emitter.instruction("b __rt_mixed_array_append_set");                       // delegate conversion, COW, growth, and ownership transfer

    emitter.label("__rt_mixed_array_append_assoc");
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the associative-array hash pointer
    emitter.instruction("cbz x10, __rt_mixed_array_append_autovivify");         // legacy null hash payloads autovivify to indexed storage
    abi::emit_load_int_immediate(emitter, "x11", NULL_SENTINEL);
    emitter.instruction("cmp x10, x11");                                        // does the hash payload carry the null-container sentinel?
    emitter.instruction("b.eq __rt_mixed_array_append_autovivify");             // sentinel hashes represent PHP null
    emitter.instruction("mov x0, x10");                                         // pass the real hash table to the append helper
    emitter.instruction("ldr x1, [sp, #8]");                                    // pass the owned boxed Mixed value as value_lo
    emitter.instruction("mov x2, xzr");                                         // boxed Mixed hash values have no high payload word
    emitter.instruction("mov x3, #7");                                          // runtime value tag 7 = boxed Mixed
    emitter.instruction("bl __rt_hash_append");                                 // insert at PHP's next automatic integer key
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the owning Mixed cell
    emitter.instruction("str x0, [x9, #8]");                                    // publish the possibly reallocated hash pointer
    emitter.instruction("b __rt_mixed_array_append_done");                      // value ownership was consumed by hash_append

    emitter.label("__rt_mixed_array_append_autovivify");
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass the existing null-shaped cell to autovivification
    emitter.instruction("bl __rt_mixed_cell_autovivify_array");                 // install a fresh empty indexed array in the receiver
    emitter.instruction("mov x1, #0");                                          // the first append key of the fresh array is zero

    emitter.label("__rt_mixed_array_append_set");
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass the target Mixed cell to the shared setter
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer array key
    emitter.instruction("ldr x3, [sp, #8]");                                    // pass the owned boxed Mixed value to the setter
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame state before the tail call
    emitter.instruction("add sp, sp, #48");                                     // release the append helper frame before the tail call
    emitter.instruction("b __rt_mixed_array_set");                              // setter consumes the value and performs the indexed write

    emitter.label("__rt_mixed_array_append_drop");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the unused boxed Mixed value
    emitter.instruction("bl __rt_decref_mixed");                                // release the value when the append cannot be applied
    emitter.label("__rt_mixed_array_append_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                     // release the append helper frame
    emitter.instruction("ret");                                                 // return after the value has been consumed or released
}

/// Emits the x86_64 Mixed-array append helper.
fn emit_mixed_array_append_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_append ---");
    emitter.label_global("__rt_mixed_array_append");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 32");                                         // reserve target/value/receiver/offset slots while preserving call alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the target Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the owned boxed value consumed by the append
    emitter.instruction("test rdi, rdi");                                       // absent receivers cannot be mutated
    emitter.instruction("je __rt_mixed_array_append_drop");                     // release the value for a missing receiver
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the receiver's runtime payload tag
    emitter.instruction("cmp r10, 4");                                          // is the receiver an indexed array?
    emitter.instruction("je __rt_mixed_array_append_indexed");                  // derive the dense-array append index
    emitter.instruction("cmp r10, 5");                                          // is the receiver an associative array?
    emitter.instruction("je __rt_mixed_array_append_assoc");                    // append using the hash's next automatic integer key
    emitter.instruction("cmp r10, 8");                                          // is the receiver PHP null?
    emitter.instruction("je __rt_mixed_array_append_autovivify");               // PHP `$x[]` autovivifies null to an indexed array
    emitter.instruction("cmp r10, 6");                                          // is the receiver an object?
    emitter.instruction("je __rt_mixed_array_append_object");                   // a PHP ArrayAccess receives offsetSet(null, $value)
    emitter.instruction("cmp r10, 1");                                          // is the receiver a string?
    emitter.instruction("je __rt_mixed_array_append_string");                   // PHP refuses `[]` on strings
    emitter.instruction("cmp r10, 3");                                          // is the receiver a bool?
    emitter.instruction("je __rt_mixed_array_append_bool");                     // false autovivifies, true is a scalar
    emitter.instruction("cmp r10, 0");                                          // is the receiver an int?
    emitter.instruction("je __rt_mixed_array_append_scalar");                   // PHP refuses `[]` on a scalar
    emitter.instruction("cmp r10, 2");                                          // is the receiver a float?
    emitter.instruction("je __rt_mixed_array_append_scalar");                   // PHP refuses `[]` on a scalar
    emitter.instruction("jmp __rt_mixed_array_append_drop");                    // any other receiver shape drops this write

    // PHP autovivifies `false` (with a deprecation) and refuses `true` like any scalar.
    emitter.label("__rt_mixed_array_append_bool");
    emitter.instruction("cmp QWORD PTR [rdi + 8], 0");                          // is the bool payload false?
    emitter.instruction("je __rt_mixed_array_append_autovivify");               // false becomes a fresh array

    emitter.label("__rt_mixed_array_append_scalar");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed value
    emitter.instruction("call __rt_decref_mixed");                              // release it before leaving through the Error
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before the tail-call
    emitter.instruction("jmp __rt_throw_scalar_as_array");                      // never returns

    emitter.label("__rt_mixed_array_append_string");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed value
    emitter.instruction("call __rt_decref_mixed");                              // release it before leaving through the Error
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before the tail-call
    emitter.instruction("jmp __rt_throw_string_append");                        // never returns

    // See the ARM64 path: a PHP ArrayAccess object gets offsetSet(null, $value).
    emitter.label("__rt_mixed_array_append_object");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the object payload from the Mixed cell
    emit_branch_if_null_container(emitter, "r10", "r11", "__rt_mixed_array_append_autovivify");
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the object receiver across the offset boxing
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the object's class id
    for symbol in SPL_CONTAINER_CLASS_IDS {
        abi::emit_load_symbol_to_reg(emitter, "r10", symbol, 0);
        emitter.instruction("cmp r11, r10");                                    // is this one of the runtime's own SPL containers?
        emitter.instruction("je __rt_mixed_array_append_drop");                 // SPL containers keep their previous append handling
    }
    emitter.instruction("test r11, r11");                                       // reject negative synthetic class ids
    emitter.instruction("js __rt_mixed_array_append_not_indexable");            // synthetic ids cannot index metadata
    emitter.instruction("cmp r11, QWORD PTR [rip + _class_iface_method_count]"); // is the id within the dense table?
    emitter.instruction("jae __rt_mixed_array_append_not_indexable");           // out-of-range ids have no entry
    emitter.instruction("lea r10, [rip + _class_offsetset_ptrs]");              // dense ArrayAccess::offsetSet table
    emitter.instruction("mov r10, QWORD PTR [r10 + r11 * 8]");                  // resolve the concrete or inherited offsetSet
    emitter.instruction("test r10, r10");                                       // 0 means the class is not ArrayAccess at all
    emitter.instruction("jz __rt_mixed_array_append_not_indexable");            // refuse rather than drop the write
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 = PHP null, the offset of an append
    emitter.instruction("xor edi, edi");                                        // null has no low payload word
    emitter.instruction("xor esi, esi");                                        // null has no high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // box the null offset passed to offsetSet
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // keep the boxed offset for its release
    emitter.instruction("mov rsi, rax");                                        // pass the boxed null offset as argument 1
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the unboxed receiver as argument 0
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // pass the boxed value as argument 2
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // reload the class id, clobbered across the boxing call
    emitter.instruction("lea r10, [rip + _class_offsetset_ptrs]");              // dense ArrayAccess::offsetSet table
    emitter.instruction("mov r10, QWORD PTR [r10 + r11 * 8]");                  // re-resolve offsetSet for the same class
    emitter.instruction("call r10");                                            // write through PHP's ArrayAccess::offsetSet
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed offset
    emitter.instruction("call __rt_decref_mixed");                              // release it
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed value
    emitter.instruction("call __rt_decref_mixed");                              // release it too
    emitter.instruction("jmp __rt_mixed_array_append_done");                    // leave through the shared epilogue

    emitter.label("__rt_mixed_array_append_not_indexable");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the boxed value
    emitter.instruction("call __rt_decref_mixed");                              // release it before leaving through the Error
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass the receiver to name its class
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before the tail-call
    emitter.instruction("jmp __rt_throw_object_not_array");                     // never returns

    emitter.label("__rt_mixed_array_append_indexed");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the indexed-array payload pointer
    emitter.instruction("test r10, r10");                                       // legacy null payloads are not dereferenceable arrays
    emitter.instruction("je __rt_mixed_array_append_autovivify");               // autovivify null payloads before any header read
    abi::emit_load_int_immediate(emitter, "r11", NULL_SENTINEL);
    emitter.instruction("cmp r10, r11");                                        // does the payload carry the in-band null-container sentinel?
    emitter.instruction("je __rt_mixed_array_append_autovivify");               // sentinels are PHP null, never array pointers
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // use the current logical length as the append key
    emitter.instruction("jmp __rt_mixed_array_append_set");                     // delegate conversion, COW, growth, and ownership transfer

    emitter.label("__rt_mixed_array_append_assoc");
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // load the associative-array hash pointer
    emitter.instruction("test r10, r10");                                       // legacy null hash payloads need autovivification
    emitter.instruction("je __rt_mixed_array_append_autovivify");               // autovivify null hashes to indexed storage
    abi::emit_load_int_immediate(emitter, "r11", NULL_SENTINEL);
    emitter.instruction("cmp r10, r11");                                        // does the hash payload carry the null-container sentinel?
    emitter.instruction("je __rt_mixed_array_append_autovivify");               // sentinel hashes represent PHP null
    emitter.instruction("mov rdi, r10");                                        // pass the real hash table to the append helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the owned boxed Mixed value as value_lo
    emitter.instruction("xor edx, edx");                                        // boxed Mixed hash values have no high payload word
    emitter.instruction("mov rcx, 7");                                          // runtime value tag 7 = boxed Mixed
    emitter.instruction("call __rt_hash_append");                               // insert at PHP's next automatic integer key
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the owning Mixed cell
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // publish the possibly reallocated hash pointer
    emitter.instruction("jmp __rt_mixed_array_append_done");                    // value ownership was consumed by hash_append

    emitter.label("__rt_mixed_array_append_autovivify");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the existing null-shaped cell to autovivification
    emitter.instruction("call __rt_mixed_cell_autovivify_array");               // install a fresh empty indexed array in the receiver
    emitter.instruction("xor esi, esi");                                        // the first append key of the fresh array is zero

    emitter.label("__rt_mixed_array_append_set");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // pass the target Mixed cell to the shared setter
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer array key
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // pass the owned boxed Mixed value to the setter
    emitter.instruction("mov rsp, rbp");                                        // release the append helper frame before the tail call
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before the tail call
    emitter.instruction("jmp __rt_mixed_array_set");                            // setter consumes the value and performs the indexed write

    emitter.label("__rt_mixed_array_append_drop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the unused boxed Mixed value
    emitter.instruction("call __rt_decref_mixed");                              // release the value when the append cannot be applied
    emitter.label("__rt_mixed_array_append_done");
    emitter.instruction("mov rsp, rbp");                                        // release the append helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return after the value has been consumed or released
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// `$mixed[] = $v` on an object dispatches through the dense `offsetSet` table with a boxed
    /// null offset, and refuses a non-ArrayAccess object, on every supported target.
    #[test]
    fn object_receivers_dispatch_offset_set_with_a_null_offset() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_mixed_array_append(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_mixed_array_append_object:"), "{name}");
            assert!(asm.contains("_class_offsetset_ptrs"), "{name}");
            assert!(asm.contains("__rt_throw_object_not_array"), "{name}");
            let null_tag = if name == "linux-x86_64" { "mov rax, 8" } else { "mov x0, #8" };
            assert!(asm.contains(null_tag), "{name}");
            for symbol in SPL_CONTAINER_CLASS_IDS {
                assert!(asm.contains(symbol), "{name}: {symbol}");
            }
        }
    }
}
