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
//! - Tag 8 and legacy tag-4/tag-5 null-container payloads autovivify through
//!   `__rt_mixed_cell_autovivify_array` before the append.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::NULL_SENTINEL;

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

    emitter.instruction("sub sp, sp, #32");                                     // reserve receiver/value slots and saved frame registers
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
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
    emitter.instruction("b __rt_mixed_array_append_drop");                      // incompatible scalar/object receivers drop this write

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
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame state before the tail call
    emitter.instruction("add sp, sp, #32");                                     // release the append helper frame before the tail call
    emitter.instruction("b __rt_mixed_array_set");                              // setter consumes the value and performs the indexed write

    emitter.label("__rt_mixed_array_append_drop");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the unused boxed Mixed value
    emitter.instruction("bl __rt_decref_mixed");                                // release the value when the append cannot be applied
    emitter.label("__rt_mixed_array_append_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                     // release the append helper frame
    emitter.instruction("ret");                                                 // return after the value has been consumed or released
}

/// Emits the x86_64 Mixed-array append helper.
fn emit_mixed_array_append_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_array_append ---");
    emitter.label_global("__rt_mixed_array_append");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("sub rsp, 16");                                         // reserve target and value slots while preserving call alignment
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
    emitter.instruction("jmp __rt_mixed_array_append_drop");                    // incompatible scalar/object receivers drop this write

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
